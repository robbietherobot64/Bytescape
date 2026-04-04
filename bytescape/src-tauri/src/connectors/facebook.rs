//! # connectors/facebook.rs — Facebook Archive Import
//!
//! Parses Facebook data export ZIP archives into Bytescape's universal format.
//! Facebook exports come in two flavors — **HTML** and **JSON** — and this module
//! handles both transparently. It also detects off-Facebook activity trackers,
//! extracts media (photos/videos), and parses Messenger conversations.
//!
//! ## How It Works
//!
//! 1. User provides a Facebook export ZIP (downloaded from facebook.com/dyi)
//! 2. `parse_facebook_zip()` opens the ZIP and auto-detects the format:
//!    - **JSON format**: Parses `posts/*.json` files with `serde_json`
//!    - **HTML format**: Parses `posts/*.html` files with CSS selectors (`scraper`)
//! 3. Posts are normalized into `ParsedPost` — an intermediate struct shared
//!    with the Instagram connector (they have similar export structures)
//! 4. Media files (photos/videos) referenced in posts are extracted from the ZIP
//!    and stored via Iroh blobs (content-addressed P2P storage) or filesystem
//! 5. Off-Facebook activity data is parsed into `TrackerEntry` records for
//!    the privacy dashboard
//! 6. Messenger conversations are parsed into `VaultMessage` records
//!
//! ## Facebook's Encoding Quirk
//!
//! Facebook JSON exports encode non-ASCII text as escaped UTF-8 byte sequences
//! (e.g. `\u00c3\u00a9` instead of `é`). The `decode_facebook_text()` function
//! fixes this by treating the escape sequences as raw bytes and re-decoding.
//!
//! ## Related Files
//! - `commands/import.rs` — Orchestrates the import pipeline, converts `ParsedPost` → `BytescapeItem`
//! - `connectors/instagram.rs` — Reuses `ParsedPost` for Instagram archive imports
//! - `connectors/mod.rs` — `Source::Facebook` enum variant
//! - `models.rs` — `VaultMessage`, `BytescapeItem` schemas

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use scraper::{Html, Selector};

use crate::sync::SyncManager;

// ═══════════════════════════════════════════════════════════════════
// SECTION 1: MEDIA URI UTILITIES
// Helpers for normalizing, detecting, and collecting media file
// references from both HTML attributes and JSON values.
// ═══════════════════════════════════════════════════════════════════

fn normalize_zip_media_uri(uri: &str) -> String {
    let mut s = uri.trim().replace('\\', "/");
    while s.starts_with("./") {
        s = s[2..].to_string();
    }
    s = s.trim_start_matches('/').to_string();

    // Some exports use percent-encoded paths in HTML attributes.
    match urlencoding::decode(&s) {
        Ok(decoded) => decoded.into_owned(),
        Err(_) => s,
    }
}

fn looks_like_media_uri(value: &str) -> bool {
    let lower = value.to_lowercase();
    let clean = lower.split('?').next().unwrap_or(&lower);
    let clean = clean.split('#').next().unwrap_or(clean);

    if clean.starts_with("data:") {
        return false;
    }

    clean.ends_with(".jpg")
        || clean.ends_with(".jpeg")
        || clean.ends_with(".png")
        || clean.ends_with(".gif")
        || clean.ends_with(".webp")
        || clean.ends_with(".heic")
        || clean.ends_with(".heif")
        || clean.ends_with(".mp4")
        || clean.ends_with(".mov")
        || clean.ends_with(".avi")
        || clean.ends_with(".m4v")
        || clean.ends_with(".webm")
        || clean.ends_with(".mkv")
}

fn collect_media_uris_from_value(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, v) in map {
                match v {
                    serde_json::Value::String(s) => {
                        let key_l = key.to_lowercase();
                        if key_l == "uri" || key_l == "src" || key_l == "href" || looks_like_media_uri(s) {
                            out.push(s.clone());
                        }
                    }
                    other => collect_media_uris_from_value(other, out),
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_media_uris_from_value(v, out);
            }
        }
        serde_json::Value::String(s) => {
            if looks_like_media_uri(s) {
                out.push(s.clone());
            }
        }
        _ => {}
    }
}

fn extract_timestamp_from_json_object(obj: &serde_json::Map<String, serde_json::Value>) -> i64 {
    let keys = [
        "timestamp",
        "creation_timestamp",
        "created_timestamp",
        "backdate_timestamp",
        "update_timestamp",
        "published",
    ];

    for key in keys {
        if let Some(v) = obj.get(key) {
            if let Some(n) = v.as_i64() {
                return if n > 2_000_000_000 { n / 1000 } else { n };
            }
            if let Some(n) = v.as_u64() {
                let n = n as i64;
                return if n > 2_000_000_000 { n / 1000 } else { n };
            }
            if let Some(f) = v.as_f64() {
                let n = f.round() as i64;
                return if n > 2_000_000_000 { n / 1000 } else { n };
            }
        }
    }

    0
}

