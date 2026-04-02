//! Authentication Commands
//!
//! User authentication lifecycle: login, session validation, local identity,
//! OAuth, logout/deletion, recovery, and linked-account management.
//!
//! # How It Works
//!
//! 1. **Session check (`check_auth_status`)** runs a 3-tier probe on app start:
//!    Bluesky session → local identity (with confirmed username) → Mastodon session.
//!    The first tier that succeeds becomes the active `AuthStatus`.
//! 2. **Login** can happen via AT Protocol credentials (`login`), Bluesky OAuth
//!    (`start_bluesky_oauth` → callback → `oauth-complete` event), or a fresh
//!    local identity (`create_local_identity` + `set_local_username`).
//! 3. **Identity layer** — every user gets a local Ed25519 keypair (auto-generated
//!    on first Bluesky/Mastodon login or explicitly via `create_local_identity`).
//!    A `did:web:bytescape.social:{username}` is minted once a username is confirmed.
//! 4. **Linked accounts** map the master identity to platform-specific sessions
//!    (Bluesky `did:plc`, Mastodon `acct:` URI). `unlink_account` removes tokens
//!    and flushes the feed cache.
//! 5. **Recovery** uses an escrow bundle + recovery code to restore the keypair
//!    on a new device (init → start → verify → finalize).
//!
//! # Tech Debt
//!
//! - **`logout` and `delete_account` are ~90% duplicated** — both clear vault,
//!   sync, pending imports, OAuth, Mastodon, and keychain. Should extract a shared
//!   `wipe_all_user_data` helper and have `logout`/`delete_account` call it.
//! - **DID generation uses `thread_rng`** (via `identity_init_full_recovery`) — fine
//!   for local use, but if the keypair ever signs federated proofs, consider a
//!   CSPRNG with explicit entropy source.
//!
//! # Related Files
//!
//! - [`keychain_encrypted.rs`] — identity storage, escrow, linked accounts
//! - [`auth.rs`] (crate root) — AT Protocol session management (`AuthManager`)
//! - [`oauth.rs`] — Bluesky OAuth PKCE + DPoP flow
//! - [`commands/mastodon.rs`] — Mastodon session helpers, `clear_mastodon_memory`
//! - [`error.rs`] — `BytescapeError` variants used for auth failures
//! - [`lib.rs`] — command registration, background session restore

use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

use crate::error::BytescapeError;
use crate::keychain_encrypted::{
    IdentityInitResult,
    IdentityRecoveryFinalizeResult,
    IdentityRecoveryStartResult,
    IdentityRecoveryStatus,
    IdentityRecoveryVerifyResult,
    LinkedAccount,
};
use crate::AppState;

// ═══════════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════════

/// Authentication status response returned to the frontend.
///
/// Encapsulates the resolved identity regardless of how the user authenticated
/// (Bluesky, local, or Mastodon). The `source` field tells the UI which
/// login path was taken.
#[derive(Serialize, Deserialize, Clone)]
pub struct AuthStatus {
    pub handle: String,
    pub did: String,
    /// "bluesky" | "local" | "mastodon" — how the user authenticated
    #[serde(default = "default_source")]
    pub source: String,
    /// The user's chosen Bytescape-local username (e.g. "lovingisliving")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_username: Option<String>,
    /// Whether the local identity setup step has been completed
    #[serde(default)]
    pub identity_ready: bool,
    /// Master identity DID (did:web:bytescape.social:{username}), if set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_did: Option<String>,
}

fn default_source() -> String { "bluesky".to_string() }

// ═══════════════════════════════════════════════════════════════════════════════
// Auth Status
// ═══════════════════════════════════════════════════════════════════════════════

