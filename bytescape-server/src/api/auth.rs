//! REST API — Authentication routes.
//! POST /api/auth/signup      — Create account with email + password (pending state)
//! POST /api/auth/verify-email — Verify email with 6-digit code
//! POST /api/auth/login       — Authenticate and receive JWT + refresh token
//! POST /api/auth/google      — Sign in / sign up with Google OAuth
//! POST /api/auth/refresh     — Refresh access token using refresh token

use axum::{Json, Router, extract::State, response::IntoResponse, routing::{post, get}};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::{AppState, auth, error::AppError};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/signup", post(signup))
        .route("/api/auth/verify-email", post(verify_email))
        .route("/api/auth/login", post(login))
        .route("/api/auth/google", post(google_auth))
        .route("/api/auth/refresh", post(refresh))
        .route("/api/auth/session", get(session))
        .route("/api/auth/logout", post(logout))
        .route("/api/admin/invite-codes", get(list_invite_codes).post(create_invite_code))
        .route("/api/admin/logs", get(get_logs))
        .route("/admin/logs", get(log_viewer_page))
}

#[derive(Deserialize)]
pub struct SignupRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub public_key: String,
    #[serde(default)]
    pub captcha_token: String,
    #[serde(default)]
    pub invite_code: String,
}

#[derive(Serialize)]
pub struct SignupResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub token_type: &'static str,
    pub user_id: String,
    pub username: String,
    pub actor_url: String,
}

#[derive(Serialize)]
pub struct AuthTokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub token_type: &'static str,
}

/// GET /api/auth/session — identity from cookie (injected by
/// `cookie_session_middleware`) or an explicit Bearer header. 401 otherwise.
/// `dids` = the caller's AT identities (native + linked) — the events UI
/// gates author-only controls on DID match (spec: events/participate).
async fn session(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Auth("Not signed in".into()))?;
    let claims = auth::validate_token(token, &state.config.jwt_secret)?;
    let uid = Uuid::parse_str(&claims.sub)
        .map_err(|_| AppError::Auth("Invalid user ID in token".into()))?;
    let dids: Vec<String> =
        crate::api::stories::own_dids(&state.db, uid).await;
    Ok(Json(serde_json::json!({
        "user_id": claims.sub,
        "username": claims.username,
        "dids": dids,
    })))
}

