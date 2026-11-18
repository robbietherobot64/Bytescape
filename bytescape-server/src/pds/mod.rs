//! PDS module — AT Protocol PDS endpoints for our axum binary.
//!
//! Uses rsky-repo for MST + signed commits, SqlxPostgresStorage for persistence.

pub mod account;
pub mod did_doc;
pub mod firehose_emit;
pub mod firehose_consumer;
pub mod plc;
/// Per-account write quiesce for the re-identify walk→cutover window (R2-3).
mod quiesce;
pub mod reidentify;
pub mod storage;
#[cfg(test)]
pub mod test_support;

use axum::{
    body::Body,
    extract::{Query, State},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use rsky_repo::{
    repo::Repo,
    types::{RecordCreateOrUpdateOp, RecordWriteEnum, RecordWriteOp, WriteOpAction, Lex},
};
use rsky_repo::storage::Ipld;
use sha2::Digest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::AppState;
use crate::pds::account::PdsAccount;
use crate::pds::storage::SqlxPostgresStorage;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/xrpc/com.atproto.repo.createRecord", post(create_record))
        .route("/xrpc/com.atproto.repo.uploadBlob", post(upload_blob))
        .route("/xrpc/com.atproto.repo.getRecord", get(get_record))
        .route("/xrpc/com.atproto.repo.listRecords", get(list_records))
        .route("/xrpc/com.atproto.repo.putRecord", post(put_record))
        .route("/xrpc/com.atproto.repo.deleteRecord", post(delete_record))
        .route("/xrpc/com.atproto.repo.describeRepo", get(describe_repo))
        .route("/xrpc/com.atproto.server.describeServer", get(describe_server))
        .route("/xrpc/com.atproto.sync.listRepos", get(list_repos))
        .route("/xrpc/com.atproto.sync.getRepo", get(get_repo))
        .route("/xrpc/com.atproto.sync.getLatestCommit", get(get_latest_commit))
        .route("/xrpc/com.atproto.sync.getRepoStatus", get(get_repo_status))
        .route("/xrpc/com.atproto.sync.getBlocks", get(get_blocks))
        .route("/xrpc/com.atproto.server.createAccount", post(create_account))
        .route("/xrpc/com.atproto.server.createSession", post(create_session))
        .route("/xrpc/com.atproto.server.getSession", get(get_session))
        .route("/xrpc/com.atproto.sync.subscribeRepos", get(subscribe_repos_ws))
        .route("/xrpc/com.atproto.sync.getBlob", get(get_blob))
        .route(
            "/.well-known/did.json",
            get(did_doc::instance_did_document),
        )
        .route(
            "/{handle}/.well-known/did.json",
            get(did_doc::user_did_document),
        )
        .route(
            "/.well-known/atproto-did",
            get(did_doc::atproto_did_instance),
        )
        .route(
            "/{handle}/.well-known/atproto-did",
            get(did_doc::atproto_did),
        )
}

// ─── createRecord ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CreateRecordRequest {
    repo: String,
    collection: String,
    rkey: Option<String>,
    record: Value,
}

#[derive(Debug, Serialize)]
struct CreateRecordResponse {
    uri: String,
    cid: String,
    commit: CommitInfo,
}

#[derive(Debug, Serialize)]
struct CommitInfo {
    cid: String,
    rev: String,
}

async fn create_record(
    State(state): State<AppState>,
    Json(req): Json<CreateRecordRequest>,
) -> Result<Json<CreateRecordResponse>, crate::error::AppError> {
    let rkey = req.rkey.clone();
    let (uri, cid, rev) = create_record_internal(
        &state.db,
        &req.repo,
        &req.collection,
        rkey.as_deref(),
        &req.record,
        Some(&state.config.domain),
    ).await?;

    Ok(Json(CreateRecordResponse {
        uri,
        cid: cid.clone(),
        commit: CommitInfo { cid, rev },
    }))
}

/// Internal createRecord logic — callable from other modules (e.g. /api/posts/create).
/// Creates a PDS record, emits firehose event, and triggers PDS→AP bridge.
pub async fn create_record_internal(
    db: &sqlx::PgPool,
    did: &str,
    collection: &str,
    rkey: Option<&str>,
    record: &serde_json::Value,
    domain: Option<&str>,
) -> Result<(String, String, String), crate::error::AppError> {
    // R2-3: a write landing in the re-identify walk→cutover window would be
    // destroyed by the cutover — refuse loudly instead (retry after cutover).
    quiesce::check_write(did).map_err(crate::error::AppError::Conflict)?;
    let account = PdsAccount::load(db, did)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("Account load failed: {e}")))?
        .ok_or_else(|| crate::error::AppError::NotFound("Account not found".into()))?;

    // Resolve domain: prefer explicit param, fall back to env var for backwards compat
    let domain_owned;
    let domain_str = match domain {
        Some(d) => d,
        None => {
            domain_owned = std::env::var("DOMAIN").unwrap_or_else(|_| "bytescape.social".to_string());
            tracing::warn!("create_record_internal called without explicit domain — using env var fallback (deprecated)");
            &domain_owned
        }
    };

    let storage = Arc::new(RwLock::new(SqlxPostgresStorage::new(
        db.clone(),
        did.to_string(),
    )));

    let mut repo = match Repo::load(storage.clone(), None).await {
        Ok(r) => r,
        Err(_) => {
            Repo::create(storage.clone(), did.to_string(), &account.keypair, None)
                .await
                .map_err(|e| crate::error::AppError::Internal(format!("Repo create failed: {e}")))?
        }
    };

    let rkey = rkey.map(|s| s.to_string()).unwrap_or_else(|| {
        use rsky_common::tid::Ticker;
        Ticker::new().next(None).0
    });

    let record_map: BTreeMap<String, Lex> =
        serde_json::from_value(record.clone()).unwrap_or_else(|_| {
            let mut m = BTreeMap::new();
            m.insert(
                "$type".to_string(),
                Lex::Ipld(Ipld::String(collection.to_string())),
            );
            m.insert(
                "text".to_string(),
                Lex::Ipld(Ipld::String(
                    record
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string(),
                )),
            );
            m
        });

    let write_op = RecordWriteOp::Create(RecordCreateOrUpdateOp {
        action: WriteOpAction::Create,
        collection: collection.to_string(),
        rkey: rkey.clone(),
        record: record_map,
    });

    // Capture the current MST root BEFORE the commit: it is the new commit's
    // `prevData` (apply_commit deletes the previous commit block, so this must
    // be read here, not at emit time). Tied to `commit_data.prev`.
    let prev_root = repo.commit.data;
    let commit_data = repo
        .format_commit(RecordWriteEnum::Single(write_op.clone()), &account.keypair)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("format_commit failed: {e}")))?;

    let repo = repo
        .apply_commit(commit_data.clone())
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("apply_commit failed: {e}")))?;

    // Emit firehose event
    let record_bytes = serde_ipld_dagcbor::to_vec(record).unwrap_or_default();
    let mut sha = sha2::Sha256::new();
    sha.update(&record_bytes);
    let hash = sha.finalize();
    let record_cid = lexicon_cid::Cid::new_v1(
        0x71,
        lexicon_cid::multihash::MultihashGeneric::wrap(0x12, &hash).unwrap(),
    );
    let ops = vec![(
        rsky_repo::types::CommitAction::Create,
        format!("{}/{}", collection, rkey),
        Some(record_cid),
    )];
    let _event = firehose_emit::emit_commit(db, did, &commit_data, ops, commit_data.prev.is_some().then_some(prev_root))
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("firehose emit failed: {e}")))?;

    // PDS→AP bridge (Phase C): federation side-effects only. Runs after the
    // AT commit and never fails the native write (failures are logged and
    // persisted inside the bridge).
    crate::ap::bridge::after_write(
        db,
        domain_str,
        did,
        collection,
        &rkey,
        record,
        &crate::api::stories::record_cid(record),
    )
    .await;

    let uri = format!("at://{}/{}/{}", did, collection, rkey);
    let cid = repo.cid.to_string();
    let rev = repo.commit.rev.clone();

    Ok((uri, cid, rev))
}

// ─── getRecord ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GetRecordParams {
    repo: String,
    collection: String,
    rkey: String,
}

#[derive(Debug, Serialize)]
struct GetRecordResponse {
    uri: String,
    cid: String,
    value: Value,
    commit: CommitInfo,
}

async fn get_record(
    State(state): State<AppState>,
    Query(params): Query<GetRecordParams>,
) -> Result<Json<GetRecordResponse>, crate::error::AppError> {
    let did = params.repo.clone();

    // Read from derived index (pds_indexed_records) — maintained by firehose consumer
    let row: Option<(String, serde_json::Value, String)> = sqlx::query_as(
        "SELECT cid, value_json, rev FROM pds_indexed_records
         WHERE did = $1 AND collection = $2 AND rkey = $3",
    )
    .bind(&did)
    .bind(&params.collection)
    .bind(&params.rkey)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| crate::error::AppError::Internal(format!("Index query failed: {e}")))?;

    let (cid, value, rev) = row
        .ok_or_else(|| crate::error::AppError::NotFound("Record not found".into()))?;

    let uri = format!("at://{}/{}/{}", did, params.collection, params.rkey);

    Ok(Json(GetRecordResponse {
        uri,
        cid: cid.clone(),
        value,
        commit: CommitInfo { cid, rev },
    }))
}

