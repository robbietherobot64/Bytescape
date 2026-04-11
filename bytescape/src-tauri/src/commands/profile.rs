//! Profile Layout, Widget, Section & Guestbook Commands
//!
//! MySpace-style customizable profile system. Users can design their own profile
//! page using the GrapesJS visual editor (or raw HTML/CSS), configure interactive
//! widgets (away message, music playlist, etc.), manage editable content sections,
//! and maintain a guestbook for visitor messages.
//!
//! ## How It Works
//!
//! 1. **Layout persistence** — The GrapesJS editor serializes the profile into
//!    HTML, CSS, and components/styles JSON. `save_profile_layout` stores all of
//!    these atomically in redb so the editor can restore state on next open.
//! 2. **Widget data** — Widget configuration (JSON blob) is stored as a field on
//!    the `ProfileLayout`. `save_widget_data` merges into the existing layout,
//!    creating a minimal skeleton layout if none exists yet.
//! 3. **Profile sections** — Structured content blocks (About Me, Photos, Links)
//!    are saved as an ordered `Vec<ProfileSection>`. The frontend renders these
//!    in display order, filtered by `visible` flag.
//! 4. **Guestbook** — Visitors can leave signed messages. Entries are stored
//!    individually in redb with UUIDs, listed newest-first, and deletable by
//!    the profile owner.
//! 5. **Render modes** — Layouts can render via GrapesJS widget hydration
//!    (default) or as raw HTML in a sandboxed iframe (`render_mode: "raw"`).
//!
//! ## Related Files
//!
//! - `db/vault.rs` — redb persistence for layouts, sections, guestbook entries
//! - `sync.rs` — P2P profile sync via Iroh Docs (future)
//! - Frontend: `src/screens/ProfileScreen.tsx` — profile display
//! - Frontend: `src/screens/ProfileEditorScreen.tsx` — GrapesJS editor
//! - Frontend: `src/components/Guestbook.tsx` — guestbook UI

use tauri::{State, Manager};
use serde::{Deserialize, Serialize};

use crate::AppState;

// ═══════════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════════

/// The stored profile layout data — HTML, CSS, and optional GrapesJS JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileLayout {
    pub html: String,
    pub css: String,
    /// GrapesJS components JSON for restoring editor state
    pub components_json: Option<String>,
    /// GrapesJS styles JSON for restoring editor state
    pub styles_json: Option<String>,
    /// Widget configuration data (away message, music playlist, etc.) as JSON
    pub widget_data: Option<String>,
    /// Render mode: None or "grapesjs" = widget hydration, "raw" = sandboxed iframe
    #[serde(default)]
    pub render_mode: Option<String>,
    /// Optional custom JavaScript for raw HTML mode (sandboxed only)
    #[serde(default)]
    pub js: Option<String>,
    /// Unix timestamp when the layout was last saved
    pub updated_at: u64,
}

/// A single guestbook entry left by a visitor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestbookEntry {
    pub id: String,
    pub author: String,
    pub message: String,
    pub timestamp: u64,
    /// Optional P2P node ID of the author
    pub node_id: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Layout Commands
// ═══════════════════════════════════════════════════════════════════════════════

/// Save the user's custom profile layout (HTML + CSS + optional GrapesJS state).
///
/// Persists the full GrapesJS editor state — HTML, CSS, components JSON, styles
/// JSON, widget data, render mode, and optional custom JS — to redb. Timestamps
/// the save with the current Unix epoch.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('save_profile_layout', { html, css, componentsJson?, stylesJson?, widgetData?, renderMode?, js? })`
#[tauri::command]
pub async fn save_profile_layout(
    html: String,
    css: String,
    components_json: Option<String>,
    styles_json: Option<String>,
    widget_data: Option<String>,
    render_mode: Option<String>,
    js: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    let layout = ProfileLayout {
        html,
        css,
        components_json,
        styles_json,
        widget_data,
        render_mode,
        js,
        updated_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    db.save_profile_layout(&layout).await
}

/// Load the user's custom profile layout, if one exists.
///
/// Returns `None` if no layout has been saved yet, signaling the frontend
/// to render the default profile wall.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('get_profile_layout')`
#[tauri::command]
pub async fn get_profile_layout(
    state: State<'_, AppState>,
) -> Result<Option<ProfileLayout>, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    db.get_profile_layout().await
}

