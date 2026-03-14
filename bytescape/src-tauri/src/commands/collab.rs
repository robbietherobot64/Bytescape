//! Tauri Commands for Collaborative Editing (C6.1)
//!
//! Exposes the CRDT storage layer to the frontend via Tauri invoke commands.

use tauri::State;
use serde::{Deserialize, Serialize};
use crate::AppState;
use crate::collab::CollabDocMeta;

/// Response for get_collab_state command
#[derive(Debug, Serialize, Deserialize)]
pub struct CollabStateResponse {
    pub state_b64: String,
    pub state_vector_b64: String,
    pub meta: CollabDocMeta,
}

/// Create a new collaborative document.
/// Optionally link it to an existing BytescapeItem via item_id.
#[tauri::command]
pub async fn create_collab_doc(
    item_id: Option<String>,
    title: Option<String>,
    state: State<'_, AppState>,
) -> Result<CollabDocMeta, String> {
    let db_lock = state.db_manager.read().await;
    let db = db_lock.as_ref().ok_or("Database not initialized")?;
    db.create_collab_doc(item_id, title)
}

/// Apply a Yjs update to an existing collab document.
/// The update should be base64-encoded binary Yjs update data.
/// Returns the new state vector (base64-encoded).
#[tauri::command]
pub async fn apply_collab_update(
    doc_id: String,
    update_b64: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let db_lock = state.db_manager.read().await;
    let db = db_lock.as_ref().ok_or("Database not initialized")?;
    db.apply_collab_update(&doc_id, &update_b64)
}

/// Get the full state of a collab document.
/// Returns the Yjs state (base64), state vector (base64), and metadata.
#[tauri::command]
pub async fn get_collab_state(
    doc_id: String,
    state: State<'_, AppState>,
) -> Result<CollabStateResponse, String> {
    let db_lock = state.db_manager.read().await;
    let db = db_lock.as_ref().ok_or("Database not initialized")?;
    let (state_b64, state_vector_b64, meta) = db.get_collab_state(&doc_id)?;
    Ok(CollabStateResponse {
        state_b64,
        state_vector_b64,
        meta,
    })
}

/// Snapshot the current content into the linked BytescapeItem.
/// The frontend provides the rendered content (Markdown or HTML) since Tiptap
/// uses Y.XmlFragment which is not trivially readable in Rust.
/// Returns the item_id that was updated.
#[tauri::command]
pub async fn snapshot_collab_to_item(
    doc_id: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let db_lock = state.db_manager.read().await;
    let db = db_lock.as_ref().ok_or("Database not initialized")?;
    db.snapshot_collab_to_item(&doc_id, &content)
}

/// List all collab documents (metadata only, sorted by updated_at desc).
#[tauri::command]
pub async fn list_collab_docs(
    state: State<'_, AppState>,
) -> Result<Vec<CollabDocMeta>, String> {
    let db_lock = state.db_manager.read().await;
    let db = db_lock.as_ref().ok_or("Database not initialized")?;
    db.list_collab_docs()
}

/// Delete a collab document.
#[tauri::command]
pub async fn delete_collab_doc(
    doc_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db_lock = state.db_manager.read().await;
    let db = db_lock.as_ref().ok_or("Database not initialized")?;
    db.delete_collab_doc(&doc_id)
}