/// POST /api/auth/logout — clears the family cookies (HttpOnly, so the
/// client cannot clear them itself). localStorage tokens are the client's to
/// clear; family-wide logout crosses the SSO hop (spec: events/session).
async fn logout(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> impl axum::response::IntoResponse {
    let cookies = clear_session_cookies(&request_host(&headers), &state.config.domain);
    attach_cookies(Json(serde_json::json!({ "ok": true })).into_response(), cookies)
}

// ─── Shared family session — cookie ⇄ Bearer (spec: events/session) ───
//
// The same JWT also travels as an HttpOnly cookie scoped to `.DOMAIN`, so
// one login covers bytescape.social and events.bytescape.social. The
// middleware injects the cookie as a Bearer header (Bearer-first ordering —
// existing handlers keep reading Authorization unchanged) and rejects
// cross-site mutating requests authenticated by cookie alone (defense in
// depth beyond SameSite=Lax).

pub const SESSION_COOKIE: &str = "bytescape_session";
pub const REFRESH_COOKIE: &str = "bytescape_refresh";

/// Attribute suffix shared by set and clear — the browser keys cookies on
/// name+domain, so a clear must carry the domain it was set with.
fn session_cookie_attrs(host: &str, domain: &str) -> String {
    let host = host.split(':').next().unwrap_or(host);
    let mut attrs = String::new();
    // Domain=.domain only when the request host lives under `domain` (prod
    // subdomains share the session). Host-only otherwise: on localhost a
    // Domain attribute would be rejected outright — and host-only cookies
    // ignore ports, so both dev servers share one login (spec: local parity).
    if !domain.is_empty() && domain != "localhost"
        && (host == domain || host.ends_with(&format!(".{}", domain)))
    {
        attrs.push_str(&format!("; Domain=.{}", domain));
    }
    if !(host.starts_with("localhost") || host.starts_with("127.0.0.1")) {
        attrs.push_str("; Secure");
    }
    attrs
}

/// The two Set-Cookie values for a freshly issued family session
/// (access lives 900s, refresh 30d — matching the token semantics).
pub fn session_set_cookies(host: &str, domain: &str, access: &str, refresh: &str) -> [String; 2] {
    let attrs = session_cookie_attrs(host, domain);
    [
        format!("bytescape_session={access}; HttpOnly; SameSite=Lax; Path=/; Max-Age=900{attrs}"),
        format!("bytescape_refresh={refresh}; HttpOnly; SameSite=Lax; Path=/; Max-Age=2592000{attrs}"),
    ]
}

/// Expired twins of the session cookies (same attrs, Max-Age=0).
pub fn clear_session_cookies(host: &str, domain: &str) -> [String; 2] {
    let attrs = session_cookie_attrs(host, domain);
    [
        format!("bytescape_session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0{attrs}"),
        format!("bytescape_refresh=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0{attrs}"),
    ]
}

/// Host header without the port (cookies key on host, not origin).
pub fn request_host(headers: &axum::http::HeaderMap) -> String {
    headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

fn cookie_value(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    for value in headers.get_all(axum::http::header::COOKIE) {
        let Ok(s) = value.to_str() else { continue };
        for pair in s.split(';') {
            let mut kv = pair.trim().splitn(2, '=');
            let (Some(k), Some(v)) = (kv.next(), kv.next()) else { continue };
            if k.trim() == name {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Origin is family when its host is `domain`, a subdomain of it, or a
/// loopback dev server (any port).
fn is_family_origin(origin: &str, domain: &str) -> bool {
    let host = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))
        .unwrap_or(origin);
    let host = host.split('/').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    host == domain
        || (!domain.is_empty() && host.ends_with(&format!(".{}", domain)))
        || host == "localhost"
        || host.starts_with("127.0.0.1")
}

/// Attach the family Set-Cookie headers to a response by APPEND.
/// (`[(HeaderName, T); N]` response parts insert — same key twice means the
/// second cookie clobbers the first; Set-Cookie must append.)
/// Cookies are computed in their own statement first: borrows of the tokens
/// end there, so the body can move them afterwards.
pub(crate) fn attach_cookies(
    mut resp: axum::response::Response,
    cookies: [String; 2],
) -> axum::response::Response {
    for c in cookies {
        if let Ok(v) = axum::http::HeaderValue::from_str(&c) {
            resp.headers_mut()
                .append(axum::http::header::SET_COOKIE, v);
        }
    }
    resp
}

/// Wrap an OAuth return redirect with the family session cookies.
pub(crate) fn redirect_with_session_cookies(
    host: &str,
    domain: &str,
    access: &str,
    refresh: &str,
    location: &str,
) -> axum::response::Response {
    attach_cookies(
        axum::response::Redirect::to(location).into_response(),
        session_set_cookies(host, domain, access, refresh),
    )
}

/// Cookie → Bearer injection + CSRF Origin check. Registered once in
/// main.rs (Bearer-first: a request with its own Authorization header is
/// untouched, so existing behavior is byte-identical).
pub async fn cookie_session_middleware(
    axum::extract::State(state): axum::extract::State<AppState>,
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let has_bearer = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("Bearer "));
    if !has_bearer {
        if let Some(token) = cookie_value(req.headers(), SESSION_COOKIE) {
            let mutating = !matches!(
                *req.method(),
                axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS
            );
            if mutating {
                // SameSite=Lax already withholds the cookie from cross-site
                // writes; this Origin check is the explicit contract
                // (spec: cross-site forged write rejected). Absent Origin =
                // non-browser client — allowed.
                if let Some(origin) = req
                    .headers()
                    .get(axum::http::header::ORIGIN)
                    .and_then(|v| v.to_str().ok())
                {
                    if !is_family_origin(origin, &state.config.domain) {
                        return (
                            axum::http::StatusCode::FORBIDDEN,
                            "cross-origin cookie-authenticated request rejected",
                        )
                            .into_response();
                    }
                }
            }
            if let Ok(value) = axum::http::HeaderValue::from_str(&format!("Bearer {token}")) {
                req.headers_mut()
                    .insert(axum::http::header::AUTHORIZATION, value);
            }
        }
    }
    next.run(req).await
}

#[derive(Deserialize)]
pub struct VerifyEmailRequest {
    pub email: String,
    pub code: String,
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    /// Body token wins (web keeps it in localStorage). When absent, the
    /// refresh falls back to the family cookie — an idle family origin can
    /// re-authenticate itself (spec: idle origin resumes).
    #[serde(default)]
    pub refresh_token: String,
}

/// Validate a hCaptcha token by POSTing to hcaptcha.com/siteverify.
async fn validate_captcha(secret: &str, token: &str) -> Result<(), AppError> {
    if secret.is_empty() {
        // CAPTCHA not configured — allow through (dev mode)
        return Ok(());
    }
    let client = reqwest::Client::new();
    let resp = client
        .post("https://hcaptcha.com/siteverify")
        .form(&[("secret", secret), ("response", token)])
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("CAPTCHA verification failed: {}", e)))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to parse CAPTCHA response: {}", e)))?;

    let success = body["success"].as_bool().unwrap_or(false);
    if !success {
        let err_codes = body["error-codes"].as_array().map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        });
        let detail = err_codes.unwrap_or_else(|| "invalid captcha".to_string());
        return Err(AppError::BadRequest(format!("CAPTCHA verification failed: {}", detail)));
    }

    Ok(())
}

