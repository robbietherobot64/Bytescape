//! Uniform filtering pipeline for all feed posts.
//!
//! Applied uniformly to ALL posts regardless of source or sort mode.

use std::collections::{BTreeMap, HashSet};

use axum::http::HeaderMap;
use uuid::Uuid;

use super::FeedPost;
use crate::auth;
use crate::db::users::NsfwPref;

/// NSFW label values (from self-labels in appview_posts.labels JSON array).
const NSFW_LABELS: &[&str] = &[
    "nsfw",
    "porn",
    "sexual",
    "nudity",
    "graphic-media",
    "adult",
    "!warn",
    "!hide",
    "!no-unspecified",
];

/// SQL predicate: "this row's own `labels` column carries no NSFW value".
///
/// Generated from [`NSFW_LABELS`] so the database pre-filter and the in-memory
/// policy cannot drift — an earlier hand-copied set omitted `nudity`,
/// `graphic-media`, `adult`, `!hide` and `!no-unspecified`, letting those rows
/// through every query. `ILIKE` keeps the match case-insensitive because the
/// firehose stores self-labels verbatim (`NSFW` is as valid as `nsfw`).
///
/// `labels_column` is the (optionally qualified) column name, e.g. `labels`
/// or `p.labels`. Splice the result into a query as `AND {predicate}`.
///
/// This is the `exclude` predicate — it keeps only rows free of NSFW labels.
/// Prefer [`sql_labels_filter`] when the viewer's [`NsfwPref`] is known; this
/// wrapper is the `exclude` default used by call sites with no viewer context.
pub fn sql_labels_clean(labels_column: &str) -> String {
    sql_labels_filter(labels_column, NsfwPref::Exclude)
}

/// NSFW SQL pre-filter for a given three-state preference.
///
/// Only `exclude` restricts the SQL result set (NSFW rows never leave the
/// database). `blur` and `show` must FETCH the marked rows — the in-memory
/// [`apply_nsfw_policy`] then marks (blur) or unmarks (show) them — so both
/// return a no-op `TRUE` predicate here.
pub fn sql_labels_filter(labels_column: &str, pref: NsfwPref) -> String {
    if pref != NsfwPref::Exclude {
        return "TRUE".to_string();
    }
    let checks: Vec<String> = NSFW_LABELS
        .iter()
        .map(|l| format!("{labels_column}::text ILIKE '%{l}%'"))
        .collect();
    format!("({labels_column} IS NULL OR NOT ({}))", checks.join(" OR "))
}

/// NSFW hashtag patterns (checked against post text).
pub const NSFW_TAGS: &[&str] = &[
    "#nsfw", "#porn", "#adultcontent", "#sexual", "#nude", "#lewd", "#18plus", "#xxx",
    "#boobs", "#nipples", "#pussy", "#dick", "#anal", "#cum", "#gayporn", "#furrynsfw",
    "#rule34", "#onlyfans",
    "#gay", "#sex", "#erotica", "#mature", "#explicit", "#horny", "#fetish", "#kink",
    "#nsfwcontent", "#adult", "#xxxcontent", "#hardcore", "#nsfwart", "#bdsm",
    "#sexy", "#nsfwpost", "#nsfwpic", "#afterdark", "#pornhub",
    "#naked", "#nudist",
];

pub struct FeedFilters {
    pub muted_dids: HashSet<String>,
    pub muted_handles: HashSet<String>,
}

impl FeedFilters {
    /// Build filters from the authenticated user's mute list.
    pub async fn from_user(
        db: &sqlx::PgPool,
        headers: &HeaderMap,
        secret: &str,
    ) -> Result<Self, crate::error::AppError> {
        let mut muted_dids = HashSet::new();
        let mut muted_handles = HashSet::new();

        if let Ok(user_id) = extract_user_id(headers, secret) {
            // Load muted DIDs
            let dids: Vec<String> =
                sqlx::query_scalar("SELECT muted_did FROM muted_accounts WHERE user_id = $1 AND muted_did IS NOT NULL")
                    .bind(user_id)
                    .fetch_all(db)
                    .await
                    .unwrap_or_default();
            muted_dids.extend(dids);

            // Load muted handles
            let handles: Vec<String> =
                sqlx::query_scalar("SELECT muted_handle FROM muted_accounts WHERE user_id = $1 AND muted_handle IS NOT NULL")
                    .bind(user_id)
                    .fetch_all(db)
                    .await
                    .unwrap_or_default();
            muted_handles.extend(handles.into_iter().map(|h| h.to_lowercase()));
        }

        Ok(FeedFilters {
            muted_dids,
            muted_handles,
        })
    }

