//! API — Post composer routes.
//!
//! POST /api/posts/create    — Create a new post with optional cross-posting
//! POST /api/media/upload    — Upload media file (image or video)

mod create;
mod delete;
mod media;
mod repost;

use axum::{
    extract::DefaultBodyLimit,
    http::HeaderMap,
    routing::{delete, post},
    Router,
};
use serde::Serialize;
use uuid::Uuid;

use crate::error::AppError;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/posts/create", post(create::create_post_handler))
        .route("/api/posts/repost", post(repost::repost_handler))
        .route("/api/media/upload", post(media::media_upload_handler))
        .route("/api/posts/{id}", delete(delete::delete_post_handler))
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024)) // Allow up to 100MB uploads
}

pub(crate) fn extract_user_id(headers: &HeaderMap, secret: &str) -> Result<Uuid, AppError> {
    let auth = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Auth("Missing or invalid Authorization header".into()))?;

    let claims = crate::auth::validate_token(auth, secret)?;
    Uuid::parse_str(&claims.sub).map_err(|_| AppError::Auth("Invalid user ID in token".into()))
}

/// The home instance recorded at Mastodon link time (`metadata.instance`).
/// M-linked users post AND delete through their home instance (design.md
/// Decision 1). Local to the posts module so the cross-post paths need not
/// reach across into `feed`.
pub(super) fn mastodon_home_instance(
    acct: &crate::db::accounts::LinkedAccountRow,
) -> Result<String, AppError> {
    acct.metadata
        .get("instance")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| AppError::BadRequest("the Mastodon link has no home instance recorded".into()))
}

#[derive(Serialize)]
pub(super) struct CrossPostResult {
    pub platform: String,
    pub ok: bool,
    pub url: Option<String>,
    pub error: Option<String>,
}
