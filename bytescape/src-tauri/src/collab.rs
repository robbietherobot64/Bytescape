//! Collaborative Editing — Yjs CRDT Storage Layer (C6.1)
//!
//! Manages Yjs documents backed by redb storage. Each document is identified
//! by a UUID and stores the full Yjs binary state. Documents can be linked
//! to BytescapeItem records via `collab_doc_id`.
//!
//! Architecture:
//! - Documents are stored as base64-encoded Yjs binary state in COLLAB_DOCS_TABLE
//! - Each doc has metadata (created_at, updated_at, linked item_id) stored as JSON
//! - The `apply_update` path merges incoming Yrs updates into the persisted state
//! - `snapshot_to_item` extracts plain text from the Yjs XmlFragment and writes
//!   it back to the linked BytescapeItem's `content` field

use serde::{Deserialize, Serialize};
use yrs::{Doc, ReadTxn, Transact, updates::decoder::Decode, updates::encoder::Encode, Update, StateVector};
use base64::Engine;
use redb::{ReadableDatabase, ReadableTable};

use crate::db::DbManager;
use crate::db::ITEMS_TABLE;

/// Metadata for a collaborative document stored alongside the Yjs binary state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollabDocMeta {
    pub id: String,
    /// Unix timestamp (seconds) when the doc was created
    pub created_at: i64,
    /// Unix timestamp (seconds) of the last update
    pub updated_at: i64,
    /// Optional link to a BytescapeItem.id
    pub item_id: Option<String>,
    /// Human-readable title (mirrors the article title)
    pub title: Option<String>,
}

/// Full persisted collab document: metadata + binary Yjs state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollabDocRecord {
    pub meta: CollabDocMeta,
    /// Base64-encoded Yjs document state (from `Doc::encode_state_as_update_v1`)
    pub state_b64: String,
}

impl DbManager {
    // ─── CRDT Document CRUD ───

