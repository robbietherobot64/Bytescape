//! Bluesky OAuth Authentication
//!
//! Implements AT Protocol OAuth (PKCE + DPoP + PAR) for native Bluesky login
//! using the `atrium-oauth` crate. This replaces app-password auth as the
//! primary login method while keeping app-password as a fallback.
//!
//! Flow:
//! 1. Frontend calls `start_bluesky_oauth(handle)` → gets auth URL
//! 2. User authenticates in system browser
//! 3. Browser redirects to local callback server (http://127.0.0.1:{port}/callback)
//! 4. Callback server exchanges code for tokens via DPoP + PKCE
//! 5. Session stored, Tauri event emitted → frontend updates

use std::sync::Arc;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex, oneshot};

use atrium_identity::{
    did::{CommonDidResolver, CommonDidResolverConfig, DEFAULT_PLC_DIRECTORY_URL},
    handle::{AtprotoHandleResolver, AtprotoHandleResolverConfig, DnsTxtResolver},
};
use atrium_oauth::{
    AtprotoLocalhostClientMetadata, AuthorizeOptions, CallbackParams, DefaultHttpClient,
    KnownScope, OAuthClient, OAuthClientConfig, OAuthResolverConfig, OAuthSession, Scope,
    store::{session::MemorySessionStore, state::MemoryStateStore},
};

/// Fixed port for the OAuth callback server.
/// The browser redirects here after authentication.
const OAUTH_CALLBACK_PORT: u16 = 28457;

// ─── DNS TXT Resolver via Cloudflare DNS-over-HTTPS (JSON API) ──────────────
//
// Avoids the `hickory-resolver` dependency by using Cloudflare's JSON DNS API.
// Used by `AtprotoHandleResolver` to look up `_atproto.{handle}` TXT records.

struct CloudflareDnsTxtResolver {
    client: reqwest::Client,
}

impl DnsTxtResolver for CloudflareDnsTxtResolver {
    async fn resolve(
        &self,
        query: &str,
    ) -> core::result::Result<Vec<String>, Box<dyn std::error::Error + Send + Sync + 'static>> {
        #[derive(Deserialize)]
        struct DnsResponse {
            #[serde(default, rename = "Answer")]
            answer: Vec<DnsAnswer>,
        }
        #[derive(Deserialize)]
        struct DnsAnswer {
            data: String,
        }

        let url = format!(
            "https://cloudflare-dns.com/dns-query?name={}&type=TXT",
            urlencoding::encode(query)
        );
        let resp = self
            .client
            .get(&url)
            .header("Accept", "application/dns-json")
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(Vec::new());
        }

        let dns: DnsResponse = resp.json().await?;
        Ok(dns
            .answer
            .into_iter()
            .map(|a| a.data.trim_matches('"').to_string())
            .collect())
    }
}

// ─── Concrete Type Aliases ──────────────────────────────────────────────────
//
// The atrium-oauth crate is heavily generic. These aliases pin the concrete
// types for our desktop app configuration.

type OAuthHttp = DefaultHttpClient;
type OAuthDidResolver = CommonDidResolver<OAuthHttp>;
type OAuthHandleResolver = AtprotoHandleResolver<CloudflareDnsTxtResolver, OAuthHttp>;

type ConcreteOAuthClient = OAuthClient<
    MemoryStateStore,
    MemorySessionStore,
    OAuthDidResolver,
    OAuthHandleResolver,
    OAuthHttp,
>;

type ConcreteOAuthSession = OAuthSession<
    OAuthHttp,
    OAuthDidResolver,
    OAuthHandleResolver,
    MemorySessionStore,
>;

// ─── OAuth Manager ─────────────────────────────────────────────────────────

/// Manages the Bluesky OAuth lifecycle: authorization, callback handling,
/// and session storage. Stored in `AppState`.
pub struct OAuthManager {
    client: Arc<ConcreteOAuthClient>,
    callback_shutdown: Mutex<Option<oneshot::Sender<()>>>,
    /// The live OAuth session after successful authentication.
    /// Used for DPoP-authenticated API calls.
    /// Access via `clear_session()`, `has_session()`, etc.
    session: Mutex<Option<ConcreteOAuthSession>>,
}

