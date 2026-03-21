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

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use base64::Engine;
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex, oneshot};

use atrium_api::types::string::Did;
use atrium_common::store::Store;
use atrium_identity::{
    did::{CommonDidResolver, CommonDidResolverConfig, DEFAULT_PLC_DIRECTORY_URL},
    handle::{AtprotoHandleResolver, AtprotoHandleResolverConfig, DnsTxtResolver},
};
use atrium_oauth::{
    AtprotoLocalhostClientMetadata, AuthorizeOptions, CallbackParams, DefaultHttpClient,
    KnownScope, OAuthClient, OAuthClientConfig, OAuthResolverConfig, OAuthSession, Scope,
    store::{
        session::{Session, SessionStore},
        state::MemoryStateStore,
    },
};

/// Preferred port for the OAuth callback server.
/// The browser redirects here after authentication.
const OAUTH_PREFERRED_PORT: u16 = 28457;

/// Fallback ports tried if the preferred port is already in use.
/// All ports are registered in the OAuth client metadata's redirect_uris.
const OAUTH_FALLBACK_PORTS: &[u16] = &[28458, 28459, 28460, 38457];

// ─── DNS TXT Resolver via Cloudflare DNS-over-HTTPS (JSON API) ──────────────
//
// Avoids the `hickory-resolver` dependency by using Cloudflare's JSON DNS API.
// Used by `AtprotoHandleResolver` to look up `_atproto.{handle}` TXT records.

pub(crate) struct CloudflareDnsTxtResolver {
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

// ─── Persistent Session Store (encrypted file-backed) ───────────────────────
//
// Replaces MemorySessionStore so OAuth sessions survive app restarts.
// Uses ChaCha20-Poly1305 + Argon2id (same primitives as EncryptedKeychain).

const SESSION_FILE_NAME: &str = "oauth_sessions.enc";
const SESSION_SALT_LEN: usize = 16;
const SESSION_NONCE_LEN: usize = 12;

/// File-backed session store with at-rest encryption.
///
/// Implements `Store<Did, Session>` + `SessionStore` for atrium-oauth.
/// Sessions are held in an in-memory HashMap for fast access and flushed
/// to an encrypted file on every mutation (`set`, `del`, `clear`).
pub struct FileSessionStore {
    /// In-memory cache of all sessions (DID → Session).
    cache: tokio::sync::Mutex<HashMap<String, Session>>,
    /// Path to the encrypted session file.
    file_path: PathBuf,
    /// Master password for Argon2id key derivation.
    master_password: String,
}

#[derive(Debug, thiserror::Error)]
#[error("session store error: {0}")]
pub struct FileSessionStoreError(String);

impl FileSessionStore {
    /// Create a new FileSessionStore, loading any existing sessions from disk.
    pub fn new(data_dir: PathBuf, master_password: String) -> Self {
        let file_path = data_dir.join(SESSION_FILE_NAME);
        let sessions = Self::load_from_disk(&file_path, &master_password)
            .unwrap_or_default();
        #[cfg(debug_assertions)]
        println!(
            "FileSessionStore: Loaded {} session(s)",
            sessions.len(),
        );
        Self {
            cache: tokio::sync::Mutex::new(sessions),
            file_path,
            master_password,
        }
    }

    /// Derive a 256-bit encryption key from the master password + salt using Argon2id.
    fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32], FileSessionStoreError> {
        use argon2::Argon2;
        let argon2 = Argon2::default();
        let mut key = [0u8; 32];
        argon2
            .hash_password_into(password.as_bytes(), salt, &mut key)
            .map_err(|e| FileSessionStoreError(format!("KDF failed: {}", e)))?;
        Ok(key)
    }

    /// Encrypt and persist the current session map to disk.
    fn flush_to_disk(
        sessions: &HashMap<String, Session>,
        file_path: &PathBuf,
        master_password: &str,
    ) -> Result<(), FileSessionStoreError> {
        use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce, aead::Aead};
        use rand::RngCore;

        let json = serde_json::to_string(sessions)
            .map_err(|e| FileSessionStoreError(format!("Serialize: {}", e)))?;

        let mut salt = vec![0u8; SESSION_SALT_LEN];
        rand::rng().fill_bytes(&mut salt);
        let mut nonce_bytes = [0u8; SESSION_NONCE_LEN];
        rand::rng().fill_bytes(&mut nonce_bytes);

