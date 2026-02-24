//! Iroh Sync tests

use bytescape_lib::sync::SyncManager;
use iroh::{Endpoint, SecretKey};
use tempfile::tempdir;

#[tokio::test]
async fn test_sync_manager_initialization() {
    let dir = tempdir().unwrap();
    let storage_path = dir.path().to_path_buf();

    let secret_key = SecretKey::generate(&mut rand::rng());
    let endpoint = Endpoint::builder()
        .secret_key(secret_key)
        .alpns(vec![b"bytescape-test".to_vec()])
        .bind()
        .await
        .unwrap();

    let sync_manager: Result<SyncManager, String> = SyncManager::new(endpoint, storage_path).await;
    assert!(sync_manager.is_ok(), "SyncManager should initialize successfully");
}

#[tokio::test]
async fn test_create_and_join_doc() {
    let dir1 = tempdir().unwrap();
    let dir2 = tempdir().unwrap();

    let ep1 = Endpoint::builder()
        .secret_key(SecretKey::generate(&mut rand::rng()))
        .alpns(vec![b"bytescape-test".to_vec()])
        .bind()
        .await
        .unwrap();

    let ep2 = Endpoint::builder()
        .secret_key(SecretKey::generate(&mut rand::rng()))
        .alpns(vec![b"bytescape-test".to_vec()])
        .bind()
        .await
        .unwrap();

    let sync1: SyncManager = SyncManager::new(ep1, dir1.path().to_path_buf()).await.unwrap();
    let sync2: SyncManager = SyncManager::new(ep2, dir2.path().to_path_buf()).await.unwrap();

    let ticket = sync1.create_doc().await.expect("Failed to create doc");
    let ns2: bytescape_lib::sync::NamespaceId = sync2.join_doc(ticket).await.expect("Failed to join doc");

    assert_eq!(ns2.to_string().len(), 64, "Namespace ID should be valid hex string");
}
