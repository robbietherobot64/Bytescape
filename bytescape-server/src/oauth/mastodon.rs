//! Mastodon OAuth sign-in (Phase A of add-activitypub-integration).
//!
//! Lets a Mastodon user sign in to Bytescape with their existing identity —
//! they keep their AP actor (no new actor is minted; their `@user@instance`
//! is recorded as their AP identity) and receive a Bytescape account.
//!
//! Flow (mirrors [`crate::oauth::bluesky`]):
//!   1. `POST /api/auth/mastodon/start`  — register app on the user's instance,
//!      return the authorize URL + a pending state token.
//!   2. Frontend redirects the user to the instance's `/oauth/authorize`.
//!   3. Instance redirects back to `/api/auth/mastodon/callback`.
//!   4. We exchange the code for an access token, fetch `verify_credentials`,
//!      and provision/link the Bytescape account (via `consume_temp_token`).
//!   5. Redirect to `/oauth/success?provider=mastodon`.
//!
//! Mastodon OAuth is per-instance (unlike Bluesky's one entrypoint): the user
//! supplies their instance domain (`@user@mastodon.example` → `mastodon.example`).
//!
//! This file also hosts the home-instance REST adapter (tasks.md 2.1/2.2,
//! design.md Decision 1): Mastodon-signed-up users favourite/unfavourite
//! remote statuses AS their real home actor through their instance's REST API
//! with the stored OAuth token.

use std::collections::HashMap;
use std::sync::LazyLock;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// OAuth scope for the link flow. Identity read is not enough: the account
/// acts AS its home-instance actor (favourite/unfavourite remote statuses,
/// follow/unfollow remote accounts), which needs the write grants. `read`
/// alone is the legacy scope — see [`scopes_favourites_write`] /
/// [`scopes_follows_write`] for how stored grants are told apart.
/// `write:accounts` is requested now so the Phase 6 visibility PATCH
/// (`hide_collections`) can ride the same grant.
pub const OAUTH_SCOPE: &str = "read write:favourites write:statuses write:follows write:accounts";

/// Fallback scope for instances that reject the granular grant at app
/// registration (older servers): plain `write` covers favourites + follows.
pub const OAUTH_SCOPE_FALLBACK: &str = "read write";

/// Does a granted scope string cover status favouriting? Handles both the
/// granular (`write:favourites`) and coarse (`write`) grants; `read`-only
/// (the pre-realignment grant) returns false — those accounts must re-link
/// before they can act as their home actor.
pub fn scopes_favourites_write(scope: &str) -> bool {
    scope
        .split(|c: char| c == ' ' || c == ',')
        .map(str::trim)
        .any(|s| s == "write" || s == "write:favourites" || s == "write:statuses")
}

/// Does a granted scope string cover account following? Granular
/// (`write:follows`) or coarse (`write`); a favourites-only or read-only
/// grant returns false — those accounts must re-link before their follows
/// can land on the home instance.
pub fn scopes_follows_write(scope: &str) -> bool {
    scope
        .split(|c: char| c == ' ' || c == ',')
        .map(str::trim)
        .any(|s| s == "write" || s == "write:follows")
}

/// A pending Mastodon OAuth attempt, keyed by `state`.
#[derive(Clone)]
pub struct PendingMastodon {
    pub instance: String,
    pub app_client_id: String,
    pub app_client_secret: String,
    pub code_verifier: String,
    pub invite_code: String,
    pub username_hint: String,
    /// The scope the app was registered with — the authorize URL and the token
    /// exchange must repeat it exactly.
    pub scope: String,
}

static PENDING: LazyLock<Mutex<HashMap<String, PendingMastodon>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Canonicalize + validate an instance host (strip scheme + trailing slash).
pub fn normalize_instance(raw: &str) -> Result<String, String> {
    let trimmed = raw
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    if trimmed.is_empty() || !trimmed.contains('.') || trimmed.contains('/') {
        return Err(format!("invalid instance host: {raw}"));
    }
    Ok(trimmed.to_lowercase())
}

