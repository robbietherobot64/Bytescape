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
#[derive(Serialize, Deserialize)]
pub struct AuthStatus {
    pub handle: String,
    pub did: String,
}

/// Check if the user has an active authenticated session.
///
/// Returns the user's handle and DID if logged in, None otherwise.
#[tauri::command]
pub async fn check_auth_status(state: State<'_, AppState>) -> std::result::Result<Option<AuthStatus>, String> {
    let session = state.keychain.load_session()
        .map_err(|e| BytescapeError::Keychain(e.to_string()))?;
    Ok(session.map(|(did, handle)| AuthStatus { handle, did }))
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

    Ok(AuthStatus { handle: session.handle, did: session.did })
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
