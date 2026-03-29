//! # activitypub/types.rs — ActivityStreams JSON-LD Type Definitions
//!
//! This module defines the core data types for the ActivityPub federation protocol.
//! ActivityPub is the standard that Mastodon, Lemmy, Pixelfed, and other "fediverse"
//! platforms use to communicate with each other. When Bytescape federates with these
//! platforms, it sends and receives messages in this format.
//!
//! ## Key Concepts
//! - **ActivityStreams**: The JSON format specification that ActivityPub messages use.
//!   Every message has a `@context` pointing to the ActivityStreams namespace.
//! - **JSON-LD**: JSON for Linked Data — a way to embed semantic meaning in JSON.
//!   The `@context` field tells receivers how to interpret the data.
//! - **Objects**: Things that exist (posts/Notes, images, profiles/Actors)
//! - **Activities**: Things that happen (Create, Like, Follow, Announce/boost)
//! - **Collections**: Ordered lists (inbox, outbox, followers list)
//!
//! ## How Bytescape Uses These
//! - **Outbound**: When a user publishes a post to the fediverse, Bytescape creates
//!   an `ApActivity` (type: "Create") wrapping an `ApObject` (type: "Note") and
//!   sends it to followers' inboxes via HTTP POST with cryptographic signatures.
//! - **Inbound**: When a remote server sends us a Follow, Like, or Create activity,
//!   the inbox handler (`activitypub/inbox.rs`) deserializes it into these types.
//!
//! ## Related Files
//! - `activitypub/inbox.rs` — Handles incoming activities from remote servers
//! - `activitypub/outbox.rs` — Creates and sends activities to remote servers
//! - `activitypub/activities.rs` — Builder functions for common activity types
//! - `activitypub/signatures.rs` — HTTP signature creation/verification
//! - `activitypub/actor.rs` — User/group actor profile representation

use serde::{Deserialize, Serialize};

/// The ActivityStreams namespace URI — included in every AP message's @context.
pub const AS_CONTEXT: &str = "https://www.w3.org/ns/activitystreams";
/// The W3C Security namespace — needed for HTTP signature verification fields.
pub const SECURITY_CONTEXT: &str = "https://w3id.org/security/v1";

/// Builds the standard `@context` array for ActivityPub messages.
/// Every outgoing message includes this to tell receivers: "I'm speaking
/// ActivityStreams and I support cryptographic signatures."
pub fn ap_context() -> serde_json::Value {
    serde_json::json!([AS_CONTEXT, SECURITY_CONTEXT])
}

// ═══════════════════════════════════════════════════════════════════
// CORE ACTIVITYPUB TYPES
// These mirror the ActivityStreams specification, adapted for Bytescape's
// use cases. Not every AS field is represented — only the ones we use.
// ═══════════════════════════════════════════════════════════════════

/// An ActivityPub Object — represents a piece of content (usually a post/Note).
///
/// In ActivityPub, everything is either an Object or an Activity. Objects are
/// the "things" — posts (Notes), articles, images, profiles (Actors).
/// Activities are the "verbs" — Create, Like, Follow, Delete, etc.
///
/// This struct covers the most common Note/Article fields. The `object_type`
/// field determines what kind of object it is (e.g., "Note", "Article", "Image").

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApObject {
    pub id: String,
    #[serde(rename = "type")]
    pub object_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<ApSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributed_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment: Option<Vec<ApMediaLink>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<Vec<ApTag>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_reply_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_reply: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_search: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// An ActivityPub Activity — represents something that happened.
///
/// Activities are the "verbs" of ActivityPub:
/// - **Create**: A user published a new post
/// - **Like**: A user liked a post
/// - **Follow**: A user wants to follow another user
/// - **Announce**: A user boosted/reposted someone else's post
/// - **Delete**: A user deleted a post
/// - **Undo**: A user un-liked or un-followed
///
/// The `object` field contains what the activity is about (the post being
/// liked, the user being followed, etc.) as a JSON value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApActivity {
    pub id: String,
    #[serde(rename = "type")]
    pub activity_type: String,
    pub actor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
}

/// An ActivityPub Collection — an ordered list of items.
///
/// Used for: inboxes, outboxes, follower lists, following lists.
/// Collections can be paginated (first/next/prev links) for large lists.
/// The total_items count helps the UI show "42 followers" without loading all.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApCollection {
    pub id: String,
    #[serde(rename = "type")]
    pub collection_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_items: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ordered_items: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_of: Option<String>,
}

/// A public key attached to an ActivityPub actor profile.
/// Used for HTTP Signature verification — when a remote server sends us
/// an activity, we fetch the sender's public key to verify the signature
/// is authentic (not forged by a third party).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApPublicKey {
    pub id: String,
    pub owner: String,
    pub public_key_pem: String,
}

/// A media attachment on an ActivityPub object (image, video, audio).
/// Appears in the `attachment` array of an `ApObject`.
/// The `media_type` field is the MIME type (e.g., "image/jpeg", "video/mp4").
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApMediaLink {
    #[serde(rename = "type")]
    pub media_type_tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

/// A tag on an ActivityPub object — mentions (@user) or hashtags (#topic).
/// The `tag_type` is usually "Mention" or "Hashtag".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApTag {
    #[serde(rename = "type")]
    pub tag_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// The original source text of a post before HTML rendering.
/// Some ActivityPub servers include this so clients can show the
/// raw Markdown/plaintext version alongside the rendered HTML.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApSource {
    pub content: String,
    pub media_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ap_context() {
        let ctx = ap_context();
        let arr = ctx.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].as_str().unwrap(), AS_CONTEXT);
    }

    #[test]
    fn test_ap_object_serialization() {
        let obj = ApObject {
            id: "https://example.com/notes/1".to_string(),
            object_type: "Note".to_string(),
            content: Some("Hello!".to_string()),
            source: None,
            attributed_to: Some("https://example.com/@alice".to_string()),
            to: Some(vec!["https://www.w3.org/ns/activitystreams#Public".to_string()]),
            cc: None,
            published: Some("2025-01-01T00:00:00Z".to_string()),
            updated: None,
            attachment: None,
            tag: None,
            in_reply_to: None,
            url: None,
            can_reply: None,
            can_search: None,
            expires_at: None,
        };
        let json = serde_json::to_value(&obj).unwrap();
        assert_eq!(json["type"], "Note");
        assert!(json.get("cc").is_none());
    }
}
