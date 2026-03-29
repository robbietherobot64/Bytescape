//! # models/mod.rs — Core Data Models
//!
//! This module defines every data structure used throughout Bytescape's backend.
//! These are the "shapes" that all data takes — posts, contacts, messages, vault
//! items, feed entries, etc. Every struct here is serializable to JSON (via serde)
//! so it can be:
//!
//! 1. Stored in the redb database (serialized to JSON strings)
//! 2. Sent to the React frontend over Tauri's IPC bridge
//! 3. Exchanged with P2P peers over the Iroh network
//!
//! ## Key Types
//! - [`BytescapeItem`] — The universal container for ALL content (posts, photos, articles)
//! - [`FeedEntry`] — A feed item optimized for UI display (sent to frontend)
//! - [`BytescapeItemHeader`] — Lightweight metadata for fast feed filtering without loading full items
//! - [`BytescapeType`] — Content classification enum (Status, Gallery, Article, Link, Video)
//!
//! ## TypeScript Mirror
//! Every public type here has a corresponding TypeScript interface in
//! `src/types/index.ts`. **These must stay in sync** — if you add a field here,
//! add it there too, or the frontend will silently drop it.
//!
//! ## Related Files
//! - `src/types/index.ts` — TypeScript counterparts
//! - `db/mod.rs` — Where these types are stored/retrieved
//! - `connectors/*.rs` — Where these types are created from platform data
//! - `commands/*.rs` — Where these types are sent to the frontend

use serde::{Deserialize, Serialize};

pub mod groups;
pub use groups::*;

// ═══════════════════════════════════════════════════════════════════
// SECTION 1: COMMUNICATION & TRANSPORT ENUMS
// Enums that classify how messages are sent between users.
// ═══════════════════════════════════════════════════════════════════

/// Whether a conversation is private (encrypted P2P) or public.
/// PendingPublic is a transitional state while waiting for the other
/// user to accept making the conversation public.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum ConversationMode {
    /// End-to-end private between two peers
    Private,
    /// Publicly visible (e.g., in a group context)
    Public,
    /// One party has requested making it public; awaiting acceptance
    PendingPublic,
}

/// Which network layer a message was sent over.
/// Iroh = direct P2P (no intermediary), Bluesky = via AT Protocol servers.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum TransportLayer {
    /// Direct peer-to-peer via Iroh networking
    Iroh,
    /// Via Bluesky's AT Protocol servers
    Bluesky,
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 2: BASIC DATA STRUCTURES
// Simple structs for posts, contacts, and conversations.
// ═══════════════════════════════════════════════════════════════════

/// A simple post record — used for P2P-originated posts before they
/// get promoted to full BytescapeItems.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Post {
    pub id: String,
    pub content: String,
    /// DID of the author (e.g., "did:plc:abc123")
    pub author_did: String,
    /// Unix timestamp when the post was created
    pub created_at: u64,
}

/// A contact in the user's address book.
/// Contacts may be from Bluesky, Mastodon, or direct P2P connections.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Contact {
    /// The contact's Decentralized Identifier
    pub did: String,
    /// Display handle (e.g., "user.bsky.social")
    pub handle: String,
    /// Whether this contact supports direct P2P connections (has a Bytescape node)
    pub p2p_capable: bool,
    /// Iroh node ID, if the contact has a Bytescape P2P node
    pub node_id: Option<String>,
    /// When we last checked this contact's status (Unix timestamp)
    pub last_polled_at: u64,
}

/// A conversation thread between the current user and another person.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Conversation {
    /// Unique conversation identifier
    pub id: String,
    /// DID of the other participant
    pub peer_did: String,
    /// Whether this conversation is private, public, or transitioning
    pub current_mode: ConversationMode,
    /// Unix timestamp of the most recent message
    pub last_message_at: u64,
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 3: CONTENT CLASSIFICATION & SYNDICATION
// How content is categorized and how it gets shared across platforms.
// ═══════════════════════════════════════════════════════════════════

