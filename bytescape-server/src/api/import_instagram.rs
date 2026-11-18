use axum::{
    extract::{DefaultBodyLimit, Multipart, State},
    http::HeaderMap,
    routing::post,
    Json, Router,
};
use serde::Serialize;
use uuid::Uuid;
use tokio::io::AsyncWriteExt;
use std::io::Read;
use std::collections::HashMap;

use crate::{auth, db, error::AppError, AppState};
use crate::api::import::ContactEntry;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/import/instagram", post(import_instagram_archive))
        .layer(DefaultBodyLimit::disable())
}

fn extract_user_id(headers: &HeaderMap, secret: &str) -> Result<Uuid, AppError> {
    let auth = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Auth("Missing or invalid Authorization header".into()))?;
    let claims = auth::validate_token(auth, secret)?;
    Uuid::parse_str(&claims.sub).map_err(|_| AppError::Auth("Invalid user ID in token".into()))
}

#[derive(Serialize)]
struct ImportResponse {
    imported: usize,
    session_id: String,
    errors: Vec<String>,
}

fn parse_ig_timestamp(val: &serde_json::Value) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Some(ts) = val.as_i64() {
        return chrono::DateTime::from_timestamp(ts, 0)
            .or_else(|| chrono::DateTime::from_timestamp_millis(ts));
    }
    if let Some(s) = val.as_str() {
        return chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&chrono::Utc))
            .or_else(|| {
                chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                    .ok()
                    .and_then(|d| d.and_hms_opt(0, 0, 0).map(|t| t.and_utc()))
            });
    }
    None
}

fn guess_mime_type(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") { "image/jpeg" }
    else if lower.ends_with(".png") { "image/png" }
    else if lower.ends_with(".gif") { "image/gif" }
    else if lower.ends_with(".webp") { "image/webp" }
    else if lower.ends_with(".mp4") { "video/mp4" }
    else if lower.ends_with(".mov") { "video/quicktime" }
    else { "application/octet-stream" }
}

/// Resolve a media reference from export JSON (`uri`) or HTML (`src`/`href`)
/// to a ZIP entry index.
///
/// Export `uri` values are relative to the export root folder, while ZIP entry
/// names carry that folder as a prefix (e.g. entry
/// `instagram-user-2025-07-31-x/media/posts/1.jpg` vs uri
/// `media/posts/1.jpg`), so an exact match misses every media file in archives
/// that ship a root folder. Fall back to a suffix match on `/uri`; the lowest
/// ZIP index wins so lookup is deterministic.
fn resolve_media_zip_index(index: &HashMap<String, usize>, path: &str) -> Option<usize> {
    if let Some(&i) = index.get(path) {
        return Some(i);
    }
    let suffix = format!("/{}", path);
    index.iter()
        .filter(|(name, _)| name.ends_with(&suffix))
        .map(|(_, &i)| i)
        .min()
}

/// Is this ZIP entry a parseable content JSON? Instagram GDPR exports carry
/// `posts_1.json`, `reels.json`, `igtv_videos.json`, `stories.json`,
/// `archived_posts.json`, `recently_deleted_content.json`,
/// `profile_photos.json` (and multi-part `posts_2.json`, …) — all of which we
/// import. Comment JSONs are excluded.
fn is_content_json(name: &str) -> bool {
    if !name.ends_with(".json") || name.contains("comments") {
        return false;
    }
    let base = name.rsplit('/').next().unwrap_or(name);
    base.starts_with("posts_")
        || base.starts_with("reels")
        || base.starts_with("igtv_videos")
        || base.starts_with("stories")
        || base.starts_with("archived_posts")
        || base.starts_with("recently_deleted_content")
        || base.starts_with("profile_photos")
}

/// Is this ZIP entry a parseable content HTML (legacy HTML-format exports)?
fn is_content_html(name: &str) -> bool {
    if !name.ends_with(".html") || name.contains("comments") {
        return false;
    }
    let base = name.rsplit('/').next().unwrap_or(name);
    base.starts_with("posts_")
        || base.starts_with("reels")
        || base.starts_with("igtv_videos")
        || base.starts_with("stories")
        || base.starts_with("archived_posts")
        || base.starts_with("other_content")
        || base.starts_with("profile_photos")
}

/// Flatten a content JSON document into its list of entry objects. Export
/// files are either a bare array (`posts_1.json`) or a single-key wrapper
/// object (`reels.json` → `{"ig_reels_media": [...]}`, `igtv_videos.json` →
/// `{"ig_igtv_media": [...]}`, `stories.json` → `{"ig_stories": [...]}`).
/// The wrapper key varies, so we take any array-valued member.
fn content_entries(json_val: &serde_json::Value) -> Vec<serde_json::Value> {
    if let Some(arr) = json_val.as_array() {
        return arr.clone();
    }
    if let Some(obj) = json_val.as_object() {
        for v in obj.values() {
            if let Some(arr) = v.as_array() {
                return arr.clone();
            }
        }
    }
    vec![json_val.clone()]
}

/// Pull the media file references out of one content entry. Handles both
/// shapes: a `media` array of items each with a `uri`, and flat entries
/// (story items) that carry a top-level `uri` with no `media` array.
fn entry_media_paths(entry: &serde_json::Value) -> Vec<String> {
    if let Some(arr) = entry.get("media").and_then(|v| v.as_array()) {
        return arr
            .iter()
            .filter_map(|m| m.get("uri").and_then(|u| u.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string()))
            .collect();
    }
    match entry.get("uri").and_then(|u| u.as_str()).filter(|s| !s.is_empty()) {
        Some(u) => vec![u.to_string()],
        None => Vec::new(),
    }
}

/// Pull the entry's caption/title text (`title`, then first media `title`,
/// then `caption`).
fn entry_content_text(entry: &serde_json::Value) -> String {
    entry.get("title")
        .or_else(|| entry.get("media").and_then(|v| v.as_array()).and_then(|arr| arr.first()).and_then(|m| m.get("title")))
        .or_else(|| entry.get("caption"))
        .and_then(|v| v.as_str()).unwrap_or("").to_string()
}

/// Pull the entry's timestamp (entry-level `creation_timestamp`/`timestamp`,
/// falling back to the first media item's).
fn entry_timestamp(entry: &serde_json::Value) -> Option<chrono::DateTime<chrono::Utc>> {
    entry.get("creation_timestamp")
        .or_else(|| entry.get("timestamp"))
        .or_else(|| entry.get("media").and_then(|v| v.as_array()).and_then(|arr| arr.first()).and_then(|m| m.get("creation_timestamp")))
        .and_then(parse_ig_timestamp)
}

struct ParsedPost {
    content_text: String,
    original_ts: chrono::DateTime<chrono::Utc>,
    zip_media_paths: Vec<String>,
}

#[derive(Clone)]
struct ExtractedMedia {
    #[allow(dead_code)]
    dest_path: String,
    url: String,
    mime: String,
    file_size: i64,
}

async fn import_instagram_archive(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<ImportResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let temp_path = format!("/tmp/bytescape_ig_{}.zip", Uuid::new_v4());
    let mut temp_file = tokio::fs::File::create(&temp_path).await
        .map_err(|e| AppError::Internal(format!("Failed to create temp file: {}", e)))?;

    let mut found_field = false;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Multipart error: {}", e)))?
    {
        if field.name() == Some("file") {
            found_field = true;
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|e| AppError::BadRequest(format!("Stream error: {}", e)))?
            {
                temp_file.write_all(&chunk).await
                    .map_err(|e| AppError::Internal(format!("Write error: {}", e)))?;
            }
            break;
        }
    }

    if !found_field {
        tokio::fs::remove_file(&temp_path).await.ok();
        return Err(AppError::BadRequest("No file provided".into()));
    }

    let (imported, session_id, errors) = import_instagram_archive_from_path(
        &state, user_id, &temp_path).await?;

    tokio::fs::remove_file(&temp_path).await.ok();

    Ok(Json(ImportResponse { imported, session_id, errors }))
}

