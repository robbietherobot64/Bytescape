use axum::{
    extract::{DefaultBodyLimit, Multipart, State},
    http::HeaderMap,
    routing::post,
    Json, Router,
};
use serde::Serialize;
use uuid::Uuid;
use tokio::io::AsyncWriteExt;
use std::io::Read;
use regex::Regex;
use std::sync::LazyLock;

use crate::{auth, db, error::AppError, AppState};

/// Regex for extracting images from HTML fragments in Facebook export.
static FB_IMG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r##"<img[^>]+src="(your_facebook_activity[^"]+)"[^>]*>"##
).unwrap());

/// Regex for extracting video elements from HTML fragments.
static FB_VIDEO_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r##"<(?:video|source)[^>]+src="(your_facebook_activity/[^"]+\.(?:mp4|mov|webm))"[^>]*>"##
).unwrap());

/// Regex for <a> tags linking to video files (Facebook sometimes uses this pattern).
static FB_A_VIDEO_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r##"<a[^>]+href="(your_facebook_activity/[^"]+\.(?:mp4|mov|webm))"[^>]*>"##
).unwrap());

// ─── Shared pre-compiled regexes ────────────────────────────────────────────

/// Date pattern: "Jan 15, 2024 3:45:30 pm"
static DATE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+(\d{1,2}),\s+(\d{4})\s+(\d{1,2}):(\d{2}):(\d{2})\s+(am|pm)"
).unwrap());

/// Whitespace collapse
static WS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static WS_SPACES_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]+").unwrap());
static WS_NEWLINES_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").unwrap());
static ALBUM_LABEL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"(?i)(Mobile uploads|Timeline photos|Cover photos|Profile pictures|Uncategorized photos)"
).unwrap());

// ─── should_skip_post regexes ──────────────────────────────────────────────

static SKIP_ACTION_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"(?i)(?:commented\s+on|replied\s+to|updated\s+(?:his|her|their)\s+status|updated\s+(?:his|her|their)\s+cover\s+photo|updated\s+(?:his|her|their)\s+profile\s+(?:picture|video)|is\s+following|is\s+with|was\s+with|changed\s+(?:his|her|their)\s+birthday|added\s+a\s+life\s+event|became\s+engaged|started\s+a\s+relationship|ended\s+a\s+relationship|created\s+(?:an\s+event|a\s+life\s+event)|shared\s+a\s+memory|shared\s+\d+\s+photos?|posted\s+a\s+link\s+to|posted\s+a\s+photo|posted\s+a\s+video|is\s+(?:feeling|celebrating)|is\s+traveling\s+to|is\s+at|was\s+at|checked\s+in|reacted\s+to|was\s+tagged\s+in)"
).unwrap());

static SKIP_ALBUM_META_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"(?i)^(Taken(?:\s+on)?|Generated\s+by)\s+(?:by\s+)?[A-Z][a-z]+\s+(?:on\s+)?(?:Sunday|Monday|Tuesday|Wednesday|Thursday|Friday|Saturday)?,?\s*(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+\d{1,2},?\s+\d{4}"
).unwrap());

static SKIP_PHOTO_COUNT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"(?i)^-?\s*\d*\s*(?:photos?|videos?|updates?)\s*(?:-\s*\d+\s*(?:photos?|videos?|updates?))?\s*(?:Month|Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)"
).unwrap());

static SKIP_ALBUM_LABEL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"(?i)^(?:Timeline Photos?|Mobile Uploads?|Cover Photos?|Profile Pictures?)\s*[:\-]?\s*\d+\s*(?:photos?|videos?)\s*"
).unwrap());

static SKIP_UPDATE_TS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"(?i)^Updated (?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+\d{1,2},?\s+\d{4}\s+\d{1,2}:\d{2}:\d{2}\s*(?:am|pm)"
).unwrap());

static SKIP_COMMENT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"(?i)^\s*(?:reply|replied|comment|commented|respostou|responder|Reply to|@\w+)\b"
).unwrap());

// ─── clean_facebook_content regexes ─────────────────────────────────────────

static CLEAN_ACTION_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"^[A-Z][a-z]+ [A-Z][a-z]+ (?:updated his|updated her|updated their|shared [a-z]+|added \d* new photos?|is with|was with|is at|was at|is feeling|was feeling|added a new photo|added a new video|created an event|checked in|commented on|replied to|reacted to|shared a memory|posted a link to)(?: [^.]*\.\s*)?"
).unwrap());

static CLEAN_TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"(?i)(?:you|me|user) (?:tagged|was tagged) [\w\s]+ (?:in )?\w+ (?:photos?|tags?|timeline|posts?)[\.\s]*"
).unwrap());

static CLEAN_SHARED_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"\b(?:Shared a (?:link|post|photo|video))\b\.?"
).unwrap());

static CLEAN_EXIF_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"(?:Camera|ISO|Aperture|Exposure|Focal Length|Flash|Metering Mode|Focal Ratio|Color Space|Make|Model|Orientation|Software|File Size|Image Dimensions|XResolution|YResolution)\s*:\s*[^;\n]*(?:;|\.|\n)?"
).unwrap());

static CLEAN_IP_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b"
).unwrap());

static CLEAN_ALBUM_LABELS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"\b(?:Timeline Photos?|Mobile [Uu]ploads?|Cover Photos?|Profile Pictures?|Added by Facebook)\b\.?"
).unwrap());

static CLEAN_UPDATE_TS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"(?i)Updated (?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+\d{1,2},\s+\d{4}\s+\d{1,2}:\d{2}:\d{2}\s+(?:am|pm)\.?\s*"
).unwrap());

static CLEAN_TAKEN_ON_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"(?i)Taken\s+(?:on\s+)?(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+\d{1,2},\s+\d{4}\s+\d{1,2}:\d{2}:\d{2}\s+(?:am|pm)\.?\s*"
).unwrap());

static CLEAN_TAKEN_PLACE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"(?i)Taken\s+[\w\s,.\-']+\s+\d{4}\.?\s*"
).unwrap());

static CLEAN_GPS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"(?i)(?:lat(?:itude)?[\s:]*\d{1,3}\.\d+[\s°,]*[NS]?[,\s]*lon(?:gitude)?[\s:]*\d{1,3}\.\d+[\s°,]*[EW]?|\d{1,3}\.\d+°?\s*[NS],?\s*\d{1,3}\.\d+°?\s*[EW])\s*\.?"
).unwrap());

static CLEAN_EXTRA_EXIF_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"(?i)(?:Exposure Time|F Number|Max Aperture Value|Brightness Value|Exposure Bias|Digital Zoom Ratio|Focal Length in 35mm Film|Contrast|Saturation|Sharpness|Scene Capture Type|Exposure Program|Subject Distance Range|Custom Rendered|White Balance)\s*:\s*[^;\n]*(?:;|\.|\n)?"
).unwrap());

static CLEAN_PHOTO_COUNT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"(?i)(?:^|\s)-?\s*\d+\s*(?:photos?|videos?|updates?)\s*-?\s*(?:\d+\s*(?:photos?|videos?|updates?))?\s*"
).unwrap());

static CLEAN_ALBUM_TITLE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"(?i)^[\w\s']+\s*[-–—:]*\s*\d+\s*(?:photos?|videos?|items?)\s*$"
).unwrap());

// ─── extract_posts_from_html regexes ────────────────────────────────────────

static ACTION_PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"^(?P<name>[A-Z][a-z]+ [A-Z][a-z]+)\s+(?P<action>updated (?:his|her|their) status|shared (?:a |an )?[a-z]+(?: [a-z]+)*|added \d* new photos?|is with|was with|is at|was at|is feeling|was feeling|added a new photo|added a new video|created an event|checked in|commented on|replied to|reacted to|shared a memory|posted a link to(?: the group: [^.]+)?|is following|was tagged in|updated (?:his|her|their) cover photo|updated (?:his|her|their) profile (?:picture|video)|added \d+ new photos?|is celebrating|wrote on [^.]+|shared an event|shared a product)\b[^.]*\.\s*"
).unwrap());

static POSTED_IN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"^(?P<name>[A-Z][a-z]+ [A-Z][a-z]+) posted in (?P<group>[^.]+)\.\s*"
).unwrap());

static LOCATION_ONLY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"^(?:in\s+)?[A-Z][a-zA-Z\s.']+(?:,\s*[A-Z]{2})?(?:,\s*[A-Z][a-zA-Z\s.']+)?$"
).unwrap());

// ─── extract_images_from_2pin regexes ──────────────────────────────────────

static PIN_IMG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r##"<img[^>]+src="(your_facebook_activity/posts/media/[^"]+)"[^>]*>"##
).unwrap());

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/import/facebook", post(import_facebook_archive))
        .layer(DefaultBodyLimit::disable())
}

fn extract_user_id(headers: &HeaderMap, secret: &str) -> Result<Uuid, AppError> {
    let auth = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Auth("Missing or invalid Authorization header".into()))?;
    let claims = auth::validate_token(auth, secret)?;
    Uuid::parse_str(&claims.sub).map_err(|_| AppError::Auth("Invalid user ID in token".into()))
}

#[derive(Serialize)]
struct ImportResponse {
    imported: usize,
    session_id: String,
    errors: Vec<String>,
}

fn parse_fb_timestamp(val: &serde_json::Value) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Some(ts) = val.as_i64() {
        return chrono::DateTime::from_timestamp(ts, 0);
    }
    if let Some(s) = val.as_str()
        && let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
            return Some(dt.with_timezone(&chrono::Utc));
        }
    None
}

#[derive(Clone)]
pub struct ParsedPost {
    pub content_text: String,
    pub display_name: Option<String>,
    pub action_text: Option<String>,
    pub group_name: Option<String>,
    pub link_url: Option<String>,
    pub link_title: Option<String>,
    pub original_ts: chrono::DateTime<chrono::Utc>,
    pub media_urls: Vec<String>,
}

// ─── Date parsing helpers ───────────────────────────────────────────────────

fn parse_fb_date(date_text: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    DATE_RE.captures(date_text).and_then(|dc| {
        let month_num = match &dc[1] {
            "Jan"=>1,"Feb"=>2,"Mar"=>3,"Apr"=>4,"May"=>5,"Jun"=>6,
            "Jul"=>7,"Aug"=>8,"Sep"=>9,"Oct"=>10,"Nov"=>11,"Dec"=>12,_=>1,
        };
        let day: u32 = dc[2].parse().ok()?;
        let year: i32 = dc[3].parse().ok()?;
        let hour_raw: u32 = dc[4].parse().unwrap_or(12);
        let minute: u32 = dc[5].parse().unwrap_or(0);
        let second: u32 = dc[6].parse().unwrap_or(0);
        let hour = match &dc[7] {
            "am" if hour_raw == 12 => 0,
            "pm" if hour_raw != 12 => hour_raw + 12,
            _ => hour_raw,
        };
        chrono::NaiveDate::from_ymd_opt(year, month_num, day)?
            .and_hms_opt(hour, minute, second)
            .map(|dt| chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc))
    })
}

// ─── Content cleaning ───────────────────────────────────────────────────────

