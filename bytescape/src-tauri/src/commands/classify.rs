//! Vault Item Re-classification Command
//!
//! Provides a Tauri IPC command that scans every item in the vault and
//! re-applies the heuristic type classifier (`models::classify_item`).
//! This is useful after the classification rules change — the user can
//! trigger a bulk pass to update stale `BytescapeType` assignments.
//!
//! ## HOW IT WORKS
//!
//! 1. Loads the active session's DID from the keychain.
//! 2. Fetches **all** vault items via `list_my_items_paged` (limit 100 000).
//! 3. Runs `classify_item()` on each, comparing the new type to the stored one.
//! 4. For changed items, recomputes the Willow path and persists the update.
//! 5. Returns aggregate counts so the frontend can display a summary.
//!
//! ## TECH DEBT
//!
//! - The full-scan approach is O(n) with a hard 100 000 row cap.
//!   A cursor-based or incremental strategy should replace this before
//!   vaults routinely exceed ~50 K items.
//!
//! ## RELATED FILES
//!
//! - `models/mod.rs`        — `classify_item()` heuristic and `BytescapeType` enum
//! - `commands/import.rs`   — `willow_path_for_item()` used to recompute paths
//! - `db/vault.rs`          — `list_my_items_paged`, `save_item` persistence

use tauri::State;
use crate::AppState;
use crate::models::{BytescapeType, classify_item};
use serde::Serialize;

/// Aggregate result of a bulk re-classification pass.
///
/// Returned to the frontend so it can display how many items were scanned,
/// how many changed type, and the per-type distribution.
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

/// Re-classify every item in the current user's vault.
///
/// Loads all items (up to 100 000), applies `classify_item()` to each,
/// and persists any whose `BytescapeType` changed.  The Willow storage
/// path is also recomputed for changed items so the content-addressed
/// tree stays consistent.
///
/// # Returns
/// `ReclassifyResult` with scan/change counts and per-type tallies.
///
/// # Errors
/// - `"No active session"` — keychain has no stored DID/session.
/// - `"Database not initialized"` — `DbManager` is `None`.
/// - Propagates any DB read/write errors as `String`.
///
/// # Side Effects
/// - Writes updated items back to redb (mutates `item_type` and `path`).
///
/// # IPC
/// Exposed as Tauri command `reclassify_vault_items`.
/// Frontend invokes: `invoke('reclassify_vault_items')`
#[tauri::command]
pub async fn reclassify_vault_items(
    state: State<'_, AppState>,
) -> Result<ReclassifyResult, String> {
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    // Get all owner identifiers for multi-identity matching
    let owner_dids = state.keychain.all_owner_dids()
        .map_err(|e| e.to_string())?;

    // Load all items
    let (items, _total) = db.list_my_items_paged(&owner_dids, None, 100_000, 0).await?;

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