// ─── listRecords ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ListRecordsParams {
    repo: String,
    collection: String,
    limit: Option<u32>,
    cursor: Option<String>,
}

#[derive(Debug, Serialize)]
struct ListRecordsResponse {
    records: Vec<ListRecordEntry>,
    cursor: Option<String>,
}

#[derive(Debug, Serialize)]
struct ListRecordEntry {
    uri: String,
    cid: String,
    value: Value,
}

async fn list_records(
    State(state): State<AppState>,
    Query(params): Query<ListRecordsParams>,
) -> Result<Json<ListRecordsResponse>, crate::error::AppError> {
    let did = params.repo.clone();
    let limit = params.limit.unwrap_or(50).min(100) as i64;

    // Read from derived index (pds_indexed_records)
    let rows: Vec<(String, String, serde_json::Value)> = sqlx::query_as(
        "SELECT rkey, cid, value_json FROM pds_indexed_records
         WHERE did = $1 AND collection = $2
         ORDER BY rkey ASC
         LIMIT $3",
    )
    .bind(&did)
    .bind(&params.collection)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(|e| crate::error::AppError::Internal(format!("Index query failed: {e}")))?;

    let records: Vec<ListRecordEntry> = rows
        .into_iter()
        .map(|(rkey, cid, value)| ListRecordEntry {
            uri: format!("at://{}/{}/{}", did, params.collection, rkey),
            cid,
            value,
        })
        .collect();

    let next_cursor = if records.len() as i64 >= limit {
        records.last().map(|r| {
            r.uri.split('/').last().unwrap_or("").to_string()
        })
    } else {
        None
    };

    Ok(Json(ListRecordsResponse {
        records,
        cursor: next_cursor,
    }))
}

// ─── blobs ────────────────────────────────────────────────────────

/// Store a blob in the PDS blob store; returns (cid, size). The CID is the
/// standard DAG-CBOR-free raw-blob CIDv1 (0x71 0x12 0x20 + sha256).
pub async fn put_blob_internal(
    db: &sqlx::PgPool,
    did: &str,
    bytes: &[u8],
    mime_type: &str,
) -> Result<(String, i64), crate::error::AppError> {
    let mut sha = sha2::Sha256::new();
    sha.update(bytes);
    let hash = sha.finalize();
    let cid = lexicon_cid::Cid::new_v1(
        0x71,
        lexicon_cid::multihash::MultihashGeneric::wrap(0x12, &hash).unwrap(),
    )
    .to_string();
    sqlx::query(
        "INSERT INTO pds_blobs (did, cid, mime_type, size, bytes) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (did, cid) DO NOTHING",
    )
    .bind(did)
    .bind(&cid)
    .bind(mime_type)
    .bind(bytes.len() as i64)
    .bind(bytes)
    .execute(db)
    .await
    .map_err(|e| crate::error::AppError::Internal(format!("blob insert failed: {e}")))?;
    Ok((cid, bytes.len() as i64))
}

/// Read a blob back (mime + bytes).
pub async fn get_blob_internal(
    db: &sqlx::PgPool,
    did: &str,
    cid: &str,
) -> Result<Option<(String, Vec<u8>)>, crate::error::AppError> {
    let row: Option<(String, Vec<u8>)> = sqlx::query_as(
        "SELECT mime_type, bytes FROM pds_blobs WHERE did = $1 AND cid = $2",
    )
    .bind(did)
    .bind(cid)
    .fetch_optional(db)
    .await
    .map_err(|e| crate::error::AppError::Internal(format!("blob read failed: {e}")))?;
    Ok(row)
}

#[derive(Debug, Deserialize)]
struct UploadBlobQuery {
    repo: String,
}

/// com.atproto.repo.uploadBlob — raw body in, ATProto blob ref out. The repo
/// must be a known PDS account (same posture as createRecord on this surface).
async fn upload_blob(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::Query(q): axum::extract::Query<UploadBlobQuery>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, crate::error::AppError> {
    if body.is_empty() {
        return Err(crate::error::AppError::BadRequest("empty blob".into()));
    }
    PdsAccount::load(&state.db, &q.repo)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("Account load failed: {e}")))?
        .ok_or_else(|| crate::error::AppError::NotFound("Account not found".into()))?;

    let mime = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    let (cid, size) = put_blob_internal(&state.db, &q.repo, &body, &mime).await?;
    Ok(Json(serde_json::json!({
        "blob": {"$type": "blob", "ref": {"$link": cid}, "mimeType": mime, "size": size}
    })))
}

#[derive(Debug, Deserialize)]
struct GetBlobQuery {
    did: String,
    cid: String,
}

/// com.atproto.sync.getBlob — public blob read. Supports Range requests
/// (video elements, iOS Safari especially, require them for playback).
async fn get_blob(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<GetBlobQuery>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, crate::error::AppError> {
    let range = headers
        .get(axum::http::header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    if let Some((mime, bytes)) = get_blob_internal(&state.db, &q.did, &q.cid).await? {
        return local_blob_response(mime, bytes, range.as_deref());
    }
    // External authors keep blobs on their own PDS — relay them (streamed;
    // story videos run to the full blob limit). Two resolution paths: a
    // linked Bluesky account carries its PDS in the OAuth metadata (fast),
    // every other DID resolves through its DID document. Without the
    // document fallback, stories from plain followed accounts 404 — a dead
    // black viewer.
    let acct: Option<crate::db::accounts::LinkedAccountRow> =
        sqlx::query_as("SELECT * FROM linked_accounts WHERE external_id = $1 LIMIT 1")
            .bind(&q.did)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| crate::error::AppError::Internal(format!("blob lookup failed: {e}")))?
            .filter(|a: &crate::db::accounts::LinkedAccountRow| a.provider == "bluesky");
    let mut pds: Option<String> = acct
        .as_ref()
        .map(|a| crate::oauth::bluesky::get_pds_url(&a.metadata));
    if pds.is_none() {
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| crate::error::AppError::Internal(format!("client build failed: {e}")))?;
        pds = crate::oauth::bluesky::resolve_pds_endpoint(&http, &q.did).await.ok();
    }
    if let Some(pds) = pds {
        let url = format!(
            "{}/xrpc/com.atproto.sync.getBlob?did={}&cid={}",
            pds,
            urlencoding::encode(&q.did),
            urlencoding::encode(&q.cid)
        );
        let mut req = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| crate::error::AppError::Internal(format!("client build failed: {e}")))?
            .get(&url);
        if let Some(r) = &range {
            req = req.header(axum::http::header::RANGE, r.as_str());
        }
        let resp = req
            .send()
            .await
            .map_err(|e| crate::error::AppError::Internal(format!("remote blob fetch failed: {e}")))?;
        if resp.status().is_success() {
            let status = resp.status().as_u16();
            let mime = resp
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();
            let content_range = resp
                .headers()
                .get(axum::http::header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            // Pass the length through: chunked video responses without a
            // Content-Length stall or refuse to play in stricter browsers
            // (Safari especially).
            let content_length = resp
                .headers()
                .get(axum::http::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            // Some upstream PDSes ignore Range on getBlob and answer 200
            // with the full blob. Serve the requested window from the
            // stream ourselves in that case.
            if content_range.is_none() {
                if let Some((start, end)) = range.as_deref().and_then(parse_range) {
                    let total: u64 = content_length
                        .as_deref()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                    if total > 0 {
                        let end = end.min(total - 1);
                        if start > end || start >= total {
                            return axum::response::Response::builder()
                                .status(416)
                                .header(
                                    axum::http::header::CONTENT_RANGE,
                                    format!("bytes */{total}"),
                                )
                                .body(axum::body::Body::empty())
                                .map_err(|e| {
                                    crate::error::AppError::Internal(format!(
                                        "response build failed: {e}"
                                    ))
                                });
                        }
                        let len = end - start + 1;
                        let stream = RangeSlice {
                            inner: Box::pin(resp.bytes_stream()),
                            skip: start,
                            remaining: len,
                        };
                        return axum::response::Response::builder()
                            .status(206)
                            .header(axum::http::header::CONTENT_TYPE, mime)
                            .header(axum::http::header::ACCEPT_RANGES, "bytes")
                            .header(
                                axum::http::header::CONTENT_RANGE,
                                format!("bytes {start}-{end}/{total}"),
                            )
                            .header(axum::http::header::CONTENT_LENGTH, len.to_string())
                            .body(axum::body::Body::from_stream(stream))
                            .map_err(|e| {
                                crate::error::AppError::Internal(format!(
                                    "response build failed: {e}"
                                ))
                            });
                    }
                }
            }
            let mut builder = axum::response::Response::builder()
                .status(status)
                .header(axum::http::header::CONTENT_TYPE, mime)
                .header(axum::http::header::ACCEPT_RANGES, "bytes");
            if let Some(cr) = content_range {
                builder = builder.header(axum::http::header::CONTENT_RANGE, cr);
            }
            if let Some(cl) = content_length {
                builder = builder.header(axum::http::header::CONTENT_LENGTH, cl);
            }
            return builder
                .body(axum::body::Body::from_stream(resp.bytes_stream()))
                .map_err(|e| crate::error::AppError::Internal(format!("response build failed: {e}")));
        }
    }
    Err(crate::error::AppError::NotFound("blob not found".into()))
}

/// Serve a byte window out of a chunked upstream stream: discard `skip`
/// bytes, then yield at most `remaining` bytes (sub-slicing chunks — no
/// copying). Needed because upstream PDSes don't all honor Range on getBlob
/// and a video element won't start playback on a full-size 200 response.
struct RangeSlice<S> {
    inner: std::pin::Pin<Box<S>>,
    skip: u64,
    remaining: u64,
}

impl<S, E> futures::Stream for RangeSlice<S>
where
    S: futures::Stream<Item = Result<bytes::Bytes, E>>,
{
    type Item = Result<bytes::Bytes, E>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            if this.remaining == 0 {
                return std::task::Poll::Ready(None);
            }
            match this.inner.as_mut().poll_next(cx) {
                std::task::Poll::Ready(Some(Ok(chunk))) => {
                    let chunk = if this.skip > 0 {
                        let n = std::cmp::min(this.skip, chunk.len() as u64) as usize;
                        this.skip -= n as u64;
                        chunk.slice(n..)
                    } else {
                        chunk
                    };
                    if chunk.is_empty() {
                        continue;
                    }
                    let take = std::cmp::min(this.remaining, chunk.len() as u64) as usize;
                    this.remaining -= take as u64;
                    return std::task::Poll::Ready(Some(Ok(chunk.slice(..take))));
                }
                std::task::Poll::Ready(Some(Err(e))) => {
                    return std::task::Poll::Ready(Some(Err(e)))
                }
                std::task::Poll::Ready(None) => return std::task::Poll::Ready(None),
                std::task::Poll::Pending => return std::task::Poll::Pending,
            }
        }
    }
}