fn should_skip_post(text: &str, original: Option<&str>) -> bool {
    let text = text.trim();
    if text.len() < 10 { return true; }
    if text.starts_with("Appropriateness of content for") { return true; }

    // Also check the ORIGINAL text before action-prefix decomposition
    if let Some(orig) = original {
        let orig = orig.trim();
        if orig.len() >= 10 {
            if SKIP_ACTION_RE.is_match(orig) { return true; }
        }
    }

    // Taken / Generated by patterns (album metadata)
    if SKIP_ALBUM_META_RE.is_match(text) { return true; }

    // Photo metadata cards: "Haines Falls, NY 2023 - 0 photos"
    if SKIP_PHOTO_COUNT_RE.is_match(text) { return true; }

    // Album metadata: "Timeline photos - 7 photos Dec 11, 2024"
    if SKIP_ALBUM_LABEL_RE.is_match(text) { return true; }

    // "Updated May 04, 2022 5:56:34 am" type timestamps (Facebook edit markers)
    if SKIP_UPDATE_TS_RE.is_match(text) { return true; }

    // Comments / replies (still check final text in case decomposition missed it)
    if SKIP_COMMENT_RE.is_match(text) { return true; }

    // Empty shells: just a name with no content
    if text.chars().filter(|c| c.is_alphanumeric()).count() < 5 { return true; }

    false
}

fn clean_facebook_content(text: &str) -> String {
    let mut result = text.to_string();

    // Strip action prefixes like "Rob Meyer updated his status. "
    result = CLEAN_ACTION_RE.replace(&result, "").to_string();

    // Strip "You tagged X in User Timeline photos" patterns
    result = CLEAN_TAG_RE.replace_all(&result, "").to_string();

    // Strip "Shared a ..." standalone
    result = CLEAN_SHARED_RE.replace_all(&result, "").to_string();

    // Strip EXIF/camera metadata
    result = CLEAN_EXIF_RE.replace_all(&result, "").to_string();

    // Strip IP addresses
    result = CLEAN_IP_RE.replace_all(&result, "").to_string();

    // Strip generic photo album labels
    result = CLEAN_ALBUM_LABELS_RE.replace_all(&result, "").to_string();

    // Strip update timestamp markers
    result = CLEAN_UPDATE_TS_RE.replace_all(&result, "").to_string();

    // Strip "Taken ..." from album view (with or without "on" prefix)
    result = CLEAN_TAKEN_ON_RE.replace_all(&result, "").to_string();

    // Strip "Taken [Place] [Year]" without "on" prefix (e.g. "Taken Haines Falls, NY 2023")
    result = CLEAN_TAKEN_PLACE_RE.replace_all(&result, "").to_string();

    // Strip GPS coordinate patterns (lat/long)
    result = CLEAN_GPS_RE.replace_all(&result, "").to_string();

    // Strip additional camera EXIF fields not already covered
    result = CLEAN_EXTRA_EXIF_RE.replace_all(&result, "").to_string();

    // Strip photo count labels in various formats (e.g. "- 0 photos", "7 photos - Update")
    result = CLEAN_PHOTO_COUNT_RE.replace_all(&result, "").to_string();

    // Strip album metadata with location names (e.g. "AlbumName - 5 photos")
    result = CLEAN_ALBUM_TITLE_RE.replace_all(&result, "").to_string();

    // Strip leading/trailing punctuation and whitespace
    result = result.trim().trim_start_matches(['.', ',', '!', '?', '-', ' ']).to_string();

    // Strip album labels (Mobile uploads, Timeline photos, etc.) — remove all occurrences as substrings
    result = ALBUM_LABEL_RE.replace_all(&result, "").to_string();
    // Clean up artifacts left after stripping
    result = result.replace("Place:", "Place:").trim().to_string();

    // Collapse spaces/tabs but preserve newlines
    result = WS_SPACES_RE.replace_all(&result, " ").trim().to_string()
        .replace(" \n", "\n")
        .replace("\n ", "\n")
        .trim().to_string();
    // Collapse 3+ newlines to 2
    result = WS_NEWLINES_RE.replace_all(&result, "\n\n").to_string();

    // Deduplicate consecutive identical lines (e.g. "Go home mother nature\nGo home mother nature")
    let lines: Vec<&str> = result.lines().collect();
    let mut deduped_lines: Vec<&str> = Vec::new();
    for line in &lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if deduped_lines.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
                continue;
            }
            deduped_lines.push(line);
            continue;
        }
        if let Some(prev) = deduped_lines.last() {
            if prev.trim() == trimmed {
                continue; // Skip duplicate consecutive line
            }
        }
        deduped_lines.push(line);
    }
    result = deduped_lines.join("\n");

    result
}

/// Same as clean_facebook_content but explicitly preserves newlines throughout all cleaning steps.
fn clean_facebook_content_preserving_newlines(text: &str) -> String {
    clean_facebook_content(text)
}

// ─── Image extraction helpers ──────────────────────────────────────────────

/// Extract local image paths from HTML `_2pin` elements.
/// Filters out stickers, UI icons, and tiny images.
fn extract_images_from_2pin(pin_html: &str) -> Vec<String> {
    PIN_IMG_RE.captures_iter(pin_html)
        .map(|c| c[1].to_string())
        .filter(|p| {
            // Skip stickers directory
            if p.contains("/stickers_used/") { return false; }
            // Skip tiny icon images (typically UI elements)
            if p.contains("_n_") {
                // Facebook photo filenames: long numeric IDs are real photos
                // Short names or known UI patterns are icons
                let filename = p.rsplit('/').next().unwrap_or(p);
                // Real photos have long numeric names like 470683772_1524442305614448_7767541228344759791_n_1524442168947795.jpg
                // Icons/stickers have shorter names
                if filename.len() < 40 { return false; }
            }
            true
        })
        .collect()
}

/// Resolve a ZIP media path to a URL path.
fn media_url_from_zip_path(zip_path: &str) -> String {
    // Strip the prefix to get relative path, then URL-encode it
    let relative = zip_path
        .strip_prefix("your_facebook_activity/posts/media/")
        .unwrap_or(zip_path);
    format!("/uploads/import/{}", relative)
}

// ─── HTML parsing ──────────────────────────────────────────────────────────

/// Main HTML post parser.
/// Re-designed from first principles:
/// Each `div._a6-g` is ONE timeline entry containing action header + content + timestamp.
/// Uses DOM traversal via scraper crate, not regex on HTML strings.
fn extract_posts_from_html(html: &str, _errors: &mut Vec<String>) -> Vec<ParsedPost> {
    let document = scraper::Html::parse_document(html);
    let mut posts = Vec::new();

    let entry_sel = match scraper::Selector::parse("div._a6-g") {
        Ok(s) => s,
        Err(_) => return posts,
    };
    let action_sel = scraper::Selector::parse("div._a6-h").unwrap();
    let pin_sel = scraper::Selector::parse("div._2pin").unwrap();
    let a_sel = scraper::Selector::parse("a[href]").unwrap();
    let img_sel = scraper::Selector::parse("img[src]").unwrap();
    let ts_sel = scraper::Selector::parse("div._a72d").unwrap();

    for entry in document.select(&entry_sel) {
        // 1. Action header: "Rob Meyer updated his status." etc
        let action_text_raw = entry.select(&action_sel).next()
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .unwrap_or_default();

        // Skip birthday entries
        if action_text_raw.starts_with("Born on") {
            continue;
        }

        // 2. Timestamp from _a72d
        let date_text = entry.select(&ts_sel).next()
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .unwrap_or_default();
        let ts = parse_fb_date(&date_text).unwrap_or_else(chrono::Utc::now);

        // 3. Extract content from _2pin divs
        let mut post_text = String::new();
        let mut all_media_urls = Vec::new();
        let mut link_url: Option<String> = None;
        let mut link_title: Option<String> = None;
        let mut seen_urls = std::collections::HashSet::new();

        for pin_el in entry.select(&pin_sel) {
            let inner_html = pin_el.inner_html();
            // Replace <br> tags with newlines before extracting text
            let html_with_newlines = inner_html
                .replace("<br>", "\n")
                .replace("<br/>", "\n")
                .replace("<br />", "\n")
                .replace("<br/ >", "\n")
                .replace("</div>", "\n")
                .replace("</p>", "\n");
            let doc = scraper::Html::parse_fragment(&html_with_newlines);
            let raw_text = doc.root_element().text().collect::<Vec<_>>().join("");

            // Collapse spaces/tabs but PRESERVE newlines
            let text_clean = WS_SPACES_RE.replace_all(&raw_text, " ").trim().to_string()
                .replace(" \n", "\n")
                .replace("\n ", "\n")
                .trim().to_string();

            // Skip "Updated X" metadata divs
            if text_clean.starts_with("Updated ") && DATE_RE.is_match(&text_clean) {
                continue;
            }

            // Extract link_url and media from <a href> FIRST — before the
            // text-based skips below. Gap fix (same class as the album parser):
            // image-only/link-only `_2pin` blocks have empty text, and the old
            // order hit `is_empty()` first, silently dropping their media.
            for a_el in pin_el.select(&a_sel) {
                if let Some(href) = a_el.value().attr("href") {
                    // Skip internal media links
                    if href.contains("your_facebook_activity/posts/media/") {
                        let url = media_url_from_zip_path(href);
                        if !seen_urls.contains(&url) {
                            seen_urls.insert(url.clone());
                            all_media_urls.push(url);
                        }
                        continue;
                    }
                    if href.starts_with('#') || href.starts_with("your_facebook_activity") {
                        continue;
                    }
                    if href.contains("facebook.com") || href.contains("fb.com") {
                        continue;
                    }
                    // Check if it's a direct media URL (gif, image, video)
                    let lower_href = href.to_lowercase();
                    if lower_href.ends_with(".gif") || lower_href.ends_with(".jpg")
                        || lower_href.ends_with(".jpeg") || lower_href.ends_with(".png")
                        || lower_href.ends_with(".mp4") || lower_href.ends_with(".webm")
                        || lower_href.ends_with(".webp") {
                        if !seen_urls.contains(href) {
                            seen_urls.insert(href.to_string());
                            all_media_urls.push(href.to_string());
                        }
                        continue;
                    }
                    // Set link_url from first external link found
                    if link_url.is_none() {
                        let anchor_text = a_el.text().collect::<Vec<_>>().join(" ").trim().to_string();
                        link_url = Some(href.to_string());
                        link_title = if anchor_text.is_empty() || anchor_text == href {
                            None
                        } else {
                            Some(anchor_text)
                        };
                    }
                }
            }

            // Extract media from <img src> tags
            for img_el in pin_el.select(&img_sel) {
                if let Some(src) = img_el.value().attr("src") {
                    if src.contains("/stickers_used/") { continue; }
                    if src.contains("your_facebook_activity/posts/media/") {
                        let url = media_url_from_zip_path(src);
                        if !seen_urls.contains(&url) {
                            seen_urls.insert(url.clone());
                            all_media_urls.push(url);
                        }
                    }
                }
            }

            // Skip empty
            if text_clean.is_empty() {
                continue;
            }
            // Skip if it's just the date repeated
            if text_clean == date_text {
                continue;
            }

            // Clean the content
            let cleaned = clean_facebook_content_preserving_newlines(&text_clean);
            if cleaned.is_empty() {
                continue;
            }

            // Skip if this is just a raw URL that we already extracted as link_url
            if (cleaned.starts_with("http://") || cleaned.starts_with("https://"))
                && link_url.as_deref() == Some(&cleaned) {
                continue;
            }

            // Detect link metadata: text contains an embedded URL (e.g. "SpotifyWhere the Waves are Highesthttps://open.spotify.com/...")
            // Extract the URL as link_url and skip this _2pin entirely (it's metadata, not content)
            if let Some(url_start) = cleaned.find("https://").or_else(|| cleaned.find("http://")) {
                let rest = &cleaned[url_start..];
                // Find end of URL (first whitespace or end of string)
                let url_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                let extracted_url = &rest[..url_end];
                if extracted_url.len() > 10 && extracted_url.contains('.') {
                    // This is link metadata — set link_url and skip this text
                    if link_url.is_none() {
                        link_url = Some(extracted_url.to_string());
                    }
                    continue;
                }
            }

            // Add to post text (dedup: don't add if identical to existing)
            if !cleaned.is_empty() && cleaned != post_text {
                if !post_text.is_empty() { post_text.push_str("\n"); }
                post_text.push_str(&cleaned);
            }
        }

        // 4. Skip empty posts (no text and no media)
        if post_text.is_empty() && all_media_urls.is_empty() {
            continue;
        }

        // 5. Decompose action header: "Rob Meyer updated his status." -> display_name + action_text
        let mut display_name: Option<String> = None;
        let mut action_text: Option<String> = None;
        let mut group_name: Option<String> = None;

        if let Some(caps) = POSTED_IN_RE.captures(&action_text_raw) {
            display_name = Some(caps[1].to_string());
            group_name = Some(caps["group"].trim().to_string());
        } else if let Some(caps) = ACTION_PREFIX_RE.captures(&action_text_raw) {
            display_name = Some(caps[1].to_string());
            action_text = Some(caps["action"].to_string());
        }

        // 6. Skip if post text is just a date
        if !post_text.is_empty() && DATE_RE.is_match(&post_text) && all_media_urls.is_empty() {
            continue;
        }

        // 7. Skip if too short and no media
        if post_text.len() < 3 && all_media_urls.is_empty() {
            continue;
        }

        posts.push(ParsedPost {
            content_text: post_text,
            display_name,
            action_text,
            group_name,
            link_url,
            link_title,
            original_ts: ts,
            media_urls: all_media_urls,
        });
    }

    posts
}