/// Check if the user has an active authenticated session (3-tier probe).
///
/// Probes in order: **Bluesky → Local identity → Mastodon**. Returns the first
/// tier that succeeds as an `AuthStatus`, or `None` if no session exists.
///
/// For Bluesky OAuth sessions, eagerly restores the token so that subsequent
/// API calls (`getTimeline`, etc.) work immediately without waiting for the
/// background restore task.
///
/// # Returns
/// - `Some(AuthStatus)` — user is logged in; includes handle, DID, source, and web_did.
/// - `None` — no active session found; frontend should show the login screen.
///
/// # Side Effects
/// - May trigger OAuth token restore (once per app lifetime).
/// - May mint a `did:web` if one doesn't exist yet.
/// - For Mastodon-only users, may auto-initialize local identity infrastructure.
///
/// # IPC
/// Tauri command — called by the frontend `AuthProvider` on mount via
/// `invoke('check_auth_status')`.
#[tauri::command]
pub async fn check_auth_status(state: State<'_, AppState>) -> std::result::Result<Option<AuthStatus>, String> {
    // 1. Check for Bluesky session first
    let session = state.keychain.load_session()
        .map_err(|e| BytescapeError::Keychain(e.to_string()))?;

    if let Some((did, handle)) = session {
        // Eagerly restore the OAuth session so that authenticated API calls
        // (getTimeline, getAuthorFeed, etc.) work immediately after the
        // frontend receives this auth status. Without this, the background
        // restore task may not have completed yet, causing "No OAuth session"
        // errors on the first feed fetch.
        if state.is_oauth_session() {
            // Guard: only restore once per app lifetime. The background task
            // in lib.rs also calls restore_session; this prevents redundant
            // (and potentially racy) duplicate restores.
            if !state.session_restored.swap(true, std::sync::atomic::Ordering::SeqCst) {
                match state.oauth_manager.restore_session(&did).await {
                    Ok(()) => {}
                    Err(e) => {
                        eprintln!("check_auth_status: OAuth restore failed (will retry in background): {}", e);
                        // Reset guard so the background task can retry
                        state.session_restored.store(false, std::sync::atomic::Ordering::SeqCst);
                    }
                }
            }
        }

        let local_username = state.keychain.get_local_username()
            .unwrap_or(None);
        let identity_ready = local_username.is_some();
        let web_did = state.keychain.get_web_did().unwrap_or(None)
            .or_else(|| state.keychain.ensure_web_did().ok().flatten());
        return Ok(Some(AuthStatus {
            handle,
            did,
            source: "bluesky".to_string(),
            local_username,
            identity_ready,
            web_did,
        }));
    }

    // 2. Check for local-only identity
    //
    // Only treat a local identity as "logged in" if the user has explicitly
    // confirmed their username. An auto-generated identity (created silently
    // during a Bluesky login) should not auto-log in — otherwise logging out
    // of Bluesky and trying OAuth will just bounce back to the local account.
    let recovery_status = state.keychain.get_identity_recovery_status()
        .map_err(|e| BytescapeError::Keychain(e.to_string()))?;

    if recovery_status.has_local_identity {
        if let Some(did) = recovery_status.did {
            let local_username = state.keychain.get_local_username()
                .unwrap_or(None);
            // Only auto-login if the user explicitly set a local username
            if local_username.is_some() {
                let web_did = state.keychain.get_web_did().unwrap_or(None)
                    .or_else(|| state.keychain.ensure_web_did().ok().flatten());
                let handle = local_username.clone()
                    .unwrap_or_else(|| format!("local:{}", &did[did.len().saturating_sub(8)..]));                return Ok(Some(AuthStatus {
                    handle,
                    did,
                    source: "local".to_string(),
                    local_username: local_username.clone(),
                    identity_ready: true,
                    web_did,
                }));
            }
        }
    }

    // 3. Check for Mastodon-only session
    //
    // If the user connected a Mastodon account but has no Bluesky or named local
    // identity, still recognize them as logged in so the app works on restart.
    if let Ok(Some(json_str)) = state.keychain.get_mastodon_session() {
        if let Ok(blob) = serde_json::from_str::<serde_json::Value>(&json_str) {
            if let Some(session_val) = blob.get("session") {
                let acct = session_val.get("acct").and_then(|v| v.as_str()).unwrap_or("unknown");
                let instance = session_val.get("instance").and_then(|v| v.as_str()).unwrap_or("");
                let instance_host = instance.replace("https://", "").replace("http://", "");

                // Ensure identity infrastructure exists for this Mastodon-only user
                let recovery_status_2 = state.keychain.get_identity_recovery_status()
                    .unwrap_or(IdentityRecoveryStatus {
                        has_local_identity: false, has_escrow: false,
                        recovery_pending: false, recovery_verified_pending_finalize: false,
                        did: None, public_key: None, last_recovered_at: None,
                    });
                if !recovery_status_2.has_local_identity {
                    let _ = state.keychain.identity_init_full_recovery("bytescape-mastodon-auto-init");
                }
                let _ = state.keychain.ensure_master_did();

                // Ensure linked account exists — use acct: URI as canonical identifier
                let acct_uri = format!("acct:{}@{}", acct, instance_host);
                // Upsert — idempotent, won't duplicate if already there
                let linked = crate::keychain_encrypted::LinkedAccount {
                    protocol: "mastodon".to_string(),
                    identifier: acct_uri.clone(),
                    platform_identifier: Some(acct_uri.clone()),
                    display_handle: format!("@{}@{}", acct, instance_host),
                    display_name: None,
                    avatar_url: None,
                    connected_at: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    active: true,
                };
                let _ = state.keychain.add_linked_account(linked);

                // get_owner_did returns web_did > bluesky did:plc > did:key
                let owner_did = state.keychain.get_owner_did()
                    .unwrap_or(None)
                    .unwrap_or_else(|| acct_uri);
                let local_username = state.keychain.get_local_username().unwrap_or(None);
                let identity_ready = local_username.is_some();
                let web_did = state.keychain.get_web_did().unwrap_or(None)
                    .or_else(|| state.keychain.ensure_web_did().ok().flatten());

                return Ok(Some(AuthStatus {
                    handle: format!("{}@{}", acct, instance_host),
                    did: owner_did,
                    source: "mastodon".to_string(),
                    local_username,
                    identity_ready,
                    web_did,
                }));
            }
        }
    }

    Ok(None)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Login
// ═══════════════════════════════════════════════════════════════════════════════

/// Log in with AT Protocol credentials (handle + app password).
///
/// Authenticates against the user's PDS, stores session tokens in the keychain,
/// registers a `LinkedAccount` for the Bluesky connection, and auto-generates a
/// local identity keypair if one doesn't exist yet.
///
/// # Parameters
/// - `identifier` — Bluesky handle (e.g. `alice.bsky.social`) or DID
/// - `password` — App password (not the main account password)
///
/// # Returns
/// `AuthStatus` with the Bluesky handle, DID, and identity readiness.
///
/// # Side Effects
/// - Saves session tokens to the OS keychain
/// - Creates a `LinkedAccount` entry for Bluesky
/// - Auto-generates local Ed25519 keypair if missing
///
/// # IPC
/// Tauri command — called from the login screen via
/// `invoke('login', { identifier, password })`.
#[tauri::command]
pub async fn login(
    identifier: String,
    password: String,
    state: State<'_, AppState>,
) -> std::result::Result<AuthStatus, String> {
    let session = state.auth_manager.login(identifier, password).await
        .map_err(|e| BytescapeError::AuthFailed(e))?;

    state.keychain.save_session(
        session.did.clone(),
        session.handle.clone(),
        session.access_jwt.clone(),
        session.refresh_jwt.clone()
    ).map_err(|e| BytescapeError::KeychainSave(e.to_string()))?;

    // Create a LinkedAccount entry for this Bluesky connection
    // Bluesky accounts use their existing did:plc directly
    let linked = LinkedAccount {
        protocol: "bluesky".to_string(),
        identifier: session.did.clone(),
        platform_identifier: Some(session.did.clone()),
        display_handle: format!("@{}", session.handle),
        display_name: None,
        avatar_url: None,
        connected_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        active: true,
    };
    let _ = state.keychain.add_linked_account(linked);

    // Auto-generate local identity if one doesn't exist yet
    let recovery_status = state.keychain.get_identity_recovery_status()
        .unwrap_or(IdentityRecoveryStatus {
            has_local_identity: false, has_escrow: false,
            recovery_pending: false, recovery_verified_pending_finalize: false,
            did: None, public_key: None, last_recovered_at: None,
        });
    if !recovery_status.has_local_identity {
        // Generate keypair silently — user will confirm username in the identity setup step
        let _ = state.keychain.identity_init_full_recovery("bytescape-auto-init");
    }

    let local_username = state.keychain.get_local_username().unwrap_or(None);
    let identity_ready = local_username.is_some();
    let web_did = state.keychain.get_web_did().unwrap_or(None)
        .or_else(|| state.keychain.ensure_web_did().ok().flatten());

    Ok(AuthStatus {
        handle: session.handle,
        did: session.did,
        source: "bluesky".to_string(),
        local_username,
        identity_ready,
        web_did,
    })
}

// ═══════════════════════════════════════════════════════════════════════════════
// Local Identity
// ═══════════════════════════════════════════════════════════════════════════════

/// Create a local-only identity without any external service.
///
/// Generates an Ed25519 keypair and a `did:key` identifier via the escrow
/// recovery system. The user can claim a username in the subsequent
/// `set_local_username` step, which mints their `did:web`.
///
/// # Returns
/// `AuthStatus` with `source: "local"` and `identity_ready: false` (username not yet set).
///
/// # Side Effects
/// - Generates and stores a keypair + escrow bundle in the keychain.
///
/// # IPC
/// Tauri command — called from the onboarding screen via
/// `invoke('create_local_identity')`.
#[tauri::command]
pub async fn create_local_identity(
    state: State<'_, AppState>,
) -> std::result::Result<AuthStatus, String> {
    // Generate keypair + escrow
    let init_result = state.keychain.identity_init_full_recovery("bytescape-local-init")
        .map_err(|e| BytescapeError::Keychain(e.to_string()))?;

    let local_username = state.keychain.get_local_username().unwrap_or(None);
    let handle = local_username.clone()
        .unwrap_or_else(|| format!("local:{}", &init_result.did[init_result.did.len().saturating_sub(8)..]));

    Ok(AuthStatus {
        handle,
        did: init_result.did,
        source: "local".to_string(),
        local_username,
        identity_ready: false, // user hasn't confirmed username yet
        web_did: None, // no username yet, so no web_did
    })
}

/// Set or update the user's local Bytescape username.
///
/// Validates the username (2–32 chars, alphanumeric + hyphens/underscores),
/// persists it in the keychain, and returns the updated `AuthStatus`.
/// This is the universal overarching identity handle displayed across the app
/// and used to mint the `did:web:bytescape.social:{username}`.
///
/// # Parameters
/// - `username` — Desired username (will be lowercased and trimmed)
///
/// # Returns
/// Updated `AuthStatus` with `identity_ready: true` and the new `web_did`.
///
/// # IPC
/// Tauri command — called from the identity setup screen via
/// `invoke('set_local_username', { username })`.
#[tauri::command]
pub async fn set_local_username(
    state: State<'_, AppState>,
    username: String,
) -> std::result::Result<AuthStatus, String> {
    let trimmed = username.trim().to_lowercase();
    if trimmed.is_empty() {
        return Err(BytescapeError::InvalidInput("Username cannot be empty".to_string()).into());
    }
    if trimmed.len() < 2 {
        return Err(BytescapeError::InvalidInput("Username must be at least 2 characters".to_string()).into());
    }
    if trimmed.len() > 32 {
        return Err(BytescapeError::InvalidInput("Username must be 32 characters or fewer".to_string()).into());
    }
    // Allow alphanumeric, hyphens, underscores
    if !trimmed.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err(BytescapeError::InvalidInput("Username may only contain letters, numbers, hyphens, and underscores".to_string()).into());
    }

    state.keychain.set_local_username(&trimmed)
        .map_err(|e| BytescapeError::KeychainSave(e.to_string()))?;

    // Return the updated auth status
    check_auth_status(state).await.map(|opt| opt.unwrap_or(AuthStatus {
        handle: trimmed.clone(),
        did: String::new(),
        source: "local".to_string(),
        local_username: Some(trimmed.clone()),
        identity_ready: true,
        web_did: Some(format!("did:web:bytescape.social:{}", trimmed)),
    }))
}

