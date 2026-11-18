//! API — Feed routes.
//!
//! GET /api/feed                 — Aggregated feed from all linked sources
//! GET /api/feed/bluesky         — Bluesky home timeline
//! GET /api/feed/hashtag?tag=X   — Search hashtag across platforms
//! GET /api/feed/search?q=X      — Search across platforms
//! GET /api/feed/combined        — Combined feed (clean rewrite)
//! GET /api/feed/sources         — Sources-based feed
//! GET /api/feed/interests       — Interest-based feed
//! GET /api/discover             — Trending/discover content
//! GET /api/feed/comments        — Post comments
//! POST /api/feed/comments       — Create reply
//! POST /api/feed/reaction       — Post reaction
//! POST /api/feed/follow         — Follow user
//! POST /api/feed/unfollow       — Unfollow user
//! GET /api/feed/follows         — List follows
//! GET /api/feed/search-communities — Search communities

pub mod filter;
pub mod normalize;
pub mod paginate;
pub mod snapshots;
pub mod sort;
pub mod sources;

use std::collections::HashSet;
use std::time::Duration;

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::interest_sources;
use crate::db::users::NsfwPref;
use crate::{auth, db, error::AppError, oauth, AppState};

use filter::{deduplicate, FeedFilters};
use normalize::normalize_bluesky_api_post;
use paginate::Cursor;
use sort::SortMode;

