//! Blog RSS/Atom feed discovery and parsing.
//!
//! Provides Tauri IPC commands for:
//! 1. Discovering RSS/Atom feed URLs from a blog homepage
//! 2. Fetching and parsing feed entries into structured blog data
//!
//! ## SECURITY
//! Uses `validate_public_url()` to reject SSRF targets (private IPs, loopback, etc.)

use serde::Serialize;
use tauri::State;
use crate::AppState;

/// A single blog post entry parsed from an RSS/Atom feed.
#[derive(Debug, Clone, Serialize)]
pub struct FeedBlogEntry {
    pub id: String,
    pub title: String,
    /// Plain-text body (HTML stripped) — for summary/fallback display.
    pub body: String,
    /// Sanitized HTML body — for iframe-based rich preview rendering.
    /// Capped at 500KB. `<script>`, `<style>`, and event handlers are removed.
    pub body_html: Option<String>,
    pub timestamp: i64,
    pub external_url: Option<String>,
    pub author: Option<String>,
}

/// Result of a blog feed fetch: the discovered feed URL + parsed entries.
#[derive(Debug, Clone, Serialize)]
pub struct BlogFeedResult {
    pub feed_url: String,
    pub site_title: Option<String>,
    pub entries: Vec<FeedBlogEntry>,
}

/// Fetches a blog URL, discovers its RSS/Atom feed, parses entries, and returns them.
///
/// Accepts either a direct feed URL (XML) or a blog homepage (HTML with feed link).
/// Tries `<link rel="alternate">` discovery first, then common feed paths.
#[tauri::command]
pub async fn fetch_blog_feed(
    url: String,
    _state: State<'_, AppState>,
) -> Result<BlogFeedResult, String> {
    let parsed = reqwest::Url::parse(&url).map_err(|e| format!("Invalid URL: {}", e))?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err("Only http/https URLs are supported".to_string());
    }
    crate::security::validate_public_url(&url, false)?;

    let client = build_client()?;

    let resp = client
        .get(&url)
        .header("User-Agent", "Bytescape/1.0 (Feed Reader)")
        .header("Accept", "application/rss+xml, application/atom+xml, application/xml, text/xml, text/html")
        .send()
        .await
        .map_err(|e| format!("Fetch failed: {}", e))?;

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();

    let body_bytes = resp.bytes().await.map_err(|e| format!("Read failed: {}", e))?;
    if body_bytes.len() > 5_242_880 {
        return Err("Response too large (>5MB)".to_string());
    }

    let is_feed = content_type.contains("xml")
        || content_type.contains("rss")
        || content_type.contains("atom");

    if is_feed {
        return parse_feed_xml(&body_bytes, &url);
    }

    // It's HTML — try to discover feed URL from <link> tags
    let html = String::from_utf8_lossy(&body_bytes);
    if let Some(feed_url) = discover_feed_from_html(&html, &url) {
        crate::security::validate_public_url(&feed_url, false)?;

        let feed_resp = client
            .get(&feed_url)
            .header("User-Agent", "Bytescape/1.0 (Feed Reader)")
            .header("Accept", "application/rss+xml, application/atom+xml, application/xml, text/xml")
            .send()
            .await
            .map_err(|e| format!("Feed fetch failed: {}", e))?;

        let feed_bytes = feed_resp.bytes().await.map_err(|e| format!("Read failed: {}", e))?;
        if feed_bytes.len() > 5_242_880 {
            return Err("Feed too large (>5MB)".to_string());
        }

        return parse_feed_xml(&feed_bytes, &feed_url);
    }

    // Fallback: try common feed paths
    let base = parsed.origin().ascii_serialization();
    let common_paths = ["/feed", "/feed/", "/rss", "/rss.xml", "/atom.xml", "/index.xml", "/feed.xml", "/?feed=rss2"];

    for path in &common_paths {
        let candidate = format!("{}{}", base, path);
        if crate::security::validate_public_url(&candidate, false).is_err() {
            continue;
        }

        let probe = client
            .get(&candidate)
            .header("User-Agent", "Bytescape/1.0 (Feed Reader)")
            .send()
            .await;

        if let Ok(resp) = probe {
            if resp.status().is_success() {
                let ct = resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_lowercase();

                if ct.contains("xml") || ct.contains("rss") || ct.contains("atom") {
                    let bytes = resp.bytes().await.map_err(|e| format!("Read failed: {}", e))?;
                    if bytes.len() <= 5_242_880 {
                        if let Ok(result) = parse_feed_xml(&bytes, &candidate) {
                            return Ok(result);
                        }
                    }
                }
            }
        }
    }

    Err("Could not find an RSS or Atom feed at this URL. Try pasting the direct feed URL (usually ending in /feed or /rss).".to_string())
}

