use crate::oauth;
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// Bluesky AT Protocol URLs
// ---------------------------------------------------------------------------
pub const BSKY_AUTH: &str = "https://bsky.social";
const BSKY_PUBLIC_API: &str = "https://api.bsky.app";
/// Chat RPCs are proxied through the user's PDS via the `atproto-proxy`
/// header — the OAuth token's audience is the PDS, not the chat entryway.
const BSKY_CHAT_PROXY: &str = "did:web:api.bsky.chat#bsky_chat";

/// Timed HTTP client for public API fetches. A dead remote (feed generator,
/// PDS) must fail fast instead of hanging the caller.
pub(crate) fn timed_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// Base scopes plus all RPC scopes needed for our XRPC calls.
/// The `#` in audience DIDs is URL-encoded as `%23`.
/// `rpc:` scopes grant access to AppView endpoints, `repo:` scopes grant PDS repo operations.
pub const BSKY_OAUTH_SCOPES: &str = concat!(
    "atproto transition:generic:basic",
    // AppView — actor
    " rpc:app.bsky.actor.getProfile?aud=did:web:api.bsky.app%23bsky_appview",
    " rpc:app.bsky.actor.getPreferences?aud=did:web:api.bsky.app%23bsky_appview",
    " rpc:app.bsky.actor.putPreferences?aud=did:web:api.bsky.app%23bsky_appview",
    " rpc:app.bsky.actor.getProfiles?aud=did:web:api.bsky.app%23bsky_appview",
    // AppView — graph
    " rpc:app.bsky.graph.getFollows?aud=did:web:api.bsky.app%23bsky_appview",
    " rpc:app.bsky.graph.getFollowers?aud=did:web:api.bsky.app%23bsky_appview",
    // AppView — feed
    " rpc:app.bsky.feed.getTimeline?aud=did:web:api.bsky.app%23bsky_appview",
    " rpc:app.bsky.feed.getAuthorFeed?aud=did:web:api.bsky.app%23bsky_appview",
    " rpc:app.bsky.feed.getFeed?aud=did:web:api.bsky.app%23bsky_appview",
    " rpc:app.bsky.feed.getFeedGenerator?aud=did:web:api.bsky.app%23bsky_appview",
    " rpc:app.bsky.feed.searchActors?aud=did:web:api.bsky.app%23bsky_appview",
    " rpc:app.bsky.feed.searchPosts?aud=did:web:api.bsky.app%23bsky_appview",
    " rpc:app.bsky.feed.post?aud=did:web:api.bsky.app%23bsky_appview",
    // AppView — repo
    " repo:app.bsky.feed.post",
    " repo:app.bsky.feed.like",
    " repo:app.bsky.feed.repost",
    " repo:app.bsky.graph.follow",
    " repo:app.bsky.feed.generator",
    // Bytescape's own collections — custom lexicons are never covered by
    // transition:generic:basic and need explicit repo grants, or PDS writes
    // fail with ScopeMissingError.
    " repo:com.bytescape.story",
    " repo:app.bsky.actor.status",
    " blob:*/*",
    // Identity
    " rpc:com.atproto.identity.resolveHandle?aud=did:web:api.bsky.app%23bsky_appview",
    // Chat
    " rpc:chat.bsky.convo.getConvoForMembers?aud=did:web:api.bsky.chat%23bsky_chat",
    " rpc:chat.bsky.convo.getConvo?aud=did:web:api.bsky.chat%23bsky_chat",
    " rpc:chat.bsky.convo.listConvos?aud=did:web:api.bsky.chat%23bsky_chat",
    " rpc:chat.bsky.convo.getMessages?aud=did:web:api.bsky.chat%23bsky_chat",
    " rpc:chat.bsky.convo.sendMessage?aud=did:web:api.bsky.chat%23bsky_chat",
    // NOTE: chat.bsky.convo.getLog is deliberately NOT requested — Bluesky's
    // PAR validates requested scopes against its cached copy of the client
    // metadata and rejects undeclared ones ("invalid_scope ... not declared in
    // the client metadata"), which breaks account linking entirely. Re-adding
    // it needs a cache-busting metadata strategy first.
    // PDS repo operations (needed for writing to the user's PDS)
    " repo:app.bsky.feed.post?action=create",
);

// ---------------------------------------------------------------------------
// Pending OAuth sessions (in-memory, keyed by state)
// ---------------------------------------------------------------------------
#[derive(Debug)]
pub struct PendingAuth {
    pub user_id: uuid::Uuid,
    pub code_verifier: String,
    pub dpop_private_key: Vec<u8>, // seed bytes
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub invite_code: String,
    /// True when this flow is a new-user signup (no authenticated account),
    /// false for a logged-in account re-linking Bluesky. Signup flows must
    /// never be resolved as "returning user" just because the same Bluesky
    /// DID exists in linked_accounts (e.g. left over from a deleted account).
    pub is_signup: bool,
}

static PENDING_AUTHS: LazyLock<Mutex<HashMap<String, PendingAuth>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register a pending OAuth session, return the state token.
pub fn register_pending(
    user_id: uuid::Uuid,
    invite_code: String,
    is_signup: bool,
) -> (String, SigningKey, String) {
    let state = uuid::Uuid::new_v4().to_string();
    let code_verifier = uuid::Uuid::new_v4().to_string().replace('-', "") + uuid::Uuid::new_v4().to_string().replace('-', "").as_str();
    let (sk, _) = oauth::generate_dpop_keypair();

    let pending = PendingAuth {
        user_id,
        code_verifier: code_verifier.clone(),
        dpop_private_key: sk.to_bytes().to_vec(),
        invite_code,
        created_at: chrono::Utc::now(),
        is_signup,
    };

    let mut store = PENDING_AUTHS.lock().expect("bluesky PENDING_AUTHS lock poisoned");
    // Clean expired entries (> 10 min)
    store.retain(|_, v| (chrono::Utc::now() - v.created_at).num_minutes() < 10);
    store.insert(state.clone(), pending);

    (state, sk, code_verifier)
}

/// Consume a pending auth by state.
pub fn consume_pending(state: &str) -> Option<PendingAuth> {
    let mut store = PENDING_AUTHS.lock().expect("bluesky PENDING_AUTHS lock poisoned");
    store.remove(state)
}

// ---------------------------------------------------------------------------
// Bluesky OAuth URL builder
// ---------------------------------------------------------------------------
pub fn authorize_url(
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
) -> String {
    format!(
        "{}/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}&code_challenge={}&code_challenge_method=S256&login_hint=",
        BSKY_AUTH,
        urlencoding(client_id),
        urlencoding(redirect_uri),
        urlencoding(BSKY_OAUTH_SCOPES),
        urlencoding(state),
        urlencoding(code_challenge),
    )
}

/// Send a Pushed Authorization Request (PAR) to Bluesky's OAuth server.
/// Bluesky requires PAR per the AT Protocol OAuth spec.
/// Returns the `request_uri` on success.
pub async fn send_par_request(
    http: &reqwest::Client,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
    dpop_sk: &ed25519_dalek::SigningKey,
) -> Result<String, String> {
    let par_url = format!("{}/oauth/par", BSKY_AUTH);

    let params = [
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("response_type", "code"),
        ("scope", BSKY_OAUTH_SCOPES),
        ("state", state),
        ("code_challenge", code_challenge),
        ("code_challenge_method", "S256"),
    ];

    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(params)
        .finish();

    let resp = crate::oauth::send_dpop_request(
        dpop_sk,
        "POST",
        &par_url,
        None,
        |dpop| {
            http.post(&par_url)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .header("DPoP", dpop)
                .body(body.clone())
        },
    )
    .await
    .map_err(|e| format!("PAR request failed: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("Read PAR response failed: {}", e))?;

    if !status.is_success() {
        return Err(format!("PAR request failed ({}): {}", status.as_u16(), text));
    }

    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("PAR response parse failed: {} — body: {}", e, text))?;

    json["request_uri"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("No request_uri in PAR response: {}", text))
}

/// Build the Bluesky OAuth authorize URL using a PAR-obtained request_uri.
pub fn authorize_url_par(
    client_id: &str,
    request_uri: &str,
) -> String {
    format!(
        "{}/oauth/authorize?client_id={}&request_uri={}",
        BSKY_AUTH,
        urlencoding(client_id),
        urlencoding(request_uri),
    )
}

fn urlencoding(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

// ---------------------------------------------------------------------------
// Token response from Bluesky OAuth
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueskyTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<i64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
    pub sub: Option<String>, // DID
}

// ---------------------------------------------------------------------------
// Resolve a DID to its DID document — shared helper for DID document fetching
// ---------------------------------------------------------------------------
async fn fetch_did_document(http: &reqwest::Client, did: &str) -> Result<Value, String> {
    let candidates: Vec<String> = if did.starts_with("did:plc:") {
        vec![format!("https://plc.directory/{}", did)]
    } else if let Some(host) = did.strip_prefix("did:web:") {
        let mut urls = vec![format!("https://{}/.well-known/did.json", host)];
        // Fallback (path form): Bytescape instances serve the same document at
        // the apex, /{host}/.well-known/did.json — covers subdomains without
        // their own TLS coverage.
        if let Some((_, apex)) = host.split_once('.') {
            if !apex.is_empty() {
                urls.push(format!("https://{}/{}/.well-known/did.json", apex, host));
            }
        }
        urls
    } else {
        return Err(format!("Unsupported DID method: {}", did));
    };

    let mut last_err = format!("DID resolve failed for {}", did);
    for url in candidates {
        let resp = match http.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                last_err = format!("DID resolve request failed: {}", e);
                continue;
            }
        };
        let status = resp.status();
        let text = match resp.text().await {
            Ok(t) => t,
            Err(e) => {
                last_err = format!("Read DID doc failed: {}", e);
                continue;
            }
        };
        if !status.is_success() {
            last_err = format!("DID resolve error ({}): {}", status.as_u16(), text);
            continue;
        }
        match serde_json::from_str(&text) {
            Ok(doc) => return Ok(doc),
            Err(e) => last_err = format!("Parse DID doc failed: {}", e),
        }
    }
    Err(last_err)
}

// ---------------------------------------------------------------------------
// Resolve a DID to its PDS endpoint (the #atproto_pds service)
// This is the correct URL for authenticated API calls — NOT bsky.social.
// ---------------------------------------------------------------------------
pub async fn resolve_pds_endpoint(http: &reqwest::Client, did: &str) -> Result<String, String> {
    let doc = fetch_did_document(http, did).await?;
    let services = doc["service"].as_array().ok_or("No service array in DID doc")?;
    for svc in services {
        let id = svc["id"].as_str().unwrap_or("");
        let ty = svc["type"].as_str().unwrap_or("");
        if id.ends_with("#atproto_pds") || ty == "AtprotoPersonalDataServer" {
            return svc["serviceEndpoint"]
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| "No serviceEndpoint for #atproto_pds".into());
        }
    }
    Err("No #atproto_pds service found in DID doc".into())
}

