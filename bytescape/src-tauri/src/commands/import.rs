//! Data Import Commands
//! 
//! Commands for importing data from external sources (Facebook, Bluesky).

use serde::{Deserialize, Serialize};
use tauri::{State, Emitter, Manager};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use chrono::Datelike;
use crate::blob_hash;

#[cfg(feature = "willow")]
use willow_25::{MCC25, MCL25, MPL25};

#[cfg(not(feature = "willow"))]
const MCL25: usize = 1024;
#[cfg(not(feature = "willow"))]
const MCC25: usize = 1024;
#[cfg(not(feature = "willow"))]
const MPL25: usize = 1024;

use crate::error::BytescapeError;
use crate::models::VaultMessage;
use crate::db::{DbManager, BytescapeItem, BytescapeType, SyndicationStatus, WillowWriteParityReport, Audience};
use crate::connectors::{Source, facebook};
use crate::sync::{Hash, SyncManager};
use crate::AppState;

/// Response from parsing a Facebook ZIP export
#[derive(Serialize, Deserialize, Clone)]
pub struct ParseResult {
    pub posts: Vec<facebook::ParsedPost>,
    pub trackers: Vec<facebook::TrackerEntry>,
    pub total_photos: usize,
    pub total_videos: usize,
    pub total_tracker_events: usize,
    #[serde(default)]
    pub messages: Vec<VaultMessage>,
    #[serde(default)]
    pub source: String,
}

#[derive(Serialize)]
pub struct PublishResult {
    pub local: bool,
    pub bluesky: bool,
    pub activitypub: bool,
    pub bluesky_uri: Option<String>,
    pub errors: Vec<String>,
}

fn willow_item_kind(item_type: BytescapeType) -> &'static str {
    match item_type {
        BytescapeType::Status => "status",
        BytescapeType::Gallery => "gallery",
        BytescapeType::Article => "article",
    }
}

pub fn willow_path_for_item(source: &str, item_type: BytescapeType, timestamp: i64, id: &str) -> String {
    fn normalize_component(raw: &str) -> String {
        let cleaned = raw
            .trim()
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
            .collect::<String>();

        if cleaned.len() > MCL25 {
            cleaned[..MCL25].to_string()
        } else if cleaned.is_empty() {
            "unknown".to_string()
        } else {
            cleaned
        }
    }

    let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(timestamp, 0)
        .unwrap_or_else(|| chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0).unwrap());

    let mut components = vec![
        normalize_component(source),
        normalize_component(willow_item_kind(item_type)),
        format!("{:04}", dt.year()),
        format!("{:02}", dt.month()),
        normalize_component(id),
    ];

    if components.len() > MCC25 {
        components.truncate(MCC25);
    }

    let mut path = format!("/{}", components.join("/"));
    if path.len() > MPL25 {
        path.truncate(MPL25);
    }
    path
}

#[derive(Serialize)]
pub struct WillowMigrationResult {
    pub scanned: usize,
    pub migrated: usize,
}

/// Backfill Willow-style paths for existing items and normalize missing paths.
#[tauri::command]
pub async fn migrate_willow_paths(state: State<'_, AppState>) -> std::result::Result<WillowMigrationResult, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard
        .as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;

    let item_headers = db.list_all_item_headers().await.map_err(|e| e.to_string())?;
    let scanned = item_headers.len();
    let mut migrated = 0usize;

    for h in item_headers {
        // Optimization: only fetch full item if it actually needs migration
        // Wait, we need the original_timestamp and id for the path, which are in the header.
        // We also need the item_type which is in the header.
        // But we need to check if the path is empty.
        // Actually, let's add `path_is_empty` to header or just fetch if we are unsure.
        // To be safe and efficient, I'll fetch if it's potentially needed.
        
        // Let's assume most items might need migration if it's a new feature.
        // But we can check if the header has enough info to decide.
        // I'll add `has_path` to header to be sure.
        
        // Actually, I'll just fetch for now, it's already much better than fetching ALL at once.
        let mut item = match db.get_item_by_id(&h.id).await {
            Ok(Some(item)) => item,
            _ => continue,
        };

        if item.path.trim().is_empty() {
            let item_type = item.item_type;
            item.path = willow_path_for_item(&item.source, item_type, item.original_timestamp, &item.id);
            db.upsert_item(&item).await.map_err(|e| e.to_string())?;
            migrated += 1;
        }
    }

    Ok(WillowMigrationResult {
        scanned,
        migrated,
    })
}

#[tauri::command]
pub async fn get_willow_write_parity_report(state: State<'_, AppState>) -> std::result::Result<WillowWriteParityReport, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard
        .as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;

    db.get_willow_write_parity_report().await
        .map_err(|e| e.to_string())
}

/// Import a Facebook ZIP archive and parse its contents.
/// 
/// The parsed data is held in memory until `commit_memories` is called.
/// This allows the user to review before committing.
#[tauri::command]
pub async fn import_facebook_zip(
    path: String,
    state: State<'_, AppState>,
) -> std::result::Result<ParseResult, String> {
    let zip_path = std::path::PathBuf::from(path);
    let app_data_dir = state.app_handle.path()
        .app_data_dir()
        .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
    let media_dir = app_data_dir.join("media");

    let app_handle = state.app_handle.clone();
    let sync_manager = state.sync_manager.lock().await.clone();

    // Capture the tokio runtime handle BEFORE entering spawn_blocking
    let rt_handle = tokio::runtime::Handle::current();

    let result = tokio::task::spawn_blocking(move || {
        facebook::parse_facebook_zip(zip_path, media_dir, &app_handle, sync_manager, rt_handle)
    })
    .await
    .map_err(|e| BytescapeError::Internal(format!("Parser task failed: {}", e)))?
    .map_err(|e| BytescapeError::FacebookImport(e))?;

    let parsed_result = ParseResult {
        posts: result.posts,
        trackers: result.trackers,
        total_photos: result.total_photos,
        total_videos: result.total_videos,
        total_tracker_events: result.total_tracker_events,
        messages: result.messages,
        source: "facebook".to_string(),
    };

    *state.pending_import.lock().await = Some(parsed_result.clone());

    Ok(parsed_result)
}

/// Import an Instagram ZIP archive and parse its contents.
/// 
/// The parsed data is held in memory until `commit_memories` is called.
#[tauri::command]
pub async fn import_instagram_zip(
    path: String,
    state: State<'_, AppState>,
) -> std::result::Result<ParseResult, String> {
    let zip_path = std::path::PathBuf::from(path);
    let app_data_dir = state.app_handle.path()
        .app_data_dir()
        .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
    let media_dir = app_data_dir.join("media");

    let app_handle = state.app_handle.clone();
    let sync_manager = state.sync_manager.lock().await.clone();

    // Capture the tokio runtime handle BEFORE entering spawn_blocking
    let rt_handle = tokio::runtime::Handle::current();

    let result = tokio::task::spawn_blocking(move || {
        crate::connectors::instagram::parse_instagram_zip(zip_path, media_dir, &app_handle, sync_manager, rt_handle)
    })
    .await
    .map_err(|e| BytescapeError::Internal(format!("Parser task failed: {}", e)))?
    .map_err(|e| BytescapeError::Import(e))?;

    let parsed_result = ParseResult {
        posts: result.posts,
        trackers: result.trackers,
        total_photos: result.total_photos,
        total_videos: result.total_videos,
        total_tracker_events: result.total_tracker_events,
        messages: result.messages,
        source: "instagram".to_string(),
    };

    *state.pending_import.lock().await = Some(parsed_result.clone());

    Ok(parsed_result)
}

