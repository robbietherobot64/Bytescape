//! Profile Layout Commands
//!
//! Save and load the user's custom GrapesJS profile layout from redb.

use tauri::State;
use serde::{Deserialize, Serialize};

use crate::AppState;

/// The stored profile layout data — HTML, CSS, and optional GrapesJS JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileLayout {
    pub html: String,
    pub css: String,
    /// GrapesJS components JSON for restoring editor state
    pub components_json: Option<String>,
    /// GrapesJS styles JSON for restoring editor state
    pub styles_json: Option<String>,
    /// Unix timestamp when the layout was last saved
    pub updated_at: u64,
}

/// Save the user's custom profile layout (HTML + CSS + optional GrapesJS state).
#[tauri::command]
pub async fn save_profile_layout(
    html: String,
    css: String,
    components_json: Option<String>,
    styles_json: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    let layout = ProfileLayout {
        html,
        css,
        components_json,
        styles_json,
        updated_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    db.save_profile_layout(&layout).await
}

/// Load the user's custom profile layout, if one exists.
#[tauri::command]
pub async fn get_profile_layout(
    state: State<'_, AppState>,
) -> Result<Option<ProfileLayout>, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    db.get_profile_layout().await
}

/// Delete the user's custom profile layout, reverting to the default wall.
#[tauri::command]
pub async fn delete_profile_layout(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    db.delete_profile_layout().await
}
