//! Media & Blob Commands
//! 
//! Commands for storing and retrieving media blobs in Iroh.

use bytes::Bytes;
use serde::Serialize;
use tauri::{State, Manager};

use crate::error::BytescapeError;
use crate::sync::Hash;
use crate::blob_hash;
use crate::AppState;

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

fn normalize_blob_hash_input(input: &str) -> String {
    blob_hash::normalize_blob_ref(input)
}

fn decode_base32_hash_like(input: &str) -> Option<Hash> {
    blob_hash::parse_blob_hash(input).ok()
}

fn parse_hash_safe(input: &str) -> std::result::Result<Hash, String> {
    blob_hash::parse_blob_hash(input)
}



/// Store media data as a blob in Iroh.
/// 
/// Optionally generates a WebP thumbnail for images.
/// Returns the content hash (and thumbnail hash if generated).
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

/// Download a media blob from a remote peer via the iroh-blobs protocol.
///
/// Creates an ephemeral [`Downloader`] from the local blob store + endpoint,
/// then fetches the blob identified by `hash_str` from the peer at `node_id`.
/// Once complete the blob lives in the local blob store and can be read by
/// [`get_media_blob`] as if it were a local blob.
///
/// Returns `true` when the blob was successfully downloaded (or already present).
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
#[tauri::command]
pub async fn get_media_blob(hash_str: String, state: State<'_, AppState>) -> std::result::Result<String, String> {
    let normalized_hash = normalize_blob_hash_input(&hash_str);

    let app_data_dir = state.app_handle.path()
        .app_data_dir()
        .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
    let cache_dir = app_data_dir.join("blob_cache");

    // Check if cached file already exists (try common extensions)
    for ext in &["mp4", "mov", "webm", "mkv", "jpg", "png", "webp", "gif", "bin"] {
        let cached = cache_dir.join(format!("{}.{}", &normalized_hash, ext));
        if cached.exists() {
            if let Ok(bytes) = std::fs::read(&cached) {
                let detected_ext = detect_extension_from_bytes(&bytes);
                let current_ext = cached.extension().and_then(|e| e.to_str()).unwrap_or("");
                if detected_ext != current_ext {
                    let corrected = cache_dir.join(format!("{}.{}", &normalized_hash, detected_ext));
                    let _ = std::fs::write(&corrected, &bytes);
                    return Ok(corrected.to_string_lossy().to_string());
                }
            }
            return Ok(cached.to_string_lossy().to_string());
        }
    }

    // Not cached — fetch from Iroh blob store
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;

    let hash = parse_hash_safe(&normalized_hash)?;

    let bytes = sync.get_media(hash).await
        .map_err(|e| BytescapeError::BlobStorage(e))?;

    // Detect extension from magic bytes
    let ext = detect_extension_from_bytes(&bytes);

    // Write to cache file
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
    let cache_path = cache_dir.join(format!("{}.{}", &normalized_hash, ext));
    std::fs::write(&cache_path, &bytes)
        .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;

    Ok(cache_path.to_string_lossy().to_string())
}

/// Retrieve media blob bytes directly for browser Object URL creation.
///
/// Returns Vec<u8> via IPC. For vault videos (< 8MB), JSON overhead is acceptable.
/// For large HLS videos, use resolve_hls_video which returns tauri::ipc::Response.
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

    // Not cached — fetch from Iroh blob store
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;

    let hash = parse_hash_safe(&normalized_hash)?;

    let bytes = sync.get_media(hash).await
        .map_err(|e| BytescapeError::BlobStorage(e))?;

    // Cache to disk for future use
    let ext = detect_extension_from_bytes(&bytes);
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
    let cache_path = cache_dir.join(format!("{}.{}", &normalized_hash, ext));
    let _ = std::fs::write(&cache_path, &bytes);

    println!("get_media_blob_bytes: fetched from Iroh, cached as {} ({} bytes)", ext, bytes.len());
    Ok(tauri::ipc::Response::new(bytes.to_vec()))
}

/// Get the media directory path for storing imported files.
#[tauri::command]
pub async fn get_media_dir(state: State<'_, AppState>) -> std::result::Result<String, String> {
    let app_data_dir = state.app_handle.path()
        .app_data_dir()
        .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
    let media_dir = app_data_dir.join("media");
    Ok(media_dir.to_string_lossy().to_string())
}

