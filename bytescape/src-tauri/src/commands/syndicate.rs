use tauri::State;

use serde_json::json;
use crate::AppState;
use crate::models::{SyndicationStatus, Audience};
use crate::connectors::bluesky::{BlueskyClient, authenticated_xrpc_candidates};
use crate::sync::Hash;
use data_encoding::BASE32_NOPAD;

#[tauri::command]
pub async fn syndicate_vault_item(
    item_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    println!("syndicate: starting for item {}", item_id);

    // 1. Load item from database
    let db_guard = state.db_manager.lock().await;
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

    let access_jwt = state.keychain.get_access_jwt()
        .map_err(|e| format!("Keychain error: {}", e))?
        .ok_or("No access token available")?;

    let candidates = authenticated_xrpc_candidates(&access_jwt, true);
    let pds_base = candidates.first().ok_or("Could not resolve Bluesky PDS")?;
    let client = BlueskyClient::new(&state);

    // 3. Harvest and upload media blobs
    let mut images_embed = Vec::new();
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref().ok_or("Sync manager not initialized")?;

    for (i, blob_hash_str) in item.media_blobs.iter().enumerate() {
        // We only support images for now. Check mime if available.
        if let Some(mime) = item.media_mimes.get(i) {
            if !mime.starts_with("image/") {
                println!("syndicate: skipping non-image blob {}", blob_hash_str);
                continue;
            }
        }

        // Parse hash
        let mut normalized = blob_hash_str.trim().to_string();
        while let Some(rest) = normalized.strip_prefix("blob:") {
            normalized = rest.trim().to_string();
        }
        if let Some(pos) = normalized.find(':') {
            normalized = normalized[..pos].to_string();
        }

        let hash_bytes = if normalized.len() == 64 {
            // Hex
            hex::decode(&normalized).map_err(|e| format!("Invalid hex hash: {}", e))?
        } else {
            // Base32
            BASE32_NOPAD.decode(normalized.to_ascii_uppercase().as_bytes())
                .map_err(|e| format!("Invalid base32 hash: {}", e))?
        };

        if hash_bytes.len() != 32 {
            return Err(format!("Invalid hash length: {} (expected 32)", hash_bytes.len()));
        }

        let mut arr = [0u8; 32];
        arr.copy_from_slice(&hash_bytes);
        let hash = Hash::from_bytes(arr);

        // Fetch from Iroh
        let data = sync.get_media(hash).await
            .map_err(|e| format!("Failed to fetch blob {} from vault: {}", blob_hash_str, e))?;

        let mime = item.media_mimes.get(i).cloned().unwrap_or_else(|| "image/jpeg".to_string());

        // Upload to Bluesky PDS
        let upload_url = format!("{}/com.atproto.repo.uploadBlob", pds_base);
        let resp = client.post_blob(&upload_url, &mime, data.to_vec()).await?;
        
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

    // 4. Construct and publish post record
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    
    let mut record = json!({
        "$type": "app.bsky.feed.post",
        "text": item.content,
        "createdAt": now,
    });

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
    item.syndicated_uris.insert("bluesky".to_string(), uri.clone());
    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default().as_secs();
    item.syndicated_at.insert("bluesky".to_string(), now_ts);

    let db_guard = state.db_manager.lock().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    db.upsert_item(&item).await?;

    println!("syndicate: success for item {}, uri: {}", item_id, uri);
    Ok(uri)
}