        let key = Self::derive_key(master_password, &salt)?;
        let cipher = ChaCha20Poly1305::new_from_slice(&key)
            .map_err(|e| FileSessionStoreError(format!("Cipher init: {}", e)))?;

        let nonce = Nonce::from(nonce_bytes);
        let ciphertext = cipher
            .encrypt(&nonce, json.as_bytes())
            .map_err(|e| FileSessionStoreError(format!("Encrypt: {}", e)))?;

        // Wire format: salt || nonce || ciphertext → base64-encoded JSON wrapper
        let mut blob = salt;
        blob.extend_from_slice(&nonce_bytes);
        blob.extend_from_slice(&ciphertext);

        let encoded = base64::engine::general_purpose::STANDARD.encode(&blob);
        let wrapper = serde_json::json!({ "encrypted_sessions": encoded });
        let content = serde_json::to_string_pretty(&wrapper)
            .map_err(|e| FileSessionStoreError(format!("JSON wrap: {}", e)))?;

        std::fs::write(file_path, content)
            .map_err(|e| FileSessionStoreError(format!("Write: {}", e)))?;

        Ok(())
    }

    /// Load and decrypt sessions from disk. Returns empty map on any failure.
    fn load_from_disk(
        file_path: &PathBuf,
        master_password: &str,
    ) -> Result<HashMap<String, Session>, FileSessionStoreError> {
        use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce, aead::Aead};

        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(HashMap::new());
            }
            Err(e) => return Err(FileSessionStoreError(format!("Read: {}", e))),
        };

        let wrapper: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| FileSessionStoreError(format!("JSON parse: {}", e)))?;

        let encoded = wrapper["encrypted_sessions"]
            .as_str()
            .ok_or_else(|| FileSessionStoreError("Missing encrypted_sessions field".into()))?;

        let blob = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| FileSessionStoreError(format!("Decode: {}", e)))?;

        if blob.len() <= SESSION_SALT_LEN + SESSION_NONCE_LEN {
            return Err(FileSessionStoreError("Blob too short".into()));
        }

        let salt = &blob[..SESSION_SALT_LEN];
        let nonce_bytes = &blob[SESSION_SALT_LEN..SESSION_SALT_LEN + SESSION_NONCE_LEN];
        let ciphertext = &blob[SESSION_SALT_LEN + SESSION_NONCE_LEN..];

        let key = Self::derive_key(master_password, salt)?;
        let cipher = ChaCha20Poly1305::new_from_slice(&key)
            .map_err(|e| FileSessionStoreError(format!("Cipher init: {}", e)))?;

        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| FileSessionStoreError("Decryption failed — wrong password?".into()))?;

        let sessions: HashMap<String, Session> = serde_json::from_slice(&plaintext)
            .map_err(|e| FileSessionStoreError(format!("Deserialize: {}", e)))?;

        Ok(sessions)
    }
}

impl Store<Did, Session> for FileSessionStore {
    type Error = FileSessionStoreError;

    async fn get(&self, key: &Did) -> Result<Option<Session>, Self::Error> {
        let cache = self.cache.lock().await;
        Ok(cache.get(key.as_str()).cloned())
    }

    async fn set(&self, key: Did, value: Session) -> Result<(), Self::Error> {
        let mut cache = self.cache.lock().await;
        cache.insert(key.as_str().to_string(), value);
        Self::flush_to_disk(&cache, &self.file_path, &self.master_password)?;
        #[cfg(debug_assertions)]
        println!("FileSessionStore: Persisted session for did:***");
        Ok(())
    }

    async fn del(&self, key: &Did) -> Result<(), Self::Error> {
        let mut cache = self.cache.lock().await;
        cache.remove(key.as_str());
        Self::flush_to_disk(&cache, &self.file_path, &self.master_password)?;
        #[cfg(debug_assertions)]
        println!("FileSessionStore: Removed session");
        Ok(())
    }

    async fn clear(&self) -> Result<(), Self::Error> {
        let mut cache = self.cache.lock().await;
        cache.clear();
        // Delete the file entirely
        let _ = std::fs::remove_file(&self.file_path);
        #[cfg(debug_assertions)]
        println!("FileSessionStore: Cleared all sessions");
        Ok(())
    }
}

impl SessionStore for FileSessionStore {}

// ─── Concrete Type Aliases ──────────────────────────────────────────────────
//
// The atrium-oauth crate is heavily generic. These aliases pin the concrete
// types for our desktop app configuration.

