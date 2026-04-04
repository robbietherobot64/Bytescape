//! Feed aggregation — merges multiple content sources into one unified timeline.
//!
//! This module implements the "All" tab in Bytescape's feed view. It provides
//! the Tauri command `get_timeline_unified` which merges posts from three
//! distinct sources into a single, chronologically-sorted, deduplicated feed.
//!
//! ## How It Works
//!
//! 1. **Parallel fetch** — Three async futures run concurrently via `tokio::join!`:
//!    the live Bluesky following-timeline, any subscribed custom/algorithm feeds,
//!    and the local vault (redb) for imported/archived items.
//! 2. **Normalization** — Each source returns its own type (`BlueskyImportedPost`,
//!    `BytescapeItem`); `From` impls convert everything into `FeedEntry`, the
//!    universal frontend container.
//! 3. **Vault dedup** — Before merging, live posts that already exist in the vault
//!    are removed so the vault's richer metadata (audiences, share_history) wins.
//! 4. **Post-merge filter** — An optional `source_filter` prunes the combined list
//!    to a single platform (e.g., only Facebook imports).
//! 5. **Sort + global dedup** — The merged feed is sorted newest-first, then
//!    deduplicated using composite keys that distinguish reposts from originals.
//!
//! ## Tech Debt
//!
//! - Vault dedup logic is duplicated in the `has_custom` and `!has_custom` branches;
//!   should be extracted into a helper function.
//! - Source detection heuristic in `From<BlueskyImportedPost>` (checking URI for
//!   "/statuses/" or "acct:" prefix) is fragile — should use an explicit source field.
//! - Pagination is inconsistent: Bluesky uses cursor-based, vault uses offset-based,
//!   custom feeds have no pagination at all in this path.
//!
//! ## Related Files
//!
//! - `commands/import.rs` — `get_timeline()` and `get_unified_feed()` network calls
//! - `models/mod.rs` — `FeedEntry`, `BytescapeItem`, `Audience`, classification logic
//! - `connectors/bluesky.rs` — `BlueskyImportedPost` intermediate type
//! - `db/vault.rs` — `list_my_items_paged()` vault query used by the vault future

use tauri::State;
use serde::Serialize;
use crate::AppState;
use crate::models::{FeedEntry, FeedPage, BytescapeItem, Audience, audiences_from_legacy, classify_feed_entry};
use crate::connectors::bluesky::BlueskyImportedPost;

// ═══════════════════════════════════════════════════════════════════════════════
// Linked Feed Sources — Platform connection status for source toggle pills
// ═══════════════════════════════════════════════════════════════════════════════

/// A connected platform that can contribute to the Following tab feed.
#[derive(Debug, Serialize, Clone)]
pub struct LinkedFeedSource {
    /// Platform identifier ("bluesky", "mastodon")
    pub id: String,
    /// Human-readable label ("Bluesky", "Mastodon")
    pub label: String,
    /// User handle on this platform ("@user.bsky.social" or "@user@mastodon.social")
    pub handle: String,
    /// Whether the platform has a valid, active session
    pub connected: bool,
}