// ═══════════════════════════════════════════════════════════════════════════════
// Logout / Delete
// ═══════════════════════════════════════════════════════════════════════════════

/// Log out and clear the stored session.
///
/// Performs a full wipe: vault DB, sync files, pending imports, OAuth/Mastodon
/// sessions, and all keychain identity data. After logout the user must go
/// through the complete onboarding flow again.
///
/// # Tech Debt
/// Nearly identical to `delete_account` — both should call a shared
/// `wipe_all_user_data()` helper to avoid duplication.
///
/// # Side Effects
/// - Clears all `BytescapeItem` records from the vault DB
/// - Removes all Iroh blob/sync files
/// - Stops the OAuth callback server
/// - Deletes all keychain entries (DIDs, sessions, linked accounts)
///
/// # IPC
/// Tauri command — called from the settings screen via `invoke('logout')`.
#[tauri::command]
pub async fn logout(state: State<'_, AppState>) -> std::result::Result<(), String> {
    {
        let db_guard = state.db_manager.read().await;
        if let Some(db) = db_guard.as_ref() {
            let _cleared = db.clear_all_user_data().await
                .map_err(BytescapeError::DatabaseTransaction)?;
        }
    }

    {
        let sync_guard = state.sync_manager.lock().await;
        if let Some(sync) = sync_guard.as_ref() {
            let _ = sync.vault_clear_all_files().await;
        }
    }

    {
        let mut pending = state.pending_import.lock().await;
        *pending = None;
    }

    // Clear OAuth session if active
    state.oauth_manager.clear_session().await;
    state.oauth_manager.stop_callback_server().await;

    // Clear in-memory Mastodon session
    super::mastodon::clear_mastodon_memory().await;

    // Nuclear wipe of ALL keychain identity data
    state.keychain.delete_all_identity_data()
        .map_err(|e| BytescapeError::Keychain(e.to_string()))?;
    Ok(())
}