/// Base URL for every request to a home instance. Production callers pass a
/// bare [`normalize_instance`] host (`https://` assumed); tests pass a full
/// `http://127.0.0.1:PORT` stub base, which is used verbatim.
fn rest_base(instance: &str) -> String {
    let trimmed = instance.trim().trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

/// `@user@instance` → `(user, instance)`; also accepts bare `user@instance`.
pub fn split_handle(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim().trim_start_matches('@');
    let (user, instance) = trimmed.split_once('@')?;
    if user.is_empty() || instance.is_empty() || instance.contains('@') {
        return None;
    }
    Some((user.to_string(), normalize_instance(instance).ok()?))
}

/// Register an OAuth app on the instance and return the pending state + the
/// authorize URL for the frontend to redirect to.
pub async fn start(
    http: &reqwest::Client,
    instance: &str,
    client_name: &str,
    redirect_uri: &str,
    invite_code: &str,
    username_hint: &str,
) -> Result<(String, String), String> {
    let instance = normalize_instance(instance)?;

    // Mastodon app registration (POST /api/v1/apps). Scopes: identity read +
    // favourites write — the account acts AS its home-instance actor when it
    // likes/unlikes remote statuses, which needs `write:favourites`. Older
    // servers reject the granular grant: fall back to coarse `read write`.
    let mut registered_scope = OAUTH_SCOPE;
    let mut app: Value = http
        .post(format!("{}/api/v1/apps", rest_base(&instance)))
        .json(&json!({
            "client_name": client_name,
            "redirect_uris": redirect_uri,
            "scopes": OAUTH_SCOPE,
            "website": redirect_uri,
        }))
        .send()
        .await
        .map_err(|e| format!("app registration request: {e}"))?
        .error_for_status()
        .map_err(|e| format!("app registration rejected: {e}"))?
        .json()
        .await
        .map_err(|e| format!("app registration parse: {e}"))?;
    if app.get("client_id").is_none() {
        // Registration with the granular scope failed or returned an error
        // body without a client_id — retry with the coarse scope.
        registered_scope = OAUTH_SCOPE_FALLBACK;
        app = http
            .post(format!("{}/api/v1/apps", rest_base(&instance)))
            .json(&json!({
                "client_name": client_name,
                "redirect_uris": redirect_uri,
                "scopes": OAUTH_SCOPE_FALLBACK,
                "website": redirect_uri,
            }))
            .send()
            .await
            .map_err(|e| format!("app registration request: {e}"))?
            .error_for_status()
            .map_err(|e| format!("app registration rejected: {e}"))?
            .json()
            .await
            .map_err(|e| format!("app registration parse: {e}"))?;
    }
    // The authorize URL and the token exchange must carry the exact scope the
    // app was registered with, or Mastodon rejects the grant.
    let scope = registered_scope.to_string();

    let client_id = app["client_id"].as_str().ok_or("missing client_id")?.to_string();
    let client_secret = app["client_secret"].as_str().ok_or("missing client_secret")?.to_string();
    let code_verifier = generate_code_verifier();

    let state = uuid::Uuid::new_v4().to_string();
    PENDING.lock().insert(
        state.clone(),
        PendingMastodon {
            instance: instance.clone(),
            app_client_id: client_id.clone(),
            app_client_secret: client_secret.clone(),
            code_verifier: code_verifier.clone(),
            invite_code: invite_code.to_string(),
            username_hint: username_hint.to_string(),
            scope: scope.clone(),
        },
    );

    let authorize_url = format!(
        "{}/oauth/authorize?response_type=code&client_id={client_id}\
         &redirect_uri={redirect_uri}&scope={}&state={state}",
        rest_base(&instance),
        urlencoding::encode(&scope)
    );
    Ok((state, authorize_url))
}

/// Consume a pending state (returns None if unknown/expired).
pub fn consume_pending(state: &str) -> Option<PendingMastodon> {
    PENDING.lock().remove(state)
}

/// Exchange the authorization code for an access token.
#[derive(Deserialize, Serialize)]
pub struct MastodonTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub scope: String,
    /// Rotation: an instance may hand out a new refresh token; absent means
    /// the previous one stays valid.
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Access-token lifetime in seconds; absent = non-expiring (the Mastodon
    /// default).
    #[serde(default)]
    pub expires_in: Option<i64>,
}

pub async fn exchange_code(
    http: &reqwest::Client,
    pending: &PendingMastodon,
    redirect_uri: &str,
    code: &str,
) -> Result<MastodonTokenResponse, String> {
    http.post(format!("{}/oauth/token", rest_base(&pending.instance)))
        .json(&json!({
            "grant_type": "authorization_code",
            "code": code,
            "client_id": pending.app_client_id,
            "client_secret": pending.app_client_secret,
            "redirect_uri": redirect_uri,
            "scope": pending.scope,
        }))
        .send()
        .await
        .map_err(|e| format!("token request: {e}"))?
        .error_for_status()
        .map_err(|e| format!("token exchange rejected: {e}"))?
        .json()
        .await
        .map_err(|e| format!("token parse: {e}"))
}

/// The verified-account payload we care about from `/api/v1/accounts/verify_credentials`.
#[derive(Debug, Clone)]
pub struct MastodonAccount {
    /// Opaque home-instance account id — the path segment every
    /// `/api/v1/accounts/{id}/…` call needs (e.g. the follow graph).
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub acct: String, // `@user@instance` (or `user` for local)
    pub avatar: Option<String>,
    pub header: Option<String>,
    pub note: String,
    pub url: String,
    /// The home instance's own collections-visibility state
    /// (`hide_collections` on the account entity) — the authoritative value
    /// we surface and proxy (identity-graph Phase 6).
    pub hide_collections: bool,
}

/// Fetch the signed-in user's identity (`GET /api/v1/accounts/verify_credentials`).
pub async fn verify_credentials(
    http: &reqwest::Client,
    instance: &str,
    access_token: &str,
) -> Result<MastodonAccount, String> {
    let me: Value = http
        .get(format!("{}/api/v1/accounts/verify_credentials", rest_base(instance)))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("verify_credentials request: {e}"))?
        .error_for_status()
        .map_err(|e| format!("verify_credentials rejected: {e}"))?
        .json()
        .await
        .map_err(|e| format!("verify_credentials parse: {e}"))?;

    let username = me["username"].as_str().unwrap_or_default().to_string();
    let acct = me["acct"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{username}@{instance}"));

    Ok(MastodonAccount {
        // Ids are strings on Mastodon but tolerated as numbers by some forks.
        id: me["id"]
            .as_str()
            .map(str::to_string)
            .or_else(|| me["id"].as_u64().map(|n| n.to_string()))
            .unwrap_or_default(),
        username,
        display_name: me["display_name"].as_str().unwrap_or("").to_string(),
        acct,
        avatar: me["avatar"].as_str().map(|s| s.to_string()),
        header: me["header"].as_str().map(|s| s.to_string()),
        note: me["note"].as_str().unwrap_or("").to_string(),
        url: me["url"].as_str().unwrap_or("").to_string(),
        hide_collections: me["hide_collections"].as_bool().unwrap_or(false),
    })
}

// ---------------------------------------------------------------------------
// Follow-graph reads (`GET /api/v1/accounts/{id}/following|followers`).
//
// The `read` grant the link flow already stores covers these; nothing below
// writes to the home instance. Both walks are bounded: Mastodon caps `limit`
// at 80 and pages through the `Link: <…>; rel="next"` header, so the caller
// gets one flat list without knowing the pagination.
// ---------------------------------------------------------------------------

/// Page size for the graph walks (Mastodon's documented maximum).
const GRAPH_PAGE_LIMIT: usize = 80;
/// Hard page cap per walk. A 33-follow account needs one page; the cap keeps a
/// pathological instance from streaming without end into a background task.
const GRAPH_MAX_PAGES: usize = 3;

/// One account from the home instance's following / followers list.
#[derive(Debug, Clone, PartialEq)]
pub struct MastodonFollowAccount {
    /// Canonical `user@host`: remote accounts already come back that way, an
    /// account local to the instance comes back bare and is qualified here.
    pub acct: String,
    /// Home-instance profile URL when the instance advertises one.
    pub url: Option<String>,
    pub display_name: String,
}

/// The `rel="next"` URL of a pagination `Link` header, if any.
///
/// Mastodon sends one comma-separated header per page
/// (`<…?max_id=…>; rel="next", <…?min_id=…>; rel="prev"`). Only the `next`
/// relation's bracketed URL is returned — a malformed header is `None`, an
/// URL is never guessed.
pub fn next_page_url(link: Option<&str>) -> Option<String> {
    let link = link?;
    link.split(',').find_map(|part| {
        let mut segments = part.split(';');
        let target = segments.next()?.trim().strip_prefix('<')?.strip_suffix('>')?;
        segments
            .any(|s| matches!(s.trim(), "rel=\"next\"" | "rel=next"))
            .then(|| target.to_string())
    })
}

/// Normalize one list entry. `acct` is qualified with `instance` when bare
/// (Mastodon returns the bare username for accounts local to that instance).
fn follow_account(instance: &str, item: &Value) -> Option<MastodonFollowAccount> {
    let acct = item["acct"]
        .as_str()
        .unwrap_or("")
        .trim()
        .trim_start_matches('@')
        .to_lowercase();
    if acct.is_empty() {
        return None;
    }
    let acct = if acct.contains('@') {
        acct
    } else {
        format!("{acct}@{}", instance.to_lowercase())
    };
    let display_name = item["display_name"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| acct.split('@').next().unwrap_or(&acct).to_string());
    Some(MastodonFollowAccount {
        url: item["url"]
            .as_str()
            .filter(|u| u.starts_with("https://"))
            .map(str::to_string),
        acct,
        display_name,
    })
}

/// Walk one collection of the token's own account: verify the identity (for
/// its id), then page through `{kind}` until the list ends or the page cap is
/// hit. `kind` is the literal path segment `following` or `followers`.
async fn walk_account_graph(
    http: &reqwest::Client,
    instance: &str,
    token: &str,
    kind: &str,
) -> Result<Vec<MastodonFollowAccount>, MastodonApiError> {
    let me = verify_credentials(http, instance, token)
        .await
        .map_err(MastodonApiError::Remote)?;
    if me.id.is_empty() {
        return Err(MastodonApiError::Remote(
            "verify_credentials returned no account id".to_string(),
        ));
    }

    let mut next = Some(format!(
        "{}/api/v1/accounts/{}/{kind}?limit={GRAPH_PAGE_LIMIT}",
        rest_base(instance),
        me.id
    ));
    let mut out = Vec::new();
    let mut pages = 0usize;
    while let Some(page_url) = next.take() {
        if pages >= GRAPH_MAX_PAGES {
            break;
        }
        pages += 1;
        let resp = http
            .get(&page_url)
            .bearer_auth(token)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| MastodonApiError::Remote(format!("{kind} request: {e}")))?;
        // The Link header is consumed with the response body, so read it first.
        let link = resp
            .headers()
            .get(reqwest::header::LINK)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body = rest_json(resp, kind).await?;
        let items = body
            .as_array()
            .ok_or_else(|| MastodonApiError::Remote(format!("{kind} response is not a list")))?;
        out.extend(items.iter().filter_map(|item| follow_account(instance, item)));
        next = next_page_url(link.as_deref());
    }
    Ok(out)
}

/// Accounts the token's owner follows on their home instance
/// (`GET /api/v1/accounts/{id}/following`, at most [`GRAPH_MAX_PAGES`] pages of
/// [`GRAPH_PAGE_LIMIT`]). Needs only the stored `read` grant.
pub async fn following_accounts(
    http: &reqwest::Client,
    instance: &str,
    token: &str,
) -> Result<Vec<MastodonFollowAccount>, MastodonApiError> {
    walk_account_graph(http, instance, token, "following").await
}

