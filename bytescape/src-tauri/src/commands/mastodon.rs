//! Mastodon & AP Tauri Commands (Sprint v7 — A1.2)
//!
//! P1: Mastodon OAuth2 (register, complete, session, disconnect)
//! P2: Mastodon Home Feed
//! P3: AP DMs (send via Mastodon C2S + AP S2S)
//! P4: Unified send_message
//! P5: Follow/Unfollow (Mastodon C2S + AP S2S)
//! P6: Unified resolve_profile

use tauri::{State, Emitter, Manager};
use crate::AppState;
use crate::connectors::mastodon::{
    self, MastodonApp, MastodonSession,
};

/// Port for the Mastodon OAuth callback server.
const MASTODON_CALLBACK_PORT: u16 = 28461;

/// In-memory storage for pending Mastodon app registrations (pre-auth).
static PENDING_APP: std::sync::OnceLock<tokio::sync::Mutex<Option<MastodonApp>>> = std::sync::OnceLock::new();

fn pending_app() -> &'static tokio::sync::Mutex<Option<MastodonApp>> {
    PENDING_APP.get_or_init(|| tokio::sync::Mutex::new(None))
}

/// In-memory Mastodon session (loaded from keychain on restore, updated on auth).
static MASTODON_SESSION: std::sync::OnceLock<tokio::sync::Mutex<Option<MastodonSession>>> = std::sync::OnceLock::new();

fn masto_session_lock() -> &'static tokio::sync::Mutex<Option<MastodonSession>> {
    MASTODON_SESSION.get_or_init(|| tokio::sync::Mutex::new(None))
}

/// Shutdown sender for the Mastodon callback server.
static MASTODON_CALLBACK_SHUTDOWN: std::sync::OnceLock<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>> = std::sync::OnceLock::new();

fn mastodon_shutdown_lock() -> &'static tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>> {
    MASTODON_CALLBACK_SHUTDOWN.get_or_init(|| tokio::sync::Mutex::new(None))
}

// ═══════════════════════════════════════════════════
// P1: Mastodon OAuth2
// ═══════════════════════════════════════════════════

/// Register Bytescape as an OAuth app on a Mastodon instance.
///
/// Spawns a local callback server on port 28461 to catch the OAuth redirect.
/// Returns the OAuth authorization URL as a plain string for `openUrl()`.
/// When the callback is received, emits `mastodon-auth-complete` with AuthStatus.
#[tauri::command]
pub async fn mastodon_register_app(
    state: State<'_, AppState>,
    instance: String,
) -> Result<String, String> {
    // Stop any previous callback server
    if let Some(tx) = mastodon_shutdown_lock().lock().await.take() {
        let _ = tx.send(());
    }

    let (app, auth_url) = mastodon::register_app(&state.http_client, &instance).await?;
    *pending_app().lock().await = Some(app.clone());

    // Bind the callback server
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", MASTODON_CALLBACK_PORT))
        .await
        .map_err(|e| format!("Failed to bind Mastodon callback server on port {}: {}", MASTODON_CALLBACK_PORT, e))?;

    println!("Mastodon OAuth: Callback server listening on http://127.0.0.1:{}/mastodon/callback", MASTODON_CALLBACK_PORT);

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    *mastodon_shutdown_lock().lock().await = Some(shutdown_tx);

    let app_handle = state.app_handle.clone();
    let http_client = state.http_client.clone();

    tokio::spawn(async move {
        run_mastodon_callback_server(listener, shutdown_rx, app, http_client, app_handle).await;
    });

    Ok(auth_url)
}

/// Run the Mastodon OAuth callback server.
///
/// Listens for the browser redirect to `/mastodon/callback?code=...`,
/// exchanges the code for tokens, and emits `mastodon-auth-complete`.
async fn run_mastodon_callback_server(
    listener: tokio::net::TcpListener,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    app: MastodonApp,
    http_client: reqwest::Client,
    app_handle: tauri::AppHandle,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    loop {
        tokio::select! {
            result = listener.accept() => {
                if let Ok((mut stream, _)) = result {
                    let mut buf = vec![0u8; 8192];
                    let n = match stream.read(&mut buf).await {
                        Ok(n) if n > 0 => n,
                        _ => continue,
                    };

                    let request = String::from_utf8_lossy(&buf[..n]);
                    let first_line = request.lines().next().unwrap_or("");
                    let path = first_line.split_whitespace().nth(1).unwrap_or("");

                    let html = if path.starts_with("/mastodon/callback") {
                        if let Some(query_str) = path.split_once('?').map(|(_, q)| q) {
                            // Extract the code parameter
                            let mut code: Option<String> = None;
                            let mut error: Option<String> = None;
                            for pair in query_str.split('&') {
                                if let Some((key, value)) = pair.split_once('=') {
                                    let decoded = urlencoding::decode(value)
                                        .unwrap_or(std::borrow::Cow::Borrowed(value))
                                        .to_string();
                                    match key {
                                        "code" => code = Some(decoded),
                                        "error" => error = Some(decoded),
                                        _ => {}
                                    }
                                }
                            }

                            if let Some(err) = error {
                                let _ = app_handle.emit("mastodon-auth-error", err.clone());
                                mastodon_error_html(&err)
                            } else if let Some(auth_code) = code {
                                handle_mastodon_callback(
                                    &http_client, &app, &auth_code, &app_handle,
                                ).await
                            } else {
                                let msg = "Missing authorization code in callback";
                                let _ = app_handle.emit("mastodon-auth-error", msg.to_string());
                                mastodon_error_html(msg)
                            }
                        } else {
                            mastodon_error_html("Missing query parameters")
                        }
                    } else {
                        // Not the callback path — return a simple 404
                        "<html><body>Not found</body></html>".to_string()
                    };

                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        html.len(), html,
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.flush().await;

                    // If we handled the callback, shut down after a brief delay
                    if path.starts_with("/mastodon/callback") {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        return;
                    }
                }
            }
            _ = &mut shutdown_rx => {
                println!("Mastodon OAuth: Callback server stopped");
                return;
            }
        }
    }
}

