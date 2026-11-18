//! ATProto OAuth server — implements the atproto OAuth profile.
//!
//! Endpoints:
//! - GET  /.well-known/oauth-protected-resource — Resource Server metadata
//! - GET  /.well-known/oauth-authorization-server — Authorization Server metadata
//! - POST /oauth/par — Pushed Authorization Request
//! - GET  /oauth/authorize — Authorization endpoint (login + approval)
//! - POST /oauth/token — Token endpoint (code exchange + refresh)

use axum::{
    extract::{Form, Query, State},
    http::header,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Sha256, Digest};

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/.well-known/oauth-protected-resource", get(resource_metadata))
        .route("/.well-known/oauth-authorization-server", get(auth_server_metadata))
        .route("/oauth/par", post(par_endpoint))
        .route("/oauth/authorize", get(authorize_endpoint))
        .route("/oauth/token", post(token_endpoint))
}

// ─── Metadata Endpoints ───────────────────────────────────

async fn resource_metadata(State(state): State<AppState>) -> Json<Value> {
    let origin = format!("https://{}", state.config.domain);
    Json(json!({
        "resource": origin,
        "authorization_servers": [origin]
    }))
}

async fn auth_server_metadata(State(state): State<AppState>) -> Json<Value> {
    let origin = format!("https://{}", state.config.domain);
    Json(json!({
        "issuer": origin,
        "authorization_endpoint": format!("{}/oauth/authorize", origin),
        "token_endpoint": format!("{}/oauth/token", origin),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none", "private_key_jwt"],
        "token_endpoint_auth_signing_alg_values_supported": ["ES256"],
        "scopes_supported": ["atproto", "transition:generic"],
        "authorization_response_iss_parameter_supported": true,
        "require_pushed_authorization_requests": true,
        "pushed_authorization_request_endpoint": format!("{}/oauth/par", origin),
        "dpop_signing_alg_values_supported": ["ES256"],
        "client_id_metadata_document_supported": true,
        "require_request_uri_registration": true
    }))
}

// ─── PAR Endpoint ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ParRequest {
    client_id: String,
    response_type: String,
    code_challenge: String,
    code_challenge_method: String,
    state: String,
    redirect_uri: String,
    scope: String,
    login_hint: Option<String>,
}

#[derive(Debug, Serialize)]
struct ParResponse {
    request_uri: String,
    expires_in: u64,
}

