//! Core ActivityStreams JSON-LD type definitions.

use serde::{Deserialize, Serialize};

pub const AS_CONTEXT: &str = "https://www.w3.org/ns/activitystreams";
pub const SECURITY_CONTEXT: &str = "https://w3id.org/security/v1";

pub fn ap_context() -> serde_json::Value {
    serde_json::json!([AS_CONTEXT, SECURITY_CONTEXT])
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApPublicKey {
    pub id: String,
    pub owner: String,
    pub public_key_pem: String,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApTag {
    #[serde(rename = "type")]
    pub tag_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

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