/// Handle the Mastodon OAuth callback: exchange code → session → emit event.
async fn handle_mastodon_callback(
    http_client: &reqwest::Client,
    app: &MastodonApp,
    auth_code: &str,
    app_handle: &tauri::AppHandle,
) -> String {
    match mastodon::complete_auth(http_client, app, auth_code).await {
        Ok(session) => {
            // Persist to keychain
            let blob = serde_json::json!({ "session": &session, "app": app });
            let json_str = serde_json::to_string(&blob).unwrap_or_default();

            let app_state = app_handle.state::<crate::AppState>();
            let _ = app_state.keychain.set_mastodon_session(&json_str);

            // Ensure identity infrastructure exists for Mastodon-only users
            let recovery_status = app_state.keychain.get_identity_recovery_status()
                .unwrap_or(crate::keychain_encrypted::IdentityRecoveryStatus {
                    has_local_identity: false, has_escrow: false,
                    recovery_pending: false, recovery_verified_pending_finalize: false,
                    did: None, public_key: None, last_recovered_at: None,
                });
            if !recovery_status.has_local_identity {
                let _ = app_state.keychain.identity_init_full_recovery("bytescape-mastodon-auto-init");
            }
            let _ = app_state.keychain.ensure_master_did();

            let instance_host = session.instance.replace("https://", "").replace("http://", "");

            // Generate a did:plc for this Mastodon account
            let acct_uri = format!("acct:{}@{}", session.acct, instance_host);
            let mastodon_plc = crate::keychain_encrypted::EncryptedKeychain::generate_platform_plc("mastodon", &acct_uri);

            let linked = crate::keychain_encrypted::LinkedAccount {
                protocol: "mastodon".to_string(),
                identifier: mastodon_plc.clone(),
                platform_identifier: Some(acct_uri.clone()),
                display_handle: format!("@{}@{}", session.acct, instance_host),
                display_name: if session.display_name.is_empty() { None } else { Some(session.display_name.clone()) },
                avatar_url: if session.avatar.is_empty() { None } else { Some(session.avatar.clone()) },
                connected_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                active: true,
            };
            app_state.keychain.add_linked_account(linked)
                .unwrap_or_else(|e| eprintln!("Failed to store linked account: {}", e));

            // Get owner DID AFTER linked account is stored so did:plc is findable
            // Priority: web_did > bluesky did > linked account did:plc > did:key
            let owner_did = app_state.keychain.get_owner_did()
                .ok().flatten()
                .unwrap_or_else(|| mastodon_plc.clone());

            let local_username = app_state.keychain.get_local_username().unwrap_or(None);
            let identity_ready = local_username.is_some();
            let web_did = app_state.keychain.get_web_did().unwrap_or(None)
                .or_else(|| app_state.keychain.ensure_web_did().ok().flatten());

            // Build AuthStatus for the frontend
            let auth_status = serde_json::json!({
                "handle": format!("{}@{}", session.acct, instance_host),
                "did": owner_did,
                "mastodon_acct": acct_uri,
                "mastodon_plc": mastodon_plc,
                "source": "mastodon",
                "local_username": local_username,
                "identity_ready": identity_ready,
                "web_did": web_did,
            });

            *masto_session_lock().lock().await = Some(session);

            let _ = app_handle.emit("mastodon-auth-complete", auth_status);

            mastodon_success_html()
        }
        Err(e) => {
            eprintln!("Mastodon OAuth callback error: {}", e);
            let _ = app_handle.emit("mastodon-auth-error", e.clone());
            mastodon_error_html(&e)
        }
    }
}

fn mastodon_success_html() -> String {
    r#"<!DOCTYPE html>
<html><body style="background:#1a1a2e;color:#e0e0e0;font-family:sans-serif;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;">
<div style="text-align:center;">
<h1 style="font-size:48px;margin-bottom:16px;">✅</h1>
<h2>Mastodon Authentication Successful</h2>
<p style="color:#a0a0b0;">You can close this tab and return to Bytescape.</p>
</div></body></html>"#.to_string()
}

