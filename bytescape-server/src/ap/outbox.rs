//! ActivityPub outbox (Phase C): `GET /@{username}/outbox`.
//!
//! Serves the account's POST stream as an AS2 `OrderedCollection` plus
//! bounded `OrderedCollectionPage`s, newest first. Posts here means the
//! Mastodon convention: `Create` and `Announce` activities only —
//! Likes/Undos/Follows are delivery bookkeeping, not posts (rendering
//! clients ignore them; the owner saw "22 posts" with an empty posts tab
//! because the 22 items were live Like/Undo rows). Paging is keyset over
//! `(created_at, id)` — stable order, no duplicated or skipped ids across
//! pages regardless of concurrent inserts.
//!
//! The served set and `totalItems` come from ONE three-source definition
//! ([`stream_count`] and [`stream_page`] run identical predicates):
//!   1. bridged post activities — the AP bridge's `Create`/`Announce` rows
//!      (its AT-record → AP-object conversion);
//!   2. local `posts` rows not yet bridged — synthesized as `Create(Note)`;
//!   3. `appview_posts` rows authored by the account's OWN AT identities,
//!      not yet bridged — synthesized as `Create(Note)` under the same object
//!      id convention as the bridge.
//! Posts authored by other actors (ingested remote posts — even the same
//! person's posts on another instance) are not ours to serve and are
//! excluded: the outbox never claims another actor's objects. The count
//! therefore always equals exactly the number of posts the pages serve.

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::ap::store::AS_PUBLIC;
use crate::db::users::UserRow;
use crate::error::AppError;
use crate::AppState;

/// Activities per outbox page.
const PAGE_SIZE: i64 = 20;

#[derive(Debug, Deserialize)]
pub struct OutboxQuery {
    /// Absent → the collection document. `first`/`true` → newest page.
    /// `last` → oldest page. Otherwise a keyset cursor from a `next` link.
    pub page: Option<String>,
}

/// GET /@{username}/outbox
pub async fn outbox_handler(
    State(state): State<AppState>,
    Path(username): Path<String>,
    Query(query): Query<OutboxQuery>,
) -> Result<Response, AppError> {
    if !crate::ap::ap_actors_enabled() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let user = UserRow::find_ap_enabled_by_username(&state.db, &username)
        .await
        .map_err(|e| AppError::Internal(format!("db: {e}")))?
        .ok_or_else(|| AppError::NotFound("no such actor".into()))?;
    let actor_id = user
        .ap_actor_id
        .clone()
        .unwrap_or_else(|| crate::ap::actor::canonical_actor_url(&state.config.domain, &user.username));
    let outbox_url = format!("{}/outbox", actor_id);

    // totalItems is the exact size of the served stream — never an
    // approximation of it.
    let total = stream_count(&state, &actor_id, user.id).await?;

    let Some(token) = query.page else {
        // The collection document: counts + page entry points.
        let doc = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": outbox_url,
            "type": "OrderedCollection",
            "totalItems": total,
            "first": { "id": format!("{outbox_url}?page=first"), "type": "OrderedCollectionPage" },
            "last": { "id": format!("{outbox_url}?page=last"), "type": "OrderedCollectionPage" },
        });
        return Ok(jsonld(doc));
    };

    // A bounded page of the merged post stream.
    let (entries, more) = match token.as_str() {
        "first" | "true" => stream_page(&state, &actor_id, user.id, None, false).await?,
        "last" => {
            // Oldest first internally, presented newest-first like every page.
            let (mut entries, _) = stream_page(&state, &actor_id, user.id, None, true).await?;
            entries.reverse();
            (entries, false)
        }
        _ => {
            let cursor = decode_cursor(&token)?;
            stream_page(&state, &actor_id, user.id, Some(cursor), false).await?
        }
    };

    let mut items: Vec<Value> = Vec::with_capacity(entries.len());
    let mut last: Option<(chrono::DateTime<chrono::Utc>, String)> = None;
    for e in &entries {
        last = Some((e.ts, e.id.clone()));
        items.push(e.payload.clone());
    }
    let mut doc = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{outbox_url}?page={token}"),
        "type": "OrderedCollectionPage",
        "partOf": outbox_url,
        "orderedItems": items,
    });
    if let Some((ts, id)) = last {
        if more {
            doc["next"] = json!(format!("{outbox_url}?page={}", encode_cursor(ts, &id)));
        }
    }
    Ok(jsonld(doc))
}

