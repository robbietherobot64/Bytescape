//! Tests for content-type classification (UF.7).
//!
//! Verifies the `classify_item` heuristic classifies BytescapeItem into
//! the correct BytescapeType variant based on media, text length, and link presence.

use bytescape_lib::models::{BytescapeItem, BytescapeType, classify_item, classify_bluesky_post};

/// Helper: create a minimal BytescapeItem for testing classification.
fn test_item() -> BytescapeItem {
    BytescapeItem {
        id: "test-id".to_string(),
        original_timestamp: 1700000000,
        content: String::new(),
        media_paths: vec![],
        source: "test".to_string(),
        path: String::new(),
        imported_at: 0,
        title: None,
        original_id: None,
        author_did: None,
        author_handle: None,
        author_avatar: None,
        tagged_people: vec![],
        location: None,
        is_liked: false,
        like_count: None,
        reply_count: None,
        repost_count: None,
        quote_count: None,
        item_type: BytescapeType::Status,
        url: None,
        link_url: None,
        content_html: None,
        preview_img: None,
        media_blobs: vec![],
        thumbnail_blobs: vec![],
        media_mimes: vec![],
        syndication_status: bytescape_lib::models::SyndicationStatus::LocalOnly,
        syndicated_uri: None,
        audiences: vec![],
        syndicated_uris: Default::default(),
        syndicated_at: Default::default(),
        share_history: vec![],
        reposted_by_did: None,
        reposted_by_handle: None,
        reply_parent_uri: None,
        reply_parent_handle: None,
        collab_doc_id: None,
    }
}

// ─── Video classification ───

#[test]
fn classify_video_mp4() {
    let mut item = test_item();
    item.media_mimes = vec!["video/mp4".to_string()];
    item.media_blobs = vec!["blob:abc".to_string()];
    assert_eq!(classify_item(&item), BytescapeType::Video);
}

#[test]
fn classify_video_webm() {
    let mut item = test_item();
    item.media_mimes = vec!["video/webm".to_string()];
    item.media_blobs = vec!["blob:abc".to_string()];
    assert_eq!(classify_item(&item), BytescapeType::Video);
}

#[test]
fn classify_video_with_long_text() {
    let mut item = test_item();
    item.media_mimes = vec!["video/mp4".to_string()];
    item.media_blobs = vec!["blob:abc".to_string()];
    item.content = "x".repeat(1000); // Long text doesn't override video
    assert_eq!(classify_item(&item), BytescapeType::Video);
}

#[test]
fn classify_video_beats_gallery() {
    let mut item = test_item();
    item.media_mimes = vec![
        "video/mp4".to_string(),
        "image/jpeg".to_string(),
        "image/jpeg".to_string(),
    ];
    item.media_blobs = vec!["a".into(), "b".into(), "c".into()];
    assert_eq!(classify_item(&item), BytescapeType::Video);
}

// ─── Gallery classification ───

#[test]
fn classify_gallery_two_images_short_text() {
    let mut item = test_item();
    item.media_mimes = vec!["image/jpeg".to_string(), "image/png".to_string()];
    item.media_blobs = vec!["a".into(), "b".into()];
    item.content = "Check out these pics!".to_string();
    assert_eq!(classify_item(&item), BytescapeType::Gallery);
}

#[test]
fn classify_gallery_boundary_300_chars() {
    let mut item = test_item();
    item.media_mimes = vec!["image/jpeg".to_string(), "image/jpeg".to_string()];
    item.media_blobs = vec!["a".into(), "b".into()];
    item.content = "x".repeat(300); // Exactly 300 — should still be Gallery
    assert_eq!(classify_item(&item), BytescapeType::Gallery);
}

#[test]
fn classify_gallery_over_300_becomes_status() {
    let mut item = test_item();
    item.media_mimes = vec!["image/jpeg".to_string(), "image/jpeg".to_string()];
    item.media_blobs = vec!["a".into(), "b".into()];
    item.content = "x".repeat(301); // 301 chars → too long for Gallery but not Article
    assert_eq!(classify_item(&item), BytescapeType::Status);
}

#[test]
fn classify_gallery_fallback_no_mimes() {
    let mut item = test_item();
    // No MIME info but has 3 media_paths → fallback counts as images
    item.media_paths = vec!["a.jpg".into(), "b.png".into(), "c.gif".into()];
    item.content = "Short caption".to_string();
    assert_eq!(classify_item(&item), BytescapeType::Gallery);
}

// ─── Article classification ───