fn local_blob_response(
    mime: String,
    bytes: Vec<u8>,
    range: Option<&str>,
) -> Result<axum::response::Response, crate::error::AppError> {
    let total = bytes.len() as u64;
    if let Some((start, end)) = range.and_then(parse_range) {
        let end = end.min(total.saturating_sub(1));
        if start > end || start >= total {
            return axum::response::Response::builder()
                .status(416)
                .header(axum::http::header::CONTENT_RANGE, format!("bytes */{total}"))
                .body(axum::body::Body::empty())
                .map_err(|e| crate::error::AppError::Internal(format!("response build failed: {e}")));
        }
        let slice = bytes[start as usize..=end as usize].to_vec();
        return axum::response::Response::builder()
            .status(206)
            .header(axum::http::header::CONTENT_TYPE, mime)
            .header(axum::http::header::ACCEPT_RANGES, "bytes")
            .header(
                axum::http::header::CONTENT_RANGE,
                format!("bytes {start}-{end}/{total}"),
            )
            .body(axum::body::Body::from(slice))
            .map_err(|e| crate::error::AppError::Internal(format!("response build failed: {e}")));
    }
    axum::response::Response::builder()
        .header(axum::http::header::CONTENT_TYPE, mime)
        .header(axum::http::header::ACCEPT_RANGES, "bytes")
        .body(axum::body::Body::from(bytes))
        .map_err(|e| crate::error::AppError::Internal(format!("response build failed: {e}")))
}

/// Parse a single-range `bytes=start-end` header (end optional).
fn parse_range(v: &str) -> Option<(u64, u64)> {
    let spec = v.strip_prefix("bytes=")?;
    let (a, b) = spec.split_once('-')?;
    let start: u64 = a.parse().ok()?;
    let end: u64 = if b.is_empty() { u64::MAX } else { b.parse().ok()? };
    Some((start, end))
}

// ─── putRecord ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct PutRecordRequest {
    repo: String,
    collection: String,
    rkey: String,
    record: Value,
}

async fn put_record(
    State(state): State<AppState>,
    Json(req): Json<PutRecordRequest>,
) -> Result<Json<CreateRecordResponse>, crate::error::AppError> {
    let (uri, cid, rev) = put_record_internal(
        &state.db,
        &req.repo,
        &req.collection,
        &req.rkey,
        &req.record,
    )
    .await?;
    Ok(Json(CreateRecordResponse {
        uri,
        cid: cid.clone(),
        commit: CommitInfo { cid, rev },
    }))
}

/// Internal putRecord logic (upsert) — used for self-keyed records that are
/// written repeatedly (app.bsky.actor.status/self).
pub async fn put_record_internal(
    db: &sqlx::PgPool,
    did: &str,
    collection: &str,
    rkey: &str,
    record: &serde_json::Value,
) -> Result<(String, String, String), crate::error::AppError> {
    // R2-3: refused during the re-identify walk→cutover window (see above).
    quiesce::check_write(did).map_err(crate::error::AppError::Conflict)?;
    let account = PdsAccount::load(db, did)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("Account load failed: {e}")))?
        .ok_or_else(|| crate::error::AppError::NotFound("Account not found".into()))?;

    let storage = Arc::new(RwLock::new(SqlxPostgresStorage::new(
        db.clone(),
        did.to_string(),
    )));

    let mut repo = match Repo::load(storage.clone(), None).await {
        Ok(r) => r,
        Err(_) => {
            Repo::create(storage.clone(), did.to_string(), &account.keypair, None)
                .await
                .map_err(|e| crate::error::AppError::Internal(format!("Repo create failed: {e}")))?
        }
    };

    let record_map: BTreeMap<String, Lex> =
        serde_json::from_value(record.clone()).unwrap_or_else(|_| {
            let mut m = BTreeMap::new();
            m.insert(
                "$type".to_string(),
                Lex::Ipld(Ipld::String(collection.to_string())),
            );
            m
        });

    let write_op = RecordWriteOp::Update(RecordCreateOrUpdateOp {
        action: WriteOpAction::Update,
        collection: collection.to_string(),
        rkey: rkey.to_string(),
        record: record_map,
    });

    let prev_root = repo.commit.data;
    let commit_data = repo
        .format_commit(RecordWriteEnum::Single(write_op.clone()), &account.keypair)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("format_commit failed: {e}")))?;

    let repo = repo
        .apply_commit(commit_data.clone())
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("putRecord apply_commit failed: {e}")))?;

    let record_bytes = serde_ipld_dagcbor::to_vec(record).unwrap_or_default();
    let mut sha = sha2::Sha256::new();
    sha.update(&record_bytes);
    let hash = sha.finalize();
    let record_cid = lexicon_cid::Cid::new_v1(
        0x71,
        lexicon_cid::multihash::MultihashGeneric::wrap(0x12, &hash).unwrap(),
    );

    let ops = vec![(
        rsky_repo::types::CommitAction::Create,
        format!("{}/{}", collection, rkey),
        Some(record_cid),
    )];
    let _event = firehose_emit::emit_commit(db, did, &commit_data, ops, commit_data.prev.is_some().then_some(prev_root))
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("firehose emit failed: {e}")))?;

    // PDS→AP bridge (Phase C): Update or idempotent Create/Update of the
    // stored AP original. After the AT commit; never fails the native write.
    {
        let domain = std::env::var("DOMAIN").unwrap_or_else(|_| "bytescape.social".to_string());
        crate::ap::bridge::after_write(
            db,
            &domain,
            did,
            collection,
            rkey,
            record,
            &crate::api::stories::record_cid(record),
        )
        .await;
    }

    let uri = format!("at://{}/{}/{}", did, collection, rkey);
    let cid = repo.cid.to_string();
    let rev = repo.commit.rev.clone();
    Ok((uri, cid, rev))
}

// ─── deleteRecord ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct DeleteRecordRequest {
    repo: String,
    collection: String,
    rkey: String,
}

async fn delete_record(
    State(state): State<AppState>,
    Json(req): Json<DeleteRecordRequest>,
) -> Result<Json<CommitInfo>, crate::error::AppError> {
    let (cid, rev) =
        delete_record_internal(&state.db, &req.repo, &req.collection, &req.rkey).await?;
    Ok(Json(CommitInfo { cid, rev }))
}

