//! API — Settings routes.
//!
//! GET  /api/settings/posting-identities  — list posting identities for the authenticated user
//! POST /api/settings/posting-identity    — update a posting identity (switch between bytescape and linked)

use axum::{
    extract::State,
    http::HeaderMap,
    routing::{get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::users::NsfwPref;
use crate::{auth, error::AppError, oauth, AppState};

/// The app-theme roster (settings-account-ia). Unknown ids are rejected.
const THEME_ROSTER: &[&str] = &[
    "default",
    "terminal",
    "1999",
    "xp",
    "retrofuturistic",
    "light",
    "dark",
];

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/settings", get(get_settings))
        .route("/api/settings/appearance", patch(patch_appearance))
        .route("/api/settings/moderation", patch(patch_moderation))
        .route("/api/settings/interests", patch(patch_interests))
        .route("/api/settings/feed", patch(patch_feed))
        .route("/api/settings/privacy", patch(patch_privacy))
        .route(
            "/api/settings/visibility",
            get(get_visibility).patch(patch_visibility),
        )
        .route("/api/settings/posting-identities", get(get_posting_identities))
        .route("/api/settings/posting-identity", post(update_posting_identity))
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

// ---------------------------------------------------------------------------
// PATCH /api/settings/{section} + GET /api/settings
//
// One uniform save pattern for the Settings sections. Each PATCH validates its
// payload, persists to the real columns (users.theme, users.nsfw_pref,
// users.interests, users.feed_prefs), and returns `{"ok":true,"settings":{...}}`
// with just that section's saved values.
// ---------------------------------------------------------------------------

/// Load the authenticated user or 404.
async fn load_user(state: &AppState, headers: &HeaderMap) -> Result<crate::db::users::UserRow, AppError> {
    let user_id = extract_user_id(headers, &state.config.jwt_secret)?;
    crate::db::users::UserRow::find_by_id(&state.db, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))
}

#[derive(Deserialize)]
struct PatchAppearanceRequest {
    theme: String,
}

async fn patch_appearance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PatchAppearanceRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user = load_user(&state, &headers).await?;
    if !THEME_ROSTER.contains(&req.theme.as_str()) {
        return Err(AppError::BadRequest(format!(
            "Invalid theme '{}'. Must be one of: {}",
            req.theme,
            THEME_ROSTER.join(", ")
        )));
    }
    crate::db::users::UserRow::set_theme(&state.db, user.id, &req.theme).await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "settings": { "theme": req.theme }
    })))
}

#[derive(Deserialize)]
struct PatchModerationRequest {
    nsfw_pref: String,
}

async fn patch_moderation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PatchModerationRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user = load_user(&state, &headers).await?;
    let pref = NsfwPref::parse(&req.nsfw_pref).ok_or_else(|| {
        AppError::BadRequest(format!(
            "Invalid nsfw_pref '{}'. Must be 'exclude', 'blur', or 'show'",
            req.nsfw_pref
        ))
    })?;
    crate::db::users::UserRow::set_nsfw_pref(&state.db, user.id, pref).await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "settings": { "nsfw_pref": pref.as_str() }
    })))
}

#[derive(Deserialize)]
struct PatchInterestsRequest {
    interests: Vec<String>,
}

async fn patch_interests(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PatchInterestsRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user = load_user(&state, &headers).await?;
    // Settings → Interests is a second entry point to the same picker: reconcile
    // the interest-driven subscriptions exactly like the bootstrap endpoint
    // (drop deselected groupings, re-expand topic feeds + hashtags), persist
    // users.interests, and mark the account onboarded.
    crate::api::subscriptions::reconcile_interests(&state.db, user.id, &req.interests)
        .await
        .map_err(AppError::Database)?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "settings": { "interests": req.interests }
    })))
}

#[derive(Deserialize)]
struct PatchFeedRequest {
    sort: Option<String>,
    source_mix: Option<String>,
}

