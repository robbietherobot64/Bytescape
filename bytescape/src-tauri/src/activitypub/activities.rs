//! Activity constructors for standard ActivityPub activity types.

use super::types::{ApObject, ApActivity};
use crate::models::BytescapeItem;

pub const AP_PUBLIC: &str = "https://www.w3.org/ns/activitystreams#Public";

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

/// Convert a BytescapeItem into an AP Note object.
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