/// Public entry point for unified import.
pub async fn import_instagram_archive_from_path(
    state: &AppState,
    user_id: Uuid,
    temp_path: &str,
) -> Result<(usize, String, Vec<String>), AppError> {
    let session = db::imports::ImportRow::create(&state.db, user_id, "instagram")
        .await
        .map_err(AppError::Database)?;

    db::imports::ImportRow::start(&state.db, session.id).await.ok();

    let mut errors: Vec<String> = Vec::new();

    // ---- Phase 1: Extract everything from ZIP synchronously ----
    tokio::fs::create_dir_all("./uploads/import").await.ok();

    let std_file = std::fs::File::open(temp_path)
        .map_err(|e| AppError::BadRequest(format!("Failed to open uploaded file: {}", e)))?;
    let mut archive = zip::ZipArchive::new(std_file)
        .map_err(|e| AppError::BadRequest(format!("Invalid ZIP: {}", e)))?;

    let mut media_index: HashMap<String, usize> = HashMap::new();
    let mut posts_json_files: Vec<(usize, String)> = Vec::new();
    let mut posts_html_files: Vec<(usize, String)> = Vec::new();
    for i in 0..archive.len() {
        if let Ok(entry) = archive.by_index(i) {
            let name = entry.name().to_string();
            // Content JSON first: real exports ship them under
            // `your_instagram_activity/media/`, which also matches the media
            // path pattern below — checking media first would swallow them.
            // This covers posts_*.json AND reels/igtv/stories/archived/
            // profile_photos/recently_deleted JSON (videos live there).
            if is_content_json(&name) {
                posts_json_files.push((i, name));
            } else if is_content_html(&name) {
                posts_html_files.push((i, name));
            } else if (name.starts_with("media/") || name.contains("/media/")) && !entry.is_dir() {
                media_index.insert(name.clone(), i);
            }
        }
    }

    // Parse JSON post data
    let mut parsed_posts: Vec<ParsedPost> = Vec::new();
    for (idx, path) in &posts_json_files {
        let content = {
            let mut entry = match archive.by_index(*idx) { Ok(e) => e, Err(_) => continue };
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).ok();
            buf
        };
        if content.is_empty() { continue; }
        let json_val: serde_json::Value = match serde_json::from_slice(&content) {
            Ok(v) => v,
            Err(e) => { errors.push(format!("JSON parse error in {}: {}", path, e)); continue; }
        };
        let posts_data: Vec<serde_json::Value> = content_entries(&json_val);
        for post_val in &posts_data {
            let content_text = entry_content_text(post_val);
            let zip_media_paths: Vec<String> = entry_media_paths(post_val);
            // Caption-less posts with media are valid (photo/video posts) —
            // only skip entries that carry neither text nor media, matching
            // the HTML fallback path.
            if content_text.trim().is_empty() && zip_media_paths.is_empty() { continue; }
            let original_ts = entry_timestamp(post_val).unwrap_or_else(chrono::Utc::now);
            parsed_posts.push(ParsedPost { content_text, original_ts, zip_media_paths });
        }
    }

    // HTML fallback for legacy HTML-format exports (no content JSON): parse the
    // content HTML files. The async path previously skipped these entirely, so
    // an HTML-format Instagram export imported zero posts.
    if parsed_posts.is_empty() {
        for (idx, _path) in &posts_html_files {
            let mut entry = match archive.by_index(*idx) { Ok(e) => e, Err(_) => continue };
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).ok();
            if buf.is_empty() { continue; }
            let html = String::from_utf8_lossy(&buf);
            let noop = |_v: serde_json::Value| {};
            let mut sync_parsed: Vec<SyncParsedPost> = Vec::new();
            let sync_out = parse_instagram_threads_html(&html, &mut media_index, &noop, &mut sync_parsed);
            for sp in sync_out {
                if sp.content_text.trim().is_empty() && sp.zip_media_paths.is_empty() { continue; }
                parsed_posts.push(ParsedPost {
                    content_text: sp.content_text,
                    original_ts: sp.original_ts,
                    zip_media_paths: sp.zip_media_paths,
                });
            }
        }
    }

    // Extract media files from ZIP, keyed by post index (content_text is not
    // unique — duplicate or empty captions would collide on a text key).
    let mut extracted_media: Vec<Vec<ExtractedMedia>> = Vec::with_capacity(parsed_posts.len());
    for post in &parsed_posts {
        let mut media_for_post = Vec::new();
        for media_path in &post.zip_media_paths {
            if let Some(zip_idx) = resolve_media_zip_index(&media_index, media_path) {
                let filename = std::path::Path::new(media_path).file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
                let served_filename = format!("{}_{}", Uuid::new_v4(), filename);
                let dest_path = format!("./uploads/import/{}", served_filename);
                let url = format!("/uploads/import/{}", served_filename);
                let mime = guess_mime_type(filename).to_string();
                if let Ok(mut entry) = archive.by_index(zip_idx) {
                    let mut file_data = Vec::new();
                    if entry.read_to_end(&mut file_data).is_ok()
                        && std::fs::write(&dest_path, &file_data).is_ok() {
                            media_for_post.push(ExtractedMedia { dest_path, url, mime, file_size: file_data.len() as i64 });
                        }
                }
            }
        }
        extracted_media.push(media_for_post);
    }

    // Drop borrow before async phase
    drop(archive);

    // ---- Phase 2: DB operations (async) ----
    let mut imported = 0usize;
    let mut imported_media = 0usize;
    for (post_idx, post) in parsed_posts.into_iter().enumerate() {
        let em_list = extracted_media.get(post_idx).cloned().unwrap_or_default();

        // Create MediaRow entries
        let mut served_urls: Vec<String> = Vec::new();
        for em in &em_list {
            if let Err(e) = db::media::MediaRow::create_for_user(
                &state.db, user_id, &em.url, None, &em.mime, None, None, None, Some(em.file_size),
            ).await {
                errors.push(format!("Media DB error: {}", e));
                std::fs::remove_file(&em.dest_path).ok();
            } else {
                served_urls.push(em.url.clone());
            }
        }

        let content_json = if served_urls.is_empty() {
            serde_json::json!({"text": post.content_text, "display_name": None::<String>, "action_text": None::<String>, "group_name": None::<String>})
        } else {
            serde_json::json!({"text": post.content_text, "display_name": None::<String>, "action_text": None::<String>, "group_name": None::<String>, "media_urls": served_urls})
        };

        match db::posts::PostRow::create(
            &state.db, user_id, "instagram", None, None, &content_json, "short",
            None, None, None, post.original_ts, true, Some(session.id), "public",
        ).await {
            Ok(_) => {
                imported += 1;
                imported_media += served_urls.len();
            }
            Err(e) => errors.push(format!("DB insert error: {}", e)),
        }
    }

    if imported > 0 {
        db::imports::ImportRow::complete(&state.db, session.id, imported as i32, imported_media as i32).await.ok();
    } else {
        db::imports::ImportRow::fail(&state.db, session.id, &format!("No posts imported. Errors: {}", errors.join("; "))).await.ok();
    }

    Ok((imported, session.id.to_string(), errors))
}

// ─── Contact parsing ────────────────────────────────────────────────────────