fn mastodon_error_html(message: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html><body style="background:#1a1a2e;color:#e0e0e0;font-family:sans-serif;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;">
<div style="text-align:center;">
<h1 style="font-size:48px;margin-bottom:16px;">❌</h1>
<h2>Authentication Failed</h2>
<p style="color:#ff6b6b;">{}</p>
<p style="color:#a0a0b0;">Return to Bytescape and try again.</p>
</div></body></html>"#,
        message,
    )
}

/// Complete Mastodon OAuth2 by exchanging the auth code for an access token.
#[tauri::command]
pub async fn mastodon_complete_auth(
    state: State<'_, AppState>,
    instance: String,
    auth_code: String,
) -> Result<serde_json::Value, String> {
    let app = {
        let guard = pending_app().lock().await;
        match guard.as_ref() {
            Some(app) if app.instance == mastodon::normalize_instance(&instance) => app.clone(),
            _ => {
                drop(guard);
                let (new_app, _) = mastodon::register_app(&state.http_client, &instance).await?;
                *pending_app().lock().await = Some(new_app.clone());
                new_app
            }
        }
    };

    let session = mastodon::complete_auth(&state.http_client, &app, &auth_code).await?;

    // Persist to keychain
    let blob = serde_json::json!({ "session": &session, "app": &app });
    let json_str = serde_json::to_string(&blob)
        .map_err(|e| format!("Serialize error: {}", e))?;
    state.keychain.set_mastodon_session(&json_str)
        .map_err(|e| format!("Keychain save failed: {}", e))?;

    // Ensure identity infrastructure exists for Mastodon-only users
    let recovery_status = state.keychain.get_identity_recovery_status()
        .unwrap_or(crate::keychain_encrypted::IdentityRecoveryStatus {
            has_local_identity: false, has_escrow: false,
            recovery_pending: false, recovery_verified_pending_finalize: false,
            did: None, public_key: None, last_recovered_at: None,
        });
    if !recovery_status.has_local_identity {
        let _ = state.keychain.identity_init_full_recovery("bytescape-mastodon-auto-init");
    }

    let _ = state.keychain.ensure_master_did();

    // Generate a did:plc for this Mastodon account
    let instance_host = session.instance.replace("https://", "").replace("http://", "");
    let acct_uri = format!("acct:{}@{}", session.acct, instance_host);
    let mastodon_plc = crate::keychain_encrypted::EncryptedKeychain::generate_platform_plc("mastodon", &acct_uri);

    let linked = crate::keychain_encrypted::LinkedAccount {
        protocol: "mastodon".to_string(),
        identifier: mastodon_plc.clone(),
        platform_identifier: Some(acct_uri.clone()),
        display_handle: format!("@{}@{}", session.acct, instance_host),
        display_name: if session.display_name.is_empty() { None } else { Some(session.display_name.clone()) },
        avatar_url: if session.avatar.is_empty() { None } else { Some(session.avatar.clone()) },
        connected_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        active: true,
    };
    state.keychain.add_linked_account(linked)
        .map_err(|e| format!("Failed to store linked account: {}", e))?;

    // Get owner DID AFTER linked account is stored so did:plc is findable
    // Priority: web_did > bluesky did > linked account did:plc > did:key
    let owner_did = state.keychain.get_owner_did()
        .map_err(|e| format!("Owner DID error: {}", e))?
        .unwrap_or_else(|| mastodon_plc.clone());

    let local_username = state.keychain.get_local_username().unwrap_or(None);
    let identity_ready = local_username.is_some();
    let web_did = state.keychain.get_web_did().unwrap_or(None)
        .or_else(|| state.keychain.ensure_web_did().ok().flatten());

    // Build AuthStatus-shaped response that frontend expects
    let auth_status = serde_json::json!({
        "handle": format!("{}@{}", session.acct, instance_host),
        "did": owner_did,
        "mastodon_acct": acct_uri,
        "mastodon_plc": mastodon_plc,
        "source": "mastodon",
        "local_username": local_username,
        "identity_ready": identity_ready,
        "web_did": web_did,
    });
    *masto_session_lock().lock().await = Some(session);

    Ok(auth_status)
}

/// Get current Mastodon session (from memory or restore from keychain).
#[tauri::command]
pub async fn mastodon_get_session(
    state: State<'_, AppState>,
) -> Result<Option<serde_json::Value>, String> {
    // In-memory first
    {
        let guard = masto_session_lock().lock().await;
        if let Some(ref session) = *guard {
            return Ok(Some(serde_json::to_value(session)
                .map_err(|e| format!("Serialize error: {}", e))?));
        }
    }
    // Restore from keychain
    if let Some(session) = restore_from_keychain(&state)? {
        let json = serde_json::to_value(&session)
            .map_err(|e| format!("Serialize error: {}", e))?;
        *masto_session_lock().lock().await = Some(session);
        return Ok(Some(json));
    }
    Ok(None)
}

/// Disconnect Mastodon session (clear from memory + keychain).
#[tauri::command]
pub async fn mastodon_disconnect(
    state: State<'_, AppState>,
) -> Result<(), String> {
    *masto_session_lock().lock().await = None;
    *pending_app().lock().await = None;
    let _ = state.keychain.clear_mastodon_session();
    Ok(())
}

