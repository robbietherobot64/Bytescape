//! Source fetchers for the combined feed.
//!
//! Each fetcher returns a Vec<FeedPost> for a single source.

use super::filter;
use super::normalize;
use super::paginate::Cursor;
use super::sort::SortMode;
use super::FeedPost;
use crate::db::users::NsfwPref;

/// Fill author display data (handle, display name, avatar) from the AppView
/// identity map for posts whose author came through without one. The
/// author_handle column is slowly backfilled and normalize_appview_post
/// falls back to the raw DID when it is empty — which put raw DIDs in the
/// UI for every author outside the viewer's own follow map. Runs at the
/// handler boundary so all sources, including served caches and snapshots,
/// are covered uniformly.
pub async fn fill_missing_profiles(db: &sqlx::PgPool, posts: &mut [FeedPost]) {
    use std::collections::{HashMap, HashSet};

    fn needs_identity(handle: &str) -> bool {
        handle.is_empty() || handle.starts_with("did:") || handle.starts_with("@did:")
    }

    let missing: Vec<String> = posts
        .iter()
        .filter(|p| needs_identity(&p.author.handle))
        .filter_map(|p| p.author_external_id.clone())
        .collect::<HashSet<String>>()
        .into_iter()
        .collect();
    if missing.is_empty() {
        return;
    }
    let rows: Vec<(String, String, Option<String>, Option<String>)> = match sqlx::query_as(
        "SELECT did, handle, display_name, avatar FROM appview_did_handle WHERE did = ANY($1)",
    )
    .bind(&missing)
    .fetch_all(db)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("[FEED] identity map lookup failed: {e}");
            return;
        }
    };
    let mut map: HashMap<String, (String, Option<String>, Option<String>)> = rows
        .into_iter()
        .map(|(did, handle, display_name, avatar)| (did, (handle, display_name, avatar)))
        .collect();

    // DIDs the map doesn't know yet: native PDS accounts resolve locally,
    // everything else goes through the Bluesky getProfiles batch (which
    // also persists to appview_did_handle — the next request is a local hit).
    let unresolved: Vec<String> = missing
        .iter()
        .filter(|d| !map.contains_key(d.as_str()))
        .cloned()
        .collect();
    if !unresolved.is_empty() {
        let native: Vec<(String, Option<String>, Option<String>, Option<String>)> =
            sqlx::query_as(
                "SELECT a.did, a.handle, u.display_name, u.avatar_url
                 FROM pds_accounts a LEFT JOIN users u ON u.id = a.user_id
                 WHERE a.did = ANY($1)",
            )
            .bind(&unresolved)
            .fetch_all(db)
            .await
            .unwrap_or_default();
        for (did, handle, display_name, avatar) in native {
            if let Some(handle) = handle.filter(|h| !h.is_empty()) {
                map.insert(did, (handle, display_name, avatar));
            }
        }
        let remote: Vec<String> = unresolved
            .iter()
            .filter(|d| !map.contains_key(d.as_str()))
            .cloned()
            .collect();
        map.extend(resolve_profiles_remote(db, &remote).await);
    }
    for p in posts.iter_mut() {
        if !needs_identity(&p.author.handle) {
            continue;
        }
        let Some(did) = p.author_external_id.as_deref() else {
            continue;
        };
        let Some((handle, display_name, avatar)) = map.get(did) else {
            continue;
        };
        p.author.handle = normalize::format_handle(handle);
        if p.author.display_name.is_none() {
            p.author.display_name = display_name.clone().filter(|s| !s.is_empty());
        }
        if p.author.avatar.is_none() {
            p.author.avatar = avatar.clone().filter(|s| !s.is_empty());
        }
    }
}

