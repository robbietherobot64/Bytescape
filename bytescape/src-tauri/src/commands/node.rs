//! Node & P2P Sync Commands
//! 
//! Commands for initializing the Iroh node and managing P2P sync documents.

use serde::{Deserialize, Serialize};
use tauri::{State, Manager};
use std::sync::Arc;

use iroh::{Endpoint, SecretKey};
use iroh::protocol::Router;
use crate::error::BytescapeError;
use crate::sync::{SyncManager, Capability, DocTicket};
use crate::protocol::{BytescapeProtocol, DocsHandler, PeerProfile, BYTESCAPE_ALPN};
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
        .address_lookup(iroh::address_lookup::DhtAddressLookup::builder().n0_dns_pkarr_relay())
        .bind()
        .await
        .map_err(|e| BytescapeError::P2PNetwork(e.to_string()))?;

    let node_id = endpoint.secret_key().public();
    *endpoint_guard = Some(endpoint.clone());

    // Initialize SyncManager (creates DocsEngine, blobs, gossip)
    let app_data_dir = state.app_handle.path()
        .app_data_dir()
        .map_err(|e: tauri::Error| BytescapeError::FileSystem(e.to_string()))?;
    let sync_path = app_data_dir.join("sync");
    let sync_manager = Arc::new(
        SyncManager::new(endpoint.clone(), sync_path).await
            .map_err(|e| BytescapeError::Sync(e))?
    );

    // --- Set up protocol Router ---
    // The Router dispatches incoming connections to the correct protocol handler
    // based on their ALPN (Application-Layer Protocol Negotiation) identifier.

    // 1. Docs handler: forwards iroh-docs connections to the DocsEngine
    let docs_handler = DocsHandler::new(sync_manager.engine_arc());

    // 2. Gossip handler: iroh-gossip implements ProtocolHandler natively
    let gossip = sync_manager.gossip().clone();

    // 3. Blobs handler: wraps the blob store for serving/fetching blobs
    let blobs_proto = iroh_blobs::BlobsProtocol::new(sync_manager.blobs(), None);

    // 4. Bytescape handler: custom protocol for profile exchange & peer comms
    let my_username = state.keychain.get_local_username()
        .unwrap_or_else(|_| None)
        .unwrap_or_else(|| "Anonymous".to_string());
    let my_did = state.keychain.get_did()
        .unwrap_or_else(|_| None)
        .unwrap_or_else(|| "local".to_string());
    let my_profile = PeerProfile {
        node_id: node_id.to_string(),
        username: my_username,
        did: my_did,
    };
    let bytescape_handler = BytescapeProtocol::new(
        my_profile,
        state.db_manager.clone(),
        state.app_handle.clone(),
    );

    // Build and spawn the Router — this starts the accept loop
    let router = Router::builder(endpoint.clone())
        .accept(iroh_docs::ALPN, docs_handler)
        .accept(iroh_gossip::ALPN, gossip)
        .accept(iroh_blobs::ALPN, blobs_proto)
        .accept(BYTESCAPE_ALPN, bytescape_handler)
        .spawn();

    println!("P2P Router: Spawned with 4 protocol handlers (docs, gossip, blobs, bytescape)");

    // Store everything in app state
    *state.sync_manager.lock().await = Some(sync_manager);
    *state.router.lock().await = Some(router);

    Ok(node_id.to_string())
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
