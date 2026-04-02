//! ActivityPub Tauri Commands
//!
//! Exposes five IPC commands that let the frontend manage a local
//! ActivityPub identity and interact with the federated social web.
//!
//! ## HOW IT WORKS
//!
//! 1. `create_ap_actor` generates an Ed25519-backed AP actor document,
//!    derives a `did:key` owner DID from the Iroh node's secret key,
//!    and persists the actor in the vault database.
//! 2. `get_ap_actor` retrieves the local actor as ActivityStreams JSON-LD
//!    for display or inclusion in outgoing activities.
//! 3. `send_ap_activity` signs an activity with HTTP Signatures and
//!    delivers it to a remote inbox via POST.
//! 4. `fetch_remote_actor` GETs a remote actor document, optionally
//!    attaching an HTTP Signature for authenticated fetches.
//! 5. `resolve_webfinger` performs RFC 7033 WebFinger lookup to turn
//!    `acct:user@domain` into an actor URL.
//!
//! ## TECH DEBT
//!
//! - `send_ap_activity` accepts `activity_json: String` and deserializes
//!   it here, meaning the JSON is parsed twice (once in JS → String, once
//!   in Rust → ApActivity).  Accepting `serde_json::Value` directly from
//!   the Tauri IPC bridge would eliminate the redundant parse.
//!
//! ## RELATED FILES
//!
//! - `activitypub/mod.rs`       — `ApActor` struct and helpers
//! - `activitypub/outbox.rs`    — `deliver_activity`, `fetch_remote_actor` HTTP logic
//! - `activitypub/webfinger.rs` — `resolve_webfinger` implementation
//! - `activitypub/types.rs`     — `ApActivity` and related AS2 types
//! - `keychain_encrypted.rs`    — `did_key_from_ed25519_bytes` DID derivation

use tauri::State;
use crate::AppState;
use crate::keychain_encrypted::EncryptedKeychain;
use crate::activitypub::ApActor;
use crate::activitypub::webfinger::WebFingerResponse;

/// Create the local user's ActivityPub actor.
///
/// Generates a new `ApActor` keyed to the Iroh node's Ed25519 identity,
/// persists it in the vault DB, and returns the ActivityStreams JSON-LD
/// representation.
///
/// # Parameters
/// - `username`     — Preferred handle (e.g. `"alice"`).
/// - `display_name` — Optional human-readable name.
/// - `summary`      — Optional bio / profile summary.
///
/// # Returns
/// ActivityStreams JSON-LD `serde_json::Value` of the newly created actor.
///
/// # Errors
/// - `"Iroh node not initialized"` — `setup_iroh_node` was not called yet.
/// - `"Database not initialized"` — `DbManager` is `None`.
/// - Propagates DB write errors.
///
/// # Side Effects
/// - Persists the actor to redb via `db.save_ap_actor()`.
///
/// # IPC
/// Exposed as Tauri command `create_ap_actor`.
/// Frontend invokes: `invoke('create_ap_actor', { username, displayName?, summary? })`
#[tauri::command]
pub async fn create_ap_actor(
    state: State<'_, AppState>,
    username: String,
    display_name: Option<String>,
    summary: Option<String>,
) -> Result<serde_json::Value, String> {
    let endpoint_guard = state.endpoint.lock().await;
    let endpoint = endpoint_guard
        .as_ref()
        .ok_or("Iroh node not initialized — call setup_iroh_node first")?;
    let secret_key = endpoint.secret_key().clone();
    drop(endpoint_guard);

    let owner_did = EncryptedKeychain::did_key_from_ed25519_bytes(secret_key.public().as_bytes());

    let mut actor = ApActor::new(&username, display_name, summary, &secret_key);
    actor.owner_did = owner_did;

    let db_guard = state.db_manager.read().await;
    let db = db_guard
        .as_ref()
        .ok_or("Database not initialized")?;
    db.save_ap_actor(&actor).await?;

    Ok(actor.to_activitystreams())
}

/// Retrieve the local user's AP actor as ActivityStreams JSON-LD.
///
/// Looks up the actor by the DID derived from the Iroh node's public key.
/// Returns `None` if no actor has been created yet.
///
/// # Returns
/// `Some(json)` with the ActivityStreams document, or `None`.
///
/// # IPC
/// Exposed as Tauri command `get_ap_actor`.
/// Frontend invokes: `invoke('get_ap_actor')`
#[tauri::command]
pub async fn get_ap_actor(
    state: State<'_, AppState>,
) -> Result<Option<serde_json::Value>, String> {
    let endpoint_guard = state.endpoint.lock().await;
    let endpoint = endpoint_guard
        .as_ref()
        .ok_or("Iroh node not initialized")?;
    let secret_key = endpoint.secret_key().clone();
    drop(endpoint_guard);

    let owner_did = EncryptedKeychain::did_key_from_ed25519_bytes(secret_key.public().as_bytes());

    let db_guard = state.db_manager.read().await;
    let db = db_guard
        .as_ref()
        .ok_or("Database not initialized")?;

    match db.get_ap_actor(&owner_did).await? {
        Some(actor) => Ok(Some(actor.to_activitystreams())),
        None => Ok(None),
    }
}

