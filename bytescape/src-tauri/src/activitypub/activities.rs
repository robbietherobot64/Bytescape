//! Builder functions for constructing ActivityPub activities as JSON-LD values.
//!
//! Each function constructs a complete `ApActivity` with proper addressing (to/cc
//! with `as:Public`) from minimal parameters (actor ID, target ID, etc.).
//!
//! # How It Works
//!
//! 1. Each function takes minimal parameters (actor ID, target ID) and constructs
//!    a complete activity with proper addressing (to/cc with `as:Public`).
//! 2. Activities are returned as typed `ApActivity` structs which serialize to
//!    JSON-LD for federation delivery.
//! 3. `bytescape_item_to_note` converts Bytescape's internal post format to an
//!    AP Note for syndication — this is the main bridge between local content
//!    and the fediverse.
//! 4. The constructed activities are then delivered by `outbox.rs` or returned
//!    as responses by `inbox.rs`.
//!
//! # Related Files
//!
//! - `outbox.rs` — delivers these activities to remote inboxes
//! - `inbox.rs` — uses `accept`/`reject` for Follow responses
//! - `types.rs` — defines `ApActivity` and `ApObject` structs used here
//! - `actor.rs` — `ApActor` identity referenced by actor IDs

use super::types::{ApObject, ApActivity};
use crate::models::BytescapeItem;

// ═══════════════════════════════════════════════════════════════════════════════
// CONSTANTS
// ═══════════════════════════════════════════════════════════════════════════════

/// The ActivityStreams Public addressing constant.
/// Adding this to the `to` field makes an activity visible to anyone.
pub const AP_PUBLIC: &str = "https://www.w3.org/ns/activitystreams#Public";

// ═══════════════════════════════════════════════════════════════════════════════
// ACTIVITY CONSTRUCTORS
// ═══════════════════════════════════════════════════════════════════════════════

/// Wrap an `ApObject` (Note) in a Create activity for initial publication.
///
/// Copies addressing (to/cc) and published timestamp from the inner note.
/// → Delivered by `outbox.rs::deliver_to_followers`.
pub fn create_note(
    actor_id: &str,
    note: &ApObject,
) -> ApActivity {
    let activity_id = format!("{}/activities/{}", actor_id, uuid::Uuid::new_v4());
    ApActivity {
        id: activity_id,
        activity_type: "Create".to_string(),
        actor: actor_id.to_string(),
        object: Some(serde_json::to_value(note).unwrap_or_default()),
        to: note.to.clone(),
        cc: note.cc.clone(),
        published: note.published.clone(),
    }
}

/// Construct a Follow activity targeting another actor.
///
/// Addressed only to the target (not public). The remote server should
/// respond with Accept or Reject.
pub fn follow(actor_id: &str, target_id: &str) -> ApActivity {
    ApActivity {
        id: format!("{}/activities/{}", actor_id, uuid::Uuid::new_v4()),
        activity_type: "Follow".to_string(),
        actor: actor_id.to_string(),
        object: Some(serde_json::Value::String(target_id.to_string())),
        to: Some(vec![target_id.to_string()]),
        cc: None,
        published: Some(chrono::Utc::now().to_rfc3339()),
    }
}

/// Accept an incoming Follow request. Wraps the original Follow activity
/// as the object so the requester can correlate the response.
/// → Called from `inbox.rs` when processing Follow activities.
pub fn accept(actor_id: &str, original_activity: &serde_json::Value) -> ApActivity {
    ApActivity {
        id: format!("{}/activities/{}", actor_id, uuid::Uuid::new_v4()),
        activity_type: "Accept".to_string(),
        actor: actor_id.to_string(),
        object: Some(original_activity.clone()),
        to: original_activity.get("actor").and_then(|a| a.as_str()).map(|a| vec![a.to_string()]),
        cc: None,
        published: Some(chrono::Utc::now().to_rfc3339()),
    }
}

/// Reject an incoming Follow request. Same structure as Accept but signals denial.
/// → Called from `inbox.rs` when the user declines a Follow.
pub fn reject(actor_id: &str, original_activity: &serde_json::Value) -> ApActivity {
    ApActivity {
        id: format!("{}/activities/{}", actor_id, uuid::Uuid::new_v4()),
        activity_type: "Reject".to_string(),
        actor: actor_id.to_string(),
        object: Some(original_activity.clone()),
        to: original_activity.get("actor").and_then(|a| a.as_str()).map(|a| vec![a.to_string()]),
        cc: None,
        published: Some(chrono::Utc::now().to_rfc3339()),
    }
}

/// Like a remote object. Addressed publicly so the like count is visible.
pub fn like(actor_id: &str, object_id: &str) -> ApActivity {
    ApActivity {
        id: format!("{}/activities/{}", actor_id, uuid::Uuid::new_v4()),
        activity_type: "Like".to_string(),
        actor: actor_id.to_string(),
        object: Some(serde_json::Value::String(object_id.to_string())),
        to: Some(vec![AP_PUBLIC.to_string()]),
        cc: None,
        published: Some(chrono::Utc::now().to_rfc3339()),
    }
}

