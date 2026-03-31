//! Lemmy Compatibility Layer — Sprint v13 P4
//!
//! Handles Lemmy-specific ActivityPub quirks:
//! - Page-based post format (Lemmy sends Create/Page instead of Create/Note)
//! - Community discovery via nodeinfo + Lemmy API
//! - Remote community follow (local state + activity construction)
//!
//! # How It Works
//!
//! 1. Lemmy sends posts as Create/Page (not Create/Note like Mastodon) —
//!    `lemmy_page_to_bytescape_item()` handles this conversion, mapping
//!    `Page.name` to title, `Page.content` to HTML body, and `Page.url` to link
//! 2. Community discovery uses a 2-step process: first nodeinfo to confirm it's
//!    Lemmy, then the Lemmy API v3 `/community/list` endpoint
//! 3. Following a remote community: fetch the Group actor, build a Follow
//!    activity, deliver it (actual delivery requires the relay infrastructure),
//!    and create a local placeholder group for the UI
//! 4. `remote_community_to_local_group()` maps AP Group actors to Bytescape's
//!    `LocalGroup` format
//!
//! # Related Files
//!
//! - `inbox.rs` — processes incoming activities from followed communities
//! - `group_actor.rs` — local group AP representation
//! - `models/groups.rs` — `LocalGroup` type
//! - `db/groups.rs` — group persistence

use serde_json::Value;
use crate::models::{BytescapeItem, BytescapeType, SyndicationStatus, Audience};

// ═══════════════════════════════════════════════════════════════════
// PAGE → BYTESCAPE CONVERSION
// ═══════════════════════════════════════════════════════════════════

/// Convert a Lemmy `Page` AP object into a `BytescapeItem`.
///
/// Lemmy communities send posts as `Create/Page` (not `Create/Note`).
/// The `Page` object includes `name` (title), `content` (HTML body),
/// `url` (optional link), `mediaType`, and `audience` (community actor).
pub fn lemmy_page_to_bytescape_item(page: &Value) -> Result<BytescapeItem, String> {
    let page_id = page.get("id")
        .and_then(|v| v.as_str())
        .ok_or("Lemmy Page missing 'id'")?;

    let title = page.get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let content_html = page.get("content")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Strip HTML tags for plain-text content
    let content = content_html.as_deref()
        .map(strip_html_tags)
        .unwrap_or_default();

    let link_url = page.get("url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let author = page.get("attributedTo")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Extract thumbnail/image if present
    let preview_img = page.get("image")
        .and_then(|v| {
            // Lemmy uses { "type": "Image", "url": "..." }
            v.get("url").and_then(|u| u.as_str())
                .or_else(|| v.as_str())
        })
        .map(|s| s.to_string());

    // Determine type: if there's a title, it's an Article; link-only → Status
    let item_type = if title.is_some() {
        BytescapeType::Article
    } else {
        BytescapeType::Status
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    // Extract published timestamp
    let original_timestamp = page.get("published")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp())
        .unwrap_or_else(|| chrono::Utc::now().timestamp());

    Ok(BytescapeItem {
        id: uuid::Uuid::new_v4().to_string(),
        original_timestamp,
        content,
        media_paths: Vec::new(),
        source: "activitypub".to_string(),
        path: String::new(),
        imported_at: now,
        title,
        original_id: Some(page_id.to_string()),
        author_did: author.clone(),
        author_handle: author,
        author_avatar: None,
        tagged_people: Vec::new(),
        location: None,
        is_liked: false,
        like_count: None,
        reply_count: None,
        repost_count: None,
        quote_count: None,
        item_type,
        url: Some(page_id.to_string()),
        link_url,
        content_html,
        preview_img,
        media_blobs: Vec::new(),
        thumbnail_blobs: Vec::new(),
        media_mimes: Vec::new(),
        syndication_status: SyndicationStatus::LocalOnly,
        syndicated_uri: Some(page_id.to_string()),
        audiences: vec![Audience::Activitypub],
        syndicated_uris: std::collections::HashMap::new(),
        syndicated_at: std::collections::HashMap::new(),
        share_history: Vec::new(),
        reposted_by_did: None,
        reposted_by_handle: None,
        reply_parent_uri: None,
        reply_parent_handle: None,
        collab_doc_id: None,
    })
}

/// Simple HTML tag stripper for plain-text content extraction.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result.trim().to_string()
}