/// Delete the user's custom profile layout, reverting to the default wall.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('delete_profile_layout')`
#[tauri::command]
pub async fn delete_profile_layout(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    db.delete_profile_layout().await
}

// ═══════════════════════════════════════════════════════════════════════════════
// Widget Commands
// ═══════════════════════════════════════════════════════════════════════════════

/// Save widget configuration data (away message, music playlist, etc.).
///
/// Merges into the existing profile layout, creating a minimal skeleton layout
/// if none exists yet. Only the `widget_data` and `updated_at` fields are modified.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('save_widget_data', { widgetData })`
#[tauri::command]
pub async fn save_widget_data(
    widget_data: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    let mut layout = db.get_profile_layout().await?.unwrap_or(ProfileLayout {
        html: String::new(),
        css: String::new(),
        components_json: None,
        styles_json: None,
        widget_data: None,
        render_mode: None,
        js: None,
        updated_at: 0,
    });

    layout.widget_data = Some(widget_data);
    layout.updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    db.save_profile_layout(&layout).await
}

/// Get widget configuration data (away message, music playlist, etc.).
///
/// Returns `None` if no widget data has been saved or no layout exists.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('get_widget_data')`
#[tauri::command]
pub async fn get_widget_data(
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    let layout = db.get_profile_layout().await?;
    Ok(layout.and_then(|l| l.widget_data))
}

// ═══════════════════════════════════════════════════════════════════════════════
// Guestbook Commands
// ═══════════════════════════════════════════════════════════════════════════════

/// Add a new guestbook entry.
///
/// Creates a UUID-identified entry with the given author name, message text,
/// and optional P2P node ID. Validates input length (author ≤ 200 chars,
/// message ≤ 10,000 chars).
///
/// # Tauri IPC
/// Frontend invokes: `invoke('add_guestbook_entry', { author, message, nodeId? })`
#[tauri::command]
pub async fn add_guestbook_entry(
    author: String,
    message: String,
    node_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<GuestbookEntry, String> {
    if author.len() > 200 {
        return Err("Author name too long (max 200 chars)".into());
    }
    if message.len() > 10_000 {
        return Err("Message too long (max 10,000 chars)".into());
    }

    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    let entry = GuestbookEntry {
        id: uuid::Uuid::new_v4().to_string(),
        author,
        message,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        node_id,
    };

    db.add_guestbook_entry(&entry).await?;
    Ok(entry)
}

/// List all guestbook entries, newest first.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('list_guestbook_entries')`
#[tauri::command]
pub async fn list_guestbook_entries(
    state: State<'_, AppState>,
) -> Result<Vec<GuestbookEntry>, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    db.list_guestbook_entries().await
}

/// Delete a guestbook entry by ID.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('delete_guestbook_entry', { entryId })`
#[tauri::command]
pub async fn delete_guestbook_entry(
    entry_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    db.delete_guestbook_entry(&entry_id).await
}

// ═══════════════════════════════════════════════════════════════════════════════
// Profile Sections (MySpace-style editable blocks)
// ═══════════════════════════════════════════════════════════════════════════════

/// A single editable profile section (About Me, Photos, Links, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSection {
    /// Unique section identifier (e.g. "about-me", "photos", "links")
    pub id: String,
    /// Section type hint for the renderer (e.g. "text", "gallery", "links", "connections")
    #[serde(rename = "type")]
    pub section_type: String,
    /// User-visible section title
    pub title: String,
    /// Section body content (HTML, Markdown, or JSON depending on type)
    pub content: String,
    /// Display order (lower = higher on page)
    pub order: u32,
    /// Whether the section is visible on the public profile
    pub visible: bool,
}

/// Save the user's profile sections (replaces all sections atomically).
///
/// Frontend usage:
/// ```typescript
/// await invoke('save_profile_sections', { sections: [...] });
/// ```
#[tauri::command]
pub async fn save_profile_sections(
    sections: Vec<ProfileSection>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    db.save_profile_sections(&sections).await
}

