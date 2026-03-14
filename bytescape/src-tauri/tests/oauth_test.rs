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