// ─── Media-file HTML parser ────────────────────────────────────────────────
/// Parse photo/video/album HTML files using the same _a6-g structure as timeline posts.
/// These files have the same DOM structure — action header + _2pin content + timestamp.
fn extract_media_posts_from_html(html: &str, album_name: Option<&str>) -> Vec<ParsedPost> {
    let document = scraper::Html::parse_document(html);
    let mut posts = Vec::new();

    let entry_sel = match scraper::Selector::parse("div._a6-g") {
        Ok(s) => s,
        Err(_) => return posts,
    };
    let action_sel = scraper::Selector::parse("div._a6-h").unwrap();
    let pin_sel = scraper::Selector::parse("div._2pin").unwrap();
    let img_sel = scraper::Selector::parse("img[src]").unwrap();
    let a_sel = scraper::Selector::parse("a[href]").unwrap();
    let ts_sel = scraper::Selector::parse("div._a72d").unwrap();

    for entry in document.select(&entry_sel) {
        let action_text_raw = entry.select(&action_sel).next()
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .unwrap_or_default();

        if action_text_raw.starts_with("Born on") {
            continue;
        }

        let date_text = entry.select(&ts_sel).next()
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .unwrap_or_default();
        let ts = parse_fb_date(&date_text).unwrap_or_else(chrono::Utc::now);

        let mut media_urls = Vec::new();
        let mut post_text = String::new();
        let mut seen_urls = std::collections::HashSet::new();

        for pin_el in entry.select(&pin_sel) {
            let inner_html = pin_el.inner_html();
            let html_with_newlines = inner_html
                .replace("<br>", "\n")
                .replace("<br/>", "\n")
                .replace("<br />", "\n")
                .replace("<br/ >", "\n")
                .replace("</div>", "\n")
                .replace("</p>", "\n");
            let doc = scraper::Html::parse_fragment(&html_with_newlines);
            let raw_text = doc.root_element().text().collect::<Vec<_>>().join("");
            let text_clean = WS_SPACES_RE.replace_all(&raw_text, " ").trim().to_string()
                .replace(" \n", "\n")
                .replace("\n ", "\n")
                .trim().to_string();

            if text_clean.starts_with("Updated ") && DATE_RE.is_match(&text_clean) {
                continue;
            }

            // Gap fix (fixture: album with 3 images): collect media BEFORE the
            // text-based skips below. Album/photo `_2pin` blocks are frequently
            // image-only (no text at all) — the old order hit `is_empty()`
            // first and silently dropped their <img>/<a> media, so an album of
            // 3 images parsed as 0 media entries.
            for img_el in pin_el.select(&img_sel) {
                if let Some(src) = img_el.value().attr("src") {
                    if src.contains("/stickers_used/") { continue; }
                    if src.contains("your_facebook_activity/posts/media/") {
                        let url = media_url_from_zip_path(src);
                        if !seen_urls.contains(&url) {
                            seen_urls.insert(url.clone());
                            media_urls.push(url);
                        }
                    }
                }
            }

            for a_el in pin_el.select(&a_sel) {
                if let Some(href) = a_el.value().attr("href") {
                    if href.contains("your_facebook_activity/posts/media/") {
                        let url = media_url_from_zip_path(href);
                        if !seen_urls.contains(&url) {
                            seen_urls.insert(url.clone());
                            media_urls.push(url);
                        }
                    }
                }
            }

            if text_clean.is_empty() || text_clean == date_text {
                continue;
            }

            let cleaned = clean_facebook_content(&text_clean);
            if !cleaned.is_empty() && cleaned != post_text {
                if !post_text.is_empty() { post_text.push_str("\n"); }
                post_text.push_str(&cleaned);
            }
        }

        if media_urls.is_empty() && post_text.is_empty() {
            continue;
        }

        let content_text = if post_text.is_empty() {
            album_name
                .map(|a| format!("{} - {} media", a, media_urls.len()))
                .unwrap_or_else(|| format!("{} media item(s)", media_urls.len()))
        } else {
            post_text
        };

        let mut display_name: Option<String> = None;
        let mut action_text: Option<String> = None;
        if let Some(caps) = ACTION_PREFIX_RE.captures(&action_text_raw) {
            display_name = Some(caps[1].to_string());
            action_text = Some(caps["action"].to_string());
        }

        posts.push(ParsedPost {
            content_text,
            display_name,
            action_text,
            group_name: album_name.map(|a| a.to_string()),
            link_url: None,
            link_title: None,
            original_ts: ts,
            media_urls,
        });
    }

    posts
}

// ─── JSON parsing ──────────────────────────────────────────────────────────
/// Extract posts from JSON format (older Facebook exports).
fn extract_posts_from_json(json_val: &serde_json::Value, _errors: &mut Vec<String>) -> Vec<ParsedPost> {
    let mut posts = Vec::new();

    let posts_data: Vec<&serde_json::Value> = if let Some(items) = json_val.as_array() {
        items.iter().collect()
    } else if let Some(items) = json_val.get("posts").and_then(|v| v.as_array()) {
        items.iter().collect()
    } else if let Some(items) = json_val.get("status_updates").and_then(|v| v.as_array()) {
        items.iter().collect()
    } else {
        return posts;
    };

    for post_val in &posts_data {
        let title = post_val.get("title").and_then(|v| v.as_str()).unwrap_or("");

        // Skip birthday/life event entries
        if title.starts_with("Born on") || title.contains("life event") {
            continue;
        }

        // Skip comment entries — these appear in the posts file but are actually
        // comments. Detect by: no `post` field in any data item, no media description,
        // only stickers or external links as attachments.
        if title.contains("commented on") {
            continue;
        }
        // Also detect comment duplicates: entries where data has no `post` field
        // and the only attachments are stickers or external links (not real media)
        let has_post_text = post_val.get("data")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().any(|d| d.get("post").and_then(|v| v.as_str()).map(|s| !s.trim().is_empty()).unwrap_or(false)))
            .unwrap_or(false);
        let has_real_media = post_val.get("attachments")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().any(|att| {
                att.get("data").and_then(|v| v.as_array()).map(|d| d.iter().any(|item| {
                    item.get("media").map(|m| {
                        let uri = m.get("uri").and_then(|v| v.as_str()).unwrap_or("");
                        !uri.contains("/stickers_used/") && !uri.contains("ai_stickers") && !uri.is_empty()
                    }).unwrap_or(false) || item.get("place").is_some()
                })).unwrap_or(false)
            }))
            .unwrap_or(false);
        if !has_post_text && !has_real_media {
            // This is likely a comment duplicate — skip it
            continue;
        }

        // Extract timestamp
        let original_ts = post_val.get("timestamp")
            .and_then(parse_fb_timestamp)
            .unwrap_or_else(chrono::Utc::now);

        // Extract post text from data[*].post (first non-empty one)
        let mut content_text = String::new();
        if let Some(data_arr) = post_val.get("data").and_then(|v| v.as_array()) {
            for data_item in data_arr {
                if let Some(post) = data_item.get("post").and_then(|v| v.as_str()) {
                    if !post.trim().is_empty() {
                        content_text = post.to_string();
                        break;
                    }
                }
            }
        }

        // Extract media URLs and links from attachments
        let mut media_urls = Vec::new();
        let mut link_url: Option<String> = None;
        let mut link_title: Option<String> = None;
        let mut seen_media = std::collections::HashSet::new();

        if let Some(attachments) = post_val.get("attachments").and_then(|v| v.as_array()) {
            for att in attachments {
                if let Some(att_data) = att.get("data").and_then(|v| v.as_array()) {
                    for data_item in att_data {
                        // Media (photos, videos) — skip stickers
                        if let Some(media) = data_item.get("media") {
                            if let Some(uri) = media.get("uri").and_then(|v| v.as_str()) {
                                if !uri.contains("/stickers_used/") && !uri.contains("ai_stickers") {
                                    let url = media_url_from_zip_path(uri);
                                    if !seen_media.contains(&url) {
                                        seen_media.insert(url.clone());
                                        media_urls.push(url);
                                    }
                                }
                            }
                            // Use media description as fallback content text
                            if content_text.is_empty() {
                                if let Some(desc) = media.get("description").and_then(|v| v.as_str()) {
                                    if !desc.trim().is_empty() && desc != "Photos" && desc != "Videos" {
                                        content_text = desc.to_string();
                                    }
                                }
                            }
                        }

                        // External links
                        if let Some(ext_ctx) = data_item.get("external_context") {
                            if let Some(url) = ext_ctx.get("url").and_then(|v| v.as_str()) {
                                // Check if it's a direct media URL (gif, image, video)
                                let lower_url = url.to_lowercase();
                                if lower_url.ends_with(".gif") || lower_url.ends_with(".jpg")
                                    || lower_url.ends_with(".jpeg") || lower_url.ends_with(".png")
                                    || lower_url.ends_with(".mp4") || lower_url.ends_with(".webm")
                                    || lower_url.ends_with(".webp") {
                                    // Add as media URL, not as link
                                    if !seen_media.contains(url) {
                                        seen_media.insert(url.to_string());
                                        media_urls.push(url.to_string());
                                    }
                                } else if link_url.is_none() && !url.contains("facebook.com") && !url.contains("fb.com") {
                                    link_url = Some(url.to_string());
                                }
                            }
                        }

                        // Skip life events (already handled above, but just in case)
                        if data_item.get("life_event").is_some() {
                            continue;
                        }
                    }
                }
            }
        }

        // Skip if no content and no media
        if content_text.trim().is_empty() && media_urls.is_empty() && link_url.is_none() {
            continue;
        }

        // Decompose action header from title
        let mut display_name: Option<String> = None;
        let mut action_text: Option<String> = None;
        let mut group_name: Option<String> = None;

        if let Some(caps) = POSTED_IN_RE.captures(title) {
            display_name = Some(caps[1].to_string());
            group_name = Some(caps["group"].trim().to_string());
        } else if let Some(caps) = ACTION_PREFIX_RE.captures(title) {
            display_name = Some(caps[1].to_string());
            action_text = Some(caps["action"].to_string());
        }

        // Clean content text
        let content_text = clean_facebook_content_preserving_newlines(&content_text);

        posts.push(ParsedPost {
            content_text,
            display_name,
            action_text,
            group_name,
            link_url,
            link_title,
            original_ts,
            media_urls,
        });
    }

    posts
}

