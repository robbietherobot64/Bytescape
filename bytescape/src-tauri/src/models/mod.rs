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

/// Networks where a post is currently published.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Audience {
    Local,
    Bluesky,
    P2p,
    Activitypub,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SyndicationEvent {
    pub platform: String, // "bluesky", "p2p"
    pub uri: String,      // The remote ID on that platform
    pub timestamp: u64,   // When the share happened
    pub active: bool,     // True if currently live, False if synced as deleted upstream
}

pub fn default_audiences() -> Vec<Audience> {
    vec![Audience::Local]
}

/// Compute audiences from legacy syndication_status + syndicated_uri.
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

/// Compute syndicated_uris map from legacy syndicated_uri.
pub fn syndicated_uris_from_legacy(syndicated_uri: &Option<String>) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if let Some(uri) = syndicated_uri {
        map.insert("bluesky".to_string(), uri.clone());
    }
    map
}

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

/// Backward-compatible deserializer for tagged_people.
/// Accepts: null, JSON string (legacy), or native Vec<TaggedPerson>.
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

pub fn default_item_type() -> BytescapeType {
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
