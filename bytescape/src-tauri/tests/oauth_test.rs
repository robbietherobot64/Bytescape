//! OAuth module tests
//!
//! Tests for FileSessionStore (GAP-1 scope) and OAuth flow components (GAP-3 scope).
//!
//! GAP-1 tests: Persistent session store (encryption, persistence, CRUD).
//! GAP-3 tests: OAuthManager construction, callback parsing, sentinel detection,
//! XRPC URL parsing, and error handling.
//!
//! Note: We construct `Session` via serde_json deserialization to avoid
//! depending on private types (OAuthTokenType) from atrium-oauth.

use bytescape_lib::oauth::{FileSessionStore, OAuthManager};
use atrium_api::types::string::Did;
use atrium_common::store::Store;
use atrium_oauth::store::session::{Session, SessionStore};
use tempfile::tempdir;

/// Create a test DID.
fn test_did(suffix: &str) -> Did {
    format!("did:plc:test{}", suffix).parse().expect("valid test DID")
}

/// Create a test Session with a dummy DPoP key and token set.
/// Uses serde_json to construct Session, avoiding dependency on private
/// `OAuthTokenType` enum from atrium-oauth's internal types module.
fn test_session(iss: &str) -> Session {
    let session_json = serde_json::json!({
        "dpop_key": {
            "kty": "EC",
            "crv": "P-256",
            "x": "NIRNgPVAwnVNzN5g2Ik2IMghWcjnBOGo9B-lKXSSXFs",
            "y": "iWF-Of43XoSTZxcadO9KWdPTjiCoviSztYw7aMtZZMc",
            "d": "9MuCYfKK4hf95p_VRj6cxKJwORTgvEU3vynfmSgFH2M"
        },
        "token_set": {
            "iss": iss,
            "sub": format!("did:plc:testabc"),
            "aud": "https://pds.example.com",
            "scope": null,
            "refresh_token": "rt_test_refresh",
            "access_token": "at_test_access",
            "token_type": "DPoP",
            "expires_at": null
        }
    });

    serde_json::from_value(session_json).expect("valid test session JSON")
}

#[tokio::test]
async fn test_file_session_store_set_and_get() {
    let dir = tempdir().unwrap();
    let store = FileSessionStore::new(
        dir.path().to_path_buf(),
        "test_password_123".to_string(),
    );

    let did = test_did("abc");
    let session = test_session("https://auth.example.com");

    // Set a session
    store.set(did.clone(), session.clone()).await.unwrap();

    // Get it back
    let retrieved = store.get(&did).await.unwrap();
    assert!(retrieved.is_some(), "Session should be retrievable after set");
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.token_set.access_token, "at_test_access");
    assert_eq!(retrieved.token_set.iss, "https://auth.example.com");
}

#[tokio::test]
async fn test_file_session_store_get_nonexistent() {
    let dir = tempdir().unwrap();
    let store = FileSessionStore::new(
        dir.path().to_path_buf(),
        "test_password_123".to_string(),
    );

    let did = test_did("nonexistent");
    let result = store.get(&did).await.unwrap();
    assert!(result.is_none(), "Non-existent session should return None");
}

#[tokio::test]
async fn test_file_session_store_del() {
    let dir = tempdir().unwrap();
    let store = FileSessionStore::new(
        dir.path().to_path_buf(),
        "test_password_123".to_string(),
    );

    let did = test_did("abc");
    let session = test_session("https://auth.example.com");

    store.set(did.clone(), session).await.unwrap();
    assert!(store.get(&did).await.unwrap().is_some());

    // Delete the session
    store.del(&did).await.unwrap();
    assert!(store.get(&did).await.unwrap().is_none(), "Session should be gone after del");
}

#[tokio::test]
async fn test_file_session_store_clear() {
    let dir = tempdir().unwrap();
    let store = FileSessionStore::new(
        dir.path().to_path_buf(),
        "test_password_123".to_string(),
    );

    let did1 = test_did("aaa");
    let did2 = test_did("bbb");

    store.set(did1.clone(), test_session("https://iss1.com")).await.unwrap();
    store.set(did2.clone(), test_session("https://iss2.com")).await.unwrap();

    assert!(store.get(&did1).await.unwrap().is_some());
    assert!(store.get(&did2).await.unwrap().is_some());

    // Clear all sessions
    store.clear().await.unwrap();
    assert!(store.get(&did1).await.unwrap().is_none());
    assert!(store.get(&did2).await.unwrap().is_none());
}

