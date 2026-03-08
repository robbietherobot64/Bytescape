//! P2P Peer Discovery & Following Commands
//!
//! Commands for managing the local peer directory — adding, listing,
//! removing, and checking the connectivity of known P2P peers.

use serde::Serialize;
use tauri::State;

use crate::db::peers::PeerInfo;
use crate::AppState;

/// Summary of the local user's P2P identity, returned to the frontend.
#[derive(Serialize)]
pub struct MyPeerInfo {
    pub node_id: String,
    pub username: Option<String>,
    pub did: Option<String>,
}

/// Return the current user's P2P identity (node ID, username, DID).
///
/// Requires the Iroh node to be initialised via `setup_iroh_node`.
#[tauri::command]
pub async fn get_my_peer_info(state: State<'_, AppState>) -> Result<MyPeerInfo, String> {
    let username = state
        .keychain
        .get_local_username()
        .map_err(|e| e.to_string())?;
    let did = state
        .keychain
        .get_owner_did()
        .map_err(|e| e.to_string())?;

    let node_id = {
        let guard = state.endpoint.lock().await;
        guard
            .as_ref()
            .ok_or_else(|| "Iroh node not initialized — go to Settings → Sync to start it".to_string())?
            .secret_key()
            .public()
            .to_string()
    };

    Ok(MyPeerInfo {
        node_id,
        username,
        did,
    })
}

/// Add (or update) a peer by their Iroh Node ID.
///
/// The `display_name` is an optional human-readable label the user
/// assigns to this peer (e.g. "Alice's laptop").
#[tauri::command]
pub async fn add_peer(
    node_id: String,
    display_name: Option<String>,
    state: State<'_, AppState>,
) -> Result<PeerInfo, String> {
    let node_id = node_id.trim().to_string();
    if node_id.is_empty() {
        return Err("Node ID cannot be empty".to_string());
    }

    // Validate that we can parse it as an Iroh PublicKey (EndpointId)
    let _parsed: iroh::PublicKey = node_id
        .parse()
        .map_err(|e| format!("Invalid Node ID format: {}", e))?;

    // Prevent adding yourself
    {
        let guard = state.endpoint.lock().await;
        if let Some(ep) = guard.as_ref() {
            let my_id = ep.secret_key().public().to_string();
            if my_id == node_id {
                return Err("That's your own Node ID — you can't follow yourself!".to_string());
            }
        }
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // If the peer already exists, preserve existing fields and update
    let db_guard = state.db_manager.lock().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    let peer = match db.get_peer(&node_id).await? {
        Some(mut existing) => {
            // Update display name if provided
            if display_name.is_some() {
                existing.display_name = display_name;
            }
            existing.is_following = true;
            existing
        }
        None => PeerInfo {
            node_id: node_id.clone(),
            display_name,
            did: None,
            added_at: now,
            last_seen: None,
            is_following: true,
        },
    };

    db.save_peer(&peer).await?;
    Ok(peer)
}

/// List all known peers from the local database.
#[tauri::command]
pub async fn list_peers(state: State<'_, AppState>) -> Result<Vec<PeerInfo>, String> {
    let db_guard = state.db_manager.lock().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    db.list_peers().await
}

/// Remove a peer from the local directory.
#[tauri::command]
pub async fn remove_peer(node_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let db_guard = state.db_manager.lock().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    db.remove_peer(&node_id).await
}

/// Attempt to connect to a peer and report whether they're reachable.
///
/// Uses Iroh's address lookup (mDNS for LAN, Pkarr DHT for internet)
/// to resolve the peer and establish a QUIC connection with a 5-second
/// timeout. On success, updates the peer's `last_seen` timestamp.
#[tauri::command]
pub async fn check_peer_connectivity(
    node_id: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let node_id_parsed: iroh::PublicKey = node_id
        .parse()
        .map_err(|e| format!("Invalid Node ID: {}", e))?;

    // Clone the endpoint so we don't hold the mutex during the network call
    let endpoint = {
        let guard = state.endpoint.lock().await;
        guard
            .as_ref()
            .ok_or("Iroh node not initialized")?
            .clone()
    };

    let endpoint_addr = iroh::EndpointAddr::from(node_id_parsed);

    let is_online = match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        endpoint.connect(endpoint_addr, b"bytescape/1.0"),
    )
    .await
    {
        Ok(Ok(_connection)) => {
            // Connection succeeded — peer is reachable.
            // The connection is dropped here, which closes it cleanly.
            true
        }
        Ok(Err(e)) => {
            println!("Peer {} unreachable: {}", &node_id[..8.min(node_id.len())], e);
            false
        }
        Err(_) => {
            println!("Peer {} connectivity check timed out", &node_id[..8.min(node_id.len())]);
            false
        }
    };

    // Update last_seen if online
    if is_online {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let db_guard = state.db_manager.lock().await;
        if let Some(db) = db_guard.as_ref() {
            if let Ok(Some(mut peer)) = db.get_peer(&node_id).await {
                peer.last_seen = Some(now);
                let _ = db.save_peer(&peer).await;
            }
        }
    }

    Ok(is_online)
}
