//! Error type tests

use bytescape_lib::error::BytescapeError;

#[test]
fn test_error_display_messages() {
    // Test various error variants display correctly
    assert_eq!(
        BytescapeError::Unauthenticated.to_string(),
        "Authentication required. Please log in."
    );
    
    assert_eq!(
        BytescapeError::DatabaseNotInitialized.to_string(),
        "Database not initialized"
    );
    
    assert_eq!(
        BytescapeError::SyncNotInitialized.to_string(),
        "Sync manager not initialized. Call setup_iroh_node first."
    );
}

#[test]
fn test_error_with_context() {
    let err = BytescapeError::AuthFailed("Invalid password".to_string());
    assert!(err.to_string().contains("Invalid password"));
    
    let err = BytescapeError::BlobStorage("Connection timeout".to_string());
    assert!(err.to_string().contains("Connection timeout"));
}

#[test]
fn test_error_from_string() {
    let err: BytescapeError = String::from("test error").into();
    match err {
        BytescapeError::Internal(msg) => assert_eq!(msg, "test error"),
        _ => panic!("Expected Internal error"),
    }
}

#[test]
fn test_error_from_str() {
    let err: BytescapeError = "test error".into();
    match err {
        BytescapeError::Internal(msg) => assert_eq!(msg, "test error"),
        _ => panic!("Expected Internal error"),
    }
}

#[test]
fn test_error_to_string_conversion() {
    let err = BytescapeError::Validation("Invalid input".to_string());
    let err_string: String = err.into();
    assert!(err_string.contains("Invalid input"));
}

#[test]
fn test_database_error_variants() {
    let err = BytescapeError::RecordNotFound("user_123".to_string());
    assert!(err.to_string().contains("user_123"));
    
    let err = BytescapeError::DuplicateRecord("post_456".to_string());
    assert!(err.to_string().contains("post_456"));
}

#[test]
fn test_import_export_errors() {
    let err = BytescapeError::ParseError("Invalid JSON".to_string());
    assert!(err.to_string().contains("Invalid JSON"));
    
    let err = BytescapeError::FileNotFound("/missing/file.zip".to_string());
    assert!(err.to_string().contains("/missing/file.zip"));
}

#[test]
fn test_connector_errors() {
    let err = BytescapeError::BlueskyApi("Rate limited".to_string());
    assert!(err.to_string().contains("Rate limited"));
    
    let err = BytescapeError::FacebookImport("Missing posts folder".to_string());
    assert!(err.to_string().contains("Missing posts folder"));
}