/// Deliver an activity to a remote inbox via signed HTTP POST.
///
/// Signs the request with the local actor's Ed25519 key (HTTP Signatures)
/// and POSTs the activity JSON to `target_inbox`.
///
/// # Parameters
/// - `activity_json` — Serialized JSON string of the `ApActivity`.
///   **Tech debt:** this is deserialized again here; accepting
///   `serde_json::Value` from IPC would avoid the double parse.
/// - `target_inbox` — Full URL of the remote actor's inbox endpoint.
///
/// # Returns
/// HTTP status code from the remote server (e.g. `202`).
///
/// # IPC
/// Exposed as Tauri command `send_ap_activity`.
/// Frontend invokes: `invoke('send_ap_activity', { activityJson, targetInbox })`
#[tauri::command]
pub async fn send_ap_activity(
    state: State<'_, AppState>,
    activity_json: String,
    target_inbox: String,
) -> Result<u16, String> {
    let endpoint_guard = state.endpoint.lock().await;
    let endpoint = endpoint_guard
        .as_ref()
        .ok_or("Iroh node not initialized")?;
    let secret_key = endpoint.secret_key().clone();
    drop(endpoint_guard);

    let owner_did = EncryptedKeychain::did_key_from_ed25519_bytes(secret_key.public().as_bytes());

    let db_guard = state.db_manager.read().await;
    let db = db_guard
        .as_ref()
        .ok_or("Database not initialized")?;
    let actor = db.get_ap_actor(&owner_did).await?
        .ok_or("No local AP actor — create one first")?;
    drop(db_guard);

    let activity: crate::activitypub::types::ApActivity = serde_json::from_str(&activity_json)
        .map_err(|e| format!("Invalid activity JSON: {}", e))?;

    crate::activitypub::outbox::deliver_activity(
        &actor,
        &activity,
        &target_inbox,
    )
    .await
}

/// Fetch a remote actor's ActivityStreams document.
///
/// If a local AP actor exists, the request is signed with HTTP Signatures
/// (required by some servers for authorized fetch). Falls back to an
/// unsigned GET when no local actor is available.
///
/// # Parameters
/// - `actor_url` — Full URL of the remote actor (e.g. `https://mastodon.social/users/alice`).
///
/// # Returns
/// The remote actor's ActivityStreams JSON-LD document.
///
/// # IPC
/// Exposed as Tauri command `fetch_remote_actor`.
/// Frontend invokes: `invoke('fetch_remote_actor', { actorUrl })`
#[tauri::command]
pub async fn fetch_remote_actor(
    state: State<'_, AppState>,
    actor_url: String,
) -> Result<serde_json::Value, String> {
    let endpoint_guard = state.endpoint.lock().await;
    let has_endpoint = endpoint_guard.is_some();

    if has_endpoint {
        let endpoint = endpoint_guard.as_ref().unwrap();
        let secret_key = endpoint.secret_key().clone();
        drop(endpoint_guard);

        let owner_did = EncryptedKeychain::did_key_from_ed25519_bytes(secret_key.public().as_bytes());

        let db_guard = state.db_manager.read().await;
        let db = db_guard.as_ref().ok_or("Database not initialized")?;
        let local_actor = db.get_ap_actor(&owner_did).await?;
        drop(db_guard);

        if let Some(ref actor) = local_actor {
            return crate::activitypub::outbox::fetch_remote_actor(
                &actor_url,
                Some(actor),
            )
            .await;
        }
    } else {
        drop(endpoint_guard);
    }

    crate::activitypub::outbox::fetch_remote_actor(
        &actor_url,
        None,
    )
    .await
}

/// Resolve a WebFinger `acct:` resource (RFC 7033).
///
/// Accepts either a bare `user@domain` or a full `acct:user@domain`
/// string and queries the domain's `/.well-known/webfinger` endpoint.
///
/// # Parameters
/// - `resource` — WebFinger resource, e.g. `"user@example.com"` or `"acct:user@example.com"`.
///
/// # Returns
/// `WebFingerResponse` containing the actor's profile links.
///
/// # IPC
/// Exposed as Tauri command `resolve_webfinger`.
/// Frontend invokes: `invoke('resolve_webfinger', { resource })`
#[tauri::command]
pub async fn resolve_webfinger(
    state: State<'_, AppState>,
    resource: String,
) -> Result<WebFingerResponse, String> {
    let acct = if resource.starts_with("acct:") {
        resource.clone()
    } else {
        format!("acct:{}", resource)
    };

    crate::activitypub::webfinger::resolve_webfinger(&state.http_client, &acct).await
}
