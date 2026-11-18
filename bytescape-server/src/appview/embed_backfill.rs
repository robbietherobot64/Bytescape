//! One-shot repair for posts whose embed was dropped at firehose ingest.
//!
//! `consumer::extract_embeds` used to swallow `app.bsky.embed.recordWithMedia`
//! — the NSID contains "record", so the generic quote branch took it, failed
//! to find the uri (the RWM wrapper nests it at `record.record.uri`) and
//! returned nothing. Every such post landed with `embeds = NULL` and
//! `has_media = false`: text only, no images, no quote (2026-11-15, post
//! 3n2e3uz4eac2z).
//!
//! The row cannot tell us its own shape any more, so the only way to find the
//! damaged rows is to ask the AppView: batch `app.bsky.feed.getPosts` over
//! candidates and write the views back through
//! `api::feed::sources::upsert_network_item`, whose `view_embed_to_db_json`
//! always handled recordWithMedia correctly (the hashtag backfill is why a
//! handful of such posts were already intact).
//!
//! Run shape: env-gated one-shot, absent = never runs (see main.rs).
//! `EMBED_BACKFILL=apply` writes; any other value dry-runs (fetch + count,
//! no writes). Window and page come from `EMBED_BACKFILL_DAYS`
//! (default 14), `EMBED_BACKFILL_LIMIT` (default 2000) and
//! `EMBED_BACKFILL_OFFSET` (default 0) so an operator can walk a range with
//! repeated runs — a genuinely embed-less row stays a candidate, so progress
//! comes from advancing the offset, not from the write.
//!
//! `EMBED_BACKFILL_URIS` (comma-separated) repairs exactly those posts and
//! ignores the window — for "this one post is still broken" reports and for
//! verifying the mapping without scanning the network.

use sqlx::PgPool;
use tracing::{info, warn};

const GET_POSTS: &str = "https://public.api.bsky.app/xrpc/app.bsky.feed.getPosts";
/// `app.bsky.feed.getPosts` takes at most 25 uris per call (lexicon
/// `maxLength: 25`); 50 came back as a non-2xx for every batch.
const BATCH: usize = 25;
const DEFAULT_DAYS: i64 = 14;
const DEFAULT_LIMIT: i64 = 2_000;

/// Counts for one run. `no_embed` is expected to dominate: most rows with
/// `embeds IS NULL` are genuinely text-only posts, and roughly 3% of the
/// network carries a recordWithMedia embed.
#[derive(Debug, Default)]
pub struct Stats {
    pub candidates: usize,
    pub fetched: usize,
    pub repaired: usize,
    pub no_embed: usize,
    /// Candidate uris the AppView did not return (deleted, blocked, takedown).
    pub unavailable: usize,
    pub failed: usize,
}

fn env_i64(key: &str, default: i64) -> i64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

/// `EMBED_BACKFILL_URIS` — comma-separated AT-URIs, anything else ignored.
fn parse_uris(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| s.starts_with("at://"))
        .collect()
}

