//! # db/mod.rs — Database Layer (redb)
//!
//! This is the heart of Bytescape's local storage. All persistent data — posts,
//! messages, contacts, vault items, groups, and more — lives in a single **redb**
//! database file on the user's machine. Nothing is stored on remote servers.
//!
//! ## How redb Works (Key Concept)
//!
//! [redb](https://github.com/cberner/redb) is an embedded key-value store, similar
//! to SQLite but with a simpler API. Every piece of data is stored as a string key
//! mapped to a JSON-serialized string value:
//!
//! ```text
//!   Key (string)              →  Value (JSON string)
//!   "bluesky:at://did:..."    →  '{"id":"...","title":"My Post","source":"bluesky",...}'
//! ```
//!
//! All tables use `TableDefinition<&str, &str>` — meaning both the key and value
//! are UTF-8 strings. The value is always a `serde_json`-serialized Rust struct.
//!
//! ## Transaction Model
//!
//! redb uses explicit transactions for all operations:
//! - **Read transactions** (`db.begin_read()`) — for querying data, can run in parallel
//! - **Write transactions** (`db.begin_write()`) — for inserting/updating/deleting,
//!   must be exclusive (only one at a time)
//!
//! Every write follows this pattern:
//! 1. Acquire a write lock on the database (`self.db.write().await`)
//! 2. Begin a write transaction
//! 3. Open the target table
//! 4. Perform inserts/updates/deletes
//! 5. Commit the transaction (atomic — all-or-nothing)
//!
//! ## Architecture
//!
//! This file defines:
//! - All **table constants** (one per data type)
//! - The **`DbManager`** struct (the main database handle, shared across the app)
//! - **Core operations** (create post, clear data, hashtags, AP actors, profile, guestbook)
//!
//! Domain-specific operations are split into submodules:
//! - `vault.rs` — BytescapeItem storage, header caching, paged queries
//! - `chat.rs` — Messages, conversations, vault messages (DMs from Bluesky/Mastodon)
//! - `contacts.rs` — Friend/contact storage
//! - `peers.rs` — P2P peer tracking (Iroh node identities)
//! - `groups.rs` — Groups, topics, memberships, group posts
//!
//! ## Related Files
//! - `models/mod.rs` — The Rust structs that get serialized into these tables
//! - `models/groups.rs` — Group-specific data models
//! - `lib.rs` — Where `DbManager` is initialized and stored in Tauri's app state

use redb::{Database, TableDefinition, ReadableTable, ReadableDatabase, ReadableTableMetadata};

use std::sync::Arc;
use tokio::sync::RwLock;
use std::path::PathBuf;

// ─── Table Definitions ─────────────────────────────────────────────
// Each constant defines a named table in the redb database. All tables
// use string keys and JSON-serialized string values.
// The table name (e.g. "posts") is what appears in the database file on disk.

/// Legacy post storage — used for early archive imports before `BytescapeItem` existed.
pub(crate) const POSTS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("posts");

/// P2P and local chat messages, keyed by message ID.
/// See `chat.rs` for CRUD operations.
pub(crate) const MESSAGES_TABLE: TableDefinition<&str, &str> = TableDefinition::new("messages");

/// Contact/friend records, keyed by DID (decentralized identifier).
/// See `contacts.rs` for CRUD operations.
pub(crate) const CONTACTS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("contacts");

/// Chat conversation metadata (participants, last message, etc.), keyed by conversation ID.
pub(crate) const CONVERSATIONS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("conversations");

/// The main vault — all normalized content (posts, photos, articles, videos) from
/// every platform, keyed by `source:original_id` (e.g. `"bluesky:at://did:plc:..."`)
/// or by UUID for locally-created items. See `vault.rs` for CRUD operations.
pub(crate) const ITEMS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("bytescape_items");

/// Direct messages imported from external platforms (Bluesky, Mastodon).
/// Keyed by `owner_did:source:message_id`. See `chat.rs` for operations.
pub(crate) const VAULT_MESSAGES_TABLE: TableDefinition<&str, &str> = TableDefinition::new("vault_messages");

/// Hashtags the user has chosen to follow for discovery, keyed by tag string.
/// Value is a Unix timestamp of when the tag was followed.
pub(crate) const FOLLOWED_HASHTAGS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("followed_hashtags");

/// Diagnostic audit trail for Willow protocol migration.
/// Tracks every write to `ITEMS_TABLE` and verifies that the storage key
/// and Willow path are consistent. Used for debugging sync issues.
pub(crate) const WILLOW_WRITE_PARITY_TABLE: TableDefinition<&str, &str> = TableDefinition::new("willow_write_parity");