type OAuthHttp = DefaultHttpClient;
type OAuthDidResolver = CommonDidResolver<OAuthHttp>;
type OAuthHandleResolver = AtprotoHandleResolver<CloudflareDnsTxtResolver, OAuthHttp>;

type ConcreteOAuthClient = OAuthClient<
    MemoryStateStore,
    FileSessionStore,
    OAuthDidResolver,
    OAuthHandleResolver,
    OAuthHttp,
>;

type ConcreteOAuthSession = OAuthSession<
    OAuthHttp,
    OAuthDidResolver,
    OAuthHandleResolver,
    FileSessionStore,
>;

// ─── OAuth Manager ─────────────────────────────────────────────────────────

/// Manages the Bluesky OAuth lifecycle: authorization, callback handling,
/// and session storage. Stored in `AppState`.
pub struct OAuthManager {
    client: Arc<ConcreteOAuthClient>,
    callback_shutdown: Mutex<Option<oneshot::Sender<()>>>,
    /// The live OAuth session after successful authentication.
    /// Used for DPoP-authenticated API calls.
    /// Wrapped in Arc so callers can clone the reference and release the lock
    /// before making network calls — this enables concurrent XRPC requests.
    pub(crate) session: tokio::sync::RwLock<Option<Arc<ConcreteOAuthSession>>>,
}

