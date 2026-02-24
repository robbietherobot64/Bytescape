//! Media & Blob Commands
//! 
//! Commands for storing and retrieving media blobs in Iroh.

use bytes::Bytes;
use data_encoding::BASE32_NOPAD;
use serde::Serialize;
use tauri::{State, Manager};

use crate::error::BytescapeError;
use crate::sync::Hash;
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
    let mut s = input.trim().to_string();
    while let Some(rest) = s.strip_prefix("blob:") {
        s = rest.trim().to_string();
    }
    // Strip optional ":thumbhash" suffix
    if let Some(pos) = s.find(':') {
        s = s[..pos].to_string();
    }
    s
}

fn decode_base32_hash_like(input: &str) -> Option<Hash> {
    let mut candidates = vec![input.trim()];
    if let Some(stripped) = input.trim().strip_prefix('b') {
        candidates.push(stripped.trim());
    }

    for candidate in candidates {
        if candidate.is_empty() {
            continue;
        }

        let uppercase = candidate.to_ascii_uppercase();
        let Ok(decoded) = BASE32_NOPAD.decode(uppercase.as_bytes()) else {
            continue;
        };

        if decoded.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&decoded);
            return Some(Hash::from_bytes(arr));
        }

        // CIDv1/multihash-style payloads are commonly 36 bytes for blake3-256.
        if decoded.len() >= 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&decoded[decoded.len() - 32..]);
            return Some(Hash::from_bytes(arr));
        }
    }

    None
}

fn parse_hash_safe(input: &str) -> std::result::Result<Hash, String> {
    let normalized = normalize_blob_hash_input(input);
    if normalized.is_empty() {
        return Err("Invalid blob hash format".to_string());
    }

    // Preferred path: hex/base32 handled by Hash::from_str for valid raw digests.
    if normalized.len() == 64 && normalized.chars().all(|c| c.is_ascii_hexdigit()) {
        let parse_result = std::panic::catch_unwind(|| normalized.parse::<Hash>());
        return match parse_result {
            Ok(Ok(hash)) => Ok(hash),
            Ok(Err(e)) => Err(BytescapeError::BlobStorage(e.to_string()).to_string()),
            Err(_) => Err("Invalid blob hash format".to_string()),
        };
    }

    if let Some(hash) = decode_base32_hash_like(&normalized) {
        return Ok(hash);
    }

    Err("Invalid blob hash format".to_string())
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
    let db_guard = state.db_manager.lock().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| crate::error::BytescapeError::DatabaseNotInitialized)?;
    let sync_guard = state.sync_manager.lock().await;
    let sync = match sync_guard.as_ref() {
        Some(s) => s.clone(),
        None => return Ok(0), // No sync manager yet, nothing to backfill
    };
    drop(sync_guard);

    let items = db.list_items().await.map_err(|e| e.to_string())?;
    let mut updated = 0usize;

    for mut item in items {
        // Skip items that already have correct MIME metadata
        if !item.media_blobs.is_empty() && item.media_mimes.len() == item.media_blobs.len() {
            continue;
        }
        if item.media_blobs.is_empty() {
            continue;
        }

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
                        let hint = item.media_paths.get(idx).map(|s| s.as_str()).unwrap_or("");
                        mimes.push(mime_from_path_hint(hint));
                    }
                }
            } else {
                let idx = mimes.len();
                let hint = item.media_paths.get(idx).map(|s| s.as_str()).unwrap_or("");
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

/// Detect MIME type from magic bytes (used by backfill)
pub fn mime_from_magic_bytes(data: &[u8]) -> String {
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg".to_string()
    } else if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png".to_string()
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        "image/gif".to_string()
    } else if data.starts_with(b"RIFF") && data.len() > 12 && &data[8..12] == b"WEBP" {
        "image/webp".to_string()
    } else if data.starts_with(b"%PDF-") {
        "application/pdf".to_string()
    } else if data.len() > 12 && &data[4..8] == b"ftyp" {
        "video/mp4".to_string()
    } else if data.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        "video/webm".to_string()
    } else if data.starts_with(b"OggS") {
        "video/ogg".to_string()
    } else {
        "application/octet-stream".to_string()
    }
}

/// Detect MIME type from file path/URI hint
pub fn mime_from_path_hint(path: &str) -> String {
    let lower = path.to_lowercase();
    let clean = lower.split('?').next().unwrap_or(&lower);
    let clean = clean.split('#').next().unwrap_or(clean);
    if clean.ends_with(".jpg") || clean.ends_with(".jpeg") {
        "image/jpeg".to_string()
    } else if clean.ends_with(".png") {
        "image/png".to_string()
    } else if clean.ends_with(".webp") {
        "image/webp".to_string()
    } else if clean.ends_with(".gif") {
        "image/gif".to_string()
    } else if clean.ends_with(".mp4") || clean.ends_with(".m4v") {
        "video/mp4".to_string()
    } else if clean.ends_with(".mov") {
        "video/quicktime".to_string()
    } else if clean.ends_with(".webm") {
        "video/webm".to_string()
    } else if clean.ends_with(".mkv") {
        "video/x-matroska".to_string()
    } else if clean.ends_with(".ts") || clean.ends_with(".mpg") {
        "video/mp2t".to_string()
    } else if clean.ends_with(".pdf") {
        "application/pdf".to_string()
    } else {
        "application/octet-stream".to_string()
    }
}

/// Download an HLS video by fetching its m3u8 playlist and all segments server-side.
///
/// Returns the filename of the cached `.ts` file. This allows the frontend to stream it 
/// via `bytescape-video://` which provides perfect HTTP Range support for scrubbing.
#[tauri::command]
pub async fn resolve_hls_video(playlist_url: String, state: tauri::State<'_, AppState>) -> std::result::Result<String, String> {
    use std::io::Write;

    let app_data_dir: std::path::PathBuf = state.app_handle.path().app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    let cache_dir = app_data_dir.join("blob_cache");
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("Failed to create cache dir: {}", e))?;

    let url_hash = format!("{:x}", md5_hash(&playlist_url));
    let filename = format!("{}.ts", url_hash);
    let file_path = cache_dir.join(&filename);

    // Cache hit — return filename directly
    if file_path.exists() {
        if let Ok(meta) = std::fs::metadata(&file_path) {
            if meta.len() > 1000 {
                println!("resolve_hls_video: Cache hit ({:.1} MB)", meta.len() as f64 / (1024.0 * 1024.0));
                return Ok(file_path.to_string_lossy().into_owned());
            }
        }
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

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
    let mut file = std::fs::File::create(&file_path)
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

    println!("resolve_hls_video: Download complete, total {:.1} MB", total_bytes as f64 / (1024.0 * 1024.0));
    Ok(file_path.to_string_lossy().into_owned())
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