// ═══════════════════════════════════════════════════════════════════
// COMMUNITY DISCOVERY
// ═══════════════════════════════════════════════════════════════════

/// Summary of a remote Lemmy community for display in ForumScreen.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct RemoteCommunity {
    pub actor_id: String,
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub banner: Option<String>,
    pub subscribers: u64,
    pub posts: u64,
    pub instance: String,
}

/// Discover communities on a remote Lemmy instance.
///
/// Steps:
/// 1. Fetch nodeinfo to confirm it's a Lemmy instance
/// 2. Fetch community list via Lemmy API
/// 3. Map to `RemoteCommunity` for display
pub async fn discover_communities(
    client: &reqwest::Client,
    instance_url: &str,
) -> Result<Vec<RemoteCommunity>, String> {
    let base = instance_url.trim_end_matches('/');

    // Step 1: Validate via nodeinfo
    let nodeinfo_url = format!("{}/.well-known/nodeinfo", base);
    let nodeinfo_resp = client.get(&nodeinfo_url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch nodeinfo: {}", e))?;

    let nodeinfo_links: Value = nodeinfo_resp.json().await
        .map_err(|e| format!("Invalid nodeinfo JSON: {}", e))?;

    let nodeinfo_href = nodeinfo_links.get("links")
        .and_then(|l| l.as_array())
        .and_then(|arr| arr.first())
        .and_then(|link| link.get("href"))
        .and_then(|h| h.as_str())
        .ok_or("No nodeinfo link found")?;

    let nodeinfo: Value = client.get(nodeinfo_href)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch nodeinfo: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Invalid nodeinfo: {}", e))?;

    let software_name = nodeinfo.get("software")
        .and_then(|s| s.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("");

    if !software_name.eq_ignore_ascii_case("lemmy") {
        return Err(format!(
            "Instance is not Lemmy (detected: {}). Use AP discovery for other software.",
            software_name
        ));
    }

    // Step 2: Fetch community list via Lemmy API v3
    let api_url = format!("{}/api/v3/community/list?sort=TopAll&limit=50", base);
    let api_resp = client.get(&api_url)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch community list: {}", e))?;

    let api_data: Value = api_resp.json().await
        .map_err(|e| format!("Invalid community list JSON: {}", e))?;

    let communities = api_data.get("communities")
        .and_then(|c| c.as_array())
        .ok_or("No communities array in response")?;

    // Step 3: Map to RemoteCommunity
    let instance_host = url::Url::parse(base)
        .map(|u| u.host_str().unwrap_or("").to_string())
        .unwrap_or_default();

    let results: Vec<RemoteCommunity> = communities.iter().filter_map(|entry| {
        let community = entry.get("community")?;
        let counts = entry.get("counts")?;

        Some(RemoteCommunity {
            actor_id: community.get("actor_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            name: community.get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            title: community.get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            description: community.get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            icon: community.get("icon")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            banner: community.get("banner")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            subscribers: counts.get("subscribers")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            posts: counts.get("posts")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            instance: instance_host.clone(),
        })
    }).collect();

    Ok(results)
}

// ═══════════════════════════════════════════════════════════════════
// REMOTE COMMUNITY FOLLOW
// ═══════════════════════════════════════════════════════════════════

/// Fetch a remote community's AP actor document and extract key info.
pub async fn fetch_community_actor(
    client: &reqwest::Client,
    actor_url: &str,
) -> Result<Value, String> {
    let resp = client.get(actor_url)
        .header("Accept", "application/activity+json")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch community actor: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Community actor returned HTTP {}", resp.status()));
    }

    resp.json::<Value>().await
        .map_err(|e| format!("Invalid actor JSON: {}", e))
}

/// Build a Follow activity targeting a remote community.
///
/// Returns the Follow activity JSON and the remote inbox URL.
/// Actual signed HTTP delivery requires the relay/server infrastructure
/// (not yet deployed). This constructs the activity and extracts the inbox
/// so the delivery layer can send it when ready.
pub fn build_community_follow(
    local_actor_id: &str,
    remote_actor: &Value,
) -> Result<(Value, String), String> {
    let remote_id = remote_actor.get("id")
        .and_then(|v| v.as_str())
        .ok_or("Remote actor missing 'id'")?;

    let inbox = remote_actor.get("inbox")
        .and_then(|v| v.as_str())
        .or_else(|| {
            // Some implementations use endpoints.sharedInbox
            remote_actor.get("endpoints")
                .and_then(|e| e.get("sharedInbox"))
                .and_then(|v| v.as_str())
        })
        .ok_or("Remote actor missing 'inbox'")?;

    let follow_activity = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Follow",
        "id": format!("{}#follow-{}", local_actor_id, uuid::Uuid::new_v4()),
        "actor": local_actor_id,
        "object": remote_id,
    });

    Ok((follow_activity, inbox.to_string()))
}

