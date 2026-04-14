//! Bytescape Library
//! 
//! Main library entry point for the Bytescape Tauri application.
//! This module sets up the application state and registers Tauri commands.

use tauri::{AppHandle, Manager};
use tokio::sync::Mutex;

use iroh::Endpoint;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

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
pub mod activitypub;
pub mod security;

mod auth;
use auth::AuthManager;
mod chat;
use chat::ChatClient;
mod contacts;
pub mod oauth;
mod traffic;
use traffic::TrafficController;

/// Cached API response with TTL.
#[derive(Clone)]
pub struct CachedResponse<T: Clone> {
    pub data: T,
    pub fetched_at: std::time::Instant,
}

impl<T: Clone> CachedResponse<T> {
    pub fn new(data: T) -> Self {
        Self { data, fetched_at: std::time::Instant::now() }
    }
    pub fn is_fresh(&self, max_age: std::time::Duration) -> bool {
        self.fetched_at.elapsed() < max_age
    }
}

/// In-memory cache for feed API responses.
/// Keyed by cache slot name, holds serialized JSON to avoid type-parameter sprawl.
pub struct FeedCache {
    entries: tokio::sync::RwLock<std::collections::HashMap<String, CachedResponse<serde_json::Value>>>,
}

impl FeedCache {
    pub fn new() -> Self {
        Self { entries: tokio::sync::RwLock::new(std::collections::HashMap::new()) }
    }

    /// Get a cached JSON value if it exists and is younger than `max_age`.
    pub async fn get(&self, key: &str, max_age: std::time::Duration) -> Option<serde_json::Value> {
        let map = self.entries.read().await;
        map.get(key).filter(|c| c.is_fresh(max_age)).map(|c| c.data.clone())
    }

    /// Store a JSON value in the cache.
    pub async fn set(&self, key: &str, value: serde_json::Value) {
        let mut map = self.entries.write().await;
        map.insert(key.to_string(), CachedResponse::new(value));
    }

    /// Invalidate a specific key.
    #[allow(dead_code)]
    pub async fn invalidate(&self, key: &str) {
        let mut map = self.entries.write().await;
        map.remove(key);
    }

