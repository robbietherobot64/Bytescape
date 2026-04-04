//! Media & Blob Commands
//!
//! Tauri IPC commands and helpers for all media operations: storing blobs in
//! Iroh, retrieving them with disk-cache acceleration, downloading blobs from
//! P2P peers, resolving HLS video streams into playable MP4 files, MIME-type
//! detection/backfill, and batch URL resolution for the frontend feed renderer.
//!
//! # How It Works
//!
//! 1. **Ingest** — `store_media_blob` writes raw bytes into the Iroh blob store
//!    and optionally generates a WebP thumbnail. The returned content-hash
//!    becomes the permanent media reference stored in `BytescapeItem`.
//! 2. **Retrieve** — `get_media_blob` / `get_media_blob_bytes` fetch blobs by
//!    hash, exporting them to a local `blob_cache/` directory with a
//!    magic-byte-detected extension. Subsequent requests are served from cache.
//! 3. **Peer sync** — `download_peer_blob` creates an ephemeral `Downloader` to
//!    pull a blob from a remote Iroh node (30 s timeout). Once downloaded, the
//!    blob is available locally like any other.
//! 4. **HLS → MP4** — `resolve_hls_video` resolves Bluesky HLS playlist URLs
//!    into local MP4 files using a 6-tier FFmpeg strategy (stream copy → audio
//!    re-encode → full re-encode, each tried for both native HLS and manual
//!    segment download). Raw `.ts` fallback if FFmpeg is unavailable.
//! 5. **URL resolution** — `resolve_media_url` and `resolve_blob_urls_batch`
//!    normalise every media reference type (blob hash, http URL, absolute path,
//!    relative path) into a frontend-ready URL, keeping all filesystem logic in
//!    the Rust engine.
//!
//! # Related Files
//!
//! - `src-tauri/src/sync.rs` — Iroh blob store, `store_media`, `get_media`
//! - `src-tauri/src/blob_hash.rs` — hash normalisation & parsing helpers
//! - `src-tauri/src/db.rs` — `BytescapeItem` schema (`media_blobs`, `media_mimes`)
//! - `src-tauri/src/commands/mod.rs` — command registration in the invoke handler
//! - `src/components/VideoPlayer.tsx` — frontend consumer of HLS/media URLs
//! - `src/hooks/useUnifiedFeed.ts` — calls `resolve_blob_urls_batch` for feeds

use bytes::Bytes;
use serde::Serialize;
use tauri::{State, Manager};

use crate::error::BytescapeError;
use crate::sync::Hash;
use crate::blob_hash;
use crate::AppState;

/// Maximum concurrent blob exports from Iroh to disk.
/// Prevents saturating disk I/O when resolve_blob_urls_batch processes many uncached blobs.
static BLOB_EXPORT_SEMAPHORE: std::sync::LazyLock<tokio::sync::Semaphore> =
    std::sync::LazyLock::new(|| tokio::sync::Semaphore::new(3));

/// Last time blob_cache eviction ran (debounce to at most once per 60s).
static LAST_EVICTION: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Maximum blob_cache directory size in bytes (500 MB).
/// Used by enforce_blob_cache_limit_debounced and startup eviction.
pub const BLOB_CACHE_MAX_BYTES: u64 = 500_000_000;

/// Enforce blob_cache size limit using LRU eviction (by modified time).
/// Debounced to run at most once per 60 seconds.
fn enforce_blob_cache_limit_debounced(cache_dir: &std::path::Path, max_bytes: u64) {
    // Debounce check — skip if we ran within the last 60 seconds
    {
        let mut last = LAST_EVICTION.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(instant) = *last {
            if instant.elapsed() < std::time::Duration::from_secs(60) {
                return;
            }
        }
        *last = Some(std::time::Instant::now());
    }

    // Spawn eviction on a background thread to avoid blocking the caller
    let dir = cache_dir.to_path_buf();
    std::thread::spawn(move || {
        enforce_blob_cache_limit(&dir, max_bytes);
    });
}

/// Enforce blob_cache size limit using LRU eviction (by modified time).
/// Deletes oldest files until total size is under max_bytes.
fn enforce_blob_cache_limit(cache_dir: &std::path::Path, max_bytes: u64) {
    let entries: Vec<_> = match std::fs::read_dir(cache_dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => return,
    };

    let mut files: Vec<(std::path::PathBuf, u64, std::time::SystemTime)> = Vec::new();
    let mut total_size: u64 = 0;

    for entry in entries {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                let size = meta.len();
                let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                total_size += size;
                files.push((entry.path(), size, modified));
            }
        }
    }

    if total_size <= max_bytes {
        return;
    }

    // Sort by modified time ascending (oldest first)
    files.sort_by_key(|f| f.2);

    let mut removed = 0usize;
    for (path, size, _) in &files {
        if total_size <= max_bytes {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            total_size -= size;
            removed += 1;
        }
    }

    if removed > 0 {
        println!("blob_cache eviction: removed {} files, cache now {:.1} MB",
            removed, total_size as f64 / 1_048_576.0);
    }
}

/// Public entry point for startup eviction (called from lib.rs background task).
pub fn enforce_blob_cache_limit_startup(cache_dir: &std::path::Path, max_bytes: u64) {
    enforce_blob_cache_limit(cache_dir, max_bytes);
}

/// Diagnostic snapshot of media reference resolution.
///
/// Returned by [`diagnose_media_ref`] to help debug blob/hash/path issues
/// when media appears in the UI but fails to load or play. Each field
/// records one step of the resolution pipeline so the frontend can display
/// a human-readable troubleshooting report.
#[derive(Debug, Serialize)]
pub struct MediaRefDiagnostic {
    pub input: String,
    pub mode: String,
    pub normalized: String,
    pub hash_parsed: bool,
    pub fetched_bytes: usize,
    pub detected_ext: Option<String>,
    pub cache_hit: bool,
    pub cache_path: Option<String>,
    pub file_exists: Option<bool>,
    pub error: Option<String>,
}

/// Strip the `blob:` prefix (if present) and normalise a raw hash string
/// into the canonical base32-lower form expected by the Iroh blob store.
fn normalize_blob_hash_input(input: &str) -> String {
    blob_hash::normalize_blob_ref(input)
}

/// Attempt to decode a base32-encoded blob hash, returning `None` on failure.
/// Used in non-critical paths where a missing hash is acceptable (e.g. MIME backfill).
fn decode_base32_hash_like(input: &str) -> Option<Hash> {
    blob_hash::parse_blob_hash(input).ok()
}