/// One post-stream entry: keyset key `(created_at, id)` + ready AS2 payload.
struct Entry {
    ts: chrono::DateTime<chrono::Utc>,
    id: String,
    payload: Value,
}

/// Exact size of the served post stream. The predicates here mirror
/// [`stream_page`] source-for-source; a test asserts count == served pages.
async fn stream_count(
    state: &AppState,
    actor_id: &str,
    user_id: uuid::Uuid,
) -> Result<i64, AppError> {
    let dids = own_at_dids(state, user_id).await?;
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ( \
           SELECT a.id FROM ap_activities a \
            WHERE a.actor_id = $1 AND a.direction = 'out' AND a.undone_at IS NULL \
              AND a.activity_type IN ('Create','Announce') \
              AND ((a.payload->'to') ? $2 OR (a.payload->'cc') ? $2) \
           UNION ALL \
           SELECT 'post:' || p.id::text FROM posts p \
            WHERE p.user_id = $3 AND p.visibility = 'public' \
              AND (p.ap_object_id IS NULL OR NOT EXISTS ( \
                   SELECT 1 FROM ap_activities a \
                    WHERE a.object_id = p.ap_object_id AND a.direction = 'out')) \
           UNION ALL \
           SELECT ap.uri FROM appview_posts ap \
            WHERE ap.author_did = ANY($4) \
              AND NOT EXISTS (SELECT 1 FROM ap_activities a WHERE a.local_uri = ap.uri) \
         ) stream",
    )
    .bind(actor_id)
    .bind(AS_PUBLIC)
    .bind(user_id)
    .bind(&dids)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Internal(format!("db: {e}")))?;
    Ok(n)
}

