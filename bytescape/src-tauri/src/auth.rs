//! # auth.rs — Bluesky App-Password Authentication
//!
//! This module handles the **legacy** authentication method for Bluesky: app passwords.
//! When a user logs in with an app password (instead of OAuth), this code creates and
//! refreshes AT Protocol sessions via the `bsky.social` XRPC API.
//!
//! ## How It Works
//! 1. User enters their Bluesky handle + app password in the login screen
//! 2. `AuthManager::login()` sends credentials to `com.atproto.server.createSession`
//! 3. Bluesky returns a `Session` with JWT access/refresh tokens and the user's DID
//! 4. Tokens are stored in the encrypted keychain (`keychain_encrypted.rs`)
//! 5. When the access token expires (~2 hours), `refresh_session()` uses the
//!    refresh token to get a new access token without re-entering the password
//!
//! ## Auth Modes in Bytescape
//! Bytescape supports two authentication modes:
//! - **App-password** (this module): Simple JWT bearer tokens, direct API calls
//! - **OAuth** (`oauth.rs`): Modern PKCE + DPoP flow, browser-based, recommended
//!
//! The active mode is detected by checking if the access JWT equals the sentinel
//! value `"__OAUTH_SESSION__"` (see `lib.rs::is_oauth_session()`).
//!
//! ## Related Files
//! - `oauth.rs` — The preferred OAuth authentication flow
//! - `keychain_encrypted.rs` — Where session tokens are stored (encrypted on disk)
//! - `lib.rs` — `xrpc_get()` / `xrpc_post_json()` which route API calls through
//!   the correct auth mode

use serde::{Deserialize, Serialize};
use reqwest::Client;

/// Base URL for Bluesky's XRPC API.
///
/// App-password sessions always target the main Bluesky server directly.
/// (OAuth sessions target the user's specific PDS instead — see `oauth.rs`.)
const XRPC_URL: &str = "https://bsky.social/xrpc";

/// Request body for `com.atproto.server.createSession`.
///
/// Sent when the user logs in with an app password. The `identifier` is
/// typically the Bluesky handle (e.g., "alice.bsky.social") and `password`
/// is the app-specific password generated in Bluesky settings.
#[derive(Debug, Serialize)]
struct LoginRequest {
    identifier: String,
    password: String,
}

/// A Bluesky AT Protocol session.
///
/// Returned by both `createSession` (login) and `refreshSession` (token renewal).
/// Contains the user's DID (permanent identity), handle (human-readable name),
/// and JWT tokens for authenticating API requests.
///
/// ## Token Lifecycle
/// - `access_jwt` expires after ~2 hours — used for every API call
/// - `refresh_jwt` expires after ~90 days — used only to get new access tokens
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Session {
    /// The user's DID (Decentralized Identifier), e.g., "did:plc:abc123..."
    /// This is the permanent, immutable identity for the user across the AT Protocol.
    pub did: String,
    /// The user's handle (e.g., "alice.bsky.social"). Can change — the DID is the
    /// stable identifier.
    pub handle: String,
    /// Short-lived JWT for authenticating API requests (~2 hour lifetime).
    #[serde(rename = "accessJwt")]
    pub access_jwt: String,
    /// Long-lived JWT for obtaining new access tokens (~90 day lifetime).
    #[serde(rename = "refreshJwt")]
    pub refresh_jwt: String,
}

/// Manages Bluesky app-password authentication.
///
/// Wraps a `reqwest::Client` and provides login/refresh methods that talk to
/// the Bluesky XRPC API. This is a stateless helper — the session tokens are
/// stored externally in the encrypted keychain, not here.
#[derive(Clone)]
pub struct AuthManager {
    client: Client,
}

impl AuthManager {
    /// Create a new AuthManager with a default HTTP client.
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Log in to Bluesky with an app password.
    ///
    /// Sends the user's handle and app password to `com.atproto.server.createSession`,
    /// which returns a `Session` with JWT tokens and the user's DID.
    ///
    /// # Arguments
    /// * `identifier` — The user's Bluesky handle (e.g., "alice.bsky.social")
    /// * `password` — An app-specific password (NOT the account password)
    ///
    /// # Side Effects
    /// * Makes an HTTPS request to `bsky.social`
    ///
    /// # Errors
    /// Returns an error if the network request fails or Bluesky rejects the credentials.
    pub async fn login(&self, identifier: String, password: String) -> Result<Session, String> {
        let url = format!("{}/com.atproto.server.createSession", XRPC_URL);
        
        let response = self.client.post(&url)
            .json(&LoginRequest { identifier, password })
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Login failed: {}", error_text));
        }

        let session = response.json::<Session>()
            .await
            .map_err(|e| format!("Failed to parse session: {}", e))?;

        Ok(session)
    }

    /// Refresh an expired Bluesky session using the refresh JWT.
    ///
    /// When the short-lived access token expires (~2 hours), this method uses the
    /// long-lived refresh token to get a new access token without re-entering
    /// the password. The refresh token itself may also be rotated in the response.
    ///
    /// # Arguments
    /// * `refresh_jwt` — The current refresh JWT from the stored session
    ///
    /// # Side Effects
    /// * Makes an HTTPS request to `bsky.social`
    /// * The old refresh token may be invalidated by the server
    ///
    /// # Errors
    /// Returns an error if the refresh token has expired (~90 days) — the user
    /// must log in again with their app password.
    pub async fn refresh_session(&self, refresh_jwt: String) -> Result<Session, String> {
        let url = format!("{}/com.atproto.server.refreshSession", XRPC_URL);
        
        let response = self.client.post(&url)
            .header("Authorization", format!("Bearer {}", refresh_jwt))
            .send()
            .await
            .map_err(|e| format!("Refresh request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Session refresh failed: {}", error_text));
        }

        let session = response.json::<Session>()
            .await
            .map_err(|e| format!("Failed to parse refreshed session: {}", e))?;

        Ok(session)
    }
}