#[test]
fn classify_article_long_text() {
    let mut item = test_item();
    item.content = "x".repeat(501); // >500 chars → Article
    assert_eq!(classify_item(&item), BytescapeType::Article);
}

#[test]
fn classify_article_boundary_500_is_status() {
    let mut item = test_item();
    item.content = "x".repeat(500); // Exactly 500 → NOT Article
    assert_eq!(classify_item(&item), BytescapeType::Status);
}

#[test]
fn classify_article_boundary_501() {
    let mut item = test_item();
    item.content = "x".repeat(501);
    assert_eq!(classify_item(&item), BytescapeType::Article);
}

// ─── Link classification ───

#[test]
fn classify_link_with_url_short_text() {
    let mut item = test_item();
    item.link_url = Some("https://example.com/article".to_string());
    item.content = "Check this out".to_string();
    assert_eq!(classify_item(&item), BytescapeType::Link);
}

#[test]
fn classify_link_boundary_100_chars() {
    let mut item = test_item();
    item.link_url = Some("https://example.com".to_string());
    item.content = "x".repeat(100); // Exactly 100 — should still be Link
    assert_eq!(classify_item(&item), BytescapeType::Link);
}

#[test]
fn classify_link_over_100_becomes_status() {
    let mut item = test_item();
    item.link_url = Some("https://example.com".to_string());
    item.content = "x".repeat(101); // >100 → falls through to Status
    assert_eq!(classify_item(&item), BytescapeType::Status);
}

#[test]
fn classify_link_no_url_is_status() {
    let mut item = test_item();
    item.content = "Just a short post".to_string();
    assert_eq!(classify_item(&item), BytescapeType::Status);
}

// ─── Status classification (default) ───

#[test]
fn classify_status_empty() {
    let item = test_item();
    assert_eq!(classify_item(&item), BytescapeType::Status);
}

#[test]
fn classify_status_short_text_no_media() {
    let mut item = test_item();
    item.content = "Hello, world!".to_string();
    assert_eq!(classify_item(&item), BytescapeType::Status);
}

#[test]
fn classify_status_one_image_long_text() {
    let mut item = test_item();
    item.media_mimes = vec!["image/jpeg".to_string()];
    item.media_blobs = vec!["blob:a".into()];
    item.content = "x".repeat(200); // 1 image + 200 chars → Status
    assert_eq!(classify_item(&item), BytescapeType::Status);
}

// ─── classify_bluesky_post tests ───

#[test]
fn bsky_classify_video() {
    assert_eq!(
        classify_bluesky_post("", 0, true, false),
        BytescapeType::Video
    );
}

#[test]
fn bsky_classify_gallery() {
    assert_eq!(
        classify_bluesky_post("hi", 3, false, false),
        BytescapeType::Gallery
    );
}

#[test]
fn bsky_classify_article() {
    let long = "x".repeat(501);
    assert_eq!(
        classify_bluesky_post(&long, 0, false, false),
        BytescapeType::Article
    );
}

#[test]
fn bsky_classify_link() {
    assert_eq!(
        classify_bluesky_post("Check this", 0, false, true),
        BytescapeType::Link
    );
}

#[test]
fn bsky_classify_single_image_gallery() {
    assert_eq!(
        classify_bluesky_post("pic", 1, false, false),
        BytescapeType::Gallery
    );
}

#[test]
fn bsky_classify_status() {
    assert_eq!(
        classify_bluesky_post("Just a post", 0, false, false),
        BytescapeType::Status
    );
}

// ─── Priority order tests ───

#[test]
fn priority_video_over_everything() {
    let mut item = test_item();
    item.media_mimes = vec!["video/mp4".to_string(), "image/jpeg".to_string(), "image/jpeg".to_string()];
    item.media_blobs = vec!["a".into(), "b".into(), "c".into()];
    item.link_url = Some("https://example.com".to_string());
    item.content = "x".repeat(600);
    assert_eq!(classify_item(&item), BytescapeType::Video);
}

#[test]
fn priority_gallery_over_article_when_short() {
    let mut item = test_item();
    item.media_mimes = vec!["image/jpeg".to_string(), "image/png".to_string()];
    item.media_blobs = vec!["a".into(), "b".into()];
    item.content = "x".repeat(200); // Under 300, has 2 images → Gallery
    assert_eq!(classify_item(&item), BytescapeType::Gallery);
}

#[test]
fn priority_article_over_link() {
    let mut item = test_item();
    item.link_url = Some("https://example.com".to_string());
    item.content = "x".repeat(600); // Long text + link → Article (text len wins)
    assert_eq!(classify_item(&item), BytescapeType::Article);
}