    /// Check if a post should be NSFW-filtered (labels + text hashtags).
    pub fn is_nsfw(text: &Option<String>, labels: &Option<serde_json::Value>) -> bool {
        // Check labels JSON array: ["nsfw", "porn", ...]
        if let Some(lbl) = labels {
            if let Some(arr) = lbl.as_array() {
                if arr
                    .iter()
                    .filter_map(|v| v.as_str())
                    .any(|l| NSFW_LABELS.contains(&l))
                {
                    return true;
                }
            }
        }
        // Check text for NSFW hashtags (case-insensitive substring match)
        if let Some(t) = text {
            let lower = t.to_lowercase();
            if NSFW_TAGS.iter().any(|tag| lower.contains(tag)) {
                return true;
            }
        }
        false
    }

    /// Apply ALL filters uniformly to a post.
    /// Returns false if the post should be filtered out.
    pub fn should_keep(&self, post: &FeedPost) -> bool {
        // 1. NULL handles — skip, don't show @unknown
        let handle_clean = post.author.handle.trim_start_matches('@').to_lowercase();
        if handle_clean.is_empty() || handle_clean == "unknown" {
            return false;
        }

        // 2. Muted accounts (by DID)
        if let Some(ref ext_id) = post.author_external_id {
            if self.muted_dids.contains(ext_id) {
                return false;
            }
        }

        // 3. Muted accounts (by handle)
        if self.muted_handles.contains(&handle_clean) {
            return false;
        }
        // Also check raw muted_dids against handle (the old code stored handles in muted_dids)
        if self
            .muted_dids
            .iter()
            .any(|m| m.to_lowercase() == handle_clean)
        {
            return false;
        }

        // 4. Future-dated posts
        if !post.created_at.is_empty() {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&post.created_at) {
                if dt.with_timezone(&chrono::Utc) > chrono::Utc::now() {
                    return false;
                }
            }
        }

        true
    }
}

/// Extract user ID from Authorization header.
fn extract_user_id(headers: &HeaderMap, secret: &str) -> Result<Uuid, crate::error::AppError> {
    let auth_header = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| {
            crate::error::AppError::Auth("Missing or invalid Authorization header".into())
        })?;

    let claims = auth::validate_token(auth_header, secret)?;
    Uuid::parse_str(&claims.sub)
        .map_err(|_| crate::error::AppError::Auth("Invalid user ID in token".into()))
}

/// Get the user's interest tags for popular feed filtering.
/// Returns lowercased hashtags and search terms.
pub async fn get_interest_tags(
    db: &sqlx::PgPool,
    headers: &HeaderMap,
    secret: &str,
) -> Vec<String> {
    // Subscription rows are the truth: interests expand into hashtag
    // subscriptions at bootstrap time. The old path resolved user.interests
    // through a hand-maintained side map — the "picked interests get ignored"
    // bug.
    let mut tags = Vec::new();
    if let Ok(user_id) = extract_user_id(headers, secret) {
        if let Ok(rows) =
            crate::db::subscriptions::SubscriptionRow::find_by_user(db, user_id).await
        {
            for row in rows.iter().filter(|r| r.enabled && r.category == "hashtags") {
                let tag = row
                    .source_id
                    .trim_start_matches("mastodon:")
                    .trim_start_matches('#')
                    .to_lowercase();
                if !tag.is_empty() && !tags.contains(&tag) {
                    tags.push(tag);
                }
            }
        }
    }
    tags
}

/// NSFW policy — three-state (`users.nsfw_pref`), always marking suspect
/// content first:
///
/// - `Exclude` (default): hide NSFW entirely (post-marking retain).
/// - `Blur`: keep NSFW items and leave them marked (`nsfw = true`) so the
///   frontend renders its blur + click-to-reveal layer.
/// - `Show`: treat NSFW as clean — clear the flag so nothing is blurred or
///   hidden.
pub fn apply_nsfw_policy(posts: &mut Vec<FeedPost>, pref: NsfwPref) {
    for p in posts.iter_mut() {
        if !p.nsfw && is_nsfw_content(&p.content, &p.labels) {
            p.nsfw = true;
        }
    }
    match pref {
        NsfwPref::Exclude => posts.retain(|p| !p.nsfw),
        NsfwPref::Blur => { /* keep marked items for the blur + reveal layer */ }
        NsfwPref::Show => {
            for p in posts.iter_mut() {
                p.nsfw = false;
            }
        }
    }
}