// ---------------------------------------------------------------------------
// Unified post format
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedPost {
    pub id: String,
    pub source: String, // 'bluesky', 'mastodon', 'bytescape'
    pub author: AuthorInfo,
    pub content: String,
    pub content_type: String, // 'short', 'long', 'image', 'video', 'link'
    pub media: Vec<MediaAttachment>,
    pub embeds: Vec<EmbedInfo>,
    pub likes_count: u32,
    pub shares_count: u32,
    pub replies_count: u32,
    pub created_at: String,
    pub url: Option<String>,
    pub thread_id: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_name: Option<String>,
    /// AP group (community) this post belongs to — populated for Lemmy/PieFed
    /// group posts (design D1). `None` elsewhere. The group post card header
    /// renders from this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<GroupRef>,
    /// Group vote counts (Lemmy/PieFed posts) — the group post card's ratio
    /// (design D3). `upvotes`/`downvotes` are the fetched counts at ingest;
    /// `my_vote` is the viewer's standing federated vote (read back from
    /// `ap_activities`, never faked).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upvotes: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downvotes: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub my_vote: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reaction_counts: Option<std::collections::HashMap<String, u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_reaction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reposted_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boosted_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quoted_post_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_reshare: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_post: Option<Box<FeedPost>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_own: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_following: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_pending: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cross_posts: Vec<CrossPostBadge>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_sources: Vec<SourceMatch>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(default)]
    pub nsfw: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_external_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorInfo {
    pub handle: String,
    pub display_name: Option<String>,
    pub avatar: Option<String>,
}

/// The AP group (community) a post belongs to — the group post card header
/// renders from this (name + instance + canonical actor id). Group identity is
/// the AP actor id so a later AT layer can coexist (design D1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupRef {
    pub actor_id: String,
    pub name: String,
    pub title: Option<String>,
    pub instance: String,
    pub kind: String,
    pub nsfw: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaAttachment {
    pub url: String,
    pub thumbnail_url: Option<String>,
    pub mime_type: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub alt_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedInfo {
    pub kind: String, // 'youtube', 'soundcloud', 'bandcamp', 'link', 'image', 'video'
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub thumbnail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossPostBadge {
    pub platform: String,
    pub external_url: String,
    pub ok: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FeedResponse {
    pub items: Vec<FeedPost>,
    pub cursor: Option<String>,
    pub has_more: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    /// Platform mix for the request's feed scope: one count per
    /// [`filter::PLATFORM_BUCKETS`] key, computed independently of the
    /// `platform=` filter (so the dropdown shows the full mix even when a
    /// bucket is selected). First pages only; absent on cursor pages and on
    /// endpoints that do not build the interest feed. `default` keeps older
    /// stored snapshots deserializable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_counts: Option<std::collections::BTreeMap<String, i64>>,
}

// ---------------------------------------------------------------------------
// Source provenance (feed chips): which subscribed source produced each post
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMatch {
    pub label: String,
    pub kind: String, // 'feed' | 'hashtag' | 'following'
    pub rank: u8,     // specificity order: feed(0) → hashtag(1) → following(2)
}

/// Tag posts with the subscribed source that produced them (feed chips).
/// Boundary-checked `#tag` presence. The local fetch is substring-based
/// (ILIKE), so chip assignment must verify the token end — `#cats` must not
/// claim `#catsky` posts.
fn post_contains_tag(p: &FeedPost, tag: &str) -> bool {
    let lower = p.content.to_lowercase();
    let needle = format!("#{}", tag.to_lowercase());
    lower.match_indices(&needle).any(|(i, _)| {
        lower
            .as_bytes()
            .get(i + needle.len())
            .map_or(true, |c| !c.is_ascii_alphanumeric())
    })
}

/// Crate-visible for the group-scope provenance stamping (`sources.rs`), which
/// attributes each group post to the branch (follow/topic/tag) that matched.
pub(crate) fn post_contains_tag_pub(p: &FeedPost, tag: &str) -> bool {
    post_contains_tag(p, tag)
}

pub fn tag_sources(posts: &mut [FeedPost], kind: &str, label: &str) {
    let rank: u8 = match kind {
        "feed" => 0,
        "hashtag" => 1,
        _ => 2,
    };
    for p in posts.iter_mut() {
        if !p
            .matched_sources
            .iter()
            .any(|m| m.kind == kind && m.label == label)
        {
            p.matched_sources.push(SourceMatch {
                label: label.to_string(),
                kind: kind.to_string(),
                rank,
            });
        }
        p.matched_sources.sort_by_key(|m| m.rank);
    }
}

/// Provenance fill: every post answers "where did this come from". Unmatched
/// non-followed content is honestly marked Discover (it matched no
/// subscription). Followed authors carry no chip — their follow state is the
/// provenance, shown by the Following button (user direction 2026-10-11).
pub fn fill_provenance(posts: &mut [FeedPost]) {
    for p in posts.iter_mut() {
        if !p.matched_sources.is_empty() {
            continue;
        }
        if p.is_following == Some(true) {
            continue;
        }
        p.matched_sources.push(SourceMatch {
            label: "Discover".to_string(),
            kind: "discover".to_string(),
            rank: 3,
        });
    }
}

/// Initial-load fairness: no single subscribed source may dominate while other
/// sources have content. Discovery posts (no matched source) pass through;
/// deferred posts sort after the kept ones and reach later pagination.
/// Anti-starvation twin of apply_source_cap. After sorting, a recency flood
/// from other sources can squeeze a low-frequency subscribed source out of the
/// initial page entirely ("picked interests get ignored"). Selection gives
/// every subscribed source with content `floor_per_source` slots; the ORDER
/// stays purely sort-driven (2026-10-11: no source class is prioritized —
/// this is representation, not ranking). Following/discover are not sources.
pub fn truncate_with_source_floor(
    posts: Vec<FeedPost>,
    page: usize,
    floor_per_source: usize,
) -> Vec<FeedPost> {
    let mut per_key: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut picked = vec![false; posts.len()];
    let mut n = 0;
    let mut following_taken = 0;
    let mut ap_picked = 0usize;
    let mut fediverse_quota = 0usize;
    // Fediverse reserve: interest-matched ActivityPub posts share `hashtag:X`
    // buckets with Bluesky posts that win on raw recency (firehose seconds vs
    // AP hours), so the page cut must reserve their slots up front. Up to 1/3
    // of the page when that many matched, balanced across networks (a per-
    // network floor first so Mastodon can't starve Lemmy/Pixelfed).
    {
        let ap_indices: Vec<usize> = posts
            .iter()
            .enumerate()
            .filter(|(_, p)| p.matched_sources.iter().any(|m| m.kind == "fediverse"))
            .map(|(i, _)| i)
            .collect();
        if !ap_indices.is_empty() {
            let quota = ap_indices.len().min((page + 2) / 3);
            fediverse_quota = quota;
            let mut reserved = 0usize;
            // Round-robin networks so no single network can crowd the reserve.
            let mut nets: Vec<String> = ap_indices
                .iter()
                .map(|&i| posts[i].source.clone())
                .collect();
            nets.sort();
            nets.dedup();
            for round in 0..floor_per_source.max(1) {
                for net in &nets {
                    if n >= page || reserved >= quota {
                        break;
                    }
                    if let Some(&i) = ap_indices
                        .iter()
                        .find(|&&i| posts[i].source == *net && !picked[i])
                    {
                        picked[i] = true;
                        n += 1;
                        reserved += 1;
                    }
                }
                if reserved >= floor_per_source * nets.len() {
                    break;
                }
                let _ = round;
            }
            for &i in &ap_indices {
                if n >= page || reserved >= quota {
                    break;
                }
                if !picked[i] {
                    picked[i] = true;
                    n += 1;
                    reserved += 1;
                }
            }
            ap_picked = reserved;
        }
    }
    for (i, p) in posts.iter().enumerate() {
        if n >= page {
            break;
        }
        if picked[i] {
            continue;
        }
        // Fediverse posts are fully handled by the reserve above: letting them
        // also take per-key floors (via their shared hashtag chips) would
        // double-book them beyond the reserved quota.
        if p.matched_sources.iter().any(|m| m.kind == "fediverse") {
            continue;
        }
        // Following gets its OWN floor: followed posts almost always match a
        // subscribed hashtag too, and the chip ordering ranks hashtags first —
        // keying on the first chip starved following off the page entirely.
        let mut floored = false;
        if following_taken < floor_per_source
            && (p.is_following == Some(true)
                || p.matched_sources.iter().any(|m| m.kind == "following"))
        {
            following_taken += 1;
            floored = true;
        }
        if !floored {
            let Some(m) = p.matched_sources.first() else {
                continue;
            };
            if m.kind == "discover" {
                continue;
            }
            let key = format!("{}:{}", m.kind, m.label);
            let c = per_key.entry(key).or_insert(0);
            if *c < floor_per_source {
                *c += 1;
                floored = true;
            }
        }
        if floored {
            picked[i] = true;
            n += 1;
        }
    }
    for (i, _) in posts.iter().enumerate() {
        if n >= page {
            break;
        }
        if picked[i] {
            continue;
        }
        // The fediverse reserve is the AP budget for the whole page: the fill
        // pass must not pull extra AP posts (sorted head is often all-AP) past
        // the reserved quota, or the "at least this much AP" guarantee becomes
        // "this much and then everything is AP".
        if posts[i].matched_sources.iter().any(|m| m.kind == "fediverse")
            && ap_picked >= fediverse_quota
        {
            continue;
        }
        if posts[i].matched_sources.iter().any(|m| m.kind == "fediverse") {
            ap_picked += 1;
        }
        picked[i] = true;
        n += 1;
    }
    posts
        .into_iter()
        .zip(picked)
        .filter(|(_, keep)| *keep)
        .map(|(p, _)| p)
        .collect()
}

#[cfg(test)]
mod truncate_tests {
    use super::*;
    use crate::api::feed::SourceMatch;

    fn post(id: &str, source: &str, chips: &[(&str, &str)]) -> FeedPost {
        let matched: Vec<SourceMatch> = chips
            .iter()
            .map(|(kind, label)| SourceMatch {
                label: label.to_string(),
                kind: kind.to_string(),
                rank: 0,
            })
            .collect();
        serde_json::from_value(serde_json::json!({
            "id": id,
            "source": source,
            "author": { "handle": "@t", "display_name": null, "avatar": null },
            "content": "",
            "content_type": "short",
            "media": [],
            "embeds": [],
            "likes_count": 0,
            "shares_count": 0,
            "replies_count": 0,
            "created_at": "",
            "matched_sources": matched,
        }))
        .expect("test FeedPost")
    }

    /// Interest-matched AP posts share hashtag buckets with Bluesky posts that
    /// win on raw recency — without the reserve they were crowded off the page
    /// entirely (the "feed is just Bluesky again" bug).
    #[test]
    fn fediverse_matches_survive_the_page_cut() {
        let mut posts = vec![];
        // 20 Bluesky posts all matching the same hashtag — they sort first.
        for i in 0..20 {
            posts.push(post(
                &format!("at://{i}"),
                "bluesky",
                &[("hashtag", "cats")],
            ));
        }
        // 2 AP posts matching the same hashtag (same bucket, would lose).
        posts.push(post(
            "https://mastodon.test/1",
            "mastodon",
            &[("hashtag", "cats"), ("fediverse", "Fediverse")],
        ));
        posts.push(post(
            "https://lemmy.world/post/2",
            "lemmy",
            &[("hashtag", "cats"), ("fediverse", "Fediverse")],
        ));
        let kept = truncate_with_source_floor(posts, 10, 2);
        assert_eq!(kept.len(), 10, "page is full");
        let ap: Vec<&str> = kept
            .iter()
            .filter(|p| p.matched_sources.iter().any(|m| m.kind == "fediverse"))
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(ap.len(), 2, "both interest-matched AP posts kept: {ap:?}");
        // Network balance: Mastodon must not crowd out Lemmy.
        assert!(
            ap.contains(&"https://mastodon.test/1") && ap.contains(&"https://lemmy.world/post/2"),
            "one per network: {ap:?}"
        );
    }

    /// The fediverse reserve is bounded — it cannot swallow the page.
    #[test]
    fn fediverse_reserve_is_capped_at_a_third() {
        let mut posts = vec![];
        for i in 0..20 {
            posts.push(post(
                &format!("https://mastodon.test/{i}"),
                "mastodon",
                &[("hashtag", "cats"), ("fediverse", "Fediverse")],
            ));
        }
        for i in 0..20 {
            posts.push(post(
                &format!("at://{i}"),
                "bluesky",
                &[("hashtag", "cats")],
            ));
        }
        let kept = truncate_with_source_floor(posts, 9, 2);
        assert_eq!(kept.len(), 9);
        let ap = kept
            .iter()
            .filter(|p| p.matched_sources.iter().any(|m| m.kind == "fediverse"))
            .count();
        assert_eq!(ap, 3, "page 9 → quota (9+2)/3 = 3 reserved for AP");
    }
}

// ---------------------------------------------------------------------------
// Re-export public normalizers for use by other modules (profile.rs, rss.rs)
// ---------------------------------------------------------------------------
pub use normalize::normalize_bluesky_api_post as normalize_bluesky_post;

// ---------------------------------------------------------------------------
// CombinedQuery
// ---------------------------------------------------------------------------
#[derive(Deserialize, Clone)]
pub struct CombinedQuery {
    pub ids: Option<String>,
    pub cursor: Option<String>,
    pub phase: Option<String>,
    pub since: Option<String>,
    pub sort: Option<String>,
    pub author: Option<String>,
    pub source_categories: Option<String>,
    /// Network mix filter: "all" (absent/default) | "bluesky" | "mastodon".
    /// "bluesky" keeps only AT-origin items; "mastodon" keeps only AP/fediverse
    /// origin items. Distinct from `source_categories` (the category axis).
    pub source_mix: Option<String>,
    /// Content-type filter (comma-separated buckets, e.g. "video,market").
    pub types: Option<String>,
    /// Platform bucket filter: absent/"all" | "bytescape" | "bluesky" |
    /// "mastodon" | "lemmy" | "pixelfed". Maps `FeedPost.source` through
    /// [`filter::platform_bucket`]; unknown values mean no filter.
    pub platform: Option<String>,
    /// Time-period filter: absent/"all" | "12h" | "24h" | "48h" | "1w" |
    /// "1mo" | "custom". Filters posts before sorting; `custom` uses
    /// `from`/`to` (YYYY-MM-DD). See [`filter::parse_window`].
    pub window: Option<String>,
    /// Custom-range start date (YYYY-MM-DD, inclusive from 00:00 UTC).
    pub from: Option<String>,
    /// Custom-range end date (YYYY-MM-DD, inclusive through 23:59 UTC).
    pub to: Option<String>,
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/feed", get(aggregated_feed))
        .route("/api/feed/bluesky", get(bluesky_feed))
        .route("/api/feed/hashtag", get(hashtag_feed))
        .route("/api/feed/search", get(search_feed))
        .route("/api/feed/interests", get(interest_feed))
        .route("/api/feed/combined", get(combined_feed))
        .route("/api/feed/sources", get(sources_feed))
        // Group-variant feed (Groups view tab scopes) — one parameterized
        // query over group-linked posts, mirroring the events feed's scopes.
        .route("/api/feed/groups", get(group_scope_feed))
        .route("/api/discover", get(discover_feed))
        .route(
            "/api/feed/comments",
            get(post_comments).post(create_reply),
        )
        .route("/api/feed/reaction", post(post_reaction))
        .route("/api/feed/follow", post(feed_follow))
        .route("/api/feed/unfollow", post(feed_unfollow))
        .route("/api/feed/follows", get(list_follows))
        // Lemmy search-communities removed — Sprint 1 (Lemmy deletion)
        .route("/api/bluesky/post", get(get_bluesky_post))
}

// ---------------------------------------------------------------------------
// Auth helper
// ---------------------------------------------------------------------------
/// Viewer's three-state NSFW preference from users.nsfw_pref (default
/// `exclude` = hide NSFW; unauthenticated viewers are always `exclude`).
async fn fetch_nsfw_pref(db: &sqlx::PgPool, headers: &HeaderMap, secret: &str) -> NsfwPref {
    match extract_user_id(headers, secret).ok() {
        Some(uid) => crate::db::users::UserRow::find_by_id(db, uid)
            .await
            .ok()
            .flatten()
            .map(|u| NsfwPref::from_db(&u.nsfw_pref))
            .unwrap_or(NsfwPref::Exclude),
        None => NsfwPref::Exclude,
    }
}

pub(crate) fn extract_user_id(headers: &HeaderMap, secret: &str) -> Result<Uuid, AppError> {
    let auth_val = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Auth("Missing or invalid Authorization header".into()))?;

    let claims = auth::validate_token(auth_val, secret)?;
    Uuid::parse_str(&claims.sub).map_err(|_| AppError::Auth("Invalid user ID in token".into()))
}

/// Optional viewer: NO Authorization header → anonymous (`Ok(None)`); a
/// PRESENT but invalid/expired token → `Err(401)`. Never silently
/// reclassify a broken session as anonymous — the old
/// `extract_user_id(...).ok()` produced a 200/401 flip-flop (the same
/// expired token got an anonymous 200 here and a 401 there) which kept the
/// expired-token UI "loading" and fed silent refetch loops.
pub(crate) fn optional_viewer(
    headers: &HeaderMap,
    secret: &str,
) -> Result<Option<Uuid>, AppError> {
    if headers
        .get(axum::http::header::AUTHORIZATION)
        .is_none()
    {
        return Ok(None);
    }
    extract_user_id(headers, secret).map(Some)
}

// ---------------------------------------------------------------------------
// Muted accounts helper
// ---------------------------------------------------------------------------
async fn get_muted_dids(db: &sqlx::PgPool, user_id: Uuid) -> Vec<String> {
    sqlx::query_scalar("SELECT muted_did FROM muted_accounts WHERE user_id = $1 AND muted_did IS NOT NULL")
        .bind(user_id)
        .fetch_all(db)
        .await
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Reaction enrichment helper
// ---------------------------------------------------------------------------
async fn enrich_with_reactions(
    db: &crate::db::DbPool,
    posts: &mut [FeedPost],
    user_id: Uuid,
) {
    if posts.is_empty() {
        return;
    }
    let post_ids: Vec<String> = posts.iter().map(|p| p.id.clone()).collect();

    let counts = match crate::db::reactions::ReactionRow::counts_for_posts(db, &post_ids).await {
        Ok(c) => c,
        Err(_) => return,
    };

    let user_reactions: std::collections::HashMap<String, String> =
        crate::db::reactions::ReactionRow::user_reactions_for_posts(db, &post_ids, user_id)
            .await
            .unwrap_or_default();

    for post in posts.iter_mut() {
        // Start from the local reaction counts, then fold the group vote
        // counts in as 👍/👎 chips (owner model: votes ARE reactions — the
        // federated upvote/downvote counts render through the standard
        // reaction UI and combine honestly with local 👍/👎 reacts).
        let mut rc: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        if let Some(post_counts) = counts.get(&post.id) {
            for (emoji, cnt) in post_counts {
                rc.insert(emoji.clone(), *cnt as u32);
            }
        }
        if let Some(up) = post.upvotes.filter(|u| *u > 0) {
            *rc.entry("👍".to_string()).or_default() += up.max(0) as u32;
        }
        if let Some(down) = post.downvotes.filter(|d| *d > 0) {
            *rc.entry("👎".to_string()).or_default() += down.max(0) as u32;
        }
        if !rc.is_empty() {
            post.reaction_counts = Some(rc);
        }
        if let Some(emoji) = user_reactions.get(&post.id) {
            post.user_reaction = Some(emoji.clone());
        }
    }
}

// ---------------------------------------------------------------------------
// Follow status enrichment
// ---------------------------------------------------------------------------
/// Refresh a linked account's OAuth token when expired or expiring. Access
/// tokens are short-lived — without this, every tokened call fails silently
/// and personal-tier content disappears (the "only discovery shows" bug).
/// Returns the current-or-fresh token.
pub(crate) async fn refresh_if_expired(
    db: &crate::db::DbPool,
    http: &reqwest::Client,
    acct: &crate::db::accounts::LinkedAccountRow,
) -> Option<String> {
    let expiring = acct
        .token_expires_at
        .map(|t| t < chrono::Utc::now() + chrono::Duration::seconds(60))
        .unwrap_or(true);
    if !expiring {
        return Some(acct.access_token.clone());
    }
    match oauth::bluesky::refresh_session(http, db, acct).await {
        Ok(tok) => Some(tok),
        Err(e) => {
            tracing::error!("[FEED-DBG] token refresh failed for {}: {}", acct.external_username, e);
            None
        }
    }
}

async fn enrich_with_follow_status(
    http: &reqwest::Client,
    db: &crate::db::DbPool,
    posts: &mut [FeedPost],
    user_id: Uuid,
    domain: &str,
) {
    if posts.is_empty() {
        return;
    }

    let accounts = match crate::db::accounts::LinkedAccountRow::find_by_user(db, user_id).await {
        Ok(a) => a,
        Err(_) => return,
    };

    let mut bluesky_posts: Vec<(usize, String)> = Vec::new();
    let mut bytescape_posts: Vec<(usize, String)> = Vec::new();
    let mut ap_posts: Vec<usize> = Vec::new();

    for (i, post) in posts.iter().enumerate() {
        if post.source == "bluesky" {
            bluesky_posts.push((i, post.author.handle.clone()));
        } else if post.source == "bytescape" {
            bytescape_posts.push((i, post.author.handle.clone()));
        } else if post
            .author_external_id
            .as_deref()
            .is_some_and(|u| u.starts_with("http"))
        {
            // Fediverse (AP) posts — matched below by exact source URL,
            // identical to `hydrate_comment_follow_state`.
            ap_posts.push(i);
        }
    }

    // --- Bluesky: get viewer's follows list and match ---
    if let Some(bluesky_acct) = accounts.iter().find(|a| a.provider == "bluesky") {
        let pds_url = crate::oauth::bluesky::get_pds_url(&bluesky_acct.metadata);
        let blueacct_dpop = bluesky_acct.dpop_private_key.clone();

        if let Ok(followed_dids) = oauth::bluesky::get_viewer_follows(
            http,
            &bluesky_acct.access_token,
            &bluesky_acct.external_id,
            blueacct_dpop.as_deref(),
        )
        .await
        {
            for (idx, handle) in &bluesky_posts {
                let raw_handle = handle.trim_start_matches('@');
                if let Ok(author_did) = oauth::bluesky::resolve_handle(http, raw_handle).await {
                    posts[*idx].is_following = Some(followed_dids.contains(&author_did));
                    posts[*idx].follow_pending = Some(false);
                }
            }
        }
    }

    // --- Bytescape local: check follows table ---
    for (idx, handle) in &bytescape_posts {
        let username = handle
            .trim_start_matches('@')
            .split('@')
            .next()
            .unwrap_or(handle);
        let actor_url = format!(
            "https://{}/@{}",
            domain,
            username
        );
        if let Ok(Some(_)) =
            crate::db::follows::FollowRow::find_by_follower_and_actor(db, user_id, &actor_url)
                .await
        {
            posts[*idx].is_following = Some(true);
            posts[*idx].follow_pending = Some(false);
        }
    }

    // --- Fediverse (AP): one batch over `follows`, exact source-URL match —
    // identical resolution to `hydrate_comment_follow_state`, so a post
    // author and a comment author by the same actor resolve the same way.
    // The shared viewer_follow_state_map is also the merged graph rows'
    // follow-state source — one matcher, never a fork.
    if !ap_posts.is_empty() {
        let followed = crate::api::follow::viewer_follow_state_map(db, user_id).await;
        for idx in ap_posts {
            if let Some(url) = posts[idx].author_external_id.as_deref() {
                if let Some(pending) = followed.get(url) {
                    posts[idx].is_following = Some(true);
                    posts[idx].follow_pending = Some(*pending);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Combined Feed Handler (clean rewrite)
// ---------------------------------------------------------------------------

/// Fire-and-forget count refresh for served AP posts (owner bug: Mastodon likes
/// showed 0 while the source had N). Refreshes `like_count` + vote counts from
/// the home instance so a reload shows the real number ("refresh on view,
/// never a stale 0"). Bounded; the response is already built, so it never
/// blocks the render.
fn spawn_count_refresh(db: crate::db::DbPool, items: &[FeedPost]) {
    let uris: Vec<String> = items
        .iter()
        .filter(|p| p.id.starts_with("https://"))
        .map(|p| p.id.clone())
        .take(25)
        .collect();
    if uris.is_empty() {
        return;
    }
    tokio::spawn(async move {
        crate::ap::public_feed::refresh_counts_for_posts(&db, &uris).await;
    });
}

/// GET /api/feed/combined — The main combined feed endpoint.
///
/// Clean data flow:
/// 1. Parse params (sort, cursor, phase)
/// 2. Load user's follows, interests, muted accounts
/// 3. Fetch sources (parallel where possible)
/// 4. Normalize (ONE path per source)
/// 5. Filter (uniform pipeline)
/// 6. Deduplicate (by URI, then by author+content)
/// 7. Sort (by sort mode)
/// 8. Paginate (cursor-based, 30 per page)
/// 9. Response
async fn combined_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CombinedQuery>,
) -> Result<impl IntoResponse, AppError> {
    let sort = SortMode::from_query(query.sort.as_deref());
    tracing::info!("[FEED] combined_feed sort={:?} phase={:?} author={:?}", sort, query.phase, query.author);
    let cursor = Cursor::decode(query.cursor.as_deref())?;

    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(8))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    // Load user context (interests, muted accounts, followed DIDs)
    let user_id = optional_viewer(&headers, &state.config.jwt_secret)?;
    let viewer_nsfw_pref = fetch_nsfw_pref(&state.db, &headers, &state.config.jwt_secret).await;

    // Source mix: the request param wins; when omitted, fall back to the saved
    // preference (users.feed_prefs.source_mix). Normalize onto the query so the
    // snapshot key and the pipeline see the same effective value.
    let mut query = query;
    if query.source_mix.is_none() {
        if let Some(uid) = user_id {
            if let Ok(Some(u)) = crate::db::users::UserRow::find_by_id(&state.db, uid).await {
                let saved = u
                    .feed_prefs
                    .as_ref()
                    .and_then(|v| v.get("source_mix"))
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string());
                query.source_mix = saved;
            }
        }
    }

    // ── Serve-first snapshots ─────────────────────────────────────────────
    // The seed never waits on the pipeline: a stored response (or a cheap
    // approximation of the feed) is returned immediately and the real
    // computation runs in the background. See feed/snapshots.rs.
    let snap_key = snapshots::seed_key(user_id.as_ref(), &query, viewer_nsfw_pref);
    // First page only: cursor requests are pagination and run the live path.
    if query.phase.as_deref() != Some("full")
        && query.phase.as_deref() != Some("rebuild")
        && query.cursor.is_none()
    {
        if let Some(snap) = snapshots::load(&state.db, &snap_key).await {
            if let Ok(mut resp_body) = serde_json::from_value::<FeedResponse>(snap.payload) {
                if snap.age_secs > snapshots::STALE_SECS {
                    snapshots::spawn_rebuild(
                        state.clone(),
                        headers.clone(),
                        query.clone(),
                        snap_key.clone(),
                    );
                }
                // The snapshot is served in place of the pipeline, so the
                // policy has to run here too: a stored payload is not a
                // guarantee. Items keep the labels/nsfw they were built with,
                // and this is the last gate before an opted-out viewer sees
                // them (feed spec OQ-8: hide entirely unless opted in).
                filter::apply_nsfw_policy(&mut resp_body.items, viewer_nsfw_pref);
                filter::apply_source_mix(&mut resp_body.items, query.source_mix.as_deref());
                filter::apply_platform_filter(&mut resp_body.items, query.platform.as_deref());
                filter::apply_time_window(
                    &mut resp_body.items,
                    filter::parse_window(
                        query.window.as_deref(),
                        query.from.as_deref(),
                        query.to.as_deref(),
                        chrono::Utc::now(),
                    )
                    .as_ref(),
                );
                sources::fill_missing_profiles(&state.db, &mut resp_body.items).await;
                let mut resp_headers = HeaderMap::new();
                resp_headers.insert("Cache-Control", "no-store".parse().unwrap());
                spawn_count_refresh(state.db.clone(), &resp_body.items);
                return Ok((resp_headers, Json(resp_body)));
            }
            // Corrupt row: fall through — the inline computation re-stores.
        }
        // No personalized snapshot yet: fall through to the inline pipeline
        // below, which computes the REAL personalized feed (follows +
        // subscribed feeds + interest hashtags) and stores it. We deliberately
        // do NOT serve a global-recent approximation here: that leaked random
        // posts from unrelated people into the home feed with no provenance.
        // The frontend shows a loading state until the real feed is ready.
    }

    let mut interest_ids: Vec<String> = query
        .ids
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let viewer = if let Some(uid) = user_id {
        crate::db::users::UserRow::find_by_id(&state.db, uid)
            .await
            .ok()
            .flatten()
    } else {
        None
    };
    // If no interest IDs passed, load from user profile
    if interest_ids.is_empty() {
        if let Some(user) = viewer.as_ref() {
            interest_ids = user.interests.clone();
        }
    }

    let sources = if !interest_ids.is_empty() {
        interest_sources::get_all_sources_for_interests(&interest_ids)
    } else {
        Vec::new()
    };

    // Source-scoped views (chips / left menu): ids of the form kind:key ask
    // for exactly that source — no other sources, no discovery fallback.
    let source_keys: Vec<String> = interest_ids
        .iter()
        .filter(|s| {
            s.starts_with("hashtag:")
                || s.starts_with("feed:")
                || s.starts_with("follows:")
                || s.starts_with("author:")
        })
        .cloned()
        .collect();
    let scoped = !source_keys.is_empty();
    let interest_tags: Vec<String> = if scoped {
        source_keys
            .iter()
            .filter_map(|k| k.strip_prefix("hashtag:"))
            .map(|t| t.to_lowercase())
            .collect()
    } else {
        filter::get_interest_tags(&state.db, &headers, &state.config.jwt_secret).await
    };
    let scope_follows = !scoped || source_keys.iter().any(|k| k.starts_with("follows:"));
    let scope_feeds: Vec<String> = source_keys
        .iter()
        .filter_map(|k| k.strip_prefix("feed:").map(|f| f.to_string()))
        .collect();

    // Platform mix for the Platforms filter: counts over the same interest
    // scope the feed draws from, independent of the `platform=` selection so
    // the dropdown still shows the whole mix while a bucket is active. First
    // pages only — cursor pages would pay for an identical aggregate.
    let platform_counts = if query.cursor.is_none() {
        Some(
            sources::fetch_platform_counts(
                &state.db,
                &interest_tags,
                query.author.as_deref(),
                viewer_nsfw_pref,
            )
            .await,
        )
    } else {
        None
    };

    // Time-period filter: parsed once per request and applied before the
    // sort/truncation stage in both phases (same ordering contract as the
    // platform filter). Applies to both sort modes.
    let time_window = filter::parse_window(
        query.window.as_deref(),
        query.from.as_deref(),
        query.to.as_deref(),
        chrono::Utc::now(),
    );

    // A platform filter also gets a bucket-targeted appview pool: the regular
    // tiers fetch a mixed recency window, which can leave a sparse bucket
    // (Lemmy/Pixelfed) empty even though counts show posts. Merged pre-dedup
    // and then the post-fetch platform retain keeps only that bucket.
    let platform_pool = match filter::parse_platform(query.platform.as_deref()) {
        Some(bucket) if query.cursor.is_none() => Some(
            sources::fetch_appview_by_platform(
                &state.db,
                &interest_tags,
                bucket,
                40,
                sort,
                viewer_nsfw_pref,
            )
            .await,
        ),
        _ => None,
    };

    // Hoisted before the tier closures: `async move` futures would each try
    // to move `query.types` out of the shared query value.
    let type_filter = filter::parse_types(query.types.as_deref());
    let type_boost = filter::type_boost(query.types.as_deref());


    // Parse source_categories (comma-separated, e.g. "following,hashtags")
    // When absent or empty, all categories are enabled (default)
    let source_categories: Option<HashSet<String>> = query.source_categories.as_ref().map(|s| {
        s.split(',').map(|c| c.trim().to_lowercase()).filter(|c| !c.is_empty()).collect()
    });
    let cat_allowed = |cat: &str| -> bool {
        source_categories.as_ref().map_or(true, |cats| cats.contains(cat))
    };

    // Load muted DIDs
    let muted_dids = match user_id {
        Some(u) => get_muted_dids(&state.db, u).await,
        None => vec![],
    };
    let muted_did_set: HashSet<String> = muted_dids.iter().cloned().collect();

    let ft_start = std::time::Instant::now();
    // Load followed DIDs (for seed phase)
    let (followed_dids, _did_to_handle) = if let Some(uid) = user_id {
        let handles = sources::get_followed_handles(&state.db, uid, &state.config.domain).await;
        let mut resolved = sources::resolve_followed_dids(&state.db, &handles).await;
        // Filter muted DIDs from followed list
        resolved.0 = resolved
            .0
            .into_iter()
            .filter(|d| !muted_did_set.contains(d))
            .collect();
        // Surface followed fediverse (ActivityPub) authors: their ingested rows
        // carry the AP actor URL in `author_did`, so include those URLs as match
        // keys alongside the AT DIDs.
        let ap_actors = sources::get_followed_ap_actors(&state.db, uid).await;
        resolved
            .0
            .extend(ap_actors.into_iter().filter(|a| !muted_did_set.contains(a)));
        resolved
    } else {
        (Vec::new(), std::collections::HashMap::new())
    };

    // =========================================================================
    // SEED PHASE (fast, ~30 posts)
    // =========================================================================
    if query.phase.as_deref() != Some("full") {
        let mut posts: Vec<FeedPost> = Vec::new();

        // Linked accounts with tokens refreshed up-front — one refresh point for
    // every tokened call in this handler.
    let mut linked_accounts: Vec<crate::db::accounts::LinkedAccountRow> = if let Some(uid) = user_id {
        crate::db::accounts::LinkedAccountRow::find_by_user(&state.db, uid)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    for a in linked_accounts.iter_mut() {
        if let Some(tok) = refresh_if_expired(&state.db, &http, a).await {
            a.access_token = tok;
        }
    }

    // The viewer's real follow graph — fetched ONCE, deterministic marks.
    // (Per-author enrichment failed silently and left buttons/chips wrong.)
    let viewer_follows: std::collections::HashSet<String> =
        match linked_accounts.iter().find(|a| a.provider == "bluesky") {
            Some(acct) => oauth::bluesky::get_viewer_follows(
                &http,
                &acct.access_token,
                &acct.external_id,
                acct.dpop_private_key.as_deref(),
            )
            .await
            .map_err(|e| {
                tracing::error!("[FEED-DBG] get_viewer_follows failed: {}", e);
            })
            .unwrap_or_default(),
            None => Default::default(),
        };

    let follow_handles: std::collections::HashSet<String> = if let Some(uid) = user_id {
        crate::db::follows::FollowRow::list_by_user(&state.db, uid)
            .await
            .unwrap_or_default()
            .iter()
            .map(|f| f.followed_actor.rsplit('/').next().unwrap_or("").to_lowercase())
            .filter(|h: &String| !h.is_empty())
            .collect()
    } else {
        Default::default()
    };
    tracing::debug!(
        "[FEED-DBG] follow context: {} viewer_follows, {} local follow handles",
        viewer_follows.len(),
        follow_handles.len()
    );

        tracing::debug!("[FEED-T] setup+dids {:?}", ft_start.elapsed());
        let ft_personal = std::time::Instant::now();
        // ── The personal tier runs for EVERY sort: people you follow and the
        // feed generators you subscribe to are the platform's whole point.
        if let Some(uid) = user_id {
            if scope_follows && cat_allowed("following") {
                for acct in linked_accounts.iter().filter(|a| a.provider == "bluesky").take(3) {
                    let fetched = fetch_bluesky_timeline(&http, acct).await.unwrap_or_default();
                    let mut batch: Vec<FeedPost> = fetched.into_iter().take(30 * type_boost as usize).collect();
                    tag_sources(&mut batch, "following", "Following");
                    posts.extend(batch);
                }
            }
            if cat_allowed("feeds") && (!scoped || !scope_feeds.is_empty()) {
                if let Ok(subs) =
                    crate::db::subscriptions::SubscriptionRow::find_by_user(&state.db, uid).await
                {
                    // Parallel: a serial loop over 15 feeds made the seed
                    // request crawl (the "only follows show" symptom).
                    let feed_futs: Vec<_> = subs
                        .iter()
                        .filter(|s| s.category == "feeds" && s.enabled && s.source_id.starts_with("at://"))
                        .filter(|s| !scoped || scope_feeds.iter().any(|f| f == &s.source_id))
                        .take(12)
                        .map(|s| {
                            let http = &http;
                            let db = &state.db;
                            async move {
                                let fut = async move {
                                    let mut batch =
                                        sources::fetch_feed_posts(db, http, &s.source_id, 10 * type_boost).await;
                                    tag_sources(&mut batch, "feed", &s.label);
                                    batch
                                };
                                // A single slow topic-feed query must not hold
                                // the whole batch (it did: 60s+ walks).
                                tokio::time::timeout(std::time::Duration::from_secs(15), fut)
                                    .await
                                    .unwrap_or_default()
                            }
                        })
                        .collect();
                    for batch in futures::future::join_all(feed_futs).await {
                        posts.extend(batch);
                    }
                }
            }
        }

        tracing::debug!("[FEED-T] personal {:?}", ft_personal.elapsed());
        let ft_pool = std::time::Instant::now();
        let ft_tail = std::time::Instant::now();
        if !scoped && query.author.is_none() && cat_allowed("rss") {
            if let Some(user) = viewer.as_ref() {
                let mut imported = crate::api::profile::identity_appview_posts(
                    &state.db,
                    user.id,
                    &user.username,
                )
                .await?;
                for post in &mut imported {
                    post.is_own = Some(true);
                }
                tag_sources(&mut imported, "import", "Imported history");
                posts.extend(imported);
            }
        }
        // One integrated pool: index follows + per-tag hashtag content (with
        // live fallback) — sort mode ALONE decides the order (user direction
        // 2026-10-11: simple, unbiased; New = recency, Popular = engagement).
        {
                let (mut appview_follows, appview_hashtags) = tokio::join!(
                    // Followed accounts' posts from AppView DB
                    async {
                        if cat_allowed("following") && !followed_dids.is_empty() {
                            // Display identities are filled at the handler
                            // boundary (sources::fill_missing_profiles).
                            sources::fetch_appview_follows(
                                &state.db,
                                &followed_dids,
                                sort,
                                &cursor,
                                100 * type_boost,
                            ).await.unwrap_or_default()
                        } else {
                            Vec::new()
                        }
                    },
                    // Local index hashtag match — ONE batched regex query
                    // (trigram-indexed, fast). The live search tier above is
                    // scope-gated (403 on tokens without searchPosts) and must
                    // not be the only hashtag source. The old shape here was
                    // 26 sequential per-tag scans; this is one query.
                    async {
                        let mut tagged: Vec<FeedPost> = Vec::new();
                        if cat_allowed("hashtags") && !interest_tags.is_empty() {
                            let mut batch =
                                sources::fetch_appview_hashtags(&state.db, &interest_tags, 30 * type_boost, sort)
                                    .await;
                            // Stamp each post ONLY with tags it actually
                            // contains — this is a combined ILIKE batch, so
                            // blanket stamping chipped every post with every
                            // subscribed tag (the "#bloomscrolling on random
                            // posts" bug). Also drops substring false
                            // positives (#catsky under a #cats match).
                            for p in batch.iter_mut() {
                                for tag in &interest_tags {
                                    if post_contains_tag(p, tag) {
                                        tag_sources(std::slice::from_mut(p), "hashtag", tag);
                                    }
                                }
                            }
                            batch.retain(|p| !p.matched_sources.is_empty());
                            tagged = batch;
                        }
                        tagged
                    }
                );
                tag_sources(&mut appview_follows, "following", "Following");
                posts.extend(appview_follows);
                posts.extend(appview_hashtags);

                // Fediverse (ActivityPub) tier: interest-matched AP content
                // (chips stamped at the fetch — exactly the interests each post
                // matched), labeled with the Fediverse source like the other
                // tiers (user direction 2026-11-13).
                if cat_allowed("fediverse") && !interest_tags.is_empty() {
                    let mut ap =
                        sources::fetch_appview_ap(&state.db, &interest_tags, 15 * type_boost, sort)
                            .await;
                    tag_sources(&mut ap, "fediverse", "Fediverse");
                    posts.extend(ap);
                }

                tracing::debug!("[FEED-T] pool+feeds+tags {:?}", ft_pool.elapsed());
                // Following + interest subscriptions + the Fediverse (AP) tier
                // the user asked for. Still no *unrelated* generic "discover"/
                // "recent" padding — only the intentionally-included fediverse
                // content supplements the personal tiers.

        }

        // Apply uniform filters
        let filters = FeedFilters::from_user(&state.db, &headers, &state.config.jwt_secret).await?;
        posts.retain(|p| filters.should_keep(p));

        // Deduplicate
        if let Some(extra) = &platform_pool {
            posts.extend(extra.iter().cloned());
        }
        posts = deduplicate(posts);

        // Sort
        filter::filter_by_types(&mut posts, &type_filter);
        // Counts matter pre-sort only for Popular's ranking; recency sorts
        // skip this network pass entirely.
        if sort == SortMode::Popular {
            sources::refresh_engagement_counts(&http, &mut posts, sort, 75).await;
        }
        sort.sort_posts(&mut posts);

        // Filter by author if specified — supports multiple comma-separated authors
        if let Some(ref author_filter) = query.author {
            let author_handles: Vec<String> = author_filter
                .split(',')
                .map(|a| a.trim().trim_start_matches('@').to_lowercase())
                .filter(|a| !a.is_empty())
                .collect();
            tracing::info!("[FEED] seed author filter: {} handles -> {:?}", author_handles.len(), author_handles);

            let mut all_author_posts: Vec<FeedPost> = Vec::new();
            let mut seen_uris: HashSet<String> = HashSet::new();

            for raw_handle in &author_handles {
                // Extract handle from URL if it looks like a URL
                let author_clean = if raw_handle.contains("bsky.app/profile/") {
                    raw_handle.split("bsky.app/profile/").last().unwrap_or(raw_handle).to_string()
                } else if raw_handle.contains("://") {
                    // Other URL pattern — try to get the last path segment
                    raw_handle.split('/').last().unwrap_or(raw_handle).to_string()
                } else {
                    raw_handle.clone()
                };
                tracing::info!("[FEED] seed author: {} -> cleaned: {}", raw_handle, author_clean);

                // Query appview_posts directly by author_handle or author_did (prefix match)
                let mut author_posts: Vec<FeedPost> = sqlx::query_as::<
                    _,
                    (String, String, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<chrono::DateTime<chrono::Utc>>, Option<serde_json::Value>, Option<serde_json::Value>, Option<i32>, Option<i32>, Option<i32>),
                >(
                    r#"SELECT uri, cid, author_did, author_handle, author_avatar, author_display_name, text, created_at, embeds, labels, like_count, repost_count, reply_count
                       FROM appview_posts
                       WHERE (author_handle ILIKE $1 || '%' OR author_did ILIKE $1 || '%')
                                                  AND reply_parent IS NULL
                         AND created_at <= now()
                       ORDER BY created_at DESC
                       LIMIT 50"#,
                )
                .bind(&author_clean)
                .fetch_all(&state.db)
                .await
                .unwrap_or_default()
                .into_iter()
                .filter_map(|(uri, cid, did, author_handle, avatar, display_name, text, created, embeds, labels, likes, reposts, replies)| {
                    normalize::normalize_appview_post(
                        uri, cid, did, author_handle, avatar, display_name, text, created, embeds,
                        normalize::labels_from_json(&labels),
                        likes.unwrap_or(0), reposts.unwrap_or(0), replies.unwrap_or(0),
                    )
                })
                .collect();

                tracing::info!("[FEED] seed author DB returned {} posts for {}", author_posts.len(), author_clean);

                // If no posts found in DB, try Bluesky public API
                if author_posts.is_empty() {
                    let bsky_handle = author_clean.trim().trim_start_matches('@');
                    let feed_url = format!(
                        "https://api.bsky.app/xrpc/app.bsky.feed.getAuthorFeed?actor={}&limit=50",
                        urlencoding::encode(bsky_handle)
                    );
                    match http.get(&feed_url).timeout(std::time::Duration::from_secs(5)).send().await {
                        Ok(resp) if resp.status().is_success() => {
                            if let Ok(json) = resp.json::<serde_json::Value>().await {
                                if let Some(feed) = json.get("feed").and_then(|f| f.as_array()) {
                                    for item in feed {
                                        if let Some(post) = normalize::normalize_bluesky_api_post(item) {
                                            author_posts.push(post);
                                        }
                                    }
                                }
                            }
                        }
                        Ok(resp) => {
                            tracing::warn!("[FEED] getAuthorFeed returned {} for {}", resp.status(), bsky_handle);
                        }
                        Err(e) => {
                            tracing::warn!("[FEED] getAuthorFeed error: {}", e);
                        }
                    }
                    tracing::info!("[FEED] seed author after Bluesky fallback: {} posts for {}", author_posts.len(), author_clean);
                }

                // Deduplicate by URI across all authors
                for post in author_posts {
                    if seen_uris.insert(post.id.clone()) {
                        all_author_posts.push(post);
                    }
                }
            }

            // Sort by engagement (popular) or by date (new/chronological)
            if sort == SortMode::Popular {
                all_author_posts.sort_by(|a, b| {
                    let score_a = (a.likes_count as i64 * 3)
                        + (a.shares_count as i64 * 2)
                        + (a.replies_count as i64);
                    let score_b = (b.likes_count as i64 * 3)
                        + (b.shares_count as i64 * 2)
                        + (b.replies_count as i64);
                    score_b.cmp(&score_a)
                });
            } else {
                all_author_posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            }
            all_author_posts.truncate(50);

            tracing::info!("[FEED] seed author filter: {} total unique posts after sort", all_author_posts.len());
            posts = all_author_posts;
        }

        // Sort — the ONLY ordering force: New = recency, Popular = engagement.
        // NSFW policy (feed spec OQ-8 amended 2026-10-11): mark, then hide
        // unless the viewer opted in. Before the floor — hidden posts must not
        // consume floored slots.
        filter::apply_nsfw_policy(&mut posts, viewer_nsfw_pref);
        filter::apply_source_mix(&mut posts, query.source_mix.as_deref());
        filter::apply_platform_filter(&mut posts, query.platform.as_deref());
        filter::apply_time_window(&mut posts, time_window.as_ref());
        // Follow state from the viewer's real follow graph — before the floor,
        // which guarantees followed posts a place on the page.
        for p in posts.iter_mut() {
            if let Some(did) = &p.author_external_id {
                if viewer_follows.contains(did) {
                    p.is_following = Some(true);
                }
            }
            if p.is_following != Some(true) {
                // Timeline handles arrive PDS-suffixed (@user.bsky.social@bsky.social);
                // the follows table stores bare handles — normalize before matching.
                let h = p
                    .author
                    .handle
                    .trim_start_matches('@')
                    .split('@')
                    .next()
                    .unwrap_or("")
                    .to_lowercase();
                if follow_handles.contains(&h) {
                    p.is_following = Some(true);
                }
            }
        }

        filter::filter_by_types(&mut posts, &type_filter);
        // Pagination: a cursor page must contain only posts OLDER than the
        // cursor. Some sources (timeline fetches, feed generators, cached
        // hashtag batches) return their newest rows regardless of cursor;
        // dropping the overlap here guarantees pages never repeat.
        if let Some(ref c) = cursor {
            posts.retain(|p| {
                (p.created_at.as_str(), p.id.as_str()) < (c.created_at.as_str(), c.uri.as_str())
            });
        }
        // Counts matter pre-sort only for Popular's ranking; recency sorts
        // skip this network pass entirely.
        if sort == SortMode::Popular {
            sources::refresh_engagement_counts(&http, &mut posts, sort, 75).await;
        }
        sort.sort_posts(&mut posts);

        // Truncate to 30 — following gets a floor here.
        posts = truncate_with_source_floor(posts, 30, 2);

        // The floor can surface posts beyond the pre-sort refresh cap, and
        // Popular order must agree with the numbers shown — so give the
        // final page one more authoritative pass and re-rank it.
        sources::refresh_engagement_counts(&http, &mut posts, sort, 100).await;
        sort.sort_posts(&mut posts);

        fill_provenance(&mut posts);


        let seed_cursor = Cursor::from_oldest(&posts).map(|c| c.encode()).flatten();

        tracing::debug!("[FEED-T] rest-after-personal {:?}", ft_tail.elapsed());
        tracing::info!("[FEED] seed phase returning {} posts", posts.len());
        sources::fill_missing_profiles(&state.db, &mut posts).await;
        let mut resp_headers = HeaderMap::new();
        resp_headers.insert("Cache-Control", "no-store".parse().unwrap());
        let resp_body = FeedResponse {
            items: posts,
            cursor: seed_cursor,
            has_more: true,
            phase: Some("seed".to_string()),
            platform_counts,
        };
        // Persist as the serve-first snapshot for this request's key. The
        // phase="rebuild" path stores through here too. First page only —
        // a cursor page would overwrite the first-page snapshot.
        if query.cursor.is_none() {
            if let Ok(value) = serde_json::to_value(&resp_body) {
                snapshots::store(&state.db, &snap_key, &value).await;
            }
        }
        spawn_count_refresh(state.db.clone(), &resp_body.items);
        return Ok((resp_headers, Json(resp_body)));
    }

    // =========================================================================
    // FULL PHASE (background pre-fetch, ~60 posts)
    // =========================================================================
    let mut posts: Vec<FeedPost> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    // 1. Linked-account timelines — fetch all in parallel
    if cat_allowed("following") {
        if let Some(uid) = user_id {
            let accounts =
                match crate::db::accounts::LinkedAccountRow::find_by_user(&state.db, uid).await {
                    Ok(a) => a,
                    Err(e) => {
                        tracing::error!("[FEED-DBG] full-phase find_by_user failed: {}", e);
                        Vec::new()
                    }
                };
            tracing::debug!(
                "[FEED-DBG] full-phase timeline tier: {} linked accounts",
                accounts.iter().filter(|a| a.provider == "bluesky").count()
            );
                let timeline_futs: Vec<_> = accounts
                    .iter()
                    .filter(|a| a.provider == "bluesky")
                    .map(|acct| {
                        let http = &http;
                        async move {
                            match acct.provider.as_str() {
                                "bluesky" => {
                                    match tokio::time::timeout(
                                        Duration::from_secs(12),
                                        fetch_bluesky_timeline(http, acct),
                                    )
                                    .await
                                    {
                                        Ok(Ok(posts)) => posts,
                                        Ok(Err(e)) => {
                                            tracing::error!("[FEED-DBG] timeline error: {}", e);
                                            vec![]
                                        }
                                        Err(_) => {
                                            tracing::error!("[FEED-DBG] timeline timed out");
                                            vec![]
                                        }
                                    }
                                }
                                // Lemmy timeline removed — Sprint 1 (Lemmy deletion)
                                _ => vec![],
                            }
                        }
                    })
                    .collect();

                let timeline_results: Vec<Vec<FeedPost>> =
                    futures::future::join_all(timeline_futs).await;
                let mut timeline_total = 0usize;
                for mut timeline_posts in timeline_results {
                    timeline_total += timeline_posts.len();
                    for post in timeline_posts {
                        if seen_ids.insert(post.id.clone()) {
                            posts.push(post);
                        }
                    }
                }
                tracing::debug!(
                    "[FEED-DBG] full-phase timeline merged: {} fetched, pool={}",
                    timeline_total,
                    posts.len()
                );
        }
    }

    // 2. Imported posts (Facebook, Instagram, etc.)
    if cat_allowed("rss") {
        if let Some(uid) = user_id {
            if let Ok(imported) = sqlx::query_as::<_, crate::db::posts::PostRow>(
                "SELECT * FROM posts WHERE user_id = $1 AND is_imported = true ORDER BY original_ts DESC LIMIT 50",
            )
            .bind(uid)
            .fetch_all(&state.db)
            .await
            {
                for row in imported {
                    let author_info = AuthorInfo {
                        handle: format!(
                            "@{}@{}",
                            "user",
                            &state.config.domain
                        ),
                        display_name: Some("User".to_string()),
                        avatar: None,
                    };
                    let post =
                        crate::api::profile::post_row_to_feed_post(&row, &author_info, None);
                    if seen_ids.insert(post.id.clone()) {
                        posts.push(post);
                    }
                }
            }
            if !scoped && query.author.is_none() {
                if let Some(user) = viewer.as_ref() {
                    let mut imported = crate::api::profile::identity_appview_posts(
                        &state.db,
                        user.id,
                        &user.username,
                    )
                    .await?;
                    for post in &mut imported {
                        post.is_own = Some(true);
                    }
                    tag_sources(&mut imported, "import", "Imported history");
                    for post in imported {
                        if seen_ids.insert(post.id.clone()) {
                            posts.push(post);
                        }
                    }
                }
            }
        }
    }

    // 3. RSS feed items
    if cat_allowed("rss") {
        if let Some(uid) = user_id {
            if let Ok(items) =
                crate::db::rss::RssFeedItemRow::feed_for_user_with_labels(&state.db, uid, 50, 0)
                    .await
            {
                for item in items {
                    let post_id = format!(
                        "rss:{}",
                        item.link
                            .as_deref()
                            .unwrap_or(&item.title.clone().unwrap_or_default())
                    );
                    if seen_ids.insert(post_id.clone()) {
                        let author_name = item
                            .author
                            .clone()
                            .unwrap_or_else(|| item.subscription_label.clone());
                        let content_text = item
                            .description
                            .clone()
                            .or_else(|| item.content.clone())
                            .or_else(|| item.title.clone())
                            .unwrap_or_else(|| "(no content)".to_string());
                        let mut embeds = vec![];
                        if let Some(ref link) = item.link {
                            embeds.push(EmbedInfo {
                                kind: "link".to_string(),
                                url: link.clone(),
                                title: item.title.clone(),
                                description: None,
                                thumbnail: None,
                            });
                        }
                        posts.push(FeedPost {
                            id: post_id,
                            source: "rss".to_string(),
                            author: AuthorInfo {
                                handle: author_name.clone(),
                                display_name: Some(author_name),
                                avatar: None,
                            },
                            content: content_text,
                            content_type: "link".to_string(),
                            media: vec![],
                            embeds,
                            likes_count: 0,
                            shares_count: 0,
                            replies_count: 0,
                            created_at: item.pub_date.to_rfc3339(),
                            url: item.link.clone(),
                            thread_id: None,
                            start_time: None,
                            end_time: None,
                            location: None,
                            display_name: None,
                            action_text: None,
                            group_name: None, group: None, upvotes: None, downvotes: None, my_vote: None,
                            reaction_counts: None,
                            user_reaction: None,
                            cid: None,
                            author_url: None,
                            reposted_by: None,
                            boosted_by: None,
                            quoted_post_uri: None,
                            is_reshare: Some(false),
                            original_post: None,
                            is_own: None,
                            is_following: None,
                            follow_pending: None,
                            cross_posts: vec![],
                            matched_sources: vec![],
                            labels: vec![],
                            nsfw: false,
                            author_external_id: None,
                        });
                    }
                }
            }
        }
    }

    // 3b. Subscribed feed generators (at:// URIs — the formal AT Protocol
    // feeds; the full phase previously had no feeds tier at all).
    if cat_allowed("feeds") && (!scoped || !scope_feeds.is_empty()) {
        if let Some(uid) = user_id {
            match crate::db::subscriptions::SubscriptionRow::find_by_user(&state.db, uid).await {
                Ok(subs) => {
                    tracing::debug!(
                        "[FEED-DBG] 3b feeds: {} subs ({} at-enabled), scoped={}",
                        subs.len(),
                        subs.iter()
                            .filter(|s| s.category == "feeds" && s.enabled && s.source_id.starts_with("at://"))
                            .count(),
                        scoped
                    );
                let feed_futs: Vec<_> = subs
                    .iter()
                    .filter(|s| s.category == "feeds" && s.enabled && s.source_id.starts_with("at://"))
                    .filter(|s| !scoped || scope_feeds.iter().any(|f| f == &s.source_id))
                    .take(12)
                    .map(|s| {
                        let http = &http;
                        let db = &state.db;
                        async move {
                            let mut batch = sources::fetch_feed_posts(db, http, &s.source_id, 15 * type_boost).await;
                            tracing::debug!("[FEED-DBG] 3b fetched {} posts from {}", batch.len(), s.source_id);
                            tag_sources(&mut batch, "feed", &s.label);
                            batch
                        }
                    })
                    .collect();
                let mut fetched_total = 0usize;
                for batch in futures::future::join_all(feed_futs).await {
                    fetched_total += batch.len();
                    for post in batch {
                        if seen_ids.insert(post.id.clone()) {
                            posts.push(post);
                        }
                    }
                }
                tracing::error!(
                    "[FEED-DBG] 3b feeds: fetched={}, pool={}",
                    fetched_total,
                    posts.len()
                );
                }
                Err(e) => {
                    tracing::error!("[FEED-DBG] 3b feeds: find_by_user failed: {}", e);
                }
            }
        }
    }

    // 4. Interest-based AppView searches (parallel)
    if cat_allowed("hashtags") {
        {
            let mut seen_interests: HashSet<String> = HashSet::new();
            let unique_sources: Vec<&(_, _)> = sources
                .iter()
                .filter(|(id, _)| seen_interests.insert(id.clone()))
                .collect();

            let mut all_tags: HashSet<String> = if scoped {
                interest_tags.iter().cloned().collect()
            } else {
                HashSet::new()
            };
            for (_, source) in &unique_sources {
                for hashtag in &source.mastodon_hashtags {
                    all_tags.insert(hashtag.trim_start_matches('#').to_string());
                }
                if let Some(ref search_term) = source.bluesky_search {
                    for word in search_term.split_whitespace() {
                        all_tags.insert(word.to_string());
                    }
                }
            }

            tracing::info!(
                "[FEED] Phase 4a: searching AppView for {} unique hashtags",
                all_tags.len()
            );

            let since_ca: Option<String> = query.since.as_ref().and_then(|raw| {
                use base64::Engine;
                let bytes =
                    base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .decode(raw)
                        .ok()?;
                let obj: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
                obj.get("ca")?.as_str().map(|s| s.to_string())
            });

            let cursor_ref = cursor
                .as_ref()
                .map(|c| (c.created_at.as_str(), c.uri.as_str()));

            // One batched scan for all tags — the per-tag loop ran up to 64
            // SCAN_SLOTS-serialized table scans per request (28s+ warm).
            let tags_vec: Vec<String> = all_tags.iter().cloned().collect();
            let search_posts = appview_search_multi_pref(&state.db, &tags_vec, 120, cursor_ref, viewer_nsfw_pref).await;
            tracing::debug!(
                "[FEED] appview_search_multi({} tags) returned {} posts",
                tags_vec.len(),
                search_posts.len()
            );
            for post in search_posts {
                // Skip posts already returned in seed phase
                if let Some(ref since) = since_ca {
                    if post.created_at.as_str() >= since.as_str() {
                        continue;
                    }
                }
                if seen_ids.insert(post.id.clone()) {
                    posts.push(post);
                }
            }

            // Apply strict interest hashtag filter (same as seed phase)
            if !all_tags.is_empty() {
                let interest_tags: Vec<String> = all_tags.iter().cloned().collect();
                filter::tag_matches(&mut posts, &interest_tags);
            }
        }
    }

    // Tag interest matches on every full-phase batch (chips), no reordering —
    // the unconditional pass; the conditional prioritize above covers search.
    if !interest_tags.is_empty() {
        filter::tag_matches(&mut posts, &interest_tags);
    }

    // Fediverse (ActivityPub) tier in the full phase too — interest-matched AP
    // content (chips stamped at the fetch), respecting the seed cutoff
    // (query.since) so nothing is shown twice across seed + full.
    if cat_allowed("fediverse") && !interest_tags.is_empty() {
        let ap =
            sources::fetch_appview_ap(&state.db, &interest_tags, 20, sort).await;
        for mut post in ap {
            if let Some(ref since) = query.since {
                if post.created_at.as_str() >= since.as_str() {
                    continue;
                }
            }
            if seen_ids.insert(post.id.clone()) {
                tag_sources(std::slice::from_mut(&mut post), "fediverse", "Fediverse");
                posts.push(post);
            }
        }
    }

    // Apply uniform filters
    let filters = FeedFilters::from_user(&state.db, &headers, &state.config.jwt_secret).await?;
    posts.retain(|p| filters.should_keep(p));
    tracing::debug!("[FEED-DBG] pipeline: after_should_keep={}", posts.len());

    // Deduplicate
    if let Some(extra) = &platform_pool {
        posts.extend(extra.iter().cloned());
    }
    posts = deduplicate(posts);
    tracing::debug!("[FEED-DBG] pipeline: after_dedupe={}", posts.len());

    // Platform bucket filter BEFORE the 200-post truncation. Applied after the
    // floor, a sparse bucket (Lemmy/Pixelfed) is already crowded out by the
    // high-volume networks and the page comes back empty despite non-zero
    // counts. Same placement as the seed phase.
    filter::apply_platform_filter(&mut posts, query.platform.as_deref());
    // Same contract for the period filter: filter first, then sort, then
    // truncate — an old custom range must still survive the page cap.
    filter::apply_time_window(&mut posts, time_window.as_ref());

    // Sort
    filter::filter_by_types(&mut posts, &type_filter);
    // Counts matter pre-sort only for Popular's ranking; recency sorts skip
    // this network pass entirely.
    if sort == SortMode::Popular {
        sources::refresh_engagement_counts(&http, &mut posts, sort, 75).await;
    }
    sort.sort_posts(&mut posts);

    // Filter by author if specified — supports multiple comma-separated authors
    // Same approach as seed phase: query DB + Bluesky fallback, dedup, sort
    if let Some(ref author_filter) = query.author {
        let author_handles: Vec<String> = author_filter
            .split(',')
            .map(|a| a.trim().trim_start_matches('@').to_lowercase())
            .filter(|a| !a.is_empty())
            .collect();
        tracing::info!("[FEED] full phase author filter: {} handles -> {:?}", author_handles.len(), author_handles);

        let mut all_author_posts: Vec<FeedPost> = Vec::new();
        let mut seen_uris: HashSet<String> = HashSet::new();

        // Build cursor condition for keyset pagination
        let cursor_condition = cursor.as_ref().map(|c| {
            format!(
                "AND (created_at, uri) < ('{}'::timestamptz, '{}')",
                c.created_at.replace('\'', "''"),
                c.uri.replace('\'', "''")
            )
        }).unwrap_or_default();

        for raw_handle in &author_handles {
            // Extract handle from URL if it looks like a URL
            let author_clean = if raw_handle.contains("bsky.app/profile/") {
                raw_handle.split("bsky.app/profile/").last().unwrap_or(raw_handle).to_string()
            } else if raw_handle.contains("://") {
                raw_handle.split('/').last().unwrap_or(raw_handle).to_string()
            } else {
                raw_handle.clone()
            };

            // Query appview_posts with cursor-based pagination
            let query_sql = format!(
                r#"SELECT uri, cid, author_did, author_handle, author_avatar, author_display_name, text, created_at, embeds, labels, like_count, repost_count, reply_count
                   FROM appview_posts
                   WHERE (author_handle ILIKE $1 || '%' OR author_did ILIKE $1 || '%')
                                          AND reply_parent IS NULL
                     AND created_at <= now()
                     {}
                   ORDER BY created_at DESC
                   LIMIT 50"#,
                cursor_condition
            );

            let mut author_posts: Vec<FeedPost> = sqlx::query_as::<
                _,
                (String, String, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<chrono::DateTime<chrono::Utc>>, Option<serde_json::Value>, Option<serde_json::Value>, Option<i32>, Option<i32>, Option<i32>),
            >(&query_sql)
            .bind(&author_clean)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(uri, cid, did, author_handle, avatar, display_name, text, created, embeds, labels, likes, reposts, replies)| {
                normalize::normalize_appview_post(
                    uri, cid, did, author_handle, avatar, display_name, text, created, embeds,
                    normalize::labels_from_json(&labels),
                    likes.unwrap_or(0), reposts.unwrap_or(0), replies.unwrap_or(0),
                )
            })
            .collect();

            tracing::info!("[FEED] full phase author DB returned {} posts for {}", author_posts.len(), author_clean);

            // If no posts found in DB, try Bluesky public API
            if author_posts.is_empty() {
                let bsky_handle = author_clean.trim().trim_start_matches('@');
                let feed_url = format!(
                    "https://api.bsky.app/xrpc/app.bsky.feed.getAuthorFeed?actor={}&limit=50",
                    urlencoding::encode(bsky_handle)
                );
                match http.get(&feed_url).timeout(std::time::Duration::from_secs(5)).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        if let Ok(json) = resp.json::<serde_json::Value>().await {
                            if let Some(feed) = json.get("feed").and_then(|f| f.as_array()) {
                                for item in feed {
                                    if let Some(post) = normalize::normalize_bluesky_api_post(item) {
                                        author_posts.push(post);
                                    }
                                }
                            }
                        }
                    }
                    Ok(resp) => {
                        tracing::warn!("[FEED] getAuthorFeed returned {} for {}", resp.status(), bsky_handle);
                    }
                    Err(e) => {
                        tracing::warn!("[FEED] getAuthorFeed error: {}", e);
                    }
                }
                tracing::info!("[FEED] full phase author after Bluesky fallback: {} posts for {}", author_posts.len(), author_clean);
            }

            // Deduplicate by URI across all authors
            for post in author_posts {
                if seen_uris.insert(post.id.clone()) {
                    all_author_posts.push(post);
                }
            }
        }

        // Sort by engagement (popular) or by date (new/chronological)
        if sort == SortMode::Popular {
            all_author_posts.sort_by(|a, b| {
                let score_a = (a.likes_count as i64 * 3)
                    + (a.shares_count as i64 * 2)
                    + (a.replies_count as i64);
                let score_b = (b.likes_count as i64 * 3)
                    + (b.shares_count as i64 * 2)
                    + (b.replies_count as i64);
                score_b.cmp(&score_a)
            });
        } else {
            all_author_posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        }
        all_author_posts.truncate(50);

        tracing::info!("[FEED] full phase author filter: {} total unique posts after sort", all_author_posts.len());
        posts = all_author_posts;
    }

    // Truncate — with per-source floors and the fediverse reserve, so the
    // scroll page keeps interest-matched AP content instead of crowding it out.
    posts = truncate_with_source_floor(posts, 200, 2);

    // Displayed counts must be network-true and Popular order must agree
    // with them — one more authoritative pass over the page head.
    sources::refresh_engagement_counts(&http, &mut posts, sort, 100).await;
    sort.sort_posts(&mut posts);

    if let Some(uid) = user_id {
        enrich_with_reactions(&state.db, &mut posts, uid).await;
    }

    // Set is_own based on linked accounts
    if let Some(uid) = user_id {
        let accounts =
            crate::db::accounts::LinkedAccountRow::find_by_user(&state.db, uid)
                .await
                .unwrap_or_default();
        for post in &mut posts {
            if post.source == "bytescape" {
                let user = crate::db::users::UserRow::find_by_id(&state.db, uid)
                    .await
                    .unwrap_or(None);
                if let Some(u) = user {
                    let handle = post.author.handle.trim_start_matches('@').to_lowercase();
                    let username = u.username.to_lowercase();
                    if handle == username || handle.starts_with(&format!("{}@", username)) {
                        post.is_own = Some(true);
                    }
                }
            }
            for acct in &accounts {
                let provider = &acct.provider;
                if post.source.starts_with(provider) {
                    let post_handle =
                        post.author.handle.trim_start_matches('@').to_lowercase();
                    let external_handle =
                        acct.external_id.trim_start_matches('@').to_lowercase();
                    if post_handle == external_handle
                        || post_handle.starts_with(&format!("{}@", external_handle))
                    {
                        post.is_own = Some(true);
                    }
                    let ext_user =
                        acct.external_username.trim_start_matches('@').to_lowercase();
                    if post_handle == ext_user
                        || post_handle.starts_with(&format!("{}@", ext_user))
                    {
                        post.is_own = Some(true);
                    }
                }
            }
        }
    }

    // Enrich cross_posts badges for local Bytescape posts
    if user_id.is_some() {
        for post in &mut posts {
            if post.source == "bytescape" {
                if let Ok(post_uuid) = uuid::Uuid::parse_str(&post.id) {
                    if let Ok(cross_post_rows) =
                        db::cross_posts::CrossPostRow::find_by_post(&state.db, post_uuid).await
                    {
                        post.cross_posts = cross_post_rows
                            .iter()
                            .map(|cp| CrossPostBadge {
                                platform: cp.platform.clone(),
                                external_url: cp.external_url.clone(),
                                ok: true,
                            })
                            .collect();
                    }
                }
            }
        }
        // Own current posts stay out of the home feed; imported history is the
        // explicit exception so linked Mastodon archives remain visible.
        posts.retain(|post| {
            post.is_own != Some(true)
                || post.matched_sources.iter().any(|m| m.kind == "import")
        });
    }

    // Follow state from the viewer's real follow graph — deterministic marks.
    if let Some(uid) = user_id {
        if let Ok(accounts) = crate::db::accounts::LinkedAccountRow::find_by_user(&state.db, uid).await {
            if let Some(acct) = accounts.iter().find(|a| a.provider == "bluesky") {
                let token = refresh_if_expired(&state.db, &http, acct).await.unwrap_or_default();
                if !token.is_empty() {
                if let Ok(followed) = oauth::bluesky::get_viewer_follows(
                    &http,
                    &token,
                    &acct.external_id,
                    acct.dpop_private_key.as_deref(),
                )
                .await
                {
                    for p in posts.iter_mut() {
                        if let Some(did) = &p.author_external_id {
                            if followed.contains(did) {
                                p.is_following = Some(true);
                            }
                        }
                    }
                }
                }
            }
        }
    }

    // NSFW policy (feed spec OQ-8 amended 2026-10-11): mark, then hide
    // unless the viewer opted in.
    filter::apply_nsfw_policy(&mut posts, viewer_nsfw_pref);
    filter::apply_source_mix(&mut posts, query.source_mix.as_deref());
    filter::apply_platform_filter(&mut posts, query.platform.as_deref());
    fill_provenance(&mut posts);

    // Build cursor
    let response_cursor = Cursor::from_oldest(&posts).map(|c| c.encode()).flatten();
    let has_more = response_cursor.is_some() && posts.len() >= 20;

    tracing::info!("[FEED] full phase returning {} posts", posts.len());
    sources::fill_missing_profiles(&state.db, &mut posts).await;
    spawn_count_refresh(state.db.clone(), &posts);
    let mut resp_headers = HeaderMap::new();
    resp_headers.insert("Cache-Control", "no-store".parse().unwrap());
    Ok((
        resp_headers,
        Json(FeedResponse {
            cursor: response_cursor,
            has_more,
            items: posts,
            phase: Some("full".to_string()),
            platform_counts,
        }),
    ))
}

// ---------------------------------------------------------------------------
// Other feed endpoints (preserved from original)
// ---------------------------------------------------------------------------

/// Shared HTTP client for feed fetches. Every outbound call gets hard
/// timeouts: one dead feed generator must never stall the fan-out.
pub(crate) fn feed_http() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// Per-user cache for the fully aggregated + enriched home feed. Short TTL:
/// new posts surface within a minute while repeat loads are instant.
static AGGREGATED_CACHE: std::sync::LazyLock<
    parking_lot::Mutex<std::collections::HashMap<String, (std::time::Instant, Vec<FeedPost>)>>,
> = std::sync::LazyLock::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));
const AGGREGATED_TTL: std::time::Duration = std::time::Duration::from_secs(60);
/// Beyond the TTL the previous response is still served (stale) while a
/// background rebuild runs — up to this age.
const AGGREGATED_STALE_MAX: std::time::Duration = std::time::Duration::from_secs(600);

/// GET /api/feed — Aggregated feed from all linked sources
async fn aggregated_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<FeedResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    // Serve-stale-while-revalidate: within TTL the cached response is served
    // as-is; beyond it the previous response is still served instantly while
    // a background task rebuilds. Only the very first load (no previous
    // response at all) pays the cold aggregation.
    {
        let map = AGGREGATED_CACHE.lock();
        if let Some((at, posts)) = map.get(&user_id.to_string()) {
            if at.elapsed() < AGGREGATED_TTL {
                return Ok(Json(FeedResponse {
                    cursor: None,
                    has_more: false,
                    items: posts.clone(),
                    phase: None,
                    platform_counts: None,
                }));
            }
            if at.elapsed() < AGGREGATED_STALE_MAX {
                let items = posts.clone();
                drop(map);
                let bg_state = state.clone();
                tokio::spawn(async move {
                    build_aggregated(bg_state, user_id).await;
                });
                return Ok(Json(FeedResponse {
                    cursor: None,
                    has_more: false,
                    items,
                    phase: None,
                    platform_counts: None,
                }));
            }
        }
    }

    let items = build_aggregated(state, user_id).await;
    Ok(Json(FeedResponse {
        cursor: None,
        has_more: false,
        items,
        phase: None,
        platform_counts: None,
    }))
}

/// Build the aggregated home feed for one user: account timelines, saved
/// feeds, interests, RSS and discovery — merged, filtered and enriched.
/// Updates AGGREGATED_CACHE on completion.
pub(crate) async fn build_aggregated(state: AppState, user_id: Uuid) -> Vec<FeedPost> {
    let accounts = crate::db::accounts::LinkedAccountRow::find_by_user(&state.db, user_id)
        .await
        .unwrap_or_default();

    let http = feed_http();
    let mut all_posts: Vec<FeedPost> = Vec::new();

    // Fetch from each linked account
    for acct in &accounts {
        match acct.provider.as_str() {
            "bluesky" => {
                if let Ok(posts) = fetch_bluesky_timeline(&http, acct).await {
                    all_posts.extend(posts);
                }
            }
            _ => {}
        }
    }

    // ── Source fan-out ────────────────────────────────────────────────────
    // Every saved feed and interest maps to one remote fetch. These ran
    // sequentially — with 100+ subscriptions that is minutes of serial
    // network latency, pushing the handler past the client timeout. Fetch
    // all sources concurrently in bounded batches, through the shared
    // result caches (fetch_feed_posts / appview_search are cached).
    enum SourceFetch {
        Feed(String, i64),
        Search(String, i64),
    }
    let mut source_fetches: Vec<SourceFetch> = Vec::new();

    if let Ok(feeds) = crate::db::subscriptions::SubscriptionRow::find_by_user_and_category(
        &state.db,
        user_id,
        "feeds",
    )
    .await
    {
        for feed in &feeds {
            if feed.source_id.starts_with("at://") {
                source_fetches.push(SourceFetch::Feed(feed.source_id.clone(), 20));
            } else {
                source_fetches.push(SourceFetch::Search(feed.source_id.clone(), 20));
            }
        }
    }

    if let Ok(Some(user)) = crate::db::users::UserRow::find_by_id(&state.db, user_id).await {
        let interest_ids: Vec<String> = user
            .interests
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !interest_ids.is_empty() {
            let sources = interest_sources::get_all_sources_for_interests(&interest_ids);
            for (_interest_id, source) in &sources {
                if let Some(search_term) = &source.bluesky_search {
                    source_fetches.push(SourceFetch::Search(search_term.clone(), 10));
                }
                if let Some(feed_uri) = &source.bluesky_feed {
                    source_fetches.push(SourceFetch::Feed(feed_uri.clone(), 10));
                }
            }
        }
    }

    // Bounded concurrency: big enough to collapse latency into a few round
    // trips, small enough to stay polite to the public AppView. Each task is
    // additionally time-boxed — a wedged source contributes nothing instead
    // of stalling its batch (clients time out at 12s; 30s covers multi-
    // request fetchers).
    for batch in source_fetches.chunks(64) {
        let fetches = batch.iter().map(|f| {
            let db = &state.db;
            let http = &http;
            async move {
                let fut = async {
                    match f {
                        SourceFetch::Feed(uri, limit) => {
                            sources::fetch_feed_posts(db, http, uri, *limit).await
                        }
                        SourceFetch::Search(term, limit) => {
                            appview_search(db, term, *limit, None, NsfwPref::Exclude).await
                        }
                    }
                };
                tokio::time::timeout(std::time::Duration::from_secs(30), fut)
                    .await
                    .unwrap_or_default()
            }
        });
        for posts in futures::future::join_all(fetches).await {
            all_posts.extend(posts);
        }
    }

    // Dedup own cross-posts
    if let Ok(user_cross_posts) =
        db::cross_posts::CrossPostRow::find_by_user(&state.db, user_id).await
    {
        let mut bluesky_ids: HashSet<String> = HashSet::new();
        let mut mastodon_ids: HashSet<String> = HashSet::new();
        for cp in &user_cross_posts {
            if let Some(ref eid) = cp.external_id {
                match cp.platform.as_str() {
                    "bluesky" => {
                        bluesky_ids.insert(eid.clone());
                    }
                    "mastodon" => {
                        mastodon_ids.insert(eid.clone());
                    }
                    _ => {}
                }
            }
        }
        all_posts.retain(|post| match post.source.as_str() {
            "bluesky" => !bluesky_ids.contains(&post.id),
            "mastodon" => !mastodon_ids.contains(&post.id),
            _ => true,
        });
    }

    // RSS feed items
    if let Ok(items) = crate::db::rss::RssFeedItemRow::feed_for_user_with_labels(
        &state.db, user_id, 50, 0,
    )
    .await
    {
        for item in items {
            let post_id = format!(
                "rss:{}",
                item.link
                    .as_deref()
                    .unwrap_or(&item.title.clone().unwrap_or_default())
            );
            let author_name = item
                .author
                .clone()
                .unwrap_or_else(|| item.subscription_label.clone());
            let content_text = item
                .description
                .clone()
                .or_else(|| item.content.clone())
                .or_else(|| item.title.clone())
                .unwrap_or_else(|| "(no content)".to_string());
            let mut embeds = vec![];
            if let Some(ref link) = item.link {
                embeds.push(EmbedInfo {
                    kind: "link".to_string(),
                    url: link.clone(),
                    title: item.title.clone(),
                    description: None,
                    thumbnail: None,
                });
            }
            all_posts.push(FeedPost {
                id: post_id,
                source: "rss".to_string(),
                author: AuthorInfo {
                    handle: author_name.clone(),
                    display_name: Some(author_name),
                    avatar: None,
                },
                content: content_text,
                content_type: "link".to_string(),
                media: vec![],
                embeds,
                likes_count: 0,
                shares_count: 0,
                replies_count: 0,
                created_at: item.pub_date.to_rfc3339(),
                url: item.link.clone(),
                thread_id: None,
                start_time: None,
                end_time: None,
                location: None,
                display_name: None,
                action_text: None,
                group_name: None, group: None, upvotes: None, downvotes: None, my_vote: None,
                reaction_counts: None,
                user_reaction: None,
                cid: None,
                author_url: None,
                reposted_by: None,
                boosted_by: None,
                quoted_post_uri: None,
                is_reshare: Some(false),
                original_post: None,
                is_own: None,
                is_following: None,
                follow_pending: None,
                cross_posts: vec![],
                matched_sources: vec![],
                labels: vec![],
                nsfw: false,
                author_external_id: None,
            });
        }
    }

    // Discover content
    if let Ok(discover) = discover_feed_inner(&state.db).await {
        all_posts.extend(discover.0.items);
    }

    // Filter muted accounts
    let muted_dids = get_muted_dids(&state.db, user_id).await;
    if !muted_dids.is_empty() {
        let muted_set: HashSet<String> = muted_dids.iter().cloned().collect();
        all_posts.retain(|post| {
            if let Some(ref ext_id) = post.author_external_id {
                if muted_set.contains(ext_id) {
                    return false;
                }
            }
            let handle_clean = post.author.handle.trim_start_matches('@').to_lowercase();
            !muted_dids
                .iter()
                .any(|m| m.to_lowercase() == handle_clean)
        });
    }

    all_posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    all_posts.truncate(50);

    enrich_with_reactions(&state.db, &mut all_posts, user_id).await;

    // Set is_own
    let accounts = crate::db::accounts::LinkedAccountRow::find_by_user(&state.db, user_id)
        .await
        .unwrap_or_default();
    for post in &mut all_posts {
        if post.source == "bytescape" {
            let user = crate::db::users::UserRow::find_by_id(&state.db, user_id)
                .await
                .unwrap_or(None);
            if let Some(u) = user {
                let handle = post.author.handle.trim_start_matches('@').to_lowercase();
                let username = u.username.to_lowercase();
                if handle == username || handle.starts_with(&format!("{}@", username)) {
                    post.is_own = Some(true);
                }
            }
        }
        for acct in &accounts {
            let provider = &acct.provider;
            if post.source.starts_with(provider) {
                let post_handle = post.author.handle.trim_start_matches('@').to_lowercase();
                let external_handle = acct.external_id.trim_start_matches('@').to_lowercase();
                if post_handle == external_handle
                    || post_handle.starts_with(&format!("{}@", external_handle))
                {
                    post.is_own = Some(true);
                }
                let ext_user = acct.external_username.trim_start_matches('@').to_lowercase();
                if post_handle == ext_user
                    || post_handle.starts_with(&format!("{}@", ext_user))
                {
                    post.is_own = Some(true);
                }
            }
        }
    }

    // Enrich cross_posts badges
    for post in &mut all_posts {
        if post.source == "bytescape" {
            if let Ok(post_uuid) = uuid::Uuid::parse_str(&post.id) {
                if let Ok(cross_post_rows) =
                    db::cross_posts::CrossPostRow::find_by_post(&state.db, post_uuid).await
                {
                    post.cross_posts = cross_post_rows
                        .iter()
                        .map(|cp| CrossPostBadge {
                            platform: cp.platform.clone(),
                            external_url: cp.external_url.clone(),
                            ok: true,
                        })
                        .collect();
                }
            }
        }
    }

    // Exclude own posts
    all_posts.retain(|post| post.is_own != Some(true));

    enrich_with_follow_status(&http, &state.db, &mut all_posts, user_id, &state.config.domain).await;
    sources::fill_missing_profiles(&state.db, &mut all_posts).await;

    {
        let mut map = AGGREGATED_CACHE.lock();
        map.retain(|_, (at, _)| at.elapsed() < AGGREGATED_TTL);
        map.insert(user_id.to_string(), (std::time::Instant::now(), all_posts.clone()));
    }

    all_posts
}

/// GET /api/feed/bluesky — Bluesky home timeline
async fn bluesky_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<FeedResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let accounts =
        crate::db::accounts::LinkedAccountRow::find_by_provider(&state.db, user_id, "bluesky")
            .await
            .map_err(AppError::Database)?;

    if accounts.is_empty() {
        return Err(AppError::NotFound(
            "No Bluesky account linked. Connect one in Settings.".into(),
        ));
    }

    let http = feed_http();
    let mut posts = fetch_bluesky_timeline(&http, &accounts[0])
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch Bluesky timeline: {}", e)))?;

    enrich_with_reactions(&state.db, &mut posts, user_id).await;

    let all_accounts = crate::db::accounts::LinkedAccountRow::find_by_user(&state.db, user_id)
        .await
        .unwrap_or_default();
    for post in &mut posts {
        for acct in &all_accounts {
            if acct.provider == "bluesky" {
                let post_handle = post.author.handle.trim_start_matches('@').to_lowercase();
                let external_handle = acct.external_id.trim_start_matches('@').to_lowercase();
                if post_handle == external_handle
                    || post_handle.starts_with(&format!("{}@", external_handle))
                {
                    post.is_own = Some(true);
                }
                let ext_user = acct.external_username.trim_start_matches('@').to_lowercase();
                if post_handle == ext_user
                    || post_handle.starts_with(&format!("{}@", ext_user))
                {
                    post.is_own = Some(true);
                }
            }
        }
    }

    sources::fill_missing_profiles(&state.db, &mut posts).await;
    Ok(Json(FeedResponse {
        cursor: None,
        has_more: false,
        items: posts,
        phase: None,
        platform_counts: None,
    }))
}

