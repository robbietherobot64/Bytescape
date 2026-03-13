use redb::{Database, TableDefinition, ReadableTable, ReadableDatabase, ReadableTableMetadata};

use std::sync::Arc;
use tokio::sync::RwLock;
use std::path::PathBuf;

pub(crate) const POSTS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("posts");
pub(crate) const MESSAGES_TABLE: TableDefinition<&str, &str> = TableDefinition::new("messages");
pub(crate) const CONTACTS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("contacts");
pub(crate) const CONVERSATIONS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("conversations");
pub(crate) const ITEMS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("bytescape_items");
pub(crate) const VAULT_MESSAGES_TABLE: TableDefinition<&str, &str> = TableDefinition::new("vault_messages");
pub(crate) const FOLLOWED_HASHTAGS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("followed_hashtags");
pub(crate) const WILLOW_WRITE_PARITY_TABLE: TableDefinition<&str, &str> = TableDefinition::new("willow_write_parity");
pub(crate) const PEERS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("peers");
pub(crate) const COLLAB_DOCS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("collab_docs");

pub use crate::models::*;

pub mod contacts;
pub mod chat;
pub mod vault;
pub mod peers;

#[derive(Clone)]
pub struct DbManager {
    pub(crate) db: Arc<RwLock<Database>>,
}

impl DbManager {
    pub fn new(path: PathBuf) -> Result<Self, String> {
        let db = Database::create(path).map_err(|e| e.to_string())?;
        
        // Ensure tables exist
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let _ = write_txn.open_table(POSTS_TABLE).map_err(|e| e.to_string())?;
            let _ = write_txn.open_table(MESSAGES_TABLE).map_err(|e| e.to_string())?;
            let _ = write_txn.open_table(CONTACTS_TABLE).map_err(|e| e.to_string())?;
            let _ = write_txn.open_table(CONVERSATIONS_TABLE).map_err(|e| e.to_string())?;
            let _ = write_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
            let _ = write_txn.open_table(VAULT_MESSAGES_TABLE).map_err(|e| e.to_string())?;
            let _ = write_txn.open_table(FOLLOWED_HASHTAGS_TABLE).map_err(|e| e.to_string())?;
            let _ = write_txn.open_table(WILLOW_WRITE_PARITY_TABLE).map_err(|e| e.to_string())?;
            let _ = write_txn.open_table(PEERS_TABLE).map_err(|e| e.to_string())?;
            let _ = write_txn.open_table(COLLAB_DOCS_TABLE).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;

        Ok(Self {
            db: Arc::new(RwLock::new(db)),
        })
    }

    pub async fn create_post(&self, content: String, author_did: String) -> Result<Post, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let post = Post {
            id: id.clone(),
            content,
            author_did,
            created_at,
        };

        let serialized = serde_json::to_string(&post).map_err(|e| e.to_string())?;
        
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(POSTS_TABLE).map_err(|e| e.to_string())?;
            table.insert(id.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;

        Ok(post)
    }

    pub async fn list_posts(&self) -> Result<Vec<Post>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(POSTS_TABLE).map_err(|e| e.to_string())?;
        
        let mut posts = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (_key, value) = item.map_err(|e| e.to_string())?;
            let post_str = value.value();
            let post: Post = serde_json::from_str(post_str).map_err(|e| e.to_string())?;
            posts.push(post);
        }

        // Sort by created_at desc
        posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(posts)
    }

    pub async fn clear_items(&self) -> Result<usize, String> {
        let db = self.db.write().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
        let count = table.len().map_err(|e: redb::StorageError| e.to_string())? as usize;
        drop(table);
        drop(read_txn);

        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            // Delete and recreate the table
            write_txn.delete_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(count)
    }

    pub async fn clear_all_user_data(&self) -> Result<usize, String> {
        let db = self.db.write().await;

        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let posts = read_txn.open_table(POSTS_TABLE).map_err(|e| e.to_string())?
            .len().map_err(|e: redb::StorageError| e.to_string())? as usize;
        let messages = read_txn.open_table(MESSAGES_TABLE).map_err(|e| e.to_string())?
            .len().map_err(|e: redb::StorageError| e.to_string())? as usize;
        let contacts = read_txn.open_table(CONTACTS_TABLE).map_err(|e| e.to_string())?
            .len().map_err(|e: redb::StorageError| e.to_string())? as usize;
        let convos = read_txn.open_table(CONVERSATIONS_TABLE).map_err(|e| e.to_string())?
            .len().map_err(|e: redb::StorageError| e.to_string())? as usize;
        let items = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?
            .len().map_err(|e: redb::StorageError| e.to_string())? as usize;
        let vault_messages = read_txn.open_table(VAULT_MESSAGES_TABLE).map_err(|e| e.to_string())?
            .len().map_err(|e: redb::StorageError| e.to_string())? as usize;
        let hashtags = read_txn.open_table(FOLLOWED_HASHTAGS_TABLE).map_err(|e| e.to_string())?
            .len().map_err(|e: redb::StorageError| e.to_string())? as usize;
        let parity = read_txn.open_table(WILLOW_WRITE_PARITY_TABLE).map_err(|e| e.to_string())?
            .len().map_err(|e: redb::StorageError| e.to_string())? as usize;
        let peers = read_txn.open_table(PEERS_TABLE).map_err(|e| e.to_string())?
            .len().map_err(|e: redb::StorageError| e.to_string())? as usize;
        drop(read_txn);

        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            write_txn.delete_table(POSTS_TABLE).map_err(|e| e.to_string())?;
            write_txn.delete_table(MESSAGES_TABLE).map_err(|e| e.to_string())?;
            write_txn.delete_table(CONTACTS_TABLE).map_err(|e| e.to_string())?;
            write_txn.delete_table(CONVERSATIONS_TABLE).map_err(|e| e.to_string())?;
            write_txn.delete_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
            write_txn.delete_table(VAULT_MESSAGES_TABLE).map_err(|e| e.to_string())?;
            write_txn.delete_table(FOLLOWED_HASHTAGS_TABLE).map_err(|e| e.to_string())?;
            write_txn.delete_table(WILLOW_WRITE_PARITY_TABLE).map_err(|e| e.to_string())?;
            write_txn.delete_table(PEERS_TABLE).map_err(|e| e.to_string())?;

            write_txn.open_table(POSTS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(MESSAGES_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(CONTACTS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(CONVERSATIONS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(VAULT_MESSAGES_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(FOLLOWED_HASHTAGS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(WILLOW_WRITE_PARITY_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(PEERS_TABLE).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;

        Ok(posts + messages + contacts + convos + items + vault_messages + hashtags + parity + peers)
    }

    pub async fn count_archive_posts(&self) -> Result<u64, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(POSTS_TABLE).map_err(|e| e.to_string())?;
        Ok(table.len().unwrap_or(0))
    }

    pub async fn save_hashtag(&self, tag: String) -> Result<(), String> {
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(FOLLOWED_HASHTAGS_TABLE).map_err(|e| e.to_string())?;
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
            table.insert(tag.as_str(), now.to_string().as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn delete_hashtag(&self, tag: String) -> Result<(), String> {
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(FOLLOWED_HASHTAGS_TABLE).map_err(|e| e.to_string())?;
            table.remove(tag.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn list_followed_hashtags(&self) -> Result<Vec<String>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(FOLLOWED_HASHTAGS_TABLE).map_err(|e| e.to_string())?;
        
        let mut tags = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            if let Ok((k, _)) = item {
                tags.push(k.value().to_string());
            }
        }
        Ok(tags)
    }
}