/// Internal deleteRecord logic — callable from other modules (stories expiry
/// transition, manual story deletion).
pub async fn delete_record_internal(
    db: &sqlx::PgPool,
    did: &str,
    collection: &str,
    rkey: &str,
) -> Result<(String, String), crate::error::AppError> {
    // R2-3: refused during the re-identify walk→cutover window (see above).
    quiesce::check_write(did).map_err(crate::error::AppError::Conflict)?;
    let account = PdsAccount::load(db, did)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("Account load failed: {e}")))?
        .ok_or_else(|| crate::error::AppError::NotFound("Account not found".into()))?;

    let storage = Arc::new(RwLock::new(SqlxPostgresStorage::new(
        db.clone(),
        did.to_string(),
    )));

    let mut repo = Repo::load(storage, None)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("Repo load failed: {e}")))?;

    let delete_op = RecordWriteOp::Delete(rsky_repo::types::RecordDeleteOp {
        action: WriteOpAction::Delete,
        collection: collection.to_string(),
        rkey: rkey.to_string(),
    });

    let prev_root = repo.commit.data;
    let commit_data = repo
        .format_commit(RecordWriteEnum::Single(delete_op.clone()), &account.keypair)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("deleteRecord format_commit failed: {e}")))?;

    let repo = repo
        .apply_commit(commit_data.clone())
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("deleteRecord apply_commit failed: {e}")))?;

    let ops = vec![(
        rsky_repo::types::CommitAction::Delete,
        format!("{}/{}", collection, rkey),
        None::<lexicon_cid::Cid>,
    )];
    let _event = firehose_emit::emit_commit(db, did, &commit_data, ops, commit_data.prev.is_some().then_some(prev_root))
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("firehose emit failed: {e}")))?;

    // PDS→AP bridge (Phase C): delete → Undo of the stored AP original
    // (resolved via ap_activities.local_uri). After the AT commit; never
    // fails the native write.
    {
        let domain = std::env::var("DOMAIN").unwrap_or_else(|_| "bytescape.social".to_string());
        crate::ap::bridge::after_delete(db, &domain, did, collection, rkey).await;
    }

    Ok((repo.cid.to_string(), repo.commit.rev.clone()))
}

/// Purge every record in a native Bytescape PDS repo for a DID. Used on account
/// deletion for native accounts ("full delete" policy — Bytescape owns the PDS).
/// Enumerates the derived record index and deletes each record. Best-effort:
/// individual failures are logged and skipped so one bad record doesn't block
/// account deletion.
pub async fn purge_native_repo(db: &sqlx::PgPool, did: &str) {
    let records: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
        "SELECT collection, rkey FROM pds_indexed_records WHERE did = $1",
    )
    .bind(did)
    .fetch_all(db)
    .await
    .unwrap_or_default();
    for (collection, rkey) in records {
        if let Err(e) = delete_record_internal(db, did, &collection, &rkey).await {
            tracing::warn!("[PDS] purge deleteRecord failed for {did}/{collection}/{rkey}: {e}");
        }
    }
    // AT-side tombstone (add-identity-realignment, task 4.1 "deletion federates
    // on both protocols"): drop the repo root so com.atproto.sync.getRepo /
    // getLatestCommit answer RepoNotFound for the dead account. The root row
    // has no FK and would otherwise serve a dead account's repo forever — the
    // pds_repo_roots orphan class. Any index rows whose per-record delete
    // failed above go with it.
    sqlx::query("DELETE FROM pds_repo_roots WHERE did = $1")
        .bind(did)
        .execute(db)
        .await
        .ok();
    sqlx::query("DELETE FROM pds_indexed_records WHERE did = $1")
        .bind(did)
        .execute(db)
        .await
        .ok();
}

// ─── describeServer ─────────────────────────────────────────────

async fn describe_server(State(state): State<AppState>) -> Json<Value> {
    let invite_required = std::env::var("REQUIRE_INVITE_CODE")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    Json(serde_json::json!({
        "did": format!("did:web:{}", state.config.domain),
        "availableUserDomains": [state.config.domain],
        "inviteCodeRequired": invite_required,
    }))
}

// ─── relay sync: listRepos / getRepo / getLatestCommit ──────────

#[derive(Debug, Deserialize)]
struct ListReposParams {
    cursor: Option<String>,
    limit: Option<i64>,
}

async fn list_repos(
    State(state): State<AppState>,
    Query(params): Query<ListReposParams>,
) -> Result<Json<Value>, crate::error::AppError> {
    let limit = params.limit.unwrap_or(500).clamp(1, 1000);
    let mut rows = sqlx::query_as::<_, (String, String, String, Option<String>)>(
        "SELECT r.did, r.root_cid, r.rev, a.status
         FROM pds_repo_roots r
         JOIN pds_accounts a ON a.did = r.did
         WHERE ($2::text IS NULL OR r.did > $2)
         ORDER BY r.did
         LIMIT $1",
    )
    .bind(limit + 1)
    .bind(params.cursor)
    .fetch_all(&state.db)
    .await?;
    let has_more = rows.len() > limit as usize;
    if has_more {
        rows.pop();
    }
    let cursor = if has_more {
        rows.last().map(|row| row.0.clone())
    } else {
        None
    };
    let repos = rows
        .into_iter()
        .map(|(did, head, rev, status)| serde_json::json!({
            "did": did,
            "head": head,
            "rev": rev,
            "active": status.as_deref() == Some("active"),
            "status": status.filter(|s| s != "active"),
        }))
        .collect::<Vec<_>>();
    Ok(Json(serde_json::json!({"repos": repos, "cursor": cursor})))
}

#[derive(Debug, Deserialize)]
struct GetRepoParams {
    did: String,
    _since: Option<String>,
}

async fn get_repo(
    State(state): State<AppState>,
    Query(params): Query<GetRepoParams>,
) -> Result<Response, crate::error::AppError> {
    tracing::info!("[sync] getRepo did={}", params.did);
    let root: Option<String> = sqlx::query_scalar(
        "SELECT root_cid FROM pds_repo_roots WHERE did = $1",
    )
    .bind(&params.did)
    .fetch_optional(&state.db)
    .await?;
    let root = root.ok_or_else(|| crate::error::AppError::NotFound("repo not found".into()))?;
    let commit_cid = lexicon_cid::Cid::from_str(&root)
        .map_err(|e| crate::error::AppError::Internal(format!("invalid repo root: {e}")))?;
    let storage: Arc<RwLock<dyn rsky_repo::storage::types::RepoStorage>> =
        Arc::new(RwLock::new(SqlxPostgresStorage::new(
            state.db.clone(),
            params.did,
        )));
    let stream = rsky_repo::sync::provider::get_full_repo(storage, commit_cid)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("repo export failed: {e}")))?;
    Response::builder()
        .header("content-type", "application/vnd.ipld.car")
        .body(Body::from_stream(stream))
        .map_err(|e| crate::error::AppError::Internal(format!("repo response failed: {e}")))
}

#[derive(Debug, Deserialize)]
struct GetLatestCommitParams {
    did: String,
}

async fn get_latest_commit(
    State(state): State<AppState>,
    Query(params): Query<GetLatestCommitParams>,
) -> Result<Json<Value>, crate::error::AppError> {
    tracing::info!("[sync] getLatestCommit did={}", params.did);
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT root_cid, rev FROM pds_repo_roots WHERE did = $1",
    )
    .bind(&params.did)
    .fetch_optional(&state.db)
    .await?;
    let (cid, rev) = row.ok_or_else(|| crate::error::AppError::NotFound("repo not found".into()))?;
    Ok(Json(serde_json::json!({"cid": cid, "rev": rev})))
}

// ─── relay sync: getRepoStatus / getBlocks ───────────────────────

/// Parse getBlocks query pairs: `did` present, `cids` repeated or comma-joined.
/// Relay clients send one `cids=` per CID, so a lone CID arrives as a scalar —
/// serde's seq deserialization rejects that shape.
fn collect_block_query(params: Vec<(String, String)>) -> (bool, Vec<String>) {
    let mut wanted: Vec<String> = Vec::new();
    let mut has_did = false;
    for (key, value) in params {
        match key.as_str() {
            "cids" => wanted.extend(
                value.split(',').filter(|s| !s.is_empty()).map(|s| s.to_string()),
            ),
            "did" => has_did |= !value.is_empty(),
            _ => {}
        }
    }
    (has_did, wanted)
}

/// com.atproto.sync.getBlocks — selected repo blocks as a CAR. Relays read
/// individual blocks when verifying a commit instead of fetching the repo.
async fn get_blocks(
    State(state): State<AppState>,
    Query(params): Query<Vec<(String, String)>>,
) -> Result<Response, crate::error::AppError> {
    let (has_did, wanted) = collect_block_query(params);
    if !has_did {
        return Err(crate::error::AppError::BadRequest("did required".into()));
    }
    if wanted.is_empty() {
        return Err(crate::error::AppError::BadRequest("cids required".into()));
    }
    let rows = sqlx::query_as::<_, (String, Vec<u8>)>(
        "SELECT cid, data FROM pds_ipld_blocks WHERE cid = ANY($1)",
    )
    .bind(&wanted)
    .fetch_all(&state.db)
    .await?;
    let mut blocks = rsky_repo::block_map::BlockMap::new();
    for (cid, data) in rows {
        match lexicon_cid::Cid::from_str(&cid) {
            Ok(c) => blocks.set(c, data),
            Err(e) => tracing::warn!("[PDS] getBlocks skipping invalid stored cid {cid}: {e}"),
        }
    }
    let car = rsky_repo::car::blocks_to_car_file(None, blocks)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("block CAR build failed: {e}")))?;
    Response::builder()
        .header("content-type", "application/vnd.ipld.car")
        .body(Body::from(car))
        .map_err(|e| crate::error::AppError::Internal(format!("block response failed: {e}")))
}