/// Parse a blob hash string, returning a user-facing error message on failure.
/// Preferred over [`decode_base32_hash_like`] when the caller must report
/// the parse error back through Tauri IPC.
fn parse_hash_safe(input: &str) -> std::result::Result<Hash, String> {
    blob_hash::parse_blob_hash(input)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Blob Storage & Retrieval
// ═══════════════════════════════════════════════════════════════════════════════

/// Store media data as a blob in Iroh.
///
/// Optionally generates a WebP thumbnail for images.
/// Returns the content hash (and thumbnail hash if generated).
///
/// **IPC**: Frontend → `invoke('store_media_blob')`
#[tauri::command]
pub async fn store_media_blob(
    data: Vec<u8>,
    generate_thumbnail: bool,
    state: State<'_, AppState>,
) -> std::result::Result<(String, Option<String>), String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;

    let bytes = Bytes::from(data);

    if generate_thumbnail {
        let (hash, thumb_hash) = sync.store_media_with_thumbnail(bytes, 256).await
            .map_err(|e| BytescapeError::BlobStorage(e))?;
        Ok((hash.to_string(), thumb_hash.map(|h| h.to_string())))
    } else {
        let hash = sync.store_media(bytes).await
            .map_err(|e| BytescapeError::BlobStorage(e))?;
        Ok((hash.to_string(), None))
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Peer Blob Download
// ═══════════════════════════════════════════════════════════════════════════════

/// Download a media blob from a remote peer via the iroh-blobs protocol.
///
/// Creates an ephemeral [`Downloader`] from the local blob store + endpoint,
/// then fetches the blob identified by `hash_str` from the peer at `node_id`.
/// Once complete the blob lives in the local blob store and can be read by
/// [`get_media_blob`] as if it were a local blob.
///
/// Returns `true` when the blob was successfully downloaded (or already present).
///
/// **IPC**: Frontend → `invoke('download_peer_blob')`
#[tauri::command]
pub async fn download_peer_blob(
    hash_str: String,
    node_id: String,
    state: State<'_, AppState>,
) -> std::result::Result<bool, String> {
    use crate::sync::Downloader;

    let normalized = normalize_blob_hash_input(&hash_str);
    let hash = parse_hash_safe(&normalized)?;

    // Fast path: blob already in local store
    {
        let sync_guard = state.sync_manager.lock().await;
        if let Some(sync) = sync_guard.as_ref() {
            if sync.has_media(hash).await {
                println!("[BLOB-DL] Already have blob {} locally", &normalized[..12.min(normalized.len())]);
                return Ok(true);
            }
        }
    }

    // Parse remote node ID
    let remote_node: iroh::PublicKey = node_id
        .parse()
        .map_err(|e| format!("Invalid node ID: {}", e))?;

    println!(
        "[BLOB-DL] Downloading blob {} from peer {}",
        &normalized[..12.min(normalized.len())],
        &node_id[..12.min(node_id.len())]
    );

    // Build a fresh Downloader from store + endpoint
    let downloader = {
        let sync_guard = state.sync_manager.lock().await;
        let sync = sync_guard.as_ref()
            .ok_or_else(|| BytescapeError::SyncNotInitialized)?;
        Downloader::new(sync.blobs(), sync.endpoint())
    };

    // Request the blob from the remote peer (BlobsProtocol on their Router serves it)
    let progress = downloader.download(hash, vec![remote_node]);

    // Await completion with a timeout (30 s per blob is generous)
    tokio::time::timeout(std::time::Duration::from_secs(30), progress)
        .await
        .map_err(|_| format!("Timeout downloading blob {}", &normalized[..12.min(normalized.len())]))?
        .map_err(|e| format!("Blob download failed: {}", e))?;

    println!(
        "[BLOB-DL] Successfully downloaded blob {} from peer {}",
        &normalized[..12.min(normalized.len())],
        &node_id[..12.min(node_id.len())]
    );

    Ok(true)
}

/// Retrieve a media blob by hash, caching it to disk.
///
/// The blob is fetched from Iroh and written to the blob cache directory.
/// Returns the cached file path for efficient subsequent access.
///
/// **IPC**: Frontend → `invoke('get_media_blob')`
///
/// # Tech debt
/// No cache eviction strategy — `blob_cache/` grows without bound. A future
/// LRU or size-cap policy should be added to prevent unbounded disk usage.
#[tauri::command]
pub async fn get_media_blob(hash_str: String, state: State<'_, AppState>) -> std::result::Result<String, String> {
    let normalized_hash = normalize_blob_hash_input(&hash_str);

    let app_data_dir = state.app_handle.path()
        .app_data_dir()
        .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
    let cache_dir = app_data_dir.join("blob_cache");

    // Check if cached file already exists (try common extensions)
    // P0.1 fix: read only the first 12 bytes for magic-byte extension detection
    // instead of loading the entire file (could be 200+ MB for videos).
    for ext in &["mp4", "mov", "webm", "mkv", "jpg", "png", "webp", "gif", "bin"] {
        let cached = cache_dir.join(format!("{}.{}", &normalized_hash, ext));
        if cached.exists() {
            // Read only the header bytes needed for magic detection (max 12)
            let detected_ext = {
                use std::io::Read;
                match std::fs::File::open(&cached) {
                    Ok(mut f) => {
                        let mut header = [0u8; 12];
                        let n = f.read(&mut header).unwrap_or(0);
                        detect_extension_from_bytes(&header[..n])
                    }
                    Err(_) => *ext, // can't open — assume current ext is fine
                }
            };
            let current_ext = cached.extension().and_then(|e| e.to_str()).unwrap_or("");
            if detected_ext != current_ext {
                let corrected = cache_dir.join(format!("{}.{}", &normalized_hash, detected_ext));
                // Rename instead of read-all + write-all to avoid loading entire file
                let _ = std::fs::rename(&cached, &corrected);
                return Ok(corrected.to_string_lossy().to_string());
            }
            return Ok(cached.to_string_lossy().to_string());
        }
    }

    // Not cached — stream from Iroh blob store directly to disk (zero memory buffering).
    // Previous implementation used sync.get_media(hash) which reads the ENTIRE blob
    // into memory (50-500 MB per video). Now we:
    //  1. Read only 12 header bytes for extension detection
    //  2. Stream blob to disk via export_media_to_file (zero heap allocation)
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;

    let hash = parse_hash_safe(&normalized_hash)?;

    // Step 1: Read ONLY header bytes for extension detection (12 bytes, not 500 MB)
    let header = sync.get_media_header(hash, 12).await
        .map_err(|e| BytescapeError::BlobStorage(e))?;
    let ext = detect_extension_from_bytes(&header);

    // Step 2: Stream blob directly to cache file (zero memory buffering)
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
    let cache_path = cache_dir.join(format!("{}.{}", &normalized_hash, ext));
    let bytes_written = sync.export_media_to_file(hash, &cache_path).await
        .map_err(|e| BytescapeError::BlobStorage(e))?;

    if bytes_written > 50_000_000 {
        println!("get_media_blob: WARNING — exported large blob {} ({:.1} MB) to cache",
            &normalized_hash[..8.min(normalized_hash.len())],
            bytes_written as f64 / 1_048_576.0);
    }

    // Enforce cache size limit (debounced — at most once per 60s)
    enforce_blob_cache_limit_debounced(&cache_dir, BLOB_CACHE_MAX_BYTES);

    Ok(cache_path.to_string_lossy().to_string())
}

/// Retrieve media blob bytes directly for browser Object URL creation.
///
/// Returns `Vec<u8>` via IPC. For vault videos (< 8 MB), JSON overhead is
/// acceptable. For large HLS videos, use [`resolve_hls_video`] which returns
/// `tauri::ipc::Response`.
///
/// **IPC**: Frontend → `invoke('get_media_blob_bytes')`
#[tauri::command]
pub async fn get_media_blob_bytes(hash_str: String, state: State<'_, AppState>) -> std::result::Result<tauri::ipc::Response, String> {
    let normalized_hash = normalize_blob_hash_input(&hash_str);

    let app_data_dir = state.app_handle.path()
        .app_data_dir()
        .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
    let cache_dir = app_data_dir.join("blob_cache");

    // Check if cached file already exists
    for ext in &["mp4", "mov", "webm", "mkv", "jpg", "png", "webp", "gif", "bin"] {
        let cached = cache_dir.join(format!("{}.{}", &normalized_hash, ext));
        if cached.exists() {
            if let Ok(metadata) = std::fs::metadata(&cached) {
                if metadata.len() > 0 {
                    if let Ok(bytes) = std::fs::read(&cached) {
                        println!("get_media_blob_bytes: cache hit {} ({} bytes)", ext, bytes.len());
                        return Ok(tauri::ipc::Response::new(bytes));
                    }
                }
            }
        }
    }

    // Not cached — stream from Iroh to cache file first, then read from cache.
    // Previous implementation held the full blob in Rust heap AND serialized it
    // for IPC simultaneously (double memory). Now we stream to disk first.
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;

    let hash = parse_hash_safe(&normalized_hash)?;

    // Step 1: detect extension from header bytes only
    let header = sync.get_media_header(hash, 12).await
        .map_err(|e| BytescapeError::BlobStorage(e))?;
    let ext = detect_extension_from_bytes(&header);

    // Step 2: stream blob to cache file (zero heap allocation)
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
    let cache_path = cache_dir.join(format!("{}.{}", &normalized_hash, ext));
    let bytes_written = sync.export_media_to_file(hash, &cache_path).await
        .map_err(|e| BytescapeError::BlobStorage(e))?;

    if bytes_written > 50_000_000 {
        println!("get_media_blob_bytes: WARNING — sending large blob over IPC ({:.1} MB)",
            bytes_written as f64 / 1_048_576.0);
    }

    // Step 3: read from cache for IPC return (single allocation, no Iroh buffer)
    let bytes = std::fs::read(&cache_path)
        .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;

    println!("get_media_blob_bytes: streamed from Iroh, cached as {} ({} bytes)", ext, bytes.len());
    enforce_blob_cache_limit_debounced(&cache_dir, BLOB_CACHE_MAX_BYTES);
    Ok(tauri::ipc::Response::new(bytes))
}

/// Get the media directory path for storing imported files.
///
/// **IPC**: Frontend → `invoke('get_media_dir')`
#[tauri::command]
pub async fn get_media_dir(state: State<'_, AppState>) -> std::result::Result<String, String> {
    let app_data_dir = state.app_handle.path()
        .app_data_dir()
        .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
    let media_dir = app_data_dir.join("media");
    Ok(media_dir.to_string_lossy().to_string())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Diagnostics
// ═══════════════════════════════════════════════════════════════════════════════

/// Diagnose media reference resolution and playback readiness.
///
/// Useful for debugging blob/hash/path issues when media appears but won't play.
/// Walks the full resolution pipeline (URL → filesystem → blob cache → Iroh fetch)
/// and records what happened at each step in a [`MediaRefDiagnostic`].
///
/// **IPC**: Frontend → `invoke('diagnose_media_ref')`
#[tauri::command]
pub async fn diagnose_media_ref(
    media_ref: String,
    state: State<'_, AppState>,
) -> std::result::Result<MediaRefDiagnostic, String> {
    let input = media_ref.clone();
    let trimmed = media_ref.trim().to_string();

    if trimmed.is_empty() {
        return Ok(MediaRefDiagnostic {
            input,
            mode: "empty".to_string(),
            normalized: "".to_string(),
            hash_parsed: false,
            fetched_bytes: 0,
            detected_ext: None,
            cache_hit: false,
            cache_path: None,
            file_exists: None,
            error: Some("Empty media reference".to_string()),
        });
    }

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Ok(MediaRefDiagnostic {
            input,
            mode: "url".to_string(),
            normalized: trimmed,
            hash_parsed: false,
            fetched_bytes: 0,
            detected_ext: None,
            cache_hit: false,
            cache_path: None,
            file_exists: None,
            error: None,
        });
    }

    let app_data_dir = state
        .app_handle
        .path()
        .app_data_dir()
        .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
    let cache_dir = app_data_dir.join("blob_cache");

    let is_blob_like = trimmed.starts_with("blob:") || parse_hash_safe(&trimmed).is_ok();
    if !is_blob_like {
        let p = std::path::PathBuf::from(&trimmed);
        let exists = p.exists();
        let (bytes_len, ext) = if exists {
            match std::fs::read(&p) {
                Ok(bytes) => (bytes.len(), Some(detect_extension_from_bytes(&bytes).to_string())),
                Err(_) => (0, None),
            }
        } else {
            (0, None)
        };

        return Ok(MediaRefDiagnostic {
            input,
            mode: "filesystem".to_string(),
            normalized: trimmed,
            hash_parsed: false,
            fetched_bytes: bytes_len,
            detected_ext: ext,
            cache_hit: false,
            cache_path: Some(p.to_string_lossy().to_string()),
            file_exists: Some(exists),
            error: if exists { None } else { Some("File does not exist".to_string()) },
        });
    }

    let normalized = normalize_blob_hash_input(&trimmed);
    let hash = match parse_hash_safe(&normalized) {
        Ok(h) => h,
        Err(e) => {
            return Ok(MediaRefDiagnostic {
                input,
                mode: "blob".to_string(),
                normalized,
                hash_parsed: false,
                fetched_bytes: 0,
                detected_ext: None,
                cache_hit: false,
                cache_path: None,
                file_exists: None,
                error: Some(e),
            })
        }
    };

    for ext in &["mp4", "mov", "webm", "mkv", "jpg", "png", "webp", "gif", "bin"] {
        let cached = cache_dir.join(format!("{}.{}", &normalized, ext));
        if cached.exists() {
            return Ok(MediaRefDiagnostic {
                input,
                mode: "blob".to_string(),
                normalized,
                hash_parsed: true,
                fetched_bytes: 0,
                detected_ext: Some((*ext).to_string()),
                cache_hit: true,
                cache_path: Some(cached.to_string_lossy().to_string()),
                file_exists: Some(true),
                error: None,
            });
        }
    }

    let sync_guard = state.sync_manager.lock().await;
    let Some(sync) = sync_guard.as_ref() else {
        return Ok(MediaRefDiagnostic {
            input,
            mode: "blob".to_string(),
            normalized,
            hash_parsed: true,
            fetched_bytes: 0,
            detected_ext: None,
            cache_hit: false,
            cache_path: None,
            file_exists: None,
            error: Some("Sync manager not initialized".to_string()),
        });
    };

    match sync.get_media_header(hash, 12).await {
        Ok(header) => {
            let ext = detect_extension_from_bytes(&header).to_string();
            let _ = std::fs::create_dir_all(&cache_dir);
            let cache_path = cache_dir.join(format!("{}.{}", &normalized, ext));
            let bytes_written = sync.export_media_to_file(hash, &cache_path).await
                .unwrap_or(0);

            Ok(MediaRefDiagnostic {
                input,
                mode: "blob".to_string(),
                normalized,
                hash_parsed: true,
                fetched_bytes: bytes_written as usize,
                detected_ext: Some(ext),
                cache_hit: false,
                cache_path: Some(cache_path.to_string_lossy().to_string()),
                file_exists: Some(true),
                error: None,
            })
        }
        Err(e) => Ok(MediaRefDiagnostic {
            input,
            mode: "blob".to_string(),
            normalized,
            hash_parsed: true,
            fetched_bytes: 0,
            detected_ext: None,
            cache_hit: false,
            cache_path: None,
            file_exists: None,
            error: Some(e),
        }),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// MIME Backfill
// ═══════════════════════════════════════════════════════════════════════════════

/// Backfill `media_mimes` for existing BytescapeItems that have media_blobs
/// but no MIME metadata yet (records created before `media_mimes` was added).
///
/// Reads the first few bytes of each blob to detect its MIME type from magic
/// bytes, then updates the item in the database.  Safe to run more than once:
/// items that already have the correct `media_mimes` length are skipped.
///
/// **IPC**: Frontend → `invoke('backfill_media_mimes')`
///
/// # Tech debt
/// Loads *all* item headers into memory via `list_all_item_headers`, then
/// fetches full blobs for MIME sniffing. For vaults with thousands of items
/// this is O(n) Iroh reads — a chunked/streaming approach would be better.
#[tauri::command]
pub async fn backfill_media_mimes(state: State<'_, AppState>) -> std::result::Result<usize, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| crate::error::BytescapeError::DatabaseNotInitialized)?;
    let sync_guard = state.sync_manager.lock().await;
    let sync = match sync_guard.as_ref() {
        Some(s) => s.clone(),
        None => return Ok(0), // No sync manager yet, nothing to backfill
    };
    drop(sync_guard);

    let item_headers = db.list_all_item_headers().await.map_err(|e| e.to_string())?;
    let mut updated = 0usize;

    for h in item_headers {
        // Skip items that already have correct MIME metadata based on header info
        if h.media_blob_count == 0 || h.media_mimes_count == h.media_blob_count {
            continue;
        }

        // Fetch full item to perform backfill
        let mut item = match db.get_item_by_id(&h.id).await {
            Ok(Some(item)) => item,
            _ => continue,
        };

        let mut mimes = Vec::with_capacity(item.media_blobs.len());
        for blob_ref in &item.media_blobs {
            let mut hash_str = blob_ref.trim().to_string();
            while let Some(rest) = hash_str.strip_prefix("blob:") {
                hash_str = rest.trim().to_string();
            }
            // Strip optional ":thumbhash" suffix
            if let Some(pos) = hash_str.find(':') {
                hash_str = hash_str[..pos].to_string();
            }

            if hash_str.is_empty() {
                mimes.push("application/octet-stream".to_string());
                continue;
            }

            // Try to parse and fetch blob header for MIME detection
            let hash_opt = decode_base32_hash_like(&hash_str);
            if let Some(hash) = hash_opt {
                // Read only 32 bytes for magic detection, not the entire blob
                match sync.get_media_header(hash, 32).await {
                    Ok(header) => {
                        mimes.push(mime_from_magic_bytes(&header));
                    }
                    Err(_) => {
                        // Fall back to path hint if available
                        let idx = mimes.len();
                        let hint = item.media_paths.get(idx).map(|s: &String| s.as_str()).unwrap_or("");
                        mimes.push(mime_from_path_hint(hint));
                    }
                }
            } else {
                let idx = mimes.len();
                let hint = item.media_paths.get(idx).map(|s: &String| s.as_str()).unwrap_or("");
                mimes.push(mime_from_path_hint(hint));
            }
        }

        item.media_mimes = mimes;
        if let Err(e) = db.upsert_item(&item).await {
            eprintln!("backfill_media_mimes: failed to update item {}: {}", item.id, e);
            continue;
        }
        updated += 1;
    }

    println!("backfill_media_mimes: updated {} items", updated);
    Ok(updated)
}

/// Detect MIME type from magic bytes using the `infer` crate.
///
/// Covers 100+ file types including TIFF, BMP, HEIC, AVI, FLAC, WAV,
/// and all formats previously handled by the manual implementation.
/// Falls back to a few manual checks that `infer` doesn't cover.
pub fn mime_from_magic_bytes(data: &[u8]) -> String {
    // Primary: use `infer` for broad, battle-tested magic byte detection
    if let Some(kind) = infer::get(data) {
        return kind.mime_type().to_string();
    }

    // Fallback: a handful of formats `infer` may not recognise
    if data.starts_with(b"OggS") {
        // infer detects Ogg Vorbis/Opus audio but may miss video/ogg containers
        return "audio/ogg".to_string();
    }
    if data.starts_with(b"%PDF-") {
        return "application/pdf".to_string();
    }

    "application/octet-stream".to_string()
}

/// Detect MIME type from file path/URI hint.
///
/// Extended to cover TIFF, BMP, HEIC, AVIF, AVI, FLAC, WAV, SVG,
/// OGG audio, OPUS, AAC, MP3, M4A, and ICO — matching the expanded
/// magic-byte coverage from MED-5.
pub fn mime_from_path_hint(path: &str) -> String {
    let lower = path.to_lowercase();
    let clean = lower.split('?').next().unwrap_or(&lower);
    let clean = clean.split('#').next().unwrap_or(clean);
    // Images
    if clean.ends_with(".jpg") || clean.ends_with(".jpeg") {
        "image/jpeg".to_string()
    } else if clean.ends_with(".png") {
        "image/png".to_string()
    } else if clean.ends_with(".webp") {
        "image/webp".to_string()
    } else if clean.ends_with(".gif") {
        "image/gif".to_string()
    } else if clean.ends_with(".tiff") || clean.ends_with(".tif") {
        "image/tiff".to_string()
    } else if clean.ends_with(".bmp") {
        "image/bmp".to_string()
    } else if clean.ends_with(".heic") || clean.ends_with(".heif") {
        "image/heic".to_string()
    } else if clean.ends_with(".avif") {
        "image/avif".to_string()
    } else if clean.ends_with(".svg") {
        "image/svg+xml".to_string()
    } else if clean.ends_with(".ico") {
        "image/x-icon".to_string()
    // Video
    } else if clean.ends_with(".mp4") || clean.ends_with(".m4v") {
        "video/mp4".to_string()
    } else if clean.ends_with(".mov") {
        "video/quicktime".to_string()
    } else if clean.ends_with(".webm") {
        "video/webm".to_string()
    } else if clean.ends_with(".mkv") {
        "video/x-matroska".to_string()
    } else if clean.ends_with(".avi") {
        "video/x-msvideo".to_string()
    } else if clean.ends_with(".ts") || clean.ends_with(".mpg") {
        "video/mp2t".to_string()
    // Audio
    } else if clean.ends_with(".mp3") {
        "audio/mpeg".to_string()
    } else if clean.ends_with(".flac") {
        "audio/flac".to_string()
    } else if clean.ends_with(".wav") {
        "audio/wav".to_string()
    } else if clean.ends_with(".ogg") || clean.ends_with(".oga") {
        "audio/ogg".to_string()
    } else if clean.ends_with(".opus") {
        "audio/opus".to_string()
    } else if clean.ends_with(".aac") {
        "audio/aac".to_string()
    } else if clean.ends_with(".m4a") {
        "audio/mp4".to_string()
    // Documents
    } else if clean.ends_with(".pdf") {
        "application/pdf".to_string()
    } else {
        "application/octet-stream".to_string()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Video Cancel
// ═══════════════════════════════════════════════════════════════════════════════

/// Cancel an active video resolve operation by killing the FFmpeg process.
///
/// Called by the frontend when a user scrolls past a video that's still converting.
/// Prevents CPU thrashing from multiple concurrent FFmpeg processes.
///
/// **IPC**: Frontend → `invoke('cancel_video_resolve')`
///
/// # Tech debt
/// Uses `kill` shell command which is Linux/macOS-only. On Windows this would
/// need `taskkill /PID` or the `kill` equivalent. Consider using
/// `std::process::Child::kill()` by storing the `Child` handle instead of the PID.
#[tauri::command]
pub async fn cancel_video_resolve(playlist_url: String, state: tauri::State<'_, AppState>) -> std::result::Result<(), String> {
    let url_hash = format!("{:x}", {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        playlist_url.hash(&mut hasher);
        hasher.finish()
    });

    let mut active = state.active_ffmpeg.lock().await;
    if let Some(pid) = active.remove(&url_hash) {
        // Kill the FFmpeg process via shell command (cross-platform)
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .output();

        // Clean up partial output files
        let app_data_dir = state.app_handle.path().app_data_dir()
            .map_err(|e| format!("Failed to get app data dir: {}", e))?;
        let cache_dir = app_data_dir.join("blob_cache");
        let mp4_path = cache_dir.join(format!("{}.mp4", url_hash));
        let _ = std::fs::remove_file(&mp4_path);

        println!("cancel_video_resolve: killed FFmpeg pid {} for {}", pid, &url_hash[..8.min(url_hash.len())]);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// HLS Video Resolution
// ═══════════════════════════════════════════════════════════════════════════════
// Tech debt: The 6 FFmpeg strategy functions below (3 for HLS URLs, 3 for local
// .ts files) are near-identical (~180 lines of duplicated arg-building). A single
// generic `run_ffmpeg(input, codecs, mp4_path)` helper would eliminate the
// duplication and make flag changes (e.g. bumping analyseduration) a one-liner.

/// Download an HLS video, returning a local MP4 file path.
///
/// Tiered strategy (in order of speed):
/// 1. Cache hit — return existing MP4 immediately
/// 2. FFmpeg native HLS download with stream copy (Strategy A — <1s)
/// 3. FFmpeg native HLS with audio re-encode only (Strategy B — 1-2s)
/// 4. Manual segment download + tiered remux (A→B→C)
/// 5. Full re-encode (Strategy C — 5-20s, last resort)
/// 6. Raw `.ts` — if FFmpeg is entirely unavailable
///
/// **IPC**: Frontend → `invoke('resolve_hls_video')`
#[tauri::command]
pub async fn resolve_hls_video(playlist_url: String, state: tauri::State<'_, AppState>) -> std::result::Result<String, String> {
    use std::io::Write;

    let app_data_dir: std::path::PathBuf = state.app_handle.path().app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    let cache_dir = app_data_dir.join("blob_cache");
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("Failed to create cache dir: {}", e))?;

    let url_hash = format!("{:x}", md5_hash(&playlist_url));
    let mp4_path = cache_dir.join(format!("{}.mp4", url_hash));
    let ts_path = cache_dir.join(format!("{}.ts", url_hash));

    // ── Cache hit — prefer MP4 (validate to catch old broken conversions) ──
    if mp4_path.exists() {
        if let Ok(meta) = std::fs::metadata(&mp4_path) {
            if meta.len() > 1000 {
                // Validate cached file — old FFmpeg flags (discardcorrupt + fps_mode cfr
                // with stream copy) produced stuttery MP4s. Re-convert if invalid.
                if validate_mp4_with_ffprobe(&mp4_path) {
                    println!("resolve_hls_video: MP4 cache hit ({:.1} MB)", meta.len() as f64 / (1024.0 * 1024.0));
                    return Ok(mp4_path.to_string_lossy().into_owned());
                }
                println!("resolve_hls_video: Cached MP4 failed validation, re-converting...");
                let _ = std::fs::remove_file(&mp4_path);
            }
        }
    }
    // Stale TS without MP4 — try tiered remux
    if ts_path.exists() {
        if let Ok(meta) = std::fs::metadata(&ts_path) {
            if meta.len() > 1000 {
                let ts_for_stale = ts_path.clone();
                let mp4_for_stale = mp4_path.clone();
                if let Ok(Ok(Ok(remuxed))) = tokio::time::timeout(
                    std::time::Duration::from_secs(120),
                    tokio::task::spawn_blocking(move || tiered_ts_to_mp4(&ts_for_stale, &mp4_for_stale))
                ).await {
                    println!("resolve_hls_video: Remuxed existing TS → MP4 ({:.1} MB)",
                        std::fs::metadata(&mp4_path).map(|m| m.len()).unwrap_or(0) as f64 / (1024.0 * 1024.0));
                    return Ok(remuxed);
                }
                // FFmpeg unavailable — return TS as fallback
                println!("resolve_hls_video: TS cache hit, no FFmpeg ({:.1} MB)", meta.len() as f64 / (1024.0 * 1024.0));
                return Ok(ts_path.to_string_lossy().into_owned());
            }
        }
    }

    // ── Strategy 1: FFmpeg native HLS → MP4 (tiered: copy → audio re-encode → full re-encode) ──
    // Try the fastest approach first, fall back to more expensive ones only if validation fails.
    println!("resolve_hls_video: Trying FFmpeg native HLS download (tiered)...");
    let playlist_for_ffmpeg = playlist_url.clone();
    let mp4_for_ffmpeg = mp4_path.clone();
    let ts_for_cleanup = ts_path.clone();
    match tokio::time::timeout(
        std::time::Duration::from_secs(120),
        tokio::task::spawn_blocking(move || {
        // Strategy A: Stream copy — fastest (<1s, I/O bound)
        let mp4 = &mp4_for_ffmpeg;
        if let Ok(path) = ffmpeg_hls_stream_copy(&playlist_for_ffmpeg, mp4) {
            if validate_mp4_with_ffprobe(mp4) {
                println!("resolve_hls_video: Strategy A (stream copy) succeeded");
                return Ok(path);
            }
            println!("resolve_hls_video: Strategy A produced invalid MP4, trying B...");
            let _ = std::fs::remove_file(mp4);
        } else {
            println!("resolve_hls_video: Strategy A failed, trying B...");
        }

        // Strategy B: Copy video, re-encode audio only (1-2s)
        if let Ok(path) = ffmpeg_hls_audio_reencode(&playlist_for_ffmpeg, mp4) {
            if validate_mp4_with_ffprobe(mp4) {
                println!("resolve_hls_video: Strategy B (audio re-encode) succeeded");
                return Ok(path);
            }
            println!("resolve_hls_video: Strategy B produced invalid MP4, trying C...");
            let _ = std::fs::remove_file(mp4);
        } else {
            println!("resolve_hls_video: Strategy B failed, trying C...");
        }

        // Strategy C: Full re-encode (5-20s, last resort)
        ffmpeg_hls_full_reencode(&playlist_for_ffmpeg, mp4)
    })
    ).await {
        Ok(Ok(Ok(path))) => {
            println!("resolve_hls_video: FFmpeg HLS → MP4 success ({:.1} MB)",
                std::fs::metadata(&mp4_path).map(|m| m.len()).unwrap_or(0) as f64 / (1024.0 * 1024.0));
            let _ = std::fs::remove_file(&ts_for_cleanup); // clean any stale TS
            return Ok(path);
        }
        Ok(Ok(Err(e))) => {
            println!("resolve_hls_video: All FFmpeg HLS strategies failed ({}), falling back to manual download...", e);
        }
        Ok(Err(e)) => {
            println!("resolve_hls_video: FFmpeg task panicked ({}), falling back to manual download...", e);
        }
        Err(_) => {
            println!("resolve_hls_video: FFmpeg timed out after 120s, falling back to manual download...");
        }
    }

    // ── Strategy 2: Manual segment download + remux ─────────────────────────
    let client = crate::connectors::bluesky::SHARED_CLIENT.clone();

    // 1. Fetch the master/media playlist
    let playlist_text = client.get(&playlist_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch playlist: {}", e))?
        .text()
        .await
        .map_err(|e| format!("Failed to read playlist body: {}", e))?;

    // 2. Determine the base URL for resolving relative paths
    let master_base = playlist_url
        .split('?').next().unwrap_or(&playlist_url)
        .rsplit_once('/').map(|(base, _)| base).unwrap_or(&playlist_url)
        .to_string();

    // 3. Select the best variant — target 720p for quality (lazy loading prevents OOM)
    let (media_playlist_text, segment_base_url) = if playlist_text.contains("#EXT-X-STREAM-INF") {
        let target_bandwidth: u64 = 3_500_000; // 3.5 Mbps ≈ 720p
        let mut variants: Vec<(u64, String)> = Vec::new();
        let mut current_bw: Option<u64> = None;

        for line in playlist_text.lines() {
            if line.starts_with("#EXT-X-STREAM-INF") {
                if let Some(bw_str) = line.split("BANDWIDTH=").nth(1) {
                    let bw: u64 = bw_str.split(',').next().unwrap_or("0").parse().unwrap_or(0);
                    current_bw = Some(bw);
                }
            } else if let Some(bw) = current_bw.take() {
                if !line.starts_with('#') && !line.trim().is_empty() {
                    variants.push((bw, line.trim().to_string()));
                }
            }
        }

        if variants.is_empty() {
            return Err("Could not find any variant stream in master playlist".to_string());
        }

        variants.sort_by_key(|(bw, _)| (*bw as i64 - target_bandwidth as i64).unsigned_abs());
        let (chosen_bw, chosen_uri) = &variants[0];

        let variant_url = if chosen_uri.starts_with("http") {
            chosen_uri.clone()
        } else {
            format!("{}/{}", master_base, chosen_uri)
        };

        println!("resolve_hls_video: Selected {}kbps variant", chosen_bw / 1000);

        let variant_text = client.get(&variant_url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch variant playlist: {}", e))?
            .text()
            .await
            .map_err(|e| format!("Failed to read variant playlist: {}", e))?;

        let variant_base = variant_url
            .split('?').next().unwrap_or(&variant_url)
            .rsplit_once('/').map(|(base, _)| base).unwrap_or(&variant_url)
            .to_string();

        (variant_text, variant_base)
    } else {
        (playlist_text, master_base)
    };

    // 4. Parse segment URLs
    let mut segment_urls: Vec<String> = Vec::new();
    for line in media_playlist_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let segment_url = if trimmed.starts_with("http") {
            trimmed.to_string()
        } else {
            format!("{}/{}", segment_base_url, trimmed)
        };
        segment_urls.push(segment_url);
    }

    if segment_urls.is_empty() {
        return Err("No segments found in HLS playlist".to_string());
    }

    println!("resolve_hls_video: Downloading {} segments...", segment_urls.len());

    // 5. Download segments in parallel batches and write to disk
    let mut file = std::fs::File::create(&ts_path)
        .map_err(|e| format!("Failed to create temp file: {}", e))?;

    let batch_size = 6;
    let mut total_bytes: usize = 0;

    for batch_start in (0..segment_urls.len()).step_by(batch_size) {
        let batch_end = (batch_start + batch_size).min(segment_urls.len());
        let batch = &segment_urls[batch_start..batch_end];

        let mut handles = Vec::with_capacity(batch.len());
        for seg_url in batch.iter() {
            let client = client.clone();
            let url = seg_url.clone();
            handles.push(tokio::spawn(async move {
                let resp = client.get(&url)
                    .send()
                    .await
                    .map_err(|e| format!("Segment fetch failed: {}", e))?;
                let bytes = resp.bytes()
                    .await
                    .map_err(|e| format!("Segment read failed: {}", e))?;
                Ok::<bytes::Bytes, String>(bytes)
            }));
        }

        for (i, handle) in handles.into_iter().enumerate() {
            let seg_bytes = handle.await
                .map_err(|e| format!("Segment {} task panicked: {}", batch_start + i, e))?
                .map_err(|e| format!("Segment {} download failed: {}", batch_start + i, e))?;
            file.write_all(&seg_bytes)
                .map_err(|e| format!("Failed to write segment {}: {}", batch_start + i, e))?;
            total_bytes += seg_bytes.len();
        }
    }

    file.flush().map_err(|e| format!("Failed to flush file: {}", e))?;
    drop(file);

    println!("resolve_hls_video: Manual download complete, total {:.1} MB", total_bytes as f64 / (1024.0 * 1024.0));

    // Tiered remux: stream copy → audio re-encode → full re-encode
    let ts_for_remux = ts_path.clone();
    let mp4_for_remux = mp4_path.clone();
    match tokio::time::timeout(
        std::time::Duration::from_secs(120),
        tokio::task::spawn_blocking(move || tiered_ts_to_mp4(&ts_for_remux, &mp4_for_remux))
    ).await {
        Ok(Ok(Ok(mp4))) => {
            println!("resolve_hls_video: Remuxed to MP4 ({:.1} MB)",
                std::fs::metadata(&mp4_path).map(|m| m.len()).unwrap_or(0) as f64 / (1024.0 * 1024.0));
            Ok(mp4)
        }
        Ok(Ok(Err(e))) => {
            println!("resolve_hls_video: FFmpeg remux failed ({}), returning raw TS", e);
            Ok(ts_path.to_string_lossy().into_owned())
        }
        Ok(Err(e)) => {
            println!("resolve_hls_video: Remux task panicked ({}), returning raw TS", e);
            Ok(ts_path.to_string_lossy().into_owned())
        }
        Err(_) => {
            println!("resolve_hls_video: FFmpeg remux timed out after 120s, returning raw TS");
            Ok(ts_path.to_string_lossy().into_owned())
        }
    }
}

/// ── Strategy A: Stream copy with PTS fixup (fastest, <1 second) ──
///
/// Copies both video and audio streams without re-encoding.
/// Uses genpts+igndts to regenerate PTS from stream data (igndts avoids DTS
/// discontinuities at HLS segment boundaries that discardcorrupt would drop).
/// Does NOT use -fps_mode cfr — cfr requires frame decoding which stream copy skips.
fn ffmpeg_hls_stream_copy(playlist_url: &str, mp4_path: &std::path::Path) -> std::result::Result<String, String> {
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-analyzeduration", "10M",
            "-probesize", "10M",
            "-fflags", "+genpts+igndts",
            "-avoid_negative_ts", "make_zero",
            "-i", playlist_url,
            "-c:v", "copy",
            "-c:a", "copy",
            "-bsf:a", "aac_adtstoasc",
            "-movflags", "+faststart",
            "-f", "mp4",
            &mp4_path.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("FFmpeg not found or failed to execute: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg stream-copy exited with {}: {}", output.status, stderr));
    }

    validate_mp4_file_size(mp4_path)
}

/// ── Strategy B: Copy video, re-encode audio only (1-2 seconds) ──
///
/// Fixes AAC-in-TS framing issues by re-encoding audio while keeping
/// video stream-copy speed. The AAC encoder outputs correct MP4 framing
/// natively — no aac_adtstoasc bitstream filter needed (that filter
/// expects ADTS input, not encoder output).
fn ffmpeg_hls_audio_reencode(playlist_url: &str, mp4_path: &std::path::Path) -> std::result::Result<String, String> {
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-analyzeduration", "10M",
            "-probesize", "10M",
            "-fflags", "+genpts+igndts",
            "-avoid_negative_ts", "make_zero",
            "-i", playlist_url,
            "-c:v", "copy",
            "-c:a", "aac",
            "-b:a", "128k",
            "-movflags", "+faststart",
            "-f", "mp4",
            &mp4_path.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("FFmpeg audio-reencode failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg audio-reencode exited with {}: {}", output.status, stderr));
    }

    validate_mp4_file_size(mp4_path)
}

/// ── Strategy C: Full re-encode (5-20 seconds, last resort) ──
///
/// Re-encodes both video and audio to regenerate all timestamps from scratch.
/// Only used when stream copy and audio-only re-encode both fail validation.
/// fps_mode cfr is valid here because the decoder produces real frames.
fn ffmpeg_hls_full_reencode(playlist_url: &str, mp4_path: &std::path::Path) -> std::result::Result<String, String> {
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-analyzeduration", "10M",
            "-probesize", "10M",
            "-fflags", "+genpts+igndts",
            "-avoid_negative_ts", "make_zero",
            "-i", playlist_url,
            "-c:v", "libx264",
            "-preset", "ultrafast",
            "-crf", "23",
            "-c:a", "aac",
            "-b:a", "128k",
            "-fps_mode", "cfr",
            "-max_muxing_queue_size", "1024",
            "-movflags", "+faststart",
            "-f", "mp4",
            &mp4_path.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("FFmpeg full-reencode failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg full-reencode exited with {}: {}", output.status, stderr));
    }

    validate_mp4_file_size(mp4_path)
}

/// Tiered TS→MP4 conversion: try stream copy first, then audio re-encode, then full.
/// Used for manually-downloaded HLS segments that have been concatenated into a .ts file.
fn tiered_ts_to_mp4(ts_path: &std::path::Path, mp4_path: &std::path::Path) -> std::result::Result<String, String> {
    let ts_str = ts_path.to_string_lossy();

    // Strategy A: Stream copy with PTS fixup
    if let Ok(path) = ts_stream_copy_to_mp4(&ts_str, mp4_path) {
        if validate_mp4_with_ffprobe(mp4_path) {
            println!("tiered_ts_to_mp4: Strategy A (stream copy) succeeded");
            let _ = std::fs::remove_file(ts_path);
            return Ok(path);
        }
        let _ = std::fs::remove_file(mp4_path);
    }

    // Strategy B: Copy video, re-encode audio
    if let Ok(path) = ts_audio_reencode_to_mp4(&ts_str, mp4_path) {
        if validate_mp4_with_ffprobe(mp4_path) {
            println!("tiered_ts_to_mp4: Strategy B (audio re-encode) succeeded");
            let _ = std::fs::remove_file(ts_path);
            return Ok(path);
        }
        let _ = std::fs::remove_file(mp4_path);
    }

    // Strategy C: Full re-encode
    let result = ts_full_reencode_to_mp4(&ts_str, mp4_path);
    if result.is_ok() {
        let _ = std::fs::remove_file(ts_path);
    }
    result
}

/// TS→MP4 Strategy A: Stream copy
fn ts_stream_copy_to_mp4(ts_input: &str, mp4_path: &std::path::Path) -> std::result::Result<String, String> {
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-analyzeduration", "10M",
            "-probesize", "10M",
            "-fflags", "+genpts+igndts",
            "-avoid_negative_ts", "make_zero",
            "-i", ts_input,
            "-c:v", "copy",
            "-c:a", "copy",
            "-bsf:a", "aac_adtstoasc",
            "-movflags", "+faststart",
            "-f", "mp4",
            &mp4_path.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("FFmpeg ts-stream-copy failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg ts-stream-copy exited with {}: {}", output.status, stderr));
    }
    validate_mp4_file_size(mp4_path)
}