/// Network source-mix filter (`users.feed_prefs.source_mix`, request
/// `source_mix=`): "bluesky" keeps only AT-origin items, "mastodon" keeps only
/// AP/fediverse-origin items, absent/"all" keeps everything. The post `source`
/// field is the single network truth (`normalize::source_from_uri`): `"bluesky"`
/// is AT; everything else (`mastodon`, `lemmy`, `pixelfed`, …) is fediverse.
pub fn apply_source_mix(posts: &mut Vec<FeedPost>, mix: Option<&str>) {
    match mix {
        Some("bluesky") => posts.retain(|p| p.source == "bluesky"),
        Some("mastodon") => posts.retain(|p| p.source != "bluesky"),
        // "all" or absent → current behavior (no filtering).
        _ => {}
    }
}

/// Stable client-facing platform bucket keys for the Platforms filter.
pub const PLATFORM_BUCKETS: [&str; 5] = [
    "bytescape",
    "bluesky",
    "mastodon",
    "lemmy",
    "pixelfed",
];

/// Map a `FeedPost.source` value onto a platform bucket.
///
/// The known multi-network labels map explicitly; every other value — the
/// local/native `bytescape`, `rss`, imported archives (`instagram`,
/// `facebook`, `threads`, `google`), group posts, and any future label — falls
/// back to "bytescape" (local/native), so no post is ever un-bucketable. This
/// is the one authority for the mapping: the frontend never re-derives it.
pub fn platform_bucket(source: &str) -> &'static str {
    match source.trim().to_ascii_lowercase().as_str() {
        "bluesky" | "atproto" | "bsky" => "bluesky",
        "mastodon" | "ap" | "activitypub" | "misskey" | "fediverse" => "mastodon",
        "lemmy" | "piefed" => "lemmy",
        "pixelfed" => "pixelfed",
        _ => "bytescape",
    }
}

/// Parse the `platform=` query parameter. Absent, `all`, or an unrecognised
/// value mean "no filter" — a bad param must never blank the feed.
pub fn parse_platform(raw: Option<&str>) -> Option<&'static str> {
    let value = raw?.trim().to_ascii_lowercase();
    PLATFORM_BUCKETS.iter().copied().find(|b| *b == value)
}

/// Retain only posts whose `source` buckets to the selected platform.
pub fn apply_platform_filter(posts: &mut Vec<FeedPost>, platform: Option<&str>) {
    if let Some(bucket) = parse_platform(platform) {
        posts.retain(|p| platform_bucket(&p.source) == bucket);
    }
}

/// Fold `(source, count)` rows into platform-bucket totals. Every bucket is
/// present, with 0 when it has no posts — the UI shows the honest zero.
pub fn bucket_counts<I>(rows: I) -> BTreeMap<String, i64>
where
    I: IntoIterator<Item = (String, i64)>,
{
    let mut counts: BTreeMap<String, i64> =
        PLATFORM_BUCKETS.iter().map(|b| (b.to_string(), 0)).collect();
    for (source, count) in rows {
        *counts.entry(platform_bucket(&source).to_string()).or_insert(0) += count;
    }
    counts
}

/// SQL for the platform-mix counts over the appview index. `tags_param` binds
/// the interest-tag `text[]` (`$n`) through the tag index; `author_param`
/// binds a single author identifier. The source label CASE comes from
/// [`super::normalize::source_case_sql`], so the aggregate and the per-post
/// label share one rule table.
pub fn platform_counts_sql(
    labels_predicate: &str,
    tags_param: Option<usize>,
    author_param: Option<usize>,
) -> String {
    let mut sql = format!(
        "SELECT {} AS src, COUNT(*)::bigint FROM appview_posts \
         WHERE reply_parent IS NULL AND text IS NOT NULL AND text != '' \
           AND created_at <= now() AND {}",
        super::normalize::source_case_sql("uri"),
        labels_predicate,
    );
    if let Some(i) = tags_param {
        sql.push_str(&format!(
            " AND uri IN (SELECT uri FROM appview_post_tags WHERE tag = ANY(${i}))"
        ));
    }
    if let Some(i) = author_param {
        sql.push_str(&format!(
            " AND (lower(author_handle) = lower(${i}) OR author_did = ${i})"
        ));
    }
    sql.push_str(" GROUP BY 1");
    sql
}

/// Time-window presets for the feed's period filter, in hours.
pub const WINDOW_PRESETS: [(&str, i64); 5] = [
    ("12h", 12),
    ("24h", 24),
    ("48h", 48),
    ("1w", 24 * 7),
    ("1mo", 24 * 30),
];

/// Inclusive time window applied to feed posts before sorting/truncation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeWindow {
    pub start: Option<chrono::DateTime<chrono::Utc>>,
    pub end: Option<chrono::DateTime<chrono::Utc>>,
}

impl TimeWindow {
    pub fn contains(&self, created_at: chrono::DateTime<chrono::Utc>) -> bool {
        self.start.is_none_or(|s| created_at >= s) && self.end.is_none_or(|e| created_at <= e)
    }
}