/// Accounts that follow the token's owner
/// (`GET /api/v1/accounts/{id}/followers`). Consumed by the merged follower
/// read model (identity-graph-unification Phase 3).
pub async fn follower_accounts(
    http: &reqwest::Client,
    instance: &str,
    token: &str,
) -> Result<Vec<MastodonFollowAccount>, MastodonApiError> {
    walk_account_graph(http, instance, token, "followers").await
}

/// RFC 7636 code verifier (43–128 chars).
fn generate_code_verifier() -> String {
    use base64::Engine;
    let bytes: [u8; 32] = rand::random();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

// ---------------------------------------------------------------------------
// Home-instance REST adapter (tasks.md 2.1/2.2 — design.md Decision 1).
//
// Mastodon-signed-up users act AS their real home-instance actor: favourite
// and unfavourite go through the home instance's REST API with the stored
// OAuth token. A grant that can no longer act (revoked token, legacy
// read-only scope) surfaces a visible "re-link" error — a silent fallback to
// the minted actor is forbidden (design.md Risks), it would resurrect the
// second-identity problem.
// ---------------------------------------------------------------------------

/// Errors from the home-instance REST adapter. The variant is what the UI
/// acts on: [`Self::ReauthRequired`] is a consent problem only the user can
/// fix (re-link the Mastodon account), [`Self::NotFound`] is a missing
/// status, [`Self::Remote`] is a transient/remote failure.
#[derive(Debug)]
pub enum MastodonApiError {
    /// The stored grant cannot act as the home actor — the user must re-link
    /// their Mastodon account (message is human-visible).
    ReauthRequired(String),
    /// The status does not exist on the home instance.
    NotFound(String),
    /// Transport, remote, or parse failure — retryable, no re-link needed.
    Remote(String),
}

impl std::fmt::Display for MastodonApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReauthRequired(m) | Self::NotFound(m) | Self::Remote(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for MastodonApiError {}

/// Map a home-instance REST response to JSON, or to the error class the UI
/// acts on. 401/403 (and any explicit scope rejection) mean the stored token
/// can no longer act as the home actor → re-link; 404 is a missing status;
/// everything else is remote.
async fn rest_json(resp: reqwest::Response, action: &str) -> Result<Value, MastodonApiError> {
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| MastodonApiError::Remote(format!("{action}: reading response: {e}")))?;
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(MastodonApiError::ReauthRequired(format!(
            "{action}: the home instance rejected the stored token — please re-link your Mastodon account"
        )));
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(MastodonApiError::NotFound(format!("{action}: status not found")));
    }
    if !status.is_success() {
        // Some instances reject an insufficient grant with a plain error body
        // instead of 403 — that is still a consent problem.
        if body.to_lowercase().contains("scope") {
            return Err(MastodonApiError::ReauthRequired(format!(
                "{action}: the grant is missing write:favourites — please re-link your Mastodon account"
            )));
        }
        return Err(MastodonApiError::Remote(format!(
            "{action}: home instance returned HTTP {status}"
        )));
    }
    serde_json::from_str(&body)
        .map_err(|e| MastodonApiError::Remote(format!("{action}: parse: {e}")))
}

/// Favourite a status as the real home-instance actor
/// (`POST /api/v1/statuses/{id}/favourite`). Returns the status JSON.
pub async fn favourite_status(
    http: &reqwest::Client,
    instance: &str,
    token: &str,
    status_id: &str,
) -> Result<Value, MastodonApiError> {
    let url = format!("{}/api/v1/statuses/{status_id}/favourite", rest_base(instance));
    let resp = http
        .post(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| MastodonApiError::Remote(format!("favourite request: {e}")))?;
    rest_json(resp, "favourite").await
}

/// Unfavourite a status as the real home-instance actor
/// (`POST /api/v1/statuses/{id}/unfavourite`). Returns the status JSON.
/// NOTE: this is Mastodon's documented route (verified live on 4.7.5);
/// `DELETE /api/v1/statuses/{id}/favourite` is NOT routed on Mastodon 4.x —
/// it returns the framework's HTML 404 while the favourite stands.
pub async fn unfavourite_status(
    http: &reqwest::Client,
    instance: &str,
    token: &str,
    status_id: &str,
) -> Result<Value, MastodonApiError> {
    let url = format!(
        "{}/api/v1/statuses/{status_id}/unfavourite",
        rest_base(instance)
    );
    let resp = http
        .post(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| MastodonApiError::Remote(format!("unfavourite request: {e}")))?;
    rest_json(resp, "unfavourite").await
}

/// Post a new status as the real home-instance actor (`POST /api/v1/statuses`).
/// This is the "M-linked users post through their home instance" model
/// (design.md Decision 1) — the same actor that favourites/unfavourites. The
/// response carries `id` + `url` for the cross-post record. Requires a grant
/// with `write:statuses` (or coarse `write`); a favourites-only grant is
/// rejected by the instance → [`MastodonApiError::ReauthRequired`].
pub async fn post_status(
    http: &reqwest::Client,
    instance: &str,
    token: &str,
    status: &str,
    visibility: &str,
) -> Result<Value, MastodonApiError> {
    let url = format!("{}/api/v1/statuses", rest_base(instance));
    let resp = http
        .post(&url)
        .bearer_auth(token)
        .json(&serde_json::json!({ "status": status, "visibility": visibility }))
        .send()
        .await
        .map_err(|e| MastodonApiError::Remote(format!("post status: {e}")))?;
    rest_json(resp, "post status").await
}

/// Delete a status the home-instance actor authored
/// (`DELETE /api/v1/statuses/{id}`). Used to cascade-delete a federated copy
/// when the parent Bytescape post is removed. Requires `write:statuses`.
pub async fn delete_status(
    http: &reqwest::Client,
    instance: &str,
    token: &str,
    status_id: &str,
) -> Result<Value, MastodonApiError> {
    let url = format!("{}/api/v1/statuses/{status_id}", rest_base(instance));
    let resp = http
        .delete(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| MastodonApiError::Remote(format!("delete status: {e}")))?;
    rest_json(resp, "delete status").await
}

/// Numeric-status-id fallback (tasks.md 2.2): the last non-empty path segment
/// of a canonical status URI (`/users/x/statuses/123`, `/@x/123`) when it is
/// all ASCII digits. Trailing slashes are fine; anything else is None — an id
/// is never guessed.
pub fn status_id_from_uri(uri: &str) -> Option<String> {
    let path = uri.split(['?', '#']).next().unwrap_or(uri);
    let segment = path.trim_end_matches('/').rsplit('/').next()?;
    if !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_digit()) {
        Some(segment.to_string())
    } else {
        None
    }
}

/// Host part of a URI or instance spec, scheme and port stripped
/// (`https://host:4430/x` → `host`).
fn host_of(s: &str) -> &str {
    let after_scheme = s.split_once("://").map(|(_, rest)| rest).unwrap_or(s);
    let host_port = after_scheme.split('/').next().unwrap_or(after_scheme);
    host_port.split(':').next().unwrap_or(host_port)
}

/// Resolve a status URI to the home instance's own status id via the lookup
/// endpoint (`GET /api/v1/statuses/lookup?uri=…`). NOTE: Mastodon removed this
/// endpoint upstream — 4.x servers (verified on 4.7.5 and nightly) return 404
/// for every URI, including their own local statuses. It remains only as a
/// fast path where a server still implements it; [`resolve_status_id`] falls
/// back to [`search_status_id`] when it is gone.
pub async fn lookup_status_id(
    http: &reqwest::Client,
    instance: &str,
    token: &str,
    uri: &str,
) -> Result<String, MastodonApiError> {
    let url = format!(
        "{}/api/v1/statuses/lookup?uri={}",
        rest_base(instance),
        urlencoding::encode(uri)
    );
    let resp = http
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| MastodonApiError::Remote(format!("status lookup request: {e}")))?;
    let body = rest_json(resp, "status lookup").await?;
    body.get("id")
        .and_then(Value::as_str)
        .map(String::from)
        .or_else(|| body.get("id").and_then(Value::as_u64).map(|n| n.to_string()))
        .ok_or_else(|| MastodonApiError::Remote("status lookup response has no id".to_string()))
}