#[derive(Deserialize)]
struct HashtagQuery {
    tag: String,
    provider: Option<String>,
}

/// GET /api/feed/hashtag?tag=X — Search hashtag across platforms
async fn hashtag_feed(
    State(state): State<AppState>,
    Query(query): Query<HashtagQuery>,
) -> Result<Json<FeedResponse>, AppError> {
    let mut all_posts: Vec<FeedPost> = Vec::new();

    if query.provider.as_deref().is_none_or(|p| p == "bluesky") {
        if let Ok(result) = oauth::bluesky::BlueskyPublicApi::search_posts(&query.tag, 20).await {
            if let Some(feed) = result["posts"].as_array() {
                for item in feed {
                    if let Some(post) = normalize_bluesky_api_post(item) {
                        all_posts.push(post);
                    }
                }
            }
        }
    }

    all_posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    all_posts.truncate(50);
    sources::fill_missing_profiles(&state.db, &mut all_posts).await;

    Ok(Json(FeedResponse {
        cursor: None,
        has_more: false,
        items: all_posts,
        phase: None,
        platform_counts: None,
    }))
}

/// GET /api/feed/search?q=X — Search across platforms
#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

async fn search_feed(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<FeedResponse>, AppError> {
    let mut all_posts: Vec<FeedPost> = Vec::new();

    if let Ok(result) = oauth::bluesky::BlueskyPublicApi::search_posts(&query.q, 20).await {
        if let Some(feed) = result["posts"].as_array() {
            for item in feed {
                if let Some(post) = normalize_bluesky_api_post(item) {
                    all_posts.push(post);
                }
            }
        }
    }

    all_posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    all_posts.truncate(50);
    sources::fill_missing_profiles(&state.db, &mut all_posts).await;

    Ok(Json(FeedResponse {
        cursor: None,
        has_more: false,
        items: all_posts,
        phase: None,
        platform_counts: None,
    }))
}

/// GET /api/discover — Trending/discover content
async fn discover_feed(State(state): State<AppState>) -> Result<Json<FeedResponse>, AppError> {
    discover_feed_inner(&state.db).await
}

#[derive(Deserialize)]
struct InterestQuery {
    #[serde(default)]
    ids: String,
}

/// GET /api/feed/interests?ids=cats,dogs,music — Feed from selected interests
async fn interest_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<InterestQuery>,
) -> Result<Json<FeedResponse>, AppError> {
    let viewer_nsfw_pref = fetch_nsfw_pref(&state.db, &headers, &state.config.jwt_secret).await;
    let mut interest_ids: Vec<String> = query
        .ids
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // If no interest IDs passed, load from user profile (mirrors combined_feed)
    if interest_ids.is_empty() {
        if let Ok(uid) = extract_user_id(&headers, &state.config.jwt_secret) {
            if let Ok(Some(user)) = crate::db::users::UserRow::find_by_id(&state.db, uid).await {
                interest_ids = user.interests;
            }
        }
    }

    let sources = interest_sources::get_all_sources_for_interests(&interest_ids);

    let http = feed_http();
    let mut all_posts: Vec<FeedPost> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    for (_interest_id, source) in &sources {
        if let Some(search_term) = &source.bluesky_search {
            if let Ok(result) =
                oauth::bluesky::BlueskyPublicApi::search_posts(search_term, 10).await
            {
                if let Some(posts) = result["posts"].as_array() {
                    for item in posts {
                        if let Some(post) = normalize_bluesky_api_post(item) {
                            if seen_ids.insert(post.id.clone()) {
                                all_posts.push(post);
                            }
                        }
                    }
                }
            }
        }

        if let Some(feed_uri) = &source.bluesky_feed {
            if let Ok(result) = oauth::bluesky::BlueskyPublicApi::get_feed(feed_uri, 10).await {
                if let Some(feed_items) = result["feed"].as_array() {
                    for item in feed_items {
                        if let Some(post) = normalize_bluesky_api_post(item) {
                            if seen_ids.insert(post.id.clone()) {
                                all_posts.push(post);
                            }
                        }
                    }
                }
            }
        }
    }

    // Fetch from linked accounts if authenticated
    if let Ok(user_id) = extract_user_id(&headers, &state.config.jwt_secret) {
        if let Ok(accounts) =
            crate::db::accounts::LinkedAccountRow::find_by_user(&state.db, user_id).await
        {
            for acct in &accounts {
                match acct.provider.as_str() {
                    "bluesky" => {
                        if let Ok(posts) = fetch_bluesky_timeline(&http, acct).await {
                            for post in posts {
                                if seen_ids.insert(post.id.clone()) {
                                    all_posts.push(post);
                                }
                            }
                        }
                    }
                    // Lemmy timeline removed — Sprint 1 (Lemmy deletion)
                    _ => {}
                }
            }
        }
    }

    // Fallback: if fewer than 20 posts, fetch recent AppView posts
    if all_posts.len() < 20 {
        let fallback_rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                Option<String>,
                Option<chrono::DateTime<chrono::Utc>>,
                Option<serde_json::Value>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<serde_json::Value>,
                Option<i64>,
                Option<i64>,
                Option<i64>,
            ),
        >(
            &format!(
                r#"SELECT uri, cid, author_did, text, created_at, embeds, author_handle, author_avatar, author_display_name, labels,
                      like_count::bigint, repost_count::bigint, reply_count::bigint
               FROM appview_posts
               WHERE reply_parent IS NULL AND text IS NOT NULL AND text != ''
                                  AND created_at <= now()
                 AND {}
               ORDER BY created_at DESC NULLS LAST
               LIMIT 30"#,
                filter::sql_labels_filter("labels", viewer_nsfw_pref),
            ),
        )
        .fetch_all(&state.db)
        .await;

        match fallback_rows {
            Ok(rows) => {
                for (uri, cid, did, text, created, embeds, author_handle, author_avatar, author_display_name, labels_json, likes, reposts, replies) in rows {
                    // The embeds column was previously ignored here — these
                    // fallback posts rendered text-only.
                    let (media, embed_info) = match &embeds {
                        Some(emb) => normalize::parse_bluesky_embeds(emb, &did),
                        None => (vec![], vec![]),
                    };
                    let content = text.unwrap_or_default();
                    let display_handle = author_handle
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                    let label_list: Vec<String> = labels_json
                        .as_ref()
                        .and_then(|v| v.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(|s| s.to_lowercase()))
                                .collect()
                        })
                        .unwrap_or_default();

                    let post = FeedPost {
                        id: uri.clone(),
                        source: normalize::source_from_uri(&uri).to_string(),
                        author: AuthorInfo {
                            handle: normalize::format_handle(&display_handle),
                            display_name: author_display_name,
                            avatar: author_avatar,
                        },
                        content_type: normalize::classify_content(&uri, &content, &media, &embed_info).to_string(),
                        content: content.clone(),
                        media,
                        embeds: embed_info,
                        likes_count: likes.unwrap_or(0) as u32,
                        shares_count: reposts.unwrap_or(0) as u32,
                        replies_count: replies.unwrap_or(0) as u32,
                        created_at: created.map(|c| c.to_rfc3339()).unwrap_or_default(),
                        url: Some(uri.clone()),
                        thread_id: Some(format!("bluesky:{}", uri)),
                        start_time: None,
                        end_time: None,
                        location: None,
                        display_name: None,
                        action_text: None,
                        group_name: None, group: None, upvotes: None, downvotes: None, my_vote: None,
                        reaction_counts: None,
                        user_reaction: None,
                        cid: Some(cid),
                        author_url: Some(format!("https://bsky.app/profile/{}", did)),
                        reposted_by: None,
                        boosted_by: None,
                        quoted_post_uri: None,
                        is_reshare: Some(false),
                        original_post: None,
                        is_own: None,
                        is_following: None,
                        follow_pending: None,
                        cross_posts: vec![],
                        matched_sources: vec![],
                        labels: label_list,
                        nsfw: false,
                        author_external_id: Some(did),
                    };
                    if seen_ids.insert(post.id.clone()) {
                        all_posts.push(post);
                    }
                }
            }
            Err(e) => {
                tracing::error!("[FEED] Fallback query error: {}", e);
            }
        }
    }

    all_posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    all_posts.truncate(200);

    if let Ok(user_id) = extract_user_id(&headers, &state.config.jwt_secret) {
        enrich_with_reactions(&state.db, &mut all_posts, user_id).await;
    }

    filter::apply_nsfw_policy(
        &mut all_posts,
        fetch_nsfw_pref(&state.db, &headers, &state.config.jwt_secret).await,
    );
    sources::fill_missing_profiles(&state.db, &mut all_posts).await;

    Ok(Json(FeedResponse {
        cursor: None,
        has_more: all_posts.len() >= 100,
        items: all_posts,
        phase: None,
        platform_counts: None,
    }))
}