/// One `app.bsky.feed.getPosts` call.
///
/// Errors carry the AppView's message: it rejects the whole request when any
/// one uri is unusable (over 25 uris, a non-`at://` uri, an unknown repo), so
/// the caller retries a rejected batch one uri at a time.
async fn fetch_views(
    http: &reqwest::Client,
    uris: &[String],
) -> Result<Vec<serde_json::Value>, String> {
    let mut req = http.get(GET_POSTS);
    for uri in uris {
        req = req.query(&[("uris", uri.as_str())]);
    }
    let resp = req.send().await.map_err(|e| format!("request failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let detail: String = resp.text().await.unwrap_or_default().chars().take(200).collect();
        return Err(format!("getPosts returned {status}: {detail}"));
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| format!("bad json: {e}"))?;
    Ok(body
        .get("posts")
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default())
}

pub async fn run(pool: &PgPool, apply: bool) -> Result<Stats, String> {
    let days = env_i64("EMBED_BACKFILL_DAYS", DEFAULT_DAYS);
    let limit = env_i64("EMBED_BACKFILL_LIMIT", DEFAULT_LIMIT);
    let offset = std::env::var("EMBED_BACKFILL_OFFSET")
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|v| *v >= 0)
        .unwrap_or(0);

    let explicit: Vec<String> = std::env::var("EMBED_BACKFILL_URIS")
        .map(|raw| parse_uris(&raw))
        .unwrap_or_default();

    let candidates: Vec<String> = if explicit.is_empty() {
        sqlx::query_scalar(
            // at:// only: the table also holds ActivityPub rows, and one
            // https:// uri in the batch fails the whole getPosts call with
            // "Invalid at-uri".
            r#"SELECT uri FROM appview_posts
               WHERE created_at > now() - make_interval(days => $1)
                 AND uri LIKE 'at://did:%/app.bsky.feed.post/%'
                 AND reply_parent IS NULL
                 AND (embeds IS NULL OR has_media = false)
               ORDER BY created_at DESC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(days as i32)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| format!("candidate query failed: {e}"))?
    } else {
        explicit
    };

    let mut stats = Stats { candidates: candidates.len(), ..Default::default() };
    info!(
        "[embed-backfill] {} run: {} candidate(s) ({})",
        if apply { "apply" } else { "dry-run" },
        stats.candidates,
        if std::env::var("EMBED_BACKFILL_URIS").is_ok() {
            "EMBED_BACKFILL_URIS".to_string()
        } else {
            format!("days={days} limit={limit} offset={offset}")
        }
    );

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("http client: {e}"))?;

    for (page, batch) in candidates.chunks(BATCH).enumerate() {
        let failed_before = stats.failed;
        let views = match fetch_views(&http, batch).await {
            Ok(v) => v,
            Err(e) => {
                // One unusable uri (an unknown repo on the AppView, a did:web
                // account it never indexed) fails the whole call, so retry the
                // batch one uri at a time rather than dropping 25 posts.
                warn!("[embed-backfill] batch {page}: {e} — retrying per-uri");
                let mut out = Vec::new();
                for uri in batch {
                    match fetch_views(&http, std::slice::from_ref(uri)).await {
                        Ok(mut v) => out.append(&mut v),
                        Err(e) => {
                            warn!("[embed-backfill] {uri}: {e}");
                            stats.failed += 1;
                        }
                    }
                }
                out
            }
        };
        let failed_here = stats.failed - failed_before;
        stats.unavailable += batch.len().saturating_sub(views.len() + failed_here);

        for view in views {
            stats.fetched += 1;
            if view.get("embed").map(|e| !e.is_null()).unwrap_or(false) {
                if apply {
                    match crate::api::feed::sources::upsert_network_item(pool, &view).await {
                        Ok(()) => stats.repaired += 1,
                        Err(e) => {
                            warn!("[embed-backfill] upsert failed for {}: {e}", view["uri"]);
                            stats.failed += 1;
                        }
                    }
                } else {
                    stats.repaired += 1;
                }
            } else {
                stats.no_embed += 1;
            }
        }
        info!(
            "[embed-backfill] batch {page} done: fetched={} repaired={} no_embed={} unavailable={} failed={}",
            stats.fetched, stats.repaired, stats.no_embed, stats.unavailable, stats.failed
        );
    }

    // Recovered embeds are only visible once the affected feed/seed snapshots
    // rebuild (2-minute staleness ladder), so nothing else is needed here.
    info!("[embed-backfill] finished: {stats:?}");
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::parse_uris;
    use crate::api::feed::normalize::parse_bluesky_embeds;
    use serde_json::json;

    /// The view the AppView returns for the owner's repro post: a quote plus
    /// two images. The repair path must yield both halves, and the stored
    /// shape must round-trip through the feed's DB parser.
    #[test]
    fn record_with_media_view_maps_to_quote_and_images() {
        let view = json!({
            "$type": "app.bsky.embed.recordWithMedia#view",
            "media": {
                "$type": "app.bsky.embed.images#view",
                "images": [
                    { "thumb": "https://cdn.bsky.app/img/feed_thumbnail/plain/did:plc:a/bafkrei1@jpeg",
                      "fullsize": "https://cdn.bsky.app/img/feed_fullsize/plain/did:plc:a/bafkrei1@jpeg",
                      "alt": "", "aspectRatio": { "width": 1280, "height": 719 } },
                    { "thumb": "https://cdn.bsky.app/img/feed_thumbnail/plain/did:plc:a/bafkrei2@jpeg",
                      "fullsize": "https://cdn.bsky.app/img/feed_fullsize/plain/did:plc:a/bafkrei2@jpeg",
                      "alt": "", "aspectRatio": { "width": 2316, "height": 3088 } }
                ]
            },
            "record": {
                "$type": "app.bsky.embed.record#view",
                "record": {
                    "uri": "at://did:plc:b/app.bsky.feed.post/3n2daoaunss2i",
                    "cid": "bafyreic",
                    "author": { "did": "did:plc:b", "handle": "danicusrexx.bsky.social" }
                }
            }
        });

        let stored = crate::api::feed::sources::view_embed_to_db_json(&view)
            .expect("recordWithMedia must produce db embeds");
        let entries = stored.as_array().expect("array shape");
        assert_eq!(entries.len(), 3, "quote + two images: {stored}");
        assert!(entries.iter().any(|e| e["type"] == "quote"
            && e["uri"] == "at://did:plc:b/app.bsky.feed.post/3n2daoaunss2i"));
        assert_eq!(entries.iter().filter(|e| e["type"] == "image").count(), 2);

        let (media, embeds) = parse_bluesky_embeds(&stored, "did:plc:a");
        assert_eq!(media.len(), 2, "both images reach FeedPost.media");
        assert!(media[0].url.contains("bafkrei1"));
        assert_eq!(embeds.len(), 1);
        assert_eq!(embeds[0].kind, "quote");
        assert_eq!(embeds[0].url, "at://did:plc:b/app.bsky.feed.post/3n2daoaunss2i");
    }

    #[test]
    fn parse_uris_trims_and_drops_non_at_uris() {
        assert_eq!(
            parse_uris(" at://did:plc:a/app.bsky.feed.post/1 , https://x, at://did:plc:b/app.bsky.feed.post/2,,"),
            vec![
                "at://did:plc:a/app.bsky.feed.post/1".to_string(),
                "at://did:plc:b/app.bsky.feed.post/2".to_string()
            ]
        );
        assert!(parse_uris("").is_empty());
    }

    /// A quote whose media half is a link card repairs the link too.
    #[test]
    fn record_with_media_view_maps_external_media() {
        let view = json!({
            "$type": "app.bsky.embed.recordWithMedia#view",
            "media": { "$type": "app.bsky.embed.external#view",
                       "external": { "uri": "https://example.com/x", "title": "T",
                                     "description": "D", "thumb": "https://cdn/t.jpg" } },
            "record": { "$type": "app.bsky.embed.record#view",
                        "record": { "uri": "at://did:plc:b/app.bsky.feed.post/q" } }
        });
        let stored = crate::api::feed::sources::view_embed_to_db_json(&view).unwrap();
        let entries = stored.as_array().unwrap();
        assert!(entries.iter().any(|e| e["type"] == "quote"));
        assert!(entries.iter().any(|e| e["type"] == "link" && e["url"] == "https://example.com/x"));
    }
}