/// URI → home-instance status id via the search API
/// (`GET /api/v2/search?q=<uri>&resolve=true`): the working resolution path on
/// Mastodon 4.x, where the lookup endpoint is gone. `resolve=true` makes the
/// home instance pull the remote status in and return its home-LOCAL id.
/// `type=` is deliberately omitted for pre-4.1 compatibility. ONLY an exact
/// `uri` match is accepted (CR-1: instances may return unmatched statuses for
/// a full-URI query — a loose hit would favourite an unrelated home status);
/// no match is `NotFound`, never a guess.
pub async fn search_status_id(
    http: &reqwest::Client,
    instance: &str,
    token: &str,
    uri: &str,
) -> Result<String, MastodonApiError> {
    let url = format!(
        "{}/api/v2/search?q={}&resolve=true",
        rest_base(instance),
        urlencoding::encode(uri)
    );
    let resp = http
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| MastodonApiError::Remote(format!("status search request: {e}")))?;
    let body = rest_json(resp, "status search").await?;
    let statuses = body
        .get("statuses")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            MastodonApiError::Remote("status search response has no statuses array".to_string())
        })?;
    let hit = statuses
        .iter()
        .find(|s| s.get("uri").and_then(Value::as_str) == Some(uri));
    hit.and_then(|s| s.get("id"))
        .and_then(|id| {
            id.as_str()
                .map(String::from)
                .or_else(|| id.as_u64().map(|n| n.to_string()))
        })
        .ok_or_else(|| MastodonApiError::NotFound("status search: status not found".to_string()))
}

/// URI → home-instance status id, in order:
/// 1. the lookup endpoint (fast path where the server still has it),
/// 2. [`search_status_id`] when lookup is gone (NotFound) or flaky (Remote),
/// 3. the numeric path id — ONLY for same-host URIs. A foreign-host id is not
///    ours to guess (design.md Risks): the URI's numeric tail names the status
///    on its ORIGIN server, and the home instance's local id for a remote
///    status differs (verified live), so both failures surface instead.
pub async fn resolve_status_id(
    http: &reqwest::Client,
    instance: &str,
    token: &str,
    uri: &str,
) -> Result<String, MastodonApiError> {
    match lookup_status_id(http, instance, token, uri).await {
        Ok(id) => return Ok(id),
        // A consent problem is terminal: every later call would fail the
        // same way, and the numeric fallback must never mask a re-link need.
        Err(e @ MastodonApiError::ReauthRequired(_)) => return Err(e),
        Err(_) => {}
    }
    let search_err = match search_status_id(http, instance, token, uri).await {
        Ok(id) => return Ok(id),
        Err(e @ MastodonApiError::ReauthRequired(_)) => return Err(e),
        Err(e) => e,
    };
    if host_of(uri) == host_of(instance) {
        if let Some(id) = status_id_from_uri(uri) {
            return Ok(id);
        }
    }
    // Both remote paths failed and no same-host fallback applies — surface
    // the search error (the primary resolution path's cause).
    Err(search_err)
}

/// Refresh a linked Mastodon account's OAuth token when expired or expiring —
/// the home-REST mirror of `api/feed/mod.rs::refresh_if_expired`. A stored
/// `token_expires_at` of None means the instance issued a non-expiring token
/// (the Mastodon default) and it is used as-is. A dead grant is a visible
/// "re-link" error, never a silent fallback (design.md Risks).
pub async fn refresh_mastodon_token(
    db: &crate::db::DbPool,
    http: &reqwest::Client,
    acct: &crate::db::accounts::LinkedAccountRow,
) -> Result<String, MastodonApiError> {
    let expiring = acct
        .token_expires_at
        .map(|t| t < chrono::Utc::now() + chrono::Duration::seconds(60))
        .unwrap_or(false);
    if !expiring {
        return Ok(acct.access_token.clone());
    }
    let refresh_token = acct.refresh_token.as_deref().ok_or_else(|| {
        MastodonApiError::ReauthRequired(
            "the Mastodon token expired and no refresh token is stored — please re-link your Mastodon account"
                .to_string(),
        )
    })?;
    let instance = acct
        .metadata
        .get("instance")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            MastodonApiError::Remote("linked Mastodon account has no instance in metadata".to_string())
        })?;

    let mut form = json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
    });
    // The app credentials were recorded at link time; include them when present.
    for key in ["client_id", "client_secret"] {
        if let Some(v) = acct.metadata.get(key).and_then(Value::as_str) {
            form[key] = v.into();
        }
    }
    if let Some(scope) = acct.scope.as_deref() {
        form["scope"] = scope.into();
    }

    let resp = http
        .post(format!("{}/oauth/token", rest_base(instance)))
        .json(&form)
        .send()
        .await
        .map_err(|e| MastodonApiError::Remote(format!("token refresh request: {e}")))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| MastodonApiError::Remote(format!("token refresh response: {e}")))?;
    if !status.is_success() {
        // The grant is dead (revoked or already rotated away) — the user
        // must re-link.
        return Err(MastodonApiError::ReauthRequired(format!(
            "the home instance rejected the token refresh (HTTP {status}) — please re-link your Mastodon account"
        )));
    }
    let parsed: MastodonTokenResponse = serde_json::from_str(&body)
        .map_err(|e| MastodonApiError::Remote(format!("token refresh parse: {e}")))?;

    // Persist the rotation — without it every call re-enters refresh. An
    // absent refresh_token keeps the old one; absent expires_in keeps the
    // token non-expiring.
    let token_expires_at = parsed
        .expires_in
        .map(|s| chrono::Utc::now() + chrono::Duration::seconds(s));
    // SEC-1: the rotation re-SEALS the new tokens before binding — only
    // `enc:v1:` ciphertext ever reaches the database (the same contract as
    // `LinkedAccountRow::create`; a plaintext write here would undo the
    // at-rest encryption until the next boot's backfill re-seals it).
    let secret = crate::db::accounts::jwt_secret();
    sqlx::query(
        "UPDATE linked_accounts SET access_token = $1, refresh_token = $2, token_expires_at = $3, updated_at = NOW() WHERE id = $4",
    )
    .bind(crate::secret::seal(&parsed.access_token, &secret))
    .bind(crate::secret::seal(
        parsed.refresh_token.as_deref().unwrap_or(refresh_token),
        &secret,
    ))
    .bind(token_expires_at)
    .bind(acct.id)
    .execute(db)
    .await
    .map_err(|e| MastodonApiError::Remote(format!("token refresh persist: {e}")))?;

    Ok(parsed.access_token)
}

/// Consent gate for acting as the home actor (tasks.md 1.2): the stored grant
/// must cover favourites write. Legacy `read`-only links get the visible
/// re-link error — never a silent fallback to another identity.
pub fn ensure_favourites_scope(scope: Option<&str>) -> Result<(), MastodonApiError> {
    match scope {
        Some(s) if scopes_favourites_write(s) => Ok(()),
        _ => Err(MastodonApiError::ReauthRequired(
            "your Mastodon link cannot write favourites — please re-link your Mastodon account to grant write:favourites"
                .to_string(),
        )),
    }
}

/// Consent gate for follows (mirror of [`ensure_favourites_scope`]): a grant
/// that cannot follow demands a visible, instruction-focused re-link — never
/// a silent fallback to another identity (design.md Risks).
pub fn ensure_follows_scope(scope: Option<&str>) -> Result<(), MastodonApiError> {
    match scope {
        Some(s) if scopes_follows_write(s) => Ok(()),
        _ => Err(MastodonApiError::ReauthRequired(
            "to follow from your Mastodon account, re-link it in Settings to grant follow permission (write:follows)"
                .to_string(),
        )),
    }
}