/// TS→MP4 Strategy B: Copy video, re-encode audio
fn ts_audio_reencode_to_mp4(ts_input: &str, mp4_path: &std::path::Path) -> std::result::Result<String, String> {
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-analyzeduration", "10M",
            "-probesize", "10M",
            "-fflags", "+genpts+igndts",
            "-avoid_negative_ts", "make_zero",
            "-i", ts_input,
            "-c:v", "copy",
            "-c:a", "aac",
            "-b:a", "128k",
            "-movflags", "+faststart",
            "-f", "mp4",
            &mp4_path.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("FFmpeg ts-audio-reencode failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg ts-audio-reencode exited with {}: {}", output.status, stderr));
    }
    validate_mp4_file_size(mp4_path)
}

/// TS→MP4 Strategy C: Full re-encode
fn ts_full_reencode_to_mp4(ts_input: &str, mp4_path: &std::path::Path) -> std::result::Result<String, String> {
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-analyzeduration", "10M",
            "-probesize", "10M",
            "-fflags", "+genpts+igndts",
            "-avoid_negative_ts", "make_zero",
            "-i", ts_input,
            "-c:v", "libx264",
            "-preset", "ultrafast",
            "-crf", "23",
            "-c:a", "aac",
            "-b:a", "128k",
            "-fps_mode", "cfr",
            "-max_muxing_queue_size", "1024",
            "-movflags", "+faststart",
            "-f", "mp4",
            &mp4_path.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("FFmpeg ts-full-reencode failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg ts-full-reencode exited with {}: {}", output.status, stderr));
    }
    validate_mp4_file_size(mp4_path)
}