/// Diagnose media reference resolution and playback readiness.
/// Useful for debugging blob/hash/path issues when media appears but won't play.
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

    match sync.get_media(hash).await {
        Ok(bytes) => {
            let ext = detect_extension_from_bytes(&bytes).to_string();
            let _ = std::fs::create_dir_all(&cache_dir);
            let cache_path = cache_dir.join(format!("{}.{}", &normalized, ext));
            let _ = std::fs::write(&cache_path, &bytes);

            Ok(MediaRefDiagnostic {
                input,
                mode: "blob".to_string(),
                normalized,
                hash_parsed: true,
                fetched_bytes: bytes.len(),
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

/// Backfill `media_mimes` for existing BytescapeItems that have media_blobs
/// but no MIME metadata yet (records created before `media_mimes` was added).
///
/// Reads the first few bytes of each blob to detect its MIME type from magic
/// bytes, then updates the item in the database.  Safe to run more than once:
/// items that already have the correct `media_mimes` length are skipped.
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

            // Try to parse and fetch blob for MIME detection
            let hash_opt = decode_base32_hash_like(&hash_str);
            if let Some(hash) = hash_opt {
                match sync.get_media(hash).await {
                    Ok(bytes) => {
                        mimes.push(mime_from_magic_bytes(bytes.as_ref()));
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

/// Download an HLS video, returning a local MP4 file path.
///
/// Strategy (in order):
/// 1. Cache hit — return existing MP4 immediately
/// 2. FFmpeg native HLS download — handles separate audio/video tracks, timestamp
///    correction, and remuxing to MP4 in a single pass (most robust)
/// 3. Manual segment download + FFmpeg remux — fallback if native HLS fails
/// 4. Raw `.ts` — last resort if FFmpeg is entirely unavailable
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

    // ── Cache hit — prefer MP4 ──────────────────────────────────────────────
    if mp4_path.exists() {
        if let Ok(meta) = std::fs::metadata(&mp4_path) {
            if meta.len() > 1000 {
                println!("resolve_hls_video: MP4 cache hit ({:.1} MB)", meta.len() as f64 / (1024.0 * 1024.0));
                return Ok(mp4_path.to_string_lossy().into_owned());
            }
        }
    }
    // Stale TS without MP4 — try remuxing it
    if ts_path.exists() {
        if let Ok(meta) = std::fs::metadata(&ts_path) {
            if meta.len() > 1000 {
                if let Ok(remuxed) = remux_ts_to_mp4(&ts_path, &mp4_path) {
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

    // ── Strategy 1: FFmpeg native HLS → MP4 ─────────────────────────────────
    // FFmpeg's HLS demuxer handles: separate audio/video playlists, variant
    // selection, timestamp correction at segment boundaries, ADTS→raw AAC, etc.
    println!("resolve_hls_video: Trying FFmpeg native HLS download...");
    let playlist_for_ffmpeg = playlist_url.clone();
    let mp4_for_ffmpeg = mp4_path.clone();
    let ts_for_cleanup = ts_path.clone();
    match tokio::task::spawn_blocking(move || ffmpeg_hls_to_mp4(&playlist_for_ffmpeg, &mp4_for_ffmpeg)).await {
        Ok(Ok(path)) => {
            println!("resolve_hls_video: FFmpeg HLS → MP4 success ({:.1} MB)",
                std::fs::metadata(&mp4_path).map(|m| m.len()).unwrap_or(0) as f64 / (1024.0 * 1024.0));
            let _ = std::fs::remove_file(&ts_for_cleanup); // clean any stale TS
            return Ok(path);
        }
        Ok(Err(e)) => {
            println!("resolve_hls_video: FFmpeg HLS failed ({}), falling back to manual download...", e);
        }
        Err(e) => {
            println!("resolve_hls_video: FFmpeg task panicked ({}), falling back to manual download...", e);
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

    // Remux concatenated MPEG-TS → MP4 (fixes audio + timestamp issues)
    let ts_for_remux = ts_path.clone();
    let mp4_for_remux = mp4_path.clone();
    match tokio::task::spawn_blocking(move || remux_ts_to_mp4(&ts_for_remux, &mp4_for_remux)).await {
        Ok(Ok(mp4)) => {
            println!("resolve_hls_video: Remuxed to MP4 ({:.1} MB)",
                std::fs::metadata(&mp4_path).map(|m| m.len()).unwrap_or(0) as f64 / (1024.0 * 1024.0));
            Ok(mp4)
        }
        Ok(Err(e)) => {
            println!("resolve_hls_video: FFmpeg remux failed ({}), returning raw TS", e);
            Ok(ts_path.to_string_lossy().into_owned())
        }
        Err(e) => {
            println!("resolve_hls_video: Remux task panicked ({}), returning raw TS", e);
            Ok(ts_path.to_string_lossy().into_owned())
        }
    }
}

/// Download an HLS stream directly via FFmpeg's native protocol handler.
///
/// Uses a lightweight video re-encode (`libx264 ultrafast crf 23`) instead of
/// stream copy to force FFmpeg to regenerate all timestamps from scratch. This
/// eliminates PTS discontinuities at HLS segment boundaries that cause stutter
/// with `-c copy`. Audio is copied (AAC passthrough) since it doesn't suffer
/// from the same discontinuity issues.
fn ffmpeg_hls_to_mp4(playlist_url: &str, mp4_path: &std::path::Path) -> std::result::Result<String, String> {
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-y",                          // overwrite without asking
            "-i", playlist_url,            // HLS URL — FFmpeg handles the protocol
            "-c:v", "libx264",             // re-encode video to fix PTS discontinuities
            "-preset", "ultrafast",        // fastest encoding — minimal CPU overhead
            "-crf", "23",                  // visually lossless quality
            "-c:a", "aac",                 // ensure audio is clean AAC
            "-b:a", "128k",               // reasonable audio bitrate
            "-fps_mode", "cfr",            // force constant frame rate output
            "-max_muxing_queue_size", "1024", // prevent interleave buffer overflow
            "-movflags", "+faststart",     // moov atom at front for instant playback
            "-f", "mp4",                   // force MP4 container output
            &mp4_path.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("FFmpeg not found or failed to execute: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg exited with {}: {}", output.status, stderr));
    }

    // Verify the output is a valid MP4 (not an error page or empty file)
    match std::fs::metadata(mp4_path) {
        Ok(meta) if meta.len() > 1000 => Ok(mp4_path.to_string_lossy().into_owned()),
        Ok(meta) => Err(format!("FFmpeg produced a suspiciously small file ({} bytes)", meta.len())),
        Err(e) => Err(format!("Output file not found: {}", e)),
    }
}

/// Re-encode an MPEG-TS file to MP4 using FFmpeg.
///
/// Uses lightweight re-encode to fix all timestamp issues from HLS segments:
/// - `-c:v libx264 -preset ultrafast -crf 23`: Re-encode video with fresh PTS
/// - `-c:a aac -b:a 128k`: Clean AAC audio
/// - `-fps_mode cfr`: Force constant frame rate to eliminate jitter
/// - `-fflags +genpts+discardcorrupt`: Extra safety for corrupt segment data
/// - `-avoid_negative_ts make_zero`: Normalize timestamps to start at 0
/// - `-movflags +faststart`: Move moov atom to file front for instant playback
fn remux_ts_to_mp4(ts_path: &std::path::Path, mp4_path: &std::path::Path) -> std::result::Result<String, String> {
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-y",                             // overwrite without asking
            "-fflags", "+genpts+discardcorrupt", // regenerate timestamps, drop corrupt
            "-avoid_negative_ts", "make_zero", // normalize timestamps to start at 0
            "-i", &ts_path.to_string_lossy(),
            "-c:v", "libx264",                // re-encode video for clean PTS
            "-preset", "ultrafast",           // fastest encoding
            "-crf", "23",                     // visually lossless quality
            "-c:a", "aac",                    // clean AAC audio
            "-b:a", "128k",                   // reasonable audio bitrate
            "-fps_mode", "cfr",               // constant frame rate output
            "-max_muxing_queue_size", "1024", // prevent interleave buffer overflow
            "-movflags", "+faststart",         // moov atom at front
            "-f", "mp4",                       // force MP4 container
            &mp4_path.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("FFmpeg not found or failed to execute: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg exited with {}: {}", output.status, stderr));
    }

    // Verify output file is valid
    match std::fs::metadata(mp4_path) {
        Ok(meta) if meta.len() > 1000 => {},
        Ok(meta) => return Err(format!("FFmpeg produced a suspiciously small file ({} bytes)", meta.len())),
        Err(e) => return Err(format!("Output file not found: {}", e)),
    }

    // Clean up the intermediate TS file to save disk space
    let _ = std::fs::remove_file(ts_path);

    Ok(mp4_path.to_string_lossy().into_owned())
}

/// Simple hash for cache key generation
fn md5_hash(input: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}



/// Detect file extension from magic bytes
fn detect_extension_from_bytes(bytes: &[u8]) -> &'static str {
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

/// Retrieve the dynamically assigned port of the native Warp localhost media server.
/// VideoPlayer uses this to construct pure HTTP URLs that natively bypass WebKitGTK drop-dead bugs.
#[tauri::command]
pub async fn get_media_server_port(state: tauri::State<'_, AppState>) -> std::result::Result<u16, String> {
    Ok(state.media_port)
}

/// Resolve a media reference to a frontend-ready URL.
///
/// Handles all reference types:
/// - `blob:HASH` → exports to cache, returns asset:// URL
/// - `http://` / `https://` → passthrough
/// - Absolute paths → asset:// URL via convertFileSrc equivalent
/// - Relative paths (`media/foo.jpg`) → resolved against app data dir
///
/// This moves ALL path resolution logic to the Rust engine, so the
/// frontend Lens never negotiates filesystem topology.
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
        }
    }

    Ok(results)
}
