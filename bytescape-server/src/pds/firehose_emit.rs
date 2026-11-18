//! Firehose emit — records commit events to pds_firehose_seq and notifies subscribers.

use anyhow::{anyhow, Result};
use rsky_repo::types::{CommitData, CommitAction};
use serde_json::{json, Value};
use sqlx::PgPool;

/// A firehose event ready to be sent to subscribers.
#[derive(Debug, Clone)]
pub struct FirehoseEvent {
    pub seq: i64,
    pub did: String,
    pub commit_cid: String,
    pub prev_cid: Option<String>,
    pub rev: String,
    pub ops: Vec<Value>,
    pub blocks: Vec<u8>,
    pub time: chrono::DateTime<chrono::Utc>,
}

/// Record a firehose event in the sequence table.
/// Called after a successful repo commit.
///
/// The `blocks` payload is a real CAR v1 file (`rsky_repo::car::blocks_to_car_file`)
/// rooted at the commit — NOT a hand-rolled "CAR-like" container. Relays parse
/// every `#commit`/`#sync` frame's `blocks` as a CAR and require a non-empty
/// `roots` (indigo `car.go` `ErrNoRoot`); a `roots: []` header or a bare DAG-CBOR
/// block is dropped, the account never gets a repo row, and `getLatestCommit`
/// stays `RepoNotSynchronized` forever. The commit block alone suffices — the
/// relay's `LoadCommitFromCAR` stops at the root commit block.
pub async fn emit_commit(
    pool: &PgPool,
    did: &str,
    commit: &CommitData,
    ops: Vec<(CommitAction, String, Option<lexicon_cid::Cid>)>,
    prev_data: Option<lexicon_cid::Cid>,
) -> Result<FirehoseEvent> {
    // Canonical CAR v1 with the commit as root (non-empty roots). commit.new_blocks
    // includes the commit block itself (format_commit sets it).
    let blocks_bytes =
        rsky_repo::car::blocks_to_car_file(Some(&commit.cid), commit.new_blocks.clone()).await?;

    // `prevData` (the previous commit's MST-root `data` CID) is REQUIRED on every
    // commit whose `prev` is set — indigo verify.go enforces `prevRepo != nil →
    // prevData MUST be present`. It is threaded in by the caller (captured from
    // `repo.commit.data` BEFORE apply_commit) because apply_commit deletes the
    // previous commit block (format_commit adds it to removed_cids), so it cannot
    // be looked up from the DB at emit time. `since` is the previous rev, already
    // carried on CommitData.

    // Build ops JSON
    let ops_json: Vec<Value> = ops
        .iter()
        .map(|(action, path, cid)| {
            json!({
                "path": path,
                "action": match action {
                    CommitAction::Create => "create",
                    CommitAction::Update => "update",
                    CommitAction::Delete => "delete",
                },
                "cid": cid.as_ref().map(|c| c.to_string()),
            })
        })
        .collect();

    let time = chrono::Utc::now();

    // Insert into pds_firehose_seq — seq is auto-incremented. `prev_data` (the
    // previous commit's `data` CID) and `since` (the previous rev) are persisted
    // so replayed events can carry them without re-deriving from block history.
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO pds_firehose_seq (did, commit_cid, prev_cid, rev, ops, blocks, time, prev_data, since)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING seq",
    )
    .bind(did)
    .bind(commit.cid.to_string())
    .bind(commit.prev.as_ref().map(|c| c.to_string()))
    .bind(&commit.rev)
    .bind(sqlx::types::Json(&ops_json))
    .bind(&blocks_bytes)
    .bind(time)
    .bind(prev_data.map(|c| c.to_string()))
    .bind(&commit.since)
    .fetch_one(pool)
    .await?;

    let event = FirehoseEvent {
        seq: row.0,
        did: did.to_string(),
        commit_cid: commit.cid.to_string(),
        prev_cid: commit.prev.as_ref().map(|c| c.to_string()),
        rev: commit.rev.clone(),
        ops: ops_json,
        blocks: blocks_bytes,
        time,
    };

    // Notify subscribers via PostgreSQL LISTEN/NOTIFY
    sqlx::query("SELECT pg_notify('firehose', $1)")
        .bind(event.seq.to_string())
        .execute(pool)
        .await?;

    Ok(event)
}