// ---------------------------------------------------------------------------
// Exchange authorization code for tokens (with DPoP)
// ---------------------------------------------------------------------------
pub async fn exchange_code(
    http: &reqwest::Client,
    token_url: &str,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
    client_id: &str,
    dpop_sk: &SigningKey,
) -> Result<BlueskyTokenResponse, String> {
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("code_verifier", code_verifier),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
    ];

    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(params)
        .finish();

    let resp = crate::oauth::send_dpop_request(
        dpop_sk,
        "POST",
        token_url,
        None,
        |dpop| {
            http.post(token_url)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .header("DPoP", dpop)
                .body(body.clone())
        },
    )
    .await
    .map_err(|e| format!("Token request failed: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("Read response failed: {}", e))?;

    if !status.is_success() {
        return Err(format!("Token exchange failed ({}): {}", status.as_u16(), text));
    }

    serde_json::from_str::<BlueskyTokenResponse>(&text)
        .map_err(|e| format!("Failed to parse token response: {} — body: {}", e, text))
}

// ---------------------------------------------------------------------------
// Refresh an access token
// ---------------------------------------------------------------------------
pub async fn refresh_token(
    http: &reqwest::Client,
    token_url: &str,
    refresh_token: &str,
    client_id: &str,
    dpop_sk: &SigningKey,
) -> Result<BlueskyTokenResponse, String> {
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
    ];

    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(params)
        .finish();

    let resp = crate::oauth::send_dpop_request(
        dpop_sk,
        "POST",
        token_url,
        None,
        |dpop| {
            http.post(token_url)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .header("DPoP", dpop)
                .body(body.clone())
        },
    )
    .await
    .map_err(|e| format!("Token refresh failed: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("Read response failed: {}", e))?;

    if !status.is_success() {
        return Err(format!("Token refresh failed ({}): {}", status.as_u16(), text));
    }

    serde_json::from_str::<BlueskyTokenResponse>(&text)
        .map_err(|e| format!("Failed to parse refresh response: {} — body: {}", e, text))
}

/// Serializes token refresh across all callers. Without this, concurrent
/// refreshes (feed seed + full phases) both consume the same refresh token;
/// Bluesky rotates on every use, so the losing save leaves a stored token the
/// server has already invalidated ("invalid_grant" until re-link).
static REFRESH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Refresh a Bluesky OAuth session. Updates the account tokens in the database.
/// Returns the new access token on success.
pub async fn refresh_session(
    http: &reqwest::Client,
    pool: &crate::db::DbPool,
    account: &crate::db::accounts::LinkedAccountRow,
) -> Result<String, String> {
    let _guard = REFRESH_LOCK.lock().await;

    // Re-read the row inside the lock — double-check: another caller may have
    // just rotated the tokens while we waited.
    let _lock: crate::db::accounts::LinkedAccountRow = sqlx::query_as(
        "SELECT * FROM linked_accounts WHERE id = $1"
    )
    .bind(account.id)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to read account row: {}", e))?;

    let expiring = _lock
        .token_expires_at
        .map(|t| t < chrono::Utc::now() + chrono::Duration::seconds(60))
        .unwrap_or(true);
    if !expiring {
        return Ok(_lock.access_token.clone());
    }

    let refresh = _lock
        .refresh_token
        .as_ref()
        .ok_or_else(|| "No refresh token available".to_string())?;

    let token_url = format!("{}/oauth/token", BSKY_AUTH);
    let client_id = _lock.metadata.get("client_id")
        .and_then(|v| v.as_str())
        .unwrap_or("https://bytescape.social/oauth/client-metadata.json");

    let dpop_sk = if let Some(hex_key) = &_lock.dpop_private_key {
        if hex_key.is_empty() {
            return Err("Empty DPoP key".to_string());
        }
        let seed = hex::decode(hex_key).map_err(|e| format!("Bad DPoP key: {}", e))?;
        let arr: [u8; 32] = seed
            .try_into()
            .map_err(|_| "Invalid DPoP key length".to_string())?;
        SigningKey::from_bytes(&arr)
    } else {
        return Err("No DPoP key for refresh".to_string());
    };

    let resp = refresh_token(http, &token_url, refresh, client_id, &dpop_sk).await?;

    let new_access = resp.access_token;
    let new_refresh = resp
        .refresh_token
        .unwrap_or_else(|| refresh.clone());
    // Persist the new expiry too — without it every call re-enters refresh
    // (each rotation is another invalid_grant risk window) and the
    // double-check above never short-circuits.
    let expires_in = resp.expires_in.unwrap_or(3600);
    let token_expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_in);

    // SEC-1: the rotation re-SEALS the new tokens before binding — only
    // `enc:v1:` ciphertext ever reaches the database (same contract as
    // `LinkedAccountRow::create`).
    let secret = crate::db::accounts::jwt_secret();
    sqlx::query(
        "UPDATE linked_accounts SET access_token = $1, refresh_token = $2, token_expires_at = $3, updated_at = NOW() WHERE id = $4",
    )
    .bind(crate::secret::seal(&new_access, &secret))
    .bind(crate::secret::seal(&new_refresh, &secret))
    .bind(token_expires_at)
    .bind(account.id)
    .execute(pool)
    .await
    .map_err(|e| format!("DB update failed: {}", e))?;

    Ok(new_access)
}

// ---------------------------------------------------------------------------
// Bluesky AT Protocol API client (authenticated with DPoP)
// ---------------------------------------------------------------------------
pub struct BlueskyApiClient {
    http: reqwest::Client,
    access_token: String,
    dpop_sk: Option<SigningKey>,
    pds_url: String,
    /// Cached DPoP nonce received from the PDS. Used in subsequent requests.
    dpop_nonce: Mutex<Option<String>>,
}

impl BlueskyApiClient {
    pub fn new(
        access_token: &str,
        dpop_seed: &[u8],
        pds_url: &str,
    ) -> Result<Self, String> {
        let dpop_sk = SigningKey::from_bytes(
            &dpop_seed.try_into().map_err(|_| "Invalid DPoP key seed (need 32 bytes)")?,
        );
        Ok(Self {
            http: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(5))
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            access_token: access_token.to_string(),
            dpop_sk: Some(dpop_sk),
            pds_url: pds_url.to_string(),
            dpop_nonce: Mutex::new(None),
        })
    }

    pub fn new_bearer(access_token: &str, pds_url: &str) -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(5))
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            access_token: access_token.to_string(),
            dpop_sk: None,
            pds_url: pds_url.to_string(),
            dpop_nonce: Mutex::new(None),
        }
    }

    /// Read cached DPoP nonce (if any).
    fn cached_dpop_nonce(&self) -> Option<String> {
        self.dpop_nonce.lock().ok().and_then(|n| n.clone())
    }

    /// Cache a DPoP nonce received from the server.
    fn set_cached_dpop_nonce(&self, nonce: String) {
        if let Ok(mut guard) = self.dpop_nonce.lock() {
            *guard = Some(nonce);
        }
    }

    /// Send an HTTP request with DPoP proof. On 400/401 `use_dpop_nonce`, retries once with
    /// the server-provided nonce and caches it for future calls.
    async fn send_dpop_request_with_retry<F>(
        &self,
        sk: &SigningKey,
        method: &str,
        url: &str,
        build_request: F,
    ) -> Result<reqwest::Response, String>
    where
        F: Fn(&str) -> reqwest::RequestBuilder,
    {
        let ath = oauth::dpop_ath(&self.access_token);
        let nonce = self.cached_dpop_nonce();
        let dpop = oauth::create_dpop_proof(sk, method, url, Some(&ath), nonce.as_deref())?;
        let resp = build_request(&dpop)
            .send()
            .await
            .map_err(|e| format!("DPoP request failed: {}", e))?;

        // Check for DPoP nonce challenge: 400 or 401 with "use_dpop_nonce" error
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::BAD_REQUEST {
            // Clone the nonce header value before consuming resp via .text()
            let server_nonce = resp
                .headers()
                .get("dpop-nonce")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            if let Some(server_nonce) = server_nonce {
                let body = resp.text().await.unwrap_or_default();
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                    if json["error"].as_str() == Some("use_dpop_nonce") {
                        // Cache the nonce and retry with it
                        self.set_cached_dpop_nonce(server_nonce.clone());
                        let dpop2 = oauth::create_dpop_proof(sk, method, url, Some(&ath), Some(&server_nonce))?;
                        return build_request(&dpop2)
                            .send()
                            .await
                            .map_err(|e| format!("DPoP request retry failed: {}", e));
                    }
                }
                // Not a nonce challenge — return the error with the body we already read
                return Err(format!("DPoP request failed ({}): {}", status, body));
            }
            // No nonce header — fall through to let caller handle the error
        }

        Ok(resp)
    }

    /// Make an authenticated AT Protocol XRPC GET call.
    pub(crate) async fn xrpc_call(&self,
        nsid: &str,
        params: Vec<(&str, &str)>,
    ) -> Result<Value, String> {
        let url = format!("{}/xrpc/{}", self.pds_url, nsid);
        let mut full_url = url.clone();
        if !params.is_empty() {
            let qs: Vec<String> = params.iter().map(|(k, v)| format!("{}={}", k, urlencoding(v))).collect();
            full_url = format!("{}?{}", url, qs.join("&"));
        }

        let resp = match &self.dpop_sk {
            Some(sk) => {
                let http = &self.http;
                let full = full_url.clone();
                let at = &self.access_token;
                self.send_dpop_request_with_retry(sk, "GET", &url, |dpop| {
                    http.get(&full)
                        .header("Authorization", format!("DPoP {}", at))
                        .header("DPoP", dpop)
                })
                .await
            }
            None => {
                self.http
                    .get(&full_url)
                    .header("Authorization", format!("Bearer {}", self.access_token))
                    .send()
                    .await
                    .map_err(|e| format!("XRPC call failed: {}", e))
            }
        }
        .map_err(|e| format!("XRPC call failed: {}", e))?;

        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("Read failed: {}", e))?;

        if !status.is_success() {
            return Err(format!("XRPC error ({}): {}", status.as_u16(), text));
        }

        serde_json::from_str(&text).map_err(|e| format!("JSON parse failed: {} — body: {}", e, text))
    }

    /// Replace the account's private preferences (app.bsky.actor.putPreferences).
    /// Read-modify-write is the caller's job — this sends the full array.
    pub(crate) async fn put_preferences(
        &self,
        preferences: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.xrpc_post(
            "app.bsky.actor.putPreferences",
            &serde_json::json!({ "preferences": preferences }),
        )
        .await
    }

    /// Make an authenticated AT Protocol XRPC POST call.
    pub(crate) async fn xrpc_post(&self, nsid: &str, body: &Value) -> Result<Value, String> {
        let url = format!("{}/xrpc/{}", self.pds_url, nsid);

        let resp = match &self.dpop_sk {
            Some(sk) => {
                let http = &self.http;
                let u = url.clone();
                let at = &self.access_token;
                let b = body.clone();
                self.send_dpop_request_with_retry(sk, "POST", &u, |dpop| {
                    http.post(&u)
                        .header("Authorization", format!("DPoP {}", at))
                        .header("DPoP", dpop)
                        .header("Content-Type", "application/json")
                        .json(&b)
                })
                .await
            }
            None => {
                self.http
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", self.access_token))
                    .header("Content-Type", "application/json")
                    .json(body)
                    .send()
                    .await
                    .map_err(|e| format!("XRPC POST failed: {}", e))
            }
        }
        .map_err(|e| format!("XRPC POST failed: {}", e))?;

        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("Read failed: {}", e))?;

        if !status.is_success() {
            return Err(format!("XRPC POST error ({}): {}", status.as_u16(), text));
        }

        serde_json::from_str(&text).map_err(|e| format!("JSON parse failed: {} — body: {}", e, text))
    }

    /// Make an authenticated XRPC GET call *to the chat proxy*.
    async fn xrpc_call_chat(
        &self,
        nsid: &str,
        params: Vec<(&str, &str)>,
    ) -> Result<Value, String> {
        // Chat RPCs route through the user's PDS with the atproto-proxy
        // header: the OAuth token's audience is the PDS, so direct calls to
        // the chat entryway are rejected ("unknown auth scheme"/"Bad token
        // scope"). The PDS validates the DPoP-bound token and proxies to the
        // account's chat delegator.
        let url = format!("{}/xrpc/{}", self.pds_url, nsid);
        let mut full_url = url.clone();
        if !params.is_empty() {
            let qs: Vec<String> = params.iter().map(|(k, v)| format!("{}={}", k, urlencoding(v))).collect();
            full_url = format!("{}?{}", url, qs.join("&"));
        }

        let resp = match &self.dpop_sk {
            Some(sk) => {
                let http = &self.http;
                let full = full_url.clone();
                let at = &self.access_token;
                self.send_dpop_request_with_retry(sk, "GET", &url, |dpop| {
                    http.get(&full)
                        .header("Authorization", format!("DPoP {}", at))
                        .header("DPoP", dpop)
                        .header("atproto-proxy", BSKY_CHAT_PROXY)
                })
                .await
            }
            None => {
                self.http
                    .get(&full_url)
                    .header("Authorization", format!("Bearer {}", self.access_token))
                    .header("atproto-proxy", BSKY_CHAT_PROXY)
                    .send()
                    .await
                    .map_err(|e| format!("XRPC chat call failed: {}", e))
            }
        }
        .map_err(|e| format!("XRPC chat call failed: {}", e))?;

        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("Read failed: {}", e))?;

        if !status.is_success() {
            return Err(format!("XRPC chat error ({}): {}", status.as_u16(), text));
        }

        serde_json::from_str(&text).map_err(|e| format!("JSON parse failed: {} — body: {}", e, text))
    }

    /// Make an authenticated XRPC POST call to chat, proxied via the PDS.
    async fn xrpc_post_chat(&self, nsid: &str, body: &Value) -> Result<Value, String> {
        let url = format!("{}/xrpc/{}", self.pds_url, nsid);

        let resp = match &self.dpop_sk {
            Some(sk) => {
                let http = &self.http;
                let u = url.clone();
                let at = &self.access_token;
                let b = body.clone();
                self.send_dpop_request_with_retry(sk, "POST", &u, |dpop| {
                    http.post(&u)
                        .header("Authorization", format!("DPoP {}", at))
                        .header("DPoP", dpop)
                        .header("atproto-proxy", BSKY_CHAT_PROXY)
                        .header("Content-Type", "application/json")
                        .json(&b)
                })
                .await
            }
            None => {
                self.http
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", self.access_token))
                    .header("atproto-proxy", BSKY_CHAT_PROXY)
                    .header("Content-Type", "application/json")
                    .json(body)
                    .send()
                    .await
                    .map_err(|e| format!("XRPC chat POST failed: {}", e))
            }
        }
        .map_err(|e| format!("XRPC chat POST failed: {}", e))?;

        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("Read failed: {}", e))?;

        if !status.is_success() {
            return Err(format!("XRPC chat POST error ({}): {}", status.as_u16(), text));
        }

        serde_json::from_str(&text).map_err(|e| format!("JSON parse failed: {} — body: {}", e, text))
    }

    /// Fetch the home timeline.
    pub async fn get_timeline(&self, limit: u32, cursor: Option<&str>) -> Result<Value, String> {
        let limit_str = limit.to_string();
        let mut params: Vec<(&str, &str)> = vec![("limit", &limit_str)];
        if let Some(c) = cursor {
            params.push(("cursor", c));
        }
        self.xrpc_call("app.bsky.feed.getTimeline", params).await
    }

    /// Search posts by query.
    pub async fn search_posts(&self, query: &str, limit: u32) -> Result<Value, String> {
        self.xrpc_call(
            "app.bsky.feed.searchPosts",
            vec![("q", query), ("limit", &limit.to_string())],
        )
        .await
    }

    /// Get a custom feed by feed URI.
    pub async fn get_feed(&self, feed_uri: &str, limit: u32) -> Result<Value, String> {
        self.xrpc_call(
            "app.bsky.feed.getFeed",
            vec![("feed", feed_uri), ("limit", &limit.to_string())],
        )
        .await
    }

    /// Resolve a feed generator's display name via the AppView. Returns None
    /// when the feed can't be resolved (unknown/deleted). Used to label
    /// imported saved feeds with their real name instead of the raw rkey.
    pub async fn feed_display_name(&self, feed_uri: &str) -> Option<String> {
        let res = self
            .xrpc_call("app.bsky.feed.getFeedGenerator", vec![("feed", feed_uri)])
            .await
            .ok()?;
        res["view"]["displayName"]
            .as_str()
            .map(|s| s.to_string())
            .filter(|s| !s.trim().is_empty())
    }

    /// Get an author's posts.
    pub async fn get_author_feed(&self, actor: &str, limit: u32) -> Result<Value, String> {
        self.xrpc_call(
            "app.bsky.feed.getAuthorFeed",
            vec![("actor", actor), ("limit", &limit.to_string())],
        )
        .await
    }

    /// Get user preferences (includes saved feeds, muted words, etc).
    pub async fn get_preferences(&self) -> Result<Value, String> {
        self.xrpc_call("app.bsky.actor.getPreferences", vec![]).await
    }

    /// List Bluesky conversations (DMs).
    pub async fn list_convos(&self, limit: u32, cursor: Option<&str>) -> Result<Value, String> {
        let limit_str = limit.to_string();
        let mut params: Vec<(&str, &str)> = vec![("limit", &limit_str)];
        if let Some(c) = cursor {
            params.push(("cursor", c));
        }
        self.xrpc_call_chat("chat.bsky.convo.listConvos", params).await
    }

    /// Get messages for a conversation.
    pub async fn get_messages(&self, convo_id: &str, limit: u32, cursor: Option<&str>) -> Result<Value, String> {
        let limit_str = limit.to_string();
        let mut params: Vec<(&str, &str)> = vec![("convoId", convo_id), ("limit", &limit_str)];
        if let Some(c) = cursor {
            params.push(("cursor", c));
        }
        self.xrpc_call_chat("chat.bsky.convo.getMessages", params).await
    }

    /// Send a message to a conversation.
    pub async fn send_message(&self, convo_id: &str, text: &str) -> Result<Value, String> {
        let body = serde_json::json!({
            "convoId": convo_id,
            "message": {
                "text": text
            }
        });
        self.xrpc_post_chat("chat.bsky.convo.sendMessage", &body).await
    }

    /// Get the chat log (recent events/messages).
    pub async fn get_log(&self, cursor: Option<&str>) -> Result<Value, String> {
        let mut params: Vec<(&str, &str)> = vec![];
        if let Some(c) = cursor {
            params.push(("cursor", c));
        }
        self.xrpc_call_chat("chat.bsky.convo.getLog", params).await
    }

    /// Check if a feed item is a reply post.
    pub fn is_reply_item(item: &serde_json::Value) -> bool {
        item.pointer("/post/record/reply").is_some()
    }

    /// Extract the reposter handle from a feed item's reason field.
    /// Returns `None` if the item is not a repost.
    pub fn repost_handle(item: &serde_json::Value) -> Option<String> {
        item.get("reason")
            .and_then(|r| r.get("$type").and_then(|t| t.as_str()))
            .filter(|t| *t == "app.bsky.feed.defs#reasonRepost")
            .and_then(|_| item.pointer("/reason/by/handle"))
            .and_then(|h| h.as_str())
            .map(|h| format!("@{}", h))
    }

    /// Extract the quoted post URI from a post's embed (app.bsky.embed.record).
    pub fn quoted_post_uri(post: &serde_json::Value) -> Option<String> {
        post.get("embed")
            .and_then(|e| {
                let t = e.get("$type").and_then(|v| v.as_str())?;
                if t == "app.bsky.embed.record" || t == "app.bsky.embed.recordWithMedia" {
                    e.pointer("/record/uri").and_then(|v| v.as_str()).map(|s| s.to_string())
                } else {
                    None
                }
            })
    }

    /// Resolve a handle to a DID.
    pub async fn resolve_handle(&self, handle: &str) -> Result<String, String> {
        let params = vec![("handle", handle)];
        let res = self.xrpc_call("com.atproto.identity.resolveHandle", params).await?;
        res["did"].as_str().map(|s| s.to_string()).ok_or_else(|| "No did in resolveHandle response".into())
    }

    /// Resolve a DID to a handle using getProfile.
    pub async fn resolve_handle_from_did(&self, did: &str) -> Result<String, String> {
        let params = vec![("actor", did)];
        let res = self.xrpc_call("app.bsky.actor.getProfile", params).await?;
        res["handle"].as_str().map(|s| s.to_string()).ok_or_else(|| "No handle in getProfile response".into())
    }

    /// Fetch follows using the authenticated (DPoP-bound) client.
    /// Unlike the standalone `get_follows`, this uses `xrpc_call` which adds
    /// the correct DPoP headers when `dpop_sk` is set.
    async fn get_follows_auth(&self, actor: &str) -> Result<Vec<(String, String, String)>, String> {
        let mut all: Vec<(String, String, String)> = Vec::new();
        let mut cursor: Option<String> = None;
        let mut page = 0u32;
        loop {
            page += 1;
            let limit_str = "100".to_string();
            let mut params: Vec<(String, String)> = vec![
                ("actor".to_string(), actor.to_string()),
                ("limit".to_string(), limit_str),
            ];
            if let Some(ref c) = cursor {
                params.push(("cursor".to_string(), c.clone()));
            }
            let params_ref: Vec<(&str, &str)> = params.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            let res = self.xrpc_call("app.bsky.graph.getFollows", params_ref).await?;

            let page_count = res["follows"].as_array().map_or(0, |a| a.len());
            eprintln!("[get_follows_auth] page {} returned {} follows", page, page_count);

            if let Some(arr) = res["follows"].as_array() {
                for f in arr {
                    let follow_did = f["did"].as_str().unwrap_or("").to_string();
                    let handle = f["handle"].as_str().unwrap_or("").to_string();
                    let display = f["displayName"].as_str().unwrap_or("").to_string();
                    all.push((follow_did, handle, display));
                }
            }
            cursor = res["cursor"].as_str().map(|s| s.to_string());
            eprintln!("[get_follows_auth] page {} cursor: {:?}", page, cursor);
            if cursor.is_none() { break; }
        }
        eprintln!("[get_follows_auth] FINAL: {} total follows across {} pages", all.len(), page);
        Ok(all)
    }

    /// Import follows from Bluesky — stores them in the follows table.
    /// Uses DPoP-aware authenticated API calls.
    pub async fn import_follows(&self, db: &sqlx::PgPool, user_id: uuid::Uuid, my_did: &str) -> Result<u32, String> {
        let follows = self.get_follows_auth(my_did).await?;
        eprintln!("[import_follows] fetched {} follows for DID {}", follows.len(), my_did);
        let mut count = 0u32;
        for (_followed_did, handle, display_name) in follows {
            let actor_url = format!("https://bsky.app/profile/{}", handle);
            let label = if display_name.is_empty() { &handle } else { &display_name };
            match sqlx::query("INSERT INTO follows (follower_id, followed_actor, follow_type, status, label) VALUES ($1, $2, 'bluesky_import', 'accepted', $3) ON CONFLICT (follower_id, followed_actor) DO UPDATE SET follow_type = 'bluesky_import', label = EXCLUDED.label")
                .bind(user_id)
                .bind(&actor_url)
                .bind(label)
                .execute(db)
                .await
            {
                Ok(_) => { count += 1; }
                Err(e) => {
                    eprintln!("[import_follows] DB insert failed for {}: {}", actor_url, e);
                    return Err(format!("Follow insert failed for {}: {}", actor_url, e));
                }
            }
        }
        eprintln!("[import_follows] DONE: {} follows imported for user {}", count, user_id);
        Ok(count)
    }

    /// Fetch the authenticated user's profile via DPoP-aware XRPC call.
    /// Unlike the standalone `get_profile_full` (Bearer-only, fails for OAuth
    /// DPoP-bound tokens with InvalidToken), this sends a proper DPoP proof.
    pub async fn get_profile_dpop(&self, actor: &str) -> Result<BlueskyProfile, String> {
        let params = vec![("actor", actor)];
        let res = self.xrpc_call("app.bsky.actor.getProfile", params).await?;
        Ok(BlueskyProfile {
            display_name: res["displayName"].as_str().unwrap_or("").to_string(),
            description: res["description"].as_str().unwrap_or("").to_string(),
            avatar_url: res["avatar"].as_str().map(|s| s.to_string()),
            banner_url: res["banner"].as_str().map(|s| s.to_string()),
            followers_count: res["followersCount"].as_u64().unwrap_or(0) as u32,
            follows_count: res["followsCount"].as_u64().unwrap_or(0) as u32,
            posts_count: res["postsCount"].as_u64().unwrap_or(0) as u32,
        })
    }

    /// Get or create a 1-on-1 conversation by member DIDs.
    pub async fn get_convo_for_members(&self, members: &[&str]) -> Result<Value, String> {
        let params: Vec<(&str, &str)> = members.iter().map(|m| ("members", *m)).collect();
        self.xrpc_call_chat("chat.bsky.convo.getConvoForMembers", params).await
    }

    /// Upload a blob to Bluesky PDS with DPoP auth. Returns the blob reference.
    pub async fn upload_blob(&self, data: Vec<u8>, mime_type: &str) -> Result<serde_json::Value, String> {
        let url = format!("{}/xrpc/com.atproto.repo.uploadBlob", self.pds_url);
        // Bytes so retry attempts share one buffer instead of copying a
        // potentially huge video per attempt.
        let body: bytes::Bytes = data.into();

        let resp = match &self.dpop_sk {
            Some(sk) => {
                let http = &self.http;
                let u = url.clone();
                let at = &self.access_token;
                let data_clone = body.clone();
                self.send_dpop_request_with_retry(sk, "POST", &u, |dpop| {
                    http.post(&u)
                        .timeout(std::time::Duration::from_secs(300))
                        .header("Authorization", format!("DPoP {}", at))
                        .header("DPoP", dpop)
                        .header("Content-Type", mime_type)
                        .body(data_clone.clone())
                }).await
            }
            None => {
                self.http
                    .post(&url)
                    .timeout(std::time::Duration::from_secs(300))
                    .header("Authorization", format!("Bearer {}", self.access_token))
                    .header("Content-Type", mime_type)
                    .body(body)
                    .send()
                    .await
                    .map_err(|e| format!("Blob upload failed: {}", e))
            }
        }.map_err(|e| format!("Blob upload failed: {}", e))?;

        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("Read failed: {}", e))?;
        if !status.is_success() {
            return Err(format!("Blob upload failed ({}): {}", status.as_u16(), &text[..text.len().min(200)]));
        }
        serde_json::from_str(&text).map_err(|e| format!("JSON parse failed: {} — body: {}", e, text))
    }

    /// Build an image embed for Bluesky from media URLs using DPoP auth.
    /// Reads from disk / downloads, compresses, uploads blobs.
    async fn build_image_embed_dpop(&self, media_urls: &[String]) -> Result<serde_json::Value, String> {
        let mut images = Vec::new();
        let upload_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

        for url in media_urls {
            let (bytes, mime) = if url.starts_with("/uploads/") {
                let rel_path = url.trim_start_matches("/uploads/");
                let file_path = upload_root.join("uploads").join(rel_path);
                tracing::info!("[bluesky] Reading media from disk: {}", file_path.display());
                let file_bytes = std::fs::read(&file_path).map_err(|e| {
                    let msg = format!("Failed to read {}: {}", file_path.display(), e);
                    tracing::error!("[bluesky] {}", msg);
                    msg
                })?;
                let mime = if url.ends_with(".png") { "image/png" }
                    else if url.ends_with(".gif") { "image/gif" }
                    else if url.ends_with(".webp") { "image/webp" }
                    else { "image/jpeg" };
                (file_bytes, mime.to_string())
            } else if url.starts_with("http://") || url.starts_with("https://") {
                let resp = self.http.get(url).send().await
                    .map_err(|e| {
                        let msg = format!("Failed to download media {}: {}", url, e);
                        tracing::error!("[bluesky] {}", msg);
                        msg
                    })?;
                let mime = resp.headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("image/jpeg")
                    .to_string();
                let bytes = resp.bytes().await
                    .map_err(|e| {
                        let msg = format!("Failed to read media bytes from {}: {}", url, e);
                        tracing::error!("[bluesky] {}", msg);
                        msg
                    })?;
                (bytes.to_vec(), mime)
            } else {
                let msg = format!("Unsupported media URL format: {}", url);
                tracing::error!("[bluesky] {}", msg);
                return Err(msg);
            };

            let final_bytes = if bytes.len() > 2 * 1024 * 1024 {
                compress_image_for_bluesky(&bytes, bytes.len())?
            } else {
                bytes
            };

            tracing::info!("[bluesky] Uploading {} bytes (mime={}) as blob", final_bytes.len(), mime);
            let blob = self.upload_blob(final_bytes, &mime).await?;
            images.push(serde_json::json!({
                "image": blob["blob"],
                "alt": "",
            }));
        }
        Ok(serde_json::json!({
            "$type": "app.bsky.embed.images",
            "images": images,
        }))
    }

    /// Create a Bluesky post (app.bsky.feed.post) with DPoP auth. Returns the AT URI.
    pub async fn create_record(&self, did: &str, text: &str, media_urls: &[String], link_url: Option<&str>) -> Result<String, String> {
        let facets = build_bluesky_facets(text);

        let embed = if !media_urls.is_empty() {
            let embed_value = self.build_image_embed_dpop(media_urls).await?;
            Some(embed_value)
        } else if let Some(link) = link_url {
            let link_embed = build_link_embed(link).await?;
            Some(link_embed)
        } else {
            None
        };

        let mut record = serde_json::json!({
            "$type": "app.bsky.feed.post",
            "text": text,
            "createdAt": chrono::Utc::now().to_rfc3339(),
        });

        if !facets.is_empty() {
            record["facets"] = serde_json::json!(facets);
        }
        if let Some(emb) = embed {
            record["embed"] = emb;
        }

        let body = serde_json::json!({
            "repo": did,
            "collection": "app.bsky.feed.post",
            "record": record,
        });

        let result = self.xrpc_post("com.atproto.repo.createRecord", &body).await?;
        Ok(result["uri"].as_str().unwrap_or("").to_string())
    }

    /// Create a Bluesky reply (app.bsky.feed.post with reply envelope) via DPoP.
    /// OAuth DPoP-bound tokens reject plain Bearer, so this goes through xrpc_post.
    pub async fn create_reply_dpop(
        &self,
        did: &str,
        root_uri: &str,
        parent_uri: &str,
        text: &str,
    ) -> Result<String, String> {
        let facets = build_bluesky_facets(text);
        let mut record = serde_json::json!({
            "$type": "app.bsky.feed.post",
            "text": text,
            "reply": {
                "root": { "uri": root_uri, "cid": "" },
                "parent": { "uri": parent_uri, "cid": "" }
            },
            "createdAt": chrono::Utc::now().to_rfc3339(),
        });
        if !facets.is_empty() {
            record["facets"] = serde_json::json!(facets);
        }

        let body = serde_json::json!({
            "repo": did,
            "collection": "app.bsky.feed.post",
            "record": record,
        });

        let result = self.xrpc_post("com.atproto.repo.createRecord", &body).await?;
        Ok(result["uri"].as_str().unwrap_or("").to_string())
    }
}

