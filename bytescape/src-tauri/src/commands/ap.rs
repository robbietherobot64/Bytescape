//! ActivityPub Tauri Commands

use tauri::State;
use crate::AppState;
use crate::keychain_encrypted::EncryptedKeychain;
use crate::activitypub::ApActor;
use crate::activitypub::webfinger::WebFingerResponse;

/// Create the local user's ActivityPub actor.
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

/// Resolve a WebFinger acct: resource.
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