/// One merged page of the post stream, bounded to [`PAGE_SIZE`]. Each source
/// is fetched with the same keyset and LIMIT+1, then merged — the true top-K
/// of the union is always contained in the union of the per-source top-Ks.
/// `before` is the exclusive keyset start; `ascending` serves the `last`
/// page (oldest items). The bool is "more items exist beyond this page".
async fn stream_page(
    state: &AppState,
    actor_id: &str,
    user_id: uuid::Uuid,
    before: Option<(chrono::DateTime<chrono::Utc>, String)>,
    ascending: bool,
) -> Result<(Vec<Entry>, bool), AppError> {
    let dids = own_at_dids(state, user_id).await?;
    let order = if ascending { "ASC" } else { "DESC" };
    let cmp = if ascending { ">" } else { "<" };
    let limit = PAGE_SIZE + 1;
    let mut entries: Vec<Entry> = Vec::new();

    // 1. Bridged post activities (the AP bridge's Create/Announce output).
    {
        let sql = format!(
            "SELECT id, created_at, payload FROM ap_activities \
             WHERE actor_id = $1 AND direction = 'out' AND undone_at IS NULL \
               AND activity_type IN ('Create','Announce') \
               AND ((payload->'to') ? $2 OR (payload->'cc') ? $2) \
               {keyset} ORDER BY created_at {order}, id {order} LIMIT {limit}",
            keyset = if before.is_some() {
                format!("AND (created_at, id) {cmp} ($3, $4)")
            } else {
                String::new()
            },
        );
        let rows: Vec<(String, chrono::DateTime<chrono::Utc>, Value)> = match &before {
            Some((ts, id)) => sqlx::query_as(&sql)
                .bind(actor_id)
                .bind(AS_PUBLIC)
                .bind(ts)
                .bind(id)
                .fetch_all(&state.db)
                .await
                .map_err(|e| AppError::Internal(format!("db: {e}")))?,
            None => sqlx::query_as(&sql)
                .bind(actor_id)
                .bind(AS_PUBLIC)
                .fetch_all(&state.db)
                .await
                .map_err(|e| AppError::Internal(format!("db: {e}")))?,
        };
        entries.extend(rows.into_iter().map(|(id, ts, payload)| Entry { ts, id, payload }));
    }

    // 2. Local posts not yet bridged → synthesized Create(Note).
    {
        let sql = format!(
            "SELECT 'post:' || p.id::text, p.created_at, p.original_ts, p.content FROM posts p \
             WHERE p.user_id = $1 AND p.visibility = 'public' \
               AND (p.ap_object_id IS NULL OR NOT EXISTS ( \
                    SELECT 1 FROM ap_activities a \
                     WHERE a.object_id = p.ap_object_id AND a.direction = 'out')) \
               {keyset} ORDER BY p.created_at {order}, ('post:' || p.id::text) {order} LIMIT {limit}",
            keyset = if before.is_some() {
                format!("AND (p.created_at, 'post:' || p.id::text) {cmp} ($2, $3)")
            } else {
                String::new()
            },
        );
        let rows: Vec<(String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, Value)> =
            match &before {
                Some((ts, id)) => sqlx::query_as(&sql)
                    .bind(user_id)
                    .bind(ts)
                    .bind(id)
                    .fetch_all(&state.db)
                    .await
                    .map_err(|e| AppError::Internal(format!("db: {e}")))?,
                None => sqlx::query_as(&sql)
                    .bind(user_id)
                    .fetch_all(&state.db)
                    .await
                    .map_err(|e| AppError::Internal(format!("db: {e}")))?,
            };
        for (id, ts, published, content) in rows {
            let rkey = id.strip_prefix("post:").unwrap_or(&id);
            let text = post_text(&content);
            let payload = synth_create_note(actor_id, "posts", rkey, text, published);
            entries.push(Entry { ts, id, payload });
        }
    }

    // 3. appview posts by the account's own AT identities, not yet bridged.
    {
        let sql = format!(
            "SELECT ap.uri, ap.created_at, ap.text FROM appview_posts ap \
             WHERE ap.author_did = ANY($1) \
               AND NOT EXISTS (SELECT 1 FROM ap_activities a WHERE a.local_uri = ap.uri) \
               {keyset} ORDER BY ap.created_at {order}, ap.uri {order} LIMIT {limit}",
            keyset = if before.is_some() {
                format!("AND (ap.created_at, ap.uri) {cmp} ($2, $3)")
            } else {
                String::new()
            },
        );
        let rows: Vec<(String, chrono::DateTime<chrono::Utc>, String)> = match &before {
            Some((ts, id)) => sqlx::query_as(&sql)
                .bind(&dids)
                .bind(ts)
                .bind(id)
                .fetch_all(&state.db)
                .await
                .map_err(|e| AppError::Internal(format!("db: {e}")))?,
            None => sqlx::query_as(&sql)
                .bind(&dids)
                .fetch_all(&state.db)
                .await
                .map_err(|e| AppError::Internal(format!("db: {e}")))?,
        };
        for (uri, ts, text) in rows {
            let rkey = uri.rsplit('/').next().unwrap_or("");
            let payload = synth_create_note(actor_id, "app.bsky.feed.post", rkey, &text, ts);
            entries.push(Entry { ts, id: uri, payload });
        }
    }

    // Merge the per-source top-(K+1)s: the union's true top-K is contained
    // in them, so truncating after one sort serves exactly the stream's next
    // K items in keyset order.
    if ascending {
        entries.sort_by(|a, b| a.ts.cmp(&b.ts).then_with(|| a.id.cmp(&b.id)));
    } else {
        entries.sort_by(|a, b| b.ts.cmp(&a.ts).then_with(|| b.id.cmp(&a.id)));
    }
    let more = entries.len() as i64 > PAGE_SIZE;
    entries.truncate(PAGE_SIZE as usize);
    Ok((entries, more))
}

/// The account's own AT DIDs (linked Bluesky account + minted PDS identity) —
/// the only appview authors whose posts this actor may claim in its outbox.
async fn own_at_dids(state: &AppState, user_id: uuid::Uuid) -> Result<Vec<String>, AppError> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT external_id FROM linked_accounts WHERE user_id = $1 AND provider = 'bluesky' \
         UNION SELECT did FROM pds_accounts WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(format!("db: {e}")))?;
    Ok(rows.into_iter().map(|(d,)| d).collect())
}

/// Synthesize the `Create(Note)` the AP bridge would have produced for a post
/// we can represent. Id conventions match bridge.rs (`write_activity_id` /
/// `object_id`), so if the post is bridged later the real row supersedes this
/// one via the `local_uri`/`object_id` dedupe instead of duplicating it.
fn synth_create_note(
    actor_id: &str,
    collection: &str,
    rkey: &str,
    text: &str,
    published: chrono::DateTime<chrono::Utc>,
) -> Value {
    let base = actor_id.trim_end_matches('/').to_string();
    let obj_id = format!("{base}/objects/{collection}/{rkey}");
    let create_id = format!("{base}/activities/{collection}/{rkey}");
    let published = published.to_rfc3339();
    json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": create_id,
        "type": "Create",
        "actor": actor_id,
        "object": {
            "id": obj_id,
            "type": "Note",
            "attributedTo": actor_id,
            "content": text,
            "published": published,
            "to": [AS_PUBLIC],
            "cc": [format!("{base}/followers")],
        },
        "published": published,
        "to": [AS_PUBLIC],
        "cc": [format!("{base}/followers")],
    })
}