/// The five content types in Bytescape. Every piece of content is automatically
/// classified into one of these based on its content (see [`classify_item`]).
/// This classification drives how the content is displayed in the UI:
/// - Status = short text (tweet-like)
/// - Gallery = image-focused with photos
/// - Article = long-form text (500+ chars)
/// - Link = shared URL with preview card
/// - Video = has video attachment(s)
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum BytescapeType {
    Status,
    Gallery,
    Article,
    Link,
    Video,
}

/// Tracks whether a post has been published to external platforms.
/// - LocalOnly = only exists in the user's vault (default)
/// - PendingSyndication = queued for publishing
/// - Syndicated = successfully published to at least one platform
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum SyndicationStatus {
    LocalOnly,
    PendingSyndication,
    Syndicated,
}

/// Returns the default syndication status for new items (LocalOnly).
pub fn default_syndication_status() -> SyndicationStatus {
    SyndicationStatus::LocalOnly
}

/// Which audiences/networks a post has been shared to.
/// A post can be on multiple networks simultaneously.
/// For example, a post might be Local + Bluesky + P2p.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Audience {
    /// Only in the user's local vault (private)
    Local,
    /// Published to Bluesky via AT Protocol
    Bluesky,
    /// Shared with connected Bytescape peers
    P2p,
    /// Published to the fediverse (Mastodon, Lemmy, etc.)
    Activitypub,
}

/// A single event in a post's sharing history.
/// These form an append-only log tracking every time a post was published
/// to (or removed from) an external platform.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SyndicationEvent {
    /// Which platform: "bluesky", "mastodon", "p2p", etc.
    pub platform: String,
    /// The post's identifier on that platform (e.g., AT-URI for Bluesky)
    pub uri: String,
    /// When the share happened (Unix timestamp)
    pub timestamp: u64,
    /// Whether the post is still live on that platform
    pub active: bool,
}

/// Default audiences for a new item: just Local (private).
pub fn default_audiences() -> Vec<Audience> {
    vec![Audience::Local]
}

/// Migrates old-format syndication data to the new Audience list.
/// Early versions of Bytescape only tracked syndication_status + syndicated_uri.
/// This converts that into the modern audiences array.
pub fn audiences_from_legacy(status: &SyndicationStatus, syndicated_uri: &Option<String>) -> Vec<Audience> {
    let mut audiences = vec![Audience::Local];
    match status {
        SyndicationStatus::Syndicated | SyndicationStatus::PendingSyndication => {
            if syndicated_uri.is_some() {
                audiences.push(Audience::Bluesky);
            }
        }
        SyndicationStatus::LocalOnly => {}
    }
    audiences
}

/// Migrates old-format syndicated_uri (single string) to the new map format.
/// Early versions only supported syndicating to Bluesky, so the single URI
/// is always mapped to the "bluesky" key.
pub fn syndicated_uris_from_legacy(syndicated_uri: &Option<String>) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if let Some(uri) = syndicated_uri {
        map.insert("bluesky".to_string(), uri.clone());
    }
    map
}

/// Migrates all legacy syndication data into the modern share_history event log.
/// Tries three migration paths in order:
/// 1. syndicated_uris + syndicated_at maps (v2 format)
/// 2. syndicated_uri + syndication_status (v1 format)
/// 3. Empty history if nothing to migrate
pub fn share_history_from_legacy(
    syndicated_uris: &std::collections::HashMap<String, String>,
    syndicated_at: &std::collections::HashMap<String, u64>,
    legacy_uri: &Option<String>,
    legacy_status: &SyndicationStatus,
) -> Vec<SyndicationEvent> {
    let mut history = Vec::new();

    // 1. Try to migrate from the old maps first
    for (platform, uri) in syndicated_uris {
        let ts = syndicated_at.get(platform).copied().unwrap_or(0);
        history.push(SyndicationEvent {
            platform: platform.clone(),
            uri: uri.clone(),
            timestamp: ts,
            active: true, // Legacy implies it was active
        });
    }

    // 2. Fallback to v1 legacy field if empty
    if history.is_empty() {
        if let Some(uri) = legacy_uri {
            if matches!(legacy_status, SyndicationStatus::PendingSyndication | SyndicationStatus::Syndicated) {
                history.push(SyndicationEvent {
                    platform: "bluesky".to_string(),
                    uri: uri.clone(),
                    timestamp: 0,
                    active: true,
                });
            }
        }
    }

    // Sort by timestamp if possible
    history.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    history
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 4: VAULT STATISTICS & AUDIT TYPES
// ═══════════════════════════════════════════════════════════════════

/// Summary statistics for the user's vault. Sent to the frontend
/// for dashboard display (total items, breakdowns by source/type).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VaultStats {
    pub total: usize,
    pub by_source: std::collections::HashMap<String, usize>,
    pub by_type: std::collections::HashMap<String, usize>,
    pub media_count: usize,
}