/// Resolve `user@host` to the home instance's local account id: direct lookup
/// first (`GET /api/v1/accounts/lookup?acct=…`), resolved search as the
/// fallback (`GET /api/v2/search?resolve=true`) for instances without the
/// lookup endpoint. The id is what `POST /api/v1/accounts/{id}/follow` needs;
/// following by URI is the deprecated API and this keeps one call shape.
pub async fn lookup_account_id(
    http: &reqwest::Client,
    instance: &str,
    token: &str,
    acct: &str,
) -> Result<String, MastodonApiError> {
    let acct = acct.trim().trim_start_matches('@');
    let base = rest_base(instance);
    let url = format!(
        "{base}/api/v1/accounts/lookup?acct={}",
        urlencoding::encode(acct)
    );
    match http.get(&url).bearer_auth(token).send().await {
        Ok(resp) if resp.status().is_success() => {
            let doc: Value = resp
                .json()
                .await
                .map_err(|e| MastodonApiError::Remote(format!("account lookup: parse: {e}")))?;
            if let Some(id) = doc.get("id").and_then(|v| v.as_str()) {
                return Ok(id.to_string());
            }
        }
        Ok(resp)
            if resp.status() == reqwest::StatusCode::UNAUTHORIZED
                || resp.status() == reqwest::StatusCode::FORBIDDEN =>
        {
            return Err(MastodonApiError::ReauthRequired(
                "account lookup: the home instance rejected the stored token — please re-link your Mastodon account"
                    .to_string(),
            ));
        }
        // Missing endpoint / unknown acct / transient: fall through to
        // resolved search, which covers the same ground on older instances.
        _ => {}
    }
    let url = format!(
        "{base}/api/v2/search?q=acct%3A{q}&resolve=true&type=accounts&limit=1",
        q = urlencoding::encode(acct)
    );
    let resp = http
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| MastodonApiError::Remote(format!("account search: {e}")))?;
    let doc = rest_json(resp, "account search").await?;
    let id = doc
        .pointer("/accounts/0/id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            MastodonApiError::NotFound(format!("no account {acct} known to {instance}"))
        })?;
    Ok(id.to_string())
}

/// Follow an account as the real home-instance actor
/// (`POST /api/v1/accounts/{id}/follow`). Returns the relationship JSON.
/// Named `home_*` to stay distinct from the follow-graph parser above.
pub async fn home_follow_account(
    http: &reqwest::Client,
    instance: &str,
    token: &str,
    account_id: &str,
) -> Result<Value, MastodonApiError> {
    let url = format!(
        "{}/api/v1/accounts/{account_id}/follow",
        rest_base(instance)
    );
    let resp = http
        .post(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| MastodonApiError::Remote(format!("follow request: {e}")))?;
    rest_json(resp, "follow").await
}

/// Unfollow as the real home-instance actor
/// (`POST /api/v1/accounts/{id}/unfollow`).
pub async fn home_unfollow_account(
    http: &reqwest::Client,
    instance: &str,
    token: &str,
    account_id: &str,
) -> Result<Value, MastodonApiError> {
    let url = format!(
        "{}/api/v1/accounts/{account_id}/unfollow",
        rest_base(instance)
    );
    let resp = http
        .post(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| MastodonApiError::Remote(format!("unfollow request: {e}")))?;
    rest_json(resp, "unfollow").await
}

/// Consent gate for account-setting writes (Phase 6 visibility parity): a
/// grant that cannot write the account demands the visible re-link.
pub fn ensure_accounts_scope(scope: Option<&str>) -> Result<(), MastodonApiError> {
    let covers = scope
        .map(|s| {
            s.split(|c: char| c == ' ' || c == ',')
                .map(str::trim)
                .any(|x| x == "write" || x == "write:accounts")
        })
        .unwrap_or(false);
    if covers {
        Ok(())
    } else {
        Err(MastodonApiError::ReauthRequired(
            "to manage your Mastodon visibility from here, re-link your Mastodon account to grant profile settings permission (write:accounts)"
                .to_string(),
        ))
    }
}

/// Proxy the home instance's collections-visibility setting
/// (`PATCH /api/v1/accounts/update_credentials`, form field
/// `hide_collections`). The home value is authoritative — callers read it
/// back via [`verify_credentials`] and never silently diverge.
pub async fn update_credentials_hide(
    http: &reqwest::Client,
    instance: &str,
    token: &str,
    hide: bool,
) -> Result<Value, MastodonApiError> {
    let url = format!(
        "{}/api/v1/accounts/update_credentials",
        rest_base(instance)
    );
    let resp = http
        .patch(&url)
        .bearer_auth(token)
        .form(&[("hide_collections", if hide { "true" } else { "false" })])
        .send()
        .await
        .map_err(|e| MastodonApiError::Remote(format!("update_credentials request: {e}")))?;
    rest_json(resp, "update_credentials").await
}

