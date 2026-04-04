//! Syndication Engine — Cross-Post Vault Items to External Platforms
//!
//! Publishes local vault items to connected social platforms and verifies
//! that syndicated posts still exist upstream. Currently **Bluesky-only**
//! despite the generic naming — future connectors (Mastodon, Nostr) will
//! plug in here.
//!
//! ## How It Works
//!
//! 1. **Load & validate** — Fetches the `BytescapeItem` from redb by ID,
//!    checks it hasn't already been syndicated to the target platform.
//! 2. **Upload media** — Iterates `media_blobs`, fetches each from Iroh via
//!    `SyncManager::get_media`, uploads to the Bluesky PDS as blobs.
//!    Silently caps at **4 images** (Bluesky embed limit).
//! 3. **Compose record** — Builds an `app.bsky.feed.post` JSON record with
//!    rich-text facets (mentions, hashtags, links) parsed by `richtext.rs`.
//! 4. **Publish** — Calls `com.atproto.repo.createRecord` via authenticated
//!    XRPC to the user's PDS.
//! 5. **Persist** — Updates the item's `syndication_status`, `syndicated_uri`,
//!    and `share_history` in redb so the UI reflects the syndication.
//!
//! ## Related Files
//!
//! - `connectors/bluesky.rs` — `BlueskyClient`, PDS resolution, blob upload
//! - `richtext.rs` — Facet parsing (mentions → DID, hashtags, URLs)
//! - `models.rs` — `SyndicationStatus`, `SyndicationEvent`, `Audience`
//! - `sync.rs` — `SyncManager::get_media` for Iroh blob retrieval
//! - `db.rs` — `upsert_item` persists syndication metadata
//! - Frontend: `src/screens/ComposeBox.tsx` invokes `syndicate_vault_item`

use tauri::{State, Manager};

use serde_json::json;
use crate::AppState;
use crate::models::{SyndicationStatus, Audience};
use crate::connectors::bluesky::{BlueskyClient, authenticated_xrpc_candidates};
use crate::richtext;
use crate::blob_hash;

// ═══════════════════════════════════════════════════════════════════════════════
// Syndication
// ═══════════════════════════════════════════════════════════════════════════════

