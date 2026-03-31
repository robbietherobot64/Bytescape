//! # traffic.rs — Message Routing Controller
//!
//! Routes outgoing chat messages to the correct transport layer based on the
//! conversation's current mode. Bytescape supports two messaging transports:
//!
//! - **Private (P2P)**: Messages go directly between peers via Iroh/QUIC —
//!   no servers involved, fully encrypted end-to-end.
//! - **Public (Bluesky)**: Messages go through Bluesky's chat API servers —
//!   convenient but server-mediated.
//!
//! The TrafficController checks each conversation's mode and dispatches accordingly.
//! Conversations can switch modes (e.g., from Public to Private once both peers
//! have Bytescape installed), though this negotiation is not yet fully implemented.
//!
//! ## Status: Scaffolding
//! This module is partially implemented. The routing logic works but the Bluesky
//! path has a TODO for JWT handling, and mode-switching negotiation (SystemMessage)
//! is defined but not yet wired up.
//!
//! ## Related Files
//! - `chat.rs` — Bluesky DM client (the "Public" transport)
//! - `protocol.rs` — P2P protocol handler (the "Private" transport)
//! - `db/chat.rs` — Message and conversation persistence
//! - `db.rs` — `Conversation`, `ConversationMode`, `TransportLayer` types

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::db::{DbManager, Message, Conversation, ConversationMode, TransportLayer};
use crate::chat::ChatClient;
// Use explicit path to avoid conflict with iroh_docs::sync or core::sync
pub type SyncManager = crate::sync::SyncManager;

// ─── Mode Switching Protocol (Scaffold) ───

/// Types of system messages used for transport mode negotiation between peers.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub enum SystemMessageType {
    ModeSwitchRequest,
    ModeSwitchAccept,
    ModeSwitchReject,
}

/// A system message for negotiating conversation mode switches.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct SystemMessage {
    pub msg_type: SystemMessageType,
    pub payload: Option<String>,
}

// ─── Traffic Controller ───

/// Routes outgoing messages to the correct transport (P2P or Bluesky).
pub struct TrafficController {
    db: Arc<tokio::sync::RwLock<Option<DbManager>>>,
    #[allow(dead_code)]
    chat: Arc<ChatClient>,
}

impl TrafficController {
    /// Creates a new TrafficController with database and chat client references.
    pub fn new(db: Arc<tokio::sync::RwLock<Option<DbManager>>>, chat: Arc<ChatClient>) -> Self {
        Self { db, chat }
    }

    // ─── Message Routing ───

    /// Routes an outgoing message to the correct transport based on conversation mode.
    pub async fn route_message(
        &self,
        peer_did: &str,
        content: String,
        user_did: &str,
        sync_manager: Option<&SyncManager>,
    ) -> Result<Message, String> {
        let convo = self.get_or_create_convo(peer_did).await?;
        
        match convo.current_mode {
            ConversationMode::Private => {
                if let Some(_sync) = sync_manager {
                    let msg = self.create_local_message(peer_did, content, user_did, TransportLayer::Iroh).await?;
                    // TODO: SyncManager will pick this up or we trigger it
                    Ok(msg)
                } else {
                    Err("P2P mode active but Iroh node not initialized.".to_string())
                }
            }
            ConversationMode::Public => {
                let _access_jwt = "TODO".to_string(); // Need to pass JWT or handle it
                // This will be handled in lib.rs for now by calling ChatClient directly
                // but using TrafficController for the decision.
                let msg = self.create_local_message(peer_did, content, user_did, TransportLayer::Bluesky).await?;
                Ok(msg)
            }
            ConversationMode::PendingPublic => {
                Err("Conversation is pending public mode approval.".to_string())
            }
        }
    }

    // ─── Conversation Management ───

    /// Gets an existing conversation or creates one with the default transport mode.
    pub async fn get_or_create_convo(&self, peer_did: &str) -> Result<Conversation, String> {
        let db_guard = self.db.read().await;
        let db = db_guard.as_ref().ok_or("Database not initialized")?;

        if let Some(convo) = db.get_conversation(peer_did).await? {
            Ok(convo)
        } else {
            let contact = db.get_contact(peer_did).await?;
            let mode = if let Some(c) = contact {
                if c.p2p_capable { ConversationMode::Private } else { ConversationMode::Public }
            } else {
                ConversationMode::Public
            };
            
            let convo = Conversation {
                id: peer_did.to_string(),
                peer_did: peer_did.to_string(),
                current_mode: mode,
                last_message_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
            };
            db.save_conversation(&convo).await?;
            Ok(convo)
        }
    }

    // ─── Message Persistence ───

    /// Creates and persists a message record in the local database.
    pub async fn create_local_message(&self, peer_did: &str, content: String, sender_did: &str, transport: TransportLayer) -> Result<Message, String> {
        let msg = Message {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: peer_did.to_string(),
            sender_did: sender_did.to_string(),
            content,
            created_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
            transport_layer: transport,
            synced_with_bsky: transport == TransportLayer::Bluesky,
        };
        let db_guard = self.db.read().await;
        let db = db_guard.as_ref().ok_or("Database not initialized")?;
        db.save_message(&msg).await?;
        Ok(msg)
    }
}