    /// Create a new empty Yjs document and persist it.
    /// Returns the document ID and initial state vector.
    pub fn create_collab_doc(
        &self,
        item_id: Option<String>,
        title: Option<String>,
    ) -> Result<CollabDocMeta, String> {
        let doc_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        // Create a fresh Yjs document with default XmlFragment
        let doc = Doc::new();
        let state = {
            let txn = doc.transact();
            txn.encode_state_as_update_v1(&StateVector::default())
        };
        let state_b64 = base64::engine::general_purpose::STANDARD.encode(&state);

        let meta = CollabDocMeta {
            id: doc_id.clone(),
            created_at: now,
            updated_at: now,
            item_id,
            title,
        };

        let record = CollabDocRecord {
            meta: meta.clone(),
            state_b64,
        };

        let json = serde_json::to_string(&record).map_err(|e| e.to_string())?;

        // Persist to redb
        let db = self.db.blocking_read();
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn
                .open_table(crate::db::COLLAB_DOCS_TABLE)
                .map_err(|e| e.to_string())?;
            table.insert(doc_id.as_str(), json.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;

        Ok(meta)
    }

    /// Apply a Yjs update (binary, base64-encoded) to an existing document.
    /// Merges the update into the persisted state and returns the new state vector.
    pub fn apply_collab_update(
        &self,
        doc_id: &str,
        update_b64: &str,
    ) -> Result<String, String> {
        // Load existing record
        let mut record = self.get_collab_record(doc_id)?;

        // Decode existing state
        let existing_state = base64::engine::general_purpose::STANDARD
            .decode(&record.state_b64)
            .map_err(|e| format!("Failed to decode existing state: {}", e))?;

        // Decode incoming update
        let update_bytes = base64::engine::general_purpose::STANDARD
            .decode(update_b64)
            .map_err(|e| format!("Failed to decode update: {}", e))?;

        // Create doc, apply existing state, then apply new update
        let doc = Doc::new();
        {
            let mut txn = doc.transact_mut();
            let existing_update = Update::decode_v1(&existing_state)
                .map_err(|e| format!("Failed to decode existing Yjs state: {}", e))?;
            txn.apply_update(existing_update)
                .map_err(|e| format!("Failed to apply existing state: {}", e))?;

            let new_update = Update::decode_v1(&update_bytes)
                .map_err(|e| format!("Failed to decode Yjs update: {}", e))?;
            txn.apply_update(new_update)
                .map_err(|e| format!("Failed to apply Yjs update: {}", e))?;
        }

        // Re-encode full state
        let new_state = {
            let txn = doc.transact();
            txn.encode_state_as_update_v1(&StateVector::default())
        };
        let new_state_b64 = base64::engine::general_purpose::STANDARD.encode(&new_state);

        // Get state vector for return
        let sv = {
            let txn = doc.transact();
            let sv = txn.state_vector();
            base64::engine::general_purpose::STANDARD.encode(sv.encode_v1())
        };

        // Update record
        record.state_b64 = new_state_b64;
        record.meta.updated_at = chrono::Utc::now().timestamp();

        // Persist
        let json = serde_json::to_string(&record).map_err(|e| e.to_string())?;
        let db = self.db.blocking_read();
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn
                .open_table(crate::db::COLLAB_DOCS_TABLE)
                .map_err(|e| e.to_string())?;
            table.insert(doc_id, json.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;

        Ok(sv)
    }

    /// Get the full Yjs state for a document (base64-encoded).
    /// Returns (state_b64, state_vector_b64, metadata).
    pub fn get_collab_state(
        &self,
        doc_id: &str,
    ) -> Result<(String, String, CollabDocMeta), String> {
        let record = self.get_collab_record(doc_id)?;

        // Decode state to compute state vector
        let state_bytes = base64::engine::general_purpose::STANDARD
            .decode(&record.state_b64)
            .map_err(|e| format!("Failed to decode state: {}", e))?;

        let doc = Doc::new();
        {
            let mut txn = doc.transact_mut();
            let update = Update::decode_v1(&state_bytes)
                .map_err(|e| format!("Failed to decode Yjs state: {}", e))?;
            txn.apply_update(update)
                .map_err(|e| format!("Failed to apply state: {}", e))?;
        }

        let sv_b64 = {
            let txn = doc.transact();
            let sv = txn.state_vector();
            base64::engine::general_purpose::STANDARD.encode(sv.encode_v1())
        };

        Ok((record.state_b64, sv_b64, record.meta))
    }

    /// Snapshot content into the linked BytescapeItem.
    /// Accepts the rendered content from the frontend (since Tiptap uses
    /// Y.XmlFragment which cannot be trivially read as plain text in Rust).
    pub fn snapshot_collab_to_item(
        &self,
        doc_id: &str,
        content: &str,
    ) -> Result<String, String> {
        let record = self.get_collab_record(doc_id)?;

        let item_id = record.meta.item_id.as_ref()
            .ok_or_else(|| "Document has no linked item_id".to_string())?
            .clone();

        // Read the linked BytescapeItem directly (blocking, no async)
        let db = self.db.blocking_read();
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;

        let mut item: Option<crate::models::BytescapeItem> = None;
        // Try direct key lookup first
        if let Some(val) = table.get(item_id.as_str()).map_err(|e| e.to_string())? {
            item = Some(serde_json::from_str(val.value()).map_err(|e| e.to_string())?);
        }
        // Fallback: scan values
        if item.is_none() {
            for row in table.iter().map_err(|e| e.to_string())? {
                let (_, value) = row.map_err(|e| e.to_string())?;
                let candidate: crate::models::BytescapeItem = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
                if candidate.id == item_id {
                    item = Some(candidate);
                    break;
                }
            }
        }
        drop(table);
        drop(read_txn);
        drop(db);

        let mut item = item.ok_or_else(|| format!("Linked item {} not found", item_id))?;
        item.content = content.to_string();

        // Persist the updated item (blocking write)
        let serialized = serde_json::to_string(&item).map_err(|e| e.to_string())?;
        let key = if let Some(ref oid) = item.original_id {
            format!("{}:{}", item.source, oid)
        } else {
            item.id.clone()
        };
        let db = self.db.blocking_write();
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut tbl = write_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
            tbl.insert(key.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;

        Ok(item_id)
    }

    /// Delete a collab document.
    pub fn delete_collab_doc(&self, doc_id: &str) -> Result<(), String> {
        let db = self.db.blocking_read();
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn
                .open_table(crate::db::COLLAB_DOCS_TABLE)
                .map_err(|e| e.to_string())?;
            table.remove(doc_id).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// List all collab document metadata.
    pub fn list_collab_docs(&self) -> Result<Vec<CollabDocMeta>, String> {
        let db = self.db.blocking_read();
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn
            .open_table(crate::db::COLLAB_DOCS_TABLE)
            .map_err(|e| e.to_string())?;

        let mut docs = Vec::new();
        for entry in table.iter().map_err(|e| e.to_string())? {
            let (_key, value) = entry.map_err(|e| e.to_string())?;
            if let Ok(record) = serde_json::from_str::<CollabDocRecord>(value.value()) {
                docs.push(record.meta);
            }
        }

        // Sort by updated_at descending
        docs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(docs)
    }

    // ─── Internal Helpers ───

    fn get_collab_record(&self, doc_id: &str) -> Result<CollabDocRecord, String> {
        let db = self.db.blocking_read();
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn
            .open_table(crate::db::COLLAB_DOCS_TABLE)
            .map_err(|e| e.to_string())?;

        let value = table
            .get(doc_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Collab document {} not found", doc_id))?;

        serde_json::from_str::<CollabDocRecord>(value.value())
            .map_err(|e| format!("Failed to deserialize collab doc: {}", e))
    }
}
