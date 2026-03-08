//! Authentication Commands
//!
//! Commands for user authentication, session management, and keychain operations.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::BytescapeError;
use crate::keychain_encrypted::{
    IdentityInitResult,
    IdentityRecoveryFinalizeResult,
    IdentityRecoveryStartResult,
    IdentityRecoveryStatus,
    IdentityRecoveryVerifyResult,
};
use crate::AppState;

/// Authentication status response
#[derive(Serialize, Deserialize, Clone)]
pub struct AuthStatus {
    pub handle: String,
    pub did: String,
    /// "bluesky" | "local" — how the user authenticated
    #[serde(default = "default_source")]
    pub source: String,
    /// The user's chosen Bytescape-local username (e.g. "lovingisliving")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_username: Option<String>,
    /// Whether the local identity setup step has been completed
    #[serde(default)]
    pub identity_ready: bool,
}

fn default_source() -> String { "bluesky".to_string() }

/// Check if the user has an active authenticated session.
///
/// Returns the user's handle and DID if logged in via Bluesky,
/// or if a local-only identity exists.
#[tauri::command]
pub async fn check_auth_status(state: State<'_, AppState>) -> std::result::Result<Option<AuthStatus>, String> {
    // 1. Check for Bluesky session first
    let session = state.keychain.load_session()
        .map_err(|e| BytescapeError::Keychain(e.to_string()))?;

    if let Some((did, handle)) = session {
        let local_username = state.keychain.get_local_username()
            .unwrap_or(None);
        let identity_ready = local_username.is_some();
        return Ok(Some(AuthStatus {
            handle,
            did,
            source: "bluesky".to_string(),
            local_username,
            identity_ready,
        }));
    }

    // 2. Check for local-only identity
    let recovery_status = state.keychain.get_identity_recovery_status()
        .map_err(|e| BytescapeError::Keychain(e.to_string()))?;

    if recovery_status.has_local_identity {
        if let Some(did) = recovery_status.did {
            let local_username = state.keychain.get_local_username()
                .unwrap_or(None);
            let handle = local_username.clone()
                .unwrap_or_else(|| format!("local:{}", &did[did.len().saturating_sub(8)..]));
            return Ok(Some(AuthStatus {
                handle,
                did,
                source: "local".to_string(),
                local_username: local_username.clone(),
                identity_ready: local_username.is_some(),
            }));
        }
    }

    Ok(None)
}

/// Log in with credentials and store the session.
///
/// Authenticates with the AT Protocol server and stores the session
/// tokens in the keychain for future use.
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

    Ok(AuthStatus {
        handle: session.handle,
        did: session.did,
        source: "bluesky".to_string(),
        local_username,
        identity_ready,
    })
}

/// Create a local-only identity without any external service.
///
/// Generates an Iroh keypair and a local DID. The user can set their
/// username in the subsequent identity setup step.
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
    })
}

/// Set or update the user's local Bytescape username.
///
/// This is the universal overarching identity handle used across the app.
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
        local_username: Some(trimmed),
        identity_ready: true,
    }))
}

/// Log out and clear the stored session.
///
/// Removes all authentication tokens from the keychain.
#[tauri::command]
pub async fn logout(state: State<'_, AppState>) -> std::result::Result<(), String> {
    {
        let db_guard = state.db_manager.lock().await;
        if let Some(db) = db_guard.as_ref() {
            let _cleared = db.clear_all_user_data().await
                .map_err(BytescapeError::DatabaseTransaction)?;
        }
    }

    {
        let sync_guard = state.sync_manager.lock().await;
        if let Some(sync) = sync_guard.as_ref() {
            sync.vault_clear_all_files().await
                .map_err(BytescapeError::Sync)?;
        }
    }

    {
        let mut pending = state.pending_import.lock().await;
        *pending = None;
    }

    state.keychain.delete_session()
        .map_err(|e| BytescapeError::Keychain(e.to_string()))?;
    Ok(())
}

/// Initialize full local identity + escrow recovery flow.
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
#[tauri::command]
pub async fn identity_finalize_recovery(
    state: State<'_, AppState>,
) -> std::result::Result<IdentityRecoveryFinalizeResult, String> {
    state
        .keychain
        .finalize_recovery()
        .map_err(|e| e.to_string())
}