#[axum::debug_handler]
async fn signup(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<SignupRequest>,
) -> Result<axum::response::Response, AppError> {
    // Validate invite code (if required)
    if state.config.require_invite_code {
        if req.invite_code.is_empty() {
            return Err(AppError::BadRequest("Invite code required".into()));
        }
        if !crate::db::invite_codes::InviteCodeRow::is_valid(&state.db, &req.invite_code).await? {
            return Err(AppError::BadRequest("Invalid or expired invite code".into()));
        }
    }

    // Validate CAPTCHA (skip when invite codes are required — invite code is the bot gate)
    if !state.config.require_invite_code {
        if !req.captcha_token.is_empty() {
            validate_captcha(&state.config.hcaptcha_secret, &req.captcha_token).await?;
        } else if !state.config.hcaptcha_secret.is_empty() {
            return Err(AppError::BadRequest("CAPTCHA verification required".into()));
        }
    }

    if req.username.len() < 2 || req.username.len() > 32 {
        return Err(AppError::BadRequest("Username must be 2-32 characters".into()));
    }
    if !req.username.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Err(AppError::BadRequest("Username may only contain letters, numbers, hyphens, underscores".into()));
    }
    if req.public_key.len() != 64 {
        return Err(AppError::BadRequest("Public key must be 64 hex characters (32 bytes)".into()));
    }

    // Check if username or email already taken
    if crate::db::users::UserRow::find_by_username(&state.db, &req.username).await?.is_some() {
        return Err(AppError::Conflict("Username already taken".into()));
    }
    if crate::db::users::UserRow::find_by_email(&state.db, &req.email).await?.is_some() {
        return Err(AppError::Conflict("Email already registered".into()));
    }

    let password_hash = auth::hash_password(&req.password)?;
    tracing::error!("[AUTH DEBUG] Signup hash for {}: prefix={:?}, email={:?}", req.username, &password_hash[..20.min(password_hash.len())], req.email);

    // When invite codes are required, skip email verification — invite code is the gate
    let skip_email_verify = state.config.require_invite_code;

    let user = if skip_email_verify {
        crate::db::users::UserRow::create_active(
            &state.db,
            &req.username,
            &req.email,
            &password_hash,
            &req.public_key,
        ).await?
    } else {
        crate::db::users::UserRow::create(
            &state.db,
            &req.username,
            &req.email,
            &password_hash,
            &req.public_key,
        ).await?
    };
    tracing::error!("[AUTH DEBUG] User created: id={}, stored_hash_prefix={:?}", user.id, &user.password_hash[..20.min(user.password_hash.len())]);

    // Consume invite code after successful user creation
    if state.config.require_invite_code && !req.invite_code.is_empty() {
        let _ = crate::db::invite_codes::InviteCodeRow::validate_and_consume(&state.db, &req.invite_code, user.id).await;
    }

    // Create PDS account (v2.0 — DID + secp256k1 signing key + repo)
    let handle = &req.username;
    match crate::pds::account::PdsAccount::create(&state.db, handle, &state.config.domain, Some(user.id), &state.config.plc_directory).await {
        Ok(account) => {
            tracing::info!("[AUTH] Created PDS account: did={} handle={}", account.did, account.handle);
        }
        Err(e) => {
            tracing::warn!("[AUTH] PDS account creation failed (user still created): {}", e);
        }
    }

    // Mint the ActivityPub actor at account birth (dual-protocol identity),
    // behind AP_ACTORS_ENABLED. Non-fatal: a failed issuance never blocks signup.
    // Email accounts have no pre-existing AP actor: the minted actor IS their
    // fediverse presence (realignment 2026-11-14 — only Mastodon signups skip).
    if crate::ap::ap_actors_enabled() {
        if let Err(e) = crate::ap::issue::issue_actor_for_user(&state, user.id).await {
            tracing::warn!("[AUTH] AP actor issuance failed (user still created): {e}");
        }
    }

    // When invite-only: return tokens immediately (no email verification)
    if skip_email_verify {
        let username = user.username.clone();
        let manager = auth::RefreshTokenManager { db: state.db.clone() };
        let access_token = auth::create_token(user.id, &username, &state.config.jwt_secret)?;
        let (refresh_token, _family) = manager.create(user.id).await?;

        let cookies = session_set_cookies(&request_host(&headers), &state.config.domain, &access_token, &refresh_token);
        return Ok(attach_cookies(
            Json(serde_json::json!({
                "status": "active",
                "access_token": access_token,
                "refresh_token": refresh_token,
                "expires_in": 900,
                "token_type": "Bearer",
                "user_id": user.id.to_string(),
                "username": username,
            }))
            .into_response(),
            cookies,
        ));
    }

    // Otherwise: generate and send verification code
    let code = auth::email_verification::generate_code(&state.db, &req.email).await?;
    let email_sent = auth::email_verification::send_verification_email(
        &state.config.smtp_host,
        state.config.smtp_port,
        &state.config.smtp_username,
        &state.config.smtp_password,
        &state.config.smtp_from_address,
        &req.email,
        &code,
    )
    .await?;

    let mut response = serde_json::json!({
        "status": "pending",
        "message": "Check your email for the verification code"
    });
    if !email_sent {
        response["code"] = code.into();
        response["message"] = "SMTP not configured — your verification code is displayed below".into();
    }

    Ok(Json(response).into_response())
}