// ─── Media extraction ──────────────────────────────────────────────────────

/// Extract media files from the ZIP to ./uploads/import/.
/// Returns a set of URL paths that were extracted.
fn extract_media_from_zip(zip: &mut zip::ZipArchive<std::fs::File>, _user_id: Uuid, _errors: &mut Vec<String>) -> Vec<(String, String)> {
    let mut extracted = Vec::new();
    let prefix = "your_facebook_activity/posts/media/";
    let base_uploads = "./uploads/import";

    let _ = std::fs::create_dir_all(base_uploads);

    for i in 0..zip.len() {
        let name = {
            let entry = match zip.by_index(i) {
                Ok(e) => e,
                Err(_) => continue,
            };
            entry.name().to_string()
        };

        // Look for ANY path segment that contains "posts/media/" or "media/"
        let media_idx = if let Some(idx) = name.find(prefix) {
            idx
        } else if let Some(idx) = name.find("/media/") {
            idx + 1  // start from "media/"
        } else {
            continue;
        };

        let relative = &name[media_idx + prefix.len()..];  // e.g. "Coverphotos_O-AmGVHypQ/xxx.jpg"
        if relative.is_empty() || name.ends_with('/') { continue; }

        // Skip sticker files — these are UI elements, not user photos
        if relative.contains("stickers_used/") { continue; }

        let dest_dir = format!("{}/{}", base_uploads, std::path::Path::new(relative).parent().unwrap_or(std::path::Path::new("")).display());
        let dest_path = format!("{}/{}", base_uploads, relative);

        let _ = std::fs::create_dir_all(&dest_dir);

        if let Ok(mut src) = zip.by_index(i)
            && let Ok(mut file) = std::fs::File::create(&dest_path)
                && std::io::copy(&mut src, &mut file).is_ok() {
                    let storage_url = format!("/uploads/import/{}", relative);
                    extracted.push((dest_path, storage_url));
                }
    }

    extracted
}

// ─── Main import handler ───────────────────────────────────────────────────

/// Public entry point for unified import — processes a temp ZIP file path.
pub async fn import_facebook_archive_from_path(
    state: &AppState,
    user_id: Uuid,
    temp_path: &str,
) -> Result<(usize, String, Vec<String>), AppError> {
    let session = db::imports::ImportRow::create(&state.db, user_id, "facebook")
        .await
        .map_err(AppError::Database)?;

    db::imports::ImportRow::start(&state.db, session.id).await.ok();

    let mut errors: Vec<String> = Vec::new();

    // ---- Phase 1: Extract media from ZIP ----
    let std_file = match std::fs::File::open(temp_path) {
        Ok(f) => f,
        Err(e) => return Err(AppError::BadRequest(format!("Failed to open file: {}", e))),
    };
    let mut archive = match zip::ZipArchive::new(std_file) {
        Ok(a) => a,
        Err(e) => return Err(AppError::BadRequest(format!("Invalid ZIP: {}", e))),
    };

    let extracted_media = extract_media_from_zip(&mut archive, user_id, &mut errors);
    if !extracted_media.is_empty() {
        errors.push(format!("Extracted {} media files", extracted_media.len()));
    }

    // Create MediaRow entries for extracted media
    let mut media_count = 0usize;
    for (file_path, storage_url) in &extracted_media {
        let mime = if file_path.ends_with(".mp4") || file_path.ends_with(".mov") {
            "video/mp4"
        } else if file_path.ends_with(".webp") {
            "image/webp"
        } else if file_path.ends_with(".png") {
            "image/png"
        } else {
            "image/jpeg"
        };
        if let Ok(file_meta) = std::fs::metadata(file_path) {
            let _ = crate::db::media::MediaRow::create_for_user(
                &state.db, user_id, storage_url, None, mime,
                None, None, None, Some(file_meta.len() as i64),
            ).await;
            media_count += 1;
        }
    }

    // ---- Phase 2: Parse posts from ZIP ----
    let mut parsed_posts: Vec<ParsedPost> = Vec::new();

    drop(archive);
    let std_file = match std::fs::File::open(temp_path) {
        Ok(f) => f,
        Err(e) => return Err(AppError::BadRequest(format!("Failed to re-open file: {}", e))),
    };
    let mut archive = match zip::ZipArchive::new(std_file) {
        Ok(a) => a,
        Err(e) => return Err(AppError::BadRequest(format!("Invalid ZIP (re-open): {}", e))),
    };

    // JSON files
    for i in 0..archive.len() {
        let path = {
            let entry = match archive.by_index(i) { Ok(e) => e, Err(_) => continue };
            entry.name().to_string()
        };
        if !path.contains("posts") || !path.ends_with(".json") || path.contains("comments") { continue; }

        let content = {
            let mut entry = match archive.by_index(i) { Ok(e) => e, Err(_) => continue };
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).ok();
            buf
        };
        if content.is_empty() { continue; }

        let json_val: serde_json::Value = match serde_json::from_slice(&content) {
            Ok(v) => v,
            Err(e) => { errors.push(format!("JSON parse error in {}: {}", path, e)); continue; }
        };
        parsed_posts.extend(extract_posts_from_json(&json_val, &mut errors));
    }

    // HTML files: process all post/photo/video/album/group files
    for i in 0..archive.len() {
        let path = {
            let entry = match archive.by_index(i) { Ok(e) => e, Err(_) => continue };
            entry.name().to_string()
        };
        if !path.ends_with(".html") { continue; }
        // Skip known non-content files
        let hard_skip = [
            "media_used_for", "edits_you", "content_sharing", "trash",
            "posts_on_other", "places_you_have", "visual_search", "fundraiser",
            "anonymous_posts",
        ];
        if hard_skip.iter().any(|p| path.contains(p)) { continue; }

        let content = {
            let mut entry = match archive.by_index(i) { Ok(e) => e, Err(_) => continue };
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).ok();
            buf
        };
        if content.is_empty() { continue; }
        let html = String::from_utf8_lossy(&content);

        // Route to the appropriate parser based on file type
        if path.contains("group_posts_and_comments") {
            // Group posts have timeline-like structure with group names
            parsed_posts.extend(extract_posts_from_html(&html, &mut errors));
        } else if path.contains("your_photos") || path.contains("uncategorized_photos") {
            // Photo gallery files
            parsed_posts.extend(extract_media_posts_from_html(&html, Some("Photos")));
        } else if path.contains("your_videos") {
            // Video gallery files
            parsed_posts.extend(extract_media_posts_from_html(&html, Some("Videos")));
        } else if path.contains("album/") {
            // Album files — extract album name from path
            let album_name = path
                .split('/')
                .find(|seg| seg.starts_with("album_") || seg.starts_with("album-"))
                .or_else(|| {
                    // Try to get the path segment after "album/"
                    let parts: Vec<&str> = path.split("album/").collect();
                    if parts.len() > 1 {
                        parts[1].split('/').next()
                    } else {
                        None
                    }
                })
                .map(|s| {
                    // Decode URL encoding and strip extension
                    let decoded = s.replace("%20", " ").replace("_", " ").replace(".html", "");
                    decoded.trim().to_string()
                });
            parsed_posts.extend(extract_media_posts_from_html(&html, album_name.as_deref()));
        } else {
            // Default: timeline posts
            parsed_posts.extend(extract_posts_from_html(&html, &mut errors));
        }
    }

    drop(archive);

    // ---- Phase 2b: Deduplicate posts ----
    // The same post may appear in multiple HTML files (timeline, group feed, album, etc.)
    // Use a HashMap: key = fingerprint, value = post (keep best version)
    let mut dedup_map: std::collections::HashMap<String, ParsedPost> = std::collections::HashMap::new();
    for post in parsed_posts.iter() {
        // Fingerprint: timestamp + first media filename (same media at same time = same post)
        let ts_secs = post.original_ts.timestamp();
        let fingerprint = if let Some(first_media) = post.media_urls.first() {
            let filename = first_media.rsplit('/').next().unwrap_or(first_media);
            format!("{}|{}", ts_secs, filename)
        } else {
            // No media: use timestamp + first 200 chars of cleaned text
            let text_prefix: String = post.content_text.chars().take(200).collect();
            format!("{}|{}", ts_secs, text_prefix)
        };

        // When keeping duplicates, prefer the one with MORE media_urls
        if let Some(existing) = dedup_map.get(&fingerprint) {
            // Prefer the post with more content (longer text or more media)
            let new_score = post.content_text.len() + post.media_urls.len() * 100;
            let existing_score = existing.content_text.len() + existing.media_urls.len() * 100;
            if new_score > existing_score {
                dedup_map.insert(fingerprint, post.clone());
            }
        } else {
            dedup_map.insert(fingerprint, post.clone());
        }
    }

    let deduped_posts: Vec<ParsedPost> = dedup_map.into_values().collect();
    let removed_count = parsed_posts.len() - deduped_posts.len();
    if removed_count > 0 {
        errors.push(format!(
            "Dedup: removed {} duplicate posts ({} unique remaining)",
            removed_count,
            deduped_posts.len()
        ));
    }

    // ---- Phase 3: Insert into DB ----
    let mut imported = 0usize;
    for post in deduped_posts {
            let content_obj = serde_json::json!({
                "text": post.content_text,
                "display_name": post.display_name,
                "action_text": post.action_text,
                "group_name": post.group_name,
                "link_url": post.link_url,
                "link_title": post.link_title,
                "media_urls": post.media_urls,
            });

        match db::posts::PostRow::create(
            &state.db, user_id, "facebook", None, None, &content_obj, "short",
            None, None, None, post.original_ts, true, Some(session.id), "public",
        ).await {
            Ok(_) => imported += 1,
            Err(e) => errors.push(format!("DB insert error: {}", e)),
        }
    }

    if imported > 0 {
        // R4: import_sessions must record total/imported media accurately —
        // this previously hard-coded 0 media.
        db::imports::ImportRow::complete(&state.db, session.id, imported as i32, media_count as i32).await.ok();
    } else {
        db::imports::ImportRow::fail(
            &state.db, session.id, &format!("No posts imported. Errors: {}", errors.join("; "))).await.ok();
    }

    Ok((imported, session.id.to_string(), errors))
}

