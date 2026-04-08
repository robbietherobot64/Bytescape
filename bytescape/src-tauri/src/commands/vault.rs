//! Vault File Management Commands
//!
//! Tauri IPC commands for the personal media vault — uploading, listing,
//! deleting, and retrieving files stored in Iroh Docs/Blobs, plus hashtag
//! CRUD, paginated post queries, contact merging, and vault-level admin
//! (clear, dedup, GC).
//!
//! # How It Works
//!
//! 1. **Upload** — file bytes arrive via IPC (`vault_upload_file`) or are read
//!    from disk paths (`vault_upload_files_from_paths`). Both delegate to
//!    `SyncManager::vault_add_file`, which stores the blob in Iroh and writes
//!    metadata to the vault Iroh Doc.
//! 2. **Read** — `vault_get_file_data` / `vault_get_thumbnail` resolve blob
//!    hashes from the vault doc and stream content back through IPC.
//! 3. **Query** — `list_vault_posts_paged` hits the redb `BytescapeItem` table
//!    with cursor-based pagination; `get_vault_stats` aggregates counts.
//! 4. **Contacts** — `list_contacts_unified` merges local DB contacts with
//!    Bluesky follows/followers into a single deduped list.
//! 5. **Admin** — `clear_vault` wipes both DB items and Iroh Doc entries;
//!    `vault_gc_tombstones` reclaims metadata from deleted entries.
//!
//! # Related Files
//!
//! - `crate::sync` — `SyncManager` owns the Iroh Doc/Blob operations
//! - `crate::db` — redb tables (`BytescapeItem`, `VaultMessage`, `Contact`)
//! - `crate::connectors::bluesky` — follow/follower fetching for contacts
//! - `crate::commands::peers` — P2P message sending (peer-side counterpart)
//!
//! # Tech Debt
//!
//! - **Two stats commands**: `vault_get_stats` (Iroh-level) and `get_vault_stats`
//!   (redb-level) return different shapes — unify into one.
//! - **Duplicated MIME detection**: `detect_mime_type` here duplicates logic in
//!   the import pipeline; extract to a shared utility.
//! - **Contacts list duplication**: `list_contacts` and `list_contacts_unified`
//!   share ~80 % of their logic; refactor into one with a format parameter.
//! - **Post-load pagination**: `list_vault_posts_paged` loads all rows then
//!   slices in Rust; push limit/offset into the redb query.

use bytes::Bytes;
use tauri::{State, Manager};

use crate::error::BytescapeError;
use crate::sync::{VaultFile, VaultFileStats, Hash, NamespaceId};
use crate::AppState;

// ═══════════════════════════════════════════════════════════════════════════════
// File Upload
// ═══════════════════════════════════════════════════════════════════════════════

/// Upload a file to the vault.
///
/// Stores the file content in Iroh Blobs and metadata in Iroh Docs.
/// For images, automatically generates a WebP thumbnail.
///
/// → IPC: `vault_upload_file` (frontend drag-and-drop / file picker)
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

/// Detect MIME type from file extension.
///
/// **Tech debt:** duplicates logic in the import pipeline (`connectors/facebook.rs`,
/// `connectors/instagram.rs`). Extract to a shared `utils::mime` module.
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

// ═══════════════════════════════════════════════════════════════════════════════
// Blob Management
// ═══════════════════════════════════════════════════════════════════════════════

