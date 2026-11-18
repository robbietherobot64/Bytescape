//! API — Mute/Block account routes.
//!
//! POST /api/mute     — mute an account
//! POST /api/unmute   — unmute an account
//! GET  /api/muted    — list muted accounts

use axum::{
    extract::State,
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{auth, error::AppError, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/mute", post(mute_account))
        .route("/api/unmute", post(unmute_account))
        .route("/api/muted", get(list_muted))
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

// ── Request/Response types ──

#[derive(Debug, Deserialize)]
struct MuteRequest {
    did: Option<String>,
    handle: Option<String>,
    #[serde(rename = "type", default = "default_mute_type")]
    mute_type: String,
}

fn default_mute_type() -> String {
    "mute".to_string()
}

#[derive(Debug, Deserialize)]
struct UnmuteRequest {
    did: String,
}

#[derive(Debug, Serialize)]
struct MutedAccount {
    id: String,
    muted_did: Option<String>,
    muted_handle: Option<String>,
    muted_type: String,
    muted_at: String,
}

#[derive(Debug, Serialize)]
struct MutedListResponse {
    accounts: Vec<MutedAccount>,
}

// ── Handlers ──

/// POST /api/mute — mute an account
async fn mute_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MuteRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let did = body.did.as_deref().filter(|s| !s.is_empty());
    let handle = body.handle.as_deref().filter(|s| !s.is_empty());

    if did.is_none() && handle.is_none() {
        return Err(AppError::BadRequest("Either 'did' or 'handle' is required".into()));
    }

    // If DID is missing but handle provided, try to resolve via appview_did_handle
    let resolved_did = if let Some(d) = did {
        Some(d.to_string())
    } else if let Some(h) = handle {
        // Try to resolve handle to DID
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT did FROM appview_did_handle WHERE handle = $1 LIMIT 1"
        )
        .bind(h)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
        row.map(|(d,)| d)
    } else {
        None
    };

    let mute_type = if body.mute_type == "block" { "block" } else { "mute" };

    sqlx::query(
        r#"INSERT INTO muted_accounts (user_id, muted_did, muted_handle, muted_type)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (user_id, muted_did) DO UPDATE SET
             muted_handle = EXCLUDED.muted_handle,
             muted_type = EXCLUDED.muted_type,
             muted_at = now()"#
    )
    .bind(user_id)
    .bind(&resolved_did)
    .bind(handle)
    .bind(mute_type)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    tracing::info!("[MUTE] user {} muted {} (type={})", user_id, resolved_did.as_deref().unwrap_or("unknown"), mute_type);

    Ok(Json(serde_json::json!({
        "ok": true,
        "muted_did": resolved_did,
        "muted_handle": handle,
        "type": mute_type
    })))
}

/// POST /api/unmute — unmute an account
async fn unmute_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<UnmuteRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    if body.did.is_empty() {
        return Err(AppError::BadRequest("'did' is required".into()));
    }

    let result = sqlx::query(
        "DELETE FROM muted_accounts WHERE user_id = $1 AND muted_did = $2"
    )
    .bind(user_id)
    .bind(&body.did)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    tracing::info!("[MUTE] user {} unmuted {} (rows={})", user_id, body.did, result.rows_affected());

    Ok(Json(serde_json::json!({
        "ok": true,
        "unmuted_did": body.did
    })))
}

/// GET /api/muted — list muted accounts for the current user
async fn list_muted(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MutedListResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let rows: Vec<(String, Option<String>, Option<String>, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT id::text, muted_did, muted_handle, muted_type, muted_at FROM muted_accounts WHERE user_id = $1 ORDER BY muted_at DESC"
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    let accounts = rows.into_iter().map(|(id, did, handle, mtype, at)| {
        MutedAccount {
            id,
            muted_did: did,
            muted_handle: handle,
            muted_type: mtype,
            muted_at: at.to_rfc3339(),
        }
    }).collect();

    Ok(Json(MutedListResponse { accounts }))
}
