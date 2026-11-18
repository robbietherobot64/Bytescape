//! OAuth routes for Bluesky account linking.
//!
//! Flow:
//!   1. Frontend GET /api/auth/{provider}/authorize → returns { url: "..." }
//!   2. Frontend redirects user to that URL
//!   3. User authorizes on external platform
//!   4. External platform redirects to {provider}/callback
//!   5. Server exchanges code for tokens, stores in linked_accounts
//!   6. Server redirects frontend to /oauth/success?provider={p}
//!
//! For unauthenticated contexts (signup), the server also provides
//! a separate /api/auth/bluesky/signup flow.

use axum::{
    extract::{Query, State},
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{oauth, AppState, auth, error::AppError};
use crate::api::auth::request_host;

pub fn routes() -> Router<AppState> {
    Router::new()
        // Bluesky
        .route("/api/auth/bluesky/authorize", get(bluesky_authorize))
        .route("/api/auth/bluesky/callback", get(bluesky_callback))
        .route("/api/oauth/bluesky/check", get(bluesky_check))
        // OAuth-only signup (no auth required)
        .route("/api/auth/oauth-signup/bluesky", post(oauth_signup_bluesky))
        .route("/api/auth/oauth-complete", post(oauth_complete))
        // Mastodon OAuth sign-in (Phase A)
        .route("/api/auth/mastodon/start", post(mastodon_start))
        .route("/api/auth/mastodon/callback", get(mastodon_callback))
        // OAuth token status check
        .route("/api/auth/oauth-status", get(oauth_status))
        // OAuth client metadata (required by AT Protocol)
        .route("/oauth/client-metadata.json", get(client_metadata))
}

// ---------------------------------------------------------------------------
// Auth header helpers
// ---------------------------------------------------------------------------
fn extract_user_id(headers: &axum::http::HeaderMap, secret: &str) -> Result<Uuid, AppError> {
    let auth = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Auth("Missing or invalid Authorization header".into()))?;

    let claims = auth::validate_token(auth, secret)?;
    Uuid::parse_str(&claims.sub).map_err(|_| AppError::Auth("Invalid user ID in token".into()))
}

pub fn extract_user_id_optional(headers: &axum::http::HeaderMap, secret: &str) -> Option<Uuid> {
    let auth = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))?;
    let claims = auth::validate_token(auth, secret).ok()?;
    Uuid::parse_str(&claims.sub).ok()
}

fn get_or_temp_user_id(headers: &axum::http::HeaderMap, secret: &str) -> Uuid {
    extract_user_id_optional(headers, secret).unwrap_or_else(Uuid::new_v4)
}

// ---------------------------------------------------------------------------
// Bluesky OAuth
// ---------------------------------------------------------------------------
#[derive(Serialize)]
struct AuthorizeResponse {
    url: String,
}