#[tokio::test]
async fn test_file_session_store_persistence() {
    let dir = tempdir().unwrap();
    let password = "persist_test_pw".to_string();
    let did = test_did("persist");
    let session = test_session("https://persist.example.com");

    // First store: write a session
    {
        let store = FileSessionStore::new(
            dir.path().to_path_buf(),
            password.clone(),
        );
        store.set(did.clone(), session.clone()).await.unwrap();
    }
    // Drop the store — only the encrypted file remains

    // Second store: read it back with the same password
    {
        let store = FileSessionStore::new(
            dir.path().to_path_buf(),
            password.clone(),
        );
        let retrieved = store.get(&did).await.unwrap();
        assert!(retrieved.is_some(), "Session should persist across store instances");
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.token_set.access_token, "at_test_access");
        assert_eq!(retrieved.token_set.refresh_token.as_deref(), Some("rt_test_refresh"));
    }
}

#[tokio::test]
async fn test_file_session_store_wrong_password_returns_empty() {
    let dir = tempdir().unwrap();
    let did = test_did("wrongpw");

    // Write with one password
    {
        let store = FileSessionStore::new(
            dir.path().to_path_buf(),
            "correct_password".to_string(),
        );
        store.set(did.clone(), test_session("https://iss.com")).await.unwrap();
    }

    // Read with a different password — should load empty (graceful failure)
    {
        let store = FileSessionStore::new(
            dir.path().to_path_buf(),
            "wrong_password".to_string(),
        );
        // The store loads at construction time; wrong password → empty cache
        let result = store.get(&did).await.unwrap();
        assert!(result.is_none(), "Wrong password should not decrypt sessions");
    }
}

#[tokio::test]
async fn test_file_session_store_overwrite_session() {
    let dir = tempdir().unwrap();
    let store = FileSessionStore::new(
        dir.path().to_path_buf(),
        "overwrite_test".to_string(),
    );

    let did = test_did("overwrite");

    // Set initial session
    store.set(did.clone(), test_session("https://iss1.com")).await.unwrap();
    let s1 = store.get(&did).await.unwrap().unwrap();
    assert_eq!(s1.token_set.iss, "https://iss1.com");

    // Overwrite with different issuer
    store.set(did.clone(), test_session("https://iss2.com")).await.unwrap();
    let s2 = store.get(&did).await.unwrap().unwrap();
    assert_eq!(s2.token_set.iss, "https://iss2.com");
}

#[tokio::test]
async fn test_file_session_store_multiple_sessions() {
    let dir = tempdir().unwrap();
    let store = FileSessionStore::new(
        dir.path().to_path_buf(),
        "multi_test".to_string(),
    );

    let did1 = test_did("user1");
    let did2 = test_did("user2");
    let did3 = test_did("user3");

    store.set(did1.clone(), test_session("https://iss1.com")).await.unwrap();
    store.set(did2.clone(), test_session("https://iss2.com")).await.unwrap();
    store.set(did3.clone(), test_session("https://iss3.com")).await.unwrap();

    assert_eq!(store.get(&did1).await.unwrap().unwrap().token_set.iss, "https://iss1.com");
    assert_eq!(store.get(&did2).await.unwrap().unwrap().token_set.iss, "https://iss2.com");
    assert_eq!(store.get(&did3).await.unwrap().unwrap().token_set.iss, "https://iss3.com");

    // Delete one, verify others remain
    store.del(&did2).await.unwrap();
    assert!(store.get(&did1).await.unwrap().is_some());
    assert!(store.get(&did2).await.unwrap().is_none());
    assert!(store.get(&did3).await.unwrap().is_some());
}

#[tokio::test]
async fn test_file_session_store_empty_dir_loads_empty() {
    let dir = tempdir().unwrap();
    let store = FileSessionStore::new(
        dir.path().to_path_buf(),
        "empty_test".to_string(),
    );

    // No file exists yet — should return None for any DID
    let result = store.get(&test_did("anything")).await.unwrap();
    assert!(result.is_none());
}

/// Verify that `FileSessionStore` satisfies the `SessionStore` trait bound.
/// This is a compile-time check — if it compiles, the trait is implemented.
#[allow(dead_code)]
fn assert_session_store_impl() {
    fn _requires_session_store<T: SessionStore>() {}
    _requires_session_store::<FileSessionStore>();
}

// ─── GAP-3: Real OAuth Flow Tests ──────────────────────────────────────────

