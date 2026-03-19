//! Sprint v6 regression tests
//!
//! Covers:
//! - FeedCache: composite keys, TTL, invalidation
//! - Source filter: cache key isolation, post-merge filtering
//! - Profile sections: round-trip save/load, ordering
//! - Facebook noise filter: content classification
//! - Facebook path matching: post file detection

// ── FeedCache Tests ──

#[cfg(test)]
mod feed_cache_tests {
    use bytescape_lib::FeedCache;
    use std::time::Duration;

    #[tokio::test]
    async fn cache_set_and_get_within_ttl() {
        let cache = FeedCache::new();
        let val = serde_json::json!({"items": [1, 2, 3]});
        cache.set("timeline:all:first", val.clone()).await;

        let result: Option<serde_json::Value> = cache.get("timeline:all:first", Duration::from_secs(30)).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap(), val);
    }

    #[tokio::test]
    async fn cache_miss_on_different_key() {
        let cache = FeedCache::new();
        let val = serde_json::json!({"items": [1, 2, 3]});
        cache.set("timeline:all:first", val).await;

        let result: Option<serde_json::Value> = cache.get("timeline:facebook:first", Duration::from_secs(30)).await;
        assert!(result.is_none(), "Different source_filter keys must not collide");
    }

    #[tokio::test]
    async fn cache_source_filter_isolation() {
        let cache = FeedCache::new();
        let all_val = serde_json::json!({"source": "all", "count": 100});
        let fb_val = serde_json::json!({"source": "facebook", "count": 5});

        cache.set("timeline:all:first", all_val.clone()).await;
        cache.set("timeline:facebook:first", fb_val.clone()).await;

        let all_result: serde_json::Value = cache.get("timeline:all:first", Duration::from_secs(30)).await.unwrap();
        let fb_result: serde_json::Value = cache.get("timeline:facebook:first", Duration::from_secs(30)).await.unwrap();

        assert_eq!(all_result["count"], 100, "All-source cache must return unfiltered data");
        assert_eq!(fb_result["count"], 5, "Facebook-filtered cache must return filtered data");
    }

    #[tokio::test]
    async fn cache_invalidation_only_removes_target_key() {
        let cache = FeedCache::new();
        cache.set("timeline:all:first", serde_json::json!("all")).await;
        cache.set("timeline:facebook:first", serde_json::json!("fb")).await;

        cache.invalidate("timeline:all:first").await;

        let r1: Option<serde_json::Value> = cache.get("timeline:all:first", Duration::from_secs(30)).await;
        assert!(r1.is_none());
        let r2: Option<serde_json::Value> = cache.get("timeline:facebook:first", Duration::from_secs(30)).await;
        assert!(r2.is_some());
    }

    #[tokio::test]
    async fn cache_expired_entry_returns_none() {
        let cache = FeedCache::new();
        cache.set("test", serde_json::json!("value")).await;

        // Use a zero TTL — entry is immediately stale
        let result: Option<serde_json::Value> = cache.get("test", Duration::from_secs(0)).await;
        assert!(result.is_none(), "Expired entries must not be returned");
    }
}

// ── Source Filter Logic Tests ──

#[cfg(test)]
mod source_filter_tests {
    /// Verify that source_filter normalization produces the expected lowercase, trimmed form.
    #[test]
    fn source_filter_normalization() {
        let raw = Some("  Facebook  ".to_string());
        let normalized = raw
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty());
        assert_eq!(normalized, Some("facebook".to_string()));
    }

    #[test]
    fn empty_source_filter_becomes_none() {
        let raw = Some("   ".to_string());
        let normalized = raw
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty());
        assert!(normalized.is_none());
    }

    #[test]
    fn source_filter_cache_key_includes_filter() {
        let source_filter: Option<String> = Some("facebook".to_string());
        let cursor: Option<String> = None;
        let filter_tag = source_filter.as_deref().unwrap_or("all");
        let cache_key = format!("timeline:{}:{}", filter_tag, cursor.as_deref().unwrap_or("first"));

        assert_eq!(cache_key, "timeline:facebook:first");
    }

    #[test]
    fn source_filter_cache_key_all_when_none() {
        let source_filter: Option<String> = None;
        let cursor: Option<String> = Some("abc123".to_string());
        let filter_tag = source_filter.as_deref().unwrap_or("all");
        let cache_key = format!("timeline:{}:{}", filter_tag, cursor.as_deref().unwrap_or("first"));

        assert_eq!(cache_key, "timeline:all:abc123");
    }

    #[test]
    fn skip_bluesky_when_filter_is_facebook() {
        let source_filter = Some("facebook".to_string());
        let skip_bluesky = source_filter.as_deref()
            .map(|sf| sf != "bluesky")
            .unwrap_or(false);
        assert!(skip_bluesky, "Non-bluesky filter should skip Bluesky API");
    }

    #[test]
    fn dont_skip_bluesky_when_filter_is_bluesky() {
        let source_filter = Some("bluesky".to_string());
        let skip_bluesky = source_filter.as_deref()
            .map(|sf| sf != "bluesky")
            .unwrap_or(false);
        assert!(!skip_bluesky, "Bluesky filter should NOT skip Bluesky API");
    }

    #[test]
    fn dont_skip_bluesky_when_no_filter() {
        let source_filter: Option<String> = None;
        let skip_bluesky = source_filter.as_deref()
            .map(|sf| sf != "bluesky")
            .unwrap_or(false);
        assert!(!skip_bluesky, "No filter should NOT skip Bluesky API");
    }

    /// Simulate the post-merge source filter (same logic as in get_timeline_unified)
    #[test]
    fn post_merge_filter_retains_matching_source() {
        let source_filter = Some("facebook".to_string());
        let mut entries = vec![
            ("bluesky", "Post from bsky"),
            ("facebook", "Post from fb"),
            ("facebook", "Another fb post"),
            ("instagram", "Post from ig"),
        ];

        if let Some(ref sf) = source_filter {
            entries.retain(|(source, _)| source.eq_ignore_ascii_case(sf));
        }

        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|(s, _)| s.eq_ignore_ascii_case("facebook")));
    }
}