/// Parse Instagram following from `connections/followers_and_following/following.json`.
/// Format: `{"relationships_following": [{"title": "Name", "string_list_data": [{"value": "handle"}]}]}`
pub fn parse_instagram_following_json(data: &[u8]) -> Vec<ContactEntry> {
    let mut contacts = Vec::new();
    let json_val: serde_json::Value = match serde_json::from_slice(data) {
        Ok(v) => v,
        Err(_) => return contacts,
    };
    if let Some(arr) = json_val.get("relationships_following").and_then(|v| v.as_array()) {
        for entry in arr {
            let name = entry.get("title").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            let handle = entry.get("string_list_data")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.get("value"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if !name.is_empty() {
                contacts.push(ContactEntry {
                    contact_name: name,
                    contact_handle: handle,
                    contact_platform_url: None,
                    source: "instagram".to_string(),
                    remote_handle: None,
                    direction: None,
                });
            }
        }
    }
    contacts
}

/// Parse Instagram followers from `connections/followers_and_following/followers_1.json`.
/// Format: Array of `{"string_list_data": [{"value": "handle"}]}` (no name field)
pub fn parse_instagram_followers_json(data: &[u8]) -> Vec<ContactEntry> {
    let mut contacts = Vec::new();
    let json_val: serde_json::Value = match serde_json::from_slice(data) {
        Ok(v) => v,
        Err(_) => return contacts,
    };
    if let Some(arr) = json_val.as_array() {
        for entry in arr {
            let handle = entry.get("string_list_data")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.get("value"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if let Some(ref h) = handle {
                if !h.is_empty() {
                    contacts.push(ContactEntry {
                        contact_name: h.clone(),
                        contact_handle: handle,
                        contact_platform_url: None,
                        source: "instagram".to_string(),
                        remote_handle: None,
                        direction: None,
                    });
                }
            }
        }
    }
    contacts
}

/// Parse Threads following from `threads/following.json`.
/// Format: `{"text_post_app_text_post_app_following": [{"title": "Name", "string_list_data": [{"value": "handle"}]}]}`
pub fn parse_threads_following_json(data: &[u8]) -> Vec<ContactEntry> {
    let mut contacts = Vec::new();
    let json_val: serde_json::Value = match serde_json::from_slice(data) {
        Ok(v) => v,
        Err(_) => return contacts,
    };
    let arr = json_val.get("text_post_app_text_post_app_following")
        .and_then(|v| v.as_array())
        .or_else(|| json_val.as_array());
    if let Some(arr) = arr {
        for entry in arr {
            let name = entry.get("title").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            let handle = entry.get("string_list_data")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.get("value"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if !name.is_empty() {
                contacts.push(ContactEntry {
                    contact_name: name,
                    contact_handle: handle,
                    contact_platform_url: None,
                    source: "threads".to_string(),
                    remote_handle: None,
                    direction: None,
                });
            }
        }
    }
    contacts
}

/// Parse Threads followers from `threads/followers.json`.
/// Format: `{"text_post_app_text_post_app_followers": [{"title": "Name", "string_list_data": [{"value": "handle"}]}]}`
pub fn parse_threads_followers_json(data: &[u8]) -> Vec<ContactEntry> {
    let mut contacts = Vec::new();
    let json_val: serde_json::Value = match serde_json::from_slice(data) {
        Ok(v) => v,
        Err(_) => return contacts,
    };
    let arr = json_val.get("text_post_app_text_post_app_followers")
        .and_then(|v| v.as_array())
        .or_else(|| json_val.as_array());
    if let Some(arr) = arr {
        for entry in arr {
            let name = entry.get("title").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            let handle = entry.get("string_list_data")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.get("value"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if !name.is_empty() || handle.is_some() {
                contacts.push(ContactEntry {
                    contact_name: name,
                    contact_handle: handle,
                    contact_platform_url: None,
                    source: "threads".to_string(),
                    remote_handle: None,
                    direction: None,
                });
            }
        }
    }
    contacts
}

/// Parse Threads/Instagram HTML follow files (following.html, followers.html).
/// Structure: div._a6-g > h2._2pim._a6-h._a6-i (name) > div._a6-p > a[href] (handle)
/// `source` is the archive origin ("instagram" or "threads") stored on each entry.
pub fn parse_follow_html(data: &[u8], source: &str) -> Vec<ContactEntry> {
    let html = String::from_utf8_lossy(data);
    let document = scraper::Html::parse_document(&html);
    let mut contacts = Vec::new();

    let entry_sel = match scraper::Selector::parse("div._a6-g") {
        Ok(s) => s,
        Err(_) => return contacts,
    };
    let h2_sel = scraper::Selector::parse("h2._a6-h").unwrap();
    let a_sel = scraper::Selector::parse("a[href]").unwrap();

    for entry in document.select(&entry_sel) {
        let name = entry.select(&h2_sel).next()
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .unwrap_or_default();

        let handle = entry.select(&a_sel).next()
            .and_then(|el| {
                let text = el.text().collect::<Vec<_>>().join("").trim().to_string();
                if text.is_empty() { None } else { Some(text) }
            });

        if !name.is_empty() || handle.is_some() {
            contacts.push(ContactEntry {
                contact_name: name,
                contact_handle: handle,
                contact_platform_url: None,
                source: source.to_string(),
                remote_handle: None,
                direction: None,
            });
        }
    }

    contacts
}

// ─── Sync parsers for streaming endpoint ────────────────────────────────────

use crate::api::import::StdSyncSender;

/// Parse Instagram/Threads HTML using the _a6-g DOM structure.
/// Both platforms use the same HTML format with:
/// - h2._2pim._a6-h._a6-i for post text
/// - img[src] / a[href] for media (paths like "media/posts/...")
/// - div._3-94._a6-o for timestamp (date string like "Oct 25, 2024 6:08 am")
/// - Table rows with "Is reply" / "In reply to" for reply detection
fn parse_instagram_threads_html(
    html: &str,
    media_index: &mut HashMap<String, usize>,
    send: &impl Fn(serde_json::Value),
    parsed: &mut Vec<SyncParsedPost>,
) -> Vec<SyncParsedPost> {
    let document = scraper::Html::parse_document(html);

    let entry_sel = match scraper::Selector::parse("div._a6-g") {
        Ok(s) => s,
        Err(_) => return std::mem::take(parsed),
    };
    let h2_sel = scraper::Selector::parse("h2._a6-h").unwrap();
    let img_sel = scraper::Selector::parse("img[src]").unwrap();
    let video_sel = scraper::Selector::parse("video[src]").unwrap();
    let source_sel = scraper::Selector::parse("video source[src]").unwrap();
    let a_sel = scraper::Selector::parse("a[href]").unwrap();
    let ts_sel = scraper::Selector::parse("div._a6-o").unwrap();
    let table_sel = scraper::Selector::parse("table").unwrap();
    let label_sel = scraper::Selector::parse("div._a6-q").unwrap();

    for entry in document.select(&entry_sel) {
        // Check if it's a reply (look for "Is reply" + "true" in table)
        let mut is_reply = false;
        for table in entry.select(&table_sel) {
            let labels: Vec<String> = table.select(&label_sel).map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string()).collect();
            if labels.iter().any(|l| l == "Is reply") {
                // Check the next div for "true"
                let all_divs: Vec<String> = table.select(&label_sel).map(|el| {
                    el.parent()
                        .and_then(|p| p.first_child())
                        .and_then(|c| c.next_sibling())
                        .and_then(|s| scraper::ElementRef::wrap(s))
                        .map(|er| er.text().collect::<Vec<_>>().join(" ").trim().to_string())
                        .unwrap_or_default()
                }).collect();
                if all_divs.iter().any(|d| d == "true") {
                    is_reply = true;
                    break;
                }
            }
        }
        if is_reply { continue; }

        // Extract post text from h2 — skip metadata section labels
        let content_text = entry.select(&h2_sel).next()
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .filter(|t| {
                !t.is_empty()
                && t != "Place" && t != "Media" && t != "Hashtags" && t != "Owner"
                && t != "Date" && t != "Device ID" && t != "Date taken"
                && t != "Scene capture type" && t != "Software"
                && t != "Source type" && t != "Marked as AI generated"
            })
            .unwrap_or_default();

        // Extract media from img src, video/source src, and a href
        let mut zip_media_paths = Vec::new();
        for img_el in entry.select(&img_sel) {
            if let Some(src) = img_el.value().attr("src") {
                if !src.is_empty() && (src.contains("media/") || src.contains("/posts/")) {
                    zip_media_paths.push(src.to_string());
                }
            }
        }
        for v_el in entry.select(&video_sel).chain(entry.select(&source_sel)) {
            if let Some(src) = v_el.value().attr("src") {
                if !src.is_empty() && (src.contains("media/") || src.contains("/posts/")) && !zip_media_paths.contains(&src.to_string()) {
                    zip_media_paths.push(src.to_string());
                }
            }
        }
        for a_el in entry.select(&a_sel) {
            if let Some(href) = a_el.value().attr("href") {
                if !href.is_empty() && (href.contains("media/") || href.contains("/posts/")) && !zip_media_paths.contains(&href.to_string()) {
                    zip_media_paths.push(href.to_string());
                }
            }
        }

        // Extract timestamp from div._a6-o (e.g. "Oct 25, 2024 6:08 am")
        let date_text = entry.select(&ts_sel).next()
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .unwrap_or_default();
        let original_ts = parse_ig_date_string(&date_text).unwrap_or_else(chrono::Utc::now);

        // Skip if no text and no media
        if content_text.trim().is_empty() && zip_media_paths.is_empty() {
            continue;
        }

        send(serde_json::json!({
            "type":"post",
            "title":content_text.chars().take(100).collect::<String>(),
            "timestamp":original_ts.to_rfc3339(),
            "media_count":zip_media_paths.len(),
            "index":parsed.len() + 1,
        }));

        parsed.push(SyncParsedPost { content_text, original_ts, media_urls: vec![], zip_media_paths });
    }

    std::mem::take(parsed)
}

/// Parse date strings like "Oct 25, 2024 6:08 am" or "Jun 20, 2026 6:31 pm"
fn parse_ig_date_string(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    // Try parsing with chrono's format
    // Format: "MMM d, yyyy h:mm am/pm"
    let trimmed = s.trim();
    // Use chrono::NaiveDateTime parsing
    let months = [("Jan",1),("Feb",2),("Mar",3),("Apr",4),("May",5),("Jun",6),
                  ("Jul",7),("Aug",8),("Sep",9),("Oct",10),("Nov",11),("Dec",12)];
    for (mon_name, mon_num) in months {
        if trimmed.starts_with(mon_name) {
            let rest = &trimmed[3..].trim();
            // Parse "d, yyyy h:mm am" or "d, yyyy h:mm pm"
            let parts: Vec<&str> = rest.splitn(2, ',').collect();
            if parts.len() != 2 { continue; }
            let day: u32 = parts[0].trim().parse().ok()?;
            let rest2 = parts[1].trim();
            // Parse "yyyy h:mm am" or "yyyy h:mm pm"
            let parts2: Vec<&str> = rest2.splitn(2, ' ').collect();
            if parts2.len() < 2 { continue; }
            let year: i32 = parts2[0].trim().parse().ok()?;
            let time_part = parts2[1..].join(" ");
            // Parse "h:mm am/pm"
            let time_lower = time_part.trim().to_lowercase();
            let (time_str, is_pm) = if time_lower.ends_with("pm") {
                (time_lower[..time_lower.len()-2].trim(), true)
            } else if time_lower.ends_with("am") {
                (time_lower[..time_lower.len()-2].trim(), false)
            } else {
                (time_lower.as_str(), false)
            };
            let time_parts: Vec<&str> = time_str.split(':').collect();
            if time_parts.len() != 2 { continue; }
            let mut hour: u32 = time_parts[0].trim().parse().ok()?;
            let min: u32 = time_parts[1].trim().parse().ok()?;
            if is_pm && hour < 12 { hour += 12; }
            if !is_pm && hour == 12 { hour = 0; }
            return chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, year, mon_num, day, hour, min, 0).single();
        }
    }
    None
}

#[derive(Clone)]
struct SyncParsedPost {
    content_text: String,
    original_ts: chrono::DateTime<chrono::Utc>,
    media_urls: Vec<String>,
    zip_media_paths: Vec<String>,
}

/// Parse Threads archive synchronously — sends SSE events through the channel.
pub fn parse_threads_archive_sync(
    temp_path: &str,
    tx: &StdSyncSender,
) -> (Vec<crate::api::import_facebook::ParsedPost>, Vec<String>) {
    let mut errors = Vec::new();
    let send = |val: serde_json::Value| { let _ = tx.send(val.to_string()); };

    send(serde_json::json!({"type":"phase","phase":"scanning_archive","total":0}));

    let std_file = match std::fs::File::open(temp_path) {
        Ok(f) => f,
        Err(e) => {
            send(serde_json::json!({"type":"error","message":format!("Failed to open file: {}", e)}));
            return (vec![], vec![format!("Failed to open file: {}", e)]);
        }
    };
    let mut archive = match zip::ZipArchive::new(std_file) {
        Ok(a) => a,
        Err(e) => {
            send(serde_json::json!({"type":"error","message":format!("Invalid ZIP: {}", e)}));
            return (vec![], vec![format!("Invalid ZIP: {}", e)]);
        }
    };

    let mut media_index: HashMap<String, usize> = HashMap::new();
    let mut posts_json_idx: Option<usize> = None;
    let mut posts_html_idx: Option<usize> = None;
    let _ = std::fs::create_dir_all("./uploads/import");

    for i in 0..archive.len() {
        let name = {
            let entry = match archive.by_index(i) { Ok(e) => e, Err(_) => continue };
            entry.name().to_string()
        };
        if name.contains("threads_and_replies") && name.ends_with(".json") {
            posts_json_idx = Some(i);
        }
        if name.contains("threads_and_replies") && name.ends_with(".html") {
            posts_html_idx = Some(i);
        }
        if (name.starts_with("media/") || name.contains("/media/posts/") || name.contains("/media/other/")) && !name.ends_with('/') {
            media_index.insert(name.clone(), i);
        }
    }

    send(serde_json::json!({"type":"phase","phase":"archive_scanned","media_count":media_index.len(),"html_files":if posts_html_idx.is_some(){1}else{0},"json_files":if posts_json_idx.is_some(){1}else{0}}));

    let mut parsed: Vec<SyncParsedPost> = Vec::new();

    // Try JSON first
    if let Some(idx) = posts_json_idx {
        send(serde_json::json!({"type":"phase","phase":"parsing_posts","total":1}));
        let mut entry = match archive.by_index(idx) { Ok(e) => e, Err(_) => { return (vec![], vec!["Failed to read posts JSON".to_string()]); } };
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).ok();
        if !buf.is_empty() {
            if let Ok(json_val) = serde_json::from_slice::<serde_json::Value>(&buf) {
                let posts_arr = json_val.get("text_post_app_text_posts")
                    .and_then(|v| v.as_array())
                    .or_else(|| json_val.as_array())
                    .cloned()
                    .unwrap_or_default();

                for post_val in &posts_arr {
                    let is_reply = post_val.get("media")
                        .and_then(|v| v.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|m| m.get("text_app_post"))
                        .and_then(|t| t.get("is_reply"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if is_reply { continue; }

                    let media_arr = post_val.get("media").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                    let content_text = media_arr.first()
                        .and_then(|m| m.get("title"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let zip_media_paths: Vec<String> = media_arr.iter()
                        .filter_map(|m| m.get("uri").and_then(|u| u.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string()))
                        .collect();

                    // Caption-less threads with media are valid — only skip
                    // entries carrying neither text nor media.
                    if content_text.trim().is_empty() && zip_media_paths.is_empty() { continue; }

                    let original_ts = media_arr.first()
                        .and_then(|m| m.get("creation_timestamp"))
                        .and_then(parse_ig_timestamp)
                        .unwrap_or_else(chrono::Utc::now);

                    send(serde_json::json!({
                        "type":"post",
                        "title":content_text.chars().take(100).collect::<String>(),
                        "timestamp":original_ts.to_rfc3339(),
                        "media_count":zip_media_paths.len(),
                        "index":parsed.len() + 1,
                    }));

                    parsed.push(SyncParsedPost { content_text, original_ts, media_urls: vec![], zip_media_paths });
                }
            }
        }
    }

    // Fall back to HTML if no JSON or JSON produced no posts
    if parsed.is_empty() {
        if let Some(idx) = posts_html_idx {
            send(serde_json::json!({"type":"phase","phase":"parsing_posts","total":1}));
            let mut entry = match archive.by_index(idx) { Ok(e) => e, Err(_) => { return (vec![], vec!["Failed to read posts HTML".to_string()]); } };
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).ok();
            if !buf.is_empty() {
                let html = String::from_utf8_lossy(&buf);
                parsed = parse_instagram_threads_html(&html, &mut media_index, &send, &mut parsed);
            }
        }
    }

    // Extract media
    send(serde_json::json!({"type":"phase","phase":"extracting_media","total":parsed.len()}));
    let mut media_idx = 0u64;
    for post in &mut parsed {
        for media_path in &post.zip_media_paths {
            if let Some(zip_idx) = resolve_media_zip_index(&media_index, media_path) {
                let filename = std::path::Path::new(media_path).file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
                let served_filename = format!("{}_{}", Uuid::new_v4(), filename);
                let dest_path = format!("./uploads/import/{}", served_filename);
                let url = format!("/uploads/import/{}", served_filename);
                if let Ok(mut entry) = archive.by_index(zip_idx) {
                    let mut file_data = Vec::new();
                    if entry.read_to_end(&mut file_data).is_ok() && std::fs::write(&dest_path, &file_data).is_ok() {
                        post.media_urls.push(url);
                        media_idx += 1;
                        let category = if media_path.ends_with(".mp4") || media_path.ends_with(".mov") { "video" } else { "photo" };
                        send(serde_json::json!({"type":"media","category":category,"filename":filename,"index":media_idx}));
                    }
                }
            }
        }
    }

    let result: Vec<crate::api::import_facebook::ParsedPost> = parsed.into_iter().map(|p| {
        crate::api::import_facebook::ParsedPost {
            content_text: p.content_text,
            display_name: None,
            action_text: Some("posted".to_string()),
            group_name: None,
            link_url: None,
            link_title: None,
            original_ts: p.original_ts,
            media_urls: p.media_urls,
        }
    }).collect();

    send(serde_json::json!({"type":"phase","phase":"deduplicating","total":result.len()}));
    (result, errors)
}

/// Parse Instagram archive synchronously — sends SSE events through the channel.
pub fn parse_instagram_archive_sync(
    temp_path: &str,
    tx: &StdSyncSender,
) -> (Vec<crate::api::import_facebook::ParsedPost>, Vec<String>) {
    let mut errors = Vec::new();
    let send = |val: serde_json::Value| { let _ = tx.send(val.to_string()); };

    send(serde_json::json!({"type":"phase","phase":"scanning_archive","total":0}));

    let std_file = match std::fs::File::open(temp_path) {
        Ok(f) => f,
        Err(e) => {
            send(serde_json::json!({"type":"error","message":format!("Failed to open file: {}", e)}));
            return (vec![], vec![format!("Failed to open file: {}", e)]);
        }
    };
    let mut archive = match zip::ZipArchive::new(std_file) {
        Ok(a) => a,
        Err(e) => {
            send(serde_json::json!({"type":"error","message":format!("Invalid ZIP: {}", e)}));
            return (vec![], vec![format!("Invalid ZIP: {}", e)]);
        }
    };

    let mut media_index: HashMap<String, usize> = HashMap::new();
    let mut posts_json_files: Vec<(usize, String)> = Vec::new();
    let mut posts_html_files: Vec<(usize, String)> = Vec::new();
    let _ = std::fs::create_dir_all("./uploads/import");

    for i in 0..archive.len() {
        let name = {
            let entry = match archive.by_index(i) { Ok(e) => e, Err(_) => continue };
            entry.name().to_string()
        };
        if (name.starts_with("media/") || name.contains("/media/posts/") || name.contains("/media/")) && !name.ends_with('/') {
            media_index.insert(name.clone(), i);
        }
        // Content JSON: posts_*.json plus reels/igtv/stories/archived/
        // profile_photos/recently_deleted (videos live there).
        if is_content_json(&name) {
            posts_json_files.push((i, name.clone()));
        }
        // Content HTML (legacy HTML-format exports): posts_N.html and the
        // other content files (reels.html, stories.html, …). Skip summary
        // pages like posts.html (no trailing _N).
        if is_content_html(&name) {
            posts_html_files.push((i, name.clone()));
        }
    }

    send(serde_json::json!({"type":"phase","phase":"archive_scanned","media_count":media_index.len(),"html_files":posts_html_files.len(),"json_files":posts_json_files.len()}));

    let mut parsed: Vec<SyncParsedPost> = Vec::new();

    // Try JSON first
    for (idx, path) in &posts_json_files {
        send(serde_json::json!({"type":"phase","phase":"parsing_posts","total":posts_json_files.len(),"progress":parsed.len()}));
        let mut entry = match archive.by_index(*idx) { Ok(e) => e, Err(_) => continue };
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).ok();
        if buf.is_empty() { continue; }
        if let Ok(json_val) = serde_json::from_slice::<serde_json::Value>(&buf) {
            let posts_data: Vec<serde_json::Value> = content_entries(&json_val);
            for post_val in &posts_data {
                let content_text = entry_content_text(post_val);
                let zip_media_paths: Vec<String> = entry_media_paths(post_val);
                // Caption-less photo/video posts are valid — only skip entries
                // that carry neither text nor media, matching the HTML fallback.
                if content_text.trim().is_empty() && zip_media_paths.is_empty() { continue; }
                let original_ts = entry_timestamp(post_val).unwrap_or_else(chrono::Utc::now);

                send(serde_json::json!({
                    "type":"post",
                    "title":content_text.chars().take(100).collect::<String>(),
                    "timestamp":original_ts.to_rfc3339(),
                    "media_count":zip_media_paths.len(),
                    "index":parsed.len() + 1,
                }));

                parsed.push(SyncParsedPost { content_text, original_ts, media_urls: vec![], zip_media_paths });
            }
        }
    }

    // Fall back to HTML if JSON produced no posts
    if parsed.is_empty() {
        for (idx, _path) in &posts_html_files {
            send(serde_json::json!({"type":"phase","phase":"parsing_posts","total":posts_html_files.len(),"progress":parsed.len()}));
            let mut entry = match archive.by_index(*idx) { Ok(e) => e, Err(_) => continue };
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).ok();
            if buf.is_empty() { continue; }
            let html = String::from_utf8_lossy(&buf);
            parsed = parse_instagram_threads_html(&html, &mut media_index, &send, &mut parsed);
        }
    }

    // Extract media
    send(serde_json::json!({"type":"phase","phase":"extracting_media","total":parsed.len()}));
    let mut media_idx = 0u64;
    for post in &mut parsed {
        for media_path in &post.zip_media_paths {
            if let Some(zip_idx) = resolve_media_zip_index(&media_index, media_path) {
                let filename = std::path::Path::new(media_path).file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
                let served_filename = format!("{}_{}", Uuid::new_v4(), filename);
                let dest_path = format!("./uploads/import/{}", served_filename);
                let url = format!("/uploads/import/{}", served_filename);
                if let Ok(mut entry) = archive.by_index(zip_idx) {
                    let mut file_data = Vec::new();
                    if entry.read_to_end(&mut file_data).is_ok() && std::fs::write(&dest_path, &file_data).is_ok() {
                        post.media_urls.push(url);
                        media_idx += 1;
                        let category = if media_path.ends_with(".mp4") || media_path.ends_with(".mov") { "video" } else { "photo" };
                        send(serde_json::json!({"type":"media","category":category,"filename":filename,"index":media_idx}));
                    }
                }
            }
        }
    }

    let result: Vec<crate::api::import_facebook::ParsedPost> = parsed.into_iter().map(|p| {
        crate::api::import_facebook::ParsedPost {
            content_text: p.content_text,
            display_name: None,
            action_text: Some("posted".to_string()),
            group_name: None,
            link_url: None,
            link_title: None,
            original_ts: p.original_ts,
            media_urls: p.media_urls,
        }
    }).collect();

    send(serde_json::json!({"type":"phase","phase":"deduplicating","total":result.len()}));
    (result, errors)
}

// ─── Fixture archives ────────────────────────────────────────────────────────

/// Fixture ZIP builders shared with the contact-extraction tests in
/// `api::import`. Each builder writes a small archive with known counts and
/// deletes it from the temp dir when dropped.
#[cfg(test)]
pub(crate) mod fixtures {
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use uuid::Uuid;

    /// A generated fixture ZIP; removed from disk on drop.
    pub struct FixtureZip {
        path: PathBuf,
    }

    impl FixtureZip {
        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for FixtureZip {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    pub fn build_zip(name: &str, entries: Vec<(String, Vec<u8>)>) -> FixtureZip {
        let path = std::env::temp_dir()
            .join(format!("bytescape_fixture_{}_{}.zip", name, Uuid::new_v4()));
        let file = std::fs::File::create(&path).expect("create fixture zip");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (entry_name, data) in &entries {
            zip.start_file(entry_name.as_str(), options).expect("add fixture entry");
            zip.write_all(data).expect("write fixture entry");
        }
        zip.finish().expect("finish fixture zip");
        FixtureZip { path }
    }

    // ── Instagram fixture ────────────────────────────────────────────────────
    // Mirrors the real Meta "Download your information" JSON export layout:
    // everything under a root folder, posts JSON at
    // `your_instagram_activity/media/posts_1.json`, media files at
    // `media/posts/...` referenced by root-relative `uri` values.

    pub const IG_ROOT: &str = "instagram-testuser-2025-07-31-ab12cd";

    pub const IG_POST_TEXTS: [&str; 4] = [
        "Sunset at the lake #pdx",
        "Fresh sourdough from the oven",
        "Trail run: 12k before sunrise",
        "Year-end recap of small wins",
    ];
    /// Known post timestamps (unix seconds), ascending.
    pub const IG_POST_TS: [i64; 4] = [1729000000, 1730700000, 1732300000, 1735100000];
    /// Which posts carry an image (posts 0 and 2 → 2 media total).
    pub const IG_POSTS_WITH_MEDIA: [bool; 4] = [true, false, true, false];

    pub const JPEG_A: &[u8] =
        b"\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01fixture-jpeg-image-alpha-0123456789";
    pub const JPEG_B: &[u8] =
        b"\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01fixture-jpeg-image-bravo-0123456789";

    pub fn instagram_archive() -> FixtureZip {
        let posts_json = serde_json::json!([
            {
                "title": IG_POST_TEXTS[0],
                "creation_timestamp": IG_POST_TS[0],
                "media": [
                    {"uri": "media/posts/202410/IMG_0001.jpg",
                     "creation_timestamp": IG_POST_TS[0], "title": ""}
                ]
            },
            {
                "title": IG_POST_TEXTS[1],
                "creation_timestamp": IG_POST_TS[1],
                "media": []
            },
            {
                "title": IG_POST_TEXTS[2],
                "creation_timestamp": IG_POST_TS[2],
                "media": [
                    {"uri": "media/posts/202411/IMG_0002.jpg",
                     "creation_timestamp": IG_POST_TS[2], "title": ""}
                ]
            },
            {
                "title": IG_POST_TEXTS[3],
                "creation_timestamp": IG_POST_TS[3],
                "media": []
            }
        ])
        .to_string();

        // 3 accounts followed (name + handle), spec R3.
        let following_json = serde_json::json!({
            "relationships_following": [
                {"title": "Alice Artist",
                 "string_list_data": [{"value": "alice_artist", "timestamp": 1729000000}]},
                {"title": "Bob Builder",
                 "string_list_data": [{"value": "bob_builder", "timestamp": 1729000001}]},
                {"title": "Carol Cooks",
                 "string_list_data": [{"value": "carol_cooks", "timestamp": 1729000002}]}
            ]
        })
        .to_string();

        // 2 followers (handle only, no name), spec R3.
        let followers_json = serde_json::json!([
            {"string_list_data": [{"value": "dave_reads", "timestamp": 1729000003}]},
            {"string_list_data": [{"value": "erin_writes", "timestamp": 1729000004}]}
        ])
        .to_string();

        build_zip(
            "instagram",
            vec![
                (format!("{}/your_instagram_activity/media/posts_1.json", IG_ROOT),
                 posts_json.into_bytes()),
                (format!("{}/media/posts/202410/IMG_0001.jpg", IG_ROOT), JPEG_A.to_vec()),
                (format!("{}/media/posts/202411/IMG_0002.jpg", IG_ROOT), JPEG_B.to_vec()),
                (format!("{}/connections/followers_and_following/following.json", IG_ROOT),
                 following_json.into_bytes()),
                (format!("{}/connections/followers_and_following/followers.json", IG_ROOT),
                 followers_json.into_bytes()),
            ],
        )
    }

    // ── Instagram video fixture ──────────────────────────────────────────────
    // Covers every place videos were historically dropped: a feed post with a
    // .mp4 in `media[].uri`, a Reel in `reels.json` (wrapper key
    // `ig_reels_media`), and a story in `stories.json` (flat top-level `uri`,
    // no `media` array). The parser must surface all three.

    pub const MP4_A: &[u8] = b"\x00\x00\x00\x18ftypmp42fixture-mp4-alpha-0123456789";
    pub const MP4_B: &[u8] = b"\x00\x00\x00\x18ftypmp42fixture-mp4-bravo-0123456789";
    pub const MP4_C: &[u8] = b"\x00\x00\x00\x18ftypmp42fixture-mp4-charlie-012345";

    pub fn instagram_video_archive() -> FixtureZip {
        // Feed post with a video in media[].
        let posts_json = serde_json::json!([
            {
                "title": "Skate line downtown",
                "creation_timestamp": 1729000000,
                "media": [
                    {"uri": "media/posts/202410/VID_0001.mp4",
                     "creation_timestamp": 1729000000, "title": ""}
                ]
            }
        ])
        .to_string();

        // Reels are wrapper-keyed (ig_reels_media), media[] with a .mp4.
        let reels_json = serde_json::json!({
            "ig_reels_media": [
                {
                    "title": "Sunset timelapse reel",
                    "creation_timestamp": 1730000000,
                    "media": [
                        {"uri": "media/reels/202411/REEL_0001.mp4",
                         "creation_timestamp": 1730000000, "title": ""}
                    ]
                }
            ]
        })
        .to_string();

        // Stories are wrapper-keyed (ig_stories) AND flat (top-level uri).
        let stories_json = serde_json::json!({
            "ig_stories": [
                {"uri": "media/stories/202412/STORY_0001.mp4",
                 "creation_timestamp": 1732000000}
            ]
        })
        .to_string();

        build_zip(
            "instagram_video",
            vec![
                (format!("{}/your_instagram_activity/media/posts_1.json", IG_ROOT),
                 posts_json.into_bytes()),
                (format!("{}/your_instagram_activity/media/reels.json", IG_ROOT),
                 reels_json.into_bytes()),
                (format!("{}/your_instagram_activity/media/stories.json", IG_ROOT),
                 stories_json.into_bytes()),
                (format!("{}/media/posts/202410/VID_0001.mp4", IG_ROOT), MP4_A.to_vec()),
                (format!("{}/media/reels/202411/REEL_0001.mp4", IG_ROOT), MP4_B.to_vec()),
                (format!("{}/media/stories/202412/STORY_0001.mp4", IG_ROOT), MP4_C.to_vec()),
            ],
        )
    }

    // ── Threads fixture ──────────────────────────────────────────────────────
    // Threads JSON export: `threads/threads_and_replies.json` (wrapper key
    // `text_post_app_text_posts`) plus `threads/following.json` /
    // `threads/followers.json`. Entries sit at the ZIP root (no root folder).

    pub const TH_POST_TEXTS: [&str; 4] = [
        "First thread about urban gardening",
        "Hot take: sourdough is not that hard",
        "", // caption-less media post — must still import
        "Thanks for a great year, everyone!",
    ];
    /// Known timestamps for the 4 parsed posts (the reply entry at
    /// 1733000000 is filtered out and must not appear).
    pub const TH_POST_TS: [i64; 4] = [1730000000, 1731000000, 1732000000, 1734000000];
    pub const TH_POSTS_WITH_MEDIA: [bool; 4] = [true, false, true, false];

    pub fn threads_archive() -> FixtureZip {
        let threads_json = serde_json::json!({
            "text_post_app_text_posts": [
                {"media": [{"creation_timestamp": TH_POST_TS[0], "title": TH_POST_TEXTS[0],
                            "uri": "media/posts/202410/THREAD_0001.jpg",
                            "text_app_post": {"is_reply": false}}]},
                {"media": [{"creation_timestamp": TH_POST_TS[1], "title": TH_POST_TEXTS[1],
                            "uri": "",
                            "text_app_post": {"is_reply": false}}]},
                {"media": [{"creation_timestamp": TH_POST_TS[2], "title": TH_POST_TEXTS[2],
                            "uri": "media/posts/202411/THREAD_0002.jpg",
                            "text_app_post": {"is_reply": false}}]},
                // Reply: has caption + media, must be filtered out entirely.
                {"media": [{"creation_timestamp": 1733000000,
                            "title": "Reply that must be skipped",
                            "uri": "media/posts/202411/THREAD_0003.jpg",
                            "text_app_post": {"is_reply": true}}]},
                {"media": [{"creation_timestamp": TH_POST_TS[3], "title": TH_POST_TEXTS[3],
                            "uri": "",
                            "text_app_post": {"is_reply": false}}]}
            ]
        })
        .to_string();

        let following_json = serde_json::json!({
            "text_post_app_text_post_app_following": [
                {"title": "Garden Gal",
                 "string_list_data": [{"value": "garden_gal", "timestamp": 1730000000}]},
                {"title": "Thread Thompson",
                 "string_list_data": [{"value": "thread_thompson", "timestamp": 1730000001}]},
                {"title": "Reply Rex",
                 "string_list_data": [{"value": "reply_rex", "timestamp": 1730000002}]}
            ]
        })
        .to_string();

        let followers_json = serde_json::json!({
            "text_post_app_text_post_app_followers": [
                {"title": "Follower One",
                 "string_list_data": [{"value": "follower_one", "timestamp": 1730000003}]},
                {"title": "Follower Two",
                 "string_list_data": [{"value": "follower_two", "timestamp": 1730000004}]}
            ]
        })
        .to_string();

        build_zip(
            "threads",
            vec![
                ("threads/threads_and_replies.json".to_string(), threads_json.into_bytes()),
                ("media/posts/202410/THREAD_0001.jpg".to_string(), JPEG_A.to_vec()),
                ("media/posts/202411/THREAD_0002.jpg".to_string(), JPEG_B.to_vec()),
                ("media/posts/202411/THREAD_0003.jpg".to_string(), JPEG_A.to_vec()),
                ("threads/following.json".to_string(), following_json.into_bytes()),
                ("threads/followers.json".to_string(), followers_json.into_bytes()),
            ],
        )
    }

    // ── HTML follow-list fixture (spec R3: "the HTML equivalents") ───────────

    pub fn instagram_follow_html_archive() -> FixtureZip {
        let html = r#"<!DOCTYPE html><html><body>
<div class="_a6-g">
  <h2 class="_2pim _a6-h _a6-i">HTML Person One</h2>
  <div class="_a6-p"><a href="https://www.instagram.com/html_person_one">html_person_one</a></div>
</div>
<div class="_a6-g">
  <h2 class="_2pim _a6-h _a6-i">HTML Person Two</h2>
  <div class="_a6-p"><a href="https://www.instagram.com/html_person_two">html_person_two</a></div>
</div>
</body></html>"#;
        build_zip(
            "instagram_follow_html",
            vec![(
                "connections/followers_and_following/following.html".to_string(),
                html.as_bytes().to_vec(),
            )],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::feed::FeedResponse;
    use crate::api::import::tokio_to_std_sender;
    use futures::FutureExt;
    use std::path::Path;

    fn test_sender() -> (crate::api::import::StdSyncSender, tokio::sync::mpsc::UnboundedReceiver<String>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        (tokio_to_std_sender(&tx), rx)
    }

    fn drain_events(rx: &mut tokio::sync::mpsc::UnboundedReceiver<String>) -> Vec<serde_json::Value> {
        let mut events = Vec::new();
        while let Some(msg) = rx.recv().now_or_never().flatten() {
            match serde_json::from_str(&msg) {
                Ok(v) => events.push(v),
                Err(e) => panic!("invalid SSE JSON from parser: {e} ({msg})"),
            }
        }
        events
    }

    /// Remove media files the parser extracted into ./uploads/import/.
    fn cleanup_media_files(posts: &[crate::api::import_facebook::ParsedPost]) {
        for post in posts {
            for url in &post.media_urls {
                if let Some(rel) = url.strip_prefix('/') {
                    let _ = std::fs::remove_file(rel);
                }
            }
        }
    }

    fn assert_media_on_disk(posts: &[crate::api::import_facebook::ParsedPost]) {
        for post in posts {
            for url in &post.media_urls {
                assert!(url.starts_with("/uploads/import/"), "unexpected media url {url}");
                let rel = url.strip_prefix('/').unwrap();
                assert!(Path::new(rel).exists(), "extracted media file missing: {rel}");
            }
        }
    }

    // ─── R1 + R2: Instagram fixture parsing ─────────────────────────────────

    #[test]
    fn instagram_fixture_posts_media_and_timestamps() {
        let fixture = fixtures::instagram_archive();
        let (tx, mut rx) = test_sender();
        let (posts, errors) = parse_instagram_archive_sync(fixture.path().to_str().unwrap(), &tx);

        assert!(errors.is_empty(), "parser errors: {errors:?}");
        assert_eq!(posts.len(), 4, "fixture has 4 posts, got {}", posts.len());

        for (i, post) in posts.iter().enumerate() {
            assert_eq!(post.content_text, fixtures::IG_POST_TEXTS[i], "post {i} text");
            assert_eq!(post.original_ts.timestamp(), fixtures::IG_POST_TS[i], "post {i} ts");
            let want_media = if fixtures::IG_POSTS_WITH_MEDIA[i] { 1 } else { 0 };
            assert_eq!(post.media_urls.len(), want_media, "post {i} media count");
            assert!(post.action_text.as_deref() == Some("posted"));
        }

        // Timestamp range over the whole import.
        let timestamps: Vec<i64> = posts.iter().map(|p| p.original_ts.timestamp()).collect();
        assert_eq!(timestamps.iter().copied().min().unwrap(), fixtures::IG_POST_TS[0]);
        assert_eq!(timestamps.iter().copied().max().unwrap(), *fixtures::IG_POST_TS.last().unwrap());

        // R2: 2 media total, extracted to disk.
        let total_media: usize = posts.iter().map(|p| p.media_urls.len()).sum();
        assert_eq!(total_media, 2, "fixture has 2 images");
        assert_media_on_disk(&posts);

        // Streaming events: 4 posts + 2 media reported.
        let events = drain_events(&mut rx);
        assert_eq!(events.iter().filter(|e| e["type"] == "post").count(), 4, "post events: {events:?}");
        assert_eq!(events.iter().filter(|e| e["type"] == "media").count(), 2, "media events: {events:?}");

        cleanup_media_files(&posts);
    }

    // ─── R4: Instagram videos (feed post, reel, story) must import ──────────

    #[test]
    fn instagram_videos_from_all_sources() {
        let fixture = fixtures::instagram_video_archive();
        let (tx, _rx) = test_sender();
        let (posts, errors) = parse_instagram_archive_sync(fixture.path().to_str().unwrap(), &tx);

        assert!(errors.is_empty(), "parser errors: {errors:?}");
        // 1 feed post + 1 reel + 1 story = 3 entries.
        assert_eq!(posts.len(), 3, "expected 3 video entries, got {}", posts.len());

        // Every entry carries its .mp4 as extracted media.
        for (i, post) in posts.iter().enumerate() {
            assert_eq!(post.media_urls.len(), 1, "entry {i} should have 1 video, got {:?}", post.media_urls);
            let url = &post.media_urls[0];
            assert!(url.ends_with(".mp4"), "entry {i} media should be an .mp4, got {url}");
        }

        // The story (flat top-level uri, no media[] array) must be present —
        // this was the hardest gap. Its text is empty (stories carry no title).
        let story = posts.iter().find(|p| p.content_text.is_empty())
            .expect("story entry (no title) should be present");
        assert_eq!(story.media_urls.len(), 1);

        // Titles present for the feed post and reel.
        let texts: Vec<&str> = posts.iter().map(|p| p.content_text.as_str()).collect();
        assert!(texts.contains(&"Skate line downtown"), "feed post text missing: {texts:?}");
        assert!(texts.contains(&"Sunset timelapse reel"), "reel text missing: {texts:?}");

        assert_media_on_disk(&posts);
        cleanup_media_files(&posts);
    }

    // ─── Threads fixture parsing ────────────────────────────────────────────

    #[test]
    fn threads_fixture_posts_media_and_timestamps() {
        let fixture = fixtures::threads_archive();
        let (tx, mut rx) = test_sender();
        let (posts, errors) = parse_threads_archive_sync(fixture.path().to_str().unwrap(), &tx);

        assert!(errors.is_empty(), "parser errors: {errors:?}");
        // 5 JSON entries, 1 is a reply → 4 parsed (incl. one media-only post).
        assert_eq!(posts.len(), 4, "reply filtered, got {} posts", posts.len());

        for (i, post) in posts.iter().enumerate() {
            assert_eq!(post.content_text, fixtures::TH_POST_TEXTS[i], "post {i} text");
            assert_eq!(post.original_ts.timestamp(), fixtures::TH_POST_TS[i], "post {i} ts");
            let want_media = if fixtures::TH_POSTS_WITH_MEDIA[i] { 1 } else { 0 };
            assert_eq!(post.media_urls.len(), want_media, "post {i} media count");
        }

        let timestamps: Vec<i64> = posts.iter().map(|p| p.original_ts.timestamp()).collect();
        assert_eq!(timestamps.iter().copied().min().unwrap(), fixtures::TH_POST_TS[0]);
        assert_eq!(timestamps.iter().copied().max().unwrap(), *fixtures::TH_POST_TS.last().unwrap());

        let total_media: usize = posts.iter().map(|p| p.media_urls.len()).sum();
        assert_eq!(total_media, 2, "reply's image must not be extracted");
        assert!(posts.iter().all(|p| p.media_urls.iter().all(|u| !u.contains("THREAD_0003"))),
            "reply media leaked into results");
        assert_media_on_disk(&posts);

        let events = drain_events(&mut rx);
        assert_eq!(events.iter().filter(|e| e["type"] == "post").count(), 4, "post events: {events:?}");
        assert_eq!(events.iter().filter(|e| e["type"] == "media").count(), 2, "media events: {events:?}");

        cleanup_media_files(&posts);
    }

    // ─── Caption-less media posts import; empty entries skip ────────────────

    #[test]
    fn media_only_posts_are_imported_and_empty_entries_skipped() {
        let posts_json = serde_json::json!([
            {"creation_timestamp": 1736000000,
             "media": [{"uri": "media/posts/202501/ONLY.jpg",
                        "creation_timestamp": 1736000000, "title": ""}]},
            {"creation_timestamp": 1736000001}
        ])
        .to_string();
        let fixture = fixtures::build_zip(
            "media_only",
            vec![
                ("your_instagram_activity/media/posts_1.json".to_string(), posts_json.into_bytes()),
                ("media/posts/202501/ONLY.jpg".to_string(), fixtures::JPEG_A.to_vec()),
            ],
        );

        let (tx, _rx) = test_sender();
        let (posts, errors) = parse_instagram_archive_sync(fixture.path().to_str().unwrap(), &tx);

        assert!(errors.is_empty(), "parser errors: {errors:?}");
        assert_eq!(posts.len(), 1, "media-only post imports, textless/medialess entry skips");
        assert_eq!(posts[0].content_text, "");
        assert_eq!(posts[0].original_ts.timestamp(), 1736000000);
        assert_eq!(posts[0].media_urls.len(), 1);
        assert_media_on_disk(&posts);
        cleanup_media_files(&posts);
    }

    // ─── R4 + R5: end-to-end import → import_sessions → profile wall ───────

    #[tokio::test]
    async fn instagram_import_fills_session_and_renders_on_profile_wall() {
        // DB integration test — skips without a database, matching the
        // existing stories/consumer integration tests.
        if std::env::var("DATABASE_URL").is_err() {
            let _ = dotenvy::dotenv();
            let _ = dotenvy::from_filename("bytescape-server/.env");
        }
        let database_url = match std::env::var("DATABASE_URL") {
            Ok(u) => u,
            Err(_) => {
                eprintln!("DATABASE_URL unset — skipping wall rendering test");
                return;
            }
        };
        let config = match crate::config::AppConfig::from_env() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("config incomplete ({e}) — skipping wall rendering test");
                return;
            }
        };

        let pool = sqlx::PgPool::connect(&database_url).await.expect("db connect");

        let unique = Uuid::new_v4().simple().to_string();
        let username = format!("igwall_{}", &unique[..12]);
        let user = db::users::UserRow::create_active(
            &pool, &username, &format!("{username}@test.local"), "x", "x",
        )
        .await
        .expect("create test user");

        let state = AppState {
            config: std::sync::Arc::new(config),
            db: pool.clone(),
            og_cache: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
        };

        let fixture = fixtures::instagram_archive();
        let (imported, session_id, errors) =
            import_instagram_archive_from_path(&state, user.id, fixture.path().to_str().unwrap())
                .await
                .expect("import should succeed");
        assert!(errors.is_empty(), "import errors: {errors:?}");
        assert_eq!(imported, 4, "all fixture posts imported");

        // R4: import_sessions records accurate counts.
        let session = db::imports::ImportRow::find_by_id(&pool, session_id.parse().unwrap())
            .await
            .expect("session query")
            .expect("session row exists");
        assert_eq!(session.status, "completed");
        assert_eq!(session.total_posts, 4);
        assert_eq!(session.imported_posts, 4);
        assert_eq!(session.total_media, 2);
        assert_eq!(session.imported_media, 2);

        // R5: the profile wall endpoint serves the imported posts.
        let app = axum::Router::new()
            .merge(crate::api::profile::routes())
            .with_state(state.clone());
        let request = axum::http::Request::builder()
            .uri(format!("/api/profile/{username}/feed"))
            .body(axum::body::Body::empty())
            .unwrap();
        let response = tower::ServiceExt::oneshot(app, request).await.expect("feed request");
        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let feed: FeedResponse = serde_json::from_slice(&body).expect("feed JSON");
        assert_eq!(feed.items.len(), 4, "wall shows all imported posts");

        // Sorted newest-first; created_at renders the original post time for
        // imported posts; media URLs point at the extracted files.
        for (position, item) in feed.items.iter().enumerate() {
            let fixture_idx = 3 - position;
            assert_eq!(item.source, "instagram");
            assert_eq!(item.content, fixtures::IG_POST_TEXTS[fixture_idx], "item {position} text");
            // Imported archive posts must show their origin, not read as native
            // Bytescape posts ("@user posted").
            assert_eq!(
                item.action_text.as_deref(),
                Some("via Instagram archive"),
                "item {position} should carry an archive-origin action_text"
            );

            let expected_ts = chrono::DateTime::<chrono::Utc>::from_timestamp(
                fixtures::IG_POST_TS[fixture_idx], 0,
            )
            .unwrap();
            assert_eq!(item.created_at, expected_ts.to_rfc3339(), "item {position} timestamp");

            let want_media = if fixtures::IG_POSTS_WITH_MEDIA[fixture_idx] { 1 } else { 0 };
            assert_eq!(item.media.len(), want_media, "item {position} media count");
            for media in &item.media {
                assert!(media.url.starts_with("/uploads/import/"), "media url {}", media.url);
                let rel = media.url.strip_prefix('/').unwrap();
                assert!(Path::new(rel).exists(), "media file missing on disk: {rel}");
                assert_eq!(media.mime_type.as_deref(), Some("image/jpeg"));
            }
        }

        // Cleanup: media files, then rows (posts → media → session → user).
        for item in &feed.items {
            for media in &item.media {
                if let Some(rel) = media.url.strip_prefix('/') {
                    let _ = std::fs::remove_file(rel);
                }
            }
        }
        let _ = sqlx::query("DELETE FROM posts WHERE user_id = $1")
            .bind(user.id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM media WHERE user_id = $1")
            .bind(user.id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM import_sessions WHERE user_id = $1")
            .bind(user.id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id).execute(&pool).await;
    }
}
