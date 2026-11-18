//! Unified error types for the bytescape.social server.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Top-level application error.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("OAuth error: {0}")]
    OAuth(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Lock poisoned: {0}")]
    LockPoisoned(String),
}

#[allow(dead_code)]
pub type AppResult<T> = Result<T, AppError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            AppError::Auth(msg) => (StatusCode::UNAUTHORIZED, "auth_error", msg.clone()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg.clone()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, "forbidden", msg.clone()),
            AppError::Database(e) => {
                tracing::error!("Database error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "A database error occurred".to_string())
            }
            AppError::OAuth(msg) => {
                let (code, status) = if msg.contains("expired") || msg.contains("token") {
                    ("oauth_token_expired", StatusCode::UNAUTHORIZED)
                } else if msg.contains("instance") || msg.contains("Invalid") {
                    ("invalid_oauth_request", StatusCode::BAD_REQUEST)
                } else {
                    ("oauth_upstream_error", StatusCode::BAD_GATEWAY)
                };
                (status, code, msg.clone())
            }
            AppError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", msg.clone())
            }
            AppError::LockPoisoned(msg) => {
                tracing::error!("Lock poisoned: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, "lock_poisoned", msg.clone())
            }
        };

        let body = json!({ "error": code, "message": message });
        (status, Json(body)).into_response()
    }
}

impl From<argon2::password_hash::Error> for AppError {
    fn from(e: argon2::password_hash::Error) -> Self {
        AppError::Internal(format!("Password hash error: {}", e))
    }
}

impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(e: jsonwebtoken::errors::Error) -> Self {
        AppError::Internal(format!("JWT error: {}", e))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Internal(format!("JSON error: {}", e))
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::Internal(format!("HTTP client error: {}", e))
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Internal(s)
    }
}