/// List all files in the vault.
///
/// Returns files sorted by added_at (newest first).
///
/// → IPC: `vault_list_files` (frontend `VaultScreen`)
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
    
    // Stream blob to cache file, then read once (avoids Iroh double-buffer
    // where get_media holds entire blob in Bytes + .to_vec() creates a copy).
    let app_data_dir = state.app_handle.path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    let cache_dir = app_data_dir.join("blob_cache");
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| e.to_string())?;

    let header = sync.get_media_header(hash, 12).await
        .map_err(|e| e.to_string())?;
    let ext = super::media::detect_extension_from_bytes(&header);
    let cache_path = cache_dir.join(format!("{}.{}", &file.blob_hash, ext));
    if !cache_path.exists() {
        sync.export_media_to_file(hash, &cache_path).await
            .map_err(|e| e.to_string())?;
    }
    let data = std::fs::read(&cache_path)
        .map_err(|e| e.to_string())?;
    
    Ok(data)
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
            // Stream thumbnail to cache file, then read from disk.
            // Thumbnails are small (≤50 KB) so this is mostly for consistency
            // with the zero-heap-allocation pattern established in P0.1.
            let app_data_dir = state.app_handle.path().app_data_dir()
                .map_err(|e| BytescapeError::FileSystem(format!("app_data_dir: {}", e)))?;
            let cache_dir = app_data_dir.join("blob_cache");
            let cache_path = cache_dir.join(format!("{}.thumb.webp", thumb_str));
            if !cache_path.exists() {
                let _ = std::fs::create_dir_all(&cache_dir);
                sync.export_media_to_file(hash, &cache_path).await
                    .map_err(|e| BytescapeError::BlobStorage(e))?;
            }
            let bytes = std::fs::read(&cache_path)
                .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
            Ok(Some(bytes))
        }
        None => Ok(None),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Messages
// ═══════════════════════════════════════════════════════════════════════════════

/// Send a message to a P2P sync document.
///
/// Stores the message content in Iroh Blobs and creates a doc entry.
///
/// → IPC: `send_p2p_message` (frontend `ChatScreen`)
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

// ═══════════════════════════════════════════════════════════════════════════════
// Vault Stats
// ═══════════════════════════════════════════════════════════════════════════════

/// Get vault statistics from the redb database.
///
/// Returns item count, total size, and per-source breakdowns.
/// This reads from the redb `BytescapeItem` table, **not** the Iroh Doc
/// (`vault_get_stats` above is the Iroh-level counterpart).
///
/// **Tech debt:** two stats commands return different shapes — unify into one.
///
/// → IPC: `get_vault_stats` (frontend `VaultScreen` stats panel)
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

/// List unique source identifiers present in the vault.
///
/// Returns e.g. `["bluesky", "facebook", "upload"]` for the filter dropdown.
///
/// → IPC: `list_vault_sources` (frontend source-filter chips)
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

// ═══════════════════════════════════════════════════════════════════════════════
// Paginated Posts
// ═══════════════════════════════════════════════════════════════════════════════

/// List vault posts with cursor-based pagination.
///
/// Supports three query strategies (tried in priority order):
/// 1. **path_prefix** — Willow-style path reads (`/bluesky/...`)
/// 2. **source** — legacy source-filter, auto-migrated to path-prefix when
///    rows exist under the `/{source}/` prefix
/// 3. **unfiltered** — all items for the current owner DID
///
/// Returns `{ items: [...], total: N }` as a JSON value.
///
/// **Tech debt:** pagination is applied after the full query; push
/// limit/offset into the redb read transaction for large vaults.
///
/// → IPC: `list_vault_posts_paged` (frontend `FeedScreen`, `VaultScreen`)
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

// ═══════════════════════════════════════════════════════════════════════════════
// Hashtags
// ═══════════════════════════════════════════════════════════════════════════════

/// Save a followed hashtag to the local database.
///
/// → IPC: `save_hashtag` (frontend hashtag manager)
#[tauri::command]
pub async fn save_hashtag(state: State<'_, AppState>, tag: String) -> std::result::Result<(), String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;
    
    db.save_hashtag(tag).await
        .map_err(|e| e.to_string())
}

/// Delete a followed hashtag from the local database.
///
/// → IPC: `delete_hashtag` (frontend hashtag manager)
#[tauri::command]
pub async fn delete_hashtag(state: State<'_, AppState>, tag: String) -> std::result::Result<(), String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;
    
    db.delete_hashtag(tag).await
        .map_err(|e| e.to_string())
}

