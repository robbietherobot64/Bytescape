// Serve-first feed snapshots.
//
// The seed branch of /api/feed/combined used to recompute the whole feed
// pipeline on every request (seconds warm, tens of seconds cold). This
// module stores each seed response as one JSON row and serves it back in
// the pipeline's place: a first paint never waits on the computation again.
//
// The expensive work runs off the request path. A stale or missing
// snapshot spawns a rebuild that calls `combined_feed` itself with
// `phase="rebuild"` — an internal marker that bypasses this serve-first
// layer — and the seed branch stores its own result on the way out.
//
// Staleness ladder at serve time:
//   snapshot present   -> serve it (any age); rebuild if older than 2 min.
//   snapshot missing   -> approximate from the global recent-post cache
//                         (muted/nsfw/tag filtered), rebuild in background.
//   no global cache    -> fall through to the inline computation (boot
//                         only; the global refresher fills within seconds).

use std::collections::HashSet;
use std::sync::LazyLock;

use axum::http::HeaderMap;
use parking_lot::Mutex;
use serde_json::Value;

use super::CombinedQuery;
use crate::db::users::NsfwPref;

/// Snapshots older than this get a background rebuild when served.
pub(crate) const STALE_SECS: i64 = 120;

// ---------------------------------------------------------------------------
// Key + storage
// ---------------------------------------------------------------------------

/// Key covers user + every query axis that changes seed content. Phase,
/// cursor and since are excluded — the seed is always the first page.
///
/// `viewer_nsfw_pref` is part of the key because the NSFW policy is applied
/// while building: a snapshot built for a `blur`/`show` viewer contains posts an
/// `exclude` viewer must never receive. Without it, flipping the setting left
/// the old snapshot serving NSFW items until the 2-minute staleness rebuild.
/// The `n=` axis is `exclude` -> 0, `blur`/`show` -> 1 (both keep marked items).
pub(crate) fn seed_key(
    user_id: Option<&uuid::Uuid>,
    q: &CombinedQuery,
    viewer_nsfw_pref: NsfwPref,
) -> String {
    format!(
        "seed:{}|s={}|i={}|a={}|c={}|t={}|n={}|x={}|p={}|w={}|f={}|e={}",
        user_id.map(|u| u.to_string()).unwrap_or_else(|| "-".to_string()),
        q.sort.as_deref().unwrap_or(""),
        q.ids.as_deref().unwrap_or(""),
        q.author.as_deref().unwrap_or(""),
        q.source_categories.as_deref().unwrap_or(""),
        q.types.as_deref().unwrap_or(""),
        viewer_nsfw_pref.snapshot_axis(),
        q.source_mix.as_deref().unwrap_or(""),
        q.platform.as_deref().unwrap_or(""),
        q.window.as_deref().unwrap_or(""),
        q.from.as_deref().unwrap_or(""),
        q.to.as_deref().unwrap_or(""),
    )
}

pub(crate) struct Snapshot {
    pub payload: Value,
    pub age_secs: i64,
}

pub(crate) async fn load(pool: &sqlx::PgPool, key: &str) -> Option<Snapshot> {
    let row: Option<(Value, f64)> = sqlx::query_as(
        "SELECT payload, EXTRACT(EPOCH FROM (now() - built_at))::float8 \
         FROM feed_snapshots WHERE key = $1",
    )
    .bind(key)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    row.map(|(payload, age)| Snapshot {
        payload,
        age_secs: age.max(0.0) as i64,
    })
}

pub(crate) async fn store(pool: &sqlx::PgPool, key: &str, payload: &Value) {
    if let Err(e) = sqlx::query(
        "INSERT INTO feed_snapshots (key, payload) VALUES ($1, $2) \
         ON CONFLICT (key) DO UPDATE SET payload = EXCLUDED.payload, built_at = now()",
    )
    .bind(key)
    .bind(payload)
    .execute(pool)
    .await
    {
        tracing::warn!("feed snapshot store failed for {}: {}", key, e);
    }
}