/// A single audit record from the Willow write-parity check.
/// Willow is the data sync protocol — this tracks whether vault items
/// were correctly written to the Willow store (for P2P sync).
/// Used during development to catch sync mismatches.
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

/// Aggregated report of Willow write-parity across all vault items.
/// Shows how many items are correctly synced vs. mismatched.
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

// ═══════════════════════════════════════════════════════════════════
// SECTION 5: BACKWARD-COMPATIBLE DESERIALIZERS
// These custom deserializers handle old data formats gracefully.
// Early versions stored tagged_people and location as JSON strings;
// newer versions store them as proper typed objects. These functions
// accept either format so old vault data can still be read.
// ═══════════════════════════════════════════════════════════════════

/// Backward-compatible deserializer for tagged_people.
/// Accepts: null, JSON string (legacy format from early imports), or native Vec<TaggedPerson>.
fn deserialize_tagged_people<'de, D>(deserializer: D) -> Result<Vec<TaggedPerson>, D::Error>
where D: serde::Deserializer<'de> {
    use serde::de;
    
    struct TaggedPeopleVisitor;
    impl<'de> de::Visitor<'de> for TaggedPeopleVisitor {
        type Value = Vec<TaggedPerson>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("null, a JSON string, or an array of TaggedPerson")
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> { Ok(vec![]) }
        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> { Ok(vec![]) }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            if v.is_empty() { return Ok(vec![]); }
            serde_json::from_str(v).map_err(|_| de::Error::custom("invalid tagged_people JSON string"))
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
            Vec::<TaggedPerson>::deserialize(de::value::SeqAccessDeserializer::new(seq))
        }
    }
    deserializer.deserialize_any(TaggedPeopleVisitor)
}

/// Backward-compatible deserializer for location.
/// Accepts: null, JSON string (legacy), or native PostLocation object.
fn deserialize_location<'de, D>(deserializer: D) -> Result<Option<PostLocation>, D::Error>
where D: serde::Deserializer<'de> {
    use serde::de;
    
    struct LocationVisitor;
    impl<'de> de::Visitor<'de> for LocationVisitor {
        type Value = Option<PostLocation>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("null, a JSON string, or a PostLocation object")
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> { Ok(None) }
        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> { Ok(None) }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            if v.is_empty() { return Ok(None); }
            // Try parsing as a PostLocation JSON object
            if let Ok(loc) = serde_json::from_str::<PostLocation>(v) {
                return Ok(Some(loc));
            }
            // Legacy: plain location name string
            Ok(Some(PostLocation { name: v.to_string(), latitude: None, longitude: None }))
        }
        fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            let loc = PostLocation::deserialize(de::value::MapAccessDeserializer::new(map))?;
            Ok(Some(loc))
        }
    }
    deserializer.deserialize_any(LocationVisitor)
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 6: BYTESCAPE ITEM — THE UNIVERSAL CONTENT CONTAINER
// This is the single most important type in the entire backend.
// Every piece of content in Bytescape becomes a BytescapeItem.
// ═══════════════════════════════════════════════════════════════════

