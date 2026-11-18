//! Background worker that resolves Bluesky handles for posts with NULL `author_handle`.
//!
//! Queries `appview_posts` for distinct `author_did` values missing a handle,
//! batch-resolves them via the public Bluesky `getProfiles` XRPC endpoint (25 at
//! a time), then backfills both `appview_did_handle` and `appview_posts`.

use sqlx::PgPool;

/// Spawn the handle-resolution background worker. Returns immediately.
pub fn start_handle_resolver(db: PgPool) {
    tokio::spawn(async move {
        tracing::info!("[HandleResolver] background worker started");
        loop {
            // 1. Fetch up to 5 distinct DIDs whose posts or events still lack
            // a handle. Events indexed from the firehose rarely have posts,
            // so without the calendar_events arm their authors never resolve
            // and the UI falls back to raw DIDs.
            //
            // Each arm is LIMIT-bounded before the UNION: the outer
            // DISTINCT-over-the-union forced Postgres to aggregate every
            // unresolved row first (2.8M rows, a 28s scan on the test DB —
            // repeated every 30s, starving feed reads). Per-arm LIMIT 5 lets
            // each arm stop at its partial index (idx_appview_posts_unresolved
            // / a short calendar_events scan); the outer DISTINCT only ever
            // sorts the ≤10 candidates. "Any 5 missing DIDs" is preserved.
            let dids: Vec<String> = sqlx::query_scalar(
                "SELECT DISTINCT author_did FROM ( \
                     (SELECT author_did FROM appview_posts \
                      WHERE author_handle IS NULL AND author_did IS NOT NULL \
                      LIMIT 5) \
                     UNION ALL \
                     (SELECT author_did FROM calendar_events \
                      WHERE author_handle IS NULL AND author_did IS NOT NULL \
                      LIMIT 5) \
                 ) missing \
                 LIMIT 5",
            )
            .fetch_all(&db)
            .await
            .unwrap_or_default();

            if dids.is_empty() {
                // Nothing left to resolve — check again in 30 s.
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                continue;
            }

            tracing::debug!(
                "[HandleResolver] resolving {} DIDs: {:?}",
                dids.len(),
                &dids[..std::cmp::min(5, dids.len())]
            );

            // 2. Build the repeated `actors=` query string for getProfiles.
            let actors_query = dids
                .iter()
                .map(|d| format!("actors={}", d))
                .collect::<Vec<_>>()
                .join("&");
            let url = format!(
                "https://api.bsky.app/xrpc/app.bsky.actor.getProfiles?{}",
                actors_query
            );

            // 3. Call the API (tolerant of failures).
            match reqwest::Client::new()
                .get(&url)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    match resp.json::<serde_json::Value>().await {
                        Ok(json) => {
                            if let Some(profiles) =
                                json.get("profiles").and_then(|p| p.as_array())
                            {
                                let mut resolved = 0u32;
                                for profile in profiles {
                                    let handle = profile
                                        .get("handle")
                                        .and_then(|h| h.as_str());
                                    let did = profile
                                        .get("did")
                                        .and_then(|d| d.as_str());
                                    let avatar = profile
                                        .get("avatar")
                                        .and_then(|v| v.as_str());
                                    let display_name = profile
                                        .get("displayName")
                                        .and_then(|v| v.as_str());
                                    let (Some(handle), Some(did)) = (handle, did) else {
                                        continue;
                                    };
                                    // The AppView reports `handle.invalid` for
                                    // accounts whose handle it cannot resolve
                                    // (e.g. did:web users). That is a failure
                                    // marker, not a handle — persisting it
                                    // poisons by-handle lookups for real.
                                    if handle == "handle.invalid" {
                                        continue;
                                    }

                                    // Cache in did_handle table.
                                    let _ = sqlx::query(
                                        "INSERT INTO appview_did_handle (did, handle) \
                                         VALUES ($1, $2) \
                                         ON CONFLICT (did) DO UPDATE SET handle = EXCLUDED.handle",
                                    )
                                    .bind(did)
                                    .bind(handle)
                                    .execute(&db)
                                    .await;

                                    // Backfill posts table (handles authoritatively;
                                    // avatar/display name opportunistically — the offline
                                    // avatar backfill owns the bulk of that work).
                                    // Bounded: a single author can have thousands of rows;
                                    // without a statement_timeout one UPDATE can run for
                                    // minutes and, under the firehose writer's row locks,
                                    // starve every profile/feed read on the same table.
                                    // SET LOCAL needs a transaction to take effect.
                                    {
                                        let mut tx = match db.begin().await {
                                            Ok(t) => t,
                                            Err(_) => continue,
                                        };
                                        let _ = sqlx::query("SET LOCAL statement_timeout = '5s'")
                                            .execute(&mut *tx)
                                            .await;
                                        let _ = sqlx::query(
                                            "UPDATE appview_posts \
                                             SET author_handle = COALESCE(author_handle, $1), \
                                                 author_avatar = COALESCE($2, author_avatar), \
                                                 author_display_name = COALESCE($3, author_display_name) \
                                             WHERE author_did = $4",
                                        )
                                        .bind(handle)
                                        .bind(avatar)
                                        .bind(display_name)
                                        .bind(did)
                                        .execute(&mut *tx)
                                        .await;
                                        let _ = sqlx::query(
                                            "UPDATE calendar_events \
                                             SET author_handle = COALESCE(author_handle, $1) \
                                             WHERE author_did = $2",
                                        )
                                        .bind(handle)
                                        .bind(did)
                                        .execute(&mut *tx)
                                        .await;
                                        let _ = tx.commit().await;
                                    }

                                    resolved += 1;
                                }
                                tracing::info!(
                                    "[HandleResolver] resolved {}/{} profiles in this batch",
                                    resolved,
                                    dids.len()
                                );
                            } else {
                                tracing::warn!(
                                    "[HandleResolver] response missing `profiles` array"
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!("[HandleResolver] failed to parse JSON: {}", e);
                        }
                    }
                }
                Ok(resp) => {
                    tracing::warn!(
                        "[HandleResolver] getProfiles returned HTTP {}",
                        resp.status()
                    );
                }
                Err(e) => {
                    tracing::warn!("[HandleResolver] request failed: {}", e);
                }
            }

            // 4. Throttle: 30 s between batches. This worker rewrites
            // appview_posts rows (one heavy UPDATE per DID); running it tight
            // saturates the table's IO/row-locks and starves every profile/feed
            // read (the "Loading posts forever" hang). Background handle
            // backfill is not latency-sensitive — yield the table generously.
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
    });
}