/// Fetch posts from AppView DB by followed accounts.
/// Uses the author_did index for fast lookup.
pub async fn fetch_appview_follows(
    db: &sqlx::PgPool,
    followed_dids: &[String],
    sort: SortMode,
    cursor: &Option<Cursor>,
    limit: i64,
) -> Result<Vec<FeedPost>, crate::error::AppError> {
    if followed_dids.is_empty() {
        return Ok(Vec::new());
    }

    // First page only, keyed by the follow set: re-scanning 200+ authors on
    // every load was the feed's dominant cost (bitmap-or over full author
    // histories).
    let cache_key = if cursor.is_none() {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let mut sorted: Vec<&String> = followed_dids.iter().collect();
        sorted.sort();
        for d in &sorted {
            std::hash::Hash::hash(d, &mut h);
        }
        std::hash::Hash::hash(&format!("{:?}", sort), &mut h);
        Some(format!("follows|{}|{}", std::hash::Hasher::finish(&h), limit))
    } else {
        None
    };
    if let Some(key) = &cache_key {
        if let Some(hit) = cache_get(&FOLLOWS_CACHE, key) {
            return Ok(hit);
        }
    }

    let order_clause = sort.order_by();
    let time_filter = sort.time_filter();

    // Per-author bounded lookups: `author_did = ANY(200+)` degenerated into
    // a bitmap-or over full histories (30s per query). Per-did `LIMIT n`
    // walks each author's (author_did, created_at) index and stops early;
    // the small results merge in memory. Recent-first window keeps semantics
    // feed-like and the per-did ranges tiny.
    let per_did = ((limit / 4).max(5)) as i64;
    let did_sql = format!(
        r#"SELECT uri,
                  like_count::bigint, repost_count::bigint, reply_count::bigint
           FROM appview_posts
           WHERE author_did = $1
             AND reply_parent IS NULL
             AND text IS NOT NULL AND text != ''
             AND created_at <= now()
             AND created_at > now() - interval '7 days'
             {}
           ORDER BY {}
           LIMIT $2"#,
        time_filter, order_clause,
    );
    // Concurrent in bounded batches: sequential per-did lookups over 200+
    // authors still took tens of seconds on a cold cache.
    let mut candidates: Vec<(String, i64)> = Vec::new();
    for batch in followed_dids.chunks(32) {
        let futs = batch.iter().map(|did| {
            let db = db;
            let sql = &did_sql;
            async move {
                sqlx::query_as::<_, (String, i64, i64, i64)>(sql)
                    .bind(did)
                    .bind(per_did)
                    .fetch_all(db)
                    .await
                    .unwrap_or_default()
            }
        });
        for rows in futures::future::join_all(futs).await {
            for (uri, likes, reposts, replies) in rows {
                candidates.push((uri, likes * 3 + reposts * 2 + replies));
            }
        }
    }
    // Newest-first candidates: without per-row created_at here, use the
    // candidate pool as-is and let the final query order the page.
    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    candidates.dedup_by(|a, b| a.0 == b.0);
    let candidate_uris: Vec<String> = candidates
        .into_iter()
        .map(|(uri, _)| uri)
        .take(240)
        .collect();
    if candidate_uris.is_empty() {
        return Ok(Vec::new());
    }

    let sql = format!(
        r#"SELECT uri, cid, author_did, text, created_at, embeds, labels, author_handle,
                   author_avatar, author_display_name,
                   like_count, repost_count, reply_count
            FROM appview_posts
            WHERE uri = ANY($1::text[])
              AND reply_parent IS NULL
              AND text IS NOT NULL AND text != ''
              AND created_at <= now()
              {}
              {}
            ORDER BY {}
            LIMIT $2"#,
        time_filter,
        cursor
            .as_ref()
            .map(|c| c.sql_condition())
            .unwrap_or_default(),
        order_clause
    );

    let rows: Vec<(
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
        Option<i32>,
        Option<i32>,
        Option<i32>,
    )> = sqlx::query_as(&sql)
        .bind(&candidate_uris)
        .bind(limit)
        .fetch_all(db)
        .await
        .map_err(|e| {
            tracing::error!("[FEED] fetch_appview_follows error: {}", e);
            crate::error::AppError::Database(e)
        })?;

    let posts: Vec<FeedPost> = rows
        .into_iter()
        .filter_map(|(uri, cid, did, text, created, embeds, labels, handle, avatar, display_name, likes, reposts, replies)| {
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
        .collect();

    if let Some(key) = cache_key {
        if !posts.is_empty() {
            cache_put(&FOLLOWS_CACHE, key, posts.clone());
        }
    }

    Ok(posts)
}

/// Fetch recent AppView posts (fallback when follows don't return enough).
/// No handle gate: the handle column is slowly backfilled and filtering on
/// it shrank every feed to the ~1% of rows that had one. Missing display
/// handles are filled from the did→handle map / a did fallback instead.
pub async fn fetch_appview_recent(
    db: &sqlx::PgPool,
    sort: SortMode,
    cursor: &Option<Cursor>,
    limit: i64,
) -> Result<Vec<FeedPost>, crate::error::AppError> {
    let order_clause = sort.order_by();
    let time_filter = sort.time_filter();

    let sql = format!(
        r#"SELECT uri, cid, author_did, text, created_at, embeds, labels, author_handle,
                   author_avatar, author_display_name,
                   like_count, repost_count, reply_count
            FROM appview_posts
            WHERE reply_parent IS NULL
              AND text IS NOT NULL AND text != ''
                            AND created_at <= now()
              {}
              {}
            ORDER BY {}
            LIMIT $1"#,
        time_filter,
        cursor
            .as_ref()
            .map(|c| c.sql_condition())
            .unwrap_or_default(),
        order_clause
    );

    let rows: Vec<(
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
        Option<i32>,
        Option<i32>,
        Option<i32>,
    )> = sqlx::query_as(&sql)
        .bind(limit)
        .fetch_all(db)
        .await
        .map_err(|e| {
            tracing::error!("[FEED] fetch_appview_recent error: {}", e);
            crate::error::AppError::Database(e)
        })?;

    let posts: Vec<FeedPost> = rows
        .into_iter()
        .filter_map(|(uri, cid, did, text, created, embeds, labels, handle, avatar, display_name, likes, reposts, replies)| {
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
        .collect();

    Ok(posts)
}

/// Match a post's text against `interest_tags` (label, needle) pairs and return
/// truthful chips. Chip truth is the shared rule ([`crate::tags::chip_claim`]):
/// a `#tag` chip is earned ONLY by an actual string literal match of the entire
/// hashtag in the text (boundary-checked — `#tech` never matches `#techno`,
/// `#climatechange` never earns `#climate`); a bare word match gets a plain
/// non-# `interest` chip. Identical to the events feed and `filter::tag_matches`
/// — one rule, three call sites.
fn match_interest_chips(
    pairs: &[(String, String)],
    text: &str,
) -> Vec<super::SourceMatch> {
    let mut out = Vec::new();
    for (label, n) in pairs {
        // Shared chip truth (crate::tags) — identical to the events feed.
        if let Some(claim) = crate::tags::chip_claim(text, n) {
            let kind = match claim {
                crate::tags::ChipClaim::Hashtag => "hashtag",
                crate::tags::ChipClaim::Word => "interest",
            };
            out.push(super::SourceMatch {
                label: label.trim_start_matches('#').to_string(),
                kind: kind.to_string(),
                rank: 1,
            });
        }
    }
    out
}

/// Does this text match the interest tag at all (hashtag OR word)? Feed
/// membership uses the inferred matcher — it may keep word matches ("they may
/// fit in the feed for that topic, which is fine"); only chips are literal.
fn interest_membership(pairs: &[(String, String)], text: &str) -> bool {
    pairs.iter().any(|(_, n)| crate::tags::text_matches_tag(text, n))
}

#[cfg(test)]
mod match_tests {
    use super::{interest_membership, match_interest_chips};

    fn pairs(tags: &[&str]) -> Vec<(String, String)> {
        tags.iter()
            .map(|t| (t.to_string(), t.trim_start_matches('#').to_lowercase()))
            .collect()
    }

    /// A `#hashtag` chip only for a real hashtag occurrence of the entire tag.
    /// `#tech` must NOT match `#techno` (the phantom-chip bug), `#climatechange`
    /// must not earn `#climate`, and a bare word mention gets a plain non-#
    /// `interest` chip — never a `#` chip from "some inferred matching system".
    #[test]
    fn chips_are_truthful() {
        // Real hashtag → hashtag chip.
        let c = match_interest_chips(&pairs(&["tech"]), "I love #tech today");
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].kind, "hashtag");

        // "#techno" must not produce a "#tech" hashtag chip.
        let c = match_interest_chips(&pairs(&["tech"]), "dancing to #techno all night");
        assert!(c.is_empty(), "#tech must not match #techno: {c:?}");

        // Entire-tag matching: "#climatechange" earns no "#climate" chip.
        let c = match_interest_chips(&pairs(&["climate"]), "a #climatechange march");
        assert!(c.is_empty(), "#climatechange must not earn #climate: {c:?}");

        // Word mention → bare `interest` chip (no `#`, label without hash).
        let c = match_interest_chips(&pairs(&["climate"]), "CLIMATE report for the region");
        assert_eq!(c.len(), 1, "word match keeps provenance: {c:?}");
        assert_eq!(c[0].kind, "interest", "word match is interest, not hashtag");
        assert!(!c[0].label.starts_with('#'), "interest label is bare: {:?}", c[0].label);

        // No match at all → no chip (a political post gets no #activitypub).
        let c = match_interest_chips(&pairs(&["activitypub"]), "Senate approves fiscal package");
        assert!(c.is_empty(), "unrelated post must get no chip: {c:?}");
    }

    /// Feed *membership* is the inferred matcher (hashtag OR word) — word
    /// matches stay in the feed ("they may fit in the feed for that topic,
    /// which is fine") even though they earn no chip.
    #[test]
    fn membership_keeps_word_matches() {
        let p = pairs(&["climate"]);
        assert!(interest_membership(&p, "CLIMATE report for the region"));
        assert!(interest_membership(&p, "#climate march downtown"));
        assert!(!interest_membership(&p, "a #climatechange march"));
        assert!(!interest_membership(&p, "Senate approves fiscal package"));
    }
}

/// Normalize the counts scope: tags lowercased/deduped/sorted (`tag = ANY`
/// is set-ordered, so the cache key must be stable under reordering), and a
/// single-valued author (a comma-joined author filter is a multi-author view;
/// the counts query takes one identifier, so leave it unscoped rather than
/// pick one).
fn counts_scope(interest_tags: &[String], author: Option<&str>) -> (Vec<String>, Option<String>) {
    let mut tags: Vec<String> = interest_tags
        .iter()
        .map(|t| t.trim_start_matches('#').to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    tags.sort();
    tags.dedup();
    let author = author
        .map(|a| a.trim().trim_start_matches('@'))
        .filter(|a| !a.is_empty() && !a.contains(','))
        .map(|a| a.to_string());
    (tags, author)
}

/// Per-platform-bucket counts for the current feed scope.
///
/// The scope mirrors the feed's own row selection: top-level, non-empty,
/// non-future rows from the appview index, NSFW-filtered by the viewer's
/// preference, narrowed to the active interest tags (when any) and to a single
/// author filter (when present). Counts are totals over the scope, not the
/// returned page, so the Platforms mix stays stable across pagination.
///
/// Every [`filter::PLATFORM_BUCKETS`] key is present, 0 when the scope holds
/// no post from that network — the UI shows the honest zero.
pub async fn fetch_platform_counts(
    db: &sqlx::PgPool,
    interest_tags: &[String],
    author: Option<&str>,
    nsfw_pref: NsfwPref,
) -> std::collections::BTreeMap<String, i64> {
    let (tags, author) = counts_scope(interest_tags, author);
    let key = format!(
        "counts:{}|a={}|n={}",
        tags.join(","),
        author.as_deref().unwrap_or(""),
        nsfw_pref.snapshot_axis(),
    );
    // Serve-first under the snapshots staleness ladder (feed/snapshots.rs).
    // The totals aggregate visits every eligible row — 17-30s on the 3.1M-row
    // test DB, un-indexable (an exact all-time total cannot skip rows) — so
    // it runs at most once per key per staleness window, off the request
    // path after the first build. Totals change only as posts land; the
    // platform already serves feed responses up to STALE_SECS old, and the
    // counts inside those snapshots are no fresher than this ladder.
    if let Some(snap) = super::snapshots::load(db, &key).await {
        if let Ok(counts) =
            serde_json::from_value::<std::collections::BTreeMap<String, i64>>(snap.payload)
        {
            if snap.age_secs > super::snapshots::STALE_SECS {
                super::snapshots::spawn_rebuild_with(key.clone(), {
                    let db = db.clone();
                    let tags = tags.clone();
                    let author = author.clone();
                    let key = key.clone();
                    move || async move {
                        if let Some(counts) = fetch_platform_counts_uncached(
                            &db,
                            &tags,
                            author.as_deref(),
                            nsfw_pref,
                        )
                        .await
                        {
                            if let Ok(value) = serde_json::to_value(&counts) {
                                super::snapshots::store(&db, &key, &value).await;
                            }
                        }
                    }
                });
            }
            return counts;
        }
    }
    match fetch_platform_counts_uncached(db, &tags, author.as_deref(), nsfw_pref).await {
        Some(counts) => {
            if let Ok(value) = serde_json::to_value(&counts) {
                super::snapshots::store(db, &key, &value).await;
            }
            counts
        }
        // Query failure: the honest all-zero fallback, never cached — a
        // transient error must not poison the key for the staleness window.
        None => filter::bucket_counts(Vec::new()),
    }
}

async fn fetch_platform_counts_uncached(
    db: &sqlx::PgPool,
    tags: &[String],
    author: Option<&str>,
    nsfw_pref: NsfwPref,
) -> Option<std::collections::BTreeMap<String, i64>> {
    // Same scan gate as the other whole-table readers: the totals aggregate
    // is the largest remaining scan and must not run alongside them.
    let _permit = crate::api::feedgen::SCAN_SLOTS
        .acquire()
        .await
        .expect("semaphore closed");
    let author = author.map(|a| a.to_string());

    let tags_param = (!tags.is_empty()).then_some(1usize);
    let author_param = author.as_ref().map(|_| if tags_param.is_some() { 2 } else { 1 });
    let sql = filter::platform_counts_sql(
        &filter::sql_labels_filter("labels", nsfw_pref),
        tags_param,
        author_param,
    );

    let mut query = sqlx::query_as::<_, (String, i64)>(&sql);
    if tags_param.is_some() {
        query = query.bind(tags);
    }
    if let Some(a) = author {
        query = query.bind(a);
    }
    let rows = match query.fetch_all(db).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("[FEED] platform counts query failed: {e}");
            return None;
        }
    };
    Some(filter::bucket_counts(rows))
}

/// Rows from the appview index for one platform bucket, scoped to the active
/// interest tags. Used when a platform filter is active: the general tiers
/// fetch a mixed, recency-ordered candidate window, so a sparse network (Lemmy,
/// Pixelfed) could be absent from that window even though the bucket has posts.
/// This adds a bucket-targeted pool so every bucket with a non-zero count can
/// actually be selected and read.
pub async fn fetch_appview_by_platform(
    db: &sqlx::PgPool,
    interest_tags: &[String],
    platform: &str,
    limit: i64,
    sort: SortMode,
    nsfw_pref: NsfwPref,
) -> Vec<FeedPost> {
    // Local/native content lives in the posts/RSS tables, not the appview
    // index — nothing to add here.
    if platform == "bytescape" {
        return Vec::new();
    }
    let tags: Vec<String> = interest_tags
        .iter()
        .map(|t| t.trim_start_matches('#').to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    // `platform` is a validated bucket key (parse_platform), never raw input.
    let source_predicate = if platform == "bluesky" {
        "uri NOT LIKE 'https://%'".to_string()
    } else {
        format!(
            "(uri LIKE 'https://%' AND {} = '{platform}')",
            normalize::source_case_sql("uri")
        )
    };

    let tags_param = (!tags.is_empty()).then_some(1usize);
    let limit_param = if tags_param.is_some() { 2 } else { 1 };

    // Phase 1 — thin candidate walk. Selecting the fat row during the ORDER
    // BY forced a corpus scan + top-N sort whenever the bucket predicate
    // could not early-stop (24-39s on the 3.1M-row test DB). Selecting only
    // `uri` lets Popular walk `idx_appview_posts_popular_rank` in score order
    // (index-only: the bucket/labels/created filters are all on indexed
    // columns) and stop at the first `limit` matches — and the fat row is
    // fetched once per finalist below, never per candidate.
    let mut cand_sql = format!(
        "SELECT uri FROM appview_posts \
         WHERE reply_parent IS NULL AND text IS NOT NULL AND text != '' \
           AND created_at <= now() AND {} AND {}",
        filter::sql_labels_filter("labels", nsfw_pref),
        source_predicate,
    );
    if tags_param.is_some() {
        cand_sql.push_str(" AND uri IN (SELECT uri FROM appview_post_tags WHERE tag = ANY($1))");
    }
    cand_sql.push_str(&format!(" ORDER BY {} LIMIT ${limit_param}", sort.order_by()));

    let mut cand_query = sqlx::query_as::<_, (String,)>(&cand_sql);
    if tags_param.is_some() {
        cand_query = cand_query.bind(tags.as_slice());
    }
    cand_query = cand_query.bind(limit);
    let candidate_uris: Vec<String> = cand_query
        .fetch_all(db)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("[FEED] platform-targeted candidates failed: {e}");
            Vec::new()
        })
        .into_iter()
        .map(|(uri,)| uri)
        .collect();
    if candidate_uris.is_empty() {
        return Vec::new();
    }

    // Phase 2 — hydrate the finalists by primary key, keeping the same
    // ordering contract as the candidate walk (≤`limit` rows, trivial sort).
    let sql = format!(
        "SELECT uri, cid, author_did, text, created_at, embeds, labels, author_handle, \
                author_avatar, author_display_name, like_count, repost_count, reply_count \
         FROM appview_posts \
         WHERE uri = ANY($1::text[]) \
         ORDER BY {} LIMIT $2",
        sort.order_by(),
    );

    let mut query = sqlx::query_as::<
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
            Option<i32>,
            Option<i32>,
            Option<i32>,
        ),
    >(&sql);
    query = query.bind(&candidate_uris);
    query = query.bind(limit);
    let rows = query.fetch_all(db).await.unwrap_or_else(|e| {
        tracing::warn!("[FEED] platform-targeted fetch failed: {e}");
        Vec::new()
    });

    let pairs: Vec<(String, String)> = interest_tags
        .iter()
        .map(|t| (t.clone(), t.trim_start_matches('#').to_lowercase()))
        .collect();
    rows.into_iter()
        .filter_map(
            |(uri, cid, did, text, created, embeds, labels, handle, avatar, display_name, likes, reposts, replies)| {
                let body = text.as_deref().unwrap_or("");
                let chips = match_interest_chips(&pairs, body);
                let mut post = normalize::normalize_appview_post(
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
                )?;
                post.matched_sources.extend(chips);
                post.matched_sources.sort_by_key(|m| m.rank);
                Some(post)
            },
        )
        .take(limit.max(0) as usize)
        .collect()
}