// ---------------------------------------------------------------------------
// Public (unauthorized) Bluesky API — no DPoP needed
// Uses the public API gateway at api.bsky.app
// ---------------------------------------------------------------------------
pub struct BlueskyPublicApi;

impl BlueskyPublicApi {
    /// Fetch the Bluesky discover feed (what's trending on the network).
    pub async fn get_discover_feed(limit: u32) -> Result<Value, String> {
        Self::get_feed("at://did:plc:z72i7hdynmb6y22adngq24og/app.bsky.feed.generator/whale", limit).await
    }

    /// Fetch a custom feed by URI via the public API.
    pub async fn get_feed(feed_uri: &str, limit: u32) -> Result<Value, String> {
        let url = format!(
            "{}/xrpc/app.bsky.feed.getFeed?feed={}&limit={}",
            BSKY_PUBLIC_API,
            urlencoding(feed_uri),
            limit
        );

        let resp = reqwest::get(&url)
            .await
            .map_err(|e| format!("Public feed request failed: {}", e))?;

        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("Read failed: {}", e))?;

        if !status.is_success() {
            return Err(format!("Public feed error ({}): {}", status.as_u16(), text));
        }

        serde_json::from_str(&text).map_err(|e| format!("JSON parse failed: {} — body: {}", e, text))
    }

    /// Search posts on the network.
    pub async fn search_posts(query: &str, limit: u32) -> Result<Value, String> {
        let url = format!(
            "{}/xrpc/app.bsky.feed.searchPosts?q={}&limit={}",
            BSKY_PUBLIC_API,
            urlencoding(query),
            limit
        );

        let resp = reqwest::get(&url)
            .await
            .map_err(|e| format!("Search request failed: {}", e))?;

        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("Read failed: {}", e))?;

        if !status.is_success() {
            return Err(format!("Search error ({}): {}", status.as_u16(), text));
        }

        serde_json::from_str(&text).map_err(|e| format!("JSON parse failed: {} — body: {}", e, text))
    }

    /// Search for actors (accounts) on the network. Still works without auth.
    pub async fn search_actors(query: &str, limit: u32) -> Result<Value, String> {
        let url = format!(
            "{}/xrpc/app.bsky.actor.searchActors?q={}&limit={}",
            BSKY_PUBLIC_API,
            urlencoding(query),
            limit
        );

        let resp = reqwest::get(&url)
            .await
            .map_err(|e| format!("Actor search request failed: {}", e))?;

        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("Read failed: {}", e))?;

        if !status.is_success() {
            return Err(format!("Actor search error ({}): {}", status.as_u16(), text));
        }

        serde_json::from_str(&text).map_err(|e| format!("JSON parse failed: {} — body: {}", e, text))
    }

    /// Get a user's posts (author feed). Works without auth for public accounts.
    pub async fn get_author_feed(actor_did: &str, limit: u32) -> Result<Value, String> {
        let url = format!(
            "{}/xrpc/app.bsky.feed.getAuthorFeed?actor={}&limit={}",
            BSKY_PUBLIC_API,
            urlencoding(actor_did),
            limit
        );

        let resp = reqwest::get(&url)
            .await
            .map_err(|e| format!("Author feed request failed: {}", e))?;

        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("Read failed: {}", e))?;

        if !status.is_success() {
            return Err(format!("Author feed error ({}): {}", status.as_u16(), text));
        }

        serde_json::from_str(&text).map_err(|e| format!("JSON parse failed: {} — body: {}", e, text))
    }

    /// Get timeline-like feed from the network (not user-specific, just public).
    pub async fn get_timeline(limit: u32) -> Result<Value, String> {
        // Use the what's hot feed as a public timeline alternative
        Self::get_discover_feed(limit).await
    }

    /// Get feed generator info (display name, description, etc).
    pub async fn get_feed_generator(feed_uri: &str) -> Result<Value, String> {
        let url = format!(
            "{}/xrpc/app.bsky.feed.getFeedGenerator?feed={}",
            BSKY_PUBLIC_API,
            urlencoding(feed_uri)
        );
        let resp = reqwest::get(&url).await.map_err(|e| format!("Feed generator request failed: {}", e))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("Read failed: {}", e))?;
        if !status.is_success() {
            return Err(format!("Feed generator error ({}): {}", status.as_u16(), text));
        }
        serde_json::from_str(&text).map_err(|e| format!("JSON parse failed: {} — body: {}", e, text))
    }
}