/// Validate MP4 file size (rejects empty/tiny files).
fn validate_mp4_file_size(mp4_path: &std::path::Path) -> std::result::Result<String, String> {
    match std::fs::metadata(mp4_path) {
        Ok(meta) if meta.len() > 1000 => Ok(mp4_path.to_string_lossy().into_owned()),
        Ok(meta) => Err(format!("FFmpeg produced a suspiciously small file ({} bytes)", meta.len())),
        Err(e) => Err(format!("Output file not found: {}", e)),
    }
}

/// Validate an MP4 file using ffprobe for stream integrity.
///
/// Checks: file size, ffprobe exit code, video stream presence, duration sanity,
/// and stderr for any errors (at -v error level, any output indicates problems).
/// Falls back to true if ffprobe is not available (permissive).
fn validate_mp4_with_ffprobe(mp4_path: &std::path::Path) -> bool {
    // Quick size check first
    if let Ok(meta) = std::fs::metadata(mp4_path) {
        if meta.len() < 1000 {
            return false;
        }
    } else {
        return false;
    }

    // Run ffprobe to check for stream errors and get duration
    let result = std::process::Command::new("ffprobe")
        .args([
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=codec_type,duration",
            "-of", "csv=p=0",
            &mp4_path.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output();

    match result {
        Ok(output) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("validate_mp4: ffprobe exited with error: {}", stderr.chars().take(200).collect::<String>());
                return false;
            }
            // At -v error level, ANY stderr output means something is wrong
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.trim().is_empty() {
                println!("validate_mp4: ffprobe reported errors: {}", stderr.chars().take(200).collect::<String>());
                return false;
            }
            // Check stdout has a video stream with reasonable duration
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.contains("video") {
                println!("validate_mp4: no video stream found in output");
                return false;
            }
            // Check duration is > 0.5s (guard against truncated files)
            let has_valid_duration = stdout.lines().any(|line| {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 2 {
                    if let Ok(dur) = parts[1].trim().parse::<f64>() {
                        return dur > 0.5;
                    }
                }
                false
            });
            if !has_valid_duration {
                println!("validate_mp4: no valid duration found or duration too short");
                return false;
            }
            true
        }
        Err(_) => {
            // ffprobe not available — assume the file is OK (permissive)
            println!("validate_mp4: ffprobe not available, skipping validation");
            true
        }
    }
}