/// POST /api/auth/verify-email — Verify email with 6-digit code and activate user.
async fn verify_email(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<VerifyEmailRequest>,
) -> Result<axum::response::Response, AppError> {
    // Verify the code
    auth::email_verification::verify_code(&state.db, &req.email, &req.code).await?;

    // Find the pending user by email
    let user = crate::db::users::UserRow::find_by_email(&state.db, &req.email)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    // Activate the user
    let activated = crate::db::users::UserRow::activate_user(&state.db, user.id)
        .await?
        .ok_or_else(|| AppError::BadRequest("User is not in pending state".into()))?;

    let username = activated.username.clone();
    let manager = auth::RefreshTokenManager { db: state.db.clone() };
    let access_token = auth::create_token(activated.id, &username, &state.config.jwt_secret)?;
    let (refresh_token, _family) = manager.create(activated.id).await?;

    let cookies = session_set_cookies(&request_host(&headers), &state.config.domain, &access_token, &refresh_token);
    Ok(attach_cookies(
        Json(SignupResponse {
            access_token,
            refresh_token,
            expires_in: 900,
            token_type: "Bearer",
            user_id: activated.id.to_string(),
            username,
            actor_url: format!("https://{}/@{}", state.config.domain, activated.username),
        })
        .into_response(),
        cookies,
    ))
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub token_type: &'static str,
    pub user_id: String,
    pub username: String,
    pub actor_url: String,
}

