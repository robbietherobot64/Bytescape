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
use serde::Serialize;
use tauri::{State, Manager};

use crate::error::BytescapeError;
use crate::sync::{VaultFile, VaultFileStats, Hash, NamespaceId};
use crate::AppState;

/// Provenance metadata for a single media blob — returned by `get_media_provenance`.
#[derive(Debug, Serialize)]
pub struct MediaProvenance {
    pub item_id: String,
    pub source: String,
    pub timestamp: i64,
    pub content_snippet: String,
    pub original_url: Option<String>,
    pub author_handle: Option<String>,
    pub author_avatar: Option<String>,
    pub title: Option<String>,
}

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

/// Retrieve a single vault item (BytescapeItem) by its ID.
#[tauri::command]
pub async fn vault_get_item(
    item_id: String,
    state: State<'_, AppState>,
) -> std::result::Result<Option<crate::models::BytescapeItem>, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    db.get_item_by_id(&item_id).await
}

/// Media provenance lookup — find the vault item that contains a given blob hash.
///
/// Used by the profile lightbox to show context for a selected media item:
/// source platform, original timestamp, post content snippet, and original URL.
///
/// # IPC
/// Frontend: `invoke<MediaProvenance | null>('get_media_provenance', { blobHash })`
#[tauri::command]
pub async fn get_media_provenance(
    blob_hash: String,
    state: State<'_, AppState>,
) -> std::result::Result<Option<MediaProvenance>, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let item = db.find_item_by_blob_hash(&blob_hash).await?;
    Ok(item.map(|i| MediaProvenance {
        item_id: i.id,
        source: i.source,
        timestamp: i.original_timestamp,
        content_snippet: i.content.chars().take(200).collect(),
        original_url: i.url,
        author_handle: i.author_handle,
        author_avatar: i.author_avatar,
        title: i.title,
    }))
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