/// Simple hash for cache key generation.
///
/// # Tech debt
/// Despite the name `md5_hash`, this actually uses `DefaultHasher` (SipHash),
/// **not** MD5 or SHA-256. The function should be renamed to `cache_key_hash`
/// to avoid confusion. The output is a `u64`, not a 128-bit digest.
fn md5_hash(input: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}

// ═══════════════════════════════════════════════════════════════════════════════
// Cache Management
// ═══════════════════════════════════════════════════════════════════════════════
// Tech debt: No cache eviction policy — `blob_cache/` grows without bound.
// Consider an LRU strategy with a configurable size cap (e.g. 2 GB).

/// Detect file extension from the first few magic bytes of a binary payload.
///
/// Supports JPEG, PNG, WebP, GIF, MP4/MOV (ISO BMFF `ftyp`), and WebM/Matroska
/// (EBML header). Returns `"bin"` for unrecognised formats.
pub(crate) fn detect_extension_from_bytes(bytes: &[u8]) -> &'static str {
    if bytes.len() < 4 {
        return "bin";
    }
    // JPEG: FF D8 FF
    if bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return "jpg";
    }
    // PNG: 89 50 4E 47
    if bytes[0] == 0x89 && bytes[1] == 0x50 && bytes[2] == 0x4E && bytes[3] == 0x47 {
        return "png";
    }
    // WebP: RIFF....WEBP
    if bytes[0] == 0x52 && bytes[1] == 0x49 && bytes[2] == 0x46 && bytes[3] == 0x46 && bytes.len() >= 12 {
        if bytes[8] == 0x57 && bytes[9] == 0x45 && bytes[10] == 0x42 && bytes[11] == 0x50 {
            return "webp";
        }
    }
    // GIF: GIF8
    if bytes[0] == 0x47 && bytes[1] == 0x49 && bytes[2] == 0x46 {
        return "gif";
    }
    // ISO BMFF family (MP4/MOV): ....ftyp....
    if bytes.len() >= 12 && bytes[4] == 0x66 && bytes[5] == 0x74 && bytes[6] == 0x79 && bytes[7] == 0x70 {
        // Brand hint: quicktime
        if bytes.len() >= 12 && bytes[8] == 0x71 && bytes[9] == 0x74 && bytes[10] == 0x20 && bytes[11] == 0x20 {
            return "mov";
        }
        return "mp4";
    }
    // WebM / Matroska (EBML): 1A 45 DF A3
    if bytes.len() >= 4 && bytes[0] == 0x1A && bytes[1] == 0x45 && bytes[2] == 0xDF && bytes[3] == 0xA3 {
        return "webm";
    }
    "bin" // unknown
}