fn build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))
}

/// Parse RSS/Atom XML bytes into structured blog entries.
fn parse_feed_xml(bytes: &[u8], feed_url: &str) -> Result<BlogFeedResult, String> {
    let feed = feed_rs::parser::parse(bytes)
        .map_err(|e| format!("Failed to parse feed: {}", e))?;

    let site_title = feed.title.map(|t| t.content);

    let entries: Vec<FeedBlogEntry> = feed
        .entries
        .into_iter()
        .take(50)
        .map(|entry| {
            let title = entry
                .title
                .map(|t| t.content)
                .unwrap_or_else(|| "Untitled".to_string());

            let body = entry
                .content
                .and_then(|c| c.body)
                .or_else(|| entry.summary.map(|s| s.content))
                .unwrap_or_default();

            let body_text = strip_html_tags(&body);
            let body_html = if body.contains('<') {
                let sanitized = sanitize_html(&body);
                // Cap at 500KB to prevent memory bloat from pathological feeds
                if sanitized.len() > 512_000 {
                    Some(sanitized[..512_000].to_string())
                } else if sanitized.trim().is_empty() {
                    None
                } else {
                    Some(sanitized)
                }
            } else {
                None
            };

            let timestamp = entry
                .published
                .or(entry.updated)
                .map(|dt| dt.timestamp())
                .unwrap_or(0);

            let external_url = entry
                .links
                .first()
                .map(|l| l.href.clone())
                .or_else(|| entry.id.parse::<reqwest::Url>().ok().map(|u| u.to_string()));

            let author = entry
                .authors
                .first()
                .map(|a| a.name.clone());

            let id = format!("feed-{:x}", {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                entry.id.hash(&mut h);
                h.finish()
            });

            FeedBlogEntry {
                id,
                title,
                body: body_text,
                body_html,
                timestamp,
                external_url,
                author,
            }
        })
        .collect();

    Ok(BlogFeedResult {
        feed_url: feed_url.to_string(),
        site_title,
        entries,
    })
}

/// Discover feed URL from HTML `<link rel="alternate">` tags.
fn discover_feed_from_html(html: &str, page_url: &str) -> Option<String> {
    let document = scraper::Html::parse_document(html);

    let feed_types = [
        "application/rss+xml",
        "application/atom+xml",
        "application/feed+json",
    ];

    for feed_type in &feed_types {
        let selector_str = format!("link[rel=\"alternate\"][type=\"{}\"]", feed_type);
        let selector = match scraper::Selector::parse(&selector_str) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if let Some(el) = document.select(&selector).next() {
            if let Some(href) = el.value().attr("href") {
                return Some(resolve_url(page_url, href));
            }
        }
    }

    None
}

/// Resolve a potentially relative URL against a base URL.
fn resolve_url(base: &str, relative: &str) -> String {
    if relative.starts_with("http://") || relative.starts_with("https://") {
        return relative.to_string();
    }
    if let Ok(base_url) = reqwest::Url::parse(base) {
        if let Ok(resolved) = base_url.join(relative) {
            return resolved.to_string();
        }
    }
    relative.to_string()
}