/// List all followed hashtags from the local database.
///
/// → IPC: `list_followed_hashtags` (frontend hashtag sidebar)
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

// ═══════════════════════════════════════════════════════════════════════════════
// Vault Admin
// ═══════════════════════════════════════════════════════════════════════════════

/// Clear all items from the vault (destructive).
///
/// Performs a two-phase wipe:
/// 1. `db.clear_all_user_data()` — removes all `BytescapeItem`, `VaultMessage`,
///    and `Contact` rows from redb.
/// 2. `sync.vault_clear_all_files()` — tombstones every entry in the Iroh Doc
///    so the deletion propagates to synced peers.
///
/// Returns the total number of items + files removed.
///
/// → IPC: `clear_vault` (frontend Settings → Danger Zone)
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

/// List vault messages.
///
/// Uses `all_owner_dids()` so messages stamped under any of the user's
/// historical DIDs (Bluesky did:plc, web_did, Mastodon acct: URI, did:key)
/// are all returned. This prevents the "messages disappear" bug that
/// occurs when the user's primary DID changes (e.g. after setting a
/// Bytescape username or linking a new account).
#[tauri::command]
pub async fn list_vault_messages(state: State<'_, AppState>) -> std::result::Result<Vec<crate::db::VaultMessage>, String> {
    let all_dids = state.keychain.all_owner_dids()
        .map_err(|e| e.to_string())?;

    if all_dids.is_empty() {
        return Err(BytescapeError::Unauthenticated.to_string());
    }

    println!("list_vault_messages: filtering by {} DIDs: {:?}", all_dids.len(), all_dids);

    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;
    
    let msgs = db.list_vault_messages_multi(all_dids.clone()).await
        .map_err(|e| e.to_string())?;
    
    println!("list_vault_messages: found {} messages across {} DIDs", msgs.len(), all_dids.len());
    
    Ok(msgs)
}

/// Legacy archive post count used by the fallback UI.
///
/// Counts rows in the `BytescapeItem` table that originated from an archive
/// import (Facebook, Instagram). Kept for backwards compatibility; prefer
/// `get_vault_stats` for new code.
///
/// → IPC: `count_archive_posts` (frontend import summary)
#[tauri::command]
pub async fn count_archive_posts(state: State<'_, AppState>) -> std::result::Result<u64, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;

    db.count_archive_posts().await
        .map_err(|e| e.to_string())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Contacts
// ═══════════════════════════════════════════════════════════════════════════════

/// List contacts — merges DB contacts with Bluesky follows so the chat
/// friends panel shows everyone the user can potentially chat with,
/// not just people they've already chatted with.
///
/// **Tech debt:** shares ~80 % of its logic with `list_contacts_unified`;
/// refactor into one function with a format parameter.
///
/// → IPC: `list_contacts` (frontend `ChatScreen` sidebar)
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
///
/// **Tech debt:** shares ~80 % of logic with `list_contacts`; unify.
///
/// → IPC: `list_contacts_unified` (frontend `ContactsScreen`)
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
///
/// → IPC: `vault_gc_tombstones` (frontend Settings → Storage → "Clean up")
#[tauri::command]
pub async fn vault_gc_tombstones(state: State<'_, AppState>) -> std::result::Result<usize, String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;

    sync.vault_gc_tombstones().await
        .map_err(|e| e.to_string())
}

/// Delete a single vault item by its UUID.
///
/// Removes the `BytescapeItem` from the redb items table. Works for any source
/// (Bluesky, Facebook, Instagram, local). This is a local-only deletion — it
/// does not remove the post from the source platform.
///
/// Returns `true` if the item was found and deleted, `false` if not found.
///
/// → IPC: `delete_vault_item` (frontend Profile → Post delete)
#[tauri::command]
pub async fn delete_vault_item(
    item_id: String,
    state: State<'_, AppState>,
) -> std::result::Result<bool, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;

    db.delete_item_by_id(&item_id).await
}