// ═══════════════════════════════════════════════════════════════════════════════
// Media URL Resolution
// ═══════════════════════════════════════════════════════════════════════════════

/// Retrieve the dynamically assigned port of the native Warp localhost media server.
///
/// `VideoPlayer` uses this to construct pure HTTP URLs that natively bypass
/// WebKitGTK playback bugs with `asset://` and `tauri://` schemes.
///
/// **IPC**: Frontend → `invoke('get_media_server_port')`
#[tauri::command]
pub async fn get_media_server_port(state: tauri::State<'_, AppState>) -> std::result::Result<u16, String> {
    Ok(state.media_port)
}

/// Resolve a media reference to a frontend-ready URL.
///
/// Handles all reference types:
/// - `blob:HASH` → exports to cache, returns `asset://` URL
/// - `http://` / `https://` → passthrough
/// - Absolute paths → `asset://` URL via `convertFileSrc` equivalent
/// - Relative paths (`media/foo.jpg`) → resolved against app data dir
///
/// This moves ALL path resolution logic to the Rust engine, so the
/// frontend Lens never negotiates filesystem topology.
///
/// **IPC**: Frontend → `invoke('resolve_media_url')`
#[tauri::command]
pub async fn resolve_media_url(src: String, state: State<'_, AppState>) -> std::result::Result<String, String> {
    let trimmed = src.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    // Passthrough: already a web or asset URL
    if trimmed.starts_with("http://") || trimmed.starts_with("https://")
        || trimmed.starts_with("asset://") || trimmed.starts_with("tauri://") {
        return Ok(trimmed.to_string());
    }

    // Blob reference: resolve via get_media_blob logic
    if trimmed.starts_with("blob:") {
        let after = &trimmed[5..];
        // Browser Object URLs (blob:http://...) are already resolved
        if after.starts_with("http://") || after.starts_with("https://") || after.starts_with("tauri://") {
            return Ok(trimmed.to_string());
        }
        // Delegate to the existing blob cache export
        let cache_path = get_media_blob(trimmed.to_string(), state).await?;
        // Convert to asset:// URL
        let url = tauri::Url::from_file_path(&cache_path)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| format!("file://{}", cache_path));
        return Ok(url);
    }

    // Absolute filesystem path
    if trimmed.starts_with('/') || (trimmed.len() >= 3 && trimmed.as_bytes()[1] == b':') {
        let url = tauri::Url::from_file_path(trimmed)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| format!("file://{}", trimmed));
        return Ok(url);
    }

    // Relative path — resolve against app data dir
    let app_data_dir = state.app_handle.path()
        .app_data_dir()
        .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;

    let resolved = if trimmed.starts_with("media/") {
        app_data_dir.join(trimmed)
    } else {
        app_data_dir.join("media").join(trimmed)
    };

    if resolved.exists() {
        let url = tauri::Url::from_file_path(&resolved)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| format!("file://{}", resolved.display()));
        return Ok(url);
    }

    // P0.3 fix: handle raw hash strings without blob: prefix.
    // Imported media stores blob hashes without the prefix in media_blobs.
    if crate::blob_hash::parse_blob_hash(trimmed).is_ok() {
        let cache_path = get_media_blob(trimmed.to_string(), state).await?;
        let url = tauri::Url::from_file_path(&cache_path)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| format!("file://{}", cache_path));
        return Ok(url);
    }

    // Fallback: resolve relative path even if file doesn't exist
    let url = tauri::Url::from_file_path(&resolved)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| format!("file://{}", resolved.display()));
    Ok(url)
}