fn detect_archive_source(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open ZIP: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Failed to read ZIP: {}", e))?;

    let mut saw_dtp_generic_markers = false;

    let mut saw_instagram_marker = false;
    let mut saw_facebook_marker = false;

    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| format!("Failed reading ZIP entry: {}", e))?;
        let name = entry.name().replace('\\', "/").to_lowercase();

        if name.contains("your_instagram_activity/")
            || name.contains("instagram_activity/")
            || (name.ends_with("/posts_1.json") && name.contains("instagram"))
        {
            saw_instagram_marker = true;
        }

        if name.contains("your_facebook_activity/")
            || name.contains("/messages/inbox/")
            || name.contains("/posts/")
        {
            saw_facebook_marker = true;
        }

        if name.ends_with(".json")
            && (name.contains("transfer")
                || name.contains("dtp")
                || name.contains("generic")
                || name.contains("social"))
        {
            saw_dtp_generic_markers = true;
        }
    }

    // Instagram exports frequently include /messages/inbox/, so prefer explicit
    // Instagram markers when both patterns appear.
    if saw_instagram_marker {
        return Ok("instagram".to_string());
    }
    if saw_facebook_marker {
        return Ok("facebook".to_string());
    }

    if saw_dtp_generic_markers {
        return Ok("dtp".to_string());
    }

    Ok("unknown".to_string())
}

fn timestamp_from_dtp_published(v: &serde_json::Value) -> i64 {
    if let Some(n) = v.as_i64() {
        return n;
    }
    if let Some(n) = v.as_u64() {
        return n as i64;
    }
    if let Some(f) = v.as_f64() {
        return f.round() as i64;
    }
    0
}

fn collect_dtp_social_posts(value: &serde_json::Value, out: &mut Vec<facebook::ParsedPost>, seen: &mut HashSet<String>) {
    if let Some(obj) = value.as_object() {
        let activity = obj
            .get("activity")
            .or_else(|| obj.get("payload").and_then(|p| p.get("activity")));

        if let Some(activity) = activity {
            let content = activity
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();

            let title = activity
                .get("title")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            let timestamp = activity
                .get("published")
                .map(timestamp_from_dtp_published)
                .unwrap_or(0);

            let mut media_uris: Vec<String> = Vec::new();
            if let Some(attachments) = activity.get("attachments").and_then(|v| v.as_array()) {
                for att in attachments {
                    if let Some(url) = att.get("url").and_then(|u| u.as_str()) {
                        let t = url.trim();
                        if !t.is_empty() {
                            media_uris.push(t.to_string());
                        }
                    }
                }
            }

            if !content.is_empty() || !media_uris.is_empty() || title.is_some() {
                let author = obj
                    .get("metadata")
                    .and_then(|m| m.get("actor"))
                    .and_then(|a| a.get("name"))
                    .and_then(|n| n.as_str())
                    .map(|s| s.to_string());

                let dedupe_key = format!("{}|{}|{}", timestamp, title.clone().unwrap_or_default(), content);
                if !seen.contains(&dedupe_key) {
                    seen.insert(dedupe_key);
                    out.push(facebook::ParsedPost {
                        timestamp,
                        content,
                        media_uris,
                        media_local_paths: Vec::new(),
                        title,
                        author_name: author,
                        action_type: Some("dtp:imported-activity".to_string()),
                        tagged_people: Vec::new(),
                        location: None,
                    });
                }
            }
        }

        for nested in obj.values() {
            collect_dtp_social_posts(nested, out, seen);
        }
    } else if let Some(arr) = value.as_array() {
        for item in arr {
            collect_dtp_social_posts(item, out, seen);
        }
    }
}

fn parse_dtp_generic_zip(path: &Path) -> Result<ParseResult, String> {
    let file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open ZIP: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Failed to read ZIP: {}", e))?;

    let mut posts: Vec<facebook::ParsedPost> = Vec::new();
    let mut seen = HashSet::new();

    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let name = entry.name().to_lowercase();
        if !name.ends_with(".json") {
            continue;
        }

        // Skip very large JSON blobs to keep import responsive.
        if entry.size() > 8 * 1024 * 1024 {
            continue;
        }

        let mut text = String::new();
        if std::io::Read::read_to_string(&mut entry, &mut text).is_err() {
            continue;
        }

        let value: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        collect_dtp_social_posts(&value, &mut posts, &mut seen);
    }

    Ok(ParseResult {
        total_photos: posts
            .iter()
            .map(|p| p.media_uris.iter().filter(|u| {
                let l = u.to_lowercase();
                l.ends_with(".jpg") || l.ends_with(".jpeg") || l.ends_with(".png") || l.ends_with(".webp")
            }).count())
            .sum(),
        total_videos: posts
            .iter()
            .map(|p| p.media_uris.iter().filter(|u| {
                let l = u.to_lowercase();
                l.ends_with(".mp4") || l.ends_with(".mov") || l.ends_with(".webm")
            }).count())
            .sum(),
        total_tracker_events: 0,
        messages: Vec::new(),
        posts,
        trackers: Vec::new(),
        source: "dtp".to_string(),
    })
}

/// Import an archive ZIP with source auto-detection, including DTP-generic JSON payloads.
#[tauri::command]
pub async fn import_archive_auto(
    path: String,
    state: State<'_, AppState>,
    source_hint: Option<String>,
) -> std::result::Result<ParseResult, String> {
    let zip_path = PathBuf::from(path.clone());
    let hinted = source_hint.unwrap_or_default().to_lowercase();

    let detected = if hinted.is_empty() {
        detect_archive_source(&zip_path)?
    } else {
        hinted
    };

    let parsed_result = match detected.as_str() {
        "facebook" => {
            import_facebook_zip(path, state.clone()).await?
        }
        "instagram" => {
            import_instagram_zip(path, state.clone()).await?
        }
        "dtp" => {
            let parsed = parse_dtp_generic_zip(&zip_path)
                .map_err(BytescapeError::Import)?;
            *state.pending_import.lock().await = Some(parsed.clone());
            parsed
        }
        _ => {
            // Attempt DTP generic parse as a fallback first.
            let parsed = parse_dtp_generic_zip(&zip_path)
                .map_err(BytescapeError::Import)?;
            if !parsed.posts.is_empty() {
                *state.pending_import.lock().await = Some(parsed.clone());
                parsed
            } else {
                return Err(BytescapeError::UnsupportedSource(
                    "Could not detect supported archive format (facebook, instagram, dtp-generic)".to_string(),
                ).to_string());
            }
        }
    };

    Ok(parsed_result)
}

/// Debug: list the contents of a ZIP archive.
/// 
/// Useful for diagnosing import issues by showing the directory structure
/// and file types contained in the export.
#[tauri::command]
pub async fn debug_zip_contents(path: String) -> std::result::Result<Vec<String>, String> {
    let zip_path = std::path::PathBuf::from(path);
    let file = std::fs::File::open(&zip_path)
        .map_err(|e| BytescapeError::ZipError(e.to_string()))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| BytescapeError::ZipError(e.to_string()))?;

    let mut results: Vec<String> = Vec::new();
    let mut dirs: BTreeSet<String> = BTreeSet::new();
    let mut ext_counts: HashMap<String, usize> = HashMap::new();
    let mut content_files: Vec<String> = Vec::new();

    for i in 0..archive.len() {
        if let Ok(entry) = archive.by_index(i) {
            let name = entry.name().to_string();
            let lower = name.to_lowercase();

            // Collect unique directory paths (up to 2 segments deep)
            let parts: Vec<&str> = name.split('/').collect();
            if parts.len() >= 2 {
                let dir2 = format!("{}/{}/", parts[0], parts[1]);
                dirs.insert(dir2);
            }

            // Count file extensions
            if let Some(ext_pos) = lower.rfind('.') {
                let ext = &lower[ext_pos..];
                *ext_counts.entry(ext.to_string()).or_insert(0) += 1;
            }

            // Collect JSON and HTML content files
            if lower.ends_with(".json") || lower.ends_with(".html") || lower.ends_with(".htm") {
                content_files.push(name.clone());
            }
        }
    }

    // File extension breakdown
    let mut ext_list: Vec<_> = ext_counts.iter().collect();
    ext_list.sort_by(|a, b| b.1.cmp(a.1));
    results.push(format!("📊 File types ({} total entries):", archive.len()));
    for (ext, count) in &ext_list {
        results.push(format!("  {} × {}", count, ext));
    }

    // Directory structure
    results.push(format!("📁 Directories ({} folders):", dirs.len()));
    for dir in &dirs {
        results.push(format!("  📁 {}", dir));
    }

    // Content files (JSON/HTML) — limit to 50
    results.push(format!("📄 Content files ({}):", content_files.len()));
    for (i, cf) in content_files.iter().enumerate() {
        if i >= 50 {
            results.push(format!("  ... and {} more", content_files.len() - 50));
            break;
        }
        results.push(format!("  📄 {}", cf));
    }

    Ok(results)
}