/// Login with Bluesky app password (not OAuth). Returns (accessJwt, did, handle, refreshJwt).
pub async fn create_session(
    http: &reqwest::Client,
    identifier: &str,
    password: &str,
    pds_url: &str,
) -> Result<(String, String, String, String), String> {
    let url = format!("{}/xrpc/com.atproto.server.createSession", pds_url);
    let body = serde_json::json!({
        "identifier": identifier,
        "password": password,
    });

    let resp = http
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Bluesky session creation failed: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("Read failed: {}", e))?;

    if !status.is_success() {
        return Err(format!("Bluesky login failed ({}): {}", status.as_u16(), text));
    }

    let json: Value = serde_json::from_str(&text)
        .map_err(|e| format!("JSON parse failed: {} — body: {}", e, text))?;

    let access_jwt = json["accessJwt"].as_str().ok_or("No accessJwt in session response")?;
    let refresh_jwt = json["refreshJwt"].as_str().unwrap_or("").to_string();
    let did = json["did"].as_str().ok_or("No DID in session response")?;
    let handle = json["handle"].as_str().ok_or("No handle in session response")?;

    Ok((access_jwt.to_string(), did.to_string(), handle.to_string(), refresh_jwt))
}

/// Fetch profile info using Bearer auth. Returns (followers, follows, posts).
pub async fn get_profile(http: &reqwest::Client, did: &str, access_token: &str, pds_url: &str) -> Result<(u32, u32, u32, String), String> {
    let url = format!("{}/xrpc/app.bsky.actor.getProfile?actor={}", pds_url, did);
    let resp = http
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| format!("Profile fetch failed: {}", e))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("Read failed: {}", e))?;
    if !status.is_success() {
        return Err(format!("Profile error ({}): {}", status.as_u16(), truncate(&text)));
    }
    let json: Value = serde_json::from_str(&text).map_err(|e| format!("JSON parse: {}", e))?;
    let followers = json["followersCount"].as_u64().unwrap_or(0) as u32;
    let follows = json["followsCount"].as_u64().unwrap_or(0) as u32;
    let posts = json["postsCount"].as_u64().unwrap_or(0) as u32;
    let display_name = json["displayName"].as_str().unwrap_or("").to_string();
    Ok((followers, follows, posts, display_name))
}