/// Forward the user to Bluesky's OAuth authorization page.
/// Bluesky requires PAR (Pushed Authorization Request) + DPoP.
async fn bluesky_authorize(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<AuthorizeResponse>, AppError> {
    let user_id = get_or_temp_user_id(&headers, &state.config.jwt_secret);
    let is_signup = extract_user_id_optional(&headers, &state.config.jwt_secret).is_none();

    let (state_token, dpop_sk, code_verifier) = oauth::bluesky::register_pending(user_id, String::new(), is_signup);

    let code_challenge = {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(code_verifier.as_bytes());
        crate::oauth::base64_url_encode(&hash)
    };

    // Step 1: Send PAR request to get a request_uri
    let http = reqwest::Client::new();
    let request_uri = oauth::bluesky::send_par_request(
        &http,
        &state.config.bluesky_client_id,
        &state.config.bluesky_redirect_uri,
        &state_token,
        &code_challenge,
        &dpop_sk,
    )
    .await
    .map_err(|e| AppError::Internal(format!("Bluesky PAR failed: {}", e)))?;

    // Step 2: Build authorize URL with the PAR-obtained request_uri
    let url = oauth::bluesky::authorize_url_par(
        &state.config.bluesky_client_id,
        &request_uri,
    );

    tracing::info!(user_id = %user_id, "Bluesky OAuth authorize initiated via PAR");
    Ok(Json(AuthorizeResponse { url }))
}

#[derive(Deserialize)]
struct BlueskyCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// Handle the OAuth callback from Bluesky.
async fn bluesky_callback(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(query): Query<BlueskyCallbackQuery>,
) -> Result<axum::response::Response, AppError> {
    if let Some(err) = query.error {
        tracing::error!("Bluesky OAuth error: {} — {}", err, query.error_description.as_deref().unwrap_or(""));
        let err_msg = format!("BLUESKY_AUTH_ERROR: Bluesky returned: {}", err);
        let msg = urlencoding::encode(&err_msg);
        let redirect_url = format!("{}/?oauth_error={}", state.config.frontend_url, msg);
        return Ok(Redirect::to(&redirect_url).into_response());
    }

    let code = query.code.ok_or_else(|| AppError::BadRequest("Missing authorization code".into()))?;
    let state_token = query.state.ok_or_else(|| AppError::BadRequest("Missing state parameter".into()))?;

    let pending = match oauth::bluesky::consume_pending(&state_token) {
        Some(p) => p,
        None => {
            // State already consumed — likely a duplicate callback (browser prefetch, etc.)
            let redirect_url = format!("{}/?oauth_signup=true&provider=bluesky&success=true", state.config.frontend_url);
            return Ok(Redirect::to(&redirect_url).into_response());
        }
    };

    let dpop_sk = ed25519_dalek::SigningKey::from_bytes(
        &pending.dpop_private_key[..32].try_into().map_err(|_| AppError::Internal("Invalid DPoP key".into()))?,
    );

    let http = reqwest::Client::new();
    let token_url = format!("{}/oauth/token", oauth::bluesky::BSKY_AUTH);

    let token = oauth::bluesky::exchange_code(
        &http,
        &token_url,
        &code,
        &pending.code_verifier,
        &state.config.bluesky_redirect_uri,
        &state.config.bluesky_client_id,
        &dpop_sk,
    )
    .await
    .map_err(|e| AppError::OAuth(format!("BLUESKY_TOKEN_EXPIRED: Token exchange failed: {}", e)))?;

    let did = token.sub.as_deref().unwrap_or("unknown");
    eprintln!("[bluesky_callback] token exchange OK — DID: {}, dpop_key_len: {}, access_token_len: {}",
        did, pending.dpop_private_key.len(), token.access_token.len());

    // Resolve the user's actual PDS endpoint from their DID document.
    // OAuth tokens are PDS-specific — they only work on the user's PDS, NOT on bsky.social.
    let pds_url = match oauth::bluesky::resolve_pds_endpoint(&http, did).await {
        Ok(url) => {
            eprintln!("[bluesky_callback] resolved PDS endpoint for {}: {}", did, url);
            url
        }
        Err(e) => {
            eprintln!("[bluesky_callback] WARN: could not resolve PDS for {}: {} — falling back to bsky.social", did, e);
            "https://bsky.social".to_string()
        }
    };

    let existing_user = crate::db::users::UserRow::find_by_id(&state.db, pending.user_id).await?;

    // Use the resolved PDS URL (not bsky.social) for authenticated API calls
    let bsky = oauth::bluesky::BlueskyApiClient::new(
        &token.access_token,
        &pending.dpop_private_key,
        &pds_url,
    ).map_err(|e| AppError::Internal(format!("Bluesky client creation failed: {}", e)))?;

    let handle = match bsky.resolve_handle_from_did(did).await {
        Ok(h) => {
            eprintln!("[bluesky_callback] resolved handle: {} → {}", did, h);
            h
        }
        Err(e) => {
            eprintln!("[bluesky_callback] FAILED to resolve handle for DID {}: {}", did, e);
            did.to_string()
        }
    };

    // A returning user is one whose live (non-deleted) account already owns
    // this Bluesky DID. This must match on every login (the login flow goes
    // through the same signup endpoint), otherwise each re-login mints a new
    // account (rabiezaater1, rabiezaater2, …) and skips onboarding. A DID row
    // left behind by a deleted account is NOT returning — it is cleaned up on
    // the fresh-signup path below.
    let returning_user: Option<uuid::Uuid> = sqlx::query_as::<_, (uuid::Uuid,)>(
        "SELECT la.user_id FROM linked_accounts la
         JOIN users u ON u.id = la.user_id
         WHERE la.provider = 'bluesky' AND la.external_id = $1 AND u.deleted_at IS NULL"
    )
    .bind(did)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .map(|(id,)| id);

    if let Some(user_id) = returning_user {
        // Returning user — update their Bluesky tokens and issue JWT directly
        crate::db::accounts::LinkedAccountRow::create(
            &state.db,
            user_id,
            "bluesky",
            did,
            &handle,
            &token.access_token,
            token.refresh_token.as_deref(),
            token.expires_in.map(|exp| chrono::Utc::now() + chrono::Duration::seconds(exp)),
            Some(&hex::encode(&pending.dpop_private_key)),
            None,
            &serde_json::json!({
                "pds_url": pds_url,
                "client_id": state.config.bluesky_client_id,
                "did": did,
            }),
        )
        .await
        .map_err(AppError::Database)?;

        // Fetch and update Bluesky profile data (avatar, banner, bio, display_name)
        match bsky.get_profile_dpop(did).await {
            Ok(bsky_profile) => {
                let display_name = if bsky_profile.display_name.is_empty() { handle.clone() } else { bsky_profile.display_name };
                let _ = sqlx::query(
                    "UPDATE users SET display_name = $1, bio = $2, avatar_url = $3, header_url = $4 WHERE id = $5"
                )
                .bind(&display_name)
                .bind(&bsky_profile.description)
                .bind(&bsky_profile.avatar_url)
                .bind(&bsky_profile.banner_url)
                .bind(user_id)
                .execute(&state.db)
                .await;
                eprintln!("[bluesky_callback] Updated Bluesky profile for returning user {}", user_id);
            }
            Err(e) => {
                eprintln!("[bluesky_callback] WARN: Failed to fetch Bluesky profile for returning user {}: {}", user_id, e);
            }
        }

        // Clean up old Bluesky imports before importing fresh follows
        let _ = sqlx::query("DELETE FROM follows WHERE follower_id = $1 AND follow_type = 'bluesky_import'")
            .bind(user_id)
            .execute(&state.db)
            .await;

        // Import follows from Bluesky (same as first-time link)
        eprintln!("[bluesky_callback] importing follows for returning user {} DID {} handle {}", user_id, did, handle);
        match bsky.import_follows(&state.db, user_id, did).await {
            Ok(count) => {
                eprintln!("[bluesky_callback] SUCCESS: imported {} Bluesky follows for returning user {}", count, user_id);
                tracing::info!(user_id = %user_id, count, "Bluesky follows imported on re-login");
            }
            Err(e) => {
                eprintln!("[bluesky_callback] FAILED to import Bluesky follows for returning user {}: {}", user_id, e);
                tracing::warn!(user_id = %user_id, error = %e, "Failed to import Bluesky follows on re-login");
            }
        }

        let user = crate::db::users::UserRow::find_by_id(&state.db, user_id).await?
            .ok_or_else(|| AppError::Internal("User from linked account not found".into()))?;
        let manager = auth::RefreshTokenManager { db: state.db.clone() };
        let access_token = auth::create_token(user.id, &user.username, &state.config.jwt_secret)?;
        let (refresh_token, _family) = manager.create(user.id).await?;

        let encoded_access = urlencoding::encode(&access_token);
        let encoded_refresh = urlencoding::encode(&refresh_token);

        tracing::info!(user_id = %user.id, did = %did, handle = %handle, "Bluesky OAuth returning user logged in");

        Ok(crate::api::auth::redirect_with_session_cookies(
            &request_host(&headers),
            &state.config.domain,
            &access_token,
            &refresh_token,
            &format!(
                // Session tokens travel in the URL FRAGMENT (#) — never the
                // query string: fragments are not sent to servers and do not
                // land in browser history, proxy logs or Referer.
                "{}/?oauth_complete=true&returning=true&username={}#access_token={}&refresh_token={}",
                state.config.frontend_url, urlencoding::encode(&user.username), encoded_access, encoded_refresh
            ),
        ))
    } else if let Some(_user) = crate::db::users::UserRow::find_by_id(&state.db, pending.user_id).await? {
        // One-per-network: an authenticated user linking a Bluesky account whose
        // DID is new to them must not already hold a different Bluesky link. (A
        // same-DID re-link lands in the returning-user branch above and is a
        // refresh, not a second link.)
        let prior_bluesky = crate::db::accounts::LinkedAccountRow::count_by_user_provider(
            &state.db,
            pending.user_id,
            "bluesky",
        )
        .await?;
        if prior_bluesky > 0 {
            return Err(AppError::BadRequest(
                "You already have a Bluesky account linked".into(),
            ));
        }
        crate::db::accounts::LinkedAccountRow::create(
            &state.db,
            pending.user_id,
            "bluesky",
            did,
            &handle,
            &token.access_token,
            token.refresh_token.as_deref(),
            token.expires_in.map(|exp| chrono::Utc::now() + chrono::Duration::seconds(exp)),
            Some(&hex::encode(&pending.dpop_private_key)),
            None,
            &serde_json::json!({
                "pds_url": pds_url,
                "client_id": state.config.bluesky_client_id,
                "did": did,
            }),
        )
        .await
        .map_err(AppError::Database)?;

        // Clean up old Bluesky imports before importing fresh follows
        let _ = sqlx::query("DELETE FROM follows WHERE follower_id = $1 AND follow_type = 'bluesky_import'")
            .bind(pending.user_id)
            .execute(&state.db)
            .await;

        // Import follows from Bluesky
        eprintln!("[bluesky_callback] importing follows for user {} DID {} handle {}", pending.user_id, did, handle);
        match bsky.import_follows(&state.db, pending.user_id, did).await {
            Ok(count) => {
                eprintln!("[bluesky_callback] SUCCESS: imported {} Bluesky follows for user {}", count, pending.user_id);
                tracing::info!(user_id = %pending.user_id, count, "Bluesky follows imported");
            }
            Err(e) => {
                eprintln!("[bluesky_callback] FAILED to import Bluesky follows for user {}: {}", pending.user_id, e);
                tracing::warn!(user_id = %pending.user_id, error = %e, "Failed to import Bluesky follows");
            }
        }

        tracing::info!(user_id = %pending.user_id, did = %did, handle = %handle, "Bluesky account linked");
        Ok(Redirect::to(&format!("{}/?provider=bluesky&success=true", state.config.frontend_url)).into_response())
    } else {
        // New user — auto-complete signup with handle-derived username
        // Bluesky OAuth is the sole auth method, no username picker needed
        let base_username = handle
            .split('.')
            .next()
            .unwrap_or("user")
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect::<String>();

        // Ensure username uniqueness
        let mut username = base_username.clone();
        let mut suffix = 1;
        while crate::db::users::UserRow::find_by_username(&state.db, &username).await?.is_some() {
            username = format!("{}{}", base_username, suffix);
            suffix += 1;
        }

        // Generate a random signing key (OAuth-only account, no password login)
        let random_key = uuid::Uuid::new_v4().to_string();
        let public_key_hex = hex::encode(random_key.as_bytes());

        // A stale linked_accounts row for this DID (left behind by a deleted
        // account) would shadow the fresh signup in by-DID lookups — remove it
        // before linking the new account.
        let _ = sqlx::query("DELETE FROM linked_accounts WHERE provider = 'bluesky' AND external_id = $1")
            .bind(did)
            .execute(&state.db)
            .await;

        let (user, _linked) = auth::oauth_signup::consume_temp_token(
            &state.db,
            &auth::oauth_signup::create_temp_token(
                &state.config.jwt_secret,
                "bluesky",
                did,
                &token.access_token,
                token.refresh_token.clone(),
                Some(hex::encode(&pending.dpop_private_key)),
                None,
                serde_json::json!({"did": did, "display_name": handle, "handle": handle, "pds_url": pds_url, "client_id": state.config.bluesky_client_id}),
            )?,
            &username,
            &public_key_hex,
            &state.config.jwt_secret,
            state.config.require_invite_code,
            &pending.invite_code,
            &state.config.domain,
            &state.config.plc_directory,
        ).await?;

        // Mint the ActivityPub actor at account birth (dual-protocol identity),
        // behind AP_ACTORS_ENABLED. Non-fatal: never blocks OAuth callback.
        if crate::ap::ap_actors_enabled() {
            if let Err(e) = crate::ap::issue::issue_actor_for_user(&state, user.id).await {
                tracing::warn!("[OAUTH] AP actor issuance failed (user still created): {e}");
            }
        }

        // Fetch and import Bluesky profile data (avatar, banner, bio, display_name, stats)
        match bsky.get_profile_dpop(did).await {
            Ok(bsky_profile) => {
                let display_name = if bsky_profile.display_name.is_empty() { handle.clone() } else { bsky_profile.display_name };
                let _ = sqlx::query(
                    "UPDATE users SET display_name = $1, bio = $2, avatar_url = $3, header_url = $4 WHERE id = $5"
                )
                .bind(&display_name)
                .bind(&bsky_profile.description)
                .bind(&bsky_profile.avatar_url)
                .bind(&bsky_profile.banner_url)
                .bind(user.id)
                .execute(&state.db)
                .await;
                eprintln!("[bluesky_callback] Imported Bluesky profile for new user {}: display_name={}", user.id, display_name);
            }
            Err(e) => {
                eprintln!("[bluesky_callback] WARN: Failed to fetch Bluesky profile for new user {}: {}", user.id, e);
            }
        }

        // Import follows from Bluesky (the same follow graph the returning-user
        // branch imports — without this a fresh signup has zero follows).
        eprintln!("[bluesky_callback] importing follows for new user {} DID {} handle {}", user.id, did, handle);
        match bsky.import_follows(&state.db, user.id, did).await {
            Ok(count) => {
                eprintln!("[bluesky_callback] SUCCESS: imported {} Bluesky follows for new user {}", count, user.id);
                tracing::info!(user_id = %user.id, count, "Bluesky follows imported (new user)");
            }
            Err(e) => {
                eprintln!("[bluesky_callback] FAILED to import Bluesky follows for new user {}: {}", user.id, e);
                tracing::warn!(user_id = %user.id, error = %e, "Failed to import Bluesky follows (new user)");
            }
        }

        // Issue Bytescape JWT tokens directly
        let manager = auth::RefreshTokenManager { db: state.db.clone() };
        let access_token = auth::create_token(user.id, &user.username, &state.config.jwt_secret)?;
        let (refresh_token, _family) = manager.create(user.id).await?;

        let encoded_access = urlencoding::encode(&access_token);
        let encoded_refresh = urlencoding::encode(&refresh_token);
        let encoded_username = urlencoding::encode(&user.username);

        tracing::info!(user_id = %user.id, did = %did, handle = %handle, username = %user.username, "Bluesky OAuth signup completed (auto-username)");

        Ok(crate::api::auth::redirect_with_session_cookies(
            &request_host(&headers),
            &state.config.domain,
            &access_token,
            &refresh_token,
            &format!(
                // Session tokens in the fragment (see the returning-user
                // redirect above) — kept out of history/proxy logs/Referer.
                "{}/?oauth_complete=true&username={}#access_token={}&refresh_token={}",
                state.config.frontend_url, encoded_username, encoded_access, encoded_refresh
            ),
        ))
    }
}

/// Check if a Bluesky account is linked (called by frontend after OAuth redirect).
async fn bluesky_check(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let accounts = crate::db::accounts::LinkedAccountRow::find_by_provider(&state.db, user_id, "bluesky")
        .await
        .map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({
        "linked": !accounts.is_empty(),
        "accounts": accounts.iter().map(|a| serde_json::json!({
            "id": a.id,
            "external_username": a.external_username,
            "provider": a.provider,
            "created_at": a.created_at,
        })).collect::<Vec<_>>(),
    })))
}