async fn login(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<axum::response::Response, AppError> {
    tracing::error!("[AUTH DEBUG] Login attempt for email: {:?}", req.email);

    let user = match crate::db::users::UserRow::find_by_email(&state.db, &req.email).await {
        Ok(Some(u)) => {
            tracing::error!("[AUTH DEBUG] User found: id={}, hash_prefix={:?}", u.id, &u.password_hash[..20.min(u.password_hash.len())]);
            u
        }
        Ok(None) => {
            tracing::error!("[AUTH DEBUG] User not found for email: {:?}", req.email);
            return Err(AppError::Auth("Invalid email or password".into()));
        }
        Err(e) => {
            tracing::error!("[AUTH DEBUG] DB error looking up email: {:?}", e);
            return Err(AppError::Auth("Invalid email or password".into()));
        }
    };

    // Reject deleted accounts
    if user.deleted_at.is_some() || user.status == "deleted" {
        return Err(AppError::Auth("ACCOUNT_DELETED".into()));
    }

    // Reject pending accounts (email not verified)
    if user.status == "pending" {
        return Err(AppError::Auth("ACCOUNT_PENDING_VERIFICATION".into()));
    }

    let valid = auth::verify_password(&req.password, &user.password_hash)?;
    tracing::error!("[AUTH DEBUG] Password verify result: {}", valid);
    if !valid {
        return Err(AppError::Auth("Invalid email or password".into()));
    }

    let manager = auth::RefreshTokenManager { db: state.db.clone() };
    let access_token = auth::create_token(user.id, &user.username, &state.config.jwt_secret)?;
    let (refresh_token, _family) = manager.create(user.id).await?;

    let cookies = session_set_cookies(&request_host(&headers), &state.config.domain, &access_token, &refresh_token);
    Ok(attach_cookies(
        Json(LoginResponse {
            access_token,
            refresh_token,
            expires_in: 900,
            token_type: "Bearer",
            user_id: user.id.to_string(),
            username: user.username.clone(),
            actor_url: format!("https://{}/@{}", state.config.domain, user.username),
        })
        .into_response(),
        cookies,
    ))
}

#[derive(Deserialize)]
pub struct GoogleAuthRequest {
    /// Google ID token from the client-side Google Sign-In flow
    pub id_token: String,
    #[serde(default)]
    pub invite_code: String,
}

#[derive(Serialize)]
pub struct GoogleAuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub token_type: &'static str,
    pub user_id: String,
    pub username: String,
    pub actor_url: String,
    pub is_new_user: bool,
}

/// POST /api/auth/google — Sign in or sign up with Google OAuth.
///
/// Verifies the Google ID token by calling Google's tokeninfo endpoint
/// (simpler than JWKS parsing — Google certs change and we'd need to
/// handle rotation). If the token is valid, logs in or creates account.
async fn google_auth(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<GoogleAuthRequest>,
) -> Result<axum::response::Response, AppError> {
    if state.config.google_client_id.is_empty() {
        return Err(AppError::Auth("Google sign-in is not configured on this server".into()));
    }

    // Verify the ID token with Google's tokeninfo endpoint
    let verify_url = format!(
        "https://oauth2.googleapis.com/tokeninfo?id_token={}",
        req.id_token
    );
    let resp = reqwest::get(&verify_url).await
        .map_err(|e| AppError::Internal(format!("Google token verification failed: {}", e)))?;

    if !resp.status().is_success() {
        return Err(AppError::Auth("Invalid Google token".into()));
    }

    let info: serde_json::Value = resp.json().await
        .map_err(|e| AppError::Internal(format!("Failed to parse Google response: {}", e)))?;

    // Verify the audience matches our client ID
    let aud = info.get("aud").and_then(|v| v.as_str()).unwrap_or("");
    if aud != state.config.google_client_id {
        return Err(AppError::Auth("Google token audience mismatch".into()));
    }

    let email = info.get("email")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Auth("Google token missing email".into()))?;

    let base_username = email.split('@').next()
        .unwrap_or("user").to_lowercase()
        .chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-').collect::<String>();
    let username = if base_username.len() < 2 { "user".to_string() } else { base_username };

    let manager = auth::RefreshTokenManager { db: state.db.clone() };

    // Check if user already exists
    if let Some(user) = crate::db::users::UserRow::find_by_email(&state.db, email).await? {
        let access_token = auth::create_token(user.id, &user.username, &state.config.jwt_secret)?;
        let (refresh_token, _family) = manager.create(user.id).await?;
        let cookies = session_set_cookies(&request_host(&headers), &state.config.domain, &access_token, &refresh_token);
        return Ok(attach_cookies(
            Json(GoogleAuthResponse {
                access_token, refresh_token, expires_in: 900, token_type: "Bearer",
                user_id: user.id.to_string(), username: user.username.clone(),
                actor_url: format!("https://{}/@{}", state.config.domain, user.username),
                is_new_user: false,
            })
            .into_response(),
            cookies,
        ));
    }

    // New user — generate keypair server-side for now (client-side key gen is future)
    let mut key_bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut key_bytes);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&key_bytes);
    let public_key_hex = hex::encode(signing_key.verifying_key().to_bytes());
    let random_password = uuid::Uuid::new_v4().to_string();
    let password_hash = auth::hash_password(&random_password)?;

    if state.config.require_invite_code {
        if req.invite_code.is_empty() {
            return Err(AppError::BadRequest("Invite code required".into()));
        }
        if !crate::db::invite_codes::InviteCodeRow::is_valid(&state.db, &req.invite_code).await? {
            return Err(AppError::BadRequest("Invalid or expired invite code".into()));
        }
    }
    let user = crate::db::users::UserRow::create(&state.db, &username, email, &password_hash, &public_key_hex).await?;
    if state.config.require_invite_code && !req.invite_code.is_empty() {
        let _ = crate::db::invite_codes::InviteCodeRow::validate_and_consume(&state.db, &req.invite_code, user.id).await;
    }

    let access_token = auth::create_token(user.id, &user.username, &state.config.jwt_secret)?;
    let (refresh_token, _family) = manager.create(user.id).await?;
    let cookies = session_set_cookies(&request_host(&headers), &state.config.domain, &access_token, &refresh_token);
    Ok(attach_cookies(
        Json(GoogleAuthResponse {
            access_token, refresh_token, expires_in: 900, token_type: "Bearer",
            user_id: user.id.to_string(), username: user.username.clone(),
            actor_url: format!("https://{}/@{}", state.config.domain, user.username),
            is_new_user: true,
        })
        .into_response(),
        cookies,
    ))
}