/// Streaming variant of `import_facebook_archive_from_path`.
/// Synchronous parsing — designed to run in a std::thread, NOT in the tokio runtime.
/// Sends events through a std::sync::mpsc channel as it parses.
/// Returns (deduped_posts, errors) for the caller to do DB inserts.
pub fn parse_facebook_archive_sync(
    temp_path: &str,
    tx: &crate::api::import::StdSyncSender,
) -> (Vec<ParsedPost>, Vec<String>) {
    let mut errors: Vec<String> = Vec::new();

    let send = |val: serde_json::Value| {
        let _ = tx.send(val.to_string());
    };

    let base_uploads = "./uploads/import";
    let _ = std::fs::create_dir_all(base_uploads);

    send(serde_json::json!({"type":"phase","phase":"scanning_archive","total":0}));

    let std_file = match std::fs::File::open(temp_path) {
        Ok(f) => f,
        Err(e) => {
            send(serde_json::json!({"type":"error","message":format!("Failed to open file: {}", e)}));
            return (vec![], vec![format!("Failed to open file: {}", e)]);
        }
    };
    let mut archive = match zip::ZipArchive::new(std_file) {
        Ok(a) => a,
        Err(e) => {
            send(serde_json::json!({"type":"error","message":format!("Invalid ZIP: {}", e)}));
            return (vec![], vec![format!("Invalid ZIP: {}", e)]);
        }
    };

    let tracker_categories: &[(&str, &[&str])] = &[
        ("ad_preferences", &["ad_preferences", "advertisers"]),
        ("off_facebook_activity", &["off_facebook_activity", "your_off_facebook_activity"]),
        ("apps_and_websites", &["apps_and_websites"]),
        ("location_history", &["location_history", "location"]),
        ("search_history", &["search_history"]),
        ("device_info", &["device_info"]),
        ("account_activity", &["account_activity"]),
    ];

    let streaming_hard_skip: &[&str] = &[
        "media_used_for", "edits_you", "content_sharing", "trash",
        "posts_on_other", "places_you_have", "visual_search", "fundraiser",
        "anonymous_posts",
        "profile_information", "security_and_login_information",
        "contact_info", "peer_group", "friend_lists", "friends_and_followers",
        "messages", "pokes", "account_management", "preferences", "about_you",
        "your_activity_history", "your_activity_time_spent",
    ];

    let max_file_size: u64 = 100 * 1024 * 1024;
    let total_entries = archive.len() as u64;
    let mut html_files: Vec<(usize, String)> = Vec::new();
    let mut json_files: Vec<usize> = Vec::new();
    let mut media_count = 0u64;
    let mut scan_progress = 0u64;
    let mut zip_index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    // Detect the top-level directory prefix (e.g. "facebook-rabiezaater-2025-01-12-TwiR2QHB/")
    let mut zip_prefix: String = String::new();
    for i in 0..archive.len() {
        let name = match archive.by_index(i) { Ok(e) => e.name().to_string(), Err(_) => continue };
        if name.contains("your_facebook_activity/posts/") {
            if let Some(prefix_end) = name.find("your_facebook_activity/") {
                zip_prefix = name[..prefix_end].to_string();
            }
            break;
        }
    }

    for i in 0..archive.len() {
        scan_progress += 1;
        if scan_progress % 500 == 0 || scan_progress == total_entries {
            send(serde_json::json!({"type":"phase","phase":"scanning_archive","total":total_entries,"progress":scan_progress}));
        }

        let (name, size) = {
            let entry = match archive.by_index(i) { Ok(e) => e, Err(_) => continue };
            (entry.name().to_string(), entry.size())
        };

        zip_index.insert(name.clone(), i);

        for (category, patterns) in tracker_categories {
            if patterns.iter().any(|p| name.contains(p)) {
                send(serde_json::json!({"type":"tracker","category":category,"filename":name.clone(),"count":1}));
                break;
            }
        }

        if name.contains("/media/") && !name.ends_with('/') && !name.contains("stickers_used/") {
            media_count += 1;
        }

        // Only parse HTML files under your_facebook_activity/posts/ — skip everything else
        if name.ends_with(".html") && size <= max_file_size
            && name.contains("your_facebook_activity/posts/")
            && !streaming_hard_skip.iter().any(|p| name.contains(p))
        {
            html_files.push((i, name.clone()));
        }

        if name.contains("posts") && name.ends_with(".json") && !name.contains("comments") && size <= max_file_size {
            json_files.push(i);
        }
    }

    send(serde_json::json!({"type":"phase","phase":"archive_scanned","media_count":media_count,"html_files":html_files.len(),"json_files":json_files.len()}));

    // ---- Phase 2: Parse posts ----
    send(serde_json::json!({"type":"phase","phase":"parsing_posts","total":html_files.len() + json_files.len()}));

    let mut parsed_posts: Vec<ParsedPost> = Vec::new();
    let max_posts = 10000usize;

    for &i in &json_files {
        if parsed_posts.len() >= max_posts { break; }
        let mut entry = match archive.by_index(i) { Ok(e) => e, Err(_) => continue };
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).ok();
        if buf.is_empty() { continue; }

        let json_val: serde_json::Value = match serde_json::from_slice(&buf) {
            Ok(v) => v,
            Err(e) => { errors.push(format!("JSON parse error: {}", e)); continue; }
        };
        for post in extract_posts_from_json(&json_val, &mut errors) {
            if parsed_posts.len() >= max_posts { break; }
            let idx = parsed_posts.len() + 1;
            send(serde_json::json!({
                "type":"post",
                "title":post.content_text.chars().take(100).collect::<String>(),
                "timestamp":post.original_ts.to_rfc3339(),
                "media_count":post.media_urls.len(),
                "index":idx,
            }));
            parsed_posts.push(post);
        }
    }

    let mut html_file_idx = 0usize;
    for (i, path) in &html_files {
        if parsed_posts.len() >= max_posts { break; }
        html_file_idx += 1;
        if html_file_idx % 5 == 0 {
            send(serde_json::json!({"type":"phase","phase":"parsing_posts","total":html_files.len() + json_files.len(),"progress":html_file_idx + json_files.len()}));
        }

        let mut entry = match archive.by_index(*i) { Ok(e) => e, Err(_) => continue };
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).ok();
        if buf.is_empty() { continue; }
        let html = String::from_utf8_lossy(&buf);

        let new_posts: Vec<ParsedPost> = if path.contains("group_posts_and_comments") {
            extract_posts_from_html(&html, &mut errors)
        } else if path.contains("your_photos") || path.contains("uncategorized_photos") {
            extract_media_posts_from_html(&html, Some("Photos"))
        } else if path.contains("your_videos") {
            extract_media_posts_from_html(&html, Some("Videos"))
        } else if path.contains("album/") {
            let album_name = path.rsplit('/').nth(1).map(|s| s.replace("%20", " ").replace("_", " ").replace(".html", ""));
            extract_media_posts_from_html(&html, album_name.as_deref())
        } else {
            extract_posts_from_html(&html, &mut errors)
        };

        for post in new_posts {
            if parsed_posts.len() >= max_posts { break; }
            let idx = parsed_posts.len() + 1;
            send(serde_json::json!({
                "type":"post",
                "title":post.content_text.chars().take(100).collect::<String>(),
                "timestamp":post.original_ts.to_rfc3339(),
                "media_count":post.media_urls.len(),
                "index":idx,
            }));
            parsed_posts.push(post);
        }
    }

    // ---- Phase 2b: Deduplicate ----
    send(serde_json::json!({"type":"phase","phase":"deduplicating","total":parsed_posts.len()}));

    let mut dedup_map: std::collections::HashMap<String, ParsedPost> = std::collections::HashMap::new();
    for post in parsed_posts.iter() {
        let ts_secs = post.original_ts.timestamp();
        let fingerprint = if let Some(first_media) = post.media_urls.first() {
            let filename = first_media.rsplit('/').next().unwrap_or(first_media);
            format!("{}|{}", ts_secs, filename)
        } else {
            let text_prefix: String = post.content_text.chars().take(200).collect();
            format!("{}|{}", ts_secs, text_prefix)
        };

        if let Some(existing) = dedup_map.get(&fingerprint) {
            // Prefer the post with more content (longer text or more media)
            let new_score = post.content_text.len() + post.media_urls.len() * 100;
            let existing_score = existing.content_text.len() + existing.media_urls.len() * 100;
            if new_score > existing_score {
                dedup_map.insert(fingerprint, post.clone());
            }
        } else {
            dedup_map.insert(fingerprint, post.clone());
        }
    }

    let deduped_posts: Vec<ParsedPost> = dedup_map.into_values().collect();
    let removed_count = parsed_posts.len() - deduped_posts.len();
    if removed_count > 0 {
        errors.push(format!("Dedup: removed {} duplicates ({} remaining)", removed_count, deduped_posts.len()));
    }

    // ---- Phase 2c: Extract ONLY referenced media ----
    send(serde_json::json!({"type":"phase","phase":"extracting_media","total":deduped_posts.len()}));

    let mut referenced_media: std::collections::HashSet<String> = std::collections::HashSet::new();
    for post in &deduped_posts {
        for url in &post.media_urls {
            if let Some(relative) = url.strip_prefix("/uploads/import/") {
                referenced_media.insert(format!("{}your_facebook_activity/posts/media/{}", zip_prefix, relative));
            }
        }
    }

    if !referenced_media.is_empty() {
        let file = match std::fs::File::open(temp_path) {
            Ok(f) => f,
            Err(e) => {
                errors.push(format!("Media extraction failed: {}", e));
                return (deduped_posts, errors);
            }
        };
        let mut archive2 = match zip::ZipArchive::new(file) {
            Ok(a) => a,
            Err(e) => {
                errors.push(format!("Media extraction failed: {}", e));
                return (deduped_posts, errors);
            }
        };

        let mut media_idx = 0u64;
        for media_path in &referenced_media {
            let idx = match zip_index.get(media_path) {
                Some(&i) => i,
                None => continue,
            };

            let name = media_path.clone();
            let relative = name.strip_prefix(&format!("{}your_facebook_activity/posts/media/", zip_prefix))
                .unwrap_or(name.strip_prefix("your_facebook_activity/posts/media/").unwrap_or(&name));
            if relative.is_empty() || name.ends_with('/') { continue; }

            let dest_dir = format!("{}/{}", base_uploads, std::path::Path::new(relative).parent().unwrap_or(std::path::Path::new("")).display());
            let dest_path = format!("{}/{}", base_uploads, relative);
            let _ = std::fs::create_dir_all(&dest_dir);

            if let Ok(mut src) = archive2.by_index(idx)
                && let Ok(mut file) = std::fs::File::create(&dest_path)
                && std::io::copy(&mut src, &mut file).is_ok() {
                media_idx += 1;
                let category = if name.ends_with(".mp4") || name.ends_with(".mov") { "video" } else { "photo" };
                let filename = relative.rsplit('/').next().unwrap_or(relative);
                send(serde_json::json!({"type":"media","category":category,"filename":filename,"index":media_idx}));
            }
        }
        errors.push(format!("Extracted {} referenced media files", media_idx));
    }

    (deduped_posts, errors)
}

