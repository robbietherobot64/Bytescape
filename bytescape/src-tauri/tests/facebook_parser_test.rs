//! Facebook parser tests

use bytescape_lib::connectors::facebook::{
    parse_facebook_zip, FacebookPost, FacebookPostData, FacebookAttachment, 
    FacebookAttachmentGroup, FacebookMedia, ParsedPost
};
use tempfile::tempdir;
use std::fs::File;
use std::io::Write;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// Create a minimal Facebook ZIP archive for testing
fn create_test_facebook_zip(posts: Vec<FacebookPost>) -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    let zip_path = dir.path().join("test_export.zip");
    let file = File::create(&zip_path).unwrap();
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);

    // Write posts JSON
    zip.start_file("posts/your_posts_1.json", options).unwrap();
    let json = serde_json::to_string(&posts).unwrap();
    zip.write_all(json.as_bytes()).unwrap();

    zip.finish().unwrap();
    dir
}

#[test]
fn test_facebook_post_deserialization() {
    let json = r#"{
        "timestamp": 1234567890,
        "data": [{"post": "Hello World"}],
        "attachments": [],
        "title": "Status Update"
    }"#;
    
    let post: FacebookPost = serde_json::from_str(json).unwrap();
    assert_eq!(post.timestamp, 1234567890);
    assert_eq!(post.data.len(), 1);
    assert_eq!(post.data[0].post, Some("Hello World".to_string()));
}

#[test]
fn test_facebook_post_with_attachments() {
    let json = r#"{
        "timestamp": 1234567890,
        "data": [{"post": "Check out this photo"}],
        "attachments": [{
            "data": [{
                "media": {
                    "uri": "photos/12345.jpg",
                    "creation_timestamp": 1234567890,
                    "title": "Vacation photo"
                }
            }]
        }],
        "title": "Photo post"
    }"#;
    
    let post: FacebookPost = serde_json::from_str(json).unwrap();
    assert_eq!(post.attachments.len(), 1);
    assert!(post.attachments[0].data[0].media.is_some());
    assert_eq!(
        post.attachments[0].data[0].media.as_ref().unwrap().uri,
        Some("photos/12345.jpg".to_string())
    );
}

#[test]
fn test_facebook_post_with_tags() {
    let json = r#"{
        "timestamp": 1234567890,
        "data": [{"post": "Great day with friends"}],
        "tags": [{"name": "John Doe"}, {"name": "Jane Smith"}]
    }"#;
    
    let post: FacebookPost = serde_json::from_str(json).unwrap();
    assert!(post.tags.is_some());
    let tags = post.tags.unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].name, Some("John Doe".to_string()));
}

#[test]
fn test_facebook_post_with_place() {
    let json = r#"{
        "timestamp": 1234567890,
        "data": [{"post": "Having fun!"}],
        "attachments": [{
            "data": [{
                "place": {
                    "name": "Central Park",
                    "coordinate": {
                        "latitude": 40.785091,
                        "longitude": -73.968285
                    }
                }
            }]
        }]
    }"#;
    
    let post: FacebookPost = serde_json::from_str(json).unwrap();
    assert_eq!(post.attachments.len(), 1);
    assert!(post.attachments[0].data[0].place.is_some());
    let place = post.attachments[0].data[0].place.as_ref().unwrap();
    assert_eq!(place.name, Some("Central Park".to_string()));
    assert_eq!(place.coordinate.as_ref().unwrap().latitude, Some(40.785091));
}

#[test]
fn test_posts_wrapper_deserialization() {
    use bytescape_lib::connectors::facebook::FacebookPostsWrapper;
    
    let json = r#"{
        "status_updates": [
            {
                "timestamp": 1234567890,
                "data": [{"post": "Test post"}]
            }
        ]
    }"#;
    
    let wrapper: FacebookPostsWrapper = serde_json::from_str(json).unwrap();
    assert!(wrapper.status_updates.is_some());
    assert_eq!(wrapper.status_updates.unwrap().len(), 1);
}

