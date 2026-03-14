//! Bytescape Library
//! 
//! Main library entry point for the Bytescape Tauri application.
//! This module sets up the application state and registers Tauri commands.

use tauri::{AppHandle, Manager};
use tokio::sync::Mutex;

use iroh::Endpoint;
use std::sync::Arc;

pub mod error;
pub mod db;
pub mod models;
pub mod sync;
pub mod connectors;
pub mod commands;
pub mod keychain_encrypted;
pub mod media_server;
pub mod protocol;
pub mod blob_hash;
pub mod richtext;
pub mod collab;

mod auth;
use auth::AuthManager;
mod chat;
use chat::ChatClient;
mod contacts;
pub mod oauth;
mod traffic;
use traffic::TrafficController;

/// Application-wide state shared across all commands
pub struct AppState {
    pub app_handle: AppHandle,
    pub endpoint: Mutex<Option<Endpoint>>,
    pub keychain: keychain_encrypted::EncryptedKeychain,
    pub auth_manager: AuthManager,
    pub db_manager: Arc<tokio::sync::RwLock<Option<db::DbManager>>>,
    pub chat_client: Arc<ChatClient>,
    pub sync_manager: Mutex<Option<Arc<sync::SyncManager>>>,
    pub router: Mutex<Option<iroh::protocol::Router>>,
    pub traffic_controller: Arc<TrafficController>,
    pub pending_import: Mutex<Option<commands::import::ParseResult>>,
    pub media_port: u16,
    pub oauth_manager: oauth::OAuthManager,
}

/// Sentinel value stored in keychain when using OAuth instead of app-password.
/// OAuth sessions manage their own DPoP-bound tokens via `OAuthManager::session`.
pub const OAUTH_SESSION_SENTINEL: &str = "oauth_session";

impl AppState {
    /// Check whether the current session was established via OAuth.
    ///
    /// Returns `true` if the keychain contains the OAuth sentinel value
    /// instead of a real app-password JWT.
    pub fn is_oauth_session(&self) -> bool {
        self.keychain
            .get_access_jwt()
            .ok()
            .flatten()
            .map(|jwt| jwt == OAUTH_SESSION_SENTINEL)
            .unwrap_or(false)
    }

    /// Get a valid (non-expired) access JWT, automatically refreshing if needed.
    ///
    /// For **app-password** sessions: checks the `exp` claim in the current JWT.
    /// If expired or within 60 seconds of expiring, uses the refresh token to
    /// obtain a new session and persists it.
    ///
    /// For **OAuth** sessions: returns an error because OAuth uses DPoP-bound
    /// tokens that cannot be used as raw Bearer tokens. Callers that need to
    /// make authenticated Bluesky API calls during an OAuth session should use
    /// `oauth_manager.session` with atrium's Agent instead.
    pub async fn get_fresh_access_jwt(&self) -> Result<String, String> {
        let access_jwt = self.keychain.get_access_jwt()
            .map_err(|e| format!("Keychain error: {}", e))?
            .ok_or_else(|| "No access token available".to_string())?;

        // OAuth sessions use DPoP-bound tokens managed by OAuthManager.
        // Raw JWT Bearer tokens are not available — API calls must go through
        // the OAuthSession's HTTP client for proper DPoP header generation.
        if access_jwt == OAUTH_SESSION_SENTINEL {
            return Err(
                "This feature requires an App Password login. \
                 OAuth sessions use DPoP-bound tokens that cannot be used as raw Bearer tokens. \
                 Please log out and sign in with an App Password, or wait for full OAuth API support."
                    .to_string(),
            );
        }

        if !Self::is_jwt_expired(&access_jwt, 60) {
            return Ok(access_jwt);
        }

        println!("Auth: Access token expired or expiring soon, refreshing...");

        let refresh_jwt = self.keychain.get_refresh_jwt()
            .map_err(|e| format!("Keychain error: {}", e))?
            .ok_or_else(|| "No refresh token available — please log in again".to_string())?;

        let new_session = self.auth_manager.refresh_session(refresh_jwt).await
            .map_err(|e| format!("Session refresh failed: {}. Please log in again.", e))?;

        self.keychain.save_session(
            new_session.did,
            new_session.handle,
            new_session.access_jwt.clone(),
            new_session.refresh_jwt,
        ).map_err(|e| format!("Failed to save refreshed session: {}", e))?;

        println!("Auth: Session refreshed successfully");
        Ok(new_session.access_jwt)
    }

