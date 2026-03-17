use tauri::State;
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::AppState;
use crate::connectors::bluesky::{BlueskyClient, authenticated_xrpc_candidates};

// ─── Types ───

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

// ─── Helpers ───

/// Extract rkey from an AT URI (at://did/collection/rkey)
fn rkey_from_uri(uri: &str) -> Option<String> {
    uri.split('/').last().map(|s| s.to_string())
}

/// Extract DID from an AT URI (at://did/collection/rkey)
#[allow(dead_code)]
fn did_from_uri(uri: &str) -> Option<String> {
    let stripped = uri.strip_prefix("at://")?;
    stripped.split('/').next().map(|s| s.to_string())
}

// ─── Like / Unlike ───

#[tauri::command]
pub async fn like_post(
    uri: String,
    cid: String,
    state: State<'_, AppState>,
) -> Result<LikeResult, String> {
    let access_jwt = state.get_fresh_access_jwt().await?;
    let (user_did, _) = state.keychain.load_session()
        .ok().flatten()
        .ok_or("Not authenticated")?;

    let candidates = authenticated_xrpc_candidates(&access_jwt, true);
    let pds_base = candidates.first().ok_or("Could not resolve PDS")?;
    let client = BlueskyClient::with_jwt(&access_jwt);

    let create_url = format!("{}/com.atproto.repo.createRecord", pds_base);
    let resp = client.create_record(&create_url, json!({
        "repo": user_did,
        "collection": "app.bsky.feed.like",
        "record": {
            "$type": "app.bsky.feed.like",
            "subject": {
                "uri": uri,
                "cid": cid
            },
            "createdAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        }
    })).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Like failed ({}): {}", status, body));
    }

    let res: serde_json::Value = resp.json().await
        .map_err(|e| format!("Parse like response: {}", e))?;

    Ok(LikeResult {
        uri: res["uri"].as_str().unwrap_or_default().to_string(),
        cid: res["cid"].as_str().unwrap_or_default().to_string(),
    })
}

#[tauri::command]
pub async fn unlike_post(
    like_uri: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let access_jwt = state.get_fresh_access_jwt().await?;
    let (user_did, _) = state.keychain.load_session()
        .ok().flatten()
        .ok_or("Not authenticated")?;

    let rkey = rkey_from_uri(&like_uri)
        .ok_or("Invalid like URI")?;

    let candidates = authenticated_xrpc_candidates(&access_jwt, true);
    let pds_base = candidates.first().ok_or("Could not resolve PDS")?;
    let client = BlueskyClient::with_jwt(&access_jwt);

    let delete_url = format!("{}/com.atproto.repo.deleteRecord", pds_base);
    let resp = client.delete_record(&delete_url, json!({
        "repo": user_did,
        "collection": "app.bsky.feed.like",
        "rkey": rkey
    })).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Unlike failed ({}): {}", status, body));
    }

    Ok(true)
}

// ─── Repost / Unrepost ───

#[tauri::command]
pub async fn repost_post(
    uri: String,
    cid: String,
    state: State<'_, AppState>,
) -> Result<RepostResult, String> {
    let access_jwt = state.get_fresh_access_jwt().await?;
    let (user_did, _) = state.keychain.load_session()
        .ok().flatten()
        .ok_or("Not authenticated")?;

    let candidates = authenticated_xrpc_candidates(&access_jwt, true);
    let pds_base = candidates.first().ok_or("Could not resolve PDS")?;
    let client = BlueskyClient::with_jwt(&access_jwt);

    let create_url = format!("{}/com.atproto.repo.createRecord", pds_base);
    let resp = client.create_record(&create_url, json!({
        "repo": user_did,
        "collection": "app.bsky.feed.repost",
        "record": {
            "$type": "app.bsky.feed.repost",
            "subject": {
                "uri": uri,
                "cid": cid
            },
            "createdAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        }
    })).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Repost failed ({}): {}", status, body));
    }

    let res: serde_json::Value = resp.json().await
        .map_err(|e| format!("Parse repost response: {}", e))?;

    Ok(RepostResult {
        uri: res["uri"].as_str().unwrap_or_default().to_string(),
        cid: res["cid"].as_str().unwrap_or_default().to_string(),
    })
}