/// Sanitize HTML for safe iframe rendering.
///
/// Removes `<script>`, `<style>`, `<link>`, `<meta>`, `<iframe>`, `<object>`,
/// `<embed>`, `<form>`, and `<base>` elements entirely.  Strips event-handler
/// attributes (`on*`) and `javascript:` hrefs from remaining elements.
fn sanitize_html(html: &str) -> String {
    use scraper::{Html, Selector};

    let doc = Html::parse_fragment(html);

    // Elements to remove entirely (including their content)
    let dangerous_tags = ["script", "style", "link", "meta", "iframe",
                          "object", "embed", "form", "base", "applet"];

    // Build a set of node ids to remove
    let mut remove_ids = std::collections::HashSet::new();
    for tag in &dangerous_tags {
        if let Ok(sel) = Selector::parse(tag) {
            for el in doc.select(&sel) {
                remove_ids.insert(el.id());
            }
        }
    }

    // Rebuild the HTML, filtering out dangerous elements and attributes
    let mut output = String::with_capacity(html.len());

    for node in doc.tree.nodes() {
        let node_id = node.id();

        // Skip nodes that are children of removed elements
        let mut ancestor = node.parent();
        let mut skip = false;
        while let Some(anc) = ancestor {
            if remove_ids.contains(&anc.id()) {
                skip = true;
                break;
            }
            ancestor = anc.parent();
        }
        if skip {
            continue;
        }

        match node.value() {
            scraper::Node::Element(el) => {
                let tag = el.name();
                if remove_ids.contains(&node_id) {
                    continue;
                }

                output.push('<');
                output.push_str(tag);

                for (name, value) in el.attrs() {
                    let name_lower = name.to_lowercase();
                    // Skip event handler attributes
                    if name_lower.starts_with("on") {
                        continue;
                    }
                    // Skip javascript: protocol in href/src/action
                    if (name_lower == "href" || name_lower == "src" || name_lower == "action")
                        && value.trim_start().to_lowercase().starts_with("javascript:")
                    {
                        continue;
                    }
                    // Skip data: URIs in src (potential XSS vector)
                    if name_lower == "src" && value.trim_start().to_lowercase().starts_with("data:text/html") {
                        continue;
                    }
                    output.push(' ');
                    output.push_str(name);
                    output.push_str("=\"");
                    // Escape HTML entities in attribute values
                    for ch in value.chars() {
                        match ch {
                            '"' => output.push_str("&quot;"),
                            '&' => output.push_str("&amp;"),
                            '<' => output.push_str("&lt;"),
                            '>' => output.push_str("&gt;"),
                            _ => output.push(ch),
                        }
                    }
                    output.push('"');
                }
                output.push('>');
            }
            scraper::Node::Text(text) => {
                output.push_str(&text.text);
            }
            _ => {}
        }
    }

    output
}

/// Strip HTML tags to produce plain text, preserving paragraph breaks.
fn strip_html_tags(html: &str) -> String {
    let with_breaks = html
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n\n")
        .replace("</div>", "\n")
        .replace("</li>", "\n")
        .replace("</h1>", "\n\n")
        .replace("</h2>", "\n\n")
        .replace("</h3>", "\n\n")
        .replace("</h4>", "\n\n")
        .replace("</blockquote>", "\n\n");

    let mut result = String::with_capacity(with_breaks.len());
    let mut in_tag = false;
    for ch in with_breaks.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    let result = result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&#8217;", "\u{2019}")
        .replace("&#8216;", "\u{2018}")
        .replace("&#8220;", "\u{201C}")
        .replace("&#8221;", "\u{201D}")
        .replace("&#8230;", "\u{2026}");

    let mut collapsed = String::with_capacity(result.len());
    let mut newline_count = 0;
    for ch in result.chars() {
        if ch == '\n' {
            newline_count += 1;
            if newline_count <= 2 {
                collapsed.push(ch);
            }
        } else {
            newline_count = 0;
            collapsed.push(ch);
        }
    }

    collapsed.trim().to_string()
}
