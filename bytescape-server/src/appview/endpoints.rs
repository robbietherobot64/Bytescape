//! Local AppView XRPC endpoints — serve search and feed queries from local index.

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::AppState;
use crate::error::AppError;

#[derive(Deserialize)]
pub struct SearchPostsQuery {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 { 25 }

#[derive(Deserialize)]
pub struct AuthorFeedQuery {
    pub actor: String,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

/// GET /xrpc/app.bsky.feed.searchPosts — search local AppView index.
pub async fn search_posts(
    State(state): State<AppState>,
    Query(q): Query<SearchPostsQuery>,
) -> Result<Json<Value>, AppError> {
    let limit = q.limit.min(100) as i64;
    let pattern = format!("%{}%", q.q.replace('%', "\\%").replace('_', "\\_"));
    
    let rows = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>, Option<chrono::DateTime<chrono::Utc>>)>(
        r#"SELECT uri, cid, author_did, author_handle, text, created_at
           FROM appview_posts
           WHERE text ILIKE $1 AND text IS NOT NULL AND text != ''
           ORDER BY created_at DESC NULLS LAST
           LIMIT $2"#,
    )
    .bind(&pattern)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(format!("Search failed: {}", e)))?;
    
    let posts: Vec<Value> = rows.iter().map(|(uri, cid, did, handle, text, created)| {
        json!({
            "uri": uri,
            "cid": cid,
            "author": {
                "did": did,
                "handle": handle,
            },
            "record": {
                "text": text,
                "createdAt": created.map(|c| c.to_rfc3339()),
            },
        })
    }).collect();
    
    Ok(Json(json!({ "posts": posts })))
}

/// GET /xrpc/app.bsky.feed.getAuthorFeed — fetch posts by author from local index.
pub async fn get_author_feed(
    State(state): State<AppState>,
    Query(q): Query<AuthorFeedQuery>,
) -> Result<Json<Value>, AppError> {
    let limit = q.limit.min(100) as i64;
    
    let rows = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>, Option<chrono::DateTime<chrono::Utc>>)>(
        r#"SELECT uri, cid, author_did, author_handle, text, created_at
           FROM appview_posts
           WHERE author_did = $1 OR author_handle = $1
           ORDER BY created_at DESC NULLS LAST
           LIMIT $2"#,
    )
    .bind(&q.actor)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(format!("Author feed failed: {}", e)))?;
    
    let feed: Vec<Value> = rows.iter().map(|(uri, cid, did, handle, text, created)| {
        json!({
            "post": {
                "uri": uri,
                "cid": cid,
                "author": {
                    "did": did,
                    "handle": handle,
                },
                "record": {
                    "text": text,
                    "createdAt": created.map(|c| c.to_rfc3339()),
                },
            }
        })
    }).collect();
    
    Ok(Json(json!({ "feed": feed })))
}

/// Get routes for AppView endpoints.
pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/xrpc/app.bsky.feed.searchPosts", axum::routing::get(search_posts))
        .route("/xrpc/app.bsky.feed.getAuthorFeed", axum::routing::get(get_author_feed))
}