/// Read the saved feed prefs, defaulting missing/invalid values.
fn feed_prefs_view(v: &serde_json::Value) -> (String, String) {
    let sort = v
        .get("sort")
        .and_then(|s| s.as_str())
        .filter(|s| matches!(*s, "new" | "popular"))
        .unwrap_or("new")
        .to_string();
    let source_mix = v
        .get("source_mix")
        .and_then(|s| s.as_str())
        .filter(|s| matches!(*s, "all" | "bluesky" | "mastodon"))
        .unwrap_or("all")
        .to_string();
    (sort, source_mix)
}

async fn patch_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PatchFeedRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user = load_user(&state, &headers).await?;

    // Validate provided fields up-front (only provided fields update).
    if let Some(sort) = req.sort.as_deref() {
        if !matches!(sort, "new" | "popular") {
            return Err(AppError::BadRequest(format!(
                "Invalid sort '{}'. Must be 'new' or 'popular'",
                sort
            )));
        }
    }
    if let Some(mix) = req.source_mix.as_deref() {
        if !matches!(mix, "all" | "bluesky" | "mastodon") {
            return Err(AppError::BadRequest(format!(
                "Invalid source_mix '{}'. Must be 'all', 'bluesky', or 'mastodon'",
                mix
            )));
        }
    }

    // Merge patch into existing feed_prefs (don't clobber unspecified fields).
    let mut patch = serde_json::Map::new();
    if let Some(sort) = req.sort {
        patch.insert("sort".into(), serde_json::Value::String(sort));
    }
    if let Some(mix) = req.source_mix {
        patch.insert("source_mix".into(), serde_json::Value::String(mix));
    }
    let patch = serde_json::Value::Object(patch);

    let merged = crate::db::users::UserRow::merge_feed_prefs(&state.db, user.id, &patch).await?;
    let (sort, source_mix) = feed_prefs_view(&merged);
    Ok(Json(serde_json::json!({
        "ok": true,
        "settings": { "sort": sort, "source_mix": source_mix }
    })))
}

async fn get_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let user = load_user(&state, &headers).await?;
    let (sort, source_mix) = feed_prefs_view(&user.feed_prefs.clone().unwrap_or(serde_json::json!({})));
    Ok(Json(serde_json::json!({
        "theme": user.theme,
        "nsfw_pref": user.nsfw_pref,
        "interests": user.interests,
        "sort": sort,
        "source_mix": source_mix,
        "manually_approves_followers": user.manually_approves_followers,
    })))
}

// ---------------------------------------------------------------------------
// PATCH /api/settings/privacy — the account-lock toggle.
//
// Placement (settings-account-ia): the Settings surface owns preferences; the
// natural home for a follow-approval lock is the Moderation & content section,
// which already houses the NSFW preference and the muted-accounts list. The
// API is section-uniform (`PATCH /api/settings/{section}`), so privacy is its
// own section slug to keep the save pattern consistent.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct PatchPrivacyRequest {
    manually_approves_followers: bool,
}

async fn patch_privacy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PatchPrivacyRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user = load_user(&state, &headers).await?;
    crate::db::users::UserRow::set_manually_approves_followers(&state.db, user.id, req.manually_approves_followers).await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "settings": { "manually_approves_followers": req.manually_approves_followers }
    })))
}

// ---------------------------------------------------------------------------
// Visibility (identity-graph Phase 6): one surface over the three networks.
//   * Bytescape AP — `users.graph_visibility` (enforced on the AP collections).
//   * Mastodon home — `hide_collections`, authoritative ON THE HOME INSTANCE;
//     we read it (verify_credentials), surface it with a deep link, and proxy
//     writes (`PATCH /api/v1/accounts/update_credentials`) gated on
//     write:accounts — the re-link prompt on stale grants, never a silent
//     fallback. Divergences are disclosed, never overridden.
//   * Bluesky — UI-only with the mandated caveat (no protocol support).
// ---------------------------------------------------------------------------

const BSKY_VISIBILITY_CAVEAT: &str = "Your follow list is still public on the \
AT Protocol network (Bluesky) — there is no protocol support to hide it there.";