/// com.atproto.sync.getRepoStatus — hosting status for one repository.
async fn get_repo_status(
    State(state): State<AppState>,
    Query(params): Query<GetLatestCommitParams>,
) -> Result<Json<Value>, crate::error::AppError> {
    let row = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT a.status, r.rev
         FROM pds_repo_roots r
         JOIN pds_accounts a ON a.did = r.did
         WHERE r.did = $1",
    )
    .bind(&params.did)
    .fetch_optional(&state.db)
    .await?;
    let (status, rev) =
        row.ok_or_else(|| crate::error::AppError::NotFound("repo not found".into()))?;
    let active = status == "active";
    let mut out = serde_json::Map::new();
    out.insert("did".to_string(), params.did.into());
    out.insert("active".to_string(), active.into());
    if let Some(rev) = rev {
        out.insert("rev".to_string(), rev.into());
    }
    if !active {
        out.insert("status".to_string(), status.into());
    }
    Ok(Json(Value::Object(out)))
}

// ─── describeRepo ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct DescribeRepoParams {
    repo: String,
}

#[derive(Debug, Serialize)]
struct DescribeRepoResponse {
    did: String,
    handle: Option<String>,
    did_doc: Value,
    collections: Vec<String>,
    repo_cid: Option<String>,
    rev: Option<String>,
}

async fn describe_repo(
    State(state): State<AppState>,
    Query(params): Query<DescribeRepoParams>,
) -> Result<Json<DescribeRepoResponse>, crate::error::AppError> {
    let did = params.repo.clone();
    let domain = state.config.domain.clone();

    let account = PdsAccount::load(&state.db, &did)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("Account load failed: {e}")))?;

    let storage = Arc::new(RwLock::new(SqlxPostgresStorage::new(
        state.db.clone(),
        did.clone(),
    )));

    let (repo_cid, rev, collections) = match Repo::load(storage, None).await {
        Ok(mut repo) => {
            let cid = repo.cid.to_string();
            let rev = repo.commit.rev.clone();
            let contents = repo.get_contents().await.unwrap_or_default();
            let cols: Vec<String> = contents.keys().cloned().collect();
            (Some(cid), Some(rev), cols)
        }
        Err(_) => (None, None, vec![]),
    };

    let did_doc = if let Some(ref acc) = account {
        serde_json::json!({
            "id": did,
            "verificationMethod": [{
                "id": format!("{}#signing-key", did),
                "type": "Multikey",
                "controller": did,
                "publicKeyMultibase": crate::pds::account::encode_public_key_multibase(&acc.keypair),
            }],
            "service": [
                {
                    "id": format!("{}#atproto_pds", did),
                    "type": "AtprotoPersonalDataServer",
                    "serviceEndpoint": format!("https://{}", domain),
                }
            ]
        })
    } else {
        serde_json::json!({})
    };

    Ok(Json(DescribeRepoResponse {
        did,
        handle: account.as_ref().map(|a| {
            // AT handles are domains: bare usernames are qualified with the
            // server domain so relays can validate handle ↔ DID.
            if a.handle.contains('.') {
                a.handle.clone()
            } else {
                format!("{}.{}", a.handle, domain)
            }
        }),
        did_doc,
        collections,
        repo_cid,
        rev,
    }))
}

// ─── createAccount ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CreateAccountRequest {
    handle: String,
    password: String,
    email: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreateAccountResponse {
    did: String,
    handle: String,
    access_token: String,
    refresh_token: String,
}

async fn create_account(
    State(state): State<AppState>,
    Json(req): Json<CreateAccountRequest>,
) -> Result<Json<CreateAccountResponse>, crate::error::AppError> {
    let domain = state.config.domain.clone();

    // Check if handle already exists
    let existing = PdsAccount::load_by_handle(&state.db, &req.handle)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("Account check failed: {e}")))?;
    if existing.is_some() {
        return Err(crate::error::AppError::Conflict("Handle already taken".into()));
    }

    // Create the PDS account with secp256k1 signing key
    let account = PdsAccount::create(&state.db, &req.handle, &domain, None, &state.config.plc_directory)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("Account creation failed: {e}")))?;

    // Create initial empty repo
    let storage = Arc::new(RwLock::new(SqlxPostgresStorage::new(
        state.db.clone(),
        account.did.clone(),
    )));
    Repo::create(storage, account.did.clone(), &account.keypair, None)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("Repo init failed: {e}")))?;

    let did = account.did.clone();
    let access_token = format!("pds_token_{}_{}", did, chrono::Utc::now().timestamp());
    let refresh_token = format!("pds_refresh_{}_{}", did, chrono::Utc::now().timestamp());

    Ok(Json(CreateAccountResponse {
        did,
        handle: req.handle,
        access_token,
        refresh_token,
    }))
}

// ─── createSession ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CreateSessionRequest {
    handle: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct CreateSessionResponse {
    did: String,
    handle: String,
    access_token: String,
    refresh_token: String,
}

async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, crate::error::AppError> {
    let account = PdsAccount::load_by_handle(&state.db, &req.handle)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("Account load failed: {e}")))?
        .ok_or_else(|| crate::error::AppError::NotFound("Account not found".into()))?;

    // TODO: verify password against hash (Sprint 3 — OAuth). For now, accept any password.
    // The session DID is the account's STORED did (did:plc since 5.4) — the
    // old derived `did:web:{handle}.{domain}` contradicted the minted row.
    let did = account.did.clone();
    let access_token = format!("pds_token_{}_{}", did, chrono::Utc::now().timestamp());
    let refresh_token = format!("pds_refresh_{}_{}", did, chrono::Utc::now().timestamp());

    Ok(Json(CreateSessionResponse {
        did,
        handle: req.handle,
        access_token,
        refresh_token,
    }))
}

// ─── getSession ──────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct GetSessionResponse {
    did: String,
    handle: String,
}

async fn get_session(
    _state: State<AppState>,
) -> Result<Json<GetSessionResponse>, crate::error::AppError> {
    // TODO: extract DID from Authorization header (Sprint 3). For now, stub.
    Ok(Json(GetSessionResponse {
        did: "did:web:placeholder".to_string(),
        handle: "placeholder".to_string(),
    }))
}

// ─── subscribeRepos (WebSocket) ───────────────────────────────

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};

#[derive(Debug, Deserialize)]
struct SubscribeReposParams {
    cursor: Option<i64>,
}

async fn subscribe_repos_ws(
    ws: WebSocketUpgrade,
    Query(params): Query<SubscribeReposParams>,
    State(state): State<AppState>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| subscribe_repos_handler(socket, params.cursor, state.db.clone()))
}

async fn subscribe_repos_handler(mut socket: WebSocket, cursor: Option<i64>, pool: sqlx::PgPool) {
    tracing::info!("subscribeRepos WebSocket connected, cursor: {:?}", cursor);

    // Lead with one `#sync` snapshot per repo to establish current state. This
    // must reach a subscriber with NO backlog to replay — a cursor-less
    // subscriber (starts at the live edge) AND a subscriber whose cursor is at
    // or beyond the current max seq (e.g. a relay reconnecting at `cursor=32`
    // when 32 is the newest commit). Both would otherwise never learn the
    // existing repos' heads and stay `RepoNotSynchronized` forever (the relay
    // waits for events after the cursor, which never come).
    let max_seq: Option<i64> = sqlx::query_scalar("SELECT MAX(seq) FROM pds_firehose_seq")
        .fetch_one(&pool)
        .await
        .ok()
        .flatten();
    let no_backlog = match cursor {
        None => true,
        Some(c) => max_seq.map_or(true, |m| c >= m),
    };
    if no_backlog {
        // Snapshot frames carry the current MAX(seq) so they never regress the
        // relay's host cursor (a seq:0 snapshot forces eternal replay churn).
        let snap_seq = max_seq.unwrap_or(0);
        for msg in build_sync_snapshot(&pool, snap_seq).await {
            if socket.send(Message::Binary(bytes::Bytes::from(msg))).await.is_err() {
                return;
            }
        }
    }

    // If cursor provided, replay past events first
    if let Some(start_seq) = cursor {
        let rows = sqlx::query_as::<_, (i64, String, String, Option<String>, String, serde_json::Value, Vec<u8>, chrono::DateTime<chrono::Utc>, Option<String>, Option<String>)>(
            "SELECT seq, did, commit_cid, prev_cid, rev, ops, blocks, COALESCE(time, NOW()) AS time, prev_data, since
             FROM pds_firehose_seq
             WHERE seq > $1
             ORDER BY seq ASC
             LIMIT 1000",
        )
        .bind(start_seq)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        for (seq, did, commit_cid, prev_cid, rev, ops, blocks, time, prev_data, since) in rows {
            let event = build_commit_event(seq, &did, &commit_cid, prev_cid.as_deref(), &rev, &ops, &blocks, time, prev_data.as_deref(), since.as_deref());
            if let Some(msg) = event {
                if socket.send(Message::Binary(bytes::Bytes::from(msg))).await.is_err() {
                    return;
                }
            }
        }
    }

    // Now stream live events by polling
    let mut last_sent_seq: i64 = cursor.unwrap_or(0);

    // Get current max seq
    let max_seq: Option<i64> = sqlx::query_scalar("SELECT MAX(seq) FROM pds_firehose_seq")
        .fetch_one(&pool)
        .await
        .ok()
        .flatten();
    if let Some(ms) = max_seq {
        last_sent_seq = last_sent_seq.max(ms);
    }

    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let rows = sqlx::query_as::<_, (i64, String, String, Option<String>, String, serde_json::Value, Vec<u8>, chrono::DateTime<chrono::Utc>, Option<String>, Option<String>)>(
            "SELECT seq, did, commit_cid, prev_cid, rev, ops, blocks, COALESCE(time, NOW()) AS time, prev_data, since
             FROM pds_firehose_seq
             WHERE seq > $1
             ORDER BY seq ASC
             LIMIT 100",
        )
        .bind(last_sent_seq)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        if rows.is_empty() {
            // Send a ping to keep connection alive
            if socket.send(Message::Ping(bytes::Bytes::from(vec![0]))).await.is_err() {
                break;
            }
            continue;
        }

        for (seq, did, commit_cid, prev_cid, rev, ops, blocks, time, prev_data, since) in rows {
            let event = build_commit_event(seq, &did, &commit_cid, prev_cid.as_deref(), &rev, &ops, &blocks, time, prev_data.as_deref(), since.as_deref());
            if let Some(msg) = event {
                if socket.send(Message::Binary(bytes::Bytes::from(msg))).await.is_err() {
                    tracing::info!("subscribeRepos WebSocket disconnected");
                    return;
                }
            }
            last_sent_seq = seq;
        }
    }
}