    // ─── Unified XRPC Helpers (OAuth + App-Password) ───────────────────────

    /// Get the user's DID from the keychain. Works for both auth modes.
    pub fn get_user_did(&self) -> Result<String, String> {
        let (did, _) = self.keychain.load_session()
            .map_err(|e| format!("Keychain error: {}", e))?
            .ok_or("Not authenticated")?;
        Ok(did)
    }

    /// Get the PDS base URL for XRPC calls.
    ///
    /// Returns a URL ending in `/xrpc` for both auth modes so callers can
    /// simply append `/{nsid}` to construct the full endpoint URL.
    ///
    /// - **App-password**: Extracts PDS from JWT `iss` claim via `authenticated_xrpc_candidates`
    ///   (these already include `/xrpc`).
    /// - **OAuth**: Gets raw PDS host from `OAuthSession::base_uri()` and appends `/xrpc`.
    pub async fn get_pds_base(&self) -> Result<String, String> {
        if self.is_oauth_session() {
            let base = self.oauth_manager.get_pds_url().await?;
            // OAuthSession::base_uri() returns the raw PDS host (no /xrpc),
            // but xrpc_get/xrpc_post_json expect the base to include /xrpc.
            Ok(format!("{}/xrpc", base.trim_end_matches('/')))
        } else {
            let jwt = self.get_fresh_access_jwt().await?;
            let candidates = connectors::bluesky::authenticated_xrpc_candidates(&jwt, true);
            candidates.into_iter().next().ok_or_else(|| "Could not resolve PDS URL".to_string())
        }
    }

    /// Make an authenticated JSON POST request (e.g. createRecord, deleteRecord).
    ///
    /// Dispatches to Bearer auth (app-password) or DPoP auth (OAuth) transparently.
    pub async fn xrpc_post_json(
        &self,
        endpoint: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let pds = self.get_pds_base().await?;
        let url = format!("{}/{}", pds.trim_end_matches('/'), endpoint);

        if self.is_oauth_session() {
            let json_bytes = serde_json::to_vec(&body)
                .map_err(|e| format!("JSON serialize failed: {}", e))?;
            let resp_str = self.oauth_manager
                .authenticated_post(&url, json_bytes, "application/json")
                .await?;
            serde_json::from_str(&resp_str)
                .map_err(|e| format!("Failed to parse response: {}", e))
        } else {
            let jwt = self.get_fresh_access_jwt().await?;
            let client = connectors::bluesky::BlueskyClient::with_jwt(&jwt);
            let resp = client.create_record(&url, body).await?;
            if !resp.status().is_success() {
                let status = resp.status();
                let err_body = resp.text().await.unwrap_or_default();
                return Err(format!("XRPC POST {} failed ({}): {}", endpoint, status, err_body));
            }
            resp.json::<serde_json::Value>().await
                .map_err(|e| format!("Failed to parse response: {}", e))
        }
    }