/// Create a local placeholder group for a remote community we're following.
///
/// This stores enough info to display the community in the forum screen
/// while we receive federated posts via the Follow relationship.
pub fn remote_community_to_local_group(
    actor: &Value,
    instance: &str,
) -> Result<crate::models::groups::LocalGroup, String> {
    let actor_id = actor.get("id")
        .and_then(|v| v.as_str())
        .ok_or("Actor missing 'id'")?;

    let name = actor.get("name")
        .and_then(|v| v.as_str())
        .or_else(|| actor.get("preferredUsername").and_then(|v| v.as_str()))
        .unwrap_or("Unknown Community")
        .to_string();

    let preferred_username = actor.get("preferredUsername")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let description = actor.get("summary")
        .and_then(|v| v.as_str())
        .map(|s| strip_html_tags(s))
        .unwrap_or_default();

    let icon_url = actor.get("icon")
        .and_then(|v| v.get("url").and_then(|u| u.as_str()).or_else(|| v.as_str()))
        .map(|s| s.to_string());

    let manually_approves = actor.get("manuallyApprovesFollowers")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    Ok(crate::models::groups::LocalGroup {
        group_id: format!("remote:{}", actor_id),
        name,
        slug: preferred_username,
        description,
        visibility: if manually_approves {
            crate::models::groups::GroupVisibility::Closed
        } else {
            crate::models::groups::GroupVisibility::Public
        },
        topic_id: format!("remote:{}", instance),
        owner_did: actor_id.to_string(),
        icon_blob: icon_url,
        banner_blob: None,
        rules: None,
        posting_restricted_to_mods: false,
        member_count: 0,
        created_at: now,
        updated_at: now,
    })
}

