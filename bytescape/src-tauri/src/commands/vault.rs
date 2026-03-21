//! Vault File Management Commands
//! 
//! Commands for managing the personal media vault - uploading, listing,
//! and retrieving files stored in Iroh Docs/Blobs.

use bytes::Bytes;
use tauri::State;

use crate::error::BytescapeError;
use crate::sync::{VaultFile, VaultFileStats, Hash, NamespaceId};
use crate::AppState;

/// Upload a file to the vault.
/// 
/// Stores the file content in Iroh Blobs and metadata in Iroh Docs.
/// For images, automatically generates a WebP thumbnail.
#[tauri::command]
pub async fn vault_upload_file(
    name: String,
    mime_type: String,
    data: Vec<u8>,
    source: String,
    item_id: Option<String>,
    created_at: Option<i64>,
    tags: Vec<String>,
    state: State<'_, AppState>,
) -> std::result::Result<VaultFile, String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;
    
    sync.vault_add_file(
        name,
        mime_type,
        Bytes::from(data),
        source,
        item_id,
        created_at,
        tags
    ).await
    .map_err(|e| e.to_string())
}

/// Upload multiple files to the vault by reading from disk paths.
/// 
/// This avoids serializing large files through the Tauri IPC bridge.
/// Files are read on the Rust side and stored directly.
#[tauri::command]
pub async fn vault_upload_files_from_paths(
    paths: Vec<String>,
    tags: Vec<String>,
    state: State<'_, AppState>,
) -> std::result::Result<Vec<VaultFile>, String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;

    let mut results = Vec::new();
    for path_str in &paths {
        let path = std::path::Path::new(path_str);
        let name = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unnamed".to_string());

        // Detect MIME type from extension
        let mime_type = detect_mime_type(path);

        // Read file on Rust side — avoids serializing through IPC
        let data = tokio::fs::read(path).await
            .map_err(|e| BytescapeError::FileSystem(format!("Failed to read '{}': {}", path_str, e)))?;

        // Get file metadata for created_at timestamp
        let created_at = tokio::fs::metadata(path).await
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);

        match sync.vault_add_file(
            name.clone(),
            mime_type,
            Bytes::from(data),
            "upload".to_string(),
            None,
            created_at,
            tags.clone(),
        ).await {
            Ok(file) => results.push(file),
            Err(e) => eprintln!("Failed to upload '{}': {}", name, e),
        }
    }

    Ok(results)
}