/// POST /api/auth/refresh — Refresh an expired access token using a refresh
/// token from the body (single-use rotation, web's localStorage flow) or,
/// when the body token is absent, from the family cookie (no rotation — the
/// same refresh token is also in web's localStorage, and rotating it here
/// would strand that origin). Either way the response re-issues both
/// cookies (spec: refresh re-issues cookie and tokens).
async fn refresh(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<RefreshRequest>,
) -> Result<axum::response::Response, AppError> {
    let manager = auth::RefreshTokenManager { db: state.db.clone() };
    let via_cookie = req.refresh_token.is_empty();
    let raw = if via_cookie {
        cookie_value(&headers, REFRESH_COOKIE)
            .ok_or_else(|| AppError::Auth("No refresh token".into()))?
    } else {
        req.refresh_token.clone()
    };
    let record = manager.validate(&raw).await?;
    let username = manager.get_username(record.user_id).await?;
    let new_access = auth::create_token(record.user_id, &username, &state.config.jwt_secret)?;
    let refresh_out = if via_cookie {
        raw
    } else {
        manager.rotate(&raw).await?.0
    };

    let cookies = session_set_cookies(&request_host(&headers), &state.config.domain, &new_access, &refresh_out);
    Ok(attach_cookies(
        Json(AuthTokenResponse {
            access_token: new_access,
            refresh_token: refresh_out,
            expires_in: 900,
            token_type: "Bearer",
        })
        .into_response(),
        cookies,
    ))
}

// ─── Admin: Invite Codes ─────────────────────────────────

/// SEC-2: admin routes fail closed. `AppError::Auth` (401) when the admin
/// interface is disabled (`ADMIN_TOKEN` unset) or the `Authorization: Bearer`
/// header is missing or carries the wrong token. No constant-time compare
/// helper exists in this codebase, so the match is plain equality.
fn authorize_admin(headers: &axum::http::HeaderMap, state: &AppState) -> Result<(), AppError> {
    let Some(expected) = state.config.admin_token.as_deref() else {
        return Err(AppError::Auth("admin interface disabled".into()));
    };
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match bearer {
        Some(tok) if tok == expected => Ok(()),
        _ => Err(AppError::Auth("Invalid admin token".into())),
    }
}

#[derive(Deserialize)]
pub struct CreateInviteCodeRequest {
    #[serde(default = "default_max_uses")]
    pub max_uses: i32,
}

fn default_max_uses() -> i32 { 1 }

async fn create_invite_code(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::Query(params): axum::extract::Query<CreateInviteCodeRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    authorize_admin(&headers, &state)?;
    let code = crate::db::invite_codes::InviteCodeRow::create(&state.db, None, params.max_uses)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create invite code: {e}")))?;
    Ok(Json(serde_json::json!({ "code": code, "max_uses": params.max_uses })))
}

