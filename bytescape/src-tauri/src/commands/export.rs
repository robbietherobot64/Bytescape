//! Vault data export to ZIP archive.
//!
//! Exports the user's entire vault — items, contacts, messages, and optionally
//! cached media files — into a single portable ZIP file. This enables backup,
//! migration, and data portability.
//!
//! ## HOW IT WORKS
//! 1. Frontend invokes `export_vault_data` via Tauri IPC with an output path
//! 2. All `BytescapeItem` records are loaded from redb (up to 100k hard limit)
//! 3. Contacts and vault messages are collected from their respective tables
//! 4. Data is serialized to pretty-printed JSON and written into a Deflate-compressed ZIP
//! 5. If `include_media` is true, blob cache and local media files are added under `media/`
//!
//! ## RELATED FILES
//! - `db.rs` / `db/vault.rs` — `list_my_items_paged`, `list_contacts`, `list_vault_messages`
//! - `types/index.ts` — TypeScript `ExportResult` interface mirrors this struct
//! - `src/screens/SettingsScreen.tsx` — UI that triggers export via file-save dialog
//!
//! ## KNOWN LIMITATIONS
//! ⚠ Memory: ALL items are loaded into memory before ZIP creation
//! (`list_my_items_paged` with limit 100,000). For vaults with tens of thousands
//! of media-heavy items, this could cause significant memory pressure. A streaming
//! serialization approach would be preferable for large vaults.

use tauri::{State, Manager};
use crate::AppState;
use serde::{Serialize, Deserialize};
use std::io::Write;

/// Summary of a completed vault export operation.
///
/// Returned to the frontend after `export_vault_data` completes successfully,
/// providing statistics about what was exported and the final archive size.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportResult {
    pub path: String,
    pub item_count: usize,
    pub contact_count: usize,
    pub message_count: usize,
    pub media_count: usize,
    pub size_bytes: u64,
}

/// Export all vault data as a ZIP archive.
///
/// Includes:
/// - `items.json` — all BytescapeItem records
/// - `contacts.json` — all contacts
/// - `messages.json` — all vault messages
/// - `media/` — cached media files (if include_media is true)
#[tauri::command]
pub async fn export_vault_data(
    output_path: String,
    include_media: bool,
    state: State<'_, AppState>,
) -> Result<ExportResult, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    let owner_did = match state.keychain.load_session() {
        Ok(Some((did, _))) => did,
        _ => return Err("No active session".to_string()),
    };

    // ── 1. Collect items ──
    // NOTE: Loads ALL items into memory at once. For large vaults this could be
    // several hundred MB. Consider streaming serialization for future improvement.
    let (items, _total) = db.list_my_items_paged(&owner_did, None, 100_000, 0).await?;
    let item_count = items.len();

    // ── 2. Collect contacts ──
    let contacts = db.list_contacts().await.unwrap_or_default();
    let contact_count = contacts.len();

    // ── 3. Collect messages ──
    let messages = db.list_vault_messages(None).await.unwrap_or_default();
    let message_count = messages.len();

    // ── 4. Build ZIP ──
    let file = std::fs::File::create(&output_path)
        .map_err(|e| format!("Failed to create output file: {}", e))?;
    let mut zip = zip::ZipWriter::new(file);

    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // Items JSON
    let items_json = serde_json::to_string_pretty(&items)
        .map_err(|e| format!("Failed to serialize items: {}", e))?;
    zip.start_file("items.json", options)
        .map_err(|e| format!("ZIP error: {}", e))?;
    zip.write_all(items_json.as_bytes())
        .map_err(|e| format!("ZIP write error: {}", e))?;

    // Contacts JSON
    let contacts_json = serde_json::to_string_pretty(&contacts)
        .map_err(|e| format!("Failed to serialize contacts: {}", e))?;
    zip.start_file("contacts.json", options)
        .map_err(|e| format!("ZIP error: {}", e))?;
    zip.write_all(contacts_json.as_bytes())
        .map_err(|e| format!("ZIP write error: {}", e))?;

    // Messages JSON
    let messages_json = serde_json::to_string_pretty(&messages)
        .map_err(|e| format!("Failed to serialize messages: {}", e))?;
    zip.start_file("messages.json", options)
        .map_err(|e| format!("ZIP error: {}", e))?;
    zip.write_all(messages_json.as_bytes())
        .map_err(|e| format!("ZIP write error: {}", e))?;

    // ── 5. Optional media files ──
    let mut media_count = 0usize;
    if include_media {
        let app_data_dir = state.app_handle.path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data dir: {}", e))?;
        let blob_cache_dir = app_data_dir.join("blob_cache");
        let media_dir = app_data_dir.join("media");

        // Collect all unique blob hashes referenced by items
        let mut blob_refs: std::collections::HashSet<String> = std::collections::HashSet::new();
        for item in &items {
            for blob_ref in &item.media_blobs {
                let normalized = if blob_ref.starts_with("blob:") {
                    blob_ref.strip_prefix("blob:").unwrap_or(blob_ref).to_string()
                } else {
                    blob_ref.clone()
                };
                blob_refs.insert(normalized);
            }
        }

        // Try to include cached blob files
        if blob_cache_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&blob_cache_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        let stem = path.file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("");
                        if blob_refs.contains(stem) {
                            if let Ok(data) = std::fs::read(&path) {
                                let filename = format!("media/{}", path.file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("unknown"));
                                zip.start_file(&filename, options)
                                    .map_err(|e| format!("ZIP error: {}", e))?;
                                zip.write_all(&data)
                                    .map_err(|e| format!("ZIP write error: {}", e))?;
                                media_count += 1;
                            }
                        }
                    }
                }
            }
        }

        // Also include local media files referenced by path
        for item in &items {
            for media_path in &item.media_paths {
                let path = std::path::Path::new(media_path);
                if path.exists() && path.is_file() {
                    if let Ok(data) = std::fs::read(path) {
                        let filename = format!("media/{}", path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown"));
                        // Avoid duplicates in zip
                        if zip.start_file(&filename, options).is_ok() {
                            let _ = zip.write_all(&data);
                            media_count += 1;
                        }
                    }
                }
            }
        }

        // Include files from media dir
        if media_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&media_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Ok(data) = std::fs::read(&path) {
                            let filename = format!("media/{}", path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("unknown"));
                            if zip.start_file(&filename, options).is_ok() {
                                let _ = zip.write_all(&data);
                                media_count += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    zip.finish().map_err(|e| format!("ZIP finalize error: {}", e))?;

    // Get final file size
    let metadata = std::fs::metadata(&output_path)
        .map_err(|e| format!("Failed to read output file metadata: {}", e))?;

    Ok(ExportResult {
        path: output_path,
        item_count,
        contact_count,
        message_count,
        media_count,
        size_bytes: metadata.len(),
    })
}