/// Full Bluesky profile data for importing during OAuth.
#[derive(Debug, Clone, Default)]
pub struct BlueskyProfile {
    pub display_name: String,
    pub description: String,
    pub avatar_url: Option<String>,
    pub banner_url: Option<String>,
    pub followers_count: u32,
    pub follows_count: u32,
    pub posts_count: u32,
}

/// Fetch full profile info (avatar, banner, bio, stats) using Bearer auth.
pub async fn get_profile_full(http: &reqwest::Client, did: &str, access_token: &str, pds_url: &str) -> Result<BlueskyProfile, String> {
    let url = format!("{}/xrpc/app.bsky.actor.getProfile?actor={}", pds_url, did);
    let resp = http
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| format!("Profile fetch failed: {}", e))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("Read failed: {}", e))?;
    if !status.is_success() {
        return Err(format!("Profile error ({}): {}", status.as_u16(), truncate(&text)));
    }
    let json: Value = serde_json::from_str(&text).map_err(|e| format!("JSON parse: {}", e))?;
    Ok(BlueskyProfile {
        display_name: json["displayName"].as_str().unwrap_or("").to_string(),
        description: json["description"].as_str().unwrap_or("").to_string(),
        avatar_url: json["avatar"].as_str().map(|s| s.to_string()),
        banner_url: json["banner"].as_str().map(|s| s.to_string()),
        followers_count: json["followersCount"].as_u64().unwrap_or(0) as u32,
        follows_count: json["followsCount"].as_u64().unwrap_or(0) as u32,
        posts_count: json["postsCount"].as_u64().unwrap_or(0) as u32,
    })
}

/// Fetch follows list (paginated). Returns Vec<(did, handle, display_name)>.
pub async fn get_follows(http: &reqwest::Client, did: &str, access_token: &str, pds_url: &str) -> Result<Vec<(String, String, String)>, String> {
    let mut all: Vec<(String, String, String)> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut page = 0u32;
    loop {
        page += 1;
        let mut url = format!("{}/xrpc/app.bsky.graph.getFollows?actor={}&limit=100", pds_url, did);
        if let Some(ref c) = cursor { url = format!("{}&cursor={}", url, urlencoding(c)); }
        eprintln!("[get_follows] page {} URL: {}", page, url);
        let resp = http.get(&url).header("Authorization", format!("Bearer {}", access_token)).send().await.map_err(|e| format!("Follows failed: {}", e))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("Read: {}", e))?;
        if !status.is_success() { return Err(format!("Follows error (page {}, {}): {}", page, status.as_u16(), truncate(&text))); }
        let json: Value = serde_json::from_str(&text).map_err(|e| format!("JSON parse (page {}): {}", page, e))?;
        let raw_cursor = json["cursor"].clone();
        let page_count = json["follows"].as_array().map_or(0, |a| a.len());
        eprintln!("[get_follows] page {} returned {} follows, raw_cursor={:?}", page, page_count, raw_cursor);
        if let Some(arr) = json["follows"].as_array() {
            for f in arr {
                let follow_did = f["did"].as_str().unwrap_or("").to_string();
                let handle = f["handle"].as_str().unwrap_or("").to_string();
                let display = f["displayName"].as_str().unwrap_or("").to_string();
                all.push((follow_did, handle, display));
            }
        }
        cursor = raw_cursor.as_str().map(|s| s.to_string())
            .or_else(|| raw_cursor.as_i64().map(|n| n.to_string()));
        eprintln!("[get_follows] page {} cursor parsed as: {:?}", page, cursor);
        if cursor.is_none() { break; }
    }
    eprintln!("[get_follows] FINAL: {} total follows across {} pages", all.len(), page);
    Ok(all)
}

/// Count follow records via com.atproto.repo.listRecords — the authoritative count
/// from the user's repository. Returns Vec of followed DIDs.
pub async fn count_follow_records(http: &reqwest::Client, repo: &str, pds_url: &str, access_token: &str) -> Result<Vec<String>, String> {
    let mut all_dids: Vec<String> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut page = 0u32;
    loop {
        page += 1;
        let mut url = format!("{}/xrpc/com.atproto.repo.listRecords?repo={}&collection=app.bsky.graph.follow&limit=100", pds_url, urlencoding(repo));
        if let Some(ref c) = cursor { url = format!("{}&cursor={}", url, urlencoding(c)); }
        let resp = http.get(&url).header("Authorization", format!("Bearer {}", access_token)).send().await
            .map_err(|e| format!("listRecords failed: {}", e))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("Read: {}", e))?;
        if !status.is_success() { return Err(format!("listRecords error (page {}, {}): {}", page, status.as_u16(), truncate(&text))); }
        let json: Value = serde_json::from_str(&text).map_err(|e| format!("JSON parse (page {}): {}", page, e))?;
        let page_count = json["records"].as_array().map_or(0, |a| a.len());
        eprintln!("[count_follow_records] page {} returned {} records", page, page_count);
        if let Some(records) = json["records"].as_array() {
            for rec in records {
                if let Some(subject) = rec["value"]["subject"].as_str() {
                    all_dids.push(subject.to_string());
                }
            }
        }
        cursor = json["cursor"].as_str().map(|s| s.to_string());
        if cursor.is_none() { break; }
    }
    eprintln!("[count_follow_records] FINAL: {} total follow records across {} pages", all_dids.len(), page);
    Ok(all_dids)
}