/// Known P2P peers (Iroh nodes), keyed by node_id (public key).
/// See `peers.rs` for CRUD operations and the `PeerInfo` struct.
pub(crate) const PEERS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("peers");

/// Collaborative document metadata for real-time editing (future feature).
pub(crate) const COLLAB_DOCS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("collab_docs");

/// ActivityPub actor profiles, keyed by the owner's DID.
/// Each entry maps a Bytescape user to their ActivityPub identity
/// (used for federating with Mastodon, Lemmy, etc.).
pub(crate) const AP_ACTORS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("ap_actors");

/// User's profile layout configuration (widget arrangement, theme choices).
/// Uses fixed keys like `"user_profile_layout"` and `"user_profile_sections"`.
pub(crate) const PROFILE_LAYOUT_TABLE: TableDefinition<&str, &str> = TableDefinition::new("profile_layout");

/// Guestbook entries left by visitors on the user's profile, keyed by entry ID.
pub(crate) const GUESTBOOK_TABLE: TableDefinition<&str, &str> = TableDefinition::new("guestbook");

// ─── Groups & Topics ───────────────────────────────────────────────
// These tables power the community/forum feature. Topics are categories,
// groups are communities within topics, and members/posts link users and
// content to groups. See `groups.rs` for all CRUD operations.

/// Topic categories (e.g. "Music", "Programming"), keyed by topic ID.
pub(crate) const TOPICS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("topics");

/// Which users are subscribed to which topics, keyed by `user_did:topic_id`.
pub(crate) const TOPIC_SUBSCRIPTIONS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("topic_subscriptions");

/// Group (community) records, keyed by group ID. Contains name, visibility, rules, etc.
pub(crate) const GROUPS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("groups");

/// Group membership records, keyed by `group_id:member_did`.
/// Tracks role (Owner/Moderator/Member) and status (Active/Pending/Invited/Banned).
pub(crate) const GROUP_MEMBERS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("group_members");

/// Posts shared into groups, keyed by `group_id:timestamp:item_id`.
/// The timestamp in the key enables chronological ordering.
pub(crate) const GROUP_POSTS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("group_posts");

pub use crate::models::*;

/// Contact CRUD — save, get, list contacts. See `contacts.rs`.
pub mod contacts;
/// Chat CRUD — messages, conversations, vault messages. See `chat.rs`.
pub mod chat;
/// Vault CRUD — BytescapeItem storage, caching, paged queries. See `vault.rs`.
pub mod vault;
/// Peer CRUD — P2P node identity tracking. See `peers.rs`.
pub mod peers;
/// Groups CRUD — topics, groups, members, group posts. See `groups.rs`.
pub mod groups;

// ─── DbManager — Main Database Handle ──────────────────────────────

/// The central database manager for all of Bytescape's local storage.
///
/// `DbManager` wraps a redb `Database` behind an async-aware `RwLock`, making it
/// safe to use from multiple Tauri commands running concurrently. It also maintains
/// an in-memory **header cache** to avoid expensive full-table scans when loading
/// the feed.
///
/// ## How It's Used
///
/// A single `DbManager` instance is created at app startup (in `lib.rs`) and stored
/// in Tauri's managed state. Every Tauri command that needs database access receives
/// it via `State<AppState>`:
///
/// ```rust
/// let items = state.db.list_all_item_headers().await?;
/// ```
///
/// ## Header Cache
///
/// Loading the feed requires reading every vault item's metadata (title, timestamp,
/// type, source, etc.) but NOT the full content (body text, media URLs). The header
/// cache stores these lightweight `BytescapeItemHeader` objects in memory so
/// subsequent feed loads skip the expensive disk read + JSON deserialization.
///
/// The cache is automatically invalidated whenever an item is saved or deleted.
#[derive(Clone)]
pub struct DbManager {
    /// The redb database handle, wrapped in Arc<RwLock> for thread-safe async access.
    /// Read operations acquire a read lock (multiple can run in parallel).
    /// Write operations acquire a write lock (exclusive — blocks other writes).
    pub(crate) db: Arc<RwLock<Database>>,
    /// Cached vault item headers — avoids full table scan + JSON deserialization
    /// on every feed load. Invalidated on any write (save_item, delete, etc.).
    pub(crate) header_cache: Arc<RwLock<Option<Vec<BytescapeItemHeader>>>>,
}

impl DbManager {
    /// Creates a new database at the given file path, or opens it if it already exists.
    ///
    /// On first run, this creates every table listed above in a single transaction.
    /// On subsequent runs, the `open_table` calls are no-ops (tables already exist).
    /// This "ensure tables exist" pattern means the app never needs a migration step.
    ///
    /// # Arguments
    /// * `path` - Absolute file path where the redb database file should live
    ///            (typically `~/.local/share/bytescape/bytescape.redb`)
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