/// Text of a local `posts.content` JSON value (bare string or one of the
/// `{text|content|body}` object shapes used by the importers/composer).
fn post_text(content: &Value) -> &str {
    content
        .as_str()
        .or_else(|| content.get("text").and_then(Value::as_str))
        .or_else(|| content.get("content").and_then(Value::as_str))
        .or_else(|| content.get("body").and_then(Value::as_str))
        .unwrap_or("")
}

fn encode_cursor(ts: chrono::DateTime<chrono::Utc>, id: &str) -> String {
    use base64::Engine;
    let raw = format!("{}|{id}", ts.to_rfc3339());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw)
}

fn decode_cursor(token: &str) -> Result<(chrono::DateTime<chrono::Utc>, String), AppError> {
    use base64::Engine;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| AppError::BadRequest("bad outbox cursor".into()))?;
    let raw = String::from_utf8(raw).map_err(|_| AppError::BadRequest("bad outbox cursor".into()))?;
    let (ts, id) = raw
        .split_once('|')
        .ok_or_else(|| AppError::BadRequest("bad outbox cursor".into()))?;
    let ts = chrono::DateTime::parse_from_rfc3339(ts)
        .map_err(|_| AppError::BadRequest("bad outbox cursor".into()))?
        .with_timezone(&chrono::Utc);
    if id.is_empty() {
        return Err(AppError::BadRequest("bad outbox cursor".into()));
    }
    Ok((ts, id.to_string()))
}