/// GAP-3 Test 1: OAuthManager::new() constructs without error.
#[test]
fn test_oauth_manager_new_constructs_successfully() {
    let dir = tempdir().unwrap();
    let result = OAuthManager::new(
        dir.path().to_path_buf(),
        "test_master_password".to_string(),
    );
    assert!(result.is_ok(), "OAuthManager::new() should succeed: {:?}", result.err());
}

/// GAP-3 Test 1b: OAuthManager::new() works with empty password.
#[test]
fn test_oauth_manager_new_empty_password() {
    let dir = tempdir().unwrap();
    let result = OAuthManager::new(
        dir.path().to_path_buf(),
        String::new(),
    );
    assert!(result.is_ok(), "OAuthManager::new() should handle empty password");
}

/// GAP-3 Test 1c: Fresh OAuthManager has no active session.
#[tokio::test]
async fn test_oauth_manager_no_initial_session() {
    let dir = tempdir().unwrap();
    let mgr = OAuthManager::new(dir.path().to_path_buf(), "pw".to_string()).unwrap();
    assert!(!mgr.has_session().await, "Fresh OAuthManager should have no active session");
}

/// GAP-3 Test 2: Callback URL parsing — complete valid query string.
#[test]
fn test_parse_query_string_complete() {
    let query = OAuthManager::parse_query_string("code=auth_code_123&state=user_handle&iss=https%3A%2F%2Fbsky.social");
    assert_eq!(query.code.as_deref(), Some("auth_code_123"));
    assert_eq!(query.state.as_deref(), Some("user_handle"));
    assert_eq!(query.iss.as_deref(), Some("https://bsky.social"));
    assert!(query.error.is_none());
    assert!(query.error_description.is_none());
}

/// GAP-3 Test 2b: Callback parsing — error response.
#[test]
fn test_parse_query_string_error_response() {
    let query = OAuthManager::parse_query_string(
        "error=access_denied&error_description=The%20user%20denied%20the%20request"
    );
    assert!(query.code.is_none());
    assert_eq!(query.error.as_deref(), Some("access_denied"));
    assert_eq!(query.error_description.as_deref(), Some("The user denied the request"));
}

/// GAP-3 Test 2c: Callback parsing — empty query string.
#[test]
fn test_parse_query_string_empty() {
    let query = OAuthManager::parse_query_string("");
    assert!(query.code.is_none());
    assert!(query.state.is_none());
    assert!(query.iss.is_none());
    assert!(query.error.is_none());
}

/// GAP-3 Test 2d: Callback parsing — partial (code only, missing state).
#[test]
fn test_parse_query_string_partial() {
    let query = OAuthManager::parse_query_string("code=abc123");
    assert_eq!(query.code.as_deref(), Some("abc123"));
    assert!(query.state.is_none(), "Missing state should be None");
}

/// GAP-3 Test 2e: Callback parsing — URL-encoded special characters.
#[test]
fn test_parse_query_string_url_encoded() {
    let query = OAuthManager::parse_query_string(
        "code=a%20b%2Bc&state=hello%26world&iss=https%3A%2F%2Fauth.example.com"
    );
    assert_eq!(query.code.as_deref(), Some("a b+c"));
    assert_eq!(query.state.as_deref(), Some("hello&world"));
    assert_eq!(query.iss.as_deref(), Some("https://auth.example.com"));
}

/// GAP-3 Test 3: is_oauth_session sentinel detection via keychain.
#[test]
fn test_oauth_sentinel_detection_via_keychain() {
    use bytescape_lib::keychain_encrypted::EncryptedKeychain;
    let dir = tempdir().unwrap();
    let keychain = EncryptedKeychain::new(
        dir.path().join("keychain.json"),
        "test_password".to_string(),
    );

    // No session → no sentinel
    let jwt = keychain.get_access_jwt().unwrap();
    assert!(jwt.is_none(), "No session should return None");

    // Save with sentinel value
    keychain.save_session(
        "did:plc:test123".to_string(),
        "test.bsky.social".to_string(),
        bytescape_lib::OAUTH_SESSION_SENTINEL.to_string(),
        bytescape_lib::OAUTH_SESSION_SENTINEL.to_string(),
    ).unwrap();

    let jwt = keychain.get_access_jwt().unwrap();
    assert_eq!(jwt.as_deref(), Some(bytescape_lib::OAUTH_SESSION_SENTINEL));

    // Save with real JWT — sentinel should no longer match
    keychain.save_session(
        "did:plc:test123".to_string(),
        "test.bsky.social".to_string(),
        "eyJhbGciOiJFUzI1NiJ9.eyJzdWIiOiJ0ZXN0In0.sig".to_string(),
        "rt_real_refresh".to_string(),
    ).unwrap();

    let jwt = keychain.get_access_jwt().unwrap();
    assert_ne!(jwt.as_deref(), Some(bytescape_lib::OAUTH_SESSION_SENTINEL));
}