async fn list_invite_codes(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    authorize_admin(&headers, &state)?;
    let codes = crate::db::invite_codes::InviteCodeRow::list(&state.db)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to list invite codes: {e}")))?;
    let result: Vec<serde_json::Value> = codes.iter().map(|c| {
        serde_json::json!({
            "code": c.code,
            "max_uses": c.max_uses,
            "use_count": c.use_count,
            "is_active": c.is_active,
            "used_at": c.used_at,
        })
    }).collect();
    Ok(Json(serde_json::json!({ "codes": result })))
}

// ─── Admin: Log Viewer ────────────────────────────────────

/// GET /api/admin/logs — returns recent journal logs (JSON, bearer-token gated)
async fn get_logs(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::Query(params): axum::extract::Query<LogQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    authorize_admin(&headers, &state)?;
    let lines = params.lines.unwrap_or(100);
    let output = tokio::process::Command::new("journalctl")
        .arg("-u").arg("bytescape")
        .arg("--no-pager")
        .arg("-n").arg(lines.to_string())
        .output()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read logs: {e}")))?;
    let log_text = String::from_utf8_lossy(&output.stdout);
    Ok(Json(serde_json::json!({ "lines": lines, "logs": log_text })))
}

/// GET /admin/logs — serves the HTML log viewer page. The page itself is a
/// static shell (reachable without auth); the data it fetches from
/// /api/admin/logs is bearer-gated (see `authorize_admin`).
async fn log_viewer_page() -> axum::response::Html<&'static str> {
    axum::response::Html(LOG_VIEWER_HTML)
}

#[derive(Deserialize)]
pub struct LogQuery {
    pub lines: Option<u32>,
}

// ─── Tests ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;

    fn admin_state(pool: sqlx::PgPool, admin_token: Option<&str>) -> AppState {
        let Ok(mut config) = crate::config::AppConfig::from_env() else {
            panic!("config incomplete — admin auth tests require env");
        };
        config.admin_token = admin_token.map(|t| t.to_string());
        AppState {
            config: std::sync::Arc::new(config),
            db: pool,
            og_cache: Default::default(),
        }
    }

    async fn admin_call(
        app: &axum::Router,
        method: &str,
        uri: &str,
        bearer: Option<&str>,
    ) -> u16 {
        let mut builder = axum::http::Request::builder().method(method).uri(uri);
        if let Some(token) = bearer {
            builder = builder.header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let req = builder.body(axum::body::Body::empty()).expect("request");
        tower::ServiceExt::oneshot(app.clone(), req).await.expect("oneshot").status().as_u16()
    }

    /// SEC-2 regression: POST/GET /api/admin/invite-codes must demand the
    /// admin bearer token (pre-fix both answered 200 unauthenticated).
    #[tokio::test]
    async fn admin_invite_codes_require_bearer_token() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else { return };
        let state = admin_state(pool, Some("sekrit-admin-token"));
        let app = routes().with_state(state);

        for (method, uri) in [("POST", "/api/admin/invite-codes"), ("GET", "/api/admin/invite-codes")] {
            assert_eq!(
                admin_call(&app, method, uri, None).await,
                401,
                "{method} {uri} unauthenticated must be rejected"
            );
            assert_eq!(
                admin_call(&app, method, uri, Some("wrong-token")).await,
                401,
                "{method} {uri} wrong bearer must be rejected"
            );
        }
        // GET with the correct bearer passes the gate (list is DB-backed —
        // the pool was reachable above, so the query runs).
        assert_eq!(
            admin_call(&app, "GET", "/api/admin/invite-codes", Some("sekrit-admin-token")).await,
            200,
            "GET with correct bearer must pass the auth gate"
        );
    }

    /// SEC-2 regression: with ADMIN_TOKEN unset (admin_token = None) every
    /// admin route fails closed — 401 for any header value.
    #[tokio::test]
    async fn admin_routes_fail_closed_when_token_unset() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else { return };
        let state = admin_state(pool, None);
        let app = routes().with_state(state);

        for (method, uri) in [
            ("POST", "/api/admin/invite-codes"),
            ("GET", "/api/admin/invite-codes"),
            ("GET", "/api/admin/logs"),
        ] {
            for bearer in [None, Some(""), Some("changeme-admin-token"), Some("anything")] {
                assert_eq!(
                    admin_call(&app, method, uri, bearer).await,
                    401,
                    "{method} {uri} must fail closed when admin_token is None (bearer {bearer:?})"
                );
            }
        }
    }
}