/// GET /api/feed/sources?sources=hashtag:dogs,feed:at://...,follows:did:plc:...
#[derive(Deserialize, Clone)]
struct SourcesQuery {
    sources: String,
}

async fn sources_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SourcesQuery>,
) -> Result<Json<FeedResponse>, AppError> {
    // Serve-first snapshots for the source views — the same staleness
    // ladder as the combined seed (feed/snapshots.rs). The hashtag/
    // subscription/feed views used to recompute their whole query set on
    // every cold hit (8-26s on the test DB); a stored response is returned
    // immediately and a stale one spawns a single background rebuild.
    // Key: viewer + normalized source set + NSFW axis (the policy and the
    // viewer's reactions are applied while building).
    let viewer_nsfw_pref = fetch_nsfw_pref(&state.db, &headers, &state.config.jwt_secret).await;
    let user_id = optional_viewer(&headers, &state.config.jwt_secret)?;
    let mut norm_sources: Vec<String> = query
        .sources
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    norm_sources.sort();
    let snap_key = format!(
        "src:{}|s={}|n={}",
        user_id.map(|u| u.to_string()).unwrap_or_else(|| "-".to_string()),
        norm_sources.join(","),
        viewer_nsfw_pref.snapshot_axis(),
    );
    if let Some(snap) = snapshots::load(&state.db, &snap_key).await {
        if let Ok(resp) = serde_json::from_value::<FeedResponse>(snap.payload) {
            if snap.age_secs > snapshots::STALE_SECS {
                snapshots::spawn_rebuild_with(snap_key.clone(), {
                    let state = state.clone();
                    let headers = headers.clone();
                    let query = query.clone();
                    let key = snap_key.clone();
                    move || async move {
                        match sources_feed_build(&state, &headers, &query).await {
                            Ok(Json(resp)) => {
                                if let Ok(value) = serde_json::to_value(&resp) {
                                    snapshots::store(&state.db, &key, &value).await;
                                }
                                tracing::info!("source snapshot rebuilt for {}", key);
                            }
                            Err(e) => {
                                tracing::warn!("source snapshot rebuild failed for {}: {}", key, e)
                            }
                        }
                    }
                });
            }
            return Ok(Json(resp));
        }
    }
    let Json(resp) = sources_feed_build(&state, &headers, &query).await?;
    if let Ok(value) = serde_json::to_value(&resp) {
        snapshots::store(&state.db, &snap_key, &value).await;
    }
    Ok(Json(resp))
}

/// The sources view's real computation — wrapped by [`sources_feed`]'s
/// serve-first snapshot layer (the rebuild path calls this directly and
/// stores the result itself).
async fn sources_feed_build(
    state: &AppState,
    headers: &HeaderMap,
    query: &SourcesQuery,
) -> Result<Json<FeedResponse>, AppError> {
    let viewer_nsfw_pref = fetch_nsfw_pref(&state.db, &headers, &state.config.jwt_secret).await;
    let mut all_posts: Vec<FeedPost> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    let sources_list: Vec<String> = query
        .sources
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    // Handle hashtag sources in parallel
    let hashtag_sources: Vec<String> = sources_list
        .iter()
        .filter(|s| s.starts_with("hashtag:"))
        .cloned()
        .collect();
    let other_sources: Vec<String> = sources_list
        .iter()
        .filter(|s| !s.starts_with("hashtag:"))
        .cloned()
        .collect();

    let mut hashtag_futures = vec![];
    for source in hashtag_sources.iter() {
        let (_, tag) = source.split_once(':').unwrap_or(("hashtag", "unknown"));
        let tag = tag.trim_start_matches('#').to_string();
        let db = state.db.clone();
        hashtag_futures.push(async move {
            let mut posts = vec![];
            // Tag-index candidate walk (appview_post_tags, migration 064)
            // instead of a `text ~*` regex scan: the regex paid a trigram
            // bitmap + whole-table recheck per tag (8s+ per tag on the
            // 3.1M-row index — the multi-tag source view stacked them).
            // Tag rows carry exactly the hashtags extracted from a post's
            // text — the same membership rule the combined feed's hashtag
            // tier uses.
            let tag_key = tag.to_lowercase();
            let candidates: Vec<String> = sqlx::query_as::<_, (String,)>(
                r#"SELECT uri FROM appview_post_tags
                   WHERE tag = $1
                   ORDER BY created_at DESC NULLS LAST
                   LIMIT 60"#,
            )
            .bind(&tag_key)
            .fetch_all(&db)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(uri,)| uri)
            .collect();
            if candidates.is_empty() {
                return posts;
            }
            let rows = sqlx::query_as::<
                _,
                (
                    String,
                    String,
                    String,
                    Option<String>,
                    Option<chrono::DateTime<chrono::Utc>>,
                    Option<serde_json::Value>,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                    Option<serde_json::Value>,
                ),
            >(
                &format!(
                    r#"SELECT uri, cid, author_did, text, created_at, embeds, author_handle, author_avatar, author_display_name, labels
                   FROM appview_posts
                   WHERE uri = ANY($1::text[]) AND reply_parent IS NULL
                                          AND created_at <= now()
                     AND {}
                   ORDER BY created_at DESC NULLS LAST
                   LIMIT 20"#,
                    filter::sql_labels_filter("labels", viewer_nsfw_pref),
                ),
            )
            .bind(&candidates)
            .fetch_all(&db)
            .await
            .unwrap_or_default();

            for (uri, cid, did, text, created, embeds, author_handle, author_avatar, author_display_name, labels_json) in rows {
                let display_handle = author_handle
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                let label_list: Vec<String> = labels_json
                    .as_ref()
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(|s| s.to_lowercase()))
                            .collect()
                    })
                    .unwrap_or_default();
                let (media, embed_info) = match &embeds {
                    Some(emb) => normalize::parse_bluesky_embeds(emb, &did),
                    None => (vec![], vec![]),
                };
                let post = FeedPost {
                    id: uri.clone(),
                    source: normalize::source_from_uri(&uri).to_string(),
                    author: AuthorInfo {
                        handle: normalize::format_handle(&display_handle),
                        display_name: author_display_name,
                        avatar: author_avatar,
                    },
                    content: text.unwrap_or_default(),
                    content_type: if !media.is_empty() {
                        "image"
                    } else {
                        "short"
                    }
                    .to_string(),
                    media,
                    embeds: embed_info,
                    likes_count: 0,
                    shares_count: 0,
                    replies_count: 0,
                    created_at: created.map(|c| c.to_rfc3339()).unwrap_or_default(),
                    url: Some(uri.clone()),
                    thread_id: Some(format!("bluesky:{}", uri)),
                    start_time: None,
                    end_time: None,
                    location: None,
                    display_name: None,
                    action_text: None,
                    group_name: None, group: None, upvotes: None, downvotes: None, my_vote: None,
                    reaction_counts: None,
                    user_reaction: None,
                    cid: Some(cid),
                    author_url: Some(format!("https://bsky.app/profile/{}", did)),
                    reposted_by: None,
                    boosted_by: None,
                    quoted_post_uri: None,
                    is_reshare: Some(false),
                    original_post: None,
                    is_own: None,
                    is_following: None,
                    follow_pending: None,
                    cross_posts: vec![],
                    matched_sources: vec![],
                    labels: label_list,
                    nsfw: false,
                    author_external_id: Some(did),
                };
                posts.push(post);
            }
            posts
        });
    }

    let mut handles = vec![];
    for fut in hashtag_futures {
        handles.push(tokio::spawn(async move { fut.await }));
    }
    for handle in handles {
        if let Ok(posts) = handle.await {
            for post in posts {
                if seen_ids.insert(post.id.clone()) {
                    all_posts.push(post);
                }
            }
        }
    }

    // Handle other source types
    for source in &other_sources {
        let (source_type, source_id) = match source.split_once(':') {
            Some((t, id)) => (t, id),
            None => continue,
        };

        match source_type {
            "feed" => {
                if let Ok(result) =
                    oauth::bluesky::BlueskyPublicApi::get_feed(source_id, 20).await
                {
                    if let Some(feed_items) = result["feed"].as_array() {
                        for item in feed_items {
                            if let Some(post) = normalize_bluesky_api_post(item) {
                                if seen_ids.insert(post.id.clone()) {
                                    all_posts.push(post);
                                }
                            }
                        }
                    }
                }
            }
            "follows" => {
                // Lemmy removed — Sprint 1
                if !source_id.starts_with("http") {
                    let posts = appview_author_feed(&state.db, source_id, 20).await;
                    for post in posts {
                        if seen_ids.insert(post.id.clone()) {
                            all_posts.push(post);
                        }
                    }
                }
            }
            // Group feed source (`groups:<actor_id>`) and the aggregate
            // "Subscriptions" source (`subscriptions:`) — design D6. Both read
            // `appview_posts.group_actor_id` (the AP group registry link).
            "groups" => {
                let posts = sources::fetch_appview_group(&state.db, source_id, 30, SortMode::New).await;
                for post in posts {
                    if seen_ids.insert(post.id.clone()) {
                        all_posts.push(post);
                    }
                }
            }
            "subscriptions" => {
                if let Ok(user_id) = extract_user_id(&headers, &state.config.jwt_secret) {
                    let posts =
                        sources::fetch_appview_subscriptions(&state.db, user_id, 50, SortMode::New).await;
                    for post in posts {
                        if seen_ids.insert(post.id.clone()) {
                            all_posts.push(post);
                        }
                    }
                }
            }
            "rss" => {}
            _ => {}
        }
    }

    all_posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    all_posts.truncate(200);

    if let Ok(user_id) = extract_user_id(&headers, &state.config.jwt_secret) {
        enrich_with_reactions(&state.db, &mut all_posts, user_id).await;
    }

    filter::apply_nsfw_policy(
        &mut all_posts,
        fetch_nsfw_pref(&state.db, &headers, &state.config.jwt_secret).await,
    );
    sources::fill_missing_profiles(&state.db, &mut all_posts).await;

    Ok(Json(FeedResponse {
        cursor: None,
        has_more: all_posts.len() >= 100,
        items: all_posts,
        phase: None,
        platform_counts: None,
    }))
}

/// GET /api/feed/groups?scope=home|following|feeds|hashtags|subscriptions
///
/// The Groups view's content — the never-blank groups population (owner fix:
/// "group content appears without joining anything"): group-linked posts
/// (`group_actor_id NOT NULL`) scoped by tab, with the default population
/// backing the default view and any empty scope (followed authors' group
/// posts ∪ interest topics ∪ interest hashtags, then the viewer's
/// hashtag/interest stream, then popular community content). Mirrors the
/// main feed's category tabs and the events feed's `source` scopes (owner
/// model: Groups is a specialized feed slice, interest-first, not
/// subscription-gated). Each post carries its `GroupRef` header + vote counts
/// so `GroupPostCard` renders identically wherever a group post appears.
#[derive(Deserialize, Clone)]
struct GroupScopeQuery {
    scope: Option<String>,
    limit: Option<i64>,
}

async fn group_scope_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<GroupScopeQuery>,
) -> Result<Json<FeedResponse>, AppError> {
    // Serve-first snapshots for the Groups view — the same staleness ladder
    // as the combined seed (feed/snapshots.rs), so a cold hit no longer
    // recomputes the scope query. Key: viewer + scope + limit + NSFW axis
    // (the viewer's reactions and follow/subscription context are baked
    // into the built payload).
    let viewer_nsfw_pref = fetch_nsfw_pref(&state.db, &headers, &state.config.jwt_secret).await;
    let user_id = optional_viewer(&headers, &state.config.jwt_secret)?;
    let scope = query.scope.as_deref().unwrap_or("home");
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let snap_key = format!(
        "grp:{}|s={}|l={}|n={}",
        user_id.map(|u| u.to_string()).unwrap_or_else(|| "-".to_string()),
        scope,
        limit,
        viewer_nsfw_pref.snapshot_axis(),
    );
    if let Some(snap) = snapshots::load(&state.db, &snap_key).await {
        if let Ok(resp) = serde_json::from_value::<FeedResponse>(snap.payload) {
            if snap.age_secs > snapshots::STALE_SECS {
                snapshots::spawn_rebuild_with(snap_key.clone(), {
                    let state = state.clone();
                    let headers = headers.clone();
                    let query = query.clone();
                    let key = snap_key.clone();
                    move || async move {
                        match group_scope_feed_build(&state, &headers, &query).await {
                            Ok(Json(resp)) => {
                                if let Ok(value) = serde_json::to_value(&resp) {
                                    snapshots::store(&state.db, &key, &value).await;
                                }
                                tracing::info!("group snapshot rebuilt for {}", key);
                            }
                            Err(e) => {
                                tracing::warn!("group snapshot rebuild failed for {}: {}", key, e)
                            }
                        }
                    }
                });
            }
            return Ok(Json(resp));
        }
    }
    let Json(resp) = group_scope_feed_build(&state, &headers, &query).await?;
    if let Ok(value) = serde_json::to_value(&resp) {
        snapshots::store(&state.db, &snap_key, &value).await;
    }
    Ok(Json(resp))
}

/// The Groups view's real computation — wrapped by [`group_scope_feed`]'s
/// serve-first snapshot layer (the rebuild path calls this directly and
/// stores the result itself).
async fn group_scope_feed_build(
    state: &AppState,
    headers: &HeaderMap,
    query: &GroupScopeQuery,
) -> Result<Json<FeedResponse>, AppError> {
    let viewer_nsfw_pref = fetch_nsfw_pref(&state.db, &headers, &state.config.jwt_secret).await;
    let user_id = optional_viewer(&headers, &state.config.jwt_secret)?;
    let scope = query.scope.as_deref().unwrap_or("home");
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let ctx = sources::GroupScopeCtx::resolve(&state.db, user_id).await;
    let mut posts = sources::fetch_appview_group_scope(&state.db, scope, &ctx, limit).await;
    if let Some(uid) = user_id {
        enrich_with_reactions(&state.db, &mut posts, uid).await;
    }
    filter::apply_nsfw_policy(&mut posts, viewer_nsfw_pref);
    sources::fill_missing_profiles(&state.db, &mut posts).await;
    // Fire-and-forget count refresh (item: stale votes): re-fetch the source's
    // real upvote/downvote/comment counts for the displayed posts so the next
    // load shows them. Never blocks this render.
    let uris: Vec<String> = posts.iter().map(|p| p.id.clone()).collect();
    let db = state.db.clone();
    tokio::spawn(async move {
        crate::ap::public_feed::refresh_counts_for_posts(&db, &uris).await;
    });
    Ok(Json(FeedResponse {
        cursor: None,
        has_more: posts.len() as i64 >= limit,
        items: posts,
        phase: None,
        platform_counts: None,
    }))
}

/// GET /api/bluesky/post?uri=at://did:plc:.../app.bsky.feed.post/...
async fn get_bluesky_post(
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let uri = params
        .get("uri")
        .ok_or_else(|| AppError::BadRequest("Missing uri parameter".into()))?;
    let url = format!(
        "https://api.bsky.app/xrpc/app.bsky.feed.getPosts?uris={}",
        urlencoding::encode(uri)
    );
    let http = feed_http();
    let resp = http
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Bluesky API error: {e}")))?;
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("JSON parse error: {e}")))?;
    Ok(Json(json))
}

// ---------------------------------------------------------------------------
// Discover inner
// ---------------------------------------------------------------------------
async fn discover_feed_inner(pool: &sqlx::PgPool) -> Result<Json<FeedResponse>, AppError> {
    let mut all_posts: Vec<FeedPost> = Vec::new();

    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            Option<String>,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<serde_json::Value>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        &format!(
            r#"SELECT uri, cid, author_did, text, created_at, embeds, author_handle, author_avatar, author_display_name
           FROM appview_posts
           WHERE text IS NOT NULL AND text != '' AND reply_parent IS NULL
                          AND created_at <= now()
             AND {}
           ORDER BY created_at DESC NULLS LAST
           LIMIT 30"#,
            filter::sql_labels_clean("labels"),
        ),
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    for (uri, cid, did, text, created, embeds, author_handle, author_avatar, author_display_name) in rows {
        let display_handle = author_handle
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let (media, embed_info) = match &embeds {
            Some(emb) => normalize::parse_bluesky_embeds(emb, &did),
            None => (vec![], vec![]),
        };

        all_posts.push(FeedPost {
            id: uri.clone(),
            source: normalize::source_from_uri(&uri).to_string(),
            author: AuthorInfo {
                handle: normalize::format_handle(&display_handle),
                display_name: author_display_name,
                avatar: author_avatar,
            },
            content: text.unwrap_or_default(),
            content_type: if !media.is_empty() {
                "image"
            } else {
                "short"
            }
            .to_string(),
            media,
            embeds: embed_info,
            likes_count: 0,
            shares_count: 0,
            replies_count: 0,
            created_at: created.map(|c| c.to_rfc3339()).unwrap_or_default(),
            url: Some(uri.clone()),
            thread_id: Some(format!("bluesky:{}", uri)),
            start_time: None,
            end_time: None,
            location: None,
            display_name: None,
            action_text: None,
            group_name: None, group: None, upvotes: None, downvotes: None, my_vote: None,
            reaction_counts: None,
            user_reaction: None,
            cid: Some(cid),
            author_url: Some(format!("https://bsky.app/profile/{}", did)),
            reposted_by: None,
            boosted_by: None,
            quoted_post_uri: None,
            is_reshare: Some(false),
            original_post: None,
            is_own: None,
            is_following: None,
            follow_pending: None,
            cross_posts: vec![],
            matched_sources: vec![],
            labels: vec![],
            nsfw: false,
            author_external_id: Some(did),
        });
    }

    all_posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    all_posts.truncate(50);
    sources::fill_missing_profiles(pool, &mut all_posts).await;

    Ok(Json(FeedResponse {
        cursor: None,
        has_more: false,
        items: all_posts,
        phase: None,
        platform_counts: None,
    }))
}

// ---------------------------------------------------------------------------
// Fetch helpers (preserved)
// ---------------------------------------------------------------------------

/// Cached home timeline: the personal tier refetches it on every feed build
/// otherwise (a remote round trip each time).
async fn fetch_bluesky_timeline(
    http: &reqwest::Client,
    acct: &crate::db::accounts::LinkedAccountRow,
) -> Result<Vec<FeedPost>, String> {
    static TIMELINE_CACHE: std::sync::LazyLock<
        parking_lot::Mutex<
            std::collections::HashMap<String, (std::time::Instant, Vec<FeedPost>)>,
        >,
    > = std::sync::LazyLock::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));
    const TTL: std::time::Duration = std::time::Duration::from_secs(120);
    {
        let map = TIMELINE_CACHE.lock();
        if let Some((at, posts)) = map.get(&acct.external_id) {
            if at.elapsed() < TTL {
                return Ok(posts.clone());
            }
        }
    }
    let result = fetch_bluesky_timeline_inner(http, acct).await;
    if let Ok(posts) = &result {
        let mut map = TIMELINE_CACHE.lock();
        map.retain(|_, (at, _)| at.elapsed() < TTL);
        map.insert(acct.external_id.clone(), (std::time::Instant::now(), posts.clone()));
    }
    result
}

async fn fetch_bluesky_timeline_inner(
    http: &reqwest::Client,
    acct: &crate::db::accounts::LinkedAccountRow,
) -> Result<Vec<FeedPost>, String> {
    let instance = crate::oauth::bluesky::get_pds_url(&acct.metadata);

    if let Some(dpop_hex) = &acct.dpop_private_key {
        if !dpop_hex.is_empty() {
            let dpop_bytes = hex::decode(dpop_hex).map_err(|e| format!("Invalid DPoP key: {}", e))?;
            let client =
                oauth::bluesky::BlueskyApiClient::new(&acct.access_token, &dpop_bytes, &instance)?;
            match client.get_timeline(100, None).await {
                Ok(result) => {
                    let mut posts = Vec::new();
                    if let Some(feed) = result["feed"].as_array() {
                        for item in feed {
                            if let Some(post) = normalize_bluesky_api_post(item) {
                                posts.push(post);
                            }
                        }
                    }
                    return Ok(posts);
                }
                Err(e) => {
                    tracing::error!(
                        "Bluesky DPoP timeline failed: {}; falling back to Bearer auth",
                        e
                    );
                }
            }
        }
    }

    let url = format!("{}/xrpc/app.bsky.feed.getTimeline?limit=100", instance);

    let resp = http
        .get(&url)
        .header(
            "Authorization",
            format!("Bearer {}", acct.access_token),
        )
        .send()
        .await
        .map_err(|e| format!("Bluesky timeline fetch failed: {}", e))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("Read failed: {}", e))?;
    if !status.is_success() {
        let status_code = status.as_u16();
        let end = text.len().min(300);
        tracing::error!("Bluesky timeline error ({}): {}", status_code, &text[..end]);
        if status_code == 401 || status_code == 403 {
            return Err(
                "Session expired — please re-link your Bluesky account in Settings.".into(),
            );
        }
        return Err(format!("Bluesky timeline error ({}): {}", status_code, text));
    }

    let result: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("JSON parse failed: {} — body: {}", e, text))?;

    let mut posts = Vec::new();
    if let Some(feed) = result["feed"].as_array() {
        for item in feed {
            if let Some(post) = normalize_bluesky_api_post(item) {
                posts.push(post);
            }
        }
    }
    Ok(posts)
}

// ---------------------------------------------------------------------------
// AppView search
// ---------------------------------------------------------------------------
/// Full-phase tag searches re-run on every view — cache by (term, limit, cursor).
async fn appview_search(
    pool: &sqlx::PgPool,
    query: &str,
    limit: i64,
    cursor: Option<(&str, &str)>,
    pref: NsfwPref,
) -> Vec<FeedPost> {
    // One cached batch per term; cursor pages slice from it. Per-cursor keys
    // defeated the cache on every scroll page (each page = a fresh scan set).
    let batch_limit = limit.max(60);
    let cache_key = format!("{}|{}|n{}", query, batch_limit, pref.snapshot_axis());
    let batch = match sources::cache_get(&sources::SEARCH_CACHE, &cache_key) {
        Some(hit) => hit,
        None => {
            let r = appview_search_uncached(pool, query, batch_limit, None, pref).await;
            if !r.is_empty() {
                sources::cache_put(&sources::SEARCH_CACHE, cache_key, r.clone());
            }
            r
        }
    };
    match cursor {
        Some((ca, uri)) => batch
            .into_iter()
            .filter(|p| (p.created_at.as_str(), p.id.as_str()) < (ca, uri))
            .take(limit.max(0) as usize)
            .collect(),
        None => batch.into_iter().take(limit.max(0) as usize).collect(),
    }
}

/// Batched variant of `appview_search`: all tags in ONE regex-any scan
/// instead of one SCAN_SLOTS-serialized table scan per tag. Cursor pages
/// slice from the cached batch, same as `appview_search`. The 4-arg form is the
/// `exclude` default kept for callers without viewer context (e.g. `sources.rs`);
/// prefer [`appview_search_multi_pref`] when the viewer's [`NsfwPref`] is known.
async fn appview_search_multi(
    pool: &sqlx::PgPool,
    queries: &[String],
    limit: i64,
    cursor: Option<(&str, &str)>,
) -> Vec<FeedPost> {
    appview_search_multi_pref(pool, queries, limit, cursor, NsfwPref::Exclude).await
}

async fn appview_search_multi_pref(
    pool: &sqlx::PgPool,
    queries: &[String],
    limit: i64,
    cursor: Option<(&str, &str)>,
    pref: NsfwPref,
) -> Vec<FeedPost> {
    if queries.is_empty() {
        return Vec::new();
    }
    let batch_limit = limit.max(60);
    let mut sorted = queries.to_vec();
    sorted.sort();
    let cache_key = format!("multi|{}|{}|n{}", sorted.join(","), batch_limit, pref.snapshot_axis());
    let batch = match sources::cache_get(&sources::SEARCH_CACHE, &cache_key) {
        Some(hit) => hit,
        None => {
            let r = appview_search_multi_uncached(pool, queries, batch_limit, pref).await;
            if !r.is_empty() {
                sources::cache_put(&sources::SEARCH_CACHE, cache_key, r.clone());
            }
            r
        }
    };
    match cursor {
        Some((ca, uri)) => batch
            .into_iter()
            .filter(|p| (p.created_at.as_str(), p.id.as_str()) < (ca, uri))
            .take(limit.max(0) as usize)
            .collect(),
        None => batch.into_iter().take(limit.max(0) as usize).collect(),
    }
}

