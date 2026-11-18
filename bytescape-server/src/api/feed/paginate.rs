//! Cursor-based pagination for the feed.
//!
//! Cursors are base64-encoded JSON: `{"ca":"<created_at>","uri":"<uri>"}`

use super::FeedPost;

#[derive(Debug, Clone)]
pub struct Cursor {
    pub created_at: String,
    pub uri: String,
}

impl Cursor {
    /// Decode a base64 cursor from the query string.
    pub fn decode(raw: Option<&str>) -> Result<Option<Self>, crate::error::AppError> {
        let Some(raw) = raw else {
            return Ok(None);
        };
        use base64::Engine;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(raw)
            .map_err(|_| crate::error::AppError::BadRequest("Invalid cursor encoding".into()))?;
        let obj: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| crate::error::AppError::BadRequest("Invalid cursor JSON".into()))?;
        let ca = obj
            .get("ca")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::error::AppError::BadRequest("Cursor missing 'ca'".into()))?;
        let uri = obj
            .get("uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::error::AppError::BadRequest("Cursor missing 'uri'".into()))?;
        Ok(Some(Cursor {
            created_at: ca.to_string(),
            uri: uri.to_string(),
        }))
    }

    /// Encode cursor to base64 for the response.
    pub fn encode(&self) -> Option<String> {
        let payload = serde_json::json!({"ca": self.created_at, "uri": self.uri});
        use base64::Engine;
        Some(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string()))
    }

    /// Build a cursor from the oldest post in the list.
    pub fn from_oldest(posts: &[FeedPost]) -> Option<Self> {
        let oldest = posts.last()?;
        Some(Cursor {
            created_at: oldest.created_at.clone(),
            uri: oldest.id.clone(),
        })
    }

    /// SQL condition for cursor-based pagination.
    pub fn sql_condition(&self) -> String {
        format!(
            "AND (created_at, uri) < ('{}'::timestamptz, '{}')",
            self.created_at.replace('\'', "''"),
            self.uri.replace('\'', "''")
        )
    }
}