/// Recent ActivityPub rows (`https://…`) across the fediverse, filtered to the
/// user's interests (hashtag *or* keyword match in text — no network call).
/// This keeps AP content responsive to interests like the Bluesky tiers, instead
/// of a raw firehose: only posts matching one of `interest_tags` are returned,
/// so the caller can stamp the matched-interest source chip (via `tag_matches`).
/// Backed by the `idx_appview_posts_ap_recent` partial index; a wider candidate
/// window is scanned then filtered in memory (matches are the minority).
pub async fn fetch_appview_ap(
    db: &sqlx::PgPool,
    interest_tags: &[String],
    limit: i64,
    sort: SortMode,
) -> Vec<FeedPost> {
    if interest_tags.is_empty() {
        return Vec::new();
    }
    let order_clause = sort.order_by();
    let time_filter = sort.time_filter();
    let tags: Vec<String> = interest_tags
        .iter()
        .map(|t| t.trim_start_matches('#').to_lowercase())
        .collect();

    // Tag-indexed candidate lookup (same mechanism as
    // fetch_appview_hashtags_uncached). The OLD implementation scanned
    // appview_posts by recency (LIMIT 1000) and text-matched below — a high
    // volume AP firehose exhausts that 1000-row window in ~an hour, so
    // interest posts older than the window never matched (the consumer-visible
    // "fediverse tier is empty on prod" bug). appview_post_tags carries both
    // '#tag' and 'kw:tag' rows keyed by uri, so a per-tag (tag, created_at)
    // index walk finds matching posts regardless of feed volume. Candidates ARE
    // matches, so the caller's limit (with a modest overscan for downstream
    // dedup) is the bound — not a 20x recency window.
    let per_tag = (limit + 8).max(12);
    let candidate_limit = (limit + 16).max(24);
    let mut candidates: Vec<(String, chrono::DateTime<chrono::Utc>)> = Vec::new();
    for batch in tags.chunks(32) {
        let futs = batch.iter().map(|tag| {
            let db = db;
            async move {
                sqlx::query_as::<_, (String, Option<chrono::DateTime<chrono::Utc>>)>(
                    r#"SELECT uri, created_at FROM appview_post_tags
                       WHERE tag = $1
                       ORDER BY created_at DESC NULLS LAST
                       LIMIT $2"#,
                )
                .bind(tag)
                .bind(per_tag)
                .fetch_all(db)
                .await
                .unwrap_or_default()
            }
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
        .take(candidate_limit as usize)
        .collect();
    if candidate_uris.is_empty() {
        return Vec::new();
    }

    let sql = format!(
        r#"SELECT uri, cid, author_did, text, created_at, embeds, labels, author_handle,
                  author_avatar, author_display_name,
                  like_count, repost_count, reply_count
           FROM appview_posts
           WHERE uri = ANY($1::text[])
             AND reply_parent IS NULL
             AND text IS NOT NULL AND text != ''
             AND uri LIKE 'https://%'
             AND created_at <= now()
             {}
           ORDER BY {}
           LIMIT $2"#,
        time_filter, order_clause,
    );
    let rows: Vec<(
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
        Option<i32>,
        Option<i32>,
        Option<i32>,
    )> = sqlx::query_as(&sql)
        .bind(&candidate_uris)
        .bind(limit)
        .fetch_all(db)
        .await
        .unwrap_or_default();

    // Chip truth is unchanged: a chip is `#tag` (kind "hashtag") ONLY for a real
    // hashtag occurrence of the entire tag in the text (boundary-checked).
    // Word/inferred matches earn NO chip but still count for membership (which is
    // now decided by the tag index — appview_post_tags carries keyword rows too).
    let pairs: Vec<(String, String)> = interest_tags
        .iter()
        .map(|t| (t.clone(), t.trim_start_matches('#').to_lowercase()))
        .collect();

    rows.into_iter()
        .filter_map(
            |(uri, cid, did, text, created, embeds, labels, handle, avatar, display_name, likes, reposts, replies)| {
                let body = text.as_deref().unwrap_or("");
                let chips = match_interest_chips(&pairs, body);
                let mut post = normalize::normalize_appview_post(
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
                )?;
                post.matched_sources.extend(chips);
                post.matched_sources.sort_by_key(|m| m.rank);
                Some(post)
            },
        )
        .take(limit.max(0) as usize)
        .collect()
}

/// Fetch AppView posts matching any of the user's interest hashtags.
/// Uses ILIKE ANY(array) for a single efficient query.
pub async fn fetch_appview_hashtags(
    db: &sqlx::PgPool,
    interest_tags: &[String],
    limit: i64,
    sort: super::sort::SortMode,
) -> Vec<FeedPost> {
    if interest_tags.is_empty() {
        return Vec::new();
    }
    let cache_key = format!("{}|{:?}", interest_tags.join(","), sort);
    if let Some(hit) = cache_get(&TAG_CACHE, &cache_key) {
        return hit.into_iter().take(limit.max(0) as usize).collect();
    }
    let result = fetch_appview_hashtags_uncached(db, interest_tags, 60, sort).await;
    if !result.is_empty() {
        cache_put(&TAG_CACHE, cache_key, result.clone());
    }
    result.into_iter().take(limit.max(0) as usize).collect()
}

async fn fetch_appview_hashtags_uncached(
    db: &sqlx::PgPool,
    interest_tags: &[String],
    limit: i64,
    sort: super::sort::SortMode,
) -> Vec<FeedPost> {
    if interest_tags.is_empty() {
        return Vec::new();
    }
    // Same scan gate as query_feed_rows: long scans must not hold pool
    // connections in parallel — that starved login ("pool timed out").
    let _permit = crate::api::feedgen::SCAN_SLOTS
        .acquire()
        .await
        .expect("semaphore closed");

    let tags: Vec<String> = interest_tags
        .iter()
        .map(|t| t.trim_start_matches('#').to_lowercase())
        .collect();

    // Sort mode drives SELECTION, not just ordering: Popular picks the
    // highest-engagement posts in the window (24h), New picks the newest.
    // The tags table (migration 064) turns tag search into an index lookup —
    // text ILIKE scans rechecked tens of thousands of rows per query.
    let (time_window, order_by) = match sort {
        super::sort::SortMode::Popular => (sort.time_filter(), sort.order_by()),
        _ => ("", "created_at DESC NULLS LAST"),
    };
    type Row = (
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
    );
    // Per-tag bounded lookups: `tag = ANY(...)` cannot early-stop across
    // values (Postgres materialized millions of tag rows before LIMIT — 33s),
    // but per-tag `ORDER BY created_at DESC LIMIT n` walks the (tag,
    // created_at) index and stops at n. Merge the small results in memory.
    let per_tag = ((limit / 4).max(8)) as i64;
    let candidate_limit = (limit * 8).max(240);
    let mut candidates: Vec<(String, chrono::DateTime<chrono::Utc>)> = Vec::new();
    for batch in tags.chunks(32) {
        let futs = batch.iter().map(|tag| {
            let db = db;
            async move {
                sqlx::query_as::<_, (String, Option<chrono::DateTime<chrono::Utc>>)>(
                    r#"SELECT uri, created_at FROM appview_post_tags
                       WHERE tag = $1
                       ORDER BY created_at DESC NULLS LAST
                       LIMIT $2"#,
                )
                .bind(tag)
                .bind(per_tag)
                .fetch_all(db)
                .await
                .unwrap_or_default()
            }
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
        .take(candidate_limit as usize)
        .collect();
    if candidate_uris.is_empty() {
        return Vec::new();
    }

    let sql = format!(
        r#"SELECT p.uri, p.cid, p.author_did, p.author_handle, p.author_avatar, p.author_display_name, p.text, p.created_at, p.embeds, p.labels, p.like_count, p.repost_count, p.reply_count
           FROM appview_posts p
           WHERE p.uri = ANY($1::text[])
             AND p.reply_parent IS NULL
                          AND p.created_at <= now()
             AND {}
             {}
           ORDER BY {}
           LIMIT $2"#,
        super::filter::sql_labels_clean("p.labels"),
        time_window,
        order_by,
    );
    let rows = sqlx::query_as::<_, Row>(&sql)
        .bind(&candidate_uris)
        .bind(limit)
        .fetch_all(db)
        .await
        .unwrap_or_default();

    let posts: Vec<FeedPost> = rows
        .into_iter()
        .filter_map(|(uri, cid, did, handle, avatar, display_name, text, created, embeds, labels, likes, reposts, replies)| {
            normalize::normalize_appview_post(
                uri, cid, did, handle, avatar, display_name, text, created, embeds,
                normalize::labels_from_json(&labels),
                likes.unwrap_or(0), reposts.unwrap_or(0), replies.unwrap_or(0),
            )
        })
        .collect();

    tracing::info!("[FEED] fetch_appview_hashtags: {} posts from {} tags", posts.len(), interest_tags.len());
    posts
}

/// Shared normalizer for the group/subscriptions fetchers: attach the
/// [`GroupRef`] header and vote counts to every normalized post so the group
/// post card can render its header + ratio without a second lookup.
#[allow(clippy::type_complexity)]
fn group_rows_to_posts(
    rows: Vec<(
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
        Option<i32>,
        Option<i32>,
        Option<i32>,
        Option<i32>,
        Option<i32>,
    )>,
    group: super::GroupRef,
    limit: i64,
) -> Vec<FeedPost> {
    rows.into_iter()
        .filter_map(
            |(uri, cid, did, text, created, embeds, labels, handle, avatar, display_name, likes, reposts, replies, upvotes, downvotes)| {
                let mut post = normalize::normalize_appview_post(
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
                )?;
                post.group = Some(group.clone());
                post.group_name = Some(group.name.clone());
                post.upvotes = Some(upvotes.unwrap_or(0));
                post.downvotes = Some(downvotes.unwrap_or(0));
                Some(post)
            },
        )
        .take(limit.max(0) as usize)
        .collect()
}

/// Fetch the [`GroupRef`] header for one group actor id (for attaching to its
/// posts). `None` when the registry has no row yet.
async fn group_ref_for(db: &sqlx::PgPool, actor_id: &str) -> Option<super::GroupRef> {
    let (name, title, instance, kind, nsfw): (String, Option<String>, String, String, bool) =
        sqlx::query_as(
            "SELECT name, title, instance, kind, nsfw FROM ap_groups WHERE actor_id = $1",
        )
        .bind(actor_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()?;
    Some(super::GroupRef {
        actor_id: actor_id.to_string(),
        name,
        title,
        instance,
        kind,
        nsfw,
    })
}

/// Posts of one group (community) by its canonical AP actor id — the
/// `groups:<actor_id>` feed source (design D6). Backed by the
/// `idx_appview_posts_group` partial index (group_actor_id, recency).
pub async fn fetch_appview_group(
    db: &sqlx::PgPool,
    group_actor_id: &str,
    limit: i64,
    sort: super::sort::SortMode,
) -> Vec<FeedPost> {
    if group_actor_id.is_empty() {
        return Vec::new();
    }
    let Some(group) = group_ref_for(db, group_actor_id).await else {
        return Vec::new();
    };
    // Order by `created_at` to match `idx_appview_posts_group (group_actor_id,
    // created_at DESC)` — the fetchers' `SortMode` ordering (`indexed_at`)
    // cannot use that partial index, so it forced a full scan + sort that hung
    // for minutes under load. A group feed is recency-ordered anyway. The
    // LIMIT is bounded by the index walk.
    let time_filter = sort.time_filter();
    let sql = format!(
        r#"SELECT uri, cid, author_did, text, created_at, embeds, labels, author_handle,
                  author_avatar, author_display_name, like_count, repost_count, comment_count,
                  upvote_count, downvote_count
           FROM appview_posts
           WHERE group_actor_id = $1
             AND reply_parent IS NULL
             AND text IS NOT NULL AND text != ''
             AND created_at <= now()
             {}
           ORDER BY created_at DESC NULLS LAST
           LIMIT $2"#,
        time_filter,
    );
    let rows: Vec<(
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
        Option<i32>,
        Option<i32>,
        Option<i32>,
        Option<i32>,
        Option<i32>,
    )> = sqlx::query_as(&sql)
        .bind(group_actor_id)
        .bind(limit)
        .fetch_all(db)
        .await
        .unwrap_or_default();
    group_rows_to_posts(rows, group, limit)
}

/// Posts across every group the user follows — the `subscriptions` feed
/// source (design D4/D6). Resolves the user's enabled `ap_group` subscription
/// rows to group actor ids and filters `appview_posts.group_actor_id`. Each
/// post carries its own group's [`GroupRef`].
pub async fn fetch_appview_subscriptions(
    db: &sqlx::PgPool,
    user_id: uuid::Uuid,
    limit: i64,
    sort: super::sort::SortMode,
) -> Vec<FeedPost> {
    let actor_ids: Vec<String> = match crate::db::subscriptions::SubscriptionRow::find_by_user_and_category(
        db,
        user_id,
        "groups",
    )
    .await
    {
        Ok(rows) => rows
            .into_iter()
            .filter(|r| r.source == "ap_group" && r.enabled)
            .map(|r| r.source_id)
            .collect(),
        Err(_) => return Vec::new(),
    };
    if actor_ids.is_empty() {
        return Vec::new();
    }
    // One group header per subscribed group, resolved up front.
    let mut group_map: std::collections::HashMap<String, super::GroupRef> =
        std::collections::HashMap::new();
    for actor_id in &actor_ids {
        if let Some(g) = group_ref_for(db, actor_id).await {
            group_map.insert(actor_id.clone(), g);
        }
    }
    let time_filter = sort.time_filter();
    let sql = format!(
        r#"SELECT uri, cid, author_did, text, created_at, embeds, labels, author_handle,
                  author_avatar, author_display_name, like_count, repost_count, comment_count, group_actor_id,
                  upvote_count, downvote_count
           FROM appview_posts
           WHERE group_actor_id = ANY($1::text[])
             AND reply_parent IS NULL
             AND text IS NOT NULL AND text != ''
             AND created_at <= now()
             {}
           ORDER BY created_at DESC NULLS LAST
           LIMIT $2"#,
        time_filter,
    );
    type SubRow = (
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
        Option<i32>,
        Option<i32>,
        Option<i32>,
        Option<String>,
        Option<i32>,
        Option<i32>,
    );
    let rows: Vec<SubRow> = sqlx::query_as(&sql)
        .bind(&actor_ids)
        .bind(limit)
        .fetch_all(db)
        .await
        .unwrap_or_default();
    // Regroup by the post's own group so each gets the right header.
    type GroupedRow = (String, String, String, Option<String>, Option<chrono::DateTime<chrono::Utc>>, Option<serde_json::Value>, Option<serde_json::Value>, Option<String>, Option<String>, Option<String>, Option<i32>, Option<i32>, Option<i32>, Option<i32>, Option<i32>);
    let mut by_group: std::collections::HashMap<String, Vec<GroupedRow>> =
        std::collections::HashMap::new();
    for (uri, cid, did, text, created, embeds, labels, handle, avatar, display_name, likes, reposts, replies, g, upvotes, downvotes) in rows {
        let key = g.unwrap_or_default();
        by_group.entry(key).or_default().push((
            uri, cid, did, text, created, embeds, labels, handle, avatar, display_name, likes,
            reposts, replies, upvotes, downvotes,
        ));
    }
    let mut out = Vec::new();
    for (actor_id, rows) in by_group {
        let Some(group) = group_map.get(&actor_id).cloned() else {
            continue;
        };
        out.extend(group_rows_to_posts(rows, group, limit));
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out.truncate(limit.max(0) as usize);
    out
}

/// The groups view's tab scopes. Mirrors the main feed's category tabs and
/// the events feed's `source` scopes (owner model: Groups is a specialized
/// feed slice — same layout, filtered content).
pub const GROUP_SCOPES: &[&str] = &["home", "following", "feeds", "hashtags", "subscriptions"];

/// The viewer's context for the group-scope query: followed author DIDs,
/// interest tags, feed topic terms, and hashtag subscription tokens. Resolved
/// once per request, exactly like the main feed and events scopes.
pub struct GroupScopeCtx {
    pub followed_dids: Vec<String>,
    pub interest_tags: Vec<String>,
    pub feed_topics: Vec<String>,
    pub hashtag_tags: Vec<String>,
    pub subscribed_groups: Vec<String>,
}

impl GroupScopeCtx {
    /// Resolve the viewer's context from their follow graph + subscriptions.
    /// Anonymous / no-follows leaves the relevant vectors empty (the scope
    /// query then ANDs `false` for that branch, matching the events feed).
    pub async fn resolve(db: &sqlx::PgPool, user_id: Option<uuid::Uuid>) -> Self {
        let mut ctx = GroupScopeCtx {
            followed_dids: Vec::new(),
            interest_tags: Vec::new(),
            feed_topics: Vec::new(),
            hashtag_tags: Vec::new(),
            subscribed_groups: Vec::new(),
        };
        let Some(user_id) = user_id else { return ctx };
        // Followed AP/AT authors → their DIDs/actor URLs. The group post's
        // `author_did` holds the AP actor URL for AP rows.
        if let Ok(rows) = crate::db::follows::FollowRow::list_by_user(db, user_id).await {
            for f in rows {
                if !f.followed_actor.is_empty() {
                    ctx.followed_dids.push(f.followed_actor);
                }
            }
        }
        // Subscriptions drive interests + feed topics + hashtags + groups —
        // the same truth source as `filter::get_interest_tags`.
        if let Ok(rows) = crate::db::subscriptions::SubscriptionRow::find_by_user(db, user_id).await {
            for row in rows.iter().filter(|r| r.enabled) {
                match row.category.as_str() {
                    "hashtags" => {
                        let tag = row
                            .source_id
                            .trim_start_matches("mastodon:")
                            .trim_start_matches('#')
                            .to_lowercase();
                        if !tag.is_empty() {
                            ctx.hashtag_tags.push(tag.clone());
                            ctx.interest_tags.push(tag);
                        }
                    }
                    "feeds" => {
                        // Topic feeds carry keyword/tag terms in source_id.
                        let term = row.source_id.trim_start_matches('#').to_lowercase();
                        if !term.is_empty() {
                            ctx.feed_topics.push(term);
                        }
                    }
                    "groups" if row.source == "ap_group" => {
                        ctx.subscribed_groups.push(row.source_id.clone());
                    }
                    _ => {}
                }
            }
        }
        ctx.followed_dids.sort();
        ctx.followed_dids.dedup();
        ctx.interest_tags.sort();
        ctx.interest_tags.dedup();
        ctx.feed_topics.sort();
        ctx.feed_topics.dedup();
        ctx.hashtag_tags.sort();
        ctx.hashtag_tags.dedup();
        ctx
    }

    /// The viewer's interest signals as one tag list (interest hashtags ∪
    /// feed topic terms) for `appview_post_tags` matching — the index carries
    /// both hashtag and keyword rows.
    pub fn interest_tags_union(&self) -> Vec<String> {
        let mut tags = self.interest_tags.clone();
        tags.extend(self.feed_topics.iter().cloned());
        tags.sort();
        tags.dedup();
        tags
    }
}

/// One parameterized group-variant query: the Groups view's content
/// (owner model: "a specialized version of the main feed", interest-first,
/// never blank). Each returned group post carries its `GroupRef` header +
/// vote counts (via [`group_rows_to_posts`]).
///
/// The explicit filter scopes keep their meaning — a non-empty scope result
/// is the answer:
///
/// * `following` — group posts by followed authors.
/// * `feeds` — group posts matching the user's feed topic terms.
/// * `hashtags` — group posts matching literal `appview_post_tags` rows.
/// * `subscriptions` — group posts in subscribed groups only.
///
/// Everything else (`home`/unknown, and any empty scope) is served the
/// DEFAULT population ([`fetch_group_default_population`]): group-community
/// matches for followed authors ∪ interest topics ∪ interest hashtags, then
/// the viewer's own hashtag/interest stream, then popular community content
/// — the section is NEVER blank by default (owner fix: "group content
/// appears without joining anything (interests + followed users)").
pub async fn fetch_appview_group_scope(
    db: &sqlx::PgPool,
    scope: &str,
    ctx: &GroupScopeCtx,
    limit: i64,
) -> Vec<FeedPost> {
    // Explicit filter scopes run first; an empty result falls through to the
    // default population (owner fix: the Groups tab populates by default —
    // the subscriptions tab is NOT a join gate).
    match scope {
        "following" | "feeds" | "hashtags" | "subscriptions" => {
            let primary = fetch_appview_group_scope_filtered(db, scope, ctx, limit).await;
            if !primary.is_empty() {
                return primary;
            }
        }
        _ => {}
    }
    fetch_group_default_population(db, ctx, limit).await
}

/// The scope-filtered group query (`following`/`feeds`/`hashtags`/
/// `subscriptions`): `group_actor_id IS NOT NULL` posts matching the scope's
/// single predicate — the `wheres`/`source_terms` shape of the events feed
/// (design D10). Empty when the viewer has no context for the scope.
async fn fetch_appview_group_scope_filtered(
    db: &sqlx::PgPool,
    scope: &str,
    ctx: &GroupScopeCtx,
    limit: i64,
) -> Vec<FeedPost> {
    let (wheres, terms): (Vec<String>, Vec<String>) = match scope {
        "following" => {
            if ctx.followed_dids.is_empty() {
                return Vec::new();
            }
            (
                vec!["AND p.author_did = ANY($2::text[])".to_string()],
                ctx.followed_dids.clone(),
            )
        }
        "feeds" => {
            if ctx.feed_topics.is_empty() {
                return Vec::new();
            }
            // Topic terms match the group post's literal tag rows or text.
            (
                vec![
                    "AND EXISTS (SELECT 1 FROM appview_post_tags t WHERE t.uri = p.uri AND t.tag = ANY($2::text[]))".to_string(),
                ],
                ctx.feed_topics.clone(),
            )
        }
        "hashtags" => {
            if ctx.hashtag_tags.is_empty() {
                return Vec::new();
            }
            (
                vec![
                    "AND EXISTS (SELECT 1 FROM appview_post_tags t WHERE t.uri = p.uri AND t.tag = ANY($2::text[]))".to_string(),
                ],
                ctx.hashtag_tags.clone(),
            )
        }
        // "subscriptions" — group posts in subscribed groups only.
        _ => {
            if ctx.subscribed_groups.is_empty() {
                return Vec::new();
            }
            (
                vec!["AND p.group_actor_id = ANY($2::text[])".to_string()],
                ctx.subscribed_groups.clone(),
            )
        }
    };
    fetch_appview_group_scope_sql(db, scope, &wheres, &terms, ctx, limit).await
}

/// The default Groups population (owner fix: never blank by default):
///
/// 1. [`fetch_home_union`] — group-community content matched by followed
///    authors ∪ interest topics ∪ interest hashtags (design D11);
/// 2. the viewer's own hashtag/interest stream ([`fetch_appview_hashtags`])
///    fills the rest of the page — "at minimum show content for the owner's
///    followed hashtags + interests";
/// 3. nothing matched at all (no interests, no follows) → popular content
///    from known community sources ([`fetch_popular_group_content`]).
async fn fetch_group_default_population(
    db: &sqlx::PgPool,
    ctx: &GroupScopeCtx,
    limit: i64,
) -> Vec<FeedPost> {
    let mut out = fetch_home_union(db, ctx, limit).await;
    if out.len() < limit.max(0) as usize {
        let tags = ctx.interest_tags_union();
        if !tags.is_empty() {
            let fill = fetch_appview_hashtags(db, &tags, limit, super::sort::SortMode::New).await;
            extend_deduped(&mut out, fill, limit.max(0) as usize);
        }
    }
    if out.is_empty() {
        out = fetch_popular_group_content(db, ctx, limit).await;
    }
    out
}

/// Video-bearing posts in the viewer's follow/interest/hashtag scope — the
/// stories rail's population source (owner: "Loops should do it, and hashtag
/// content too"). Loops videos federate as AP `/ap/users/{id}/video/{id}`
/// objects; Pixelfed/Mastodon video posts fill the same pool. Newest first;
/// NOT group-restricted (video moments live outside communities). Empty when
/// the viewer follows nothing video-ish — the rail then shows its honest
/// empty state.
pub async fn fetch_interest_video_posts(
    db: &sqlx::PgPool,
    ctx: &GroupScopeCtx,
    limit: i64,
) -> Vec<FeedPost> {
    let Some((combined, terms)) = home_union_clause(&ctx.followed_dids, &ctx.interest_tags_union())
    else {
        return Vec::new();
    };
    let sql = format!(
        r#"SELECT p.uri, p.cid, p.author_did, p.text, p.created_at, p.embeds, p.labels,
                  p.author_handle, p.author_avatar, p.author_display_name,
                  p.like_count, p.repost_count, GREATEST(p.reply_count, p.comment_count),
                  p.upvote_count, p.downvote_count
           FROM appview_posts p
           WHERE p.reply_parent IS NULL
             AND p.created_at <= now()
             AND EXISTS (
               SELECT 1 FROM jsonb_array_elements(COALESCE(p.embeds, '[]'::jsonb)) e
               WHERE e->>'type' = 'video'
                  OR e->>'url' ~* '\.(mp4|webm|mov|m4v|m3u8)([?#].*)?$'
             )
             {combined}
           ORDER BY p.created_at DESC NULLS LAST
           LIMIT $1"#,
    );
    let mut query = sqlx::query_as::<
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
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
        ),
    >(&sql)
    .bind(limit);
    // Binds mirror home_union_clause: $2 = follows (when present) and $3 =
    // tags; with a single branch everything lands in $2.
    let interest_tags_union = ctx.interest_tags_union();
    if combined.contains("$3") {
        query = query.bind(&ctx.followed_dids).bind(&interest_tags_union);
    } else {
        query = query.bind(&terms);
    }
    let rows = query.fetch_all(db).await.unwrap_or_default();
    rows.into_iter()
        .filter_map(
            |(uri, cid, did, text, created, embeds, labels, handle, avatar, display_name, likes, reposts, replies, upvotes, downvotes)| {
                let mut post = normalize::normalize_appview_post(
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
                )?;
                post.upvotes = Some(upvotes.unwrap_or(0));
                post.downvotes = Some(downvotes.unwrap_or(0));
                Some(post)
            },
        )
        .collect()
}

/// The home OR-union over the group pool: followed authors' group posts ∪
/// posts tagged with the viewer's interest topics/hashtags (design D11).
/// Empty when the viewer has no follow or interest context.
async fn fetch_home_union(
    db: &sqlx::PgPool,
    ctx: &GroupScopeCtx,
    limit: i64,
) -> Vec<FeedPost> {
    let Some((combined, terms)) = home_union_clause(&ctx.followed_dids, &ctx.interest_tags_union())
    else {
        return Vec::new();
    };
    fetch_appview_group_scope_sql(db, "home", &[combined], &terms, ctx, limit).await
}

/// The home OR-union predicate shaping — pure, unit-tested below. The two
/// branches are OR-joined inside one parenthesized AND-clause; each branch
/// addresses its own bind (`$2`, and `$3` when both are present) with values
/// packed `[follows…, tags…]`. `None` when the viewer has no context.
/// (Regression: d242043 — the predicates need the AND prefix and the OR
/// join, or the union silently returns nothing.)
fn home_union_clause(
    followed_dids: &[String],
    tags: &[String],
) -> Option<(String, Vec<String>)> {
    let mut branches: Vec<String> = Vec::new();
    let mut terms: Vec<String> = Vec::new();
    let mut bind_idx = 2usize;
    if !followed_dids.is_empty() {
        branches.push(format!("(p.author_did = ANY(${bind_idx}::text[]))"));
        terms.extend(followed_dids.iter().cloned());
        bind_idx += 1;
    }
    if !tags.is_empty() {
        branches.push(format!(
            "(EXISTS (SELECT 1 FROM appview_post_tags t WHERE t.uri = p.uri AND t.tag = ANY(${bind_idx}::text[])))"
        ));
        terms.extend(tags.iter().cloned());
    }
    if branches.is_empty() {
        return None;
    }
    Some((format!("AND ({})", branches.join(" OR ")), terms))
}

/// Layer 3 of the default population: popular content from known community
/// sources — the most engaging recent posts across the `ap_groups` registry
/// (Lemmy/PieFed/Mbin communities). Ranks a bounded recency window (the
/// `idx_appview_posts_group` index walk) in memory — never a scan (design
/// D10's pool-starvation lesson).
async fn fetch_popular_group_content(
    db: &sqlx::PgPool,
    ctx: &GroupScopeCtx,
    limit: i64,
) -> Vec<FeedPost> {
    const POPULAR_WINDOW: i64 = 200;
    let mut out =
        fetch_appview_group_scope_sql(db, "popular", &[], &[], ctx, POPULAR_WINDOW).await;
    out.sort_by(|a, b| engagement_score(b).cmp(&engagement_score(a)));
    out.truncate(limit.max(0) as usize);
    out
}

/// Engagement score for the popular fallback ranking — the feed's Popular
/// weighting (`likes*3 + shares*2 + replies`) plus group upvotes.
fn engagement_score(p: &FeedPost) -> u64 {
    u64::from(p.likes_count) * 3
        + u64::from(p.shares_count) * 2
        + u64::from(p.replies_count)
        + p.upvotes.unwrap_or(0).max(0) as u64
}

/// Merge the default-population layers: append `items` onto `out`, skipping
/// ids already present, stopping at `limit` — pure, unit-tested below.
fn extend_deduped(out: &mut Vec<FeedPost>, items: Vec<FeedPost>, limit: usize) {
    let mut seen: std::collections::HashSet<String> = out.iter().map(|p| p.id.clone()).collect();
    for p in items {
        if out.len() >= limit {
            break;
        }
        if seen.insert(p.id.clone()) {
            out.push(p);
        }
    }
}

#[cfg(test)]
mod group_scope_tests {
    use super::{engagement_score, extend_deduped, home_union_clause, GroupScopeCtx};

    fn post(id: &str, likes: i32) -> super::FeedPost {
        super::normalize::normalize_appview_post(
            id.to_string(),
            "cid".to_string(),
            "did:plc:test".to_string(),
            Some("tester".to_string()),
            None,
            None,
            Some("hello".to_string()),
            None,
            None,
            Vec::new(),
            likes,
            0,
            0,
        )
        .expect("normalize builds a post")
    }

    #[test]
    fn home_union_joins_branches_with_or() {
        let (clause, terms) =
            home_union_clause(&["did:a".to_string()], &["cats".to_string()]).expect("two branches");
        assert!(clause.starts_with("AND ("));
        assert!(clause.contains(" OR "));
        assert!(clause.contains("$2") && clause.contains("$3"));
        assert_eq!(terms, vec!["did:a".to_string(), "cats".to_string()]);
    }

    #[test]
    fn home_union_single_branch_binds_only_d2() {
        let (clause, terms) =
            home_union_clause(&[], &["cats".to_string()]).expect("tags branch");
        assert!(!clause.contains("$3"));
        assert!(clause.contains("$2"));
        assert_eq!(terms, vec!["cats".to_string()]);
        let (clause, terms) =
            home_union_clause(&["did:a".to_string()], &[]).expect("follows branch");
        assert!(!clause.contains("$3"));
        assert_eq!(terms, vec!["did:a".to_string()]);
    }

    #[test]
    fn home_union_needs_some_context() {
        assert!(home_union_clause(&[], &[]).is_none());
    }

    #[test]
    fn interest_tags_union_merges_topics_and_hashtags() {
        let ctx = GroupScopeCtx {
            followed_dids: Vec::new(),
            interest_tags: vec!["cats".to_string(), "dogs".to_string()],
            feed_topics: vec!["dogs".to_string(), "space".to_string()],
            hashtag_tags: vec!["cats".to_string(), "dogs".to_string()],
            subscribed_groups: Vec::new(),
        };
        assert_eq!(ctx.interest_tags_union(), vec!["cats", "dogs", "space"]);
    }

    #[test]
    fn extend_deduped_fills_without_duplicates() {
        let mut out = vec![post("a", 0)];
        let fill = vec![post("a", 0), post("b", 0), post("c", 0)];
        extend_deduped(&mut out, fill, 2);
        let ids: Vec<&str> = out.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
    }

    #[test]
    fn engagement_score_weights_likes_shares_replies() {
        let mut p = post("a", 1);
        p.shares_count = 2;
        p.replies_count = 3;
        p.upvotes = Some(4);
        assert_eq!(engagement_score(&p), 1 * 3 + 2 * 2 + 3 + 4);
    }
}

/// Shared SQL execution for [`fetch_appview_group_scope`]. `wheres` are the
/// scope predicates (already `AND …`-prefixed); `terms` are the bind values
/// for `$2` (and `$3` on the home union, packed as `[follows…, interest_tags…]`).
async fn fetch_appview_group_scope_sql(
    db: &sqlx::PgPool,
    scope: &str,
    wheres: &[String],
    terms: &[String],
    ctx: &GroupScopeCtx,
    limit: i64,
) -> Vec<FeedPost> {
    let _permit = crate::api::feedgen::SCAN_SLOTS
        .acquire()
        .await
        .expect("semaphore closed");
    // Ordered by `created_at` to match `idx_appview_posts_group (group_actor_id,
    // created_at DESC)` — the partial index can't satisfy an `indexed_at` sort
    // (design D10). The LIMIT is a bounded index walk.
    let where_extra = wheres.join(" ");
    let sql = format!(
        r#"SELECT p.uri, p.cid, p.author_did, p.text, p.created_at, p.embeds, p.labels,
                  p.author_handle, p.author_avatar, p.author_display_name,
                  p.like_count, p.repost_count, GREATEST(p.reply_count, p.comment_count),
                  p.upvote_count, p.downvote_count, p.group_actor_id
           FROM appview_posts p
           WHERE p.group_actor_id IS NOT NULL
             AND p.reply_parent IS NULL
             AND p.text IS NOT NULL AND p.text != ''
             AND p.created_at <= now()
             {where_extra}
           ORDER BY p.created_at DESC NULLS LAST
           LIMIT $1"#,
    );
    // Home union binds $2 = follows and $3 = the interest/topic tag union;
    // other scopes use only $2. Bind exactly the placeholders the clause
    // references (sqlx rejects extra binds — a stray bind silently empties
    // the query through the `unwrap_or_default` below).
    let uses_d3 = where_extra.contains("$3");
    let follows_bind: Vec<String> = ctx.followed_dids.clone();
    let interests_bind: Vec<String> = ctx.interest_tags_union();
    let mut query = sqlx::query_as::<
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
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<String>,
        ),
    >(&sql)
    .bind(limit);
    if uses_d3 {
        query = query.bind(&follows_bind).bind(&interests_bind);
    } else if where_extra.contains("$2") {
        query = query.bind(terms);
    }
    let rows = query.fetch_all(db).await.unwrap_or_default();

    // Attach each post's GroupRef header + vote counts (group, up/down from
    // the row; my_vote resolved below).
    let mut group_map: std::collections::HashMap<String, super::GroupRef> =
        std::collections::HashMap::new();
    type Row = (
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
        Option<i32>,
        Option<i32>,
        Option<i32>,
        Option<i32>,
        Option<i32>,
        Option<String>,
    );
    let mut by_group: std::collections::HashMap<String, Vec<(String, String, String, Option<String>, Option<chrono::DateTime<chrono::Utc>>, Option<serde_json::Value>, Option<serde_json::Value>, Option<String>, Option<String>, Option<String>, Option<i32>, Option<i32>, Option<i32>, Option<i32>, Option<i32>)>> =
        std::collections::HashMap::new();
    for (uri, cid, did, text, created, embeds, labels, handle, avatar, display_name, likes, reposts, replies, upvotes, downvotes, g) in rows {
        let key = g.unwrap_or_default();
        if !group_map.contains_key(&key) {
            if let Some(gr) = group_ref_for(db, &key).await {
                group_map.insert(key.clone(), gr);
            }
        }
        by_group.entry(key).or_default().push((
            uri, cid, did, text, created, embeds, labels, handle, avatar, display_name, likes,
            reposts, replies, upvotes, downvotes,
        ));
    }
    let mut out = Vec::new();
    for (actor_id, rows) in by_group {
        let Some(group) = group_map.get(&actor_id).cloned() else {
            continue;
        };
        out.extend(group_rows_to_posts(rows, group, limit));
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out.truncate(limit.max(0) as usize);
    stamp_group_provenance(&mut out, scope, ctx);
    out
}

/// Provenance chips (owner round 3): each group post answers "where did this
/// come from" — the branch (follow / topic feed / hashtag / subscription) that
/// matched it, mirroring the events surface's ProvenanceView. Computed
/// server-side per scope and rendered as `matched_sources` chips.
fn stamp_group_provenance(posts: &mut [super::FeedPost], scope: &str, ctx: &GroupScopeCtx) {
    use super::SourceMatch;
    // A group post matches an interest tag via its content OR its community
    // name/title (the ingest writes the community name as a `#community` tag,
    // so interest matches often come through the group identity, not the text).
    let matches_tag = |p: &super::FeedPost, tag: &str| -> bool {
        if super::post_contains_tag_pub(p, tag) {
            return true;
        }
        if let Some(g) = &p.group {
            let t = tag.trim_start_matches('#').to_lowercase();
            return g.name.to_lowercase() == t
                || g.title.as_deref().map(|x| x.to_lowercase()) == Some(t.clone());
        }
        false
    };
    for p in posts.iter_mut() {
        match scope {
            // The popular default-population fallback matched no viewer
            // source — it earns NO provenance chip (chips are "truthful by
            // construction"; an unattributed row is the honest answer).
            "popular" => {}
            "subscriptions" => p.matched_sources.push(SourceMatch {
                label: "Subscriptions".into(),
                kind: "subscriptions".into(),
                rank: 2,
            }),
            "following" => p.matched_sources.push(SourceMatch {
                label: "Following".into(),
                kind: "following".into(),
                rank: 2,
            }),
            "feeds" => {
                let topic = ctx
                    .feed_topics
                    .iter()
                    .find(|t| matches_tag(p, t))
                    .cloned()
                    .unwrap_or_else(|| "Feeds".into());
                p.matched_sources.push(SourceMatch {
                    label: topic,
                    kind: "feeds".into(),
                    rank: 0,
                });
            }
            "hashtags" => {
                let tag = ctx
                    .hashtag_tags
                    .iter()
                    .find(|t| matches_tag(p, t))
                    .cloned()
                    .unwrap_or_else(|| "Hashtags".into());
                p.matched_sources.push(SourceMatch {
                    label: tag,
                    kind: "hashtag".into(),
                    rank: 1,
                });
            }
            // "home" — attribute to the OR-union branch that matched: a
            // followed author first, then a topic feed, then a hashtag.
            _ => {
                let author = p.author_external_id.as_deref().unwrap_or("");
                if !author.is_empty() && ctx.followed_dids.iter().any(|d| d == author) {
                    p.matched_sources.push(SourceMatch {
                        label: "Following".into(),
                        kind: "following".into(),
                        rank: 2,
                    });
                } else if let Some(topic) =
                    ctx.feed_topics.iter().find(|t| matches_tag(p, t))
                {
                    p.matched_sources.push(SourceMatch {
                        label: topic.clone(),
                        kind: "feeds".into(),
                        rank: 0,
                    });
                } else if let Some(tag) =
                    ctx.hashtag_tags.iter().find(|t| matches_tag(p, t))
                {
                    p.matched_sources.push(SourceMatch {
                        label: tag.clone(),
                        kind: "hashtag".into(),
                        rank: 1,
                    });
                } else {
                    // Matched the home query (a group-community interest tag we
                    // can't pin to a specific subscription) — the closest honest
                    // bucket is Subscriptions/interests, never a fake "following".
                    p.matched_sources.push(SourceMatch {
                        label: "Subscriptions".into(),
                        kind: "subscriptions".into(),
                        rank: 2,
                    });
                }
            }
        }
        p.matched_sources.sort_by_key(|m| m.rank);
    }
}

/// Refresh engagement counts from the public AppView (batched getPosts).
/// Local-index counters only increment on events seen since indexing, so
/// posts arrive missing historical likes/reposts/replies — which both
/// misdisplays and mis-sorts, since Popular scores these same fields.
/// Best-effort: on failure the counts stay as they are.
pub async fn refresh_engagement_counts(
    http: &reqwest::Client,
    posts: &mut [FeedPost],
    sort: super::sort::SortMode,
    cap: usize,
) {
    // Refresh the posts most likely to reach the page FIRST, in the active
    // sort's own order — the floor selects from the head of that same order,
    // so every displayed item is guaranteed covered within the cap.
    let mut candidates: Vec<&FeedPost> = posts
        .iter()
        .filter(|p| p.id.starts_with("at://"))
        .collect();
    match sort {
        super::sort::SortMode::Popular => candidates.sort_by_key(|p| {
            std::cmp::Reverse(p.likes_count * 3 + p.shares_count * 2 + p.replies_count)
        }),
        _ => candidates.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
    }
    let uris: Vec<String> = candidates
        .into_iter()
        .map(|p| p.id.clone())
        .take(cap)
        .collect();
    if uris.is_empty() {
        return;
    }
    let futs: Vec<_> = uris
        .chunks(25)
        .map(|chunk| {
            let http = http;
            let qs = chunk
                .iter()
                .map(|u| format!("uris={}", urlencoding::encode(u)))
                .collect::<Vec<_>>()
                .join("&");
            let url = format!("https://api.bsky.app/xrpc/app.bsky.feed.getPosts?{}", qs);
            async move {
                http.get(&url)
                    .timeout(std::time::Duration::from_secs(3))
                    .send()
                    .await
                    .ok()?
                    .json::<serde_json::Value>()
                    .await
                    .ok()
            }
        })
        .collect();
    let mut by_uri: std::collections::HashMap<String, (u32, u32, u32)> =
        std::collections::HashMap::new();
    for json in futures::future::join_all(futs).await.into_iter().flatten() {
        if let Some(arr) = json.get("posts").and_then(|p| p.as_array()) {
            for p in arr {
                let Some(uri) = p.get("uri").and_then(|v| v.as_str()) else { continue };
                by_uri.insert(
                    uri.to_string(),
                    (
                        p.get("likeCount").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                        p.get("repostCount").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                        p.get("replyCount").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    ),
                );
            }
        }
    }
    if by_uri.is_empty() {
        return;
    }
    for p in posts.iter_mut() {
        if let Some(&(likes, reposts, replies)) = by_uri.get(&p.id) {
            // Network numbers are authoritative — plain set, not max: an
            // un-like must disappear from the count too.
            p.likes_count = likes;
            p.shares_count = reposts;
            p.replies_count = replies;
        }
    }
}

/// Fetch Bluesky hot-classic feed (for sort=popular).
/// Public API, no auth required.
pub async fn fetch_bluesky_popular(
    http: &reqwest::Client,
    limit: i64,
) -> Result<Vec<FeedPost>, crate::error::AppError> {
    let hot_url = format!(
        "https://api.bsky.app/xrpc/app.bsky.feed.getFeed?feed=at://did:plc:z72i7hdynmk6r22z27h6tvur/app.bsky.feed.generator/hot-classic&limit={}",
        limit
    );

    let resp = http
        .get(&hot_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| {
            tracing::warn!("[FEED] Bluesky hot-classic error: {}", e);
            crate::error::AppError::Internal(format!("Bluesky API error: {}", e))
        })?;

    if !resp.status().is_success() {
        tracing::warn!("[FEED] Bluesky hot-classic status {}", resp.status());
        return Ok(Vec::new());
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| {
        crate::error::AppError::Internal(format!("Bluesky JSON parse error: {}", e))
    })?;

    let mut posts = Vec::new();
    if let Some(feed) = json.get("feed").and_then(|f| f.as_array()) {
        tracing::info!(
            "[FEED] Bluesky hot-classic returned {} items",
            feed.len()
        );
        for item in feed {
            if let Some(post) = normalize::normalize_bluesky_api_post(item) {
                posts.push(post);
            }
        }
    }

    Ok(posts)
}

/// Fetch any Bluesky feed generator's content by URI (the user's subscribed
/// feeds). Same public API shape as hot-classic.
pub async fn fetch_bluesky_feed_uri(
    http: &reqwest::Client,
    feed_uri: &str,
    limit: i64,
) -> Result<Vec<FeedPost>, crate::error::AppError> {
    let url = format!(
        "https://api.bsky.app/xrpc/app.bsky.feed.getFeed?feed={}&limit={}",
        urlencoding::encode(feed_uri),
        limit
    );
    let resp = http
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| {
            tracing::warn!("[FEED] Bluesky feed {} error: {}", feed_uri, e);
            crate::error::AppError::Internal(format!("Bluesky API error: {}", e))
        })?;
    if !resp.status().is_success() {
        tracing::warn!("[FEED] Bluesky feed {} status {}", feed_uri, resp.status());
        return Ok(Vec::new());
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| {
        crate::error::AppError::Internal(format!("Bluesky JSON parse error: {}", e))
    })?;
    let mut posts = Vec::new();
    if let Some(feed) = json.get("feed").and_then(|f| f.as_array()) {
        for item in feed {
            if let Some(post) = normalize::normalize_bluesky_api_post(item) {
                posts.push(post);
            }
        }
    }
    Ok(posts)
}

/// Live hashtag search (the user's subscribed hashtags) — works even when the
/// local appview index is empty.
pub async fn fetch_bluesky_search_tags(
    http: &reqwest::Client,
    tag: &str,
    limit: i64,
) -> Result<Vec<FeedPost>, crate::error::AppError> {
    let url = format!(
        "https://api.bsky.app/xrpc/app.bsky.feed.searchPosts?q={}&limit={}",
        urlencoding::encode(&format!("#{}", tag.trim_start_matches('#'))),
        limit
    );
    let resp = http
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| {
            tracing::warn!("[FEED] Bluesky search #{} error: {}", tag, e);
            crate::error::AppError::Internal(format!("Bluesky API error: {}", e))
        })?;
    if !resp.status().is_success() {
        tracing::warn!("[FEED] Bluesky search #{} status {}", tag, resp.status());
        return Ok(Vec::new());
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| {
        crate::error::AppError::Internal(format!("Bluesky JSON parse error: {}", e))
    })?;
    let mut posts = Vec::new();
    if let Some(results) = json.get("posts").and_then(|f| f.as_array()) {
        for item in results {
            if let Some(post) = normalize::normalize_bluesky_api_post(item) {
                posts.push(post);
            }
        }
    }
    Ok(posts)
}

/// Batch-resolve profile identities (did -> handle, display name, avatar)
/// via the Bluesky getProfiles API — accepts DIDs or handles. Budget-bounded
/// and parallel (25 actors per request) to keep feed requests fast on cold
/// caches; every hit is persisted to appview_did_handle so later requests
/// resolve locally.
pub async fn resolve_profiles_remote(
    db: &sqlx::PgPool,
    actors: &[String],
) -> std::collections::HashMap<String, (String, Option<String>, Option<String>)> {
    let mut out = std::collections::HashMap::new();
    if actors.is_empty() {
        return out;
    }
    // Negative cache: handles that getProfiles can't resolve (deleted
    // accounts, typos in imported follows) used to be re-fetched on every
    // request — each miss costs its network timeout and the rail crawled.
    // Skip anything that failed within the TTL; it still resolves later if
    // the account reappears.
    static RECENTLY_UNRESOLVED: std::sync::LazyLock<
        std::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>,
    > = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    let budget: Vec<&String> = actors
        .iter()
        .filter(|a| {
            RECENTLY_UNRESOLVED
                .lock()
                .map(|m| {
                    m.get(a.as_str())
                        .map(|t| t.elapsed() > std::time::Duration::from_secs(1800))
                        .unwrap_or(true)
                })
                .unwrap_or(true)
        })
        .take(75)
        .collect();
    if budget.is_empty() {
        return out;
    }
    let mut resolved_forms: std::collections::HashSet<String> = std::collections::HashSet::new();
    let http = super::feed_http();
    let futs: Vec<_> = budget
        .chunks(25)
        .map(|chunk| {
            let http = &http;
            let actors_query = chunk
                .iter()
                .map(|a| format!("actors={}", a))
                .collect::<Vec<_>>()
                .join("&");
            let url = format!(
                "https://api.bsky.app/xrpc/app.bsky.actor.getProfiles?{}",
                actors_query
            );
            async move {
                http.get(&url)
                    .timeout(std::time::Duration::from_secs(3))
                    .send()
                    .await
            }
        })
        .collect();
    for result in futures::future::join_all(futs).await {
        let resp = match result {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                tracing::warn!("[FEED] getProfiles returned {}", r.status());
                continue;
            }
            Err(e) => {
                tracing::warn!("[FEED] getProfiles failed: {}", e);
                continue;
            }
        };
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            if let Some(profiles) = json.get("profiles").and_then(|p| p.as_array()) {
                for profile in profiles {
                    // `handle.invalid` is the AppView's unresolved-handle
                    // sentinel — skip so the resolveHandle fallback below can
                    // still map the actor correctly.
                    if profile.get("handle").and_then(|h| h.as_str()) == Some("handle.invalid") {
                        continue;
                    }
                    if let (Some(handle), Some(did)) = (
                        profile.get("handle").and_then(|h| h.as_str()),
                        profile.get("did").and_then(|d| d.as_str()),
                    ) {
                        let display_name = profile
                            .get("displayName")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                        let avatar = profile
                            .get("avatar")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                        let _ = sqlx::query(
                            "INSERT INTO appview_did_handle (did, handle, display_name, avatar)
                             VALUES ($1, $2, $3, $4)
                             ON CONFLICT (did) DO UPDATE SET
                               handle = EXCLUDED.handle,
                               display_name = COALESCE(EXCLUDED.display_name, appview_did_handle.display_name),
                               avatar = COALESCE(EXCLUDED.avatar, appview_did_handle.avatar)",
                        )
                        .bind(did)
                        .bind(handle)
                        .bind(&display_name)
                        .bind(&avatar)
                        .execute(db)
                        .await;
                        // The request actors can be handles or DIDs; mark
                        // whichever form matched so only true misses get the
                        // cooldown below.
                        resolved_forms.insert(handle.to_string());
                        resolved_forms.insert(did.to_string());
                        out.insert(
                            did.to_string(),
                            (handle.to_string(), display_name, avatar),
                        );
                    }
                }
            }
        }
    }
    // getProfiles only sees actors the AppView has indexed — did:web users on
    // other instances (or too fresh to index) come back empty there. Fall
    // back to the identity resolver for the true misses so a follow-by-handle
    // of a foreign Bytescape user still resolves to their DID.
    let missing: Vec<&String> = budget
        .iter()
        .copied()
        .filter(|a| !a.starts_with("did:") && !resolved_forms.contains(a.as_str()))
        .collect();
    for batch in missing.chunks(10) {
        let futs = batch.iter().map(|actor| {
            let http = &http;
            let actor = (*actor).clone();
            async move {
                let url = format!(
                    "https://api.bsky.app/xrpc/com.atproto.identity.resolveHandle?handle={}",
                    actor
                );
                let res = http
                    .get(&url)
                    .timeout(std::time::Duration::from_secs(3))
                    .send()
                    .await;
                (actor, res)
            }
        });
        for (actor, res) in futures::future::join_all(futs).await {
            let did = match res {
                Ok(r) if r.status().is_success() => r
                    .json::<serde_json::Value>()
                    .await
                    .ok()
                    .and_then(|v| v.get("did").and_then(|d| d.as_str()).map(String::from)),
                _ => None,
            };
            let Some(did) = did else { continue; };
            let _ = sqlx::query(
                "INSERT INTO appview_did_handle (did, handle) VALUES ($1, $2)
                 ON CONFLICT (did) DO UPDATE SET handle = EXCLUDED.handle",
            )
            .bind(&did)
            .bind(&actor)
            .execute(db)
            .await;
            resolved_forms.insert(actor.clone());
            resolved_forms.insert(did.clone());
            out.insert(did, (actor, None, None));
        }
    }
    // Whatever the budget tried but didn't resolve gets a cooldown so the
    // next request skips it instead of paying its timeout again.
    if let Ok(mut failed) = RECENTLY_UNRESOLVED.lock() {
        for actor in &budget {
            if !resolved_forms.contains(actor.as_str()) {
                failed.insert((*actor).clone(), std::time::Instant::now());
            }
        }
    }
    out
}

/// The `https://` ActivityPub actor URLs a user follows (non-AT). Ingested AP
/// rows carry the actor URL in `author_did`, so these URLs act as match keys
/// for the Following tier — which is otherwise DID-keyed and would drop
/// followed fediverse authors.
pub async fn get_followed_ap_actors(db: &sqlx::PgPool, user_id: uuid::Uuid) -> Vec<String> {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT DISTINCT followed_actor FROM follows \
         WHERE follower_id = $1 \
           AND followed_actor LIKE 'https://%' \
           AND followed_actor NOT LIKE 'https://bsky.app/profile/%'",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
    .unwrap_or_default()
    .into_iter()
    .flatten()
    .collect()
}

/// Resolve followed Bluesky handles to DIDs.
/// Returns (followed_dids, did_to_handle_map).
pub async fn resolve_followed_dids(
    db: &sqlx::PgPool,
    followed_handles: &[String],
) -> (Vec<String>, std::collections::HashMap<String, String>) {
    let mut did_to_handle: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    if followed_handles.is_empty() {
        return (Vec::new(), did_to_handle);
    }

    // First try the local mapping table. DIDs pass through untouched; the
    // table is consulted by handle OR did so display handles resolve for both.
    let local_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT did, handle FROM appview_did_handle WHERE handle = ANY($1::text[]) OR did = ANY($1::text[])",
    )
    .bind(followed_handles)
    .fetch_all(db)
    .await
    .unwrap_or_default();

    let local_dids: Vec<String> = local_rows.iter().map(|(d, _)| d.clone()).collect();
    for (d, h) in &local_rows {
        did_to_handle.insert(d.clone(), h.clone());
    }

    // For handles not resolved locally, batch-resolve via Bluesky getProfiles API
    let unresolved: Vec<String> = followed_handles
        .iter()
        .filter(|h| !h.starts_with("did:") && !local_rows.iter().any(|(_, lh)| lh == *h))
        .cloned()
        .collect();

    let mut all_dids = local_dids;
    for d in followed_handles.iter().filter(|a| a.starts_with("did:")) {
        if !all_dids.contains(d) {
            all_dids.push(d.clone());
        }
    }
    if !unresolved.is_empty() {
        // Resolutions persist to appview_did_handle, so later requests fill
        // from the local table — the remote budget just bounds the worst case.
        for (did, (handle, _, _)) in resolve_profiles_remote(db, &unresolved).await {
            all_dids.push(did.clone());
            did_to_handle.insert(did, handle);
        }
    }

    // Backfill at most once per 5 minutes: this UPDATE touches the 4.5M-row
    // index on every request otherwise, holding connections and writing WAL.
    static LAST_BACKFILL: std::sync::LazyLock<std::sync::Mutex<Option<Instant>>> =
        std::sync::LazyLock::new(|| std::sync::Mutex::new(None));
    let run_backfill = {
        let mut last = LAST_BACKFILL.lock().unwrap();
        match *last {
            Some(at) if at.elapsed() < std::time::Duration::from_secs(300) => false,
            _ => {
                *last = Some(Instant::now());
                true
            }
        }
    };

    // Backfill: UPDATE appview_posts.author_handle for all posts by these DIDs.
    if run_backfill && !did_to_handle.is_empty() {
        let dids: Vec<String> = did_to_handle.keys().cloned().collect();
        let _ = sqlx::query(
            "UPDATE appview_posts p SET author_handle = h.handle
             FROM appview_did_handle h
             WHERE p.author_did = h.did
               AND p.author_handle IS NULL
               AND h.did = ANY($1::text[])",
        )
        .bind(&dids)
        .execute(db)
        .await;
    }

    (all_dids, did_to_handle)
}

/// Get the user's followed actors from the follows table as handles or DIDs.
/// Bluesky-import rows yield bsky handles (or dids). Native rows
/// (`https://{domain}/@{user}`, written by UI follows and import matching)
/// resolve to the target's AT identities locally: their native PDS did plus
/// any linked account dids — stories may be authored under either.
pub async fn get_followed_handles(
    db: &sqlx::PgPool,
    user_id: uuid::Uuid,
    domain: &str,
) -> Vec<String> {
    let mut out: Vec<String> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT DISTINCT
            CASE
                WHEN followed_actor LIKE 'https://bsky.app/profile/%' THEN
                    substring(followed_actor from 'https://bsky\\.app/profile/(.+)')
                WHEN followed_actor LIKE 'http://bsky.app/profile/%' THEN
                    substring(followed_actor from 'http://bsky\\.app/profile/(.+)')
                WHEN followed_actor LIKE 'did:%' THEN followed_actor
                ELSE NULL
            END
         FROM follows
         WHERE follower_id = $1
           AND follow_type = 'bluesky_import'
           AND followed_actor IS NOT NULL",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
    .unwrap_or_default()
    .into_iter()
    .flatten()
    .collect();

    // Native follows never reach the CASE above: resolve them to DIDs here.
    // The exact-form join (`https://{domain}/@{username}`) keeps foreign `/@`
    // URLs (e.g. legacy Mastodon rows) from colliding with a same-named local
    // user.
    let native: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT f.followed_actor, pa.did, la.external_id
         FROM follows f
         LEFT JOIN users u ON f.followed_actor = 'https://' || $2 || '/@' || u.username
         LEFT JOIN pds_accounts pa ON pa.user_id = u.id
         LEFT JOIN linked_accounts la ON la.user_id = u.id
         WHERE f.follower_id = $1
           AND f.follow_type = 'native'
           AND f.followed_actor IS NOT NULL",
    )
    .bind(user_id)
    .bind(domain)
    .fetch_all(db)
    .await
    .unwrap_or_default();
    for (actor, native_did, linked_did) in native {
        // A did-shaped actor is already an identity. The rest contribute both
        // their native did and their linked did (stories authored under either).
        if actor.starts_with("did:") && !out.contains(&actor) {
            out.push(actor);
        }
        for did in [native_did, linked_did].into_iter().flatten() {
            if !out.contains(&did) {
                out.push(did);
            }
        }
    }
    out
}


/// Fetch a feed's posts — our own generated feeds (topic/combo rkeys) resolve
/// in-process against the local index; anything else goes to the network.
/// Boot-time + periodic cache warmer. The first feed request after a restart
/// otherwise pays ~70s of cold scans; TTL expiry drops users back to multi-
/// second scans. Priming every active user's sources on a 240s loop (< the
/// 300s TTL) keeps the common path warm for everyone.
/// Keep the aggregated home feed warm for users with linked accounts, so
/// page loads are cache hits. The rebuild cadence sits under the response
/// TTL; the per-source caches make each pass cheap.
pub fn spawn_aggregated_refresher(state: crate::AppState) {
    tokio::spawn(async move {
        loop {
            let users: Vec<uuid::Uuid> =
                sqlx::query_scalar("SELECT DISTINCT user_id FROM linked_accounts LIMIT 32")
                    .fetch_all(&state.db)
                    .await
                    .unwrap_or_default();
            for user_id in users {
                super::build_aggregated(state.clone(), user_id).await;
                // Pace the pass: back-to-back warm scans stacked up under
                // load and each user's request queued behind the pile.
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            tokio::time::sleep(std::time::Duration::from_secs(55)).await;
        }
    });
}

pub fn spawn_cache_warmer(pool: sqlx::PgPool) {
    tokio::spawn(async move {
        let http = super::feed_http();
        let mut first_pass = true;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            match warm_once(&pool, &http).await {
                Ok(users) => tracing::info!("[warmup] primed caches for {} users", users),
                Err(e) => tracing::warn!("[warmup] failed: {}", e),
            }
            if first_pass {
                first_pass = false;
                // Prime the default platform-counts keys (no tags/author,
                // both NSFW axes) so request-path cold hits never pay the
                // whole-corpus totals aggregate; the snapshots staleness
                // ladder keeps them fresh from then on.
                let _ =
                    fetch_platform_counts(&pool, &[], None, crate::db::users::NsfwPref::Exclude)
                        .await;
                let _ = fetch_platform_counts(&pool, &[], None, crate::db::users::NsfwPref::Blur)
                    .await;
                match backfill_sources_from_network(&pool).await {
                    Ok(n) => tracing::info!("[backfill] imported {} source posts from network", n),
                    Err(e) => tracing::warn!("[backfill] failed: {}", e),
                }
            }
            match prune_old_posts(&pool).await {
                Ok(n) if n > 0 => tracing::info!("[retention] pruned {} stale posts", n),
                Ok(_) => {}
                Err(e) => tracing::warn!("[retention] failed: {}", e),
            }
            tokio::time::sleep(std::time::Duration::from_secs(240)).await;
        }
    });
}

/// Keep the firehose cache to ~7 days. It grew to 9.2M rows / 13G and filled
/// the disk (Postgres died with "No space left on device"); older content is
/// served from the AT Protocol network anyway. Small batches keep WAL short
/// so pruning works even on a nearly-full disk.
///
/// ONLY firehose (AT Protocol, `at://` URIs) rows are pruned. Imported /
/// backfilled fediverse history (ActivityPub, `https://…/statuses/…` URIs) is
/// the user's personal archive — its `created_at` is the original post date,
/// often years old, so an age-based prune would delete it the moment it lands.
/// That content is not served from the network on demand (it IS the import), so
/// it must be retained regardless of age.
async fn prune_old_posts(pool: &sqlx::PgPool) -> Result<u64, sqlx::Error> {
    let mut total = 0u64;
    for _ in 0..50 {
        let r = sqlx::query(
            "WITH gone AS (SELECT uri FROM appview_posts WHERE uri LIKE 'at://%' AND created_at < now() - interval '5 days' LIMIT 20000)
             DELETE FROM appview_posts p USING gone WHERE p.uri = gone.uri",
        )
        .execute(pool)
        .await?;
        total += r.rows_affected();
        if r.rows_affected() < 20000 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    Ok(total)
}

/// One-shot gap fill: the disk-full era froze the firehose cursor, leaving a
/// multi-day hole the relay won't replay. Pull each subscribed source's recent
/// posts from the public AppView straight into the local index.
async fn backfill_sources_from_network(pool: &sqlx::PgPool) -> Result<usize, sqlx::Error> {
    use crate::oauth::bluesky::BlueskyPublicApi;
    // searchPosts is auth-gated on the public AppView — use the user's OAuth
    // token (plain Bearer to api.bsky.app, the established pattern).
    let users: Vec<(uuid::Uuid,)> =
        sqlx::query_as("SELECT DISTINCT user_id FROM subscriptions LIMIT 100")
            .fetch_all(pool)
            .await?;
    let mut inserted = 0usize;
    for (uid,) in users {
        // The OAuth token is DPoP-bound: searchPosts must be called with a
        // DPoP proof against the audience its scope names (bsky_appview).
        let acct: Option<crate::db::accounts::LinkedAccountRow> =
            match crate::db::accounts::LinkedAccountRow::find_by_user(pool, uid).await {
                Ok(accts) => accts.into_iter().find(|a| a.provider == "bluesky"),
                Err(_) => None,
            };
        let Ok(rows) = crate::db::subscriptions::SubscriptionRow::find_by_user(pool, uid).await
        else {
            continue;
        };
        for r in rows.iter().filter(|r| r.enabled && r.category == "hashtags") {
            let tag = r
                .source_id
                .trim_start_matches("mastodon:")
                .trim_start_matches('#')
                .to_lowercase();
            if tag.is_empty() {
                continue;
            }
            if let Some(v) = search_posts_network(pool, acct.as_ref(), &tag, 50).await {
                for item in v["posts"].as_array().into_iter().flatten() {
                    if upsert_network_item(pool, item).await.is_ok() {
                        inserted += 1;
                    }
                }
            }
        }
        for r in rows
            .iter()
            .filter(|r| r.enabled && r.category == "feeds" && r.source_id.starts_with("at://"))
        {
            if let Ok(v) = BlueskyPublicApi::get_feed(&r.source_id, 50).await {
                for entry in v["feed"].as_array().into_iter().flatten() {
                    if let Some(post) = entry.get("post") {
                        if upsert_network_item(pool, post).await.is_ok() {
                            inserted += 1;
                        }
                    }
                }
            }
        }
    }
    Ok(inserted)
}

/// searchPosts via the DPoP client against the AppView audience the token's
/// scope names — plain-Bearer variants are rejected ("DPoP not supported" /
/// scope 403s).
async fn search_posts_network(
    pool: &sqlx::PgPool,
    acct: Option<&crate::db::accounts::LinkedAccountRow>,
    tag: &str,
    limit: u32,
) -> Option<serde_json::Value> {
    use crate::oauth::bluesky::{fresh_token, BlueskyApiClient};
    let acct = acct?;
    let token = fresh_token(pool, acct).await.ok()?;
    let seed = hex::decode(acct.dpop_private_key.as_deref().unwrap_or("")).unwrap_or_default();
    let client = if seed.len() == 32 {
        BlueskyApiClient::new(&token, &seed, "https://api.bsky.app").ok()?
    } else {
        BlueskyApiClient::new_bearer(&token, "https://api.bsky.app")
    };
    let q = format!("#{}", tag);
    let limit_s = limit.to_string();
    match client
        .xrpc_call("app.bsky.feed.searchPosts", vec![("q", q.as_str()), ("limit", limit_s.as_str())])
        .await
    {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!("[backfill] searchPosts '{}' failed: {}", tag, e);
            None
        }
    }
}

/// Upsert one AppView PostView into the local index.
pub(crate) async fn upsert_network_item(
    pool: &sqlx::PgPool,
    item: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    let Some(uri) = item["uri"].as_str() else {
        return Ok(());
    };
    let cid = item["cid"].as_str().unwrap_or("");
    let did = item["author"]["did"].as_str().unwrap_or("");
    let handle = item["author"]["handle"].as_str();
    let text = item["record"]["text"].as_str().unwrap_or("");
    let parent = item["record"]["reply"]["parent"]["uri"].as_str();
    let root = item["record"]["reply"]["root"]["uri"].as_str();
    let has_media = item.get("embed").map(|e| !e.is_null()).unwrap_or(false);
    let embeds_json = item.get("embed").and_then(view_embed_to_db_json);
    let author_avatar = item["author"]["avatar"].as_str();
    let author_display_name = item["author"]["displayName"].as_str();
    let ts = item["record"]["createdAt"]
        .as_str()
        .and_then(|c| chrono::DateTime::parse_from_rfc3339(c).ok())
        .map(|d| d.with_timezone(&chrono::Utc));
    sqlx::query(
        r#"INSERT INTO appview_posts (uri, cid, author_did, author_handle, text, reply_parent, reply_root, created_at, has_media, embeds, author_avatar, author_display_name)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
           ON CONFLICT (uri) DO UPDATE SET
             text = EXCLUDED.text,
             author_handle = COALESCE(EXCLUDED.author_handle, appview_posts.author_handle),
             embeds = COALESCE(EXCLUDED.embeds, appview_posts.embeds),
             has_media = appview_posts.has_media OR EXCLUDED.has_media,
             author_avatar = COALESCE(EXCLUDED.author_avatar, appview_posts.author_avatar),
             author_display_name = COALESCE(EXCLUDED.author_display_name, appview_posts.author_display_name),
             indexed_at = now()"#,
    )
    .bind(uri)
    .bind(cid)
    .bind(did)
    .bind(handle)
    .bind(text)
    .bind(parent)
    .bind(root)
    .bind(ts)
    .bind(has_media)
    .bind(embeds_json.as_ref())
    .bind(author_avatar)
    .bind(author_display_name)
    .execute(pool)
    .await
    .map(|_| ())
}

/// Convert an AppView view-shaped embed (app.bsky.embed.*#view) into the
/// compact array shape stored in appview_posts.embeds (parsed by
/// feed::normalize::parse_bluesky_embeds). Image/video blob CIDs are
/// recovered from the CDN URLs the view embed carries.
pub(crate) fn view_embed_to_db_json(embed: &serde_json::Value) -> Option<serde_json::Value> {
    let etype = embed.get("$type").and_then(|v| v.as_str()).unwrap_or("");
    let mut out: Vec<serde_json::Value> = Vec::new();

    if etype.contains("images") || etype.contains("gallery") {
        for img in embed.get("images").or_else(|| embed.get("items")).and_then(|v| v.as_array()).into_iter().flatten() {
            // fullsize: https://cdn.bsky.app/img/feed_fullsize/plain/{did}/{cid}@jpeg
            let cid = img
                .get("fullsize")
                .and_then(|v| v.as_str())
                .and_then(|u| u.rsplit('/').next())
                .and_then(|s| s.split('@').next())
                .filter(|s| !s.is_empty());
            if let Some(cid) = cid {
                out.push(serde_json::json!({
                    "type": "image",
                    "cid": cid,
                    "mime_type": "image/jpeg",
                    "alt": img.get("alt").and_then(|v| v.as_str()).unwrap_or(""),
                }));
            }
        }
    } else if etype.contains("video") {
        // playlist: https://video.bsky.app/watch/{did}/{cid}/playlist.m3u8
        let cid = embed
            .get("playlist")
            .and_then(|v| v.as_str())
            .and_then(|u| u.strip_suffix("/playlist.m3u8"))
            .and_then(|u| u.rsplit('/').next())
            .filter(|s| !s.is_empty());
        if let Some(cid) = cid {
            let thumb_cid = embed
                .get("thumbnail")
                .and_then(|v| v.as_str())
                .and_then(|u| u.rsplit('/').next())
                .and_then(|s| s.split('@').next())
                .filter(|s| !s.is_empty());
            let mut entry = serde_json::json!({ "type": "video", "cid": cid });
            if let Some(t) = thumb_cid {
                entry["thumbnail_cid"] = serde_json::json!(t);
            }
            out.push(entry);
        }
    } else if etype.contains("external") {
        if let Some(ext) = embed.get("external") {
            let uri = ext.get("uri").and_then(|v| v.as_str()).unwrap_or("");
            if !uri.is_empty() {
                out.push(serde_json::json!({
                    "type": "link",
                    "url": uri,
                    "title": ext.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                    "description": ext.get("description").and_then(|v| v.as_str()).unwrap_or(""),
                    "thumbnail": ext.get("thumb").and_then(|v| v.as_str()).unwrap_or(""),
                }));
            }
        }
    } else if etype.contains("recordWithMedia") {
        if let Some(rec) = embed.get("record").and_then(|r| r.get("record")) {
            if let Some(uri) = rec.get("uri").and_then(|v| v.as_str()) {
                out.push(serde_json::json!({ "type": "quote", "uri": uri }));
            }
        }
        if let Some(med) = embed.get("media") {
            if let Some(mut m) = view_embed_to_db_json(med) {
                if let Some(arr) = m.as_array_mut() {
                    out.append(arr);
                }
            }
        }
    } else if etype.contains("record") {
        if let Some(uri) = embed
            .get("record")
            .and_then(|r| r.get("uri"))
            .and_then(|v| v.as_str())
        {
            out.push(serde_json::json!({ "type": "quote", "uri": uri }));
        }
    }

    if out.is_empty() {
        None
    } else {
        Some(serde_json::Value::Array(out))
    }
}

async fn warm_once(pool: &sqlx::PgPool, http: &reqwest::Client) -> Result<usize, sqlx::Error> {
    let users: Vec<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT DISTINCT user_id FROM subscriptions LIMIT 100",
    )
    .fetch_all(pool)
    .await?;
    let mut warmed = 0;
    for (uid,) in users {
        let Ok(rows) = crate::db::subscriptions::SubscriptionRow::find_by_user(pool, uid).await
        else {
            continue;
        };
        let mut tags: Vec<String> = Vec::new();
        for r in rows.iter().filter(|r| r.enabled && r.category == "hashtags") {
            let tag = r
                .source_id
                .trim_start_matches("mastodon:")
                .trim_start_matches('#')
                .to_lowercase();
            if !tag.is_empty() && !tags.contains(&tag) {
                tags.push(tag);
            }
        }
        if !tags.is_empty() {
            let _ = fetch_appview_hashtags(pool, &tags, 60, super::sort::SortMode::New).await;
            let _ = super::appview_search_multi(pool, &tags, 120, None).await;
        }
        for f in rows
            .iter()
            .filter(|r| r.enabled && r.category == "feeds" && r.source_id.starts_with("at://"))
        {
            let _ = fetch_feed_posts(pool, http, &f.source_id, 30).await;
        }
        warmed += 1;
    }
    Ok(warmed)
}

