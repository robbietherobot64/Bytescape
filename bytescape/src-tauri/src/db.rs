use redb::{Database, TableDefinition, ReadableTable, ReadableDatabase, ReadableTableMetadata};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::path::PathBuf;

const POSTS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("posts");
const MESSAGES_TABLE: TableDefinition<&str, &str> = TableDefinition::new("messages");
const CONTACTS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("contacts");
const CONVERSATIONS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("conversations");
const ITEMS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("bytescape_items");
const VAULT_MESSAGES_TABLE: TableDefinition<&str, &str> = TableDefinition::new("vault_messages");
const FOLLOWED_HASHTAGS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("followed_hashtags");
const WILLOW_WRITE_PARITY_TABLE: TableDefinition<&str, &str> = TableDefinition::new("willow_write_parity");

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum ConversationMode {
    Private,
    Public,
    PendingPublic,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum TransportLayer {
    Iroh,
    Bluesky,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Post {
    pub id: String,
    pub content: String,
    pub author_did: String,
    pub created_at: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Contact {
    pub did: String,
    pub handle: String,
    pub p2p_capable: bool,
    pub node_id: Option<String>,
    pub last_polled_at: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Conversation {
    pub id: String,
    pub peer_did: String,
    pub current_mode: ConversationMode,
    pub last_message_at: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum BytescapeType {
    Status,
    Gallery,
    Article,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum SyndicationStatus {
    LocalOnly,
    PendingSyndication,
    Syndicated,
}

fn default_syndication_status() -> SyndicationStatus {
    SyndicationStatus::LocalOnly
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VaultStats {
    pub total: usize,
    pub by_source: std::collections::HashMap<String, usize>,
    pub by_type: std::collections::HashMap<String, usize>,
    pub media_count: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WillowWriteParityEntry {
    pub id: String,
    pub written_at: u64,
    pub write_mode: String,
    pub item_id: String,
    pub source: String,
    pub legacy_key: String,
    pub expected_legacy_key: String,
    pub legacy_key_matches: bool,
    pub willow_path: String,
    pub canonical_path: String,
    pub path_present: bool,
    pub path_matches_canonical: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WillowWriteParityReport {
    pub total_writes: usize,
    pub save_writes: usize,
    pub upsert_writes: usize,
    pub mismatched_legacy_keys: usize,
    pub missing_paths: usize,
    pub mismatched_paths: usize,
    pub last_written_at: Option<u64>,
    pub total_items: usize,
    pub items_with_path: usize,
    pub items_missing_path: usize,
    pub items_matching_canonical_path: usize,
    pub items_mismatched_canonical_path: usize,
    pub parity_coverage_ratio: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BytescapeItem {
    pub id: String,
    pub original_timestamp: i64,
    pub content: String,
    pub media_paths: Vec<String>,
    pub source: String,
    #[serde(default)]
    pub path: String,
    pub imported_at: u64,
    pub title: Option<String>,
    #[serde(default)]
    pub original_id: Option<String>,
    #[serde(default)]
    pub author_did: Option<String>,
    #[serde(default)]
    pub author_handle: Option<String>,
    #[serde(default)]
    pub author_avatar: Option<String>,
    /// JSON-serialized Vec<TaggedPerson> — stored privately in the vault.
    /// Displayed as placeholders (👤) until tagged users link their accounts.
    #[serde(default)]
    pub tagged_people: Option<String>,
    /// JSON-serialized PostLocation — structured place/check-in data
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub is_liked: bool,
    #[serde(default)]
    pub like_count: Option<i64>,
    #[serde(default)]
    pub reply_count: Option<i64>,
    #[serde(default)]
    pub repost_count: Option<i64>,
    #[serde(default)]
    pub quote_count: Option<i64>,
    // Universal Container Fields
    #[serde(default = "default_item_type")]
    pub item_type: BytescapeType,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub link_url: Option<String>,
    #[serde(default)]
    pub content_html: Option<String>,
    #[serde(default)]
    pub preview_img: Option<String>,
    // Iroh Blob Storage (format: "hash" or "hash:thumbhash")
    #[serde(default)]
    pub media_blobs: Vec<String>,
    #[serde(default)]
    pub thumbnail_blobs: Vec<String>,
    /// MIME types for each media blob, aligned with media_blobs.
    /// e.g. ["video/mp4", "image/jpeg"]. Populated at import time from magic bytes.
    #[serde(default)]
    pub media_mimes: Vec<String>,
    // Syndication
    #[serde(default = "default_syndication_status")]
    pub syndication_status: SyndicationStatus,
}

fn default_item_type() -> BytescapeType {
    BytescapeType::Status
}

fn willow_component_limit() -> usize {
    1024
}

fn canonical_willow_path(item: &BytescapeItem) -> String {
    fn normalize_component(raw: &str) -> String {
        let cleaned = raw
            .trim()
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
            .collect::<String>();

        if cleaned.is_empty() {
            "unknown".to_string()
        } else if cleaned.len() > willow_component_limit() {
            cleaned[..willow_component_limit()].to_string()
        } else {
            cleaned
        }
    }

    let kind = match item.item_type {
        BytescapeType::Status => "status",
        BytescapeType::Gallery => "gallery",
        BytescapeType::Article => "article",
    };

    let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(item.original_timestamp, 0)
        .unwrap_or_else(|| chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0).unwrap());

    let path = format!(
        "/{}/{}/{:04}/{:02}/{}",
        normalize_component(&item.source),
        normalize_component(kind),
        chrono::Datelike::year(&dt),
        chrono::Datelike::month(&dt),
        normalize_component(&item.id),
    );

    if path.len() > willow_component_limit() {
        path[..willow_component_limit()].to_string()
    } else {
        path
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VaultMessage {
    pub id: String,
    #[serde(default)]
    pub owner_did: Option<String>,
    pub convo_id: String,
    pub sender_did: String,
    pub sender_handle: String,
    pub text: String,
    pub sent_at: String,
    pub source: String,
    pub imported_at: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub sender_did: String,
    pub content: String,
    pub created_at: u64,
    #[serde(default = "default_transport_layer")]
    pub transport_layer: TransportLayer,
    pub synced_with_bsky: bool,
}

fn default_transport_layer() -> TransportLayer {
    TransportLayer::Bluesky
}

#[derive(Clone)]
pub struct DbManager {
    db: Arc<Mutex<Database>>,
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
        }
        write_txn.commit().map_err(|e| e.to_string())?;

        Ok(Self {
            db: Arc::new(Mutex::new(db)),
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
        
        let db = self.db.lock().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(POSTS_TABLE).map_err(|e| e.to_string())?;
            table.insert(id.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;

        Ok(post)
    }

    pub async fn list_posts(&self) -> Result<Vec<Post>, String> {
        let db = self.db.lock().await;
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

    pub async fn save_message(&self, msg: &Message) -> Result<(), String> {
        let serialized = serde_json::to_string(msg).map_err(|e| e.to_string())?;

        let db = self.db.lock().await;
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

        let db = self.db.blocking_lock();
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(MESSAGES_TABLE).map_err(|e| e.to_string())?;
            table.insert(msg.id.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn get_messages(&self, conversation_id: &str) -> Result<Vec<Message>, String> {
        let db = self.db.lock().await;
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

    pub async fn save_contact(&self, contact: &Contact) -> Result<(), String> {
        let serialized = serde_json::to_string(contact).map_err(|e| e.to_string())?;
        let db = self.db.lock().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(CONTACTS_TABLE).map_err(|e| e.to_string())?;
            table.insert(contact.did.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn get_contact(&self, did: &str) -> Result<Option<Contact>, String> {
        let db = self.db.lock().await;
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
        let db = self.db.lock().await;
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

    pub async fn save_conversation(&self, convo: &Conversation) -> Result<(), String> {
        let serialized = serde_json::to_string(convo).map_err(|e| e.to_string())?;
        let db = self.db.lock().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(CONVERSATIONS_TABLE).map_err(|e| e.to_string())?;
            table.insert(convo.id.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn get_conversation(&self, id: &str) -> Result<Option<Conversation>, String> {
        let db = self.db.lock().await;
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

    // === Facebook Imported Posts ===

    pub async fn save_item(&self, item: &BytescapeItem) -> Result<bool, String> {
        let serialized = serde_json::to_string(item).map_err(|e| e.to_string())?;
        // Use source:original_id as key for dedup, fallback to item.id
        let key = if let Some(ref oid) = item.original_id {
            format!("{}:{}", item.source, oid)
        } else {
            item.id.clone()
        };
        let db = self.db.lock().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
        let already_exists = table.get(key.as_str()).map_err(|e| e.to_string())?.is_some();
        drop(table);
        drop(read_txn);

        if already_exists {
            println!("db: Item already exists with key: {}", key);
            return Ok(false); // not new
        }

        let parity = Self::build_willow_write_parity_entry(item, "save", &key);
        let parity_key = Self::willow_parity_storage_key(&parity);
        let parity_serialized = serde_json::to_string(&parity).map_err(|e| e.to_string())?;

        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
            table.insert(key.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
            drop(table);

            let mut parity_table = write_txn.open_table(WILLOW_WRITE_PARITY_TABLE).map_err(|e| e.to_string())?;
            parity_table.insert(parity_key.as_str(), parity_serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(true) // was new
    }

    fn item_storage_key(item: &BytescapeItem) -> String {
        if let Some(ref oid) = item.original_id {
            format!("{}:{}", item.source, oid)
        } else {
            item.id.clone()
        }
    }

    fn willow_parity_storage_key(entry: &WillowWriteParityEntry) -> String {
        format!("{}:{}", entry.written_at, entry.id)
    }

    fn build_willow_write_parity_entry(item: &BytescapeItem, write_mode: &str, legacy_key: &str) -> WillowWriteParityEntry {
        let expected_legacy_key = Self::item_storage_key(item);
        let canonical_path = canonical_willow_path(item);
        let willow_path = item.path.trim().to_string();
        let written_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        WillowWriteParityEntry {
            id: uuid::Uuid::new_v4().to_string(),
            written_at,
            write_mode: write_mode.to_string(),
            item_id: item.id.clone(),
            source: item.source.clone(),
            legacy_key: legacy_key.to_string(),
            expected_legacy_key: expected_legacy_key.clone(),
            legacy_key_matches: legacy_key == expected_legacy_key,
            willow_path: willow_path.clone(),
            canonical_path: canonical_path.clone(),
            path_present: !willow_path.is_empty(),
            path_matches_canonical: !willow_path.is_empty() && willow_path == canonical_path,
        }
    }

    pub async fn upsert_item(&self, item: &BytescapeItem) -> Result<(), String> {
        let serialized = serde_json::to_string(item).map_err(|e| e.to_string())?;
        let key = Self::item_storage_key(item);
        let parity = Self::build_willow_write_parity_entry(item, "upsert", &key);
        let parity_key = Self::willow_parity_storage_key(&parity);
        let parity_serialized = serde_json::to_string(&parity).map_err(|e| e.to_string())?;

        let db = self.db.lock().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
            table.insert(key.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
            drop(table);

            let mut parity_table = write_txn.open_table(WILLOW_WRITE_PARITY_TABLE).map_err(|e| e.to_string())?;
            parity_table.insert(parity_key.as_str(), parity_serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn get_willow_write_parity_report(&self) -> Result<WillowWriteParityReport, String> {
        let db = self.db.lock().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(WILLOW_WRITE_PARITY_TABLE).map_err(|e| e.to_string())?;
        let items_table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;

        let mut total_writes = 0usize;
        let mut save_writes = 0usize;
        let mut upsert_writes = 0usize;
        let mut mismatched_legacy_keys = 0usize;
        let mut missing_paths = 0usize;
        let mut mismatched_paths = 0usize;
        let mut last_written_at: Option<u64> = None;
        let mut total_items = 0usize;
        let mut items_with_path = 0usize;
        let mut items_missing_path = 0usize;
        let mut items_matching_canonical_path = 0usize;
        let mut items_mismatched_canonical_path = 0usize;

        for row in table.iter().map_err(|e| e.to_string())? {
            let (_key, value) = row.map_err(|e| e.to_string())?;
            let entry: WillowWriteParityEntry = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;

            total_writes += 1;
            match entry.write_mode.as_str() {
                "save" => save_writes += 1,
                "upsert" => upsert_writes += 1,
                _ => {}
            }

            if !entry.legacy_key_matches {
                mismatched_legacy_keys += 1;
            }
            if !entry.path_present {
                missing_paths += 1;
            } else if !entry.path_matches_canonical {
                mismatched_paths += 1;
            }

            last_written_at = Some(last_written_at.map_or(entry.written_at, |current| current.max(entry.written_at)));
        }

        for row in items_table.iter().map_err(|e| e.to_string())? {
            let (_key, value) = row.map_err(|e| e.to_string())?;
            let item: BytescapeItem = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
            total_items += 1;

            if item.path.trim().is_empty() {
                items_missing_path += 1;
                continue;
            }

            items_with_path += 1;
            let canonical_path = canonical_willow_path(&item);
            if item.path == canonical_path {
                items_matching_canonical_path += 1;
            } else {
                items_mismatched_canonical_path += 1;
            }
        }

        let parity_coverage_ratio = if total_items == 0 {
            1.0
        } else {
            (items_with_path as f64) / (total_items as f64)
        };

        Ok(WillowWriteParityReport {
            total_writes,
            save_writes,
            upsert_writes,
            mismatched_legacy_keys,
            missing_paths,
            mismatched_paths,
            last_written_at,
            total_items,
            items_with_path,
            items_missing_path,
            items_matching_canonical_path,
            items_mismatched_canonical_path,
            parity_coverage_ratio,
        })
    }

    pub async fn list_items(&self) -> Result<Vec<BytescapeItem>, String> {
        let db = self.db.lock().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;

        let mut items = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (_key, value) = item.map_err(|e| e.to_string())?;
            let item_struct: BytescapeItem = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
            items.push(item_struct);
        }

        items.sort_by(|a, b| b.original_timestamp.cmp(&a.original_timestamp));
        Ok(items)
    }

    pub async fn list_items_filtered(&self, filter_did: Option<&str>, exclude: bool) -> Result<Vec<BytescapeItem>, String> {
        let all = self.list_items().await?;
        if let Some(did) = filter_did {
            let filtered = all.into_iter().filter(|item| {
                if let Some(ref author_did) = item.author_did {
                    if exclude {
                        author_did != did
                    } else {
                        author_did == did
                    }
                } else {
                    // Items without author_did:
                    // If we want "Me" (exclude=false), we probably don't want them unless we know they are ours.
                    // If we want "Us" (exclude=true), we probably do want them? Or maybe not.
                    if exclude {
                        true // Not me
                    } else {
                        false // Not me
                    }
                }
            }).collect();
            Ok(filtered)
        } else {
            Ok(all)
        }
    }

    /// Return only items owned by the current user.
    /// An item is "mine" if:
    ///  - author_did matches the user's DID, OR
    ///  - author_did is None (archive imports like Facebook that don't have a DID)
    ///  - is_liked is true (posts the user explicitly liked/saved)
    pub async fn list_my_items(&self, owner_did: &str) -> Result<Vec<BytescapeItem>, String> {
        let all = self.list_items().await?;
        let filtered = all.into_iter().filter(|item| {
            // Posts the user explicitly liked/saved are theirs
            if item.is_liked {
                return true;
            }
            match &item.author_did {
                // Bluesky/API items: only include if authored by user
                Some(did) => did == owner_did,
                // Archive imports (Facebook, etc.): no author_did means it's the user's own data
                None => true,
            }
        }).collect();
        Ok(filtered)
    }

    pub async fn list_items_on_this_day(&self, month: u32, day: u32) -> Result<Vec<BytescapeItem>, String> {
        let all = self.list_items().await?;
        let matching: Vec<BytescapeItem> = all.into_iter().filter(|p| {
            if let Some(dt) = chrono::DateTime::from_timestamp(p.original_timestamp, 0) {
                use chrono::Datelike;
                dt.month() == month && dt.day() == day
            } else {
                false
            }
        }).collect();
        Ok(matching)
    }

    pub async fn count_items(&self) -> Result<usize, String> {
        let db = self.db.lock().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
        let count = table.len().map_err(|e: redb::StorageError| e.to_string())?;
        Ok(count as usize)
    }

    /// Paginated version: returns (items, total_count) sorted newest-first.
    pub async fn list_my_items_paged(&self, owner_did: &str, source_filter: Option<&str>, limit: usize, offset: usize) -> Result<(Vec<BytescapeItem>, usize), String> {
        let mut all = self.list_my_items(owner_did).await?;

        // Optional source filter
        if let Some(src) = source_filter {
            all.retain(|item| item.source.eq_ignore_ascii_case(src));
        }

        // Sort newest first
        all.sort_by(|a, b| b.original_timestamp.cmp(&a.original_timestamp));

        let total = all.len();
        let page: Vec<BytescapeItem> = all.into_iter().skip(offset).take(limit).collect();
        Ok((page, total))
    }

    /// Path-prefix paginated query for Willow migration compatibility.
    /// Returns (items, total_count) sorted newest-first.
    pub async fn list_my_items_by_path_prefix_paged(
        &self,
        owner_did: &str,
        path_prefix: &str,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<BytescapeItem>, usize), String> {
        let mut all = self.list_my_items(owner_did).await?;
        let normalized_prefix = if path_prefix.starts_with('/') {
            path_prefix.to_string()
        } else {
            format!("/{}", path_prefix)
        };

        all.retain(|item| item.path.starts_with(&normalized_prefix));
        all.sort_by(|a, b| b.original_timestamp.cmp(&a.original_timestamp));

        let total = all.len();
        let page: Vec<BytescapeItem> = all.into_iter().skip(offset).take(limit).collect();
        Ok((page, total))
    }

    pub async fn list_items_by_source(&self, source: &str, owner_did: &str) -> Result<Vec<BytescapeItem>, String> {
        let all = self.list_my_items(owner_did).await?;
        let filtered = all.into_iter()
            .filter(|item| item.source.eq_ignore_ascii_case(source))
            .collect();
        Ok(filtered)
    }

    pub async fn list_items_by_type(&self, item_type: BytescapeType, owner_did: &str) -> Result<Vec<BytescapeItem>, String> {
        let all = self.list_my_items(owner_did).await?;
        let filtered = all.into_iter()
            .filter(|item| item.item_type == item_type)
            .collect();
        Ok(filtered)
    }

    pub async fn list_items_by_date_range(&self, start_ts: i64, end_ts: i64, owner_did: &str) -> Result<Vec<BytescapeItem>, String> {
        let all = self.list_my_items(owner_did).await?;
        let filtered = all.into_iter()
            .filter(|item| item.original_timestamp >= start_ts && item.original_timestamp <= end_ts)
            .collect();
        Ok(filtered)
    }

    pub async fn list_vault_sources(&self, owner_did: &str) -> Result<Vec<String>, String> {
        let all = self.list_my_items(owner_did).await?;
        let mut sources: Vec<String> = all.iter()
            .map(|item| item.source.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        sources.sort();
        Ok(sources)
    }

    pub async fn vault_stats(&self, owner_did: &str) -> Result<VaultStats, String> {
        let all = self.list_my_items(owner_did).await?;
        let total = all.len();
        let mut by_source: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut by_type: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut media_count: usize = 0;

        for item in &all {
            *by_source.entry(item.source.clone()).or_insert(0) += 1;
            let type_str = match item.item_type {
                BytescapeType::Status => "status",
                BytescapeType::Gallery => "gallery",
                BytescapeType::Article => "article",
            };
            *by_type.entry(type_str.to_string()).or_insert(0) += 1;
            if !item.media_paths.is_empty() || !item.media_blobs.is_empty() {
                media_count += 1;
            }
        }

        Ok(VaultStats { total, by_source, by_type, media_count })
    }

    pub async fn clear_items(&self) -> Result<usize, String> {
        let db = self.db.lock().await;
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

    /// Clear all user-scoped local data tables.
    ///
    /// This is used when switching accounts so no previous account data leaks
    /// into the next session.
    pub async fn clear_all_user_data(&self) -> Result<usize, String> {
        let db = self.db.lock().await;

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

            write_txn.open_table(POSTS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(MESSAGES_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(CONTACTS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(CONVERSATIONS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(VAULT_MESSAGES_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(FOLLOWED_HASHTAGS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(WILLOW_WRITE_PARITY_TABLE).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;

        Ok(posts + messages + contacts + convos + items + vault_messages + hashtags + parity)
    }

    // === Vault Messages ===

    pub async fn save_vault_message(&self, msg: &VaultMessage) -> Result<bool, String> {
        let serialized = serde_json::to_string(msg).map_err(|e| e.to_string())?;
        let owner_key = msg.owner_did.clone().unwrap_or_else(|| "unknown".to_string());
        let key = format!("{}:{}:{}", owner_key, msg.source, msg.id);
        let db = self.db.lock().await;

        // Check if exists and if needs update
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(VAULT_MESSAGES_TABLE).map_err(|e| e.to_string())?;
        let existing_val = table.get(key.as_str()).map_err(|e| e.to_string())?;
        
        let mut needs_update = false;
        let mut is_new = true;
        
        if let Some(val) = existing_val {
            is_new = false;
            if let Ok(existing_msg) = serde_json::from_str::<VaultMessage>(val.value()) {
                if existing_msg.sender_handle != msg.sender_handle {
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
        let db = self.db.lock().await;
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

    pub async fn count_vault_messages(&self) -> Result<usize, String> {
        let db = self.db.lock().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(VAULT_MESSAGES_TABLE).map_err(|e| e.to_string())?;
        Ok(table.len().map_err(|e: redb::StorageError| e.to_string())? as usize)
    }

    pub async fn count_archive_posts(&self) -> Result<u64, String> {
        let db = self.db.lock().await; // Added this line for consistency
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(POSTS_TABLE).map_err(|e| e.to_string())?;
        Ok(table.len().unwrap_or(0))
    }

    // === Hashtags ===

    pub async fn save_hashtag(&self, tag: String) -> Result<(), String> {
        let db = self.db.lock().await; // Added this line for consistency
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(FOLLOWED_HASHTAGS_TABLE).map_err(|e| e.to_string())?;
            // Determine if we need to store metadata. For now, just timestamp.
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
            table.insert(tag.as_str(), now.to_string().as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn delete_hashtag(&self, tag: String) -> Result<(), String> {
        let db = self.db.lock().await; // Added this line for consistency
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(FOLLOWED_HASHTAGS_TABLE).map_err(|e| e.to_string())?;
            table.remove(tag.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn list_followed_hashtags(&self) -> Result<Vec<String>, String> {
        let db = self.db.lock().await; // Added this line for consistency
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
