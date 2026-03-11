use super::*;
use redb::ReadableTable;


impl DbManager {
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