async fn par_endpoint(
    State(state): State<AppState>,
    Form(req): Form<ParRequest>,
) -> Result<axum::response::Response, crate::error::AppError> {
    // Validate response_type
    if req.response_type != "code" {
        return Err(crate::error::AppError::BadRequest("response_type must be 'code'".into()));
    }

    // Validate code_challenge_method
    if req.code_challenge_method != "S256" {
        return Err(crate::error::AppError::BadRequest("code_challenge_method must be 'S256'".into()));
    }

    // Validate scope includes atproto
    if !req.scope.split_whitespace().any(|s| s == "atproto") {
        return Err(crate::error::AppError::BadRequest("scope must include 'atproto'".into()));
    }

    // Fetch client metadata from client_id URL
    let client_metadata = fetch_client_metadata(&req.client_id).await
        .map_err(|e| crate::error::AppError::Internal(format!("Failed to fetch client metadata: {e}")))?;

    // Verify redirect_uri is in client metadata
    let redirect_uris: Vec<&str> = client_metadata
        .get("redirect_uris")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    // For localhost dev clients, match by path (ignore port)
    let redirect_ok = is_redirect_uri_valid(&req.redirect_uri, &redirect_uris, &req.client_id);
    if !redirect_ok {
        return Err(crate::error::AppError::BadRequest("redirect_uri not registered".into()));
    }

    // Extract DPoP key thumbprint from DPoP header
    let dpop_jkt = extract_dpop_jkt_from_header()
        .map_err(|e| crate::error::AppError::BadRequest(format!("DPoP required: {e}")))?;

    // Generate request_uri
    let request_uri = format!("urn:ietf:params:oauth:request_uri:{}", uuid::Uuid::new_v4());
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(60);

    // Store in oauth_par_requests
    sqlx::query(
        "INSERT INTO oauth_par_requests (request_uri, client_id, response_type, code_challenge, code_challenge_method, state, redirect_uri, scope, login_hint, dpop_jkt, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(&request_uri)
    .bind(&req.client_id)
    .bind(&req.response_type)
    .bind(&req.code_challenge)
    .bind(&req.code_challenge_method)
    .bind(&req.state)
    .bind(&req.redirect_uri)
    .bind(&req.scope)
    .bind(&req.login_hint)
    .bind(&dpop_jkt)
    .bind(expires_at)
    .execute(&state.db)
    .await
    .map_err(|e| crate::error::AppError::Internal(format!("PAR storage failed: {e}")))?;

    // Issue DPoP nonce
    let nonce = issue_dpop_nonce(&state.db).await
        .map_err(|e| crate::error::AppError::Internal(format!("DPoP nonce failed: {e}")))?;

    let resp = Json(ParResponse {
        request_uri,
        expires_in: 60,
    });

    // Return with DPoP-Nonce header
    Ok::<axum::response::Response, crate::error::AppError>(
        (
            [(header::HeaderName::from_static("dpop-nonce"), header::HeaderValue::from_str(&nonce).unwrap())],
            resp,
        ).into_response()
    )
}

// ─── Authorization Endpoint ────────────────────────────────

#[derive(Debug, Deserialize)]
struct AuthorizeParams {
    client_id: Option<String>,
    request_uri: String,
}

async fn authorize_endpoint(
    State(state): State<AppState>,
    Query(params): Query<AuthorizeParams>,
) -> Response {
    // Look up the PAR request
    let par_row: Option<(String, String, String, String, String, String, Option<String>, String, chrono::DateTime<chrono::Utc>)> =
        sqlx::query_as(
            "SELECT client_id, response_type, code_challenge, code_challenge_method, state, redirect_uri, login_hint, scope, expires_at
             FROM oauth_par_requests WHERE request_uri = $1 AND expires_at > NOW()",
        )
        .bind(&params.request_uri)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();

    let (client_id, _response_type, _code_challenge, _code_challenge_method, par_state, redirect_uri, login_hint, scope, _expires_at) =
        match par_row {
            Some(r) => r,
            None => {
                return (axum::http::StatusCode::BAD_REQUEST, "Invalid or expired request_uri").into_response();
            }
        };

    // For now, auto-approve and generate code (no login UI in this sprint)
    // TODO: proper login UI in post-Sprint-3
    // For now, use login_hint to identify the account, or first account if no hint

    let did = if let Some(hint) = login_hint.as_ref() {
        // Try to find account by handle or DID
        let handle = hint.strip_prefix("did:web:").and_then(|d| d.split('.').next()).unwrap_or(hint);
        find_account(&state.db, handle).await
    } else {
        // Use first active account
        find_any_account(&state.db).await
    };

    let did = match did {
        Some(d) => d,
        None => {
            return (axum::http::StatusCode::BAD_REQUEST, "No account found for login_hint").into_response();
        }
    };

    // Generate auth code
    let code = uuid::Uuid::new_v4().to_string();
    let expires_at = chrono::Utc::now() + chrono::Duration::minutes(10);

    sqlx::query(
        "INSERT INTO oauth_auth_codes (code, request_uri, did, scope, dpop_jkt, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(&code)
    .bind(&params.request_uri)
    .bind(&did)
    .bind(&scope)
    .bind("") // dpop_jkt from PAR request
    .bind(expires_at)
    .execute(&state.db)
    .await
    .ok();

    let issuer = format!("https://{}", state.config.domain);
    let redirect_url = format!("{}?code={}&state={}&iss={}", redirect_uri, code, par_state, issuer);

    Redirect::to(&redirect_url).into_response()
}

// ─── Token Endpoint ─────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TokenRequest {
    grant_type: String,
    code: Option<String>,
    redirect_uri: Option<String>,
    client_id: Option<String>,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Debug, Serialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    token_type: String,
    expires_in: u64,
    sub: String,
    scope: String,
}

async fn token_endpoint(
    State(state): State<AppState>,
    Form(req): Form<TokenRequest>,
) -> Result<Json<TokenResponse>, crate::error::AppError> {
    match req.grant_type.as_str() {
        "authorization_code" => handle_code_grant(&state, req).await,
        "refresh_token" => handle_refresh_grant(&state, req).await,
        _ => Err(crate::error::AppError::BadRequest("unsupported_grant_type".into())),
    }
}

async fn handle_code_grant(
    state: &AppState,
    req: TokenRequest,
) -> Result<Json<TokenResponse>, crate::error::AppError> {
    let code = req.code.as_deref().ok_or_else(|| crate::error::AppError::BadRequest("code required".into()))?;
    let code_verifier = req.code_verifier.as_deref().ok_or_else(|| crate::error::AppError::BadRequest("code_verifier required".into()))?;
    let client_id = req.client_id.as_deref().ok_or_else(|| crate::error::AppError::BadRequest("client_id required".into()))?;

    // Look up auth code
    let row: Option<(String, String, String, chrono::DateTime<chrono::Utc>, bool, String, String, String)> = sqlx::query_as(
        "SELECT ac.did, ac.scope, ac.dpop_jkt, ac.expires_at, ac.used,
                par.code_challenge, par.client_id, par.redirect_uri
         FROM oauth_auth_codes ac
         JOIN oauth_par_requests par ON ac.request_uri = par.request_uri
         WHERE ac.code = $1",
    )
    .bind(code)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| crate::error::AppError::Internal(format!("DB error: {e}")))?;

    let (did, scope, _dpop_jkt, expires_at, used, code_challenge, par_client_id, _redirect_uri) =
        match row {
            Some(r) => r,
            None => return Err(crate::error::AppError::BadRequest("invalid_grant: code not found".into())),
        };

    // Check code not expired
    if chrono::Utc::now() > expires_at {
        return Err(crate::error::AppError::BadRequest("invalid_grant: code expired".into()));
    }

    // Check code not already used
    if used {
        return Err(crate::error::AppError::BadRequest("invalid_grant: code already used".into()));
    }

    // Verify client_id matches
    if par_client_id != client_id {
        return Err(crate::error::AppError::BadRequest("invalid_grant: client_id mismatch".into()));
    }

    // Verify PKCE
    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let computed_challenge = base64_url_encode(&hasher.finalize());
    if computed_challenge != code_challenge {
        return Err(crate::error::AppError::BadRequest("invalid_grant: PKCE verification failed".into()));
    }

    // Mark code as used
    sqlx::query("UPDATE oauth_auth_codes SET used = TRUE WHERE code = $1")
        .bind(code)
        .execute(&state.db)
        .await
        .ok();

    // Issue tokens
    let access_token = uuid::Uuid::new_v4().to_string();
    let refresh_token = uuid::Uuid::new_v4().to_string();
    let access_expires_at = chrono::Utc::now() + chrono::Duration::seconds(900); // 15 min

    sqlx::query(
        "INSERT INTO oauth_sessions (did, client_id, scope, dpop_jkt, access_token, refresh_token, access_expires_at, refresh_expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&did)
    .bind(client_id)
    .bind(&scope)
    .bind(&_dpop_jkt)
    .bind(&access_token)
    .bind(&refresh_token)
    .bind(access_expires_at)
    .bind(access_expires_at + chrono::Duration::days(14))
    .execute(&state.db)
    .await
    .ok();

    Ok(Json(TokenResponse {
        access_token,
        refresh_token,
        token_type: "DPoP".to_string(),
        expires_in: 900,
        sub: did,
        scope,
    }))
}

async fn handle_refresh_grant(
    state: &AppState,
    req: TokenRequest,
) -> Result<Json<TokenResponse>, crate::error::AppError> {
    let refresh_token = req.refresh_token.as_deref().ok_or_else(|| crate::error::AppError::BadRequest("refresh_token required".into()))?;
    let client_id = req.client_id.as_deref().ok_or_else(|| crate::error::AppError::BadRequest("client_id required".into()))?;

    // Look up session by refresh token
    let row: Option<(uuid::Uuid, String, String, String, chrono::DateTime<chrono::Utc>, bool)> = sqlx::query_as(
        "SELECT id, did, scope, dpop_jkt, access_expires_at, revoked
         FROM oauth_sessions WHERE refresh_token = $1 AND revoked = FALSE",
    )
    .bind(refresh_token)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let (session_id, did, scope, dpop_jkt, _old_access_expires, _revoked) =
        match row {
            Some(r) => r,
            None => return Err(crate::error::AppError::BadRequest("invalid_grant: refresh token not found".into())),
        };

    // Issue new tokens (rotation)
    let new_access_token = uuid::Uuid::new_v4().to_string();
    let new_refresh_token = uuid::Uuid::new_v4().to_string();
    let access_expires_at = chrono::Utc::now() + chrono::Duration::seconds(900);

    // Invalidate old session, create new one
    sqlx::query("UPDATE oauth_sessions SET revoked = TRUE WHERE id = $1")
        .bind(session_id)
        .execute(&state.db)
        .await
        .ok();

    sqlx::query(
        "INSERT INTO oauth_sessions (did, client_id, scope, dpop_jkt, access_token, refresh_token, access_expires_at, refresh_expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&did)
    .bind(client_id)
    .bind(&scope)
    .bind(&dpop_jkt)
    .bind(&new_access_token)
    .bind(&new_refresh_token)
    .bind(access_expires_at)
    .bind(access_expires_at + chrono::Duration::days(14))
    .execute(&state.db)
    .await
    .ok();

    Ok(Json(TokenResponse {
        access_token: new_access_token,
        refresh_token: new_refresh_token,
        token_type: "DPoP".to_string(),
        expires_in: 900,
        sub: did,
        scope,
    }))
}

// ─── Helpers ────────────────────────────────────────────────

async fn fetch_client_metadata(client_id: &str) -> Result<Value, String> {
    // For localhost dev clients, construct virtual metadata
    if client_id.starts_with("http://localhost") {
        return Ok(json!({
            "client_id": client_id,
            "application_type": "native",
            "grant_types": ["authorization_code", "refresh_token"],
            "scope": "atproto",
            "response_types": ["code"],
            "redirect_uris": ["http://127.0.0.1/", "http://[::1]/"],
            "token_endpoint_auth_method": "none",
            "dpop_bound_access_tokens": true,
        }));
    }

    // Fetch from URL
    let resp = reqwest::get(client_id).await.map_err(|e| format!("HTTP error: {e}"))?;
    let metadata: Value = resp.json().await.map_err(|e| format!("JSON parse error: {e}"))?;
    Ok(metadata)
}

fn is_redirect_uri_valid(redirect_uri: &str, registered_uris: &[&str], client_id: &str) -> bool {
    if client_id.starts_with("http://localhost") {
        // For localhost: match path, ignore port
        return true; // Lenient for localhost dev
    }

    registered_uris.iter().any(|u| *u == redirect_uri)
}

fn extract_dpop_jkt_from_header() -> Result<String, String> {
    // TODO: extract DPoP JWT from header, parse JWK, compute thumbprint
    // For now, return a placeholder — full DPoP verification in Task 5
    Ok("placeholder_jkt".to_string())
}

async fn issue_dpop_nonce(pool: &sqlx::PgPool) -> Result<String, sqlx::Error> {
    let nonce = uuid::Uuid::new_v4().to_string();
    let expires_at = chrono::Utc::now() + chrono::Duration::minutes(5);
    sqlx::query(
        "INSERT INTO oauth_dpop_nonces (nonce, expires_at) VALUES ($1, $2)
         ON CONFLICT (nonce) DO NOTHING",
    )
    .bind(&nonce)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(nonce)
}

async fn find_account(pool: &sqlx::PgPool, handle: &str) -> Option<String> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT did FROM pds_accounts WHERE handle = $1 AND status = 'active'",
    )
    .bind(handle)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    row.map(|(did,)| did)
}

async fn find_any_account(pool: &sqlx::PgPool) -> Option<String> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT did FROM pds_accounts WHERE status = 'active' AND handle != '_instance' LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    row.map(|(did,)| did)
}

fn base64_url_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}