//! # db/chat.rs — Chat & Messaging Storage
//!
//! Handles all database operations for Bytescape's messaging system:
//!
//! - **Messages** — Individual chat messages between peers (P2P or local),
//!   stored in `MESSAGES_TABLE`, keyed by message ID.
//! - **Conversations** — Metadata about a chat thread (participants, last
//!   message timestamp), stored in `CONVERSATIONS_TABLE`, keyed by convo ID.
//! - **Vault Messages** — Direct messages imported from external platforms
//!   (Bluesky DMs, Mastodon DMs), stored in `VAULT_MESSAGES_TABLE` with
//!   composite key `owner_did:source:message_id`.
//!
//! ## Messages vs. Vault Messages
//!
//! **Messages** are native Bytescape P2P messages — sent between peers using
//! Iroh. They live in `MESSAGES_TABLE`.
//!
//! **Vault Messages** are imported copies of DMs from external platforms.
//! They're read-only (you can't reply through Bytescape) and exist so the
//! user can search and browse their full message history in one place.
//! They live in `VAULT_MESSAGES_TABLE`.
//!
//! ## Multi-DID Support
//!
//! A user can have multiple identities (Bluesky DID, Mastodon acct URI,
//! web DID, did:key, etc.). The `list_vault_messages_multi()` method accepts
//! a list of all known DIDs and matches messages owned by ANY of them.
//!
//! ## Related Files
//! - `models/mod.rs` — `Message`, `Conversation`, `VaultMessage` structs
//! - `db/mod.rs` — Table constants
//! - `commands/chat.rs` — Tauri commands for the chat UI
//! - `sync.rs` — P2P message delivery via Iroh

use super::*;
use redb::ReadableTable;


impl DbManager {
    // ─── P2P Chat Messages ─────────────────────────────────────────────