async fn get_visibility(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let user = load_user(&state, &headers).await?;
    let home = match crate::api::follow::home_hide_state(&state, user.id).await {
        Some((instance, hide)) => {
            let acct = crate::api::feed::linked_mastodon_account(&state.db, user.id)
                .await?
                .unwrap();
            serde_json::json!({
                "linked": true,
                "instance": instance,
                "hide_collections": hide,
                "settings_url": format!("https://{}/settings/profile", {
                    let i = crate::api::feed::home_instance_of(&acct)?;
                    i.trim_start_matches("http://").trim_start_matches("https://").to_string()
                }),
                "can_proxy": crate::oauth::mastodon::ensure_accounts_scope(acct.scope.as_deref()).is_ok(),
            })
        }
        None => {
            let linked = crate::db::accounts::LinkedAccountRow::find_by_provider(
                &state.db, user.id, "mastodon",
            )
            .await
            .map_err(AppError::Database)?
            .len()
                > 0;
            serde_json::json!({ "linked": linked, "instance": null, "hide_collections": null, "settings_url": null, "can_proxy": false })
        }
    };
    let bsky_linked = crate::db::accounts::LinkedAccountRow::find_by_provider(
        &state.db, user.id, "bluesky",
    )
    .await
    .map_err(AppError::Database)?
    .len()
        > 0;
    Ok(Json(serde_json::json!({
        "graph_visibility": user.graph_visibility,
        "home": home,
        "bluesky": { "linked": bsky_linked, "caveat": BSKY_VISIBILITY_CAVEAT },
        "caveat": BSKY_VISIBILITY_CAVEAT,
    })))
}

#[derive(serde::Deserialize)]
struct PatchVisibilityRequest {
    graph_visibility: Option<String>,
    hide_collections: Option<bool>,
}

async fn patch_visibility(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PatchVisibilityRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user = load_user(&state, &headers).await?;

    if let Some(v) = req.graph_visibility.as_deref() {
        crate::db::users::UserRow::set_graph_visibility(&state.db, user.id, v).await?;
    }

    // The home value is authoritative: proxy the write, then READ IT BACK and
    // return what the home instance says — never silently diverge.
    let mut home_hide: Option<bool> = None;
    if let Some(hide) = req.hide_collections {
        let acct = crate::api::feed::linked_mastodon_account(&state.db, user.id)
            .await?
            .ok_or_else(|| AppError::BadRequest("no Mastodon account linked".into()))?;
        crate::oauth::mastodon::ensure_accounts_scope(acct.scope.as_deref())
            .map_err(crate::api::feed::mastodon_error)?;
        let instance = crate::api::feed::home_instance_of(&acct)?;
        let http = crate::api::feed::feed_http();
        let token = crate::oauth::mastodon::refresh_mastodon_token(&state.db, &http, &acct)
            .await
            .map_err(crate::api::feed::mastodon_error)?;
        crate::oauth::mastodon::update_credentials_hide(&http, &instance, &token, hide)
            .await
            .map_err(crate::api::feed::mastodon_error)?;
        let me = crate::oauth::mastodon::verify_credentials(&http, &instance, &token)
            .await
            .map_err(|e| AppError::OAuth(format!("read-back failed: {e}")))?;
        home_hide = Some(me.hide_collections);
        let _ = sqlx::query(
            "UPDATE linked_accounts SET metadata = metadata || $2::jsonb WHERE id = $1",
        )
        .bind(acct.id)
        .bind(serde_json::json!({
            "hide_collections": me.hide_collections,
            "hide_checked_at": chrono::Utc::now().to_rfc3339(),
        }))
        .execute(&state.db)
        .await;
    }

    Ok(Json(serde_json::json!({
        "ok": true,
        "graph_visibility": user.graph_visibility,
        "home_hide_collections": home_hide,
    })))
}

// ---------------------------------------------------------------------------
// GET /api/settings/posting-identities
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct PostingIdentityView {
    protocol: String,
    identity_type: String,
    linked_account_id: Option<String>,
    external_username: Option<String>,
}

