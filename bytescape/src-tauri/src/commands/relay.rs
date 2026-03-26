//! Relay and server provisioning commands.
//!
//! These commands are forward-looking stubs for Phase 4.5 server infrastructure.
//! The `bytescape.social` server will host:
//! - An Iroh relay (encrypted store-and-forward for offline clients)
//! - An AT Protocol PDS (Tranquil PDS — Bluesky federation)
//! - An ActivityPub instance (custom axum — Mastodon/fediverse federation)
//!
//! Until the server is deployed, all commands return descriptive errors
//! explaining what they will do and what infrastructure is required.

use tauri::State;
use crate::AppState;

// ─── Relay Sync (A-S2) ─────────────────────────────────────────────

/// Connect to a Bytescape relay server for encrypted store-and-forward sync.
///
/// When the relay is deployed, this will:
/// 1. Establish a QUIC connection to the relay via Iroh
/// 2. Authenticate using the user's `did:key` (Ed25519 keypair)
/// 3. Register for push notifications (if token is set)
/// 4. Begin syncing any pending blobs/docs
///
/// Returns the relay's node ID on success.
#[tauri::command]
pub async fn relay_connect(
    _state: State<'_, AppState>,
    _relay_url: String,
) -> Result<String, String> {
    Err("Relay not configured — Phase 4.5 infrastructure required. \
         The Iroh relay at relay.bytescape.social will provide encrypted \
         store-and-forward sync for offline clients.".to_string())
}

/// Push a local blob to the relay for availability when the client is offline.
///
/// When deployed, this will:
/// 1. Look up the blob by hash in the local Iroh blob store
/// 2. Upload it to the relay's blob cache
/// 3. The relay stores it encrypted, keyed to the user's DID
/// 4. Other authorized peers can pull it when this client is offline
#[tauri::command]
pub async fn relay_push_blob(
    _state: State<'_, AppState>,
    _blob_hash: String,
) -> Result<(), String> {
    Err("Relay not configured — Phase 4.5 infrastructure required. \
         Blob push requires a running Iroh relay at relay.bytescape.social.".to_string())
}

/// Pull any pending items from the relay that arrived while this client was offline.
///
/// When deployed, this will:
/// 1. Query the relay for blobs/docs deposited for this user's DID
/// 2. Download and integrate them into the local Iroh store
/// 3. Return a list of blob hashes that were pulled
#[tauri::command]
pub async fn relay_pull_pending(
    _state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    Err("Relay not configured — Phase 4.5 infrastructure required. \
         Pending pull requires a running Iroh relay at relay.bytescape.social.".to_string())
}

/// Register a push notification token with the relay for mobile wakeup.
///
/// When deployed, this will:
/// 1. Send the platform-specific push token (APNs/FCM) to the relay
/// 2. The relay will send a push notification when new data arrives
///    for this user while their client is offline
/// 3. Platform: "ios" | "android" | "desktop" (desktop uses WebSocket wakeup)
#[tauri::command]
pub async fn relay_register_push_token(
    _state: State<'_, AppState>,
    _token: String,
    _platform: String,
) -> Result<(), String> {
    Err("Relay not configured — Phase 4.5 infrastructure required. \
         Push token registration requires a running Iroh relay.".to_string())
}

/// Check the current relay sync status.
///
/// When deployed, returns a JSON object with:
/// - `connected`: bool — whether currently connected to the relay
/// - `last_sync`: timestamp — when the last successful sync occurred
/// - `pending_push`: number — blobs waiting to be pushed
/// - `pending_pull`: number — blobs waiting to be pulled
/// - `relay_url`: string — the configured relay URL
#[tauri::command]
pub async fn relay_sync_status(
    _state: State<'_, AppState>,
) -> Result<String, String> {
    Ok(serde_json::json!({
        "connected": false,
        "last_sync": null,
        "pending_push": 0,
        "pending_pull": 0,
        "relay_url": null,
        "status": "not_configured",
        "message": "Phase 4.5 infrastructure required — relay.bytescape.social not yet deployed"
    }).to_string())
}

// ─── PDS Account Provisioning (A-S3) ───────────────────────────────

/// Provision an account on the Bytescape AT Protocol PDS.
///
/// When deployed, this will:
/// 1. Call the Tranquil PDS XRPC endpoint to create a new account
/// 2. Optionally use an invite code for gated access
/// 3. Store the returned `did:plc` and PDS credentials in the keychain
/// 4. Update the user's `did:web` document to include the PDS service endpoint
/// 5. The user's Bluesky handle becomes `{username}.bytescape.social`
///
/// Returns the assigned `did:plc` on success.
#[tauri::command]
pub async fn provision_pds_account(
    _state: State<'_, AppState>,
    _username: String,
    _invite_code: Option<String>,
) -> Result<String, String> {
    Err("PDS not deployed — Phase 4.5 infrastructure required. \
         Account provisioning requires the Tranquil PDS at pds.bytescape.social. \
         Once deployed, this will create your AT Protocol identity and enable \
         Bluesky federation from your Bytescape account.".to_string())
}

// ─── AP Actor Provisioning (A-S4) ──────────────────────────────────

/// Provision an ActivityPub actor on the Bytescape AP instance.
///
/// When deployed, this will:
/// 1. Create an AP actor at `https://bytescape.social/@{username}`
/// 2. Set up WebFinger resolution for `acct:{username}@bytescape.social`
/// 3. Link the AP actor to the user's `did:web` document
/// 4. Store AP credentials in the keychain
/// 5. Optionally use an invite code for gated access
///
/// Returns the AP actor URL on success.
#[tauri::command]
pub async fn provision_ap_actor(
    _state: State<'_, AppState>,
    _username: String,
    _invite_code: Option<String>,
) -> Result<String, String> {
    Err("AP instance not deployed — Phase 4.5 infrastructure required. \
         Actor provisioning requires the custom axum AP server at bytescape.social. \
         Once deployed, this will create your fediverse identity and enable \
         Mastodon/Lemmy/Threads federation from your Bytescape account.".to_string())
}