/// Short-lived result cache for the heavy content fetchers. Feed content is
/// recent-first and re-requested on every tab switch — 60s of staleness buys
/// instant repeat loads instead of repeated multi-second scans.
use std::sync::LazyLock;
use std::time::Instant;
pub(crate) type FeedCache = std::sync::Mutex<std::collections::HashMap<String, (Instant, Vec<FeedPost>)>>;
pub(crate) static FEED_CACHE: LazyLock<FeedCache> =
    LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));
pub(crate) static TAG_CACHE: LazyLock<FeedCache> =
    LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));
pub(crate) static SEARCH_CACHE: LazyLock<FeedCache> =
    LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));
pub(crate) static FOLLOWS_CACHE: LazyLock<FeedCache> =
    LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));
pub(crate) const CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

pub(crate) fn cache_get(cache: &FeedCache, key: &str) -> Option<Vec<FeedPost>> {
    let map = cache.lock().unwrap();
    map.get(key)
        .filter(|(at, _)| at.elapsed() < CACHE_TTL)
        .map(|(_, v)| v.clone())
}

pub(crate) fn cache_put(cache: &FeedCache, key: String, value: Vec<FeedPost>) {
    let mut map = cache.lock().unwrap();
    map.retain(|_, (at, _)| at.elapsed() < CACHE_TTL);
    if map.len() > 512 {
        // Drop the oldest half — never the whole cache (browsing many topics
        // must not evict everything the user just loaded).
        let mut entries: Vec<(String, Instant)> = map
            .iter()
            .map(|(k, (at, _))| (k.clone(), *at))
            .collect();
        entries.sort_by_key(|(_, at)| *at);
        for (k, _) in entries.into_iter().take(map.len() / 2) {
            map.remove(&k);
        }
    }
    map.insert(key, (Instant::now(), value));
}