/// Fetch saved Bluesky feeds from user preferences. Returns Vec of feed AT-URIs.
pub async fn get_saved_feeds(http: &reqwest::Client, pds_url: &str, access_token: &str) -> Result<Vec<String>, String> {
    let url = format!("{}/xrpc/app.bsky.actor.getPreferences", pds_url);
    let resp = http.get(&url).header("Authorization", format!("Bearer {}", access_token)).send().await
        .map_err(|e| format!("Preferences fetch failed: {}", e))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("Read: {}", e))?;
    if !status.is_success() { return Err(format!("Preferences error: {}", truncate(&text))); }
    let json: Value = serde_json::from_str(&text).map_err(|e| format!("JSON: {}", e))?;

    let mut feeds = Vec::new();
    if let Some(arr) = json["preferences"].as_array() {
        for pref in arr {
            let t = pref["$type"].as_str().unwrap_or("");
            if t.contains("savedFeedsPref") {
                // V2 format: items array with type/value/id/pinned
                if let Some(items) = pref["items"].as_array() {
                    for item in items {
                        if item["type"].as_str() == Some("feed")
                            && let Some(val) = item["value"].as_str() {
                                feeds.push(val.to_string());
                            }
                    }
                }
                // V1 format: pinned/saved string arrays
                for key in &["pinned", "saved"] {
                    if let Some(arr2) = pref[key].as_array() {
                        for v in arr2 {
                            if let Some(s) = v.as_str() {
                                feeds.push(s.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    feeds.sort();
    feeds.dedup();
    tracing::info!("Bluesky get_saved_feeds found {} saved feeds", feeds.len());
    Ok(feeds)
}

/// Batch-resolve profiles by DID. Returns JSON profile objects.
pub async fn get_profiles(http: &reqwest::Client, pds_url: &str, access_token: &str, dids: &[String]) -> Result<Vec<Value>, String> {
    if dids.is_empty() { return Ok(Vec::new()); }
    let params: Vec<String> = dids.iter().map(|d| format!("actors={}", urlencoding(d))).collect();
    let url = format!("{}/xrpc/app.bsky.actor.getProfiles?{}", pds_url, params.join("&"));
    let resp = http.get(&url).header("Authorization", format!("Bearer {}", access_token)).send().await
        .map_err(|e| format!("getProfiles failed: {}", e))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("Read: {}", e))?;
    if !status.is_success() { return Err(format!("getProfiles error ({}): {}", status.as_u16(), truncate(&text))); }
    let json: Value = serde_json::from_str(&text).map_err(|e| format!("JSON: {}", e))?;
    Ok(json["profiles"].as_array().cloned().unwrap_or_default())
}

/// Fetch followers list (paginated). Returns Vec<(did, handle, display_name, avatar)>.
pub async fn get_followers(http: &reqwest::Client, did: &str, access_token: &str, pds_url: &str) -> Result<Vec<(String, String, String, Option<String>)>, String> {
    let mut all: Vec<(String, String, String, Option<String>)> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut page = 0u32;
    loop {
        page += 1;
        let mut url = format!("{}/xrpc/app.bsky.graph.getFollowers?actor={}&limit=100", pds_url, did);
        if let Some(ref c) = cursor { url = format!("{}&cursor={}", url, urlencoding(c)); }
        let resp = http.get(&url).header("Authorization", format!("Bearer {}", access_token)).send().await.map_err(|e| format!("Followers failed: {}", e))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("Read: {}", e))?;
        if !status.is_success() { return Err(format!("Followers error (page {}, {}): {}", page, status.as_u16(), truncate(&text))); }
        let json: Value = serde_json::from_str(&text).map_err(|e| format!("JSON parse (page {}): {}", page, e))?;
        let page_count = json["followers"].as_array().map_or(0, |a| a.len());
        tracing::info!("Bluesky get_followers page {} returned {} followers", page, page_count);
        if let Some(arr) = json["followers"].as_array() {
            for f in arr {
                let did = f["did"].as_str().unwrap_or("").to_string();
                let handle = f["handle"].as_str().unwrap_or("").to_string();
                let display = f["displayName"].as_str().unwrap_or("").to_string();
                let avatar = f["avatar"].as_str().map(|s| s.to_string());
                all.push((did, handle, display, avatar));
            }
        }
        cursor = json["cursor"].as_str().map(|s| s.to_string())
            .or_else(|| json["cursor"].as_i64().map(|n| n.to_string()));
        if cursor.is_none() { break; }
    }
    tracing::info!("Bluesky get_followers: {} total followers across {} pages", all.len(), page);
    Ok(all)
}

fn truncate(s: &str) -> String {
    let end = s.len().min(300);
    s[..end].to_string()
}

/// Canonical base32 CID string ("bafy..."). Accepts an already-canonical
/// value or the hex-encoded CID bytes the firehose consumer stored ("000171...",
/// optionally with a leading 0x00 tag byte). Likes/reposts need this — a hex
/// value in the record's subject.cid gets "Invalid CID string" from the PDS.
pub fn canonical_cid(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if raw.starts_with("baf") {
        return Some(raw.to_string());
    }
    let bytes = hex::decode(raw).ok()?;
    cid_from_bytes(&bytes)
}

/// Convert raw CID bytes (0x01 0x71 0x12 0x20 <sha2-256 digest>, optionally
/// 0x00-tagged) to the canonical base32 string.
pub fn cid_from_bytes(raw: &[u8]) -> Option<String> {
    let bytes = raw.strip_prefix(&[0u8]).unwrap_or(raw);
    if bytes.len() != 36 || bytes[0] != 0x01 || bytes[2] != 0x12 || bytes[3] != 0x20 {
        return None;
    }
    Some(base32_cid_v1(bytes))
}

fn base32_cid_v1(bytes: &[u8]) -> String {
    const ALPHA: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::with_capacity(1 + bytes.len() * 8 / 5 + 1);
    out.push('b');
    let mut acc: u32 = 0;
    let mut nbits = 0u32;
    for &b in bytes {
        acc = (acc << 8) | u32::from(b);
        nbits += 8;
        while nbits >= 5 {
            nbits -= 5;
            out.push(ALPHA[((acc >> nbits) & 31) as usize] as char);
        }
    }
    if nbits > 0 {
        out.push(ALPHA[((acc << (5 - nbits)) & 31) as usize] as char);
    }
    out.make_ascii_lowercase();
    out
}

/// Resolve a record's CID from the public AppView (no auth needed) — fallback
/// when the stored value is missing or unconvertible.
pub async fn resolve_record_cid(uri: &str) -> Option<String> {
    if !uri.starts_with("at://") {
        return None;
    }
    let url = format!(
        "https://api.bsky.app/xrpc/app.bsky.feed.getPosts?uris={}",
        uri
    );
    let json: serde_json::Value = timed_client()
        .get(&url)
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    Some(json["posts"][0]["cid"].as_str()?.to_string())
}

#[cfg(test)]
mod cid_tests {
    use super::*;

    /// Ground truth fetched via com.atproto.repo.getRecord for
    /// at://did:plc:24d4cy44cmgbukyq2g2fl7j5/app.bsky.feed.post/3mybddmuohc2m
    #[test]
    fn hex_cid_bytes_collapse_to_the_real_network_cid() {
        let stored = "0001711220d560754a72ee0aab3cd388016f85c989ff9280784e13db166e18d2fd3e17091e";
        assert_eq!(
            canonical_cid(stored).as_deref(),
            Some("bafyreigvmb2uu4xobkvtzu4iafxylsmj76jia6cocpnrm3qy2l6t4fyjdy")
        );
    }
}

/// The one write path for PDS record operations. The OAuth access token is
/// DPoP-bound: plain-Bearer writes are rejected ("Bad token scope"/"Malformed
/// token") and a nonce-less proof fails too — only BlueskyApiClient, which
/// performs the DPoP nonce challenge, succeeds. Also refreshes an expired
/// session first (the refresh rotates and persists the tokens).
pub async fn repo_write(
    db: &crate::db::DbPool,
    acct: &crate::db::accounts::LinkedAccountRow,
    nsid: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let token = fresh_token(db, acct).await?;
    let seed = hex::decode(acct.dpop_private_key.as_deref().unwrap_or(""))
        .map_err(|e| format!("Bad DPoP key: {}", e))?;
    let client = BlueskyApiClient::new(&token, &seed, &get_pds_url(&acct.metadata))?;
    client.xrpc_post(nsid, body).await
}

/// The one blob-upload path for linked accounts — same refresh + DPoP
/// posture as repo_write (plain Bearer is rejected with "Malformed token").
pub async fn repo_upload_blob(
    db: &crate::db::DbPool,
    acct: &crate::db::accounts::LinkedAccountRow,
    bytes: Vec<u8>,
    mime: &str,
) -> Result<serde_json::Value, String> {
    let token = fresh_token(db, acct).await?;
    let seed = hex::decode(acct.dpop_private_key.as_deref().unwrap_or(""))
        .map_err(|e| format!("Bad DPoP key: {}", e))?;
    let client = BlueskyApiClient::new(&token, &seed, &get_pds_url(&acct.metadata))?;
    client.upload_blob(bytes, mime).await
}

pub(crate) async fn fresh_token(
    db: &crate::db::DbPool,
    acct: &crate::db::accounts::LinkedAccountRow,
) -> Result<String, String> {
    let expiring = acct
        .token_expires_at
        .map(|t| t < chrono::Utc::now() + chrono::Duration::seconds(60))
        .unwrap_or(true);
    if expiring {
        let http = timed_client();
        refresh_session(&http, db, acct).await
    } else {
        Ok(acct.access_token.clone())
    }
}

/// Like a Bluesky post. `post_uri` should be the AT-URI (e.g., at://did:plc:.../app.bsky.feed.post/...).
/// Returns the rkey of the created like record.
pub async fn like_post(
    db: &crate::db::DbPool,
    acct: &crate::db::accounts::LinkedAccountRow,
    post_uri: &str,
    post_cid: &str,
) -> Result<String, String> {
    let body = serde_json::json!({
        "repo": acct.external_id,
        "collection": "app.bsky.feed.like",
        "record": {
            "subject": {
                "uri": post_uri,
                "cid": post_cid,
            },
            "createdAt": chrono::Utc::now().to_rfc3339(),
        }
    });
    let json = repo_write(db, acct, "com.atproto.repo.createRecord", &body).await?;
    let rkey = json["uri"].as_str()
        .and_then(|uri| uri.rsplit('/').next())
        .unwrap_or("")
        .to_string();
    Ok(rkey)
}

/// Delete a Bluesky like record by rkey.
pub async fn delete_like(
    db: &crate::db::DbPool,
    acct: &crate::db::accounts::LinkedAccountRow,
    rkey: &str,
) -> Result<(), String> {
    let body = serde_json::json!({
        "repo": acct.external_id,
        "collection": "app.bsky.feed.like",
        "rkey": rkey,
    });
    repo_write(db, acct, "com.atproto.repo.deleteRecord", &body).await?;
    Ok(())
}

/// Follow a Bluesky user via createRecord (app.bsky.graph.follow).
/// Returns the rkey of the created follow record.
pub async fn follow_user(
    db: &crate::db::DbPool,
    acct: &crate::db::accounts::LinkedAccountRow,
    subject_did: &str,
) -> Result<String, String> {
    let body = serde_json::json!({
        "repo": acct.external_id,
        "collection": "app.bsky.graph.follow",
        "record": {
            "subject": subject_did,
            "createdAt": chrono::Utc::now().to_rfc3339(),
        }
    });
    let json = repo_write(db, acct, "com.atproto.repo.createRecord", &body).await?;
    let rkey = json["uri"].as_str()
        .and_then(|uri| uri.rsplit('/').next())
        .unwrap_or("")
        .to_string();
    Ok(rkey)
}

/// Unfollow a Bluesky user by deleting the follow record.
pub async fn unfollow_user(
    db: &crate::db::DbPool,
    acct: &crate::db::accounts::LinkedAccountRow,
    follow_rkey: &str,
) -> Result<(), String> {
    delete_record(db, acct, "app.bsky.graph.follow", follow_rkey).await
}

/// Resolve a Bluesky handle to a DID using the public API.
pub async fn resolve_handle(http: &reqwest::Client, handle: &str) -> Result<String, String> {
    let url = format!("https://api.bsky.app/xrpc/com.atproto.identity.resolveHandle?handle={}", urlencoding(handle));
    let resp = http.get(&url).send().await
        .map_err(|e| format!("Bluesky resolve failed: {}", e))?;
    let json: serde_json::Value = resp.json().await
        .map_err(|e| format!("Bluesky resolve parse failed: {}", e))?;
    json["did"].as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Could not resolve handle: {}", handle))
}

/// Cached get_viewer_follows: the follow graph changes slowly, and feed
/// builds otherwise paginate it in full on every request.
pub async fn get_viewer_follows(
    http: &reqwest::Client,
    access_token: &str,
    actor: &str,
    dpop_hex: Option<&str>,
) -> Result<std::collections::HashSet<String>, String> {
    static FOLLOW_CACHE: std::sync::LazyLock<
        parking_lot::Mutex<
            std::collections::HashMap<
                String,
                (std::time::Instant, std::collections::HashSet<String>),
            >,
        >,
    > = std::sync::LazyLock::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));
    const TTL: std::time::Duration = std::time::Duration::from_secs(300);
    {
        let map = FOLLOW_CACHE.lock();
        if let Some((at, set)) = map.get(actor) {
            if at.elapsed() < TTL {
                return Ok(set.clone());
            }
        }
    }
    let result = get_viewer_follows_inner(http, access_token, actor, dpop_hex).await;
    if let Ok(set) = &result {
        let mut map = FOLLOW_CACHE.lock();
        map.retain(|_, (at, _)| at.elapsed() < TTL);
        map.insert(actor.to_string(), (std::time::Instant::now(), set.clone()));
    }
    result
}

/// Get the DIDs that the viewer follows. Returns a set of DIDs.
async fn get_viewer_follows_inner(
    http: &reqwest::Client,
    access_token: &str,
    actor: &str,
    dpop_hex: Option<&str>,
) -> Result<std::collections::HashSet<String>, String> {
    // getFollows MUST go to the AppView, not the user's PDS: the OAuth token's
    // scopes carry aud=api.bsky.app#bsky_appview, and the PDS rejects them
    // ("Bad token scope"/"Malformed token"). The AppView requires a DPoP proof
    // for DPoP-bound tokens — plain Bearer is rejected ("DPoP not supported").
    let signing_key = match dpop_hex {
        Some(hex_str) => {
            let bytes = hex::decode(hex_str)
                .map_err(|e| format!("Bad DPoP key: {}", e))?;
            let seed: [u8; 32] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| format!("Bad DPoP key: expected 32 bytes, got {}", bytes.len()))?;
            Some(ed25519_dalek::SigningKey::from_bytes(&seed))
        }
        None => None,
    };
    let ath = oauth::dpop_ath(access_token);
    let mut nonce: Option<String> = None;

    let mut followed_dids = std::collections::HashSet::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut url = format!(
            "https://api.bsky.app/xrpc/app.bsky.graph.getFollows?actor={}&limit=100",
            urlencoding::encode(actor)
        );
        if let Some(c) = &cursor {
            url.push_str(&format!("&cursor={}", c));
        }

        // Build the request, attaching a DPoP proof when we have a key.
        let build = |dpop: Option<String>| {
            let mut req = http.get(&url);
            if let Some(d) = dpop {
                req = req
                    .header("Authorization", format!("DPoP {}", access_token))
                    .header("DPoP", d);
            } else {
                req = req.header("Authorization", format!("Bearer {}", access_token));
            }
            req
        };

        let dpop_proof = match &signing_key {
            Some(sk) => Some(oauth::create_dpop_proof(sk, "GET", &url, Some(&ath), nonce.as_deref())?),
            None => None,
        };
        let resp = build(dpop_proof)
            .send()
            .await
            .map_err(|e| format!("Bluesky getFollows failed: {}", e))?;

        // DPoP nonce challenge: retry once with the server-provided nonce.
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::BAD_REQUEST {
            // Capture the nonce header before consuming the body.
            let header_nonce = resp
                .headers()
                .get("dpop-nonce")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            let body = resp.text().await.unwrap_or_default();
            let parsed: Option<serde_json::Value> = serde_json::from_str(&body).ok();
            let is_nonce = parsed
                .as_ref()
                .and_then(|v| v["error"].as_str())
                == Some("use_dpop_nonce");
            let body_nonce = parsed
                .as_ref()
                .and_then(|v| v["dpop_nonce"].as_str().map(|s| s.to_string()));
            if is_nonce {
                if let Some(nn) = header_nonce.or(body_nonce) {
                    nonce = Some(nn);
                    if let Some(sk) = &signing_key {
                        let d2 = oauth::create_dpop_proof(sk, "GET", &url, Some(&ath), nonce.as_deref())?;
                        let resp2 = build(Some(d2))
                            .send()
                            .await
                            .map_err(|e| format!("Bluesky getFollows retry failed: {}", e))?;
                        if !resp2.status().is_success() {
                            let code = resp2.status().as_u16();
                            let t = resp2.text().await.unwrap_or_default();
                            return Err(format!("Bluesky getFollows failed ({}): {}", code, &t[..t.len().min(200)]));
                        }
                        let json: serde_json::Value = resp2.json().await
                            .map_err(|e| format!("Bluesky getFollows parse failed: {}", e))?;
                        if let Some(follows) = json["follows"].as_array() {
                            for f in follows {
                                if let Some(did) = f["did"].as_str() {
                                    followed_dids.insert(did.to_string());
                                }
                            }
                        }
                        let nc = json["cursor"].as_str().map(|s| s.to_string());
                        if nc.is_none() || nc == cursor {
                            break;
                        }
                        cursor = nc;
                        continue;
                    }
                }
            }
            return Err(format!("Bluesky getFollows failed ({}): {}", status.as_u16(), &body[..body.len().min(200)]));
        }

        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Bluesky getFollows failed ({}): {}", status.as_u16(), &text[..text.len().min(200)]));
        }
        let json: serde_json::Value = resp.json().await
            .map_err(|e| format!("Bluesky getFollows parse failed: {}", e))?;
        if let Some(follows) = json["follows"].as_array() {
            for f in follows {
                if let Some(did) = f["did"].as_str() {
                    followed_dids.insert(did.to_string());
                }
            }
        }
        let next_cursor = json["cursor"].as_str().map(|s| s.to_string());
        if next_cursor.is_none() || next_cursor == cursor {
            break;
        }
        cursor = next_cursor;
    }
    Ok(followed_dids)
}