/// Processes the ZIP archive and sends SSE events over the provided channel.
/// Performance fixes: skips >50MB files, caps at 10K posts, skips individual MediaRow creation,
/// and skips more non-content HTML files aggressively.
pub async fn import_facebook_archive_streaming(
    state: &AppState,
    user_id: Uuid,
    temp_path: &str,
    tx: tokio::sync::mpsc::UnboundedSender<String>,
) {
    let mut errors: Vec<String> = Vec::new();

    let send = |val: serde_json::Value| {
        let _ = tx.send(val.to_string());
    };

    let base_uploads = "./uploads/import";
    let _ = std::fs::create_dir_all(base_uploads);

    // ---- Phase 1: Scan ZIP — count media, detect trackers, identify HTML/JSON files ----
    send(serde_json::json!({"type":"phase","phase":"scanning_archive","total":0}));

    let std_file = match std::fs::File::open(temp_path) {
        Ok(f) => f,
        Err(e) => {
            send(serde_json::json!({"type":"error","message":format!("Failed to open file: {}", e)}));
            send(serde_json::json!({"type":"complete","imported":0,"errors":vec![format!("Failed to open file: {}", e)]}));
            return;
        }
    };
    let mut archive = match zip::ZipArchive::new(std_file) {
        Ok(a) => a,
        Err(e) => {
            send(serde_json::json!({"type":"error","message":format!("Invalid ZIP: {}", e)}));
            send(serde_json::json!({"type":"complete","imported":0,"errors":vec![format!("Invalid ZIP: {}", e)]}));
            return;
        }
    };

    let tracker_categories: &[(&str, &[&str])] = &[
        ("ad_preferences", &["ad_preferences", "advertisers"]),
        ("off_facebook_activity", &["off_facebook_activity", "your_off_facebook_activity"]),
        ("apps_and_websites", &["apps_and_websites"]),
        ("location_history", &["location_history", "location"]),
        ("search_history", &["search_history"]),
        ("device_info", &["device_info"]),
        ("account_activity", &["account_activity"]),
    ];

    let streaming_hard_skip: &[&str] = &[
        "media_used_for", "edits_you", "content_sharing", "trash",
        "posts_on_other", "places_you_have", "visual_search", "fundraiser",
        "anonymous_posts",
        "profile_information", "security_and_login_information",
        "contact_info", "peer_group", "friend_lists", "friends_and_followers",
        "messages", "pokes", "account_management", "preferences", "about_you",
        "your_activity_history", "your_activity_time_spent",
    ];

    let max_file_size: u64 = 50 * 1024 * 1024;
    let mut html_files: Vec<(usize, String)> = Vec::new();
    let mut json_files: Vec<usize> = Vec::new();
    let mut media_count = 0u64;
    let total_entries = archive.len();
    let mut scan_progress = 0u64;

    for i in 0..total_entries {
        scan_progress += 1;
        if scan_progress % 500 == 0 {
            send(serde_json::json!({"type":"phase","phase":"scanning_archive","total":total_entries,"progress":scan_progress}));
        }

        let (name, size) = {
            let entry = match archive.by_index(i) { Ok(e) => e, Err(_) => continue };
            (entry.name().to_string(), entry.size())
        };

        // Detect trackers
        for (category, patterns) in tracker_categories {
            if patterns.iter().any(|p| name.contains(p)) {
                send(serde_json::json!({"type":"tracker","category":category,"filename":name.clone(),"count":1}));
                break;
            }
        }

        // Count media files
        if name.contains("/media/") && !name.ends_with('/') && !name.contains("stickers_used/") {
            media_count += 1;
        }

        // Collect HTML files for parsing
        if name.ends_with(".html") && size <= max_file_size && !streaming_hard_skip.iter().any(|p| name.contains(p)) {
            html_files.push((i, name.clone()));
        }

        // Collect JSON post files
        if name.contains("posts") && name.ends_with(".json") && !name.contains("comments") && size <= max_file_size {
            json_files.push(i);
        }
    }

    send(serde_json::json!({"type":"phase","phase":"archive_scanned","media_count":media_count,"html_files":html_files.len(),"json_files":json_files.len()}));

    // ---- Phase 2: Parse posts from HTML/JSON ----
    send(serde_json::json!({"type":"phase","phase":"parsing_posts","total":html_files.len() + json_files.len()}));

    let mut parsed_posts: Vec<ParsedPost> = Vec::new();
    let max_posts = 10000usize;

    // JSON files first
    for &i in &json_files {
        if parsed_posts.len() >= max_posts { break; }
        let mut entry = match archive.by_index(i) { Ok(e) => e, Err(_) => continue };
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).ok();
        if buf.is_empty() { continue; }

        let json_val: serde_json::Value = match serde_json::from_slice(&buf) {
            Ok(v) => v,
            Err(e) => {
                errors.push(format!("JSON parse error: {}", e));
                continue;
            }
        };
        for post in extract_posts_from_json(&json_val, &mut errors) {
            if parsed_posts.len() >= max_posts { break; }
            let idx = parsed_posts.len() + 1;
            send(serde_json::json!({
                "type":"post",
                "title":post.content_text.chars().take(100).collect::<String>(),
                "timestamp":post.original_ts.to_rfc3339(),
                "media_count":post.media_urls.len(),
                "index":idx,
            }));
            parsed_posts.push(post);
        }
    }

    // HTML files
    for (i, path) in &html_files {
        if parsed_posts.len() >= max_posts { break; }

        let mut entry = match archive.by_index(*i) { Ok(e) => e, Err(_) => continue };
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).ok();
        if buf.is_empty() { continue; }
        let html = String::from_utf8_lossy(&buf);

        let new_posts: Vec<ParsedPost> = if path.contains("group_posts_and_comments") {
            extract_posts_from_html(&html, &mut errors)
        } else if path.contains("your_photos") || path.contains("uncategorized_photos") {
            extract_media_posts_from_html(&html, Some("Photos"))
        } else if path.contains("your_videos") {
            extract_media_posts_from_html(&html, Some("Videos"))
        } else if path.contains("album/") {
            let album_name = path.rsplit('/').nth(1).map(|s| s.replace("%20", " ").replace("_", " ").replace(".html", ""));
            extract_media_posts_from_html(&html, album_name.as_deref())
        } else {
            extract_posts_from_html(&html, &mut errors)
        };

        for post in new_posts {
            if parsed_posts.len() >= max_posts { break; }
            let idx = parsed_posts.len() + 1;
            send(serde_json::json!({
                "type":"post",
                "title":post.content_text.chars().take(100).collect::<String>(),
                "timestamp":post.original_ts.to_rfc3339(),
                "media_count":post.media_urls.len(),
                "index":idx,
            }));
            parsed_posts.push(post);
        }

        // No yield — the archive is not Send, so we can't yield in an async context
        // that requires Send. Processing is synchronous but fast per file.
    }

    // Archive is still open — we'll reuse it for media extraction later
    // Don't drop it yet

    // ---- Phase 2b: Deduplicate ----
    send(serde_json::json!({"type":"phase","phase":"deduplicating","total":parsed_posts.len()}));

    let mut dedup_map: std::collections::HashMap<String, ParsedPost> = std::collections::HashMap::new();
    for post in parsed_posts.iter() {
        let ts_secs = post.original_ts.timestamp();
        let fingerprint = if let Some(first_media) = post.media_urls.first() {
            let filename = first_media.rsplit('/').next().unwrap_or(first_media);
            format!("{}|{}", ts_secs, filename)
        } else {
            let text_prefix: String = post.content_text.chars().take(200).collect();
            format!("{}|{}", ts_secs, text_prefix)
        };

        if let Some(existing) = dedup_map.get(&fingerprint) {
            // Prefer the post with more content (longer text or more media)
            let new_score = post.content_text.len() + post.media_urls.len() * 100;
            let existing_score = existing.content_text.len() + existing.media_urls.len() * 100;
            if new_score > existing_score {
                dedup_map.insert(fingerprint, post.clone());
            }
        } else {
            dedup_map.insert(fingerprint, post.clone());
        }
    }

    let deduped_posts: Vec<ParsedPost> = dedup_map.into_values().collect();
    let removed_count = parsed_posts.len() - deduped_posts.len();
    if removed_count > 0 {
        errors.push(format!("Dedup: removed {} duplicates ({} remaining)", removed_count, deduped_posts.len()));
    }

    // ---- Phase 2c: Extract ONLY referenced media ----
    send(serde_json::json!({"type":"phase","phase":"extracting_media","total":deduped_posts.len()}));

    let mut referenced_media: std::collections::HashSet<String> = std::collections::HashSet::new();
    for post in &deduped_posts {
        for url in &post.media_urls {
            // Convert URL back to ZIP path
            if let Some(relative) = url.strip_prefix("/uploads/import/") {
                referenced_media.insert(format!("your_facebook_activity/posts/media/{}", relative));
            }
        }
    }

    // Done with the parsing archive — drop it before re-opening for media
    drop(archive);

    if !referenced_media.is_empty() {
        let std_file = match std::fs::File::open(temp_path) {
            Ok(f) => f,
            Err(e) => {
                send(serde_json::json!({"type":"error","message":format!("Failed to re-open for media: {}", e)}));
                errors.push(format!("Media extraction failed: {}", e));
                send(serde_json::json!({"type":"complete","imported":0,"errors":errors}));
                return;
            }
        };
        let mut archive = match zip::ZipArchive::new(std_file) {
            Ok(a) => a,
            Err(e) => {
                send(serde_json::json!({"type":"error","message":format!("Failed to re-open ZIP: {}", e)}));
                errors.push(format!("Media extraction failed: {}", e));
                send(serde_json::json!({"type":"complete","imported":0,"errors":errors}));
                return;
            }
        };

        let mut media_idx = 0u64;
        for i in 0..archive.len() {
            let name = {
                let entry = match archive.by_index(i) { Ok(e) => e, Err(_) => continue };
                entry.name().to_string()
            };
            if !referenced_media.contains(&name) { continue; }

            let relative = name.strip_prefix("your_facebook_activity/posts/media/").unwrap_or(&name);
            if relative.is_empty() || name.ends_with('/') { continue; }

            let dest_dir = format!("{}/{}", base_uploads, std::path::Path::new(relative).parent().unwrap_or(std::path::Path::new("")).display());
            let dest_path = format!("{}/{}", base_uploads, relative);
            let _ = std::fs::create_dir_all(&dest_dir);

            if let Ok(mut src) = archive.by_index(i)
                && let Ok(mut file) = std::fs::File::create(&dest_path)
                && std::io::copy(&mut src, &mut file).is_ok() {
                media_idx += 1;
                let category = if name.ends_with(".mp4") || name.ends_with(".mov") { "video" } else { "photo" };
                let filename = relative.rsplit('/').next().unwrap_or(relative);
                send(serde_json::json!({"type":"media","category":category,"filename":filename,"index":media_idx}));
            }
        }
        errors.push(format!("Extracted {} referenced media files", media_idx));
    }

    // ---- Phase 3: Insert into DB ----
    send(serde_json::json!({"type":"phase","phase":"inserting","total":deduped_posts.len()}));

    let session = match db::imports::ImportRow::create(&state.db, user_id, "facebook").await {
        Ok(s) => s,
        Err(e) => {
            send(serde_json::json!({"type":"error","message":format!("Failed to create import session: {}", e)}));
            send(serde_json::json!({"type":"complete","imported":0,"errors":errors}));
            return;
        }
    };

    db::imports::ImportRow::start(&state.db, session.id).await.ok();

    let mut imported = 0usize;
    for post in &deduped_posts {
        let content_obj = serde_json::json!({
            "text": post.content_text,
            "display_name": post.display_name,
            "action_text": post.action_text,
            "group_name": post.group_name,
            "link_url": post.link_url,
            "link_title": post.link_title,
            "media_urls": post.media_urls,
        });

        match db::posts::PostRow::create(
            &state.db, user_id, "facebook", None, None, &content_obj, "short",
            None, None, None, post.original_ts, true, Some(session.id), "public",
        ).await {
            Ok(_) => imported += 1,
            Err(e) => {
                let msg = format!("DB insert error: {}", e);
                errors.push(msg.clone());
                if errors.len() < 10 {
                    send(serde_json::json!({"type":"error","message":msg}));
                }
            }
        }
    }

    if imported > 0 {
        db::imports::ImportRow::complete(&state.db, session.id, imported as i32, 0).await.ok();
    } else {
        db::imports::ImportRow::fail(
            &state.db, session.id, &format!("No posts imported. Errors: {}", errors.join("; "))).await.ok();
    }

    send(serde_json::json!({"type":"complete","imported":imported,"errors":errors}));
}

async fn import_facebook_archive(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<ImportResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let temp_path = format!("/tmp/bytescape_fb_{}.zip", Uuid::new_v4());
    let mut temp_file = tokio::fs::File::create(&temp_path).await
        .map_err(|e| AppError::Internal(format!("Failed to create temp file: {}", e)))?;

    let mut found_field = false;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Multipart error: {}", e)))?
    {
        if field.name() == Some("file") {
            found_field = true;
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|e| AppError::BadRequest(format!("Stream error: {}", e)))?
            {
                temp_file.write_all(&chunk).await
                    .map_err(|e| AppError::Internal(format!("Write error: {}", e)))?;
            }
            break;
        }
    }

    if !found_field {
        tokio::fs::remove_file(&temp_path).await.ok();
        return Err(AppError::BadRequest("No file provided".into()));
    }

    // Flush temp file to disk before handing off to sync reader
    temp_file.sync_all().await
        .map_err(|e| AppError::Internal(format!("Failed to flush temp file: {}", e)))?;

    let (imported, session_id, errors) = import_facebook_archive_from_path(&state, user_id, &temp_path).await?;

    tokio::fs::remove_file(&temp_path).await.ok();

    Ok(Json(ImportResponse { imported, session_id, errors }))
}