// ---------------------------------------------------------------------------
// Approximation removed.
//
// The global-recent approximation (`approximate`) and its write-only cache
// (`GLOBAL_RECENT`, `ensure_global`, `refresh_global`, `spawn_global_refresher`)
// were deleted: they served up to 30 unrelated network posts into the home feed
// whenever no personalized snapshot existed, leaking random content with no
// provenance. When a snapshot is missing the feed now falls through to the real
// personalized pipeline (see api/feed/mod.rs) and the frontend shows a loading
// state until it is ready.
// ---------------------------------------------------------------------------
// Background rebuild
// ---------------------------------------------------------------------------

/// One rebuild per snapshot key at a time.
static BUILDING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Rebuilds a snapshot by running the real seed computation off the
/// request path. `phase="rebuild"` makes the handler skip the serve-first
/// layer above (it would otherwise return the stale snapshot being
/// replaced). The seed branch stores its own result on the way out.
pub(crate) fn spawn_rebuild(
    state: crate::AppState,
    headers: HeaderMap,
    query: CombinedQuery,
    key: String,
) {
    {
        if !BUILDING.lock().insert(key.clone()) {
            return;
        }
    }
    tokio::spawn(async move {
        let mut q = query;
        q.phase = Some("rebuild".to_string());
        match super::combined_feed(
            axum::extract::State(state),
            headers,
            axum::extract::Query(q),
        )
        .await
        {
            Ok(_resp) => {
                tracing::info!("feed snapshot rebuilt for {}", key);
            }
            Err(e) => tracing::warn!("feed snapshot rebuild failed for {}: {}", key, e),
        }
        BUILDING.lock().remove(&key);
    });
}