impl OAuthManager {
    /// Create a new OAuthManager with the AT Protocol localhost client metadata.
    pub fn new() -> Result<Self, String> {
        let http_client = Arc::new(DefaultHttpClient::default());
        let redirect_uri = format!("http://127.0.0.1:{}/callback", OAUTH_CALLBACK_PORT);

        let config = OAuthClientConfig {
            client_metadata: AtprotoLocalhostClientMetadata {
                redirect_uris: Some(vec![redirect_uri]),
                scopes: Some(vec![
                    Scope::Known(KnownScope::Atproto),
                    Scope::Known(KnownScope::TransitionGeneric),
                ]),
            },
            keys: None,
            resolver: OAuthResolverConfig {
                did_resolver: CommonDidResolver::new(CommonDidResolverConfig {
                    plc_directory_url: DEFAULT_PLC_DIRECTORY_URL.to_string(),
                    http_client: Arc::clone(&http_client),
                }),
                handle_resolver: AtprotoHandleResolver::new(AtprotoHandleResolverConfig {
                    dns_txt_resolver: CloudflareDnsTxtResolver {
                        client: reqwest::Client::new(),
                    },
                    http_client: Arc::clone(&http_client),
                }),
                authorization_server_metadata: Default::default(),
                protected_resource_metadata: Default::default(),
            },
            state_store: MemoryStateStore::default(),
            session_store: MemorySessionStore::default(),
        };

        let client =
            OAuthClient::new(config).map_err(|e| format!("OAuth client init failed: {}", e))?;

        Ok(Self {
            client: Arc::new(client),
            callback_shutdown: Mutex::new(None),
            session: Mutex::new(None),
        })
    }

    /// Start the OAuth authorization flow for a given Bluesky handle.
    ///
    /// 1. Spins up a temporary callback server on `127.0.0.1:{OAUTH_CALLBACK_PORT}`
    /// 2. Calls the authorization server to get an auth URL
    /// 3. Returns the URL for the frontend to open in the system browser
    pub async fn start_auth(
        &self,
        handle: &str,
        app_handle: AppHandle,
    ) -> Result<String, String> {
        // Stop any previous callback server
        self.stop_callback_server().await;

        // Spawn raw TCP callback server (avoids warp HRTB lifetime issues with tokio::spawn)
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let client = Arc::clone(&self.client);
        let app = app_handle.clone();

        tokio::spawn(async move {
            Self::run_callback_server(client, shutdown_rx, app).await;
        });

        *self.callback_shutdown.lock().await = Some(shutdown_tx);

        // Get authorization URL.
        // Pass the handle as `state` so we get it back in the callback
        // (the library stores it alongside PKCE state internally).
        let url = self
            .client
            .authorize(
                handle,
                AuthorizeOptions {
                    scopes: vec![
                        Scope::Known(KnownScope::Atproto),
                        Scope::Known(KnownScope::TransitionGeneric),
                    ],
                    state: Some(handle.to_string()),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| format!("OAuth authorization failed: {}", e))?;

        Ok(url)
    }

    /// Run the callback server using raw TCP.
    ///
    /// Accepts connections and dispatches each to `handle_tcp_connection`.
    /// Stops when the shutdown signal is received.
    async fn run_callback_server(
        client: Arc<ConcreteOAuthClient>,
        mut shutdown_rx: oneshot::Receiver<()>,
        app_handle: AppHandle,
    ) {
        let listener = match tokio::net::TcpListener::bind(format!(
            "127.0.0.1:{}",
            OAUTH_CALLBACK_PORT
        ))
        .await
        {
            Ok(l) => l,
            Err(e) => {
                eprintln!(
                    "OAuth: Failed to bind callback server on port {}: {}",
                    OAUTH_CALLBACK_PORT, e
                );
                let _ = app_handle.emit(
                    "oauth-error",
                    format!("Failed to start callback server: {}", e),
                );
                return;
            }
        };

        println!(
            "OAuth: Callback server listening on http://127.0.0.1:{}",
            OAUTH_CALLBACK_PORT
        );

        loop {
            tokio::select! {
                result = listener.accept() => {
                    if let Ok((stream, _)) = result {
                        let client = Arc::clone(&client);
                        let app = app_handle.clone();
                        tokio::spawn(async move {
                            Self::handle_tcp_connection(stream, client, app).await;
                        });
                    }
                }
                _ = &mut shutdown_rx => {
                    println!("OAuth: Callback server stopped");
                    return;
                }
            }
        }
    }

    /// Handle a single TCP connection from the browser redirect.
    async fn handle_tcp_connection(
        mut stream: tokio::net::TcpStream,
        client: Arc<ConcreteOAuthClient>,
        app_handle: AppHandle,
    ) {
        // Read the HTTP request
        let mut buf = vec![0u8; 8192];
        let n = match stream.read(&mut buf).await {
            Ok(n) if n > 0 => n,
            _ => return,
        };

        let request = String::from_utf8_lossy(&buf[..n]);
        let first_line = request.lines().next().unwrap_or("");
        let path = first_line.split_whitespace().nth(1).unwrap_or("");

        let html = if path.starts_with("/callback") {
            if let Some(query_str) = path.split_once('?').map(|(_, q)| q) {
                let query = Self::parse_query_string(query_str);
                Self::process_callback(query, client, app_handle).await
            } else {
                Self::error_html("Missing query parameters")
            }
        } else {
            Self::error_html("Not found")
        };

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            html.len(),
            html
        );

        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.flush().await;
    }