/// Commit pending Facebook import to the database.
/// 
/// Converts parsed Facebook posts into BytescapeItems and saves them.
/// Returns the count of new items saved.
#[tauri::command]
pub async fn commit_memories(state: State<'_, AppState>) -> std::result::Result<usize, String> {
    let pending_guard = state.pending_import.lock().await;
    let pending = pending_guard.as_ref()
        .ok_or_else(|| BytescapeError::Import("No pending import. Call import_facebook_zip first.".to_string()))?;

    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;

    let mut saved_count = 0;
    let sync_manager = state.sync_manager.lock().await.clone();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Save pending messages — use owner DID which works for both Bluesky and local-only users
    let user_did = state.keychain.get_owner_did()
        .map_err(|e| BytescapeError::Keychain(e.to_string()))?
        .ok_or_else(|| BytescapeError::Unauthenticated)?;

    for mut msg in pending.messages.clone() {
        msg.owner_did = Some(user_did.clone());
        if let Err(e) = db.save_vault_message(&msg).await {
            eprintln!("Failed to save imported message: {}", e);
        }
    }

    for post in &pending.posts {
        let item_id = uuid::Uuid::new_v4().to_string();
        let app_data_dir = state.app_handle.path()
            .app_data_dir()
            .unwrap_or_default();
        let media_dir = app_data_dir.join("media");
        let imported_media = import_item_media_into_vault(
            &sync_manager,
            &post.media_local_paths,
            &pending.source,
            Some(item_id.clone()),
            Some(post.timestamp),
            Some(&media_dir),
        ).await;
        let media_blobs: Vec<String> = imported_media.iter().map(|(_, h, _)| h.clone()).collect();
        let media_mimes: Vec<String> = imported_media.iter().map(|(_, _, m)| m.clone()).collect();

        let media_hints: Vec<String> = if media_blobs.is_empty() {
            post.media_local_paths.clone()
        } else {
            imported_media
                .iter()
                .filter_map(|(idx, _, _)| {
                    post.media_uris
                        .get(*idx)
                        .cloned()
                        .or_else(|| post.media_local_paths.get(*idx).cloned())
                })
                .collect()
        };

        let tagged_people: Vec<crate::models::TaggedPerson> = post.tagged_people.iter().map(|name| crate::models::TaggedPerson {
            source_name: name.clone(),
            source_platform: "facebook".to_string(),
            linked_did: None,
            linked_handle: None,
        }).collect();

        let location: Option<crate::models::PostLocation> = post.location.as_ref()
            .map(|loc| crate::models::PostLocation {
                name: loc.name.clone(),
                latitude: loc.latitude,
                longitude: loc.longitude,
            });

        // Determine item type based on content
        let item_type = if !post.media_uris.is_empty() {
            BytescapeType::Gallery
        } else if post.title.as_ref().map_or(false, |t| t.len() > 100) {
            BytescapeType::Article
        } else {
            BytescapeType::Status
        };

        let item = BytescapeItem {
            id: item_id.clone(),
            original_timestamp: post.timestamp,
            content: post.content.clone(),
            media_paths: media_hints,
            source: pending.source.clone(),
            path: willow_path_for_item(&pending.source, item_type, post.timestamp, &item_id),
            imported_at: now,
            title: post.title.clone(),
            original_id: None,
            syndicated_uris: std::collections::HashMap::new(),
            syndicated_at: std::collections::HashMap::new(),
            author_did: None,
            author_handle: None,
            author_avatar: None,
            tagged_people: tagged_people,
            location: location,
            is_liked: false,
            like_count: None,
            reply_count: None,
            repost_count: None,
            quote_count: None,
            item_type,
            url: None,
            link_url: None,
            content_html: None,
            preview_img: None,
            media_blobs,
            thumbnail_blobs: vec![],
            media_mimes,
            syndication_status: SyndicationStatus::LocalOnly,
            syndicated_uri: None,
            audiences: vec![Audience::Local],
            share_history: Vec::new(),
            reposted_by_did: None,
            reposted_by_handle: None,
            reply_parent_uri: None,
            reply_parent_handle: None,
            collab_doc_id: None,
        };

        if let Ok(true) = db.save_item(&item).await {
            saved_count += 1;
        }
    }

    Ok(saved_count)
}

/// Import Bluesky posts from the authenticated user's feed.
/// 
/// Fetches the user's feed and downloads embedded media.
/// Returns the count of new posts imported.
#[tauri::command]
pub async fn import_bluesky_posts(state: State<'_, AppState>) -> std::result::Result<usize, String> {
    let session = state.keychain.load_session()
        .map_err(|e| BytescapeError::Keychain(e.to_string()))?;
    let (user_did, _user_handle) = session.ok_or_else(|| BytescapeError::Unauthenticated)?;

    println!("lib: Starting import_bluesky_posts...");

    let app_handle = state.app_handle.clone();
    let app_data_dir = app_handle.path()
        .app_data_dir()
        .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
    let media_dir = app_data_dir.join("media");
    let sync_manager = state.sync_manager.lock().await.clone();

    // Import user's own feed
    let _ = app_handle.emit("import-progress", crate::connectors::bluesky::ImportProgress {
        source: "bluesky".to_string(),
        stage: "fetching".to_string(),
        posts_imported: 0,
        photos_downloaded: 0,
        current_item: "Fetching your feed...".to_string(),
    });

    let db_guard = state.db_manager.read().await;
    let known_uris = if let Some(db) = db_guard.as_ref() {
        db.get_all_known_uris().await.unwrap_or_default()
    } else {
        std::collections::HashSet::new()
    };
    drop(db_guard);

    // For OAuth sessions, use empty JWT — the public API (getAuthorFeed) works
    // without authentication for public profiles.
    // For app-password sessions, use the real JWT for full access.
    let access_jwt = if state.is_oauth_session() {
        String::new()
    } else {
        state.get_fresh_access_jwt().await
            .map_err(|e| BytescapeError::AuthFailed(e))?
    };

    let imported_posts = match crate::connectors::bluesky::import_bluesky_feed(
        &user_did,
        &access_jwt,
        media_dir.clone(),
        &app_handle,
        None,
        None,
        sync_manager.clone(),
    ).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Own feed import failed: {}", e);
            Vec::new()
        }
    };

    // Save to database
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let saved_posts = save_posts("Own Feed", db, &imported_posts, false, now, &sync_manager, Some(&media_dir), &known_uris).await;

    println!("Bluesky import: saved {} own posts", saved_posts);
    Ok(saved_posts)
}

