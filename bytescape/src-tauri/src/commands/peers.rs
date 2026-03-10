//! P2P Peer Discovery & Following Commands
//!
//! Commands for managing the local peer directory — adding, listing,
//! removing, and checking the connectivity of known P2P peers.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::peers::PeerInfo;
use crate::protocol::{PeerProfile, ProtocolRequest, ChatMessagePayload, BYTESCAPE_ALPN};
use crate::AppState;

/// Summary of the local user's P2P identity, returned to the frontend.
#[derive(Serialize)]
pub struct MyPeerInfo {
    pub node_id: String,
    pub username: Option<String>,
    pub did: Option<String>,
    pub also_known_as: Vec<String>,
}

/// Return the current user's P2P identity (node ID, username, DID).
///
/// Requires the Iroh node to be initialised via `setup_iroh_node`.
#[tauri::command]
pub async fn get_my_peer_info(state: State<'_, AppState>) -> Result<MyPeerInfo, String> {
    let username = Some(state.keychain.get_display_name());
    let did = state
        .keychain
        .get_owner_did()
        .map_err(|e| e.to_string())?;

    // Build alsoKnownAs linked identities
    let mut also_known_as = Vec::new();
    if let Ok(Some(bsky_did)) = state.keychain.get_bluesky_did() {
        also_known_as.push(bsky_did);
    }
    if let Ok(Some(handle)) = state.keychain.get_handle() {
        also_known_as.push(format!("at://{}", handle));
    }

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
        also_known_as,
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
    let my_username = state.keychain.get_display_name();
    let my_did = state.keychain.get_owner_did()
        .unwrap_or_else(|_| None)
        .unwrap_or_else(|| "local".to_string());

    let mut also_known_as = Vec::new();
    if let Ok(Some(bsky_did)) = state.keychain.get_bluesky_did() {
        also_known_as.push(bsky_did);
    }
    if let Ok(Some(handle)) = state.keychain.get_handle() {
        also_known_as.push(format!("at://{}", handle));
    }

    let my_profile = PeerProfile {
        node_id: endpoint.secret_key().public().to_string(),
        username: my_username,
        did: my_did,
        also_known_as,
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
                message: None,
                feed_params: None,
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

/// A P2P conversation entry for the frontend chat sidebar.
#[derive(Serialize, Clone, Debug)]
pub struct P2pConversation {
    /// Deterministic convo ID: "p2p:{node_id}"
    pub convo_id: String,
    /// Peer's display name or truncated node ID
    pub display_name: String,
    /// Peer's node ID
    pub node_id: String,
    /// Peer's DID if known
    pub did: Option<String>,
    /// Whether the peer was recently seen online
    pub online: bool,
    /// Number of messages in this conversation
    pub message_count: usize,
}

/// List all known P2P peers as potential conversations.
///
/// Returns a list of P2P conversation entries that the chat sidebar
/// can display. Each peer gets a deterministic `convo_id` of `p2p:{node_id}`.
#[tauri::command]
pub async fn list_p2p_conversations(
    state: State<'_, AppState>,
) -> Result<Vec<P2pConversation>, String> {
    let db_guard = state.db_manager.lock().await;
    let db = db_guard.as_ref()
        .ok_or("Database not initialized")?;

    let peers = db.list_peers().await.unwrap_or_default();
    let owner_did = state.keychain.get_owner_did()
        .map_err(|e| e.to_string())?;

    // Count messages per P2P convo
    let all_msgs = db.list_vault_messages(owner_did.as_deref()).await.unwrap_or_default();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let conversations: Vec<P2pConversation> = peers.iter().map(|peer| {
        let convo_id = format!("p2p:{}", peer.node_id);
        let msg_count = all_msgs.iter().filter(|m| m.convo_id == convo_id).count();
        let online = peer.last_seen.map(|ls| now.saturating_sub(ls) < 120).unwrap_or(false);

        P2pConversation {
            convo_id,
            display_name: peer.display_name.clone()
                .unwrap_or_else(|| {
                    let id = &peer.node_id;
                    if id.len() > 12 { format!("{}…", &id[..12]) } else { id.clone() }
                }),
            node_id: peer.node_id.clone(),
            did: peer.did.clone(),
            online,
            message_count: msg_count,
        }
    }).collect();

    Ok(conversations)
}

/// Send a P2P chat message to a peer.
///
/// 1. Connects to the peer via the Bytescape protocol
/// 2. Sends a CHAT_MESSAGE with the text
/// 3. Saves a copy locally as a VaultMessage (source: "p2p")
/// 4. Returns the message ID on success
#[tauri::command]
pub async fn send_p2p_chat_message(
    node_id: String,
    text: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let node_id_parsed: iroh::PublicKey = node_id
        .parse()
        .map_err(|e| format!("Invalid Node ID: {}", e))?;

    let endpoint = {
        let guard = state.endpoint.lock().await;
        guard
            .as_ref()
            .ok_or("Iroh node not initialized")?
            .clone()
    };

    // Build sender info — use get_owner_did to get the real DID (Bluesky or local)
    let my_username = state.keychain.get_display_name();
    let my_did = state.keychain.get_owner_did()
        .unwrap_or(None)
        .unwrap_or_else(|| "local".to_string());
    let my_node_id = endpoint.secret_key().public().to_string();
    println!("[P2P-CHAT] Sending message to {}: my_did={}, my_username={}", &node_id[..8.min(node_id.len())], my_did, my_username);

    let mut chat_also_known_as = Vec::new();
    if let Ok(Some(bsky_did)) = state.keychain.get_bluesky_did() {
        chat_also_known_as.push(bsky_did);
    }
    if let Ok(Some(handle)) = state.keychain.get_handle() {
        chat_also_known_as.push(format!("at://{}", handle));
    }

    let my_profile = PeerProfile {
        node_id: my_node_id.clone(),
        username: my_username.clone(),
        did: my_did.clone(),
        also_known_as: chat_also_known_as,
    };

    // Build message payload
    let msg_id = uuid::Uuid::new_v4().to_string();
    let sent_at = chrono::Utc::now().to_rfc3339();
    let convo_id = format!("p2p:{}", node_id);

    let msg_payload = ChatMessagePayload {
        id: msg_id.clone(),
        text: text.clone(),
        sent_at: sent_at.clone(),
    };

    let request = ProtocolRequest {
        request: "CHAT_MESSAGE".to_string(),
        profile: Some(my_profile),
        message: Some(msg_payload),
        feed_params: None,
    };

    // Try to send over the network (10s timeout)
    let send_result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        async {
            let endpoint_addr = iroh::EndpointAddr::from(node_id_parsed);
            let connection = endpoint.connect(endpoint_addr, BYTESCAPE_ALPN).await
                .map_err(|e| format!("Connection failed: {}", e))?;
            let (mut send, mut recv) = connection.open_bi().await
                .map_err(|e| format!("Failed to open stream: {}", e))?;

            let request_json = serde_json::to_vec(&request)
                .map_err(|e| format!("Serialize error: {}", e))?;
            send.write_all(&request_json).await
                .map_err(|e| format!("Send error: {}", e))?;
            let _ = send.finish();

            let ack = recv.read_to_end(256).await
                .map_err(|e| format!("Read ACK error: {}", e))?;
            let ack_str = String::from_utf8_lossy(&ack);
            if ack_str != "ACK" {
                return Err(format!("Peer rejected message: {}", ack_str));
            }
            Ok(())
        }
    ).await;

    let delivered = match send_result {
        Ok(Ok(())) => {
            println!("[P2P-CHAT] Message delivered to {}", &node_id[..8.min(node_id.len())]);
            true
        }
        Ok(Err(e)) => {
            println!("[P2P-CHAT] Send failed to {}: {}", &node_id[..8.min(node_id.len())], e);
            false
        }
        Err(_) => {
            println!("[P2P-CHAT] Send timed out to {}", &node_id[..8.min(node_id.len())]);
            false
        }
    };

    // Always save our outgoing message locally
    let vault_msg = crate::db::VaultMessage {
        id: msg_id.clone(),
        owner_did: Some(my_did.clone()),
        convo_id: convo_id.clone(),
        sender_did: my_did.clone(),
        sender_handle: my_username.clone(),
        text: text.clone(),
        sent_at: sent_at.clone(),
        source: "p2p".to_string(),
        imported_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        peer_did: Some(format!("p2p:{}", node_id)),
        peer_handle: None,
    };

    let db_guard = state.db_manager.lock().await;
    if let Some(db) = db_guard.as_ref() {
        match db.save_vault_message(&vault_msg).await {
            Ok(is_new) => println!("[P2P-CHAT] Outgoing message saved: id={}, is_new={}, convo_id={}", msg_id, is_new, convo_id),
            Err(e) => println!("[P2P-CHAT] ERROR saving outgoing message: {}", e),
        }
    }

    Ok(serde_json::json!({
        "id": msg_id,
        "convo_id": convo_id,
        "delivered": delivered,
        "sent_at": sent_at,
    }))
}

/// Request a peer's feed (posts/profile) over P2P.
///
/// Connects to the peer's Iroh node, sends a FEED_REQUEST, and returns
/// their profile + a page of BytescapeItems.
#[tauri::command]
pub async fn request_peer_feed(
    node_id: String,
    limit: Option<usize>,
    offset: Option<usize>,
    source_filter: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let node_id = node_id.trim().to_string();
    let node_id_parsed: iroh::PublicKey = node_id
        .parse()
        .map_err(|e| format!("Invalid Node ID: {}", e))?;

    let endpoint = {
        let guard = state.endpoint.lock().await;
        guard
            .as_ref()
            .ok_or("Iroh node not initialized")?
            .clone()
    };

    // Build our profile to include in the request
    let my_username = state.keychain.get_display_name();
    let my_did = state.keychain.get_owner_did()
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "local".to_string());
    let my_node_id = endpoint.secret_key().public().to_string();

    let mut also_known_as = Vec::new();
    if let Ok(Some(bsky_did)) = state.keychain.get_bluesky_did() {
        also_known_as.push(bsky_did);
    }
    if let Ok(Some(handle)) = state.keychain.get_handle() {
        also_known_as.push(format!("at://{}", handle));
    }

    let my_profile = PeerProfile {
        node_id: my_node_id,
        username: my_username,
        did: my_did,
        also_known_as,
    };

    let feed_params = crate::protocol::FeedRequestParams {
        limit: limit.unwrap_or(20),
        offset: offset.unwrap_or(0),
        source_filter,
    };

    let request = ProtocolRequest {
        request: "FEED_REQUEST".to_string(),
        profile: Some(my_profile),
        message: None,
        feed_params: Some(feed_params),
    };

    // Fetch cached peer info from DB for fallback profile
    let cached_peer = {
        let db_guard = state.db_manager.lock().await;
        if let Some(db) = db_guard.as_ref() {
            db.get_peer(&node_id).await.ok().flatten()
        } else {
            None
        }
    };

    println!("[P2P-FEED] Requesting feed from {} (cached_name: {:?})",
        &node_id[..8.min(node_id.len())],
        cached_peer.as_ref().and_then(|p| p.display_name.as_ref()));

    // Retry once on timeout — first attempt uses longer timeout since
    // NAT traversal may take time on first connection
    let mut last_error = String::new();
    for attempt in 0..2u32 {
        let timeout_secs = if attempt == 0 { 20 } else { 15 };
        println!("[P2P-FEED] Attempt {} (timeout: {}s)", attempt + 1, timeout_secs);

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            async {
                let endpoint_addr = iroh::EndpointAddr::from(node_id_parsed);
                println!("[P2P-FEED] Connecting to {}...", &node_id[..8.min(node_id.len())]);
                let connection = endpoint.connect(endpoint_addr, BYTESCAPE_ALPN).await
                    .map_err(|e| format!("Connection failed: {}", e))?;
                println!("[P2P-FEED] Connected, opening bi-stream...");
                let (mut send, mut recv) = connection.open_bi().await
                    .map_err(|e| format!("Failed to open stream: {}", e))?;

                let request_json = serde_json::to_vec(&request)
                    .map_err(|e| format!("Serialize error: {}", e))?;
                println!("[P2P-FEED] Sending {} bytes request...", request_json.len());
                send.write_all(&request_json).await
                    .map_err(|e| format!("Send error: {}", e))?;
                let _ = send.finish(); // Non-fatal: data is already queued

                println!("[P2P-FEED] Waiting for response...");
                // Read feed response (up to 2 MiB for large feeds)
                let response_bytes = recv.read_to_end(2 * 1024 * 1024).await
                    .map_err(|e| format!("Read error: {}", e))?;

                println!("[P2P-FEED] Received {} bytes response", response_bytes.len());
                let feed_response: crate::protocol::FeedResponse = serde_json::from_slice(&response_bytes)
                    .map_err(|e| format!("Parse error: {}", e))?;

                Ok::<_, String>(feed_response)
            }
        ).await;

        match result {
            Ok(Ok(feed)) => {
                println!(
                    "[P2P-FEED] Received feed from {}: {} items, total={} (attempt {})",
                    &node_id[..8.min(node_id.len())],
                    feed.items.len(),
                    feed.total_count,
                    attempt + 1
                );
                return Ok(serde_json::to_value(&feed).map_err(|e| e.to_string())?);
            }
            Ok(Err(e)) => {
                println!("[P2P-FEED] Feed request failed for {} (attempt {}): {}",
                    &node_id[..8.min(node_id.len())], attempt + 1, e);
                last_error = e;
                // Don't retry on non-timeout errors (parse, serialize, connection refused)
                if !last_error.contains("Connection failed") {
                    break;
                }
            }
            Err(_) => {
                println!("[P2P-FEED] Feed request timed out for {} (attempt {})",
                    &node_id[..8.min(node_id.len())], attempt + 1);
                last_error = "Feed request timed out (peer may be offline)".to_string();
                // Retry on timeout
            }
        }

        // Brief pause before retry
        if attempt < 1 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    // If we have a cached peer profile, return it with empty items so the UI
    // at least shows the peer's name instead of "Anonymous"
    if let Some(peer) = &cached_peer {
        let fallback = crate::protocol::FeedResponse {
            profile: PeerProfile {
                node_id: node_id.clone(),
                username: peer.display_name.clone().unwrap_or_else(|| "Anonymous".to_string()),
                did: peer.did.clone().unwrap_or_else(|| "unknown".to_string()),
                also_known_as: vec![],
            },
            items: vec![],
            total_count: 0,
        };
        // Return the error alongside the cached profile info
        // Use a special structure the frontend can detect
        let mut val = serde_json::to_value(&fallback).map_err(|e| e.to_string())?;
        val["_error"] = serde_json::Value::String(last_error);
        return Ok(val);
    }

    Err(last_error)
}