#[cfg(test)]
pub(crate) use tests::stub_http;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ap::testutil::{connect_pool, unique};
    use crate::db::accounts::LinkedAccountRow;
    use crate::db::users::UserRow;
    use std::io::{Read, Write};
    use std::sync::Arc;

    #[test]
    fn split_handle_parses_full_and_bare_forms() {
        assert_eq!(
            split_handle("@alice@mastodon.example"),
            Some(("alice".into(), "mastodon.example".into()))
        );
        assert_eq!(
            split_handle("bob@social.example"),
            Some(("bob".into(), "social.example".into()))
        );
        // Rejects malformed.
        assert_eq!(split_handle("@alice"), None);
        assert_eq!(split_handle("@a@b@c.example"), None);
    }

    #[test]
    fn normalize_instance_strips_scheme_and_slash() {
        assert_eq!(normalize_instance("https://Mastodon.Example/").unwrap(), "mastodon.example");
        assert_eq!(normalize_instance("social.example").unwrap(), "social.example");
        assert!(normalize_instance("nodots").is_err());
        assert!(normalize_instance("has/slash.com").is_err());
    }

    /// One fixed-response HTTP stub: accepts `calls` connections, captures
    /// every request head into one string, answers each with the same
    /// `status_line`/`body` (correct Content-Length, `Connection: close`).
    /// Returns `(base_url, captured_heads, thread)` — pass `base_url` as the
    /// `instance` argument.
    pub(crate) fn stub_http(
        status_line: &str,
        body: &str,
        calls: usize,
    ) -> (String, Arc<Mutex<String>>, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind stub listener");
        let base = format!("http://{}", listener.local_addr().expect("stub addr"));
        let heads = Arc::new(Mutex::new(String::new()));
        let captured = Arc::clone(&heads);
        let status_line = status_line.to_string();
        let body = body.to_string();
        let thread = std::thread::spawn(move || {
            let response = format!(
                "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            for _ in 0..calls {
                let Ok((mut stream, _)) = listener.accept() else { return };
                // Read until the blank line that ends the request head.
                let mut head = Vec::new();
                let mut buf = [0u8; 1024];
                loop {
                    let Ok(n) = stream.read(&mut buf) else { break };
                    if n == 0 {
                        break;
                    }
                    head.extend_from_slice(&buf[..n]);
                    if head.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                captured.lock().push_str(&String::from_utf8_lossy(&head));
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (base, heads, thread)
    }

    /// Fixed-response twin of [`stub_http`] with a per-request response
    /// sequence: `responses[i]` = `(status_line, body)` for the i-th accepted
    /// connection (extra connections repeat the last). Captures every request
    /// head, in order, into one string.
    fn stub_http_seq(
        responses: Vec<(&str, &str)>,
    ) -> (String, Arc<Mutex<String>>, std::thread::JoinHandle<()>) {
        stub_http_seq_hdrs(
            responses
                .into_iter()
                .map(|(status_line, body)| (status_line, Vec::new(), body))
                .collect(),
        )
    }

    /// [`stub_http_seq`] with per-response extra headers:
    /// `responses[i] = (status_line, extra_headers, body)`. The follow-graph
    /// walks page through the `Link` header, which the plain stubs omit.
    fn stub_http_seq_hdrs(
        responses: Vec<(&str, Vec<(&str, &str)>, &str)>,
    ) -> (String, Arc<Mutex<String>>, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind stub listener");
        let base = format!("http://{}", listener.local_addr().expect("stub addr"));
        let heads = Arc::new(Mutex::new(String::new()));
        let captured = Arc::clone(&heads);
        let responses: Vec<String> = responses
            .into_iter()
            .map(|(status_line, headers, body)| {
                let mut out = format!("{status_line}\r\nContent-Type: application/json\r\n");
                for (name, value) in headers {
                    // `__BASE__` is the stub's own origin — a Link header
                    // cannot name it before the listener is bound.
                    out.push_str(&format!("{name}: {}\r\n", value.replace("__BASE__", &base)));
                }
                out.push_str(&format!(
                    "Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                ));
                out
            })
            .collect();
        let n = responses.len();
        let thread = std::thread::spawn(move || {
            // Bounded accept: a call-count regression must fail the head
            // assertions, not hang the join (8s arrival window per call).
            if listener.set_nonblocking(true).is_err() {
                return;
            }
            for i in 0..n {
                let mut accepted = None;
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
                loop {
                    match listener.accept() {
                        Ok(pair) => {
                            accepted = Some(pair);
                            break;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            if std::time::Instant::now() >= deadline {
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(25));
                        }
                        Err(_) => break,
                    }
                }
                let Some((mut stream, _)) = accepted else { return };
                let _ = stream.set_nonblocking(false);
                let mut head = Vec::new();
                let mut buf = [0u8; 1024];
                loop {
                    let Ok(n) = stream.read(&mut buf) else { break };
                    if n == 0 {
                        break;
                    }
                    head.extend_from_slice(&buf[..n]);
                    if head.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                captured.lock().push_str(&String::from_utf8_lossy(&head));
                let _ = stream.write_all(responses[i].as_bytes());
            }
        });
        (base, heads, thread)
    }

    /// Task 2.1: favourite posts to the home instance's status endpoint.
    #[tokio::test]
    async fn favourite_posts_to_home_instance() {
        let (base, heads, thread) =
            stub_http("HTTP/1.1 200 OK", r#"{"id":"1","favourited":true}"#, 1);
        let http = reqwest::Client::new();
        let status = favourite_status(&http, &base, "tok", "1").await.expect("favourite");
        assert_eq!(status["favourited"], json!(true));
        let head = heads.lock().clone();
        assert!(head.contains("POST /api/v1/statuses/1/favourite"), "head: {head}");
        let _ = thread.join();
    }

    /// Task 2.1: unfavourite posts to the documented unfavourite endpoint
    /// (DELETE /favourite is not routed on Mastodon 4.x — live-verified).
    #[tokio::test]
    async fn unfavourite_posts_to_unfavourite_endpoint() {
        let (base, heads, thread) =
            stub_http("HTTP/1.1 200 OK", r#"{"id":"1","favourited":false}"#, 1);
        let http = reqwest::Client::new();
        let status = unfavourite_status(&http, &base, "tok", "1").await.expect("unfavourite");
        assert_eq!(status["favourited"], json!(false));
        let head = heads.lock().clone();
        assert!(head.contains("POST /api/v1/statuses/1/unfavourite"), "head: {head}");
        let _ = thread.join();
    }

    /// Task 2.2: resolution uses the lookup endpoint's local id.
    #[tokio::test]
    async fn resolve_status_id_uses_lookup_id() {
        let (base, heads, thread) = stub_http("HTTP/1.1 200 OK", r#"{"id":"42"}"#, 1);
        let http = reqwest::Client::new();
        let id = resolve_status_id(&http, &base, "tok", "https://remote.test/users/x/statuses/42")
            .await
            .expect("lookup resolves");
        assert_eq!(id, "42");
        let head = heads.lock().clone();
        assert!(head.contains("GET /api/v1/statuses/lookup?uri="), "head: {head}");
        assert!(head.contains("uri=https%3A%2F%2Fremote.test"), "uri must be urlencoded, head: {head}");
        let _ = thread.join();
    }

    /// Task 2.2: same-host numeric fallback when lookup AND search are
    /// unavailable (the numeric tail is the URI's own host's id — safe here).
    #[tokio::test]
    async fn resolve_falls_back_to_numeric_id_for_same_host_uri() {
        let (base, heads, thread) =
            stub_http("HTTP/1.1 404 Not Found", r#"{"error":"Not found"}"#, 2);
        let http = reqwest::Client::new();
        let uri = format!("{base}/users/x/statuses/123");
        let id = resolve_status_id(&http, &base, "tok", &uri)
            .await
            .expect("same-host numeric fallback");
        assert_eq!(id, "123");
        let head = heads.lock().clone();
        assert!(head.contains("GET /api/v1/statuses/lookup?uri="), "lookup first: {head}");
        assert!(head.contains("GET /api/v2/search?q="), "search second: {head}");
        let _ = thread.join();
    }

    /// Production regression (404 on home-REST react, 2026-11): the lookup
    /// endpoint is gone on Mastodon 4.x — every lookup 404s — and the URI's
    /// numeric tail is the REMOTE server's id, never the home-local one
    /// (proven live: tail 117616332269977505 vs home-local 117616332435798026).
    /// Resolution must fall to `GET /api/v2/search?q=…&resolve=true` and return
    /// the search hit's home-local id — NOT the tail.
    #[tokio::test]
    async fn resolve_uses_v2_search_when_lookup_is_gone() {
        let (base, heads, thread) = stub_http_seq(vec![
            ("HTTP/1.1 404 Not Found", r#"{"error":"Not found"}"#),
            (
                "HTTP/1.1 200 OK",
                r#"{"statuses":[{"id":"117616332435798026","uri":"https://piaille.fr/users/CatEveryHour/statuses/117616332269977505"}]}"#,
            ),
        ]);
        let http = reqwest::Client::new();
        let uri = "https://piaille.fr/users/CatEveryHour/statuses/117616332269977505";
        let id = resolve_status_id(&http, &base, "tok", uri)
            .await
            .expect("search resolves when lookup is gone");
        assert_eq!(id, "117616332435798026", "the home-LOCAL search id");
        assert_ne!(id, "117616332269977505", "never the URI's remote numeric tail");
        let head = heads.lock().clone();
        assert!(head.contains("GET /api/v2/search?q="), "search second: {head}");
        assert!(head.contains("&resolve=true"), "search must resolve: {head}");
        assert!(
            head.contains("q=https%3A%2F%2Fpiaille.fr%2Fusers%2FCatEveryHour%2Fstatuses%2F117616332269977505"),
            "the URI must be urlencoded in q: {head}"
        );
        let _ = thread.join();
    }

    /// CR-1 (sprint review 2026-11-14): a search hit whose `uri` is not the
    /// requested URI must NEVER be used — the old `statuses[0]` fallback could
    /// favourite an unrelated home status. No exact match is NotFound, and a
    /// foreign-host URI is never guessed.
    #[tokio::test]
    async fn search_rejects_loose_hits_and_never_guesses() {
        let (base, heads, thread) = stub_http_seq(vec![
            ("HTTP/1.1 404 Not Found", r#"{"error":"Not found"}"#),
            (
                "HTTP/1.1 200 OK",
                r#"{"statuses":[{"id":"999","uri":"https://piaille.fr/users/SomeoneElse/statuses/555"}]}"#,
            ),
        ]);
        let http = reqwest::Client::new();
        let uri = "https://piaille.fr/users/CatEveryHour/statuses/117616332269977505";
        let err = resolve_status_id(&http, &base, "tok", uri)
            .await
            .expect_err("a loose search hit must never resolve");
        assert!(matches!(err, MastodonApiError::NotFound(_)), "got {err:?}");
        let head = heads.lock().clone();
        assert!(head.contains("GET /api/v2/search?q="), "search ran: {head}");
        let _ = thread.join();
    }

    /// design.md Risks: a 200 search with no match is `NotFound` — the remote
    /// numeric tail is NEVER guessed, even when both endpoints come up empty.
    #[tokio::test]
    async fn search_with_no_matches_is_not_found_and_never_guesses() {
        let (base, heads, thread) = stub_http_seq(vec![
            ("HTTP/1.1 404 Not Found", r#"{"error":"Not found"}"#),
            ("HTTP/1.1 200 OK", r#"{"accounts":[],"statuses":[],"hashtags":[]}"#),
        ]);
        let http = reqwest::Client::new();
        let err = resolve_status_id(&http, &base, "tok", "https://piaille.fr/users/x/statuses/123")
            .await
            .expect_err("an unmatched foreign URI must not resolve");
        assert!(matches!(err, MastodonApiError::NotFound(_)), "got {err:?}");
        let head = heads.lock().clone();
        assert!(head.contains("GET /api/v2/search?q="), "search ran: {head}");
        let _ = thread.join();
    }

    /// design.md Risks: a search-time 401 is a dead grant — demand re-link,
    /// never fall through to the numeric path (a stale id must not mask the
    /// consent problem).
    #[tokio::test]
    async fn search_401_demands_relink() {
        let (base, heads, thread) = stub_http_seq(vec![
            ("HTTP/1.1 404 Not Found", r#"{"error":"Not found"}"#),
            ("HTTP/1.1 401 Unauthorized", r#"{"error":"The access token is invalid"}"#),
        ]);
        let http = reqwest::Client::new();
        // Same-host URI with a numeric tail: the fallback WOULD apply — it
        // must not, the consent error is terminal.
        let uri = format!("{base}/users/x/statuses/123");
        let err = resolve_status_id(&http, &base, "tok", &uri)
            .await
            .expect_err("401 must demand re-link");
        assert!(matches!(err, MastodonApiError::ReauthRequired(_)), "got {err:?}");
        let head = heads.lock().clone();
        assert!(head.contains("GET /api/v2/search?q="), "the search ran before failing: {head}");
        let _ = thread.join();
    }

    /// Task 2.2: numeric-id parsing from canonical URI forms.
    #[test]
    fn status_id_from_uri_extracts_numeric_tail() {
        assert_eq!(
            status_id_from_uri("https://remote.test/users/x/statuses/123/"),
            Some("123".to_string())
        );
        assert_eq!(status_id_from_uri("https://remote.test/@x/9"), Some("9".to_string()));
        // Non-numeric tails are never guessed.
        assert_eq!(status_id_from_uri("https://remote.test/users/x/statuses/abc"), None);
        assert_eq!(status_id_from_uri("https://remote.test/@x"), None);
    }

    /// design.md Risks: a revoked token demands a visible re-link, never a
    /// silent fallback to another identity.
    #[tokio::test]
    async fn revoked_token_demands_relink() {
        let (base, _heads, thread) = stub_http(
            "HTTP/1.1 401 Unauthorized",
            r#"{"error":"The access token is invalid"}"#,
            1,
        );
        let http = reqwest::Client::new();
        let err = favourite_status(&http, &base, "revoked", "1")
            .await
            .expect_err("401 must demand re-link");
        assert!(matches!(err, MastodonApiError::ReauthRequired(_)), "got {err:?}");
        let _ = thread.join();
    }

    /// Task 1.2: the favourites-write scope gate passes write grants and
    /// demands a visible re-link for legacy read-only ones.
    #[test]
    fn favourites_scope_gate_passes_write_grants_and_rejects_read_only() {
        assert!(ensure_favourites_scope(Some("read write:favourites")).is_ok());
        assert!(ensure_favourites_scope(Some("read write")).is_ok());
        for scope in [Some("read"), None] {
            let err = ensure_favourites_scope(scope).expect_err("read-only grant must demand re-link");
            assert!(matches!(err, MastodonApiError::ReauthRequired(_)), "got {err:?}");
        }
    }

    /// Task 3.3/3.2: the follows-write scope gate passes follow-capable
    /// grants and demands the instruction-focused re-link otherwise.
    #[test]
    fn follows_scope_gate_passes_follow_grants_and_rejects_weaker_ones() {
        assert!(ensure_follows_scope(Some("read write:follows")).is_ok());
        assert!(ensure_follows_scope(Some("read write")).is_ok());
        for scope in [Some("read"), Some("read write:favourites"), None] {
            let err = ensure_follows_scope(scope)
                .expect_err("grant without follow write must demand re-link");
            assert!(matches!(err, MastodonApiError::ReauthRequired(_)), "got {err:?}");
            assert!(err.to_string().contains("re-link"), "the error names the fix");
        }
    }

    #[test]
    fn scopes_follows_write_covers_granular_and_comma_forms() {
        assert!(scopes_follows_write("read,write:follows"));
        assert!(scopes_follows_write("read write"));
        assert!(!scopes_follows_write("read write:favourites"));
        assert!(!scopes_follows_write("read"));
    }

    /// Task 3.2: the follow adapter posts to the home instance and reads the
    /// relationship back.
    #[tokio::test]
    async fn follow_account_posts_the_follow_to_the_home_instance() {
        let (base, heads, thread) = stub_http(
            "HTTP/1.1 200 OK",
            r#"{"id":"7","following":true,"requested":false}"#,
            1,
        );
        let http = reqwest::Client::new();
        let rel = home_follow_account(&http, &base, "tok", "7").await.expect("follow");
        assert_eq!(rel["following"], true);
        assert!(heads.lock().contains("POST /api/v1/accounts/7/follow"), "{heads:?}");
        let _ = thread.join();
    }

    #[tokio::test]
    async fn unfollow_account_posts_the_unfollow_to_the_home_instance() {
        let (base, heads, thread) = stub_http(
            "HTTP/1.1 200 OK",
            r#"{"id":"7","following":false}"#,
            1,
        );
        let http = reqwest::Client::new();
        let rel = home_unfollow_account(&http, &base, "tok", "7").await.expect("unfollow");
        assert_eq!(rel["following"], false);
        assert!(heads.lock().contains("POST /api/v1/accounts/7/unfollow"), "{heads:?}");
        let _ = thread.join();
    }

    /// A revoked/under-scoped token on the write itself is the visible
    /// re-link error, never a silent fallback.
    #[tokio::test]
    async fn follow_with_rejected_token_demands_relink() {
        let (base, _heads, thread) = stub_http(
            "HTTP/1.1 403 Forbidden",
            r#"{"error":"The access token is not authorized for this scope"}"#,
            1,
        );
        let http = reqwest::Client::new();
        let err = home_follow_account(&http, &base, "stale", "7").await.expect_err("403");
        assert!(matches!(err, MastodonApiError::ReauthRequired(_)), "got {err:?}");
        let _ = thread.join();
    }

    /// Task 3.2: account resolution — the direct lookup wins; an instance
    /// without the endpoint falls back to resolved search.
    #[tokio::test]
    async fn lookup_account_id_uses_lookup_then_resolved_search_fallback() {
        // Direct lookup path.
        let (base, heads, thread) = stub_http("HTTP/1.1 200 OK", r#"{"id":"42","username":"a"}"#, 1);
        let http = reqwest::Client::new();
        let id = lookup_account_id(&http, &base, "tok", "@a@remote.example")
            .await
            .expect("lookup");
        assert_eq!(id, "42");
        assert!(heads.lock().contains("GET /api/v1/accounts/lookup?acct=a%40remote.example"), "{heads:?}");
        let _ = thread.join();

        // Fallback path: lookup 404s, resolved search answers.
        let (base, heads, thread) = stub_http_seq(vec![
            ("HTTP/1.1 404 Not Found", r#"{"error":"Not found"}"#),
            ("HTTP/1.1 200 OK", r#"{"accounts":[{"id":"99","username":"a"}]}"#),
        ]);
        let id = lookup_account_id(&http, &base, "tok", "a@remote.example")
            .await
            .expect("search fallback");
        assert_eq!(id, "99");
        let head = heads.lock().clone();
        assert!(head.contains("GET /api/v1/accounts/lookup?acct="), "lookup first: {head}");
        assert!(head.contains("GET /api/v2/search?q=acct%3A"), "then search: {head}");
        let _ = thread.join();
    }

    #[test]
    fn scopes_favourites_write_covers_granular_and_comma_forms() {
        assert!(scopes_favourites_write("read,write:favourites"));
        assert!(scopes_favourites_write("read write:statuses"));
        assert!(scopes_favourites_write("read write:favourites"));
        assert!(scopes_favourites_write("read write"));
        assert!(!scopes_favourites_write("read"));
    }

    /// Task 2.1: expired-token refresh rotates via the home instance's token
    /// endpoint and persists the new tokens; a link without a refresh token
    /// gets the visible re-link error.
    #[tokio::test]
    async fn refresh_mastodon_token_rotates_and_persists() {
        let Some(pool) = connect_pool().await else { return };
        let suffix = unique();
        let user = UserRow::create_active(&pool, &format!("mstoken_{suffix}"), "", "x", "x")
            .await
            .expect("create user");
        let (base, heads, thread) = stub_http(
            "HTTP/1.1 200 OK",
            r#"{"access_token":"newtok","refresh_token":"rt2","token_type":"Bearer","scope":"read write:favourites","expires_in":3600}"#,
            1,
        );
        let metadata = json!({"instance": base, "client_id": "c", "client_secret": "s"});
        let expired = chrono::Utc::now() - chrono::Duration::seconds(3600);
        let acct = LinkedAccountRow::create(
            &pool,
            user.id,
            "mastodon",
            &format!("alice_{suffix}@home.test"),
            "alice",
            "oldtok",
            Some("rt1"),
            Some(expired),
            None,
            Some("read write:favourites"),
            &metadata,
        )
        .await
        .expect("link Mastodon account");

        let http = reqwest::Client::new();
        let new_token = refresh_mastodon_token(&pool, &http, &acct).await.expect("refresh");
        assert_eq!(new_token, "newtok");
        let head = heads.lock().clone();
        assert!(head.contains("POST /oauth/token"), "head: {head}");
        let updated = LinkedAccountRow::find_by_id(&pool, acct.id)
            .await
            .expect("re-read account")
            .expect("row");
        assert_eq!(updated.access_token, "newtok");
        assert_eq!(updated.refresh_token.as_deref(), Some("rt2"));
        // SEC-1 regression: the rotation must re-SEAL the new tokens at rest —
        // the raw columns carry `enc:v1:` ciphertext, never plaintext (a
        // plaintext rotation write would undo the at-rest encryption).
        let (raw_access, raw_refresh): (String, String) = sqlx::query_as(
            "SELECT access_token, refresh_token FROM linked_accounts WHERE id = $1",
        )
        .bind(acct.id)
        .fetch_one(&pool)
        .await
        .expect("raw token columns");
        assert!(
            raw_access.starts_with("enc:v1:"),
            "rotated access_token must be sealed at rest: {raw_access}"
        );
        assert!(
            raw_refresh.starts_with("enc:v1:"),
            "rotated refresh_token must be sealed at rest: {raw_refresh}"
        );
        assert_ne!(raw_access, "newtok", "the raw column must never hold the plaintext");
        assert!(
            updated.token_expires_at.expect("expiry persisted") > chrono::Utc::now(),
            "expires_in must roll the expiry forward"
        );

        // Same account with the refresh token gone and the token expired →
        // visible re-link error.
        sqlx::query(
            "UPDATE linked_accounts SET refresh_token = NULL, token_expires_at = $1 WHERE id = $2",
        )
        .bind(expired)
        .bind(acct.id)
        .execute(&pool)
        .await
        .expect("drop refresh token");
        let stale = LinkedAccountRow::find_by_id(&pool, acct.id)
            .await
            .expect("re-read account")
            .expect("row");
        let err = refresh_mastodon_token(&pool, &http, &stale)
            .await
            .expect_err("missing refresh token must demand re-link");
        assert!(matches!(err, MastodonApiError::ReauthRequired(_)), "got {err:?}");

        // User delete cascades the linked account.
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await;
        let _ = thread.join();
    }

    #[test]
    fn next_page_url_takes_only_the_next_relation() {
        assert_eq!(
            next_page_url(Some(
                r#"<https://m.example/api/v1/accounts/1/following?max_id=40>; rel="next", <https://m.example/api/v1/accounts/1/following?min_id=81>; rel="prev""#
            )),
            Some("https://m.example/api/v1/accounts/1/following?max_id=40".to_string())
        );
        // A header without a next relation never yields a page.
        assert_eq!(
            next_page_url(Some(r#"<https://m.example/x?min_id=1>; rel="prev""#)),
            None
        );
        assert_eq!(next_page_url(Some("garbage")), None);
        assert_eq!(next_page_url(None), None);
    }

    /// The follow-graph walk: resolve the account id first, then page through
    /// the `Link: rel="next"` header; an acct local to the instance (bare, no
    /// `@host`) is qualified with the instance host.
    #[tokio::test]
    async fn following_accounts_pages_and_qualifies_local_accts() {
        let (base, heads, thread) = stub_http_seq_hdrs(vec![
            (
                "HTTP/1.1 200 OK",
                vec![],
                r#"{"id":"42","acct":"lovingisliving","username":"lovingisliving"}"#,
            ),
            (
                "HTTP/1.1 200 OK",
                vec![("Link", r#"<__BASE__/api/v1/accounts/42/following?max_id=40>; rel="next""#)],
                r#"[{"acct":"eff@mastodon.social","url":"https://mastodon.social/@eff","display_name":"EFF"},
                    {"acct":"localuser","url":"https://indieweb.social/@localuser","display_name":""}]"#,
            ),
            (
                "HTTP/1.1 200 OK",
                vec![],
                r#"[{"acct":"Cory@mamot.fr","url":"https://mamot.fr/@pluralistic","display_name":"Cory"}]"#,
            ),
        ]);
        let http = reqwest::Client::new();
        let accounts = following_accounts(&http, &base, "tok")
            .await
            .expect("following walk");

        assert_eq!(accounts.len(), 3, "both pages collected: {accounts:?}");
        assert_eq!(accounts[0].acct, "eff@mastodon.social");
        assert_eq!(accounts[0].display_name, "EFF");
        assert_eq!(accounts[0].url.as_deref(), Some("https://mastodon.social/@eff"));
        // Bare acct → qualified with the instance; empty name → user part.
        assert_eq!(accounts[1].acct, format!("localuser@{}", base.to_lowercase()));
        assert_eq!(accounts[1].display_name, "localuser");
        // Handles are lowercased (the matcher compares case-insensitively).
        assert_eq!(accounts[2].acct, "cory@mamot.fr");

        let head = heads.lock().clone();
        assert!(
            head.contains("GET /api/v1/accounts/42/following?limit=80"),
            "walk must page the account's own following list: {head}"
        );
        // Headers are captured as sent (hyper lowercases them).
        assert!(
            head.to_lowercase().contains("authorization: bearer tok"),
            "the stored token authenticates the read: {head}"
        );
        assert!(head.contains("max_id=40"), "page 2 must follow rel=next: {head}");
        let _ = thread.join();
    }
}
