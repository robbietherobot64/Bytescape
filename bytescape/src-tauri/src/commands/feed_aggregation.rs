use tauri::State;
use crate::AppState;
use crate::models::{FeedEntry, BytescapeItem, BytescapeType, Audience, audiences_from_legacy};
use crate::connectors::bluesky::BlueskyImportedPost;
use serde::{Serialize, Deserialize};

impl From<BlueskyImportedPost> for FeedEntry {
    fn from(post: BlueskyImportedPost) -> Self {
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

        FeedEntry {
            id: if !post.uri.is_empty() { post.uri.clone() } else { post.cid.clone() },
            timestamp: post.created_at,
            content: post.text,
            author_did: Some(post.author_did),
            author_handle: Some(post.author_handle),
            author_avatar: post.author_avatar,
            source: "bluesky".to_string(),
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
            quoted_post: post.quoted_post.map(|q| Box::new(FeedEntry::from(*q))),
            item_type: Some(BytescapeType::Status),
            syndication_status: None,
            audiences: vec![Audience::Local, Audience::Bluesky],
            share_history: vec![
                crate::models::SyndicationEvent {
                    platform: "bluesky".to_string(),
                    uri: post.uri.clone(),
                    timestamp: post.created_at as u64,
                    active: true,
                }
            ],
            location: None,
            tagged_people: vec![],
            title: None,
            original_url: Some(post.uri),
        }
    }
}

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

        FeedEntry {
            id: item.id.clone(),
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
            reposted_by: None,
            reposted_by_handle: None,
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

#[derive(Serialize, Deserialize)]
pub struct UnifiedFeedResponse {
    pub items: Vec<FeedEntry>,
    pub next_cursor: Option<String>,
}

#[tauri::command]
pub async fn get_timeline_unified(
    state: State<'_, AppState>,
    tags: Vec<String>,
    feed_uris: Vec<String>,
    limit: Option<u8>,
    cursor: Option<String>,
    vault_offset: Option<usize>
) -> Result<UnifiedFeedResponse, String> {
    let mut feed = Vec::new();
    let mut next_cursor = None;

    // 1. Fetch live timeline
    let tl_res = crate::commands::import::get_timeline(state.clone(), limit, cursor.clone()).await;
    if let Ok((posts, nc)) = tl_res {
        next_cursor = nc;
        for p in posts {
            feed.push(FeedEntry::from(p));
        }
    }

    // 2. Fetch custom feeds
    if !tags.is_empty() || !feed_uris.is_empty() {
        let feed_res = crate::commands::import::get_unified_feed(state.clone(), tags, feed_uris, limit.unwrap_or(50), cursor).await;
        if let Ok((posts, _)) = feed_res {
            for p in posts {
                feed.push(FeedEntry::from(p));
            }
        }
    }

    // 3. Fetch vault items
    let mut _vault_total = 0;
    let mut vault_entries = Vec::new();
    
    // Explicit block to limit the lifetime of `db_guard` so it's dropped 
    // before any subsequent await calls (if any), though here it's isolated anyway.
    {
        let db_manager = state.db_manager.clone();
        let db_guard = db_manager.lock().await;
        if let Some(db) = db_guard.as_ref() {
            let offset = vault_offset.unwrap_or(0);
            let vault_limit = 200; // Pull more vault items to mix properly
            if let Ok(Some((did, _))) = state.keychain.load_session() {
                if let Ok((items, total)) = db.list_my_items_paged(&did, None, vault_limit, offset).await {
                    _vault_total = total;
                    for item in items {
                        vault_entries.push(FeedEntry::from(item));
                    }
                }
            }
        }
    }

    // 4. Merge timeline items with vault items (Chain of Custody dedup)
    let mut syndicated_uris_in_vault = std::collections::HashSet::new();

    for v in &mut vault_entries {
        for event in &mut v.share_history {
            if event.timestamp == 0 {
                let matched_timeline_post = feed.iter().find(|e| e.id == event.uri || e.original_url.as_ref() == Some(&event.uri));
                if let Some(timeline_entry) = matched_timeline_post {
                    event.timestamp = timeline_entry.timestamp as u64;
                }
            }
            if event.active {
                syndicated_uris_in_vault.insert(event.uri.clone());
            }
        }
    }

    // Remove timeline entries that are already part of a vault item's chain of custody
    feed.retain(|entry| {
        let uri = entry.original_url.as_ref().unwrap_or(&entry.id);
        !syndicated_uris_in_vault.contains(uri)
    });

    // Bring vault items into the main feed
    feed.extend(vault_entries);

    // Sort by effective timestamp (latest of original or any active syndication)
    feed.sort_by(|a, b| {
        let a_max = a
            .share_history
            .iter()
            .filter(|e| e.active)
            .map(|e| e.timestamp as i64)
            .max()
            .unwrap_or(a.timestamp)
            .max(a.timestamp);
        let b_max = b
            .share_history
            .iter()
            .filter(|e| e.active)
            .map(|e| e.timestamp as i64)
            .max()
            .unwrap_or(b.timestamp)
            .max(b.timestamp);
        b_max.cmp(&a_max)
    });

    // Deduplicate by ID
    let mut seen = std::collections::HashSet::new();
    feed.retain(|entry| {
        let dedup_key = if !entry.id.is_empty() { entry.id.clone() } else { entry.original_url.clone().unwrap_or_default() };
        if seen.contains(&dedup_key) {
            false
        } else {
            seen.insert(dedup_key);
            true
        }
    });

    Ok(UnifiedFeedResponse {
        items: feed,
        next_cursor,
    })
}
