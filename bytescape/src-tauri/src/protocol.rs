//! Bytescape P2P Protocol Handlers
//!
//! Implements [`ProtocolHandler`] for the custom Bytescape protocol
//! and a thin wrapper for forwarding iroh-docs connections.

use std::sync::Arc;

use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh_docs::engine::Engine as DocsEngine;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
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
    /// Linked identities (e.g., Bluesky did:plc, AT Protocol handle)
    #[serde(default)]
    pub also_known_as: Vec<String>,
}

/// A request envelope sent over the bytescape/1.0 protocol.
/// The requester includes their own profile so both sides learn about each other.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProtocolRequest {
    /// The request type: "PING", "PROFILE_EXCHANGE", "CHAT_MESSAGE", "FEED_REQUEST"
    pub request: String,
    /// The requester's profile (included so the responder learns who we are)
    #[serde(default)]
    pub profile: Option<PeerProfile>,
    /// Chat message payload (for CHAT_MESSAGE requests)
    #[serde(default)]
    pub message: Option<ChatMessagePayload>,
    /// Feed request parameters (for FEED_REQUEST)
    #[serde(default)]
    pub feed_params: Option<FeedRequestParams>,
}

/// Parameters for a FEED_REQUEST — ask a peer for their posts.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FeedRequestParams {
    /// Maximum number of items to return
    #[serde(default = "default_feed_limit")]
    pub limit: usize,
    /// Offset for pagination
    #[serde(default)]
    pub offset: usize,
    /// Optional source filter (e.g., "bluesky", "facebook")
    #[serde(default)]
    pub source_filter: Option<String>,
}

fn default_feed_limit() -> usize { 20 }