/// The universal content container — the core type of Bytescape.
///
/// Every piece of content, regardless of its origin platform, gets
/// normalized into this struct before storage. A Bluesky post, a Facebook
/// memory, an Instagram photo, and a locally-composed article all share
/// this exact shape. This is what makes the unified feed possible.
///
/// ## Fields by Category
/// - **Identity**: id, original_id, source, path
/// - **Content**: content, content_html, title, item_type
/// - **Author**: author_did, author_handle, author_avatar
/// - **Media**: media_paths (legacy), media_blobs + thumbnail_blobs (current), media_mimes
/// - **Metadata**: tagged_people, location, link_url
/// - **Engagement**: is_liked, like_count, reply_count, repost_count, quote_count
/// - **Syndication**: syndication_status, audiences, share_history
/// - **Context**: reposted_by_*, reply_parent_*, collab_doc_id
///
/// ## TypeScript Mirror
/// `src/types/index.ts::BytescapeItem`
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
    #[serde(default, deserialize_with = "deserialize_tagged_people")]
    pub tagged_people: Vec<TaggedPerson>,
    #[serde(default, deserialize_with = "deserialize_location")]
    pub location: Option<PostLocation>,
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
    #[serde(default)]
    pub media_blobs: Vec<String>,
    #[serde(default)]
    pub thumbnail_blobs: Vec<String>,
    #[serde(default)]
    pub media_mimes: Vec<String>,
    #[serde(default = "default_syndication_status")]
    pub syndication_status: SyndicationStatus,
    #[serde(default)]
    pub syndicated_uri: Option<String>,
    #[serde(default = "default_audiences")]
    pub audiences: Vec<Audience>, // Kept for backward compat locally, but share_history is source of truth
    
    // Legacy maps kept purely for deserialization of old items, but skipped for serialization going forward
    #[serde(default, skip_serializing)]
    pub syndicated_uris: std::collections::HashMap<String, String>,
    #[serde(default, skip_serializing)]
    pub syndicated_at: std::collections::HashMap<String, u64>,
    
    // New Event Sourced History Log
    #[serde(default)]
    pub share_history: Vec<SyndicationEvent>,

    // Repost metadata: set when this vault item is a repost by the owner
    #[serde(default)]
    pub reposted_by_did: Option<String>,
    #[serde(default)]
    pub reposted_by_handle: Option<String>,

    // Reply context: set when this vault item is a reply to another post
    #[serde(default)]
    pub reply_parent_uri: Option<String>,
    #[serde(default)]
    pub reply_parent_handle: Option<String>,

    // Collaborative editing: links to a Yjs CRDT document (C6.1)
    #[serde(default)]
    pub collab_doc_id: Option<String>,
}

/// Lightweight metadata extracted from a BytescapeItem for fast filtering.
///
/// Loading all ~500+ BytescapeItems from the database just to filter by type
/// or date would be slow. Instead, the database maintains a cache of these
/// headers, which contain just enough info to filter and sort the feed
/// without deserializing the full item JSON.
///
/// Created by `BytescapeItem::header()` and cached in `DbManager::header_cache`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BytescapeItemHeader {
    pub id: String,
    pub original_timestamp: i64,
    pub effective_timestamp: i64,
    pub source: String,
    pub path: String,
    pub author_did: Option<String>,
    pub item_type: BytescapeType,
    pub is_liked: bool,
    pub has_media: bool,
    pub media_blob_count: usize,
    pub media_mimes_count: usize,
    pub month: u32,
    pub day: u32,
}

impl BytescapeItem {
    /// Creates a lightweight header from this item for cache storage.
    /// The effective_timestamp is the later of original_timestamp and
    /// the most recent active share event — so reshared posts sort
    /// by when they were reshared, not when originally created.
    pub fn header(&self) -> BytescapeItemHeader {
        let max_share_ts = self.share_history.iter()
            .filter(|e| e.active)
            .map(|e| e.timestamp as i64)
            .max()
            .unwrap_or(self.original_timestamp);
        let effective_ts = max_share_ts.max(self.original_timestamp);

        let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(self.original_timestamp, 0)
            .unwrap_or_else(|| chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0).unwrap());
        
        use chrono::Datelike;