/// Rebuild spawn for the non-combined feed endpoints (the sources and
/// groups views): the same one-build-per-key guard as [`spawn_rebuild`],
/// with the caller supplying the build-and-store future. The build future
/// MUST call the endpoint's raw builder (not the serve-first wrapper) and
/// store its own result.
pub(crate) fn spawn_rebuild_with<F, Fut>(key: String, build: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    {
        if !BUILDING.lock().insert(key.clone()) {
            return;
        }
    }
    tokio::spawn(async move {
        build().await;
        BUILDING.lock().remove(&key);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::feed::{filter, FeedResponse};

    /// A stored snapshot is served in the pipeline's place, so the serve path
    /// must re-apply the NSFW policy — a payload built (or built wrongly) with
    /// an NSFW item must not reach an opted-out viewer.
    #[test]
    fn served_snapshot_hides_nsfw_for_opted_out_viewer() {
        let payload = serde_json::json!({
            "items": [
                {
                    "id": "at://did:plc:x/app.bsky.feed.post/explicit",
                    "source": "bluesky",
                    "author": { "handle": "@explicit.example", "display_name": null, "avatar": null },
                    "content": "explicit but labeled",
                    "content_type": "short",
                    "media": [],
                    "embeds": [],
                    "likes_count": 0,
                    "shares_count": 0,
                    "replies_count": 0,
                    "created_at": "2026-11-15T12:00:00+00:00",
                    "url": null,
                    "thread_id": null,
                    "start_time": null,
                    "end_time": null,
                    "location": null,
                    "labels": ["porn"],
                    "nsfw": true
                },
                {
                    "id": "at://did:plc:x/app.bsky.feed.post/cat",
                    "source": "bluesky",
                    "author": { "handle": "@cat.example", "display_name": null, "avatar": null },
                    "content": "my cat is adorable",
                    "content_type": "short",
                    "media": [],
                    "embeds": [],
                    "likes_count": 0,
                    "shares_count": 0,
                    "replies_count": 0,
                    "created_at": "2026-11-15T11:00:00+00:00",
                    "url": null,
                    "thread_id": null,
                    "start_time": null,
                    "end_time": null,
                    "location": null,
                    "nsfw": false
                }
            ],
            "cursor": null,
            "has_more": false
        });

        let mut body: FeedResponse = serde_json::from_value(payload).expect("snapshot payload parses");

        let mut hidden = body.items.clone();
        filter::apply_nsfw_policy(&mut hidden, NsfwPref::Exclude);
        assert_eq!(hidden.len(), 1, "exclude viewer gets no NSFW item from a snapshot");
        assert_eq!(hidden[0].id, "at://did:plc:x/app.bsky.feed.post/cat");

        // Blur viewers keep it, flagged for the blur + reveal layer.
        let mut kept = body.items.clone();
        filter::apply_nsfw_policy(&mut kept, NsfwPref::Blur);
        assert_eq!(kept.len(), 2);
        assert!(kept[0].nsfw);

        // Show viewers get it unflagged.
        let mut shown = body.items.clone();
        filter::apply_nsfw_policy(&mut shown, NsfwPref::Show);
        assert_eq!(shown.len(), 2);
        assert!(!shown.iter().any(|p| p.nsfw), "show unmarks NSFW items");

        // Unlabeled content is untouched by any branch.
        body.items.retain(|p| p.id.ends_with("/cat"));
        filter::apply_nsfw_policy(&mut body.items, NsfwPref::Exclude);
        assert_eq!(body.items.len(), 1);
    }

    /// Flipping the preference must not keep serving the old snapshot.
    #[test]
    fn seed_key_separates_nsfw_preference() {
        let q = CombinedQuery {
            ids: None,
            cursor: None,
            phase: None,
            since: None,
            sort: Some("new".to_string()),
            author: None,
            source_categories: None,
            source_mix: None,
            types: None,
            platform: None,
            window: None,
            from: None,
            to: None,
        };
        let off = seed_key(None, &q, NsfwPref::Exclude);
        let on = seed_key(None, &q, NsfwPref::Blur);
        let shown = seed_key(None, &q, NsfwPref::Show);
        assert_ne!(off, on);
        assert_eq!(on, shown, "blur and show share the snapshot axis (both keep marked items)");
        assert!(off.contains("|n=0"), "{off}");
        assert!(on.contains("|n=1"), "{on}");
        assert!(shown.contains("|n=1"), "{shown}");

        // Source mix participates in the key: a "bluesky" viewer must not reuse
        // an unfiltered snapshot.
        let mut q2 = CombinedQuery {
            ids: None,
            cursor: None,
            phase: None,
            since: None,
            sort: None,
            author: None,
            source_categories: None,
            source_mix: Some("bluesky".to_string()),
            types: None,
            platform: None,
            window: None,
            from: None,
            to: None,
        };
        let mix_key = seed_key(None, &q2, NsfwPref::Exclude);
        assert_ne!(off, mix_key, "source_mix changes the snapshot key");
        assert!(mix_key.contains("|x=bluesky"), "{mix_key}");

        // The time-period filter is part of the key too: an unfiltered seed
        // must never be served to a windowed request (or vice versa), and the
        // custom range separates by both endpoints.
        let mut windowed = q.clone();
        windowed.window = Some("24h".to_string());
        let window_key = seed_key(None, &windowed, NsfwPref::Exclude);
        assert_ne!(off, window_key, "window changes the snapshot key");
        assert!(window_key.contains("|w=24h"), "{window_key}");

        let mut custom = q.clone();
        custom.window = Some("custom".to_string());
        custom.from = Some("2026-11-10".to_string());
        custom.to = Some("2026-11-12".to_string());
        let custom_key = seed_key(None, &custom, NsfwPref::Exclude);
        assert_ne!(off, custom_key);
        assert_ne!(window_key, custom_key);
        assert!(custom_key.contains("|w=custom|f=2026-11-10|e=2026-11-12"), "{custom_key}");
        let _ = &mut q2;
    }
}