    /// Saves a chat message to the `messages` table, keyed by `msg.id`.\n    ///
    /// Used for P2P messages sent/received via Iroh. The message must have
    /// a `conversation_id` linking it to a conversation record.
    pub async fn save_message(&self, msg: &Message) -> Result<(), String> {
        let serialized = serde_json::to_string(msg).map_err(|e| e.to_string())?;

        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(MESSAGES_TABLE).map_err(|e| e.to_string())?;
            table.insert(msg.id.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Blocking (non-async) version of `save_message()` for use in synchronous
    /// contexts like Iroh's gossip callback handler, which can't use `.await`.
    pub fn blocking_save_message(&self, msg: &Message) -> Result<(), String> {
        let serialized = serde_json::to_string(msg).map_err(|e| e.to_string())?;

        let db = self.db.blocking_write();
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(MESSAGES_TABLE).map_err(|e| e.to_string())?;
            table.insert(msg.id.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Returns all messages in a conversation, sorted oldest-first (chronological).
    ///
    /// Performs a full table scan with filter (since messages aren't keyed by
    /// conversation ID). Fine for typical chat histories (hundreds of messages).
    pub async fn get_messages(&self, conversation_id: &str) -> Result<Vec<Message>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(MESSAGES_TABLE).map_err(|e| e.to_string())?;
        
        let mut messages = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (_key, value) = item.map_err(|e| e.to_string())?;
            let msg_str = value.value();
            let msg: Message = serde_json::from_str(msg_str).map_err(|e| e.to_string())?;
            
            if msg.conversation_id == conversation_id {
                messages.push(msg);
            }
        }

        // Sort by created_at asc
        messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        Ok(messages)
    }

    // ─── Conversations ─────────────────────────────────────────────────

    /// Saves or updates conversation metadata (participant list, last message, etc.).
    pub async fn save_conversation(&self, convo: &Conversation) -> Result<(), String> {
        let serialized = serde_json::to_string(convo).map_err(|e| e.to_string())?;
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(CONVERSATIONS_TABLE).map_err(|e| e.to_string())?;
            table.insert(convo.id.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Retrieves conversation metadata by ID, or `None` if not found.
    pub async fn get_conversation(&self, id: &str) -> Result<Option<Conversation>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(CONVERSATIONS_TABLE).map_err(|e| e.to_string())?;
        
        match table.get(id).map_err(|e| e.to_string())? {
            Some(v) => {
                let convo: Conversation = serde_json::from_str(v.value()).map_err(|e| e.to_string())?;
                Ok(Some(convo))
            },
            None => Ok(None),
        }
    }

    // ─── Vault Messages (Imported DMs) ──────────────────────────────────

    /// Saves a DM imported from an external platform (Bluesky, Mastodon).
    ///
    /// Returns `true` if the message was new, `false` if it already existed.
    /// If the message exists but metadata has changed (sender handle, peer handle,
    /// or peer DID), it updates the record and returns `false`.
    ///
    /// Key format: `owner_did:source:message_id` — enables per-owner filtering.
    pub async fn save_vault_message(&self, msg: &VaultMessage) -> Result<bool, String> {
        let serialized = serde_json::to_string(msg).map_err(|e| e.to_string())?;
        let owner_key = msg.owner_did.clone().unwrap_or_else(|| "unknown".to_string());
        let key = format!("{}:{}:{}", owner_key, msg.source, msg.id);
        let db = self.db.write().await;

        // Check if exists and if needs update
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(VAULT_MESSAGES_TABLE).map_err(|e| e.to_string())?;
        let existing_val = table.get(key.as_str()).map_err(|e| e.to_string())?;
        
        let mut needs_update = false;
        let mut is_new = true;
        
        if let Some(val) = existing_val {
            is_new = false;
            if let Ok(existing_msg) = serde_json::from_str::<VaultMessage>(val.value()) {
                if existing_msg.sender_handle != msg.sender_handle
                || existing_msg.peer_handle != msg.peer_handle
                || existing_msg.peer_did != msg.peer_did {
                    needs_update = true;
                }
            }
        }
        drop(table);
        drop(read_txn);

        if !is_new && !needs_update {
            return Ok(false);
        }

        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(VAULT_MESSAGES_TABLE).map_err(|e| e.to_string())?;
            table.insert(key.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(is_new)
    }

    /// Lists vault messages for a single owner DID, sorted newest-first.
    /// Convenience wrapper around `list_vault_messages_multi()`.
    pub async fn list_vault_messages(&self, owner_did: Option<&str>) -> Result<Vec<VaultMessage>, String> {
        self.list_vault_messages_multi(owner_did.map(|d| vec![d.to_string()]).unwrap_or_default()).await
    }

    /// List vault messages matching ANY of the given owner DIDs.
    ///
    /// The user's identity can change over time (Bluesky did:plc, web_did,
    /// Mastodon acct: URI, did:key, etc.) and messages stamped under older
    /// DIDs must still be visible. Pass `all_owner_dids()` from the keychain
    /// to get comprehensive results.
    pub async fn list_vault_messages_multi(&self, owner_dids: Vec<String>) -> Result<Vec<VaultMessage>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(VAULT_MESSAGES_TABLE).map_err(|e| e.to_string())?;
        let mut messages = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (_key, value) = item.map_err(|e| e.to_string())?;
            let msg: VaultMessage = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
            if !owner_dids.is_empty() {
                match &msg.owner_did {
                    Some(d) if owner_dids.iter().any(|od| od == d) => {},
                    Some(_) => continue,
                    None => continue,
                }
            }
            messages.push(msg);
        }
        messages.sort_by(|a, b| b.sent_at.cmp(&a.sent_at));
        Ok(messages)
    }

    /// Paginated version of vault message listing with optional conversation filter.
    ///
    /// Returns `(messages_for_page, total_matching_count)`. Supports filtering
    /// by conversation ID to show a single DM thread.
    pub async fn list_vault_messages_paged(
        &self,
        owner_dids: &[String],
        convo_id: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<VaultMessage>, usize), String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(VAULT_MESSAGES_TABLE).map_err(|e| e.to_string())?;
        let mut messages = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (_key, value) = item.map_err(|e| e.to_string())?;
            let msg: VaultMessage = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
            if !owner_dids.is_empty() {
                match &msg.owner_did {
                    Some(d) if owner_dids.iter().any(|od| od == d) => {},
                    Some(_) => continue,
                    None => continue,
                }
            }
            if let Some(cid) = convo_id {
                if msg.convo_id != cid {
                    continue;
                }
            }
            messages.push(msg);
        }
        messages.sort_by(|a, b| b.sent_at.cmp(&a.sent_at));
        let total = messages.len();
        let page: Vec<VaultMessage> = messages.into_iter().skip(offset).take(limit).collect();
        Ok((page, total))
    }

    /// Returns the total number of imported vault messages across all sources.
    pub async fn count_vault_messages(&self) -> Result<usize, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(VAULT_MESSAGES_TABLE).map_err(|e| e.to_string())?;
        Ok(table.len().map_err(|e: redb::StorageError| e.to_string())? as usize)
    }
}