async fn appview_search_multi_uncached(
    pool: &sqlx::PgPool,
    queries: &[String],
    limit: i64,
    pref: NsfwPref,
) -> Vec<FeedPost> {
    // Hashtag lookups go through the extracted-tag index
    // (appview_post_tags, migration 064) via per-tag bounded walks — the
    // same pattern `sources.rs` uses. The regex-over-appview_posts shapes
    // (even OR-expanded for the trigram index) fan out to table-scale
    // bitmap scans once a feed aggregates many tags; those scans ran for
    // hours on the 4M-row table and starved the whole DB pool.
    let tags: Vec<String> = queries
        .iter()
        .map(|q| q.trim().trim_start_matches('#').to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    if tags.is_empty() {
        return Vec::new();
    }

    // Per-tag `ORDER BY created_at DESC LIMIT n` walks the (tag,
    // created_at) index and stops at n; merge the small results in memory.
    let per_tag = ((limit / 4).max(8)) as i64;
    let mut candidates: Vec<(String, chrono::DateTime<chrono::Utc>)> = Vec::new();
    for batch in tags.chunks(32) {
        let futs = batch.iter().map(|tag| async move {
            sqlx::query_as::<_, (String, Option<chrono::DateTime<chrono::Utc>>)>(
                r#"SELECT uri, created_at FROM appview_post_tags
                   WHERE tag = $1
                   ORDER BY created_at DESC NULLS LAST
                   LIMIT $2"#,
            )
            .bind(tag)
            .bind(per_tag)
            .fetch_all(pool)
            .await
            .unwrap_or_default()
        });
        for rows in futures::future::join_all(futs).await {
            for (uri, created) in rows {
                candidates.push((uri, created.unwrap_or_default()));
            }
        }
    }
    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    candidates.dedup_by(|a, b| a.0 == b.0);
    let candidate_uris: Vec<String> = candidates
        .into_iter()
        .map(|(uri, _)| uri)
        .take((limit.max(0) as usize).saturating_mul(4).max(240))
        .collect();
    if candidate_uris.is_empty() {
        return Vec::new();
    }

    // Hydrate the matched URIs in one indexed lookup, preserving the
    // original row filters (no replies, clean labels).
    let hydrate_sql = format!(
        r#"SELECT uri, cid, author_did, author_handle, author_avatar, author_display_name, text, created_at, embeds, labels, like_count, repost_count, reply_count
           FROM appview_posts
           WHERE uri = ANY($1::text[])
             AND text IS NOT NULL AND text != ''
             AND reply_parent IS NULL
             AND created_at <= now()
             AND {}
           ORDER BY created_at DESC NULLS LAST
           LIMIT $2"#,
        filter::sql_labels_filter("labels", pref),
    );
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<serde_json::Value>,
            Option<serde_json::Value>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
        ),
    >(&hydrate_sql)
        .bind(&candidate_uris)
        .bind(limit)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    rows.into_iter()
        .filter_map(|(uri, cid, did, handle, avatar, display_name, text, created, embeds, labels, likes, reposts, replies)| {
            normalize::normalize_appview_post(
                uri,
                cid,
                did,
                handle,
                avatar,
                display_name,
                text,
                created,
                embeds,
                normalize::labels_from_json(&labels),
                likes.unwrap_or(0),
                reposts.unwrap_or(0),
                replies.unwrap_or(0),
            )
        })
        .collect()
}

async fn appview_search_uncached(
    pool: &sqlx::PgPool,
    query: &str,
    limit: i64,
    cursor: Option<(&str, &str)>,
    pref: NsfwPref,
) -> Vec<FeedPost> {
    let _permit = crate::api::feedgen::SCAN_SLOTS
        .acquire()
        .await
        .expect("semaphore closed");
    // Tag-index candidate walk (appview_post_tags, migration 064) instead
    // of a `text ~*` regex scan: the term is always matched as a hashtag
    // (the old regex wrapped it in `#…` boundaries), so the extracted-tag
    // index gives the same membership rule the combined feed's hashtag
    // tiers use — without the per-term whole-table regex scan.
    let tag_key = query.trim().trim_start_matches('#').to_lowercase();
    let candidates: Vec<String> = sqlx::query_as::<_, (String,)>(
        r#"SELECT uri FROM appview_post_tags
           WHERE tag = $1
           ORDER BY created_at DESC NULLS LAST
           LIMIT $2"#,
    )
    .bind(&tag_key)
    .bind((limit + 16).max(24))
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|(uri,)| uri)
    .collect();
    if candidates.is_empty() {
        return Vec::new();
    }

    let rows = if let Some((ca, uri)) = cursor {
        sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                Option<String>,
                Option<chrono::DateTime<chrono::Utc>>,
                Option<serde_json::Value>,
                Option<serde_json::Value>,
                Option<String>,
                Option<String>,
                Option<String>,
            ),
        >(
            &format!(
                r#"SELECT uri, cid, author_did, text, created_at, embeds, labels, author_handle, author_avatar, author_display_name
               FROM appview_posts
               WHERE uri = ANY($1::text[]) AND text IS NOT NULL AND text != ''
                 AND reply_parent IS NULL
                                  AND created_at <= now()
                 AND {}
                 AND (created_at, uri) < ($3::timestamptz, $4::text)
               ORDER BY created_at DESC NULLS LAST
               LIMIT $2"#,
                filter::sql_labels_filter("labels", pref),
            ),
        )
        .bind(&candidates)
        .bind(limit)
        .bind(ca)
        .bind(uri)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                Option<String>,
                Option<chrono::DateTime<chrono::Utc>>,
                Option<serde_json::Value>,
                Option<serde_json::Value>,
                Option<String>,
                Option<String>,
                Option<String>,
            ),
        >(
            &format!(
                r#"SELECT uri, cid, author_did, text, created_at, embeds, labels, author_handle, author_avatar, author_display_name
               FROM appview_posts
               WHERE uri = ANY($1::text[]) AND text IS NOT NULL AND text != ''
                 AND reply_parent IS NULL
                                  AND created_at <= now()
                 AND {}
               ORDER BY created_at DESC NULLS LAST
               LIMIT $2"#,
                filter::sql_labels_filter("labels", pref),
            ),
        )
        .bind(&candidates)
        .bind(limit)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
    };

    // NSFW text filtering in Rust — the single shared tag list, not a copy.
    let rows: Vec<_> = rows
        .into_iter()
        .filter(|row| {
            let text = &row.3;
            if let Some(t) = text {
                let lower = t.to_lowercase();
                !filter::NSFW_TAGS.iter().any(|tag| lower.contains(tag))
            } else {
                false
            }
        })
        .collect();

    let handle_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    rows.into_iter()
        .filter_map(
            |(uri, cid, did, text, created, embeds, labels, author_handle, author_avatar, author_display_name)| {
                let display_handle = author_handle
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .or_else(|| handle_map.get(&did).map(|s| s.clone()))
                    .unwrap_or_else(|| "unknown".to_string());

                let (media, embed_info) = match &embeds {
                    Some(emb) => normalize::parse_bluesky_embeds(emb, &did),
                    None => (vec![], vec![]),
                };

                Some(FeedPost {
                    id: uri.clone(),
                    source: normalize::source_from_uri(&uri).to_string(),
                    author: AuthorInfo {
                        handle: normalize::format_handle(&display_handle),
                        display_name: author_display_name,
                        avatar: author_avatar,
                    },
                    content: text.unwrap_or_default(),
                    content_type: if !media.is_empty() {
                        "image"
                    } else {
                        "short"
                    }
                    .to_string(),
                    media,
                    embeds: embed_info,
                    likes_count: 0,
                    shares_count: 0,
                    replies_count: 0,
                    created_at: created.map(|c| c.to_rfc3339()).unwrap_or_default(),
                    url: Some(uri.clone()),
                    thread_id: Some(format!("bluesky:{}", uri)),
                    start_time: None,
                    end_time: None,
                    location: None,
                    display_name: None,
                    action_text: None,
                    group_name: None, group: None, upvotes: None, downvotes: None, my_vote: None,
                    reaction_counts: None,
                    user_reaction: None,
                    cid: Some(cid),
                    author_url: Some(format!("https://bsky.app/profile/{}", did)),
                    reposted_by: None,
                    boosted_by: None,
                    quoted_post_uri: None,
                    is_reshare: Some(false),
                    original_post: None,
                    is_own: None,
                    is_following: None,
                    follow_pending: None,
                    cross_posts: vec![],
                    matched_sources: vec![],
                    labels: normalize::labels_from_json(&labels),
                    nsfw: false,
                    author_external_id: Some(did),
                })
            },
        )
        .collect()
}

async fn appview_author_feed(pool: &sqlx::PgPool, did: &str, limit: i64) -> Vec<FeedPost> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<String>,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<serde_json::Value>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        r#"SELECT uri, cid, text, created_at, embeds, author_handle, author_avatar, author_display_name
           FROM appview_posts
           WHERE author_did = $1 AND reply_parent IS NULL
           ORDER BY created_at DESC NULLS LAST
           LIMIT $2"#,
    )
    .bind(did)
    .bind(limit)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.into_iter()
        .filter_map(|(uri, cid, text, created, embeds, author_handle, author_avatar, author_display_name)| {
            let display_handle = author_handle
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| did.to_string());

            let (media, embed_info) = match &embeds {
                Some(emb) => normalize::parse_bluesky_embeds(emb, did),
                None => (vec![], vec![]),
            };

            Some(FeedPost {
                id: uri.clone(),
                source: normalize::source_from_uri(&uri).to_string(),
                author: AuthorInfo {
                    handle: normalize::format_handle(&display_handle),
                    display_name: author_display_name,
                    avatar: author_avatar,
                },
                content: text.unwrap_or_default(),
                content_type: if !media.is_empty() {
                    "image"
                } else {
                    "short"
                }
                .to_string(),
                media,
                embeds: embed_info,
                likes_count: 0,
                shares_count: 0,
                replies_count: 0,
                created_at: created.map(|c| c.to_rfc3339()).unwrap_or_default(),
                url: Some(uri.clone()),
                thread_id: Some(format!("bluesky:{}", uri)),
                start_time: None,
                end_time: None,
                location: None,
                display_name: None,
                action_text: None,
                group_name: None, group: None, upvotes: None, downvotes: None, my_vote: None,
                reaction_counts: None,
                user_reaction: None,
                cid: Some(cid),
                author_url: Some(format!("https://bsky.app/profile/{}", did)),
                reposted_by: None,
                boosted_by: None,
                quoted_post_uri: None,
                is_reshare: Some(false),
                original_post: None,
                is_own: None,
                is_following: None,
                follow_pending: None,
                cross_posts: vec![],
                matched_sources: vec![],
                labels: vec![],
                nsfw: false,
                author_external_id: Some(did.to_string()),
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Post comments
// ---------------------------------------------------------------------------
#[derive(Deserialize)]
struct CommentsQuery {
    post_id: String,
    source: String,
}

#[derive(Serialize)]
pub(crate) struct CommentItem {
    pub(crate) id: String,
    pub(crate) author_handle: String,
    pub(crate) author_avatar: Option<String>,
    pub(crate) content: String,
    pub(crate) created_at: String,
    pub(crate) parent_id: Option<String>,
    pub(crate) likes_count: u32,
    pub(crate) media: Vec<MediaAttachment>,
    pub(crate) embeds: Vec<EmbedInfo>,
    /// Source profile URL (AP actor URL or bsky DID profile) — the author
    /// name links here (owner: comment author must be clickable).
    pub(crate) author_url: Option<String>,
    /// Whether the viewer currently follows this comment's author (drives the
    /// Follow / ✓ Following button state).
    pub(crate) author_following: bool,
    /// Whether `author_handle` is a fully-resolved handle (true) or a temporary
    /// did-derived placeholder pending an async resolve (false). The frontend
    /// re-fetches the thread after a short delay when any are false, so a
    /// placeholder hydrates to the real handle (owner: "render fast with
    /// placeholder, then hydrate").
    pub(crate) author_resolved: bool,
}

/// Reply rows whose `reply_parent` is `post_id`, normalized to the comment
/// shape. Shared by the `at://` and `https://` (group/AP) comment paths.
async fn local_comment_rows(
    db: &crate::db::DbPool,
    post_id: &str,
) -> Result<Vec<CommentItem>, AppError> {
    let rows: Vec<(String, String, Option<String>, Option<String>, Option<String>, Option<String>, chrono::DateTime<chrono::Utc>, i64)> =
        sqlx::query_as(
            "SELECT uri, author_did, author_handle, author_display_name, author_avatar, text, created_at, like_count::bigint
             FROM appview_posts WHERE reply_parent = $1 ORDER BY created_at ASC LIMIT 200",
        )
        .bind(post_id)
        .fetch_all(db)
        .await?;
    let mut items: Vec<CommentItem> = rows
        .into_iter()
        .map(|(uri, did, handle, display, avatar, text, created, likes)| CommentItem {
            id: uri,
            author_handle: handle.or(display).unwrap_or_else(|| did.clone()),
            author_avatar: avatar,
            content: text.unwrap_or_default(),
            created_at: created.to_rfc3339(),
            parent_id: None,
            likes_count: likes as u32,
            media: vec![],
            embeds: vec![],
            author_url: author_url_from(&did),
            author_following: false,
            author_resolved: true,
        })
        .collect();
    hydrate_comment_authors(db, &mut items).await;
    Ok(items)
}

/// Resolve comment author identity the same way posts do (owner: comment
/// authors rendered as raw DIDs). `author_handle` holds the raw DID / AP actor
/// URL when the ingest couldn't derive a handle — resolve those from
/// `appview_did_handle` (AT DIDs) and `ap_remote_actors` (AP actor URLs).
/// Idempotent; anything already a human handle is left alone.
async fn hydrate_comment_authors(db: &crate::db::DbPool, items: &mut [CommentItem]) {
    // Handles needing resolution: empty, a raw `did:`, or a bare actor URL.
    let needs: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, it)| {
            let h = it.author_handle.as_str();
            h.is_empty() || h.starts_with("did:") || h.starts_with("http")
        })
        .map(|(i, _)| i)
        .collect();
    if needs.is_empty() {
        return;
    }
    // AT DIDs → appview_did_handle (did, handle, display_name, avatar).
    let dids: Vec<String> = needs
        .iter()
        .map(|&i| items[i].author_handle.clone())
        .filter(|h| h.starts_with("did:"))
        .collect();
    let mut did_map: std::collections::HashMap<String, (String, Option<String>, Option<String>)> =
        std::collections::HashMap::new();
    if !dids.is_empty() {
        if let Ok(rows) = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
            "SELECT did, handle, COALESCE(display_name, handle), avatar FROM appview_did_handle WHERE did = ANY($1)",
        )
        .bind(&dids)
        .fetch_all(db)
        .await
        {
            for (did, handle, display, avatar) in rows {
                did_map.insert(did, (handle, display, avatar));
            }
        }
    }
    // AP actor URLs → ap_remote_actors (actor_id, preferred_username,
    // display_name, avatar_url); handle = @user@host derived from the URL.
    let urls: Vec<String> = needs
        .iter()
        .map(|&i| items[i].author_handle.clone())
        .filter(|h| h.starts_with("http"))
        .collect();
    let mut ap_map: std::collections::HashMap<String, (String, Option<String>, Option<String>)> =
        std::collections::HashMap::new();
    if !urls.is_empty() {
        if let Ok(rows) = sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>)>(
            "SELECT actor_id, preferred_username, display_name, avatar_url FROM ap_remote_actors WHERE actor_id = ANY($1)",
        )
        .bind(&urls)
        .fetch_all(db)
        .await
        {
            for (actor_id, username, display, avatar) in rows {
                let handle = ap_handle_from_url(&actor_id, username.as_deref());
                ap_map.insert(actor_id, (handle, display, avatar));
            }
        }
    }
    for i in needs {
        let raw = items[i].author_handle.clone();
        let resolved: Option<&(String, Option<String>, Option<String>)> =
            if raw.starts_with("did:") {
                did_map.get(&raw)
            } else if raw.starts_with("http") {
                ap_map.get(&raw)
            } else {
                None
            };
        let (handle, display, avatar, resolved) = match resolved {
            Some((h, d, a)) => (h.clone(), d.clone(), a.clone(), true),
            // Fallback (owner bug: raw DIDs / AP URLs surfaced when the lookup
            // missed). Never leave a bare DID/URL — derive a handle-like label.
            None => fallback_identity(db, &raw).await,
        };
        items[i].author_resolved = resolved;
        items[i].author_handle = if handle.is_empty() {
            display.clone().unwrap_or_else(|| raw.clone())
        } else {
            handle
        };
        if items[i].author_avatar.is_none() {
            items[i].author_avatar = avatar;
        }
    }
}

/// Last-resort author identity when the maps miss: derive a human handle, never
/// a bare DID/URL. `did:` → `pds_accounts`/`users` (Bytescape did:web) then a
/// short `@name`; `http` → `@user@host` from the actor URL. For external
/// `did:plc` (and any did not resolvable locally) the real handle is resolved
/// ASYNCHRONOUSLY (AppView getProfile → cached in `appview_did_handle`) and a
/// short placeholder is returned now — the comment renders fast and hydrates
/// on the next load (owner: "comments render fast with placeholder, then
/// hydrate").
async fn fallback_identity(
    db: &crate::db::DbPool,
    raw: &str,
) -> (String, Option<String>, Option<String>, bool) {
    if raw.starts_with("http") {
        return (ap_handle_from_url(raw, None), None, None, true);
    }
    if raw.starts_with("did:") {
        // Bytescape minted/linked actors (did:web) live in pds_accounts — a
        // local, fast lookup.
        if let Ok(row) = sqlx::query_as::<_, (String, Option<String>,)>(
            "SELECT u.username, u.display_name FROM pds_accounts p \
             JOIN users u ON u.id = p.user_id WHERE p.did = $1",
        )
        .bind(raw)
        .fetch_optional(db)
        .await
        {
            if let Some((username, display)) = row {
                let handle = if username.is_empty() {
                    format!("@{}", raw.rsplit(':').next().unwrap_or("user"))
                } else {
                    format!("@{}", username.trim_start_matches('@'))
                };
                return (handle, display, None, true);
            }
        }
        // External did:plc / unresolvable locally — resolve the REAL handle in
        // the background (multi-source: DID document → PDS describeRepo →
        // AppView getProfile, cached into appview_did_handle) and return a
        // short placeholder now. Never blocks the comment render.
        spawn_resolve_did_handle(db.clone(), raw.to_string());
        let short = raw.rsplit(':').next().unwrap_or("user");
        let short: String = short.chars().take(12).collect();
        return (format!("@{}", short), None, None, false);
    }
    (raw.to_string(), None, None, true)
}

/// Spawn an async resolve of a `did:` author's real handle. Multi-source
/// (owner: "REAL handles" — the `@<suffix>` placeholder is only for genuinely
/// unresolvable DIDs) and cached into `appview_did_handle` so subsequent
/// comment loads are free. Also caches display name + avatar when the source
/// provides them.
fn spawn_resolve_did_handle(db: crate::db::DbPool, did: String) {
    tokio::spawn(async move {
        if let Some((handle, display, avatar)) = resolve_did_handle_remote(&did).await {
            let _ = sqlx::query(
                "INSERT INTO appview_did_handle (did, handle, display_name, avatar) \
                 VALUES ($1, $2, $3, $4) \
                 ON CONFLICT (did) DO UPDATE SET \
                   handle = EXCLUDED.handle, \
                   display_name = COALESCE(EXCLUDED.display_name, appview_did_handle.display_name), \
                   avatar = COALESCE(EXCLUDED.avatar, appview_did_handle.avatar), \
                   resolved_at = now()",
            )
            .bind(&did)
            .bind(&handle)
            .bind(&display)
            .bind(&avatar)
            .execute(&db)
            .await;
        }
    });
}

/// Resolve a `did:`'s REAL handle, first hit wins:
/// 1. the DID document — `https://plc.directory/{did}` for `did:plc`,
///    `/.well-known/did.json` for `did:web` — `alsoKnownAs` carries
///    `at://<handle>`, and the `PersonalDataServer` service endpoint locates
///    the PDS;
/// 2. `com.atproto.repo.describeRepo` on that PDS — the repo's own handle,
///    straight from the source;
/// 3. the Bluesky AppView `getProfile` (works for any live account; also the
///    only source for display name + avatar).
/// `None` = genuinely unresolvable (dead/withdrawn DID) — the caller keeps the
/// short `@<suffix>` placeholder for those.
async fn resolve_did_handle_remote(did: &str) -> Option<(String, Option<String>, Option<String>)> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .ok()?;
    // 1. DID document → alsoKnownAs handle + PDS endpoint.
    let doc_url = if let Some(rest) = did.strip_prefix("did:web:") {
        let mut parts = rest.split(':');
        let domain = parts.next()?;
        let path: Vec<&str> = parts.collect();
        if path.is_empty() {
            format!("https://{domain}/.well-known/did.json")
        } else {
            format!("https://{domain}/{}/.well-known/did.json", path.join("/"))
        }
    } else if did.starts_with("did:plc:") {
        format!("https://plc.directory/{did}")
    } else {
        String::new()
    };
    let mut aka_handle: Option<String> = None;
    let mut pds: Option<String> = None;
    if !doc_url.is_empty() {
        if let Ok(resp) = client.get(&doc_url).send().await {
            if let Ok(doc) = resp.json::<serde_json::Value>().await {
                aka_handle = doc
                    .get("alsoKnownAs")
                    .and_then(serde_json::Value::as_array)
                    .and_then(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .find_map(|s| s.strip_prefix("at://").map(str::to_string))
                    })
                    .filter(|h| !h.is_empty());
                pds = doc
                    .get("service")
                    .and_then(serde_json::Value::as_array)
                    .and_then(|arr| {
                        arr.iter()
                            .find(|s| {
                                s.get("type").and_then(|t| t.as_str()) == Some("PersonalDataServer")
                            })
                            .and_then(|s| s.get("serviceEndpoint"))
                            .and_then(|e| e.as_str())
                            .map(str::to_string)
                    });
            }
        }
    }
    if let Some(handle) = aka_handle {
        return Some((handle, None, None));
    }
    // 2. PDS describeRepo: did → handle at the source of truth.
    if let Some(pds) = pds.as_deref() {
        let url = format!(
            "{}/xrpc/com.atproto.repo.describeRepo?did={}",
            pds.trim_end_matches('/'),
            urlencoding::encode(did)
        );
        if let Ok(resp) = client.get(&url).send().await {
            if let Ok(v) = resp.json::<serde_json::Value>().await {
                if let Some(h) = v
                    .get("handle")
                    .and_then(|h| h.as_str())
                    .filter(|h| !h.is_empty())
                {
                    return Some((h.to_string(), None, None));
                }
            }
        }
    }
    // 3. AppView getProfile — last resort, and the display/avatar source.
    let url = format!(
        "https://api.bsky.app/xrpc/app.bsky.actor.getProfile?actor={}",
        did
    );
    let v: serde_json::Value = client.get(&url).send().await.ok()?.json().await.ok()?;
    let handle = v
        .get("handle")
        .and_then(|h| h.as_str())
        .filter(|h| !h.is_empty())?
        .to_string();
    let display = v
        .get("displayName")
        .and_then(|d| d.as_str())
        .filter(|d| !d.is_empty())
        .map(str::to_string);
    let avatar = v
        .get("avatar")
        .and_then(|a| a.as_str())
        .map(str::to_string);
    Some((handle, display, avatar))
}

/// Source profile URL for a comment author: an AP actor URL links directly; a
/// DID links to its bsky profile (owner: comment author must be clickable).
fn author_url_from(did: &str) -> Option<String> {
    if did.starts_with("http") {
        Some(did.to_string())
    } else if did.starts_with("did:") {
        Some(format!("https://bsky.app/profile/{}", did))
    } else {
        None
    }
}

/// Fill `author_following` (the viewer follows this comment's author) through
/// the SHARED matcher (`api/follow.rs::viewer_follow_state_map`) — a comment
/// author and a post author by the same actor must resolve identically; one
/// matcher, never a fork.
async fn hydrate_comment_follow_state(
    db: &crate::db::DbPool,
    user_id: uuid::Uuid,
    items: &mut [CommentItem],
) {
    let followed = crate::api::follow::viewer_follow_state_map(db, user_id).await;
    for it in items.iter_mut() {
        if let Some(u) = it.author_url.as_deref() {
            if followed.contains_key(u) {
                it.author_following = true;
            }
        }
    }
}

/// `@user@host` from an AP actor URL (`https://host/c/name`, `/u/name`,
/// `/users/name`, `/@name`), preferring the cached preferred_username.
fn ap_handle_from_url(actor_id: &str, username: Option<&str>) -> String {
    let host = actor_id
        .split("://")
        .nth(1)
        .and_then(|r| r.split('/').next())
        .unwrap_or("");
    let name = username
        .map(str::to_string)
        .unwrap_or_else(|| actor_id.rsplit('/').next().unwrap_or("").to_string());
    if host.is_empty() || name.is_empty() {
        return actor_id.to_string();
    }
    format!("@{}@{}", name, host)
}

/// One reply of a live AppView thread, in the shape a background durable
/// ingest needs (the renderable form is [`CommentItem`]).
struct ThreadReplyIngest {
    uri: String,
    cid: String,
    did: String,
    text: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// Live thread fetch from the public AppView (`app.bsky.feed.getPostThread`).
/// Returns the renderable top-level replies plus the raw rows for a durable
/// background ingest. Shared by the `source=bluesky` path and the at://
/// render-before-ingest fallback — one fetcher, one thread shape.
async fn fetch_bluesky_thread_items(
    post_id: &str,
) -> Result<(Vec<CommentItem>, Vec<ThreadReplyIngest>), AppError> {
    let at_uri = post_id.trim_start_matches("bluesky:");
    let encoded: String = url::form_urlencoded::byte_serialize(at_uri.as_bytes()).collect();
    let url = format!(
        "https://api.bsky.app/xrpc/app.bsky.feed.getPostThread?uri={}",
        encoded
    );

    let http = feed_http();
    let resp = http
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch Bluesky comments: {}", e)))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read response: {}", e)))?;
    if !status.is_success() {
        return Err(AppError::Internal(format!(
            "Bluesky API error ({}): {}",
            status.as_u16(),
            &text[..text.len().min(300)]
        )));
    }
    let result: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| AppError::Internal(format!("JSON parse failed: {}", e)))?;

    let mut comments = Vec::new();
    let mut ingest = Vec::new();
    if let Some(replies) = result["thread"]["replies"].as_array() {
        for reply in replies {
            let post = &reply["post"];
            let uri = post["uri"].as_str().unwrap_or("").to_string();
            if uri.is_empty() {
                continue;
            }
            // AppView "view"-shaped embeds (images, link cards,
            // videos, quotes) — previously discarded entirely.
            let (media, embeds) = normalize::parse_bluesky_view_embed(&post["embed"]);
            let text = post["record"]["text"].as_str().unwrap_or("").to_string();
            let created_raw = post["record"]["createdAt"].as_str().unwrap_or("");
            let did = post["author"]["did"].as_str().unwrap_or("").to_string();
            if !did.is_empty() {
                let created_at = chrono::DateTime::parse_from_rfc3339(created_raw)
                    .map(|t| t.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now());
                ingest.push(ThreadReplyIngest {
                    uri: uri.clone(),
                    cid: post["cid"].as_str().unwrap_or("").to_string(),
                    did,
                    text: text.clone(),
                    created_at,
                });
            }
            comments.push(CommentItem {
                id: uri,
                author_handle: post["author"]["handle"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string(),
                author_avatar: post["author"]["avatar"].as_str().map(|s| s.to_string()),
                content: text,
                created_at: created_raw.to_string(),
                parent_id: None,
                likes_count: post["likeCount"].as_u64().unwrap_or(0) as u32,
                media,
                embeds,
                author_url: post["author"]["did"]
                    .as_str()
                    .map(|d| format!("https://bsky.app/profile/{}", d)),
                author_following: false,
                author_resolved: true,
            });
        }
    }
    Ok((comments, ingest))
}

async fn post_comments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CommentsQuery>,
) -> Result<Json<Vec<CommentItem>>, AppError> {
    let viewer = optional_viewer(&headers, &state.config.jwt_secret)?;
    // Story/status cards AND group/AP posts: comments are reply rows whose
    // `reply_parent` is the post URI — indexed appview rows cover replies from
    // any app (AT or AP). For AP group posts, if no rows are indexed yet we
    // fetch the thread from the remote Lemmy/PieFed API and ingest it, so the
    // comment thread attaches live (design D5).
    if query.post_id.starts_with("at://") || query.post_id.starts_with("https://") {
        let mut rows = local_comment_rows(&state.db, &query.post_id).await?;
        // AP group post with no indexed comments yet → render-before-ingest
        // (owner item 2): fetch the thread and return it immediately; the
        // durable DB ingest runs in the background (no long spinner).
        if rows.is_empty() && query.post_id.starts_with("https://") {
            match crate::ap::public_feed::fetch_comment_items(&state.db, &query.post_id).await {
                Ok(items) => rows = items,
                Err(e) => {
                    tracing::debug!("[comments] remote thread fetch for {} failed: {e}", query.post_id)
                }
            }
        }
        // AT thread with no indexed replies yet — the same render-before-ingest
        // contract the AP branch has (owner bug: "comments never load" — the
        // thread UI rendered empty for every Bluesky post whose replies the
        // firehose hadn't indexed, while the card "loaded them fine" once any
        // surface had warmed the rows). Fetch the live AppView thread and
        // return it immediately; the durable ingest runs in the background.
        if rows.is_empty() && query.post_id.starts_with("at://") {
            match fetch_bluesky_thread_items(&query.post_id).await {
                Ok((items, ingest)) => {
                    rows = items;
                    if !ingest.is_empty() {
                        let db = state.db.clone();
                        let parent = query.post_id.clone();
                        tokio::spawn(async move {
                            for r in ingest {
                                if let Err(e) = crate::appview::consumer::upsert_appview_post(
                                    &db,
                                    &r.uri,
                                    &r.cid,
                                    &r.did,
                                    &r.text,
                                    Some(&parent),
                                    None,
                                    r.created_at,
                                )
                                .await
                                {
                                    tracing::debug!("[comments] thread ingest {} failed: {e}", r.uri);
                                }
                            }
                        });
                    }
                }
                Err(e) => {
                    tracing::debug!("[comments] AppView thread fetch for {} failed: {e}", query.post_id)
                }
            }
        }
        if let Some(uid) = viewer {
            hydrate_comment_follow_state(&state.db, uid, &mut rows).await;
        }
        return Ok(Json(rows));
    }
    match query.source.as_str() {
        "bluesky" => {
            let (comments, _) = fetch_bluesky_thread_items(&query.post_id).await?;
            Ok(Json(comments))
        }
        // Lemmy removed — Sprint 1
        _ => Ok(Json(vec![])),
    }
}

// ---------------------------------------------------------------------------
// Create reply (POST /api/feed/comments)
// ---------------------------------------------------------------------------
#[derive(Deserialize)]
struct CreateReplyReq {
    post_id: String,
    source: String,
    content: String,
    parent_reply_id: Option<String>,
}