    /// Upload a binary blob (image/video) to the PDS.
    ///
    /// Returns the parsed JSON response containing the blob reference.
    pub async fn xrpc_upload_blob(
        &self,
        data: Vec<u8>,
        mime: &str,
    ) -> Result<serde_json::Value, String> {
        let pds = self.get_pds_base().await?;
        let url = format!("{}/com.atproto.repo.uploadBlob", pds.trim_end_matches('/'));

        if self.is_oauth_session() {
            let resp_str = self.oauth_manager
                .authenticated_post(&url, data, mime)
                .await?;
            serde_json::from_str(&resp_str)
                .map_err(|e| format!("Failed to parse upload response: {}", e))
        } else {
            let jwt = self.get_fresh_access_jwt().await?;
            let client = connectors::bluesky::BlueskyClient::with_jwt(&jwt);
            let resp = client.post_blob(&url, mime, data).await?;
            if !resp.status().is_success() {
                let status = resp.status();
                let err_body = resp.text().await.unwrap_or_default();
                return Err(format!("Blob upload failed ({}): {}", status, err_body));
            }
            resp.json::<serde_json::Value>().await
                .map_err(|e| format!("Failed to parse upload response: {}", e))
        }
    }

    /// Make an authenticated GET request.
    ///
    /// Returns the parsed JSON response.
    pub async fn xrpc_get(&self, endpoint: &str) -> Result<serde_json::Value, String> {
        let pds = self.get_pds_base().await?;
        let url = format!("{}/{}", pds.trim_end_matches('/'), endpoint);

        if self.is_oauth_session() {
            let resp_str = self.oauth_manager.authenticated_get(&url).await?;
            serde_json::from_str(&resp_str)
                .map_err(|e| format!("Failed to parse response: {}", e))
        } else {
            let jwt = self.get_fresh_access_jwt().await?;
            let client = connectors::bluesky::BlueskyClient::with_jwt(&jwt);
            let resp = client.get_resource(&url).await?;
            if !resp.status().is_success() {
                let status = resp.status();
                let err_body = resp.text().await.unwrap_or_default();
                return Err(format!("XRPC GET {} failed ({}): {}", endpoint, status, err_body));
            }
            resp.json::<serde_json::Value>().await
                .map_err(|e| format!("Failed to parse response: {}", e))
        }
    }

    /// Make an authenticated GET request routed through the Bluesky chat proxy.
    ///
    /// Adds `atproto-proxy: did:web:api.bsky.chat#bsky_chat` header for chat API calls.
    pub async fn xrpc_chat_get(&self, endpoint: &str) -> Result<serde_json::Value, String> {
        let pds = self.get_pds_base().await?;
        let url = format!("{}/{}", pds.trim_end_matches('/'), endpoint);

        if self.is_oauth_session() {
            let resp_str = self.oauth_manager.authenticated_chat_get(&url).await?;
            serde_json::from_str(&resp_str)
                .map_err(|e| format!("Failed to parse chat response: {}", e))
        } else {
            let jwt = self.get_fresh_access_jwt().await?;
            let resp = reqwest::Client::new()
                .get(&url)
                .header("Authorization", format!("Bearer {}", jwt))
                .header("atproto-proxy", "did:web:api.bsky.chat#bsky_chat")
                .send()
                .await
                .map_err(|e| format!("Chat GET request failed: {}", e))?;
            if !resp.status().is_success() {
                let status = resp.status();
                let err_body = resp.text().await.unwrap_or_default();
                return Err(format!("Chat GET {} failed ({}): {}", endpoint, status, err_body));
            }
            resp.json::<serde_json::Value>().await
                .map_err(|e| format!("Failed to parse chat response: {}", e))
        }
    }

