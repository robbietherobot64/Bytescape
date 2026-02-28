//! Node & P2P Sync Commands
//! 
//! Commands for initializing the Iroh node and managing P2P sync documents.

use serde::{Deserialize, Serialize};
use tauri::{State, Emitter, Manager};
use tokio::sync::Mutex;
use std::sync::Arc;

use iroh::{Endpoint, SecretKey};
use crate::error::BytescapeError;
use crate::db;
use crate::sync::{SyncManager, Capability, DocTicket};
use crate::AppState;

/// Response containing sync document ticket information
#[derive(Serialize, Deserialize)]
pub struct DocTicketInfo {
    pub ticket: String,
    pub namespace_id: String,
}

/// Initialize the Iroh P2P node and sync manager.
/// 
/// This command:
/// 1. Loads or generates Iroh secret key from keychain
/// 2. Creates and binds the Iroh endpoint
/// 3. Initializes the SyncManager for docs/blobs
/// 4. Spawns background P2P listener for incoming connections
/// 
/// Returns the node's public key (node ID).
#[tauri::command]
pub async fn setup_iroh_node(state: State<'_, AppState>) -> std::result::Result<String, String> {
    let mut endpoint_guard = state.endpoint.lock().await;
    if endpoint_guard.is_some() {
        return Ok(endpoint_guard.as_ref().unwrap().secret_key().public().to_string());
    }

    let secret_key = match state.keychain.load_iroh_secret() {
        Ok(Some(key)) => key,
        Ok(None) => {
            let key = SecretKey::generate(&mut rand::rng());
            state.keychain.save_iroh_secret(&key)
                .map_err(|e| BytescapeError::KeychainSave(e.to_string()))?;
            key
        },
        Err(e) => return Err(BytescapeError::KeychainLoad(e.to_string()).to_string()),
    };

    let endpoint = Endpoint::builder()
        .secret_key(secret_key.clone())
        .alpns(vec![b"bytescape/1.0".to_vec()])
        .address_lookup(iroh::address_lookup::DhtAddressLookup::builder().n0_dns_pkarr_relay())
        .bind()
        .await
        .map_err(|e| BytescapeError::P2PNetwork(e.to_string()))?;

    let node_id = endpoint.secret_key().public();
    *endpoint_guard = Some(endpoint.clone());

    // Initialize SyncManager
    let app_data_dir = state.app_handle.path()
        .app_data_dir()
        .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
    let sync_path = app_data_dir.join("sync");
    let sync_manager = Arc::new(
        SyncManager::new(endpoint.clone(), sync_path).await
            .map_err(|e| BytescapeError::Sync(e))?
    );
    *state.sync_manager.lock().await = Some(sync_manager);

    // Spawn background P2P listener
    let _db_manager_arc = state.db_manager.clone();
    let _app_handle = state.app_handle.clone();

    /* 
    // TODO: This loop steals connections from Iroh DocsEngine. 
    // We need a proper dispatcher or use Iroh's protocol handling differently to support both Chat and Docs.
    // For now, disabling Chat listener to fix Sync stability.
    
    tokio::spawn(async move {
        while let Some(incoming) = endpoint.accept().await {
            let db = db_manager_arc.clone();
            let app = app_handle.clone();
            tokio::spawn(async move {
                match incoming.accept() {
                    Ok(accepting) => {
                        match accepting.await {
                            Ok(connection) => {
                                let alpn = connection.alpn();
                                if alpn == b"bytescape/1.0" {
                                    handle_established_connection(connection, db, app).await;
                                } else {
                                    // Let DocsEngine handle it?
                                    // Actually, if we're stealing connections from DocsEngine, this loop is problematic.
                                    // For now, let's just log it and see if DocsEngine complains.
                                    // If DocsEngine handles its own loop, we are definitely racing. 
                                    // But maybe iroh handles this gracefully? I'll check if DocsEngine loop is running.
                                    // If we accepted it here, DocsEngine MISSED it.
                                    println!("Received non-chat connection (ALPN: {:?}). DocsEngine might have missed this.", String::from_utf8_lossy(&alpn));
                                }
                            }
                            Err(e) => eprintln!("Connection handshake failed: {}", e),
                        }
                    }
                    Err(e) => eprintln!("Failed to accept incoming connection: {}", e),
                }
            });
        }
    }); 
    */

    Ok(node_id.to_string())
}

/// Handle established P2P connections and save received messages
async fn handle_established_connection(
    connection: iroh::endpoint::Connection,
    db: Arc<Mutex<Option<crate::db::DbManager>>>,
    app: tauri::AppHandle,
) {
    let remote_node_id = connection.remote_id();
    if let Ok((_send, mut recv)) = connection.accept_bi().await {
        if let Ok(content) = recv.read_to_end(1024 * 1024).await {
            if let Ok(msg_str) = String::from_utf8(content) {
                println!("Received P2P message from {}: {}", remote_node_id, msg_str);
                process_p2p_message(&msg_str, remote_node_id.to_string(), &db, &app).await;
            }
        }
    }
}

