use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::Json,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::error::AppError;
use crate::AppState;

#[derive(Deserialize)]
struct OgQuery {
    url: String,
}

#[derive(Serialize, Clone, Deserialize)]
pub struct OgMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub url: Option<String>,
    pub site_name: Option<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/og", get(fetch_og))
}

async fn fetch_og(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OgQuery>,
) -> Result<Json<OgMetadata>, AppError> {
    // Validate auth
    let token = headers.get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));
    if token.is_none() {
        return Err(AppError::Auth("Not authenticated".into()));
    }

    let url = query.url;
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(AppError::BadRequest("Invalid URL".into()));
    }

    // Check cache
    {
        if let Ok(cache) = state.og_cache.lock() {
            if let Some(val) = cache.get(&url) {
                if let Some(meta) = serde_json::from_value::<OgMetadata>(val.clone()).ok() {
                    return Ok(Json(meta));
                }
            }
        }
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .user_agent("Bytescape/1.0 (link preview bot)")
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP client error: {}", e)))?;

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(_) => {
            let meta = OgMetadata {
                title: None,
                description: None,
                image: None,
                url: Some(url.clone()),
                site_name: None,
            };
            return Ok(Json(meta));
        }
    };

    let content_type = resp.headers().get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !content_type.contains("text/html") {
        let meta = OgMetadata {
            title: None,
            description: None,
            image: None,
            url: Some(url.clone()),
            site_name: None,
        };
        if let Ok(mut cache) = state.og_cache.lock() {
            cache.insert(url, serde_json::to_value(&meta).unwrap_or_default());
        }
        return Ok(Json(meta));
    }

    let body = resp.text().await.unwrap_or_default();
    let body_lower = body.to_lowercase();
    let meta = OgMetadata {
        title: extract_meta(&body, &body_lower, "og:title")
            .or_else(|| extract_title_tag(&body)),
        description: extract_meta(&body, &body_lower, "og:description")
            .or_else(|| extract_meta(&body, &body_lower, "description")),
        image: extract_meta(&body, &body_lower, "og:image"),
        url: extract_meta(&body, &body_lower, "og:url").or(Some(url.clone())),
        site_name: extract_meta(&body, &body_lower, "og:site_name"),
    };

    if let Ok(mut cache) = state.og_cache.lock() {
        cache.insert(url, serde_json::to_value(&meta).unwrap_or_default());
    }

    Ok(Json(meta))
}

fn extract_meta(html: &str, html_lower: &str, property: &str) -> Option<String> {
    let prop_lower = format!("property=\"{}\"", property);
    let name_lower = format!("name=\"{}\"", property);

    for search in &[&prop_lower, &name_lower] {
        if let Some(pos) = html_lower.find(&search.to_lowercase()) {
            let rest = &html[pos + search.len()..];
            if let Some(content_start) = rest.find("content=\"") {
                let content = &rest[content_start + 9..];
                if let Some(end) = content.find('"') {
                    let value = content[..end].trim().to_string();
                    if !value.is_empty() {
                        return Some(value);
                    }
                }
            }
        }
    }
    None
}

fn extract_title_tag(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    if let Some(start) = lower.find("<title>") {
        let content_start = start + 7;
        if let Some(end) = html[content_start..].find("</title>") {
            let title = html[content_start..content_start + end].trim().to_string();
            if !title.is_empty() {
                return Some(title);
            }
        }
    }
    None
}