const LOG_VIEWER_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>Bytescape Logs</title>
<style>
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body { background: #0d1117; color: #8b949e; font-family: "SF Mono","Fira Code","Cascadia Code",monospace; font-size: 12px; height: 100vh; display: flex; flex-direction: column; }
  .header { background: #161b22; padding: 8px 16px; border-bottom: 1px solid #30363d; display: flex; align-items: center; gap: 12px; }
  .header h1 { font-size: 13px; color: #58a6ff; font-weight: 500; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: #3fb950; animation: pulse 2s infinite; }
  .dot.paused { background: #f85149; animation: none; }
  @keyframes pulse { 0%,100% { opacity: 1; } 50% { opacity: 0.4; } }
  .controls { margin-left: auto; display: flex; align-items: center; gap: 8px; font-size: 11px; }
  .controls label { color: #8b949e; }
  .controls select { background: #0d1117; color: #c9d1d9; border: 1px solid #30363d; border-radius: 4px; padding: 2px 6px; font-size: 11px; }
  .controls button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 4px; padding: 3px 10px; font-size: 11px; cursor: pointer; }
  .controls button:hover { background: #30363d; }
  .controls button.active { background: #1f6feb; border-color: #1f6feb; color: #fff; }
  .log-wrap { flex: 1; overflow: hidden; position: relative; }
  .log-list { overflow-y: auto; height: 100%; padding: 8px 16px; scroll-behavior: smooth; }
  .log-line { padding: 1px 0; white-space: pre-wrap; word-break: break-word; line-height: 1.6; }
  .log-line.error { color: #f85149; }
  .log-line.warn { color: #d29922; }
  .log-line.info { color: #58a6ff; }
  .log-line.debug { color: #8b949e; }
  .log-line.system { color: #3fb950; }
  .ts-sep { color: #484f58; border-top: 1px solid #21262d; margin: 8px 0 4px; padding-top: 4px; font-size: 10px; }
</style>
</head>
<body>
<div class="header">
  <span class="dot" id="status-dot"></span>
  <h1>Bytescape Server Logs</h1>
  <div class="controls">
    <label>Lines</label>
    <select id="line-count">
      <option value="50">50</option>
      <option value="100" selected>100</option>
      <option value="200">200</option>
      <option value="500">500</option>
    </select>
    <button id="pause-btn" onclick="togglePause()">Pause</button>
  </div>
</div>
<div class="log-wrap"><div class="log-list" id="logs"></div></div>
<script>
  const params = new URLSearchParams(location.search);
  const token = params.get('token') || '';
  let paused = false;
  let lastLogs = '';

  function togglePause() {
    paused = !paused;
    const btn = document.getElementById('pause-btn');
    const dot = document.getElementById('status-dot');
    btn.textContent = paused ? 'Resume' : 'Pause';
    btn.classList.toggle('active', paused);
    dot.classList.toggle('paused', paused);
  }

  async function fetchLogs() {
    if (paused) return;
    try {
      const lines = document.getElementById('line-count').value;
      const res = await fetch('/api/admin/logs?lines=' + lines, { headers: { 'Authorization': 'Bearer ' + token } });
      const data = await res.json();
      const text = data.logs || '';
      if (text === lastLogs) return;
      lastLogs = text;
      const el = document.getElementById('logs');
      const wasAtBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 50;
      const lineArr = text.trim().split('\n');
      el.innerHTML = lineArr.map(l => {
        let cls = '';
        if (/error|panic|crash/i.test(l)) cls = 'error';
        else if (/warn/i.test(l)) cls = 'warn';
        else if (/\binfo\b/i.test(l)) cls = 'info';
        else if (/debug/i.test(l)) cls = 'debug';
        else if (/systemd|Started|Stopped|active/i.test(l)) cls = 'system';
        return '<div class="log-line ' + cls + '">' + l.replace(/</g,'&lt;') + '</div>';
      }).join('');
      if (wasAtBottom) el.scrollTop = el.scrollHeight;
    } catch(e) {}
  }
  fetchLogs();
  setInterval(fetchLogs, 3000);
</script>
</body>
</html>"#;