// ---------------------------------------------------------------------------
// OAuth-Only Signup (no auth required)
// ---------------------------------------------------------------------------

/// POST /api/auth/oauth-signup/bluesky — Start Bluesky OAuth flow for new signup.
async fn oauth_signup_bluesky(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Validate the invite at the door; it is consumed when the account is
    // actually created (the callback funnels through consume_temp_token).
    let invite_code = params.get("invite_code").cloned().unwrap_or_default();
    if state.config.require_invite_code {
        if invite_code.is_empty() {
            return Err(AppError::BadRequest("Invite code required".into()));
        }
        if !crate::db::invite_codes::InviteCodeRow::is_valid(&state.db, &invite_code).await? {
            return Err(AppError::BadRequest("Invalid or expired invite code".into()));
        }
    }
    let http = reqwest::Client::new();
    let temp_user_id = uuid::Uuid::new_v4(); // Temporary — real user created at oauth-complete

    let (state_token, dpop_sk, code_verifier) = oauth::bluesky::register_pending(temp_user_id, invite_code, true);

    let code_challenge = {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(code_verifier.as_bytes());
        crate::oauth::base64_url_encode(&hash)
    };

    let request_uri = oauth::bluesky::send_par_request(
        &http,
        &state.config.bluesky_client_id,
        &state.config.bluesky_redirect_uri,
        &state_token,
        &code_challenge,
        &dpop_sk,
    )
    .await
    .map_err(|e| AppError::OAuth(format!("Bluesky PAR failed: {}", e)))?;

    let url = oauth::bluesky::authorize_url_par(
        &state.config.bluesky_client_id,
        &request_uri,
    );

    Ok(Json(serde_json::json!({
        "url": url,
        "state": state_token,
    })))
}

