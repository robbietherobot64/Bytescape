//! Social Interactions — Like, Repost, Reply & Thread Fetching
//!
//! Implements the interactive social verbs that users trigger from the feed:
//! liking/unliking posts, reposting/unreposting, composing replies with
//! media attachments, and fetching full conversation threads.
//!
//! **Currently Bluesky-only** via the AT Protocol. All operations go through
//! XRPC calls to the user's authenticated PDS. Mastodon/ActivityPub
//! interactions are not yet implemented.
//!
//! ## How It Works
//!
//! 1. **Authenticate** — Each command reads the user's DID and fresh access JWT
//!    from `AppState` (see `get_user_did()` / `get_fresh_access_jwt()`).
//! 2. **Build record** — Constructs the appropriate AT Protocol record JSON
//!    (`app.bsky.feed.like`, `app.bsky.feed.repost`, or `app.bsky.feed.post`
//!    with a `reply` facet).
//! 3. **XRPC call** — Sends `com.atproto.repo.createRecord` (or `deleteRecord`
//!    for undo operations) to the user's PDS via `AppState::xrpc_post_json`.
//! 4. **Media handling** (replies only) — Uploads image/video attachments from
//!    Iroh blobs or local files to the PDS before composing the reply record.
//! 5. **Return references** — Returns the AT URI + CID of the created record
//!    so the frontend can update optimistic UI state.
//!
//! ## Related Files
//!
//! - `lib.rs` — `AppState` with `xrpc_post_json`, `xrpc_get`, `xrpc_upload_blob`
//! - `connectors/bluesky.rs` — `extract_image_urls_from_view` for thread images
//! - `sync.rs` — `SyncManager::get_media` for fetching attachment blobs from Iroh
//! - Frontend: `src/components/PostCard.tsx` — like/repost/reply UI triggers
//! - Frontend: `src/components/ThreadView.tsx` — thread display

use tauri::{State, Manager};
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::AppState;

