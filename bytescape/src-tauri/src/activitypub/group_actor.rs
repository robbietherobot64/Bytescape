//! AP Group Actor — federate local Bytescape groups as ActivityPub `Group` actors.
//!
//! Enables interop with Lemmy communities, PieFed groups, and any AP-compatible
//! software that understands the `Group` actor type.
//!
//! Key differences from Person actors:
//! - `type: "Group"` instead of `"Person"`
//! - `manuallyApprovesFollowers` derived from group visibility
//! - `discoverable` false for Private groups
//! - Posts are relayed as `Announce` activities (Lemmy pattern)

use serde_json::json;
use crate::models::groups::{LocalGroup, GroupVisibility, GroupPost};
use crate::models::BytescapeItem;
use super::types::{ap_context, ApPublicKey};
use super::actor::{AP_DOMAIN, ed25519_public_key_to_pem};
use super::activities::AP_PUBLIC;

/// Generate an AP Group actor JSON-LD document for a local group.
///
/// The resulting document conforms to the ActivityPub spec for `Group` actors
/// and is compatible with Lemmy's community representation.
pub fn group_to_ap_actor(group: &LocalGroup, public_key: Option<&ApPublicKey>) -> serde_json::Value {
    let base_url = format!("https://{}", AP_DOMAIN);
    let actor_id = group_actor_id(&group.group_id);

    let mut doc = json!({
        "@context": ap_context(),
        "id": actor_id,
        "type": "Group",
        "preferredUsername": group.slug,
        "name": group.name,
        "inbox": format!("{}/inbox", actor_id),
        "outbox": format!("{}/outbox", actor_id),
        "followers": format!("{}/followers", actor_id),
        "manuallyApprovesFollowers": group.visibility != GroupVisibility::Public,
        "discoverable": group.visibility != GroupVisibility::Private,
        "sensitive": false,
        "url": format!("{}/group/{}", base_url, group.slug),
    });

    if !group.description.is_empty() {
        doc["summary"] = json!(group.description);
    }

    if let Some(ref icon) = group.icon_blob {
        doc["icon"] = json!({
            "type": "Image",
            "url": format!("{}/media/{}", base_url, icon),
        });
    }

    if let Some(ref banner) = group.banner_blob {
        doc["image"] = json!({
            "type": "Image",
            "url": format!("{}/media/{}", base_url, banner),
        });
    }

    if let Some(pk) = public_key {
        doc["publicKey"] = json!({
            "id": pk.id,
            "owner": pk.owner,
            "publicKeyPem": pk.public_key_pem,
        });
    }

    doc
}

/// Generate the Group's outbox as an OrderedCollection.
///
/// Contains recent posts wrapped as `Create/Note` activities, which Lemmy
/// and other AP software use to populate community feeds.
pub fn group_outbox(
    group_id: &str,
    posts: &[GroupPost],
    items: &[BytescapeItem],
    total_items: u64,
) -> serde_json::Value {
    let actor_id = group_actor_id(group_id);
    let outbox_id = format!("{}/outbox", actor_id);

    // Build ordered items from posts + hydrated items
    let ordered_items: Vec<serde_json::Value> = posts.iter()
        .filter_map(|gp| {
            items.iter().find(|item| item.id == gp.item_id).map(|item| {
                create_note_for_group(item, &actor_id)
            })
        })
        .collect();

    json!({
        "@context": ap_context(),
        "id": outbox_id,
        "type": "OrderedCollection",
        "totalItems": total_items,
        "orderedItems": ordered_items,
    })
}

/// Generate the Group's followers Collection.
///
/// Lists all active member DIDs as follower references.
pub fn group_followers(
    group_id: &str,
    follower_actor_ids: &[String],
) -> serde_json::Value {
    let actor_id = group_actor_id(group_id);
    let followers_id = format!("{}/followers", actor_id);

    json!({
        "@context": ap_context(),
        "id": followers_id,
        "type": "OrderedCollection",
        "totalItems": follower_actor_ids.len(),
        "orderedItems": follower_actor_ids,
    })
}