/// CBOR CID link: tag 42 over identity-multibase (0x00) prefixed CID bytes —
/// the DAG-CBOR link form that relay decoders expect.
fn cbor_cid_link(raw: &str) -> Option<ciborium::value::Value> {
    let cid = lexicon_cid::Cid::from_str(raw).ok()?;
    let mut bz = vec![0u8];
    bz.extend_from_slice(&cid.to_bytes());
    Some(ciborium::value::Value::Tag(
        42,
        Box::new(ciborium::value::Value::Bytes(bz)),
    ))
}

/// Build a #commit firehose frame for the firehose.
/// Format: [header CBOR] [body CBOR]
///
/// The body is DAG-CBOR per the AT Protocol event-stream wire format: CID
/// fields are tag-42 links and `blocks` is a byte string. Building the body
/// through serde_json produced text CIDs and an integer-array `blocks`, which
/// relays reject — the CBOR value tree is constructed directly instead.
fn build_commit_event(
    seq: i64,
    did: &str,
    commit_cid: &str,
    prev_cid: Option<&str>,
    rev: &str,
    ops: &serde_json::Value,
    blocks: &[u8],
    time: chrono::DateTime<chrono::Utc>,
    prev_data: Option<&str>,
    since: Option<&str>,
) -> Option<Vec<u8>> {
    use ciborium::value::Value as Cbor;

    let mut op_entries = Vec::new();
    for op in ops.as_array()? {
        let path = op.get("path")?.as_str()?.to_string();
        let action = op.get("action")?.as_str()?.to_string();
        let cid = match op.get("cid").and_then(|v| v.as_str()) {
            Some(raw) => cbor_cid_link(raw)?,
            None => Cbor::Null,
        };
        op_entries.push(Cbor::Map(vec![
            (Cbor::Text("action".to_string()), Cbor::Text(action)),
            (Cbor::Text("cid".to_string()), cid),
            (Cbor::Text("path".to_string()), Cbor::Text(path)),
        ]));
    }

    // Header: {op: 1, t: "#commit"}
    let header = Cbor::Map(vec![
        (Cbor::Text("op".to_string()), Cbor::Integer(1.into())),
        (Cbor::Text("t".to_string()), Cbor::Text("#commit".to_string())),
    ]);
    let prev = match prev_cid {
        Some(raw) => cbor_cid_link(raw)?,
        None => Cbor::Null,
    };
    // `prevData` is required on every commit after the first (indigo verify.go:
    // prevRepo != nil → prevData MUST be present); null on the first commit.
    let prev_data_v = match prev_data {
        Some(raw) => cbor_cid_link(raw)?,
        None => Cbor::Null,
    };
    // `since` is the previous rev; null on the first commit.
    let since_v = match since {
        Some(raw) => Cbor::Text(raw.to_string()),
        None => Cbor::Null,
    };
    // Body: SubscribeReposCommit as DAG-CBOR
    let body = Cbor::Map(vec![
        (Cbor::Text("blobs".to_string()), Cbor::Array(Vec::new())),
        (Cbor::Text("blocks".to_string()), Cbor::Bytes(blocks.to_vec())),
        (Cbor::Text("commit".to_string()), cbor_cid_link(commit_cid)?),
        (Cbor::Text("ops".to_string()), Cbor::Array(op_entries)),
        (Cbor::Text("prev".to_string()), prev),
        (Cbor::Text("prevData".to_string()), prev_data_v),
        (Cbor::Text("rebase".to_string()), Cbor::Bool(false)),
        (Cbor::Text("repo".to_string()), Cbor::Text(did.to_string())),
        (Cbor::Text("rev".to_string()), Cbor::Text(rev.to_string())),
        (Cbor::Text("seq".to_string()), Cbor::Integer(seq.into())),
        (Cbor::Text("since".to_string()), since_v),
        (Cbor::Text("time".to_string()), Cbor::Text(time.to_rfc3339())),
        (Cbor::Text("tooBig".to_string()), Cbor::Bool(false)),
    ]);

    let mut header_bytes = Vec::new();
    ciborium::ser::into_writer(&header, &mut header_bytes).ok()?;
    let mut body_bytes = Vec::new();
    ciborium::ser::into_writer(&body, &mut body_bytes).ok()?;

    let mut out = Vec::with_capacity(header_bytes.len() + body_bytes.len());
    out.extend_from_slice(&header_bytes);
    out.extend_from_slice(&body_bytes);
    Some(out)
}

/// Build a `#sync` firehose frame: the repo's current state snapshot at
/// `seq`, so a fresh subscriber learns an existing account's head instead of
/// waiting for a future commit. Body is DAG-CBOR per `SubscribeReposSync`.
fn build_sync_event(
    seq: i64,
    did: &str,
    rev: &str,
    blocks: &[u8],
    time: chrono::DateTime<chrono::Utc>,
) -> Option<Vec<u8>> {
    use ciborium::value::Value as Cbor;

    let header = Cbor::Map(vec![
        (Cbor::Text("op".to_string()), Cbor::Integer(1.into())),
        (Cbor::Text("t".to_string()), Cbor::Text("#sync".to_string())),
    ]);
    let body = Cbor::Map(vec![
        (Cbor::Text("blocks".to_string()), Cbor::Bytes(blocks.to_vec())),
        (Cbor::Text("did".to_string()), Cbor::Text(did.to_string())),
        (Cbor::Text("rev".to_string()), Cbor::Text(rev.to_string())),
        (Cbor::Text("seq".to_string()), Cbor::Integer(seq.into())),
        (Cbor::Text("time".to_string()), Cbor::Text(time.to_rfc3339())),
    ]);

    let mut header_bytes = Vec::new();
    ciborium::ser::into_writer(&header, &mut header_bytes).ok()?;
    let mut body_bytes = Vec::new();
    ciborium::ser::into_writer(&body, &mut body_bytes).ok()?;

    let mut out = Vec::with_capacity(header_bytes.len() + body_bytes.len());
    out.extend_from_slice(&header_bytes);
    out.extend_from_slice(&body_bytes);
    Some(out)
}

/// Current head snapshot per repo as `#sync` frames, one per managed account.
/// Sent when a subscriber connects without a cursor so an existing repo's head
/// is visible immediately (a relay cannot synchronize an account whose commits
/// all predate its connection).
///
/// Each frame's `blocks` is a CAR rooted at the repo's commit block (the relay's
/// `LoadCommitFromCAR` stops at the root commit block, so the commit block alone
/// suffices) — a bare block is not a valid CAR and gets dropped. `seq` is the
/// current `MAX(seq)`, NOT 0: a `seq:0` snapshot regresses the relay's host
/// cursor and forces eternal replay churn. `_instance` is excluded — it is not a
/// real user and the relay logs `invalid DID` and drops it.
async fn build_sync_snapshot(pool: &sqlx::PgPool, max_seq: i64) -> Vec<Vec<u8>> {
    let rows = sqlx::query_as::<_, (String, String, String, Vec<u8>, chrono::DateTime<chrono::Utc>)>(
        "SELECT r.did, r.rev, r.root_cid, COALESCE(
            (SELECT data FROM pds_ipld_blocks b WHERE b.cid = r.root_cid), ''::bytea
         ) AS root_block, COALESCE(r.updated_at, NOW()) AS time
         FROM pds_repo_roots r
         JOIN pds_accounts a ON a.did = r.did
         WHERE a.status = 'active' AND a.handle <> '_instance'
         ORDER BY r.did",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut frames = Vec::with_capacity(rows.len());
    for (did, rev, root_cid, root_block, time) in rows {
        if root_block.is_empty() {
            tracing::warn!("[PDS] sync snapshot skipping {did}: no root block stored");
            continue;
        }
        // Wrap the root commit block in a CAR rooted at root_cid.
        let root = match lexicon_cid::Cid::from_str(&root_cid) {
            Ok(cid) => cid,
            Err(e) => {
                tracing::warn!("[PDS] sync snapshot skipping {did}: bad root cid {root_cid}: {e}");
                continue;
            }
        };
        let mut bm = rsky_repo::block_map::BlockMap::new();
        bm.set(root, root_block);
        let car = match rsky_repo::car::blocks_to_car_file(Some(&root), bm).await {
            Ok(car) => car,
            Err(e) => {
                tracing::warn!("[PDS] sync snapshot skipping {did}: CAR build failed: {e}");
                continue;
            }
        };
        if let Some(frame) = build_sync_event(max_seq, &did, &rev, &car, time) {
            frames.push(frame);
        }
    }
    frames
}