/// Save Bluesky posts to the database
async fn save_posts(
    _feed_name: &str,
    db: &DbManager,
    posts: &[crate::connectors::bluesky::BlueskyImportedPost],
    _is_own_feed: bool,
    now: u64,
    sync_manager: &Option<Arc<SyncManager>>,
    media_base_dir: Option<&Path>,
    known_uris: &std::collections::HashSet<String>,
) -> usize {
    let mut saved = 0;
    for post in posts {
        if known_uris.contains(&post.uri) || known_uris.contains(&post.cid) {
            println!("import: Skipping already known post (likely syndicated): {}", post.uri);
            continue;
        }

        let imported_media = import_item_media_into_vault(
            sync_manager,
            &post.image_local_paths,
            Source::Bluesky.as_str(),
            Some(post.cid.clone()),
            Some(post.created_at),
            media_base_dir,
        ).await;
        let media_blobs: Vec<String> = imported_media.iter().map(|(_, h, _)| h.clone()).collect();
        let media_mimes: Vec<String> = imported_media.iter().map(|(_, _, m)| m.clone()).collect();

        let item_type = if !post.image_urls.is_empty() {
            BytescapeType::Gallery
        } else if post.external_url.is_some() {
            BytescapeType::Article
        } else {
            BytescapeType::Status
        };

        let item = BytescapeItem {
            id: post.cid.clone(),
            original_timestamp: post.created_at,
            content: post.text.clone(),
            media_paths: post.image_local_paths.clone(),
            source: Source::Bluesky.as_str().to_string(),
            path: willow_path_for_item(Source::Bluesky.as_str(), item_type, post.created_at, &post.cid),
            imported_at: now,
            title: post.external_title.clone(),
            original_id: Some(post.uri.clone()),
            syndicated_uris: std::collections::HashMap::new(),
            syndicated_at: std::collections::HashMap::new(),
            author_did: Some(post.author_did.clone()),
            author_handle: Some(post.author_handle.clone()),
            author_avatar: post.author_avatar.clone(),
            tagged_people: vec![],
            location: None,
            is_liked: false,
            like_count: post.like_count,
            reply_count: post.reply_count,
            repost_count: post.repost_count,
            quote_count: None,
            item_type,
            url: Some(post.uri.clone()),
            link_url: post.external_url.clone(),
            content_html: None,
            preview_img: post.external_thumb.clone(),
            media_blobs,
            thumbnail_blobs: vec![],
            media_mimes,
            syndication_status: SyndicationStatus::Syndicated,
            syndicated_uri: Some(post.uri.clone()),
            audiences: vec![Audience::Local, Audience::Bluesky],
            share_history: vec![crate::models::SyndicationEvent {
                platform: "bluesky".to_string(),
                uri: post.uri.clone(),
                timestamp: post.created_at as u64,
                active: true,
            }],
            reposted_by_did: post.reposted_by.clone(),
            reposted_by_handle: post.reposted_by_handle.clone(),
            reply_parent_uri: post.reply_parent_uri.clone(),
            reply_parent_handle: post.reply_parent_handle.clone(),
            collab_doc_id: None,
        };

        if let Ok(true) = db.save_item(&item).await {
            saved += 1;
        }
    }
    saved
}

fn mime_from_path(path: &Path) -> String {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()) {
        Some(ext) if ext == "jpg" || ext == "jpeg" => "image/jpeg".to_string(),
        Some(ext) if ext == "png" => "image/png".to_string(),
        Some(ext) if ext == "webp" => "image/webp".to_string(),
        Some(ext) if ext == "gif" => "image/gif".to_string(),
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
        _ => "application/octet-stream".to_string(),
    }
}

fn mime_from_bytes(data: &[u8]) -> String {
    super::media::mime_from_magic_bytes(data)
}

fn parse_blob_hash_path(media_path: &str) -> Option<Hash> {
    blob_hash::parse_blob_hash(media_path).ok()
}

/// Returns Vec<(index, blob_hash, mime_type)> for each successfully imported media.
async fn import_item_media_into_vault(
    sync_manager: &Option<Arc<SyncManager>>,
    media_paths: &[String],
    source: &str,
    item_id: Option<String>,
    created_at: Option<i64>,
    media_base_dir: Option<&Path>,
) -> Vec<(usize, String, String)> {
    let Some(sync) = sync_manager.as_ref() else {
        return Vec::new();
    };

    let mut imported_blobs = Vec::new();

    for (idx, media_path) in media_paths.iter().enumerate() {
        if media_path.trim().is_empty() {
            continue;
        }

        // blob:<hash> paths (common for sync-backed imports), plus legacy raw
        // hash strings from older pending import payloads.
        if let Some(hash) = parse_blob_hash_path(media_path) {
            let bytes = match sync.get_media(hash).await {
                Ok(b) => b,
                Err(e) => {
                    println!("import_item_media_into_vault: Failed to get media for hash {}: {}", hash, e);
                    continue;
                }
            };

            let mime_type = mime_from_bytes(bytes.as_ref());
            let ext = if mime_type == "image/jpeg" {
                "jpg"
            } else if mime_type == "image/png" {
                "png"
            } else if mime_type == "image/gif" {
                "gif"
            } else if mime_type == "image/webp" {
                "webp"
            } else if mime_type == "video/mp4" {
                "mp4"
            } else if mime_type == "video/webm" {
                "webm"
            } else if mime_type == "video/ogg" {
                "ogv"
            } else if mime_type == "application/pdf" {
                "pdf"
            } else {
                "bin"
            };

            let name = format!("{}-{}-{}.{}", source, item_id.clone().unwrap_or_else(|| "item".to_string()), idx, ext);
            match sync.vault_add_file(
                name,
                mime_type.clone(),
                bytes,
                source.to_string(),
                item_id.clone(),
                created_at,
                vec![],
            ).await {
                Ok(file) => {
                    imported_blobs.push((idx, file.blob_hash, mime_type));
                    println!("import_item_media_into_vault: Successfully added blob file to vault");
                }
                Err(e) => println!("import_item_media_into_vault: Failed to add blob file to vault: {}", e),
            }
            continue;
        }

        // Filesystem-backed media paths
        let mut path = PathBuf::from(media_path);
        if !path.is_absolute() {
            if let Some(base) = media_base_dir {
                path = base.join(path);
            }
        }

        let data = match tokio::fs::read(&path).await {
            Ok(d) => d,
            Err(e) => {
                println!("import_item_media_into_vault: Failed to read file {:?}: {}", path, e);
                continue;
            }
        };

        let mime_type = mime_from_path(&path);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("{}-{}", source, idx));

        match sync.vault_add_file(
            name,
            mime_type.clone(),
            bytes::Bytes::from(data),
            source.to_string(),
            item_id.clone(),
            created_at,
            vec![],
        ).await {
            Ok(file) => {
                imported_blobs.push((idx, file.blob_hash, mime_type));
                println!("import_item_media_into_vault: Successfully added fs file to vault");
            }
            Err(e) => println!("import_item_media_into_vault: Failed to add fs file to vault: {}", e),
        }
    }

    imported_blobs
}

/// Get the Bluesky timeline (friends feed)
#[tauri::command]
pub async fn get_timeline(
    state: State<'_, AppState>,
    limit: Option<u8>,
    cursor: Option<String>,
) -> std::result::Result<(Vec<crate::connectors::bluesky::BlueskyImportedPost>, Option<String>), String> {
    let session = state.keychain.load_session()
        .map_err(|e| BytescapeError::Keychain(e.to_string()))?;
    let (_user_did, _user_handle) = session.ok_or_else(|| BytescapeError::Unauthenticated)?;

    if state.is_oauth_session() {
        // OAuth path: use DPoP-authenticated requests via OAuthManager.
        // DPoP tokens are audience-bound to the user's PDS, not the public
        // appview (bsky.social). The PDS proxies reads to the AppView.
        let pds_url = state.oauth_manager.get_pds_url().await?;
        let limit_val = limit.unwrap_or(50);
        let mut url = format!(
            "{}/xrpc/app.bsky.feed.getTimeline?limit={}",
            pds_url, limit_val
        );
        if let Some(ref c) = cursor {
            url.push_str(&format!("&cursor={}", urlencoding::encode(c)));
        }

        let json = state.oauth_manager.authenticated_get(&url).await?;
        let (posts, next_cursor) = crate::connectors::bluesky::parse_feed_json(&json)?;
        return Ok((posts, next_cursor));
    }

    let access_jwt = state.get_fresh_access_jwt().await
        .map_err(|e| BytescapeError::AuthFailed(e))?;

    let app_handle = state.app_handle.clone();
    let (posts, next_cursor) = crate::connectors::bluesky::import_bluesky_timeline(
        &access_jwt,
        std::path::PathBuf::new(),
        &app_handle,
        limit,
        cursor,
        None,
        false,
    ).await?;

    Ok((posts, next_cursor))
}