/// Announce (boost/reblog) a remote object. Addressed to Public with
/// the actor's followers collection in `cc` for proper distribution.
pub fn announce(actor_id: &str, object_id: &str, followers_collection: &str) -> ApActivity {
    ApActivity {
        id: format!("{}/activities/{}", actor_id, uuid::Uuid::new_v4()),
        activity_type: "Announce".to_string(),
        actor: actor_id.to_string(),
        object: Some(serde_json::Value::String(object_id.to_string())),
        to: Some(vec![AP_PUBLIC.to_string()]),
        cc: Some(vec![followers_collection.to_string()]),
        published: Some(chrono::Utc::now().to_rfc3339()),
    }
}

/// Delete a previously published object. Creates a Tombstone as the object
/// per AP spec, signaling to remote servers that the content was removed.
pub fn delete(actor_id: &str, object_id: &str) -> ApActivity {
    let tombstone = serde_json::json!({
        "id": object_id,
        "type": "Tombstone",
    });
    ApActivity {
        id: format!("{}/activities/{}", actor_id, uuid::Uuid::new_v4()),
        activity_type: "Delete".to_string(),
        actor: actor_id.to_string(),
        object: Some(tombstone),
        to: Some(vec![AP_PUBLIC.to_string()]),
        cc: None,
        published: Some(chrono::Utc::now().to_rfc3339()),
    }
}

/// Undo a previous activity (e.g., Undo(Follow) to unfollow, Undo(Like) to unlike).
/// Wraps the original activity as the object.
pub fn undo(actor_id: &str, original_activity: &ApActivity) -> ApActivity {
    ApActivity {
        id: format!("{}/activities/{}", actor_id, uuid::Uuid::new_v4()),
        activity_type: "Undo".to_string(),
        actor: actor_id.to_string(),
        object: Some(serde_json::to_value(original_activity).unwrap_or_default()),
        to: original_activity.to.clone(),
        cc: original_activity.cc.clone(),
        published: Some(chrono::Utc::now().to_rfc3339()),
    }
}

/// Update a previously published object with new content.
/// Copies addressing from the updated object.
pub fn update(actor_id: &str, object: &ApObject) -> ApActivity {
    ApActivity {
        id: format!("{}/activities/{}", actor_id, uuid::Uuid::new_v4()),
        activity_type: "Update".to_string(),
        actor: actor_id.to_string(),
        object: Some(serde_json::to_value(object).unwrap_or_default()),
        to: object.to.clone(),
        cc: object.cc.clone(),
        published: Some(chrono::Utc::now().to_rfc3339()),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// BYTESCAPE → AP CONVERSION
// ═══════════════════════════════════════════════════════════════════════════════

/// Convert a `BytescapeItem` into an AP Note object for federation.
///
/// This is the primary bridge between Bytescape's internal content model and
/// the ActivityPub protocol. Maps fields as follows:
/// - `item.id` → Note ID (under the actor's namespace)
/// - `item.content` → Note content (HTML)
/// - `item.original_timestamp` → `published` (RFC 3339)
///
/// **NOTE**: Media attachments are not yet mapped — images/videos from the
/// BytescapeItem are silently dropped. This needs to be implemented for
/// full media federation support.
pub fn bytescape_item_to_note(item: &BytescapeItem, actor_id: &str) -> ApObject {
    let note_id = format!("{}/notes/{}", actor_id, item.id);
    ApObject {
        id: note_id.clone(),
        object_type: "Note".to_string(),
        content: Some(item.content.clone()),
        source: None,
        attributed_to: Some(actor_id.to_string()),
        to: Some(vec![AP_PUBLIC.to_string()]),
        cc: Some(vec![format!("{}/followers", actor_id)]),
        published: Some(chrono::DateTime::from_timestamp(item.original_timestamp, 0).map(|dt| dt.to_rfc3339()).unwrap_or_default()),
        updated: None,
        attachment: None,
        tag: None,
        in_reply_to: None,
        url: Some(note_id),
        can_reply: None,
        can_search: None,
        expires_at: None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_note() {
        let note = ApObject {
            id: "https://bytescape.social/@alice/notes/1".to_string(),
            object_type: "Note".to_string(),
            content: Some("Hello!".to_string()),
            source: None,
            attributed_to: Some("https://bytescape.social/@alice".to_string()),
            to: Some(vec![AP_PUBLIC.to_string()]),
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
        let activity = create_note("https://bytescape.social/@alice", &note);
        assert_eq!(activity.activity_type, "Create");
        assert!(activity.object.is_some());
    }

    #[test]
    fn test_follow() {
        let activity = follow(
            "https://bytescape.social/@alice",
            "https://mastodon.social/@bob",
        );
        assert_eq!(activity.activity_type, "Follow");
        assert_eq!(
            activity.object.unwrap().as_str().unwrap(),
            "https://mastodon.social/@bob"
        );
    }

    #[test]
    fn test_delete_tombstone() {
        let activity = delete(
            "https://bytescape.social/@alice",
            "https://bytescape.social/@alice/notes/1",
        );
        assert_eq!(activity.activity_type, "Delete");
        let obj = activity.object.unwrap();
        assert_eq!(obj["type"], "Tombstone");
    }

    #[test]
    fn test_announce_addressing() {
        let activity = announce(
            "https://bytescape.social/@alice",
            "https://mastodon.social/@bob/status/123",
            "https://bytescape.social/@alice/followers",
        );
        assert_eq!(activity.activity_type, "Announce");
        assert!(activity.to.unwrap().contains(&AP_PUBLIC.to_string()));
        assert!(activity.cc.unwrap().contains(&"https://bytescape.social/@alice/followers".to_string()));
    }
}
