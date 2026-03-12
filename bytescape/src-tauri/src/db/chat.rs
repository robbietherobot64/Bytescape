use super::*;
use redb::ReadableTable;


impl DbManager {
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

    /// Blocking version of save_message for use in non-async contexts
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

    pub async fn list_vault_messages(&self, owner_did: Option<&str>) -> Result<Vec<VaultMessage>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(VAULT_MESSAGES_TABLE).map_err(|e| e.to_string())?;
        let mut messages = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (_key, value) = item.map_err(|e| e.to_string())?;
            let msg: VaultMessage = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
            if let Some(owner) = owner_did {
                if msg.owner_did.as_deref() != Some(owner) {
                    continue;
                }
            }
            messages.push(msg);
        }
        messages.sort_by(|a, b| b.sent_at.cmp(&a.sent_at));
        Ok(messages)
    }

    /// Paginated version of list_vault_messages. Returns (messages, total_count).
    pub async fn list_vault_messages_paged(
        &self,
        owner_did: Option<&str>,
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
            if let Some(owner) = owner_did {
                if msg.owner_did.as_deref() != Some(owner) {
                    continue;
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

    pub async fn count_vault_messages(&self) -> Result<usize, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(VAULT_MESSAGES_TABLE).map_err(|e| e.to_string())?;
        Ok(table.len().map_err(|e: redb::StorageError| e.to_string())? as usize)
    }
}