/// Import Bluesky DM messages
#[tauri::command]
pub async fn import_vault_messages(state: State<'_, AppState>) -> std::result::Result<serde_json::Value, String> {
    use crate::db::VaultMessage;
    
    let session = state.keychain.load_session()
        .map_err(|e| BytescapeError::Keychain(e.to_string()))?;
    let (user_did, _user_handle) = session.ok_or_else(|| BytescapeError::Unauthenticated)?;

    let db_manager = state.db_manager.clone();

    if state.is_oauth_session() {
        // OAuth path: use xrpc_chat_get with DPoP + atproto-proxy
        let convos_resp = state.xrpc_chat_get("chat.bsky.convo.listConvos").await
            .map_err(|e| format!("Failed to list conversations: {}", e))?;

        let convos = convos_resp.get("convos")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let db_guard = db_manager.read().await;
        let db = db_guard.as_ref()
            .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;

        let mut new_count = 0;

        for convo_val in &convos {
            let convo_id = convo_val.get("id").and_then(|v| v.as_str()).unwrap_or_default();
            if convo_id.is_empty() { continue; }

            let members = convo_val.get("members")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let peer = members.iter().find(|m| {
                m.get("did").and_then(|d| d.as_str()) != Some(&user_did)
            });
            let peer_did = peer.and_then(|p| p.get("did").and_then(|d| d.as_str()).map(|s| s.to_string()));
            let peer_handle = peer.and_then(|p| p.get("handle").and_then(|h| h.as_str()).map(|s| s.to_string()));

            let endpoint = format!("chat.bsky.convo.getMessages?convoId={}&limit=100", convo_id);
            let msgs_resp = match state.xrpc_chat_get(&endpoint).await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("import_vault_messages: Failed to get messages for convo {}: {}", convo_id, e);
                    continue;
                }
            };

            let messages = msgs_resp.get("messages")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            for msg_val in &messages {
                // Only process actual chat messages (skip deletions etc.)
                let msg_type = msg_val.get("$type").and_then(|t| t.as_str()).unwrap_or("");
                if msg_type != "chat.bsky.convo.defs#messageView" && !msg_type.is_empty() {
                    continue;
                }

                let msg_id = msg_val.get("id").and_then(|v| v.as_str()).unwrap_or_default();
                if msg_id.is_empty() { continue; }

                let sender_did = msg_val.get("sender")
                    .and_then(|s| s.get("did"))
                    .and_then(|d| d.as_str())
                    .unwrap_or_default()
                    .to_string();
                let sender_handle = msg_val.get("sender")
                    .and_then(|s| s.get("handle"))
                    .and_then(|h| h.as_str())
                    .unwrap_or_default()
                    .to_string();
                let text = msg_val.get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or_default()
                    .to_string();
                let sent_at = msg_val.get("sentAt")
                    .and_then(|t| t.as_str())
                    .unwrap_or_default()
                    .to_string();

                let vault_msg = VaultMessage {
                    id: msg_id.to_string(),
                    owner_did: Some(user_did.clone()),
                    convo_id: convo_id.to_string(),
                    sender_did,
                    sender_handle,
                    text,
                    sent_at,
                    source: "bluesky".to_string(),
                    imported_at: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as u64,
                    peer_did: peer_did.clone(),
                    peer_handle: peer_handle.clone(),
                };

                if let Ok(true) = db.save_vault_message(&vault_msg).await {
                    new_count += 1;
                }
            }
        }

        return Ok(serde_json::json!({
            "new": new_count,
            "convos": convos.len()
        }));
    }

    // App-password path: use ChatClient with Bearer auth
    let access_jwt = state.get_fresh_access_jwt().await
        .map_err(|e| BytescapeError::AuthFailed(e))?;

    let chat_client = state.chat_client.clone();
    
    // List conversations
    let convos = chat_client.list_convos(&access_jwt, &user_did).await
        .map_err(|e| BytescapeError::BlueskyApi(e))?;
    
    let db_guard = db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;
    
    let mut new_count = 0;
    
    for convo in &convos {
        // Get messages from this conversation
        let (messages, _cursor) = chat_client.get_messages(&access_jwt, &user_did, &convo.id, None).await
            .map_err(|e| BytescapeError::BlueskyApi(e))?;
        
        let peer = convo.members.iter().find(|m| m.did != user_did);
        let peer_did = peer.map(|p| p.did.clone());
        let peer_handle = peer.and_then(|p| p.handle.clone());

        for msg in messages {
            let vault_msg = VaultMessage {
                id: msg.id.clone(),
                owner_did: Some(user_did.clone()),
                convo_id: convo.id.clone(),
                sender_did: msg.sender.did.clone(),
                sender_handle: msg.sender.handle.clone().unwrap_or_default(),
                text: msg.text.clone(),
                sent_at: msg.sent_at.clone().unwrap_or_default(),
                source: "bluesky".to_string(),
                imported_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as u64,
                peer_did: peer_did.clone(),
                peer_handle: peer_handle.clone(),
            };
            
            if let Ok(true) = db.save_vault_message(&vault_msg).await {
                new_count += 1;
            }
        }
    }
    
    Ok(serde_json::json!({
        "new": new_count,
        "convos": convos.len()
    }))
}

fn archive_message_placeholder(msg: &serde_json::Value) -> Option<String> {
    if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let photos = msg.get("photos").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
    if photos > 0 {
        return Some(format!("[{} photo{}]", photos, if photos == 1 { "" } else { "s" }));
    }

    let videos = msg.get("videos").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
    if videos > 0 {
        return Some(format!("[{} video{}]", videos, if videos == 1 { "" } else { "s" }));
    }

    let audio = msg.get("audio_files").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
    if audio > 0 {
        return Some(format!("[{} audio file{}]", audio, if audio == 1 { "" } else { "s" }));
    }

    let gifs = msg.get("gifs").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
    if gifs > 0 {
        return Some(format!("[{} GIF{}]", gifs, if gifs == 1 { "" } else { "s" }));
    }

    if msg.get("sticker").is_some() {
        return Some("[sticker]".to_string());
    }
    if msg.get("share").is_some() {
        return Some("[shared attachment]".to_string());
    }

    None
}

fn parse_facebook_html_timestamp_to_rfc3339(raw: &str) -> Option<String> {
    let value = raw
        .replace('\u{202f}', " ")
        .replace('\u{00a0}', " ")
        .trim()
        .to_string();

    let formats = [
        "%b %d, %Y %I:%M:%S %P",
        "%b %-d, %Y %-I:%M:%S %P",
        "%B %d, %Y %I:%M:%S %P",
        "%B %-d, %Y %-I:%M:%S %P",
    ];

    for fmt in formats {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(&value, fmt) {
            let dt = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(naive, chrono::Utc);
            return Some(dt.to_rfc3339());
        }
    }

    None
}

