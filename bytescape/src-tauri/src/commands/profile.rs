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

use tauri::State;
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