#[derive(sqlx::FromRow)]
struct PostingIdentityWithUsername {
    protocol: String,
    identity_type: String,
    linked_account_id: Option<Uuid>,
    external_username: Option<String>,
}

async fn get_posting_identities(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<PostingIdentityView>>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let rows: Vec<PostingIdentityWithUsername> = sqlx::query_as(
        r#"
        SELECT
            pi.protocol,
            pi.identity_type,
            pi.linked_account_id,
            la.external_username
        FROM posting_identities pi
        LEFT JOIN linked_accounts la ON la.id = pi.linked_account_id
        WHERE pi.user_id = $1
        ORDER BY pi.protocol
        "#,
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    let identities = rows
        .into_iter()
        .map(|r| PostingIdentityView {
            protocol: r.protocol,
            identity_type: r.identity_type,
            linked_account_id: r.linked_account_id.map(|id| id.to_string()),
            external_username: r.external_username,
        })
        .collect();

    Ok(Json(identities))
}

// ---------------------------------------------------------------------------
// POST /api/settings/posting-identity
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct UpdatePostingIdentityRequest {
    protocol: String,
    identity_type: String,
    linked_account_id: Option<Uuid>,
}

async fn update_posting_identity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<UpdatePostingIdentityRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    // Validate protocol
    if req.protocol != "atproto" {
        return Err(AppError::BadRequest(format!(
            "Invalid protocol '{}'. Must be 'atproto'",
            req.protocol
        )));
    }

    // Validate identity_type
    if req.identity_type != "bytescape" && req.identity_type != "linked" {
        return Err(AppError::BadRequest(format!(
            "Invalid identity_type '{}'. Must be 'bytescape' or 'linked'",
            req.identity_type
        )));
    }

    // If identity_type is 'linked', linked_account_id is required
    if req.identity_type == "linked" && req.linked_account_id.is_none() {
        return Err(AppError::BadRequest(
            "linked_account_id is required when identity_type is 'linked'".into(),
        ));
    }

    // If identity_type is 'bytescape', linked_account_id must be None
    // (the DB CHECK constraint enforces this too, but we validate early for a better error message)
    if req.identity_type == "bytescape" && req.linked_account_id.is_some() {
        return Err(AppError::BadRequest(
            "linked_account_id must be null when identity_type is 'bytescape'".into(),
        ));
    }

    // If switching to a linked account, validate it belongs to the user and matches the protocol
    if let Some(linked_account_id) = req.linked_account_id {
        let account = crate::db::accounts::LinkedAccountRow::find_by_id(&state.db, linked_account_id)
            .await
            .map_err(AppError::Database)?;

        let account = match account {
            Some(a) if a.user_id == user_id => a,
            Some(_) => {
                return Err(AppError::BadRequest(
                    "linked_account_id does not belong to the authenticated user".into(),
                ));
            }
            None => {
                return Err(AppError::BadRequest(
                    "linked_account_id not found".into(),
                ));
            }
        };

        // Validate provider matches protocol
        let expected_protocol = match account.provider.as_str() {
            "bluesky" => "atproto",
            _ => {
                return Err(AppError::BadRequest(format!(
                    "Unsupported provider '{}' for posting identity",
                    account.provider
                )));
            }
        };

        if expected_protocol != req.protocol {
            return Err(AppError::BadRequest(format!(
                "Provider '{}' maps to protocol '{}', not '{}'",
                account.provider, expected_protocol, req.protocol
            )));
        }

        // Token freshness check
        match account.provider.as_str() {
            "bluesky" => {
                validate_bluesky_token(&state, &account).await?;
            }
            _ => {}
        }
    }

    // Upsert the posting identity
    let identity = crate::db::posting_identities::PostingIdentityRow::upsert(
        &state.db,
        user_id,
        &req.protocol,
        &req.identity_type,
        req.linked_account_id,
    )
    .await
    .map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({
        "ok": true,
        "posting_identity": {
            "protocol": identity.protocol,
            "identity_type": identity.identity_type,
            "linked_account_id": identity.linked_account_id.map(|id| id.to_string()),
        }
    })))
}