/// Detect MIME type from file extension
fn detect_mime_type(path: &std::path::Path) -> String {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()) {
        Some(ext) if ext == "jpg" || ext == "jpeg" => "image/jpeg".to_string(),
        Some(ext) if ext == "png" => "image/png".to_string(),
        Some(ext) if ext == "webp" => "image/webp".to_string(),
        Some(ext) if ext == "gif" => "image/gif".to_string(),
        Some(ext) if ext == "svg" => "image/svg+xml".to_string(),
        Some(ext) if ext == "bmp" => "image/bmp".to_string(),
        Some(ext) if ext == "mp4" => "video/mp4".to_string(),
        Some(ext) if ext == "mov" => "video/quicktime".to_string(),
        Some(ext) if ext == "avi" => "video/x-msvideo".to_string(),
        Some(ext) if ext == "mkv" => "video/x-matroska".to_string(),
        Some(ext) if ext == "mp3" => "audio/mpeg".to_string(),
        Some(ext) if ext == "wav" => "audio/wav".to_string(),
        Some(ext) if ext == "ogg" => "audio/ogg".to_string(),
        Some(ext) if ext == "flac" => "audio/flac".to_string(),
        Some(ext) if ext == "pdf" => "application/pdf".to_string(),
        Some(ext) if ext == "txt" => "text/plain".to_string(),
        Some(ext) if ext == "json" => "application/json".to_string(),
        Some(ext) if ext == "zip" => "application/zip".to_string(),
        Some(ext) if ext == "tar" => "application/x-tar".to_string(),
        Some(ext) if ext == "gz" || ext == "gzip" => "application/gzip".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

/// List all files in the vault.
/// 
/// Returns files sorted by added_at (newest first).
#[tauri::command]
pub async fn vault_list_files(state: State<'_, AppState>) -> std::result::Result<Vec<VaultFile>, String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;
    sync.vault_list_files().await
        .map_err(|e| e.to_string())
}

/// Get metadata for a single vault file by ID.
#[tauri::command]
pub async fn vault_get_file(file_id: String, state: State<'_, AppState>) -> std::result::Result<Option<VaultFile>, String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;
    sync.vault_get_file(&file_id).await
        .map_err(|e| e.to_string())
}

/// Delete a file from the vault.
/// 
/// Writes a tombstone marker to the Iroh Doc entry.
#[tauri::command]
pub async fn vault_delete_file(file_id: String, state: State<'_, AppState>) -> std::result::Result<(), String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;
    sync.vault_delete_file(&file_id).await
        .map_err(|e| e.to_string())
}

/// Get aggregate statistics about the vault.
/// 
/// Returns total file count, total size, and breakdowns by kind and source.
#[tauri::command]
pub async fn vault_get_stats(state: State<'_, AppState>) -> std::result::Result<VaultFileStats, String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;
    sync.vault_stats().await
        .map_err(|e| e.to_string())
}

/// Deduplicate vault metadata entries by blob hash.
/// Keeps the newest entry for identical content and removes older duplicates.
#[tauri::command]
pub async fn vault_deduplicate_files(state: State<'_, AppState>) -> std::result::Result<usize, String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;

    sync.vault_deduplicate_files().await
        .map_err(|e| e.to_string())
}

/// Get the raw content bytes for a vault file.
/// 
/// Fetches the file from Iroh Blobs using the stored hash.
#[tauri::command]
pub async fn vault_get_file_data(file_id: String, state: State<'_, AppState>) -> std::result::Result<Vec<u8>, String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;
    
    let file = sync.vault_get_file(&file_id).await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| BytescapeError::FileNotFound(file_id.clone()))?;
    
    let hash: Hash = match file.blob_hash.parse() {
        Ok(h) => h,
        Err(e) => return Err(e.to_string()),
    };
    
    let bytes = sync.get_media(hash).await
        .map_err(|e| e.to_string())?;
    
    Ok(bytes.to_vec())
}

/// Get the thumbnail bytes for a vault file.
/// 
/// Returns None if the file has no thumbnail (non-images or thumbnail generation failed).
#[tauri::command]
pub async fn vault_get_thumbnail(file_id: String, state: State<'_, AppState>) -> std::result::Result<Option<Vec<u8>>, String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;
    
    let file = sync.vault_get_file(&file_id).await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| BytescapeError::FileNotFound(file_id.clone()))?;
    
    match file.thumbnail_hash {
        Some(thumb_str) => {
            let hash: Hash = match thumb_str.parse() {
                Ok(h) => h,
                Err(e) => return Err(e.to_string()),
            };
            let bytes = sync.get_media(hash).await
                .map_err(|e: String| e)?;
            Ok(Some(bytes.to_vec()))
        }
        None => Ok(None),
    }
}

/// Send a message to a P2P sync document.
/// 
/// Stores the message content in Iroh Blobs and creates a doc entry.
#[tauri::command]
pub async fn send_p2p_message(
    namespace_id: String,
    content: String,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;

    let namespace: NamespaceId = match namespace_id.parse() {
        Ok(n) => n,
        Err(e) => return Err(e.to_string()),
    };

    sync.send_message(namespace, content).await
        .map_err(|e| e.to_string())
}

/// Get vault statistics (alias for vault_get_stats for frontend compatibility)
#[tauri::command]
pub async fn get_vault_stats(state: State<'_, AppState>) -> std::result::Result<crate::db::VaultStats, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;
    
    let owner_did = state.keychain.get_did()
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "unknown".to_string());
    
    db.vault_stats(&owner_did).await
        .map_err(|e| e.to_string())
}