/// Cross-post a vault item to Bluesky.
///
/// Takes a local `BytescapeItem` by ID, uploads its media blobs to the user's
/// Bluesky PDS, composes an `app.bsky.feed.post` record with rich-text facets,
/// and publishes it. On success the item's syndication metadata is persisted
/// back to redb.
///
/// # Parameters
/// - `item_id` — The redb key of the vault item to syndicate.
/// - `state` — Tauri managed state (database, keychain, sync manager).
///
/// # Returns
/// The AT URI of the newly created Bluesky post (`at://did:plc:.../app.bsky.feed.post/...`).
///
/// # Errors
/// - Item not found or already syndicated to Bluesky.
/// - Not authenticated (no session in keychain).
/// - Blob upload or record creation fails on the PDS.
///
/// # Side Effects
/// - Uploads image blobs to the user's Bluesky PDS.
/// - Writes updated `BytescapeItem` to redb (`syndication_status`, `syndicated_uri`,
///   `audiences`, `share_history`).
///
/// # Notes
/// - Currently **Bluesky-only** despite the generic function name.
/// - Silently caps at **4 images** per post (Bluesky embed limit); extras are dropped.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('syndicate_vault_item', { itemId })`
#[tauri::command]
pub async fn syndicate_vault_item(
    item_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    println!("syndicate: starting for item {}", item_id);

    // 1. Load item from database
    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let mut item = db.get_item_by_id(&item_id).await?
        .ok_or_else(|| format!("Item not found: {}", item_id))?;
    drop(db_guard);

    if item.audiences.contains(&Audience::Bluesky) || item.syndication_status == SyndicationStatus::Syndicated {
        return Err("Item is already syndicated to Bluesky".to_string());
    }

    // 2. Prepare Bluesky client and session
    let session = state.keychain.load_session()
        .map_err(|e| format!("Keychain error: {}", e))?
        .ok_or("Not authenticated with Bluesky")?;
    let (user_did, _) = session;

    let access_jwt = state.get_fresh_access_jwt().await?;

    let candidates = authenticated_xrpc_candidates(&access_jwt, true);
    let pds_base = candidates.first().ok_or("Could not resolve Bluesky PDS")?;
    let client = BlueskyClient::with_jwt(&access_jwt);

    // 3. Harvest and upload media blobs
    let mut images_embed = Vec::new();
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref().ok_or("Sync manager not initialized")?;

    // Prepare cache directory for streaming blob exports (avoids full-blob memory reads)
    let app_data_dir = state.app_handle.path()
        .app_data_dir()
        .map_err(|e| format!("App data dir error: {}", e))?;
    let cache_dir = app_data_dir.join("blob_cache");
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("Cache dir error: {}", e))?;

    for (i, blob_hash_str) in item.media_blobs.iter().enumerate() {
        // We only support images for now. Check mime if available.
        if let Some(mime) = item.media_mimes.get(i) {
            if !mime.starts_with("image/") {
                println!("syndicate: skipping non-image blob {}", blob_hash_str);
                continue;
            }
        }

        // Parse hash (centralized)
        let hash = match blob_hash::parse_blob_hash(blob_hash_str) {
            Ok(h) => h,
            Err(e) => {
                println!("syndicate: invalid blob hash '{}': {}", blob_hash_str, e);
                continue;
            }
        };

        // Stream blob to cache file, then read once for upload (halves peak memory
        // vs. get_media which holds entire blob in Iroh's Bytes + a .to_vec() copy).
        let normalized = blob_hash::normalize_blob_ref(blob_hash_str);
        let header = sync.get_media_header(hash, 12).await
            .map_err(|e| format!("Failed to read header for blob {}: {}", blob_hash_str, e))?;
        let ext = super::media::detect_extension_from_bytes(&header);
        let cache_path = cache_dir.join(format!("{}.{}", &normalized, ext));
        if !cache_path.exists() {
            sync.export_media_to_file(hash, &cache_path).await
                .map_err(|e| format!("Failed to export blob {} to cache: {}", blob_hash_str, e))?;
        }
        let data = std::fs::read(&cache_path)
            .map_err(|e| format!("Failed to read cached blob {}: {}", blob_hash_str, e))?;

        let mime = item.media_mimes.get(i).cloned().unwrap_or_else(|| "image/jpeg".to_string());

        // Upload to Bluesky PDS
        let upload_url = format!("{}/com.atproto.repo.uploadBlob", pds_base);
        let resp = client.post_blob(&upload_url, &mime, data).await?;
        
        if !resp.status().is_success() {
            let err_body = resp.text().await.unwrap_or_default();
            return Err(format!("Bluesky blob upload failed: {}", err_body));
        }

        let upload_res: serde_json::Value = resp.json().await
            .map_err(|e| format!("Failed to parse upload response: {}", e))?;

        let blob_ref = upload_res.get("blob").ok_or("Missing blob ref in response")?.clone();
        
        images_embed.push(json!({
            "alt": "", // Could potentially leverage AI for alt text in the future
            "image": blob_ref
        }));

        // Bluesky has a limit of 4 images per post
        if images_embed.len() >= 4 {
            break;
        }
    }
    drop(sync_guard);

    // 4. Parse rich text facets (mentions → DID resolution, hashtags, links)
    let facets = richtext::parse_facets_for_bluesky(&item.content, &access_jwt).await;

    // 5. Construct and publish post record
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    
    let mut record = json!({
        "$type": "app.bsky.feed.post",
        "text": item.content,
        "createdAt": now,
    });

    if !facets.is_empty() {
        record["facets"] = serde_json::to_value(&facets).unwrap_or_default();
    }

    if !images_embed.is_empty() {
        record["embed"] = json!({
            "$type": "app.bsky.embed.images",
            "images": images_embed
        });
    }

    let create_url = format!("{}/com.atproto.repo.createRecord", pds_base);
    let create_body = json!({
        "repo": user_did,
        "collection": "app.bsky.feed.post",
        "record": record
    });

    let resp = client.create_record(&create_url, create_body).await?;
    if !resp.status().is_success() {
        let err_body = resp.text().await.unwrap_or_default();
        return Err(format!("Bluesky post creation failed: {}", err_body));
    }

    let create_res: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse create record response: {}", e))?;

    let uri = create_res["uri"].as_str().ok_or("Missing uri in response")?.to_string();
    let _cid = create_res["cid"].as_str().ok_or("Missing cid in response")?.to_string();

    // 5. Update item in database
    item.syndication_status = SyndicationStatus::Syndicated;
    item.syndicated_uri = Some(uri.clone());
    if !item.audiences.contains(&Audience::Bluesky) {
        item.audiences.push(Audience::Bluesky);
    }
    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default().as_secs();
    item.share_history.push(crate::models::SyndicationEvent {
        platform: "bluesky".to_string(),
        uri: uri.clone(),
        timestamp: now_ts,
        active: true,
    });

    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    db.upsert_item(&item).await?;

    println!("syndicate: success for item {}, uri: {}", item_id, uri);
    Ok(uri)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Verification
