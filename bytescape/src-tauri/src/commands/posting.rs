//! Post Publishing & Autocomplete Commands
//!
//! Multi-destination post creation with media handling, rich text, and link cards,
//! plus mention/hashtag autocomplete for the compose box.
//!
//! # How It Works
//!
//! 1. **Frontend calls `publish_post_v2`** with text, attachments, a destination string
//!    (comma-separated: `"local"`, `"bluesky"`, `"mastodon"`, or `"all"`), and an
//!    optional link card.
//! 2. **Attachments are ingested into Iroh blobs** — either referencing existing vault
//!    blob hashes or reading new files from disk and storing them via `sync.store_media`.
//! 3. **Local vault publish** creates a `BytescapeItem` with automatic type detection
//!    (Status / Gallery / Article), audience flags, and syndication status.
//! 4. **Bluesky publish** uploads media blobs to the PDS, resolves rich-text facets
//!    (mentions → DIDs, hashtags, links), builds the `app.bsky.feed.post` record with
//!    embeds (images, video, or external link card), and creates it via XRPC.
//! 5. **Mastodon publish** posts via the Mastodon API (text only — see tech debt below).
//!    On success, the local vault item is updated with syndication metadata.
//!
//! # Tech Debt
//!
//! - **Mastodon path drops attachments silently** — media blobs are not uploaded to
//!   Mastodon; only plain text is posted via `post_status`. Needs media upload support.
//! - **Duplicates `publish_post`** in `import.rs` — the original single-destination
//!   publish function still exists; should be consolidated into this v2 implementation.
//! - **`search_mentions` has no debounce/cache** — every keystroke triggers a full DB
//!   scan of contacts. Consider a short TTL cache or moving debounce to the frontend.
//!
//! # Related Files
//!
//! - [`commands/import.rs`] — original `publish_post`, `PublishResult`, `willow_path_for_item`
//! - [`richtext.rs`] — facet parsing for Bluesky (mentions, hashtags, links)
//! - [`connectors/bluesky.rs`] — `BlueskyClient`, XRPC helpers
//! - [`connectors/mastodon.rs`] — `post_status`, session helpers
//! - [`models.rs`] — `BytescapeItem`, `SyndicationStatus`, `Audience`
//! - [`blob_hash.rs`] — blob hash parsing for vault media references

use tauri::{State, Manager};
use serde::Deserialize;
use serde_json::json;
use crate::AppState;
use crate::models::{BytescapeItem, SyndicationStatus, BytescapeType, Audience};
use crate::connectors::Source;
use crate::commands::import::{PublishResult, willow_path_for_item};
use crate::richtext;
use crate::blob_hash;
use std::path::Path;
use bytes::Bytes;

// ═══════════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════════

/// A media attachment for a new post.
///
/// Either references an existing vault blob (via `blob_hash`) or a new file
/// on the local filesystem (via `path`). The publisher resolves these into
/// Iroh blob hashes before writing the `BytescapeItem`.
#[derive(Debug, Deserialize)]
pub struct Attachment {
    pub path: Option<String>,
    pub blob_hash: Option<String>,
    pub mime_type: Option<String>,
}