// ── Profile Section Tests ──

#[cfg(test)]
mod profile_section_tests {
    use bytescape_lib::commands::profile::ProfileSection;

    #[test]
    fn profile_section_serialization_round_trip() {
        let sections = vec![
            ProfileSection {
                id: "about-me".to_string(),
                section_type: "text".to_string(),
                title: "About Me".to_string(),
                content: "<p>Hello world</p>".to_string(),
                order: 0,
                visible: true,
            },
            ProfileSection {
                id: "photos".to_string(),
                section_type: "gallery".to_string(),
                title: "My Photos".to_string(),
                content: "[]".to_string(),
                order: 1,
                visible: true,
            },
            ProfileSection {
                id: "links".to_string(),
                section_type: "links".to_string(),
                title: "Cool Links".to_string(),
                content: "[]".to_string(),
                order: 2,
                visible: false,
            },
        ];

        let json = serde_json::to_string(&sections).unwrap();
        let deserialized: Vec<ProfileSection> = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.len(), 3);
        assert_eq!(deserialized[0].id, "about-me");
        assert_eq!(deserialized[0].section_type, "text");
        assert_eq!(deserialized[1].title, "My Photos");
        assert!(!deserialized[2].visible);
    }

    #[test]
    fn profile_sections_sort_by_order() {
        let mut sections = vec![
            ProfileSection {
                id: "links".to_string(),
                section_type: "links".to_string(),
                title: "Links".to_string(),
                content: "".to_string(),
                order: 5,
                visible: true,
            },
            ProfileSection {
                id: "about".to_string(),
                section_type: "text".to_string(),
                title: "About".to_string(),
                content: "".to_string(),
                order: 0,
                visible: true,
            },
            ProfileSection {
                id: "photos".to_string(),
                section_type: "gallery".to_string(),
                title: "Photos".to_string(),
                content: "".to_string(),
                order: 2,
                visible: true,
            },
        ];

        sections.sort_by_key(|s| s.order);

        assert_eq!(sections[0].id, "about");
        assert_eq!(sections[1].id, "photos");
        assert_eq!(sections[2].id, "links");
    }
}

// ── Facebook Import Quality Tests ──

#[cfg(test)]
mod facebook_import_tests {
    use bytescape_lib::connectors::facebook::{is_post_file_path, is_noise_content};

    #[test]
    fn detects_your_posts_html() {
        assert!(is_post_file_path("your_facebook_activity/posts/your_posts_1.html"));
    }

    #[test]
    fn detects_album_pages() {
        assert!(is_post_file_path("your_facebook_activity/posts/album/1.html"));
        assert!(is_post_file_path("your_facebook_activity/posts/album/5.html"));
    }

    #[test]
    fn detects_your_photos() {
        assert!(is_post_file_path("your_facebook_activity/posts/your_photos.html"));
    }

    #[test]
    fn detects_your_videos() {
        assert!(is_post_file_path("your_facebook_activity/posts/your_videos.html"));
    }

    #[test]
    fn detects_uncategorized_photos() {
        assert!(is_post_file_path("your_facebook_activity/posts/your_uncategorized_photos.html"));
    }

    #[test]
    fn rejects_security_files() {
        assert!(!is_post_file_path("security_and_login_information/logins_and_logouts.html"));
        assert!(!is_post_file_path("security_and_login_information/active_sessions.html"));
    }

    #[test]
    fn rejects_ads_files() {
        assert!(!is_post_file_path("ads_information/ad_preferences.html"));
    }

    #[test]
    fn rejects_messages_files() {
        assert!(!is_post_file_path("your_facebook_activity/messages/inbox/thread1.html"));
    }

    #[test]
    fn rejects_non_post_files_under_posts() {
        assert!(!is_post_file_path("your_facebook_activity/posts/trash.html"));
        assert!(!is_post_file_path("your_facebook_activity/posts/edits_you_made_to_posts.html"));
    }

    // Noise content filter tests
    #[test]
    fn filters_account_deactivation_noise() {
        assert!(is_noise_content("Account deactivated", None));
        assert!(is_noise_content("Account reactivated", None));
    }

    #[test]
    fn filters_friend_request_noise() {
        assert!(is_noise_content("is now friends with John Doe", None));
        assert!(is_noise_content("accepted your friend request", None));
    }

    #[test]
    fn filters_lifecycle_noise() {
        assert!(is_noise_content("born on May 16, 1986", None));
        assert!(is_noise_content("lives in Springfield", None));
    }

    #[test]
    fn keeps_real_posts() {
        assert!(!is_noise_content("Just had an amazing day at the beach! 🏖️", None));
        assert!(!is_noise_content("Check out this photo from our trip", Some("Rob Meyer")));
    }

    #[test]
    fn filters_shared_a_post_without_content() {
        assert!(is_noise_content("shared a post", None));
    }

    #[test]
    fn filters_empty_content() {
        assert!(is_noise_content("", None));
        assert!(is_noise_content("   ", None));
    }
}