/// Remove vault file entries that belong to reposted items.
///
/// Vault files linked to BytescapeItems where `reposted_by_did` is set are
/// tombstoned so they no longer appear in the vault gallery. The underlying
/// blob data remains in the Iroh store for feed display.
///
/// Remove vault media linked to reposts or replied-to posts.
///
/// Keeps all archive imports (Facebook, Instagram, etc.) and the user's own
/// posts on any platform. Only removes media from items that are reposts
/// (`reposted_by_did` is set) or replies to other posts (`reply_parent_uri`
/// is set).
///
/// Should be called once when the media gallery first loads.
///
/// → IPC: `vault_cleanup_repost_media`
#[tauri::command]
pub async fn vault_cleanup_repost_media(state: State<'_, AppState>) -> std::result::Result<usize, String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;

    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;

    let files = sync.vault_list_files().await
        .map_err(|e| e.to_string())?;

    // Collect all unique item_ids to batch-lookup instead of N+1 queries
    let item_ids: Vec<String> = files
        .iter()
        .filter_map(|f| f.item_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    // Build a set of item IDs that are reposts or replies — their media
    // shouldn't appear in the vault gallery.
    let mut excluded_item_ids = std::collections::HashSet::new();
    for item_id in &item_ids {
        if let Ok(Some(item)) = db.get_item_by_id(item_id).await {
            if item.reposted_by_did.is_some() || item.reply_parent_uri.is_some() {
                excluded_item_ids.insert(item_id.clone());
            }
        }
    }

    println!(
        "vault_cleanup_repost_media: scanned {} vault files, {} unique item_ids, {} excluded (reposts/replies)",
        files.len(),
        item_ids.len(),
        excluded_item_ids.len(),
    );

    let mut removed = 0usize;
    for file in &files {
        if let Some(ref item_id) = file.item_id {
            if excluded_item_ids.contains(item_id) {
                println!(
                    "vault_cleanup_repost_media: tombstoning file id={} name='{}' source='{}' item_id={}",
                    file.id, file.name, file.source, item_id,
                );
                if let Err(e) = sync.vault_delete_file(&file.id).await {
                    eprintln!("vault_cleanup_repost_media: failed to delete {}: {}", file.id, e);
                } else {
                    removed += 1;
                }
            }
        }
    }

    println!("vault_cleanup_repost_media: removed {} repost/reply media entries from vault", removed);
    Ok(removed)
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
/// Capped at 50 MB to prevent massive IPC payloads — use the media server
/// URL for larger files (videos, large archives).
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
    // Enforce IPC size cap — files over 50 MB should use media server URL instead.
    let metadata = std::fs::metadata(&cache_path).map_err(|e| e.to_string())?;
    if metadata.len() > 50 * 1024 * 1024 {
        return Err(format!(
            "File too large for IPC transfer ({:.1} MB). Use the media server URL instead.",
            metadata.len() as f64 / (1024.0 * 1024.0)
        ));
    }
    
    let data = std::fs::read(&cache_path)
        .map_err(|e| e.to_string())?;
    
    Ok(data)
}

/// Get the thumbnail bytes for a vault file.
/// 
/// Returns None if the file has no thumbnail (non-images or thumbnail generation failed).
/// For images that lack a persisted thumbnail, generates one on demand and caches it.
#[tauri::command]
pub async fn vault_get_thumbnail(file_id: String, state: State<'_, AppState>) -> std::result::Result<Option<Vec<u8>>, String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;
    
    let mut file = sync.vault_get_file(&file_id).await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| BytescapeError::FileNotFound(file_id.clone()))?;
    
    // If no thumbnail exists for an image, generate one on demand
    if file.thumbnail_hash.is_none() {
        if file.kind == crate::sync::VaultFileKind::Image {
            let _ = sync.generate_thumbnail_for_vault_file(&mut file).await;
        }
    }

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
    
    let owner_dids = state.keychain.all_owner_dids()
        .map_err(|e| e.to_string())?;
    
    db.vault_stats(&owner_dids).await
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
    
    let owner_dids = state.keychain.all_owner_dids()
        .map_err(|e| e.to_string())?;
    
    db.list_vault_sources(&owner_dids).await
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
    
    let owner_dids = state.keychain.all_owner_dids()
        .map_err(|e| e.to_string())?;

    let canary_path_only = std::env::var("BYTESCAPE_WILLOW_PATH_ONLY_READS")
        .ok()
        .map(|v| {
            let normalized = v.trim().to_ascii_lowercase();
            normalized == "1" || normalized == "true" || normalized == "yes" || normalized == "on"
        })
        .unwrap_or(false);
    
    let (items, total) = if let Some(prefix) = path_prefix.as_deref() {
        db.list_my_items_by_path_prefix_paged(&owner_dids, prefix, limit, offset).await
            .map_err(|e| e.to_string())?
    } else if let Some(source_name) = source.as_deref() {
        // Migration bridge: prefer path-prefix reads, then fall back to source-filter reads.
        let source_prefix = format!("/{}/", source_name.to_lowercase());
        let by_path = db.list_my_items_by_path_prefix_paged(&owner_dids, &source_prefix, limit, offset).await
            .map_err(|e| e.to_string())?;

        if canary_path_only {
            by_path
        } else if by_path.1 == 0 {
            db.list_my_items_paged(&owner_dids, Some(source_name), limit, offset).await
                .map_err(|e| e.to_string())?
        } else {
            by_path
        }
    } else {
        db.list_my_items_paged(&owner_dids, None, limit, offset).await
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

    // ── 3. Mastodon follows ──
    if let Ok(session) = crate::connectors::mastodon::get_session_from_keychain(&state.keychain) {
        if let Ok(accounts) = crate::connectors::mastodon::get_following_list(&state.http_client, &session, None).await {
            let instance = &session.instance;
            for a in &accounts {
                let pv = crate::connectors::mastodon::account_to_profile_view(instance, a);
                contacts.entry(pv.did.clone()).or_insert_with(|| crate::db::Contact {
                    did: pv.did,
                    handle: pv.handle,
                    p2p_capable: false,
                    node_id: None,
                    last_polled_at: 0,
                });
            }
        }
    }

    // ── 4. Sort: P2P-capable first, then alphabetically by handle ──
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

    // ── 3. Mastodon following + followers ──
    if let Ok(session) = crate::connectors::mastodon::get_session_from_keychain(&state.keychain) {
        // Following
        if let Ok(accounts) = crate::connectors::mastodon::get_following_list(&state.http_client, &session, None).await {
            let instance = &session.instance;
            for a in &accounts {
                let pv = crate::connectors::mastodon::account_to_profile_view(instance, a);
                contacts.entry(pv.did.clone()).or_insert_with(|| serde_json::json!({
                    "did": pv.did,
                    "handle": pv.handle,
                    "display_name": pv.display_name,
                    "avatar": pv.avatar,
                    "source": "mastodon",
                    "p2p_capable": false,
                }));
            }
        }
        // Followers
        if let Ok(accounts) = crate::connectors::mastodon::get_followers_list(&state.http_client, &session, None).await {
            let instance = &session.instance;
            for a in &accounts {
                let pv = crate::connectors::mastodon::account_to_profile_view(instance, a);
                contacts.entry(pv.did.clone()).or_insert_with(|| serde_json::json!({
                    "did": pv.did,
                    "handle": pv.handle,
                    "display_name": pv.display_name,
                    "avatar": pv.avatar,
                    "source": "mastodon",
                    "p2p_capable": false,
                }));
            }
        }
    }

    // ── 4. Sort: P2P-capable first, then alphabetically ──
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

// ═══════════════════════════════════════════════════════════════════════════════
// Reply Parent Resolution
// ═══════════════════════════════════════════════════════════════════════════════

/// Lightweight parent post context for reply display.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ReplyParentContext {
    /// Platform URI of the parent post
    pub uri: String,
    /// Text content of the parent post
    pub content: String,
    /// Author handle (e.g. "@alice.bsky.social", "@alice@mastodon.social")
    pub author_handle: String,
    /// Author avatar URL
    pub author_avatar: Option<String>,
    /// Author DID or identifier
    pub author_did: Option<String>,
    /// Original post timestamp (unix seconds)
    pub created_at: i64,
    /// Image URLs from the parent post (if any)
    pub image_urls: Vec<String>,
    /// Whether this was resolved from the local vault vs network
    pub from_vault: bool,
}

/// Resolve the parent post of a reply, returning its content and author info.
///
/// Tries the local vault first (by matching `original_id` against the URI).
/// If not found locally, fetches from the appropriate platform API:
/// - Bluesky: `app.bsky.feed.getPosts` (public API, no auth)
/// - Mastodon: `GET /api/v1/statuses/{id}` (public endpoint)
///
/// # Parameters
/// - `uri` — The `reply_parent_uri` from the reply item.
///
/// # Returns
/// `ReplyParentContext` with the parent post's content, author, and metadata.
///
/// → IPC: `resolve_reply_parent` (frontend profile → reply context display)
#[tauri::command]
pub async fn resolve_reply_parent(
    uri: String,
    state: State<'_, AppState>,
) -> std::result::Result<ReplyParentContext, String> {
    // ── 1. Try local vault first ──
    {
        let db_guard = state.db_manager.read().await;
        if let Some(db) = db_guard.as_ref() {
            // Items are keyed as source:original_id — try common prefixes
            for prefix in &["bluesky:", "mastodon:", "activitypub:"] {
                let key = format!("{}{}", prefix, uri);
                if let Ok(Some(item)) = db.get_item_by_id(&key).await {
                    return Ok(ReplyParentContext {
                        uri: uri.clone(),
                        content: item.content.clone(),
                        author_handle: item.author_handle.clone().unwrap_or_default(),
                        author_avatar: item.author_avatar.clone(),
                        author_did: item.author_did.clone(),
                        created_at: item.original_timestamp,
                        image_urls: item.media_paths.iter()
                            .filter(|p| p.starts_with("http://") || p.starts_with("https://"))
                            .cloned()
                            .collect(),
                        from_vault: true,
                    });
                }
            }
            // Also try direct ID lookup (item.id == uri)
            if let Ok(Some(item)) = db.get_item_by_id(&uri).await {
                return Ok(ReplyParentContext {
                    uri: uri.clone(),
                    content: item.content.clone(),
                    author_handle: item.author_handle.clone().unwrap_or_default(),
                    author_avatar: item.author_avatar.clone(),
                    author_did: item.author_did.clone(),
                    created_at: item.original_timestamp,
                    image_urls: item.media_paths.iter()
                        .filter(|p| p.starts_with("http://") || p.starts_with("https://"))
                        .cloned()
                        .collect(),
                    from_vault: true,
                });
            }
        }
    }

    // ── 2. Fetch from network ──
    if uri.starts_with("at://") {
        // Bluesky AT URI — use public getPosts API (no auth required)
        resolve_bluesky_parent(&state.http_client, &uri).await
    } else if uri.contains("/statuses/") || uri.contains("/api/v1/") {
        // Mastodon-style URL — extract instance and status ID
        resolve_mastodon_parent(&state.http_client, &uri).await
    } else {
        Err(format!("Cannot resolve reply parent: unsupported URI scheme: {}", uri))
    }
}

/// Fetch a single Bluesky post via the public getPosts API.
async fn resolve_bluesky_parent(
    client: &reqwest::Client,
    uri: &str,
) -> Result<ReplyParentContext, String> {
    let url = format!(
        "https://public.api.bsky.app/xrpc/app.bsky.feed.getPosts?uris={}",
        urlencoding::encode(uri)
    );

    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Bluesky getPosts failed: {}", resp.status()));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let post = body.get("posts")
        .and_then(|p| p.as_array())
        .and_then(|a| a.first())
        .ok_or("Post not found on Bluesky")?;

    let author = post.get("author").unwrap_or(&serde_json::Value::Null);
    let record = post.get("record").unwrap_or(&serde_json::Value::Null);

    let created_at = record.get("createdAt")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp())
        .unwrap_or(0);

    let mut image_urls = Vec::new();
    if let Some(embed) = post.get("embed") {
        crate::connectors::bluesky::extract_image_urls_from_view(embed, &mut image_urls);
    }

    Ok(ReplyParentContext {
        uri: post.get("uri").and_then(|v| v.as_str()).unwrap_or(uri).to_string(),
        content: record.get("text").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
        author_handle: author.get("handle").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
        author_avatar: author.get("avatar").and_then(|v| v.as_str()).map(|s| s.to_string()),
        author_did: author.get("did").and_then(|v| v.as_str()).map(|s| s.to_string()),
        created_at,
        image_urls,
        from_vault: false,
    })
}