/// POST /api/auth/oauth-complete — Complete OAuth signup with temp token and chosen username.
#[derive(Deserialize)]
struct OauthCompleteReq {
    temp_token: String,
    username: String,
    public_key: String,
    #[serde(default)]
    invite_code: Option<String>,
}

async fn oauth_complete(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<OauthCompleteReq>,
) -> Result<axum::response::Response, AppError> {
    if req.public_key.len() != 64 {
        return Err(AppError::BadRequest("Public key must be 64 hex characters (32 bytes)".into()));
    }

    let (user, linked) = auth::oauth_signup::consume_temp_token(
        &state.db,
        &req.temp_token,
        &req.username,
        &req.public_key,
        &state.config.jwt_secret,
        state.config.require_invite_code,
        req.invite_code.as_deref().unwrap_or(""),
        &state.config.domain,
        &state.config.plc_directory,
    )
    .await?;

    // Mint the ActivityPub actor at account birth (dual-protocol identity),
    // behind AP_ACTORS_ENABLED. Non-fatal: never blocks OAuth completion.
    // Realignment (owner decision 2026-11-14): Mastodon-signed-up users act AS
    // their existing home-instance actor — NO local AP actor is minted for
    // them ("one account, one person"). Bluesky/email signups keep minting.
    if crate::ap::ap_actors_enabled() && linked.provider != "mastodon" {
        if let Err(e) = crate::ap::issue::issue_actor_for_user(&state, user.id).await {
            tracing::warn!("[OAUTH] AP actor issuance failed (user still created): {e}");
        }
    }

    // Backfill the linked Mastodon identity's own AP post history into the
    // feed (behind AP_INGEST_ENABLED). Non-fatal background task: the tokens
    // below are returned synchronously either way.
    //
    // Then import the home-instance FOLLOW GRAPH (live REST read, already
    // covered by the stored `read` grant) — without it a Mastodon sign-up
    // starts with an empty following list and an empty following feed tier.
    // This one runs inline (bounded) so the completion UI can report the count;
    // a slow instance degrades to the background task and reports 0.
    let mut imported_follows: u32 = 0;
    if linked.provider == "mastodon" {
        let instance = linked
            .metadata
            .get("instance")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        crate::ap::backfill::spawn_history_backfill(
            state.db.clone(),
            &linked.external_id,
            &instance,
            user.id,
            state.config.jwt_secret.clone(),
        );
        if !instance.is_empty() {
            let imported = tokio::time::timeout(
                std::time::Duration::from_secs(20),
                crate::ap::follow_import::import_follows(
                    &state.db,
                    user.id,
                    &instance,
                    &linked.access_token,
                    &state.config.jwt_secret,
                ),
            )
            .await;
            match imported {
                Ok(Ok(outcome)) => {
                    imported_follows = outcome.written as u32;
                    crate::ap::follow_import::persist_outcome(&state.db, user.id, &outcome).await;
                    crate::ap::follow_import::spawn_interest_ingest(
                        state.db.clone(),
                        user.id,
                        state.config.jwt_secret.clone(),
                    );
                }
                Ok(Err(e)) => {
                    tracing::warn!(
                        user_id = %user.id,
                        error = %e,
                        "[OAUTH] Mastodon follow import failed (signup continues)"
                    );
                    crate::ap::follow_import::spawn_interest_ingest(
                        state.db.clone(),
                        user.id,
                        state.config.jwt_secret.clone(),
                    );
                }
                Err(_) => {
                    // Timed out: the run wrote whatever resolved so far and the
                    // retry re-runs the whole (idempotent) import.
                    tracing::warn!(
                        user_id = %user.id,
                        "[OAUTH] Mastodon follow import timed out — retrying in background"
                    );
                    crate::ap::follow_import::spawn_follow_import(
                        state.db.clone(),
                        user.id,
                        instance,
                        linked.access_token.clone(),
                        state.config.jwt_secret.clone(),
                    );
                }
            }
        }
    }

    let manager = auth::RefreshTokenManager { db: state.db.clone() };
    let access_token = auth::create_token(user.id, &user.username, &state.config.jwt_secret)?;
    let (refresh_token, _family) = manager.create(user.id).await?;

    let cookies = crate::api::auth::session_set_cookies(
        &request_host(&headers),
        &state.config.domain,
        &access_token,
        &refresh_token,
    );
    Ok(crate::api::auth::attach_cookies(
        Json(serde_json::json!({
            "access_token": access_token,
            "refresh_token": refresh_token,
            "expires_in": 900,
            "token_type": "Bearer",
            "user_id": user.id.to_string(),
            "username": user.username,
            "actor_url": format!("https://{}/@{}", state.config.domain, user.username),
            "linked_provider": linked.provider,
            "linked_external_id": linked.external_id,
            // Mastodon only: how many home-instance follows the sign-up import
            // mirrored into `follows` (0 = nothing to import / still running).
            "imported_follows": imported_follows,
        }))
        .into_response(),
        cookies,
    ))
}