// ---------------------------------------------------------------------------
// Post composer — create a Bluesky post
// ---------------------------------------------------------------------------

/// Upload a blob (image/video) to Bluesky PDS. Returns the blob reference.
pub async fn upload_blob_to_bluesky(
    http: &reqwest::Client,
    pds_url: &str,
    access_token: &str,
    data: Vec<u8>,
    mime_type: &str,
) -> Result<serde_json::Value, String> {
    let url = format!("{}/xrpc/com.atproto.repo.uploadBlob", pds_url);
    let resp = http
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", mime_type)
        .body(data)
        .send()
        .await
        .map_err(|e| format!("Bluesky blob upload failed: {}", e))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("Read failed: {}", e))?;
    if !status.is_success() {
        return Err(format!("Blob upload failed ({}): {}", status.as_u16(), &text[..text.len().min(200)]));
    }
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("JSON parse failed: {} — body: {}", e, text))?;
    Ok(json)
}

/// Compress an image to fit under Bluesky's 2MB blob limit.
/// Re-encodes as JPEG with progressively lower quality until under the limit.
/// Returns the compressed bytes.
fn compress_image_for_bluesky(data: &[u8], original_size: usize) -> Result<Vec<u8>, String> {
    use image::codecs::jpeg::JpegEncoder;

    if original_size <= 2 * 1024 * 1024 {
        return Ok(data.to_vec());
    }

    tracing::info!(
        "[bluesky] Image is {} bytes (>{:.1} MB), compressing...",
        original_size,
        2.0
    );

    let img = image::load_from_memory(data)
        .map_err(|e| format!("Failed to decode image for compression: {}", e))?;

    // Try progressively lower quality until under 2MB
    for quality in [90, 80, 70, 60, 50] {
        let mut buf = Vec::new();
        let encoder = JpegEncoder::new_with_quality(&mut buf, quality);
        img.write_with_encoder(encoder)
            .map_err(|e| format!("JPEG encode failed at quality {}: {}", quality, e))?;

        tracing::info!(
            "[bluesky] Compressed at quality {}: {} bytes",
            quality,
            buf.len()
        );

        if buf.len() <= 2 * 1024 * 1024 {
            tracing::info!(
                "[bluesky] Compression succeeded: {} -> {} bytes ({:.1} MB -> {:.1} MB)",
                original_size,
                buf.len(),
                original_size as f64 / 1_048_576.0,
                buf.len() as f64 / 1_048_576.0,
            );
            return Ok(buf);
        }
    }

    // If still over 2MB, resize the image
    tracing::warn!(
        "[bluesky] Quality reduction alone insufficient, resizing image"
    );
    let max_dim = 2048u32;
    let resized = img.resize(max_dim, max_dim, image::imageops::FilterType::Lanczos3);
    for quality in [80, 60, 40] {
        let mut buf = Vec::new();
        let encoder = JpegEncoder::new_with_quality(&mut buf, quality);
        resized
            .write_with_encoder(encoder)
            .map_err(|e| format!("JPEG encode failed at quality {}: {}", quality, e))?;

        if buf.len() <= 2 * 1024 * 1024 {
            tracing::info!(
                "[bluesky] Compressed (resized + quality {}): {} -> {} bytes",
                quality,
                original_size,
                buf.len()
            );
            return Ok(buf);
        }
    }

    Err(format!(
        "Could not compress image below 2MB (smallest attempt: still too large)"
    ))
}