#[tauri::command]
pub async fn unrepost_post(
    repost_uri: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let access_jwt = state.get_fresh_access_jwt().await?;
    let (user_did, _) = state.keychain.load_session()
        .ok().flatten()
        .ok_or("Not authenticated")?;

    let rkey = rkey_from_uri(&repost_uri)
        .ok_or("Invalid repost URI")?;

    let candidates = authenticated_xrpc_candidates(&access_jwt, true);
    let pds_base = candidates.first().ok_or("Could not resolve PDS")?;
    let client = BlueskyClient::with_jwt(&access_jwt);

    let delete_url = format!("{}/com.atproto.repo.deleteRecord", pds_base);
    let resp = client.delete_record(&delete_url, json!({
        "repo": user_did,
        "collection": "app.bsky.feed.repost",
        "rkey": rkey
    })).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Unrepost failed ({}): {}", status, body));
    }

    Ok(true)
}

// ─── Reply ───

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

    let access_jwt = state.get_fresh_access_jwt().await?;
    let (user_did, _) = state.keychain.load_session()
        .ok().flatten()
        .ok_or("Not authenticated")?;

    let candidates = authenticated_xrpc_candidates(&access_jwt, true);
    let pds_base = candidates.first().ok_or("Could not resolve PDS")?;
    let client = BlueskyClient::with_jwt(&access_jwt);

    // Upload attachments as blobs
    let mut images_embed = Vec::new();
    let mut video_embed = None;

    {
        let sync_guard = state.sync_manager.lock().await;
        let sync = sync_guard.as_ref().ok_or("Sync manager not initialized")?;

        for attachment in &attachments {
            let mime = attachment.mime_type.clone().unwrap_or_else(|| "image/jpeg".to_string());

            let data = if let Some(blob_hash_str) = &attachment.blob_hash {
                // Read from Iroh blob store
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
                sync.get_media(hash).await.map_err(|e| e.to_string())?.to_vec()
            } else if let Some(path_str) = &attachment.path {
                let path = std::path::Path::new(path_str);
                if !path.exists() { continue; }
                std::fs::read(path).map_err(|e| format!("Failed to read attachment: {}", e))?
            } else {
                continue;
            };

            let upload_url = format!("{}/com.atproto.repo.uploadBlob", pds_base);
            if let Ok(resp) = client.post_blob(&upload_url, &mime, data).await {
                if resp.status().is_success() {
                    if let Ok(upload_res) = resp.json::<serde_json::Value>().await {
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
        }
    }

    // Build the reply record
    let mut record = json!({
        "$type": "app.bsky.feed.post",
        "text": content,
        "createdAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "reply": {
            "root": {
                "uri": root_uri,
                "cid": root_cid
            },
            "parent": {
                "uri": parent_uri,
                "cid": parent_cid
            }
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

    let create_url = format!("{}/com.atproto.repo.createRecord", pds_base);
    let resp = client.create_record(&create_url, json!({
        "repo": user_did,
        "collection": "app.bsky.feed.post",
        "record": record
    })).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Reply failed ({}): {}", status, body));
    }

    let res: serde_json::Value = resp.json().await
        .map_err(|e| format!("Parse reply response: {}", e))?;

    Ok(ReplyResult {
        uri: res["uri"].as_str().unwrap_or_default().to_string(),
        cid: res["cid"].as_str().unwrap_or_default().to_string(),
    })
}

// ─── Thread ───

#[tauri::command]
pub async fn get_post_thread(
    uri: String,
    depth: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<ThreadReply>, String> {
    let access_jwt = state.get_fresh_access_jwt().await?;

    let candidates = authenticated_xrpc_candidates(&access_jwt, true);
    let pds_base = candidates.first().ok_or("Could not resolve PDS")?;
    let client = BlueskyClient::with_jwt(&access_jwt);

    let depth_val = depth.unwrap_or(6);
    let url = format!(
        "{}/app.bsky.feed.getPostThread?uri={}&depth={}",
        pds_base,
        urlencoding::encode(&uri),
        depth_val
    );

    let resp = client.get_resource(&url).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Get thread failed ({}): {}", status, body));
    }

    let body: serde_json::Value = resp.json().await
        .map_err(|e| format!("Parse thread response: {}", e))?;

    let mut replies = Vec::new();
    
    // Extract replies from the thread response
    if let Some(thread) = body.get("thread") {
        extract_replies_recursive(thread, &mut replies);
    }

    Ok(replies)
}

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
