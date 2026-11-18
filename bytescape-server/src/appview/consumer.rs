//! Relay firehose consumer — subscribes to wss://bsky.network,
//! parses commit events, and indexes app.bsky.feed.post records.

use sqlx::PgPool;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::StreamExt;
use serde_cbor::Value as CborValue;
use tracing::{info, warn, error};

const RELAY_URL: &str = "wss://bsky.network/xrpc/com.atproto.sync.subscribeRepos";

/// Slowest pace for the periodic `frames=` progress line. At firehose frame
/// rates — and especially during post-deploy backlog replay bursts — the
/// every-100-frames line ran at ~80 journald lines/s and ate journald's rate
/// budget, hiding the AP-inbox rejection WARNs exactly when diagnosis
/// mattered (2026-11-18 window). One line a minute keeps the progress
/// signal without starving other loggers.
const PROGRESS_LOG_EVERY: std::time::Duration = std::time::Duration::from_secs(60);

/// Start the relay firehose consumer as a background task.
pub fn start_consumer(pool: PgPool) {
    tokio::spawn(async move {
        let mut backoff = 1u64;
        let mut failures = 0u64;
        loop {
            info!("[AppView] Connecting to relay firehose: {}", RELAY_URL);
            match connect_consumer(&pool).await {
                Ok(()) => {
                    info!("[AppView] Firehose consumer ended normally");
                    backoff = 1;
                    failures = 0;
                }
                Err(e) => {
                    error!("[AppView] Firehose consumer error: {}", e);
                    failures += 1;
                    if failures >= 3 {
                        // The relay refuses multi-day backlogs over
                        // subscribeRepos: a cursor stuck in the past loops
                        // through reset -> replay -> reset forever and never
                        // reaches the present (the "feeds start 3 days ago"
                        // bug). Jump to the live head — retention keeps only
                        // days of history anyway.
                        warn!("[AppView] cursor appears stuck — jumping to live head");
                        let _ = sqlx::query(
                            "UPDATE appview_cursor SET cursor = 0, updated_at = now() WHERE id = 1",
                        )
                        .execute(&pool)
                        .await;
                        failures = 0;
                        backoff = 1;
                    }
                    if backoff < 60 {
                        warn!("[AppView] Reconnecting in {}s", backoff);
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
                    backoff = (backoff * 2).min(60);
                }
            }
        }
    });
}

async fn connect_consumer(pool: &PgPool) -> Result<(), String> {
    let cursor: i64 = sqlx::query_scalar("SELECT cursor FROM appview_cursor WHERE id = 1")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    let url = if cursor > 0 {
        format!("{}?cursor={}", RELAY_URL, cursor)
    } else {
        RELAY_URL.to_string()
    };

    let (ws_stream, resp) = connect_async(&url)
        .await
        .map_err(|e| format!("WebSocket connect failed: {}", e))?;

    info!("[AppView] Connected to relay firehose (cursor={})", cursor);

    let (mut _write, mut read) = ws_stream.split();
    let mut last_cursor = cursor;
    let mut count = 0u64;
    let mut processed = 0u64;
    let mut parse_fails = 0u64;
    let mut last_progress_log = std::time::Instant::now();

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Binary(data)) => {
                count += 1;
                if let Err(e) = process_frame(pool, &data, &mut last_cursor).await {
                    if !e.contains("skip") {
                        parse_fails += 1;
                        if parse_fails <= 5 {
                            warn!("[AppView] frame failed: {}", e);
                        }
                    }
                } else {
                    processed += 1;
                }
                if count % 100 == 0 {
                    // Periodic progress: frames seen vs frames fully processed
                    // tells us where the ingest goes missing. Time-throttled
                    // to PROGRESS_LOG_EVERY — the raw every-100-frames pace
                    // bursts to ~80 journald lines/s during backlog replay.
                    if last_progress_log.elapsed() >= PROGRESS_LOG_EVERY {
                        info!("[AppView] frames={} processed={} parse_fails={}", count, processed, parse_fails);
                        last_progress_log = std::time::Instant::now();
                    }
                    if let Err(e) = sqlx::query("UPDATE appview_cursor SET cursor = $1, updated_at = now() WHERE id = 1")
                        .bind(last_cursor)
                        .execute(pool)
                        .await {
                        warn!("[AppView] Failed to update cursor: {}", e);
                    }
                }
            }
            Ok(Message::Close(_)) => {
                warn!("[AppView] WebSocket closed by relay");
                break;
            }
            Err(e) => {
                // Persist progress before surfacing the error — losing the
                // cursor on every reset made reconnects replay stale frames.
                if let Err(u) = sqlx::query("UPDATE appview_cursor SET cursor = $1, updated_at = now() WHERE id = 1")
                    .bind(last_cursor)
                    .execute(pool)
                    .await {
                    warn!("[AppView] Failed to save cursor on error: {}", u);
                }
                return Err(format!("WebSocket error: {}", e));
            }
            _ => {}
        }
    }

    if let Err(e) = sqlx::query("UPDATE appview_cursor SET cursor = $1, updated_at = now() WHERE id = 1")
        .bind(last_cursor)
        .execute(pool)
        .await {
        warn!("[AppView] Failed to update final cursor: {}", e);
    }

    Ok(())
}