async fn create_reply(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateReplyReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let accounts = crate::db::accounts::LinkedAccountRow::find_by_user(&state.db, user_id)
        .await
        .map_err(AppError::Database)?;

    match req.source.as_str() {
        s if req.post_id.starts_with("at://") || s == "story" => {
            // A story comment is a reply post whose parent is the story
            // record: write the record, self-index it, bump the story.
            let target = req.post_id.trim_start_matches("bluesky:").to_string();
            let parent = req
                .parent_reply_id
                .clone()
                .filter(|p| p.starts_with("at://"))
                .unwrap_or_else(|| target.clone());
            let id = crate::api::stories::at_identity_for(&state.db, user_id)
                .await?
                .ok_or_else(|| AppError::BadRequest("no AT identity linked to this account".into()))?;
            let did = match &id {
                crate::api::stories::AtId::Native(d) => d.clone(),
                crate::api::stories::AtId::Linked(a) => a.external_id.clone(),
            };
            let now = chrono::Utc::now();
            let record = serde_json::json!({
                "$type": "app.bsky.feed.post",
                "text": req.content,
                "createdAt": now.to_rfc3339(),
                "reply": {
                    "root": {"uri": target, "cid": ""},
                    "parent": {"uri": parent, "cid": ""}
                },
            });
            let rkey = {
                use rsky_common::tid::Ticker;
                Ticker::new().next(None).0
            };
            crate::api::stories::write_record(
                &state.db, &state, &id, "app.bsky.feed.post", &rkey, &record,
            )
            .await?;
            let uri = format!("at://{}/app.bsky.feed.post/{}", did, rkey);
            crate::appview::consumer::upsert_appview_post(
                &state.db,
                &uri,
                &crate::api::stories::record_cid(&record),
                &did,
                &req.content,
                Some(&parent),
                None,
                now,
            )
            .await?;
            crate::appview::consumer::bump_interaction_count(
                &state.db,
                &parent,
                crate::appview::consumer::CountKind::Reply,
            )
            .await;
            // Notify the story owner of the local reply (spec: local replies).
            if let Ok(Some((owner_id, _owner_name))) = crate::ap::notify::owner_of_at_record(&state.db, &parent).await {
                if owner_id != user_id {
                    // Provenance: the ACTOR of a reply notification is the
                    // replier, never the recipient (identity-graph P5 audit).
                    let replier: String = sqlx::query_scalar(
                        "SELECT username FROM users WHERE id = $1",
                    )
                    .bind(user_id)
                    .fetch_one(&state.db)
                    .await
                    .unwrap_or_else(|_| "someone".to_string());
                    let _ = crate::ap::notify::notify(
                        &state.db,
                        owner_id,
                        crate::ap::notify::TYPE_REPLY,
                        "New reply",
                        Some(&format!("@{} replied to your post", replier)),
                        Some(serde_json::json!({ "subject_uri": parent })),
                        &format!("local-reply:{}", uri),
                        &crate::ap::notify::ActorRef::local(&replier),
                    )
                    .await;
                }
            }
            Ok(Json(serde_json::json!({ "ok": true, "source": "story", "uri": uri })))
        }
        "bluesky" => {
            let acct = accounts
                .iter()
                .find(|a| a.provider == "bluesky")
                .ok_or_else(|| AppError::NotFound("No Bluesky account linked".into()))?;
            let pds = crate::oauth::bluesky::get_pds_url(&acct.metadata);
            let did = acct.metadata["did"].as_str().unwrap_or(&acct.external_id);
            let http = feed_http();

            let root_uri = req.post_id.trim_start_matches("bluesky:");
            let parent_uri = req.parent_reply_id.as_deref().unwrap_or(root_uri);

            // OAuth DPoP-bound tokens are rejected as plain Bearer; build a
            // DPoP-aware client and post the reply with a proof.
            let dpop_seed = acct.dpop_private_key.as_deref().ok_or_else(|| {
                AppError::Internal("No DPoP key stored for Bluesky account".into())
            })?;
            let dpop_bytes = hex::decode(dpop_seed)
                .map_err(|e| AppError::Internal(format!("Bad DPoP key: {}", e)))?;

            // Bluesky OAuth access tokens expire (~1h). Refresh a stale token
            // before use, otherwise we present an expired token and get 401.
            let access_token = {
                use base64::Engine as _;
                let now = chrono::Utc::now().timestamp();
                let expired = acct.access_token.split('.').nth(1)
                    .and_then(|p| {
                        let pad = (4 - p.len() % 4) % 4;
                        let b64 = format!("{}{}", p, "=".repeat(pad));
                        let bytes = base64::engine::general_purpose::STANDARD
                            .decode(b64.replace('_', "/").replace('-', "+"))
                            .ok()?;
                        serde_json::from_slice::<serde_json::Value>(&bytes).ok()
                    })
                    .and_then(|v| v["exp"].as_i64())
                    .map(|exp| now >= exp - 600) // refresh within 10 min of expiry
                    .unwrap_or(true);
                if expired {
                    crate::oauth::bluesky::refresh_session(&http, &state.db, acct).await
                        .map_err(|e| AppError::Internal(format!("Token refresh failed: {}", e)))?
                } else {
                    acct.access_token.clone()
                }
            };

            let client = crate::oauth::bluesky::BlueskyApiClient::new(
                &access_token,
                &dpop_bytes,
                &pds,
            )
            .map_err(|e| AppError::Internal(format!("Failed to build Bluesky client: {}", e)))?;

            match client
                .create_reply_dpop(did, root_uri, parent_uri, &req.content)
                .await
            {
                Ok(uri) => Ok(Json(serde_json::json!({ "ok": true, "source": "bluesky", "uri": uri }))),
                Err(e) => Err(AppError::Internal(format!("Bluesky reply failed: {}", e))),
            }
        }
        s if req.post_id.starts_with("https://") => {
            // Lemmy/PieFed (any AP group post): a federated Create(Note) reply
            // through the AP bridge — the same proven delivery path as
            // ui_like. Nested replies address the parent comment; top-level
            // replies address the post.
            let parent = req
                .parent_reply_id
                .clone()
                .filter(|p| p.starts_with("https://"))
                .unwrap_or_else(|| req.post_id.clone());
            provision_ap_actor(&state, user_id).await?;
            let id = crate::ap::bridge::ui_comment(
                &state.db,
                user_id,
                &req.post_id,
                if parent != req.post_id {
                    Some(parent.as_str())
                } else {
                    None
                },
                &req.content,
            )
            .await
            .map_err(|e| AppError::Internal(format!("AP comment failed: {e}")))?;
            // Counts reconcile from the source on view (refresh-on-view) —
            // fire-and-forget so the reply response is never blocked.
            let db = state.db.clone();
            let uri = req.post_id.clone();
            tokio::spawn(async move {
                crate::ap::public_feed::refresh_counts_for_posts(
                    &db,
                    std::slice::from_ref(&uri),
                )
                .await;
            });
            Ok(Json(serde_json::json!({ "ok": true, "source": "ap", "uri": id })))
        }
        _ => Err(AppError::BadRequest(format!(
            "Unsupported source: {}",
            req.source
        ))),
    }
}

// ---------------------------------------------------------------------------
// Post reactions
// ---------------------------------------------------------------------------
const POSITIVE_REACTIONS: &[&str] = &["❤️", "\u{2764}", "♥", "👍", "🔥", "😄", "😂", "😮"];
const NEGATIVE_REACTIONS: &[&str] = &["😢", "😡", "👎", "👎🏻", "👎🏼", "👎🏽", "👎🏾", "👎🏿"];

#[derive(Deserialize)]
struct ReactionReq {
    post_id: String,
    source: String,
    reaction: String,
    url: Option<String>,
    cid: Option<String>,
    /// The `user_reaction` this server last reported to the client (`""` =
    /// none seen). Echoed back so a repeat click / stale unlike — byte-
    /// identical to a genuine toggle — is recognized as a toggle-OFF intent
    /// instead of re-federating a fresh Like. Absent = legacy toggle
    /// semantics (payload-derived) for pre-contract clients.
    seen_user_reaction: Option<String>,
}

/// How long a just-retracted federated Like keeps shadowing its object for
/// echo-less (legacy) clients: the double-click race's trailing request lands
/// within this window, so a same-payload request in it can only be that
/// trailing click — never a genuine fresh like-on.
const REPEAT_CLICK_WINDOW_SQL: &str = "now() - interval '30 seconds'";

/// Did this user's federated reaction of `post_id` retract within the
/// repeat-click window? Two shadows make this identity-agnostic (CR-2/CR-10):
/// `ap_activities.undone_at` for the minted-AP cohort, and the reactions
/// table's retraction shadow (`retracted_at`, migration 092) for every
/// cohort — the home-REST cohort writes zero ap_activities rows and at://
/// reactions have no ap_activities shadow at all, so without the shadow rows
/// the window was blind to exactly the users it exists to protect.
pub(crate) async fn recently_retracted(db: &sqlx::PgPool, user_id: Uuid, post_id: &str) -> Result<bool, AppError> {
    let hit: Option<(String,)> = sqlx::query_as(&format!(
        "SELECT id FROM ap_activities \
         WHERE user_id = $1 AND object_id = $2 AND activity_type = 'Like' \
           AND undone_at > {REPEAT_CLICK_WINDOW_SQL} LIMIT 1"
    ))
    .bind(user_id)
    .bind(post_id)
    .fetch_optional(db)
    .await?;
    if hit.is_some() {
        return Ok(true);
    }
    Ok(crate::db::reactions::ReactionRow::recently_retracted(db, user_id, post_id).await?)
}

async fn post_reaction(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ReactionReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let is_negative = NEGATIVE_REACTIONS.contains(&req.reaction.as_str());
    // Set on toggle-off: whether the federated unreact was dispatched.
    let mut undo_queued: Option<bool> = None;
    let existing = crate::db::reactions::ReactionRow::find(&state.db, &req.post_id, user_id)
        .await?;

    // Toggle intent comes from what the client last SAW (its `seen_user_
    // reaction` echo of this server's truth), not from the request payload
    // alone: a repeat click and a stale unlike arrive byte-identical to a
    // genuine toggle and mean the OPPOSITE of a fresh click. `seen == clicked`
    // → the click was meant to remove that exact reaction. Legacy clients
    // (field absent) keep the payload-derived toggle.
    let toggle_off = match req.seen_user_reaction.as_deref() {
        Some(seen) => seen == req.reaction,
        None => match &existing {
            Some(row) => row.emoji == req.reaction,
            // Echo-less (legacy) client with nothing standing: the payload is
            // equally a fresh like-on and the trailing half of a double-click
            // race (both clicks carry the same toggle; the first already
            // removed the row — proven live to RE-FEDERATE the favourite).
            // Rapid repeats must collapse to one action: if a federated Like
            // of this object retracted within the repeat-click window, this
            // can only be that trailing click — never a fresh Like from it.
            // (A contract client sends `seen` and is exact above; a genuine
            // legacy re-like outside the window federates as usual. Negative
            // emojis never federate, so their local rows are unaffected.)
            None => !is_negative
                && recently_retracted(&state.db, user_id, &req.post_id).await?,
        },
    };
    let standing_same = existing.as_ref().is_some_and(|row| row.emoji == req.reaction);

    if toggle_off && standing_same {
        // Toggle-off: surface whether the unreact actually dispatched
        // (false = "nothing queued / failed to resolve"), instead of
        // silent success.
        undo_queued = Some(
            perform_platform_unreact(
                req.source.as_str(),
                &state,
                &req.post_id,
                is_negative,
                req.url.as_deref(),
                user_id,
            )
            .await?,
        );
        crate::db::reactions::ReactionRow::delete(&state.db, &req.post_id, user_id).await?;
    } else if !toggle_off && !standing_same {
        // Toggle-on or emoji change (one Like stands regardless of which
        // positive emoji is shown).
        let existing_rkey = existing
            .as_ref()
            .and_then(|row| row.metadata["bluesky_like_rkey"].as_str());
        let result = sync_federated_like(
            req.source.as_str(),
            &state,
            &req.post_id,
            is_negative,
            user_id,
            req.cid.as_deref(),
            existing_rkey,
        )
        .await?;
        crate::db::reactions::ReactionRow::upsert(
            &state.db,
            &req.post_id,
            user_id,
            &req.reaction,
        )
        .await?;
        if let Some((rkey, identity)) = result {
            // Home-REST rows also record the resolved home status id in its
            // own slot: `home_rest_unreact` prefers it over the record-id slot
            // (the 089 migration's field for legacy rows).
            let mut meta = serde_json::json!({"bluesky_like_rkey": rkey, "like_identity": identity});
            if identity == "home-rest" {
                meta["home_status_id"] = rkey.clone().into();
            }
            crate::db::reactions::update_metadata(&state.db, user_id, &req.post_id, "bluesky", meta)
                .await?;
        }
    }
    // Remaining combinations are idempotent no-ops by contract, and their
    // absence is the double-federation bug this handler exists to kill:
    // * toggle-off with nothing matching standing (repeat click / stale
    //   unlike after the first click already landed) — NEVER re-federate a
    //   fresh Like from an unlike;
    // * like/change intent on an identical standing reaction (stale "none
    //   seen" client) — never re-federate or remove what already stands.
    // The response reports the standing truth either way.

    let post_ids = vec![req.post_id.clone()];
    let counts = crate::db::reactions::ReactionRow::counts_for_posts(&state.db, &post_ids)
        .await
        .unwrap_or_default();
    let user_reactions = crate::db::reactions::ReactionRow::user_reactions_for_posts(
        &state.db,
        &post_ids,
        user_id,
    )
    .await
    .unwrap_or_default();

    Ok(Json(serde_json::json!({
        "ok": true,
        "reaction": req.reaction,
        "reaction_counts": counts.get(&req.post_id).cloned().unwrap_or_default(),
        "user_reaction": user_reactions.get(&req.post_id).cloned(),
        "undo_queued": undo_queued
    })))
}