/// Clear in-memory Mastodon session and pending app state.
/// Called by delete_account to ensure no stale session survives in memory.
pub async fn clear_mastodon_memory() {
    *masto_session_lock().lock().await = None;
    *pending_app().lock().await = None;
}

// ═══════════════════════════════════════════════════
// P2: Mastodon Home Timeline
// ═══════════════════════════════════════════════════

/// Fetch the authenticated user's Mastodon home timeline.
#[tauri::command]
pub async fn mastodon_get_timeline(
    state: State<'_, AppState>,
    limit: Option<i32>,
    max_id: Option<String>,
) -> Result<serde_json::Value, String> {
    let session = get_active_session(&state).await?;

    let cache_key = format!(
        "mastodon_timeline:{}:{}",
        limit.unwrap_or(20),
        max_id.as_deref().unwrap_or("latest"),
    );

    // 30s cache TTL
    if let Some(cached) = state.feed_cache.get(&cache_key, std::time::Duration::from_secs(30)).await {
        return Ok(cached);
    }

    let items = mastodon::get_home_timeline(
        &state.http_client,
        &session,
        limit,
        max_id.as_deref(),
    ).await?;

    let json = serde_json::to_value(&items)
        .map_err(|e| format!("Serialize error: {}", e))?;
    state.feed_cache.set(&cache_key, json.clone()).await;
    Ok(json)
}

// ═══════════════════════════════════════════════════
// P3: AP Direct Messages
// ═══════════════════════════════════════════════════

/// Send a DM via Mastodon C2S API (visibility: "direct").
#[tauri::command]
pub async fn send_mastodon_dm(
    state: State<'_, AppState>,
    recipient_acct: String,
    text: String,
) -> Result<serde_json::Value, String> {
    let session = get_active_session(&state).await?;
    let dm_text = format!("@{} {}", recipient_acct, text);

    let status = mastodon::post_status(
        &state.http_client, &session, &dm_text, "direct", None,
    ).await?;

    let msg = mastodon::status_to_vault_message(&session.instance, &status, &session.acct);

    let db_guard = state.db_manager.read().await;
    if let Some(db) = db_guard.as_ref() {
        let _ = db.save_vault_message(&msg).await;
    }

    serde_json::to_value(&msg).map_err(|e| format!("Serialize error: {}", e))
}

/// Send a DM via AP S2S (signed Create(Note) to recipient inbox only).
#[tauri::command]
pub async fn send_ap_dm(
    state: State<'_, AppState>,
    recipient_actor_url: String,
    text: String,
) -> Result<serde_json::Value, String> {
    let (actor, owner_did) = get_local_ap_actor(&state).await?;

    let remote = crate::activitypub::outbox::fetch_remote_actor(
        &recipient_actor_url, Some(&actor),
    ).await?;

    let inbox = remote.get("inbox")
        .and_then(|v| v.as_str())
        .ok_or("Remote actor has no inbox URL")?
        .to_string();

    let recipient_name = remote.get("preferredUsername")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // Build Note addressed only to recipient (not as:Public = DM)
    let note_id = format!("{}/notes/{}", actor.id, uuid::Uuid::new_v4());
    let note = crate::activitypub::types::ApObject {
        id: note_id.clone(),
        object_type: "Note".to_string(),
        content: Some(text.clone()),
        source: None,
        attributed_to: Some(actor.id.clone()),
        to: Some(vec![recipient_actor_url.clone()]),
        cc: Some(vec![]),
        published: Some(chrono::Utc::now().to_rfc3339()),
        updated: None,
        attachment: None,
        tag: None,
        in_reply_to: None,
        url: Some(note_id.clone()),
        can_reply: None,
        can_search: None,
        expires_at: None,
    };

    let activity = crate::activitypub::activities::create_note(&actor.id, &note);
    crate::activitypub::outbox::deliver_activity(&actor, &activity, &inbox).await?;

    // Save locally as VaultMessage
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();

    let msg = crate::models::VaultMessage {
        id: uuid::Uuid::new_v4().to_string(),
        owner_did: Some(owner_did),
        convo_id: format!("ap:{}", recipient_actor_url),
        sender_did: actor.id.clone(),
        sender_handle: actor.preferred_username.clone(),
        text: text.clone(),
        sent_at: now.as_millis().to_string(),
        source: "activitypub".to_string(),
        imported_at: now.as_secs() as u64,
        peer_did: Some(recipient_actor_url),
        peer_handle: Some(recipient_name),
    };

    let db_guard = state.db_manager.read().await;
    if let Some(db) = db_guard.as_ref() {
        let _ = db.save_vault_message(&msg).await;
    }

    serde_json::to_value(&msg).map_err(|e| format!("Serialize error: {}", e))
}

// ═══════════════════════════════════════════════════
// P4: Unified send_message
// ═══════════════════════════════════════════════════

