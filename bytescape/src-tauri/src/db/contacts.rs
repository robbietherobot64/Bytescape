//! # db/contacts.rs — Contact / Friend Storage
//!
//! Stores the user's contacts (friends, followed accounts) in the `contacts`
//! redb table. Each contact is keyed by their DID (decentralized identifier).
//!
//! Contacts may come from:
//! - Bluesky follows (imported during sync)
//! - P2P peers the user has befriended
//! - Manually added contacts
//!
//! ## Related Files
//! - `models/mod.rs` — The `Contact` struct definition
//! - `db/mod.rs` — `CONTACTS_TABLE` constant
//! - `commands/contacts.rs` — Tauri commands for the contacts UI

use super::*;
use redb::ReadableTable;


impl DbManager {
    /// Saves a contact record, keyed by `contact.did`.\n    ///
    /// If a contact with this DID already exists, it's overwritten (upsert).
    pub async fn save_contact(&self, contact: &Contact) -> Result<(), String> {
        let serialized = serde_json::to_string(contact).map_err(|e| e.to_string())?;
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(CONTACTS_TABLE).map_err(|e| e.to_string())?;
            table.insert(contact.did.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Retrieves a contact by DID, or `None` if not found. O(1) key lookup.
    pub async fn get_contact(&self, did: &str) -> Result<Option<Contact>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(CONTACTS_TABLE).map_err(|e| e.to_string())?;
        
        match table.get(did).map_err(|e| e.to_string())? {
            Some(v) => {
                let contact: Contact = serde_json::from_str(v.value()).map_err(|e| e.to_string())?;
                Ok(Some(contact))
            },
            None => Ok(None),
        }
    }

    /// Returns all contacts in the database (unordered).
    pub async fn list_contacts(&self) -> Result<Vec<Contact>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(CONTACTS_TABLE).map_err(|e| e.to_string())?;
        
        let mut contacts = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (_key, value) = item.map_err(|e| e.to_string())?;
            let contact: Contact = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
            contacts.push(contact);
        }
        Ok(contacts)
    }
}