/// Link card metadata for embed.external (passed from frontend after unfurl).
#[derive(Debug, Deserialize)]
pub struct LinkCardInput {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Publishing
// ═══════════════════════════════════════════════════════════════════════════════

/// Publish a post to one or more destinations (local vault, Bluesky, Mastodon).
///
/// This is the primary compose-box publish command (v2). It handles:
/// - Multi-destination routing via comma-separated `destination` string
/// - Media attachment ingestion (local files → Iroh blobs, or existing blob refs)
/// - Rich-text facet resolution for Bluesky (mentions → DIDs, hashtags, links)
/// - Link card embeds (`app.bsky.embed.external`) with thumbnail upload
/// - Syndication tracking (`share_history`, `syndicated_uri`, audience flags)
///
/// # Parameters
/// - `text` — Post body (plain text; rich-text facets are resolved server-side)
/// - `attachments` — Media files or blob references to attach
/// - `destination` — Comma-separated targets: `"local"`, `"bluesky"`, `"mastodon"`, or `"all"`
/// - `item_type` — Optional override: `"Article"`, `"Gallery"`, or auto-detected
/// - `title` — Optional title (for Article type)
/// - `link_card` — Optional unfurled link card metadata from the frontend
///
/// # Returns
/// `PublishResult` with per-destination success flags and any error messages.
///
/// # Side Effects
/// - Stores media blobs in Iroh via `sync.store_media`
/// - Writes a `BytescapeItem` to the local vault DB
/// - Creates an `app.bsky.feed.post` record on the user's Bluesky PDS
/// - Posts a status to the user's Mastodon instance
/// - Updates the local item with syndication metadata on remote success
///
/// # IPC
/// Tauri command — called from the frontend ComposeBox via
/// `invoke('publish_post_v2', ...)`.
#[tauri::command]
pub async fn publish_post_v2(
    text: String,
    attachments: Vec<Attachment>,
    destination: String,
    item_type: Option<String>,
    title: Option<String>,
    link_card: Option<LinkCardInput>,
    state: State<'_, AppState>,
) -> Result<PublishResult, String> {
    let content = text.trim();
    if content.is_empty() && attachments.is_empty() {
        return Err("Post cannot be empty".to_string());
    }

    let dest = destination.to_lowercase();
    let parts: Vec<&str> = dest.split(',').map(|s| s.trim()).collect();
    let publish_local = parts.contains(&"local") || parts.contains(&"all");
    let publish_bluesky = parts.contains(&"bluesky") || parts.contains(&"all");
    let publish_mastodon = parts.contains(&"mastodon") || parts.contains(&"all");
    
    let mut result = PublishResult {
        local: false,
        bluesky: false,
        mastodon: false,
        activitypub: false,
        bluesky_uri: None,
        errors: Vec::new(),
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // 1. Process attachments into Iroh blobs and gather hashes
    let mut media_blobs = Vec::new();
    let mut media_mimes = Vec::new();

    {
        let sync_guard = state.sync_manager.lock().await;
        let sync = sync_guard.as_ref().ok_or("Sync manager not initialized")?;

        for attachment in attachments {
            if let Some(blob_hash_str) = attachment.blob_hash {
                // Existing vault item (blob:hash)
                media_blobs.push(blob_hash_str.clone());
                media_mimes.push(attachment.mime_type.unwrap_or_else(|| "image/jpeg".to_string()));
            } else if let Some(path_str) = attachment.path {
                // New upload from local filesystem
                let path = Path::new(&path_str);
                if !path.exists() {
                     println!("posting: attachment path not found: {}", path_str);
                     continue;
                }
                
                let bytes_vec = std::fs::read(path).map_err(|e| format!("Failed to read attachment: {}", e))?;
                let bytes = Bytes::from(bytes_vec);
                
                let mime = attachment.mime_type.unwrap_or_else(|| {
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                    match ext.as_str() {
                        "jpg" | "jpeg" => "image/jpeg".to_string(),
                        "png" => "image/png".to_string(),
                        "webp" => "image/webp".to_string(),
                        "gif" => "image/gif".to_string(),
                        "mp4" => "video/mp4".to_string(),
                        "mov" => "video/quicktime".to_string(),
                        _ => "image/jpeg".to_string()
                    }
                });

                match sync.store_media(bytes).await {
                    Ok(hash) => {
                        media_blobs.push(format!("blob:{}", hash));
                        media_mimes.push(mime);
                    }
                    Err(e) => {
                        println!("posting: failed to store attachment in vault: {}", e);
                    }
                }
            }
        }
    }

    // 2. Local publish (Social Vault)
    let mut local_item_id = None;
    if publish_local {
         let db_guard = state.db_manager.read().await;
         if let Some(db) = db_guard.as_ref() {
            let item_id = uuid::Uuid::new_v4().to_string();
            local_item_id = Some(item_id.clone());
            
            // Determine item type (allow override from frontend)
            let resolved_item_type = match item_type.as_deref() {
                Some("Article") => BytescapeType::Article,
                Some("Gallery") => BytescapeType::Gallery,
                _ => {
                    let has_video = media_mimes.iter().any(|m| m.starts_with("video/"));
                    if has_video {
                        BytescapeType::Status
                    } else if media_blobs.len() > 1 {
                        BytescapeType::Gallery
                    } else {
                        BytescapeType::Status
                    }
                }
            };
            
            let item = BytescapeItem {
                id: item_id.clone(),
                original_timestamp: now as i64,
                content: content.to_string(),
                media_paths: vec![],
                source: Source::Manual.as_str().to_string(),
                path: willow_path_for_item(Source::Manual.as_str(), resolved_item_type, now as i64, &item_id),
                imported_at: now,
                title: title.clone(),
                original_id: None,
                syndicated_uris: std::collections::HashMap::new(),
                syndicated_at: std::collections::HashMap::new(),
                author_did: state.keychain.get_did().ok().flatten(),
                author_handle: {
                    // Use Bytescape handle (username@bytescape.social) rather than
                    // the Bluesky handle.  Falls back to Bluesky handle if no
                    // local username has been set yet.
                    let display = state.keychain.get_display_name();
                    if display == "Anonymous" {
                        state.keychain.load_session().ok().flatten().map(|(_, handle)| handle)
                    } else {
                        Some(format!("{}@bytescape.social", display))
                    }
                },
                author_avatar: None,
                tagged_people: vec![],
                location: None,
                is_liked: false,
                like_count: None,
                reply_count: None,
                repost_count: None,
                quote_count: None,
                item_type: resolved_item_type,
                url: None,
                link_url: link_card.as_ref().map(|lc| lc.url.clone()),
                content_html: None,
                preview_img: link_card.as_ref().and_then(|lc| lc.image_url.clone()),
                media_blobs: media_blobs.clone(),
                thumbnail_blobs: vec![],
                media_mimes: media_mimes.clone(),
                syndication_status: if publish_bluesky || publish_mastodon { SyndicationStatus::PendingSyndication } else { SyndicationStatus::LocalOnly },
                syndicated_uri: None,
                audiences: {
                    let mut a = vec![Audience::Local];
                    if publish_bluesky { a.push(Audience::Bluesky); }
                    if publish_mastodon { a.push(Audience::Activitypub); }
                    a
                },
                share_history: Vec::new(),
                reposted_by_did: None,
                reposted_by_handle: None,
                reply_parent_uri: None,
                reply_parent_handle: None,
                collab_doc_id: None,
            };
            
            if let Err(e) = db.save_item(&item).await {
                result.errors.push(format!("Local save failed: {}", e));
            } else {
                result.local = true;
            }
         }
    }

    // 3. Bluesky publish
    if publish_bluesky {
        let user_did = state.get_user_did()?;

        let mut images_embed = Vec::new();
        let mut video_embed = None;

        let sync_guard = state.sync_manager.lock().await;
        let sync = sync_guard.as_ref().ok_or("Sync manager not initialized")?;

        // Prepare cache directory for streaming blob exports (avoids full-blob memory reads)
        let app_data_dir = state.app_handle.path()
            .app_data_dir()
            .map_err(|e| format!("App data dir error: {}", e))?;
        let cache_dir = app_data_dir.join("blob_cache");
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| format!("Cache dir error: {}", e))?;

        for (i, blob_hash_str) in media_blobs.iter().enumerate() {
            let mime = media_mimes.get(i).cloned().unwrap_or_else(|| "image/jpeg".to_string());
            
            let hash = match blob_hash::parse_blob_hash(blob_hash_str) {
                Ok(h) => h,
                Err(e) => {
                    println!("posting: invalid blob hash '{}': {}", blob_hash_str, e);
                    continue;
                }
            };

            if let Ok(header) = sync.get_media_header(hash, 12).await {
                let ext = super::media::detect_extension_from_bytes(&header);
                let normalized = blob_hash::normalize_blob_ref(blob_hash_str);
                let cache_path = cache_dir.join(format!("{}.{}", &normalized, ext));
                if !cache_path.exists() {
                    if let Err(e) = sync.export_media_to_file(hash, &cache_path).await {
                        println!("posting: failed to export blob {}: {}", blob_hash_str, e);
                        continue;
                    }
                }
                let data = match std::fs::read(&cache_path) {
                    Ok(d) => d,
                    Err(e) => {
                        println!("posting: failed to read cached blob {}: {}", blob_hash_str, e);
                        continue;
                    }
                };
                if let Ok(upload_res) = state.xrpc_upload_blob(data, &mime).await {
                    if let Some(blob_ref) = upload_res.get("blob") {
                        if mime.starts_with("video/") {
                            video_embed = Some(json!({
                                "$type": "app.bsky.embed.video",
                                "video": blob_ref
                            }));
                            break; 
                        } else {
                            images_embed.push(json!({
                                "alt": "",
                                "image": blob_ref
                            }));
                            if images_embed.len() >= 4 { break; }
                        }
                    }
                }
            }
        }
        drop(sync_guard);

        // Parse rich text facets (mentions → DID resolution, hashtags, links)
        // For OAuth sessions, pass empty string — facet resolution uses public API
        let facet_jwt = state.get_fresh_access_jwt().await.unwrap_or_default();
        let facets = richtext::parse_facets_for_bluesky(content, &facet_jwt).await;

        let mut record = json!({
            "$type": "app.bsky.feed.post",
            "text": content,
            "createdAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        });

        if !facets.is_empty() {
            record["facets"] = serde_json::to_value(&facets).unwrap_or_default();
        }

        if let Some(video) = video_embed {
            record["embed"] = video;
        } else if !images_embed.is_empty() {
            record["embed"] = json!({
                "$type": "app.bsky.embed.images",
                "images": images_embed
            });
        } else if let Some(ref lc) = link_card {
            // Build app.bsky.embed.external from link card
            let mut external = json!({
                "uri": lc.url,
                "title": lc.title.as_deref().unwrap_or(""),
                "description": lc.description.as_deref().unwrap_or(""),
            });

            // If there's a thumbnail URL, download and upload it as a blob
            if let Some(ref img_url) = lc.image_url {
                if let Ok(thumb_blob_ref) = download_and_upload_thumb(&state, img_url).await {
                    external["thumb"] = thumb_blob_ref;
                }
            }

            record["embed"] = json!({
                "$type": "app.bsky.embed.external",
                "external": external
            });
        }

        match state.xrpc_post_json("com.atproto.repo.createRecord", json!({
            "repo": user_did,
            "collection": "app.bsky.feed.post",
            "record": record
        })).await {
            Ok(res) => {
                if let Some(uri) = res["uri"].as_str() {
                    result.bluesky = true;
                    result.bluesky_uri = Some(uri.to_string());

                    // Update local item if created
                    if let Some(ref id) = local_item_id {
                         let db_guard = state.db_manager.read().await;
                         if let Some(db) = db_guard.as_ref() {
                             if let Ok(Some(mut item)) = db.get_item_by_id(&id).await {
                                 item.syndication_status = SyndicationStatus::Syndicated;
                                 item.syndicated_uri = Some(uri.to_string());
                                 if !item.audiences.contains(&Audience::Bluesky) {
                                     item.audiences.push(Audience::Bluesky);
                                 }
                                 let now_ts = std::time::SystemTime::now()
                                     .duration_since(std::time::UNIX_EPOCH)
                                     .unwrap_or_default().as_secs();
                                 item.share_history.push(crate::models::SyndicationEvent {
                                     platform: "bluesky".to_string(),
                                     uri: uri.to_string(),
                                     timestamp: now_ts,
                                     active: true,
                                 });
                                 item.url = Some(uri.to_string());
                                 let _ = db.upsert_item(&item).await;
                             }
                         }
                    }
                }
            }
            Err(e) => {
                result.errors.push(format!("Bluesky publish failed: {}", e));
            }
        }
    }

    // 4. Mastodon publish
    if publish_mastodon {
        match crate::connectors::mastodon::get_session_from_keychain(&state.keychain) {
            Ok(session) => {
                match crate::connectors::mastodon::post_status(
                    &state.http_client,
                    &session,
                    content,
                    "public",
                    None,
                ).await {
                    Ok(status) => {
                        result.mastodon = true;
                        println!("posting: mastodon status published: id={}", status.id);

                        // Update local vault item with Mastodon URI + share_history
                        // so the unified feed dedup can remove the Mastodon timeline copy.
                        if let Some(ref id) = local_item_id {
                            let mastodon_url = status.url.clone()
                                .unwrap_or_else(|| status.uri.clone());
                            let db_guard = state.db_manager.read().await;
                            if let Some(db) = db_guard.as_ref() {
                                if let Ok(Some(mut item)) = db.get_item_by_id(id).await {
                                    if !item.audiences.contains(&Audience::Activitypub) {
                                        item.audiences.push(Audience::Activitypub);
                                    }
                                    item.share_history.push(crate::models::SyndicationEvent {
                                        platform: "mastodon".to_string(),
                                        uri: mastodon_url.clone(),
                                        timestamp: now,
                                        active: true,
                                    });
                                    // If no Bluesky syndication happened, use Mastodon as the syndicated URI
                                    if item.syndicated_uri.is_none() {
                                        item.syndicated_uri = Some(mastodon_url);
                                    }
                                    item.syndication_status = SyndicationStatus::Syndicated;
                                    let _ = db.upsert_item(&item).await;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        result.errors.push(format!("Mastodon publish failed: {}", e));
                    }
                }
            }
            Err(_) => {
                result.errors.push("Mastodon not connected".to_string());
            }
        }
    }

    Ok(result)
}

/// Downloads a thumbnail image from a URL and uploads it to the Bluesky PDS.
/// Returns the blob ref JSON on success.
async fn download_and_upload_thumb(
    state: &State<'_, AppState>,
    image_url: &str,
) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client.get(image_url)
        .header("User-Agent", "Bytescape/1.0")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch thumbnail: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Thumbnail fetch returned {}", resp.status()));
    }

    // Infer MIME from content-type header or fall back to jpeg
    let content_type = resp.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();

    let mime = if content_type.contains("png") {
        "image/png"
    } else if content_type.contains("webp") {
        "image/webp"
    } else if content_type.contains("gif") {
        "image/gif"
    } else {
        "image/jpeg"
    };

    // Limit to 1MB
    let bytes = resp.bytes().await.map_err(|e| format!("Failed to read thumbnail: {}", e))?;
    if bytes.len() > 1_000_000 {
        return Err("Thumbnail too large (>1MB)".to_string());
    }

    let upload_res = state.xrpc_upload_blob(bytes.to_vec(), mime).await
        .map_err(|e| format!("Failed to upload thumbnail: {}", e))?;

    upload_res.get("blob").cloned()
        .ok_or_else(|| "Upload response missing blob ref".to_string())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Mentions & Autocomplete
// ═══════════════════════════════════════════════════════════════════════════════

/// A single mention suggestion returned by `search_mentions`.
///
/// Populated from local contacts stored in the vault DB.
#[derive(serde::Serialize, Clone)]
pub struct MentionSuggestion {
    pub handle: String,
    pub display_name: Option<String>,
    pub did: Option<String>,
    pub avatar_url: Option<String>,
}

/// Search for @-mention suggestions while composing a post.
///
/// Queries local vault contacts whose handle contains the search string.
/// Results are deduplicated by handle and capped at 10.
///
/// # Parameters
/// - `query` — The partial handle text (without the leading `@`)
///
/// # Returns
/// Up to 10 `MentionSuggestion` items sorted alphabetically by handle.
///
/// # Tech Debt
/// - No debounce or result caching — every keystroke triggers a full DB scan.
/// - Only searches local contacts; does not query Bluesky typeahead API.
///
/// # IPC
/// Tauri command — called from the ComposeBox mention autocomplete via
/// `invoke('search_mentions', { query })`.
#[tauri::command]
pub async fn search_mentions(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<MentionSuggestion>, String> {
    let q = query.to_lowercase();
    let mut results: Vec<MentionSuggestion> = Vec::new();

    // Search local contacts
    let db_guard = state.db_manager.read().await;
    if let Some(db) = db_guard.as_ref() {
        if let Ok(contacts) = db.list_contacts().await {
            for c in contacts {
                let handle_lower = c.handle.to_lowercase();
                if handle_lower.contains(&q) {
                    results.push(MentionSuggestion {
                        handle: c.handle.clone(),
                        display_name: None,
                        did: Some(c.did.clone()),
                        avatar_url: None,
                    });
                }
            }
        }
    }
    drop(db_guard);

    // De-duplicate by handle
    results.sort_by(|a, b| a.handle.cmp(&b.handle));
    results.dedup_by(|a, b| a.handle == b.handle);

    // Limit results
    results.truncate(10);

    Ok(results)
}

/// Search for #hashtag suggestions while composing a post.
///
/// Queries the user's followed hashtags for matches containing the search
/// string (case-insensitive, leading `#` stripped).
///
/// # Parameters
/// - `query` — The partial hashtag text (with or without leading `#`)
///
/// # Returns
/// Up to 10 matching hashtag strings.
///
/// # IPC
/// Tauri command — called from the ComposeBox hashtag autocomplete via
/// `invoke('search_hashtags_autocomplete', { query })`.
#[tauri::command]
pub async fn search_hashtags_autocomplete(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let q = query.to_lowercase().replace('#', "");
    let mut results: Vec<String> = Vec::new();

    let db_guard = state.db_manager.read().await;
    if let Some(db) = db_guard.as_ref() {
        if let Ok(hashtags) = db.list_followed_hashtags().await {
            for tag in hashtags {
                if tag.to_lowercase().contains(&q) {
                    results.push(tag);
                }
            }
        }
    }
    drop(db_guard);

    results.truncate(10);
    Ok(results)
}