    /// Parse OAuth callback query string parameters.
    fn parse_query_string(qs: &str) -> OAuthCallbackQuery {
        let mut code = None;
        let mut state = None;
        let mut iss = None;
        let mut error = None;
        let mut error_description = None;

        for pair in qs.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                let decoded = urlencoding::decode(value)
                    .unwrap_or(std::borrow::Cow::Borrowed(value))
                    .to_string();
                match key {
                    "code" => code = Some(decoded),
                    "state" => state = Some(decoded),
                    "iss" => iss = Some(decoded),
                    "error" => error = Some(decoded),
                    "error_description" => error_description = Some(decoded),
                    _ => {}
                }
            }
        }

        OAuthCallbackQuery {
            code,
            state,
            iss,
            error,
            error_description,
        }
    }

    /// Process the OAuth callback: exchange code for tokens, store session, emit event.
    async fn process_callback(
        query: OAuthCallbackQuery,
        client: Arc<ConcreteOAuthClient>,
        app_handle: AppHandle,
    ) -> String {
        // Check for OAuth error response (e.g. user denied access)
        if let Some(ref error) = query.error {
            let desc = query
                .error_description
                .as_deref()
                .unwrap_or("Unknown error");
            eprintln!("OAuth: Authorization denied: {} — {}", error, desc);
            let _ = app_handle.emit("oauth-error", format!("Authorization denied: {}", desc));
            return Self::error_html(desc);
        }

        // Extract required params
        let code = match query.code {
            Some(c) => c,
            None => {
                let _ = app_handle.emit("oauth-error", "Missing authorization code".to_string());
                return Self::error_html("Missing authorization code");
            }
        };

        let state = match query.state {
            Some(s) => s,
            None => {
                let _ = app_handle.emit("oauth-error", "Missing state parameter".to_string());
                return Self::error_html("Missing state parameter");
            }
        };

        let params = CallbackParams {
            code,
            state: Some(state),
            iss: query.iss,
        };

        match client.callback(params).await {
            Ok((session, returned_state)) => {
                // returned_state contains the handle we passed during authorize()
                let handle = returned_state.unwrap_or_else(|| "unknown".to_string());

                // Get DID from session via SessionManager trait
                use atrium_api::agent::SessionManager;
                let did = session
                    .did()
                    .await
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "unknown".to_string());

                println!(
                    "OAuth: Authentication successful — did={}, handle={}",
                    did, handle
                );

                // Access AppState to store session and identity
                let app_state = app_handle.state::<crate::AppState>();

                // Store in keychain with sentinel JWT values —
                // real API calls use the OAuthSession's DPoP client, not these.
                let _ = app_state.keychain.save_session(
                    did.clone(),
                    handle.clone(),
                    crate::OAUTH_SESSION_SENTINEL.to_string(),
                    crate::OAUTH_SESSION_SENTINEL.to_string(),
                );

                // Store the live OAuth session for future DPoP-authenticated API calls
                *app_state.oauth_manager.session.lock().await = Some(session);

                // Auto-generate local identity if one doesn't exist yet
                let recovery_status = app_state
                    .keychain
                    .get_identity_recovery_status()
                    .unwrap_or_else(|_| crate::keychain_encrypted::IdentityRecoveryStatus {
                        has_local_identity: false,
                        has_escrow: false,
                        recovery_pending: false,
                        recovery_verified_pending_finalize: false,
                        did: None,
                        public_key: None,
                        last_recovered_at: None,
                    });
                if !recovery_status.has_local_identity {
                    let _ = app_state
                        .keychain
                        .identity_init_full_recovery("bytescape-auto-init");
                }

                let local_username = app_state.keychain.get_local_username().unwrap_or(None);
                let identity_ready = local_username.is_some();

                // Emit success event to frontend
                let auth_status = serde_json::json!({
                    "handle": handle,
                    "did": did,
                    "source": "bluesky",
                    "identity_ready": identity_ready,
                    "local_username": local_username,
                });
                let _ = app_handle.emit("oauth-complete", auth_status);

                // Stop the callback server after a short delay
                let app_for_shutdown = app_handle.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    let st = app_for_shutdown.state::<crate::AppState>();
                    st.oauth_manager.stop_callback_server().await;
                });

                Self::success_html()
            }
            Err(e) => {
                eprintln!("OAuth: Callback error: {}", e);
                let _ = app_handle.emit("oauth-error", format!("{}", e));
                Self::error_html(&format!("{}", e))
            }
        }
    }

    fn success_html() -> String {
        r#"<!DOCTYPE html>
<html><body style="background:#1a1a2e;color:#e0e0e0;font-family:sans-serif;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;">
<div style="text-align:center;">
<h1 style="font-size:48px;margin-bottom:16px;">✅</h1>
<h2>Authentication Successful</h2>
<p style="color:#a0a0b0;">You can close this window and return to Bytescape.</p>
</div></body></html>"#
            .to_string()
    }

    fn error_html(message: &str) -> String {
        format!(
            r#"<!DOCTYPE html>
<html><body style="background:#1a1a2e;color:#e0e0e0;font-family:sans-serif;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;">
<div style="text-align:center;">
<h1 style="font-size:48px;margin-bottom:16px;">❌</h1>
<h2>Authentication Failed</h2>
<p style="color:#a0a0b0;">{}</p>
<p style="color:#666;margin-top:24px;">You can close this window and try again in Bytescape.</p>
</div></body></html>"#,
            message
        )
    }

    /// Stop the callback server if running.
    pub async fn stop_callback_server(&self) {
        if let Some(tx) = self.callback_shutdown.lock().await.take() {
            let _ = tx.send(());
        }
    }

    /// Clear the stored OAuth session (used during logout).
    pub async fn clear_session(&self) {
        *self.session.lock().await = None;
    }

    /// Check whether an OAuth session is currently active.
    pub async fn has_session(&self) -> bool {
        self.session.lock().await.is_some()
    }

    /// Get the user's PDS URL from the active OAuth session.
    ///
    /// The `OAuthSession` implements `XrpcClient` whose `base_uri()` returns
    /// the PDS endpoint that issued the DPoP tokens. All authenticated XRPC
    /// requests must target this URL, not the public appview (`bsky.social`).
    pub async fn get_pds_url(&self) -> Result<String, String> {
        use atrium_xrpc::XrpcClient;

        let session_guard = self.session.lock().await;
        let session = session_guard
            .as_ref()
            .ok_or_else(|| "No OAuth session — please log in again".to_string())?;

        Ok(session.base_uri())
    }

    /// Make an authenticated GET request using the OAuth session's DPoP credentials.
    ///
    /// The `OAuthSession` implements `atrium_xrpc::HttpClient` which automatically
    /// adds the `Authorization: DPoP {token}` and `DPoP: {proof}` headers.
    /// Returns the response body as a UTF-8 string.
    pub async fn authenticated_get(&self, url: &str) -> Result<String, String> {
        use atrium_xrpc::HttpClient;
        use atrium_xrpc::http;

        let session_guard = self.session.lock().await;
        let session = session_guard
            .as_ref()
            .ok_or_else(|| "No OAuth session — please log in again".to_string())?;

        let request = http::Request::builder()
            .method(http::Method::GET)
            .uri(url)
            .header("Accept", "application/json")
            .body(Vec::new())
            .map_err(|e| format!("Failed to build request: {}", e))?;

        let response = session
            .send_http(request)
            .await
            .map_err(|e| format!("OAuth request failed: {}", e))?;

        let status = response.status();
        let body = String::from_utf8(response.into_body())
            .map_err(|e| format!("Invalid UTF-8 response: {}", e))?;

        if !status.is_success() {
            return Err(format!("API returned {}: {}", status, body));
        }

        Ok(body)
    }

    /// Make an authenticated POST request using the OAuth session's DPoP credentials.
    ///
    /// `content_type` should be `"application/json"` for XRPC calls.
    /// Returns the response body as a UTF-8 string.
    pub async fn authenticated_post(
        &self,
        url: &str,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<String, String> {
        use atrium_xrpc::HttpClient;
        use atrium_xrpc::http;

        let session_guard = self.session.lock().await;
        let session = session_guard
            .as_ref()
            .ok_or_else(|| "No OAuth session — please log in again".to_string())?;

        let request = http::Request::builder()
            .method(http::Method::POST)
            .uri(url)
            .header("Content-Type", content_type)
            .header("Accept", "application/json")
            .body(body)
            .map_err(|e| format!("Failed to build request: {}", e))?;

        let response = session
            .send_http(request)
            .await
            .map_err(|e| format!("OAuth request failed: {}", e))?;

        let status = response.status();
        let resp_body = String::from_utf8(response.into_body())
            .map_err(|e| format!("Invalid UTF-8 response: {}", e))?;

        if !status.is_success() {
            return Err(format!("API returned {}: {}", status, resp_body));
        }

        Ok(resp_body)
    }
}

/// Query parameters from the OAuth callback redirect.
struct OAuthCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    iss: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}