/// Validate a Bluesky linked account token. If expired, attempt refresh.
async fn validate_bluesky_token(
    state: &AppState,
    account: &crate::db::accounts::LinkedAccountRow,
) -> Result<(), AppError> {
    let now = chrono::Utc::now();

    // Check if token is expired
    let is_expired = match account.token_expires_at {
        Some(expires_at) => expires_at <= now,
        None => false, // No expiry set — assume valid
    };

    if !is_expired {
        return Ok(()); // Token still valid
    }

    // Token expired — attempt refresh
    let refresh_token = account.refresh_token.as_ref().ok_or_else(|| {
        AppError::BadRequest("re-link required: no refresh token available".into())
    })?;

    if refresh_token.is_empty() {
        return Err(AppError::BadRequest(
            "re-link required: refresh token is empty".into(),
        ));
    }

    let http = reqwest::Client::new();
    match oauth::bluesky::refresh_session(&http, &state.db, account).await {
        Ok(_) => Ok(()),
        Err(e) => {
            tracing::warn!(
                "Bluesky token refresh failed for account {}: {}",
                account.id,
                e
            );
            Err(AppError::BadRequest(format!(
                "re-link required: token refresh failed: {}",
                e
            )))
        }
    }
}

// ---------------------------------------------------------------------------
// Tests — settings PATCH routes, NSFW three-state persistence, one-per-network.
// DB integration tests skip without DATABASE_URL (same convention as the AP
// tests). Pure policy/mapping tests always run.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ap::testutil::{connect_pool, test_state, unique};
    use crate::db::users::NsfwPref;

    /// Three-state parse + snapshot axis + lenient DB fallback.
    #[test]
    fn nsfw_pref_parses_three_states_and_defaults_to_exclude() {
        assert_eq!(NsfwPref::parse("exclude"), Some(NsfwPref::Exclude));
        assert_eq!(NsfwPref::parse("blur"), Some(NsfwPref::Blur));
        assert_eq!(NsfwPref::parse("show"), Some(NsfwPref::Show));
        assert_eq!(NsfwPref::parse("nonsense"), None);

        // Lenient read (stored column): unknown -> safe default.
        assert_eq!(NsfwPref::from_db("nonsense"), NsfwPref::Exclude);
        assert_eq!(NsfwPref::from_db("show"), NsfwPref::Show);

        // Snapshot axis: exclude -> 0, blur/show -> 1.
        assert_eq!(NsfwPref::Exclude.snapshot_axis(), 0);
        assert_eq!(NsfwPref::Blur.snapshot_axis(), 1);
        assert_eq!(NsfwPref::Show.snapshot_axis(), 1);
    }

    /// The migration mapping (094): `false -> exclude`, `true -> show`, and new
    /// users default to `exclude`. Encoded here as the round-trip through the
    /// real columns plus the `exclude` column default.
    #[tokio::test]
    async fn nsfw_pref_round_trips_all_states_and_defaults_to_exclude() {
        let Some(pool) = connect_pool().await else { return };
        let Some(state) = test_state(&pool) else { return };
        let suffix = unique();
        let username = format!("nsfw_{suffix}");
        let user = crate::db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");

        // New user defaults to 'exclude' (the migrated false-state).
        assert_eq!(user.nsfw_pref, "exclude", "new user defaults to exclude");

        // true -> 'show' equivalent: setting 'show' persists.
        crate::db::users::UserRow::set_nsfw_pref(&pool, user.id, NsfwPref::Show)
            .await
            .expect("set show");
        let reloaded = crate::db::users::UserRow::find_by_id(&pool, user.id)
            .await
            .expect("reload")
            .expect("row");
        assert_eq!(reloaded.nsfw_pref, "show");

        // blur persists too.
        crate::db::users::UserRow::set_nsfw_pref(&pool, user.id, NsfwPref::Blur)
            .await
            .expect("set blur");
        let reloaded = crate::db::users::UserRow::find_by_id(&pool, user.id)
            .await
            .expect("reload")
            .expect("row");
        assert_eq!(reloaded.nsfw_pref, "blur");

        // And back to exclude.
        crate::db::users::UserRow::set_nsfw_pref(&pool, user.id, NsfwPref::Exclude)
            .await
            .expect("set exclude");
        let reloaded = crate::db::users::UserRow::find_by_id(&pool, user.id)
            .await
            .expect("reload")
            .expect("row");
        assert_eq!(reloaded.nsfw_pref, "exclude");

        let _ = state;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await;
    }

    async fn authed_post(
        state: &AppState,
        token: &str,
        uri: &str,
        body: serde_json::Value,
    ) -> (axum::http::StatusCode, serde_json::Value) {
        let app = axum::Router::new().merge(routes()).with_state(state.clone());
        let request = axum::http::Request::builder()
            .method("PATCH")
            .uri(uri)
            .header("Authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
            .expect("request");
        let response = tower::ServiceExt::oneshot(app, request).await.expect("response");
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    async fn authed_get(
        state: &AppState,
        token: &str,
        uri: &str,
    ) -> (axum::http::StatusCode, serde_json::Value) {
        let app = axum::Router::new().merge(routes()).with_state(state.clone());
        let request = axum::http::Request::builder()
            .method("GET")
            .uri(uri)
            .header("Authorization", format!("Bearer {token}"))
            .body(axum::body::Body::empty())
            .expect("request");
        let response = tower::ServiceExt::oneshot(app, request).await.expect("response");
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    /// Each section round-trips through PATCH and is reflected by GET /api/settings;
    /// invalid values are rejected with 400.
    #[tokio::test]
    async fn settings_sections_round_trip_and_reject_invalid() {
        let Some(pool) = connect_pool().await else { return };
        let Some(state) = test_state(&pool) else { return };
        let suffix = unique();
        let username = format!("set_{suffix}");
        let user = crate::db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");
        let token = crate::auth::create_token(user.id, &username, &state.config.jwt_secret)
            .expect("token");

        // Appearance: valid theme round-trips.
        let (status, body) = authed_post(
            &state,
            &token,
            "/api/settings/appearance",
            serde_json::json!({"theme": "retrofuturistic"}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "appearance ok: {body}");
        assert_eq!(body["ok"], serde_json::json!(true));
        assert_eq!(body["settings"]["theme"], serde_json::json!("retrofuturistic"));

        // Moderation: valid nsfw_pref round-trips.
        let (status, body) = authed_post(
            &state,
            &token,
            "/api/settings/moderation",
            serde_json::json!({"nsfw_pref": "blur"}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "moderation ok: {body}");
        assert_eq!(body["settings"]["nsfw_pref"], serde_json::json!("blur"));

        // Interests: round-trips and marks onboarded.
        let (status, body) = authed_post(
            &state,
            &token,
            "/api/settings/interests",
            serde_json::json!({"interests": ["cats", "dogs"]}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "interests ok: {body}");
        assert_eq!(body["settings"]["interests"], serde_json::json!(["cats", "dogs"]));
        let reloaded = crate::db::users::UserRow::find_by_id(&pool, user.id)
            .await
            .expect("reload")
            .expect("row");
        assert!(reloaded.onboarded, "setting interests marks onboarded");

        // Feed: partial update (only sort) round-trips; source_mix keeps default.
        let (status, body) = authed_post(
            &state,
            &token,
            "/api/settings/feed",
            serde_json::json!({"sort": "popular"}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "feed ok: {body}");
        assert_eq!(body["settings"]["sort"], serde_json::json!("popular"));
        assert_eq!(body["settings"]["source_mix"], serde_json::json!("all"), "unspecified field keeps default");

        // Feed: second partial update (only source_mix) must NOT clobber sort.
        let (status, body) = authed_post(
            &state,
            &token,
            "/api/settings/feed",
            serde_json::json!({"source_mix": "bluesky"}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "feed mix ok: {body}");
        assert_eq!(body["settings"]["sort"], serde_json::json!("popular"), "sort preserved on merge");
        assert_eq!(body["settings"]["source_mix"], serde_json::json!("bluesky"));

        // GET /api/settings reflects everything saved.
        let (status, all) = authed_get(&state, &token, "/api/settings").await;
        assert_eq!(status, axum::http::StatusCode::OK, "get settings: {all}");
        assert_eq!(all["theme"], serde_json::json!("retrofuturistic"));
        assert_eq!(all["nsfw_pref"], serde_json::json!("blur"));
        assert_eq!(all["interests"], serde_json::json!(["cats", "dogs"]));
        assert_eq!(all["sort"], serde_json::json!("popular"));
        assert_eq!(all["source_mix"], serde_json::json!("bluesky"));

        // Invalid values -> 400.
        let (status, _) = authed_post(
            &state,
            &token,
            "/api/settings/appearance",
            serde_json::json!({"theme": "bogus-theme"}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST, "invalid theme rejected");

        let (status, _) = authed_post(
            &state,
            &token,
            "/api/settings/moderation",
            serde_json::json!({"nsfw_pref": "sometimes"}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST, "invalid nsfw_pref rejected");

        let (status, _) = authed_post(
            &state,
            &token,
            "/api/settings/feed",
            serde_json::json!({"sort": "sideways"}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST, "invalid sort rejected");

        let (status, _) = authed_post(
            &state,
            &token,
            "/api/settings/feed",
            serde_json::json!({"source_mix": "tiktok"}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST, "invalid source_mix rejected");

        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await;
    }

    /// One-per-network: a second link for a provider that already has one is
    /// rejected (400); a link for a DIFFERENT network alongside one is allowed.
    #[tokio::test]
    async fn one_per_network_rejects_second_link_and_allows_other_network() {
        let Some(pool) = connect_pool().await else { return };
        let suffix = unique();
        let username = format!("opn_{suffix}");
        let user = crate::db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");

        // No links yet: both networks allowed.
        crate::api::accounts::ensure_one_per_network(&pool, user.id, "bluesky")
            .await
            .expect("first bluesky link allowed");
        crate::api::accounts::ensure_one_per_network(&pool, user.id, "mastodon")
            .await
            .expect("first mastodon link allowed");

        // Seed one Bluesky link.
        crate::db::accounts::LinkedAccountRow::create(
            &pool,
            user.id,
            "bluesky",
            &format!("did:plc:{suffix}"),
            &format!("u{suffix}"),
            "at-1",
            Some("rt-1"),
            None,
            None,
            None,
            &serde_json::json!({}),
        )
        .await
        .expect("seed bluesky link");

        // Second Bluesky link -> rejected (400 path).
        let err = crate::api::accounts::ensure_one_per_network(&pool, user.id, "bluesky")
            .await
            .expect_err("second bluesky link rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("already have a Bluesky account linked"),
            "clear message, got: {msg}"
        );

        // Mastodon alongside the one Bluesky link -> allowed (different network).
        crate::api::accounts::ensure_one_per_network(&pool, user.id, "mastodon")
            .await
            .expect("mastodon alongside one bluesky allowed");

        // After unlinking the Bluesky link, a fresh Bluesky link is allowed again.
        let links = crate::db::accounts::LinkedAccountRow::find_by_provider(&pool, user.id, "bluesky")
            .await
            .expect("find bluesky");
        for l in &links {
            crate::db::accounts::LinkedAccountRow::delete(&pool, l.id, user.id)
                .await
                .expect("unlink");
        }
        crate::api::accounts::ensure_one_per_network(&pool, user.id, "bluesky")
            .await
            .expect("re-link after unlink allowed");

        let _ = sqlx::query("DELETE FROM linked_accounts WHERE user_id = $1")
            .bind(user.id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await;
    }
}