/// Protocol-agnostic message sender.
///
/// Dispatches to the correct protocol handler:
/// - `"bluesky"` → Bluesky chat DM (convo_id = Bluesky convo ID)
/// - `"mastodon"` → Mastodon C2S DM (convo_id = recipient acct)
/// - `"activitypub"` → AP S2S DM (convo_id = recipient actor URL)
/// - `"p2p"` → Iroh P2P message (convo_id = namespace ID)
#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    protocol: String,
    convo_id: String,
    text: String,
) -> Result<serde_json::Value, String> {
    match protocol.as_str() {
        "bluesky" => send_bluesky_dm_internal(&state, &convo_id, &text).await,
        "mastodon" => {
            let session = get_active_session(&state).await?;
            let dm_text = format!("@{} {}", convo_id, text);
            let status = mastodon::post_status(
                &state.http_client, &session, &dm_text, "direct", None,
            ).await?;
            let msg = mastodon::status_to_vault_message(&session.instance, &status, &session.acct);
            let db_guard = state.db_manager.read().await;
            if let Some(db) = db_guard.as_ref() {
                let _ = db.save_vault_message(&msg).await;
            }
            serde_json::to_value(&msg).map_err(|e| format!("Serialize error: {}", e))
        }
        "activitypub" => {
            let (actor, owner_did) = get_local_ap_actor(&state).await?;
            let remote = crate::activitypub::outbox::fetch_remote_actor(&convo_id, Some(&actor)).await?;
            let inbox = remote.get("inbox")
                .and_then(|v| v.as_str())
                .ok_or("Remote actor has no inbox")?
                .to_string();
            let recipient_name = remote.get("preferredUsername")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            let note_id = format!("{}/notes/{}", actor.id, uuid::Uuid::new_v4());
            let note = crate::activitypub::types::ApObject {
                id: note_id.clone(),
                object_type: "Note".to_string(),
                content: Some(text.clone()),
                source: None,
                attributed_to: Some(actor.id.clone()),
                to: Some(vec![convo_id.clone()]),
                cc: Some(vec![]),
                published: Some(chrono::Utc::now().to_rfc3339()),
                updated: None,
                attachment: None,
                tag: None,
                in_reply_to: None,
                url: Some(note_id),
                can_reply: None,
                can_search: None,
                expires_at: None,
            };
            let activity = crate::activitypub::activities::create_note(&actor.id, &note);
            crate::activitypub::outbox::deliver_activity(&actor, &activity, &inbox).await?;

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
            let msg = crate::models::VaultMessage {
                id: uuid::Uuid::new_v4().to_string(),
                owner_did: Some(owner_did),
                convo_id: format!("ap:{}", convo_id),
                sender_did: actor.id.clone(),
                sender_handle: actor.preferred_username.clone(),
                text: text.clone(),
                sent_at: now.as_millis().to_string(),
                source: "activitypub".to_string(),
                imported_at: now.as_secs() as u64,
                peer_did: Some(convo_id),
                peer_handle: Some(recipient_name),
            };
            let db_guard = state.db_manager.read().await;
            if let Some(db) = db_guard.as_ref() {
                let _ = db.save_vault_message(&msg).await;
            }
            serde_json::to_value(&msg).map_err(|e| format!("Serialize error: {}", e))
        }
        "p2p" => {
            let sync_guard = state.sync_manager.lock().await;
            let sync = sync_guard.as_ref()
                .ok_or("P2P sync not initialized")?;
            let namespace: iroh_docs::NamespaceId = convo_id.parse()
                .map_err(|e: anyhow::Error| format!("Invalid namespace ID: {}", e))?;
            sync.send_message(namespace, text).await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({"sent": true}))
        }
        _ => Err(format!(
            "Unknown protocol: {}. Expected: bluesky, mastodon, activitypub, p2p",
            protocol,
        )),
    }
}

// ═══════════════════════════════════════════════════
// P5: Follow / Unfollow
// ═══════════════════════════════════════════════════