pub(crate) fn jsonld(doc: Value) -> Response {
    (
        [(header::CONTENT_TYPE, "application/activity+json")],
        Json(doc),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ap::store::insert_activity;
    use crate::ap::testutil::*;
    use tower::ServiceExt;

    async fn get_outbox(app: &axum::Router, uri: &str, host: &str) -> (u16, Value) {
        let req = axum::http::Request::builder()
            .method("GET")
            .uri(uri)
            .header("host", host)
            .body(axum::body::Body::empty())
            .expect("outbox request");
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        let status = resp.status().as_u16();
        let bytes = axum::body::to_bytes(resp.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap_or_default();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    /// Insert one public outbound activity with an explicit timestamp.
    async fn seed_activity(
        pool: &sqlx::PgPool,
        actor_id: &str,
        id: &str,
        minutes_ago: i64,
    ) {
        let mut tx = pool.begin().await.expect("txn");
        let payload = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": id,
            "type": "Create",
            "actor": actor_id,
            "to": ["https://www.w3.org/ns/activitystreams#Public"],
            "object": {"id": format!("{id}/object"), "type": "Note"},
        });
        insert_activity(&mut tx, id, actor_id, "Create", Some(&format!("{id}/object")), None, "out", &payload)
            .await
            .expect("insert");
        sqlx::query("UPDATE ap_activities SET created_at = now() - ($2 || ' minutes')::interval WHERE id = $1")
            .bind(id)
            .bind(minutes_ago.to_string())
            .execute(&mut *tx)
            .await
            .expect("backdate");
        tx.commit().await.expect("commit");
    }

    #[tokio::test]
    async fn outbox_unknown_actor_is_404() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = ap_router(state);

        let (status, _) = get_outbox(
            &app,
            &format!("/@nosuchuser_{}/outbox", unique()),
            &domain,
        )
        .await;
        assert_eq!(status, 404, "unknown actor outbox is 404");
    }

    #[tokio::test]
    async fn outbox_pages_are_bounded_stable_and_duplicate_free() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = ap_router(state);

        let target = make_local_actor(&pool, &domain).await;

        // 45 public outbound activities, staggered: id i is i minutes old, so
        // newest first = seeder-01 … seeder-45.
        let mut expected: Vec<String> = Vec::new();
        for i in 1..=45i64 {
            let id = format!("{}/activities/seeder-{:02}", target.actor_id, i);
            seed_activity(&pool, &target.actor_id, &id, i).await;
            expected.push(id);
        }

        // Non-public and undone rows must not surface.
        let mut tx = pool.begin().await.expect("txn");
        let accept_payload = json!({
            "id": format!("{}/activities/accept-x", target.actor_id),
            "type": "Accept",
            "actor": target.actor_id,
            "to": ["https://remote.test/users/x"],
        });
        insert_activity(
            &mut tx,
            &format!("{}/activities/accept-x", target.actor_id),
            &target.actor_id,
            "Accept",
            None,
            None,
            "out",
            &accept_payload,
        )
        .await
        .expect("insert accept");
        let undone_payload = json!({
            "id": format!("{}/activities/undone-x", target.actor_id),
            "type": "Create",
            "actor": target.actor_id,
            "to": ["https://www.w3.org/ns/activitystreams#Public"],
        });
        insert_activity(
            &mut tx,
            &format!("{}/activities/undone-x", target.actor_id),
            &target.actor_id,
            "Create",
            None,
            None,
            "out",
            &undone_payload,
        )
        .await
        .expect("insert undone");
        sqlx::query("UPDATE ap_activities SET undone_at = now() WHERE id = $1")
            .bind(format!("{}/activities/undone-x", target.actor_id))
            .execute(&mut *tx)
            .await
            .expect("mark undone");
        tx.commit().await.expect("commit");

        // Collection document.
        let (status, collection) = get_outbox(
            &app,
            &format!("/@{}/outbox", target.username),
            &domain,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(collection["type"], json!("OrderedCollection"));
        assert_eq!(collection["totalItems"], json!(45), "public, live, outbound only");
        assert!(collection["first"]["id"]
            .as_str()
            .unwrap()
            .ends_with("?page=first"));
        assert!(collection["last"]["id"].as_str().unwrap().ends_with("?page=last"));

        // Page chain: 20 + 20 + 5, newest first, no duplicate ids.
        let mut seen: Vec<String> = Vec::new();
        let mut uri = format!("/@{}/outbox?page=first", target.username);
        let mut nexts = 0;
        loop {
            let (status, page) = get_outbox(&app, &uri, &domain).await;
            assert_eq!(status, 200, "page status");
            assert_eq!(page["type"], json!("OrderedCollectionPage"));
            assert_eq!(page["partOf"], json!(format!("{}/outbox", target.actor_id)));
            let items = page["orderedItems"].as_array().expect("orderedItems");
            assert!(items.len() <= 20, "bounded page");
            for it in items {
                seen.push(it["id"].as_str().expect("activity id").to_string());
            }
            match page["next"].as_str() {
                Some(next) => {
                    nexts += 1;
                    let token = next.rsplit("page=").next().expect("cursor");
                    uri = format!("/@{}/outbox?page={token}", target.username);
                }
                None => break,
            }
        }
        assert_eq!(nexts, 2, "three bounded pages total");
        assert_eq!(seen, expected, "newest first, stable order, no gaps");
        let unique: std::collections::HashSet<&String> = seen.iter().collect();
        assert_eq!(unique.len(), 45, "no duplicate ids across pages");

        // The last page carries the oldest items, presented newest-first.
        let (status, last) = get_outbox(
            &app,
            &format!("/@{}/outbox?page=last", target.username),
            &domain,
        )
        .await;
        assert_eq!(status, 200);
        let items = last["orderedItems"].as_array().expect("orderedItems");
        assert_eq!(items.len(), 20, "last page is one bounded page");
        assert_eq!(
            items[0]["id"].as_str().unwrap(),
            expected[25],
            "last page starts at the 26th-newest"
        );

        // Cleanup.
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id = $1")
            .bind(&target.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(target.user.id)
            .execute(&pool)
            .await;
    }

    /// Owner QA 2026-11-17: totalItems must equal exactly the posts the pages
    /// serve. Sources: bridged Create/Announce rows + public local posts + the
    /// account's own-identity appview posts (synthesized as Create(Note)).
    /// Likes/Undos and foreign-authored appview posts are not this actor's
    /// posts — neither counted nor served.
    #[tokio::test]
    async fn outbox_count_matches_served_posts_and_hides_non_posts() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = ap_router(state);

        let target = make_local_actor(&pool, &domain).await;
        // The account's own AT identity (its minted PDS DID).
        let own_did = format!("did:plc:outbox{}", unique());
        sqlx::query(
            "INSERT INTO pds_accounts (did, handle, signing_key_private, signing_key_public, status, user_id) \
             VALUES ($1, 'outboxtester', 'x', 'x', 'active', $2)",
        )
        .bind(&own_did)
        .bind(target.user.id)
        .execute(&pool)
        .await
        .expect("mint at identity");

        // 2 bridged posts (public Create activities) + 1 public Like —
        // delivery bookkeeping, must NOT surface as a post.
        let bridge_a = format!("{}/activities/bridge-a-{}", target.actor_id, unique());
        let bridge_b = format!("{}/activities/bridge-b-{}", target.actor_id, unique());
        seed_activity(&pool, &target.actor_id, &bridge_a, 40).await;
        seed_activity(&pool, &target.actor_id, &bridge_b, 35).await;
        let like_id = format!("{}/activities/like-x-{}", target.actor_id, unique());
        let mut tx = pool.begin().await.expect("txn");
        let like = json!({
            "id": like_id,
            "type": "Like",
            "actor": target.actor_id,
            "to": ["https://www.w3.org/ns/activitystreams#Public"],
        });
        insert_activity(&mut tx, &like_id, &target.actor_id, "Like", None, None, "out", &like)
            .await
            .expect("insert like");
        tx.commit().await.expect("commit");

        // 2 local posts (one private — excluded from the public stream).
        for (text, visibility) in [("local post one", "public"), ("local post secret", "private")] {
            sqlx::query(
                "INSERT INTO posts (id, user_id, source, content, content_type, visibility, original_ts, original_platform) \
                 VALUES (gen_random_uuid(), $1, 'local', $2::jsonb, 'text/plain', $3, now() - interval '30 minutes', 'bytescape')",
            )
            .bind(target.user.id)
            .bind(json!({ "text": text }).to_string())
            .bind(visibility)
            .execute(&pool)
            .await
            .expect("insert local post");
        }

        // 2 appview posts by the account's own DID, 1 by a foreign actor.
        let mut own_uris: Vec<String> = Vec::new();
        let foreign_uri = format!("at://did:plc:foreign{}/app.bsky.feed.post/other", unique());
        for (uri, did, text) in [
            (format!("at://{}/app.bsky.feed.post/own{}", own_did, unique()), own_did.clone(), "appview own one"),
            (format!("at://{}/app.bsky.feed.post/own{}", own_did, unique()), own_did.clone(), "appview own two"),
            (foreign_uri.clone(), format!("did:plc:foreign{}", unique()), "appview foreign post"),
        ] {
            sqlx::query(
                "INSERT INTO appview_posts (uri, cid, author_did, author_handle, text, created_at) \
                 VALUES ($1, 'testcid', $2, 'tester.test', $3, now() - interval '25 minutes')",
            )
            .bind(&uri)
            .bind(&did)
            .bind(text)
            .execute(&pool)
            .await
            .expect("insert appview post");
            if did == own_did {
                own_uris.push(uri);
            }
        }

        // Collection: totalItems is the exact served size (2 bridged + 1
        // public local + 2 own-identity appview = 5).
        let (status, collection) =
            get_outbox(&app, &format!("/@{}/outbox", target.username), &domain).await;
        assert_eq!(status, 200);
        assert_eq!(
            collection["totalItems"],
            json!(5),
            "2 bridged + 1 public local + 2 own-identity appview"
        );

        // Walk every page: served == counted, unique, posts only.
        let mut seen: Vec<Value> = Vec::new();
        let mut uri = format!("/@{}/outbox?page=first", target.username);
        loop {
            let (status, page) = get_outbox(&app, &uri, &domain).await;
            assert_eq!(status, 200, "page status");
            for it in page["orderedItems"].as_array().expect("orderedItems") {
                assert_eq!(it["type"], json!("Create"), "post stream serves posts");
                seen.push(it.clone());
            }
            match page["next"].as_str() {
                Some(next) => {
                    let token = next.rsplit("page=").next().expect("cursor");
                    uri = format!("/@{}/outbox?page={token}", target.username);
                }
                None => break,
            }
        }
        let ids: Vec<&str> = seen.iter().map(|it| it["id"].as_str().unwrap()).collect();
        assert_eq!(ids.len(), 5, "count equals the number of posts served");
        let unique_ids: std::collections::HashSet<&&str> = ids.iter().collect();
        assert_eq!(unique_ids.len(), 5, "no duplicates across pages");
        assert!(!ids.contains(&like_id.as_str()), "Likes are not posts");
        assert!(ids.iter().all(|id| !id.contains("other")), "foreign-authored posts are not ours to serve");
        assert!(ids.contains(&bridge_a.as_str()) && ids.contains(&bridge_b.as_str()), "bridged posts are served as-is");

        // The synthesized Note carries the post text under the bridge's id
        // conventions.
        let local = seen
            .iter()
            .find(|it| it["id"].as_str().unwrap().contains("/activities/posts/"))
            .expect("synthesized local post");
        assert_eq!(local["object"]["type"], json!("Note"));
        assert_eq!(local["object"]["content"], json!("local post one"));
        assert_eq!(local["object"]["attributedTo"], Value::String(target.actor_id.clone()));
        let appview = seen
            .iter()
            .find(|it| ids.contains(&it["id"].as_str().unwrap()) && it["id"].as_str().unwrap().contains("app.bsky.feed.post"))
            .expect("synthesized appview post");
        assert_eq!(appview["object"]["type"], json!("Note"));

        // Cleanup.
        let _ = sqlx::query("DELETE FROM appview_posts WHERE uri = ANY($1)")
            .bind(&own_uris)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM appview_posts WHERE uri = $1")
            .bind(&foreign_uri)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM posts WHERE user_id = $1")
            .bind(target.user.id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM pds_accounts WHERE user_id = $1")
            .bind(target.user.id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id = $1")
            .bind(&target.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(target.user.id)
            .execute(&pool)
            .await;
    }

    /// Owner QA follow-up: the alias outbox forms (`/users/{u}/outbox`,
    /// `/actors/{u}/outbox`) serve JSON identical to the canonical one —
    /// cached alias actor ids derive collection URLs exactly as they derive
    /// inbox URLs (same defect class as the alias inboxes, which used to 404).
    #[tokio::test]
    async fn alias_outbox_forms_match_canonical() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = ap_router(state);

        let target = make_local_actor(&pool, &domain).await;
        // Seeded stream: one bridged post + one local post = 2 items.
        let bridge_id = format!("{}/activities/alias-a-{}", target.actor_id, unique());
        seed_activity(&pool, &target.actor_id, &bridge_id, 30).await;
        sqlx::query(
            "INSERT INTO posts (id, user_id, source, content, content_type, visibility, original_ts, original_platform) \
             VALUES (gen_random_uuid(), $1, 'local', '{\"text\":\"alias outbox post\"}'::jsonb, 'text/plain', 'public', now() - interval '20 minutes', 'bytescape')",
        )
        .bind(target.user.id)
        .execute(&pool)
        .await
        .expect("insert local post");

        // Collection doc and page output identical across all three forms.
        for page in ["", "?page=first"] {
            let (s0, canonical) =
                get_outbox(&app, &format!("/@{}/outbox{page}", target.username), &domain).await;
            let (s1, users) =
                get_outbox(&app, &format!("/users/{}/outbox{page}", target.username), &domain).await;
            let (s2, actors) =
                get_outbox(&app, &format!("/actors/{}/outbox{page}", target.username), &domain).await;
            assert_eq!((s0, s1, s2), (200, 200, 200), "all three forms serve '{page}'");
            assert_eq!(users, canonical, "/users/ form identical for '{page}'");
            assert_eq!(actors, canonical, "/actors/ form identical for '{page}'");
        }

        // And the shared stream is the seeded one: bridged + synthesized.
        let (_, collection) =
            get_outbox(&app, &format!("/@{}/outbox", target.username), &domain).await;
        assert_eq!(collection["totalItems"], json!(2), "bridged + synthesized local post");
        let (_, first) =
            get_outbox(&app, &format!("/@{}/outbox?page=first", target.username), &domain).await;
        let items = first["orderedItems"].as_array().expect("orderedItems");
        assert_eq!(items.len(), 2);
        assert!(
            items.iter().any(|it| it["object"]["content"] == json!("alias outbox post")),
            "synthesized local post served"
        );

        // Cleanup.
        let _ = sqlx::query("DELETE FROM posts WHERE user_id = $1")
            .bind(target.user.id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id = $1")
            .bind(&target.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(target.user.id)
            .execute(&pool)
            .await;
    }
}