// ═══════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lemmy_page_to_bytescape_item_full() {
        let page = serde_json::json!({
            "type": "Page",
            "id": "https://lemmy.ml/post/12345",
            "name": "Interesting Article About Rust",
            "content": "<p>This is a great article about Rust programming.</p>",
            "url": "https://blog.rust-lang.org/2024/rust-is-great",
            "mediaType": "text/html",
            "audience": "https://lemmy.ml/c/rust",
            "attributedTo": "https://lemmy.ml/u/alice",
            "published": "2024-06-15T10:30:00Z",
            "image": {
                "type": "Image",
                "url": "https://lemmy.ml/pictrs/image/thumb.jpg"
            }
        });

        let item = lemmy_page_to_bytescape_item(&page).unwrap();

        assert_eq!(item.title.as_deref(), Some("Interesting Article About Rust"));
        assert_eq!(item.content, "This is a great article about Rust programming.");
        assert_eq!(item.content_html.as_deref(), Some("<p>This is a great article about Rust programming.</p>"));
        assert_eq!(item.link_url.as_deref(), Some("https://blog.rust-lang.org/2024/rust-is-great"));
        assert_eq!(item.source, "activitypub");
        assert_eq!(item.original_id.as_deref(), Some("https://lemmy.ml/post/12345"));
        assert_eq!(item.author_did.as_deref(), Some("https://lemmy.ml/u/alice"));
        assert_eq!(item.preview_img.as_deref(), Some("https://lemmy.ml/pictrs/image/thumb.jpg"));
        assert_eq!(item.item_type, BytescapeType::Article);
        assert_eq!(item.syndication_status, SyndicationStatus::LocalOnly);
    }

    #[test]
    fn test_lemmy_page_minimal() {
        let page = serde_json::json!({
            "type": "Page",
            "id": "https://lemmy.ml/post/99",
        });

        let item = lemmy_page_to_bytescape_item(&page).unwrap();

        assert_eq!(item.original_id.as_deref(), Some("https://lemmy.ml/post/99"));
        assert!(item.title.is_none());
        assert_eq!(item.item_type, BytescapeType::Status);
        assert!(item.content.is_empty());
    }

    #[test]
    fn test_lemmy_page_missing_id_errors() {
        let page = serde_json::json!({
            "type": "Page",
            "name": "No ID",
        });

        assert!(lemmy_page_to_bytescape_item(&page).is_err());
    }

    #[test]
    fn test_strip_html_tags() {
        assert_eq!(strip_html_tags("<p>Hello <b>world</b></p>"), "Hello world");
        assert_eq!(strip_html_tags("plain text"), "plain text");
        assert_eq!(strip_html_tags("<div><a href=\"#\">link</a></div>"), "link");
    }

    #[test]
    fn test_build_community_follow() {
        let remote_actor = serde_json::json!({
            "id": "https://lemmy.ml/c/rust",
            "type": "Group",
            "inbox": "https://lemmy.ml/c/rust/inbox",
        });

        let (follow, inbox) = build_community_follow(
            "https://bytescape.social/@alice",
            &remote_actor,
        ).unwrap();

        assert_eq!(follow["type"], "Follow");
        assert_eq!(follow["actor"], "https://bytescape.social/@alice");
        assert_eq!(follow["object"], "https://lemmy.ml/c/rust");
        assert_eq!(inbox, "https://lemmy.ml/c/rust/inbox");
    }

    #[test]
    fn test_build_community_follow_shared_inbox() {
        let remote_actor = serde_json::json!({
            "id": "https://lemmy.ml/c/programming",
            "type": "Group",
            "endpoints": {
                "sharedInbox": "https://lemmy.ml/inbox"
            }
        });

        let (follow, inbox) = build_community_follow(
            "https://bytescape.social/@bob",
            &remote_actor,
        ).unwrap();

        assert_eq!(follow["type"], "Follow");
        assert_eq!(inbox, "https://lemmy.ml/inbox");
    }

    #[test]
    fn test_build_community_follow_no_inbox_errors() {
        let remote_actor = serde_json::json!({
            "id": "https://lemmy.ml/c/test",
            "type": "Group",
        });

        assert!(build_community_follow("https://bytescape.social/@alice", &remote_actor).is_err());
    }

    #[test]
    fn test_remote_community_to_local_group() {
        let actor = serde_json::json!({
            "id": "https://lemmy.ml/c/rust",
            "type": "Group",
            "name": "Rust Programming",
            "preferredUsername": "rust",
            "summary": "<p>All things Rust</p>",
            "manuallyApprovesFollowers": false,
            "icon": {
                "type": "Image",
                "url": "https://lemmy.ml/pictrs/image/rust-icon.png"
            }
        });

        let group = remote_community_to_local_group(&actor, "lemmy.ml").unwrap();

        assert_eq!(group.group_id, "remote:https://lemmy.ml/c/rust");
        assert_eq!(group.name, "Rust Programming");
        assert_eq!(group.slug, "rust");
        assert_eq!(group.description, "All things Rust");
        assert_eq!(group.visibility, crate::models::groups::GroupVisibility::Public);
        assert_eq!(group.topic_id, "remote:lemmy.ml");
        assert_eq!(group.icon_blob.as_deref(), Some("https://lemmy.ml/pictrs/image/rust-icon.png"));
    }

    #[test]
    fn test_remote_community_to_local_group_closed() {
        let actor = serde_json::json!({
            "id": "https://lemmy.ml/c/private",
            "type": "Group",
            "preferredUsername": "private",
            "manuallyApprovesFollowers": true,
        });

        let group = remote_community_to_local_group(&actor, "lemmy.ml").unwrap();
        assert_eq!(group.visibility, crate::models::groups::GroupVisibility::Closed);
    }
}