/// A peer's feed response — their profile + posts.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FeedResponse {
    pub profile: PeerProfile,
    pub items: Vec<crate::models::BytescapeItem>,
    pub total_count: usize,
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
        println!("[P2P-PROTO] Incoming connection from {}", remote_id);

        let (mut send, mut recv) = connection.accept_bi().await.map_err(to_accept_err)?;

        // Read request (max 64 KiB for feed params etc.)
        let request_bytes = recv.read_to_end(65536).await.map_err(to_accept_err)?;
        println!("[P2P-PROTO] Received {} bytes from {}", request_bytes.len(), remote_id);

        // Single parse — extract all fields at once.
        // Fall back to legacy plaintext commands.
        let (command, remote_profile, chat_message, feed_params) = match serde_json::from_slice::<ProtocolRequest>(&request_bytes) {
            Ok(req) => {
                println!("[P2P-PROTO] Parsed: command={}, has_profile={}, has_message={}, has_feed_params={}",
                    req.request, req.profile.is_some(), req.message.is_some(), req.feed_params.is_some());
                (req.request, req.profile, req.message, req.feed_params)
            }
            Err(_) => {
                let raw = String::from_utf8_lossy(&request_bytes).trim().to_string();
                println!("[P2P-PROTO] Legacy plaintext command: {}", raw);
                (raw, None, None, None)
            }
        };

        // Get fresh profile from keychain (DID may have changed since node init)
        let fresh_profile = self.get_fresh_profile();

        match command.as_str() {
            "PING" => {
                send.write_all(b"PONG").await.map_err(to_accept_err)?;
                let _ = send.finish();
                println!("[P2P-PROTO] PONG → {}", remote_id);
            }
            "PROFILE_EXCHANGE" | "PROFILE_REQUEST" => {
                // Send our fresh profile back
                let profile_json =
                    serde_json::to_vec(&fresh_profile).map_err(to_accept_err)?;
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
                    "[P2P-PROTO] Profile exchange with {} (username: {})",
                    remote_id,
                    remote_profile.as_ref().map(|p| p.username.as_str()).unwrap_or("unknown"),
                );
            }
            "CHAT_MESSAGE" => {
                // Use already-parsed chat_message (no double-parse)
                if let Some(ref msg_payload) = chat_message {
                    println!(
                        "[P2P-PROTO] Chat message from {}: id={}, text_len={}",
                        remote_id, msg_payload.id, msg_payload.text.len()
                    );

                    // Save the incoming message to our VaultMessage table
                    self.save_incoming_chat_message(
                        &remote_id.to_string(),
                        remote_profile.as_ref(),
                        msg_payload,
                    ).await;

                    send.write_all(b"ACK").await.map_err(to_accept_err)?;
                    let _ = send.finish();
                    println!("[P2P-PROTO] ACK sent to {} (id: {})", remote_id, msg_payload.id);
                } else {
                    println!("[P2P-PROTO] CHAT_MESSAGE from {} missing payload!", remote_id);
                    send.write_all(b"ERROR: Missing message payload")
                        .await
                        .map_err(to_accept_err)?;
                    let _ = send.finish();
                }
            }
            "FEED_REQUEST" => {
                let params = feed_params.unwrap_or(FeedRequestParams {
                    limit: 20,
                    offset: 0,
                    source_filter: None,
                });

                println!(
                    "[P2P-PROTO] Feed request from {}: limit={}, offset={}, source={:?}",
                    remote_id, params.limit, params.offset, params.source_filter
                );

                // Update peer info from requester first (before response)
                self.update_peer_from_incoming(
                    &remote_id.to_string(),
                    remote_profile.as_ref(),
                ).await;

                // Serve our posts from the vault
                let feed_response = self.serve_feed(&fresh_profile, &params).await;
                match serde_json::to_vec(&feed_response) {
                    Ok(response_bytes) => {
                        let response_len = response_bytes.len();
                        println!(
                            "[P2P-PROTO] Sending feed to {}: {} items (total: {}), {} bytes",
                            remote_id, feed_response.items.len(), feed_response.total_count, response_len
                        );
                        match send.write_all(&response_bytes).await {
                            Ok(()) => {
                                let _ = send.finish();
                                println!("[P2P-PROTO] Feed sent successfully to {}", remote_id);
                            }
                            Err(e) => {
                                println!("[P2P-PROTO] Feed write_all failed for {}: {}", remote_id, e);
                                let _ = send.finish();
                            }
                        }
                    }
                    Err(e) => {
                        println!("[P2P-PROTO] Feed serialize failed: {}", e);
                        let _ = send.write_all(b"ERROR: Internal serialization error").await;
                        let _ = send.finish();
                    }
                }

                // Brief sleep to let QUIC flush the response data before
                // the Connection handle is dropped (which sends CONNECTION_CLOSE).
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
            _ => {
                println!("[P2P-PROTO] Unknown request from {}: {}", remote_id, command);
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
    /// Get a fresh profile with the live DID from the keychain.
    ///
    /// The protocol's `my_profile` is frozen at node init time, which may
    /// be before the user authenticates. This reads the current DID from
    /// the keychain so profile exchanges always return the real identity.
    fn get_fresh_profile(&self) -> PeerProfile {
        let app_state = self.app_handle.state::<crate::AppState>();
        let live_did = app_state.keychain.get_owner_did()
            .unwrap_or(None)
            .unwrap_or_else(|| self.my_profile.did.clone());
        let live_username = app_state.keychain.get_display_name();

        // Build alsoKnownAs: include Bluesky DID and handle if available
        let mut also_known_as = Vec::new();
        if let Ok(Some(bsky_did)) = app_state.keychain.get_bluesky_did() {
            also_known_as.push(bsky_did);
        }
        if let Ok(Some(handle)) = app_state.keychain.get_handle() {
            also_known_as.push(format!("at://{}", handle));
        }

        PeerProfile {
            node_id: self.my_profile.node_id.clone(),
            username: live_username,
            did: live_did,
            also_known_as,
        }
    }

    /// Get the live owner DID from the keychain.
    ///
    /// Falls back to the protocol's cached DID if the keychain is unavailable.
    fn get_live_owner_did(&self) -> String {
        let app_state = self.app_handle.state::<crate::AppState>();
        app_state.keychain.get_owner_did()
            .unwrap_or(None)
            .unwrap_or_else(|| self.my_profile.did.clone())
    }

    /// Serve our vault feed to a requesting peer.
    ///
    /// Returns our profile + a page of BytescapeItems from the local vault.
    /// Strips sensitive fields (media_paths with local filesystem paths) before serving.
    async fn serve_feed(&self, my_profile: &PeerProfile, params: &FeedRequestParams) -> FeedResponse {
        let owner_did = self.get_live_owner_did();
        let db_guard = self.db_manager.lock().await;

        let (items, total_count) = if let Some(db) = db_guard.as_ref() {
            match db.list_my_items_paged(
                &owner_did,
                params.source_filter.as_deref(),
                params.limit.min(50), // Cap at 50 per request
                params.offset,
            ).await {
                Ok((mut items, total)) => {
                    // Sanitize media for P2P sharing:
                    // - media_blobs contain local vault blob hashes that peers can't access
                    // - media_paths may contain local filesystem paths
                    // Keep only HTTP/HTTPS URLs that peers can actually load (e.g. CDN URLs).
                    // preview_img (often a CDN URL for link cards) is preserved as-is.
                    for item in &mut items {
                        // Keep only HTTP URLs in media_paths
                        item.media_paths.retain(|p| {
                            p.starts_with("http://") || p.starts_with("https://")
                        });
                        // Clear blob refs — peers don't have our local blob store
                        item.media_blobs.clear();
                        item.thumbnail_blobs.clear();
                    }
                    (items, total)
                }
                Err(e) => {
                    println!("[P2P-PROTO] Error reading feed for serve: {}", e);
                    (vec![], 0)
                }
            }
        } else {
            (vec![], 0)
        };
        drop(db_guard);

        FeedResponse {
            profile: my_profile.clone(),
            items,
            total_count,
        }
    }

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

        // Use the LIVE owner DID from the keychain — not the stale protocol profile.
        // The protocol's my_profile.did is frozen at node init time (before auth),
        // so it would be "local" for Bluesky users, causing messages to be invisible
        // when list_vault_messages filters by the real DID.
        let owner_did = self.get_live_owner_did();
        println!("[P2P-CHAT] Saving incoming message: owner_did={}, from={}", owner_did, remote_node_id);

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
            owner_did: Some(owner_did.clone()),
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
            match db.save_vault_message(&vault_msg).await {
                Ok(is_new) => println!("[P2P-CHAT] Message saved: id={}, is_new={}, convo_id={}", msg.id, is_new, convo_id),
                Err(e) => println!("[P2P-CHAT] ERROR saving message: {}", e),
            }
        } else {
            println!("[P2P-CHAT] ERROR: Database not initialized, message lost!");
        }
        drop(db_guard);

        // Also update the peer's last_seen
        self.update_peer_from_incoming(remote_node_id, remote_profile).await;

        // Emit event to frontend for live update
        let emit_result = self.app_handle.emit("p2p-message", serde_json::json!({
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
        println!("[P2P-CHAT] Event emitted: p2p-message, convo_id={}, emit_ok={}", convo_id, emit_result.is_ok());
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
