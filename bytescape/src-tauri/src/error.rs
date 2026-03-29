//! # error.rs — Bytescape Error Types
//!
//! This module defines a structured error hierarchy for all backend operations.
//! Instead of passing around raw `String` errors, Bytescape uses typed errors
//! so callers can distinguish between "you need to log in again" vs. "the
//! database is corrupted" vs. "the network is down".
//!
//! ## How It Works
//! - All errors are variants of [`BytescapeError`]
//! - [`BytescapeResult<T>`] is a convenience alias for `Result<T, BytescapeError>`
//! - Tauri commands return `Result<T, String>` — the `From<BytescapeError> for String`
//!   impl converts our typed errors to human-readable strings for the frontend
//! - Common error conversions (`reqwest::Error`, `serde_json::Error`, etc.) are
//!   implemented via `#[from]` so they auto-convert with `?`
//!
//! ## Error Categories
//! - **Initialization**: App failed to start up
//! - **Database**: redb read/write failures
//! - **Authentication**: Login failures, expired sessions
//! - **Keychain**: Credential storage problems
//! - **P2P/Iroh**: Network and peer connection issues
//! - **Import/Export**: Archive parsing and file handling
//! - **Connector**: Platform-specific API errors
//! - **Network**: HTTP, URL, and JSON errors
//! - **Validation**: Invalid user input
//! - **Internal**: Catch-all for unexpected errors

use thiserror::Error;

/// The main error type for all Bytescape backend operations.
/// Each variant represents a specific category of failure.
/// The `#[error("...")]` attribute defines the human-readable message
/// shown to the user (via Tauri IPC error strings).
#[derive(Error, Debug)]
pub enum BytescapeError {
    // === Initialization Errors ===
    // These occur during app startup if something is misconfigured
    #[error("Failed to initialize application: {0}")]
    Initialization(String),

    #[error("Database not initialized")]
    DatabaseNotInitialized,

    #[error("Sync manager not initialized. Call setup_iroh_node first.")]
    SyncNotInitialized,

    // === Database Errors ===
    // Problems reading/writing the redb database. These can occur if the
    // database file is corrupted, the disk is full, or a transaction fails.
    #[error("Database error: {0}")]
    Database(#[from] redb::Error),

    #[error("Database storage error: {0}")]
    DatabaseStorage(#[from] redb::StorageError),

    #[error("Database transaction error: {0}")]
    DatabaseTransaction(String),

    #[error("Record not found: {0}")]
    RecordNotFound(String),

    #[error("Duplicate record: {0}")]
    DuplicateRecord(String),

    // === Authentication Errors ===
    // Login failures, expired sessions, and invalid tokens.
    // The frontend checks for these to redirect to the login screen.
    #[error("Authentication required. Please log in.")]
    Unauthenticated,

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Session expired. Please log in again.")]
    SessionExpired,

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Token refresh failed: {0}")]
    TokenRefreshFailed(String),

    // === Keychain Errors ===
    // Problems with the encrypted credential store (keychain_encrypted.rs).
    // Usually means the OS keychain is locked or the encryption key is wrong.
    #[error("Keychain error: {0}")]
    Keychain(String),

    #[error("Failed to load secret key: {0}")]
    KeychainLoad(String),

    #[error("Failed to save secret key: {0}")]
    KeychainSave(String),

    // === Iroh/P2P Errors ===
    // Problems with the peer-to-peer networking layer (Iroh).
    // Connection failures, sync errors, and blob storage issues.
    #[error("P2P network error: {0}")]
    P2PNetwork(String),

    #[error("Failed to connect to peer: {0}")]
    P2PConnection(String),

    #[error("Sync error: {0}")]
    Sync(String),

    #[error("Invalid namespace ID: {0}")]
    InvalidNamespaceId(String),

    #[error("Invalid ticket: {0}")]
    InvalidTicket(String),

    #[error("Blob storage error: {0}")]
    BlobStorage(String),

    #[error("Blob not found: {0}")]
    BlobNotFound(String),

    // === Import/Export Errors ===
    // Problems parsing social media archives or exporting vault data.
    #[error("Import failed: {0}")]
    Import(String),

    #[error("Export failed: {0}")]
    Export(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Invalid file format: {0}")]
    InvalidFileFormat(String),

    #[error("ZIP archive error: {0}")]
    ZipError(String),

    #[error("Media processing error: {0}")]
    MediaProcessing(String),

    // === Connector Errors ===
    // Platform-specific API failures (Bluesky rate limit, Facebook parse error, etc.)
    #[error("Bluesky API error: {0}")]
    BlueskyApi(String),

    #[error("Facebook import error: {0}")]
    FacebookImport(String),

    #[error("Unsupported source: {0}")]
    UnsupportedSource(String),

    // === Network Errors ===
    #[error("Network request failed: {0}")]
    NetworkRequest(#[from] reqwest::Error),

    #[error("HTTP error {status}: {message}")]
    HttpError { status: u16, message: String },

    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),

    #[error("JSON parse error: {0}")]
    JsonParse(#[from] serde_json::Error),

    // === File System Errors ===
    #[error("File system error: {0}")]
    FileSystem(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    // === Validation Errors ===
    #[error("Validation failed: {0}")]
    Validation(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    // === Internal Errors ===
    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Unexpected error: {0}")]
    Unexpected(String),
}

/// Result type alias for Bytescape operations
pub type BytescapeResult<T> = std::result::Result<T, BytescapeError>;

// === Conversion helpers ===
// These `From` implementations let us use the `?` operator to automatically
// convert common error types into BytescapeError without explicit mapping.

impl From<String> for BytescapeError {
    fn from(s: String) -> Self {
        BytescapeError::Internal(s)
    }
}

impl From<&str> for BytescapeError {
    fn from(s: &str) -> Self {
        BytescapeError::Internal(s.to_string())
    }
}

impl From<anyhow::Error> for BytescapeError {
    fn from(e: anyhow::Error) -> Self {
        BytescapeError::Internal(e.to_string())
    }
}

// === Tauri-compatible error conversion ===
// Tauri commands must return Result<T, String>. This converts our typed
// errors to human-readable strings that get sent to the React frontend.

impl From<BytescapeError> for String {
    fn from(e: BytescapeError) -> Self {
        e.to_string()
    }
}

/// Extension trait for converting Result<String> to BytescapeResult<T>
pub trait ResultExt<T> {
    fn context(self, msg: &str) -> BytescapeResult<T>;
    fn map_bytescape_error(self) -> BytescapeResult<T>;
}

impl<T> ResultExt<T> for std::result::Result<T, String> {
    fn context(self, msg: &str) -> BytescapeResult<T> {
        self.map_err(|e| BytescapeError::Internal(format!("{}: {}", msg, e)))
    }

    fn map_bytescape_error(self) -> BytescapeResult<T> {
        self.map_err(BytescapeError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = BytescapeError::Unauthenticated;
        assert_eq!(err.to_string(), "Authentication required. Please log in.");

        let err = BytescapeError::DatabaseNotInitialized;
        assert_eq!(err.to_string(), "Database not initialized");
    }

    #[test]
    fn test_result_conversion() {
        let ok_result: std::result::Result<i32, String> = Ok(42);
        assert_eq!(ok_result.context("test").unwrap(), 42);

        let err_result: std::result::Result<i32, String> = Err("test error".to_string());
        assert!(matches!(
            err_result.context("prefix"),
            Err(BytescapeError::Internal(msg)) if msg.contains("prefix")
        ));
    }
}