impl OAuthManager {
    /// Create a new OAuthManager with persistent file-backed session storage.
    ///
    /// `data_dir` is the Tauri app data directory where `oauth_sessions.enc` will be stored.
    /// `master_password` is the same password used by `EncryptedKeychain`.
    pub fn new(data_dir: PathBuf, master_password: String) -> Result<Self, String> {
        let http_client = Arc::new(DefaultHttpClient::default());

        // Register redirect URIs for all candidate ports
        let mut redirect_uris: Vec<String> = Vec::with_capacity(1 + OAUTH_FALLBACK_PORTS.len());
        redirect_uris.push(format!("http://127.0.0.1:{}/callback", OAUTH_PREFERRED_PORT));
        for &port in OAUTH_FALLBACK_PORTS {
            redirect_uris.push(format!("http://127.0.0.1:{}/callback", port));
        }

        let session_store = FileSessionStore::new(data_dir, master_password);

        let config = OAuthClientConfig {
            client_metadata: AtprotoLocalhostClientMetadata {
                redirect_uris: Some(redirect_uris),
                scopes: Some(vec![
                    Scope::Known(KnownScope::Atproto),
                    Scope::Known(KnownScope::TransitionGeneric),
                    Scope::Known(KnownScope::TransitionChatBsky),
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
            session_store,
        };

        let client =
            OAuthClient::new(config).map_err(|e| format!("OAuth client init failed: {}", e))?;

        Ok(Self {
            client: Arc::new(client),
            callback_shutdown: Mutex::new(None),
            session: tokio::sync::RwLock::new(None),
        })
    }

    /// Restore a previously persisted OAuth session by DID.
    ///
    /// Called on app startup when the keychain indicates an OAuth session was active.
    /// The `FileSessionStore` already has the session data loaded from disk;
    /// `client.restore()` reads from it and rebuilds a live `OAuthSession`
    /// with working DPoP credentials and automatic token refresh.
    pub async fn restore_session(&self, did: &str) -> Result<(), String> {
        let did: Did = did
            .parse()
            .map_err(|_| format!("Invalid DID for session restore: {}", did))?;

        let session = self
            .client
            .restore(&did)
            .await
            .map_err(|e| format!("OAuth session restore failed: {}", e))?;

        *self.session.write().await = Some(Arc::new(session));
        #[cfg(debug_assertions)]
        println!("OAuth: Restored persisted session");
        Ok(())
    }

    /// Start the OAuth authorization flow for a given Bluesky handle.
    ///
    /// 1. Binds a callback server (preferred port, then fallbacks)
    /// 2. Calls the authorization server to get an auth URL with the correct redirect_uri
    /// 3. Returns the URL for the frontend to open in the system browser
    pub async fn start_auth(
        &self,
        handle: &str,
        app_handle: AppHandle,
    ) -> Result<String, String> {
        // Stop any previous callback server
        self.stop_callback_server().await;

        // Try binding to ports in order: preferred, then fallbacks
        let listener = Self::bind_callback_server().await?;
        let actual_port = listener
            .local_addr()
            .map_err(|e| format!("Failed to get listener address: {}", e))?
            .port();
        let redirect_uri = format!("http://127.0.0.1:{}/callback", actual_port);

        println!("OAuth: Callback server bound on port {}", actual_port);

        // Spawn the callback server with the already-bound listener
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let client = Arc::clone(&self.client);
        let app = app_handle.clone();

        tokio::spawn(async move {
            Self::run_callback_server(listener, client, shutdown_rx, app).await;
        });

        *self.callback_shutdown.lock().await = Some(shutdown_tx);

        // Get authorization URL with the actual redirect URI for the bound port
        let url = self
            .client
            .authorize(
                handle,
                AuthorizeOptions {
                    scopes: vec![
                        Scope::Known(KnownScope::Atproto),
                        Scope::Known(KnownScope::TransitionGeneric),
                        Scope::Known(KnownScope::TransitionChatBsky),
                    ],
                    state: Some(handle.to_string()),
                    redirect_uri: Some(redirect_uri),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| format!("OAuth authorization failed: {}", e))?;

        Ok(url)
    }

    /// Try binding to the preferred port, then fallback ports.
    ///
    /// Returns the first successfully bound `TcpListener`.
    async fn bind_callback_server() -> Result<tokio::net::TcpListener, String> {
        // Build candidate list: preferred port first, then fallbacks
        let mut ports = Vec::with_capacity(1 + OAUTH_FALLBACK_PORTS.len());
        ports.push(OAUTH_PREFERRED_PORT);
        ports.extend_from_slice(OAUTH_FALLBACK_PORTS);

        for &port in &ports {
            match tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await {
                Ok(listener) => return Ok(listener),
                Err(e) => {
                    eprintln!(
                        "OAuth: Port {} unavailable ({}), trying next...",
                        port, e
                    );
                }
            }
        }

        Err(format!(
            "OAuth: All callback ports unavailable ({:?}). Close the conflicting application and try again.",
            ports
        ))
    }

    /// Run the callback server using a pre-bound TCP listener.
    ///
    /// Accepts connections and dispatches each to `handle_tcp_connection`.
    /// Stops when the shutdown signal is received.
    async fn run_callback_server(
        listener: tokio::net::TcpListener,
        client: Arc<ConcreteOAuthClient>,
        mut shutdown_rx: oneshot::Receiver<()>,
        app_handle: AppHandle,
    ) {
        let addr = listener.local_addr().unwrap();
        println!("OAuth: Callback server listening on http://{}", addr);

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
    pub fn parse_query_string(qs: &str) -> OAuthCallbackQuery {
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
                *app_state.oauth_manager.session.write().await = Some(Arc::new(session));

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

                // Register Bluesky as a linked account
                let linked = crate::keychain_encrypted::LinkedAccount {
                    protocol: "bluesky".to_string(),
                    identifier: did.clone(),
                    display_handle: format!("@{}", handle),
                    display_name: None,
                    avatar_url: None,
                    connected_at: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    active: true,
                };
                let _ = app_state.keychain.add_linked_account(linked);

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
        *self.session.write().await = None;
    }

    /// Check whether an OAuth session is currently active.
    pub async fn has_session(&self) -> bool {
        self.session.read().await.is_some()
    }

    /// Get the user's PDS URL from the active OAuth session.
    ///
    /// The `OAuthSession` implements `XrpcClient` whose `base_uri()` returns
    /// the PDS endpoint that issued the DPoP tokens. All authenticated XRPC
    /// requests must target this URL, not the public appview (`bsky.social`).
    pub async fn get_pds_url(&self) -> Result<String, String> {
        use atrium_xrpc::XrpcClient;

        let session = {
            let guard = self.session.read().await;
            Arc::clone(guard.as_ref()
                .ok_or_else(|| "No OAuth session — please log in again".to_string())?)
        };

        Ok(session.base_uri())
    }

    /// Make an authenticated XRPC GET request using `send_xrpc` which properly
    /// handles DPoP Authorization headers, proof generation, and automatic
    /// token refresh on `invalid_token` responses.
    ///
    /// `nsid` is the XRPC method name, e.g. `"app.bsky.feed.getTimeline"`.
    /// `query_params` is an optional pre-serialized query string (without `?`).
    /// Returns the response body as a JSON string.
    pub async fn xrpc_get(
        &self,
        nsid: &str,
        query_params: Option<&str>,
    ) -> Result<String, String> {
        use atrium_xrpc::XrpcClient;

        // Clone Arc and drop the lock BEFORE the network call.
        // This allows concurrent XRPC requests — critical for feed parallelization.
        let session = {
            let guard = self.session.read().await;
            Arc::clone(guard.as_ref()
                .ok_or_else(|| "No OAuth session — please log in again".to_string())?)
        };

        // Build a raw XrpcRequest with no typed params/input.
        // We'll manually append query params to the nsid since the serde
        // serialization path requires a concrete Serialize type.
        let effective_nsid = if let Some(qs) = query_params {
            format!("{}?{}", nsid, qs)
        } else {
            nsid.to_string()
        };

        let request = atrium_xrpc::XrpcRequest::<(), ()> {
            method: atrium_xrpc::http::Method::GET,
            nsid: effective_nsid,
            parameters: None,
            input: None,
            encoding: None,
        };

        let result: Result<atrium_xrpc::OutputDataOrBytes<serde_json::Value>, atrium_xrpc::Error<serde_json::Value>> =
            session.send_xrpc(&request).await;

        match result {
            Ok(atrium_xrpc::OutputDataOrBytes::Data(v)) => {
                serde_json::to_string(&v).map_err(|e| format!("JSON serialize error: {}", e))
            }
            Ok(atrium_xrpc::OutputDataOrBytes::Bytes(b)) => {
                String::from_utf8(b).map_err(|e| format!("Invalid UTF-8 response: {}", e))
            }
            Err(atrium_xrpc::Error::XrpcResponse(e)) => {
                Err(format!("API returned {}: {:?}", e.status, e.error))
            }
            Err(e) => Err(format!("XRPC request failed: {}", e)),
        }
    }

    /// Make an authenticated XRPC POST request with a JSON body.
    ///
    /// Uses `send_xrpc` for proper DPoP auth and automatic token refresh.
    pub async fn xrpc_post(
        &self,
        nsid: &str,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<String, String> {
        use atrium_xrpc::XrpcClient;

        let session = {
            let guard = self.session.read().await;
            Arc::clone(guard.as_ref()
                .ok_or_else(|| "No OAuth session — please log in again".to_string())?)
        };

        let request = atrium_xrpc::XrpcRequest::<(), ()> {
            method: atrium_xrpc::http::Method::POST,
            nsid: nsid.to_string(),
            parameters: None,
            input: Some(atrium_xrpc::InputDataOrBytes::Bytes(body)),
            encoding: Some(content_type.to_string()),
        };

        let result: Result<atrium_xrpc::OutputDataOrBytes<serde_json::Value>, atrium_xrpc::Error<serde_json::Value>> =
            session.send_xrpc(&request).await;

        match result {
            Ok(atrium_xrpc::OutputDataOrBytes::Data(v)) => {
                serde_json::to_string(&v).map_err(|e| format!("JSON serialize error: {}", e))
            }
            Ok(atrium_xrpc::OutputDataOrBytes::Bytes(b)) => {
                String::from_utf8(b).map_err(|e| format!("Invalid UTF-8 response: {}", e))
            }
            Err(atrium_xrpc::Error::XrpcResponse(e)) => {
                Err(format!("API returned {}: {:?}", e.status, e.error))
            }
            Err(e) => Err(format!("XRPC request failed: {}", e)),
        }
    }

    /// Legacy authenticated_get — delegates to xrpc_get by parsing the URL.
    ///
    /// Callers pass a full URL like `https://pds.example.com/xrpc/app.bsky.feed.getTimeline?limit=50`.
    /// We extract the NSID and query string, then use `xrpc_get` for proper DPoP auth.
    pub async fn authenticated_get(&self, url: &str) -> Result<String, String> {
        let (nsid, query) = Self::parse_xrpc_url(url)?;
        self.xrpc_get(&nsid, query.as_deref()).await
    }

    /// Legacy authenticated_post — delegates to xrpc_post by parsing the URL.
    pub async fn authenticated_post(
        &self,
        url: &str,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<String, String> {
        let (nsid, _query) = Self::parse_xrpc_url(url)?;
        self.xrpc_post(&nsid, body, content_type).await
    }

    /// Parse a full XRPC URL into (nsid, optional_query_string).
    ///
    /// Input: `https://pds.example.com/xrpc/app.bsky.feed.getTimeline?limit=50`
    /// Output: `("app.bsky.feed.getTimeline", Some("limit=50"))`
    pub fn parse_xrpc_url(url: &str) -> Result<(String, Option<String>), String> {
        // Find /xrpc/ in the URL
        let xrpc_idx = url.find("/xrpc/")
            .ok_or_else(|| format!("URL is not an XRPC endpoint: {}", url))?;
        let after_xrpc = &url[xrpc_idx + 6..]; // skip "/xrpc/"

        // Split on '?' for query string
        if let Some(q_idx) = after_xrpc.find('?') {
            let nsid = after_xrpc[..q_idx].to_string();
            let query = after_xrpc[q_idx + 1..].to_string();
            Ok((nsid, Some(query)))
        } else {
            Ok((after_xrpc.to_string(), None))
        }
    }

    /// Make a DPoP-authenticated GET request routed through the Bluesky chat proxy.
    ///
    /// Adds `atproto-proxy: did:web:api.bsky.chat#bsky_chat` header so the PDS
    /// forwards the request to the chat service.
    pub async fn authenticated_chat_get(&self, url: &str) -> Result<String, String> {
        use atrium_xrpc::XrpcClient;
        use atrium_api::agent::CloneWithProxy;

        let (nsid, query) = Self::parse_xrpc_url(url)?;

        // Clone Arc, drop lock, then create chat proxy clone — no lock held during network call.
        let session = {
            let guard = self.session.read().await;
            Arc::clone(guard.as_ref()
                .ok_or_else(|| "No OAuth session — please log in again".to_string())?)
        };

        let chat_session = session.clone_with_proxy(
            "did:web:api.bsky.chat".parse().map_err(|_| "Invalid chat proxy DID")?,
            "bsky_chat",
        );

        let effective_nsid = if let Some(qs) = query {
            format!("{}?{}", nsid, qs)
        } else {
            nsid.clone()
        };

        let request = atrium_xrpc::XrpcRequest::<(), ()> {
            method: atrium_xrpc::http::Method::GET,
            nsid: effective_nsid,
            parameters: None,
            input: None,
            encoding: None,
        };

        let result: Result<atrium_xrpc::OutputDataOrBytes<serde_json::Value>, atrium_xrpc::Error<serde_json::Value>> =
            chat_session.send_xrpc(&request).await;

        match result {
            Ok(atrium_xrpc::OutputDataOrBytes::Data(v)) => {
                serde_json::to_string(&v).map_err(|e| format!("JSON serialize error: {}", e))
            }
            Ok(atrium_xrpc::OutputDataOrBytes::Bytes(b)) => {
                String::from_utf8(b).map_err(|e| format!("Invalid UTF-8 response: {}", e))
            }
            Err(atrium_xrpc::Error::XrpcResponse(e)) => {
                Err(format!("Chat API returned {}: {:?}", e.status, e.error))
            }
            Err(e) => Err(format!("XRPC chat request failed: {}", e)),
        }
    }

    /// Make a DPoP-authenticated POST request routed through the Bluesky chat proxy.
    pub async fn authenticated_chat_post(
        &self,
        url: &str,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<String, String> {
        use atrium_xrpc::XrpcClient;
        use atrium_api::agent::CloneWithProxy;
        let (nsid, _query) = Self::parse_xrpc_url(url)?;

        let session = {
            let guard = self.session.read().await;
            Arc::clone(guard.as_ref()
                .ok_or_else(|| "No OAuth session — please log in again".to_string())?)
        };

        let chat_session = session.clone_with_proxy(
            "did:web:api.bsky.chat".parse().map_err(|_| "Invalid chat proxy DID")?,
            "bsky_chat",
        );

        let request = atrium_xrpc::XrpcRequest::<(), ()> {
            method: atrium_xrpc::http::Method::POST,
            nsid: nsid.to_string(),
            parameters: None,
            input: Some(atrium_xrpc::InputDataOrBytes::Bytes(body)),
            encoding: Some(content_type.to_string()),
        };

        let result: Result<atrium_xrpc::OutputDataOrBytes<serde_json::Value>, atrium_xrpc::Error<serde_json::Value>> =
            chat_session.send_xrpc(&request).await;

        match result {
            Ok(atrium_xrpc::OutputDataOrBytes::Data(v)) => {
                serde_json::to_string(&v).map_err(|e| format!("JSON serialize error: {}", e))
            }
            Ok(atrium_xrpc::OutputDataOrBytes::Bytes(b)) => {
                String::from_utf8(b).map_err(|e| format!("Invalid UTF-8 response: {}", e))
            }
            Err(atrium_xrpc::Error::XrpcResponse(e)) => {
                Err(format!("Chat API returned {}: {:?}", e.status, e.error))
            }
            Err(e) => Err(format!("XRPC chat request failed: {}", e)),
        }
    }
}

/// Query parameters from the OAuth callback redirect.
pub struct OAuthCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub iss: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}