/// Wrap a BytescapeItem posted to a group as a Create/Note activity
/// with `audience` pointing to the group actor.
///
/// This is the format Lemmy expects for community posts: a Create activity
/// containing a Note/Page with the community as audience.
pub fn create_note_for_group(item: &BytescapeItem, group_actor_id: &str) -> serde_json::Value {
    let note_id = format!("{}/posts/{}", group_actor_id, item.id);
    let author = item.author_did.as_deref().unwrap_or(group_actor_id);

    let mut note = json!({
        "id": note_id,
        "type": "Note",
        "content": item.content,
        "attributedTo": author,
        "audience": group_actor_id,
        "to": [AP_PUBLIC],
        "cc": [format!("{}/followers", group_actor_id)],
        "published": chrono::DateTime::from_timestamp_millis(item.imported_at as i64)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default(),
    });

    if let Some(ref title) = item.title {
        note["name"] = json!(title);
    }

    // Attach media URLs if present
    if !item.media_blobs.is_empty() {
        let base_url = format!("https://{}", AP_DOMAIN);
        let attachments: Vec<serde_json::Value> = item.media_blobs.iter()
            .enumerate()
            .map(|(i, blob_ref)| {
                let mime = item.media_mimes.get(i)
                    .map(|s| s.as_str())
                    .unwrap_or("application/octet-stream");
                json!({
                    "type": "Document",
                    "mediaType": mime,
                    "url": format!("{}/media/{}", base_url, blob_ref),
                })
            })
            .collect();
        note["attachment"] = json!(attachments);
    }

    // Wrap in Create activity
    let activity_id = format!("{}/activities/{}", group_actor_id, uuid::Uuid::new_v4());
    json!({
        "id": activity_id,
        "type": "Create",
        "actor": author,
        "object": note,
        "audience": group_actor_id,
        "to": [AP_PUBLIC],
        "cc": [format!("{}/followers", group_actor_id)],
        "published": chrono::DateTime::from_timestamp_millis(item.imported_at as i64)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default(),
    })
}

/// Wrap a group post in an Announce activity (the group relays to followers).
///
/// This is how Lemmy distributes community posts: the Group actor wraps the
/// original Create activity in an Announce and delivers to all followers.
pub fn announce_to_followers(
    group_actor_id: &str,
    original_activity: &serde_json::Value,
    followers_collection: &str,
) -> serde_json::Value {
    let activity_id = format!("{}/activities/{}", group_actor_id, uuid::Uuid::new_v4());
    json!({
        "@context": ap_context(),
        "id": activity_id,
        "type": "Announce",
        "actor": group_actor_id,
        "object": original_activity,
        "to": [AP_PUBLIC],
        "cc": [followers_collection],
        "published": chrono::Utc::now().to_rfc3339(),
    })
}

/// Generate an ed25519 keypair for a group and return (public_key, secret_key_hex).
///
/// Each group needs its own signing keypair for AP HTTP signatures.
/// The secret key should be stored in the encrypted keychain under
/// `group_key:{group_id}`.
pub fn generate_group_keypair(group_id: &str) -> (ApPublicKey, String) {
    let secret = iroh::SecretKey::generate(&mut rand::rng());
    let public = secret.public();
    let actor_id = group_actor_id(group_id);
    let key_id = format!("{}#main-key", actor_id);
    let pem = ed25519_public_key_to_pem(public.as_bytes());

    let pk = ApPublicKey {
        id: key_id,
        owner: actor_id,
        public_key_pem: pem,
    };

    (pk, hex::encode(secret.to_bytes()))
}