/// GAP-3 Test 4a: process_callback validation — missing code.
#[test]
fn test_callback_query_missing_code() {
    let query = OAuthManager::parse_query_string("state=handle_only");
    assert!(query.code.is_none(), "Missing code should be detected");
    assert!(query.state.is_some());
}

/// GAP-3 Test 4b: process_callback validation — missing state.
#[test]
fn test_callback_query_missing_state() {
    let query = OAuthManager::parse_query_string("code=auth_code");
    assert!(query.code.is_some());
    assert!(query.state.is_none(), "Missing state should be detected");
}

/// GAP-3 Test 4c: process_callback — error takes precedence over valid params.
#[test]
fn test_callback_query_error_takes_precedence() {
    let query = OAuthManager::parse_query_string(
        "code=valid&state=valid&error=server_error&error_description=Internal+error"
    );
    assert!(query.error.is_some(), "Error should be detected even with valid code+state");
    assert_eq!(query.error.as_deref(), Some("server_error"));
}

/// GAP-3 Test 5a: XRPC URL parsing — with query string.
#[test]
fn test_parse_xrpc_url_with_query() {
    let (nsid, query) = OAuthManager::parse_xrpc_url(
        "https://pds.example.com/xrpc/app.bsky.feed.getTimeline?limit=50"
    ).unwrap();
    assert_eq!(nsid, "app.bsky.feed.getTimeline");
    assert_eq!(query.as_deref(), Some("limit=50"));
}

/// GAP-3 Test 5b: XRPC URL parsing — without query string.
#[test]
fn test_parse_xrpc_url_without_query() {
    let (nsid, query) = OAuthManager::parse_xrpc_url(
        "https://pds.example.com/xrpc/com.atproto.repo.createRecord"
    ).unwrap();
    assert_eq!(nsid, "com.atproto.repo.createRecord");
    assert!(query.is_none());
}

/// GAP-3 Test 5c: XRPC URL parsing — complex multi-param query.
#[test]
fn test_parse_xrpc_url_complex_query() {
    let (nsid, query) = OAuthManager::parse_xrpc_url(
        "https://pds.example.com/xrpc/app.bsky.feed.getAuthorFeed?actor=did:plc:abc&limit=100&cursor=xyz"
    ).unwrap();
    assert_eq!(nsid, "app.bsky.feed.getAuthorFeed");
    assert_eq!(query.as_deref(), Some("actor=did:plc:abc&limit=100&cursor=xyz"));
}

/// GAP-3 Test 5d: XRPC URL parsing — non-XRPC URL returns error.
#[test]
fn test_parse_xrpc_url_non_xrpc_returns_error() {
    let result = OAuthManager::parse_xrpc_url("https://example.com/api/v1/status");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not an XRPC endpoint"));
}

/// GAP-3 Test 5e: get_pds_url — no session returns error.
#[tokio::test]
async fn test_get_pds_url_no_session_returns_error() {
    let dir = tempdir().unwrap();
    let mgr = OAuthManager::new(dir.path().to_path_buf(), "pw".to_string()).unwrap();
    let result = mgr.get_pds_url().await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("No OAuth session"));
}

/// GAP-3 Test 5f: xrpc_get — no session returns error.
#[tokio::test]
async fn test_xrpc_get_no_session_returns_error() {
    let dir = tempdir().unwrap();
    let mgr = OAuthManager::new(dir.path().to_path_buf(), "pw".to_string()).unwrap();
    let result = mgr.xrpc_get("app.bsky.feed.getTimeline", None).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("No OAuth session"));
}

/// GAP-3 Test 5g: restore_session — invalid DID format returns error.
#[tokio::test]
async fn test_restore_session_invalid_did() {
    let dir = tempdir().unwrap();
    let mgr = OAuthManager::new(dir.path().to_path_buf(), "pw".to_string()).unwrap();
    let result = mgr.restore_session("not-a-valid-did").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid DID"));
}