/// GET /api/auth/oauth-status — Check if a linked token is still valid.
async fn oauth_status(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let accounts = crate::db::accounts::LinkedAccountRow::find_by_user(&state.db, user_id)
        .await
        .map_err(AppError::Database)?;

    let mut results = Vec::new();
    for acct in &accounts {
        let valid = match acct.provider.as_str() {
            "bluesky" => {
                // Simple check: try a lightweight API call to see if token is valid
                let http = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(5))
                    .build()
                    .unwrap_or_else(|_| reqwest::Client::new());
                let pds_url = oauth::bluesky::get_pds_url(&acct.metadata);
                let resp = http
                    .get(format!("{}/xrpc/com.atproto.server.getSession", pds_url))
                    .bearer_auth(&acct.access_token)
                    .send()
                    .await;
                resp.is_ok_and(|r| r.status().is_success())
            }
            _ => true,
        };

        results.push(serde_json::json!({
            "provider": acct.provider,
            "external_id": acct.external_id,
            "valid": valid,
        }));
    }

    Ok(Json(serde_json::json!({ "accounts": results })))
}

// ---------------------------------------------------------------------------
// AT Protocol OAuth client metadata (served at /oauth/client-metadata.json)
// ---------------------------------------------------------------------------
async fn client_metadata(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    // Same-origin guarantee: client_uri/logo/tos/policy must share the
    // client_id origin or Bluesky's PAR rejects the client. Derive them from
    // the config's client_id so they track whatever origin it is served from.
    let base = state
        .config
        .bluesky_client_id
        .rsplit_once("/oauth/client-metadata.json")
        .map(|(b, _)| b.to_string())
        .unwrap_or_else(|| format!("https://{}", state.config.domain));
    Json(serde_json::json!({
        "client_id": state.config.bluesky_client_id,
        "client_name": "Bytescape",
        "client_uri": base,
        "logo_uri": format!("{}/logo.png", base),
        "tos_uri": format!("{}/tos", base),
        "policy_uri": format!("{}/privacy", base),
        "redirect_uris": [state.config.bluesky_redirect_uri],
        "scope": oauth::bluesky::BSKY_OAUTH_SCOPES,
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
        "application_type": "native",
        "dpop_bound_access_tokens": true,
    }))
}