        BytescapeItemHeader {
            id: self.id.clone(),
            original_timestamp: self.original_timestamp,
            effective_timestamp: effective_ts,
            source: self.source.clone(),
            path: self.path.clone(),
            author_did: self.author_did.clone(),
            item_type: self.item_type,
            is_liked: self.is_liked,
            has_media: !self.media_paths.is_empty() || !self.media_blobs.is_empty(),
            media_blob_count: self.media_blobs.len(),
            media_mimes_count: self.media_mimes.len(),
            month: dt.month(),
            day: dt.day(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 7: CONTENT CLASSIFICATION
// Automatic classification of content into BytescapeType.
// Three entry points depending on the data source:
// - classify_item() — for BytescapeItems already in the vault
// - classify_bluesky_post() — for raw Bluesky API data
// - classify_feed_entry() — for FeedEntry objects in the UI pipeline
// ═══════════════════════════════════════════════════════════════════

pub fn default_item_type() -> BytescapeType {
    BytescapeType::Status
}

/// Classify a BytescapeItem based on its content.
/// Priority order (most specific first): Video > Gallery > Article > Link > Status
pub fn classify_item(item: &BytescapeItem) -> BytescapeType {
    let has_video = item.media_mimes.iter().any(|m| m.starts_with("video/"));
    let image_count = item.media_mimes.iter().filter(|m| m.starts_with("image/")).count();
    // Fallback: count non-video media as images if no MIME info
    let effective_image_count = if item.media_mimes.is_empty() {
        item.media_blobs.len() + item.media_paths.len()
    } else {
        image_count
    };
    let body_len = item.content.trim().len();

    // 1. Video: has video attachment(s)
    if has_video {
        return BytescapeType::Video;
    }

    // 2. Gallery: >=2 images and text <=300 chars
    if effective_image_count >= 2 && body_len <= 300 {
        return BytescapeType::Gallery;
    }

    // 3. Article: >500 chars body text
    if body_len > 500 {
        return BytescapeType::Article;
    }

    // 4. Link: has link_url and <=100 chars original text
    if item.link_url.is_some() && body_len <= 100 {
        return BytescapeType::Link;
    }

    // 5. Default: Status
    BytescapeType::Status
}

/// Classify a FeedEntry for live timeline display.
/// Classifies a Bluesky feed item from its fields (before BytescapeItem creation).
pub fn classify_bluesky_post(
    text: &str,
    image_count: usize,
    has_video: bool,
    has_external_url: bool,
) -> BytescapeType {
    if has_video {
        return BytescapeType::Video;
    }

    if image_count >= 2 && text.len() <= 300 {
        return BytescapeType::Gallery;
    }

    if text.trim().len() > 500 {
        return BytescapeType::Article;
    }

    if has_external_url && text.trim().len() <= 100 {
        return BytescapeType::Link;
    }

    if image_count == 1 && text.trim().len() <= 100 {
        return BytescapeType::Gallery;
    }

    BytescapeType::Status
}

/// Classify a FeedEntry (used for live Bluesky timeline items that don't have a BytescapeItem).
pub fn classify_feed_entry(entry: &FeedEntry) -> BytescapeType {
    let has_video = entry.media_kinds.iter().any(|k| k == "video");
    if has_video {
        return BytescapeType::Video;
    }

    let image_count = entry.media_kinds.iter().filter(|k| *k == "image").count()
        .max(if entry.media_kinds.is_empty() { entry.media_urls.len() } else { 0 });

    if image_count >= 2 && entry.content.len() <= 300 {
        return BytescapeType::Gallery;
    }

    if entry.content.trim().len() > 500 {
        return BytescapeType::Article;
    }

    if entry.link_url.is_some() && entry.content.trim().len() <= 100 {
        return BytescapeType::Link;
    }

    if image_count == 1 && entry.content.trim().len() <= 100 {
        return BytescapeType::Gallery;
    }

    BytescapeType::Status
}

/// A location or check-in attached to a post — universal across all connectors.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PostLocation {
    pub name: String,
    #[serde(default)]
    pub latitude: Option<f64>,
    #[serde(default)]
    pub longitude: Option<f64>,
}

/// A person tagged in a post — universal across all connectors.
/// The source_name is stored privately in the vault and only revealed
/// if the tagged person links their account.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaggedPerson {
    /// The name from the source platform (e.g. Facebook display name)
    pub source_name: String,
    /// Which platform this tag came from
    pub source_platform: String,
    /// Once the tagged person registers and links their account, their DID goes here
    #[serde(default)]
    pub linked_did: Option<String>,
    /// Their Bytescape handle once linked
    #[serde(default)]
    pub linked_handle: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 8: WILLOW PATH & VAULT STORAGE
// Functions for generating hierarchical storage paths for vault items.
// Each item gets a path like /facebook/status/2024/03/abc123 which
// organizes the Willow/Iroh sync store.
// ═══════════════════════════════════════════════════════════════════

/// Maximum length for any single path component in a Willow path.
pub fn willow_component_limit() -> usize {
    1024
}

/// Generates the canonical storage path for a BytescapeItem in the Willow store.
/// Format: /{source}/{type}/{year}/{month}/{id}
/// Example: /facebook/status/2024/03/abc123
///
/// Path components are normalized: lowercased, non-alphanumeric chars replaced
/// with hyphens, and truncated to the component limit.
pub fn canonical_willow_path(item: &BytescapeItem) -> String {
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
        BytescapeType::Link => "link",
        BytescapeType::Video => "video",
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

// ═══════════════════════════════════════════════════════════════════
// SECTION 9: MESSAGING TYPES
// Structs for chat messages — both imported (from archives) and
// live (from P2P or Bluesky DMs).
// ═══════════════════════════════════════════════════════════════════

/// A chat message stored in the vault from an imported archive
/// (Facebook Messenger, Instagram DMs, etc.).
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
    #[serde(default)]
    pub peer_did: Option<String>,
    #[serde(default)]
    pub peer_handle: Option<String>,
}

/// A live chat message (P2P or Bluesky DMs).
/// Unlike VaultMessage (imported from archives), Message represents
/// real-time conversations between Bytescape users.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    /// Unique message identifier
    pub id: String,
    /// Which conversation this message belongs to
    pub conversation_id: String,
    /// DID of the sender
    pub sender_did: String,
    /// Message text content
    pub content: String,
    /// When the message was sent (Unix timestamp)
    pub created_at: u64,
    /// Which network the message was sent over (Iroh P2P or Bluesky)
    #[serde(default = "default_transport_layer")]
    pub transport_layer: TransportLayer,
    /// Whether this message has been synced to the Bluesky convo
    pub synced_with_bsky: bool,
}

pub fn default_transport_layer() -> TransportLayer {
    TransportLayer::Bluesky
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 10: FEED ENTRY (BACKEND → FRONTEND)
// The struct sent across the IPC bridge to the React frontend.
// Unlike BytescapeItem (which is for storage), FeedEntry is
// optimized for what the UI needs to render each feed card.
// ═══════════════════════════════════════════════════════════════════

/// A feed item ready for frontend display.
///
/// Created by `commands/feed_aggregation.rs` from either:
/// - A live Bluesky/Mastodon API response (never stored in vault)
/// - A BytescapeItem loaded from the vault database
///
/// TypeScript mirror: `src/types/index.ts::FeedEntry`
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FeedEntry {
    pub id: String,
    pub timestamp: i64,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_did: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_handle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_avatar: Option<String>,
    pub source: String,
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewer_like: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewer_repost: Option<String>,
    
    #[serde(default)]
    pub media_urls: Vec<String>,
    #[serde(default)]
    pub media_thumbnails: Vec<String>,
    #[serde(default)]
    pub media_kinds: Vec<String>,
    #[serde(default)]
    pub media_mimes: Vec<String>,
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_thumbnail: Option<String>,
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub like_count: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_count: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repost_count: Option<i64>,
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reposted_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reposted_by_handle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_parent_handle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_parent_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quoted_post: Option<Box<FeedEntry>>,
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_type: Option<BytescapeType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub syndication_status: Option<SyndicationStatus>,
    #[serde(default)]
    pub audiences: Vec<Audience>,
    #[serde(default)]
    pub share_history: Vec<SyndicationEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<PostLocation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tagged_people: Vec<TaggedPerson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_url: Option<String>,
}