// ─── Contact parsing ───────────────────────────────────────────────────────

use crate::api::import::ContactEntry;

/// Parse Facebook friends from `connections/friends/your_friends.json`.
/// Format: `{"friends_v2": [{"name": "...", "timestamp": ...}, ...]}`
pub fn parse_facebook_friends_json(data: &[u8]) -> Vec<ContactEntry> {
    let mut contacts = Vec::new();
    let json_val: serde_json::Value = match serde_json::from_slice(data) {
        Ok(v) => v,
        Err(_) => return contacts,
    };
    if let Some(friends) = json_val.get("friends_v2").and_then(|v| v.as_array()) {
        for friend in friends {
            if let Some(name) = friend.get("name").and_then(|v| v.as_str()) {
                let name = name.trim().to_string();
                if !name.is_empty() {
                    contacts.push(ContactEntry {
                        contact_name: name,
                        contact_handle: None,
                        contact_platform_url: None,
                        source: "facebook".to_string(),
                        remote_handle: None,
                        direction: None,
                    });
                }
            }
        }
    }
    contacts
}

/// Parse Facebook following from `connections/followers/who_you've_followed.json`.
/// Format: `{"following_v3": [{"name": "...", "timestamp": ...}, ...]}`
pub fn parse_facebook_following_json(data: &[u8]) -> Vec<ContactEntry> {
    let mut contacts = Vec::new();
    let json_val: serde_json::Value = match serde_json::from_slice(data) {
        Ok(v) => v,
        Err(_) => return contacts,
    };
    if let Some(following) = json_val.get("following_v3").and_then(|v| v.as_array()) {
        for entry in following {
            if let Some(name) = entry.get("name").and_then(|v| v.as_str()) {
                let name = name.trim().to_string();
                if !name.is_empty() {
                    contacts.push(ContactEntry {
                        contact_name: name,
                        contact_handle: None,
                        contact_platform_url: None,
                        source: "facebook".to_string(),
                        remote_handle: None,
                        direction: None,
                    });
                }
            }
        }
    }
    contacts
}