/// Batch-resolve multiple media references to frontend-ready URLs in a single IPC call.
///
/// Takes a list of media source strings (blob hashes, URLs, paths) and returns
/// a map of input → resolved URL. This eliminates N sequential IPC round-trips
/// for a feed page with N images — the single biggest feed render optimization.
///
/// Entries that fail to resolve are omitted from the result (not fatal).
///
/// **IPC**: Frontend → `invoke('resolve_blob_urls_batch')`
#[tauri::command]
pub async fn resolve_blob_urls_batch(
    sources: Vec<String>,
    state: State<'_, AppState>,
) -> std::result::Result<std::collections::HashMap<String, String>, String> {
    let mut results = std::collections::HashMap::with_capacity(sources.len());

    let app_data_dir = state.app_handle.path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    let cache_dir = app_data_dir.join("blob_cache");

    for src in &sources {
        let trimmed = src.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Passthrough: already a web or asset URL
        if trimmed.starts_with("http://") || trimmed.starts_with("https://")
            || trimmed.starts_with("asset://") || trimmed.starts_with("tauri://") {
            results.insert(src.clone(), trimmed.to_string());
            continue;
        }

        // Blob reference: check cache first (no Iroh fetch in batch — too slow)
        if trimmed.starts_with("blob:") {
            let after = &trimmed[5..];
            if after.starts_with("http://") || after.starts_with("https://") || after.starts_with("tauri://") {
                results.insert(src.clone(), trimmed.to_string());
                continue;
            }
            let normalized = normalize_blob_hash_input(trimmed);
            // Fast cache check — try common extensions
            let mut found = false;
            for ext in &["jpg", "png", "webp", "gif", "mp4", "mov", "webm", "bin"] {
                let cached = cache_dir.join(format!("{}.{}", &normalized, ext));
                if cached.exists() {
                    let url = format!("http://127.0.0.1:{}/{}.{}", state.media_port, &normalized, ext);
                    results.insert(src.clone(), url);
                    found = true;
                    break;
                }
            }
            if !found {
                // Fallback: try Iroh export (expensive, but needed for uncached blobs)
                // Semaphore limits concurrent exports to 3 to prevent disk I/O saturation
                let _permit = BLOB_EXPORT_SEMAPHORE.acquire().await;
                match get_media_blob(trimmed.to_string(), state.clone()).await {
                    Ok(cache_path) => {
                        if let Some(filename) = std::path::Path::new(&cache_path).file_name() {
                            let url = format!("http://127.0.0.1:{}/{}", state.media_port, filename.to_string_lossy());
                            results.insert(src.clone(), url);
                        }
                    }
                    Err(_) => {} // skip failed blobs
                }
            }
            continue;
        }

        // Absolute filesystem path
        if trimmed.starts_with('/') || (trimmed.len() >= 3 && trimmed.as_bytes()[1] == b':') {
            if let Some(filename) = std::path::Path::new(trimmed).file_name() {
                let in_cache = cache_dir.join(filename);
                if in_cache.exists() {
                    let url = format!("http://127.0.0.1:{}/{}", state.media_port, filename.to_string_lossy());
                    results.insert(src.clone(), url);
                    continue;
                }
            }
            let url = tauri::Url::from_file_path(trimmed)
                .map(|u| u.to_string())
                .unwrap_or_else(|_| format!("file://{}", trimmed));
            results.insert(src.clone(), url);
            continue;
        }

        // Relative path — resolve against app data dir
        let resolved = if trimmed.starts_with("media/") {
            app_data_dir.join(trimmed)
        } else {
            app_data_dir.join("media").join(trimmed)
        };

        if resolved.exists() {
            if let Some(filename) = resolved.file_name() {
                let url = format!("http://127.0.0.1:{}/{}", state.media_port, filename.to_string_lossy());
                results.insert(src.clone(), url);
            }
            continue;
        }

        // P0.3 fix: handle raw hash strings without blob: prefix.
        // commit_memories stores media_blobs as raw base32/hex hashes from vault_add_file.
        // Without this, imported Instagram/Facebook media silently fails to resolve.
        if crate::blob_hash::parse_blob_hash(trimmed).is_ok() {
            let normalized = normalize_blob_hash_input(trimmed);
            let mut found = false;
            for ext in &["jpg", "png", "webp", "gif", "mp4", "mov", "webm", "bin"] {
                let cached = cache_dir.join(format!("{}.{}", &normalized, ext));
                if cached.exists() {
                    let url = format!("http://127.0.0.1:{}/{}.{}", state.media_port, &normalized, ext);
                    results.insert(src.clone(), url);
                    found = true;
                    break;
                }
            }
            if !found {
                // Export from Iroh to cache (semaphore-gated, max 3 concurrent)
                let _permit = BLOB_EXPORT_SEMAPHORE.acquire().await;
                match get_media_blob(trimmed.to_string(), state.clone()).await {
                    Ok(cache_path) => {
                        if let Some(filename) = std::path::Path::new(&cache_path).file_name() {
                            let url = format!("http://127.0.0.1:{}/{}", state.media_port, filename.to_string_lossy());
                            results.insert(src.clone(), url);
                        }
                    }
                    Err(_) => {} // skip failed blobs
                }
            }
        }
    }

    // P0.7: Memory guard logging — report how many items needed Iroh export
    let uncached_count = sources.len().saturating_sub(results.len());
    if uncached_count > 0 || sources.len() > 10 {
        println!("resolve_blob_urls_batch: {} sources, {} resolved, {} required Iroh export",
            sources.len(), results.len(), uncached_count);
    }

    Ok(results)
}