    // ─── Legacy Post Operations ───────────────────────────────────────
    // These operate on the `posts` table, which predates the vault system.
    // Kept for backward compatibility with early archive imports.

    /// Creates a new post in the legacy `posts` table.
    ///
    /// Generates a UUID, timestamps it, serializes to JSON, and stores it.
    /// Most new content should use `save_item()` in `vault.rs` instead.
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

    /// Lists all posts from the legacy `posts` table, sorted newest-first.
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

    // ─── Bulk Data Management ────────────────────────────────────────

    /// Deletes ALL vault items and recreates an empty table.
    ///
    /// Returns the number of items that were deleted. This is a destructive
    /// operation — used when the user wants to start fresh or re-import.
    /// The table is deleted and recreated atomically in a single transaction.
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

    /// Nuclear option: deletes ALL user data across all tables.
    ///
    /// Wipes posts, messages, contacts, conversations, vault items, hashtags,
    /// parity entries, peers, and profile layouts. Returns total count of all
    /// deleted records. Each table is deleted and immediately recreated as empty.
    ///
    /// **Does NOT delete**: Groups, topics, group posts, AP actors, guestbook, collab docs.
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
        let profile_layouts = read_txn.open_table(PROFILE_LAYOUT_TABLE).map_err(|e| e.to_string())?
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
            write_txn.delete_table(PROFILE_LAYOUT_TABLE).map_err(|e| e.to_string())?;

            write_txn.open_table(POSTS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(MESSAGES_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(CONTACTS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(CONVERSATIONS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(VAULT_MESSAGES_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(FOLLOWED_HASHTAGS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(WILLOW_WRITE_PARITY_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(PEERS_TABLE).map_err(|e| e.to_string())?;
            write_txn.open_table(PROFILE_LAYOUT_TABLE).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;

        Ok(posts + messages + contacts + convos + items + vault_messages + hashtags + parity + peers + profile_layouts)
    }

    /// Returns the total number of posts in the legacy `posts` table.
    /// Used to show import progress to the user.
    pub async fn count_archive_posts(&self) -> Result<u64, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(POSTS_TABLE).map_err(|e| e.to_string())?;
        Ok(table.len().unwrap_or(0))
    }

    // ─── Followed Hashtags ────────────────────────────────────────────
    // Users can follow hashtags for content discovery. The tag string is the
    // key, and the value is the Unix timestamp when it was followed.

    /// Start following a hashtag. Stores `tag → timestamp` in the hashtags table.
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

    /// Stop following a hashtag. Removes it from the hashtags table.
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

    /// Returns all followed hashtag strings (without the `#` prefix).
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


    // ─── ActivityPub Actor CRUD ─────────────────────────────────────
    // ActivityPub actors are the user's identity on federated networks
    // (Mastodon, Lemmy, etc.). Each actor record maps a Bytescape DID to
    // an AP identity with inbox/outbox URLs, public key, etc.

    /// Saves (or overwrites) an ActivityPub actor profile, keyed by `owner_did`.
    ///
    /// Called when the user enables federation or when their AP profile is updated.
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

    // ─── Profile Layout ──────────────────────────────────────────────
    // The user's customizable profile page layout. Stored as a single JSON
    // blob under a fixed key, since there's only one profile per device.

    /// Saves the user's profile layout configuration (widget positions, theme, etc.).
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

    /// Retrieves the ActivityPub actor for a given DID, or `None` if not found.
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

    /// Retrieves the user's saved profile layout, or `None` if they haven't customized it yet.
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

    /// Removes the ActivityPub actor for a DID. Used when disabling federation.
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

    /// Resets the profile layout to defaults by deleting the saved configuration.
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

    // ─── Profile Sections ──────────────────────────────────────────
    // Profile sections are individual content blocks on the user's profile
    // page (e.g. "About Me", "Recent Posts", "Music"). They're stored as
    // an ordered array under a fixed key in PROFILE_LAYOUT_TABLE.

    /// Saves the user's profile sections (ordered content blocks).
    /// The entire array is replaced atomically — there's no per-section update.
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

    /// Retrieves all profile sections, sorted by their `order` field.
    /// Returns an empty array if none have been saved yet.
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

    // ─── Guestbook ────────────────────────────────────────────────────
    // A MySpace-inspired feature where visitors to the user's profile can
    // leave a message. Entries are keyed by a UUID and sorted newest-first.

    /// Adds a guestbook entry (a message left by a profile visitor).
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

    /// Lists all guestbook entries, sorted newest-first by timestamp.
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

    /// Deletes a guestbook entry by its unique ID.
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