// ═══════════════════════════════════════════════════════════════════════════════

/// Response shape for `app.bsky.feed.getPosts` — used internally for verification.
#[derive(serde::Deserialize)]
struct GetPostsResponse {
    posts: Vec<serde_json::Value>,
}

/// Verify that previously syndicated posts still exist on Bluesky.
///
/// Checks each active `SyndicationEvent` in the item's `share_history` against
/// the Bluesky public API (`app.bsky.feed.getPosts`). If a post is no longer
/// found upstream (deleted by user, moderation takedown, etc.), its `active`
/// flag is set to `false` so the UI can reflect the broken syndication.
///
/// Also migrates legacy syndication fields (`syndicated_uris`, `syndicated_at`,
/// `syndicated_uri`) into the modern `share_history` array on first call.
///
/// # Parameters
/// - `item_id` — The redb key of the vault item to verify.
/// - `state` — Tauri managed state (database, keychain).
///
/// # Returns
/// `true` if any syndication events were updated (rolled back), `false` if
/// everything is still in sync.
///
/// # Errors
/// - Item not found in database.
/// - Bluesky public API request fails.
///
/// # Side Effects
/// - May set `share_history` entries to `active: false` in redb.
/// - May migrate legacy syndication fields into `share_history`.
///
/// # Tauri IPC
/// Frontend invokes: `invoke('verify_syndications', { itemId })`
#[tauri::command]
pub async fn verify_syndications(
    item_id: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    println!("sync: verifying syndications for item {}", item_id);

    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let mut item = db.get_item_by_id(&item_id).await?.ok_or("Item not found")?;
    drop(db_guard);

    let mut changed = false;

    // Migrate from legacy fields if necessary
    if item.share_history.is_empty() {
        let migrated = crate::models::share_history_from_legacy(
            &item.syndicated_uris,
            &item.syndicated_at,
            &item.syndicated_uri,
            &item.syndication_status,
        );
        if !migrated.is_empty() {
            item.share_history = migrated;
            changed = true; // Needs saving even if Bluesky responds OK
        }
    }

    let mut bsky_uris_to_check = Vec::new();
    for event in &item.share_history {
        if event.platform == "bluesky" && event.active {
            bsky_uris_to_check.push(event.uri.clone());
        }
    }

    if bsky_uris_to_check.is_empty() {
        if changed {
            // Save the migrated history even if nothing to check
            let db_guard = state.db_manager.read().await;
            if let Some(db) = db_guard.as_ref() {
                let _ = db.upsert_item(&item).await;
            }
        }
        return Ok(false); // Nothing to do
    }

    // Check Bluesky posts
    let mut url = "https://public.api.bsky.app/xrpc/app.bsky.feed.getPosts?".to_string();
    for uri in &bsky_uris_to_check {
        url.push_str(&format!("uris={}&", urlencoding::encode(uri)));
    }

    let client = reqwest::Client::new();
    println!("sync: Sending request to: {}", url);
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;

    let status = resp.status();
    println!("sync: bsky api returned status: {}", status);

    if status.is_success() {
        let body: GetPostsResponse = resp.json().await.map_err(|e| e.to_string())?;
        let returned_uris: std::collections::HashSet<String> = body.posts
            .into_iter()
            .filter_map(|p| p.get("uri").and_then(|u| u.as_str()).map(|s| s.to_string()))
            .collect();
        println!("sync: bsky returned {} posts", returned_uris.len());

        for event in &mut item.share_history {
            if event.platform == "bluesky" && event.active {
                if !returned_uris.contains(&event.uri) {
                    println!("sync: Post {} not found upstream. Rolling back active status.", event.uri);
                    event.active = false;
                    changed = true;
                }
            }
        }
    } else {
        let err_text = resp.text().await.unwrap_or_default();
        println!("sync: bsky api error: {}", err_text);
        return Err(format!("Failed to query Bluesky API: {}", status));
    }

    if changed {
        let db_guard = state.db_manager.read().await;
        if let Some(db) = db_guard.as_ref() {
            db.upsert_item(&item).await?;
            println!("sync: Rolled back history successfully for {}", item_id);
        }
    }

    Ok(changed)
}
