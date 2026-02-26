//! Node & P2P Sync Commands
//! 
//! Commands for initializing the Iroh node and managing P2P sync documents.

use serde::{Deserialize, Serialize};
use tauri::{State, Emitter, Manager};
use tokio::sync::Mutex;
use std::sync::Arc;

use iroh::{Endpoint, SecretKey};
use iroh::address_lookup::mdns::MdnsAddressLookupBuilder;
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

/// Response containing P2P node status
#[derive(Serialize)]
pub struct NodeStatus {
    pub node_id: String,
    pub is_online: bool,
    pub relay_url: Option<String>,
    pub local_addrs: Vec<String>,
}

/// Response for P2P test doc creation
#[derive(Serialize)]
pub struct TestDocResult {
    pub ticket: String,
    pub namespace_id: String,
    pub entry_count: usize,
}

/// Response for joining and reading a test doc
#[derive(Serialize)]
pub struct TestDocReadResult {
    pub namespace_id: String,
    pub entries: Vec<TestDocEntry>,
    pub sync_elapsed_ms: u64,
}

/// A single entry read from a synced test doc
#[derive(Serialize)]
pub struct TestDocEntry {
    pub key: String,
    pub content: String,
    pub author: String,
}

/// Initialize the Iroh P2P node and sync manager.
/// 
/// This command:
/// 1. Loads or generates Iroh secret key from keychain
/// 2. Creates and binds the Iroh endpoint with mDNS LAN discovery
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
        .address_lookup(MdnsAddressLookupBuilder::default())
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
    // let db_manager_arc = state.db_manager.clone();
    // let app_handle = state.app_handle.clone();

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

/// Get the current P2P node status including node ID, relay, and local addresses.
#[tauri::command]
pub async fn get_node_status(state: State<'_, AppState>) -> Result<NodeStatus, String> {
    let endpoint_guard = state.endpoint.lock().await;
    let endpoint = endpoint_guard.as_ref()
        .ok_or("Iroh node not initialized. Call setup_iroh_node first.")?;

    let node_id = endpoint.secret_key().public().to_string();

    let addr = endpoint.addr();
    let relay_url: Option<String> = None; // relay_url extracted from addrs if available
    let local_addrs: Vec<String> = addr.addrs.iter()
        .map(|a| format!("{:?}", a))
        .collect();

    Ok(NodeStatus {
        node_id,
        is_online: true,
        relay_url,
        local_addrs,
    })
}

/// Create a test sync document with a sample entry. Returns the ticket for sharing.
#[tauri::command]
pub async fn p2p_create_test_doc(state: State<'_, AppState>) -> Result<TestDocResult, String> {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;

    // Create a new document
    let ticket = sync.create_doc().await
        .map_err(|e| BytescapeError::Sync(e))?;

    let namespace_id = match &ticket.capability {
        Capability::Write(secret) => secret.id(),
        Capability::Read(id) => *id,
    };

    // Write a test entry
    let test_content = format!(
        "Hello from Bytescape! Timestamp: {}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );
    sync.send_message(namespace_id, test_content).await
        .map_err(|e| format!("Failed to write test entry: {}", e))?;

    Ok(TestDocResult {
        ticket: ticket.to_string(),
        namespace_id: namespace_id.to_string(),
        entry_count: 1,
    })
}

/// Join a test sync document by ticket, wait for sync, and return all entries.
#[tauri::command]
pub async fn p2p_join_and_read_test_doc(
    ticket_str: String,
    state: State<'_, AppState>,
) -> Result<TestDocReadResult, String> {
    let start = std::time::Instant::now();

    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;

    let ticket: DocTicket = ticket_str.parse()
        .map_err(|e| format!("Invalid ticket: {}", e))?;

    let namespace_id = sync.join_doc(ticket).await
        .map_err(|e: String| BytescapeError::Sync(e))?;

    // Give sync some time to propagate entries
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // Read all entries from the synced document
    let entries = sync.read_doc_entries(namespace_id).await
        .map_err(|e| format!("Failed to read entries: {}", e))?;

    let elapsed = start.elapsed().as_millis() as u64;

    Ok(TestDocReadResult {
        namespace_id: namespace_id.to_string(),
        entries,
        sync_elapsed_ms: elapsed,
    })
}