#[test]
fn test_alternative_posts_key() {
    use bytescape_lib::connectors::facebook::FacebookPostsWrapper;
    
    // Newer exports may use "posts" instead of "status_updates"
    let json = r#"{
        "posts": [
            {
                "timestamp": 1234567890,
                "data": [{"post": "Test post"}]
            }
        ]
    }"#;
    
    let wrapper: FacebookPostsWrapper = serde_json::from_str(json).unwrap();
    assert!(wrapper.posts.is_some());
    assert_eq!(wrapper.posts.unwrap().len(), 1);
}

#[tokio::test]
async fn test_parse_facebook_zip_basic() {
    let posts = vec![FacebookPost {
        timestamp: 1234567890,
        data: vec![FacebookPostData {
            post: Some("Test post from ZIP".to_string()),
            update_timestamp: None,
        }],
        attachments: vec![],
        title: Some("Status".to_string()),
        tags: None,
        created_timestamp: None,
        string_map_data: None,
        backdate_timestamp: None,
    }];
    
    let temp_dir = create_test_facebook_zip(posts);
    let zip_path = temp_dir.path().join("test_export.zip");
    let media_dir = temp_dir.path().join("media");
    
    // Create a mock app handle - this will fail in tests but we can test the parse logic
    // For now, just test that the ZIP can be opened
    let file = File::open(&zip_path).unwrap();
    let archive = zip::ZipArchive::new(file).unwrap();
    assert!(archive.len() >= 1);
}

#[test]
fn test_decode_facebook_text() {
    use bytescape_lib::connectors::facebook::decode_facebook_text;
    
    // Test ASCII text passes through unchanged
    assert_eq!(decode_facebook_text("Hello World"), "Hello World");
    
    // Test UTF-8 text (this would be mangled in Facebook's encoding)
    // Note: This is a simplified test - real Facebook encoding issues
    // would require actual mangled input
    assert_eq!(decode_facebook_text("café"), "café");
}

#[test]
fn test_is_excluded_path() {
    use bytescape_lib::connectors::facebook::is_excluded_path;
    
    // These should be excluded (non-post data)
    assert!(is_excluded_path("messages/inbox/message_123.json"));
    assert!(is_excluded_path("friends/friends.json"));
    assert!(is_excluded_path("security_and_login/logins.json"));
    assert!(is_excluded_path("search/search_history.json"));
    assert!(is_excluded_path("ads_and_businesses/ads.json"));
    
    // These should NOT be excluded (actual posts)
    assert!(!is_excluded_path("posts/your_posts_1.json"));
    assert!(!is_excluded_path("timeline/posts.json"));
}

#[test]
fn test_is_post_file_path() {
    use bytescape_lib::connectors::facebook::is_post_file_path;
    
    // These should be identified as post files
    assert!(is_post_file_path("posts/your_posts_1.json"));
    assert!(is_post_file_path("posts/your_photos/album.json"));
    assert!(is_post_file_path("timeline.posts.json"));
    
    // These should NOT be identified as post files
    assert!(!is_post_file_path("messages/inbox/message.json"));
    assert!(!is_post_file_path("friends/friends.json"));
    assert!(!is_post_file_path("posts/trash/deleted.json"));
}

#[test]
fn test_is_noise_content() {
    use bytescape_lib::connectors::facebook::is_noise_content;
    
    // These should be filtered as noise
    assert!(is_noise_content("Account deactivated", None));
    assert!(is_noise_content("is now friends with John", None));
    assert!(is_noise_content("logged in", None));
    assert!(is_noise_content("", Some("")));
    
    // These should NOT be filtered
    assert!(!is_noise_content("Had a great day at the beach!", None));
    assert!(!is_noise_content("Check out this photo", Some("Status Update")));
}

#[test]
fn test_detect_photo_change_label() {
    use bytescape_lib::connectors::facebook::detect_photo_change_label;
    
    // Profile picture changes
    assert_eq!(
        detect_photo_change_label("updated their profile picture", None),
        Some("Updated profile picture".to_string())
    );
    
    // Cover photo changes
    assert_eq!(
        detect_photo_change_label("updated their cover photo", None),
        Some("Updated cover photo".to_string())
    );
    
    // Regular posts
    assert_eq!(
        detect_photo_change_label("Had lunch", None),
        None
    );
}