    /// Make an authenticated POST request routed through the Bluesky chat proxy.
    pub async fn xrpc_chat_post(&self, endpoint: &str, body: serde_json::Value) -> Result<serde_json::Value, String> {
        let pds = self.get_pds_base().await?;
        let url = format!("{}/{}", pds.trim_end_matches('/'), endpoint);

        if self.is_oauth_session() {
            let json_bytes = serde_json::to_vec(&body)
                .map_err(|e| format!("JSON serialize failed: {}", e))?;
            let resp_str = self.oauth_manager
                .authenticated_chat_post(&url, json_bytes, "application/json")
                .await?;
            serde_json::from_str(&resp_str)
                .map_err(|e| format!("Failed to parse chat response: {}", e))
        } else {
            let jwt = self.get_fresh_access_jwt().await?;
            let resp = reqwest::Client::new()
                .post(&url)
                .header("Authorization", format!("Bearer {}", jwt))
                .header("atproto-proxy", "did:web:api.bsky.chat#bsky_chat")
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("Chat POST request failed: {}", e))?;
            if !resp.status().is_success() {
                let status = resp.status();
                let err_body = resp.text().await.unwrap_or_default();
                return Err(format!("Chat POST {} failed ({}): {}", endpoint, status, err_body));
            }
            resp.json::<serde_json::Value>().await
                .map_err(|e| format!("Failed to parse chat response: {}", e))
        }
    }

    /// Check whether a JWT's `exp` claim has passed (or is within `margin_secs`).
    fn is_jwt_expired(token: &str, margin_secs: u64) -> bool {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;

        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() < 2 {
            return true; // malformed → treat as expired
        }

        let payload_bytes = match URL_SAFE_NO_PAD.decode(parts[1]) {
            Ok(b) => b,
            Err(_) => return true,
        };

        let payload: serde_json::Value = match serde_json::from_slice(&payload_bytes) {
            Ok(v) => v,
            Err(_) => return true,
        };

        let exp = match payload.get("exp").and_then(|v| v.as_u64()) {
            Some(e) => e,
            None => return true, // no exp claim → treat as expired
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        now + margin_secs >= exp
    }
}