/// Build an image embed for Bluesky from media URLs (reads from disk + uploads blobs).
/// Handles local /uploads/ paths and compresses images over 2MB.
pub async fn build_image_embed(
    http: &reqwest::Client,
    pds_url: &str,
    access_token: &str,
    media_urls: &[String],
) -> Result<serde_json::Value, String> {
    let mut images = Vec::new();
    let upload_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    for url in media_urls {
        let (bytes, mime) = if url.starts_with("/uploads/") {
            // Read from local filesystem
            let rel_path = url.trim_start_matches("/uploads/");
            let file_path = upload_root.join("uploads").join(rel_path);
            tracing::info!("[bluesky] Reading media from disk: {}", file_path.display());
            let file_bytes = std::fs::read(&file_path).map_err(|e| {
                let msg = format!("Failed to read {}: {}", file_path.display(), e);
                tracing::error!("[bluesky] {}", msg);
                msg
            })?;
            let mime = if url.ends_with(".png") { "image/png" }
                else if url.ends_with(".gif") { "image/gif" }
                else if url.ends_with(".webp") { "image/webp" }
                else { "image/jpeg" };
            (file_bytes, mime.to_string())
        } else if url.starts_with("http://") || url.starts_with("https://") {
            // Download from HTTP (for external URLs)
            let resp = http.get(url).send().await
                .map_err(|e| {
                    let msg = format!("Failed to download media {}: {}", url, e);
                    tracing::error!("[bluesky] {}", msg);
                    msg
                })?;
            let mime = resp.headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("image/jpeg")
                .to_string();
            let bytes = resp.bytes().await
                .map_err(|e| {
                    let msg = format!("Failed to read media bytes from {}: {}", url, e);
                    tracing::error!("[bluesky] {}", msg);
                    msg
                })?;
            (bytes.to_vec(), mime)
        } else {
            let msg = format!("Unsupported media URL format: {}", url);
            tracing::error!("[bluesky] {}", msg);
            return Err(msg);
        };

        // Compress if over 2MB
        let final_bytes = if bytes.len() > 2 * 1024 * 1024 {
            compress_image_for_bluesky(&bytes, bytes.len())?
        } else {
            bytes
        };

        tracing::info!("[bluesky] Uploading {} bytes (mime={}) as blob", final_bytes.len(), mime);
        let blob = upload_blob_to_bluesky(http, pds_url, access_token, final_bytes, &mime).await?;
        images.push(serde_json::json!({
            "image": blob["blob"],
            "alt": "",
        }));
    }
    Ok(serde_json::json!({
        "$type": "app.bsky.embed.images",
        "images": images,
    }))
}

/// Build a link card (external embed) for Bluesky.
pub async fn build_link_embed(
    url: &str,
) -> Result<serde_json::Value, String> {
    // Fetch the page to get Open Graph metadata
    let resp = reqwest::get(url).await
        .map_err(|e| format!("Failed to fetch link: {}", e))?;
    let html = resp.text().await.unwrap_or_default();

    let title = extract_meta(&html, "og:title")
        .or_else(|| extract_title(&html))
        .unwrap_or_default();
    let description = extract_meta(&html, "og:description").unwrap_or_default();
    let image = extract_meta(&html, "og:image").unwrap_or_default();

    Ok(serde_json::json!({
        "$type": "app.bsky.embed.external",
        "external": {
            "uri": url,
            "title": title,
            "description": description,
            "thumb": if image.is_empty() { serde_json::Value::Null } else {
                serde_json::Value::String(image)
            },
        }
    }))
}

/// Extract a meta property from HTML.
fn extract_meta(html: &str, property: &str) -> Option<String> {
    let pattern = format!("property=\"{}\"", property);
    if let Some(pos) = html.find(&pattern) {
        let after = &html[pos..];
        if let Some(content_start) = after.find("content=\"") {
            let start = content_start + 9;
            let remaining = &after[start..];
            if let Some(end) = remaining.find('"') {
                return Some(remaining[..end].to_string());
            }
        }
    }
    // Also try without quotes around property
    let pattern2 = format!("property='{}'", property);
    if let Some(pos) = html.find(&pattern2) {
        let after = &html[pos..];
        if let Some(content_start) = after.find("content='") {
            let start = content_start + 9;
            let remaining = &after[start..];
            if let Some(end) = remaining.find('\'') {
                return Some(remaining[..end].to_string());
            }
        }
    }
    None
}

/// Extract the <title> tag from HTML.
fn extract_title(html: &str) -> Option<String> {
    if let Some(start) = html.find("<title>") {
        let after = &html[start + 7..];
        if let Some(end) = after.find("</title>") {
            return Some(after[..end].to_string());
        }
    }
    None
}

/// Build Bluesky facets from text (detect @mentions and #hashtags).
fn build_bluesky_facets(text: &str) -> Vec<serde_json::Value> {
    let mut facets = Vec::new();
    // Simple hashtag detection
    for (i, word) in text.split_whitespace().enumerate() {
        if word.starts_with('#') && word.len() > 1 {
            let tag = &word[1..];
            // Find the byte position
            let byte_start = text.split_whitespace()
                .take(i)
                .map(|w| w.len() + 1)
                .sum::<usize>();
            let byte_end = byte_start + word.len();
            facets.push(serde_json::json!({
                "index": {
                    "byteStart": byte_start,
                    "byteEnd": byte_end,
                },
                "features": [{
                    "$type": "app.bsky.richtext.facet#tag",
                    "tag": tag,
                }]
            }));
        }
    }
    facets
}

/// Create a Bluesky post (app.bsky.feed.post). Returns the AT URI.
pub async fn create_post(
    db: &crate::db::DbPool,
    acct: &crate::db::accounts::LinkedAccountRow,
    text: &str,
    media_urls: &[String],
    link_url: Option<&str>,
) -> Result<String, String> {
    // Locals derived from the account so the record/embed construction below
    // (incl. blob upload) is unchanged.
    let client = timed_client();
    let http = &client;
    let pds_url = get_pds_url(&acct.metadata);
    let token = fresh_token(db, acct).await?;
    let access_token = token.as_str();
    let did_owned = acct
        .metadata["did"]
        .as_str()
        .unwrap_or(&acct.external_id)
        .to_string();
    let did = did_owned.as_str();

    let facets = build_bluesky_facets(text);

    let embed = if !media_urls.is_empty() {
        Some(build_image_embed(http, &pds_url, access_token, media_urls).await?)
    } else if let Some(link) = link_url {
        let link_embed = build_link_embed(link).await?;
        Some(link_embed)
    } else {
        None
    };

    let mut record = serde_json::json!({
        "$type": "app.bsky.feed.post",
        "text": text,
        "createdAt": chrono::Utc::now().to_rfc3339(),
    });

    if !facets.is_empty() {
        record["facets"] = serde_json::json!(facets);
    }

    if let Some(emb) = embed {
        record["embed"] = emb;
    }

    let body = serde_json::json!({
        "repo": did,
        "collection": "app.bsky.feed.post",
        "record": record,
    });

    let json = repo_write(db, acct, "com.atproto.repo.createRecord", &body).await?;

    Ok(json["uri"].as_str().unwrap_or("").to_string())
}

/// Resolve a DID to its PDS URL via PLC directory / DID document.
/// Alias for resolve_pds_endpoint for backward compatibility.
pub async fn resolve_pds_from_did(http: &reqwest::Client, did: &str) -> Result<String, String> {
    resolve_pds_endpoint(http, did).await
}

/// Get the correct PDS URL for a Bluesky linked account from its metadata.
/// Checks metadata["pds_url"] first, then metadata["instance"], then resolves from DID.
/// Falls back to "https://bsky.social" as a last resort (which will fail for OAuth tokens).
pub fn get_pds_url(metadata: &serde_json::Value) -> String {
    // First check if we've stored the PDS URL directly
    if let Some(pds) = metadata["pds_url"].as_str() {
        if !pds.is_empty() {
            return pds.to_string();
        }
    }
    // Then check the instance field (used by Mastodon, but may be set for Bluesky too)
    if let Some(inst) = metadata["instance"].as_str() {
        if !inst.is_empty() && inst != "https://bsky.social" {
            return inst.to_string();
        }
    }
    // Fallback — will cause InvalidToken errors for OAuth accounts
    "https://bsky.social".to_string()
}

/// Delete a Bluesky record (com.atproto.repo.deleteRecord).
pub async fn delete_record(
    db: &crate::db::DbPool,
    acct: &crate::db::accounts::LinkedAccountRow,
    collection: &str,
    rkey: &str,
) -> Result<(), String> {
    let body = serde_json::json!({
        "repo": acct.external_id,
        "collection": collection,
        "rkey": rkey,
    });
    repo_write(db, acct, "com.atproto.repo.deleteRecord", &body).await?;
    Ok(())
}



// === Client metadata scope sync ===
// The OAuth AS validates requested scopes against the client metadata served
// from deploy/client-metadata.json (Caddy serves it statically). Keep both
// repo copies byte-identical to BSKY_OAUTH_SCOPES or authorization fails
// with "Scope ... is not declared in the client metadata".
#[cfg(test)]
mod scope_sync_tests {
    #[test]
    fn client_metadata_scopes_match_constant() {
        let expected = super::BSKY_OAUTH_SCOPES;
        for f in [
            concat!(env!("CARGO_MANIFEST_DIR"), "/../deploy/client-metadata.json"),
            concat!(env!("CARGO_MANIFEST_DIR"), "/../ops/client-metadata.json"),
        ] {
            let raw = std::fs::read_to_string(f).unwrap_or_else(|e| panic!("missing {f}: {e}"));
            let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(
                v["scope"].as_str().unwrap_or_default(),
                expected,
                "scope drift in {f} — regenerate it from BSKY_OAUTH_SCOPES"
            );
        }
    }
}