/// List unique sources in the vault
#[tauri::command]
pub async fn list_vault_sources(state: State<'_, AppState>) -> std::result::Result<Vec<String>, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;
    
    let owner_did = state.keychain.get_did()
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "unknown".to_string());
    
    db.list_vault_sources(&owner_did).await
        .map_err(|e| e.to_string())
}

/// List vault posts with pagination
#[tauri::command]
pub async fn list_vault_posts_paged(
    state: State<'_, AppState>,
    limit: usize,
    offset: usize,
    source: Option<String>,
    path_prefix: Option<String>,
) -> std::result::Result<serde_json::Value, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;
    
    let owner_did = state.keychain.get_did()
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "unknown".to_string());

    let canary_path_only = std::env::var("BYTESCAPE_WILLOW_PATH_ONLY_READS")
        .ok()
        .map(|v| {
            let normalized = v.trim().to_ascii_lowercase();
            normalized == "1" || normalized == "true" || normalized == "yes" || normalized == "on"
        })
        .unwrap_or(false);
    
    let (items, total) = if let Some(prefix) = path_prefix.as_deref() {
        db.list_my_items_by_path_prefix_paged(&owner_did, prefix, limit, offset).await
            .map_err(|e| e.to_string())?
    } else if let Some(source_name) = source.as_deref() {
        // Migration bridge: prefer path-prefix reads, then fall back to source-filter reads.
        let source_prefix = format!("/{}/", source_name.to_lowercase());
        let by_path = db.list_my_items_by_path_prefix_paged(&owner_did, &source_prefix, limit, offset).await
            .map_err(|e| e.to_string())?;

        if canary_path_only {
            by_path
        } else if by_path.1 == 0 {
            db.list_my_items_paged(&owner_did, Some(source_name), limit, offset).await
                .map_err(|e| e.to_string())?
        } else {
            by_path
        }
    } else {
        db.list_my_items_paged(&owner_did, None, limit, offset).await
            .map_err(|e| e.to_string())?
    };
    
    Ok(serde_json::json!({
        "items": items,
        "total": total
    }))
}

/// Save a followed hashtag
#[tauri::command]
pub async fn save_hashtag(state: State<'_, AppState>, tag: String) -> std::result::Result<(), String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;
    
    db.save_hashtag(tag).await
        .map_err(|e| e.to_string())
}

/// Delete a followed hashtag
#[tauri::command]
pub async fn delete_hashtag(state: State<'_, AppState>, tag: String) -> std::result::Result<(), String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;
    
    db.delete_hashtag(tag).await
        .map_err(|e| e.to_string())
}

/// List followed hashtags
#[tauri::command]
pub async fn list_followed_hashtags(state: State<'_, AppState>) -> std::result::Result<Vec<String>, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;
    
    db.list_followed_hashtags().await
        .map_err(|e| e.to_string())
}

/// Alias for list_followed_hashtags to support existing frontend calls.
#[tauri::command]
pub async fn list_hashtags(state: State<'_, AppState>) -> std::result::Result<Vec<String>, String> {
    list_followed_hashtags(state).await
}

/// Add a followed hashtag and return the refreshed list.
///
/// The frontend calls this (not `save_hashtag`) because it expects
/// the updated list back immediately.
#[tauri::command]
pub async fn add_followed_hashtag(state: State<'_, AppState>, hashtag: String) -> std::result::Result<Vec<String>, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;

    db.save_hashtag(hashtag).await
        .map_err(|e| e.to_string())?;

    db.list_followed_hashtags().await
        .map_err(|e| e.to_string())
}

/// Remove a followed hashtag and return the refreshed list.
#[tauri::command]
pub async fn remove_followed_hashtag(state: State<'_, AppState>, hashtag: String) -> std::result::Result<Vec<String>, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;

    db.delete_hashtag(hashtag).await
        .map_err(|e| e.to_string())?;

    db.list_followed_hashtags().await
        .map_err(|e| e.to_string())
}