// ===================== Mastodon OAuth sign-in (Phase A) =====================

#[derive(Deserialize)]
struct MastodonStartReq {
    instance: String,
    #[serde(default)]
    invite_code: String,
    #[serde(default)]
    username_hint: String,
}

/// POST /api/auth/mastodon/start — register an OAuth app on the user's Mastodon
/// instance and return the authorize URL to redirect to.
async fn mastodon_start(
    State(state): State<AppState>,
    Json(req): Json<MastodonStartReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let instance = oauth::mastodon::normalize_instance(&req.instance)
        .map_err(AppError::BadRequest)?;
    let http = reqwest::Client::new();
    let redirect_uri = format!(
        "{}/api/auth/mastodon/callback",
        state.config.frontend_url.trim_end_matches('/')
    );
    let (state_token, url) = oauth::mastodon::start(
        &http,
        &instance,
        "Bytescape",
        &redirect_uri,
        &req.invite_code,
        &req.username_hint,
    )
    .await
    .map_err(|e| AppError::OAuth(format!("Mastodon start failed: {e}")))?;

    Ok(Json(serde_json::json!({ "url": url, "state": state_token })))
}

/// GET /api/auth/mastodon/callback — complete the Mastodon OAuth flow. If a
/// Bytescape account is already linked to this Mastodon identity, log in and
/// redirect with tokens; otherwise create a temp token and send the user to the
/// frontend to pick a username (reusing the shared `oauth_complete` flow).
async fn mastodon_callback(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<axum::response::Response, AppError> {
    if let Some(err) = query.get("error").cloned() {
        let msg = urlencoding::encode(&format!("MASTODON_AUTH_ERROR: {err}")).to_string();
        return Ok(Redirect::to(&format!("{}/?oauth_error={}", state.config.frontend_url, msg)).into_response());
    }
    let code = query.get("code").cloned().unwrap_or_default();
    let state_token = query.get("state").cloned().unwrap_or_default();
    let pending = oauth::mastodon::consume_pending(&state_token)
        .ok_or_else(|| AppError::BadRequest("Unknown or expired OAuth state".into()))?;

    let http = reqwest::Client::new();
    let redirect_uri = format!(
        "{}/api/auth/mastodon/callback",
        state.config.frontend_url.trim_end_matches('/')
    );
    let token = oauth::mastodon::exchange_code(&http, &pending, &redirect_uri, &code)
        .await
        .map_err(|e| AppError::OAuth(format!("Mastodon token exchange failed: {e}")))?;

    let me = oauth::mastodon::verify_credentials(&http, &pending.instance, &token.access_token)
        .await
        .map_err(|e| AppError::OAuth(format!("Mastodon verify_credentials failed: {e}")))?;

    // The Mastodon identity is the user's real AP identity (`user@instance`).
    // `me.acct` is BARE for an account local to its own instance — the normal
    // sign-in case — so it is qualified here: a bare username is neither
    // globally unique nor WebFinger-able. Rows linked before this fix stored
    // the bare form, so the lookup accepts both; the re-link UPDATE below
    // promotes the row to the canonical form.
    let previous_id = me.acct.trim().trim_start_matches('@').to_string();
    let external_id = if previous_id.contains('@') {
        previous_id.clone()
    } else {
        format!("{previous_id}@{}", pending.instance.to_lowercase())
    };

    // Existing link → log in (return tokens). Otherwise → temp token for signup.
    let existing: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM linked_accounts WHERE provider = 'mastodon' \
         AND (external_id = $1 OR external_id = $2) LIMIT 1",
    )
    .bind(&external_id)
    .bind(&previous_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(format!("link lookup: {e}")))?;

    if let Some((user_id,)) = existing {
        let user = crate::db::users::UserRow::find_by_id(&state.db, user_id)
            .await?
            .ok_or_else(|| AppError::Auth("linked account has no user".into()))?;
        // Re-link (also the re-consent path for read-only grants upgraded to
        // favourites write): refresh the stored token + scope + actor ref, and
        // promote the identity to the canonical `user@instance` form. One
        // Mastodon link per user (`ensure_one_per_network`), so targeting the
        // provider is unambiguous.
        // Token sealed at rest (SEC-1) — only `enc:v1:` ciphertext is bound.
        sqlx::query(
            "UPDATE linked_accounts SET access_token = $1, scope = $2, external_id = $3, \
             metadata = metadata || $4::jsonb, updated_at = now() \
             WHERE user_id = $5 AND provider = 'mastodon'",
        )
        .bind(crate::secret::seal(&token.access_token, &state.config.jwt_secret))
        .bind(&token.scope)
        .bind(&external_id)
        .bind(serde_json::json!({ "instance": pending.instance, "actor_url": me.url }))
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(format!("link refresh: {e}")))?;
        // Backfill this identity's own AP post history in the background
        // (behind AP_INGEST_ENABLED). Non-fatal: the tokens below are returned
        // synchronously regardless of the outcome.
        crate::ap::backfill::spawn_history_backfill(
            state.db.clone(),
            &external_id,
            &pending.instance,
            user.id,
            state.config.jwt_secret.clone(),
        );
        // Re-import the home-instance follow graph (delete-then-reimport, so an
        // unfollow on the instance is reflected), then pull those contacts'
        // content into the feed. Background: the login redirect must not wait
        // on the WebFinger fan-out.
        crate::ap::follow_import::spawn_follow_import(
            state.db.clone(),
            user.id,
            pending.instance.clone(),
            token.access_token.clone(),
            state.config.jwt_secret.clone(),
        );
        let manager = auth::RefreshTokenManager { db: state.db.clone() };
        let access_token = auth::create_token(user.id, &user.username, &state.config.jwt_secret)?;
        let (refresh_token, _f) = manager.create(user.id).await?;
        let enc_a = urlencoding::encode(&access_token);
        let enc_r = urlencoding::encode(&refresh_token);
        let enc_u = urlencoding::encode(&user.username);
        return Ok(crate::api::auth::attach_cookies(
            Redirect::to(&format!(
                // Session tokens in the fragment — kept out of history/proxy
                // logs/Referer (SEC-1).
                "{}/?oauth_complete=true&returning=true&username={enc_u}#access_token={enc_a}&refresh_token={enc_r}",
                state.config.frontend_url,
            ))
            .into_response(),
            crate::api::auth::session_set_cookies(
                &request_host(&headers),
                &state.config.domain,
                &access_token,
                &refresh_token,
            ),
        ));
    }

    // New sign-in → build a temp token and hand off to the shared signup flow.
    let temp_token = auth::oauth_signup::create_temp_token(
        &state.config.jwt_secret,
        "mastodon",
        &external_id,
        &token.access_token,
        None,
        None,
        Some(token.scope.clone()),
        serde_json::json!({
            "did": external_id.clone(),
            "display_name": me.display_name,
            "handle": external_id,
            "instance": pending.instance,
            "avatar": me.avatar,
            // The account's ActivityPub identity IS this actor (realignment:
            // no local AP actor is minted for Mastodon signups).
            "actor_url": me.url,
        }),
    )?;
    let enc_t = urlencoding::encode(&temp_token);
    Ok(Redirect::to(&format!(
        "{}/?oauth_signup=true&provider=mastodon&temp_token={enc_t}",
        state.config.frontend_url,
    ))
    .into_response())
}