/// Load the user's profile sections, ordered by `order` field.
///
/// Returns an empty array if no sections have been saved yet.
///
/// Frontend usage:
/// ```typescript
/// const sections = await invoke<ProfileSection[]>('get_profile_sections');
/// ```
#[tauri::command]
pub async fn get_profile_sections(
    state: State<'_, AppState>,
) -> Result<Vec<ProfileSection>, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    db.get_profile_sections().await
}

// ═══════════════════════════════════════════════════════════════════════════════
// Profile Settings (theme, background, customization preferences)
// ═══════════════════════════════════════════════════════════════════════════════

/// Save profile settings (theme, background, customization preferences).
///
/// Stores an opaque JSON blob — the frontend defines the schema.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('save_profile_settings', { settingsJson })`
#[tauri::command]
pub async fn save_profile_settings(
    settings_json: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    db.save_profile_settings(&settings_json).await
}

/// Load profile settings.
///
/// Returns `None` if no settings have been saved yet.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('get_profile_settings')`
#[tauri::command]
pub async fn get_profile_settings(
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    db.get_profile_settings().await
}

// ═══════════════════════════════════════════════════════════════════════════════
// Profile Pages (multi-page personal website)
// ═══════════════════════════════════════════════════════════════════════════════

/// Save a profile page (creates or updates).
///
/// # Tauri IPC
/// Frontend invokes: `invoke('save_profile_page', { pageId, pageJson })`
#[tauri::command]
pub async fn save_profile_page(
    page_id: String,
    page_json: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    db.save_profile_page(&page_id, &page_json).await
}

/// List all profile pages.
///
/// Returns a vector of JSON strings, each representing a page.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('get_profile_pages')`
#[tauri::command]
pub async fn get_profile_pages(
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    db.get_profile_pages().await
}

/// Delete a profile page by ID.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('delete_profile_page', { pageId })`
#[tauri::command]
pub async fn delete_profile_page(
    page_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    db.delete_profile_page(&page_id).await
}

// ═══════════════════════════════════════════════════════════════════════════════
// Avatar Image Storage
// ═══════════════════════════════════════════════════════════════════════════════

const MAX_AVATAR_SIZE: usize = 5 * 1024 * 1024; // 5 MB
const ALLOWED_AVATAR_MIMES: &[&str] = &["image/png", "image/jpeg", "image/gif", "image/webp"];

/// Save an avatar image to local storage.
///
/// Validates MIME type (png/jpeg/gif/webp only) and file size (< 5 MB),
/// writes the image to `app_data_dir/avatars/{hash}.{ext}`, and returns
/// a `file://` URL the frontend can use to display the image.
///
/// # Parameters
/// - `image_data` — Raw image bytes (base64-decoded by Tauri's serde bridge)
/// - `mime_type` — MIME type string (e.g. "image/png")
///
/// # Tauri IPC
/// Frontend invokes: `invoke('save_avatar_image', { imageData: [...], mimeType: 'image/png' })`
#[tauri::command]
pub async fn save_avatar_image(
    image_data: Vec<u8>,
    mime_type: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Validate MIME type
    let mime_lower = mime_type.to_lowercase();
    if !ALLOWED_AVATAR_MIMES.contains(&mime_lower.as_str()) {
        return Err(format!(
            "Unsupported image type '{}'. Allowed: png, jpeg, gif, webp",
            mime_type
        ));
    }

    // Validate size
    if image_data.len() > MAX_AVATAR_SIZE {
        return Err(format!(
            "Avatar image too large ({:.1} MB). Maximum is 5 MB.",
            image_data.len() as f64 / (1024.0 * 1024.0)
        ));
    }

    if image_data.is_empty() {
        return Err("Avatar image data is empty".to_string());
    }

    // Determine file extension from MIME
    let ext = match mime_lower.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => return Err("Unsupported image type".to_string()),
    };

    // Hash the image data for a content-addressed filename
    let hash = blake3::hash(&image_data);
    let filename = format!("{}.{}", hash.to_hex(), ext);

    // Write to app_data_dir/avatars/
    let app_data_dir = state.app_handle.path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;

    let avatars_dir = app_data_dir.join("avatars");
    std::fs::create_dir_all(&avatars_dir)
        .map_err(|e| format!("Failed to create avatars directory: {}", e))?;

    let file_path = avatars_dir.join(&filename);
    std::fs::write(&file_path, &image_data)
        .map_err(|e| format!("Failed to write avatar file: {}", e))?;

    // Return a file:// URL the frontend can display
    let url = tauri::Url::from_file_path(&file_path)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| format!("file://{}", file_path.display()));

    Ok(url)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Profile Data Export (composite bundle for hosted profile publishing)