/// Clear all items from the vault
#[tauri::command]
pub async fn clear_vault(state: State<'_, AppState>) -> std::result::Result<usize, String> {
    let cleared_db = {
        let db_guard = state.db_manager.read().await;
        let db = db_guard.as_ref()
            .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;

        db.clear_all_user_data().await
            .map_err(|e| e.to_string())?
    };

    let mut cleared_files = 0usize;
    {
        let sync_guard = state.sync_manager.lock().await;
        if let Some(sync) = sync_guard.as_ref() {
            cleared_files = sync.vault_clear_all_files().await
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(cleared_db + cleared_files)
}

/// List vault messages
#[tauri::command]
pub async fn list_vault_messages(state: State<'_, AppState>) -> std::result::Result<Vec<crate::db::VaultMessage>, String> {
    let owner_did = state.keychain.get_owner_did()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| BytescapeError::Unauthenticated)?;

    println!("list_vault_messages: filtering by owner_did={}", owner_did);

    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;
    
    let msgs = db.list_vault_messages(Some(&owner_did)).await
        .map_err(|e| e.to_string())?;
    
    println!("list_vault_messages: found {} messages for owner {}", msgs.len(), owner_did);
    
    // If zero messages but there ARE messages in the DB for other owners, log it
    if msgs.is_empty() {
        let all_msgs = db.list_vault_messages(None).await.unwrap_or_default();
        if !all_msgs.is_empty() {
            let sample_owners: Vec<String> = all_msgs.iter()
                .take(5)
                .filter_map(|m| m.owner_did.clone())
                .collect();
            eprintln!(
                "list_vault_messages: WARNING — {} messages exist in DB but NONE match owner_did={}. Sample stored owner_dids: {:?}",
                all_msgs.len(), owner_did, sample_owners
            );
        }
    }
    
    Ok(msgs)
}

/// Legacy archive post count used by fallback UI.
#[tauri::command]
pub async fn count_archive_posts(state: State<'_, AppState>) -> std::result::Result<u64, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;

    db.count_archive_posts().await
        .map_err(|e| e.to_string())
}

/// List contacts — merges DB contacts with Bluesky follows so the chat
/// friends panel shows everyone the user can potentially chat with,
/// not just people they've already chatted with.
#[tauri::command]
pub async fn list_contacts(state: State<'_, AppState>) -> std::result::Result<Vec<crate::db::Contact>, String> {
    let mut contacts: std::collections::HashMap<String, crate::db::Contact> = std::collections::HashMap::new();

    // ── 1. DB contacts (existing chat participants) ──
    {
        let db_guard = state.db_manager.read().await;
        if let Some(db) = db_guard.as_ref() {
            if let Ok(db_contacts) = db.list_contacts().await {
                for c in db_contacts {
                    contacts.entry(c.did.clone()).or_insert(c);
                }
            }
        }
    }

    // ── 2. Bluesky follows ──
    {
        let actor_did = state.keychain.get_did()
            .ok()
            .flatten()
            .unwrap_or_default();

        if !actor_did.is_empty() {
            let access_jwt = if state.is_oauth_session() {
                String::new()
            } else {
                state.get_fresh_access_jwt().await.unwrap_or_default()
            };

            let mut cursor: Option<String> = None;
            for _ in 0..10 {
                match crate::connectors::bluesky::get_follows(
                    &access_jwt, &actor_did, 100, cursor.clone()
                ).await {
                    Ok((profiles, next_cursor)) => {
                        for p in profiles {
                            contacts.entry(p.did.clone()).or_insert_with(|| crate::db::Contact {
                                did: p.did,
                                handle: p.handle,
                                p2p_capable: false,
                                node_id: None,
                                last_polled_at: 0,
                            });
                        }
                        cursor = next_cursor;
                        if cursor.is_none() { break; }
                    }
                    Err(_) => break,
                }
            }
        }
    }

    // ── 3. Sort: P2P-capable first, then alphabetically by handle ──
    let mut result: Vec<crate::db::Contact> = contacts.into_values().collect();
    result.sort_by(|a, b| {
        b.p2p_capable.cmp(&a.p2p_capable)
            .then_with(|| a.handle.to_lowercase().cmp(&b.handle.to_lowercase()))
    });

    Ok(result)
}

/// Unified contact list — merges DB contacts with Bluesky follows AND followers.
///
/// Returns a combined, deduplicated contact list sorted so P2P-capable
/// contacts appear first, then alphabetically by display name / handle.
#[tauri::command]
pub async fn list_contacts_unified(
    state: State<'_, AppState>,
) -> std::result::Result<Vec<serde_json::Value>, String> {
    let mut contacts: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();

    // ── 1. DB contacts (users we've chatted with) ──
    {
        let db_guard = state.db_manager.read().await;
        if let Some(db) = db_guard.as_ref() {
            if let Ok(db_contacts) = db.list_contacts().await {
                for c in db_contacts {
                    let key = c.did.clone();
                    contacts.entry(key).or_insert_with(|| serde_json::json!({
                        "did": c.did,
                        "handle": c.handle,
                        "display_name": null,
                        "avatar": null,
                        "source": "chat",
                        "p2p_capable": c.p2p_capable,
                    }));
                }
            }
        }
    }

    // ── 2. Bluesky follows + followers (paginated) ──
    {
        let actor_did = state.keychain.get_did()
            .map_err(|e| e.to_string())?
            .unwrap_or_default();

        if !actor_did.is_empty() {
            let access_jwt = if state.is_oauth_session() {
                String::new()
            } else {
                state.get_fresh_access_jwt().await.unwrap_or_default()
            };

            // Follows — paginate up to 1000
            let mut cursor: Option<String> = None;
            for _ in 0..10 {
                match crate::connectors::bluesky::get_follows(
                    &access_jwt, &actor_did, 100, cursor.clone()
                ).await {
                    Ok((profiles, next_cursor)) => {
                        for p in profiles {
                            contacts.entry(p.did.clone()).or_insert_with(|| serde_json::json!({
                                "did": p.did,
                                "handle": p.handle,
                                "display_name": p.display_name,
                                "avatar": p.avatar,
                                "source": "bluesky",
                                "p2p_capable": false,
                            }));
                        }
                        cursor = next_cursor;
                        if cursor.is_none() { break; }
                    }
                    Err(_) => break,
                }
            }

            // Followers — fetch via chat_client
            match state.chat_client.get_followers(&actor_did).await {
                Ok(members) => {
                    for m in members {
                        contacts.entry(m.did.clone()).or_insert_with(|| serde_json::json!({
                            "did": m.did,
                            "handle": m.handle.unwrap_or_default(),
                            "display_name": m.display_name,
                            "avatar": null,
                            "source": "bluesky",
                            "p2p_capable": false,
                        }));
                    }
                }
                Err(_) => {} // Silently skip if followers fetch fails
            }
        }
    }

    // ── 3. Sort: P2P-capable first, then alphabetically ──
    let mut result: Vec<serde_json::Value> = contacts.into_values().collect();
    result.sort_by(|a, b| {
        let a_p2p = a.get("p2p_capable").and_then(|v| v.as_bool()).unwrap_or(false);
        let b_p2p = b.get("p2p_capable").and_then(|v| v.as_bool()).unwrap_or(false);
        b_p2p.cmp(&a_p2p).then_with(|| {
            let a_name = a.get("display_name").and_then(|v| v.as_str())
                .or_else(|| a.get("handle").and_then(|v| v.as_str()))
                .unwrap_or("");
            let b_name = b.get("display_name").and_then(|v| v.as_str())
                .or_else(|| b.get("handle").and_then(|v| v.as_str()))
                .unwrap_or("");
            a_name.to_lowercase().cmp(&b_name.to_lowercase())
        })
    });

    Ok(result)
}

/// Garbage-collect orphaned tombstone entries from the Iroh vault namespace.
/// 
/// Tombstones accumulate as files are deleted. This reclaims namespace metadata weight.
/// Returns the number of tombstone entries purged.
#[tauri::command]
pub async fn vault_gc_tombstones(state: State<'_, AppState>) -> std::result::Result<usize, String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;

    sync.vault_gc_tombstones().await
        .map_err(|e| e.to_string())
}
