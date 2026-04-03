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
pub(crate) const PEERS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("peers");
pub(crate) const COLLAB_DOCS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("collab_docs");
pub(crate) const AP_ACTORS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("ap_actors");
pub(crate) const PROFILE_LAYOUT_TABLE: TableDefinition<&str, &str> = TableDefinition::new("profile_layout");
pub(crate) const GUESTBOOK_TABLE: TableDefinition<&str, &str> = TableDefinition::new("guestbook");

// ─── Groups & Topics (Sprint v12) ─────────────────────────────────
pub(crate) const TOPICS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("topics");
pub(crate) const TOPIC_SUBSCRIPTIONS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("topic_subscriptions");
pub(crate) const GROUPS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("groups");
pub(crate) const GROUP_MEMBERS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("group_members");
pub(crate) const GROUP_POSTS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("group_posts");

pub use crate::models::*;

pub mod contacts;
pub mod chat;
pub mod vault;
pub mod peers;
pub mod groups;

#[derive(Clone)]
pub struct DbManager {
    pub(crate) db: Arc<RwLock<Database>>,
    /// Cached vault item headers — avoids full table scan + JSON deserialization
    /// on every feed load. Invalidated on any write (save_item, delete, etc.).
    pub(crate) header_cache: Arc<RwLock<Option<Vec<BytescapeItemHeader>>>>,
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
            let _ = write_txn.open_table(PEERS_TABLE).map_err(|e| e.to_string())?;
            let _ = write_txn.open_table(COLLAB_DOCS_TABLE).map_err(|e| e.to_string())?;
            let _ = write_txn.open_table(AP_ACTORS_TABLE).map_err(|e| e.to_string())?;
            let _ = write_txn.open_table(PROFILE_LAYOUT_TABLE).map_err(|e| e.to_string())?;
            let _ = write_txn.open_table(GUESTBOOK_TABLE).map_err(|e| e.to_string())?;
            let _ = write_txn.open_table(TOPICS_TABLE).map_err(|e| e.to_string())?;
            let _ = write_txn.open_table(TOPIC_SUBSCRIPTIONS_TABLE).map_err(|e| e.to_string())?;
            let _ = write_txn.open_table(GROUPS_TABLE).map_err(|e| e.to_string())?;
            let _ = write_txn.open_table(GROUP_MEMBERS_TABLE).map_err(|e| e.to_string())?;
            let _ = write_txn.open_table(GROUP_POSTS_TABLE).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;