/// Returns all platform connections available for feed source filtering.
///
/// The frontend uses this to render source toggle pills on the Following tab.
/// Each entry indicates whether the platform is connected and the user's handle.
///
/// # IPC
/// Frontend: `invoke('get_linked_feed_sources')`
#[tauri::command]
pub async fn get_linked_feed_sources(
    state: State<'_, AppState>,
) -> Result<Vec<LinkedFeedSource>, String> {
    let mut sources = Vec::new();

    // Bluesky
    match state.keychain.load_session() {
        Ok(Some((did, _jwt))) => {
            // Try to get the handle from the DID
            let handle = if did.starts_with("did:plc:") {
                // Use DID as fallback; the frontend can resolve the handle
                did.clone()
            } else {
                did.clone()
            };
            sources.push(LinkedFeedSource {
                id: "bluesky".into(),
                label: "Bluesky".into(),
                handle: format!("@{}", handle),
                connected: true,
            });
        }
        _ => {
            sources.push(LinkedFeedSource {
                id: "bluesky".into(),
                label: "Bluesky".into(),
                handle: String::new(),
                connected: false,
            });
        }
    }

    // Mastodon
    match crate::connectors::mastodon::get_session_from_keychain(&state.keychain) {
        Ok(session) => {
            sources.push(LinkedFeedSource {
                id: "mastodon".into(),
                label: "Mastodon".into(),
                handle: format!("@{}", session.acct),
                connected: true,
            });
        }
        Err(_) => {
            sources.push(LinkedFeedSource {
                id: "mastodon".into(),
                label: "Mastodon".into(),
                handle: String::new(),
                connected: false,
            });
        }
    }

    Ok(sources)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Type Conversions — Source-specific types → universal FeedEntry
// ═══════════════════════════════════════════════════════════════════════════════

// === BlueskyImportedPost → FeedEntry ===
//
// Converts the Bluesky-specific intermediate type into the universal FeedEntry
// container consumed by the frontend.
//
// For reposts:
//   - `author_*` fields refer to the ORIGINAL post's author (whose content is shown).
//   - `reposted_by` / `reposted_by_handle` identify who reposted it.
//   - `timestamp` is already set to the repost time by `parse_feed_item`.
//
// For quote posts:
//   - `quoted_post` contains the recursively-converted inner post.

impl From<BlueskyImportedPost> for FeedEntry {
    fn from(post: BlueskyImportedPost) -> Self {
        // Build parallel media arrays.
        let media_urls = if let Some(ref v) = post.video_url {
            vec![v.clone()]
        } else {
            post.image_urls.clone()
        };

        let media_kinds = if post.video_url.is_some() {
            vec!["video".to_string()]
        } else {
            post.image_urls.iter().map(|_| "image".to_string()).collect()
        };

        let media_thumbnails = if let Some(ref t) = post.video_thumbnail {
            vec![t.clone()]
        } else {
            vec![]
        };

        // Detect source from URI — Mastodon adapter encodes "https://{instance}/statuses/{id}"
        let source = if post.uri.contains("/statuses/") && post.uri.starts_with("https://") {
            "mastodon".to_string()
        } else if post.author_did.starts_with("acct:") {
            "mastodon".to_string()
        } else {
            "bluesky".to_string()
        };
        let audience = if source == "mastodon" {
            vec![Audience::Local, Audience::Activitypub]
        } else {
            vec![Audience::Local, Audience::Bluesky]
        };

        let mut entry = FeedEntry {
            id: post.uri.clone(),
            cid: Some(post.cid.clone()),
            viewer_like: post.viewer_like,
            viewer_repost: post.viewer_repost,
            timestamp: post.created_at,
            content: post.text,
            author_did: Some(post.author_did),
            author_handle: Some(post.author_handle),
            author_avatar: post.author_avatar,
            source: source.clone(),
            media_urls,
            media_thumbnails,
            media_kinds,
            media_mimes: vec![],
            link_url: post.external_url,
            link_title: post.external_title,
            link_description: post.external_description,
            link_thumbnail: post.external_thumb,
            like_count: post.like_count,
            reply_count: post.reply_count,
            repost_count: post.repost_count,
            reposted_by: post.reposted_by,
            reposted_by_handle: post.reposted_by_handle,
            reply_parent_handle: post.reply_parent_handle,
            reply_parent_uri: post.reply_parent_uri,
            quoted_post: post.quoted_post.map(|q| Box::new(FeedEntry::from(*q))),
            item_type: None,
            syndication_status: None,
            audiences: audience,
            share_history: vec![
                crate::models::SyndicationEvent {
                    platform: source,
                    uri: post.uri.clone(),
                    timestamp: post.created_at as u64,
                    active: true,
                }
            ],
            location: None,
            tagged_people: vec![],
            title: None,
            original_url: Some(post.uri),
        };
        // Classify based on content (links, media, text length) instead of hardcoding Status
        entry.item_type = Some(classify_feed_entry(&entry));
        entry
    }
}

// === BytescapeItem → FeedEntry ===
//
// Converts a local vault item into the universal FeedEntry container.
// Media blobs are resolved to `blob:{hash}` URIs for the frontend asset protocol.
// Media kinds are inferred from MIME types, with a fallback for HLS videos that
// lack a stored MIME (see the Video-type guard below).

impl From<BytescapeItem> for FeedEntry {
    fn from(item: BytescapeItem) -> Self {
        let media_urls: Vec<String> = if !item.media_blobs.is_empty() {
            item.media_blobs.into_iter().map(|b| {
                if b.starts_with("blob:") || b.starts_with("http") || b.contains('/') { b } else { format!("blob:{}", b) }
            }).collect()
        } else {
            item.media_paths.clone()
        };

        let media_thumbnails: Vec<String> = if !item.thumbnail_blobs.is_empty() {
            item.thumbnail_blobs.into_iter().map(|b| {
                if b.starts_with("blob:") || b.starts_with("http") || b.contains('/') { b } else { format!("blob:{}", b) }
            }).collect()
        } else if let Some(ref img) = item.preview_img {
            // Vault items rarely have thumbnail_blobs. Fall back to preview_img
            // (the item's preview image from the original platform, if any).
            if !img.is_empty() { vec![img.clone()] } else { vec![] }
        } else {
            vec![]
        };

        let mut media_kinds = Vec::new();
        for mime in &item.media_mimes {
            if mime.starts_with("video/") { media_kinds.push("video".to_string()); }
            else if mime.starts_with("audio/") { media_kinds.push("audio".to_string()); }
            else if mime == "application/pdf" { media_kinds.push("pdf".to_string()); }
            else if mime.starts_with("image/") { media_kinds.push("image".to_string()); }
            else { media_kinds.push("unknown".to_string()); }
        }
        while media_kinds.len() < media_urls.len() {
            media_kinds.push("unknown".to_string());
        }
        // Override "unknown" entries for Video items — vault videos often have
        // empty media_mimes (MIME wasn't stored during import), causing the
        // while-loop above to fill media_kinds with "unknown". The frontend
        // treats "unknown" as "image", making videos invisible. Fix: if the
        // item_type is Video, promote all "unknown" slots to "video".
        if item.item_type == crate::models::BytescapeType::Video {
            for kind in &mut media_kinds {
                if kind == "unknown" {
                    *kind = "video".to_string();
                }
            }
            // If media_kinds is still empty (no media_urls either), inject one
            if media_kinds.is_empty() {
                media_kinds.push("video".to_string());
            }
        }

        FeedEntry {
            id: item.id.clone(),
            cid: None,
            viewer_like: None,
            viewer_repost: None,
            timestamp: item.original_timestamp,
            content: item.content,
            author_did: item.author_did,
            author_handle: item.author_handle,
            author_avatar: item.author_avatar,
            source: item.source,
            media_urls,
            media_thumbnails,
            media_kinds,
            media_mimes: item.media_mimes,
            link_url: item.link_url,
            link_title: item.title.clone(),
            link_description: None,
            link_thumbnail: item.preview_img,
            like_count: item.like_count,
            reply_count: item.reply_count,
            repost_count: item.repost_count,
            reposted_by: item.reposted_by_did,
            reposted_by_handle: item.reposted_by_handle,
            reply_parent_handle: item.reply_parent_handle.clone(),
            reply_parent_uri: item.reply_parent_uri.clone(),
            quoted_post: None,
            item_type: Some(item.item_type),
            syndication_status: Some(item.syndication_status.clone()),
            audiences: if item.audiences.is_empty() {
                audiences_from_legacy(&item.syndication_status, &item.syndicated_uri)
            } else {
                item.audiences.clone()
            },
            share_history: if item.share_history.is_empty() {
                crate::models::share_history_from_legacy(&item.syndicated_uris, &item.syndicated_at, &item.syndicated_uri, &item.syndication_status)
            } else {
                item.share_history.clone()
            },
            location: item.location,
            tagged_people: item.tagged_people,
            title: item.title,
            original_url: item.url,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Response Types
// ═══════════════════════════════════════════════════════════════════════════════

// Response type: FeedPage (defined in crate::models)
// Replaces the old UnifiedFeedResponse with a standardized shape
// shared across all feed commands: { items, cursor, has_more }.

// ═══════════════════════════════════════════════════════════════════════════════
// Main Aggregation Command
// ═══════════════════════════════════════════════════════════════════════════════

// Merges three sources into one chronological feed:
//   1. Live Bluesky timeline (following feed)
//   2. Custom/subscribed feeds
//   3. Local vault items
//
// Deduplication uses composite keys so that:
//   - A repost of post X by user Y → key "repost:Y:X" (distinct event)
//   - The original post X           → key "X"
//   - Two copies of the same post   → collapsed (same key)

/// Fetches and merges posts from all configured sources into one unified feed.
///
/// This is the primary Tauri IPC command backing the "All" tab in the feed view.
/// It runs three async fetches in parallel (live Bluesky timeline, custom/subscribed
/// feeds, and the local vault), then deduplicates, filters, and sorts the results.
///
/// # Parameters
///
/// - `tags` — Tag filters for custom feed selection (empty = no custom feeds)
/// - `feed_uris` — AT Protocol feed generator URIs to include
/// - `limit` — Max items per network source (default varies by source)
/// - `cursor` — Bluesky pagination cursor from a previous response
/// - `vault_offset` — Offset-based pagination for local vault items
/// - `source_filter` — Optional platform filter (e.g., "bluesky", "facebook")
///
/// # Returns
///
/// A `UnifiedFeedResponse` with the merged items and an optional next cursor.
///
/// # Side Effects
///
/// - Reads from the feed cache (30s TTL) to avoid redundant network calls
/// - Populates the feed cache on cache miss
/// - Reads from the local redb vault via `DbManager`
///
/// # IPC
///
/// Exposed as Tauri command `get_timeline_unified` — called from `useUnifiedFeed` hook.
#[tauri::command]
pub async fn get_timeline_unified(
    state: State<'_, AppState>,
    tags: Vec<String>,
    feed_uris: Vec<String>,
    limit: Option<u8>,
    cursor: Option<String>,
    vault_offset: Option<usize>,
    source_filter: Option<String>,
) -> Result<FeedPage, String> {
    #[cfg(debug_assertions)]
    let t_start = std::time::Instant::now();

    let mut feed: Vec<FeedEntry> = Vec::new();
    let mut next_cursor: Option<String> = None;

    // Normalize the source filter (lowercase, trim, None if empty)
    let source_filter = source_filter
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());

    #[cfg(debug_assertions)]
    if let Some(ref sf) = source_filter {
        println!("get_timeline_unified: source_filter={:?}", sf);
    }

    // Feed cache TTL — 30 seconds balances freshness vs speed.
    // The frontend has its own 2-minute stale-while-revalidate layer on top.
    let cache_ttl = std::time::Duration::from_secs(30);

    // Load session DID upfront (keychain is not Clone)
    let owner_did = state.keychain.load_session()
        .ok()
        .flatten()
        .map(|(did, _)| did);

    // ── Prepare vault future (will run in parallel with network) ──
    // Pass source_filter to the vault query so the DB returns only matching items.
    let db_manager_clone = state.db_manager.clone();
    let vault_offset_val = vault_offset.unwrap_or(0);
    let owner_did_clone = owner_did.clone();
    let vault_source_filter = source_filter.clone();
    let vault_fut = async move {
        if let Some(did) = owner_did_clone {
            let db_guard = db_manager_clone.read().await;
            if let Some(db) = db_guard.as_ref() {
                if let Ok((items, _)) = db.list_my_items_paged(
                    &did,
                    vault_source_filter.as_deref(),
                    200,
                    vault_offset_val,
                ).await {
                    return items.into_iter().map(FeedEntry::from).collect::<Vec<_>>();
                }
            }
        }
        Vec::new()
    };

    // ── 1 + 2. Live timeline + custom feeds — with feed cache ──
    let has_custom = !tags.is_empty() || !feed_uris.is_empty();

    // When filtering to a source that doesn't come from the live timeline
    // (e.g., archived imports like facebook, instagram), skip the network
    // round-trip — those posts will just be discarded anyway.
    // Mastodon posts flow through get_timeline, so don't skip for "mastodon".
    let skip_network = source_filter.as_deref()
        .map(|sf| sf != "bluesky" && sf != "mastodon" && sf != "all")
        .unwrap_or(false);

    // Build a cache key incorporating cursor/offset AND source filter to prevent
    // cache poisoning (filtered result served for unfiltered request or vice-versa).
    let filter_tag = source_filter.as_deref().unwrap_or("all");
    let cache_key_timeline = format!("timeline:{}:{}", filter_tag, cursor.as_deref().unwrap_or("first"));

    // Try to serve timeline from cache (avoids network round-trip)
    let cached_timeline = if cursor.is_none() && !skip_network {
        state.feed_cache.get(&cache_key_timeline, cache_ttl).await
    } else {
        None // don't cache paginated requests or when skipping network
    };

    let timeline_fut = async {
        // When filtering to an archive-only source, return empty immediately
        if skip_network {
            #[cfg(debug_assertions)]
            println!("get_timeline_unified: skipping network (source_filter is archive-only)");
            return Ok((Vec::new(), None));
        }

        if let Some(cached) = cached_timeline {
            // Cache hit — deserialize cached response
            #[cfg(debug_assertions)]
            println!("get_timeline_unified: timeline CACHE HIT");
            if let Ok(posts) = serde_json::from_value::<Vec<BlueskyImportedPost>>(cached.get("posts").cloned().unwrap_or_default()) {
                let nc = cached.get("cursor").and_then(|c| c.as_str()).map(|s| s.to_string());
                return Ok((posts, nc));
            }
        }
        // Cache miss — hit the network
        let result = crate::commands::import::get_timeline(state.clone(), limit, cursor.clone()).await;
        // Cache the result for next time (fire-and-forget, errors are fine)
        if cursor.is_none() {
            if let Ok((ref posts, ref nc)) = result {
                let cache_val = serde_json::json!({
                    "posts": posts,
                    "cursor": nc,
                });
                state.feed_cache.set(&cache_key_timeline, cache_val).await;
            }
        }
        result
    };

    if has_custom && !skip_network {
        let custom_fut = crate::commands::import::get_unified_feed(
            state.clone(), tags, feed_uris, limit.unwrap_or(50) as u16, None
        );

        // Run ALL three concurrently: timeline + custom feeds + vault
        let (timeline_result, custom_result, vault_entries) = tokio::join!(timeline_fut, custom_fut, vault_fut);

        #[cfg(debug_assertions)]
        println!("get_timeline_unified: network+vault parallel fetch took {:?}", t_start.elapsed());

        match timeline_result {
            Ok((posts, nc)) => {
                #[cfg(debug_assertions)]
                println!("get_timeline_unified: live timeline returned {} posts", posts.len());
                next_cursor = nc;
                feed.extend(posts.into_iter().map(FeedEntry::from));
            }
            Err(e) => {
                eprintln!("get_timeline_unified: live timeline fetch FAILED: {}", e);
            }
        }

        match custom_result {
            Ok((posts, _)) => {
                #[cfg(debug_assertions)]
                println!("get_timeline_unified: custom feeds returned {} posts", posts.len());
                feed.extend(posts.into_iter().map(FeedEntry::from));
            }
            Err(e) => {
                eprintln!("get_timeline_unified: custom feeds fetch FAILED: {}", e);
            }
        }

        // ── 3. Vault items (already fetched in parallel) ──
        #[cfg(debug_assertions)]
        println!("get_timeline_unified: vault returned {} items", vault_entries.len());

        // Vault dedup
        let vault_uris: std::collections::HashSet<String> = vault_entries.iter()
            .flat_map(|v| {
                let mut uris = Vec::new();
                for event in &v.share_history {
                    if event.active { uris.push(event.uri.clone()); }
                }
                if let Some(ref url) = v.original_url {
                    if !url.is_empty() { uris.push(url.clone()); }
                }
                uris
            })
            .collect();

        feed.retain(|entry| {
            if entry.reposted_by.is_some() { return true; }
            let uri = entry.original_url.as_ref().unwrap_or(&entry.id);
            !vault_uris.contains(uri) && !vault_uris.contains(&entry.id)
        });

        feed.extend(vault_entries);
    } else {
        // Run timeline + vault in parallel (no custom feeds)
        let (timeline_result, vault_entries) = tokio::join!(timeline_fut, vault_fut);

        #[cfg(debug_assertions)]
        println!("get_timeline_unified: network+vault parallel fetch took {:?}", t_start.elapsed());

        match timeline_result {
            Ok((posts, nc)) => {
                #[cfg(debug_assertions)]
                println!("get_timeline_unified: live timeline returned {} posts", posts.len());
                next_cursor = nc;
                feed.extend(posts.into_iter().map(FeedEntry::from));
            }
            Err(e) => {
                eprintln!("get_timeline_unified: live timeline fetch FAILED: {}", e);
            }
        }

        // ── 3. Vault items (already fetched in parallel) ──
        #[cfg(debug_assertions)]
        println!("get_timeline_unified: vault returned {} items", vault_entries.len());

        // Vault dedup
        let vault_uris: std::collections::HashSet<String> = vault_entries.iter()
            .flat_map(|v| {
                let mut uris = Vec::new();
                for event in &v.share_history {
                    if event.active { uris.push(event.uri.clone()); }
                }
                if let Some(ref url) = v.original_url {
                    if !url.is_empty() { uris.push(url.clone()); }
                }
                uris
            })
            .collect();

        feed.retain(|entry| {
            if entry.reposted_by.is_some() { return true; }
            let uri = entry.original_url.as_ref().unwrap_or(&entry.id);
            !vault_uris.contains(uri) && !vault_uris.contains(&entry.id)
        });

        feed.extend(vault_entries);
    }

    // ── 4. Post-merge source filter ──
    // Even though we skip Bluesky API calls for non-bluesky filters and pass
    // the filter to the vault query, apply a final safety filter to catch any
    // items that slipped through (e.g. custom feeds returning mixed sources).
    if let Some(ref sf) = source_filter {
        let before = feed.len();
        feed.retain(|entry| entry.source.eq_ignore_ascii_case(sf));
        #[cfg(debug_assertions)]
        println!("get_timeline_unified: post-merge source filter '{}' removed {} items", sf, before - feed.len());
    }

    // ── 5. Sort by timestamp descending ──
    feed.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    // ── 6. Deduplicate ──
    let mut seen = std::collections::HashSet::new();
    feed.retain(|entry| {
        let uri = if !entry.id.is_empty() { &entry.id } else {
            match entry.original_url.as_deref() {
                Some(u) if !u.is_empty() => u,
                _ => return true,
            }
        };

        let key = match &entry.reposted_by {
            Some(reposter) => format!("repost:{}:{}", reposter, uri),
            None => uri.to_string(),
        };

        if seen.contains(&key) {
            false
        } else {
            seen.insert(key);
            true
        }
    });

    #[cfg(debug_assertions)]
    println!("get_timeline_unified: total pipeline took {:?}, returning {} items", t_start.elapsed(), feed.len());

    let has_more = next_cursor.is_some() || !feed.is_empty();
    Ok(FeedPage {
        items: feed,
        cursor: next_cursor,
        has_more,
    })
}