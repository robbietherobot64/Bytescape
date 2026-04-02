//! Link unfurling and Open Graph metadata extraction.
//!
//! Fetches user-provided URLs and extracts rich link preview metadata using
//! Open Graph (`og:*`) and Twitter Card (`twitter:*`) meta tags, with fallbacks
//! to standard HTML `<meta>` and `<title>` tags.
//!
//! ## HOW IT WORKS
//! 1. Frontend calls `unfurl_link` via Tauri IPC with a URL from the compose box
//! 2. The URL is validated (http/https only) and fetched with a 5s timeout / 3-redirect cap
//! 3. HTML response is parsed with `scraper`, extracting OG → Twitter Card → HTML fallbacks
//! 4. Relative URLs (images, favicons) are resolved against the page's base URL
//! 5. Result is returned as a `LinkCard` struct for inline preview rendering
//!
//! ## RELATED FILES
//! - `src/components/ComposeBox/` — frontend compose UI that triggers unfurling
//! - `commands/bluesky.rs` — `publish_post_v2` uses `LinkCard` for `app.bsky.embed.external`
//! - `types/index.ts` — TypeScript `LinkCard` interface mirrors this Rust struct
//!
//! ## SECURITY NOTE
//! ⚠ No private-IP / SSRF filtering is applied to user-provided URLs.
//! A malicious URL like `http://169.254.169.254/...` (cloud metadata endpoint) or
//! `http://localhost:8080/admin` would be fetched from the user's machine.
//! This is acceptable for a local-first desktop app (the user controls their own
//! network), but MUST be addressed before any server-side unfurling is added.

use tauri::State;
use serde::Serialize;
use crate::AppState;

/// Rich link preview card extracted from a URL's HTML metadata.
///
/// Fields are populated in priority order: Open Graph → Twitter Card → HTML fallback.
/// All text fields are trimmed and capped at 500 characters.
#[derive(Debug, Clone, Serialize)]
pub struct LinkCard {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub site_name: Option<String>,
    pub favicon_url: Option<String>,
}

/// Fetches a URL and extracts Open Graph / Twitter Card metadata for link card embeds.
///
/// Returns a `LinkCard` with title, description, thumbnail, site name, and favicon.
/// Used by ComposeBox to show inline link previews, and by `publish_post_v2` to emit
/// `app.bsky.embed.external` when publishing to Bluesky.
#[tauri::command]
pub async fn unfurl_link(
    url: String,
    _state: State<'_, AppState>,
) -> Result<LinkCard, String> {
    // Validate URL
    // SSRF NOTE: No private-IP filtering — see module-level security note above.
    let parsed = reqwest::Url::parse(&url).map_err(|e| format!("Invalid URL: {}", e))?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err("Only http/https URLs are supported".to_string());
    }

    // Fetch with safety limits
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .get(&url)
        .header("User-Agent", "Bytescape/1.0 (Link Preview)")
        .header("Accept", "text/html")
        .send()
        .await
        .map_err(|e| format!("Fetch failed: {}", e))?;

    // Only process HTML responses
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !content_type.contains("text/html") && !content_type.contains("application/xhtml") {
        return Err(format!("Not an HTML page (content-type: {})", content_type));
    }

    // Limit body size to 1 MB
    let body = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read body: {}", e))?;
    if body.len() > 1_048_576 {
        return Err("Response too large (>1MB)".to_string());
    }

    let html = String::from_utf8_lossy(&body);
    let document = scraper::Html::parse_document(&html);

    // Extract OG tags with Twitter Card fallbacks
    let title = extract_meta(&document, "og:title")
        .or_else(|| extract_meta(&document, "twitter:title"))
        .or_else(|| extract_title_tag(&document));

    let description = extract_meta(&document, "og:description")
        .or_else(|| extract_meta(&document, "twitter:description"))
        .or_else(|| extract_meta_by_name(&document, "description"));

    let image_url = extract_meta(&document, "og:image")
        .or_else(|| extract_meta(&document, "twitter:image"))
        .map(|img| resolve_url(&url, &img));

    let site_name = extract_meta(&document, "og:site_name");

    let favicon_url = extract_favicon(&document)
        .map(|fav| resolve_url(&url, &fav));

    // Sanitize: trim and cap string lengths
    let sanitize = |s: String| -> Option<String> {
        let trimmed = s.trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed.chars().take(500).collect()) }
    };

    Ok(LinkCard {
        url: url.clone(),
        title: title.and_then(sanitize),
        description: description.and_then(sanitize),
        image_url,
        site_name: site_name.and_then(sanitize),
        favicon_url,
    })
}

/// Extract content from `<meta property="X" content="...">` (Open Graph style)
fn extract_meta(doc: &scraper::Html, property: &str) -> Option<String> {
    let selector_str = format!("meta[property=\"{}\"]", property);
    let selector = scraper::Selector::parse(&selector_str).ok()?;
    doc.select(&selector)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string())
}

/// Extract content from `<meta name="X" content="...">` (standard HTML meta)
fn extract_meta_by_name(doc: &scraper::Html, name: &str) -> Option<String> {
    let selector_str = format!("meta[name=\"{}\"]", name);
    let selector = scraper::Selector::parse(&selector_str).ok()?;
    doc.select(&selector)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string())
}

/// Extract text content from `<title>` tag
fn extract_title_tag(doc: &scraper::Html) -> Option<String> {
    let selector = scraper::Selector::parse("title").ok()?;
    doc.select(&selector)
        .next()
        .map(|el| el.text().collect::<String>())
}

/// Extract favicon URL from `<link rel="icon" href="...">` or similar
fn extract_favicon(doc: &scraper::Html) -> Option<String> {
    // Use static selectors to avoid lifetime issues with Selector::parse + format!()
    let selectors: &[&str] = &[
        "link[rel=\"icon\"]",
        "link[rel=\"shortcut icon\"]",
        "link[rel=\"apple-touch-icon\"]",
    ];
    for sel_str in selectors {
        if let Ok(selector) = scraper::Selector::parse(sel_str) {
            if let Some(el) = doc.select(&selector).next() {
                if let Some(href) = el.value().attr("href") {
                    return Some(href.to_string());
                }
            }
        }
    }
    None
}

/// Resolve a potentially relative URL against a base URL
fn resolve_url(base: &str, relative: &str) -> String {
    if relative.starts_with("http://") || relative.starts_with("https://") {
        return relative.to_string();
    }
    if relative.starts_with("//") {
        return format!("https:{}", relative);
    }
    match reqwest::Url::parse(base) {
        Ok(base_url) => match base_url.join(relative) {
            Ok(resolved) => resolved.to_string(),
            Err(_) => relative.to_string(),
        },
        Err(_) => relative.to_string(),
    }
}