        Ok(Self {
            db: Arc::new(RwLock::new(db)),
            header_cache: Arc::new(RwLock::new(None)),
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
        let peers = read_txn.open_table(PEERS_TABLE).map_err(|e| e.to_string())?
            .len().map_err(|e: redb::StorageError| e.to_string())? as usize;
        let collab_docs = read_txn.open_table(COLLAB_DOCS_TABLE).map_err(|e| e.to_string())?
            .len().map_err(|e: redb::StorageError| e.to_string())? as usize;
        let ap_actors = read_txn.open_table(AP_ACTORS_TABLE).map_err(|e| e.to_string())?
            .len().map_err(|e: redb::StorageError| e.to_string())? as usize;
        let profile_layouts = read_txn.open_table(PROFILE_LAYOUT_TABLE).map_err(|e| e.to_string())?
            .len().map_err(|e: redb::StorageError| e.to_string())? as usize;
        let guestbook = read_txn.open_table(GUESTBOOK_TABLE).map_err(|e| e.to_string())?
            .len().map_err(|e: redb::StorageError| e.to_string())? as usize;
        let topics = read_txn.open_table(TOPICS_TABLE).map_err(|e| e.to_string())?
            .len().map_err(|e: redb::StorageError| e.to_string())? as usize;
        let topic_subs = read_txn.open_table(TOPIC_SUBSCRIPTIONS_TABLE).map_err(|e| e.to_string())?
            .len().map_err(|e: redb::StorageError| e.to_string())? as usize;
        let groups = read_txn.open_table(GROUPS_TABLE).map_err(|e| e.to_string())?
            .len().map_err(|e: redb::StorageError| e.to_string())? as usize;
        let group_members = read_txn.open_table(GROUP_MEMBERS_TABLE).map_err(|e| e.to_string())?
            .len().map_err(|e: redb::StorageError| e.to_string())? as usize;
        let group_posts = read_txn.open_table(GROUP_POSTS_TABLE).map_err(|e| e.to_string())?
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
            write_txn.delete_table(PEERS_TABLE).map_err(|e| e.to_string())?;
            write_txn.delete_table(COLLAB_DOCS_TABLE).map_err(|e| e.to_string())?;
            write_txn.delete_table(AP_ACTORS_TABLE).map_err(|e| e.to_string())?;
            write_txn.delete_table(PROFILE_LAYOUT_TABLE).map_err(|e| e.to_string())?;
            write_txn.delete_table(GUESTBOOK_TABLE).map_err(|e| e.to_string())?;
            write_txn.delete_table(TOPICS_TABLE).map_err(|e| e.to_string())?;
            write_txn.delete_table(TOPIC_SUBSCRIPTIONS_TABLE).map_err(|e| e.to_string())?;
            write_txn.delete_table(GROUPS_TABLE).map_err(|e| e.to_string())?;
            write_txn.delete_table(GROUP_MEMBERS_TABLE).map_err(|e| e.to_string())?;
            write_txn.delete_table(GROUP_POSTS_TABLE).map_err(|e| e.to_string())?;

            write_txn.open_table(POSTS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(MESSAGES_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(CONTACTS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(CONVERSATIONS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(VAULT_MESSAGES_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(FOLLOWED_HASHTAGS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(PEERS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(COLLAB_DOCS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(AP_ACTORS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(PROFILE_LAYOUT_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(GUESTBOOK_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(TOPICS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(TOPIC_SUBSCRIPTIONS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(GROUPS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(GROUP_MEMBERS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(GROUP_POSTS_TABLE).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;

        Ok(posts + messages + contacts + convos + items + vault_messages + hashtags + peers + collab_docs + ap_actors + profile_layouts + guestbook + topics + topic_subs + groups + group_members + group_posts)
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


    // ─── AP Actor CRUD ───

    pub async fn save_ap_actor(
        &self,
        actor: &crate::activitypub::ApActor,
    ) -> Result<(), String> {
        let serialized = serde_json::to_string(actor).map_err(|e| e.to_string())?;
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(AP_ACTORS_TABLE).map_err(|e| e.to_string())?;
            table
                .insert(actor.owner_did.as_str(), serialized.as_str())
                .map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    // ─── Profile Layout ───

    pub async fn save_profile_layout(
        &self,
        layout: &crate::commands::profile::ProfileLayout,
    ) -> Result<(), String> {
        let serialized = serde_json::to_string(layout).map_err(|e| e.to_string())?;
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn
                .open_table(PROFILE_LAYOUT_TABLE)
                .map_err(|e| e.to_string())?;
            table
                .insert("user_profile_layout", serialized.as_str())
                .map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn get_ap_actor(
        &self,
        owner_did: &str,
    ) -> Result<Option<crate::activitypub::ApActor>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(AP_ACTORS_TABLE).map_err(|e| e.to_string())?;
        match table.get(owner_did) {
            Ok(Some(value)) => {
                let actor: crate::activitypub::ApActor =
                    serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
                Ok(Some(actor))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub async fn get_profile_layout(
        &self,
    ) -> Result<Option<crate::commands::profile::ProfileLayout>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn
            .open_table(PROFILE_LAYOUT_TABLE)
            .map_err(|e| e.to_string())?;
        match table.get("user_profile_layout") {
            Ok(Some(value)) => {
                let layout: crate::commands::profile::ProfileLayout =
                    serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
                Ok(Some(layout))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub async fn delete_ap_actor(&self, owner_did: &str) -> Result<(), String> {
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(AP_ACTORS_TABLE).map_err(|e| e.to_string())?;
            let _ = table.remove(owner_did);
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn delete_profile_layout(&self) -> Result<(), String> {
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn
                .open_table(PROFILE_LAYOUT_TABLE)
                .map_err(|e| e.to_string())?;
            let _ = table.remove("user_profile_layout");
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    // ─── Profile Sections ───

    pub async fn save_profile_sections(
        &self,
        sections: &[crate::commands::profile::ProfileSection],
    ) -> Result<(), String> {
        let serialized = serde_json::to_string(sections).map_err(|e| e.to_string())?;
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn
                .open_table(PROFILE_LAYOUT_TABLE)
                .map_err(|e| e.to_string())?;
            table
                .insert("user_profile_sections", serialized.as_str())
                .map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn get_profile_sections(
        &self,
    ) -> Result<Vec<crate::commands::profile::ProfileSection>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn
            .open_table(PROFILE_LAYOUT_TABLE)
            .map_err(|e| e.to_string())?;
        match table.get("user_profile_sections") {
            Ok(Some(value)) => {
                let mut sections: Vec<crate::commands::profile::ProfileSection> =
                    serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
                sections.sort_by_key(|s| s.order);
                Ok(sections)
            }
            Ok(None) => Ok(Vec::new()),
            Err(e) => Err(e.to_string()),
        }
    }

    // ─── Guestbook ───

    pub async fn add_guestbook_entry(
        &self,
        entry: &crate::commands::profile::GuestbookEntry,
    ) -> Result<(), String> {
        let serialized = serde_json::to_string(entry).map_err(|e| e.to_string())?;
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn
                .open_table(GUESTBOOK_TABLE)
                .map_err(|e| e.to_string())?;
            table
                .insert(entry.id.as_str(), serialized.as_str())
                .map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn list_guestbook_entries(
        &self,
    ) -> Result<Vec<crate::commands::profile::GuestbookEntry>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn
            .open_table(GUESTBOOK_TABLE)
            .map_err(|e| e.to_string())?;
        let mut entries = Vec::new();
        let iter = table.iter().map_err(|e| e.to_string())?;
        for result in iter {
            let (_, value) = result.map_err(|e| e.to_string())?;
            let entry: crate::commands::profile::GuestbookEntry =
                serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
            entries.push(entry);
        }
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(entries)
    }

    pub async fn delete_guestbook_entry(&self, entry_id: &str) -> Result<(), String> {
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn
                .open_table(GUESTBOOK_TABLE)
                .map_err(|e| e.to_string())?;
            let _ = table.remove(entry_id);
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }
}