/// Canonical group actor ID from a group_id.
pub fn group_actor_id(group_id: &str) -> String {
    format!("https://{}/group/{}", AP_DOMAIN, group_id)
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::groups::*;

    fn make_test_group(vis: GroupVisibility) -> LocalGroup {
        LocalGroup {
            group_id: "test-group-1".to_string(),
            name: "Test Community".to_string(),
            slug: "test-community".to_string(),
            description: "A test community for AP federation".to_string(),
            visibility: vis,
            topic_id: "topic:fediverse".to_string(),
            owner_did: "did:key:owner123".to_string(),
            icon_blob: Some("blobhash123".to_string()),
            banner_blob: None,
            rules: None,
            posting_restricted_to_mods: false,
            member_count: 42,
            created_at: 1000,
            updated_at: 2000,
        }
    }

    fn make_test_item() -> BytescapeItem {
        BytescapeItem {
            id: "item-abc".to_string(),
            original_timestamp: 1710000000000,
            content: "Hello from the group!".to_string(),
            media_paths: vec![],
            source: "Manual".to_string(),
            path: String::new(),
            imported_at: 1710000000000,
            title: Some("Test Post".to_string()),
            original_id: None,
            author_did: Some("did:key:author1".to_string()),
            author_handle: Some("alice".to_string()),
            author_avatar: None,
            tagged_people: vec![],
            location: None,
            is_liked: false,
            like_count: None,
            reply_count: None,
            repost_count: None,
            quote_count: None,
            item_type: crate::models::BytescapeType::Status,
            url: None,
            link_url: None,
            content_html: None,
            preview_img: None,
            media_blobs: vec!["blob1".to_string()],
            thumbnail_blobs: vec![],
            media_mimes: vec!["image/jpeg".to_string()],
            syndication_status: crate::models::SyndicationStatus::LocalOnly,
            syndicated_uri: None,
            audiences: vec![],
            syndicated_uris: std::collections::HashMap::new(),
            syndicated_at: std::collections::HashMap::new(),
            share_history: vec![],
            reposted_by_did: None,
            reposted_by_handle: None,
            reply_parent_uri: None,
            reply_parent_handle: None,
            collab_doc_id: None,
        }
    }

    #[test]
    fn test_group_to_ap_actor_public() {
        let group = make_test_group(GroupVisibility::Public);
        let doc = group_to_ap_actor(&group, None);

        assert_eq!(doc["type"], "Group");
        assert_eq!(doc["preferredUsername"], "test-community");
        assert_eq!(doc["name"], "Test Community");
        assert_eq!(doc["manuallyApprovesFollowers"], false);
        assert_eq!(doc["discoverable"], true);
        assert!(doc["inbox"].as_str().unwrap().ends_with("/inbox"));
        assert!(doc["outbox"].as_str().unwrap().ends_with("/outbox"));
        assert!(doc["icon"]["url"].as_str().unwrap().contains("blobhash123"));
        assert!(doc.get("image").is_none()); // no banner
    }

    #[test]
    fn test_group_to_ap_actor_closed() {
        let group = make_test_group(GroupVisibility::Closed);
        let doc = group_to_ap_actor(&group, None);

        assert_eq!(doc["type"], "Group");
        assert_eq!(doc["manuallyApprovesFollowers"], true);
        assert_eq!(doc["discoverable"], true); // Closed is still discoverable
    }

    #[test]
    fn test_group_to_ap_actor_private() {
        let group = make_test_group(GroupVisibility::Private);
        let doc = group_to_ap_actor(&group, None);

        assert_eq!(doc["type"], "Group");
        assert_eq!(doc["manuallyApprovesFollowers"], true);
        assert_eq!(doc["discoverable"], false); // Private is not discoverable
    }

    #[test]
    fn test_create_note_for_group() {
        let item = make_test_item();
        let actor_id = "https://bytescape.social/group/test-group-1";
        let result = create_note_for_group(&item, actor_id);

        assert_eq!(result["type"], "Create");
        assert_eq!(result["actor"], "did:key:author1");
        assert_eq!(result["audience"], actor_id);

        let note = &result["object"];
        assert_eq!(note["type"], "Note");
        assert_eq!(note["content"], "Hello from the group!");
        assert_eq!(note["name"], "Test Post");
        assert_eq!(note["audience"], actor_id);
        assert!(note["attachment"].as_array().unwrap().len() == 1);
    }

    #[test]
    fn test_announce_wrapping() {
        let original = json!({
            "id": "https://example.com/activities/123",
            "type": "Create",
            "actor": "https://example.com/@alice",
            "object": {
                "type": "Note",
                "content": "Hello",
            }
        });

        let actor_id = "https://bytescape.social/group/test-group-1";
        let followers = "https://bytescape.social/group/test-group-1/followers";
        let result = announce_to_followers(actor_id, &original, followers);

        assert_eq!(result["type"], "Announce");
        assert_eq!(result["actor"], actor_id);
        assert_eq!(result["object"]["type"], "Create");
        assert!(result["cc"].as_array().unwrap().iter().any(|v| v == followers));
        assert!(result["to"].as_array().unwrap().iter().any(|v| v == AP_PUBLIC));
    }

    #[test]
    fn test_group_outbox() {
        let item = make_test_item();
        let post = GroupPost {
            group_id: "test-group-1".to_string(),
            item_id: "item-abc".to_string(),
            pinned: false,
            shared_by: None,
            posted_at: 1710000000000,
        };

        let result = group_outbox("test-group-1", &[post], &[item], 1);

        assert_eq!(result["type"], "OrderedCollection");
        assert_eq!(result["totalItems"], 1);
        let items = result["orderedItems"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["type"], "Create");
    }

    #[test]
    fn test_group_followers() {
        let followers = vec![
            "https://mastodon.social/@alice".to_string(),
            "https://lemmy.ml/u/bob".to_string(),
        ];
        let result = group_followers("test-group-1", &followers);

        assert_eq!(result["type"], "OrderedCollection");
        assert_eq!(result["totalItems"], 2);
        let items = result["orderedItems"].as_array().unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_generate_group_keypair() {
        let (pk, secret_hex) = generate_group_keypair("my-group");

        assert!(pk.id.contains("my-group"));
        assert!(pk.id.ends_with("#main-key"));
        assert!(pk.public_key_pem.contains("BEGIN PUBLIC KEY"));
        assert_eq!(secret_hex.len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn test_group_actor_id() {
        let id = group_actor_id("test-123");
        assert_eq!(id, "https://bytescape.social/group/test-123");
    }
}