fn parse_facebook_html_messages(html_text: &str) -> Vec<(String, String, String)> {
    use scraper::{Html, Selector};

    let mut out = Vec::new();
    let doc = Html::parse_document(html_text);

    let Ok(block_sel) = Selector::parse("div._a6-g") else { return out; };
    let Ok(sender_sel) = Selector::parse("div._a6-h._a6-i") else { return out; };
    let Ok(text_sel) = Selector::parse("div._a6-p") else { return out; };
    let Ok(time_sel) = Selector::parse("div._a72d") else { return out; };

    for block in doc.select(&block_sel) {
        let sender = block
            .select(&sender_sel)
            .next()
            .map(|n| n.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Unknown".to_string());

        let text = block
            .select(&text_sel)
            .next()
            .map(|n| n.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();

        let sent_raw = block
            .select(&time_sel)
            .next()
            .map(|n| n.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .unwrap_or_default();

        if text.is_empty() || sent_raw.is_empty() {
            continue;
        }

        out.push((sender, text, sent_raw));
    }

    out
}

/// Import archived chat threads from a Facebook/Instagram ZIP export into vault messages.
#[tauri::command]
pub async fn import_archive_messages(
    state: State<'_, AppState>,
    path: String,
    source: Option<String>,
) -> std::result::Result<serde_json::Value, String> {
    use std::io::Read;
    use zip::ZipArchive;

    let normalized_source = source
        .unwrap_or_else(|| {
            if path.to_lowercase().contains("instagram") {
                "instagram".to_string()
            } else {
                "facebook".to_string()
            }
        })
        .to_lowercase();

    let owner_did = state
        .keychain
        .get_owner_did()
        .map_err(|e| BytescapeError::Keychain(e.to_string()))?
        .ok_or_else(|| BytescapeError::Unauthenticated)?;

    let db_manager = state.db_manager.clone();
    let db_guard = db_manager.read().await;
    let db = db_guard
        .as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;

    let archive_file = std::fs::File::open(&path)
        .map_err(|e| BytescapeError::FileSystem(format!("Failed to open archive: {}", e)))?;
    let mut zip = ZipArchive::new(archive_file)
        .map_err(|e| BytescapeError::ZipError(e.to_string()))?;

    let mut message_files: Vec<(usize, String)> = Vec::new();
    for i in 0..zip.len() {
        let Ok(file) = zip.by_index(i) else { continue; };
        let name = file.name().replace('\\', "/");
        let lower = name.to_lowercase();
        let is_chat_file = (lower.ends_with(".json") || lower.ends_with(".html"))
            && lower.contains("/messages/")
            && (lower.contains("/inbox/") || lower.contains("/archived_threads/"))
            && (lower.contains("/message_") || lower.ends_with("/message.html"));

        if is_chat_file {
            message_files.push((i, name));
        }
    }

    let mut imported_new = 0usize;
    let mut parsed_messages = 0usize;
    let mut imported_threads = BTreeSet::new();
    let imported_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u64;

    for (index, file_name) in message_files {
        let mut file = match zip.by_index(index) {
            Ok(f) => f,
            Err(_) => continue,
        };

        let mut text = String::new();
        if file.read_to_string(&mut text).is_err() {
            continue;
        }

        let thread_id = {
            let normalized = file_name.to_lowercase();
            if let Some(idx) = normalized.rfind("/message_") {
                normalized[..idx].to_string()
            } else if let Some(idx) = normalized.rfind("/message.html") {
                normalized[..idx].to_string()
            } else {
                normalized
            }
        };
        let convo_id = format!("archive:{}:{}", normalized_source, thread_id);
        imported_threads.insert(convo_id.clone());

        if file_name.to_lowercase().ends_with(".json") {
            let parsed: serde_json::Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let messages = parsed
                .get("messages")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            for msg in messages {
                let sender_name = msg
                    .get("sender_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string();

                let Some(message_text) = archive_message_placeholder(&msg) else {
                    continue;
                };

                let timestamp_ms = msg
                    .get("timestamp_ms")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);

                if timestamp_ms <= 0 {
                    continue;
                }

                let sent_at = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(timestamp_ms)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());

                let id = format!(
                    "archive:{}:{}:{}:{}",
                    normalized_source,
                    thread_id,
                    timestamp_ms,
                    sender_name
                );

                let vault_msg = crate::db::VaultMessage {
                    id,
                    owner_did: Some(owner_did.clone()),
                    convo_id: convo_id.clone(),
                    sender_did: String::new(),
                    sender_handle: sender_name,
                    text: message_text,
                    sent_at,
                    source: normalized_source.clone(),
                    imported_at,
                    peer_did: None,
                    peer_handle: None,
                };

                parsed_messages += 1;
                if let Ok(true) = db.save_vault_message(&vault_msg).await {
                    imported_new += 1;
                }
            }
        } else if normalized_source == "facebook" && file_name.to_lowercase().ends_with(".html") {
            let html_messages = parse_facebook_html_messages(&text);
            for (idx, (sender_name, message_text, sent_raw)) in html_messages.into_iter().enumerate() {
                let sent_at = parse_facebook_html_timestamp_to_rfc3339(&sent_raw)
                    .unwrap_or(sent_raw.clone());

                let id = format!(
                    "archive:{}:{}:{}:{}",
                    normalized_source,
                    thread_id,
                    idx,
                    sender_name
                );

                let vault_msg = crate::db::VaultMessage {
                    id,
                    owner_did: Some(owner_did.clone()),
                    convo_id: convo_id.clone(),
                    sender_did: String::new(),
                    sender_handle: sender_name,
                    text: message_text,
                    sent_at,
                    source: normalized_source.clone(),
                    imported_at,
                    peer_did: None,
                    peer_handle: None,
                };

                parsed_messages += 1;
                if let Ok(true) = db.save_vault_message(&vault_msg).await {
                    imported_new += 1;
                }
            }
        }
    }

    Ok(serde_json::json!({
        "source": normalized_source,
        "threads": imported_threads.len(),
        "parsed": parsed_messages,
        "new": imported_new
    }))
}

/// Send a Bluesky DM
#[tauri::command]
pub async fn send_bsky_dm(
    state: State<'_, AppState>,
    convo_id: String,
    _recipient_did: Option<String>,
    content: String,
) -> std::result::Result<serde_json::Value, String> {

    let session = state.keychain.load_session()
        .map_err(|e| BytescapeError::Keychain(e.to_string()))?;
    let (user_did, _user_handle) = session.ok_or_else(|| BytescapeError::Unauthenticated)?;

    if state.is_oauth_session() {
        // OAuth path: use xrpc_chat_post with DPoP + atproto-proxy
        let body = serde_json::json!({
            "convoId": convo_id,
            "message": {
                "text": content.clone()
            }
        });
        let resp = state.xrpc_chat_post("chat.bsky.convo.sendMessage", body).await
            .map_err(|e| format!("Failed to send DM: {}", e))?;

        let msg_id = resp.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();

        // Save to local DB
        let db_guard = state.db_manager.read().await;
        let db = db_guard.as_ref()
            .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;

        let vault_msg = crate::db::VaultMessage {
            id: msg_id.clone(),
            owner_did: Some(user_did.clone()),
            convo_id: convo_id.clone(),
            sender_did: user_did.clone(),
            sender_handle: _user_handle.clone(),
            text: content,
            sent_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis().to_string(),
            source: "bluesky".to_string(),
            imported_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as u64,
            peer_did: _recipient_did,
            peer_handle: None,
        };

        let _ = db.save_vault_message(&vault_msg).await;

        return Ok(serde_json::json!({
            "id": msg_id,
            "convo_id": convo_id,
            "sent": true
        }));
    }

    let access_jwt = state.get_fresh_access_jwt().await
        .map_err(|e| BytescapeError::AuthFailed(e))?;

    let chat_client = state.chat_client.clone();
    let db_manager = state.db_manager.clone();
    
    // Send via chat API
    let msg = chat_client.send_dm(&access_jwt, &user_did, &convo_id, &content).await
        .map_err(|e| BytescapeError::BlueskyApi(e))?;
    
    // Save to local DB
    let db_guard = db_manager.read().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| BytescapeError::DatabaseNotInitialized)?;
    
    let vault_msg = crate::db::VaultMessage {
        id: msg.id.clone(),
        owner_did: Some(user_did.clone()),
        convo_id: convo_id.clone(),
        sender_did: user_did.clone(),
        sender_handle: _user_handle.clone(),
        text: content,
        sent_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis().to_string(), // approximation
        source: "bluesky".to_string(),
        imported_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as u64,
        peer_did: _recipient_did,
        peer_handle: None,
    };
    
    let _ = db.save_vault_message(&vault_msg).await;
    
    Ok(serde_json::json!({
        "id": msg.id,
        "convo_id": convo_id,
        "sent": true
    }))
}

/// List saved Bluesky custom feeds.
#[tauri::command]
pub async fn list_saved_feeds(state: State<'_, AppState>) -> std::result::Result<Vec<crate::connectors::bluesky::GeneratorView>, String> {
    if state.is_oauth_session() {
        // OAuth path: use DPoP-authenticated XRPC helpers
        return list_saved_feeds_oauth(&state).await;
    }
    let access_jwt = state.get_fresh_access_jwt().await?;
    crate::connectors::bluesky::get_saved_feeds(&access_jwt).await
}

/// OAuth-aware implementation of saved feeds fetching.
///
/// Uses the unified XRPC helpers to make DPoP-authenticated requests
/// for getPreferences and getFeedGenerators.
async fn list_saved_feeds_oauth(state: &AppState) -> std::result::Result<Vec<crate::connectors::bluesky::GeneratorView>, String> {
    use crate::connectors::bluesky::GeneratorView;

    // 1. Fetch preferences via OAuth
    let prefs_json = state
        .xrpc_get("app.bsky.actor.getPreferences")
        .await?;

    // 2. Extract feed URIs from preferences
    fn collect_feed_uris(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(s) => {
                if s.starts_with("at://") && s.contains("/app.bsky.feed.generator/") {
                    out.push(s.to_string());
                }
            }
            serde_json::Value::Array(arr) => {
                for v in arr { collect_feed_uris(v, out); }
            }
            serde_json::Value::Object(map) => {
                if let Some(v) = map.get("value").and_then(|v| v.as_str()) {
                    if v.starts_with("at://") && v.contains("/app.bsky.feed.generator/") {
                        out.push(v.to_string());
                    }
                }
                if let Some(v) = map.get("uri").and_then(|v| v.as_str()) {
                    if v.starts_with("at://") && v.contains("/app.bsky.feed.generator/") {
                        out.push(v.to_string());
                    }
                }
                for v in map.values() { collect_feed_uris(v, out); }
            }
            _ => {}
        }
    }

    let mut feed_uris = Vec::new();
    if let Some(prefs) = prefs_json.get("preferences").and_then(|p| p.as_array()) {
        for pref in prefs {
            collect_feed_uris(pref, &mut feed_uris);
        }
    }

    // Deduplicate
    let mut seen = std::collections::HashSet::new();
    feed_uris.retain(|uri| seen.insert(uri.clone()));

    if feed_uris.is_empty() {
        return Ok(Vec::new());
    }

    // 3. Resolve feed generators in batches via OAuth
    let mut resolved_feeds: Vec<GeneratorView> = Vec::new();
    for chunk in feed_uris.chunks(10) {
        let params: Vec<String> = chunk.iter().map(|uri| format!("feeds={}", urlencoding::encode(uri))).collect();
        let query = params.join("&");
        let endpoint = format!("app.bsky.feed.getFeedGenerators?{}", query);

        match state.xrpc_get(&endpoint).await {
            Ok(resp) => {
                if let Some(feeds) = resp.get("feeds").and_then(|f| f.as_array()) {
                    for feed in feeds {
                        if let Ok(view) = serde_json::from_value::<GeneratorView>(feed.clone()) {
                            resolved_feeds.push(view);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("OAuth: getFeedGenerators batch error: {}", e);
            }
        }
    }

    // 4. Synthetic fallback for unresolved URIs
    let resolved_uris: std::collections::HashSet<String> = resolved_feeds.iter().map(|f| f.uri.clone()).collect();
    for uri in feed_uris {
        if resolved_uris.contains(&uri) { continue; }
        let trimmed = uri.trim_start_matches("at://");
        let mut parts = trimmed.split('/');
        let did = parts.next().unwrap_or_default().to_string();
        let _collection = parts.next();
        let rkey = parts.next().unwrap_or("feed");
        resolved_feeds.push(GeneratorView {
            uri: uri.clone(),
            cid: String::new(),
            did,
            display_name: Some(rkey.replace('-', " ")),
            description: None,
            avatar: None,
        });
    }

    Ok(resolved_feeds)
}

/// List profiles this actor follows.
#[tauri::command]
pub async fn list_follows(state: State<'_, AppState>, actor: String) -> std::result::Result<Vec<crate::connectors::bluesky::ProfileView>, String> {
    // getFollows is a public endpoint — empty JWT works
    let access_jwt = if state.is_oauth_session() {
        String::new()
    } else {
        state.get_fresh_access_jwt().await?
    };
    let (profiles, _cursor) = crate::connectors::bluesky::get_follows(&access_jwt, &actor, 100, None).await?;
    Ok(profiles)
}

/// List profiles following this actor.
#[tauri::command]
pub async fn list_followers(state: State<'_, AppState>, actor: String) -> std::result::Result<Vec<crate::connectors::bluesky::ProfileView>, String> {
    let members = state.chat_client.get_followers(&actor).await
        .map_err(BytescapeError::BlueskyApi)?;

    let profiles = members.into_iter().map(|m| crate::connectors::bluesky::ProfileView {
        did: m.did,
        handle: m.handle.unwrap_or_default(),
        display_name: m.display_name,
        avatar: None,
        description: None,
    }).collect();

    Ok(profiles)
}

/// Build a merged feed from followed hashtags and saved feeds.
#[tauri::command]
pub async fn get_unified_feed(
    state: State<'_, AppState>,
    tags: Vec<String>,
    feed_uris: Vec<String>,
    limit: u8,
    cursor: Option<String>,
) -> std::result::Result<(Vec<crate::connectors::bluesky::BlueskyImportedPost>, Option<String>), String> {
    if state.is_oauth_session() {
        // OAuth path: use DPoP-authenticated XRPC for each sub-query
        return get_unified_feed_oauth(&state, tags, feed_uris, limit, cursor).await;
    }
    let access_jwt = state.get_fresh_access_jwt().await?;
    crate::connectors::bluesky::get_unified_feed(&access_jwt, tags, Some(feed_uris), limit, cursor).await
}

/// OAuth-aware unified feed aggregation.
///
/// Fetches hashtag search results and custom feed posts via DPoP-authenticated
/// XRPC helpers, then merges and deduplicates by URI.
async fn get_unified_feed_oauth(
    state: &AppState,
    tags: Vec<String>,
    feed_uris: Vec<String>,
    limit: u8,
    cursor: Option<String>,
) -> std::result::Result<(Vec<crate::connectors::bluesky::BlueskyImportedPost>, Option<String>), String> {
    use crate::connectors::bluesky::BlueskyImportedPost;

    let parsed_cursor: crate::connectors::bluesky::UnifiedCursor = cursor
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default();

    let mut all_posts: Vec<BlueskyImportedPost> = Vec::new();
    let mut next_cursor = crate::connectors::bluesky::UnifiedCursor::default();

    // 1. Hashtag search — get raw JSON via OAuth, then parse with existing parsers
    if !tags.is_empty() {
        let tag_query = tags.join(" OR ");
        let mut search_endpoint = format!(
            "app.bsky.feed.searchPosts?q={}&limit={}",
            urlencoding::encode(&tag_query),
            limit
        );
        if let Some(ref c) = parsed_cursor.hashtag_cursor {
            search_endpoint.push_str(&format!("&cursor={}", urlencoding::encode(c)));
        }

        match state.xrpc_get(&search_endpoint).await {
            Ok(resp) => {
                // searchPosts returns { posts: [...], cursor: ... }
                // Convert to the feed format that parse_feed_json expects
                if let Some(posts_arr) = resp.get("posts").and_then(|p| p.as_array()) {
                    let strict_tags: Vec<String> = tags.iter()
                        .map(|t| t.trim_start_matches('#').to_lowercase())
                        .collect();
                    for post_val in posts_arr {
                        // Parse each post using manual extraction (search results are FeedPost, not FeedItem)
                        if let Some(post) = parse_search_post_json(post_val) {
                            let post_text = post.text.to_lowercase();
                            if strict_tags.iter().any(|t| post_text.contains(t)) {
                                all_posts.push(post);
                            }
                        }
                    }
                }
                next_cursor.hashtag_cursor = resp
                    .get("cursor")
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_string());
            }
            Err(e) => eprintln!("OAuth unified feed: hashtag search error: {}", e),
        }
    }

    // 2. Custom feed posts — get raw JSON via OAuth, parse with parse_feed_json
    let uris = if feed_uris.is_empty() {
        match list_saved_feeds_oauth(state).await {
            Ok(views) => views.into_iter()
                .map(|v| v.uri)
                .filter(|uri| !uri.contains("app.bsky.feed.generator/following"))
                .collect(),
            Err(_) => Vec::new(),
        }
    } else {
        feed_uris
    };

    for feed_uri in &uris {
        let mut endpoint = format!(
            "app.bsky.feed.getFeed?feed={}&limit={}",
            urlencoding::encode(feed_uri),
            limit
        );
        if let Some(c) = parsed_cursor.feed_cursors.get(feed_uri.as_str()) {
            endpoint.push_str(&format!("&cursor={}", urlencoding::encode(c)));
        }

        match state.xrpc_get(&endpoint).await {
            Ok(resp) => {
                // getFeed returns { feed: [...], cursor: ... } — same shape as getTimeline
                let json_str = serde_json::to_string(&resp).unwrap_or_default();
                if let Ok((posts, _)) = crate::connectors::bluesky::parse_feed_json(&json_str) {
                    all_posts.extend(posts);
                }
                if let Some(c) = resp.get("cursor").and_then(|c| c.as_str()) {
                    next_cursor.feed_cursors.insert(feed_uri.clone(), c.to_string());
                }
            }
            Err(e) => eprintln!("OAuth unified feed: getFeed error for {}: {}", feed_uri, e),
        }
    }

    // 3. Deduplicate by URI
    let mut seen = std::collections::HashSet::new();
    all_posts.retain(|p| seen.insert(p.uri.clone()));

    // Sort by timestamp descending
    all_posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    all_posts.truncate(limit as usize);

    let cursor_str = serde_json::to_string(&next_cursor).ok();
    Ok((all_posts, cursor_str))
}

/// Parse a single search result post (FeedPost shape) into a BlueskyImportedPost.
///
/// Search results have a flat structure unlike timeline items which wrap posts in FeedItem.
fn parse_search_post_json(post_val: &serde_json::Value) -> Option<crate::connectors::bluesky::BlueskyImportedPost> {
    use crate::connectors::bluesky::BlueskyImportedPost;

    let uri = post_val.get("uri")?.as_str()?.to_string();
    let cid = post_val.get("cid").and_then(|c| c.as_str()).unwrap_or_default().to_string();
    let author = post_val.get("author")?;
    let author_did = author.get("did")?.as_str()?.to_string();
    let author_handle = author.get("handle").and_then(|h| h.as_str()).unwrap_or_default().to_string();
    let author_avatar = author.get("avatar").and_then(|a| a.as_str()).map(|s| s.to_string());

    let record = post_val.get("record")?;
    let text = record.get("text").and_then(|t| t.as_str()).unwrap_or_default().to_string();
    let created_at = record.get("createdAt")
        .and_then(|c| c.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp())
        .unwrap_or(0);

    let reply_count = post_val.get("replyCount").and_then(|v| v.as_i64());
    let repost_count = post_val.get("repostCount").and_then(|v| v.as_i64());
    let like_count = post_val.get("likeCount").and_then(|v| v.as_i64());

    let viewer_like = post_val.get("viewer")
        .and_then(|v| v.get("like"))
        .and_then(|l| l.as_str())
        .map(|s| s.to_string());
    let viewer_repost = post_val.get("viewer")
        .and_then(|v| v.get("repost"))
        .and_then(|r| r.as_str())
        .map(|s| s.to_string());

    Some(BlueskyImportedPost {
        uri,
        cid,
        text,
        created_at,
        image_urls: Vec::new(),
        image_local_paths: Vec::new(),
        author_did,
        author_handle,
        author_avatar,
        reply_count,
        repost_count,
        like_count,
        video_url: None,
        video_thumbnail: None,
        external_url: None,
        external_title: None,
        external_description: None,
        external_thumb: None,
        reposted_by: None,
        reposted_by_handle: None,
        reply_parent_handle: None,
        reply_parent_uri: None,
        quoted_post: None,
        viewer_like,
        viewer_repost,
    })
}

/// Publish a new post with explicit destination routing.
///
/// Supported destinations:
/// - "local": save locally in Bytescape vault (P2P/local-first)
/// - "bluesky": publish to Bluesky via createRecord
/// - "activitypub": placeholder (not implemented yet)
/// - "all": attempt local + bluesky + activitypub placeholder
#[tauri::command]
pub async fn publish_post(
    state: State<'_, AppState>,
    text: String,
    destination: String,
) -> std::result::Result<PublishResult, String> {
    let content = text.trim();
    if content.is_empty() {
        return Err("Post cannot be empty".to_string());
    }

    let dest = destination.to_lowercase();
    let publish_local = dest == "local" || dest == "all";
    let publish_bluesky = dest == "bluesky" || dest == "all";
    let publish_activitypub = dest == "activitypub" || dest == "all";

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

    // Local/P2P-first save
    if publish_local {
        let db_guard = state.db_manager.read().await;
        if let Some(db) = db_guard.as_ref() {
            let item_id = uuid::Uuid::new_v4().to_string();
            let item = BytescapeItem {
                id: item_id.clone(),
                original_timestamp: now as i64,
                content: content.to_string(),
                media_paths: vec![],
                source: Source::Manual.as_str().to_string(),
                path: willow_path_for_item(Source::Manual.as_str(), BytescapeType::Status, now as i64, &item_id),
                imported_at: now,
                title: None,
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
                item_type: BytescapeType::Status,
                url: None,
                link_url: None,
                content_html: None,
                preview_img: None,
                media_blobs: vec![],
                thumbnail_blobs: vec![],
                media_mimes: vec![],
                syndication_status: SyndicationStatus::LocalOnly,
                syndicated_uri: None,
                audiences: vec![Audience::Local],
                share_history: Vec::new(),
                reposted_by_did: None,
                reposted_by_handle: None,
                reply_parent_uri: None,
                reply_parent_handle: None,
                collab_doc_id: None,
            };

            match db.save_item(&item).await {
                Ok(_) => result.local = true,
                Err(e) => result.errors.push(format!("Local publish failed: {}", e)),
            }
        } else {
            result.errors.push("Local publish failed: database not initialized".to_string());
        }
    }

    // Bluesky publish
    if publish_bluesky {
        let user_did = state.get_user_did().unwrap_or_default();
        let user_handle = state.keychain.load_session()
            .ok().flatten()
            .map(|(_, h)| h)
            .unwrap_or_default();

        if !user_did.is_empty() {
            let record_body = serde_json::json!({
                "repo": user_did,
                "collection": "app.bsky.feed.post",
                "record": {
                    "$type": "app.bsky.feed.post",
                    "text": content,
                    "createdAt": chrono::Utc::now().to_rfc3339()
                }
            });

            match state.xrpc_post_json("com.atproto.repo.createRecord", record_body).await {
                Ok(res) => {
                    let uri = res["uri"].as_str().unwrap_or_default().to_string();
                    let cid = res["cid"].as_str().unwrap_or_default().to_string();
                    if !uri.is_empty() {
                        result.bluesky = true;
                        result.bluesky_uri = Some(uri.clone());

                        // Save a local copy as syndicated
                        let db_guard = state.db_manager.read().await;
                        if let Some(db) = db_guard.as_ref() {
                            let item_id = cid.clone();
                            let item = BytescapeItem {
                                id: cid,
                                original_timestamp: now as i64,
                                content: content.to_string(),
                                media_paths: vec![],
                                source: Source::Bluesky.as_str().to_string(),
                                path: willow_path_for_item(Source::Bluesky.as_str(), BytescapeType::Status, now as i64, &item_id),
                                imported_at: now,
                                title: None,
                                original_id: Some(uri.clone()),
                                syndicated_uris: std::collections::HashMap::new(),
                                syndicated_at: std::collections::HashMap::new(),
                                author_did: Some(user_did.clone()),
                                author_handle: Some(user_handle),
                                author_avatar: None,
                                tagged_people: vec![],
                                location: None,
                                is_liked: false,
                                like_count: None,
                                reply_count: None,
                                repost_count: None,
                                quote_count: None,
                                item_type: BytescapeType::Status,
                                url: Some(uri.clone()),
                                link_url: None,
                                content_html: None,
                                preview_img: None,
                                media_blobs: vec![],
                                thumbnail_blobs: vec![],
                                media_mimes: vec![],
                                syndication_status: SyndicationStatus::Syndicated,
                                syndicated_uri: Some(uri.clone()),
                                audiences: vec![Audience::Local, Audience::Bluesky],
                                share_history: vec![
                                    crate::models::SyndicationEvent {
                                        platform: "bluesky".to_string(),
                                        uri: uri.clone(),
                                        timestamp: now,
                                        active: true,
                                    }
                                ],
                                reposted_by_did: None,
                                reposted_by_handle: None,
                                reply_parent_uri: None,
                                reply_parent_handle: None,
                                collab_doc_id: None,
                            };
                            let _ = db.save_item(&item).await;
                        }
                    }
                }
                Err(e) => result.errors.push(format!("Bluesky publish failed: {}", e)),
            }
        }
    }

    // ActivityPub placeholder
    if publish_activitypub {
        result.errors.push("ActivityPub posting is not implemented yet (placeholder).".to_string());
    }

    Ok(result)
}
