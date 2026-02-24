//! Bytescape Error Types
//! 
//! Structured error hierarchy for type-safe error handling throughout the application.

use thiserror::Error;

/// Main error type for all Bytescape operations
#[derive(Error, Debug)]
pub enum BytescapeError {
    // === Initialization Errors ===
    #[error("Failed to initialize application: {0}")]
    Initialization(String),

    #[error("Database not initialized")]
    DatabaseNotInitialized,

    #[error("Sync manager not initialized. Call setup_iroh_node first.")]
    SyncNotInitialized,

    // === Database Errors ===
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
    #[error("Keychain error: {0}")]
    Keychain(String),

    #[error("Failed to load secret key: {0}")]
    KeychainLoad(String),

    #[error("Failed to save secret key: {0}")]
    KeychainSave(String),

    // === Iroh/P2P Errors ===
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