/// Fetch a single Mastodon status via the public API.
async fn resolve_mastodon_parent(
    client: &reqwest::Client,
    uri: &str,
) -> Result<ReplyParentContext, String> {
    // Mastodon URIs look like: https://mastodon.social/users/alice/statuses/12345
    // or https://mastodon.social/@alice/12345
    // API endpoint: GET /api/v1/statuses/:id

    // Parse instance and status ID from the URL
    let parsed = url::Url::parse(uri).map_err(|e| format!("Invalid Mastodon URI: {}", e))?;
    let host = parsed.host_str().ok_or("No host in Mastodon URI")?;

    // Extract status ID — it's always the last path segment
    let status_id = parsed.path_segments()
        .and_then(|segs| segs.last())
        .ok_or("Cannot extract status ID from URI")?;

    let api_url = format!("https://{}/api/v1/statuses/{}", host, status_id);
    let resp = client.get(&api_url).send().await.map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("Mastodon status fetch failed: {}", resp.status()));
    }

    let status: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let account = status.get("account").unwrap_or(&serde_json::Value::Null);

    let created_at = status.get("created_at")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp())
        .unwrap_or(0);

    // Strip HTML tags from Mastodon content
    let content_html = status.get("content").and_then(|v| v.as_str()).unwrap_or_default();
    let content = scraper::Html::parse_fragment(content_html)
        .root_element()
        .text()
        .collect::<String>();

    let image_urls: Vec<String> = status.get("media_attachments")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|a| a.get("type").and_then(|t| t.as_str()) == Some("image"))
                .filter_map(|a| a.get("url").and_then(|u| u.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let acct = account.get("acct").and_then(|v| v.as_str()).unwrap_or_default();
    let handle = if acct.contains('@') {
        format!("@{}", acct)
    } else {
        format!("@{}@{}", acct, host)
    };

    Ok(ReplyParentContext {
        uri: uri.to_string(),
        content,
        author_handle: handle,
        author_avatar: account.get("avatar").and_then(|v| v.as_str()).map(|s| s.to_string()),
        author_did: Some(format!("acct:{}@{}", acct, host)),
        created_at,
        image_urls,
        from_vault: false,
    })
}