#[cfg(test)]
mod frame_tests {
    use super::*;
    use ciborium::value::Value as Cbor;
    use std::str::FromStr;

    fn map_get<'a>(v: &'a Cbor, key: &str) -> &'a Cbor {
        let Cbor::Map(entries) = v else {
            panic!("expected CBOR map, got {v:?}");
        };
        entries
            .iter()
            .find(|(k, _)| k == &Cbor::Text(key.to_string()))
            .map(|(_, v)| v)
            .unwrap_or_else(|| panic!("missing key {key}"))
    }

    /// CID links on the wire are CBOR tag 42 over identity-multibase-prefixed
    /// CID bytes — the form every relay decoder expects.
    fn assert_cid_link(v: &Cbor, raw: &str) {
        let cid = lexicon_cid::Cid::from_str(raw).expect("valid fixture CID");
        let mut expected = vec![0u8];
        expected.extend_from_slice(&cid.to_bytes());
        match v {
            Cbor::Tag(42, inner) => assert_eq!(
                **inner,
                Cbor::Bytes(expected),
                "tag-42 payload must be 0x00-prefixed CID bytes for {raw}"
            ),
            other => panic!("expected tag-42 CID link for {raw}, got {other:?}"),
        }
    }

    /// Wire contract for #commit frames: CID fields are tag-42 links and
    /// `blocks` is a byte string. Text CIDs / integer-array blocks make relays
    /// reject the stream and drop the account from their index.
    #[test]
    fn commit_frame_encodes_cids_and_blocks_as_dag_cbor() {
        let commit = "bafyreidqwopsxprkicjbrkk7kqz6xrp2cnit6qm5wu37hpxsl55hv4qaoi";
        let prev = "bafyreiao5k6cmcddfppk55tozeycm7ql7v5dd4fqv5x4m3y5p7lnwrlyje";
        let prev_data = "bafyreig7ui2hp3cmt7fvbvwaz66c2ldhkywwuw2tfe7u2nv2edvqnvq6sq";
        let like_cid = "bafyreig7ui2hp3cmt7fvbvwaz66c2ldhkywwuw2tfe7u2nv2edvqnvq6sq";
        let car = vec![0x0a, 0x11, 0x22, 0x33];
        let ops = serde_json::json!([
            {"path": "app.bsky.feed.like/3n23czzu42tjo", "action": "create", "cid": like_cid},
            {"path": "app.bsky.feed.like/3n23czzu42tjo", "action": "delete", "cid": null}
        ]);
        let time = chrono::DateTime::parse_from_rfc3339("2026-11-12T01:29:59Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let frame = build_commit_event(
            7,
            "did:web:alice.test",
            commit,
            Some(prev),
            "3mzxrev",
            &ops,
            &car,
            time,
            Some(prev_data),
            Some("3mzxprev"),
        )
        .expect("frame encodes");

        // Consumers split one CBOR header from the remainder of the message.
        let mut cursor = std::io::Cursor::new(&frame[..]);
        let header: Cbor = ciborium::de::from_reader(&mut cursor).expect("header parses");
        let body: Cbor = ciborium::de::from_reader(&mut std::io::Cursor::new(
            &frame[cursor.position() as usize..],
        ))
        .expect("body parses");

        assert_eq!(map_get(&header, "t"), &Cbor::Text("#commit".into()));
        assert_eq!(map_get(&header, "op"), &Cbor::Integer(1.into()));
        assert_eq!(map_get(&body, "seq"), &Cbor::Integer(7.into()));
        assert_eq!(map_get(&body, "repo"), &Cbor::Text("did:web:alice.test".into()));
        assert_eq!(map_get(&body, "rev"), &Cbor::Text("3mzxrev".into()));
        assert_eq!(map_get(&body, "time"), &Cbor::Text(time.to_rfc3339()));
        assert_eq!(map_get(&body, "blocks"), &Cbor::Bytes(car));
        assert_eq!(map_get(&body, "blobs"), &Cbor::Array(Vec::new()));
        assert_eq!(map_get(&body, "since"), &Cbor::Text("3mzxprev".into()));
        assert_eq!(map_get(&body, "rebase"), &Cbor::Bool(false));
        assert_eq!(map_get(&body, "tooBig"), &Cbor::Bool(false));
        assert_cid_link(map_get(&body, "commit"), commit);
        assert_cid_link(map_get(&body, "prev"), prev);
        // R2-firehose Fix 3: prevData is a tag-42 CID link on every commit after
        // the first — the relay drops a commit whose prevData is absent/typed.
        assert_cid_link(map_get(&body, "prevData"), prev_data);

        let Cbor::Array(op_entries) = map_get(&body, "ops") else {
            panic!("ops must be an array");
        };
        assert_eq!(op_entries.len(), 2);
        let Cbor::Map(_) = &op_entries[0] else {
            panic!("op must be a map");
        };
        assert_eq!(map_get(&op_entries[0], "action"), &Cbor::Text("create".into()));
        assert_eq!(
            map_get(&op_entries[0], "path"),
            &Cbor::Text("app.bsky.feed.like/3n23czzu42tjo".into())
        );
        assert_cid_link(map_get(&op_entries[0], "cid"), like_cid);
        assert_eq!(map_get(&op_entries[1], "cid"), &Cbor::Null, "delete ops carry no cid");
    }

    /// Relay clients send one `cids=` per CID; a single-element request must
    /// parse just like repeated or comma-joined values.
    #[test]
    fn block_query_accepts_single_and_repeated_cids() {
        let (has_did, cids) = collect_block_query(vec![
            ("did".to_string(), "did:web:alice.test".to_string()),
            ("cids".to_string(), "bafyaa".to_string()),
        ]);
        assert!(has_did, "did is required by the lexicon");
        assert_eq!(cids, vec!["bafyaa".to_string()], "single scalar cids value");

        let (has_did, cids) = collect_block_query(vec![
            ("did".to_string(), "did:web:alice.test".to_string()),
            ("cids".to_string(), "bafyaa".to_string()),
            ("cids".to_string(), "bafybb,bafycc".to_string()),
        ]);
        assert!(has_did);
        assert_eq!(
            cids,
            vec!["bafyaa".to_string(), "bafybb".to_string(), "bafycc".to_string()],
            "repeated and comma-joined cids merge in order"
        );

        let (has_did, cids) = collect_block_query(vec![("cids".to_string(), "bafyaa".to_string())]);
        assert!(!has_did, "missing did is reported");
        assert_eq!(cids, vec!["bafyaa".to_string()]);
    }

    /// `#sync` snapshot frames must carry the repo head as a byte-string block
    /// so a fresh subscriber can establish existing accounts' state.
    #[test]
    fn sync_event_carries_repo_head_snapshot() {
        let blocks = vec![0x0a, 0x11, 0x22];
        let time = chrono::DateTime::parse_from_rfc3339("2026-11-12T02:30:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let frame = build_sync_event(0, "did:web:alice.test", "3mzxrev", &blocks, time)
            .expect("sync frame encodes");

        let mut cursor = std::io::Cursor::new(&frame[..]);
        let header: Cbor = ciborium::de::from_reader(&mut cursor).expect("header parses");
        let body: Cbor = ciborium::de::from_reader(&mut std::io::Cursor::new(
            &frame[cursor.position() as usize..],
        ))
        .expect("body parses");

        assert_eq!(map_get(&header, "t"), &Cbor::Text("#sync".into()));
        assert_eq!(map_get(&header, "op"), &Cbor::Integer(1.into()));
        assert_eq!(map_get(&body, "did"), &Cbor::Text("did:web:alice.test".into()));
        assert_eq!(map_get(&body, "rev"), &Cbor::Text("3mzxrev".into()));
        assert_eq!(map_get(&body, "seq"), &Cbor::Integer(0.into()));
        assert_eq!(map_get(&body, "time"), &Cbor::Text(time.to_rfc3339()));
        assert_eq!(map_get(&body, "blocks"), &Cbor::Bytes(blocks));
    }

    /// Split one CBOR header from the body of a frame.
    fn split_frame(frame: &[u8]) -> (Cbor, Cbor) {
        let mut cursor = std::io::Cursor::new(frame);
        let header: Cbor = ciborium::de::from_reader(&mut cursor).expect("header parses");
        let body: Cbor = ciborium::de::from_reader(&mut std::io::Cursor::new(
            &frame[cursor.position() as usize..],
        ))
        .expect("body parses");
        (header, body)
    }

    /// Fix 1 (red-then-green): the `blocks` of a #commit frame must be a real
    /// CAR v1 with a NON-EMPTY `roots` — indigo's car.go returns `ErrNoRoot`
    /// on `roots: []`, dropping the event and leaving the account
    /// `RepoNotSynchronized` forever. Parse the frame's block bytes and assert
    /// the CAR has exactly one root equal to the commit CID.
    #[tokio::test]
    async fn commit_frame_blocks_are_a_car_with_nonempty_root() {
        let commit_cid = lexicon_cid::Cid::from_str(
            "bafyreidqwopsxprkicjbrkk7kqz6xrp2cnit6qm5wu37hpxsl55hv4qaoi",
        )
        .expect("commit cid");
        // A commit block (DAG-CBOR map) plus a child block, as format_commit emits.
        let commit_block = rsky_repo::block_map::BlockMap::new();
        let mut bm = commit_block;
        let child_cid = bm
            .add(serde_json::json!({"data": "leaf"}))
            .expect("leaf block");
        bm.set(commit_cid, vec![0xa1, 0x64, 0x74, 0x65, 0x73, 0x74]); // placeholder bytes

        // This is exactly what emit_commit does: a CAR rooted at the commit.
        let car =
            rsky_repo::car::blocks_to_car_file(Some(&commit_cid), bm).await.expect("car builds");

        let ops = serde_json::json!([{"path": "app.bsky.feed.like/x", "action": "create", "cid": child_cid.to_string()}]);
        let time = chrono::Utc::now();
        let frame = build_commit_event(
            1,
            "did:web:alice.test",
            &commit_cid.to_string(),
            None,
            "3mzxrev",
            &ops,
            &car,
            time,
            None,
            None,
        )
        .expect("frame encodes");

        let (_header, body) = split_frame(&frame);
        let Cbor::Bytes(car_bytes) = map_get(&body, "blocks").clone() else {
            panic!("blocks must be a byte string");
        };

        // Parse the CAR: roots must be non-empty and equal the commit CID.
        let parsed = rsky_repo::car::read_car(car_bytes.clone())
            .await
            .expect("frame blocks must parse as a CAR");
        assert!(!parsed.roots.is_empty(), "CAR roots must be non-empty (ErrNoRoot otherwise)");
        assert_eq!(parsed.roots[0], commit_cid, "CAR root must be the commit CID");
        assert!(parsed.blocks.get(commit_cid).is_some(), "CAR must contain the commit block");
    }

    /// Fix 3: the FIRST commit has no previous repo — `prevData` and `since`
    /// must be CBOR null (the relay only requires prevData on commits after the
    /// first; a wrongly-typed first-commit prevData is rejected).
    #[test]
    fn first_commit_frame_has_null_prevdata_and_since() {
        let commit_cid = "bafyreidqwopsxprkicjbrkk7kqz6xrp2cnit6qm5wu37hpxsl55hv4qaoi";
        let ops = serde_json::json!([{"path": "app.bsky.feed.like/x", "action": "create", "cid": "bafyreig7ui2hp3cmt7fvbvwaz66c2ldhkywwuw2tfe7u2nv2edvqnvq6sq"}]);
        let frame = build_commit_event(
            1,
            "did:web:alice.test",
            commit_cid,
            None,
            "3mzxrev",
            &ops,
            &vec![0x0a, 0x11],
            chrono::Utc::now(),
            None,
            None,
        )
        .expect("frame encodes");
        let (_header, body) = split_frame(&frame);
        assert_eq!(map_get(&body, "prevData"), &Cbor::Null, "first commit has no prevData");
        assert_eq!(map_get(&body, "since"), &Cbor::Null, "first commit has no since");
    }

    /// Fix 2 (red-then-green): a #sync frame's `blocks` must be a CAR with the
    /// root commit block, not a bare DAG-CBOR block. Build the exact CAR the
    /// snapshot path builds and assert it parses with one root.
    #[tokio::test]
    async fn sync_frame_blocks_are_a_car_rooted_at_the_commit() {
        let root_cid = lexicon_cid::Cid::from_str(
            "bafyreidqwopsxprkicjbrkk7kqz6xrp2cnit6qm5wu37hpxsl55hv4qaoi",
        )
        .expect("root cid");
        let mut bm = rsky_repo::block_map::BlockMap::new();
        bm.set(root_cid, vec![0xa1, 0x64, 0x74, 0x65, 0x73, 0x74]);
        let car = rsky_repo::car::blocks_to_car_file(Some(&root_cid), bm)
            .await
            .expect("car builds");

        let frame = build_sync_event(99, "did:web:alice.test", "3mzxrev", &car, chrono::Utc::now())
            .expect("sync frame encodes");
        let (_header, body) = split_frame(&frame);
        // Fix 4: the snapshot carries a real seq (the current MAX(seq)), never 0.
        assert_eq!(map_get(&body, "seq"), &Cbor::Integer(99.into()), "snapshot seq is the live max, not 0");

        let Cbor::Bytes(car_bytes) = map_get(&body, "blocks").clone() else {
            panic!("sync blocks must be a byte string");
        };
        let parsed = rsky_repo::car::read_car(car_bytes.clone()).await.expect("sync blocks parse as CAR");
        assert!(!parsed.roots.is_empty(), "sync CAR roots must be non-empty");
        assert_eq!(parsed.roots[0], root_cid, "sync CAR root must be the commit");
    }

    /// Fix 4 (DB): snapshot frames carry MAX(seq) and are monotonic — every
    /// frame's seq equals the max, never 0, so the relay cursor never regresses.
    /// Fix 5 (DB): `_instance` is excluded from the snapshot output.
    #[tokio::test]
    async fn snapshot_uses_max_seq_and_excludes_instance() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else {
            eprintln!("[test] snapshot_uses_max_seq_and_excludes_instance skipped: DATABASE_URL unset — asserted nothing");
            return;
        };
        let suffix = crate::ap::testutil::unique();
        // Seed two managed accounts with repo roots + blocks, plus _instance.
        let max_seq: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM pds_firehose_seq")
            .fetch_one(&pool)
            .await
            .expect("max seq");

        // Build a root block + repo root for a real account and for _instance.
        for (handle, did) in [
            (format!("alice{suffix}"), format!("did:web:alice{suffix}.test")),
            ("_instance".to_string(), format!("did:web:_instance.test.bytescape.social")),
        ] {
            let root_cid = lexicon_cid::Cid::from_str(
                "bafyreidqwopsxprkicjbrkk7kqz6xrp2cnit6qm5wu37hpxsl55hv4qaoi",
            )
            .expect("cid");
            let root_block: Vec<u8> = vec![0xa1, 0x64, 0x74, 0x65, 0x73, 0x74];
            let user = crate::db::users::UserRow::create_active(
                &pool,
                &format!("snap_{suffix}_{handle}"),
                "",
                "x",
                "x",
            )
            .await
            .expect("create fixture user");
            sqlx::query("INSERT INTO pds_accounts (did, handle, signing_key_private, signing_key_public, status, user_id) VALUES ($1, $2, $3, $4, 'active', $5) ON CONFLICT (did) DO NOTHING")
                .bind(&did)
                .bind(&handle)
                .bind(&[0x22u8; 32][..])
                .bind("k256pub")
                .bind(user.id)
                .execute(&pool).await.expect("seed account");
            sqlx::query("INSERT INTO pds_repo_roots (did, rev, root_cid, updated_at) VALUES ($1, '3mzxrev', $2, NOW()) ON CONFLICT (did) DO UPDATE SET rev = EXCLUDED.rev, root_cid = EXCLUDED.root_cid")
                .bind(&did)
                .bind(root_cid.to_string())
                .execute(&pool).await.expect("seed root");
            sqlx::query("INSERT INTO pds_ipld_blocks (cid, did, data, rev) VALUES ($1, $2, $3, $4) ON CONFLICT (cid) DO NOTHING")
                .bind(root_cid.to_string())
                .bind(&did)
                .bind(&root_block[..])
                .bind("3mzxrev")
                .execute(&pool).await.expect("seed block");
        }

        let frames = build_sync_snapshot(&pool, max_seq).await;
        assert!(!frames.is_empty(), "snapshot must produce frames");
        for frame in &frames {
            let (_h, body) = split_frame(frame);
            let Cbor::Integer(seq) = map_get(&body, "seq").clone() else {
                panic!("sync seq must be an integer");
            };
            assert_eq!(seq, max_seq.into(), "every snapshot frame carries the current MAX(seq), never 0");
            let Cbor::Text(did) = map_get(&body, "did").clone() else {
                panic!("sync did must be text");
            };
            assert!(!did.contains("_instance"), "_instance must be excluded from the relay-facing snapshot: {did}");
        }
    }
}