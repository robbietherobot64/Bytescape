//! Bytescape P2P Protocol Handlers
//!
//! Implements [`ProtocolHandler`] for the custom Bytescape protocol
//! and a thin wrapper for forwarding iroh-docs connections.

use std::sync::Arc;

use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh_docs::engine::Engine as DocsEngine;
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::sync::Mutex;

/// ALPN identifier for the Bytescape custom protocol.
pub const BYTESCAPE_ALPN: &[u8] = b"bytescape/1.0";

// ---------------------------------------------------------------------------
// Trivial Error wrapper (needed because AcceptError::from_err requires
// std::error::Error, and anyhow::Error doesn't implement it).
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct ProtoError(String);

impl std::fmt::Display for ProtoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ProtoError {}

fn to_accept_err(e: impl std::fmt::Display) -> AcceptError {
    AcceptError::from_err(ProtoError(e.to_string()))
}

// ---------------------------------------------------------------------------
// Peer Profile — exchanged over the bytescape/1.0 protocol
// ---------------------------------------------------------------------------

/// Profile information exchanged between peers on connect.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PeerProfile {
    pub node_id: String,
    pub username: String,
    pub did: String,
}

/// A request envelope sent over the bytescape/1.0 protocol.
/// The requester includes their own profile so both sides learn about each other.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProtocolRequest {
    /// The request type: "PING", "PROFILE_EXCHANGE", "CHAT_MESSAGE"
    pub request: String,
    /// The requester's profile (included so the responder learns who we are)
    #[serde(default)]
    pub profile: Option<PeerProfile>,
    /// Chat message payload (for CHAT_MESSAGE requests)
    #[serde(default)]
    pub message: Option<ChatMessagePayload>,
}

/// A chat message sent between peers.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatMessagePayload {
    /// Unique message ID (UUID)
    pub id: String,
    /// The message text
    pub text: String,
    /// ISO 8601 timestamp
    pub sent_at: String,
}

// ---------------------------------------------------------------------------
// BytescapeProtocol — handles incoming `bytescape/1.0` connections
// ---------------------------------------------------------------------------

/// Protocol handler for the Bytescape custom protocol (`bytescape/1.0`).
///
/// Supports:
/// - `PING`              → responds `PONG`
/// - `PROFILE_REQUEST`   → responds with JSON [`PeerProfile`] (legacy)
/// - `PROFILE_EXCHANGE`  → bidirectional: requester sends their profile,
///                         responder sends theirs back. Both sides update.
#[derive(Clone)]
pub struct BytescapeProtocol {
    my_profile: PeerProfile,
    db_manager: Arc<Mutex<Option<crate::db::DbManager>>>,
    app_handle: tauri::AppHandle,
}

impl std::fmt::Debug for BytescapeProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BytescapeProtocol")
            .field("my_profile", &self.my_profile)
            .finish()
    }
}

impl BytescapeProtocol {
    pub fn new(
        my_profile: PeerProfile,
        db_manager: Arc<Mutex<Option<crate::db::DbManager>>>,
        app_handle: tauri::AppHandle,
    ) -> Self {
        Self {
            my_profile,
            db_manager,
            app_handle,
        }
    }
}

impl ProtocolHandler for BytescapeProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let remote_id = connection.remote_id();
        println!(
            "BytescapeProtocol: Incoming connection from {}",
            remote_id
        );

        let (mut send, mut recv) = connection.accept_bi().await.map_err(to_accept_err)?;

        // Read request (max 8 KiB)
        let request = recv.read_to_end(8192).await.map_err(to_accept_err)?;
        let request_str = String::from_utf8_lossy(&request);

        // Try to parse as JSON ProtocolRequest first (new format),
        // fall back to legacy plaintext commands.
        let (command, remote_profile) = match serde_json::from_slice::<ProtocolRequest>(&request) {
            Ok(req) => (req.request, req.profile),
            Err(_) => (request_str.trim().to_string(), None),
        };

        match command.as_str() {
            "PING" => {
                send.write_all(b"PONG").await.map_err(to_accept_err)?;
                let _ = send.finish();
                println!("BytescapeProtocol: PONG → {}", remote_id);
            }
            "PROFILE_EXCHANGE" | "PROFILE_REQUEST" => {
                // Send our profile back
                let profile_json =
                    serde_json::to_vec(&self.my_profile).map_err(to_accept_err)?;
                send.write_all(&profile_json)
                    .await
                    .map_err(to_accept_err)?;
                let _ = send.finish();

                // Save the remote peer's info
                self.update_peer_from_incoming(
                    &remote_id.to_string(),
                    remote_profile.as_ref(),
                ).await;

                println!(
                    "BytescapeProtocol: Profile exchange with {} (username: {})",
                    remote_id,
                    remote_profile.as_ref().map(|p| p.username.as_str()).unwrap_or("unknown"),
                );
            }
            "CHAT_MESSAGE" => {
                // Receive a chat message from a peer
                if let Some(ref msg_payload) = serde_json::from_slice::<ProtocolRequest>(&request)
                    .ok()
                    .and_then(|r| r.message)
                {
                    // Save the incoming message to our VaultMessage table
                    self.save_incoming_chat_message(
                        &remote_id.to_string(),
                        remote_profile.as_ref(),
                        msg_payload,
                    ).await;

                    send.write_all(b"ACK").await.map_err(to_accept_err)?;
                    let _ = send.finish();

                    println!(
                        "BytescapeProtocol: Chat message from {} saved (id: {})",
                        remote_id, msg_payload.id,
                    );
                } else {
                    send.write_all(b"ERROR: Missing message payload")
                        .await
                        .map_err(to_accept_err)?;
                    let _ = send.finish();
                }
            }
            _ => {
                println!(
                    "BytescapeProtocol: Unknown request from {}: {}",
                    remote_id, command
                );
                send.write_all(b"ERROR: Unknown request")
                    .await
                    .map_err(to_accept_err)?;
                let _ = send.finish();
            }
        }

        Ok(())
    }
}

