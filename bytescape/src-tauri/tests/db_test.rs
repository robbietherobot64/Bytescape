//! Database operation tests

use bytescape_lib::db::{DbManager, Post, Message, TransportLayer, Contact, Conversation, ConversationMode};
use tempfile::tempdir;

#[test]
fn test_db_manager_initialization() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    
    let result = DbManager::new(db_path);
    assert!(result.is_ok(), "Database should initialize successfully");
}

#[tokio::test]
async fn test_create_and_list_posts() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let db: DbManager = DbManager::new(db_path).unwrap();
    
    // Create a post
    let post: Post = db.create_post(
        "Hello, Bytescape!".to_string(),
        "did:example:user123".to_string()
    ).await.unwrap();
    
    assert!(!post.id.is_empty());
    assert_eq!(post.content, "Hello, Bytescape!");
    assert_eq!(post.author_did, "did:example:user123");
    
    // List posts
    let posts: Vec<Post> = db.list_posts().await.unwrap();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].id, post.id);
}

#[tokio::test]
async fn test_posts_sorted_by_date() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let db: DbManager = DbManager::new(db_path).unwrap();

    // Create posts in reverse order
    let post1: Post = db.create_post("First".to_string(), "did:user1".to_string()).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    let post2: Post = db.create_post("Second".to_string(), "did:user1".to_string()).await.unwrap();

    let posts: Vec<Post> = db.list_posts().await.unwrap();

    // Should be sorted newest first
    assert_eq!(posts.len(), 2);
    assert_eq!(posts[0].id, post2.id);
    assert_eq!(posts[1].id, post1.id);
}

#[tokio::test]
async fn test_save_and_get_messages() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let db: DbManager = DbManager::new(db_path).unwrap();

    let msg = Message {
        id: "msg_123".to_string(),
        conversation_id: "convo_456".to_string(),
        sender_did: "did:sender".to_string(),
        content: "Test message".to_string(),
        created_at: 1234567890,
        transport_layer: TransportLayer::Iroh,
        synced_with_bsky: false,
    };

    db.save_message(&msg).await.unwrap();

    let messages: Vec<Message> = db.get_messages("convo_456").await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, "msg_123");
    assert_eq!(messages[0].content, "Test message");
}

#[tokio::test]
async fn test_messages_sorted_by_date_asc() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let db: DbManager = DbManager::new(db_path).unwrap();

    let msg1 = Message {
        id: "msg_1".to_string(),
        conversation_id: "convo_1".to_string(),
        sender_did: "did:user".to_string(),
        content: "First".to_string(),
        created_at: 1000,
        transport_layer: TransportLayer::Iroh,
        synced_with_bsky: false,
    };

    let msg2 = Message {
        id: "msg_2".to_string(),
        conversation_id: "convo_1".to_string(),
        sender_did: "did:user".to_string(),
        content: "Second".to_string(),
        created_at: 2000,
        transport_layer: TransportLayer::Iroh,
        synced_with_bsky: false,
    };

    db.save_message(&msg1).await.unwrap();
    db.save_message(&msg2).await.unwrap();

    let messages: Vec<Message> = db.get_messages("convo_1").await.unwrap();

    // Should be sorted oldest first (ascending)
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].id, "msg_1");
    assert_eq!(messages[1].id, "msg_2");
}

#[tokio::test]
async fn test_contact_management() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let db: DbManager = DbManager::new(db_path).unwrap();

    let contact = Contact {
        did: "did:contact123".to_string(),
        handle: "contact.bsky.social".to_string(),
        p2p_capable: true,
        node_id: Some("node_abc".to_string()),
        last_polled_at: 1234567890,
    };

    db.save_contact(&contact).await.unwrap();

    // Get contact
    let retrieved: Option<Contact> = db.get_contact("did:contact123").await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().handle, "contact.bsky.social");

    // List contacts
    let contacts: Vec<Contact> = db.list_contacts().await.unwrap();
    assert_eq!(contacts.len(), 1);
}

#[tokio::test]
async fn test_conversation_management() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let db: DbManager = DbManager::new(db_path).unwrap();

    let convo = Conversation {
        id: "convo_123".to_string(),
        peer_did: "did:peer".to_string(),
        current_mode: ConversationMode::Private,
        last_message_at: 1234567890,
    };

    db.save_conversation(&convo).await.unwrap();

    let retrieved: Option<Conversation> = db.get_conversation("convo_123").await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().current_mode, ConversationMode::Private);
}
