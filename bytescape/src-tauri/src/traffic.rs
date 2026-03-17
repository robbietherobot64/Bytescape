use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::db::{DbManager, Message, Conversation, ConversationMode, TransportLayer};
use crate::chat::ChatClient;
// Use explicit path to avoid conflict with iroh_docs::sync or core::sync
pub type SyncManager = crate::sync::SyncManager;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub enum SystemMessageType {
    ModeSwitchRequest,
    ModeSwitchAccept,
    ModeSwitchReject,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct SystemMessage {
    pub msg_type: SystemMessageType,
    pub payload: Option<String>,
}


pub struct TrafficController {
    db: Arc<tokio::sync::RwLock<Option<DbManager>>>,
    #[allow(dead_code)]
    chat: Arc<ChatClient>,
}

impl TrafficController {
    pub fn new(db: Arc<tokio::sync::RwLock<Option<DbManager>>>, chat: Arc<ChatClient>) -> Self {
        Self { db, chat }
    }

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
