//! Identity & DID — serves the account's real DID document.
//!
//! `GET /api/identity/did` answers the same document the wildcard-subdomain
//! well-known serves ([`crate::pds::did_doc::account_did_doc`]): the account's
//! STORED DID (a minted `did:plc` since 5.4; pre-existing `did:web` rows keep
//! resolving) with its real signing key. The old fabricator that derived
//! `did:web:{username}.{domain}` around an unrelated signup key is gone —
//! publishing an identity the mint no longer creates is exactly the mismatch
//! class this change retires.

use crate::pds::account::PdsAccount;
use crate::pds::did_doc::account_did_doc;

/// axum handler: GET /api/identity/did?username=… — the account's DID document.
/// Requires authentication (JWT in Authorization header).
pub async fn get_did_handler(
    axum::extract::State(state): axum::extract::State<crate::AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<axum::Json<serde_json::Value>, crate::error::AppError> {
    let username = params.get("username")
        .ok_or_else(|| crate::error::AppError::BadRequest("username query parameter required".into()))?;

    crate::db::users::UserRow::find_by_username(&state.db, username)
        .await?
        .ok_or_else(|| crate::error::AppError::NotFound(format!("User {}", username)))?;

    // The stored PDS account IS the identity — no account row (mint failed or
    // never ran) means no AT identity to document; an honest 404 beats a
    // fabricated document.
    let account = PdsAccount::load_by_handle(&state.db, username)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("Failed to load account: {e}")))?
        .ok_or_else(|| crate::error::AppError::NotFound(format!("no AT identity for {username}")))?;

    let host = format!("{}.{}", username, state.config.domain);
    Ok(axum::Json(account_did_doc(&account, &host, &state.config.domain)))
}
