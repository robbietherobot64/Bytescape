//! API — Vault routes (media gallery + file browser).

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{auth, db, error::AppError, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/vault", get(vault_index))
        .route("/api/vault/media/{id}", get(media_detail))
}

fn extract_user_id(headers: &HeaderMap, secret: &str) -> Result<Uuid, AppError> {
    let auth = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Auth("Missing or invalid Authorization header".into()))?;
    let claims = auth::validate_token(auth, secret)?;
    Uuid::parse_str(&claims.sub).map_err(|_| AppError::Auth("Invalid user ID in token".into()))
}

#[derive(Debug, Deserialize)]
pub struct VaultQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub source: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MediaView {
    pub id: String,
    pub storage_url: String,
    pub thumbnail_url: Option<String>,
    pub mime_type: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub alt_text: Option<String>,
    pub file_size: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct VaultResponse {
    pub items: Vec<MediaView>,
    pub has_more: bool,
}

// GET /api/vault?limit=20&offset=0&source=
// Returns paginated media items for the authenticated user.
// Includes both uploaded media (media table) and imported post media (content.media_urls).
async fn vault_index(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<VaultQuery>,
) -> Result<Json<VaultResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.offset.unwrap_or(0);
    let items = db::media::MediaRow::vault(&state.db, user_id, limit, offset, query.source.as_deref()).await?;
    let mut views: Vec<MediaView> = items.into_iter().map(|m| MediaView {
        id: m.id.to_string(),
        storage_url: m.storage_url,
        thumbnail_url: m.thumbnail_url,
        mime_type: m.mime_type,
        width: m.width,
        height: m.height,
        alt_text: m.alt_text,
        file_size: m.file_size,
        created_at: m.created_at.to_rfc3339(),
    }).collect();

    // Also include media from imported posts (content.media_urls)
    if let Ok(posts) = sqlx::query_as::<_, crate::db::posts::PostRow>(
        "SELECT * FROM posts WHERE user_id = $1 AND is_imported = true ORDER BY original_ts DESC LIMIT 500"
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    {
        for post in posts {
            if let Some(media_urls) = post.content.get("media_urls").and_then(|v| v.as_array()) {
                let post_ts = post.original_ts;
                for url_val in media_urls {
                    if let Some(url) = url_val.as_str() {
                        let mime_type = if url.ends_with(".mp4") || url.ends_with(".mov") || url.ends_with(".webm") {
                            "video/mp4".to_string()
                        } else if url.ends_with(".gif") {
                            "image/gif".to_string()
                        } else if url.ends_with(".png") {
                            "image/png".to_string()
                        } else {
                            "image/jpeg".to_string()
                        };
                        views.push(MediaView {
                            id: format!("imported-{}", post.id),
                            storage_url: url.to_string(),
                            thumbnail_url: None,
                            mime_type,
                            width: None,
                            height: None,
                            alt_text: None,
                            file_size: None,
                            created_at: post_ts.to_rfc3339(),
                        });
                    }
                }
            }
        }
    }

    // Source 3: media from the user's ATProto presence — images in their
    // Bluesky posts. Owned by the PDS, fetched live from the public AppView.
    // Unauthenticated: the account's feed is public, and the AppView rejects
    // DPoP-bound OAuth tokens ("DPoP not supported") anyway.
    if let Ok(Some(acct)) = crate::db::accounts::LinkedAccountRow::find_by_provider(&state.db, user_id, "bluesky")
        .await
        .map(|v| v.into_iter().next())
    {
        let actor = urlencoding::encode(&acct.external_id);
        let url = format!(
            "https://api.bsky.app/xrpc/app.bsky.feed.getAuthorFeed?actor={}&limit=100&filter=posts_and_author_threads",
            actor
        );
        if let Ok(resp) = reqwest::Client::new().get(&url).send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(feed) = json["feed"].as_array() {
                    for item in feed {
                        let post = &item["post"];
                        // Own posts only: skip repost wrappers and anything not
                        // authored by this account. (Quote posts embed the
                        // quoted post's media under embed.record — never read
                        // that; only embed.media is the author's own.)
                        if item.get("reason").is_some() {
                            continue;
                        }
                        if post["author"]["did"].as_str() != Some(acct.external_id.as_str()) {
                            continue;
                        }
                        let created_at = post["record"]["createdAt"]
                            .as_str()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| post["indexedAt"].as_str().unwrap_or("").to_string());
                        let rkey = post["uri"]
                            .as_str()
                            .unwrap_or("")
                            .rsplit('/')
                            .next()
                            .unwrap_or("")
                            .to_string();
                        // Direct image embeds and the media half of quote posts.
                        let mut n = 0;
                        let direct = post["embed"]["images"].as_array();
                        let quoted = post["embed"]["media"]["images"].as_array();
                        for images in direct.into_iter().chain(quoted) {
                            for img in images {
                                let fullsize = img["fullsize"].as_str().unwrap_or("");
                                if fullsize.is_empty() {
                                    continue;
                                }
                                views.push(MediaView {
                                    id: format!("atproto-{}-{}", rkey, n),
                                    storage_url: fullsize.to_string(),
                                    thumbnail_url: img["thumb"].as_str().map(|s| s.to_string()),
                                    mime_type: "image/jpeg".to_string(),
                                    width: None,
                                    height: None,
                                    alt_text: img["alt"].as_str().map(|s| s.to_string()),
                                    file_size: None,
                                    created_at: created_at.clone(),
                                });
                                n += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    views.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let has_more = views.len() >= limit as usize;
    Ok(Json(VaultResponse { items: views, has_more }))
}

// GET /api/vault/media/{id}
// Returns full media record for a single item.
async fn media_detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<MediaView>, AppError> {
    let _user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let row = sqlx::query_as::<_, db::media::MediaRow>(
        "SELECT * FROM media WHERE id = $1"
    ).bind(id).fetch_optional(&state.db).await?
        .ok_or_else(|| AppError::NotFound("Media not found".into()))?;
    Ok(Json(MediaView {
        id: row.id.to_string(),
        storage_url: row.storage_url,
        thumbnail_url: row.thumbnail_url,
        mime_type: row.mime_type,
        width: row.width,
        height: row.height,
        alt_text: row.alt_text,
        file_size: row.file_size,
        created_at: row.created_at.to_rfc3339(),
    }))
}