/// Process and save a received P2P message
async fn process_p2p_message(
    content: &str,
    remote_node_id: String,
    db: &Arc<Mutex<Option<crate::db::DbManager>>>,
    app: &tauri::AppHandle,
) {
    let id = uuid::Uuid::new_v4().to_string();
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    
    let message = db::Message {
        id: id.clone(),
        conversation_id: remote_node_id.clone(),
        sender_did: remote_node_id,
        content: content.to_string(),
        created_at,
        transport_layer: db::TransportLayer::Iroh,
        synced_with_bsky: false,
    };
    
    let db_guard = db.lock().await;
    if let Some(db_ref) = db_guard.as_ref() {
        if let Err(e) = db_ref.save_message(&message).await {
            eprintln!("Failed to save message: {}", e);
        }
    }
    
    if let Err(e) = app.emit("new-message", &message) {
        eprintln!("Failed to emit event: {}", e);
    }
}

/// Create a new sync document and return a ticket for sharing.
/// 
/// The ticket can be shared with other nodes to grant them access
/// to the sync document.
#[tauri::command]
pub async fn create_sync_doc(state: State<'_, AppState>) -> std::result::Result<DocTicketInfo, String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;

    let ticket = sync.create_doc().await
        .map_err(|e| BytescapeError::Sync(e))?;
    
    let namespace_id = match &ticket.capability {
        Capability::Write(secret) => secret.id().to_string(),
        Capability::Read(id) => id.to_string(),
    };

    Ok(DocTicketInfo {
        ticket: ticket.to_string(),
        namespace_id,
    })
}

/// Join an existing sync document using a ticket.
/// 
/// The ticket contains the namespace ID and capability needed
/// to participate in the sync document.
#[tauri::command]
pub async fn join_sync_doc(ticket_str: String, state: State<'_, AppState>) -> std::result::Result<String, String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;

    let ticket: DocTicket = match ticket_str.parse() {
        Ok(t) => t,
        Err(e) => return Err(BytescapeError::InvalidTicket(e.to_string()).to_string()),
    };

    let namespace_id = sync.join_doc(ticket).await
        .map_err(|e: String| BytescapeError::Sync(e))?;
    Ok(namespace_id.to_string())
}

/// Entry from a P2P test/diagnostic document.
#[derive(Serialize, Deserialize, Clone)]
pub struct TestDocEntry {
    pub key: String,
    pub content: String,
    pub author: String,
}

/// Return the current status of the Iroh P2P node.
///
/// Reports whether the node is online and its public key.
#[tauri::command]
pub async fn get_node_status(state: State<'_, AppState>) -> std::result::Result<serde_json::Value, String> {
    let endpoint_guard = state.endpoint.lock().await;
    match endpoint_guard.as_ref() {
        Some(endpoint) => {
            let node_id = endpoint.secret_key().public().to_string();
            Ok(serde_json::json!({
                "online": true,
                "node_id": node_id,
            }))
        }
        None => Ok(serde_json::json!({
            "online": false,
            "node_id": null,
        })),
    }
}

/// Create a new Iroh doc and write a test entry into it.
/// Returns a shareable ticket and the namespace ID.
#[tauri::command]
pub async fn p2p_create_test_doc(
    content: String,
    state: State<'_, AppState>,
) -> std::result::Result<DocTicketInfo, String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard
        .as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;

    let ticket = sync
        .create_doc()
        .await
        .map_err(|e| BytescapeError::Sync(e))?;

    let namespace_id = match &ticket.capability {
        Capability::Write(secret) => secret.id(),
        Capability::Read(id) => *id,
    };

    // Write a test entry using the same pattern as send_message
    sync.send_message(namespace_id, content)
        .await
        .map_err(|e| BytescapeError::Sync(e))?;

    Ok(DocTicketInfo {
        ticket: ticket.to_string(),
        namespace_id: namespace_id.to_string(),
    })
}

/// Join an existing doc via ticket and read all entries from it.
#[tauri::command]
pub async fn p2p_join_and_read_test_doc(
    ticket_str: String,
    state: State<'_, AppState>,
) -> std::result::Result<Vec<TestDocEntry>, String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard
        .as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;

    let ticket: DocTicket = ticket_str
        .parse()
        .map_err(|e| BytescapeError::InvalidTicket(format!("{}", e)).to_string())?;

    let namespace_id = sync
        .join_doc(ticket)
        .await
        .map_err(|e| BytescapeError::Sync(e))?;

    // Give the doc a moment to sync before reading
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    sync.read_doc_entries(namespace_id).await
}