/// Follow a Mastodon account via C2S API.
#[tauri::command]
pub async fn mastodon_follow(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<serde_json::Value, String> {
    let session = get_active_session(&state).await?;
    let rel = mastodon::follow_account(&state.http_client, &session, &account_id).await?;
    serde_json::to_value(&rel).map_err(|e| format!("Serialize error: {}", e))
}

/// Unfollow a Mastodon account via C2S API.
#[tauri::command]
pub async fn mastodon_unfollow(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<serde_json::Value, String> {
    let session = get_active_session(&state).await?;
    let rel = mastodon::unfollow_account(&state.http_client, &session, &account_id).await?;
    serde_json::to_value(&rel).map_err(|e| format!("Serialize error: {}", e))
}

/// Get the list of accounts the authenticated user follows.
#[tauri::command]
pub async fn mastodon_get_following(
    state: State<'_, AppState>,
) -> Result<Vec<crate::connectors::bluesky::ProfileView>, String> {
    let session = get_active_session(&state).await?;
    let accounts = mastodon::get_following_list(&state.http_client, &session, None).await?;
    Ok(accounts.iter().map(|a| mastodon::account_to_profile_view(&session.instance, a)).collect())
}

/// Get the list of accounts that follow the authenticated user.
#[tauri::command]
pub async fn mastodon_get_followers(
    state: State<'_, AppState>,
) -> Result<Vec<crate::connectors::bluesky::ProfileView>, String> {
    let session = get_active_session(&state).await?;
    let accounts = mastodon::get_followers_list(&state.http_client, &session, None).await?;
    Ok(accounts.iter().map(|a| mastodon::account_to_profile_view(&session.instance, a)).collect())
}

/// Follow a remote actor via AP S2S (signed Follow activity).
#[tauri::command]
pub async fn send_ap_follow(
    state: State<'_, AppState>,
    target_actor_url: String,
) -> Result<u16, String> {
    let (actor, _) = get_local_ap_actor(&state).await?;
    let remote = crate::activitypub::outbox::fetch_remote_actor(
        &target_actor_url, Some(&actor),
    ).await?;
    let inbox = remote.get("inbox")
        .and_then(|v| v.as_str())
        .ok_or("Remote actor has no inbox")?;
    let activity = crate::activitypub::activities::follow(&actor.id, &target_actor_url);
    crate::activitypub::outbox::deliver_activity(&actor, &activity, inbox).await
}

/// Unfollow a remote actor via AP S2S (signed Undo(Follow) activity).
#[tauri::command]
pub async fn send_ap_unfollow(
    state: State<'_, AppState>,
    target_actor_url: String,
) -> Result<u16, String> {
    let (actor, _) = get_local_ap_actor(&state).await?;
    let remote = crate::activitypub::outbox::fetch_remote_actor(
        &target_actor_url, Some(&actor),
    ).await?;
    let inbox = remote.get("inbox")
        .and_then(|v| v.as_str())
        .ok_or("Remote actor has no inbox")?;
    let follow_activity = crate::activitypub::activities::follow(&actor.id, &target_actor_url);
    let undo = crate::activitypub::activities::undo(&actor.id, &follow_activity);
    crate::activitypub::outbox::deliver_activity(&actor, &undo, inbox).await
}

// ═══════════════════════════════════════════════════
// Hashtag follow / unfollow (Mastodon C2S API)
// ═══════════════════════════════════════════════════

/// Follow a hashtag on Mastodon. Silently no-ops if no Mastodon session.
#[tauri::command]
pub async fn mastodon_follow_hashtag(
    state: State<'_, AppState>,
    hashtag: String,
) -> Result<(), String> {
    let session = match get_active_session(&state).await {
        Ok(s) => s,
        Err(_) => return Ok(()), // not connected — skip silently
    };
    mastodon::follow_hashtag(&state.http_client, &session, &hashtag)
        .await
        .map(|_| ())
}

/// Unfollow a hashtag on Mastodon. Silently no-ops if no Mastodon session.
#[tauri::command]
pub async fn mastodon_unfollow_hashtag(
    state: State<'_, AppState>,
    hashtag: String,
) -> Result<(), String> {
    let session = match get_active_session(&state).await {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };
    mastodon::unfollow_hashtag(&state.http_client, &session, &hashtag)
        .await
        .map(|_| ())
}

// ═══════════════════════════════════════════════════
// Mastodon Interactions (Favourite, Reblog, Reply)
// ═══════════════════════════════════════════════════

/// Favourite (like) a Mastodon status by its numeric ID.
#[tauri::command]
pub async fn mastodon_favourite(
    state: State<'_, AppState>,
    status_id: String,
) -> Result<serde_json::Value, String> {
    let session = get_active_session(&state).await?;
    let _status = mastodon::favourite_status(&state.http_client, &session, &status_id).await?;
    Ok(serde_json::json!({ "status_id": status_id }))
}

/// Unfavourite (unlike) a Mastodon status by its numeric ID.
#[tauri::command]
pub async fn mastodon_unfavourite(
    state: State<'_, AppState>,
    status_id: String,
) -> Result<serde_json::Value, String> {
    let session = get_active_session(&state).await?;
    let _status = mastodon::unfavourite_status(&state.http_client, &session, &status_id).await?;
    Ok(serde_json::json!({ "status_id": status_id }))
}

/// Reblog (repost) a Mastodon status by its numeric ID.
#[tauri::command]
pub async fn mastodon_reblog(
    state: State<'_, AppState>,
    status_id: String,
) -> Result<serde_json::Value, String> {
    let session = get_active_session(&state).await?;
    let _status = mastodon::reblog_status(&state.http_client, &session, &status_id).await?;
    Ok(serde_json::json!({ "status_id": status_id }))
}

/// Unreblog (undo repost) a Mastodon status by its numeric ID.
#[tauri::command]
pub async fn mastodon_unreblog(
    state: State<'_, AppState>,
    status_id: String,
) -> Result<serde_json::Value, String> {
    let session = get_active_session(&state).await?;
    let _status = mastodon::unreblog_status(&state.http_client, &session, &status_id).await?;
    Ok(serde_json::json!({ "status_id": status_id }))
}

/// Reply to a Mastodon status.
#[tauri::command]
pub async fn mastodon_reply(
    state: State<'_, AppState>,
    status_id: String,
    content: String,
) -> Result<serde_json::Value, String> {
    let session = get_active_session(&state).await?;
    let status = mastodon::post_status(
        &state.http_client,
        &session,
        &content,
        "public",
        Some(&status_id),
    ).await?;
    Ok(serde_json::json!({ "id": status.id, "url": status.url }))
}

// ═══════════════════════════════════════════════════
// P6: Unified Profile Resolver
// ═══════════════════════════════════════════════════

/// Resolve a profile by handle — auto-detects protocol.
///
/// - `user@instance.social` → Mastodon / AP WebFinger
/// - `did:plc:...` or `user.bsky.social` → Bluesky public API
#[tauri::command]
pub async fn resolve_profile(
    state: State<'_, AppState>,
    handle: String,
    protocol: Option<String>,
) -> Result<serde_json::Value, String> {
    let proto = protocol.unwrap_or_else(|| detect_protocol(&handle));

    match proto.as_str() {
        "activitypub" | "mastodon" => resolve_ap_profile(&state, &handle).await,
        "bluesky" | "atproto" => resolve_bluesky_profile(&state, &handle).await,
        _ => Err(format!("Unknown protocol: {}", proto)),
    }
}

// ═══════════════════════════════════════════════════
// Internal Helpers
// ═══════════════════════════════════════════════════

/// Get the active in-memory Mastodon session, restoring from keychain if needed.
async fn get_active_session(state: &AppState) -> Result<MastodonSession, String> {
    {
        let guard = masto_session_lock().lock().await;
        if let Some(ref s) = *guard {
            return Ok(s.clone());
        }
    }
    if let Some(session) = restore_from_keychain(state)? {
        *masto_session_lock().lock().await = Some(session.clone());
        return Ok(session);
    }
    Err("No Mastodon session — connect to a Mastodon instance first".to_string())
}

/// Restore Mastodon session from encrypted keychain.
fn restore_from_keychain(state: &AppState) -> Result<Option<MastodonSession>, String> {
    match state.keychain.get_mastodon_session() {
        Ok(Some(json_str)) => {
            let blob: serde_json::Value = serde_json::from_str(&json_str)
                .map_err(|e| format!("Parse error: {}", e))?;
            let session: MastodonSession = serde_json::from_value(
                blob.get("session").cloned().ok_or("Missing session key")?
            ).map_err(|e| format!("Deserialize error: {}", e))?;
            Ok(Some(session))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(format!("Keychain error: {}", e)),
    }
}

/// Get the local AP actor from DB.
async fn get_local_ap_actor(state: &AppState) -> Result<(crate::activitypub::ApActor, String), String> {
    let endpoint_guard = state.endpoint.lock().await;
    let endpoint = endpoint_guard.as_ref()
        .ok_or("Iroh node not initialized")?;
    let secret_key = endpoint.secret_key().clone();
    drop(endpoint_guard);

    let owner_did = crate::keychain_encrypted::EncryptedKeychain::did_key_from_ed25519_bytes(secret_key.public().as_bytes());

    let db_guard = state.db_manager.read().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;
    let actor = db.get_ap_actor(&owner_did).await?
        .ok_or("No local AP actor — create one first via create_ap_actor")?;
    Ok((actor, owner_did))
}

/// Detect protocol from handle format.
fn detect_protocol(handle: &str) -> String {
    if handle.starts_with("did:") || handle.ends_with(".bsky.social") || !handle.contains('@') {
        "bluesky".to_string()
    } else {
        "activitypub".to_string()
    }
}

/// Internal Bluesky DM sender for the unified send_message command.
///
/// Duplicates the essential logic of send_bsky_dm to avoid needing a
/// Tauri `State<'_>` re-extraction.
async fn send_bluesky_dm_internal(
    state: &AppState,
    convo_id: &str,
    content: &str,
) -> Result<serde_json::Value, String> {
    let session = state.keychain.load_session()
        .map_err(|e| format!("Keychain error: {}", e))?;
    let (user_did, user_handle) = session.ok_or("Not authenticated with Bluesky")?;

    if state.is_oauth_session() {
        let body = serde_json::json!({
            "convoId": convo_id,
            "message": { "text": content }
        });
        let resp = state.xrpc_chat_post("chat.bsky.convo.sendMessage", body).await
            .map_err(|e| format!("Failed to send DM: {}", e))?;
        let msg_id = resp.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();

        let db_guard = state.db_manager.read().await;
        if let Some(db) = db_guard.as_ref() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
            let vault_msg = crate::models::VaultMessage {
                id: msg_id.clone(),
                owner_did: Some(user_did.clone()),
                convo_id: convo_id.to_string(),
                sender_did: user_did,
                sender_handle: user_handle,
                text: content.to_string(),
                sent_at: now.as_millis().to_string(),
                source: "bluesky".to_string(),
                imported_at: now.as_secs() as u64,
                peer_did: None,
                peer_handle: None,
            };
            let _ = db.save_vault_message(&vault_msg).await;
        }
        return Ok(serde_json::json!({ "id": msg_id, "convo_id": convo_id, "sent": true }));
    }

    // App-password path
    let access_jwt = state.get_fresh_access_jwt().await
        .map_err(|e| format!("Auth error: {}", e))?;
    let msg = state.chat_client.send_dm(&access_jwt, &user_did, convo_id, content).await
        .map_err(|e| format!("Chat API error: {}", e))?;

    let db_guard = state.db_manager.read().await;
    if let Some(db) = db_guard.as_ref() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        let vault_msg = crate::models::VaultMessage {
            id: msg.id.clone(),
            owner_did: Some(user_did.clone()),
            convo_id: convo_id.to_string(),
            sender_did: user_did,
            sender_handle: user_handle,
            text: content.to_string(),
            sent_at: now.as_millis().to_string(),
            source: "bluesky".to_string(),
            imported_at: now.as_secs() as u64,
            peer_did: None,
            peer_handle: None,
        };
        let _ = db.save_vault_message(&vault_msg).await;
    }

    Ok(serde_json::json!({ "id": msg.id, "convo_id": convo_id, "sent": true }))
}

/// Resolve an ActivityPub/Mastodon profile.
async fn resolve_ap_profile(
    state: &AppState,
    handle: &str,
) -> Result<serde_json::Value, String> {
    // Try C2S lookup if we have a Mastodon session
    if let Ok(session) = get_active_session(state).await {
        if let Ok(account) = mastodon::lookup_account(&state.http_client, &session, handle).await {
            let instance_host = session.instance.replace("https://", "").replace("http://", "");
            let acct_uri = if account.acct.contains('@') {
                format!("acct:{}", account.acct)
            } else {
                format!("acct:{}@{}", account.acct, instance_host)
            };
            return Ok(serde_json::json!({
                "protocol": "activitypub",
                "did": acct_uri,
                "actor_url": account.url,
                "handle": account.acct,
                "display_name": account.display_name,
                "description": mastodon::strip_html(&account.note),
                "avatar": account.avatar,
                "banner": account.header,
                "followers_count": account.followers_count,
                "following_count": account.following_count,
                "posts_count": account.statuses_count,
                "account_id": account.id,
            }));
        }
    }

    // Fallback: WebFinger + fetch actor document
    let acct = if handle.starts_with("acct:") {
        handle.to_string()
    } else if handle.starts_with('@') {
        format!("acct:{}", &handle[1..])
    } else {
        format!("acct:{}", handle)
    };

    let webfinger = crate::activitypub::webfinger::resolve_webfinger(&state.http_client, &acct).await?;
    let actor_url = webfinger.links.iter()
        .find(|l| l.rel == "self" && l.link_type.as_deref() == Some("application/activity+json"))
        .and_then(|l| l.href.as_ref())
        .ok_or("WebFinger response missing self link")?;

    let actor_doc = crate::activitypub::outbox::fetch_remote_actor(actor_url, None).await?;

    Ok(serde_json::json!({
        "protocol": "activitypub",
        "actor_url": actor_doc.get("id").and_then(|v| v.as_str()).unwrap_or(""),
        "handle": handle,
        "display_name": actor_doc.get("name").and_then(|v| v.as_str()).unwrap_or(""),
        "description": actor_doc.get("summary").and_then(|v| v.as_str())
            .map(|s| mastodon::strip_html(s)).unwrap_or_default(),
        "avatar": actor_doc.get("icon").and_then(|v| v.get("url")).and_then(|v| v.as_str()).unwrap_or(""),
        "banner": actor_doc.get("image").and_then(|v| v.get("url")).and_then(|v| v.as_str()).unwrap_or(""),
        "followers_count": 0,
        "following_count": 0,
        "posts_count": 0,
    }))
}

/// Resolve a Bluesky profile via public API.
async fn resolve_bluesky_profile(
    state: &AppState,
    handle: &str,
) -> Result<serde_json::Value, String> {
    let url = format!(
        "https://public.api.bsky.app/xrpc/app.bsky.actor.getProfile?actor={}",
        urlencoding::encode(handle),
    );

    let resp = state.http_client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Bluesky profile fetch failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Bluesky profile fetch failed ({}): {}", status, body));
    }

    let p: serde_json::Value = resp.json().await
        .map_err(|e| format!("Invalid profile response: {}", e))?;

    Ok(serde_json::json!({
        "protocol": "bluesky",
        "did": p.get("did").and_then(|v| v.as_str()).unwrap_or(""),
        "handle": p.get("handle").and_then(|v| v.as_str()).unwrap_or(""),
        "display_name": p.get("displayName").and_then(|v| v.as_str()).unwrap_or(""),
        "description": p.get("description").and_then(|v| v.as_str()).unwrap_or(""),
        "avatar": p.get("avatar").and_then(|v| v.as_str()).unwrap_or(""),
        "banner": p.get("banner").and_then(|v| v.as_str()).unwrap_or(""),
        "followers_count": p.get("followersCount").and_then(|v| v.as_u64()).unwrap_or(0),
        "following_count": p.get("followsCount").and_then(|v| v.as_u64()).unwrap_or(0),
        "posts_count": p.get("postsCount").and_then(|v| v.as_u64()).unwrap_or(0),
    }))
}