impl BytescapeProtocol {
    /// Update the peer record in the database from an incoming connection.
    /// If the remote sent their profile, save their username/DID.
    /// Always updates last_seen and emits a frontend event.
    async fn update_peer_from_incoming(
        &self,
        remote_node_id: &str,
        remote_profile: Option<&PeerProfile>,
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let db_guard = self.db_manager.lock().await;
        if let Some(db) = db_guard.as_ref() {
            // Try to find an existing peer record, or create one if we don't have one
            let mut peer = match db.get_peer(remote_node_id).await {
                Ok(Some(p)) => p,
                Ok(None) => {
                    // Peer connected to us but we don't have them saved yet.
                    // Auto-add them so we can show them in the UI.
                    crate::db::peers::PeerInfo {
                        node_id: remote_node_id.to_string(),
                        display_name: None,
                        did: None,
                        added_at: now,
                        last_seen: None,
                        is_following: false,
                    }
                }
                Err(_) => return,
            };

            peer.last_seen = Some(now);

            // Update display name and DID from the remote profile if provided
            if let Some(profile) = remote_profile {
                if !profile.username.is_empty() && profile.username != "Anonymous" {
                    peer.display_name = Some(profile.username.clone());
                }
                if !profile.did.is_empty() && profile.did != "local" {
                    peer.did = Some(profile.did.clone());
                }
            }

            let _ = db.save_peer(&peer).await;
        }
        drop(db_guard);

        // Build the event payload with profile info if available
        let event_payload = if let Some(profile) = remote_profile {
            serde_json::json!({
                "node_id": remote_node_id,
                "username": profile.username,
                "did": profile.did,
                "online": true,
            })
        } else {
            serde_json::json!({
                "node_id": remote_node_id,
                "online": true,
            })
        };

        // Notify frontend that a peer connected to us
        let _ = self.app_handle.emit("peer-connected", event_payload);
    }

    /// Save an incoming chat message from a remote peer as a VaultMessage.
    /// Also emits a `p2p-message` event so the frontend can update in real-time.
    async fn save_incoming_chat_message(
        &self,
        remote_node_id: &str,
        remote_profile: Option<&PeerProfile>,
        msg: &ChatMessagePayload,
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Determine sender info from profile or fallback to node_id
        let sender_handle = remote_profile
            .map(|p| p.username.clone())
            .unwrap_or_else(|| remote_node_id[..12.min(remote_node_id.len())].to_string());
        let sender_did = remote_profile
            .map(|p| p.did.clone())
            .unwrap_or_else(|| format!("p2p:{}", remote_node_id));

        // convo_id uses a deterministic format: "p2p:{remote_node_id}"
        let convo_id = format!("p2p:{}", remote_node_id);

        // Build VaultMessage
        let vault_msg = crate::db::VaultMessage {
            id: msg.id.clone(),
            owner_did: Some(self.my_profile.did.clone()),
            convo_id: convo_id.clone(),
            sender_did: sender_did.clone(),
            sender_handle: sender_handle.clone(),
            text: msg.text.clone(),
            sent_at: msg.sent_at.clone(),
            source: "p2p".to_string(),
            imported_at: now,
            peer_did: Some(format!("p2p:{}", remote_node_id)),
            peer_handle: Some(sender_handle.clone()),
        };

        // Save to DB
        let db_guard = self.db_manager.lock().await;
        if let Some(db) = db_guard.as_ref() {
            let _ = db.save_vault_message(&vault_msg).await;
        }
        drop(db_guard);

        // Also update the peer's last_seen
        self.update_peer_from_incoming(remote_node_id, remote_profile).await;

        // Emit event to frontend for live update
        let _ = self.app_handle.emit("p2p-message", serde_json::json!({
            "convo_id": convo_id,
            "message": {
                "id": msg.id,
                "sender_handle": sender_handle,
                "sender_did": sender_did,
                "text": msg.text,
                "sent_at": msg.sent_at,
                "source": "p2p",
            }
        }));
    }
}

// ---------------------------------------------------------------------------
// DocsHandler — thin ProtocolHandler wrapper for iroh-docs Engine
// ---------------------------------------------------------------------------

/// Forwards incoming iroh-docs connections to `Engine::handle_connection`.
#[derive(Debug, Clone)]
pub struct DocsHandler {
    engine: Arc<DocsEngine>,
}

impl DocsHandler {
    pub fn new(engine: Arc<DocsEngine>) -> Self {
        Self { engine }
    }
}

impl ProtocolHandler for DocsHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        self.engine
            .handle_connection(connection)
            .await
            .map_err(|err| to_accept_err(err))?;
        Ok(())
    }

    async fn shutdown(&self) {
        if let Err(e) = self.engine.shutdown().await {
            eprintln!("DocsHandler shutdown error: {:?}", e);
        }
    }
}
