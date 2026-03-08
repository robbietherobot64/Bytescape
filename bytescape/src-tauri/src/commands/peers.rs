//! P2P Peer Discovery & Following Commands
//!
//! Commands for managing the local peer directory — adding, listing,
//! removing, and checking the connectivity of known P2P peers.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::peers::PeerInfo;
use crate::protocol::{PeerProfile, ProtocolRequest, BYTESCAPE_ALPN};
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

/// Result of a peer connectivity check, including the remote peer's profile
/// if the connection succeeded.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PeerConnectResult {
    pub online: bool,
    pub profile: Option<PeerProfile>,
}

/// Attempt to connect to a peer, exchange profiles, and report connectivity.
///
/// Uses Iroh's address lookup (mDNS for LAN, Pkarr DHT for internet)
/// to resolve the peer and establish a QUIC connection. On success,
/// sends a PROFILE_EXCHANGE with our own profile, reads the remote's
/// profile, and updates the peer's display name, DID, and last_seen
/// timestamp in the local database.
#[tauri::command]
pub async fn check_peer_connectivity(
    node_id: String,
    state: State<'_, AppState>,
) -> Result<PeerConnectResult, String> {
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

    // Build our profile to send to the remote peer
    let my_username = state.keychain.get_local_username()
        .unwrap_or_else(|_| None)
        .unwrap_or_else(|| "Anonymous".to_string());
    let my_did = state.keychain.get_did()
        .unwrap_or_else(|_| None)
        .unwrap_or_else(|| "local".to_string());
    let my_profile = PeerProfile {
        node_id: endpoint.secret_key().public().to_string(),
        username: my_username,
        did: my_did,
    };

    let endpoint_addr = iroh::EndpointAddr::from(node_id_parsed);

    // Try to connect and exchange profiles (15s timeout for mobile NAT traversal)
    let result = match tokio::time::timeout(
        std::time::Duration::from_secs(15),
        async {
            let connection = endpoint.connect(endpoint_addr, BYTESCAPE_ALPN).await
                .map_err(|e| format!("Connection failed: {}", e))?;
            let (mut send, mut recv) = connection.open_bi().await
                .map_err(|e| format!("Failed to open stream: {}", e))?;

            // Send PROFILE_EXCHANGE with our profile (bidirectional)
            let request = ProtocolRequest {
                request: "PROFILE_EXCHANGE".to_string(),
                profile: Some(my_profile),
            };
            let request_json = serde_json::to_vec(&request)
                .map_err(|e| format!("Failed to serialize request: {}", e))?;
            send.write_all(&request_json).await
                .map_err(|e| format!("Failed to send request: {}", e))?;
            let _ = send.finish();

            // Read response (max 4 KiB)
            let response = recv.read_to_end(4096).await
                .map_err(|e| format!("Failed to read response: {}", e))?;

            // Parse as PeerProfile
            let profile: PeerProfile = serde_json::from_slice(&response)
                .map_err(|e| format!("Failed to parse profile: {}", e))?;

            Ok::<PeerProfile, String>(profile)
        }
    ).await {
        Ok(Ok(profile)) => {
            println!("Peer {} connected — username: {}", &node_id[..8.min(node_id.len())], profile.username);
            PeerConnectResult { online: true, profile: Some(profile) }
        }
        Ok(Err(e)) => {
            println!("Peer {} unreachable: {}", &node_id[..8.min(node_id.len())], e);
            PeerConnectResult { online: false, profile: None }
        }
        Err(_) => {
            // Even though we couldn't connect outbound, the peer may have
            // connected TO us recently. Check last_seen.
            let db_guard = state.db_manager.lock().await;
            let recently_seen = if let Some(db) = db_guard.as_ref() {
                if let Ok(Some(peer)) = db.get_peer(&node_id).await {
                    if let Some(last_seen) = peer.last_seen {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        // If we saw them within the last 60 seconds, they're online
                        now.saturating_sub(last_seen) < 60
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };
            drop(db_guard);

            if recently_seen {
                println!("Peer {} timed out but was recently seen (inbound connection)", &node_id[..8.min(node_id.len())]);
                PeerConnectResult { online: true, profile: None }
            } else {
                println!("Peer {} connectivity check timed out", &node_id[..8.min(node_id.len())]);
                PeerConnectResult { online: false, profile: None }
            }
        }
    };

    // Update peer in database with profile info and last_seen
    if result.online {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let db_guard = state.db_manager.lock().await;
        if let Some(db) = db_guard.as_ref() {
            if let Ok(Some(mut peer)) = db.get_peer(&node_id).await {
                peer.last_seen = Some(now);
                // Update display name and DID from the remote profile
                if let Some(ref profile) = result.profile {
                    if !profile.username.is_empty() && profile.username != "Anonymous" {
                        peer.display_name = Some(profile.username.clone());
                    }
                    if !profile.did.is_empty() && profile.did != "local" {
                        peer.did = Some(profile.did.clone());
                    }
                }
                let _ = db.save_peer(&peer).await;
            }
        }
    }

    Ok(result)
}
