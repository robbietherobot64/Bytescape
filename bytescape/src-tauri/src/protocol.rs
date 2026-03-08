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

// ---------------------------------------------------------------------------
// BytescapeProtocol — handles incoming `bytescape/1.0` connections
// ---------------------------------------------------------------------------

/// Protocol handler for the Bytescape custom protocol (`bytescape/1.0`).
///
/// Supports:
/// - `PING`             → responds `PONG`
/// - `PROFILE_REQUEST`  → responds with JSON [`PeerProfile`]
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

        // Read request (max 4 KiB)
        let request = recv.read_to_end(4096).await.map_err(to_accept_err)?;
        let request_str = String::from_utf8_lossy(&request);

        match request_str.as_ref() {
            "PING" => {
                send.write_all(b"PONG").await.map_err(to_accept_err)?;
                let _ = send.finish();
                println!("BytescapeProtocol: PONG → {}", remote_id);
            }
            "PROFILE_REQUEST" => {
                let profile_json =
                    serde_json::to_vec(&self.my_profile).map_err(to_accept_err)?;
                send.write_all(&profile_json)
                    .await
                    .map_err(to_accept_err)?;
                let _ = send.finish();

                // Update peer's last_seen in our database
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                let db_guard = self.db_manager.lock().await;
                if let Some(db) = db_guard.as_ref() {
                    let node_id_str = remote_id.to_string();
                    if let Ok(Some(mut peer)) = db.get_peer(&node_id_str).await {
                        peer.last_seen = Some(now);
                        let _ = db.save_peer(&peer).await;
                    }
                }

                // Notify frontend
                let _ = self.app_handle.emit(
                    "peer-connected",
                    serde_json::json!({ "node_id": remote_id.to_string() }),
                );

                println!("BytescapeProtocol: Sent profile to {}", remote_id);
            }
            _ => {
                println!(
                    "BytescapeProtocol: Unknown request from {}: {}",
                    remote_id, request_str
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
