use tauri::State;
use crate::AppState;
use crate::models::{BytescapeType, classify_item};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ReclassifyResult {
    pub total_scanned: usize,
    pub total_changed: usize,
    pub status_count: usize,
    pub gallery_count: usize,
    pub article_count: usize,
    pub link_count: usize,
    pub video_count: usize,
}

#[tauri::command]
pub async fn reclassify_vault_items(
    state: State<'_, AppState>,
) -> Result<ReclassifyResult, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    // Get owner DID
    let owner_did = match state.keychain.load_session() {
        Ok(Some((did, _))) => did,
        _ => return Err("No active session".to_string()),
    };

    // Load all items
    let (items, _total) = db.list_my_items_paged(&owner_did, None, 100_000, 0).await?;

    let total_scanned = items.len();
    let mut total_changed = 0usize;
    let mut status_count = 0usize;
    let mut gallery_count = 0usize;
    let mut article_count = 0usize;
    let mut link_count = 0usize;
    let mut video_count = 0usize;

    for mut item in items {
        let new_type = classify_item(&item);
        match new_type {
            BytescapeType::Status => status_count += 1,
            BytescapeType::Gallery => gallery_count += 1,
            BytescapeType::Article => article_count += 1,
            BytescapeType::Link => link_count += 1,
            BytescapeType::Video => video_count += 1,
        }

        if item.item_type != new_type {
            item.item_type = new_type;
            // Re-compute willow path with new type
            item.path = crate::commands::import::willow_path_for_item(
                &item.source,
                new_type,
                item.original_timestamp,
                &item.id,
            );
            db.save_item(&item).await?;
            total_changed += 1;
        }
    }

    Ok(ReclassifyResult {
        total_scanned,
        total_changed,
        status_count,
        gallery_count,
        article_count,
        link_count,
        video_count,
    })
}