/// Parse the `window=`/`from=`/`to=` query parameters.
///
/// - preset (`12h`, `24h`, `48h`, `1w`, `1mo`): posts newer than `now - preset`.
/// - `custom`: `from`/`to` are `YYYY-MM-DD` dates; `from` starts that day (UTC)
///   and `to` covers through the end of that day. Either side may be empty for
///   an open-ended range.
/// - absent / `all` / unknown preset: no filter (`None`).
/// - `custom` with neither date, or malformed dates, also means no filter: a
///   partial form is not a reason to blank the feed.
pub fn parse_window(
    preset: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<TimeWindow> {
    let preset = preset?.trim().to_ascii_lowercase();
    if preset.is_empty() || preset == "all" {
        return None;
    }
    if preset == "custom" {
        let start = from
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .and_then(parse_date_start);
        let end = to
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .and_then(parse_date_end);
        if start.is_none() && end.is_none() {
            return None;
        }
        return Some(TimeWindow { start, end });
    }
    let hours = WINDOW_PRESETS
        .iter()
        .find(|(key, _)| *key == preset)
        .map(|(_, hours)| *hours)?;
    Some(TimeWindow {
        start: Some(now - chrono::Duration::hours(hours)),
        end: None,
    })
}

fn parse_date_start(raw: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .ok()?
        .and_hms_opt(0, 0, 0)
        .map(|t| t.and_utc())
}

fn parse_date_end(raw: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .ok()?
        .and_hms_opt(23, 59, 59)
        .map(|t| t.and_utc())
}

/// Keep only posts inside the window. Applied BEFORE the sort/truncation stage
/// so the sort sees the filtered pool (a late filter would let high-volume
/// recent posts crowd an old custom range out of the truncated page).
///
/// With a window active, a post whose `created_at` cannot be parsed is dropped:
/// it cannot be proven to be inside the range.
pub fn apply_time_window(posts: &mut Vec<FeedPost>, window: Option<&TimeWindow>) {
    let Some(window) = window else {
        return;
    };
    posts.retain(|p| {
        chrono::DateTime::parse_from_rfc3339(&p.created_at)
            .map(|t| window.contains(t.with_timezone(&chrono::Utc)))
            .unwrap_or(false)
    });
}

/// Combined NSFW check for a fully-built post: moderation/self labels first,
/// then text signals. Labels cover image-only posts that carry no text markers
/// (the gap that put explicit media in feeds).
pub fn is_nsfw_content(text: &str, labels: &[String]) -> bool {
    if labels.iter().any(|l| NSFW_LABELS.contains(&l.to_lowercase().as_str())) {
        return true;
    }
    let content_lower = text.to_lowercase();
    NSFW_TAGS.iter().any(|tag| content_lower.contains(tag))
}

/// Deduplicate posts by URI, then by (author + normalized content). Duplicate
/// copies contribute their source provenance to the kept post (feed chips).
/// Parse the `types=` query parameter into canonical filter buckets.
/// Empty/`all` selections mean no filter.
pub fn parse_types(raw: Option<&str>) -> Option<Vec<String>> {
    let v: Vec<String> = raw?
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty() && s != "all")
        .collect();
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

/// Tier-fetch multiplier when a type filter is active: sparse buckets need
/// a wider upstream net to still fill a page after filtering.
pub fn type_boost(raw: Option<&str>) -> i64 {
    if parse_types(raw).is_some() {
        3
    } else {
        1
    }
}

/// Keep only posts matching one of the selected type buckets.
pub fn filter_by_types(posts: &mut Vec<FeedPost>, types: &Option<Vec<String>>) {
    let Some(sel) = types else { return };
    posts.retain(|p| sel.iter().any(|t| bucket_matches(t, &p.content_type)));
}

/// `types=` bucket -> the concrete `content_type` values it covers.
fn bucket_matches(bucket: &str, content_type: &str) -> bool {
    match bucket {
        "text" => content_type == "short",
        "blog" => content_type == "long",
        "image" => content_type == "image",
        "video" => content_type == "video",
        "link" => content_type == "link",
        "event" => content_type == "event",
        "market" => content_type == "market",
        "live" => content_type == "live",
        "audio" => content_type == "audio",
        "poll" => content_type == "poll",
        "list" => content_type == "list",
        "other" => content_type == "other",
        _ => false,
    }
}

pub fn deduplicate(posts: Vec<FeedPost>) -> Vec<FeedPost> {
    use std::collections::HashMap;
    let mut seen_ids: HashMap<String, usize> = HashMap::new();
    let mut seen_content: HashMap<(String, String), usize> = HashMap::new();
    let mut kept: Vec<FeedPost> = Vec::new();

    for mut p in posts {
        if let Some(&i) = seen_ids.get(&p.id) {
            merge_matched_sources(&mut kept[i], &mut p);
            continue;
        }
        // Content dedup key: (author_external_id + normalized content)
        let author_key = p
            .author_external_id
            .clone()
            .unwrap_or_else(|| p.author.handle.clone());
        let text_key = p.content.to_lowercase();
        let normalized = text_key
            .split_whitespace()
            .filter(|w| !w.starts_with("http"))
            .collect::<Vec<_>>()
            .join(" ");
        let key = (author_key, normalized);
        if let Some(&i) = seen_content.get(&key) {
            merge_matched_sources(&mut kept[i], &mut p);
            continue;
        }
        seen_ids.insert(p.id.clone(), kept.len());
        seen_content.insert(key, kept.len());
        kept.push(p);
    }
    kept
}

/// Tag posts with the interest tags they contain — without reordering. Used on
/// full-phase batches where a later sort reorders anyway but matching must be
/// reflected as chips. Chip truth is the shared rule ([`crate::tags::chip_claim`]):
/// a `#tag` chip is earned only by the literal hashtag (entire tag), and a bare
/// word match gets a plain `interest` chip (no `#`) — a chip must never claim
/// `#climate` on a post with no `#climate` in it.
pub fn tag_matches(posts: &mut [FeedPost], interest_tags: &[String]) {
    for p in posts.iter_mut() {
        for tag in interest_tags {
            // Shared chip truth (crate::tags) — identical to the events feed.
            if let Some(claim) = crate::tags::chip_claim(&p.content, tag) {
                let kind = match claim {
                    crate::tags::ChipClaim::Hashtag => "hashtag",
                    crate::tags::ChipClaim::Word => "interest",
                };
                // Labels are bare (`cats`); the FE renders `#` for hashtag kind.
                let label = tag.trim_start_matches('#').to_string();
                if !p
                    .matched_sources
                    .iter()
                    .any(|m| m.kind == kind && m.label == label)
                {
                    p.matched_sources.push(super::SourceMatch {
                        label,
                        kind: kind.to_string(),
                        rank: 1,
                    });
                }
            }
        }
        p.matched_sources.sort_by_key(|m| m.rank);
    }
}

fn merge_matched_sources(into: &mut FeedPost, from: &mut FeedPost) {
    for m in from.matched_sources.drain(..) {
        if !into
            .matched_sources
            .iter()
            .any(|x| x.kind == m.kind && x.label == m.label)
        {
            into.matched_sources.push(m);
        }
    }
    into.matched_sources.sort_by_key(|m| m.rank);
    // Counts too: the local index under-counts (incremental counters only
    // see events since indexing), so a stale copy must not clobber an
    // authoritative one during dedup.
    into.likes_count = into.likes_count.max(from.likes_count);
    into.shares_count = into.shares_count.max(from.shares_count);
    into.replies_count = into.replies_count.max(from.replies_count);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::feed::AuthorInfo;

    #[test]
    fn nsfw_labels_mark_content() {
        let porn = vec!["porn".to_string()];
        assert!(is_nsfw_content("just text", &porn));
        let warn = vec!["!warn".to_string()];
        assert!(is_nsfw_content("just text", &warn));
        let mixed = vec!["cooking".to_string(), "Nudity".to_string()];
        assert!(is_nsfw_content("just text", &mixed));
    }

    #[test]
    fn nsfw_text_hashtags_mark_content() {
        assert!(is_nsfw_content("check this #nsfw drop", &[]));
        assert!(is_nsfw_content("NEW #OnlyFans link in bio", &[]));
    }

    #[test]
    fn clean_content_is_not_marked() {
        assert!(!is_nsfw_content("my cat is adorable #cats", &[]));
        assert!(!is_nsfw_content(
            "the word snafu contains no tag markers",
            &["cooking".to_string()]
        ));
    }

    /// The SQL pre-filter must cover the whole NSFW label set — the old
    /// hand-copied clause list omitted nudity/graphic-media/adult/!hide.
    #[test]
    fn sql_label_predicate_covers_every_nsfw_label() {
        let predicate = sql_labels_clean("p.labels");
        assert!(predicate.starts_with("(p.labels IS NULL OR NOT ("));
        for label in NSFW_LABELS {
            assert!(
                predicate.contains(&format!("p.labels::text ILIKE '%{label}%'")),
                "predicate missing {label}: {predicate}"
            );
        }
        assert!(!predicate.to_lowercase().contains("ilike '%cooking%'"));
    }

    #[test]
    fn policy_hides_by_default_and_keeps_when_opted_in() {
        let post = sample_post("#nsfw drop");

        // Exclude (default): hidden entirely.
        let mut hidden = vec![post.clone()];
        apply_nsfw_policy(&mut hidden, NsfwPref::Exclude);
        assert!(hidden.is_empty(), "default policy hides NSFW entirely");

        // Blur: kept and marked for the blur + reveal layer.
        let mut blurred = vec![post.clone()];
        apply_nsfw_policy(&mut blurred, NsfwPref::Blur);
        assert_eq!(blurred.len(), 1, "blur keeps the post");
        assert!(blurred[0].nsfw, "blur keeps the post marked");

        // Show: kept and unmarked (treated as clean).
        let mut shown = vec![post];
        apply_nsfw_policy(&mut shown, NsfwPref::Show);
        assert_eq!(shown.len(), 1, "show keeps the post");
        assert!(!shown[0].nsfw, "show unmarks the post");
    }

    /// Source-mix network filter: "bluesky" keeps only AT-origin items,
    /// "mastodon" keeps only AP/fediverse-origin items, absent/"all" keeps all.
    #[test]
    fn source_mix_partitions_bluesky_vs_fediverse() {
        let mut mk = |source: &str| {
            let mut p = sample_post("hello");
            p.source = source.to_string();
            p
        };
        let mut posts = vec![
            mk("bluesky"),
            mk("mastodon"),
            mk("lemmy"),
            mk("pixelfed"),
            mk("bytescape"),
        ];

        // bluesky → only the AT-origin item.
        let mut bsky = posts.clone();
        apply_source_mix(&mut bsky, Some("bluesky"));
        assert_eq!(bsky.len(), 1);
        assert!(bsky.iter().all(|p| p.source == "bluesky"), "{:?}", bsky);

        // mastodon → only AP/fediverse-origin items (not Bluesky).
        let mut ap = posts.clone();
        apply_source_mix(&mut ap, Some("mastodon"));
        assert_eq!(ap.len(), 4);
        assert!(ap.iter().all(|p| p.source != "bluesky"), "{:?}", ap);

        // all / absent → unchanged.
        let mut all = posts.clone();
        apply_source_mix(&mut all, Some("all"));
        assert_eq!(all.len(), 5);
        apply_source_mix(&mut posts, None);
        assert_eq!(posts.len(), 5);
        let _ = &mut mk;
    }

    /// Every `source` value the pipeline can emit must land in a bucket —
    /// including the local/imported labels that have no network of their own.
    #[test]
    fn platform_bucket_maps_every_known_source() {
        let cases: &[(&str, &str)] = &[
            // AT Protocol.
            ("bluesky", "bluesky"),
            ("atproto", "bluesky"),
            ("bsky", "bluesky"),
            // ActivityPub microblogs.
            ("mastodon", "mastodon"),
            ("ap", "mastodon"),
            ("activitypub", "mastodon"),
            ("misskey", "mastodon"),
            ("fediverse", "mastodon"),
            // Threadiverse.
            ("lemmy", "lemmy"),
            ("piefed", "lemmy"),
            // Image network.
            ("pixelfed", "pixelfed"),
            // Local / native / imported / unknown — all fall back to local.
            ("bytescape", "bytescape"),
            ("rss", "bytescape"),
            ("import", "bytescape"),
            ("group", "bytescape"),
            ("instagram", "bytescape"),
            ("facebook", "bytescape"),
            ("threads", "bytescape"),
            ("google", "bytescape"),
            ("", "bytescape"),
            ("something-brand-new", "bytescape"),
        ];
        for (source, bucket) in cases {
            assert_eq!(platform_bucket(source), *bucket, "source {source:?}");
        }
    }

    #[test]
    fn platform_filter_keeps_only_the_selected_bucket() {
        let mk = |source: &str| {
            let mut p = sample_post("hello");
            p.source = source.to_string();
            p
        };
        let base = || {
            vec![
                mk("bluesky"),
                mk("mastodon"),
                mk("lemmy"),
                mk("pixelfed"),
                mk("bytescape"),
                mk("rss"),
            ]
        };

        let mut bluesky = base();
        apply_platform_filter(&mut bluesky, Some("bluesky"));
        assert_eq!(bluesky.len(), 1);
        assert!(bluesky.iter().all(|p| p.source == "bluesky"));

        let mut lemmy = base();
        apply_platform_filter(&mut lemmy, Some("lemmy"));
        assert_eq!(lemmy.len(), 1);
        assert_eq!(lemmy[0].source, "lemmy");

        // Local bucket folds native + imported/RSS labels.
        let mut local = base();
        apply_platform_filter(&mut local, Some("bytescape"));
        assert_eq!(local.len(), 2, "{:?}", local.iter().map(|p| &p.source).collect::<Vec<_>>());

        // Absent / all / unknown params leave the feed untouched.
        for param in [None, Some("all"), Some("ALL"), Some("nonsense")] {
            let mut posts = base();
            apply_platform_filter(&mut posts, param);
            assert_eq!(posts.len(), 6, "param {param:?} must not filter");
        }
    }

    #[test]
    fn parse_platform_accepts_only_known_buckets() {
        assert_eq!(parse_platform(Some("pixelfed")), Some("pixelfed"));
        assert_eq!(parse_platform(Some(" Pixelfed ")), Some("pixelfed"));
        assert_eq!(parse_platform(Some("all")), None);
        assert_eq!(parse_platform(Some("")), None);
        assert_eq!(parse_platform(Some("blueskyy")), None);
        assert_eq!(parse_platform(None), None);
    }

    #[test]
    fn bucket_counts_folds_sources_and_zeros_empty_buckets() {
        let rows = vec![
            ("bluesky".to_string(), 7),
            ("mastodon".to_string(), 3),
            ("piefed".to_string(), 2),
            ("pixelfed".to_string(), 1),
            ("bytescape".to_string(), 1),
            ("rss".to_string(), 2),
        ];
        let counts = bucket_counts(rows);
        assert_eq!(counts.len(), 5, "all buckets reported");
        assert_eq!(counts["bluesky"], 7);
        assert_eq!(counts["mastodon"], 3);
        assert_eq!(counts["lemmy"], 2);
        assert_eq!(counts["pixelfed"], 1);
        assert_eq!(counts["bytescape"], 3, "native + rss fold into local");

        // Absent buckets are reported as an honest zero.
        let empty = bucket_counts(Vec::<(String, i64)>::new());
        assert_eq!(empty.len(), 5);
        assert!(empty.values().all(|n| *n == 0));
    }

    #[test]
    fn platform_counts_sql_scopes_tags_and_author() {
        let labels = sql_labels_clean("labels");

        let global = platform_counts_sql(&labels, None, None);
        assert!(global.contains("GROUP BY 1"), "{global}");
        assert!(global.contains(&labels), "labels predicate missing: {global}");
        assert!(!global.contains("appview_post_tags"), "no tag scope: {global}");
        assert!(!global.contains("author_handle"), "no author scope: {global}");

        let tagged = platform_counts_sql(&labels, Some(1), None);
        assert!(
            tagged.contains("uri IN (SELECT uri FROM appview_post_tags WHERE tag = ANY($1))"),
            "{tagged}"
        );

        let authored = platform_counts_sql(&labels, Some(1), Some(2));
        assert!(
            authored.contains("(lower(author_handle) = lower($2) OR author_did = $2)"),
            "{authored}"
        );
    }

    fn mk_at(created: chrono::DateTime<chrono::Utc>) -> FeedPost {
        let mut p = sample_post("hello");
        p.created_at = created.to_rfc3339();
        p
    }

    #[test]
    fn parse_window_presets_and_neutral_values() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-17T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        for (preset, hours) in WINDOW_PRESETS {
            let w = parse_window(Some(preset), None, None, now).expect(preset);
            assert_eq!(w.start, Some(now - chrono::Duration::hours(hours)), "{preset}");
            assert_eq!(w.end, None, "{preset}");
        }
        for neutral in [None, Some(""), Some("all"), Some("ALL"), Some("nonsense")] {
            assert_eq!(parse_window(neutral, None, None, now), None, "{neutral:?}");
        }
    }

    #[test]
    fn parse_window_custom_is_inclusive_and_open_ended() {
        let now = chrono::Utc::now();
        let range = parse_window(Some("custom"), Some("2026-11-10"), Some("2026-11-12"), now)
            .expect("custom range");
        assert_eq!(
            range.start,
            Some(
                chrono::NaiveDate::from_ymd_opt(2026, 11, 10)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc()
            )
        );
        assert_eq!(
            range.end,
            Some(
                chrono::NaiveDate::from_ymd_opt(2026, 11, 12)
                    .unwrap()
                    .and_hms_opt(23, 59, 59)
                    .unwrap()
                    .and_utc()
            )
        );

        // Open-ended: from only / to only; empty sides ignored.
        let from_only = parse_window(Some("custom"), Some("2026-11-10"), Some(""), now).unwrap();
        assert!(from_only.start.is_some() && from_only.end.is_none());
        let to_only = parse_window(Some("custom"), None, Some("2026-11-12"), now).unwrap();
        assert!(to_only.start.is_none() && to_only.end.is_some());

        // No usable dates or malformed dates → no filter (never blank).
        assert_eq!(parse_window(Some("custom"), None, None, now), None);
        assert_eq!(parse_window(Some("custom"), Some("nope"), None, now), None);
    }

    #[test]
    fn time_window_filters_before_sort_at_preset_boundaries() {
        let now = chrono::Utc::now();
        let mut posts = vec![
            mk_at(now - chrono::Duration::hours(11)),  // inside 12h
            mk_at(now - chrono::Duration::hours(13)),  // outside 12h
            mk_at(now - chrono::Duration::hours(100)), // inside 1w, outside 48h
            mk_at(now - chrono::Duration::days(8)),    // outside 1w
        ];

        let mut twelve = posts.clone();
        apply_time_window(
            &mut twelve,
            parse_window(Some("12h"), None, None, now).as_ref(),
        );
        assert_eq!(twelve.len(), 1);

        let mut week = posts.clone();
        apply_time_window(&mut week, parse_window(Some("1w"), None, None, now).as_ref());
        assert_eq!(week.len(), 3, "1w keeps the 11h, 13h and 100h posts");

        let mut day = posts.clone();
        apply_time_window(&mut day, parse_window(Some("24h"), None, None, now).as_ref());
        assert_eq!(day.len(), 2, "24h keeps the 11h and 13h posts");

        // No window → untouched, including unparseable timestamps.
        let mut broken = vec![mk_at(now)];
        broken[0].created_at = "not-a-date".to_string();
        apply_time_window(&mut broken, None);
        assert_eq!(broken.len(), 1, "no window never drops anything");
        apply_time_window(&mut broken, parse_window(Some("24h"), None, None, now).as_ref());
        assert!(broken.is_empty(), "active window drops unverifiable timestamps");

        let _ = &mut posts;
    }

    #[test]
    fn time_window_composes_with_platform_filter() {
        let now = chrono::Utc::now();
        let mut old_bsky = mk_at(now - chrono::Duration::days(40));
        old_bsky.source = "bluesky".to_string();
        let mut new_bsky = mk_at(now - chrono::Duration::hours(2));
        new_bsky.source = "bluesky".to_string();
        let mut new_masto = mk_at(now - chrono::Duration::hours(3));
        new_masto.source = "mastodon".to_string();
        let mut posts = vec![old_bsky, new_bsky, new_masto];

        apply_platform_filter(&mut posts, Some("bluesky"));
        apply_time_window(
            &mut posts,
            parse_window(Some("24h"), None, None, now).as_ref(),
        );
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].source, "bluesky");
        assert!(posts[0].created_at.as_str() > (now - chrono::Duration::days(1)).to_rfc3339().as_str());
    }

    /// Chip truth on the main feed: a `#tag` chip only for the literal
    /// hashtag — a bare word match gets `interest` (no `#`), never `hashtag`.
    /// This is the phantom-`#climate` regression: a "CLIMATE report" post must
    /// never carry a `#climate` chip.
    #[test]
    fn tag_matches_chips_are_literal() {
        let mut posts = vec![
            sample_post("I love #climate talk"),
            sample_post("CLIMATE report for the region"),
            sample_post("a #climatechange march"),
        ];
        tag_matches(&mut posts, &["climate".to_string()]);

        let chip = |p: &FeedPost, kind: &str| {
            p.matched_sources
                .iter()
                .any(|m| m.kind == kind && m.label == "climate")
        };
        assert!(chip(&posts[0], "hashtag"), "literal #climate earns #chip: {:?}", posts[0].matched_sources);
        assert!(!chip(&posts[0], "interest"));
        assert!(chip(&posts[1], "interest"), "bare word earns plain interest chip: {:?}", posts[1].matched_sources);
        assert!(!chip(&posts[1], "hashtag"), "bare word must NOT earn a #chip: {:?}", posts[1].matched_sources);
        assert!(posts[2].matched_sources.is_empty(), "#climatechange earns no climate chip: {:?}", posts[2].matched_sources);
    }

    fn sample_post(content: &str) -> FeedPost {
        FeedPost {
            id: "at://did:plc:x/app.bsky.feed.post/1".to_string(),
            source: "bluesky".to_string(),
            author: AuthorInfo {
                handle: "@smoke.bsky.social".to_string(),
                display_name: None,
                avatar: None,
            },
            content: content.to_string(),
            content_type: "short".to_string(),
            media: vec![],
            embeds: vec![],
            likes_count: 0,
            shares_count: 0,
            replies_count: 0,
            created_at: String::new(),
            url: None,
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
            is_reshare: None,
            original_post: None,
            is_own: None,
            is_following: None,
            follow_pending: None,
            cross_posts: vec![],
            matched_sources: vec![],
            labels: vec![],
            nsfw: false,
            author_external_id: None,
        }
    }
}