// ═══════════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, Deserialize)]
pub struct LikeResult {
    pub uri: String,
    pub cid: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RepostResult {
    pub uri: String,
    pub cid: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReplyResult {
    pub uri: String,
    pub cid: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThreadReply {
    pub uri: String,
    pub cid: String,
    pub text: String,
    pub author_did: String,
    pub author_handle: String,
    pub author_avatar: Option<String>,
    pub created_at: i64,
    pub like_count: Option<i64>,
    pub reply_count: Option<i64>,
    pub repost_count: Option<i64>,
    pub viewer_like: Option<String>,
    pub viewer_repost: Option<String>,
    pub image_urls: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReplyAttachment {
    pub path: Option<String>,
    pub blob_hash: Option<String>,
    pub mime_type: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

/// Extract rkey from an AT URI (at://did/collection/rkey)
fn rkey_from_uri(uri: &str) -> Option<String> {
    uri.split('/').last().map(|s| s.to_string())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Like / Unlike
// ═══════════════════════════════════════════════════════════════════════════════

/// Create a "like" on a Bluesky post.
///
/// Creates an `app.bsky.feed.like` record on the user's PDS, referencing the
/// target post by AT URI and CID.
///
/// # Parameters
/// - `uri` — AT URI of the post to like (`at://did:plc:.../app.bsky.feed.post/...`).
/// - `cid` — Content hash (CID) of the post.
/// - `state` — Tauri managed state.
///
/// # Returns
/// A `LikeResult` with the URI and CID of the created like record.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('like_post', { uri, cid })`
#[tauri::command]
pub async fn like_post(
    uri: String,
    cid: String,
    state: State<'_, AppState>,
) -> Result<LikeResult, String> {
    let user_did = state.get_user_did()?;

    let res = state.xrpc_post_json(
        "com.atproto.repo.createRecord",
        json!({
            "repo": user_did,
            "collection": "app.bsky.feed.like",
            "record": {
                "$type": "app.bsky.feed.like",
                "subject": { "uri": uri, "cid": cid },
                "createdAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
            }
        }),
    ).await?;

    Ok(LikeResult {
        uri: res["uri"].as_str().unwrap_or_default().to_string(),
        cid: res["cid"].as_str().unwrap_or_default().to_string(),
    })
}

/// Remove a "like" from a Bluesky post.
///
/// Deletes the `app.bsky.feed.like` record identified by its AT URI.
/// The rkey is extracted from the URI to target the correct record.
///
/// # Parameters
/// - `like_uri` — AT URI of the like record to delete.
/// - `state` — Tauri managed state.
///
/// # Returns
/// `true` on successful deletion.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('unlike_post', { likeUri })`
#[tauri::command]
pub async fn unlike_post(
    like_uri: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let user_did = state.get_user_did()?;
    let rkey = rkey_from_uri(&like_uri).ok_or("Invalid like URI")?;

    state.xrpc_post_json(
        "com.atproto.repo.deleteRecord",
        json!({
            "repo": user_did,
            "collection": "app.bsky.feed.like",
            "rkey": rkey
        }),
    ).await?;

    Ok(true)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Repost / Unrepost
// ═══════════════════════════════════════════════════════════════════════════════

/// Repost (boost) a Bluesky post.
///
/// Creates an `app.bsky.feed.repost` record on the user's PDS, referencing
/// the target post by AT URI and CID.
///
/// # Parameters
/// - `uri` — AT URI of the post to repost.
/// - `cid` — Content hash (CID) of the post.
/// - `state` — Tauri managed state.
///
/// # Returns
/// A `RepostResult` with the URI and CID of the created repost record.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('repost_post', { uri, cid })`
#[tauri::command]
pub async fn repost_post(
    uri: String,
    cid: String,
    state: State<'_, AppState>,
) -> Result<RepostResult, String> {
    let user_did = state.get_user_did()?;

    let res = state.xrpc_post_json(
        "com.atproto.repo.createRecord",
        json!({
            "repo": user_did,
            "collection": "app.bsky.feed.repost",
            "record": {
                "$type": "app.bsky.feed.repost",
                "subject": { "uri": uri, "cid": cid },
                "createdAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
            }
        }),
    ).await?;

    Ok(RepostResult {
        uri: res["uri"].as_str().unwrap_or_default().to_string(),
        cid: res["cid"].as_str().unwrap_or_default().to_string(),
    })
}

/// Remove a repost from a Bluesky post.
///
/// Deletes the `app.bsky.feed.repost` record identified by its AT URI.
///
/// # Parameters
/// - `repost_uri` — AT URI of the repost record to delete.
/// - `state` — Tauri managed state.
///
/// # Returns
/// `true` on successful deletion.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('unrepost_post', { repostUri })`
#[tauri::command]
pub async fn unrepost_post(
    repost_uri: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let user_did = state.get_user_did()?;
    let rkey = rkey_from_uri(&repost_uri).ok_or("Invalid repost URI")?;

    state.xrpc_post_json(
        "com.atproto.repo.deleteRecord",
        json!({
            "repo": user_did,
            "collection": "app.bsky.feed.repost",
            "rkey": rkey
        }),
    ).await?;

    Ok(true)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Reply
// ═══════════════════════════════════════════════════════════════════════════════

/// Compose and publish a reply to a Bluesky post.
///
/// Supports text content with optional image and video attachments. Attachments
/// can be sourced from Iroh blob hashes or local file paths. Uploads media to
/// the PDS before composing the `app.bsky.feed.post` record with a `reply`
/// reference linking to both the parent and root posts.
///
/// # Parameters
/// - `parent_uri` / `parent_cid` — The direct parent post being replied to.
/// - `root_uri` / `root_cid` — The root post of the thread.
/// - `text` — Reply text content.
/// - `attachments` — Optional media (images or one video).
/// - `state` — Tauri managed state.
///
/// # Returns
/// A `ReplyResult` with the URI and CID of the created reply.
///
/// # Notes
/// - Image embeds capped at **4** (Bluesky limit); extras are silently dropped.
/// - A single video embed takes priority — if a video is present, images are skipped.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('reply_to_post', { parentUri, parentCid, rootUri, rootCid, text, attachments })`
#[tauri::command]
pub async fn reply_to_post(
    parent_uri: String,
    parent_cid: String,
    root_uri: String,
    root_cid: String,
    text: String,
    attachments: Vec<ReplyAttachment>,
    state: State<'_, AppState>,
) -> Result<ReplyResult, String> {
    let content = text.trim();
    if content.is_empty() && attachments.is_empty() {
        return Err("Reply cannot be empty".to_string());
    }

    let user_did = state.get_user_did()?;

    // Upload attachments as blobs
    let mut images_embed = Vec::new();
    let mut video_embed = None;

    {
        let sync_guard = state.sync_manager.lock().await;
        let sync = sync_guard.as_ref().ok_or("Sync manager not initialized")?;

        // Prepare cache directory for streaming blob exports
        let app_data_dir = state.app_handle.path()
            .app_data_dir()
            .map_err(|e| e.to_string())?;
        let cache_dir = app_data_dir.join("blob_cache");
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| e.to_string())?;

        for attachment in &attachments {
            let mime = attachment.mime_type.clone().unwrap_or_else(|| "image/jpeg".to_string());

            let data = if let Some(blob_hash_str) = &attachment.blob_hash {
                let mut normalized = blob_hash_str.trim().to_string();
                while let Some(rest) = normalized.strip_prefix("blob:") {
                    normalized = rest.trim().to_string();
                }
                if let Some(pos) = normalized.find(':') {
                    normalized = normalized[..pos].to_string();
                }

                let hash_bytes = if normalized.len() == 64 {
                    hex::decode(&normalized).map_err(|e| e.to_string())?
                } else {
                    data_encoding::BASE32_NOPAD.decode(normalized.to_ascii_uppercase().as_bytes())
                        .map_err(|e| e.to_string())?
                };

                if hash_bytes.len() != 32 { continue; }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&hash_bytes);
                let hash = crate::sync::Hash::from_bytes(arr);
                // Stream blob to cache, then read (avoids Iroh double-buffer)
                let header = sync.get_media_header(hash, 12).await
                    .map_err(|e| e.to_string())?;
                let ext = super::media::detect_extension_from_bytes(&header);
                let cache_path = cache_dir.join(format!("{}.{}", &normalized, ext));
                if !cache_path.exists() {
                    sync.export_media_to_file(hash, &cache_path).await
                        .map_err(|e| e.to_string())?;
                }
                std::fs::read(&cache_path).map_err(|e| e.to_string())?
            } else if let Some(path_str) = &attachment.path {
                let path = std::path::Path::new(path_str);
                if !path.exists() { continue; }
                std::fs::read(path).map_err(|e| format!("Failed to read attachment: {}", e))?
            } else {
                continue;
            };

            let upload_res = state.xrpc_upload_blob(data, &mime).await?;
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

    // Build the reply record
    let mut record = json!({
        "$type": "app.bsky.feed.post",
        "text": content,
        "createdAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "reply": {
            "root": { "uri": root_uri, "cid": root_cid },
            "parent": { "uri": parent_uri, "cid": parent_cid }
        }
    });

    if let Some(video) = video_embed {
        record["embed"] = video;
    } else if !images_embed.is_empty() {
        record["embed"] = json!({
            "$type": "app.bsky.embed.images",
            "images": images_embed
        });
    }

    let res = state.xrpc_post_json(
        "com.atproto.repo.createRecord",
        json!({
            "repo": user_did,
            "collection": "app.bsky.feed.post",
            "record": record
        }),
    ).await?;

    Ok(ReplyResult {
        uri: res["uri"].as_str().unwrap_or_default().to_string(),
        cid: res["cid"].as_str().unwrap_or_default().to_string(),
    })
}

// ═══════════════════════════════════════════════════════════════════════════════
// Thread
// ═══════════════════════════════════════════════════════════════════════════════

/// Fetch a full conversation thread from Bluesky.
///
/// Retrieves the thread rooted at the given post URI using
/// `app.bsky.feed.getPostThread`, then flattens all nested replies into a
/// flat `Vec<ThreadReply>` via depth-first recursive traversal.
///
/// # Parameters
/// - `uri` — AT URI of the post whose thread to fetch.
/// - `depth` — Maximum reply depth to retrieve (default: 6).
/// - `state` — Tauri managed state.
///
/// # Returns
/// A flat list of `ThreadReply` structs ordered by depth-first traversal.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('get_post_thread', { uri, depth? })`
#[tauri::command]
pub async fn get_post_thread(
    uri: String,
    depth: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<ThreadReply>, String> {
    let depth_val = depth.unwrap_or(6);
    let endpoint = format!(
        "app.bsky.feed.getPostThread?uri={}&depth={}",
        urlencoding::encode(&uri),
        depth_val
    );

    let body = state.xrpc_get(&endpoint).await?;

    let mut replies = Vec::new();
    if let Some(thread) = body.get("thread") {
        extract_replies_recursive(thread, &mut replies);
    }

    Ok(replies)
}

/// Recursively walk a Bluesky thread node, extracting each reply into the flat list.
///
/// Parses author, record text, timestamps, engagement counts, viewer state, and
/// embedded image URLs from the AT Protocol `getPostThread` response shape.
fn extract_replies_recursive(thread_node: &serde_json::Value, replies: &mut Vec<ThreadReply>) {
    if let Some(replies_arr) = thread_node.get("replies").and_then(|r| r.as_array()) {
        for reply_node in replies_arr {
            if let Some(post) = reply_node.get("post") {
                let empty = json!({});
                let author = post.get("author").unwrap_or(&empty);
                let record = post.get("record").unwrap_or(&empty);
                let viewer = post.get("viewer").unwrap_or(&empty);

                let created_at = record.get("createdAt")
                    .and_then(|s| s.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.timestamp())
                    .unwrap_or(0);

                let mut image_urls = Vec::new();
                if let Some(embed) = post.get("embed") {
                    crate::connectors::bluesky::extract_image_urls_from_view(embed, &mut image_urls);
                }

                replies.push(ThreadReply {
                    uri: post.get("uri").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                    cid: post.get("cid").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                    text: record.get("text").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                    author_did: author.get("did").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                    author_handle: author.get("handle").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                    author_avatar: author.get("avatar").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    created_at,
                    like_count: post.get("likeCount").and_then(|v| v.as_i64()),
                    reply_count: post.get("replyCount").and_then(|v| v.as_i64()),
                    repost_count: post.get("repostCount").and_then(|v| v.as_i64()),
                    viewer_like: viewer.get("like").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    viewer_repost: viewer.get("repost").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    image_urls,
                });

                // Recurse into nested replies
                extract_replies_recursive(reply_node, replies);
            }
        }
    }
}