/// Which protocol owns a reaction target — derived from the object's own URI
/// (`at://` → AT Protocol, `http(s)://` → ActivityPub), never from a client
/// label. `source` survives only as a fallback for legacy non-URI ids. This
/// kills the whole "unsupported reaction source" failure class at every
/// surface (feed, events) regardless of what a client sends.
fn reaction_target(post_id: &str, source: &str) -> Option<ReactionTarget> {
    if post_id.starts_with("at://") {
        return Some(ReactionTarget::At);
    }
    if post_id.starts_with("http://") || post_id.starts_with("https://") {
        return Some(ReactionTarget::Ap);
    }
    // Legacy ids without a URI scheme: fall back to the client's label.
    match source.split('|').next().unwrap_or(source).trim() {
        "ap" | "activitypub" | "mastodon" | "lemmy" | "pixelfed" | "misskey" | "fediverse" => {
            Some(ReactionTarget::Ap)
        }
        "bluesky" | "bsky" | "bytescape" => Some(ReactionTarget::At),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ReactionTarget {
    At,
    Ap,
}

/// Is this emoji a "not a Like" reaction (locally recorded, never federated)?
pub(crate) fn is_negative_reaction(emoji: &str) -> bool {
    NEGATIVE_REACTIONS.contains(&emoji)
}

/// The user's linked Bluesky account — their real identity on the AT network.
async fn linked_bluesky_account(
    db: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<Option<db::accounts::LinkedAccountRow>, AppError> {
    Ok(db::accounts::LinkedAccountRow::find_by_user(db, user_id)
        .await?
        .into_iter()
        .find(|a| a.provider == "bluesky"))
}

/// The user's linked Mastodon account — under the realignment (Decision 1,
/// 2026-11-14) their AP identity IS their home-instance actor: every
/// `https://` like/unlike dispatches through its REST API as that actor
/// ("one account, one person"), never through a minted second actor.
pub(crate) async fn linked_mastodon_account(
    db: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<Option<db::accounts::LinkedAccountRow>, AppError> {
    Ok(db::accounts::LinkedAccountRow::find_by_user(db, user_id)
        .await?
        .into_iter()
        .find(|a| a.provider == "mastodon"))
}

/// Home-REST failures reach the API surface with their cause intact.
/// `ReauthRequired` is the visible "re-link your Mastodon account" error
/// (tasks.md 1.2): a stale read-only grant fails LOUDLY — a silent fallback
/// to another identity is forbidden (it would resurrect the second-identity
/// problem this change exists to kill).
pub(crate) fn mastodon_error(e: oauth::mastodon::MastodonApiError) -> AppError {
    use oauth::mastodon::MastodonApiError as E;
    match e {
        E::ReauthRequired(msg) => AppError::Forbidden(msg),
        E::NotFound(msg) => AppError::NotFound(msg),
        E::Remote(msg) => AppError::OAuth(msg),
    }
}

/// Favourite `post_id` as the user's REAL home-instance actor (Decision 1).
/// Returns `(home status id, "home-rest")` — the home status id rides in the
/// `bluesky_like_rkey` record-id slot like every other reaction identity's
/// record id, so [`perform_platform_unreact`] undoes this the same way it
/// undoes linked/native Bluesky likes: on the identity that created it.
async fn home_rest_react(
    state: &AppState,
    user_id: Uuid,
    acct: &db::accounts::LinkedAccountRow,
    post_id: &str,
) -> Result<Option<(String, &'static str)>, AppError> {
    oauth::mastodon::ensure_favourites_scope(acct.scope.as_deref()).map_err(mastodon_error)?;
    let instance = home_instance_of(acct)?;
    let http = feed_http();
    let token = oauth::mastodon::refresh_mastodon_token(&state.db, &http, acct)
        .await
        .map_err(mastodon_error)?;
    let status_id = oauth::mastodon::resolve_status_id(&http, &instance, &token, post_id)
        .await
        .map_err(|e| {
            tracing::warn!(
                action = "home-REST react (resolve)",
                instance = %instance,
                post_id,
                error = %e,
                "home-instance adapter error"
            );
            mastodon_error(e)
        })?;
    oauth::mastodon::favourite_status(&http, &instance, &token, &status_id)
        .await
        .map_err(|e| {
            tracing::warn!(
                action = "home-REST react (favourite)",
                instance = %instance,
                post_id,
                error = %e,
                "home-instance adapter error"
            );
            mastodon_error(e)
        })?;
    let _ = user_id;
    Ok(Some((status_id, "home-rest")))
}

/// Unfavourite as the home-instance actor — dispatched on the recorded
/// `like_identity: "home-rest"` hint. `home_status_id` (the 089 metadata
/// migration's field for legacy rows) wins over the record-id slot, which
/// carries the home status id for rows this codepath created. A stored id
/// that is not numeric — e.g. the minted-actor AP activity URL the record-id
/// slot carries on 089-migrated rows after 090 nulls `home_status_id` (CR-3)
/// — is NEVER POSTed (a garbage unfavourite URL 404s and sticks the reaction
/// forever): the id is re-resolved from the post URI via the resolver chain
/// instead. And when nothing can be undone this WARN-logs and reports
/// nothing-to-undo — never a silent no-op and never a stuck row (CR-9).
async fn home_rest_unreact(
    db: &sqlx::PgPool,
    user_id: Uuid,
    row: &db::reactions::ReactionRow,
) -> Result<bool, AppError> {
    use oauth::mastodon::MastodonApiError as E;
    // Stored ids must be bare numeric status ids (the fallback slots can carry
    // an AP activity URL — never a valid home status id).
    let stored_id = ["home_status_id", "bluesky_like_rkey"]
        .iter()
        .filter_map(|k| row.metadata[*k].as_str())
        .find(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        .map(str::to_string);
    let Some(acct) = linked_mastodon_account(db, user_id).await? else {
        tracing::warn!(
            action = "home-REST unreact",
            post_id = %row.post_id,
            "home-instance adapter skipped: no linked Mastodon account — nothing can act as that actor"
        );
        return Ok(false);
    };
    oauth::mastodon::ensure_favourites_scope(acct.scope.as_deref()).map_err(mastodon_error)?;
    let instance = home_instance_of(&acct)?;
    let http = feed_http();
    let token = oauth::mastodon::refresh_mastodon_token(db, &http, &acct)
        .await
        .map_err(mastodon_error)?;
    let status_id = match stored_id {
        Some(id) => id,
        // CR-3: no trustworthy stored id — re-resolve from the post URI (the
        // lookup/search chain) instead of POSTing whatever the metadata slots
        // carry.
        None => {
            match oauth::mastodon::resolve_status_id(&http, &instance, &token, &row.post_id).await {
                Ok(id) => id,
                Err(E::NotFound(e)) => {
                    tracing::warn!(
                        action = "home-REST unreact (resolve)",
                        instance = %instance,
                        post_id = %row.post_id,
                        error = %e,
                        "status not resolvable on the home instance — nothing to undo"
                    );
                    return Ok(false);
                }
                Err(e) => {
                    tracing::warn!(
                        action = "home-REST unreact (resolve)",
                        instance = %instance,
                        post_id = %row.post_id,
                        error = %e,
                        "home-instance adapter error"
                    );
                    return Err(mastodon_error(e));
                }
            }
        }
    };
    match oauth::mastodon::unfavourite_status(&http, &instance, &token, &status_id).await {
        Ok(_) => Ok(true),
        // Already gone on the home instance (status purged / never favourited):
        // there is nothing to undo — report it instead of sticking the row.
        Err(E::NotFound(e)) => {
            tracing::warn!(
                action = "home-REST unreact (unfavourite)",
                instance = %instance,
                post_id = %row.post_id,
                error = %e,
                "nothing to undo on the home instance"
            );
            Ok(false)
        }
        Err(e) => {
            tracing::warn!(
                action = "home-REST unreact (unfavourite)",
                instance = %instance,
                post_id = %row.post_id,
                error = %e,
                "home-instance adapter error"
            );
            Err(mastodon_error(e))
        }
    }
}

/// Death-path sweep (immediate delete + grace expiry): retract every standing
/// home-REST favourite of the user AS ITS HOME ACTOR before the account (and
/// its link tokens) go away — the AP retraction routine
/// ([`crate::ap::retire::stage_actor_retirement`]) never covers these: they
/// live on the home instance under the home actor, not in `ap_activities`.
/// Best-effort per row (spec: "recorded, never blocks deletion"); returns how
/// many remote favourites were actually removed.
pub(crate) async fn retract_home_rest_reactions(
    db: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<usize, AppError> {
    let mut retracted = 0usize;
    let rows = db::reactions::ReactionRow::all_for_user(db, user_id).await?;
    for row in rows {
        if row.metadata["like_identity"].as_str() != Some("home-rest") {
            continue; // AP/linked/native rows are retracted on their own identity
        }
        match home_rest_unreact(db, user_id, &row).await {
            Ok(true) => retracted += 1,
            Ok(false) => {} // nothing standing (no status id / no link)
            Err(e) => {
                tracing::warn!(
                    "[delete] home-REST retraction of reaction {} failed: {e} — continuing",
                    row.post_id
                );
            }
        }
    }
    Ok(retracted)
}

/// The home instance recorded at link time (`metadata.instance`).
pub(crate) fn home_instance_of(acct: &db::accounts::LinkedAccountRow) -> Result<String, AppError> {
    acct.metadata
        .get("instance")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| AppError::BadRequest("the Mastodon link has no home instance recorded".into()))
}

/// The DID of the user's minted Bytescape PDS account, if any.
async fn minted_pds_row(db: &sqlx::PgPool, user_id: Uuid) -> Result<Option<String>, AppError> {
    let native: Option<(String,)> = sqlx::query_as("SELECT did FROM pds_accounts WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    Ok(native.map(|(did,)| did))
}

/// The user's minted Bytescape PDS DID — created on demand (same posture as
/// signup) so every account shape owns AT records. On-demand minting is the
/// permanent fix for "no AT identity linked to this account".
async fn minted_pds_did(state: &AppState, user_id: Uuid) -> Result<String, AppError> {
    if let Some(did) = minted_pds_row(&state.db, user_id).await? {
        return Ok(did);
    }
    let user = db::users::UserRow::find_by_id(&state.db, user_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("no such user".into()))?;
    let acct = crate::pds::account::PdsAccount::create(
        &state.db,
        &user.username,
        &state.config.domain,
        Some(user_id),
        &state.config.plc_directory,
    )
    .await
    .map_err(|e| AppError::Internal(format!("PDS account mint failed: {e}")))?;
    tracing::info!("[react] minted on-demand PDS account {}", acct.did);
    Ok(acct.did)
}

/// Issue the user's AP actor on demand — signup-time issuance is warn-only, so
/// accounts whose issuance failed reach reactions without one. Idempotent.
/// `pub(crate)`: the groups surface (`api::groups`) mints an actor before a
/// federated vote the same way the reaction surface does.
pub(crate) async fn provision_ap_actor(state: &AppState, user_id: Uuid) -> Result<(), AppError> {
    if !crate::ap::ap_actors_enabled() {
        return Err(AppError::BadRequest("ActivityPub is disabled".into()));
    }
    let user = db::users::UserRow::find_by_id(&state.db, user_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("no such user".into()))?;
    if user.ap_enabled && user.ap_actor_id.is_some() {
        return Ok(());
    }
    crate::ap::issue::issue_actor_for_user(state, user_id)
        .await
        .map_err(|e| AppError::Internal(format!("AP actor issuance failed: {e}")))?;
    Ok(())
}

/// Converge the federated side of a reaction to the local state: a positive
/// reaction earns exactly ONE platform Like regardless of emoji changes;
/// switching to a negative emoji (or clearing the reaction) undoes it.
/// `existing_rkey` is the metadata `bluesky_like_rkey` on the current row, if
/// any — its presence is what "already federated" means. Returns the new
/// `(rkey, identity)` when a Like was just created, for the caller to persist.
pub(crate) async fn sync_federated_like(
    source: &str,
    state: &AppState,
    post_id: &str,
    is_negative: bool,
    user_id: Uuid,
    cid: Option<&str>,
    existing_rkey: Option<&str>,
) -> Result<Option<(String, &'static str)>, AppError> {
    match (is_negative, existing_rkey) {
        (false, None) => {
            perform_platform_react(source, state, post_id, false, None, user_id, cid).await
        }
        (true, Some(_)) => {
            // Negative flip with a standing Like: undo it and clear the hint
            // so a later removal doesn't double-undo.
            perform_platform_unreact(source, state, post_id, false, None, user_id).await?;
            crate::db::reactions::update_metadata(
                &state.db,
                user_id,
                post_id,
                "bluesky",
                serde_json::json!({"bluesky_like_rkey": null, "like_identity": null}),
            )
            .await?;
            Ok(None)
        }
        // Already converged: (positive, standing Like) or (negative, none).
        _ => Ok(None),
    }
}

/// Federate a reaction on `post_id`. Routing is by the target's own URI scheme
/// ([`reaction_target`]); identity provisioning is on demand so every account
/// shape can react. Returns `(record id, identity)` where `identity` names
/// where the record lives — `"ap"` (AP actor), `"linked"` (the user's real
/// Bluesky account), `"native"` (their minted Bytescape PDS) — so
/// [`perform_platform_unreact`] undoes it on the same identity.
pub(crate) async fn perform_platform_react(
    source: &str,
    state: &AppState,
    post_id: &str,
    is_negative: bool,
    _status_url: Option<&str>,
    user_id: Uuid,
    cid: Option<&str>,
) -> Result<Option<(String, &'static str)>, AppError> {
    let target = reaction_target(post_id, source)
        .ok_or_else(|| AppError::BadRequest(format!("unsupported reaction source: {source}")))?;
    if is_negative {
        // Negative reactions are local-only: never a Like on either protocol.
        return Ok(None);
    }
    match target {
        ReactionTarget::Ap => {
            // Realignment Decision 1 (2026-11-14): a Mastodon signup acts AS
            // its real home-instance actor via the home REST API — clean
            // cutover, no minted-actor path for this cohort. Stale read-only
            // grants fail loudly with a re-link error (1.2), never silently.
            if let Some(acct) = linked_mastodon_account(&state.db, user_id).await? {
                return home_rest_react(state, user_id, &acct, post_id).await;
            }
            // Bluesky/email signups: the minted AP actor IS their fediverse
            // presence — issue it on demand if signup-era issuance never ran
            // (warn-only there).
            provision_ap_actor(state, user_id).await?;
            let id = crate::ap::bridge::ui_like(&state.db, user_id, post_id)
                .await
                .map_err(|e| AppError::Internal(format!("AP like failed: {e}")))?;
            Ok(Some((id, "ap")))
        }
        ReactionTarget::At => {
            let cid = match crate::oauth::bluesky::canonical_cid(cid.unwrap_or("")) {
                Some(c) => c,
                None => crate::oauth::bluesky::resolve_record_cid(post_id)
                    .await
                    .ok_or_else(|| AppError::BadRequest("the post CID is unavailable".into()))?,
            };
            // Prefer the user's real Bluesky account — the like is instantly
            // visible on the real network under their real identity. Fall
            // back to their minted Bytescape PDS identity (created on demand),
            // the only option for accounts without a Bluesky link.
            if let Some(acct) = linked_bluesky_account(&state.db, user_id).await? {
                match oauth::bluesky::like_post(&state.db, &acct, post_id, &cid).await {
                    Ok(rkey) => return Ok(Some((rkey, "linked"))),
                    Err(e) => tracing::warn!(
                        "[react] linked Bluesky like failed ({e}); falling back to minted identity"
                    ),
                }
            }
            let did = minted_pds_did(state, user_id).await?;
            let rkey = {
                use rsky_common::tid::Ticker;
                Ticker::new().next(None).0
            };
            let record = serde_json::json!({
                "$type": "app.bsky.feed.like",
                "subject": {"uri": post_id, "cid": cid},
                "createdAt": chrono::Utc::now().to_rfc3339(),
            });
            crate::pds::create_record_internal(
                &state.db,
                &did,
                "app.bsky.feed.like",
                Some(&rkey),
                &record,
                Some(&state.config.domain),
            )
            .await?;
            Ok(Some((rkey, "native")))
        }
    }
}

/// Undo the federated reaction recorded by [`perform_platform_react`] — routed
/// by URI scheme and dispatched to the *same* identity that created it (the
/// `like_identity` hint in the reaction metadata). Idempotent: no recorded
/// reaction is a no-op, and a missing identity can never block removal.
/// Returns whether a federated undo was actually dispatched (false = nothing
/// queued / nothing to resolve).
pub(crate) async fn perform_platform_unreact(
    source: &str,
    state: &AppState,
    post_id: &str,
    is_negative: bool,
    _status_url: Option<&str>,
    user_id: Uuid,
) -> Result<bool, AppError> {
    let target = reaction_target(post_id, source)
        .ok_or_else(|| AppError::BadRequest(format!("unsupported reaction source: {source}")))?;
    if is_negative {
        return Ok(false);
    }
    match target {
        ReactionTarget::Ap => {
            // Dispatch on the identity that created the reaction (the
            // `like_identity` hint) — home-REST first, so a Mastodon signup's
            // unlike is an unfavourite as its home actor and never depends on
            // a minted actor it does not have.
            let reaction =
                db::reactions::ReactionRow::find_by_user_post_platform(&state.db, user_id, post_id, "ap")
                    .await?;
            if let Some(row) = reaction
                .as_ref()
                .filter(|r| r.metadata["like_identity"].as_str() == Some("home-rest"))
            {
                return home_rest_unreact(&state.db, user_id, row).await;
            }
            // Minted-actor rows (Bluesky/email signups + pre-realignment rows):
            // no AP actor → nothing was ever federated → nothing to undo.
            if !crate::ap::ap_actors_enabled() {
                return Ok(false);
            }
            let user = db::users::UserRow::find_by_id(&state.db, user_id)
                .await?
                .ok_or_else(|| AppError::BadRequest("no such user".into()))?;
            if user.ap_actor_id.is_none() || !user.ap_enabled {
                return Ok(false);
            }
            // `ui_unlike` reports whether it queued the Undo (false = no
            // outstanding original to undo).
            crate::ap::bridge::ui_unlike(&state.db, user_id, post_id)
                .await
                .map_err(|e| AppError::Internal(format!("AP unlike failed: {e}")))
        }
        ReactionTarget::At => {
            let reaction = db::reactions::ReactionRow::find_by_user_post_platform(
                &state.db,
                user_id,
                post_id,
                "bluesky",
            )
            .await?;
            let Some(rkey) = reaction
                .as_ref()
                .and_then(|r| r.metadata["bluesky_like_rkey"].as_str().map(str::to_string))
            else {
                return Ok(false); // nothing was federated
            };
            let hint = reaction
                .as_ref()
                .and_then(|r| r.metadata["like_identity"].as_str())
                .map(str::to_string);
            match hint.as_deref() {
                Some("linked") => {
                    if let Some(acct) = linked_bluesky_account(&state.db, user_id).await? {
                        oauth::bluesky::delete_like(&state.db, &acct, &rkey)
                            .await
                            .map_err(|e| AppError::Internal(format!("Bluesky unlike failed: {e}")))?;
                        return Ok(true);
                    }
                    Ok(false)
                }
                Some("native") => {
                    if let Some(did) = minted_pds_row(&state.db, user_id).await? {
                        delete_native_like(state, &did, &rkey).await?;
                        return Ok(true);
                    }
                    Ok(false)
                }
                // Legacy rows predate the hint: old resolution order.
                _ => match crate::api::stories::at_identity_for(&state.db, user_id).await? {
                    Some(crate::api::stories::AtId::Native(did)) => {
                        delete_native_like(state, &did, &rkey).await?;
                        Ok(true)
                    }
                    Some(crate::api::stories::AtId::Linked(acct)) => {
                        oauth::bluesky::delete_like(&state.db, &acct, &rkey)
                            .await
                            .map_err(|e| AppError::Internal(format!("Bluesky unlike failed: {e}")))?;
                        Ok(true)
                    }
                    None => Ok(false),
                },
            }
        }
    }
}

/// Delete a minted-repo like record; a missing record counts as undone.
async fn delete_native_like(
    state: &AppState,
    did: &str,
    rkey: &str,
) -> Result<(), AppError> {
    match crate::pds::delete_record_internal(
        &state.db,
        did,
        "app.bsky.feed.like",
        rkey,
    )
    .await
    {
        Ok(_) | Err(AppError::NotFound(_)) => Ok(()),
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// Follow/unfollow
// ---------------------------------------------------------------------------
#[derive(Deserialize)]
struct FeedFollowReq {
    handle: String,
    source: String,
}



/// Does an AP actor URL match a `user@host` handle (last path segment =
/// username, host = instance)? Used for stale follow-row cleanup when the
/// remote actor no longer resolves.
pub(crate) fn ap_actor_matches_handle(actor_url: &str, handle: &str) -> bool {
    let Some((user, host)) = handle.split_once('@') else {
        return false;
    };
    let Ok(url) = url::Url::parse(actor_url) else {
        return false;
    };
    let url_user = url.path().rsplit('/').next().unwrap_or("").trim_start_matches('@');
    url.host_str().is_some_and(|h| h.eq_ignore_ascii_case(host))
        && url_user.eq_ignore_ascii_case(user)
}

async fn feed_follow(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<FeedFollowReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    // All adapter routing lives in the FollowService (identity-graph Phase 2);
    // the response shape is the endpoint's historic contract plus additive
    // provenance fields.
    let outcome = crate::api::follow::follow(&state, user_id, &req.handle, &req.source).await?;
    Ok(Json(serde_json::json!({
        "status": "following",
        "actor": outcome.actor,
        "network": outcome.network,
        "identity_used": outcome.identity_used,
    })))
}

#[derive(Deserialize)]
struct FeedUnfollowReq {
    handle: String,
    source: String,
}

async fn feed_unfollow(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<FeedUnfollowReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    // Undo routes back through the adapter that made the follow (the follows
    // row's follow_type is the undo key) — all in the FollowService.
    let outcome = crate::api::follow::unfollow(&state, user_id, &req.handle, &req.source).await?;
    Ok(Json(serde_json::json!({
        "status": "none",
        "undo_queued": outcome.federated,
        "network": outcome.network,
    })))
}



/// GET /api/feed/follows — list the user's native follows
async fn list_follows(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let rows = sqlx::query_as::<_, (uuid::Uuid, String, Option<String>, String, String)>(
        "SELECT id, followed_actor, label, status, follow_type FROM follows WHERE follower_id = $1 ORDER BY created_at DESC"
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    let mut bsky_handles: Vec<String> = Vec::new();
    for (_, actor, _, _, ft) in &rows {
        let is_bsky = ft == "bluesky_import" || actor.contains("bsky.app/profile/");
        if is_bsky {
            if let Some(handle) = actor.split("/profile/").last() {
                bsky_handles.push(handle.to_string());
            }
        }
    }

    let mut avatar_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for chunk in bsky_handles.chunks(25) {
        let url = format!(
            "https://api.bsky.app/xrpc/app.bsky.actor.getProfiles?{}",
            chunk
                .iter()
                .map(|h| format!("actors={}", h))
                .collect::<Vec<_>>()
                .join("&")
        );
        if let Ok(resp) = reqwest::get(&url).await {
            if let Ok(profiles) = resp.json::<serde_json::Value>().await {
                if let Some(arr) = profiles["profiles"].as_array() {
                    for profile in arr {
                        let handle = profile["handle"].as_str().unwrap_or("");
                        let avatar = profile["avatar"].as_str().unwrap_or("");
                        if !handle.is_empty() && !avatar.is_empty() {
                            avatar_map.insert(handle.to_string(), avatar.to_string());
                        }
                    }
                }
            }
        }
    }

    let follows: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(id, actor, label, status, follow_type)| {
            let is_bsky = follow_type == "bluesky_import" || actor.contains("bsky.app/profile/");
            let avatar = if is_bsky {
                actor
                    .split("/profile/")
                    .last()
                    .and_then(|handle| avatar_map.get(handle).cloned())
            } else {
                None
            };
            let mut val = serde_json::json!({
                "id": id.to_string(),
                "actor": actor,
                "label": label.unwrap_or_else(|| actor.split('/').last().unwrap_or(&actor).to_string()),
                "status": status,
                "follow_type": follow_type,
            });
            if let Some(av) = avatar {
                val["avatar"] = serde_json::json!(av);
            }
            val
        })
        .collect();

    Ok(Json(serde_json::json!({ "follows": follows })))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Routing is derived from the object's own URI, never a client label —
    /// the permanent fix for the "unsupported reaction source" 400 class.
    #[test]
    fn reaction_target_routes_by_uri_scheme() {
        assert_eq!(
            reaction_target("at://did:plc:x/app.bsky.feed.post/1", "mastodon"),
            Some(ReactionTarget::At),
            "at:// always routes to AT regardless of label"
        );
        assert_eq!(
            reaction_target("https://fedibird.com/users/mole/statuses/1", "fediverse"),
            Some(ReactionTarget::Ap),
            "https:// always routes to AP regardless of label"
        );
        // Legacy non-URI ids fall back to the label.
        assert_eq!(reaction_target("12345", "mastodon"), Some(ReactionTarget::Ap));
        assert_eq!(reaction_target("12345", "bsky"), Some(ReactionTarget::At));
        assert_eq!(
            reaction_target("12345", "activitypub|mastodon"),
            Some(ReactionTarget::Ap),
            "pipe-joined labels take their first segment"
        );
        assert_eq!(reaction_target("12345", ""), None);
        assert_eq!(reaction_target("12345", "forums"), None);
    }

    async fn connect_pool() -> Option<sqlx::PgPool> {
        if std::env::var("DATABASE_URL").is_err() {
            let _ = dotenvy::dotenv();
        }
        let url = std::env::var("DATABASE_URL").ok()?;
        Some(sqlx::PgPool::connect(&url).await.expect("db connect"))
    }

    #[tokio::test]
    async fn combined_feed_includes_linked_mastodon_history_in_seed_and_full() {
        let Some(pool) = connect_pool().await else { return };
        let Ok(config) = crate::config::AppConfig::from_env() else {
            eprintln!("config incomplete — skipping feed DB test");
            return;
        };
        let state = AppState {
            config: std::sync::Arc::new(config),
            db: pool.clone(),
            og_cache: Default::default(),
        };

        let suffix = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let username = format!("feedimport_{suffix}");
        let remote = format!("remote_{suffix}");
        let post_uri = format!("https://mastodon.test/users/{remote}/statuses/{suffix}");
        let user = db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");
        db::accounts::LinkedAccountRow::create(
            &pool,
            user.id,
            "mastodon",
            &remote,
            &remote,
            "unused",
            None,
            None,
            None,
            None,
            &serde_json::json!({"instance": "mastodon.test"}),
        )
        .await
        .expect("link Mastodon identity");
        sqlx::query(
            "INSERT INTO appview_posts (uri, cid, author_did, author_handle, text, created_at)
             VALUES ($1, $2, $3, $4, 'old imported Mastodon post', now() - interval '30 days')",
        )
        .bind(&post_uri)
        .bind(format!("cid_{suffix}"))
        .bind(format!("https://mastodon.test/users/{remote}"))
        .bind(format!("{remote}@mastodon.test"))
        .execute(&pool)
        .await
        .expect("insert imported history");

        let token = auth::create_token(user.id, &username, &state.config.jwt_secret)
            .expect("viewer token");
        for phase in ["rebuild", "full"] {
            let app = Router::new().merge(routes()).with_state(state.clone());
            let request = axum::http::Request::builder()
                .uri(format!("/api/feed/combined?phase={phase}"))
                .header("Authorization", format!("Bearer {token}"))
                .body(axum::body::Body::empty())
                .expect("feed request");
            let response = tower::ServiceExt::oneshot(app, request)
                .await
                .expect("feed response");
            assert_eq!(response.status(), axum::http::StatusCode::OK, "{phase} status");
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("feed body");
            let feed: FeedResponse = serde_json::from_slice(&body).expect("feed JSON");
            assert!(
                feed.items.iter().any(|p| p.id == post_uri && p.source == "mastodon" && p.is_own == Some(true)),
                "{phase} phase must include the viewer's old linked Mastodon history; got {:?}",
                feed.items.iter().map(|p| (&p.id, &p.source, p.is_own)).collect::<Vec<_>>()
            );
        }

        let _ = sqlx::query("DELETE FROM feed_snapshots WHERE key LIKE $1")
            .bind(format!("seed:{}|%", user.id))
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM appview_posts WHERE uri = $1")
            .bind(&post_uri)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM linked_accounts WHERE user_id = $1")
            .bind(user.id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await;
    }
    #[tokio::test]
    async fn combined_feed_includes_activitypub_hashtags_in_seed_and_full() {
        let Some(pool) = connect_pool().await else { return };
        let Ok(config) = crate::config::AppConfig::from_env() else {
            eprintln!("config incomplete — skipping feed DB test");
            return;
        };
        let state = AppState {
            config: std::sync::Arc::new(config),
            db: pool.clone(),
            og_cache: Default::default(),
        };

        let suffix = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let username = format!("feedhashtag_{suffix}");
        let tag = format!("apfeed{suffix}");
        let post_uri = format!("https://mastodon.test/users/remote/statuses/{suffix}");
        let user = db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");
        sqlx::query(
            "INSERT INTO appview_posts (uri, cid, author_did, author_handle, text, created_at)
             VALUES ($1, $2, $3, $4, $5, now())",
        )
        .bind(&post_uri)
        .bind(format!("cid_{suffix}"))
        .bind("https://mastodon.test/users/remote")
        .bind("remote@mastodon.test")
        .bind(format!("ActivityPub hashtag post #{tag}"))
        .execute(&pool)
        .await
        .expect("insert AP hashtag post");

        let token = auth::create_token(user.id, &username, &state.config.jwt_secret)
            .expect("viewer token");
        for phase in ["rebuild", "full"] {
            let app = Router::new().merge(routes()).with_state(state.clone());
            let request = axum::http::Request::builder()
                .uri(format!("/api/feed/combined?ids=hashtag:{tag}&source_categories=hashtags&phase={phase}"))
                .header("Authorization", format!("Bearer {token}"))
                .body(axum::body::Body::empty())
                .expect("feed request");
            let response = tower::ServiceExt::oneshot(app, request)
                .await
                .expect("feed response");
            assert_eq!(response.status(), axum::http::StatusCode::OK, "{phase} status");
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("feed body");
            let feed: FeedResponse = serde_json::from_slice(&body).expect("feed JSON");
            assert!(
                feed.items.iter().any(|p| p.id == post_uri
                    && p.source == "mastodon"
                    && p.matched_sources.iter().any(|m| m.kind == "hashtag" && m.label == tag)),
                "{phase} phase must include the AP post for its subscribed hashtag; got {:?}",
                feed.items.iter().map(|p| (&p.id, &p.source)).collect::<Vec<_>>()
            );
        }

        let _ = sqlx::query("DELETE FROM feed_snapshots WHERE key LIKE $1")
            .bind(format!("seed:{}|%", user.id))
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM appview_posts WHERE uri = $1")
            .bind(&post_uri)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn reactions_require_and_use_a_real_at_identity() {
        let Some(pool) = connect_pool().await else { return };
        let Ok(mut config) = crate::config::AppConfig::from_env() else {
            eprintln!("config incomplete — skipping reaction DB test");
            return;
        };
        // Point the PLC directory at a closed port: minting is an irreversible
        // write to the PUBLIC directory and must never happen in tests. The
        // on-demand block below drives the deterministic failure leg (loud
        // error, no row); the mint success leg runs offline against a stub
        // directory in pds::account::fresh_mint_stores_did_plc_with_same_key_material.
        config.plc_directory = "http://127.0.0.1:9".to_string();
        let state = AppState {
            config: std::sync::Arc::new(config),
            db: pool.clone(),
            og_cache: Default::default(),
        };
        let suffix = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let username = format!("react_{suffix}");
        let user = db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create native user");
        // Seed the pds_accounts row directly (same convention as ap/bridge.rs
        // tests) instead of PdsAccount::create — minting a real did:plc is a
        // network write to the PLC directory, and this test's subject is the
        // DB/firehose commit path, not PLC minting (which plc.rs's unit tests
        // cover offline). "account" is only read as `account.did` below.
        let did = format!("did:plc:testnative{suffix}");
        // Must be a loadable 32-byte secp256k1 secret — PdsAccount::load_by_handle
        // parses it with SecretKey::from_slice, which rejects anything shorter.
        let signing_key_private = [0x11u8; 32];
        sqlx::query(
            "INSERT INTO pds_accounts (did, handle, signing_key_private, signing_key_public, status, user_id) \
             VALUES ($1, $2, $3, 'test-pub', 'active', $4)",
        )
        .bind(&did)
        .bind(&username)
        .bind(&signing_key_private[..])
        .bind(user.id)
        .execute(&pool)
        .await
        .expect("seed native pds account");
        struct TestAccount { did: String }
        let account = TestAccount { did: did.clone() };
        let token = auth::create_token(user.id, &username, &state.config.jwt_secret)
            .expect("viewer token");
        let subject = format!("at://did:plc:example/app.bsky.feed.post/{suffix}");
        let payload = serde_json::json!({
            "post_id": subject,
            "source": "bluesky",
            "reaction": "❤️",
            "cid": "bafyreidqwopsxprkicjbrkk7kqz6xrp2cnit6qm5wu37hpxsl55hv4qaoi"
        });
        let app = Router::new().merge(routes()).with_state(state.clone());
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/feed/reaction")
            .header("Authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(axum::body::Body::from(serde_json::to_vec(&payload).unwrap()))
            .expect("reaction request");
        let response = tower::ServiceExt::oneshot(app, request)
            .await
            .expect("reaction response");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let like_commit: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pds_firehose_seq WHERE did = $1 AND ops::text LIKE '%app.bsky.feed.like/%')",
        )
        .bind(&account.did)
        .fetch_one(&pool)
        .await
        .expect("check like commit");
        assert!(like_commit, "native reaction must emit a PDS like commit");
        let rkey: Option<String> = sqlx::query_scalar(
            "SELECT metadata->>'bluesky_like_rkey' FROM reactions WHERE user_id = $1 AND post_id = $2",
        )
        .bind(user.id)
        .bind(&subject)
        .fetch_one(&pool)
        .await
        .expect("reaction metadata");
        assert!(rkey.as_deref().is_some_and(|r| !r.is_empty()), "persist native like rkey");

        let app = Router::new().merge(routes()).with_state(state.clone());
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/feed/reaction")
            .header("Authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(axum::body::Body::from(serde_json::to_vec(&payload).unwrap()))
            .expect("unlike request");
        let response = tower::ServiceExt::oneshot(app, request)
            .await
            .expect("unlike response");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let delete_commit: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pds_firehose_seq WHERE did = $1 AND ops @> jsonb_build_array(jsonb_build_object('action', 'delete', 'path', $2)))",
        )
        .bind(&account.did)
        .bind(format!("app.bsky.feed.like/{}", rkey.as_deref().unwrap()))
        .fetch_one(&pool)
        .await
        .expect("check unlike commit");
        assert!(delete_commit, "native unlike must emit a PDS delete commit");
        let local_like: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM reactions \
             WHERE user_id = $1 AND post_id = $2 AND retracted_at IS NULL)",
        )
        .bind(user.id)
        .bind(&subject)
        .fetch_one(&pool)
        .await
        .expect("local unlike check");
        assert!(!local_like);

        // On-demand identity provisioning (permanent fix 2026-11-13): an
        // account with no AT writer gets one minted at reaction time. A mint
        // failure is LOUD — the old "no AT identity linked to this account"
        // silent 400 is gone, and there is no did:web fallback (config points
        // `plc_directory` at a closed port, set at the top of this test), so
        // this leg is deterministic offline: 500 + zero rows.
        let orphan_username = format!("orphan_{suffix}");
        let orphan = db::users::UserRow::create_active(&pool, &orphan_username, "", "x", "x")
            .await
            .expect("create user without AT identity");
        let orphan_token = auth::create_token(orphan.id, &orphan_username, &state.config.jwt_secret)
            .expect("orphan token");
        let app = Router::new().merge(routes()).with_state(state.clone());
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/feed/reaction")
            .header("Authorization", format!("Bearer {orphan_token}"))
            .header("content-type", "application/json")
            .body(axum::body::Body::from(serde_json::to_vec(&payload).unwrap()))
            .expect("orphan reaction request");
        let response = tower::ServiceExt::oneshot(app, request)
            .await
            .expect("orphan reaction response");
        assert_eq!(
            response.status(),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "a failed mint must surface as a visible server error, never a silent skip or fallback"
        );
        let minted_did: Option<String> = sqlx::query_scalar(
            "SELECT did FROM pds_accounts WHERE user_id = $1",
        )
        .bind(orphan.id)
        .fetch_optional(&pool)
        .await
        .expect("on-demand mint check")
        .flatten();
        assert!(
            minted_did.is_none(),
            "a failed mint leaves no row (and never a did:web fallback): {minted_did:?}"
        );

        for table in ["pds_firehose_seq", "pds_indexed_records", "pds_repo_roots", "pds_accounts"] {
            let _ = sqlx::query(&format!("DELETE FROM {table} WHERE did = ANY($1)"))
                .bind(&[account.did.clone(), minted_did.clone().unwrap_or_default()])
                .execute(&pool)
                .await;
        }
        let _ = sqlx::query("DELETE FROM users WHERE id = ANY($1)")
            .bind(&[user.id, orphan.id])
            .execute(&pool)
            .await;
    }

    /// Toggle idempotence (the proven double-federation bug): a repeat click
    /// or a stale unlike — byte-identical to a genuine toggle — must never
    /// re-federate a fresh Like. The client's `seen_user_reaction` echo of the
    /// server's truth is what disambiguates; every response reports the truth.
    #[tokio::test]
    async fn reaction_toggle_is_idempotent_and_never_refederates_from_an_unlike() {
        let Some(pool) = connect_pool().await else { return };
        let Ok(config) = crate::config::AppConfig::from_env() else {
            eprintln!("config incomplete — skipping reaction DB test");
            return;
        };
        crate::ap::testutil::enable_ap_flags();
        let state = AppState {
            config: std::sync::Arc::new(config),
            db: pool.clone(),
            og_cache: Default::default(),
        };
        let la = crate::ap::testutil::make_local_actor(&pool, &state.config.domain).await;
        let token = auth::create_token(la.user.id, &la.username, &state.config.jwt_secret)
            .expect("viewer token");
        let target = format!("https://z.test/users/z/statuses/toggle{}", la.username);

        // Counts of what got FEDERATED — the invariant under test. A Like row
        // carries the target as its `object_id`; its Undo carries the Like's
        // id there instead and names the target via the embedded original
        // (`payload.object.object`), so match either shape.
        let count = {
            let pool = pool.clone();
            move |kind: &'static str, target: String, actor: String| {
                let pool = pool.clone();
                async move {
                    let n: i64 = sqlx::query_scalar(
                        "SELECT count(*) FROM ap_activities \
                         WHERE actor_id = $1 AND activity_type = $2 \
                           AND (object_id = $3 OR payload->'object'->>'object' = $3)",
                    )
                    .bind(&actor)
                    .bind(kind)
                    .bind(&target)
                    .fetch_one(&pool)
                    .await
                    .expect("activity count");
                    n
                }
            }
        };

        let post = {
            let state = state.clone();
            move |body: serde_json::Value, token: String| {
                let state = state.clone();
                async move {
                    let app = Router::new().merge(routes()).with_state(state.clone());
                    let request = axum::http::Request::builder()
                        .method("POST")
                        .uri("/api/feed/reaction")
                        .header("Authorization", format!("Bearer {token}"))
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
                        .expect("reaction request");
                    let response = tower::ServiceExt::oneshot(app, request)
                        .await
                        .expect("reaction response");
                    assert_eq!(response.status(), axum::http::StatusCode::OK);
                    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                        .await
                        .expect("reaction body");
                    serde_json::from_slice::<serde_json::Value>(&bytes).expect("reaction JSON")
                }
            }
        };

        let like_body = |seen: &str| {
            serde_json::json!({
                "post_id": target, "source": "mastodon", "reaction": "❤️",
                "seen_user_reaction": seen
            })
        };
        // The proven-live shape: no `seen` echo at all (old UI / raw API).
        let legacy_body =
            serde_json::json!({"post_id": target, "source": "mastodon", "reaction": "❤️"});

        // 1. Legacy like-on (nothing standing, no retraction history): a
        //    genuine fresh Like federates.
        let resp = post(legacy_body.clone(), token.clone()).await;
        assert_eq!(resp["user_reaction"], serde_json::json!("❤️"), "server truth: standing");
        assert_eq!(count("Like", target.clone(), la.actor_id.clone()).await, 1);

        // 2. Legacy unlike (row standing): the Undo goes out.
        let resp = post(legacy_body.clone(), token.clone()).await;
        assert!(resp["user_reaction"].is_null(), "server truth: cleared");
        assert_eq!(resp["undo_queued"], serde_json::json!(true), "the unreact dispatched");
        assert_eq!(count("Undo", target.clone(), la.actor_id.clone()).await, 1);

        // 3. THE proven live bug, legacy shape: the double-click's second
        //    request is byte-identical to 2's and lands with the row already
        //    gone. It must collapse into the first click — never a fresh Like.
        let resp = post(legacy_body.clone(), token.clone()).await;
        assert!(resp["user_reaction"].is_null(), "truth still cleared");
        assert_eq!(count("Like", target.clone(), la.actor_id.clone()).await, 1, "an unlike must never re-federate a Like");
        assert_eq!(count("Undo", target.clone(), la.actor_id.clone()).await, 1, "and never a second Undo");

        // 4. Contract unlike-with-no-row (client saw ❤️): exact no-op, always.
        let resp = post(like_body("❤️"), token.clone()).await;
        assert!(resp["user_reaction"].is_null());
        assert_eq!(count("Like", target.clone(), la.actor_id.clone()).await, 1);

        // 5-6. Contract like-on, then a repeat with stale "none seen": the
        //      repeat is a no-op, never a second Like.
        let resp = post(like_body(""), token.clone()).await;
        assert_eq!(resp["user_reaction"], serde_json::json!("❤️"));
        assert_eq!(count("Like", target.clone(), la.actor_id.clone()).await, 2, "a genuine new like federates");
        let resp = post(like_body(""), token.clone()).await;
        assert_eq!(resp["user_reaction"], serde_json::json!("❤️"), "truth unchanged by the repeat");
        assert_eq!(count("Like", target.clone(), la.actor_id.clone()).await, 2, "repeat click never duplicates the Like");

        // 7-8. Contract unlike (Undo #2), then the double-click's trailing
        //      identical request: no second Undo, no re-federated Like.
        let resp = post(like_body("❤️"), token.clone()).await;
        assert!(resp["user_reaction"].is_null());
        assert_eq!(resp["undo_queued"], serde_json::json!(true));
        assert_eq!(count("Undo", target.clone(), la.actor_id.clone()).await, 2);
        let resp = post(like_body("❤️"), token.clone()).await;
        assert!(resp["user_reaction"].is_null(), "truth still cleared");
        assert_eq!(count("Like", target.clone(), la.actor_id.clone()).await, 2);
        assert_eq!(count("Undo", target.clone(), la.actor_id.clone()).await, 2, "and never a third Undo");

        let _ = sqlx::query("DELETE FROM reactions WHERE user_id = $1")
            .bind(la.user.id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM ap_delivery_queue WHERE activity_id LIKE $1")
            .bind(format!("{}/activities/%", la.actor_id))
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id = $1")
            .bind(&la.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM followers WHERE user_id = $1")
            .bind(la.user.id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM ap_remote_actors WHERE actor_id LIKE $1")
            .bind("https://z.test/%")
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(la.user.id)
            .execute(&pool)
            .await;
    }

    /// One-shot HTTP stub for a home Mastodon instance: answers `responses`
    /// requests by method + path (lookup → `{"id":"42"}`, favourite and
    /// unfavourite → status JSON) and captures every request head.
    fn home_instance_stub(
        responses: usize,
    ) -> (String, std::sync::Arc<parking_lot::Mutex<Vec<String>>>, std::thread::JoinHandle<()>) {
        home_instance_stub_mode(responses, false)
    }

    /// [`home_instance_stub`] with the production-4.x shape (`lookup_gone`):
    /// `/api/v1/statuses/lookup` 404s (the endpoint was removed upstream) and
    /// `/api/v2/search?q=…&resolve=true` returns the home-LOCAL id for the
    /// piaille status — the proven working resolution. Favourite/unfavourite
    /// acknowledge that local id.
    fn home_instance_stub_mode(
        responses: usize,
        lookup_gone: bool,
    ) -> (String, std::sync::Arc<parking_lot::Mutex<Vec<String>>>, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("stub bind");
        let base = format!("http://{}", listener.local_addr().expect("stub addr"));
        let seen: std::sync::Arc<parking_lot::Mutex<Vec<String>>> =
            std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        let handle = std::thread::spawn(move || {
            // Bounded accept: a call pattern regression (fewer requests than
            // expected) must FAIL the test's length assertions, not hang the
            // join below. Each expected connection gets an 8s arrival window.
            if listener.set_nonblocking(true).is_err() {
                return;
            }
            for _ in 0..responses {
                let mut accepted = None;
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
                loop {
                    match listener.accept() {
                        Ok(pair) => {
                            accepted = Some(pair);
                            break;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            if std::time::Instant::now() >= deadline {
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(25));
                        }
                        Err(_) => break,
                    }
                }
                let Some((mut stream, _)) = accepted else { return };
                let _ = stream.set_nonblocking(false);
                let mut buf = Vec::new();
                let mut tmp = [0u8; 1024];
                loop {
                    let n = stream.read(&mut tmp).expect("stub read");
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let head = String::from_utf8_lossy(&buf).to_string();
                let (status_line, body): (&str, &str) = if head.contains("statuses/lookup") {
                    if lookup_gone {
                        ("HTTP/1.1 404 Not Found", r#"{"error":"Not found"}"#)
                    } else {
                        ("HTTP/1.1 200 OK", r#"{"id":"42"}"#)
                    }
                } else if head.contains("/api/v2/search") {
                    (
                        "HTTP/1.1 200 OK",
                        r#"{"accounts":[],"statuses":[{"id":"117616332435798026","uri":"https://piaille.fr/users/CatEveryHour/statuses/117616332269977505"}],"hashtags":[]}"#,
                    )
                } else {
                    ("HTTP/1.1 200 OK", r#"{"id":"42","favourited":true}"#)
                };
                let resp = format!(
                    "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(resp.as_bytes()).expect("stub write");
                seen2.lock().push(head);
            }
        });
        (base, seen, handle)
    }

    /// Task 2.3 acceptance (integration): a Mastodon signup's like + unlike of
    /// an AP post dispatch through its home instance's REST API AS ITS REAL
    /// ACTOR — asserted calls are lookup + POST favourite then POST
    /// unfavourite (`DELETE /favourite` is not routed on Mastodon 4.x) — and
    /// ZERO `ap_activities` rows are written (clean cutover: no minted actor
    /// is ever involved for this cohort).
    #[tokio::test]
    async fn mastodon_user_likes_and_unlikes_via_home_instance_rest() {
        let Some(pool) = connect_pool().await else { return };
        let Ok(config) = crate::config::AppConfig::from_env() else {
            eprintln!("config incomplete — skipping home-REST test");
            return;
        };
        let state = AppState {
            config: std::sync::Arc::new(config),
            db: pool.clone(),
            og_cache: Default::default(),
        };
        let (base, seen, stub) = home_instance_stub(3);

        let username = format!("mhr_{}", crate::ap::testutil::unique());
        let user = db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");
        db::accounts::LinkedAccountRow::create(
            &pool,
            user.id,
            "mastodon",
            &format!("{username}@home.test"),
            &username,
            "tok-abc",
            None,
            Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            None,
            Some("read write:favourites"),
            &serde_json::json!({
                "instance": base.clone(),
                "actor_url": format!("https://home.test/@{username}"),
            }),
        )
        .await
        .expect("link mastodon");
        let token = auth::create_token(user.id, &username, &state.config.jwt_secret).expect("token");
        let target = format!("{base}/users/z/statuses/42");

        let post = {
            let state = state.clone();
            move |body: serde_json::Value, token: String| {
                let state = state.clone();
                async move {
                    let app = Router::new().merge(routes()).with_state(state.clone());
                    let request = axum::http::Request::builder()
                        .method("POST")
                        .uri("/api/feed/reaction")
                        .header("Authorization", format!("Bearer {token}"))
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
                        .expect("reaction request");
                    let response = tower::ServiceExt::oneshot(app, request)
                        .await
                        .expect("reaction response");
                    let status = response.status();
                    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                        .await
                        .expect("reaction body");
                    (status, String::from_utf8_lossy(&bytes).to_string())
                }
            }
        };

        // Like-on: resolved via home lookup, favourited via home POST.
        let (status, body) = post(
            serde_json::json!({
                "post_id": target, "source": "mastodon", "reaction": "❤️",
                "seen_user_reaction": ""
            }),
            token.clone(),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "like-on: {body}");
        let row = db::reactions::ReactionRow::find(&pool, &target, user.id)
            .await
            .expect("reaction row")
            .expect("standing reaction");
        assert_eq!(row.metadata["like_identity"], serde_json::json!("home-rest"));
        assert_eq!(row.metadata["bluesky_like_rkey"], serde_json::json!("42"), "home status id recorded");

        // Unlike: dispatched as the SAME home actor via POST unfavourite.
        let (status, body) = post(
            serde_json::json!({
                "post_id": target, "source": "mastodon", "reaction": "❤️",
                "seen_user_reaction": "❤️"
            }),
            token.clone(),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "unlike: {body}");
        let doc: serde_json::Value = serde_json::from_str(&body).expect("JSON");
        assert_eq!(doc["undo_queued"], serde_json::json!(true), "the unfavourite dispatched");
        assert!(doc["user_reaction"].is_null(), "server truth: cleared");

        let _ = stub.join();
        let heads = seen.lock().clone();
        assert!(heads[0].contains("GET /api/v1/statuses/lookup?uri="), "lookup first: {}", heads[0]);
        assert!(heads[1].contains("POST /api/v1/statuses/42/favourite"), "favourited as the real actor");
        assert!(heads[2].contains("POST /api/v1/statuses/42/unfavourite"), "unfavourited as the real actor");
        assert!(
            heads[1].to_lowercase().contains("authorization: bearer tok-abc"),
            "the home token signs the call: {}",
            heads[1]
        );
        drop(heads);

        let (ap_rows,): (i64,) = sqlx::query_as("SELECT count(*) FROM ap_activities WHERE user_id = $1")
            .bind(user.id)
            .fetch_one(&pool)
            .await
            .expect("ap row count");
        assert_eq!(ap_rows, 0, "clean cutover: zero minted-actor rows for this cohort");
        let (queue_rows,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ap_delivery_queue q \
             JOIN ap_activities a ON a.id = q.activity_id WHERE a.user_id = $1",
        )
        .bind(user.id)
        .fetch_one(&pool)
        .await
        .expect("queue row count");
        assert_eq!(queue_rows, 0, "clean cutover: nothing queued for this cohort");
        let me = db::users::UserRow::find_by_id(&pool, user.id).await.expect("reload").expect("row");
        assert!(me.ap_actor_id.is_none(), "no second fediverse identity was minted");

        let _ = sqlx::query("DELETE FROM users WHERE id = $1").bind(user.id).execute(&pool).await;
    }

    /// THE production 404 (2026-11-14): a Mastodon 4.x home instance (lookup
    /// endpoint removed upstream) reacting to a FOREIGN-host status — the URI
    /// numeric tail is the remote server's id and must never be used. The
    /// chain falls to `/api/v2/search?…&resolve=true`, favouriting the
    /// home-LOCAL id, and the unlike undoes it on that same local id. Zero AP
    /// rows for the pair (home-REST cohort writes none).
    #[tokio::test]
    async fn home_rest_resolves_foreign_status_via_search_when_lookup_is_gone() {
        let Some(pool) = connect_pool().await else { return };
        let Ok(config) = crate::config::AppConfig::from_env() else {
            eprintln!("config incomplete — skipping home-REST search test");
            return;
        };
        let state = AppState {
            config: std::sync::Arc::new(config),
            db: pool.clone(),
            og_cache: Default::default(),
        };
        let (base, seen, stub) = home_instance_stub_mode(4, true);

        let username = format!("mhs_{}", crate::ap::testutil::unique());
        let user = db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");
        db::accounts::LinkedAccountRow::create(
            &pool,
            user.id,
            "mastodon",
            &format!("{username}@home.test"),
            &username,
            "tok-abc",
            None,
            Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            None,
            Some("read write:favourites"),
            &serde_json::json!({
                "instance": base.clone(),
                "actor_url": format!("https://home.test/@{username}"),
            }),
        )
        .await
        .expect("link mastodon");
        let token = auth::create_token(user.id, &username, &state.config.jwt_secret).expect("token");
        let target = "https://piaille.fr/users/CatEveryHour/statuses/117616332269977505";

        let post = {
            let state = state.clone();
            move |body: serde_json::Value, token: String| {
                let state = state.clone();
                async move {
                    let app = Router::new().merge(routes()).with_state(state.clone());
                    let request = axum::http::Request::builder()
                        .method("POST")
                        .uri("/api/feed/reaction")
                        .header("Authorization", format!("Bearer {token}"))
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
                        .expect("reaction request");
                    let response = tower::ServiceExt::oneshot(app, request)
                        .await
                        .expect("reaction response");
                    let status = response.status();
                    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                        .await
                        .expect("reaction body");
                    (status, String::from_utf8_lossy(&bytes).to_string())
                }
            }
        };

        // Like-on: 200 (was 404 in production) via the search fallback.
        let (status, body) = post(
            serde_json::json!({
                "post_id": target, "source": "mastodon", "reaction": "❤️",
                "seen_user_reaction": ""
            }),
            token.clone(),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "like-on: {body}");
        let row = db::reactions::ReactionRow::find(&pool, target, user.id)
            .await
            .expect("reaction row")
            .expect("standing reaction");
        assert_eq!(row.metadata["like_identity"], serde_json::json!("home-rest"));
        assert_eq!(
            row.metadata["home_status_id"], serde_json::json!("117616332435798026"),
            "the home-LOCAL id is recorded — never the URI's remote numeric tail"
        );

        // Unlike: undone on the SAME home-local id.
        let (status, body) = post(
            serde_json::json!({
                "post_id": target, "source": "mastodon", "reaction": "❤️",
                "seen_user_reaction": "❤️"
            }),
            token.clone(),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "unlike: {body}");
        let doc: serde_json::Value = serde_json::from_str(&body).expect("JSON");
        assert_eq!(doc["undo_queued"], serde_json::json!(true), "the unfavourite dispatched");

        let _ = stub.join();
        let heads = seen.lock().clone();
        assert_eq!(heads.len(), 4, "sequence: lookup, search, favourite, unfavourite — got {heads:?}");
        assert!(heads[0].contains("GET /api/v1/statuses/lookup?uri="), "lookup first: {}", heads[0]);
        assert!(
            heads[1].contains("GET /api/v2/search?q=") && heads[1].contains("&resolve=true"),
            "search second (resolve=true): {}",
            heads[1]
        );
        assert!(
            heads[2].contains("POST /api/v1/statuses/117616332435798026/favourite"),
            "favourited the home-LOCAL id: {}",
            heads[2]
        );
        assert!(
            heads[3].contains("POST /api/v1/statuses/117616332435798026/unfavourite"),
            "unfavourited the same home-LOCAL id: {}",
            heads[3]
        );
        drop(heads);

        let (ap_rows,): (i64,) = sqlx::query_as("SELECT count(*) FROM ap_activities WHERE user_id = $1")
            .bind(user.id)
            .fetch_one(&pool)
            .await
            .expect("ap row count");
        assert_eq!(ap_rows, 0, "clean cutover: zero minted-actor rows for this cohort");
        let (queue_rows,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ap_delivery_queue q \
             JOIN ap_activities a ON a.id = q.activity_id WHERE a.user_id = $1",
        )
        .bind(user.id)
        .fetch_one(&pool)
        .await
        .expect("queue row count");
        assert_eq!(queue_rows, 0, "clean cutover: nothing queued for this cohort");

        let _ = sqlx::query("DELETE FROM users WHERE id = $1").bind(user.id).execute(&pool).await;
    }

    /// One `POST /api/feed/reaction` round-trip (shared by the sprint-review
    /// regression tests below).
    async fn reaction_request(
        state: &AppState,
        token: &str,
        body: serde_json::Value,
    ) -> (axum::http::StatusCode, String) {
        let app = Router::new().merge(routes()).with_state(state.clone());
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/feed/reaction")
            .header("Authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
            .expect("reaction request");
        let response = tower::ServiceExt::oneshot(app, request)
            .await
            .expect("reaction response");
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("reaction body");
        (status, String::from_utf8_lossy(&bytes).to_string())
    }

    /// CR-2 (sprint review 2026-11-14): the 30s double-click window was blind
    /// to the home-REST cohort (it read ap_activities only, which this cohort
    /// never writes) — a legacy unlike followed within 30s by the trailing
    /// half of a double-click RE-FEDERATED a favourite. The reactions table's
    /// retraction shadow (migration 092) must collapse it to a no-op.
    #[tokio::test]
    async fn home_rest_legacy_double_click_never_refederates_from_an_unlike() {
        let Some(pool) = connect_pool().await else { return };
        let Ok(config) = crate::config::AppConfig::from_env() else {
            eprintln!("config incomplete — skipping home-REST double-click test");
            return;
        };
        let state = AppState {
            config: std::sync::Arc::new(config),
            db: pool.clone(),
            og_cache: Default::default(),
        };
        // lookup 404 + search 200 + favourite 200 + unfavourite 200 = 4 calls;
        // the repeat click below must add a FIFTH-less no-op.
        let (base, seen, stub) = home_instance_stub_mode(4, true);

        let username = format!("mhr2_{}", crate::ap::testutil::unique());
        let user = db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");
        db::accounts::LinkedAccountRow::create(
            &pool,
            user.id,
            "mastodon",
            &format!("{username}@home.test"),
            &username,
            "tok-abc",
            None,
            Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            None,
            Some("read write:favourites"),
            &serde_json::json!({
                "instance": base.clone(),
                "actor_url": format!("https://home.test/@{username}"),
            }),
        )
        .await
        .expect("link mastodon");
        let token = auth::create_token(user.id, &username, &state.config.jwt_secret).expect("token");
        let target = "https://piaille.fr/users/CatEveryHour/statuses/117616332269977505";
        // LEGACY contract shape: `seen_user_reaction` absent.
        let payload = serde_json::json!({
            "post_id": target, "source": "mastodon", "reaction": "❤️"
        });

        // Like-on (federates), unlike (unfavourites + removes the row)…
        let (status, body) = reaction_request(&state, &token, payload.clone()).await;
        assert_eq!(status, axum::http::StatusCode::OK, "like-on: {body}");
        let (status, body) = reaction_request(&state, &token, payload.clone()).await;
        assert_eq!(status, axum::http::StatusCode::OK, "unlike: {body}");

        // …then the double-click's trailing byte-identical request: the row is
        // gone and the retraction shadow is fresh — this can only be the
        // trailing click. It must NOT re-federate a favourite.
        let (status, body) = reaction_request(&state, &token, payload.clone()).await;
        assert_eq!(status, axum::http::StatusCode::OK, "repeat click: {body}");
        let doc: serde_json::Value = serde_json::from_str(&body).expect("JSON");
        assert!(doc["user_reaction"].is_null(), "server truth stays cleared");

        let _ = stub.join();
        let heads = seen.lock().clone();
        assert_eq!(
            heads.len(),
            4,
            "lookup, search, favourite, unfavourite — the repeat click must be a no-op: {heads:?}"
        );
        assert!(
            heads[2].contains("POST /api/v1/statuses/117616332435798026/favourite"),
            "favourited once: {}",
            heads[2]
        );
        assert!(
            heads[3].contains("POST /api/v1/statuses/117616332435798026/unfavourite"),
            "unfavourited once: {}",
            heads[3]
        );

        let standing: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM reactions WHERE user_id = $1 AND post_id = $2 AND retracted_at IS NULL",
        )
        .bind(user.id)
        .bind(target)
        .fetch_one(&pool)
        .await
        .expect("standing count");
        assert_eq!(standing, 0, "no reaction stands after the unlike");

        let _ = sqlx::query("DELETE FROM users WHERE id = $1").bind(user.id).execute(&pool).await;
    }

    /// CR-10 (sprint review 2026-11-14): the same double-click race on an
    /// `at://` target wrote a SECOND `app.bsky.feed.like` (fresh TID) — the
    /// window's ap_activities shadow never covers AT-native reactions. The
    /// reactions-table shadow must collapse the repeat to a no-op.
    #[tokio::test]
    async fn at_legacy_double_click_never_writes_a_duplicate_like() {
        let Some(pool) = connect_pool().await else { return };
        let Ok(mut config) = crate::config::AppConfig::from_env() else {
            eprintln!("config incomplete — skipping at:// double-click test");
            return;
        };
        // Minting is an irreversible PUBLIC write — never in tests (same
        // convention as reactions_require_and_use_a_real_at_identity).
        config.plc_directory = "http://127.0.0.1:9".to_string();
        let state = AppState {
            config: std::sync::Arc::new(config),
            db: pool.clone(),
            og_cache: Default::default(),
        };
        let suffix = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let username = format!("atdc_{suffix}");
        let user = db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create native user");
        let did = format!("did:plc:testatdc{suffix}");
        let signing_key_private = [0x11u8; 32];
        sqlx::query(
            "INSERT INTO pds_accounts (did, handle, signing_key_private, signing_key_public, status, user_id) \
             VALUES ($1, $2, $3, 'test-pub', 'active', $4)",
        )
        .bind(&did)
        .bind(&username)
        .bind(&signing_key_private[..])
        .bind(user.id)
        .execute(&pool)
        .await
        .expect("seed native pds account");
        let token = auth::create_token(user.id, &username, &state.config.jwt_secret).expect("token");
        let subject = format!("at://did:plc:example/app.bsky.feed.post/{suffix}");
        // LEGACY contract shape: `seen_user_reaction` absent.
        let payload = serde_json::json!({
            "post_id": subject,
            "source": "bluesky",
            "reaction": "❤️",
            "cid": "bafyreidqwopsxprkicjbrkk7kqz6xrp2cnit6qm5wu37hpxsl55hv4qaoi"
        });

        // Like-on (writes one like record), unlike (deletes it)…
        let (status, body) = reaction_request(&state, &token, payload.clone()).await;
        assert_eq!(status, axum::http::StatusCode::OK, "like-on: {body}");
        let (status, body) = reaction_request(&state, &token, payload.clone()).await;
        assert_eq!(status, axum::http::StatusCode::OK, "unlike: {body}");

        // …then the double-click's trailing byte-identical request: it must
        // NOT write a second like record (pre-fix: a fresh TID like commit).
        let (status, body) = reaction_request(&state, &token, payload.clone()).await;
        assert_eq!(status, axum::http::StatusCode::OK, "repeat click: {body}");
        let doc: serde_json::Value = serde_json::from_str(&body).expect("JSON");
        assert!(doc["user_reaction"].is_null(), "server truth stays cleared");

        let like_commits: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pds_firehose_seq \
             WHERE did = $1 AND ops::text LIKE '%app.bsky.feed.like/%' \
               AND NOT (ops @> jsonb_build_array(jsonb_build_object('action','delete')))",
        )
        .bind(&did)
        .fetch_one(&pool)
        .await
        .expect("like commit count");
        assert_eq!(
            like_commits, 1,
            "exactly one like record was ever written — the repeat click must not re-federate"
        );

        let _ = sqlx::query("DELETE FROM users WHERE id = $1").bind(user.id).execute(&pool).await;
    }

    /// CR-3 (sprint review 2026-11-14): after 090 nulls `home_status_id`, the
    /// record-id slot on a 089-migrated row carries the minted-actor AP
    /// activity URL — never a home status id. The unreact must re-resolve the
    /// id from the post URI (the lookup/search chain) and unfavourite THAT;
    /// posting the garbage slot 404s and sticks the reaction forever.
    #[tokio::test]
    async fn home_rest_unreact_reresolves_when_stored_id_is_poisoned() {
        let Some(pool) = connect_pool().await else { return };
        let Ok(config) = crate::config::AppConfig::from_env() else {
            eprintln!("config incomplete — skipping home-REST re-resolve test");
            return;
        };
        let state = AppState {
            config: std::sync::Arc::new(config),
            db: pool.clone(),
            og_cache: Default::default(),
        };
        // Unlike path only: lookup 404 + search 200 + unfavourite 200 = 3.
        let (base, seen, stub) = home_instance_stub_mode(3, true);

        let username = format!("mhr3_{}", crate::ap::testutil::unique());
        let user = db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");
        db::accounts::LinkedAccountRow::create(
            &pool,
            user.id,
            "mastodon",
            &format!("{username}@home.test"),
            &username,
            "tok-abc",
            None,
            Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            None,
            Some("read write:favourites"),
            &serde_json::json!({
                "instance": base.clone(),
                "actor_url": format!("https://home.test/@{username}"),
            }),
        )
        .await
        .expect("link mastodon");
        let token = auth::create_token(user.id, &username, &state.config.jwt_secret).expect("token");
        let target = "https://piaille.fr/users/CatEveryHour/statuses/117616332269977505";
        // The 089-migrated shape: home_status_id NULLed by 090, record-id slot
        // poisoned with the minted-actor AP activity URL.
        sqlx::query(
            "INSERT INTO reactions (post_id, user_id, emoji, metadata) VALUES ($1, $2, '❤️', $3)",
        )
        .bind(target)
        .bind(user.id)
        .bind(serde_json::json!({
            "like_identity": "home-rest",
            "home_status_id": null,
            "bluesky_like_rkey": "https://home.test/ap/activities/like-0123456789abcdef",
        }))
        .execute(&pool)
        .await
        .expect("seed poisoned reaction row");

        // Unlike: contract toggle-off on the standing row.
        let (status, body) = reaction_request(
            &state,
            &token,
            serde_json::json!({
                "post_id": target, "source": "mastodon", "reaction": "❤️",
                "seen_user_reaction": "❤️"
            }),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "unlike must clear the row: {body}");
        let doc: serde_json::Value = serde_json::from_str(&body).expect("JSON");
        assert_eq!(doc["undo_queued"], serde_json::json!(true), "the unfavourite dispatched");
        assert!(doc["user_reaction"].is_null(), "server truth: cleared");

        let _ = stub.join();
        let heads = seen.lock().clone();
        assert_eq!(
            heads.len(),
            3,
            "lookup, search, unfavourite — the id came from re-resolution: {heads:?}"
        );
        assert!(heads[0].contains("GET /api/v1/statuses/lookup?uri="), "lookup first: {}", heads[0]);
        assert!(
            heads[1].contains("GET /api/v2/search?q=") && heads[1].contains("&resolve=true"),
            "search second: {}",
            heads[1]
        );
        assert!(
            heads[2].contains("POST /api/v1/statuses/117616332435798026/unfavourite"),
            "unfavourited the RE-RESOLVED home-local id: {}",
            heads[2]
        );
        assert!(
            !heads.iter().any(|h| h.contains("like-0123456789abcdef")),
            "the poisoned AP activity URL must never reach the home instance: {heads:?}"
        );

        let standing = db::reactions::ReactionRow::find(&pool, target, user.id)
            .await
            .expect("reaction lookup");
        assert!(standing.is_none(), "the reaction is cleared, never stuck");

        let _ = sqlx::query("DELETE FROM users WHERE id = $1").bind(user.id).execute(&pool).await;
    }

    /// Death-path sweep (deletion E2E / tasks 6.2): retract_home_rest_reactions
    /// unfavourites every standing home-REST favourite AS THE HOME ACTOR (the
    /// minted-actor AP routine never covers these) and covers rows whose home
    /// status id lives only in the 089 `home_status_id` metadata slot.
    #[tokio::test]
    async fn deletion_sweep_retracts_standing_home_rest_favourites() {
        let Some(pool) = connect_pool().await else { return };
        let Ok(config) = crate::config::AppConfig::from_env() else {
            eprintln!("config incomplete — skipping home-REST death-path test");
            return;
        };
        let state = AppState {
            config: std::sync::Arc::new(config),
            db: pool.clone(),
            og_cache: Default::default(),
        };
        let (base, seen, stub) = home_instance_stub(2);

        let username = format!("mhrd_{}", crate::ap::testutil::unique());
        let user = db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");
        db::accounts::LinkedAccountRow::create(
            &pool,
            user.id,
            "mastodon",
            &format!("{username}@home.test"),
            &username,
            "tok-death",
            None,
            Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            None,
            Some("read write:favourites"),
            &serde_json::json!({
                "instance": base.clone(),
                "actor_url": format!("https://home.test/@{username}"),
            }),
        )
        .await
        .expect("link mastodon");

        // Two standing home-REST favourites: one created post-realignment (id
        // in the record-id slot), one 089-migrated (id in home_status_id only)
        // — plus one AP row the sweep must leave to its own identity.
        for (post_id, metadata) in [
            (
                format!("{base}/users/z/statuses/42"),
                serde_json::json!({"like_identity": "home-rest", "bluesky_like_rkey": "42"}),
            ),
            (
                "https://remote.test/users/z/statuses/77".to_string(),
                serde_json::json!({"like_identity": "home-rest", "home_status_id": "77"}),
            ),
            (
                "https://remote.test/users/z/statuses/99".to_string(),
                serde_json::json!({"like_identity": "ap"}),
            ),
        ] {
            sqlx::query(
                "INSERT INTO reactions (post_id, user_id, emoji, metadata) \
                 VALUES ($1, $2, '⭐', $3)",
            )
            .bind(&post_id)
            .bind(user.id)
            .bind(metadata)
            .execute(&pool)
            .await
            .expect("seed reaction");
        }

        let n = retract_home_rest_reactions(&pool, user.id)
            .await
            .expect("death-path sweep");
        assert_eq!(n, 2, "both home-REST favourites retracted; AP row untouched");

        let _ = stub.join();
        let heads = seen.lock().clone();
        let deletes: Vec<&String> = heads
            .iter()
            .filter(|h| h.contains("POST /api/v1/statuses/"))
            .filter(|h| h.contains("/unfavourite"))
            .collect();
        assert_eq!(deletes.len(), 2, "one unfavourite per standing favourite: {heads:?}");
        assert!(
            deletes.iter().all(|h| h.to_lowercase().contains("authorization: bearer tok-death")),
            "both dispatched with the home token"
        );

        // The AP row is not this sweep's business (its own identity retracts it).
        let (ap_left,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM reactions WHERE user_id = $1 AND metadata->>'like_identity' = 'ap'",
        )
        .bind(user.id)
        .fetch_one(&pool)
        .await
        .expect("ap row count");
        assert_eq!(ap_left, 1);

        let _ = sqlx::query("DELETE FROM users WHERE id = $1").bind(user.id).execute(&pool).await;
    }

    /// Task 2.4 dispatch half (verify): after the 089 metadata migration the
    /// reaction's UNREACT dispatches the same identity that created it — a
    /// `home-rest` row (home status id in the `home_status_id` slot) is
    /// undone via the home instance's unfavourite API AS THE HOME ACTOR, and
    /// no minted-actor row/activity is touched.
    #[tokio::test]
    async fn post_migration_unreact_dispatches_home_rest_identity() {
        let Some(pool) = connect_pool().await else { return };
        let Ok(config) = crate::config::AppConfig::from_env() else {
            eprintln!("config incomplete — skipping unreact-dispatch test");
            return;
        };
        let state = AppState {
            config: std::sync::Arc::new(config),
            db: pool.clone(),
            og_cache: Default::default(),
        };
        let (base, seen, stub) = home_instance_stub(1);

        let username = format!("m089d_{}", crate::ap::testutil::unique());
        let user = db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");
        db::accounts::LinkedAccountRow::create(
            &pool,
            user.id,
            "mastodon",
            &format!("{username}@home.test"),
            &username,
            "tok-m089",
            None,
            Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            None,
            Some("read write:favourites"),
            &serde_json::json!({
                "instance": base.clone(),
                "actor_url": format!("https://home.test/@{username}"),
            }),
        )
        .await
        .expect("link mastodon");

        // The post-migration row shape (089 output): identity hint + home
        // status id in `home_status_id`, no record-id slot.
        let post_id = "https://remote.test/users/z/statuses/55";
        sqlx::query(
            "INSERT INTO reactions (post_id, user_id, emoji, metadata) \
             VALUES ($1, $2, '⭐', $3)",
        )
        .bind(post_id)
        .bind(user.id)
        .bind(serde_json::json!({"like_identity": "home-rest", "home_status_id": "55"}))
        .execute(&pool)
        .await
        .expect("seed migrated reaction");

        let undone = perform_platform_unreact("mastodon", &state, post_id, false, None, user.id)
            .await
            .expect("unreact dispatch");
        assert!(undone, "the home-REST undo dispatched");

        let _ = stub.join();
        let heads = seen.lock().clone();
        assert_eq!(heads.len(), 1, "exactly one call: {heads:?}");
        assert!(
            heads[0].contains("POST /api/v1/statuses/55/unfavourite"),
            "unfavourite as the home actor: {}",
            heads[0]
        );
        let (ap_rows,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ap_activities WHERE user_id = $1",
        )
        .bind(user.id)
        .fetch_one(&pool)
        .await
        .expect("ap row count");
        assert_eq!(ap_rows, 0, "unreact stayed on the home identity");

        let _ = sqlx::query("DELETE FROM users WHERE id = $1").bind(user.id).execute(&pool).await;
    }

    /// Task 1.2 acceptance (API half): a stale read-only grant attempting a
    /// federated like returns a VISIBLE re-link error — never a silent
    /// fallback to another identity, never a half-dispatched call.
    #[tokio::test]
    async fn stale_scope_federated_like_returns_visible_relink_error() {
        let Some(pool) = connect_pool().await else { return };
        let Ok(config) = crate::config::AppConfig::from_env() else {
            eprintln!("config incomplete — skipping re-link test");
            return;
        };
        let state = AppState {
            config: std::sync::Arc::new(config),
            db: pool.clone(),
            og_cache: Default::default(),
        };
        let username = format!("mrl_{}", crate::ap::testutil::unique());
        let user = db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");
        db::accounts::LinkedAccountRow::create(
            &pool,
            user.id,
            "mastodon",
            &format!("{username}@home.test"),
            &username,
            "tok-legacy",
            None,
            Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            None,
            Some("read"), // the pre-realignment grant
            &serde_json::json!({
                "instance": "http://127.0.0.1:1", // unreachable is fine: the scope gate fires first
                "actor_url": format!("https://home.test/@{username}"),
            }),
        )
        .await
        .expect("link mastodon (legacy scope)");
        let token = auth::create_token(user.id, &username, &state.config.jwt_secret).expect("token");

        let app = Router::new().merge(routes()).with_state(state);
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/feed/reaction")
            .header("Authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "post_id": "https://z.test/users/z/statuses/42",
                    "source": "mastodon",
                    "reaction": "❤️",
                    "seen_user_reaction": ""
                }))
                .unwrap(),
            ))
            .expect("reaction request");
        let response = tower::ServiceExt::oneshot(app, request).await.expect("reaction response");
        assert_eq!(
            response.status(),
            axum::http::StatusCode::FORBIDDEN,
            "stale scope must fail loudly"
        );
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.expect("body");
        let body = String::from_utf8_lossy(&bytes).to_string();
        assert!(body.contains("re-link"), "the error names the fix: {body}");

        // Never a silent fallback: no minted identity, no federated rows.
        let me = db::users::UserRow::find_by_id(&pool, user.id).await.expect("reload").expect("row");
        assert!(me.ap_actor_id.is_none(), "no second fediverse identity was minted");
        let (ap_rows,): (i64,) = sqlx::query_as("SELECT count(*) FROM ap_activities WHERE user_id = $1")
            .bind(user.id)
            .fetch_one(&pool)
            .await
            .expect("ap row count");
        assert_eq!(ap_rows, 0, "nothing half-dispatched");

        let _ = sqlx::query("DELETE FROM users WHERE id = $1").bind(user.id).execute(&pool).await;
    }
}