pub async fn fetch_feed_posts(
    db: &sqlx::PgPool,
    http: &reqwest::Client,
    feed_uri: &str,
    limit: i64,
) -> Vec<FeedPost> {
    let cache_key = feed_uri.to_string();
    if let Some(hit) = cache_get(&FEED_CACHE, &cache_key) {
        return hit.into_iter().take(limit.max(0) as usize).collect();
    }
    // One batch covers the seed (10) and full (15) phases — limit-independent
    // key, slice to need.
    let result = fetch_feed_posts_uncached(db, http, feed_uri, 30).await;
    if !result.is_empty() {
        cache_put(&FEED_CACHE, cache_key, result.clone());
    }
    result.into_iter().take(limit.max(0) as usize).collect()
}

async fn fetch_feed_posts_uncached(
    db: &sqlx::PgPool,
    http: &reqwest::Client,
    feed_uri: &str,
    limit: i64,
) -> Vec<FeedPost> {
    if let Some(rkey) = feed_uri.rsplit('/').next() {
        if crate::api::feedgen::is_our_feed(db, rkey).await {
            if let Some(topics) = crate::api::feedgen::topics_for_rkey(db, rkey).await {
                let terms = crate::api::feedgen::topic_terms(&topics);
                if let Ok(rows) = crate::api::feedgen::query_feed_rows(db, &terms, limit, 0).await {
                    return crate::api::feedgen::rows_to_feed_posts(rows);
                }
            }
        }
    }
    fetch_bluesky_feed_uri(http, feed_uri, limit).await.unwrap_or_default()
}