fn extract_content_from_json_object(obj: &serde_json::Map<String, serde_json::Value>) -> String {
    let keys = ["post", "text", "description", "caption", "message", "name", "title", "value"];
    for key in keys {
        if let Some(v) = obj.get(key).and_then(|v| v.as_str()) {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    String::new()
}

fn collect_facebook_json_media_posts(value: &serde_json::Value, out: &mut Vec<ParsedPost>, seen: &mut std::collections::HashSet<String>) {
    match value {
        serde_json::Value::Object(obj) => {
            let mut media_uris = Vec::new();
            collect_media_uris_from_value(value, &mut media_uris);
            media_uris.retain(|u| looks_like_media_uri(u));
            media_uris.sort();
            media_uris.dedup();

            if !media_uris.is_empty() {
                let timestamp = extract_timestamp_from_json_object(obj);
                let content = decode_facebook_text(&extract_content_from_json_object(obj));

                // Keep only candidates that are plausibly post-level records.
                if timestamp > 0 || !content.is_empty() {
                    let sig = format!("{}|{}|{}", timestamp, content, media_uris.join("|"));
                    if seen.insert(sig) {
                        out.push(ParsedPost {
                            timestamp,
                            content,
                            media_uris,
                            media_local_paths: Vec::new(),
                            title: None,
                            author_name: None,
                            action_type: None,
                            tagged_people: Vec::new(),
                            location: None,
                        });
                    }
                }
            }

            for v in obj.values() {
                collect_facebook_json_media_posts(v, out, seen);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_facebook_json_media_posts(v, out, seen);
            }
        }
        _ => {}
    }
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 2: FACEBOOK DATA SCHEMAS
// Structs that mirror the JSON shapes inside Facebook export ZIPs.
// These are used for serde deserialization of `posts/*.json` files.
// ═══════════════════════════════════════════════════════════════════

/// A single post from `posts/your_posts_1.json`
/// Facebook exports posts as either a flat array `[...]` or wrapped in `{"status_updates": [...]}`
/// Supports both old DYI format and new Accounts Center export format.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FacebookPost {
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub data: Vec<FacebookPostData>,
    #[serde(default)]
    pub attachments: Vec<FacebookAttachmentGroup>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<FacebookTag>>,
    /// Newer exports may use creation_timestamp instead
    #[serde(default, alias = "creation_timestamp")]
    pub created_timestamp: Option<i64>,
    /// Newer Meta Accounts Center exports use string_map_data for text content
    #[serde(default)]
    pub string_map_data: Option<serde_json::Value>,
    /// Some exports include a "backdate_timestamp" 
    #[serde(default)]
    pub backdate_timestamp: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FacebookPostData {
    pub post: Option<String>,
    pub update_timestamp: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FacebookAttachmentGroup {
    #[serde(default)]
    pub data: Vec<FacebookAttachment>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FacebookAttachment {
    pub media: Option<FacebookMedia>,
    pub external_context: Option<FacebookExternalContext>,
    pub place: Option<FacebookPlace>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FacebookMedia {
    pub uri: Option<String>,
    pub creation_timestamp: Option<i64>,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FacebookExternalContext {
    pub url: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FacebookPlace {
    pub name: Option<String>,
    pub coordinate: Option<FacebookCoordinate>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FacebookCoordinate {
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FacebookTag {
    pub name: Option<String>,
}

/// Wrapper for old-format Facebook exports (pre-~2019)
/// Posts wrapped in `{"status_updates": [...]}`
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FacebookPostsWrapper {
    #[serde(default)]
    pub status_updates: Option<Vec<FacebookPost>>,
    /// Newer Meta exports sometimes use different key names
    #[serde(default, alias = "your_posts_1", alias = "your_posts")]
    pub posts: Option<Vec<FacebookPost>>,
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 3: OFF-FACEBOOK ACTIVITY & TRACKER DETECTION
// Parses the "Your Off-Facebook Activity" data that shows which
// third-party apps/websites shared your activity with Facebook.
// ═══════════════════════════════════════════════════════════════════

/// Off-Facebook Activity (surveillance data)
/// Found in `apps_and_websites_off_of_facebook/your_off-facebook_activity.json`
/// or `ads_and_businesses/your_off-facebook_activity/*.json`
#[derive(Debug, Deserialize, Clone)]
pub struct OffFacebookActivityFile {
    #[serde(alias = "off_facebook_activity_v2")]
    #[serde(alias = "off_facebook_activity")]
    pub off_facebook_activity: Option<Vec<OffFacebookActivity>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OffFacebookActivity {
    pub name: String,
    #[serde(default)]
    pub events: Vec<OffFacebookEvent>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OffFacebookEvent {
    pub id: Option<i64>,
    #[serde(rename = "type")]
    pub event_type: Option<String>,
    pub timestamp: Option<i64>,
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 4: SHARED OUTPUT TYPES
// `ParsedPost` and `ParseResult` are the intermediate formats shared
// with the Instagram connector. They hold parsed post data before
// conversion to `BytescapeItem`.
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ParseProgress {
    pub stage: String,
    pub posts_found: usize,
    pub photos_found: usize,
    pub videos_found: usize,
    pub trackers_found: usize,
    pub current_file: String,
}


/// Classify a tracker by name into a human-readable category.
fn classify_tracker(name: &str) -> String {
    let lower = name.to_lowercase();

    if lower.contains("ad") || lower.contains("advertising")
        || lower.contains("sponsor") || lower.contains("promotion")
        || lower.contains("marketing") || lower.contains("campaign")
    {
        return "ad_tracking".to_string();
    }
    if lower.contains("location") || lower.contains("places")
        || lower.contains("check-in") || lower.contains("checkin")
        || lower.contains("geo") || lower.contains("map")
    {
        return "location_tracking".to_string();
    }
    if lower.contains("off-facebook") || lower.contains("off_facebook")
        || lower.contains("off meta") || lower.contains("apps and websites")
        || lower.contains("cookie") || lower.contains("pixel")
        || lower.contains("third-party") || lower.contains("third_party")
    {
        return "cross_site_tracking".to_string();
    }
    if lower.contains("device") || lower.contains("session")
        || lower.contains("login") || lower.contains("ip address")
        || lower.contains("browser") || lower.contains("security")
    {
        return "data_collection".to_string();
    }
    if lower.contains("friend") || lower.contains("contact")
        || lower.contains("interaction") || lower.contains("follower")
        || lower.contains("following") || lower.contains("social")
    {
        return "social_tracking".to_string();
    }

    "general".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrackerEntry {
    pub name: String,
    pub event_count: usize,
    pub event_types: Vec<String>,
    pub category: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ParseResult {
    pub posts: Vec<ParsedPost>,
    pub trackers: Vec<TrackerEntry>,
    pub total_photos: usize,
    pub total_videos: usize,
    pub total_tracker_events: usize,
    #[serde(default)]
    pub messages: Vec<crate::models::VaultMessage>,
}

/// A cleaned-up post ready for display/storage
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ParsedPost {
    pub timestamp: i64,
    pub content: String,
    pub media_uris: Vec<String>,       // original URIs within the ZIP
    pub media_local_paths: Vec<String>, // extracted local file paths
    pub title: Option<String>,
    /// The Facebook display name of the post author (e.g. "Rob Meyer")
    #[serde(default)]
    pub author_name: Option<String>,
    /// What action the author performed ("shared a link", "added 6 new photos", etc.)
    #[serde(default)]
    pub action_type: Option<String>,
    /// People tagged in this post (their Facebook display names, stored privately)
    #[serde(default)]
    pub tagged_people: Vec<String>,
    /// Location/check-in data
    #[serde(default)]
    pub location: Option<PostLocation>,
}

/// A location or check-in attached to a post — re-exported from models.
pub use crate::models::PostLocation;

/// A person tagged in a post — re-exported from models.
pub use crate::models::TaggedPerson;

// ═══════════════════════════════════════════════════════════════════
// SECTION 5: TEXT DECODING & CONTENT FILTERING
// Facebook's broken UTF-8 encoding fix and noise content detection.
// ═══════════════════════════════════════════════════════════════════

/// Decode Facebook's broken UTF-8 encoding.
/// Facebook exports encode non-ASCII text as escaped UTF-8 byte sequences,
/// e.g. "café" becomes "\u00c3\u00a9" (each UTF-8 byte as a \u00XX escape).
/// This function detects and fixes that encoding.
///
/// Handles three cases:
/// 1. Pure ASCII (no chars ≥ 0x80) → return as-is
/// 2. All chars < 256 (pure double-encoded) → re-interpret all as bytes → UTF-8
/// 3. Mixed: some chars ≥ 256 (already correct), some < 256 (possibly
///    double-encoded) → flush byte segments as UTF-8, pass through ≥256 directly
pub fn decode_facebook_text(text: &str) -> String {
    // Quick bail: no high bytes in the Latin-1 supplement range means no
    // double-encoding to fix.
    if !text.chars().any(|c| {
        let cp = c as u32;
        cp >= 0x80 && cp <= 0xFF
    }) {
        return text.to_string();
    }

    // Fast path: all chars < 256 — pure double-encoded content.
    if text.chars().all(|c| (c as u32) < 256) {
        let bytes: Vec<u8> = text.chars().map(|c| c as u8).collect();
        if let Ok(decoded) = String::from_utf8(bytes) {
            if decoded != text {
                return decoded;
            }
        }
        return text.to_string();
    }

    // Slow path: mixed content. Some chars are already-valid Unicode (≥ 256),
    // some are raw byte values (< 256) from double-encoding.
    // Accumulate chars < 256 as raw bytes. When we hit a char ≥ 256 or end of
    // string, flush the byte buffer as UTF-8 and append the char directly.
    let mut result = String::with_capacity(text.len());
    let mut byte_buf: Vec<u8> = Vec::new();

    for c in text.chars() {
        let cp = c as u32;
        if cp < 256 {
            byte_buf.push(cp as u8);
        } else {
            // Flush accumulated bytes as UTF-8
            if !byte_buf.is_empty() {
                match String::from_utf8(std::mem::take(&mut byte_buf)) {
                    Ok(decoded) => result.push_str(&decoded),
                    Err(e) => {
                        result.push_str(&String::from_utf8_lossy(e.as_bytes()));
                        byte_buf.clear();
                    }
                }
            }
            // Append the already-correct character
            result.push(c);
        }
    }

    // Flush any remaining bytes
    if !byte_buf.is_empty() {
        match String::from_utf8(byte_buf) {
            Ok(decoded) => result.push_str(&decoded),
            Err(e) => result.push_str(&String::from_utf8_lossy(e.as_bytes())),
        }
    }

    if result != text { result } else { text.to_string() }
}

/// Check if a file path within a Facebook export ZIP is known non-post data.
/// Returns true if this file should be EXCLUDED from post parsing.
pub fn is_excluded_path(lower_name: &str) -> bool {
    // Categories of non-post data in Facebook exports
    let excluded_segments = [
        // Messages / conversations
        "messages/",
        "/inbox/",
        "/archived_threads/",
        "/filtered_threads/",
        "/message_requests/",
        // Friends / connections
        "friends/",
        "friends_and_followers/",
        "friend_requests",
        "rejected_friend_requests",
        "removed_friends",
        // Account lifecycle (deactivated/reactivated etc.)
        "account_status",
        "account_activity",
        // Comments and reactions on OTHER people's content
        "comments_and_reactions/",
        "likes_and_reactions/",
        // Security and login
        "security_and_login",
        "login_and_security",
        "logins_and_logouts",
        "active_sessions",
        "recognized_devices",
        "account_access",
        "login_protection",
        // Search history
        "search/",
        "search_history",
        // Notifications
        "notifications/",
        // Ads, apps, off-facebook (already handled, but belt-and-suspenders)
        "off_facebook",
        "off-facebook",
        "apps_and_websites",
        "advertisers",
        "ads_and_businesses",
        "ads_information",
        // Profile metadata (not posts)
        "profile_information/",
        "personal_information/",
        // Preferences / settings
        "preferences/",
        "settings/",
        // Saved items (bookmarks, not user's own posts)
        "saved_items",
        "collections/",
        // Pages, groups, events, marketplace
        "pages/",
        "groups/",
        "events/",
        "marketplace/",
        // Payments
        "payments/",
        "facebook_payments",
        // Stories (ephemeral, separate from wall)
        "stories/",
        // Location history (GPS logs, not check-in posts)
        "location/",
        "your_places/",
        // Other non-post metadata
        "other_logged_information/",
        "bug_bounty/",
        "fundraisers/",
        "polls/",
        "your_topics/",
        "in_app_messages/",
        // Activity you're tagged in (other people's posts)
        "activity_you_re_tagged_in/",
        // Voting, navigation, link history
        "voting/",
        "navigation_bar/",
        "link_history/",
        "other_activity/",
        "facebook_gaming/",
        // Short/live/ephemeral video metadata
        "short_videos/",
        "live_videos/",
        // AR / Spark effects metadata
        "meta_spark/",
        // Shops / commerce
        "shops/",
        // Support messages
        "support_messages/",
        // Reviews you wrote
        "reviews/",
        // Fantasy games
        "fantasy_games/",
        // Volunteering signups
        "volunteering/",
        // Problem reports
        "your_problem_reports/",
        // Bonuses / credits
        "bonuses/",
        // News feed preferences
        "facebook_news/",
        // Device information
        "device_information/",
        // Music / audio recommendations
        "music_recommendations/",
        // Activity messages (system logs)
        "activity_messages/",
        // Navigation / index files
        "index.htm",
    ];

    excluded_segments.iter().any(|seg| lower_name.contains(seg))
}

/// Check if a file path looks like it contains actual user posts.
/// More precise than the previous broad matching.
pub fn is_post_file_path(lower_name: &str) -> bool {
    // Must NOT be an excluded path first
    if is_excluded_path(lower_name) {
        return false;
    }

    // Specific files under /posts/ that are NOT wall posts — exclude them
    let non_post_files = [
        "edits_you_made_to_posts",        // Edit history records, not posts
        "content_sharing_links",          // Sharing link metadata
        "trash",                          // Deleted/trashed posts
        "visual_search_on_your_posts",    // Visual search metadata
        "media_used_for_memories",        // Memory suggestions metadata
        "places_you_have_been_tagged_in", // Tagged by others
        "posts_on_other_pages",           // Posts on other pages/profiles
    ];
    if non_post_files.iter().any(|f| lower_name.contains(f)) {
        return false;
    }

    // Positive patterns for actual post files
    lower_name.contains("your_posts")
        || lower_name.contains("/posts/")
        || lower_name.contains("timeline.")
        || lower_name.contains("/wall.")
        || lower_name.contains("your_uncategorized_photos")
        || lower_name.contains("your_videos")
        || lower_name.contains("check-ins")
        || lower_name.contains("check_ins")
        || lower_name.contains("status_updates")
        || lower_name.contains("your_photos")
        || lower_name.contains("/album/")
}

/// Check if post content looks like noise that shouldn't be shown on the wall.
/// Returns true if the post should be FILTERED OUT.
pub fn is_noise_content(content: &str, title: Option<&str>) -> bool {
    let lower_content = content.to_lowercase();
    let combined = if let Some(t) = title {
        format!("{} {}", t.to_lowercase(), lower_content)
    } else {
        lower_content.clone()
    };

    // Account lifecycle events
    let account_noise = [
        "account deactivated",
        "account reactivated",
        "account status change",
        "account status update",
    ];
    if account_noise.iter().any(|n| combined.contains(n)) {
        return true;
    }

    // Content that is ONLY timestamps / "Time:" records (combined status logs)
    // e.g. "Account reactivated Time:Nov 09, 2019 4:34:55 pm Account deactivated Time:Oct 07, 2019 5:01:37 pm"
    if combined.contains("time:") && account_noise.iter().any(|n| combined.contains(n)) {
        return true;
    }

    // Friend connection records
    let friend_noise = [
        "is now friends with",
        "became friends",
        "accepted your friend request",
        "sent you a friend request",
        "friend request",
    ];
    if friend_noise.iter().any(|n| combined.contains(n)) {
        return true;
    }

    // Logged actions that aren't posts
    let action_noise = [
        "logged in",
        "logged out",
        "changed password",
        "added a phone number",
        "removed a phone number",
        "session updated",
        "settings changed",
        "security check",
    ];
    if action_noise.iter().any(|n| combined.contains(n)) {
        return true;
    }

    // Very short content that's just a label (not a real post)
    let label_noise = [
        "account status changes",
        "login activity",
        "security and login",
        "active sessions",
    ];
    if label_noise.iter().any(|n| combined.trim() == *n) {
        return true;
    }

    // Profile lifecycle events (not wall posts)
    let lifecycle_noise = [
        "born on ",
        "was born",
        "updated his profile",
        "updated her profile",
        "updated their profile",
        "lives in ",
        "moved to ",
        "is feeling ",
        "started a new job",
        "works at ",
        "went to ",
        "studied at ",
    ];
    // Only filter these if the ENTIRE content is the lifecycle event (no user text after it)
    if lifecycle_noise.iter().any(|n| combined.starts_with(n) && combined.len() < 120) {
        return true;
    }

    // Activity records that describe actions on OTHER people's content
    let activity_noise = [
        "commented on ",
        "reacted to ",
        "responded to ",
        "joined the group",
        "liked a ",
        "voted on ",
        "saved a ",
        "you searched for",
        "replied to ",
        "shared a memory",
        "shared a post",
        "was tagged in",
        "was mentioned in",
    ];
    if activity_noise.iter().any(|n| combined.starts_with(n) || combined.contains(&format!(" {}", n))) {
        return true;
    }

    // Empty content with no meaningful title
    if content.trim().is_empty() && title.map_or(true, |t| t.trim().is_empty()) {
        return true;
    }

    false
}

/// Detect if a post is a profile or cover photo change, and return an appropriate label.
/// Returns Some("Updated profile picture") or Some("Updated cover photo") if detected.
pub fn detect_photo_change_label(content: &str, title: Option<&str>) -> Option<String> {
    let combined = format!("{} {}", 
        title.unwrap_or("").to_lowercase(),
        content.to_lowercase()
    );

    if combined.contains("profile picture")
        || combined.contains("profile photo")
        || combined.contains("updated their profile")
        || combined.contains("updated his profile")
        || combined.contains("updated her profile")
    {
        return Some("Updated profile picture".to_string());
    }

    if combined.contains("cover photo")
        || combined.contains("updated their cover")
        || combined.contains("updated his cover")
        || combined.contains("updated her cover")
    {
        return Some("Updated cover photo".to_string());
    }

    // Header image is sometimes how cover photos are described
    if combined.contains("header image") || combined.contains("header photo") {
        return Some("Updated cover photo".to_string());
    }

    None
}

/// Parse a Facebook data export ZIP archive into posts, trackers, and messages.
///
/// This is the main entry point for Facebook archive imports. It:
///
/// 1. Opens the ZIP and detects the format (HTML vs JSON) by checking
///    whether post files have `.html` or `.json` extensions
/// 2. Identifies the top-level folder prefix (e.g. `facebook-username/`)
/// 3. Finds all post files, excluding non-post paths (settings, ads, etc.)
/// 4. Parses each post file according to its format
/// 5. Extracts media (photos/videos) from the ZIP, storing them via
///    Iroh blobs or filesystem fallback
/// 6. Parses off-Facebook activity data into tracker entries
/// 7. Parses Messenger conversations into VaultMessage records
/// 8. Deduplicates posts by content hash
///
/// ## Parameters
/// - `zip_path`: Path to the Facebook export ZIP file
/// - `media_dir`: Directory to extract media files into
/// - `app_handle`: Tauri app handle for emitting progress events
/// - `sync_manager`: Optional Iroh sync manager for blob storage
/// - `rt_handle`: Tokio runtime handle for async operations within sync context
/// - `timezone_offset_seconds`: UTC offset in seconds (east-positive) to correct
///   local timestamps. E.g. UTC+3 → 10800. Applied as `ts - offset` to convert
///   local timestamps to true UTC. Pass 0 for no correction.
///
/// ## Returns
/// A `ParseResult` containing parsed posts, tracker entries, and messages.
pub fn parse_facebook_zip(
    zip_path: PathBuf,
    media_dir: PathBuf,
    app_handle: &AppHandle,
    sync_manager: Option<Arc<SyncManager>>,
    rt_handle: tokio::runtime::Handle,
    timezone_offset_seconds: i64,
) -> Result<ParseResult, String> {
    let file = std::fs::File::open(&zip_path)
        .map_err(|e| format!("Failed to open ZIP: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Failed to read ZIP archive: {}", e))?;

    let mut posts: Vec<ParsedPost> = Vec::new();
    let mut trackers: Vec<TrackerEntry> = Vec::new();
    let mut messages: Vec<crate::models::VaultMessage> = Vec::new();
    let mut total_photos: usize = 0;
    let mut total_videos: usize = 0;

    // Ensure media dir exists
    std::fs::create_dir_all(&media_dir)
        .map_err(|e| format!("Failed to create media directory: {}", e))?;

    // Collect all file names and detect the top-level folder prefix
    let file_names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .collect();

    // Detect top-level prefix (e.g. "facebook-username/")
    // Facebook exports wrap everything in a single top-level folder
    let top_prefix = detect_top_level_prefix(&file_names);
    if let Some(ref prefix) = top_prefix {
        #[cfg(debug_assertions)]
        println!("facebook: Detected top-level prefix: {}", prefix);
    }
    #[cfg(debug_assertions)]
    println!("facebook: ZIP contains {} entries", file_names.len());

    // Build a lookup map of lowercase zip entry names for fuzzy media matching
    let entry_lookup: HashMap<String, String> = file_names.iter()
        .map(|n| (n.to_lowercase(), n.clone()))
        .collect();

    // Build a basename lookup for robust media matching when folder paths drift
    // between HTML references and ZIP entry layout.
    let mut basename_lookup: HashMap<String, Vec<String>> = HashMap::new();
    for name in &file_names {
        if let Some(base) = PathBuf::from(name)
            .file_name()
            .and_then(|f| f.to_str())
            .map(|s| s.to_lowercase())
        {
            basename_lookup.entry(base).or_default().push(name.clone());
        }
    }

    // Debug: detect export format (JSON vs HTML)
    let json_count = file_names.iter()
        .filter(|n| n.to_lowercase().ends_with(".json"))
        .count();
    let html_count = file_names.iter()
        .filter(|n| n.to_lowercase().ends_with(".html") || n.to_lowercase().ends_with(".htm"))
        .count();
    #[cfg(debug_assertions)]
    println!("facebook: Found {} JSON files, {} HTML files in archive", json_count, html_count);

    let is_html_export = json_count == 0 && html_count > 0;

    if is_html_export {
        #[cfg(debug_assertions)]
        println!("facebook: HTML format export detected — parsing HTML files");
        // =============================================
        // HTML FORMAT PARSING
        // =============================================
        for file_name in &file_names {
            let lower_name = file_name.to_lowercase();

            let _ = app_handle.emit("parse-progress", ParseProgress {
                stage: "scanning".to_string(),
                posts_found: posts.len(),
                photos_found: total_photos,
                videos_found: total_videos,
                trackers_found: trackers.len(),
                current_file: file_name.clone(),
            });

            if !lower_name.ends_with(".html") && !lower_name.ends_with(".htm") {
                continue;
            }

            // Read HTML contents
            let contents = if let Ok(mut entry) = archive.by_name(file_name) {
                let mut buf = String::new();
                if entry.read_to_string(&mut buf).is_ok() {
                    buf
                } else {
                    continue;
                }
            } else {
                continue;
            };

            // Determine file type from path using precise matching
            let is_off_facebook = lower_name.contains("off_facebook")
                || lower_name.contains("off-facebook")
                || lower_name.contains("apps_and_websites")
                || lower_name.contains("your_off-facebook")
                || lower_name.contains("advertisers");

            let is_message = lower_name.contains("messages/inbox/")
                || lower_name.contains("messages/archived_threads/");

            // Skip files in known non-post directories unless they are messages or off_facebook
            if is_excluded_path(&lower_name) && !is_off_facebook && !is_message {
                continue;
            }

            let is_posts_file = is_post_file_path(&lower_name);

            if is_message {
                let parsed_msgs = parse_html_messages(&contents, file_name);
                if !parsed_msgs.is_empty() {
                    #[cfg(debug_assertions)]
                    println!("facebook: ✅ Parsed {} messages (HTML) from {}", parsed_msgs.len(), file_name);
                    messages.extend(parsed_msgs);
                }
            } else if is_posts_file {
                let parsed = parse_html_posts(&contents, file_name, &top_prefix);
                if !parsed.is_empty() {
                    for p in &parsed {
                        for uri in &p.media_uris {
                            let lower_uri = uri.to_lowercase();
                            if lower_uri.ends_with(".jpg") || lower_uri.ends_with(".png") || lower_uri.ends_with(".jpeg") || lower_uri.ends_with(".gif") || lower_uri.ends_with(".webp") {
                                total_photos += 1;
                            } else if lower_uri.ends_with(".mp4") || lower_uri.ends_with(".mov") || lower_uri.ends_with(".avi") {
                                total_videos += 1;
                            }
                        }
                    }
                    #[cfg(debug_assertions)]
                    println!("facebook: ✅ Parsed {} posts (HTML) from {}", parsed.len(), file_name);
                    posts.extend(parsed);
                }
            } else if is_off_facebook {
                let tracker_entries = parse_html_trackers(&contents, file_name);
                if !tracker_entries.is_empty() {
                    // Deduplicate: only add trackers we haven't seen across files
                    let mut added = 0usize;
                    for t in tracker_entries {
                        // Merge by name — if we already have this tracker, add to its count
                        if let Some(existing) = trackers.iter_mut().find(|x| x.name == t.name) {
                            existing.event_count = existing.event_count.saturating_add(t.event_count);
                        } else {
                            trackers.push(t);
                            added += 1;
                        }
                    }
                    if added > 0 {
                        #[cfg(debug_assertions)]
                        println!("facebook: Added {} new trackers from {} (total: {})", added, file_name, trackers.len());
                    }
                }
            }
        }

        // If strict path matching found nothing, try ALL HTML files as posts
        if posts.is_empty() {
            #[cfg(debug_assertions)]
            println!("facebook: Named path matching found 0 posts, trying ALL HTML files...");
            for file_name in &file_names {
                let lower_name = file_name.to_lowercase();
                if !lower_name.ends_with(".html") && !lower_name.ends_with(".htm") {
                    continue;
                }
                // Skip non-post files using the comprehensive exclusion list
                if is_excluded_path(&lower_name) {
                    continue;
                }

                let contents = if let Ok(mut entry) = archive.by_name(file_name) {
                    let mut buf = String::new();
                    if entry.read_to_string(&mut buf).is_ok() { buf } else { continue }
                } else { continue };

                let parsed = parse_html_posts(&contents, file_name, &top_prefix);
                if !parsed.is_empty() {
                    for p in &parsed {
                        for uri in &p.media_uris {
                            let lower_uri = uri.to_lowercase();
                            if lower_uri.ends_with(".jpg") || lower_uri.ends_with(".png") || lower_uri.ends_with(".jpeg") || lower_uri.ends_with(".gif") || lower_uri.ends_with(".webp") {
                                total_photos += 1;
                            } else if lower_uri.ends_with(".mp4") || lower_uri.ends_with(".mov") || lower_uri.ends_with(".avi") {
                                total_videos += 1;
                            }
                        }
                    }
                    #[cfg(debug_assertions)]
                    println!("facebook: ✅ Parsed {} posts (HTML, broad scan) from {}", parsed.len(), file_name);
                    posts.extend(parsed);
                }
            }
        }
    } else {
        // =============================================
        // JSON FORMAT PARSING (existing logic)
        // =============================================
        for file_name in &file_names {
        // Emit progress
        let _ = app_handle.emit("parse-progress", ParseProgress {
            stage: "scanning".to_string(),
            posts_found: posts.len(),
            photos_found: total_photos,
            videos_found: total_videos,
            trackers_found: trackers.len(),
            current_file: file_name.clone(),
        });

        let lower_name = file_name.to_lowercase();

        // Skip non-JSON files
        if !lower_name.ends_with(".json") {
            continue;
        }

        // Read the JSON file contents
        let contents = if let Ok(mut entry) = archive.by_name(file_name) {
            let mut buf = String::new();
            if entry.read_to_string(&mut buf).is_ok() {
                buf
            } else {
                continue;
            }
        } else {
            continue;
        };

        // Classify and parse the file
        let is_off_facebook = lower_name.contains("off_facebook_activity")
            || lower_name.contains("off-facebook")
            || lower_name.contains("off_facebook");

        if is_off_facebook {
            // Parse off-Facebook activity (tracker/surveillance data)
            if let Ok(activity_file) = serde_json::from_str::<OffFacebookActivityFile>(&contents) {
                if let Some(activities) = activity_file.off_facebook_activity {
                    for activity in &activities {
                        let event_types: Vec<String> = activity.events.iter()
                            .filter_map(|e| e.event_type.clone())
                            .collect::<std::collections::HashSet<_>>()
                            .into_iter()
                            .collect();

                        let cat = classify_tracker(&activity.name);
                        trackers.push(TrackerEntry {
                            name: activity.name.clone(),
                            event_count: activity.events.len(),
                            event_types,
                            category: cat,
                        });
                    }
                    #[cfg(debug_assertions)]
                    println!("facebook: Found {} trackers from {}", activities.len(), file_name);
                }
            } else if let Ok(activities) = serde_json::from_str::<Vec<OffFacebookActivity>>(&contents) {
                for activity in &activities {
                    let event_types: Vec<String> = activity.events.iter()
                        .filter_map(|e| e.event_type.clone())
                        .collect::<std::collections::HashSet<_>>()
                        .into_iter()
                        .collect();

                    let cat = classify_tracker(&activity.name);
                    trackers.push(TrackerEntry {
                        name: activity.name.clone(),
                        event_count: activity.events.len(),
                        event_types,
                        category: cat,
                    });
                }
            }
        } else if !is_excluded_path(&lower_name) {
            // Try to parse JSON files that aren't in excluded directories as posts.
            // Only attempt files that could plausibly contain posts.
            let fb_posts = parse_posts_json(&contents, file_name);
            if !fb_posts.is_empty() {
                for fb_post in &fb_posts {
                    // Only accept posts that have meaningful content
                    let has_timestamp = fb_post.timestamp > 0;
                    let has_content = fb_post.data.iter().any(|d| d.post.is_some());
                    let has_attachments = fb_post.attachments.iter().any(|ag| !ag.data.is_empty());
                    let has_title = fb_post.title.is_some();
                    let has_string_map = fb_post.string_map_data.as_ref().map_or(false, |v| !v.is_null());
                    
                    if !has_timestamp && !has_content && !has_attachments && !has_title && !has_string_map {
                        continue; // Skip empty/meaningless entries
                    }

                    // Extract text content from the `data` array
                    let mut content = fb_post.data.iter()
                        .filter_map(|d| d.post.as_ref())
                        .cloned()
                        .collect::<Vec<String>>()
                        .join("\n");

                    // Also try string_map_data (newer Accounts Center exports)
                    if content.is_empty() {
                        if let Some(ref smd) = fb_post.string_map_data {
                            if let Some(obj) = smd.as_object() {
                                for (_key, val) in obj {
                                    if let Some(inner) = val.as_object() {
                                        if let Some(serde_json::Value::String(text)) = inner.get("value") {
                                            if !text.is_empty() {
                                                if !content.is_empty() { content.push('\n'); }
                                                content.push_str(text);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    let mut media_uris = Vec::new();
                    for ag in &fb_post.attachments {
                        for att in &ag.data {
                            if let Some(ref media) = att.media {
                                if let Some(ref uri) = media.uri {
                                    media_uris.push(uri.clone());
                                }
                            }
                        }
                    }

                    // Fallback for newer/complex JSON exports: recursively inspect the
                    // full post payload for nested uri/src/href/media strings.
                    if let Ok(post_value) = serde_json::to_value(fb_post) {
                        let mut discovered = Vec::new();
                        collect_media_uris_from_value(&post_value, &mut discovered);
                        for uri in discovered {
                            if !media_uris.contains(&uri) {
                                media_uris.push(uri);
                            }
                        }
                    }

                    // Deduplicate and recount media type stats from the final list.
                    let mut seen_media = std::collections::HashSet::new();
                    media_uris.retain(|uri| seen_media.insert(uri.clone()));
                    for uri in &media_uris {
                        let lower_uri = uri.to_lowercase();
                        let clean_uri = lower_uri.split('?').next().unwrap_or(&lower_uri);
                        let clean_uri = clean_uri.split('#').next().unwrap_or(clean_uri);
                        if clean_uri.ends_with(".jpg")
                            || clean_uri.ends_with(".png")
                            || clean_uri.ends_with(".jpeg")
                            || clean_uri.ends_with(".gif")
                            || clean_uri.ends_with(".webp")
                            || clean_uri.ends_with(".heic")
                            || clean_uri.ends_with(".heif")
                        {
                            total_photos += 1;
                        } else if clean_uri.ends_with(".mp4")
                            || clean_uri.ends_with(".mov")
                            || clean_uri.ends_with(".avi")
                            || clean_uri.ends_with(".m4v")
                            || clean_uri.ends_with(".webm")
                            || clean_uri.ends_with(".mkv")
                        {
                            total_videos += 1;
                        }
                    }

                    let decoded_content = decode_facebook_text(&content);
                    let decoded_title = fb_post.title.as_ref().map(|t| decode_facebook_text(t));
                    
                    // Filter out noise content (account status, friend records, etc.)
                    if media_uris.is_empty() && is_noise_content(&decoded_content, decoded_title.as_deref()) {
                        continue;
                    }

                    // Detect profile/cover photo changes and use a clear label
                    let display_title = detect_photo_change_label(&decoded_content, decoded_title.as_deref())
                        .or(decoded_title);

                    posts.push(ParsedPost {
                        timestamp: fb_post.timestamp,
                        content: decoded_content,
                        media_uris,
                        media_local_paths: Vec::new(),
                        title: display_title,
                        author_name: None, // JSON format lacks header info
                        action_type: None,
                        tagged_people: Vec::new(),
                        location: None,
                    });
                }
                #[cfg(debug_assertions)]
                println!("facebook: ✅ Parsed {} posts from {}", fb_posts.len(), file_name);
            } else {
                // Fallback for complex/variant JSON structures (e.g. video-focused exports)
                // that do not deserialize cleanly into FacebookPost.
                if let Ok(raw_json) = serde_json::from_str::<serde_json::Value>(&contents) {
                    let mut heuristic_posts: Vec<ParsedPost> = Vec::new();
                    let mut seen_heuristic = std::collections::HashSet::new();
                    collect_facebook_json_media_posts(&raw_json, &mut heuristic_posts, &mut seen_heuristic);

                    if !heuristic_posts.is_empty() {
                        let mut h_photos = 0usize;
                        let mut h_videos = 0usize;

                        for post in &mut heuristic_posts {
                            post.content = decode_facebook_text(&post.content);

                            let mut seen_media = std::collections::HashSet::new();
                            post.media_uris.retain(|uri| seen_media.insert(uri.clone()));

                            for uri in &post.media_uris {
                                let lower_uri = uri.to_lowercase();
                                let clean_uri = lower_uri.split('?').next().unwrap_or(&lower_uri);
                                let clean_uri = clean_uri.split('#').next().unwrap_or(clean_uri);
                                if clean_uri.ends_with(".jpg")
                                    || clean_uri.ends_with(".png")
                                    || clean_uri.ends_with(".jpeg")
                                    || clean_uri.ends_with(".gif")
                                    || clean_uri.ends_with(".webp")
                                    || clean_uri.ends_with(".heic")
                                    || clean_uri.ends_with(".heif")
                                {
                                    h_photos += 1;
                                } else if clean_uri.ends_with(".mp4")
                                    || clean_uri.ends_with(".mov")
                                    || clean_uri.ends_with(".avi")
                                    || clean_uri.ends_with(".m4v")
                                    || clean_uri.ends_with(".webm")
                                    || clean_uri.ends_with(".mkv")
                                {
                                    h_videos += 1;
                                }
                            }
                        }

                        total_photos += h_photos;
                        total_videos += h_videos;
                        #[cfg(debug_assertions)]
                        println!(
                            "facebook: ⚠️ Heuristic JSON parse recovered {} media posts from {} ({} photos, {} videos)",
                            heuristic_posts.len(),
                            file_name,
                            h_photos,
                            h_videos
                        );
                        posts.extend(heuristic_posts);
                    }
                }
            }
        }
    }
    } // end JSON format else block

    // Second pass: extract media files
    let _ = app_handle.emit("parse-progress", ParseProgress {
        stage: "extracting_media".to_string(),
        posts_found: posts.len(),
        photos_found: total_photos,
        videos_found: total_videos,
        trackers_found: trackers.len(),
        current_file: "Extracting media files...".to_string(),
    });

    // Use the runtime handle passed from the async caller
    let rt_handle = Some(rt_handle);

    let total_media_refs: usize = posts.iter().map(|p| p.media_uris.len()).sum();
    let skippable_refs: usize = posts.iter().map(|p| p.media_uris.iter()
        .filter(|u| u.starts_with("data:") || u.starts_with("http://") || u.starts_with("https://"))
        .count()).sum();
    #[cfg(debug_assertions)]
    println!("facebook: Second pass — extracting {} media refs across {} posts ({} external/inline skipped)",
        total_media_refs, posts.len(), skippable_refs);
    let mut media_found = 0usize;
    let mut media_missed = 0usize;

    for post in &mut posts {
        let mut local_paths = Vec::new();
        for uri in &post.media_uris {
            let normalized_uri = normalize_zip_media_uri(uri);

            // Skip URIs that can't be found in a ZIP file:
            //  - data: URIs are inline base64 icons/emojis embedded in HTML
            //  - http/https URLs are external links (tenor, giphy, etc.)
            if normalized_uri.starts_with("data:") || normalized_uri.starts_with("http://") || normalized_uri.starts_with("https://") {
                local_paths.push(String::new()); // sentinel to keep alignment with media_uris
                continue;
            }

            // Try normalized URI first, then with top-level prefix prepended.
            // Keep both original and normalized variants for resilience.
            let mut candidates = Vec::new();
            candidates.push(uri.clone());
            if normalized_uri != *uri {
                candidates.push(normalized_uri.clone());
            }
            if let Some(prefix) = &top_prefix {
                candidates.push(format!("{}{}", prefix, uri));
                if normalized_uri != *uri {
                    candidates.push(format!("{}{}", prefix, normalized_uri));
                }
            }
            candidates.sort();
            candidates.dedup();

            let mut found = false;
            for candidate in &candidates {
                // Try exact match first
                if let Ok(mut entry) = archive.by_name(candidate) {
                    let file_name = PathBuf::from(&normalized_uri)
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| format!("media_{}", post.timestamp));

                    let mut data = Vec::new();
                    if entry.read_to_end(&mut data).is_ok() && !data.is_empty() {
                        if let Some(path) = store_media_data(&data, &file_name, &media_dir, &sync_manager, &rt_handle) {
                            local_paths.push(path);
                            found = true;
                            media_found += 1;
                            break;
                        }
                    }
                }
            }

            // Fallback: case-insensitive lookup
            if !found {
                let lower_uri = normalized_uri.to_lowercase();
                let mut candidates_lower = vec![lower_uri.clone()];
                if let Some(prefix) = &top_prefix {
                    candidates_lower.push(format!("{}{}", prefix.to_lowercase(), lower_uri));
                }
                candidates_lower.sort();
                candidates_lower.dedup();

                for candidate_lower in &candidates_lower {
                    if let Some(real_name) = entry_lookup.get(candidate_lower.as_str()) {
                        if let Ok(mut entry) = archive.by_name(real_name) {
                            let file_name = PathBuf::from(&normalized_uri)
                                .file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_else(|| format!("media_{}", post.timestamp));

                            let mut data = Vec::new();
                            if entry.read_to_end(&mut data).is_ok() && !data.is_empty() {
                                if let Some(path) = store_media_data(&data, &file_name, &media_dir, &sync_manager, &rt_handle) {
                                    local_paths.push(path);
                                    found = true;
                                    media_found += 1;
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            // Fallback: basename match (directory mismatch tolerant)
            if !found {
                if let Some(base_lower) = PathBuf::from(&normalized_uri)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .map(|s| s.to_lowercase())
                {
                    if let Some(entry_names) = basename_lookup.get(&base_lower) {
                        for entry_name in entry_names {
                            if let Ok(mut entry) = archive.by_name(entry_name) {
                                let file_name = PathBuf::from(&normalized_uri)
                                    .file_name()
                                    .map(|f| f.to_string_lossy().to_string())
                                    .unwrap_or_else(|| format!("media_{}", post.timestamp));

                                let mut data = Vec::new();
                                if entry.read_to_end(&mut data).is_ok() && !data.is_empty() {
                                    if let Some(path) = store_media_data(&data, &file_name, &media_dir, &sync_manager, &rt_handle) {
                                        local_paths.push(path);
                                        found = true;
                                        media_found += 1;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if !found {
                local_paths.push(String::new()); // sentinel to keep alignment with media_uris
                media_missed += 1;
                // Log first few misses for debugging
                if media_missed <= 5 {
                    #[cfg(debug_assertions)]
                    println!("facebook: ⚠️ Media not found in ZIP: '{}'", uri);
                }
            }
        }
        post.media_local_paths = local_paths;
    }

    #[cfg(debug_assertions)]
    println!("facebook: Media extraction — {} found, {} missed out of {} total refs",
        media_found, media_missed, total_media_refs);

    // === CROSS-FILE DEDUPLICATION ===
    // Posts from different HTML files (main posts, album pages, your_photos)
    // may contain the same content or reference the same media files.
    {
        let pre_dedup = posts.len();
        let mut seen_post_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
        posts.retain(|p| {
            // Build a stable key that uses both content and media fingerprint.
            let mut uris: Vec<String> = p.media_uris.iter()
                .map(|u| u.rsplit('/').next().unwrap_or(u).to_lowercase())
                .collect();
            uris.sort();
            let media_key = uris.join(",");
            let content_key = p.content.trim().to_lowercase();
            let ts_bucket = if p.timestamp > 0 { (p.timestamp / 60).to_string() } else { "0".to_string() };
            let key = format!("{}|{}|{}", ts_bucket, content_key, media_key);
            seen_post_keys.insert(key)
        });
        if pre_dedup != posts.len() {
            #[cfg(debug_assertions)]
            println!("facebook: Cross-file dedup removed {} duplicate posts ({} → {})",
                pre_dedup - posts.len(), pre_dedup, posts.len());
        }
    }

    // Sort posts by timestamp descending (newest first)
    posts.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    // Sort trackers by event count descending (worst offenders first)
    trackers.sort_by(|a, b| b.event_count.cmp(&a.event_count));

    // Recalculate total tracker events from the final deduplicated list
    let total_tracker_events: usize = trackers.iter().map(|t| t.event_count).sum();
    #[cfg(debug_assertions)]
    println!("facebook: Final tracker stats — {} unique trackers, {} total events", trackers.len(), total_tracker_events);

    // Final progress event
    let _ = app_handle.emit("parse-progress", ParseProgress {
        stage: "complete".to_string(),
        posts_found: posts.len(),
        photos_found: total_photos,
        videos_found: total_videos,
        trackers_found: trackers.len(),
        current_file: "Done!".to_string(),
    });

    #[cfg(debug_assertions)]
    println!("facebook: Import complete — {} posts, {} photos, {} videos, {} trackers",
        posts.len(), total_photos, total_videos, trackers.len());

    // Apply timezone offset correction: Facebook exports may store timestamps
    // in the user's local time. Convert to true UTC: utc = local - offset.
    if timezone_offset_seconds != 0 {
        for post in &mut posts {
            if post.timestamp > 0 {
                post.timestamp -= timezone_offset_seconds;
            }
        }
        for msg in &mut messages {
            // VaultMessage.sent_at is an RFC3339 string — parse, adjust, re-format
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&msg.sent_at) {
                let adjusted = dt.timestamp() - timezone_offset_seconds;
                if let Some(naive) = chrono::DateTime::from_timestamp(adjusted, 0) {
                    msg.sent_at = naive.to_rfc3339();
                }
            }
        }
        #[cfg(debug_assertions)]
        println!("facebook: Applied timezone offset of {}s to {} posts and {} messages",
            timezone_offset_seconds, posts.len(), messages.len());
    }

    Ok(ParseResult {
        posts,
        trackers,
        total_photos,
        total_videos,
        total_tracker_events,
        messages,
    })
}

/// Detect the common top-level folder prefix in a ZIP archive.
/// Facebook exports typically wrap everything in `facebook-<username>/`.
fn detect_top_level_prefix(file_names: &[String]) -> Option<String> {
    let first = file_names.first()?;
    let prefix = if let Some(slash_pos) = first.find('/') {
        &first[..=slash_pos]
    } else {
        return None;
    };

    // Verify this prefix is common to all (or most) entries
    let matching = file_names.iter().filter(|n| n.starts_with(prefix)).count();
    if matching > file_names.len() / 2 {
        Some(prefix.to_string())
    } else {
        None
    }
}

/// Check if parsed FacebookPosts have any real content (not just default values)
fn has_meaningful_posts(posts: &[FacebookPost]) -> bool {
    posts.iter().any(|p| {
        p.timestamp > 0
            || p.created_timestamp.map_or(false, |t| t > 0)
            || p.data.iter().any(|d| d.post.is_some())
            || p.attachments.iter().any(|ag| !ag.data.is_empty())
            || p.title.is_some()
            || p.string_map_data.as_ref().map_or(false, |v| !v.is_null())
    })
}

/// Try to parse a JSON string as Facebook posts, supporting multiple formats.
fn parse_posts_json(contents: &str, file_name: &str) -> Vec<FacebookPost> {
    // Try 1: Modern flat array format `[{timestamp, data, attachments}, ...]`
    if let Ok(mut fb_posts) = serde_json::from_str::<Vec<FacebookPost>>(contents) {
        if !fb_posts.is_empty() && has_meaningful_posts(&fb_posts) {
            // Fix up timestamps: use created_timestamp as fallback
            for post in &mut fb_posts {
                if post.timestamp == 0 {
                    if let Some(ct) = post.created_timestamp {
                        post.timestamp = ct;
                    }
                }
            }
            #[cfg(debug_assertions)]
            println!("facebook: ✅ Parsed {} posts (flat array) from {}", fb_posts.len(), file_name);
            return fb_posts;
        }
    }

    // Try 2: Old wrapper format `{"status_updates": [...]}`
    if let Ok(wrapper) = serde_json::from_str::<FacebookPostsWrapper>(contents) {
        if let Some(mut posts) = wrapper.status_updates {
            if !posts.is_empty() && has_meaningful_posts(&posts) {
                for post in &mut posts {
                    if post.timestamp == 0 {
                        if let Some(ct) = post.created_timestamp {
                            post.timestamp = ct;
                        }
                    }
                }
                #[cfg(debug_assertions)]
                println!("facebook: ✅ Parsed {} posts (status_updates) from {}", posts.len(), file_name);
                return posts;
            }
        }
        if let Some(mut posts) = wrapper.posts {
            if !posts.is_empty() && has_meaningful_posts(&posts) {
                for post in &mut posts {
                    if post.timestamp == 0 {
                        if let Some(ct) = post.created_timestamp {
                            post.timestamp = ct;
                        }
                    }
                }
                #[cfg(debug_assertions)]
                println!("facebook: ✅ Parsed {} posts (posts key) from {}", posts.len(), file_name);
                return posts;
            }
        }
    }

    // Try 3: JSON object with any key containing an array of post-like objects
    if let Ok(obj) = serde_json::from_str::<serde_json::Value>(contents) {
        if let Some(map) = obj.as_object() {
            for (key, value) in map {
                if let Some(arr) = value.as_array() {
                    if arr.is_empty() {
                        continue;
                    }
                    if let Ok(mut fb_posts) = serde_json::from_value::<Vec<FacebookPost>>(serde_json::Value::Array(arr.clone())) {
                        if !fb_posts.is_empty() && has_meaningful_posts(&fb_posts) {
                            for post in &mut fb_posts {
                                if post.timestamp == 0 {
                                    if let Some(ct) = post.created_timestamp {
                                        post.timestamp = ct;
                                    }
                                }
                            }
                            #[cfg(debug_assertions)]
                            println!("facebook: ✅ Parsed {} posts (key='{}') from {}", fb_posts.len(), key, file_name);
                            return fb_posts;
                        }
                    }
                }
            }
        }
    }

    // Don't log failures for every file since we're trying every JSON file now
    Vec::new()
}

/// Store media data in Iroh blobs or filesystem, returning the path/hash string.
fn store_media_data(
    data: &[u8],
    file_name: &str,
    media_dir: &PathBuf,
    sync_manager: &Option<Arc<SyncManager>>,
    rt_handle: &Option<tokio::runtime::Handle>,
) -> Option<String> {
    // Try Iroh blobs first
    if let (Some(ref sync), Some(ref handle)) = (sync_manager, rt_handle) {
        let sync_clone = sync.clone();
        let bytes = bytes::Bytes::from(data.to_vec());
        match handle.block_on(async move {
            sync_clone.store_media_with_thumbnail(bytes, 256).await
        }) {
            Ok((media_hash, _thumb_hash)) => {
                return Some(format!("blob:{}", media_hash));
            }
            Err(e) => {
                eprintln!("facebook: Failed to store in Iroh, falling back: {}", e);
            }
        }
    }

    // Fallback: write to filesystem
    let dest = media_dir.join(file_name);
    if std::fs::write(&dest, data).is_ok() {
        Some(dest.to_string_lossy().to_string())
    } else {
        None
    }
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 6: HTML FORMAT PARSING
// CSS selector-based parsing for Facebook's HTML export format.
// ═══════════════════════════════════════════════════════════════════

/// Facebook HTML exports use a structure like:
///   <div class="...">
///     <div class="...">Post text content</div>
///     <div class="..."><img src="photos/image.jpg" /></div>
///     <div class="...">Jan 1, 2024, 12:00 PM</div>
///   </div>
///
/// The CSS classes change between exports, so we use structural heuristics
/// rather than class-based selectors.

/// Parse posts from a Facebook HTML export file.
/// Facebook HTML exports group each post in a container div. We look for these
/// containers and extract text, timestamps, and media from each one.
fn parse_html_posts(html: &str, file_name: &str, _top_prefix: &Option<String>) -> Vec<ParsedPost> {
    let document = Html::parse_document(html);
    let mut posts: Vec<ParsedPost> = Vec::new();

    // Compute the directory of this HTML file within the ZIP, so we can
    // resolve relative media paths (e.g. src="../photos_and_videos/img.jpg")
    let html_dir = if let Some(slash) = file_name.rfind('/') {
        &file_name[..=slash]
    } else {
        ""
    };

    // Facebook HTML exports use container divs with obfuscated class names.
    // Each post container typically holds:
    //   - text content (the post body)
    //   - a timestamp string
    //   - optional <img>/<video> elements for media

    // Helper: resolve a relative media src to a ZIP-friendly path.
    // Facebook HTML exports use inconsistent relative paths — sometimes
    // relative to the HTML file's directory (with ../), sometimes relative
    // to the top-level export folder. We resolve ../ paths against html_dir,
    // but pass through other relative paths as-is so the second pass can
    // try them with the top-level prefix prepended.
    let resolve_media_path = |src: &str| -> String {
        if src.starts_with("http") || src.starts_with('/') {
            return src.to_string();
        }
        // Only resolve against html_dir if the path uses ../ (truly relative)
        if src.contains("../") && !html_dir.is_empty() {
            let mut resolved = format!("{}{}", html_dir, src);
            // Normalize ../ segments
            while let Some(pos) = resolved.find("/../") {
                if let Some(parent_end) = resolved[..pos].rfind('/') {
                    resolved = format!("{}{}", &resolved[..parent_end], &resolved[pos + 3..]);
                } else {
                    break;
                }
            }
            return resolved;
        }
        // Non-../ paths: return raw src so the second pass can try prefix+src
        src.to_string()
    };

    // Try container class selectors
    // For the modern Meta export format (2024+), each post is a div._a6-g with:
    //   - div._a6-h._a6-i = header ("Rob Meyer shared a link.", "Rob Meyer updated his status.")
    //   - div._a6-p = body (contains div._2pin with content text and "Updated ..." dates)
    //   - div._a6-o = footer (contains div._a72d with the actual timestamp)
    //
    // We ONLY use div._a6-g as the primary selector. The _2pin divs are INSIDE _a6-g
    // and should not be used as top-level selectors (they'd double-match).
    let primary_selectors = ["div._a6-g"];
    let fallback_selectors = ["div._2pin", "div[class*='pim']"];

    let try_parse_containers = |selectors: &[&str], posts: &mut Vec<ParsedPost>| {
        for sel_str in selectors {
            if let Ok(sel) = Selector::parse(sel_str) {
                let elements: Vec<_> = document.select(&sel).collect();
                if elements.is_empty() {
                    continue;
                }
                #[cfg(debug_assertions)]
                println!("facebook: HTML selector '{}' matched {} elements in {}", sel_str, elements.len(), file_name);

                // Pre-parse sub-selectors once
                let header_sel = Selector::parse("div._a6-h").ok();
                let timestamp_sel = Selector::parse("div._a72d").ok();
                let body_sel = Selector::parse("div._a6-p").ok();
                // Metadata selectors — EXTRACTED as structured data, not stripped
                let meta_tag_sel = Selector::parse("div._a701").ok();   // "You tagged ..."
                let meta_date_sel = Selector::parse("div._a703").ok();  // standalone date labels
                let meta_ts_sel = Selector::parse("div._a72d").ok();    // footer timestamp
                let meta_place_sel = Selector::parse("div._a6-o").ok(); // footer section
                let img_sel = Selector::parse("img").unwrap();
                let video_sel = Selector::parse("video source, video").unwrap();
                let a_sel = Selector::parse("a").unwrap();

                for el in &elements {
                    // === EXTRACT HEADER: author name + action ===
                    let header_text = header_sel.as_ref()
                        .and_then(|s| el.select(s).next())
                        .map(|h| h.text().collect::<Vec<_>>().join(" ").trim().to_string());

                    // Parse author name and action from header
                    // Headers follow pattern: "Rob Meyer shared a link."
                    // "Rob Meyer added 6 new photos to the album: Timeline photos."
                    let (author_name, action_type) = if let Some(ref header) = header_text {
                        parse_header_author_action(header)
                    } else {
                        (None, None)
                    };

                    // === EXTRACT TIMESTAMP from div._a72d footer ===
                    let mut timestamp: i64 = 0;
                    if let Some(ref ts_sel) = timestamp_sel {
                        for ts_el in el.select(ts_sel) {
                            let ts_text = ts_el.text().collect::<Vec<_>>().join(" ");
                            let ts_trimmed = ts_text.trim();
                            if let Some(ts) = try_parse_fb_date(ts_trimmed) {
                                timestamp = ts;
                                break;
                            }
                        }
                    }

                    // === EXTRACT TAGGED PEOPLE from div._a701 ===
                    // Format: "You tagged Jake Lucas" → extract "Jake Lucas"
                    let mut tagged_people: Vec<String> = Vec::new();
                    let mut metadata_texts: Vec<String> = Vec::new();
                    if let Some(ref sel) = meta_tag_sel {
                        for m in el.select(sel) {
                            let text = m.text().collect::<Vec<_>>().join(" ").trim().to_string();
                            metadata_texts.push(text.clone());
                            // Extract name from "You tagged [Name]" pattern
                            if let Some(name) = text.strip_prefix("You tagged ") {
                                let name = name.trim().trim_end_matches('.');
                                if !name.is_empty() {
                                    tagged_people.push(name.to_string());
                                }
                            }
                        }
                    }
                    // Standalone date labels — still exclude from body text
                    if let Some(ref sel) = meta_date_sel {
                        for m in el.select(sel) {
                            metadata_texts.push(m.text().collect::<Vec<_>>().join(" ").trim().to_string());
                        }
                    }
                    // Footer timestamp text — exclude from body
                    if let Some(ref sel) = meta_ts_sel {
                        for m in el.select(sel) {
                            metadata_texts.push(m.text().collect::<Vec<_>>().join(" ").trim().to_string());
                        }
                    }
                    // Footer section — exclude from body
                    if let Some(ref sel) = meta_place_sel {
                        for m in el.select(sel) {
                            metadata_texts.push(m.text().collect::<Vec<_>>().join(" ").trim().to_string());
                        }
                    }

                    // === EXTRACT BODY CONTENT (excluding metadata) ===
                    let raw_body = if let Some(ref b_sel) = body_sel {
                        let body_parts: Vec<String> = el.select(b_sel)
                            .map(|b| b.text().collect::<Vec<_>>().join(" "))
                            .collect();
                        if !body_parts.is_empty() {
                            body_parts.join(" ").trim().to_string()
                        } else {
                            el.text().collect::<Vec<_>>().join(" ").trim().to_string()
                        }
                    } else {
                        el.text().collect::<Vec<_>>().join(" ").trim().to_string()
                    };

                    // Remove metadata text fragments from body
                    let mut body_text = raw_body.clone();
                    for meta in &metadata_texts {
                        if !meta.is_empty() {
                            body_text = body_text.replace(meta, "");
                        }
                    }
                    // Also strip header text from body (it gets concatenated in)
                    if let Some(ref ht) = header_text {
                        if !ht.is_empty() {
                            body_text = body_text.replace(ht.as_str(), "");
                        }
                    }
                    // Extract location/place data as structured PostLocation
                    let (body_text, post_location) = extract_place_metadata(&body_text);

                    if body_text.trim().is_empty() && header_text.as_ref().map_or(true, |h| h.is_empty()) {
                        continue;
                    }

                    // === CLEAN CONTENT: remove "Updated Mon DD, YYYY ..." and timestamp strings ===
                    let content = clean_post_text_enhanced(&body_text);
                    let decoded_content = decode_facebook_text(&content);

                    // If timestamp wasn't found in _a72d, try extracting from raw body
                    if timestamp == 0 {
                        timestamp = extract_timestamp_from_text_enhanced(&raw_body).unwrap_or(0);
                    }

                    // === EXTRACT MEDIA ===
                    let mut media_uris: Vec<String> = Vec::new();
                    // Images (skip base64 data URIs which are just icons)
                    for img in el.select(&img_sel) {
                        if let Some(src) = img.value().attr("src") {
                            if !src.starts_with("data:") {
                                let resolved = resolve_media_path(src);
                                if !media_uris.contains(&resolved) {
                                    media_uris.push(resolved);
                                }
                            }
                        }
                    }
                    // Videos
                    for vid in el.select(&video_sel) {
                        if let Some(src) = vid.value().attr("src") {
                            let resolved = resolve_media_path(src);
                            if !media_uris.contains(&resolved) {
                                media_uris.push(resolved);
                            }
                        }
                    }
                    // <a> tags linking to media files
                    for a in el.select(&a_sel) {
                        if let Some(href) = a.value().attr("href") {
                            let lower = href.to_lowercase();
                            if lower.ends_with(".jpg") || lower.ends_with(".jpeg") 
                                || lower.ends_with(".png") || lower.ends_with(".mp4")
                                || lower.ends_with(".mov") || lower.ends_with(".gif")
                                || lower.ends_with(".webp") {
                                let resolved = resolve_media_path(href);
                                if !media_uris.contains(&resolved) {
                                    media_uris.push(resolved);
                                }
                            }
                        }
                    }

                    // === FILTER NOISE ===
                    if media_uris.is_empty() && is_noise_content(&decoded_content, header_text.as_deref()) {
                        continue;
                    }

                    if !decoded_content.trim().is_empty() || !media_uris.is_empty() {
                        // Use header as title, or detect photo changes
                        let title = header_text
                            .filter(|h| !h.is_empty())
                            .or_else(|| detect_photo_change_label(&decoded_content, None));

                        posts.push(ParsedPost {
                            timestamp,
                            content: decoded_content,
                            media_uris,
                            media_local_paths: Vec::new(),
                            title,
                            author_name: author_name.clone(),
                            action_type: action_type.clone(),
                            tagged_people,
                            location: post_location,
                        });
                    }
                }
                if !posts.is_empty() {
                    return true;
                }
            }
        }
        false
    };

    // Try primary selectors first
    if !try_parse_containers(&primary_selectors, &mut posts) {
        // Fall back to older selectors only if primary found nothing
        try_parse_containers(&fallback_selectors, &mut posts);
    }

    // Fallback: If class selectors didn't work, try a broader structural approach
    // Only run on files that look like actual post files to avoid extracting noise.
    if posts.is_empty() && is_post_file_path(&file_name.to_lowercase()) {
        #[cfg(debug_assertions)]
        println!("facebook: Class selectors found nothing, trying structural parse in {}", file_name);

        // Collect ALL images and videos from the document
        let img_sel = Selector::parse("img").unwrap();
        let video_sel = Selector::parse("video source, video").unwrap();
        let all_images: Vec<String> = document.select(&img_sel)
            .filter_map(|el| el.value().attr("src").map(|s| resolve_media_path(s)))
            .collect();
        let all_videos: Vec<String> = document.select(&video_sel)
            .filter_map(|el| el.value().attr("src").map(|s| resolve_media_path(s)))
            .collect();

        // Find text blocks that look like post content
        let div_sel = Selector::parse("div").unwrap();
        let mut seen_texts: std::collections::HashSet<String> = std::collections::HashSet::new();
        
        for div in document.select(&div_sel) {
            let direct_text: String = div.text().collect::<Vec<_>>().join(" ");
            let trimmed = direct_text.trim().to_string();
            
            if trimmed.len() < 2 || seen_texts.contains(&trimmed) {
                continue;
            }
            
            let has_substantive_content = trimmed.len() > 15 
                && !trimmed.starts_with("<!")
                && !looks_like_fb_date(&trimmed)
                && !trimmed.contains("Facebook")
                && !trimmed.contains("Settings")
                && !trimmed.contains("Log Out");
            
            if has_substantive_content {
                let is_parent_of_existing = posts.iter()
                    .any(|p| trimmed.contains(&p.content) && trimmed.len() > p.content.len() + 50);
                if !is_parent_of_existing {
                    let decoded = decode_facebook_text(&clean_post_text(&trimmed));

                    seen_texts.insert(trimmed.clone());
                    let timestamp = extract_timestamp_from_text(&trimmed).unwrap_or(0);
                    
                    // Find media within this div element
                    let mut media_uris: Vec<String> = Vec::new();
                    for img in div.select(&img_sel) {
                        if let Some(src) = img.value().attr("src") {
                            media_uris.push(resolve_media_path(src));
                        }
                    }
                    for vid in div.select(&video_sel) {
                        if let Some(src) = vid.value().attr("src") {
                            let resolved = resolve_media_path(src);
                            if !media_uris.contains(&resolved) {
                                media_uris.push(resolved);
                            }
                        }
                    }

                    // Filter out noise content when no media is attached.
                    if media_uris.is_empty() && is_noise_content(&decoded, None) {
                        continue;
                    }

                    let title = detect_photo_change_label(&decoded, None);

                    posts.push(ParsedPost {
                        timestamp,
                        content: decoded,
                        media_uris,
                        media_local_paths: Vec::new(),
                        title,
                        author_name: None,
                        action_type: None,
                        tagged_people: Vec::new(),
                        location: None,
                    });
                }
            }
        }

        // If posts have no media yet, distribute document-level media across posts
        let posts_without_media = posts.iter().filter(|p| p.media_uris.is_empty()).count();
        let mut all_media = all_images;
        all_media.extend(all_videos);
        if posts_without_media == posts.len() && !all_media.is_empty() && !posts.is_empty() {
            let media_per_post = (all_media.len() / posts.len()).max(1);
            for (i, post) in posts.iter_mut().enumerate() {
                let start = i * media_per_post;
                let end = ((i + 1) * media_per_post).min(all_media.len());
                if start < all_media.len() {
                    post.media_uris.extend_from_slice(&all_media[start..end]);
                }
            }
        }
    }

    // === DEDUPLICATION ===
    // 1. Deduplicate media within each post
    for post in &mut posts {
        let mut seen = std::collections::HashSet::new();
        post.media_uris.retain(|uri| {
            let key = uri.to_lowercase();
            seen.insert(key)
        });
    }

    // 2. Deduplicate posts by content (adjacent)
    posts.dedup_by(|a, b| a.content == b.content && !a.content.is_empty());

    // 3. Deduplicate posts with identical media sets (same photos from different HTML files)
    {
        let mut seen_media: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut seen_content: std::collections::HashSet<String> = std::collections::HashSet::new();
        posts.retain(|p| {
            // Build a dedup key from content + sorted media URIs
            let mut media_key = p.media_uris.iter()
                .map(|u| u.to_lowercase())
                .collect::<Vec<_>>();
            media_key.sort();
            let key = format!("{}|{}", p.content.trim(), media_key.join(","));
            
            // For posts with only media (no text), dedup by media alone
            if p.content.trim().is_empty() && !p.media_uris.is_empty() {
                let media_str = media_key.join(",");
                return seen_media.insert(media_str);
            }
            
            // For posts with text, dedup by content
            if !p.content.trim().is_empty() {
                return seen_content.insert(key);
            }
            
            true
        });
    }

    // === FINAL CLEANUP: Remove blank posts ===
    // Posts with empty/whitespace-only content AND no media are noise
    posts.retain(|p| !p.content.trim().is_empty() || !p.media_uris.is_empty());
    
    #[cfg(debug_assertions)]
    println!("facebook: parse_html_posts found {} posts with {} total media refs in {}",
        posts.len(),
        posts.iter().map(|p| p.media_uris.len()).sum::<usize>(),
        file_name);
    
    posts
}

/// Extract "Place: ... (lat, lng)" metadata from text and return
/// (cleaned_text, Option<PostLocation>). The location is extracted as
/// structured data rather than being stripped and lost.
///
/// Facebook HTML exports include place metadata like:
///   "Place: Meyer Experimental Farm & Homestead (40.054, -74.142)"
fn extract_place_metadata(text: &str) -> (String, Option<PostLocation>) {
    let mut result = text.to_string();
    let mut location: Option<PostLocation> = None;

    if let Some(place_pos) = result.find("Place:") {
        // Compute what to extract and what range to remove using indices only
        let after_str = result[place_pos..].to_string(); // owned copy to avoid borrow
        let place_content = after_str[6..].trim_start().to_string();

        if let Some(paren_end) = after_str.find(')') {
            let segment = &after_str[..paren_end + 1];
            // Try to extract GPS coords from "(lat, lng)"
            if let Some(paren_start) = segment.rfind('(') {
                let inside = &segment[paren_start + 1..segment.len() - 1];
                let parts: Vec<&str> = inside.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    let lat = parts[0].parse::<f64>().ok();
                    let lng = parts[1].parse::<f64>().ok();
                    if lat.is_some() && lng.is_some() {
                        let name_end = segment[6..].find('(').unwrap_or(segment.len() - 6);
                        let name = segment[6..6 + name_end].trim().to_string();
                        location = Some(PostLocation {
                            name: if name.is_empty() { "Unknown".to_string() } else { name },
                            latitude: lat,
                            longitude: lng,
                        });
                        let before = &result[..place_pos];
                        let after_removal = &result[place_pos + paren_end + 1..];
                        result = format!("{}{}", before, after_removal);
                    }
                }
            }
            // If we couldn't parse coords, treat it as name-only
            if location.is_none() {
                let name_str = place_content.split('\n').next().unwrap_or("").trim().to_string();
                let end_offset = after_str.find("Updated ")
                    .or_else(|| after_str.find('\n'))
                    .unwrap_or(after_str.len());
                if !name_str.is_empty() {
                    location = Some(PostLocation {
                        name: name_str,
                        latitude: None,
                        longitude: None,
                    });
                }
                let before = &result[..place_pos];
                let after_removal = if place_pos + end_offset < result.len() {
                    &result[place_pos + end_offset..]
                } else { "" };
                result = format!("{}{}", before, after_removal);
            }
        } else {
            // No parens at all — just "Place: Name"
            let remaining = &after_str[6..];
            let end_offset = remaining.find("Updated ")
                .or_else(|| remaining.find('\n'))
                .unwrap_or(remaining.len());
            let name = remaining[..end_offset].trim().to_string();
            if !name.is_empty() {
                location = Some(PostLocation {
                    name,
                    latitude: None,
                    longitude: None,
                });
            }
            let remove_end = place_pos + 6 + end_offset;
            let before = result[..place_pos].to_string();
            let after_removal = if remove_end < result.len() {
                result[remove_end..].to_string()
            } else { String::new() };
            result = format!("{}{}", before, after_removal);
        }
    }

    // Also extract standalone GPS coordinate patterns like "(40.054, -74.142)"
    let mut cleaned = String::with_capacity(result.len());
    let chars: Vec<char> = result.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '(' {
            let rest: String = chars[i..].iter().collect();
            if let Some(close) = rest.find(')') {
                let inside = &rest[1..close];
                let parts: Vec<&str> = inside.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2
                    && parts.iter().all(|p| {
                        let p = p.trim_start_matches('-');
                        p.parse::<f64>().is_ok() && p.contains('.')
                    })
                {
                    // If we haven't captured location yet, grab these coords
                    if location.is_none() {
                        let lat = parts[0].parse::<f64>().ok();
                        let lng = parts[1].parse::<f64>().ok();
                        if lat.is_some() && lng.is_some() {
                            location = Some(PostLocation {
                                name: "Unknown".to_string(),
                                latitude: lat,
                                longitude: lng,
                            });
                        }
                    }
                    i += close + 1;
                    continue;
                }
            }
        }
        cleaned.push(chars[i]);
        i += 1;
    }

    // Collapse multiple spaces
    while cleaned.contains("  ") {
        cleaned = cleaned.replace("  ", " ");
    }
    (cleaned.trim().to_string(), location)
}

/// Parse author name and action type from a Facebook post header.
///
/// Facebook HTML headers follow the pattern: "Rob Meyer shared a link."
/// or "Rob Meyer added 6 new photos to the album: Timeline photos."
///
/// Returns (Option<author_name>, Option<action_type>)
fn parse_header_author_action(header: &str) -> (Option<String>, Option<String>) {
    if header.is_empty() {
        return (None, None);
    }

    // Known action verb phrases that separate the author from the action.
    // Order matters: longer/more specific patterns first.
    let action_verbs = [
        " shared a memory.",
        " shared a link.",
        " shared a post.",
        " shared ",  // catch-all for shared
        " updated his status.",
        " updated her status.",
        " updated his cover photo.",
        " updated her cover photo.",
        " updated his profile picture.",
        " updated her profile picture.",
        " updated his bio.",
        " updated her bio.",
        " updated ",
        " added ",
        " posted ",
        " changed ",
        " wrote on ",
        " commented on ",
        " likes ",
        " liked ",
        " was with ",
        " is with ",
        " created ",
        " reviewed ",
        " recommended ",
        " was tagged ",
        " was feeling ",
        " is feeling ",
        " was at ",
        " is at ",
        " was watching ",
        " is watching ",
        " was playing ",
        " was listening to ",
        " was reading ",
        " was drinking ",
        " was eating ",
        " was traveling to ",
        "'s post.",
        "'s photo.",
        "'s video.",
    ];

    for verb in &action_verbs {
        if let Some(pos) = header.find(verb) {
            let author = header[..pos].trim().to_string();
            let action = header[pos..].trim().to_string();
            if !author.is_empty() {
                return (Some(author), Some(action));
            }
        }
    }

    // If no action verb found, the entire header might just be the author name
    // (rare, but defensive)
    let trimmed = header.trim().trim_end_matches('.');
    if !trimmed.is_empty() {
        (Some(trimmed.to_string()), None)
    } else {
        (None, None)
    }
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 7: TIMESTAMP & TEXT EXTRACTION UTILITIES
// Helpers for extracting dates, text content, and locations from
// both HTML DOM nodes and JSON objects.
// ═══════════════════════════════════════════════════════════════════

/// Enhanced timestamp extraction: scan for "Updated Mon DD, YYYY H:MM:SS am/pm"
/// or any substring that looks like a Facebook date, even within a single long line.
fn extract_timestamp_from_text_enhanced(text: &str) -> Option<i64> {
    // Pattern: "Updated May 04, 2022 5:56:34 am" — strip the "Updated " prefix
    let lower = text.to_lowercase();
    if let Some(pos) = lower.find("updated ") {
        let after_updated = &text[pos + 8..];
        let candidate = after_updated.trim();
        // Take up to ~40 chars (enough for any date format)
        let end = candidate.len().min(40);
        let snippet = &candidate[..end];
        if let Some(ts) = try_parse_fb_date(snippet) {
            return Some(ts);
        }
    }

    // Scan for month names followed by date-like patterns
    let months = [
        "January", "February", "March", "April", "May", "June",
        "July", "August", "September", "October", "November", "December",
        "Jan", "Feb", "Mar", "Apr", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    for month in &months {
        let mut start = 0;
        while let Some(pos) = text[start..].find(month) {
            let abs_pos = start + pos;
            // Take a window around the month name (date could be before or after)
            let window_start = abs_pos.saturating_sub(5);
            let window_end = (abs_pos + 40).min(text.len());
            let window = text[window_start..window_end].trim();
            if looks_like_fb_date(window) {
                if let Some(ts) = try_parse_fb_date(window) {
                    return Some(ts);
                }
            }
            // Try just from the month name forward
            let forward = text[abs_pos..].get(..35).unwrap_or(&text[abs_pos..]).trim();
            if looks_like_fb_date(forward) {
                if let Some(ts) = try_parse_fb_date(forward) {
                    return Some(ts);
                }
            }
            start = abs_pos + month.len();
        }
    }

    // Fall back to newline splitting (works for multi-line HTML)
    extract_timestamp_from_text(text)
}

/// Clean post text: remove timestamps, "Updated ..." metadata, excessive whitespace
fn clean_post_text_enhanced(text: &str) -> String {
    let mut result = text.to_string();

    // Remove Facebook album metadata artifacts that often appear once per image
    // in timeline photo dumps, e.g. "Timeline photos Timeline photos ..."
    for marker in ["Timeline photos", "timeline photos"] {
        while result.contains(marker) {
            result = result.replace(marker, " ");
        }
    }

    // Remove "Updated Mon DD, YYYY H:MM:SS am/pm" patterns
    // Use a simple approach: find "Updated " followed by a month name and remove
    // the date portion
    let months_re = [
        "January", "February", "March", "April", "May", "June",
        "July", "August", "September", "October", "November", "December",
        "Jan", "Feb", "Mar", "Apr", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    
    for month in &months_re {
        let pattern = format!("Updated {}", month);
        if let Some(pos) = result.find(&pattern) {
            // Find the end of the date string (look for am/pm or end of digits)
            let after = &result[pos..];
            let end_offset = find_date_end(after);
            result = format!("{}{}", &result[..pos], &result[pos + end_offset..]);
        }
    }
    
    // Also remove standalone date strings that match FB format
    // (the timestamp from the footer that got concatenated into text)
    let text_clone = result.clone();
    for month in &months_re {
        if let Some(pos) = text_clone.find(month) {
            let window_end = (pos + 35).min(text_clone.len());
            let candidate = text_clone[pos..window_end].trim();
            if looks_like_fb_date(candidate) {
                if try_parse_fb_date(candidate).is_some() {
                    // Remove this date string from result
                    if let Some(rpos) = result.find(candidate) {
                        result = format!("{}{}", &result[..rpos], &result[rpos + candidate.len()..]);
                    }
                }
            }
        }
    }

    // Collapse multiple spaces
    while result.contains("  ") {
        result = result.replace("  ", " ");
    }
    result.trim().to_string()
}

/// Find the end offset of a date string starting with "Updated Month..."
fn find_date_end(s: &str) -> usize {
    // Look for am/pm (case insensitive) followed by whitespace or end
    let lower = s.to_lowercase();
    if let Some(am_pos) = lower.find(" am").or_else(|| lower.find(" pm")) {
        // Include the "am"/"pm" (3 chars including space)
        return am_pos + 3;
    }
    // Fallback: take up to 40 chars or the whole string
    s.len().min(40)
}

/// Extract a timestamp from text that may contain a Facebook date string
fn extract_timestamp_from_text(text: &str) -> Option<i64> {
    // Facebook dates in HTML often appear as separate text within the div
    // Split text into segments and check each for date patterns
    let segments: Vec<&str> = text.split('\n').collect();
    for seg in &segments {
        let trimmed = seg.trim();
        if looks_like_fb_date(trimmed) {
            if let Some(ts) = try_parse_fb_date(trimmed) {
                return Some(ts);
            }
        }
    }
    // Also try the whole text
    if looks_like_fb_date(text) {
        return try_parse_fb_date(text);
    }
    None
}

/// Clean post text by removing timestamp portions and other noise
fn clean_post_text(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let cleaned: Vec<&str> = lines.iter()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !looks_like_fb_date(trimmed)
        })
        .copied()
        .collect();
    cleaned.join(" ").trim().to_string()
}

/// Check if a string looks like a Facebook date format
fn looks_like_fb_date(text: &str) -> bool {
    let trimmed = text.trim();
    // Facebook dates: "January 1, 2024 at 12:00 PM", "Jan 1, 2024, 12:00 PM",
    // "1 January 2024", "2024-01-01", etc.
    let months = ["january", "february", "march", "april", "may", "june",
                  "july", "august", "september", "october", "november", "december",
                  "jan", "feb", "mar", "apr", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let lower = trimmed.to_lowercase();
    
    // Must be reasonably short (dates aren't paragraphs)
    if trimmed.len() > 80 {
        return false;
    }
    
    months.iter().any(|m| lower.contains(m)) && (lower.contains("20") || lower.contains("19"))
}

/// Try to extract timestamps from the HTML document and associate them with posts
#[allow(dead_code)]
fn extract_timestamps_from_html(_document: &Html, _posts: &mut [ParsedPost]) {
    // No longer used — timestamps are extracted per-post in parse_html_posts
}

/// Try to parse a Facebook-style date string into a Unix timestamp
fn try_parse_fb_date(text: &str) -> Option<i64> {
    let cleaned = text.trim()
        .replace(" at ", " ")
        .replace("  ", " ")
        // chrono's %p requires uppercase AM/PM
        .replace(" am", " AM")
        .replace(" pm", " PM");
    
    // Try several known formats
    let formats = [
        "%B %d, %Y %I:%M %p",      // "January 1, 2024 12:00 PM"
        "%B %d, %Y, %I:%M %p",     // "January 1, 2024, 12:00 PM"
        "%b %d, %Y %I:%M %p",      // "Jan 1, 2024 12:00 PM"
        "%b %d, %Y, %I:%M %p",     // "Jan 1, 2024, 12:00 PM"
        "%B %d, %Y %H:%M",         // "January 1, 2024 13:00"
        "%B %d, %Y",               // "January 1, 2024"
        "%d %B %Y",                // "1 January 2024"
        "%Y-%m-%d %H:%M:%S",       // "2024-01-01 12:00:00"
        "%Y-%m-%d",                // "2024-01-01"
        "%b %d, %Y %I:%M:%S %p",   // "Jan 1, 2024 12:00:00 PM"
        "%B %d, %Y %I:%M:%S %p",   // "January 1, 2024 12:00:00 PM"
    ];
    
    for fmt in &formats {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&cleaned, fmt) {
            return Some(dt.and_utc().timestamp());
        }
        // Try as date only (no time)
        if let Ok(d) = chrono::NaiveDate::parse_from_str(&cleaned, fmt) {
            return Some(d.and_hms_opt(0, 0, 0)?.and_utc().timestamp());
        }
    }
    
    None
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 8: TRACKER & MESSAGE PARSING (HTML FORMAT)
// Extracts off-Facebook activity trackers and Messenger conversations
// from the HTML format export files.
// ═══════════════════════════════════════════════════════════════════

/// Parse off-Facebook activity tracker entries from an HTML file.
///
/// Facebook off-Facebook activity HTML pages list companies that tracked the
/// user.  The structure varies across export versions, but generally each
/// company appears inside a container div (often with the same obfuscated
/// class names used by posts) and may include:
///   - Company / app name
///   - Event count ("N interactions", "N events")
///   - Event types (e.g. "CUSTOM", "PAGE_VIEW", "PURCHASE")
///
/// We try multiple strategies:
///   1. Container class selectors (same obfuscated classes used elsewhere)
///   2. Leaf-level text divs — small divs whose text looks like a company name
///   3. List items and table rows
fn parse_html_trackers(html: &str, file_name: &str) -> Vec<TrackerEntry> {
    let document = Html::parse_document(html);
    let mut trackers: Vec<TrackerEntry> = Vec::new();
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Helper: extract an event count from text like "3 interactions" or "12 events"
    // Only matches patterns where digits are immediately followed by " interactions"/" events"
    // Rejects year-like numbers (1900-2099) and anything over 100,000
    let extract_event_count = |text: &str| -> usize {
        let lower = text.to_lowercase();
        // Only match "interactions" and "events" (not "activity" which catches year strings)
        for keyword in &[" interactions", " events"] {
            if let Some(pos) = lower.find(keyword) {
                let before = text[..pos].trim_end();
                if let Some(num_str) = before.rsplit_once(|c: char| !c.is_ascii_digit())
                    .map(|(_, n)| n)
                    .or_else(|| {
                        if before.chars().all(|c| c.is_ascii_digit()) && !before.is_empty() {
                            Some(before)
                        } else {
                            None
                        }
                    })
                {
                    if let Ok(n) = num_str.parse::<usize>() {
                        // Reject year-like numbers and absurdly large counts
                        if n > 0 && n < 100_000 && !(n >= 1900 && n <= 2099) {
                            return n;
                        }
                    }
                }
            }
        }
        1
    };

    // ----- Strategy 1: container class selectors -----
    let container_selectors = [
        "div._a6-g",
        "div._2pin",
        "div[class*='pim']",
    ];
    for sel_str in &container_selectors {
        if let Ok(sel) = Selector::parse(sel_str) {
            for el in document.select(&sel) {
                let text: String = el.text().collect::<Vec<_>>().join(" ");
                let trimmed = text.trim().to_string();
                if trimmed.len() < 2 || trimmed.len() > 500 || seen_names.contains(&trimmed) {
                    continue;
                }
                // Skip navigation / chrome
                if trimmed.contains("Settings") || trimmed.contains("Log Out") || trimmed.contains("Help Center") {
                    continue;
                }
                seen_names.insert(trimmed.clone());

                let count = extract_event_count(&trimmed);
                let mut types = vec!["UNKNOWN".to_string()];
                // Try to extract event types (comma-separated list of uppercase words)
                let upper_words: Vec<String> = trimmed.split_whitespace()
                    .filter(|w| w.len() > 2 && *w == w.to_uppercase() && w.chars().all(|c| c.is_ascii_alphabetic() || c == '_'))
                    .map(|w| w.to_string())
                    .collect();
                if !upper_words.is_empty() {
                    types = upper_words;
                }

                // Use the first short text segment as the name
                let name = trimmed.split('\n').next()
                    .or_else(|| trimmed.split("  ").next())
                    .unwrap_or(&trimmed)
                    .trim().to_string();

                let cat = classify_tracker(&name);
                trackers.push(TrackerEntry {
                    name,
                    event_count: count,
                    event_types: types,
                    category: cat,
                });
            }
        }
        if !trackers.is_empty() {
            break;
        }
    }

    // ----- Strategy 2: leaf-level text divs -----
    // Only try this if Strategy 1 found nothing, and apply stricter filters
    if trackers.is_empty() {
        let div_sel = Selector::parse("div").unwrap();
        for div in document.select(&div_sel) {
            if trackers.len() >= 5000 {
                break; // Safety cap per file
            }
            // Only consider "leaf" divs (those without child divs)
            let child_div_sel = Selector::parse("div").unwrap();
            if div.select(&child_div_sel).count() > 1 {
                continue;
            }
            let text: String = div.text().collect::<Vec<_>>().join(" ");
            let trimmed = text.trim().to_string();
            // Company names are short — reject very short or very long text
            if trimmed.len() < 3 || trimmed.len() > 80 || seen_names.contains(&trimmed) {
                continue;
            }
            // Skip any text with common Facebook chrome words
            let lower = trimmed.to_lowercase();
            if lower.contains("facebook") || lower.contains("settings") || lower.contains("help")
                || lower.contains("privacy") || lower.contains("download")
                || lower.contains("manage") || lower.contains("learn more")
                || lower.contains("log out") || lower.contains("©")
                || lower.starts_with("your ") || lower.starts_with("this ")
                || lower.starts_with("we ") || lower.starts_with("the ")
            {
                continue;
            }
            // Reject pure numbers, single words that look like labels, etc.
            if trimmed.chars().all(|c| c.is_ascii_digit() || c == ',' || c == '.') {
                continue;
            }
            // Company names start with uppercase
            if !trimmed.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                continue;
            }
            // Reject text that's just a date
            if looks_like_fb_date(&trimmed) {
                continue;
            }
            seen_names.insert(trimmed.clone());

            let count = extract_event_count(&trimmed);

            let cat = classify_tracker(&trimmed);
            trackers.push(TrackerEntry {
                name: trimmed,
                event_count: count,
                event_types: vec!["UNKNOWN".to_string()],
                category: cat,
            });
        }
    }

    // ----- Strategy 3: list items / table rows -----
    if trackers.is_empty() {
        let li_sel = Selector::parse("li, tr").unwrap();
        for el in document.select(&li_sel) {
            if trackers.len() >= 5000 {
                break; // Safety cap per file
            }
            let text: String = el.text().collect::<Vec<_>>().join(" ");
            let trimmed = text.trim().to_string();
            if trimmed.len() < 3 || trimmed.len() > 120 || seen_names.contains(&trimmed) {
                continue;
            }
            let lower = trimmed.to_lowercase();
            if lower.contains("facebook") || lower.contains("settings") || lower.contains("help")
                || lower.contains("privacy") || lower.contains("©") {
                continue;
            }
            seen_names.insert(trimmed.clone());
            let count = extract_event_count(&trimmed);
            let cat = classify_tracker(&trimmed);
            trackers.push(TrackerEntry {
                name: trimmed,
                event_count: count,
                event_types: vec!["UNKNOWN".to_string()],
                category: cat,
            });
        }
    }

    #[cfg(debug_assertions)]
    println!("facebook: parse_html_trackers found {} tracker entries from {}", trackers.len(), file_name);
    trackers
}

/// Parse messages from a Facebook HTML export file.
///
/// Facebook HTML message exports are typically structured with a main container.
/// For old export formats:
///   <div class="message">
///     <div class="message_header">
///       <span class="user">Sender Name</span>
///       <span class="meta">Date</span>
///     </div>
///     <div class="message_content">Message text</div>
///   </div>
///
/// For new Meta accounts center formats (obfuscated classes like _a6-g):
///   <div class="pam _3-95 _2pi0 _2lej uiBoxWhite noborder">
///     <div class="_3-96 _2pio _2lek _2lel">Sender Name</div>
///     <div class="_3-96 _2let">
///        <div><div></div><div>Message text</div></div>
///     </div>
///     <div class="_3-94 _2lem">Date</div>
///   </div>
fn parse_html_messages(html: &str, file_name: &str) -> Vec<crate::models::VaultMessage> {
    let document = Html::parse_document(html);
    let mut messages: Vec<crate::models::VaultMessage> = Vec::new();

    // Try to find the conversation partner name
    // It's usually the <title> or an <h3> tag, e.g., "Conversation with John Doe"
    let mut convo_partner = String::new();
    let title_sel = Selector::parse("title").unwrap();
    if let Some(title_el) = document.select(&title_sel).next() {
        let title_text = title_el.text().collect::<Vec<_>>().join(" ");
        convo_partner = title_text.replace("Conversation with ", "").trim().to_string();
    }
    
    // Fallback: look for generic headers if title is missing
    if convo_partner.is_empty() {
        if file_name.contains("messages/inbox/") {
            let parts: Vec<&str> = file_name.split("messages/inbox/").collect();
            if parts.len() > 1 {
                let folder_name = parts[1].split('/').next().unwrap_or("");
                convo_partner = folder_name.split('_').next().unwrap_or("").to_string();
            }
        }
    }

    if convo_partner.is_empty() {
        convo_partner = "Unknown".to_string();
    }

    // A simple convo_id derived from the peer's name
    let convo_id = format!("fb_convo_{}", convo_partner.to_lowercase().replace(' ', "_").replace(|c: char| !c.is_alphanumeric() && c != '_', ""));

    // Strategy 1: New Meta format (obfuscated classes)
    let container_sel = Selector::parse("div.pam._3-95, div.pam.uiBoxWhite").unwrap();
    let containers: Vec<_> = document.select(&container_sel).collect();
    
    if !containers.is_empty() {
        for el in containers {
            // Find sender
            let sender_sel = Selector::parse("div._3-96._2pio, div._2lel").unwrap();
            let sender = el.select(&sender_sel).next()
                .map(|e| e.text().collect::<Vec<_>>().join(" ").trim().to_string())
                .unwrap_or_default();

            // Find text
            let text_sel = Selector::parse("div._2let, div[class*='msg']").unwrap();
            let text = el.select(&text_sel).next()
                .map(|e| e.text().collect::<Vec<_>>().join(" ").trim().to_string())
                .unwrap_or_default();

            // Find date
            let date_sel = Selector::parse("div._3-94._2lem, div[class*='meta']").unwrap();
            let date_str = el.select(&date_sel).next()
                .map(|e| e.text().collect::<Vec<_>>().join(" ").trim().to_string())
                .unwrap_or_default();

            if !sender.is_empty() && (!text.is_empty() || !date_str.is_empty()) {
                let decoded_text = decode_facebook_text(&text);
                let decoded_sender = decode_facebook_text(&sender);

                let timestamp = try_parse_fb_date(&date_str).unwrap_or(0);
                
                // Keep it simple for now, we don't have owner DID here so leave blank or dummy
                messages.push(crate::models::VaultMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    owner_did: None, // Filled during commit
                    convo_id: convo_id.clone(),
                    sender_did: format!("fb_user:{}", decoded_sender.to_lowercase().replace(' ', "_")),
                    sender_handle: decoded_sender.clone(),
                    text: decoded_text,
                    sent_at: if timestamp > 0 { timestamp.to_string() } else { date_str.clone() },
                    source: "facebook".to_string(),
                    imported_at: chrono::Utc::now().timestamp() as u64,
                    peer_did: Some(format!("fb_user:{}", convo_partner.to_lowercase().replace(' ', "_"))),
                    peer_handle: Some(decode_facebook_text(&convo_partner)),
                });
            }
        }
    }

    // Strategy 2: Old format
    if messages.is_empty() {
        let msg_sel = Selector::parse("div.message").unwrap();
        for el in document.select(&msg_sel) {
            let sender_sel = Selector::parse("span.user").unwrap();
            let _sender = el.select(&sender_sel).next()
                .map(|e| e.text().collect::<Vec<_>>().join(" ").trim().to_string())
                .unwrap_or_default();

            let date_sel = Selector::parse("span.meta").unwrap();
            let _date_str = el.select(&date_sel).next()
                .map(|e| e.text().collect::<Vec<_>>().join(" ").trim().to_string())
                .unwrap_or_default();

            // Content is typically the next sibling of div.message with class .message_content, 
            // but sometimes inside. Let's just find the first p tag or adjacent div.
            // For simplicity, skip if we can't cleanly parse old structure here
        }
    }

    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_pure_ascii() {
        assert_eq!(decode_facebook_text("Hello, world!"), "Hello, world!");
    }

    #[test]
    fn test_decode_double_encoded_latin() {
        // "café" in UTF-8 is [63 61 66 C3 A9]. Facebook stores each byte as
        // a \u00XX escape, which the JSON parser turns into chars U+0063,
        // U+0061, U+0066, U+00C3, U+00A9 (i.e. "cafÃ©").
        let input = "caf\u{00c3}\u{00a9}";
        assert_eq!(decode_facebook_text(input), "café");
    }

    #[test]
    fn test_decode_double_encoded_emoji() {
        // 😀 (U+1F600) = UTF-8 [F0 9F 98 80]. Double-encoded as \u00f0\u009f\u0098\u0080.
        let input = "\u{00f0}\u{009f}\u{0098}\u{0080}";
        assert_eq!(decode_facebook_text(input), "😀");
    }

    #[test]
    fn test_decode_double_encoded_cjk() {
        // 中 (U+4E2D) = UTF-8 [E4 B8 AD]. Double-encoded as \u00e4\u00b8\u00ad.
        let input = "\u{00e4}\u{00b8}\u{00ad}";
        assert_eq!(decode_facebook_text(input), "中");
    }

    #[test]
    fn test_decode_double_encoded_cyrillic() {
        // Й (U+0419) = UTF-8 [D0 99]. Double-encoded as \u00d0\u0099.
        let input = "\u{00d0}\u{0099}";
        assert_eq!(decode_facebook_text(input), "Й");
    }

    #[test]
    fn test_decode_mixed_content_preserves_existing_unicode() {
        // Mixed: already-correct emoji 😀 (U+1F600, ≥ 256) plus
        // double-encoded "café" (\u00c3\u00a9).
        // The old code would drop 😀 silently. The new code preserves it.
        let input = format!("{} caf\u{00c3}\u{00a9}", "😀");
        assert_eq!(decode_facebook_text(&input), "😀 café");
    }

    #[test]
    fn test_decode_already_correct_unicode_passthrough() {
        // Already-correct non-Latin text should pass through unchanged.
        let input = "Hello 世界 🌍";
        assert_eq!(decode_facebook_text(input), "Hello 世界 🌍");
    }

    #[test]
    fn test_decode_empty_string() {
        assert_eq!(decode_facebook_text(""), "");
    }
}
