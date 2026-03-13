use tauri::State;
use serde::Deserialize;
use serde_json::json;
use crate::AppState;
use crate::models::{BytescapeItem, SyndicationStatus, BytescapeType, Audience};
use crate::connectors::{Source, bluesky::{BlueskyClient, authenticated_xrpc_candidates}};
use crate::commands::import::{PublishResult, willow_path_for_item};
use crate::richtext;
use crate::blob_hash;
use std::path::Path;
use bytes::Bytes;

#[derive(Debug, Deserialize)]
pub struct Attachment {
    pub path: Option<String>,
    pub blob_hash: Option<String>,
    pub mime_type: Option<String>,
}

/// Enhanced publish command supporting multiple media attachments and P2P/Bluesky routing.
#[tauri::command]
pub async fn publish_post_v2(
    text: String,
    attachments: Vec<Attachment>,
    destination: String,
    item_type: Option<String>,
    title: Option<String>,
    state: State<'_, AppState>,
) -> Result<PublishResult, String> {
    let content = text.trim();
    if content.is_empty() && attachments.is_empty() {
        return Err("Post cannot be empty".to_string());
    }

    let dest = destination.to_lowercase();
    let publish_local = dest == "local" || dest == "all";
    let publish_bluesky = dest == "bluesky" || dest == "all";
    
    let mut result = PublishResult {
        local: false,
        bluesky: false,
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
                author_handle: state.keychain.load_session().ok().flatten().map(|(_, handle)| handle),
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
                link_url: None,
                content_html: None,
                preview_img: None,
                media_blobs: media_blobs.clone(),
                thumbnail_blobs: vec![],
                media_mimes: media_mimes.clone(),
                syndication_status: if publish_bluesky { SyndicationStatus::PendingSyndication } else { SyndicationStatus::LocalOnly },
                syndicated_uri: None,
                audiences: {
                    let mut a = vec![Audience::Local];
                    if publish_bluesky { a.push(Audience::Bluesky); }
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
        let (user_did, _) = match state.keychain.load_session().ok().flatten() {
            Some(v) => v,
            None => return Err("Not authenticated with Bluesky".to_string()),
        };

        let access_jwt = state.get_fresh_access_jwt().await?;

        let candidates = authenticated_xrpc_candidates(&access_jwt, true);
        let pds_base = candidates.first().ok_or("Could not resolve Bluesky PDS")?;
        let client = BlueskyClient::with_jwt(&access_jwt);

        let mut images_embed = Vec::new();
        let mut video_embed = None;

        let sync_guard = state.sync_manager.lock().await;
        let sync = sync_guard.as_ref().ok_or("Sync manager not initialized")?;

        for (i, blob_hash_str) in media_blobs.iter().enumerate() {
            let mime = media_mimes.get(i).cloned().unwrap_or_else(|| "image/jpeg".to_string());
            
            let hash = match blob_hash::parse_blob_hash(blob_hash_str) {
                Ok(h) => h,
                Err(e) => {
                    println!("posting: invalid blob hash '{}': {}", blob_hash_str, e);
                    continue;
                }
            };

            if let Ok(data) = sync.get_media(hash).await {
                let upload_url = format!("{}/com.atproto.repo.uploadBlob", pds_base);
                if let Ok(resp) = client.post_blob(&upload_url, &mime, data.to_vec()).await {
                    if resp.status().is_success() {
                        if let Ok(upload_res) = resp.json::<serde_json::Value>().await {
                            if let Some(blob_ref) = upload_res.get("blob") {
                                if mime.starts_with("video/") {
                                    video_embed = Some(json!({
                                        "$type": "app.bsky.embed.video",
                                        "video": blob_ref
                                    }));
                                    // Normally video upload to Bluesky also wants a thumbnail, 
                                    // but we'll keep it simple for now as PDS might accept it without.
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
        drop(sync_guard);

        // Parse rich text facets (mentions → DID resolution, hashtags, links)
        let facets = richtext::parse_facets_for_bluesky(content, &access_jwt).await;

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
        }

        let create_url = format!("{}/com.atproto.repo.createRecord", pds_base);
        let resp = client.create_record(&create_url, json!({
            "repo": user_did,
            "collection": "app.bsky.feed.post",
            "record": record
        })).await?;

        if resp.status().is_success() {
            if let Ok(res) = resp.json::<serde_json::Value>().await {
                if let Some(uri) = res["uri"].as_str() {
                    result.bluesky = true;
                    result.bluesky_uri = Some(uri.to_string());

                    // Update local item if created
                    if let Some(id) = local_item_id {
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
        } else {
             result.errors.push(format!("Bluesky publish failed: {}", resp.status()));
        }
    }

    Ok(result)
}

// ── Autocomplete Commands ────────────────────────────────────────────────

#[derive(serde::Serialize, Clone)]
pub struct MentionSuggestion {
    pub handle: String,
    pub display_name: Option<String>,
    pub did: Option<String>,
    pub avatar_url: Option<String>,
}

/// Search for mention suggestions — queries local contacts + Bluesky follows.
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

/// Search for hashtag suggestions from followed hashtags.
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