/// **Delete Account** — permanently wipe ALL user data.
///
/// This is the nuclear option. It clears:
/// - All vault items (DB + sync files)
/// - All keychain identity data (DIDs, usernames, linked accounts, sessions)
/// - In-memory Mastodon session
/// - OAuth session
/// - Pending imports
///
/// After this, the user is as if they freshly installed the app.
///
/// # Tech Debt
/// ~90% identical to `logout` — should share a `wipe_all_user_data()` helper.
///
/// # IPC
/// Tauri command — called from the settings screen via `invoke('delete_account')`.
#[tauri::command]
pub async fn delete_account(state: State<'_, AppState>) -> std::result::Result<(), String> {
    // 1. Clear all vault data from the database
    {
        let db_guard = state.db_manager.read().await;
        if let Some(db) = db_guard.as_ref() {
            let _cleared = db.clear_all_user_data().await
                .map_err(BytescapeError::DatabaseTransaction)?;
        }
    }

    // 2. Clear all vault files from sync/blob storage
    {
        let sync_guard = state.sync_manager.lock().await;
        if let Some(sync) = sync_guard.as_ref() {
            let _ = sync.vault_clear_all_files().await;
        }
    }

    // 3. Clear pending imports
    {
        let mut pending = state.pending_import.lock().await;
        *pending = None;
    }

    // 4. Clear OAuth session
    state.oauth_manager.clear_session().await;
    state.oauth_manager.stop_callback_server().await;

    // 5. Clear in-memory Mastodon session
    super::mastodon::clear_mastodon_memory().await;

    // 6. Nuclear wipe of ALL keychain identity data
    //    (DIDs, usernames, linked accounts, Mastodon session, recovery data, tokens)
    state.keychain.delete_all_identity_data()
        .map_err(|e| BytescapeError::Keychain(e.to_string()))?;

    println!("delete_account: Full account wipe complete");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Recovery
// ═══════════════════════════════════════════════════════════════════════════════

/// Initialize full local identity + escrow recovery flow.
///
/// Generates an Ed25519 keypair and creates an escrow bundle encrypted with
/// the provided recovery code. This is the first step of identity creation.
///
/// # Parameters
/// - `recovery_code` — User-chosen passphrase to encrypt the escrow bundle
///
/// # Returns
/// `IdentityInitResult` containing the new DID and public key.
///
/// # IPC
/// Tauri command — called from the identity setup screen via
/// `invoke('identity_init_full_recovery', { recoveryCode })`.
#[tauri::command]
pub async fn identity_init_full_recovery(
    state: State<'_, AppState>,
    recovery_code: String,
) -> std::result::Result<IdentityInitResult, String> {
    state
        .keychain
        .identity_init_full_recovery(&recovery_code)
        .map_err(|e| e.to_string())
}

/// Get local identity and recovery lifecycle status.
///
/// Returns flags indicating whether a local identity exists, whether an escrow
/// bundle is stored, and the current state of any in-progress recovery.
///
/// # Returns
/// `IdentityRecoveryStatus` with booleans for each lifecycle stage.
///
/// # IPC
/// Tauri command — called from the settings/recovery screen via
/// `invoke('identity_get_recovery_status')`.
#[tauri::command]
pub async fn identity_get_recovery_status(
    state: State<'_, AppState>,
) -> std::result::Result<IdentityRecoveryStatus, String> {
    state
        .keychain
        .get_identity_recovery_status()
        .map_err(|e| e.to_string())
}

/// Start a recovery session from an escrow bundle.
///
/// Parses the escrow bundle and prepares a recovery session. The user must
/// then call `identity_verify_recovery` with the correct recovery code.
///
/// # Parameters
/// - `escrow_bundle` — Base64-encoded escrow data (exported from original device)
///
/// # Returns
/// `IdentityRecoveryStartResult` indicating the recovery session is ready.
///
/// # IPC
/// Tauri command — called from the recovery screen via
/// `invoke('identity_start_recovery', { escrowBundle })`.
#[tauri::command]
pub async fn identity_start_recovery(
    state: State<'_, AppState>,
    escrow_bundle: String,
) -> std::result::Result<IdentityRecoveryStartResult, String> {
    state
        .keychain
        .start_recovery_from_bundle(&escrow_bundle)
        .map_err(|e| e.to_string())
}

/// Verify a recovery code against the current pending recovery session.
///
/// Decrypts the escrow bundle with the provided code. If correct, the
/// recovery session advances to `verified_pending_finalize` state.
///
/// # Parameters
/// - `recovery_code` — The passphrase used during `identity_init_full_recovery`
///
/// # Returns
/// `IdentityRecoveryVerifyResult` indicating success or failure.
///
/// # IPC
/// Tauri command — called from the recovery screen via
/// `invoke('identity_verify_recovery', { recoveryCode })`.
#[tauri::command]
pub async fn identity_verify_recovery(
    state: State<'_, AppState>,
    recovery_code: String,
) -> std::result::Result<IdentityRecoveryVerifyResult, String> {
    state
        .keychain
        .verify_recovery_code(&recovery_code)
        .map_err(|e| e.to_string())
}

/// Finalize a verified recovery and promote it to active identity.
///
/// Takes the decrypted keypair from the verified recovery session and
/// installs it as the active local identity, replacing any existing one.
///
/// # Returns
/// `IdentityRecoveryFinalizeResult` confirming the restored DID.
///
/// # IPC
/// Tauri command — called from the recovery screen via
/// `invoke('identity_finalize_recovery')`.
#[tauri::command]
pub async fn identity_finalize_recovery(
    state: State<'_, AppState>,
) -> std::result::Result<IdentityRecoveryFinalizeResult, String> {
    state
        .keychain
        .finalize_recovery()
        .map_err(|e| e.to_string())
}

// ═══════════════════════════════════════════════════════════════════════════════
// OAuth
// ═══════════════════════════════════════════════════════════════════════════════

/// Start the Bluesky OAuth login flow.
///
/// Resolves the user's handle, initiates PKCE + DPoP authorization,
/// starts a local callback server, and returns the authorization URL
/// to be opened in the system browser.
///
/// The callback server handles the token exchange and emits an
/// `oauth-complete` event when authentication succeeds.
///
/// # Parameters
/// - `handle` — The user's Bluesky handle (e.g. `alice.bsky.social`)
///
/// # Returns
/// The authorization URL to open in the system browser.
///
/// # IPC
/// Tauri command — called from the login screen via
/// `invoke('start_bluesky_oauth', { handle })`.
#[tauri::command]
pub async fn start_bluesky_oauth(
    handle: String,
    state: State<'_, AppState>,
) -> std::result::Result<String, String> {
    let trimmed = handle.trim().to_string();
    if trimmed.is_empty() {
        return Err("Handle cannot be empty".to_string());
    }

    let app_handle = state.app_handle.clone();
    let url = state.oauth_manager.start_auth(&trimmed, app_handle).await?;

    Ok(url)
}

/// Cancel an in-progress OAuth login flow.
///
/// Stops the callback server if it's running. Safe to call even if
/// no OAuth flow is in progress.
///
/// # IPC
/// Tauri command — called from the login screen via `invoke('cancel_bluesky_oauth')`.
#[tauri::command]
pub async fn cancel_bluesky_oauth(
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    state.oauth_manager.stop_callback_server().await;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Linked Accounts
// ═══════════════════════════════════════════════════════════════════════════════

/// List all platform accounts linked to the user's master identity.
///
/// # Returns
/// Vec of `LinkedAccount` entries (Bluesky, Mastodon, etc.) with connection metadata.
///
/// # IPC
/// Tauri command — called from the settings screen via `invoke('list_linked_accounts')`.
#[tauri::command]
pub async fn list_linked_accounts(
    state: State<'_, AppState>,
) -> Result<Vec<LinkedAccount>, String> {
    state.keychain.list_linked_accounts()
        .map_err(|e| format!("Failed to list linked accounts: {}", e))
}

/// Unlink a platform account from the user's master identity.
///
/// Removes the `LinkedAccount` entry, clears protocol-specific session tokens
/// (Mastodon keychain + memory, or Bluesky OAuth + keychain), flushes the
/// entire feed cache, and emits an `account-unlinked` event to the frontend.
///
/// # Parameters
/// - `protocol` — `"mastodon"` or `"bluesky"`
/// - `identifier` — The platform identifier (DID for Bluesky, `acct:` URI for Mastodon)
///
/// # Returns
/// The updated list of remaining `LinkedAccount` entries.
///
/// # Side Effects
/// - Clears session tokens from keychain and memory
/// - Flushes all feed cache entries
/// - Emits `account-unlinked` Tauri event
///
/// # IPC
/// Tauri command — called from the settings screen via
/// `invoke('unlink_account', { protocol, identifier })`.
#[tauri::command]
pub async fn unlink_account(
    state: State<'_, AppState>,
    protocol: String,
    identifier: String,
) -> Result<Vec<LinkedAccount>, String> {
    // Remove the linked account entry
    state.keychain.remove_linked_account(&protocol, &identifier)
        .map_err(|e| format!("Failed to remove linked account: {}", e))?;

    // Clear protocol-specific session data
    match protocol.as_str() {
        "mastodon" => {
            state.keychain.clear_mastodon_session()
                .map_err(|e| format!("Failed to clear Mastodon session: {}", e))?;
            // Clear in-memory Mastodon session
            super::mastodon::clear_mastodon_memory().await;
        }
        "bluesky" => {
            // Clear OAuth session
            state.oauth_manager.clear_session().await;
            state.oauth_manager.stop_callback_server().await;
            // Clear Bluesky session tokens from keychain
            let _ = state.keychain.delete_session();
        }
        _ => {}
    }

    // Flush the entire feed cache so stale data from the disconnected
    // account doesn't keep appearing in the UI.
    state.feed_cache.clear_all().await;

    // Notify the frontend so it can refresh its feed/friends views
    let _ = state.app_handle.emit("account-unlinked", &protocol);

    // Return updated list
    state.keychain.list_linked_accounts()
        .map_err(|e| format!("Failed to list linked accounts: {}", e))
}