/// Parse Facebook friends from `connections/friends/your_friends.html`.
/// Uses scraper to extract names from `div._a6-g` entries.
pub fn parse_facebook_friends_html(data: &[u8]) -> Vec<ContactEntry> {
    let mut contacts = Vec::new();
    let html = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => return contacts,
    };
    let document = scraper::Html::parse_document(html);
    let entry_sel = match scraper::Selector::parse("div._a6-g") {
        Ok(s) => s,
        Err(_) => return contacts,
    };
    for entry in document.select(&entry_sel) {
        let name = entry.text().collect::<Vec<_>>().join(" ").trim().to_string();
        if !name.is_empty() && name.len() > 2 && !name.contains("_") {
            contacts.push(ContactEntry {
                contact_name: name,
                contact_handle: None,
                contact_platform_url: None,
                source: "facebook".to_string(),
                remote_handle: None,
                direction: None,
            });
        }
    }
    contacts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_breaks_and_url_dedup() {
        // Real HTML structure from Facebook archive: link metadata as plain text (no <a> tag) + lyrics with <br />
        let html = r##"<!DOCTYPE html><html><body>
<div class="_a6-g">
  <div class="_2ph_ _a6-h _a6-i">Rob Meyer shared a link.</div>
  <div class="_2ph_ _a6-p"><div>
    <div class="_2pin"><div><div><div><div><div><div>SpotifyWhere the Waves are Highesthttps://open.spotify.com/wrapped/share/share-b2bbe20fa8c140619d592c7a84db3ab9</div></div></div></div></div></div></div>
    <div class="_2pin"><div>Where the waves are highest<br /> Where the house of stone falls down <br /> And your reasons to go on grow quiet<br /> This is where the answer will be found</div><div>Updated Dec 08, 2024 10:27:30 am</div><div></div><div></div><div></div></div>
  </div></div>
  <div class="_3-94 _a6-o"><a><div class="_a72d">Dec 08, 2024 10:27:30 am</div></a></div>
</div>
</body></html>"##;

        let mut errors = Vec::new();
        let posts = extract_posts_from_html(html, &mut errors);
        
        assert_eq!(posts.len(), 1, "Should have exactly 1 post, got {}", posts.len());
        let post = &posts[0];
        
        // link_url should be set to the Spotify URL extracted from metadata text
        assert_eq!(post.link_url.as_deref(), Some("https://open.spotify.com/wrapped/share/share-b2bbe20fa8c140619d592c7a84db3ab9"),
            "link_url should be extracted from metadata text");
        
        // content_text should NOT contain "Spotify" or the raw URL (it's metadata)
        assert!(!post.content_text.contains("SpotifyWhere"), "Content should not contain link metadata, got: {}", post.content_text);
        assert!(!post.content_text.contains("https://open.spotify.com"), "Content should not contain raw URL, got: {}", post.content_text);
        
        // content_text should have newlines (line breaks preserved from <br />)
        assert!(post.content_text.contains("\n"), "Content should have newlines, got: {}", post.content_text);
        
        // content_text should contain the lyrics
        assert!(post.content_text.contains("Where the waves are highest"), "Content should contain lyrics, got: {}", post.content_text);
        assert!(post.content_text.contains("This is where the answer will be found"), "Content should contain last line, got: {}", post.content_text);
        
        // content_text should NOT contain "Updated"
        assert!(!post.content_text.contains("Updated"), "Content should not contain 'Updated' metadata, got: {}", post.content_text);
        
        println!("✅ All assertions passed!");
        println!("   link_url: {:?}", post.link_url);
        println!("   content_text: {:?}", post.content_text);
    }

    #[test]
    fn test_album_labels_and_repeat_lines() {
        let html = r##"<!DOCTYPE html><html><body>
<div class="_a6-g">
  <div class="_2ph_ _a6-h _a6-i">Rob Meyer added a new photo.</div>
  <div class="_2ph_ _a6-p"><div>
    <div class="_2pin"><div><div><div><div class="_a7nf"><div class="_a7ng"><div><a href="your_facebook_activity/posts/media/Mobileuploads_oChXF8YEpw/test.jpg"><img src="your_facebook_activity/posts/media/Mobileuploads_oChXF8YEpw/test.jpg" /></a><div>Mobile uploads</div></div></div></div></div></div></div></div>
    <div class="_2pin"><div>Go home mother nature, you're drunk.<br />Go home mother nature, you're drunk.</div></div>
  </div></div>
  <div class="_3-94 _a6-o"><a><div class="_a72d">Jul 09, 2023 2:00:23 pm</div></a></div>
</div>
</body></html>"##;

        let mut errors = Vec::new();
        let posts = extract_posts_from_html(html, &mut errors);
        
        assert_eq!(posts.len(), 1, "Should have 1 post, got {}", posts.len());
        let post = &posts[0];
        
        // Should NOT contain "Mobile uploads"
        assert!(!post.content_text.contains("Mobile uploads"), "Content should not contain 'Mobile uploads', got: {}", post.content_text);
        
        // Should contain the caption once (not twice)
        let count = post.content_text.matches("Go home mother nature").count();
        assert_eq!(count, 1, "Caption should appear once, not {}, got: {}", count, post.content_text);
        
        // Should have the media
        assert_eq!(post.media_urls.len(), 1, "Should have 1 media URL");
        
        println!("✅ Album label test passed!");
        println!("   content_text: {:?}", post.content_text);
        println!("   media_urls: {:?}", post.media_urls);
    }

    #[test]
    fn test_json_parser_extracts_media_and_text() {
        let json = r#"[
  {
    "timestamp": 1771728631,
    "attachments": [
      {
        "data": [
          {
            "media": {
              "uri": "your_facebook_activity/posts/media/Photos_122111151303220534/122111151159220534.jpg",
              "creation_timestamp": 1771728528,
              "title": "Photos",
              "description": "Testing Post 1😂"
            }
          }
        ]
      }
    ],
    "data": [
      { "post": "Testing Post 1😂" },
      {}
    ],
    "title": "Bob Mayer added a new photo."
  },
  {
    "timestamp": 1771728671,
    "attachments": [
      {
        "data": [
          {
            "external_context": {
              "url": "https://media1.giphy.com/media/v1.Y2lk/test/giphy.gif"
            }
          }
        ]
      }
    ],
    "data": [
      { "update_timestamp": 1771728671 },
      {},
      {}
    ],
    "title": "Bob Mayer shared a link."
  },
  {
    "timestamp": 1768495762,
    "attachments": [
      {
        "data": [
          {
            "life_event": {
              "title": "Born on May 16, 1986"
            }
          }
        ]
      }
    ],
    "data": [
      { "backdated_timestamp": 516610800 },
      {}
    ],
    "title": "Bob Mayer added a life event from May 16, 1986: Born on May 16, 1986."
  }
]"#;

        let mut errors = Vec::new();
        let posts = extract_posts_from_json(&serde_json::from_str(json).unwrap(), &mut errors);

        // Should have 1 post (photo with text). The giphy link entry is a comment
        // duplicate (no `post` field in data, only external link) — filtered out.
        // Birthday skipped.
        assert_eq!(posts.len(), 1, "Should have 1 post (giphy comment + birthday skipped), got {}", posts.len());

        // Post 0: photo with text
        assert_eq!(posts[0].content_text, "Testing Post 1😂", "Post 0 text wrong: {}", posts[0].content_text);
        assert_eq!(posts[0].media_urls.len(), 1, "Post 0 should have 1 media URL");
        assert_eq!(posts[0].action_text.as_deref(), Some("added a new photo"));

        println!("✅ JSON parser test passed!");
        println!("   Post 0: text={:?}, media={}", posts[0].content_text, posts[0].media_urls.len());
    }

    // ─── Fixture-based verification (specs/import/facebook-archive/spec.md) ──
    //
    // Fixture: `tests/fixtures/facebook_fixture.zip` (checked in, ~3.4 KB):
    //   - your_facebook_activity/posts/your_posts.json        → 3 JSON posts
    //   - your_facebook_activity/posts/timeline/timeline.html → 1 HTML post
    //   - your_facebook_activity/posts/album/album_Test_Photos/album.html
    //       → 1 album post: one image-only `_2pin` with 3 images + a caption pin
    //   - your_facebook_activity/posts/media/AlbumTestPhotos/photo{1,2,3}.jpg
    //   - connections/friends/your_friends.json               → 4 friends
    //
    // Known counts: 5 posts (3 JSON + 2 HTML), 3 media entries (all on the
    // album post), 4 contacts. Timestamps: 3 JSON posts at epochs
    // 1708938000/1709037000/1709156700 (2024-02-26..28), timeline
    // "Mar 05, 2024 8:15:00 am" → 2024-03-05T08:15:00Z, album
    // "Mar 06, 2024 6:30:00 pm" → 2024-03-06T18:30:00Z (parse_fb_date reads
    // Facebook's local-looking stamps as UTC).
    //
    // Gaps found while building these tests (correctness fixes only):
    // 1. Image-only `_2pin` blocks were dropped by BOTH HTML parsers: media
    //    extraction ran after `text_clean.is_empty() → continue`, so a pin with
    //    only <img> tags contributed 0 media — an album of 3 images parsed as
    //    0 media entries (R2 violated). Fixed by hoisting media extraction
    //    above the text-based skips in `extract_media_posts_from_html` and
    //    `extract_posts_from_html`.
    // 2. `import_sessions` always recorded 0 media: `ImportRow::complete(..., 0)`
    //    in `import_facebook_archive_from_path` and in the streaming import in
    //    `import.rs`. Fixed to record the extracted (from_path) / imported
    //    (streaming) media count (R4).
    // Not a gap: `your_friends.json` carries no profile-URL field in real
    // Facebook exports, so `ContactEntry::contact_platform_url` is None per
    // spec R3 ("where present").
    //
    // Endpoint note (R5): `GET /api/profile/{username}/wall` is the guestbook
    // (wall_posts table — visitor messages), which never contains imported
    // posts. Imported posts land in the `posts` table and render on the
    // profile's post wall, `GET /api/profile/{username}/feed` — the endpoint
    // that serves media + original timestamps (design.md: "Imported posts land
    // in the same posts table the app serves the profile wall from"). That is
    // the endpoint exercised below.

    /// The two tests below extract media into the shared `./uploads/import/`
    /// directory; serialize them so one test's cleanup can't race the other's
    /// reads.
    static FIXTURE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    const FIXTURE_ZIP: &[u8] = include_bytes!("../../tests/fixtures/facebook_fixture.zip");

    const JSON_POSTS: [(&str, i64); 3] = [
        ("Fixture JSON post one: coffee before the commute", 1708938000),
        ("Fixture JSON post two: rain on the trail", 1709037000),
        ("Fixture JSON post three: wrapping up the week", 1709156700),
    ];
    const TIMELINE_NEEDLE: &str = "Fixture HTML timeline post with a line break";
    const ALBUM_NEEDLE: &str = "Sunset over the ridge";
    const TIMELINE_TS: &str = "2024-03-05T08:15:00+00:00";
    const ALBUM_TS: &str = "2024-03-06T18:30:00+00:00";
    const ALBUM_MEDIA: [&str; 3] = [
        "/uploads/import/AlbumTestPhotos/photo1.jpg",
        "/uploads/import/AlbumTestPhotos/photo2.jpg",
        "/uploads/import/AlbumTestPhotos/photo3.jpg",
    ];

    /// Materialize the checked-in fixture ZIP at a unique temp path.
    fn fixture_path(tag: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "bytescape_fb_fixture_{}_{}.zip",
            tag,
            std::process::id()
        ));
        std::fs::write(&path, FIXTURE_ZIP).expect("write fixture zip to temp");
        path
    }

    fn rfc3339(s: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(s)
            .expect("rfc3339 literal")
            .with_timezone(&chrono::Utc)
    }

    fn find_post<'a>(posts: &'a [ParsedPost], needle: &str) -> &'a ParsedPost {
        posts.iter().find(|p| p.content_text.contains(needle)).unwrap_or_else(|| {
            panic!(
                "no post containing {needle:?}; got: {:?}",
                posts.iter().map(|p| p.content_text.clone()).collect::<Vec<_>>()
            )
        })
    }

    /// Best-effort removal of media this fixture extracts into ./uploads/import/.
    fn cleanup_fixture_media() {
        for n in 1..=3 {
            let _ = std::fs::remove_file(format!("./uploads/import/AlbumTestPhotos/photo{n}.jpg"));
        }
        let _ = std::fs::remove_dir("./uploads/import/AlbumTestPhotos");
    }

    /// R1 + R2 + R3: `parse_facebook_archive_sync` against the fixture —
    /// 5 posts (3 JSON, 2 HTML) with exact timestamps and action metadata,
    /// 3 media entries on the album post (extracted to disk), 4 contacts.
    #[test]
    fn fixture_archive_sync_parses_known_counts() {
        let _guard = FIXTURE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = fixture_path("sync");

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let sender = crate::api::import::tokio_to_std_sender(&tx);
        let (posts, errors) = parse_facebook_archive_sync(path.to_str().unwrap(), &sender);

        // R1: exactly 5 posts — 3 JSON + 2 HTML, and dedup must not drop any.
        assert_eq!(
            posts.len(),
            5,
            "expected 5 posts, got {:?} (errors: {:?})",
            posts.iter().map(|p| p.content_text.clone()).collect::<Vec<_>>(),
            errors
        );

        // R1: JSON posts — content, epoch timestamps, action metadata.
        for (text, epoch) in JSON_POSTS {
            let post = find_post(&posts, text);
            assert_eq!(post.original_ts.timestamp(), epoch, "timestamp for {text:?}");
            assert_eq!(post.display_name.as_deref(), Some("Rob Meyer"), "display_name for {text:?}");
            assert_eq!(post.action_text.as_deref(), Some("updated his status"), "action for {text:?}");
        }

        // R1: HTML timeline post — "Mar 05, 2024 8:15:00 am" parsed as UTC.
        let timeline = find_post(&posts, TIMELINE_NEEDLE);
        assert_eq!(timeline.original_ts, rfc3339(TIMELINE_TS), "timeline timestamp");
        assert!(
            timeline.content_text.contains("second line here"),
            "line break should be preserved: {:?}",
            timeline.content_text
        );
        assert_eq!(timeline.action_text.as_deref(), Some("updated his status"));

        // R2: the album post carries all 3 media entries (image-only pin) and
        // the caption; no other post carries media.
        let album = find_post(&posts, ALBUM_NEEDLE);
        assert_eq!(album.original_ts, rfc3339(ALBUM_TS), "album timestamp");
        assert_eq!(album.action_text.as_deref(), Some("added 3 new photos"));
        assert_eq!(album.media_urls, ALBUM_MEDIA.to_vec(), "album media urls");
        let total_media: usize = posts.iter().map(|p| p.media_urls.len()).sum();
        assert_eq!(total_media, 3, "exactly 3 media entries across all posts");

        // The 3 referenced media files were extracted from the ZIP.
        for url in ALBUM_MEDIA {
            let on_disk = format!(".{url}");
            assert!(
                std::path::Path::new(&on_disk).exists(),
                "extracted media missing on disk: {on_disk}"
            );
        }
        assert!(
            errors.iter().any(|e| e.contains("Extracted 3 referenced media files")),
            "media extraction not reported: {errors:?}"
        );

        // R3: contact-extraction step over the same fixture — 4 friends with
        // names. Facebook's friends JSON has no profile-URL field, so
        // contact_platform_url stays None (spec: "where present").
        let contacts = crate::api::import::parse_contacts_from_archive(path.to_str().unwrap(), "facebook");
        assert_eq!(contacts.len(), 4, "expected 4 contacts, got {contacts:?}");
        let mut names: Vec<&str> = contacts.iter().map(|c| c.contact_name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, vec!["Alice Anderson", "Bob Brown", "Carol Clark", "Dave Davis"]);
        for contact in &contacts {
            assert_eq!(contact.source, "facebook");
            assert!(contact.contact_platform_url.is_none());
        }

        let _ = std::fs::remove_file(&path);
        cleanup_fixture_media();
    }

    fn database_url() -> Option<String> {
        if let Ok(url) = std::env::var("DATABASE_URL") {
            return Some(url);
        }
        // cargo test doesn't load .env — read it from the crate root like main() does.
        let env_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".env");
        let content = std::fs::read_to_string(env_file).ok()?;
        content
            .lines()
            .find_map(|l| l.strip_prefix("DATABASE_URL=").map(|s| s.trim().to_string()))
    }

    /// R4 + R5: import the fixture through `import_facebook_archive_from_path`,
    /// assert `import_sessions` counts match the fixture (5/5 posts, 3/3
    /// media), then query the profile's post wall endpoint and assert the
    /// imported posts render with correct media and timestamps.
    #[tokio::test]
    async fn fixture_import_records_session_counts_and_renders_on_profile_wall() {
        let _guard = FIXTURE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let Some(database_url) = database_url() else {
            eprintln!("DATABASE_URL unset — skipping DB integration test");
            return;
        };

        // Config for AppState: JWT_SECRET etc. come from bytescape-server/.env
        // (dotenvy preserves variables already set in the environment).
        let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let _ = dotenvy::from_path(crate_root.join(".env"));
        let config = crate::config::AppConfig::from_env().expect("AppConfig::from_env");
        let pool = sqlx::PgPool::connect(&database_url).await.expect("db connect");
        let state = crate::AppState {
            config: std::sync::Arc::new(config),
            db: pool.clone(),
            og_cache: Default::default(),
        };

        // Fresh user so the profile starts empty.
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let username = format!("fbwalltest{stamp}");
        let user = db::users::UserRow::create_active(&pool, &username, "", "fixture-hash", "fixture-pubkey")
            .await
            .expect("create test user");
        let user_id = user.id;

        let path = fixture_path("import");
        let (imported, session_id, errors) = import_facebook_archive_from_path(&state, user_id, path.to_str().unwrap())
            .await
            .expect("fixture import");
        assert_eq!(imported, 5, "imported post count (errors: {errors:?})");
        assert!(
            errors.iter().any(|e| e.contains("Extracted 3 media files")),
            "media extraction not reported: {errors:?}"
        );

        // R4: import_sessions counts match the fixture's known content counts.
        let session_uuid = uuid::Uuid::parse_str(&session_id).expect("session id is a uuid");
        let session = db::imports::ImportRow::find_by_id(&pool, session_uuid)
            .await
            .expect("session query")
            .expect("session row exists");
        assert_eq!(session.status, "completed");
        assert_eq!(session.total_posts, 5, "total_posts");
        assert_eq!(session.imported_posts, 5, "imported_posts");
        assert_eq!(session.total_media, 3, "total_media");
        assert_eq!(session.imported_media, 3, "imported_media");

        // R5: query the profile's post wall (feed endpoint — see note above)
        // and assert every imported post renders with media and timestamps.
        let app = axum::Router::new()
            .merge(crate::api::profile::routes())
            .with_state(state.clone());
        let request = axum::http::Request::builder()
            .method("GET")
            .uri(format!("/api/profile/{username}/feed?limit=20"))
            .body(axum::body::Body::empty())
            .expect("build request");
        let response = tower::ServiceExt::oneshot(app, request).await.expect("endpoint call");
        assert_eq!(response.status(), axum::http::StatusCode::OK, "feed endpoint status");
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.expect("response body");
        let feed: crate::api::feed::FeedResponse = serde_json::from_slice(&body).expect("feed JSON");

        assert_eq!(feed.items.len(), 5, "imported posts rendered on profile wall");

        // Album post: 3 media entries with correct URLs + original timestamp.
        let album = feed
            .items
            .iter()
            .find(|p| p.content.contains(ALBUM_NEEDLE))
            .expect("album post rendered");
        let media_urls: Vec<&str> = album.media.iter().map(|m| m.url.as_str()).collect();
        assert_eq!(media_urls, ALBUM_MEDIA, "rendered media urls");
        assert_eq!(album.created_at, ALBUM_TS, "rendered album timestamp");

        // JSON posts: present with original epoch timestamps.
        for (text, epoch) in JSON_POSTS {
            let post = feed
                .items
                .iter()
                .find(|p| p.content.contains(text))
                .unwrap_or_else(|| panic!("JSON post {text:?} not rendered"));
            let expected = chrono::DateTime::from_timestamp(epoch, 0).unwrap().to_rfc3339();
            assert_eq!(post.created_at, expected, "rendered timestamp for {text:?}");
        }

        // HTML timeline post: present with its parsed timestamp.
        let timeline = feed
            .items
            .iter()
            .find(|p| p.content.contains(TIMELINE_NEEDLE))
            .expect("timeline post rendered");
        assert_eq!(timeline.created_at, TIMELINE_TS, "rendered timeline timestamp");

        // Cleanup: imported rows cascade from the user; drop the fixture files.
        let _ = sqlx::query("DELETE FROM media WHERE user_id = $1").bind(user_id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM posts WHERE user_id = $1").bind(user_id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM import_sessions WHERE user_id = $1").bind(user_id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1").bind(user_id).execute(&pool).await;
        let _ = std::fs::remove_file(&path);
        cleanup_fixture_media();
    }
}