/// Run the Bytescape application
/// 
/// This is the main entry point called from main.rs
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    println!("Starting Bytescape...");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            // Node & P2P commands
            commands::node::setup_iroh_node,
            commands::node::create_sync_doc,
            commands::node::join_sync_doc,
            commands::node::get_node_status,
            commands::node::p2p_create_test_doc,
            commands::node::p2p_join_and_read_test_doc,
            
            // Vault commands
            commands::vault::vault_upload_file,
            commands::vault::vault_upload_files_from_paths,
            commands::vault::vault_list_files,
            commands::vault::vault_get_file,
            commands::vault::vault_delete_file,
            commands::vault::vault_get_stats,
            commands::vault::vault_deduplicate_files,
            commands::vault::vault_get_file_data,
            commands::vault::vault_get_thumbnail,
            commands::vault::send_p2p_message,
            commands::vault::get_vault_stats,
            commands::vault::list_vault_sources,
            commands::vault::list_vault_posts_paged,
            commands::vault::save_hashtag,
            commands::vault::delete_hashtag,
            commands::vault::list_followed_hashtags,
            commands::vault::list_hashtags,
            commands::vault::clear_vault,
            commands::vault::list_vault_messages,
            commands::vault::count_archive_posts,
            commands::vault::list_contacts,
            commands::vault::vault_gc_tombstones,
            
            // Auth commands
            commands::auth::check_auth_status,
            commands::auth::login,
            commands::auth::logout,
            commands::auth::start_bluesky_oauth,
            commands::auth::cancel_bluesky_oauth,
            commands::auth::create_local_identity,
            commands::auth::set_local_username,
            commands::auth::identity_init_full_recovery,
            commands::auth::identity_get_recovery_status,
            commands::auth::identity_start_recovery,
            commands::auth::identity_verify_recovery,
            commands::auth::identity_finalize_recovery,
            
            // Import commands
            commands::import::import_facebook_zip,
            commands::import::import_instagram_zip,
            commands::import::import_archive_auto,
            commands::import::debug_zip_contents,
            commands::import::commit_memories,
            commands::import::import_bluesky_posts,
            commands::import::migrate_willow_paths,
            commands::import::get_willow_write_parity_report,
            commands::import::get_timeline,
            commands::feed_aggregation::get_timeline_unified,
            commands::import::import_vault_messages,
            commands::import::import_archive_messages,
            commands::import::send_bsky_dm,
            commands::import::list_saved_feeds,
            commands::import::list_follows,
            commands::import::list_followers,
            commands::import::get_unified_feed,
            commands::import::publish_post,
            
            // Media commands
            commands::media::store_media_blob,
            commands::media::get_media_blob,
            commands::media::get_media_blob_bytes,
            commands::media::download_peer_blob,
            commands::media::resolve_hls_video,
            commands::media::get_media_dir,
            commands::media::diagnose_media_ref,
            commands::media::backfill_media_mimes,
            commands::media::get_media_server_port,
            commands::media::resolve_media_url,
            commands::syndicate::syndicate_vault_item,
            commands::syndicate::verify_syndications,
            commands::posting::publish_post_v2,
            commands::posting::search_mentions,
            commands::posting::search_hashtags_autocomplete,
            commands::interactions::like_post,
            commands::interactions::unlike_post,
            commands::interactions::repost_post,
            commands::interactions::unrepost_post,
            commands::interactions::reply_to_post,
            commands::interactions::get_post_thread,
            
            // Peer commands
            commands::peers::get_my_peer_info,
            commands::peers::add_peer,
            commands::peers::list_peers,
            commands::peers::remove_peer,
            commands::peers::check_peer_connectivity,
            commands::peers::list_p2p_conversations,
            commands::peers::send_p2p_chat_message,
            commands::peers::request_peer_feed,
            
            // Collab commands (C6.1)
            commands::collab::create_collab_doc,
            commands::collab::apply_collab_update,
            commands::collab::get_collab_state,
            commands::collab::snapshot_collab_to_item,
            commands::collab::list_collab_docs,
            commands::collab::delete_collab_doc,
        ])
        .setup(|app| {
            // Initialize application state
            let app_handle = app.handle().clone();

            // Create app data directory
            let app_data_dir = app_handle.path().app_data_dir()
                .map_err(|e| format!("Failed to get app data dir: {}", e))?;
            std::fs::create_dir_all(&app_data_dir)
                .map_err(|e| format!("Failed to create app data dir: {}", e))?;

            // Start dedicated native media server on localhost to bypass WebKitGTK protocol bugs
            let media_port = tauri::async_runtime::block_on(async {
                media_server::start_warp_media_server(app_data_dir.clone()).await
            });

            // Initialize encrypted keychain
            // Note: In production, the master password should come from:
            // 1. User input at startup (most secure)
            // 2. OS keychain (convenient, still secure)
            // 3. Environment variable (for testing only)
            let keychain_path = app_data_dir.join("keychain.json");
            let master_password = std::env::var("BYTESCAPE_MASTER_PASSWORD")
                .unwrap_or_else(|_| "default_dev_password_CHANGE_IN_PRODUCTION".to_string());
            let keychain = keychain_encrypted::EncryptedKeychain::new(
                keychain_path,
                master_password.clone(),
            );

            // Initialize database
            let db_path = app_data_dir.join("bytescape.redb");
            let db_manager = Arc::new(tokio::sync::RwLock::new(
                Some(db::DbManager::new(db_path)
                    .map_err(|e| format!("Failed to initialize database: {}", e))?)
            ));

            // Initialize auth manager
            let auth_manager = AuthManager::new();

            // Initialize OAuth manager for Bluesky OAuth login
            let oauth_manager = oauth::OAuthManager::new(
                app_data_dir.clone(),
                master_password.clone(),
            )
                .map_err(|e| format!("OAuth manager init failed: {}", e))?;

            // Initialize chat client
            let chat_client = Arc::new(ChatClient::new());

            // Initialize traffic controller
            let traffic_controller = Arc::new(TrafficController::new(
                db_manager.clone(),
                chat_client.clone(),
            ));

            // Set up app state
            let state = AppState {
                app_handle: app_handle.clone(),
                endpoint: Mutex::new(None),
                keychain,
                auth_manager,
                db_manager,
                chat_client,
                sync_manager: Mutex::new(None),
                router: Mutex::new(None),
                traffic_controller,
                pending_import: Mutex::new(None),
                media_port,
                oauth_manager,
            };

            app.manage(state);

            // Background: restore OAuth session if one was active before the app closed
            let restore_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let state = restore_handle.state::<AppState>();
                if state.is_oauth_session() {
                    match state.get_user_did() {
                        Ok(did) => {
                            println!("Startup: Restoring OAuth session for {}...", did);
                            match state.oauth_manager.restore_session(&did).await {
                                Ok(()) => println!("Startup: OAuth session restored successfully"),
                                Err(e) => {
                                    eprintln!("Startup: OAuth session restore failed: {}", e);
                                    eprintln!("Startup: User will need to re-authenticate");
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Startup: Cannot restore OAuth — no DID in keychain: {}", e);
                        }
                    }
                }
            });

            // Background: backfill media_mimes for existing items
            let backfill_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Short delay to let the rest of setup complete
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                let state = backfill_handle.state::<AppState>();

                // Clone DbManager and SyncManager out of the locks so we don't
                // hold them during the (potentially long) backfill loop.
                let db = {
                    let guard = state.db_manager.read().await;
                    match guard.as_ref() {
                        Some(d) => d.clone(),
                        None => return,
                    }
                };
                let sync = {
                    let guard = state.sync_manager.lock().await;
                    match guard.as_ref() {
                        Some(s) => s.clone(),
                        None => return,
                    }
                };

                let item_headers = match db.list_all_item_headers().await {
                    Ok(headers) => headers,
                    Err(_) => return,
                };
                let mut updated = 0usize;
                for h in item_headers {
                    // Lightweight check before full fetch
                    if h.media_blob_count == 0 { continue; }
                    if h.media_blob_count == h.media_mimes_count { continue; }

                    // Fetch full item only if it needs backfill
                    let mut item = match db.get_item_by_id(&h.id).await {
                        Ok(Some(item)) => item,
                        _ => continue,
                    };

                    let mut mimes = Vec::with_capacity(item.media_blobs.len());
                    for blob_ref in &item.media_blobs {
                        let hash = match crate::blob_hash::parse_blob_hash(blob_ref) {
                            Ok(h) => h,
                            Err(_) => {
                                mimes.push("application/octet-stream".to_string());
                                continue;
                            }
                        };
                        match sync.get_media(hash).await {
                            Ok(data) => {
                                mimes.push(crate::commands::media::mime_from_magic_bytes(&data));
                            }
                            Err(_) => mimes.push("application/octet-stream".to_string()),
                        }
                    }
                    item.media_mimes = mimes;
                    if db.upsert_item(&item).await.is_ok() {
                        updated += 1;
                    }

                    // Yield between items to avoid starving other tasks
                    tokio::task::yield_now().await;
                }
                if updated > 0 {
                    println!("backfill_media_mimes: updated {} items on startup", updated);
                }
            });

            // Background: clean up mis-imported syndicated duplicates
            let cleanup_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Short delay to let the rest of setup complete
                tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                let state = cleanup_handle.state::<AppState>();
                // Clone DbManager out of the lock so cleanup doesn't hold the outer lock
                let db = {
                    let guard = state.db_manager.read().await;
                    match guard.as_ref() {
                        Some(d) => d.clone(),
                        None => return,
                    }
                };
                if let Ok(removed_count) = db.cleanup_syndication_duplicates().await {
                    if removed_count > 0 {
                        println!("cleanup: Removed {} duplicate syndicated items from vault.", removed_count);
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running bytescape");
}

// Re-export commonly used types
pub use error::{BytescapeError, BytescapeResult};
pub use db::{DbManager, BytescapeItem, Message, Contact, Conversation};
pub use sync::SyncManager;
