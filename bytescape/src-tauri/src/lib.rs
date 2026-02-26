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

mod auth;
use auth::AuthManager;
mod chat;
use chat::ChatClient;
mod contacts;
mod traffic;
use traffic::TrafficController;

/// Application-wide state shared across all commands
pub struct AppState {
    pub app_handle: AppHandle,
    pub endpoint: Mutex<Option<Endpoint>>,
    pub keychain: keychain_encrypted::EncryptedKeychain,
    pub auth_manager: AuthManager,
    pub db_manager: Arc<Mutex<Option<db::DbManager>>>,
    pub chat_client: Arc<ChatClient>,
    pub sync_manager: Mutex<Option<Arc<sync::SyncManager>>>,
    pub traffic_controller: Arc<TrafficController>,
    pub pending_import: Mutex<Option<commands::import::ParseResult>>,
    pub media_port: u16,
}

/// Run the Bytescape application
/// 
/// This is the main entry point called from main.rs
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
            
            // Auth commands
            commands::auth::check_auth_status,
            commands::auth::login,
            commands::auth::logout,
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
            commands::media::resolve_hls_video,
            commands::media::get_media_dir,
            commands::media::diagnose_media_ref,
            commands::media::backfill_media_mimes,
            commands::media::get_media_server_port,
            commands::syndicate::syndicate_vault_item,
            commands::posting::publish_post_v2,
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
                master_password,
            );

            // Initialize database
            let db_path = app_data_dir.join("bytescape.redb");
            let db_manager = Arc::new(Mutex::new(
                Some(db::DbManager::new(db_path)
                    .map_err(|e| format!("Failed to initialize database: {}", e))?)
            ));

            // Initialize auth manager
            let auth_manager = AuthManager::new();

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
                traffic_controller,
                pending_import: Mutex::new(None),
                media_port,
            };

            app.manage(state);

            // Background: backfill media_mimes for existing items
            let backfill_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Short delay to let the rest of setup complete
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                let state = backfill_handle.state::<AppState>();
                let db_guard = state.db_manager.lock().await;
                let sync_guard = state.sync_manager.lock().await;
                if db_guard.is_some() && sync_guard.is_some() {
                    drop(db_guard);
                    drop(sync_guard);
                    // Use the same logic as the command but directly
                    let db_guard = state.db_manager.lock().await;
                    let db = match db_guard.as_ref() {
                        Some(d) => d,
                        None => return,
                    };
                    let sync_guard2 = state.sync_manager.lock().await;
                    let sync = match sync_guard2.as_ref() {
                        Some(s) => s.clone(),
                        None => return,
                    };
                    drop(sync_guard2);

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
                            let mut hash_str = blob_ref.trim().to_string();
                            while let Some(rest) = hash_str.strip_prefix("blob:") {
                                hash_str = rest.trim().to_string();
                            }
                            if let Some(pos) = hash_str.find(':') {
                                hash_str = hash_str[..pos].to_string();
                            }
                            if hash_str.is_empty() {
                                mimes.push("application/octet-stream".to_string());
                                continue;
                            }
                            // Try to parse hash and detect MIME
                            let uppercase = hash_str.to_ascii_uppercase();
                            let decoded = data_encoding::BASE32_NOPAD.decode(uppercase.as_bytes());
                            if let Ok(bytes) = decoded {
                                if bytes.len() >= 32 {
                                    let mut arr = [0u8; 32];
                                    arr.copy_from_slice(&bytes[bytes.len()-32..]);
                                    let hash = crate::sync::Hash::from_bytes(arr);
                                    match sync.get_media(hash).await {
                                        Ok(data) => {
                                            let mime = if data.starts_with(&[0xFF, 0xD8, 0xFF]) { "image/jpeg" }
                                                else if data.starts_with(b"\x89PNG\r\n\x1a\n") { "image/png" }
                                                else if data.starts_with(b"GIF8") { "image/gif" }
                                                else if data.starts_with(b"RIFF") && data.len() > 12 && &data[8..12] == b"WEBP" { "image/webp" }
                                                else if data.len() > 12 && &data[4..8] == b"ftyp" { "video/mp4" }
                                                else if data.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) { "video/webm" }
                                                else if data.starts_with(b"OggS") { "video/ogg" }
                                                else { "application/octet-stream" };
                                            mimes.push(mime.to_string());
                                        }
                                        Err(_) => mimes.push("application/octet-stream".to_string()),
                                    }
                                } else {
                                    mimes.push("application/octet-stream".to_string());
                                }
                            } else {
                                mimes.push("application/octet-stream".to_string());
                            }
                        }
                        item.media_mimes = mimes;
                        if db.upsert_item(&item).await.is_ok() {
                            updated += 1;
                        }
                    }
                    if updated > 0 {
                        println!("backfill_media_mimes: updated {} items on startup", updated);
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
