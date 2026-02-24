use serde::{Deserialize, Serialize};

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

pub fn default_syndication_status() -> SyndicationStatus {
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
    #[serde(default)]
    pub tagged_people: Option<String>,
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
}

pub fn default_item_type() -> BytescapeType {
    BytescapeType::Status
}

pub fn willow_component_limit() -> usize {
    1024
}

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
    #[serde(default)]
    pub peer_did: Option<String>,
    #[serde(default)]
    pub peer_handle: Option<String>,
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

pub fn default_transport_layer() -> TransportLayer {
    TransportLayer::Bluesky
}

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
    pub quoted_post: Option<Box<FeedEntry>>,
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_type: Option<BytescapeType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub syndication_status: Option<SyndicationStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tagged_people: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_url: Option<String>,
}