    /// Flush the entire cache (e.g. after unlinking an account).
    pub async fn clear_all(&self) {
        let mut map = self.entries.write().await;
        map.clear();
    }
}

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
    /// Shared HTTP client for connection pooling across all API calls.
    /// Reusing a single client avoids redundant TLS handshakes per request.
    pub http_client: reqwest::Client,
    /// Guard to ensure `restore_session` runs at most once per app lifetime.
    pub session_restored: AtomicBool,
    /// In-memory cache for feed API responses (timeline, saved feeds, etc.).
    /// TTL-based: entries expire after 30s to balance freshness vs speed.
    pub feed_cache: FeedCache,
    /// Active FFmpeg processes tracked by URL hash for cancellation support.
    /// Frontend can call `cancel_video_resolve` when user scrolls past a video.
    pub active_ffmpeg: tokio::sync::Mutex<std::collections::HashMap<String, u32>>,
    /// Prevents concurrent token refreshes that would race against each other.
    /// The Bluesky PDS invalidates the old refresh token on use, so if two
    /// requests both see an expired access token and both try to refresh, the
    /// second one will fail with an invalid token error.
    pub session_refresh_lock: tokio::sync::Mutex<()>,
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

        // Acquire the refresh lock to prevent concurrent refresh races.
        // The PDS invalidates the old refresh token on use, so only one
        // concurrent refresh is allowed.
        let _guard = self.session_refresh_lock.lock().await;

        // Re-check after acquiring lock — another task may have already refreshed
        let access_jwt = self.keychain.get_access_jwt()
            .map_err(|e| format!("Keychain error: {}", e))?
            .ok_or_else(|| "No access token available".to_string())?;
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
            commands::vault::vault_get_item,
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
            commands::vault::add_followed_hashtag,
            commands::vault::remove_followed_hashtag,
            commands::vault::clear_vault,
            commands::vault::list_vault_messages,
            commands::vault::count_archive_posts,
            commands::vault::list_contacts,
            commands::vault::list_contacts_unified,
            commands::vault::vault_gc_tombstones,
            commands::vault::delete_vault_item,
            commands::vault::resolve_reply_parent,
            
            // Auth commands
            commands::auth::check_auth_status,
            commands::auth::login,
            commands::auth::logout,
            commands::auth::delete_account,
            commands::auth::start_bluesky_oauth,
            commands::auth::cancel_bluesky_oauth,
            commands::auth::create_local_identity,
            commands::auth::set_local_username,
            commands::auth::identity_init_full_recovery,
            commands::auth::identity_get_recovery_status,
            commands::auth::identity_start_recovery,
            commands::auth::identity_verify_recovery,
            commands::auth::identity_finalize_recovery,
            commands::auth::list_linked_accounts,
            commands::auth::unlink_account,
            
            // Import commands
            commands::import::import_facebook_zip,
            commands::import::import_instagram_zip,
            commands::import::import_archive_auto,
            commands::import::debug_zip_contents,
            commands::import::commit_memories,
            commands::import::import_bluesky_posts,
            commands::import::import_mastodon_posts,
            commands::import::migrate_willow_paths,
            commands::import::get_timeline,
            commands::import::get_author_feed,
            commands::feed_aggregation::get_timeline_unified,
            commands::feed_aggregation::get_linked_feed_sources,
            commands::import::import_vault_messages,
            commands::import::import_archive_messages,
            commands::import::send_bsky_dm,
            commands::import::get_or_create_bsky_convo,
            commands::import::list_saved_feeds,
            commands::import::list_follows,
            commands::import::list_followers,
            commands::import::bluesky_follow,
            commands::import::bluesky_unfollow,
            commands::import::get_unified_feed,
            commands::import::publish_post,
            
            // Media commands
            commands::media::store_media_blob,
            commands::media::get_media_blob,
            commands::media::get_media_blob_bytes,
            commands::media::download_peer_blob,
            commands::media::resolve_hls_video,
            commands::media::cancel_video_resolve,
            commands::media::get_media_dir,
            commands::media::diagnose_media_ref,
            commands::media::backfill_media_mimes,
            commands::media::get_media_server_port,
            commands::media::resolve_media_url,
            commands::media::resolve_blob_urls_batch,
            commands::syndicate::syndicate_vault_item,
            commands::syndicate::verify_syndications,
            commands::posting::publish_post_v2,
            commands::posting::search_mentions,
            commands::posting::search_hashtags_autocomplete,
            commands::interactions::like_post,
            commands::interactions::unlike_post,
            commands::interactions::repost_post,
            commands::interactions::unrepost_post,
            commands::interactions::quote_post_to_bluesky,
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
            // Profile layout commands
            commands::profile::save_profile_layout,
            commands::profile::get_profile_layout,
            commands::profile::delete_profile_layout,
            // Widget & guestbook commands (B2.4)
            commands::profile::save_widget_data,
            commands::profile::get_widget_data,
            commands::profile::add_guestbook_entry,
            commands::profile::list_guestbook_entries,
            commands::profile::delete_guestbook_entry,
            // Profile section commands (B2.4 — editable blocks)
            commands::profile::save_profile_sections,
            commands::profile::get_profile_sections,
            // Profile settings & pages commands (v18a — profile polish)
            commands::profile::save_profile_settings,
            commands::profile::get_profile_settings,
            commands::profile::save_profile_page,
            commands::profile::get_profile_pages,
            commands::profile::delete_profile_page,
            // Avatar & profile export (v19)
            commands::profile::save_avatar_image,
            commands::profile::export_profile_data,
            // Link unfurling (C2.1)
            commands::unfurl::unfurl_link,
            // Classification commands
            commands::classify::reclassify_vault_items,
            // Export commands
            commands::export::export_vault_data,
            
            // Browser tab commands (B10)
            commands::browser::browser_create,
            commands::browser::browser_close,
            commands::browser::browser_set_bounds,
            commands::browser::browser_hide,
            commands::browser::browser_show,
            commands::browser::browser_show_at,
            commands::browser::browser_navigate,
            commands::browser::browser_eval,
            commands::browser::browser_get_url,
            commands::browser::browser_clear_data,
            
            // ActivityPub commands (A1.1)
            commands::ap::create_ap_actor,
            commands::ap::get_ap_actor,
            commands::ap::send_ap_activity,
            commands::ap::fetch_remote_actor,
            commands::ap::resolve_webfinger,
            
            // Mastodon & AP commands (A1.2)
            commands::mastodon::mastodon_register_app,
            commands::mastodon::mastodon_complete_auth,
            commands::mastodon::mastodon_get_session,
            commands::mastodon::mastodon_disconnect,
            commands::mastodon::mastodon_get_timeline,
            commands::mastodon::send_mastodon_dm,
            commands::mastodon::send_ap_dm,
            commands::mastodon::send_message,
            commands::mastodon::mastodon_follow,
            commands::mastodon::mastodon_unfollow,
            commands::mastodon::mastodon_follow_hashtag,
            commands::mastodon::mastodon_unfollow_hashtag,
            commands::mastodon::mastodon_get_followed_tags,
            commands::mastodon::mastodon_get_hashtag_timeline,
            commands::mastodon::mastodon_get_following,
            commands::mastodon::mastodon_get_followers,
            commands::mastodon::mastodon_favourite,
            commands::mastodon::mastodon_unfavourite,
            commands::mastodon::mastodon_reblog,
            commands::mastodon::mastodon_unreblog,
            commands::mastodon::mastodon_reply,
            commands::mastodon::send_ap_follow,
            commands::mastodon::send_ap_unfollow,
            commands::mastodon::resolve_profile,

            // Relay & server provisioning stubs (Phase 4.5)
            commands::relay::relay_connect,
            commands::relay::relay_push_blob,
            commands::relay::relay_pull_pending,
            commands::relay::relay_register_push_token,
            commands::relay::relay_sync_status,
            commands::relay::provision_pds_account,
            commands::relay::provision_ap_actor,

            // Groups & Topics commands (Sprint v12)
            commands::groups::create_topic,
            commands::groups::update_topic,
            commands::groups::delete_topic_cmd,
            commands::groups::get_topic_tree,
            commands::groups::subscribe_topic,
            commands::groups::unsubscribe_topic,
            commands::groups::get_subscribed_topics,
            commands::groups::create_group,
            commands::groups::update_group,
            commands::groups::delete_group_cmd,
            commands::groups::get_group_cmd,
            commands::groups::list_groups_by_topic,
            commands::groups::search_groups_cmd,
            commands::groups::join_group,
            commands::groups::leave_group,
            commands::groups::get_group_members_cmd,
            commands::groups::get_user_group_memberships,
            commands::groups::post_to_group,
            commands::groups::share_to_group,
            commands::groups::get_group_feed,
            commands::groups::get_groups_feed,

            // Group moderation commands (Sprint v14)
            commands::groups::approve_member,
            commands::groups::reject_member,
            commands::groups::invite_member,
            commands::groups::accept_invite,
            commands::groups::ban_member,
            commands::groups::set_member_role,
            commands::groups::pin_group_post,
            commands::groups::unpin_group_post,
            commands::groups::remove_group_post,
            commands::groups::get_pending_members,
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
            //
            // Priority order for master password:
            // 1. BYTESCAPE_MASTER_PASSWORD env var (explicit config)
            // 2. Persisted random key from a prior run (stored in data dir)
            // 3. Freshly generated random 32-byte key (persisted for next run)
            //
            // Option 3 means first launch generates a machine-local key that
            // survives restarts but is never hardcoded in source code.
            let keychain_path = app_data_dir.join("keychain.json");
            let master_key_file = app_data_dir.join(".master_key");
            let master_password = if let Ok(pw) = std::env::var("BYTESCAPE_MASTER_PASSWORD") {
                pw
            } else if let Ok(existing) = std::fs::read_to_string(&master_key_file) {
                existing.trim().to_string()
            } else {
                // Generate a cryptographically random 32-byte key and persist it
                use rand::RngCore;
                use base64::Engine;
                let mut key_bytes = [0u8; 32];
                rand::rng().fill_bytes(&mut key_bytes);
                let key_str = base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(key_bytes);
                // Persist the key so sessions survive app restarts
                if let Err(e) = std::fs::write(&master_key_file, &key_str) {
                    eprintln!("WARN: Could not persist master key to {:?}: {}. \
                              Sessions will not survive restarts.", master_key_file, e);
                }
                // Restrict file permissions on Unix
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(
                        &master_key_file,
                        std::fs::Permissions::from_mode(0o600),
                    );
                }
                println!("Startup: Generated new master key (persisted to app data dir)");
                key_str
            };
            let keychain = keychain_encrypted::EncryptedKeychain::new(
                keychain_path,
                master_password.clone(),
            );

            // Run identity migrations before any identity operations
            match keychain.migrate_fabricated_plc_identifiers() {
                Ok(0) => {},
                Ok(n) => println!("Startup: migrated {} linked account(s) from fabricated did:plc to native identifiers", n),
                Err(e) => eprintln!("Startup: identity migration failed (non-fatal): {}", e),
            }

            // Initialize database
            let db_path = app_data_dir.join("bytescape.redb");
            let db_manager = Arc::new(tokio::sync::RwLock::new(
                Some(db::DbManager::new(db_path)
                    .map_err(|e| format!("Failed to initialize database: {}", e))?)
            ));

            // Seed default topic tree on first launch (deferred — non-blocking)
            // P0.4 fix: Moved from blocking `block_on` to async spawn so it
            // doesn't delay app window appearance or contribute to CPU spike.
            let topic_db = db_manager.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                let db_guard = topic_db.read().await;
                if let Some(ref db) = *db_guard {
                    if let Err(e) = commands::groups::seed_default_topics(db).await {
                        eprintln!("Startup: topic seeding failed (non-fatal): {}", e);
                    }
                }
            });

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
                http_client: reqwest::Client::builder()
                    .pool_max_idle_per_host(4)
                    .pool_idle_timeout(std::time::Duration::from_secs(90))
                    .timeout(std::time::Duration::from_secs(30))
                    .build()
                    .unwrap_or_else(|_| reqwest::Client::new()),
                session_restored: AtomicBool::new(false),
                feed_cache: FeedCache::new(),
                active_ffmpeg: tokio::sync::Mutex::new(std::collections::HashMap::new()),
                session_refresh_lock: tokio::sync::Mutex::new(()),
            };

            app.manage(state);

            // Background: restore OAuth session if one was active before the app closed
            let restore_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let state = restore_handle.state::<AppState>();
                if state.is_oauth_session() {
                    // Guard: only restore once per app lifetime
                    if state.session_restored.swap(true, Ordering::SeqCst) {
                        return; // already restored by check_auth_status
                    }
                    match state.get_user_did() {
                        Ok(did) => {
                            match state.oauth_manager.restore_session(&did).await {
                                Ok(()) => println!("Startup: OAuth session restored successfully"),
                                Err(e) => {
                                    eprintln!("Startup: OAuth session restore failed: {}", e);
                                    eprintln!("Startup: User will need to re-authenticate");
                                    // Reset guard so check_auth_status can retry
                                    state.session_restored.store(false, Ordering::SeqCst);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Startup: Cannot restore OAuth — no DID in keychain: {}", e);
                            state.session_restored.store(false, Ordering::SeqCst);
                        }
                    }
                }
            });

            // Background: backfill media_mimes for existing items
            // P0.2 fix: only run if last backfill was >24h ago to avoid CPU spike on every launch
            let backfill_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Short delay to let the rest of setup complete
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                let state = backfill_handle.state::<AppState>();

                // Check timestamp guard — skip if backfill ran within the last 24 hours
                let app_data_dir = match state.app_handle.path().app_data_dir() {
                    Ok(d) => d,
                    Err(_) => return,
                };
                let marker_path = app_data_dir.join("blob_cache").join(".last_backfill");
                if let Ok(meta) = std::fs::metadata(&marker_path) {
                    if let Ok(modified) = meta.modified() {
                        if modified.elapsed().unwrap_or_default() < std::time::Duration::from_secs(24 * 3600) {
                            println!("backfill_media_mimes: skipped — last run was less than 24h ago");
                            return;
                        }
                    }
                }

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

                    // P0.1 fix: try path-hint based MIME detection first to avoid
                    // fetching entire blobs from Iroh just for magic-byte sniffing.
                    let mut mimes = Vec::with_capacity(item.media_blobs.len());
                    let mut needs_blob_fetch = false;
                    for (i, blob_ref) in item.media_blobs.iter().enumerate() {
                        // Try path hint first (zero I/O cost)
                        let hint = item.media_paths.get(i).map(|s| s.as_str()).unwrap_or("");
                        let hint_mime = crate::commands::media::mime_from_path_hint(hint);
                        if hint_mime != "application/octet-stream" {
                            mimes.push(hint_mime);
                        } else {
                            // Path hint inconclusive — fetch blob for magic bytes
                            let hash = match crate::blob_hash::parse_blob_hash(blob_ref) {
                                Ok(h) => h,
                                Err(_) => {
                                    mimes.push("application/octet-stream".to_string());
                                    continue;
                                }
                            };
                            match sync.get_media_header(hash, 32).await {
                                Ok(header) => {
                                    mimes.push(crate::commands::media::mime_from_magic_bytes(&header));
                                }
                                Err(_) => mimes.push("application/octet-stream".to_string()),
                            }
                            needs_blob_fetch = true;
                        }
                    }
                    let _ = needs_blob_fetch; // suppress unused warning
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

                // Write timestamp marker so we skip the next 24h
                let _ = std::fs::create_dir_all(marker_path.parent().unwrap_or(&app_data_dir));
                let _ = std::fs::write(&marker_path, format!("{}", chrono::Utc::now().timestamp()));
            });

            // Background: enforce blob_cache size limit on startup
            let eviction_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                let state = eviction_handle.state::<AppState>();
                if let Ok(app_data_dir) = state.app_handle.path().app_data_dir() {
                    let cache_dir = app_data_dir.join("blob_cache");
                    crate::commands::media::enforce_blob_cache_limit_startup(&cache_dir, crate::commands::media::BLOB_CACHE_MAX_BYTES);
                }
            });

            // Background: periodic malloc_trim to return freed pages to the OS.
            // On Linux, glibc's allocator keeps freed pages in the process address
            // space for reuse. After large Iroh blob exports or FFmpeg processing,
            // RSS stays inflated even though the memory is logically free. This task
            // calls malloc_trim(0) every 30s to force page return, preventing the
            // RSS from creeping upward during long sessions.
            #[cfg(target_os = "linux")]
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    // Safety: malloc_trim is thread-safe in glibc and always safe to call.
                    unsafe { libc::malloc_trim(0); }
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

            // Background: cross-store blob consistency check (P7)
            // Verifies that all blob hashes referenced in BytescapeItem.media_blobs
            // actually exist in the Iroh blob store. Logs orphans and broken references.
            // P0.2 fix: only run if last check was >24h ago to avoid CPU spike on every launch.
            let consistency_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Wait 10s so startup, session restore, and other background jobs finish first
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;

                let state = consistency_handle.state::<AppState>();

                // Check timestamp guard — skip if consistency check ran within 24 hours
                let app_data_dir = match state.app_handle.path().app_data_dir() {
                    Ok(d) => d,
                    Err(_) => return,
                };
                let marker_path = app_data_dir.join("blob_cache").join(".last_consistency");
                if let Ok(meta) = std::fs::metadata(&marker_path) {
                    if let Ok(modified) = meta.modified() {
                        if modified.elapsed().unwrap_or_default() < std::time::Duration::from_secs(24 * 3600) {
                            println!("consistency_check: skipped — last run was less than 24h ago");
                            return;
                        }
                    }
                }

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
                        None => {
                            // No SyncManager = no blob store to check against
                            println!("consistency_check: skipped — SyncManager not initialized");
                            return;
                        }
                    }
                };

                let headers = match db.list_all_item_headers().await {
                    Ok(h) => h,
                    Err(e) => {
                        eprintln!("consistency_check: failed to list items: {}", e);
                        return;
                    }
                };

                let mut total_refs = 0usize;
                let mut broken_refs = 0usize;
                let mut items_with_broken = 0usize;
                let mut items_healed = 0usize;

                for header in &headers {
                    if header.media_blob_count == 0 {
                        continue;
                    }

                    // Fetch full item to inspect blob references
                    let mut item = match db.get_item_by_id(&header.id).await {
                        Ok(Some(item)) => item,
                        _ => continue,
                    };

                    let mut has_broken = false;
                    let mut valid_blobs = Vec::with_capacity(item.media_blobs.len());
                    let mut valid_thumbs = Vec::with_capacity(item.thumbnail_blobs.len());
                    let mut valid_mimes = Vec::with_capacity(item.media_mimes.len());

                    for (i, blob_ref) in item.media_blobs.iter().enumerate() {
                        total_refs += 1;
                        let hash = match crate::blob_hash::parse_blob_hash(blob_ref) {
                            Ok(h) => h,
                            Err(_) => {
                                // Unparseable hash — broken reference
                                broken_refs += 1;
                                has_broken = true;
                                continue;
                            }
                        };

                        if sync.has_media(hash).await {
                            valid_blobs.push(blob_ref.clone());
                            // Preserve parallel arrays
                            if i < item.thumbnail_blobs.len() {
                                valid_thumbs.push(item.thumbnail_blobs[i].clone());
                            }
                            if i < item.media_mimes.len() {
                                valid_mimes.push(item.media_mimes[i].clone());
                            }
                        } else {
                            broken_refs += 1;
                            has_broken = true;
                        }
                    }

                    if has_broken {
                        items_with_broken += 1;

                        // Auto-heal: remove broken blob references from the item
                        item.media_blobs = valid_blobs;
                        item.thumbnail_blobs = valid_thumbs;
                        item.media_mimes = valid_mimes;

                        if db.upsert_item(&item).await.is_ok() {
                            items_healed += 1;
                        }
                    }

                    // Yield between items to avoid starving other tasks
                    tokio::task::yield_now().await;
                }

                if broken_refs > 0 {
                    println!(
                        "consistency_check: {}/{} blob refs broken across {} items ({} healed)",
                        broken_refs, total_refs, items_with_broken, items_healed
                    );
                } else if total_refs > 0 {
                    println!(
                        "consistency_check: all {} blob refs valid across {} items",
                        total_refs,
                        headers.iter().filter(|h| h.media_blob_count > 0).count()
                    );
                }

                // Write timestamp marker so we skip the next 24h
                let _ = std::fs::create_dir_all(marker_path.parent().unwrap_or(&app_data_dir));
                let _ = std::fs::write(&marker_path, format!("{}", chrono::Utc::now().timestamp()));
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