// ═══════════════════════════════════════════════════════════════════════════════

/// Composite profile data bundle for export/publishing.
///
/// Contains everything needed to reconstruct a user's profile on a hosted domain:
/// settings, pages, sections, and avatar. The v20 publish flow will serialize
/// this and push it to the user's `did:web` domain.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileExport {
    /// Profile settings JSON (theme, background, widget layout, etc.)
    pub settings: Option<serde_json::Value>,
    /// Profile pages as JSON values
    pub pages: Vec<serde_json::Value>,
    /// Profile sections (about, links, connections, etc.)
    pub sections: Vec<ProfileSection>,
    /// Avatar URL — either an external URL or base64 data URL for local images
    pub avatar: Option<String>,
    /// Profile layout (HTML, CSS, GrapesJS editor state, widget data)
    pub layout: Option<ProfileLayout>,
    /// Guestbook entries
    pub guestbook: Vec<GuestbookEntry>,
    /// ISO 8601 timestamp of when the export was generated
    pub exported_at: String,
}

/// Export the complete profile as a single composite data structure.
///
/// Bundles profile settings, pages, sections, and avatar into one JSON-serializable
/// struct. If the avatar is stored locally (file:// path in settings), the image
/// bytes are inlined as a base64 data URL for portability.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('export_profile_data')`
#[tauri::command]
pub async fn export_profile_data(
    state: State<'_, AppState>,
) -> Result<ProfileExport, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    // 1. Load profile settings
    let settings_json = db.get_profile_settings().await?;
    let settings: Option<serde_json::Value> = settings_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());

    // 2. Load profile pages
    let page_strings = db.get_profile_pages().await?;
    let pages: Vec<serde_json::Value> = page_strings
        .iter()
        .filter_map(|s| serde_json::from_str(s).ok())
        .collect();

    // 3. Load profile sections
    let sections = db.get_profile_sections().await?;

    // 4. Resolve avatar — inline local files as base64 data URLs
    let avatar = resolve_avatar_for_export(&settings).await;

    // 5. Load profile layout (HTML/CSS/GrapesJS state)
    let layout = db.get_profile_layout().await.ok().flatten();

    // 6. Load guestbook entries
    let guestbook = db.list_guestbook_entries().await.unwrap_or_default();

    // 7. Bundle
    let exported_at = chrono::Utc::now().to_rfc3339();

    Ok(ProfileExport {
        settings,
        pages,
        sections,
        avatar,
        layout,
        guestbook,
        exported_at,
    })
}

/// Resolve avatar URL for export. If the avatar is a local file, read it and
/// encode as a base64 data URL so the export is self-contained.
async fn resolve_avatar_for_export(settings: &Option<serde_json::Value>) -> Option<String> {
    let settings = settings.as_ref()?;
    let avatar_url = settings.get("avatar_url")?.as_str()?;

    if avatar_url.is_empty() {
        return None;
    }

    // External URLs pass through as-is
    if avatar_url.starts_with("http://") || avatar_url.starts_with("https://") {
        return Some(avatar_url.to_string());
    }

    // Local file:// URL — try to read and inline as data URL
    let file_path = if avatar_url.starts_with("file://") {
        avatar_url.strip_prefix("file://").unwrap_or(avatar_url)
    } else {
        avatar_url
    };

    let path = std::path::Path::new(file_path);
    if !path.exists() {
        return Some(avatar_url.to_string());
    }

    let bytes = std::fs::read(path).ok()?;
    let mime = match path.extension().and_then(|e| e.to_str()) {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "image/png",
    };

    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(format!("data:{};base64,{}", mime, b64))
}