/// Process a single firehose frame (CBOR header + CBOR body).
async fn process_frame(pool: &PgPool, data: &[u8], cursor: &mut i64) -> Result<(), String> {
    // AT Protocol firehose frame: [CBOR header][CBOR body]
    // Header: {"t": "#commit", "op": 1}
    // Body for #commit: {repo, ops, blocks, rev, seq, since, time}

    use serde::de::Deserialize;
    let mut deserializer = serde_cbor::Deserializer::from_slice(data);
    let header_val: CborValue = Deserialize::deserialize(&mut deserializer)
        .map_err(|e| format!("Header CBOR parse: {}", e))?;

    let header_end = deserializer.byte_offset();
    let body = &data[header_end..];
    let frame_type = cbor_text(&header_val, "t").unwrap_or("");
    
    if frame_type == "#handle" {
        // Handle update event — store DID → handle mapping
        let commit: CborValue = serde_cbor::from_slice(body)
            .map_err(|e| format!("Body CBOR parse (handle): {}", e))?;
        let did = cbor_text(&commit, "did").unwrap_or("");
        let handle = cbor_text(&commit, "handle").unwrap_or("");
        if !did.is_empty() && !handle.is_empty() && handle != "handle.invalid" {
            // `handle.invalid` is the relay's unresolved-account sentinel, not
            // a handle (e.g. for did:web users pre-TLS-fix) — never persist it.
            // Store in did_handle mapping table
            if let Err(e) = sqlx::query(
                "INSERT INTO appview_did_handle (did, handle, resolved_at) VALUES ($1, $2, now())
                 ON CONFLICT (did) DO UPDATE SET handle = EXCLUDED.handle, resolved_at = now()"
            )
            .bind(did).bind(handle)
            .execute(pool).await {
                warn!("[AppView] Failed to upsert did_handle mapping: {}", e);
            }
            
            // Also update existing posts with this DID
            if let Err(e) = sqlx::query("UPDATE appview_posts SET author_handle = $1 WHERE author_did = $2")
                .bind(handle).bind(did)
                .execute(pool).await {
                warn!("[AppView] Failed to update author_handle for {}: {}", did, e);
            }
        }
        return Err("skip: handle event processed".to_string());
    }
    
    if frame_type != "#commit" {
        return Err("skip: not a commit".to_string());
    }

    let commit: CborValue = serde_cbor::from_slice(body)
        .map_err(|e| format!("Body CBOR parse: {} (body len={})", e, body.len()))?;

    // Update cursor from seq
    if let Some(seq) = cbor_int(&commit, "seq") {
        *cursor = seq;
    }

    let did = cbor_text(&commit, "repo").unwrap_or("");
    if did.is_empty() {
        return Err("skip: no repo DID".to_string());
    }

    let ops = match &commit {
        CborValue::Map(map) => map.get(&CborValue::Text("ops".to_string())).and_then(|v| if let CborValue::Array(a) = v { Some(a) } else { None }),
        _ => None,
    };

    let blocks_bytes = match &commit {
        CborValue::Map(map) => map.get(&CborValue::Text("blocks".to_string())).and_then(|v| if let CborValue::Bytes(b) = v { Some(b.clone()) } else { None }),
        _ => None,
    };

    if let Some(ops) = ops {
        for op in ops {
            let action = cbor_text(op, "action").unwrap_or("");
            let path = cbor_text(op, "path").unwrap_or("");

            // Handle post indexing (existing logic)
            if path.starts_with("app.bsky.feed.post/") {
                let uri = format!("at://{}/{}", did, path);

                if action == "create" || action == "update" {
                    let cid_bytes = match op {
                        CborValue::Map(map) => map.get(&CborValue::Text("cid".to_string())).and_then(|v| if let CborValue::Bytes(b) = v { Some(b.clone()) } else { None }),
                        _ => None,
                    };
                    // Store the canonical base32 CID — the op carries raw CID
                    // bytes and a hex dump here poisoned every like/repost
                    // ("Invalid CID string").
                    let cid_str = cid_bytes
                        .as_deref()
                        .and_then(crate::oauth::bluesky::cid_from_bytes)
                        .unwrap_or_default();

                    if let Some(ref blocks_data) = blocks_bytes {
                        if let Some(record) = extract_record_from_car(blocks_data, cid_bytes.as_deref()) {
                            // Postgres UTF8 rejects NUL bytes — some network
                            // posts carry them in text.
                            let text = cbor_text(&record, "text").unwrap_or("").replace('\u{0}', "");
                            let created_at = cbor_text(&record, "createdAt");
                            let reply_parent = cbor_text_cborecord(&record, "reply", "parent", "uri");
                            let reply_root = cbor_text_cborecord(&record, "reply", "root", "uri");
                            let langs = match &record {
                                CborValue::Map(map) => map.get(&CborValue::Text("langs".to_string())).and_then(|v| if let CborValue::Array(a) = v { Some(a.iter().filter_map(|v| if let CborValue::Text(s) = v { Some(s.clone()) } else { None }).collect::<Vec<_>>()) } else { None }),
                                _ => None,
                            };

                            // Extract labels (self-labels for NSFW etc.)
                            let labels = extract_labels(&record);

                            // Extract embeds (images, videos, link cards)
                            let (embeds_json, has_media) = extract_embeds(&record, did);

                            let created_ts = created_at.and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map(|dt| dt.with_timezone(&chrono::Utc));

                            if let Err(e) = sqlx::query(
                                r#"INSERT INTO appview_posts (uri, cid, author_did, author_handle, text, reply_parent, reply_root, languages, created_at, embeds, labels, has_media)
                                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                                   ON CONFLICT (uri) DO UPDATE SET
                                     cid = EXCLUDED.cid, text = EXCLUDED.text,
                                     reply_parent = EXCLUDED.reply_parent, reply_root = EXCLUDED.reply_root,
                                     embeds = EXCLUDED.embeds, labels = EXCLUDED.labels, has_media = EXCLUDED.has_media,
                                     indexed_at = now()"#
                            )
                            .bind(&uri).bind(&cid_str).bind(did)
                            .bind::<Option<&str>>(None).bind(text)
                            .bind(reply_parent.as_deref()).bind(reply_root.as_deref())
                            .bind(langs.as_deref()).bind(created_ts)
                            .bind(embeds_json.as_ref().map(|j| j))
                            .bind(labels.as_ref().map(|j| j))
                            .bind(has_media)
                            .execute(pool).await {
                                warn!("[AppView] Failed to upsert post {}: {}", uri, e);
                            }

                            // A quote embed targeting a story record counts as
                            // that story's reshare (story reshares surface as
                            // quote posts so they ride every feed path).
                            if let Some(emb) = &embeds_json {
                                if let Some(arr) = emb.as_array() {
                                    for e in arr {
                                        if e.get("type").and_then(|t| t.as_str()) == Some("quote") {
                                            if let Some(q) = e.get("uri").and_then(|u| u.as_str()) {
                                                if q.contains("/com.bytescape.story/") {
                                                    bump_interaction_count(pool, q, CountKind::Repost).await;
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Track reply engagement: increment reply_count on the
                            // parent record (post, story, or status card).
                            if let Some(parent_uri) = &reply_parent {
                                bump_interaction_count(pool, parent_uri, CountKind::Reply).await;
                                if let Err(e) = crate::ap::notify::notify_at_record_interaction(
                                    pool,
                                    parent_uri,
                                    did,
                                    crate::ap::notify::TYPE_REPLY,
                                    &format!("at-reply:{}", uri),
                                )
                                .await
                                {
                                    warn!("[AppView] reply notification failed for {}: {}", parent_uri, e);
                                }
                            }
                        }
                    }
                } else if action == "delete" {
                    if let Err(e) = sqlx::query("DELETE FROM appview_posts WHERE uri = $1")
                        .bind(&uri).execute(pool).await {
                        warn!("[AppView] Failed to delete post {}: {}", uri, e);
                    }
                }
            }

            // Handle story records (com.bytescape.story): flat stories index.
            if path.starts_with("com.bytescape.story/") {
                let uri = format!("at://{}/{}", did, path);
                if action == "create" || action == "update" {
                    if let Some(blocks_data) = &blocks_bytes {
                        if let Some(record) = extract_record_from_car_by_field(blocks_data, "items") {
                            let cid_str = match op {
                                CborValue::Map(map) => map.get(&CborValue::Text("cid".to_string())).and_then(|v| if let CborValue::Bytes(b) = v { Some(b.as_slice()) } else { None }),
                                _ => None,
                            }.and_then(crate::oauth::bluesky::cid_from_bytes).unwrap_or_default();

                            match parse_story_record(&record) {
                                Some(s) => {
                                    if let Err(e) = upsert_story_row(
                                        pool, &uri, &cid_str, did,
                                        s.created_at, s.active_from, s.expires_at,
                                        s.pinned, s.text, &s.items, s.can_react, s.can_reply,
                                    ).await {
                                        warn!("[AppView] Failed to upsert story {}: {}", uri, e);
                                    }
                                }
                                None => warn!("[AppView] Skipping malformed com.bytescape.story {}", uri),
                            }
                        }
                    }
                } else if action == "delete" {
                    if let Err(e) = sqlx::query("DELETE FROM appview_stories WHERE uri = $1")
                        .bind(&uri).execute(pool).await {
                        warn!("[AppView] Failed to delete story {}: {}", uri, e);
                    }
                }
            }

            // Handle status records (app.bsky.actor.status): text status cards.
            if path.starts_with("app.bsky.actor.status/") {
                if action == "create" || action == "update" {
                    if let Some(blocks_data) = &blocks_bytes {
                        if let Some(record) = extract_record_from_car_by_field(blocks_data, "status") {
                            let cid_str = match op {
                                CborValue::Map(map) => map.get(&CborValue::Text("cid".to_string())).and_then(|v| if let CborValue::Bytes(b) = v { Some(b.as_slice()) } else { None }),
                                _ => None,
                            }.and_then(crate::oauth::bluesky::cid_from_bytes).unwrap_or_default();

                            match parse_status_record(&record) {
                                Some(s) => {
                                    // Self-keyed: a new cid means a new status
                                    // instance — interaction counts reset so
                                    // they never stick to a replacement status.
                                    if let Err(e) = sqlx::query(
                                        r#"INSERT INTO appview_status (author_did, cid, status, music_url, expires_at, is_live)
                                           VALUES ($1, $2, $3, $4, $5, $6)
                                           ON CONFLICT (author_did) DO UPDATE SET
                                             cid = EXCLUDED.cid, status = EXCLUDED.status, music_url = EXCLUDED.music_url,
                                             expires_at = EXCLUDED.expires_at, is_live = EXCLUDED.is_live, updated_at = now(),
                                             created_at = CASE WHEN appview_status.cid = EXCLUDED.cid THEN appview_status.created_at ELSE now() END,
                                             like_count = CASE WHEN appview_status.cid = EXCLUDED.cid THEN appview_status.like_count ELSE 0 END,
                                             repost_count = CASE WHEN appview_status.cid = EXCLUDED.cid THEN appview_status.repost_count ELSE 0 END,
                                             reply_count = CASE WHEN appview_status.cid = EXCLUDED.cid THEN appview_status.reply_count ELSE 0 END"#
                                    )
                                    .bind(did).bind(&cid_str).bind(s.status)
                                    .bind(s.music_url).bind(s.expires_at).bind(s.is_live)
                                    .execute(pool).await {
                                        warn!("[AppView] Failed to upsert status {}: {}", did, e);
                                    }
                                }
                                None => warn!("[AppView] Skipping malformed app.bsky.actor.status for {}", did),
                            }
                        }
                    }
                } else if action == "delete" {
                    if let Err(e) = sqlx::query("DELETE FROM appview_status WHERE author_did = $1")
                        .bind(did).execute(pool).await {
                        warn!("[AppView] Failed to delete status {}: {}", did, e);
                    }
                }
            }

            // Calendar records (community.lexicon.calendar.event/.rsvp + legacy
            // Smoke Signal): the derived events index — spec events/ingestion.
            if crate::calendar::is_event_path(&path) || crate::calendar::is_rsvp_path(&path) {
                let uri = format!("at://{}/{}", did, path);
                let is_rsvp = crate::calendar::is_rsvp_path(&path);
                if action == "create" || action == "update" {
                    if let Some(blocks_data) = &blocks_bytes {
                        let cid_str = match op {
                            CborValue::Map(map) => map.get(&CborValue::Text("cid".to_string())).and_then(|v| if let CborValue::Bytes(b) = v { Some(b.as_slice()) } else { None }),
                            _ => None,
                        }.and_then(crate::oauth::bluesky::cid_from_bytes).unwrap_or_default();

                        let wanted: &[&str] = if is_rsvp {
                            &[crate::calendar::RSVP_NSID]
                        } else {
                            crate::calendar::RECORD_NSIDS
                        };
                        match extract_record_from_car_by_type(blocks_data, wanted) {
                            Some(record) => {
                                let value = crate::calendar::cbor_to_json(&record);
                                if is_rsvp {
                                    match crate::calendar::parse_rsvp(&value) {
                                        Some(r) => {
                                            if let Err(e) = crate::calendar::upsert_rsvp(pool, &uri, did, r).await {
                                                warn!("[AppView] Failed to upsert rsvp {}: {}", uri, e);
                                            }
                                        }
                                        None => warn!("[AppView] Skipping malformed calendar rsvp {}", uri),
                                    }
                                } else {
                                    let nsid = path.split('/').next().unwrap_or("");
                                    match crate::calendar::parse_event(&value, nsid) {
                                        Some(ev) => {
                                            if let Err(e) = crate::calendar::upsert_event(pool, &uri, &cid_str, did, ev, "atproto").await {
                                                warn!("[AppView] Failed to upsert event {}: {}", uri, e);
                                            }
                                        }
                                        None => warn!("[AppView] Skipping malformed calendar event {}", uri),
                                    }
                                }
                            }
                            None => warn!("[AppView] Calendar record not found in CAR for {}", uri),
                        }
                    }
                } else if action == "delete" {
                    let result = if is_rsvp {
                        crate::calendar::delete_rsvp(pool, &uri).await
                    } else {
                        crate::calendar::delete_event(pool, &uri).await
                    };
                    if let Err(e) = result {
                        warn!("[AppView] Failed to delete calendar row {}: {}", uri, e);
                    }
                }
            }

            // Handle like events: increment like_count on the referenced record
            if path.starts_with("app.bsky.feed.like/") && (action == "create" || action == "update") {
                if let Some(blocks_data) = &blocks_bytes {
                    if let Some(record) = extract_record_from_car_by_field(blocks_data, "subject") {
                        if let Some(subject_uri) = cbor_text_nested(&record, "subject", "uri") {
                            bump_interaction_count(pool, &subject_uri, CountKind::Like).await;
                            if let Err(e) = crate::ap::notify::notify_at_record_interaction(
                                pool,
                                &subject_uri,
                                did,
                                crate::ap::notify::TYPE_LIKE,
                                &format!("at-like:{}", path),
                            )
                            .await
                            {
                                warn!("[AppView] like notification failed for {}: {}", subject_uri, e);
                            }
                        }
                    }
                }
            }

            // Handle like deletion: decrement like_count on the referenced post
            if path.starts_with("app.bsky.feed.like/") && action == "delete" {
                // For deletes, the subject URI isn't in the record anymore.
                // We can't easily resolve which post was liked, so we skip.
                // The like_count will be corrected by periodic re-indexing.
            }

            // Handle repost events: increment repost_count on the referenced record
            if path.starts_with("app.bsky.feed.repost/") && (action == "create" || action == "update") {
                if let Some(blocks_data) = &blocks_bytes {
                    if let Some(record) = extract_record_from_car_by_field(blocks_data, "subject") {
                        if let Some(subject_uri) = cbor_text_nested(&record, "subject", "uri") {
                            bump_interaction_count(pool, &subject_uri, CountKind::Repost).await;
                            if let Err(e) = crate::ap::notify::notify_at_record_interaction(
                                pool,
                                &subject_uri,
                                did,
                                crate::ap::notify::TYPE_REPOST,
                                &format!("at-repost:{}", path),
                            )
                            .await
                            {
                                warn!("[AppView] repost notification failed for {}: {}", subject_uri, e);
                            }
                        }
                    }
                }
            }

            // Handle inbound Bluesky follows: notify the target (or queue a
            // pending request when the target account is locked — AT has no
            // locked accounts; this is UI-only gating on our ingest).
            if path.starts_with("app.bsky.graph.follow/") && (action == "create" || action == "update") {
                if let Some(blocks_data) = &blocks_bytes {
                    if let Some(record) = extract_record_from_car_by_field(blocks_data, "subject") {
                        if let Some(subject_raw) = cbor_text_nested(&record, "subject", "uri") {
                            let subject_did = subject_raw
                                .strip_prefix("at://")
                                .and_then(|d| d.split('/').next())
                                .map(str::to_string)
                                .unwrap_or(subject_raw);
                            let follow_uri = format!("at://{}/{}", did, path);
                            if let Err(e) = crate::ap::notify::notify_at_follow(
                                pool,
                                &subject_did,
                                did,
                                &follow_uri,
                            )
                            .await
                            {
                                warn!("[AppView] follow notification failed for {}: {}", subject_did, e);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Extract a post record from CAR-encoded blocks.
/// CAR format: [header section][CID+block pairs...]
///
/// `expected_cid` is the op's cid. A commit CAR can carry more than the
/// created record — a reply drags its parent/root in — so "the first block
/// that has a text field" can return a *different* post's body: the row then
/// loses the real record's reply/embed fields (2026-11-15, post
/// 3n2e7dq7bac2d: reply_parent NULL and its external embed dropped). Match
/// the op's cid when we have it; keep the old heuristic only as a fallback.
fn extract_record_from_car(data: &[u8], expected_cid: Option<&[u8]>) -> Option<CborValue> {
    // subscribeRepos cid-links carry a 0x00 multibase prefix on some relays.
    let expected = expected_cid
        .map(|c| c.strip_prefix(&[0u8]).unwrap_or(c))
        .filter(|c| !c.is_empty());

    let mut pos = 0;
    let (header_len, n) = read_uvarint(&data[pos..])?;
    pos += n + header_len as usize;
    if pos >= data.len() { return None; }

    let mut fallback: Option<CborValue> = None;

    while pos < data.len() {
        let (entry_len, n) = match read_uvarint(&data[pos..]) {
            Some(v) => v, None => break,
        };
        pos += n;
        let entry_end = pos + entry_len as usize;
        if entry_end > data.len() { break; }
        let entry = &data[pos..entry_end];
        pos = entry_end;

        // Entry: [CID bytes][DAG-CBOR block data]
        if let Some(expected) = expected {
            if entry.len() > expected.len() && entry.starts_with(expected) {
                if let Ok(val) = serde_cbor::from_slice::<CborValue>(&entry[expected.len()..]) {
                    return Some(val);
                }
            }
        }

        // CID v1 is typically 36 bytes. Try common CID lengths.
        for cid_len in [36, 38, 40, 32, 34, 42, 44, 48, 52].iter() {
            if *cid_len >= entry.len() { continue; }
            if let Ok(val) = serde_cbor::from_slice::<CborValue>(&entry[*cid_len..]) {
                // Check if this looks like a post record (has "text" field)
                if cbor_text(&val, "text").is_some() {
                    if fallback.is_none() {
                        fallback = Some(val);
                    }
                    break;
                }
            }
        }
    }
    fallback
}

/// Extract a record from CAR-encoded blocks by looking for a specific field name.
/// Used for like/repost records ("subject") and story/status records
/// ("items"/"status") where the field is not necessarily text. Matches on key
/// presence, any value type.
fn extract_record_from_car_by_field(data: &[u8], field_name: &str) -> Option<CborValue> {
    let mut pos = 0;
    let (header_len, n) = read_uvarint(&data[pos..])?;
    pos += n + header_len as usize;
    if pos >= data.len() { return None; }

    let field_key = CborValue::Text(field_name.to_string());
    while pos < data.len() {
        let (entry_len, n) = match read_uvarint(&data[pos..]) {
            Some(v) => v, None => break,
        };
        pos += n;
        let entry_end = pos + entry_len as usize;
        if entry_end > data.len() { break; }
        let entry = &data[pos..entry_end];
        pos = entry_end;

        for cid_len in [36, 38, 40, 32, 34, 42, 44, 48, 52].iter() {
            if *cid_len >= entry.len() { continue; }
            if let Ok(val) = serde_cbor::from_slice::<CborValue>(&entry[*cid_len..]) {
                if matches!(&val, CborValue::Map(m) if m.contains_key(&field_key)) {
                    return Some(val);
                }
            }
        }
    }
    None
}

/// Extract a record from CAR-encoded blocks by its `$type` value — used for
/// calendar-family records where several NSIDs share one shape and field
/// presence alone is ambiguous.
fn extract_record_from_car_by_type(data: &[u8], nsids: &[&str]) -> Option<CborValue> {
    let mut pos = 0;
    let (header_len, n) = read_uvarint(&data[pos..])?;
    pos += n + header_len as usize;
    if pos >= data.len() { return None; }

    while pos < data.len() {
        let (entry_len, n) = match read_uvarint(&data[pos..]) {
            Some(v) => v, None => break,
        };
        pos += n;
        let entry_end = pos + entry_len as usize;
        if entry_end > data.len() { break; }
        let entry = &data[pos..entry_end];
        pos = entry_end;

        for cid_len in [36, 38, 40, 32, 34, 42, 44, 48, 52].iter() {
            if *cid_len >= entry.len() { continue; }
            if let Ok(val) = serde_cbor::from_slice::<CborValue>(&entry[*cid_len..]) {
                let matches = cbor_text(&val, "$type")
                    .is_some_and(|t| nsids.contains(&t));
                if matches {
                    return Some(val);
                }
            }
        }
    }
    None
}

fn read_uvarint(data: &[u8]) -> Option<(u64, usize)> {
    let mut result = 0u64;
    let mut shift = 0u32;
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        i += 1;
        result |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 { return Some((result, i)); }
        shift += 7;
        if shift >= 64 { return None; }
    }
    None
}

// === Stories support (com.bytescape.story + app.bsky.actor.status) ===

struct StoryRecord {
    created_at: chrono::DateTime<chrono::Utc>,
    active_from: Option<chrono::DateTime<chrono::Utc>>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pinned: bool,
    text: Option<String>,
    items: serde_json::Value,
    can_react: bool,
    can_reply: bool,
}

struct StatusRecord {
    status: String,
    expires_at: chrono::DateTime<chrono::Utc>,
    music_url: Option<String>,
    is_live: bool,
}

fn parse_rfc3339(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|dt| dt.with_timezone(&chrono::Utc))
}

fn cbor_map_get<'a>(val: &'a CborValue, key: &str) -> Option<&'a CborValue> {
    match val {
        CborValue::Map(map) => map.get(&CborValue::Text(key.to_string())),
        _ => None,
    }
}

fn cbor_bool(val: &CborValue, key: &str) -> Option<bool> {
    cbor_map_get(val, key).and_then(|v| if let CborValue::Bool(b) = v { Some(*b) } else { None })
}

/// Extract (cid, mime) from an AT Protocol blob value: either
/// {"ref": {"$link": "bafy…"}, "mimeType": …} or {"ref": <CID bytes>}.
fn blob_ref(blob: &CborValue) -> Option<(String, String)> {
    let map = match blob {
        CborValue::Map(m) => m,
        _ => return None,
    };
    let cid = map.get(&CborValue::Text("ref".to_string())).and_then(|r| match r {
        CborValue::Bytes(b) => Some(cid_bytes_to_string(b)),
        CborValue::Map(ref_map) => ref_map
            .get(&CborValue::Text("$link".to_string()))
            .and_then(|l| if let CborValue::Text(s) = l { Some(s.clone()) } else { None }),
        _ => None,
    })?;
    let mime = map
        .get(&CborValue::Text("mimeType".to_string()))
        .and_then(|v| if let CborValue::Text(s) = v { Some(s.clone()) } else { None })
        .unwrap_or_default();
    Some((cid, mime))
}

/// Parse a com.bytescape.story record. Malformed records (missing/invalid
/// createdAt, invalid scheduling timestamps, zero usable media items) are
/// rejected so they never reach the index.
fn parse_story_record(record: &CborValue) -> Option<StoryRecord> {
    let created_at = parse_rfc3339(cbor_text(record, "createdAt")?)?;
    let active_from = match cbor_text(record, "activeFrom") {
        Some(s) => Some(parse_rfc3339(s)?),
        None => None,
    };
    let expires_at = match cbor_text(record, "expiresAt") {
        Some(s) => Some(parse_rfc3339(s)?),
        None => None,
    };
    let text = cbor_text(record, "text").map(|s| s.replace('\u{0}', "")).filter(|s| !s.is_empty());
    let pinned = cbor_bool(record, "pinned").unwrap_or(false);
    let can_react = cbor_bool(record, "canReact").unwrap_or(true);
    let can_reply = cbor_bool(record, "canReply").unwrap_or(true);

    let mut items = Vec::new();
    if let Some(CborValue::Array(arr)) = cbor_map_get(record, "items") {
        for it in arr {
            if items.len() == 6 { break; }
            let blob = ["image", "video", "blob"].iter().find_map(|k| cbor_map_get(it, k));
            if let Some((cid, mime)) = blob.and_then(blob_ref) {
                let kind = if mime.starts_with("video") { "video" } else { "image" };
                let alt = cbor_text(it, "alt").unwrap_or("");
                let duration = cbor_int(it, "duration").map(|d| d.clamp(1, 300));
                items.push(serde_json::json!({
                    "type": kind, "cid": cid, "mime_type": mime, "alt": alt, "duration": duration,
                }));
            }
        }
    }
    if items.is_empty() {
        return None;
    }

    Some(StoryRecord {
        created_at,
        active_from,
        expires_at,
        pinned,
        text,
        items: serde_json::Value::Array(items),
        can_react,
        can_reply,
    })
}

/// Parse an app.bsky.actor.status record. Expiry is computed here:
/// createdAt + durationMinutes, defaulting to 24h when the duration is absent.
fn parse_status_record(record: &CborValue) -> Option<StatusRecord> {
    let created_at = parse_rfc3339(cbor_text(record, "createdAt")?)?;
    let minutes = cbor_int(record, "durationMinutes").unwrap_or(24 * 60).max(1);
    let expires_at = created_at + chrono::Duration::minutes(minutes);
    let music_url = cbor_map_get(record, "embed")
        .and_then(|e| {
            cbor_text(e, "uri").map(|s| s.to_string()).or_else(|| {
                cbor_map_get(e, "external").and_then(|x| cbor_text(x, "uri").map(|s| s.to_string()))
            })
        })
        .filter(|s| !s.is_empty());
    let status = cbor_text(record, "status").unwrap_or("").replace('\u{0}', "");
    // Bluesky's live statuses carry a sentinel ("app.bsky.actor.status#live")
    // as the status text; the human text lives in the embed's title.
    let is_live = status.starts_with("app.bsky.actor.status#");
    let status = if is_live {
        cbor_map_get(record, "embed")
            .and_then(|e| Some(cbor_map_get(e, "external").unwrap_or(e)))
            .and_then(|x| cbor_text(x, "title"))
            .filter(|t| !t.is_empty())
            .map(|t| t.to_string())
            .unwrap_or_else(|| "Live now".to_string())
    } else {
        status
    };
    Some(StatusRecord { status, expires_at, music_url, is_live })
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum CountKind {
    Like,
    Repost,
    Reply,
}

/// Index a post row directly (self-index path for records written through
/// our API — our PDS firehose is not guaranteed to loop back through
/// bsky.network). Same column shape as the firehose arm.
pub(crate) async fn upsert_appview_post(
    pool: &PgPool,
    uri: &str,
    cid: &str,
    author_did: &str,
    text: &str,
    reply_parent: Option<&str>,
    embeds_json: Option<&serde_json::Value>,
    created_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO appview_posts (uri, cid, author_did, author_handle, text, reply_parent, reply_root, created_at, embeds, has_media)
           VALUES ($1, $2, $3, NULL, $4, $5, $5, $6, $7, false)
           ON CONFLICT (uri) DO UPDATE SET
             text = EXCLUDED.text, reply_parent = EXCLUDED.reply_parent,
             embeds = EXCLUDED.embeds, indexed_at = now()"#,
    )
    .bind(uri)
    .bind(cid)
    .bind(author_did)
    .bind(text.replace('\u{0}', ""))
    .bind(reply_parent)
    .bind(created_at)
    .bind(embeds_json)
    .execute(pool)
    .await
    .map(|_| ())
}

/// Upsert a story row (shared by the firehose arm, the create endpoint, and
/// catch-up). Interaction counts are preserved across updates.
pub(crate) async fn upsert_story_row(
    pool: &PgPool,
    uri: &str,
    cid: &str,
    author_did: &str,
    created_at: chrono::DateTime<chrono::Utc>,
    active_from: Option<chrono::DateTime<chrono::Utc>>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pinned: bool,
    text: Option<String>,
    items: &serde_json::Value,
    can_react: bool,
    can_reply: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO appview_stories (uri, cid, author_did, created_at, active_from, expires_at, pinned, text, media, can_react, can_reply)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
           ON CONFLICT (uri) DO UPDATE SET
             cid = EXCLUDED.cid, text = EXCLUDED.text, media = EXCLUDED.media,
             active_from = EXCLUDED.active_from, expires_at = EXCLUDED.expires_at,
             pinned = EXCLUDED.pinned, can_react = EXCLUDED.can_react, can_reply = EXCLUDED.can_reply,
             updated_at = now()"#,
    )
    .bind(uri)
    .bind(cid)
    .bind(author_did)
    .bind(created_at)
    .bind(active_from)
    .bind(expires_at)
    .bind(pinned)
    .bind(text)
    .bind(items)
    .bind(can_react)
    .bind(can_reply)
    .execute(pool)
    .await
    .map(|_| ())
}

/// Route an interaction bump to the right table by subject URI: story and
/// status records count on their own rows, everything else is a post. No-op
/// on empty URIs.
pub(crate) async fn bump_interaction_count(pool: &PgPool, subject_uri: &str, kind: CountKind) {
    if subject_uri.is_empty() {
        return;
    }
    let (sql, key) = if subject_uri.contains("/com.bytescape.story/") {
        let sql = match kind {
            CountKind::Like => "UPDATE appview_stories SET like_count = like_count + 1 WHERE uri = $1",
            CountKind::Repost => "UPDATE appview_stories SET repost_count = repost_count + 1 WHERE uri = $1",
            CountKind::Reply => "UPDATE appview_stories SET reply_count = reply_count + 1 WHERE uri = $1",
        };
        (sql, subject_uri)
    } else if subject_uri.contains("/app.bsky.actor.status/") {
        // at://<did>/app.bsky.actor.status/self — rows are keyed by author DID.
        let author_did = subject_uri
            .strip_prefix("at://")
            .and_then(|s| s.split('/').next())
            .unwrap_or("");
        let sql = match kind {
            CountKind::Like => "UPDATE appview_status SET like_count = like_count + 1 WHERE author_did = $1",
            CountKind::Repost => "UPDATE appview_status SET repost_count = repost_count + 1 WHERE author_did = $1",
            CountKind::Reply => "UPDATE appview_status SET reply_count = reply_count + 1 WHERE author_did = $1",
        };
        (sql, author_did)
    } else {
        let sql = match kind {
            CountKind::Like => "UPDATE appview_posts SET like_count = like_count + 1 WHERE uri = $1",
            CountKind::Repost => "UPDATE appview_posts SET repost_count = repost_count + 1 WHERE uri = $1",
            CountKind::Reply => "UPDATE appview_posts SET reply_count = reply_count + 1 WHERE uri = $1",
        };
        (sql, subject_uri)
    };
    if let Err(e) = sqlx::query(sql).bind(key).execute(pool).await {
        warn!("[AppView] Failed to increment {:?} for {}: {}", kind, subject_uri, e);
    }
}

// === CBOR helper functions ===

fn cbor_text<'a>(val: &'a CborValue, key: &str) -> Option<&'a str> {
    match val {
        CborValue::Map(map) => map.get(&CborValue::Text(key.to_string())).and_then(|v| if let CborValue::Text(s) = v { Some(s.as_str()) } else { None }),
        _ => None,
    }
}

fn cbor_int(val: &CborValue, key: &str) -> Option<i64> {
    match val {
        CborValue::Map(map) => map.get(&CborValue::Text(key.to_string())).and_then(|v| if let CborValue::Integer(i) = v { Some(i128_to_i64(*i)) } else { None }),
        _ => None,
    }
}

fn cbor_text_cborecord(val: &CborValue, key1: &str, key2: &str, key3: &str) -> Option<String> {
    // Navigate: val[key1][key2][key3] where key1 and key2 are maps, key3 is text
    match val {
        CborValue::Map(map) => {
            map.get(&CborValue::Text(key1.to_string())).and_then(|v1| {
                match v1 {
                    CborValue::Map(m2) => m2.get(&CborValue::Text(key2.to_string())).and_then(|v2| {
                        match v2 {
                            CborValue::Map(m3) => m3.get(&CborValue::Text(key3.to_string())).and_then(|v3| {
                                if let CborValue::Text(s) = v3 { Some(s.clone()) } else { None }
                            }),
                            _ => None,
                        }
                    }),
                    _ => None,
                }
            })
        }
        _ => None,
    }
}

/// Navigate val[key1][key2] and return the text value.
/// Used for extracting subject.uri from like/repost records.
fn cbor_text_nested(val: &CborValue, key1: &str, key2: &str) -> Option<String> {
    match val {
        CborValue::Map(map) => {
            map.get(&CborValue::Text(key1.to_string())).and_then(|v1| {
                match v1 {
                    CborValue::Map(m2) => m2.get(&CborValue::Text(key2.to_string())).and_then(|v2| {
                        if let CborValue::Text(s) = v2 { Some(s.clone()) } else { None }
                    }),
                    _ => None,
                }
            })
        }
        _ => None,
    }
}
fn i128_to_i64(i: i128) -> i64 {
    if i > i64::MAX as i128 { i64::MAX }
    else if i < i64::MIN as i128 { i64::MIN }
    else { i as i64 }
}

/// Convert raw CID bytes (from DAG-CBOR blob ref) to a base32 CID string (bafy...)
/// CIDv1 format: [version][codec][multihash]
/// We encode as base32 lower-case with "b" prefix.
fn cid_bytes_to_string(bytes: &[u8]) -> String {
    // In DAG-CBOR, CID references are encoded with a 0x00 multibase prefix byte
    // followed by the raw CIDv1 bytes: [version=0x01, codec, multihash...]
    // We skip the 0x00 prefix and base32-encode the CID.
    let cid_bytes = if bytes.len() > 1 && bytes[0] == 0x00 {
        &bytes[1..]
    } else {
        bytes
    };
    
    let mut base32 = String::new();
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for &b in cid_bytes {
        buffer = (buffer << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            base32.push(ALPHABET[((buffer >> bits) & 0x1f) as usize] as char);
        }
    }
    if bits > 0 {
        base32.push(ALPHABET[((buffer << (5 - bits)) & 0x1f) as usize] as char);
    }
    format!("b{}", base32)
}

/// Extract self-labels from a post record (NSFW, etc.)
/// Returns a JSON array of label strings like ["!warn", "nsfw", "porn"]
fn extract_labels(record: &CborValue) -> Option<serde_json::Value> {
    let labels_val = match record {
        CborValue::Map(map) => map.get(&CborValue::Text("labels".to_string())),
        _ => None,
    }?;
    
    // labels can be a self-labels object: { "$type": "com.atproto.label.defs#selfLabels", "values": [{ "val": "nsfw" }] }
    let values = match labels_val {
        CborValue::Map(map) => map.get(&CborValue::Text("values".to_string())),
        _ => None,
    }?;
    
    if let CborValue::Array(arr) = values {
        let labels: Vec<String> = arr.iter().filter_map(|v| {
            match v {
                CborValue::Map(m) => m.get(&CborValue::Text("val".to_string())).and_then(|v| if let CborValue::Text(s) = v { Some(s.clone()) } else { None }),
                _ => None,
            }
        }).collect();
        
        if labels.is_empty() { return None; }
        return Some(serde_json::Value::Array(labels.into_iter().map(serde_json::Value::String).collect()));
    }
    
    None
}

/// Extract embeds from a post record (images, videos, link cards).
/// Returns (JSON representation, has_media flag)
fn extract_embeds(record: &CborValue, author_did: &str) -> (Option<serde_json::Value>, bool) {
    let embed_val = match record {
        CborValue::Map(map) => {
            // Try both "embed" and "$type" as keys
            map.get(&CborValue::Text("embed".to_string()))
        },
        _ => None,
    };
    
    let embed = match embed_val {
        Some(e) => e,
        None => return (None, false),
    };
    
    // Debug: dump all keys in the embed
    if let CborValue::Map(map) = embed {
        let keys: Vec<String> = map.keys().filter_map(|k| if let CborValue::Text(s) = k { Some(s.clone()) } else { None }).collect();
    }
    
    let embed_type = cbor_text(embed, "$type").unwrap_or("");

    // recordWithMedia = a quote plus whatever the `media` half holds
    // (images, gallery, video or an external card). This must be tested
    // BEFORE the `record` arm below, because the NSID contains "record":
    // the generic quote branch used to swallow the whole embed, fail to find
    // its uri (the RWM wrapper nests it at record.record.uri) and return
    // (None, false) — every such post landed text-only with no images and no
    // quote (2026-11-15, post 3n2e3uz4eac2z).
    if embed_type.contains("recordWithMedia") {
        let mut entries: Vec<serde_json::Value> = Vec::new();
        let mut has_media = false;
        if let Some(uri) = quote_uri(cbor_map_get(embed, "record")) {
            entries.push(serde_json::json!({ "type": "quote", "uri": uri }));
        }
        if let Some(media) = cbor_map_get(embed, "media") {
            // The `media` half is one of images/gallery/video/external —
            // re-enter through a record-shaped wrapper so the single-embed
            // branches below stay the only place that reads a blob.
            let mut wrapper = std::collections::BTreeMap::new();
            wrapper.insert(CborValue::Text("embed".to_string()), media.clone());
            let (m, hm) = extract_embeds(&CborValue::Map(wrapper), author_did);
            has_media = hm;
            if let Some(arr) = m.and_then(|v| v.as_array().cloned()) {
                entries.extend(arr);
            }
        }
        return if entries.is_empty() {
            (None, false)
        } else {
            (Some(serde_json::Value::Array(entries)), has_media)
        };
    }

    if embed_type.contains("images") || embed_type.contains("gallery") {
        // Image embed: { images|items: [{ alt, image: { ref: { $link }, mimeType, ... }, ... }] }
        let images = match embed {
            CborValue::Map(map) => map
                .get(&CborValue::Text("images".to_string()))
                .or_else(|| map.get(&CborValue::Text("items".to_string()))),
            _ => None,
        };
        
        if let Some(CborValue::Array(arr)) = images {
            let mut media_urls = vec![];
            for img in arr {
                let alt = cbor_text(img, "alt").unwrap_or("");
                
                // Debug: dump image keys
                if let CborValue::Map(m) = img {
                    let keys: Vec<String> = m.keys().filter_map(|k| if let CborValue::Text(s) = k { Some(s.clone()) } else { None }).collect();
                }
                
                // Get image blob — structure: { "$type": "blob", "ref": { "$link": "bafy..." }, "mimeType": "image/jpeg", "size": N }
                let image_blob = match img {
                    CborValue::Map(map) => map.get(&CborValue::Text("image".to_string())),
                    _ => None,
                };
                
                let cid = match image_blob {
                    Some(CborValue::Map(blob_map)) => {
                        let ref_val = blob_map.get(&CborValue::Text("ref".to_string()));
                        ref_val.and_then(|r| {
                            match r {
                                CborValue::Bytes(b) => Some(cid_bytes_to_string(b)),
                                CborValue::Map(ref_map) => ref_map.get(&CborValue::Text("$link".to_string())).and_then(|l| if let CborValue::Text(s) = l { Some(s.clone()) } else { None }),
                                _ => None,
                            }
                        })
                    },
                    Some(CborValue::Tag(tag, val)) => {
                        // Tag 42 = CID in DAG-CBOR
                        if let CborValue::Bytes(b) = val.as_ref() {
                            Some(hex::encode(b))
                        } else { None }
                    },
                    _ => {
                        None
                    }
                };
                
                let mime = image_blob.and_then(|b| if let CborValue::Map(m) = b { m.get(&CborValue::Text("mimeType".to_string())).and_then(|v| if let CborValue::Text(s) = v { Some(s.clone()) } else { None }) } else { None }).unwrap_or_default();
                
                
                if let Some(cid) = cid {
                    // Bluesky blob URLs: https://cdn.bsky.app/img/feed_thumbnail/plain/{did}/{cid}@jpeg
                    // We don't have the author DID here easily, so store the CID and resolve later
                    media_urls.push(serde_json::json!({
                        "type": "image",
                        "cid": cid,
                        "mime_type": mime,
                        "alt": alt,
                    }));
                }
            }
            
            if !media_urls.is_empty() {
                return (Some(serde_json::Value::Array(media_urls.clone())), true);
            }
        }
    } else if embed_type.contains("video") {
        // Video embed — the video is a blob, playlist URL is at cdn.bsky.app/hls/{did}/{cid}/playlist.m3u8
        let video_blob = match embed {
            CborValue::Map(map) => map.get(&CborValue::Text("video".to_string())),
            _ => None,
        };
        let cid = match video_blob {
            Some(CborValue::Map(blob_map)) => {
                blob_map.get(&CborValue::Text("ref".to_string())).and_then(|r| {
                    match r {
                        CborValue::Bytes(b) => Some(cid_bytes_to_string(b)),
                        CborValue::Map(ref_map) => ref_map.get(&CborValue::Text("$link".to_string())).and_then(|l| if let CborValue::Text(s) = l { Some(s.clone()) } else { None }),
                        _ => None,
                    }
                })
            },
            _ => None,
        };
        if let Some(cid) = cid {
            // We don't have the author DID here, so we'll store the CID and resolve in the feed
            return (Some(serde_json::json!([{"type": "video", "cid": cid}])), true);
        }
    } else if embed_type.contains("record") {
        // Quote post — extract the quoted post URI
        let record = match embed {
            CborValue::Map(map) => map.get(&CborValue::Text("record".to_string())),
            _ => None,
        };
        if let Some(uri) = quote_uri(record) {
            return (Some(serde_json::json!([{"type": "quote", "uri": uri}])), false);
        }
    } else if embed_type.contains("external") {
        // Link card — extract URI, title, description, and thumbnail
        let external = match embed {
            CborValue::Map(map) => map.get(&CborValue::Text("external".to_string())),
            _ => None,
        };
        if let Some(ext) = external {
            let uri = cbor_text(ext, "uri").unwrap_or("");
            let title = cbor_text(ext, "title").unwrap_or("");
            let desc = cbor_text(ext, "description").unwrap_or("");
            // Extract thumbnail — it's a blob ref in the "thumb" field
            let thumb = match ext {
                CborValue::Map(map) => map.get(&CborValue::Text("thumb".to_string())),
                _ => None,
            };
            let thumb_cid = match thumb {
                Some(CborValue::Map(blob_map)) => {
                    blob_map.get(&CborValue::Text("ref".to_string())).and_then(|r| {
                        match r {
                            CborValue::Bytes(b) => Some(cid_bytes_to_string(b)),
                            CborValue::Map(ref_map) => ref_map.get(&CborValue::Text("$link".to_string())).and_then(|l| if let CborValue::Text(s) = l { Some(s.clone()) } else { None }),
                            _ => None,
                        }
                    })
                }
                _ => None,
            };
            // We need the author DID for the CDN URL — get it from the record
            let thumb_url = thumb_cid.map(|cid| format!("https://cdn.bsky.app/img/feed_thumbnail/plain/{}/{}@jpeg", author_did, cid));
            return (Some(serde_json::json!([{"type": "link", "url": uri, "title": title, "description": desc, "thumbnail": thumb_url}])), false);
        }
    }
    
    (None, false)
}

/// Quoted-post URI from either nesting an embed can carry:
///
/// * flat — a plain `app.bsky.embed.record` posts `record: {uri, cid}`
/// * wrapped — `recordWithMedia` posts `record: {$type, record: {uri, cid}}`
///
/// Reading only the flat form is what made every recordWithMedia post drop
/// its quote as well as its media.
fn quote_uri(record: Option<&CborValue>) -> Option<String> {
    let rec = record?;
    cbor_text(rec, "uri")
        .or_else(|| cbor_map_get(rec, "record").and_then(|inner| cbor_text(inner, "uri")))
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sqlx::Row;
    use std::collections::BTreeMap;

    fn uvarint(mut n: u64) -> Vec<u8> {
        let mut out = vec![];
        loop {
            let b = (n & 0x7f) as u8;
            n >>= 7;
            out.push(if n == 0 { b } else { b | 0x80 });
            if n == 0 {
                break;
            }
        }
        out
    }

    /// Synthetic CAR: one record block behind a stand-in 36-byte CID prefix.
    fn car_with_record(record: &serde_json::Value) -> Vec<u8> {
        let block = serde_cbor::to_vec(record).unwrap();
        let header = serde_cbor::to_vec(&json!({"roots": [], "version": 1})).unwrap();
        let mut car = uvarint(header.len() as u64);
        car.extend(header);
        let mut entry = vec![0u8; 36];
        entry.extend(&block);
        car.extend(uvarint(entry.len() as u64));
        car.extend(entry);
        car
    }

    /// A structurally valid CIDv1 (01 71 12 20 + 32-byte digest) with a
    /// distinct digest per seed.
    fn test_cid(seed: u8) -> Vec<u8> {
        let mut v = vec![0x01, 0x71, 0x12, 0x20];
        v.extend([seed; 32]);
        v
    }

    /// Synthetic #commit frame carrying one record in its CAR blocks.
    fn frame_for(did: &str, action: &str, path: &str, record: &serde_json::Value, cid_seed: u8) -> Vec<u8> {
        let blocks = car_with_record(record);
        let mut op = BTreeMap::new();
        op.insert(CborValue::Text("action".into()), CborValue::Text(action.into()));
        op.insert(CborValue::Text("path".into()), CborValue::Text(path.into()));
        op.insert(CborValue::Text("cid".into()), CborValue::Bytes(test_cid(cid_seed)));
        let mut body = BTreeMap::new();
        body.insert(CborValue::Text("repo".into()), CborValue::Text(did.into()));
        body.insert(CborValue::Text("ops".into()), CborValue::Array(vec![CborValue::Map(op)]));
        body.insert(CborValue::Text("blocks".into()), CborValue::Bytes(blocks));
        body.insert(CborValue::Text("seq".into()), CborValue::Integer(1));
        let mut header = BTreeMap::new();
        header.insert(CborValue::Text("t".into()), CborValue::Text("#commit".into()));
        header.insert(CborValue::Text("op".into()), CborValue::Integer(1));
        let mut out = serde_cbor::to_vec(&CborValue::Map(header)).unwrap();
        out.extend(serde_cbor::to_vec(&CborValue::Map(body)).unwrap());
        out
    }

    fn story_json() -> serde_json::Value {
        json!({
            "createdAt": "2026-11-01T12:00:00Z",
            "expiresAt": "2026-11-02T12:00:00Z",
            "activeFrom": "2026-11-01T11:00:00Z",
            "pinned": true,
            "text": "show flyer",
            "canReact": false,
            "items": [
                {"image": {"ref": {"$link": "bafyimg"}, "mimeType": "image/jpeg"}, "alt": "flyer", "duration": 900},
                {"video": {"ref": {"$link": "bafyvid"}, "mimeType": "video/mp4"}}
            ]
        })
    }

    #[test]
    fn parse_story_record_full() {
        let val = serde_cbor::from_slice::<CborValue>(&serde_cbor::to_vec(&story_json()).unwrap()).unwrap();
        let s = parse_story_record(&val).expect("valid record parses");
        assert_eq!(s.created_at.to_rfc3339(), "2026-11-01T12:00:00+00:00");
        assert_eq!(s.expires_at.unwrap().to_rfc3339(), "2026-11-02T12:00:00+00:00");
        assert_eq!(s.active_from.unwrap().to_rfc3339(), "2026-11-01T11:00:00+00:00");
        assert!(s.pinned);
        assert!(!s.can_react);
        assert!(s.can_reply);
        assert_eq!(s.text.as_deref(), Some("show flyer"));
        let items = s.items.as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["type"], "image");
        assert_eq!(items[0]["cid"], "bafyimg");
        assert_eq!(items[0]["duration"], 300); // clamped from 900
        assert_eq!(items[1]["type"], "video");
    }

    #[test]
    fn parse_story_record_defaults() {
        let val = serde_cbor::from_slice::<CborValue>(
            &serde_cbor::to_vec(&json!({
                "createdAt": "2026-11-01T12:00:00Z",
                "items": [{"image": {"ref": {"$link": "bafyimg"}, "mimeType": "image/png"}}]
            })).unwrap(),
        ).unwrap();
        let s = parse_story_record(&val).expect("minimal record parses");
        assert!(!s.pinned);
        assert!(s.can_react && s.can_reply);
        assert!(s.active_from.is_none() && s.expires_at.is_none());
        assert!(s.text.is_none());
    }

    #[test]
    fn parse_story_record_rejects_malformed() {
        // Missing createdAt.
        let v = serde_cbor::from_slice::<CborValue>(
            &serde_cbor::to_vec(&json!({"items": [{"image": {"ref": {"$link": "b"}}}]})).unwrap(),
        ).unwrap();
        assert!(parse_story_record(&v).is_none());
        // Bad timestamp.
        let v = serde_cbor::from_slice::<CborValue>(
            &serde_cbor::to_vec(&json!({"createdAt": "yesterday", "items": [{"image": {"ref": {"$link": "b"}}}]})).unwrap(),
        ).unwrap();
        assert!(parse_story_record(&v).is_none());
        // Zero media items.
        let v = serde_cbor::from_slice::<CborValue>(
            &serde_cbor::to_vec(&json!({"createdAt": "2026-11-01T12:00:00Z", "items": []})).unwrap(),
        ).unwrap();
        assert!(parse_story_record(&v).is_none());
        // Items present but no usable blobs.
        let v = serde_cbor::from_slice::<CborValue>(
            &serde_cbor::to_vec(&json!({"createdAt": "2026-11-01T12:00:00Z", "items": [{"alt": "x"}]})).unwrap(),
        ).unwrap();
        assert!(parse_story_record(&v).is_none());
    }

    #[test]
    fn parse_status_record_expiry_and_music() {
        // Explicit duration + embed with nested external link.
        let v = serde_cbor::from_slice::<CborValue>(
            &serde_cbor::to_vec(&json!({
                "status": "listening to…",
                "createdAt": "2026-11-01T12:00:00Z",
                "durationMinutes": 60,
                "embed": {"external": {"uri": "https://soundcloud.com/x"}}
            })).unwrap(),
        ).unwrap();
        let s = parse_status_record(&v).expect("valid status parses");
        assert_eq!(s.expires_at.to_rfc3339(), "2026-11-01T13:00:00+00:00");
        assert_eq!(s.music_url.as_deref(), Some("https://soundcloud.com/x"));
        assert_eq!(s.status, "listening to…");
        // Missing duration defaults to 24h.
        let v = serde_cbor::from_slice::<CborValue>(
            &serde_cbor::to_vec(&json!({"status": "hi", "createdAt": "2026-11-01T12:00:00Z"})).unwrap(),
        ).unwrap();
        let s = parse_status_record(&v).unwrap();
        assert_eq!(s.expires_at.to_rfc3339(), "2026-11-02T12:00:00+00:00");
        assert!(s.music_url.is_none());
        // Missing createdAt rejects.
        let v = serde_cbor::from_slice::<CborValue>(&serde_cbor::to_vec(&json!({"status": "hi"})).unwrap()).unwrap();
        assert!(parse_status_record(&v).is_none());
    }

    #[test]
    fn by_field_matches_map_valued_keys() {
        // Regression: likes/reposts carry "subject" as a map — the old
        // text-only matcher never found them.
        let car = car_with_record(&json!({"subject": {"uri": "at://did/x", "cid": "c"}}));
        assert!(extract_record_from_car_by_field(&car, "subject").is_some());
        // Story records may have no "text" field at all.
        let car = car_with_record(&json!({"createdAt": "2026-11-01T12:00:00Z", "items": [{"image": {"ref": {"$link": "b"}}}]}));
        assert!(extract_record_from_car_by_field(&car, "items").is_some());
        // Key absence finds nothing.
        assert!(extract_record_from_car_by_field(&car, "subject").is_none());
    }

    /// DB integration: synthetic commit frames index rows and interaction
    /// frames bump counts. Skips without DATABASE_URL.
    #[tokio::test]
    async fn frame_indexes_story_status_and_interactions() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("DATABASE_URL unset — skipping DB integration test");
            return;
        };
        let pool = PgPool::connect(&url).await.expect("db connect");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let did = format!("did:plc:ctest{}", stamp);
        let story_uri = format!("at://{}/com.bytescape.story/test", did);
        let status_uri = format!("at://{}/app.bsky.actor.status/self", did);
        let mut cursor = 0i64;

        // Story create: row lands with clamped duration and pinned flag.
        let frame = frame_for(&did, "create", "com.bytescape.story/test", &story_json(), 1);
        process_frame(&pool, &frame, &mut cursor).await.expect("story frame");
        let row = sqlx::query(
            "SELECT pinned, text, expires_at, media FROM appview_stories WHERE uri = $1",
        )
        .bind(&story_uri)
        .fetch_optional(&pool)
        .await
        .unwrap()
        .expect("story row indexed");
        let (pinned, text): (bool, String) = (row.get(0), row.get(1));
        let expires: chrono::DateTime<chrono::Utc> = row.get(2);
        let media: serde_json::Value = row.get(3);
        assert!(pinned);
        assert_eq!(text, "show flyer");
        assert_eq!(expires.to_rfc3339(), "2026-11-02T12:00:00+00:00");
        assert_eq!(media[0]["duration"], 300);
        assert_eq!(media[1]["type"], "video");

        // Like targeting the story bumps the story row.
        let like = frame_for(&did, "create", "app.bsky.feed.like/l1",
            &json!({"subject": {"uri": story_uri, "cid": "c"}}), 2);
        process_frame(&pool, &like, &mut cursor).await.expect("like frame");
        let likes: i64 = sqlx::query("SELECT like_count FROM appview_stories WHERE uri = $1")
            .bind(&story_uri).fetch_one(&pool).await.unwrap().get(0);
        assert_eq!(likes, 1);

        // Repost (reshare) targeting the story bumps repost_count.
        let repost = frame_for(&did, "create", "app.bsky.feed.repost/r0",
            &json!({"subject": {"uri": story_uri, "cid": "c"}}), 8);
        process_frame(&pool, &repost, &mut cursor).await.expect("repost frame");
        let reposts: i64 = sqlx::query("SELECT repost_count FROM appview_stories WHERE uri = $1")
            .bind(&story_uri).fetch_one(&pool).await.unwrap().get(0);
        assert_eq!(reposts, 1);

        // A reply whose parent is the story bumps the story's reply_count.
        let reply = frame_for(&did, "create", "app.bsky.feed.post/r1",
            &json!({
                "text": "nice!", "createdAt": "2026-11-01T12:30:00Z",
                "reply": {"parent": {"uri": story_uri, "cid": "c"}, "root": {"uri": story_uri, "cid": "c"}}
            }), 3);
        process_frame(&pool, &reply, &mut cursor).await.expect("reply frame");
        let replies: i64 = sqlx::query("SELECT reply_count FROM appview_stories WHERE uri = $1")
            .bind(&story_uri).fetch_one(&pool).await.unwrap().get(0);
        assert_eq!(replies, 1);

        // Status create: computed expiry + music link captured.
        let status = frame_for(&did, "create", "app.bsky.actor.status/self",
            &json!({
                "status": "listening to…", "createdAt": "2026-11-01T12:00:00Z",
                "durationMinutes": 60,
                "embed": {"external": {"uri": "https://soundcloud.com/x"}}
            }), 4);
        process_frame(&pool, &status, &mut cursor).await.expect("status frame");
        let row = sqlx::query("SELECT status, expires_at, music_url, is_live FROM appview_status WHERE author_did = $1")
            .bind(&did).fetch_optional(&pool).await.unwrap().expect("status row indexed");
        let (st, exp, music, live): (String, chrono::DateTime<chrono::Utc>, Option<String>, bool) =
            (row.get(0), row.get(1), row.get(2), row.get(3));
        assert_eq!(st, "listening to…");
        assert_eq!(exp.to_rfc3339(), "2026-11-01T13:00:00+00:00");
        assert_eq!(music.as_deref(), Some("https://soundcloud.com/x"));
        assert!(!live, "ordinary status must not be flagged live");

        // Like on the status, then replacement (new cid) resets counts.
        let like = frame_for(&did, "create", "app.bsky.feed.like/l2",
            &json!({"subject": {"uri": status_uri, "cid": "c"}}), 5);
        process_frame(&pool, &like, &mut cursor).await.expect("status like frame");
        let likes: i64 = sqlx::query("SELECT like_count FROM appview_status WHERE author_did = $1")
            .bind(&did).fetch_one(&pool).await.unwrap().get(0);
        assert_eq!(likes, 1);
        let status2 = frame_for(&did, "create", "app.bsky.actor.status/self",
            &json!({"status": "writing", "createdAt": "2026-11-01T14:00:00Z"}), 6);
        process_frame(&pool, &status2, &mut cursor).await.expect("status replace frame");
        let row = sqlx::query("SELECT status, like_count FROM appview_status WHERE author_did = $1")
            .bind(&did).fetch_one(&pool).await.unwrap();
        let (st, likes): (String, i64) = (row.get(0), row.get(1));
        assert_eq!(st, "writing");

        // Real-world Bluesky live status: sentinel text, human text in embed title.
        let status3 = frame_for(&did, "create", "app.bsky.actor.status/self",
            &json!({
                "status": "app.bsky.actor.status#live", "createdAt": "2026-11-01T15:00:00Z",
                "embed": {"$type": "app.bsky.embed.external",
                    "external": {"title": "Walking through Shalgora", "uri": "https://www.twitch.tv/daggerwalk"}}
            }), 7);
        process_frame(&pool, &status3, &mut cursor).await.expect("live status frame");
        let row = sqlx::query("SELECT status, music_url, is_live FROM appview_status WHERE author_did = $1")
            .bind(&did).fetch_one(&pool).await.unwrap();
        let (st, music, live): (String, Option<String>, bool) = (row.get(0), row.get(1), row.get(2));
        assert_eq!(st, "Walking through Shalgora");
        assert_eq!(music.as_deref(), Some("https://www.twitch.tv/daggerwalk"));
        assert!(live, "sentinel live status must be flagged");
        assert_eq!(likes, 0, "counts reset on self-keyed replacement");

        // Story delete removes the row.
        let del = frame_for(&did, "delete", "com.bytescape.story/test", &json!({}), 7);
        process_frame(&pool, &del, &mut cursor).await.expect("delete frame");
        let gone = sqlx::query("SELECT 1 FROM appview_stories WHERE uri = $1")
            .bind(&story_uri).fetch_optional(&pool).await.unwrap();
        assert!(gone.is_none(), "deleted story row removed");

        // Cleanup.
        let _ = sqlx::query("DELETE FROM appview_status WHERE author_did = $1").bind(&did).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM appview_posts WHERE author_did = $1").bind(&did).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM appview_story_seen WHERE viewer_did = $1").bind(&did).execute(&pool).await;
    }

    #[test]
    fn by_type_matches_only_listed_calendar_nsids() {
        let ev = car_with_record(&json!({
            "$type": "community.lexicon.calendar.event",
            "name": "x", "createdAt": "2026-11-01T12:00:00Z"
        }));
        assert!(extract_record_from_car_by_type(&ev, crate::calendar::RECORD_NSIDS).is_some());

        let legacy = car_with_record(&json!({
            "$type": "events.smokesignal.Event", "name": "old"
        }));
        assert!(extract_record_from_car_by_type(&legacy, crate::calendar::RECORD_NSIDS).is_some());

        let rsvp = car_with_record(&json!({
            "$type": "community.lexicon.calendar.rsvp",
            "subject": {"uri": "at://did:plc:x/community.lexicon.calendar.event/3y"},
            "status": "community.lexicon.calendar.rsvp#going"
        }));
        // Event extractor must not grab RSVP records and vice versa.
        assert!(extract_record_from_car_by_type(&rsvp, crate::calendar::RECORD_NSIDS).is_none());
        assert!(extract_record_from_car_by_type(&rsvp, &[crate::calendar::RSVP_NSID]).is_some());

        // Unrelated records never match.
        let post = car_with_record(&json!({"$type": "app.bsky.feed.post", "text": "hi", "createdAt": "2026-11-01T12:00:00Z"}));
        assert!(extract_record_from_car_by_type(&post, crate::calendar::RECORD_NSIDS).is_none());
    }

    /// DB integration: calendar frames upsert flat rows idempotently and
    /// deletes remove them (spec: replay never duplicates; no phantom rows).
    /// Skips without DATABASE_URL.
    #[tokio::test]
    async fn frame_indexes_calendar_records() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("DATABASE_URL unset — skipping DB integration test");
            return;
        };
        let pool = PgPool::connect(&url).await.expect("db connect");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let did = format!("did:plc:ctestcal{}", stamp);
        let event_uri = format!("at://{}/community.lexicon.calendar.event/testev", did);
        let rsvp_uri = format!("at://{}/community.lexicon.calendar.rsvp/testrsvp", did);
        let mut cursor = 0i64;

        let event_json = json!({
            "$type": "community.lexicon.calendar.event",
            "name": "Frame test event",
            "description": "indexed from a synthetic commit",
            "createdAt": "2026-11-01T12:00:00Z",
            "startsAt": "2026-11-20T18:00:00Z",
            "locations": [{"$type": "community.lexicon.location.address", "name": "Hall", "locality": "Lisbon"}]
        });

        // create → flat columns land
        let frame = frame_for(&did, "create", "community.lexicon.calendar.event/testev", &event_json, 11);
        process_frame(&pool, &frame, &mut cursor).await.expect("event frame");
        let row = sqlx::query(
            "SELECT name, starts_at, location_text, cid, status FROM calendar_events WHERE uri = $1",
        )
        .bind(&event_uri)
        .fetch_optional(&pool)
        .await
        .unwrap()
        .expect("event row indexed");
        let (name, starts_at, location_text, _cid, status): (String, Option<chrono::DateTime<chrono::Utc>>, Option<String>, String, String) =
            (row.get(0), row.get(1), row.get(2), row.get(3), row.get(4));
        assert_eq!(name, "Frame test event");
        assert_eq!(starts_at.expect("starts_at column").to_rfc3339(), "2026-11-20T18:00:00+00:00");
        assert_eq!(location_text.as_deref(), Some("Hall, Lisbon"));
        assert_eq!(status, "community.lexicon.calendar.event#scheduled");

        // replay with changed content → one row, updated (idempotent upsert)
        let frame2 = frame_for(
            &did, "create", "community.lexicon.calendar.event/testev",
            &json!({
                "$type": "community.lexicon.calendar.event",
                "name": "Frame test event (updated)",
                "createdAt": "2026-11-01T12:00:00Z",
                "startsAt": "2026-11-20T19:00:00Z"
            }),
            12,
        );
        process_frame(&pool, &frame2, &mut cursor).await.expect("event replay frame");
        let (count, name): (i64, String) = sqlx::query_as(
            "SELECT count(*), max(name) FROM calendar_events WHERE uri = $1",
        )
        .bind(&event_uri)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1, "replay must never duplicate the row");
        assert_eq!(name, "Frame test event (updated)");

        // rsvp create → rollup row keyed to the event
        let rsvp_json = json!({
            "$type": "community.lexicon.calendar.rsvp",
            "subject": {"uri": event_uri, "cid": "bafytest"},
            "status": "community.lexicon.calendar.rsvp#going"
        });
        let frame3 = frame_for(&did, "create", "community.lexicon.calendar.rsvp/testrsvp", &rsvp_json, 13);
        process_frame(&pool, &frame3, &mut cursor).await.expect("rsvp frame");
        let (status, event): (String, String) = sqlx::query_as(
            "SELECT status, event_uri FROM calendar_rsvps WHERE uri = $1",
        )
        .bind(&rsvp_uri)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(status, "community.lexicon.calendar.rsvp#going");
        assert_eq!(event, event_uri);

        // event delete → row gone (and its comments/reactions presentation)
        let del = frame_for(&did, "delete", "community.lexicon.calendar.event/testev", &json!({}), 14);
        process_frame(&pool, &del, &mut cursor).await.expect("event delete frame");
        let gone = sqlx::query("SELECT 1 FROM calendar_events WHERE uri = $1")
            .bind(&event_uri).fetch_optional(&pool).await.unwrap();
        assert!(gone.is_none(), "deleted event row removed");

        // rsvp delete → rollup row gone
        let del2 = frame_for(&did, "delete", "community.lexicon.calendar.rsvp/testrsvp", &json!({}), 15);
        process_frame(&pool, &del2, &mut cursor).await.expect("rsvp delete frame");
        let gone = sqlx::query("SELECT 1 FROM calendar_rsvps WHERE uri = $1")
            .bind(&rsvp_uri).fetch_optional(&pool).await.unwrap();
        assert!(gone.is_none(), "deleted rsvp row removed");

        // Cleanup.
        let _ = sqlx::query("DELETE FROM calendar_events WHERE uri = $1").bind(&event_uri).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM calendar_rsvps WHERE uri LIKE $1").bind(format!("at://{}/%", did)).execute(&pool).await;
    }

    // ── extract_embeds: the compact array stored in appview_posts.embeds ──
    // The array is the only embed data the DB feed path ever sees, so these
    // tests pin the shape the firehose ingest produces for every embed kind
    // `api::feed::normalize::parse_bluesky_embeds` claims to handle.

    fn post_with_embed(embed: serde_json::Value) -> CborValue {
        serde_cbor::from_slice::<CborValue>(
            &serde_cbor::to_vec(&json!({
                "$type": "app.bsky.feed.post",
                "text": "hi",
                "createdAt": "2026-11-15T13:15:52.647Z",
                "embed": embed
            }))
            .unwrap(),
        )
        .unwrap()
    }

    fn entries_of(embeds: &serde_json::Value) -> Vec<serde_json::Value> {
        embeds.as_array().expect("array shape").clone()
    }

    fn image_blob(cid: &str, mime: &str) -> serde_json::Value {
        json!({ "$type": "blob", "ref": { "$link": cid }, "mimeType": mime, "size": 1 })
    }

    #[test]
    fn extract_embeds_images() {
        let rec = post_with_embed(json!({
            "$type": "app.bsky.embed.images",
            "images": [
                { "alt": "a cat", "image": image_blob("bafkrei1", "image/jpeg") },
                { "alt": "", "image": image_blob("bafkrei2", "image/jpeg") }
            ]
        }));
        let (embeds, has_media) = extract_embeds(&rec, "did:plc:a");
        assert!(has_media);
        let entries = entries_of(&embeds.expect("images produce embeds"));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["type"], "image");
        assert_eq!(entries[0]["cid"], "bafkrei1");
        assert_eq!(entries[0]["mime_type"], "image/jpeg");
        assert_eq!(entries[0]["alt"], "a cat");
        assert_eq!(entries[1]["cid"], "bafkrei2");
    }

    #[test]
    fn extract_embeds_video() {
        let rec = post_with_embed(json!({
            "$type": "app.bsky.embed.video",
            "video": image_blob("bafyvid", "video/mp4")
        }));
        let (embeds, has_media) = extract_embeds(&rec, "did:plc:a");
        assert!(has_media);
        let entries = entries_of(&embeds.expect("video produces embeds"));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["type"], "video");
        assert_eq!(entries[0]["cid"], "bafyvid");
    }

    #[test]
    fn extract_embeds_plain_quote() {
        // app.bsky.embed.record posts a flat strongRef: record: {uri, cid}.
        let rec = post_with_embed(json!({
            "$type": "app.bsky.embed.record",
            "record": { "uri": "at://did:plc:b/app.bsky.feed.post/q", "cid": "bafyq" }
        }));
        let (embeds, has_media) = extract_embeds(&rec, "did:plc:a");
        assert!(!has_media);
        let entries = entries_of(&embeds.expect("quote produces embeds"));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["type"], "quote");
        assert_eq!(entries[0]["uri"], "at://did:plc:b/app.bsky.feed.post/q");
    }

    #[test]
    fn extract_embeds_quote_with_wrapped_nesting() {
        // The nesting recordWithMedia carries: record: {$type, record: {uri}}.
        // Reading only record.uri here is what dropped every quote — and,
        // because the NSID contains "record", the whole embed.
        let rec = post_with_embed(json!({
            "$type": "app.bsky.embed.record",
            "record": {
                "$type": "app.bsky.embed.record",
                "record": { "uri": "at://did:plc:b/app.bsky.feed.post/q", "cid": "bafyq" }
            }
        }));
        let (embeds, has_media) = extract_embeds(&rec, "did:plc:a");
        assert!(!has_media);
        let entries = entries_of(&embeds.expect("wrapped quote still resolves"));
        assert_eq!(entries[0]["type"], "quote");
        assert_eq!(entries[0]["uri"], "at://did:plc:b/app.bsky.feed.post/q");
    }

    /// The owner's repro post (3n2e3uz4eac2z): two images plus a quote. Both
    /// halves must survive — this is the shape that used to index as
    /// embeds=NULL, has_media=false.
    #[test]
    fn extract_embeds_record_with_media_images_and_quote() {
        let rec = post_with_embed(json!({
            "$type": "app.bsky.embed.recordWithMedia",
            "media": {
                "$type": "app.bsky.embed.images",
                "images": [
                    { "alt": "", "aspectRatio": { "width": 1280, "height": 719 },
                      "image": image_blob("bafkreifbvnpj4lx2pkfm3ca4ul7d4wocjdhpvebxhfs3q2srhkkp635zs4", "image/jpeg") },
                    { "alt": "", "aspectRatio": { "width": 2316, "height": 3088 },
                      "image": image_blob("bafkreibnxq24ojcr5hpe7bqm2vecvrvsfxdywycfhkoqjf62z6ph75r2c4", "image/jpeg") }
                ]
            },
            "record": {
                "$type": "app.bsky.embed.record",
                "record": { "uri": "at://did:plc:roc7kz76kqmgz2wom4azxu7u/app.bsky.feed.post/3n2daoaunss2i",
                            "cid": "bafyreiftun4bk2syyknozvxxojofdr46hmvusjb6kxjzvlrvchmxg373ge" }
            }
        }));
        let (embeds, has_media) = extract_embeds(&rec, "did:plc:h7nrtbesrqn7whklq3unvxvj");
        assert!(has_media, "recordWithMedia with images must set has_media");
        let entries = entries_of(&embeds.expect("recordWithMedia produces embeds"));
        assert_eq!(entries.iter().filter(|e| e["type"] == "image").count(), 2);
        assert_eq!(entries.iter().filter(|e| e["type"] == "quote").count(), 1);
        assert!(entries.iter().any(|e| e["type"] == "quote"
            && e["uri"] == "at://did:plc:roc7kz76kqmgz2wom4azxu7u/app.bsky.feed.post/3n2daoaunss2i"));
    }

    #[test]
    fn extract_embeds_record_with_media_video_and_quote() {
        let rec = post_with_embed(json!({
            "$type": "app.bsky.embed.recordWithMedia",
            "media": { "$type": "app.bsky.embed.video", "video": image_blob("bafyvid", "video/mp4") },
            "record": { "$type": "app.bsky.embed.record",
                        "record": { "uri": "at://did:plc:b/app.bsky.feed.post/q", "cid": "bafyq" } }
        }));
        let (embeds, has_media) = extract_embeds(&rec, "did:plc:a");
        assert!(has_media);
        let entries = entries_of(&embeds.expect("recordWithMedia produces embeds"));
        assert!(entries.iter().any(|e| e["type"] == "video" && e["cid"] == "bafyvid"));
        assert!(entries.iter().any(|e| e["type"] == "quote"));
    }

    #[test]
    fn extract_embeds_record_with_media_external_media() {
        let rec = post_with_embed(json!({
            "$type": "app.bsky.embed.recordWithMedia",
            "media": { "$type": "app.bsky.embed.external",
                       "external": { "uri": "https://example.com/x", "title": "T", "description": "D" } },
            "record": { "$type": "app.bsky.embed.record",
                        "record": { "uri": "at://did:plc:b/app.bsky.feed.post/q", "cid": "bafyq" } }
        }));
        let (embeds, has_media) = extract_embeds(&rec, "did:plc:a");
        assert!(!has_media, "a link card is not media");
        let entries = entries_of(&embeds.expect("recordWithMedia produces embeds"));
        assert!(entries.iter().any(|e| e["type"] == "link" && e["url"] == "https://example.com/x"));
        assert!(entries.iter().any(|e| e["type"] == "quote"));
    }

    #[test]
    fn extract_embeds_external_link() {
        let rec = post_with_embed(json!({
            "$type": "app.bsky.embed.external",
            "external": { "uri": "https://example.com/y", "title": "Title",
                          "description": "Desc", "thumb": image_blob("bafythumb", "image/jpeg") }
        }));
        let (embeds, has_media) = extract_embeds(&rec, "did:plc:a");
        assert!(!has_media);
        let entries = entries_of(&embeds.expect("external produces embeds"));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["type"], "link");
        assert_eq!(entries[0]["url"], "https://example.com/y");
        assert_eq!(entries[0]["title"], "Title");
        assert!(entries[0]["thumbnail"]
            .as_str()
            .unwrap()
            .contains("cdn.bsky.app/img/feed_thumbnail/plain/did:plc:a/bafythumb@jpeg"));
    }

    #[test]
    fn extract_embeds_without_embed() {
        let rec: CborValue = serde_cbor::from_slice::<CborValue>(
            &serde_cbor::to_vec(&json!({ "$type": "app.bsky.feed.post", "text": "just words" })).unwrap(),
        )
        .unwrap();
        assert_eq!(extract_embeds(&rec, "did:plc:a"), (None, false));
    }
}
