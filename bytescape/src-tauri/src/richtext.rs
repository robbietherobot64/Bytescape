//! Rich Text Facet Detection & Generation for Bluesky AT Protocol
//!
//! Detects @mentions, #hashtags, and hyperlinks in post text and produces
//! `app.bsky.richtext.facet` arrays with correct UTF-8 byte offsets.
//!
//! See: https://docs.bsky.app/docs/advanced-guides/post-richtext

use regex::Regex;
use serde::{Deserialize, Serialize};

// ─── AT Protocol Facet Types (for serialization into createRecord payloads) ───

/// A rich text facet annotation for Bluesky posts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RichTextFacet {
    pub index: FacetByteSlice,
    pub features: Vec<FacetFeature>,
}

/// Byte range within UTF-8 encoded text (inclusive start, exclusive end).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacetByteSlice {
    #[serde(rename = "byteStart")]
    pub byte_start: usize,
    #[serde(rename = "byteEnd")]
    pub byte_end: usize,
}

/// The type of annotation applied to a text range.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "$type")]
pub enum FacetFeature {
    #[serde(rename = "app.bsky.richtext.facet#mention")]
    Mention {
        /// The resolved DID of the mentioned user.
        did: String,
    },
    #[serde(rename = "app.bsky.richtext.facet#tag")]
    Tag {
        /// The hashtag value without the leading `#`.
        tag: String,
    },
    #[serde(rename = "app.bsky.richtext.facet#link")]
    Link {
        /// The full URL.
        uri: String,
    },
}

// ─── Facet Detection ───

/// Intermediate representation of a detected facet before DID resolution.
#[derive(Debug, Clone)]
pub struct DetectedFacet {
    pub byte_start: usize,
    pub byte_end: usize,
    pub kind: DetectedFacetKind,
}

#[derive(Debug, Clone)]
pub enum DetectedFacetKind {
    /// A @mention — handle needs to be resolved to a DID.
    Mention { handle: String },
    /// A #hashtag.
    Tag { tag: String },
    /// A hyperlink.
    Link { uri: String },
}

/// Detect all @mentions, #hashtags, and URLs in the given text.
/// Returns facets with UTF-8 byte offsets. Mention DIDs are NOT resolved yet
/// — call `resolve_mention_dids` afterward to fill them in.
pub fn detect_facets(text: &str) -> Vec<DetectedFacet> {
    let mut facets = Vec::new();

    detect_mentions(text, &mut facets);
    detect_hashtags(text, &mut facets);
    detect_links(text, &mut facets);

    // Sort by byte_start for consistent ordering
    facets.sort_by_key(|f| f.byte_start);

    // Remove overlapping facets (keep the first one)
    let mut filtered = Vec::new();
    let mut last_end: usize = 0;
    for facet in facets {
        if facet.byte_start >= last_end {
            last_end = facet.byte_end;
            filtered.push(facet);
        }
    }

    filtered
}

/// Detect @mentions in text. Handles must look like valid domain names
/// (e.g. `@alice.bsky.social`, `@bob.test`).
fn detect_mentions(text: &str, facets: &mut Vec<DetectedFacet>) {
    // Match @handle patterns: preceded by start-of-string, whitespace, or opening paren
    // Handle must contain at least one dot (domain-like)
    let re = Regex::new(
        r"(?:^|[\s(])(@([a-zA-Z0-9]([a-zA-Z0-9\-]{0,61}[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9\-]{0,61}[a-zA-Z0-9])?)+))"
    ).unwrap();

    for cap in re.captures_iter(text) {
        let full_match = cap.get(1).unwrap(); // The @handle portion
        let handle = cap.get(2).unwrap().as_str(); // Without the @

        // Validate: must have at least one dot (domain-like)
        if !handle.contains('.') {
            continue;
        }

        // Regex positions on &str are already UTF-8 byte offsets
        let byte_start = full_match.start();
        let byte_end = full_match.end();

        facets.push(DetectedFacet {
            byte_start,
            byte_end,
            kind: DetectedFacetKind::Mention {
                handle: handle.to_string(),
            },
        });
    }
}

/// Detect #hashtags in text.
fn detect_hashtags(text: &str, facets: &mut Vec<DetectedFacet>) {
    // Match #tag patterns: preceded by start-of-string or whitespace
    // Tag must not start with a digit, max 64 chars (excluding #)
    let re = Regex::new(r"(?:^|\s)(#[^\d\s]\S*)").unwrap();

    for cap in re.captures_iter(text) {
        let full_match = cap.get(1).unwrap();
        let mut tag = full_match.as_str().to_string();

        // Strip trailing punctuation (except # _ -)
        while tag.ends_with(|c: char| c.is_ascii_punctuation() && c != '#' && c != '_' && c != '-') {
            tag.pop();
        }

        // Max length: # + 64 chars
        if tag.len() > 66 {
            continue;
        }

        // Must have content after #
        let tag_value = &tag[1..]; // Without #
        if tag_value.is_empty() {
            continue;
        }

        let byte_start = full_match.start();
        let byte_end = byte_start + tag.len();

        facets.push(DetectedFacet {
            byte_start,
            byte_end,
            kind: DetectedFacetKind::Tag {
                tag: tag_value.to_string(),
            },
        });
    }
}

/// Detect URLs (http:// and https://) in text.
fn detect_links(text: &str, facets: &mut Vec<DetectedFacet>) {
    // Match URLs starting with http:// or https://
    let re = Regex::new(r"(?:^|\s)(https?://\S+)").unwrap();

    for cap in re.captures_iter(text) {
        let url_match = cap.get(1).unwrap();
        let mut uri = url_match.as_str().to_string();

        // Strip trailing punctuation that's likely not part of the URL
        while uri.ends_with(|c: char| matches!(c, '.' | ',' | ';' | '!' | '?')) {
            uri.pop();
        }
        // Strip trailing ) only if there's no matching ( in the URL
        if uri.ends_with(')') && !uri.contains('(') {
            uri.pop();
        }

        let byte_start = url_match.start();
        let byte_end = byte_start + uri.len();

        facets.push(DetectedFacet {
            byte_start,
            byte_end,
            kind: DetectedFacetKind::Link { uri },
        });
    }
}

// ─── DID Resolution ───

/// Resolve @mention handles to DIDs by calling `com.atproto.identity.resolveHandle`.
/// Mentions that fail to resolve are silently dropped from the output.
pub async fn resolve_mention_dids(
    facets: &[DetectedFacet],
    access_jwt: &str,
) -> Vec<RichTextFacet> {
    let client = reqwest::Client::new();
    let mut resolved = Vec::new();

    for facet in facets {
        match &facet.kind {
            DetectedFacetKind::Mention { handle } => {
                match resolve_handle_to_did(&client, handle, access_jwt).await {
                    Ok(did) => {
                        resolved.push(RichTextFacet {
                            index: FacetByteSlice {
                                byte_start: facet.byte_start,
                                byte_end: facet.byte_end,
                            },
                            features: vec![FacetFeature::Mention { did }],
                        });
                    }
                    Err(e) => {
                        // Failed to resolve — the mention will appear as plain text on Bluesky
                        println!("richtext: failed to resolve @{}: {}", handle, e);
                    }
                }
            }
            DetectedFacetKind::Tag { tag } => {
                resolved.push(RichTextFacet {
                    index: FacetByteSlice {
                        byte_start: facet.byte_start,
                        byte_end: facet.byte_end,
                    },
                    features: vec![FacetFeature::Tag {
                        tag: tag.clone(),
                    }],
                });
            }
            DetectedFacetKind::Link { uri } => {
                resolved.push(RichTextFacet {
                    index: FacetByteSlice {
                        byte_start: facet.byte_start,
                        byte_end: facet.byte_end,
                    },
                    features: vec![FacetFeature::Link {
                        uri: uri.clone(),
                    }],
                });
            }
        }
    }

    resolved
}

/// Resolve a single Bluesky handle to a DID via the public XRPC API.
async fn resolve_handle_to_did(
    client: &reqwest::Client,
    handle: &str,
    access_jwt: &str,
) -> Result<String, String> {
    let clean = handle.trim().trim_start_matches('@');

    // Already a DID
    if clean.starts_with("did:") {
        return Ok(clean.to_string());
    }

    // Use public API for handle resolution — works without auth
    let url = format!(
        "https://public.api.bsky.app/xrpc/com.atproto.identity.resolveHandle?handle={}",
        clean
    );

    let mut req = client.get(&url);
    if !access_jwt.is_empty() && access_jwt != "oauth_session" {
        req = req.header("Authorization", format!("Bearer {}", access_jwt));
    }
    let response = req
        .send()
        .await
        .map_err(|e| format!("resolveHandle request failed: {}", e))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!(
            "resolveHandle failed for '{}': {}",
            clean, error_text
        ));
    }

    #[derive(Deserialize)]
    struct ResolveResponse {
        did: String,
    }

    let parsed: ResolveResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse DID response: {}", e))?;

    Ok(parsed.did)
}

// ─── Convenience: Full Pipeline ───

/// Parse text for facets and resolve all @mention DIDs.
/// Returns AT Protocol-compatible facet objects ready for `app.bsky.feed.post`.
pub async fn parse_facets_for_bluesky(
    text: &str,
    access_jwt: &str,
) -> Vec<RichTextFacet> {
    let detected = detect_facets(text);
    if detected.is_empty() {
        return Vec::new();
    }
    resolve_mention_dids(&detected, access_jwt).await
}

/// Parse text for facets without resolving DIDs (for local-only posts).
/// Mentions will have the handle as a placeholder DID.
pub fn parse_facets_local(text: &str) -> Vec<RichTextFacet> {
    let detected = detect_facets(text);
    detected
        .into_iter()
        .map(|f| match f.kind {
            DetectedFacetKind::Mention { handle } => RichTextFacet {
                index: FacetByteSlice {
                    byte_start: f.byte_start,
                    byte_end: f.byte_end,
                },
                features: vec![FacetFeature::Mention {
                    did: format!("@{}", handle),
                }],
            },
            DetectedFacetKind::Tag { tag } => RichTextFacet {
                index: FacetByteSlice {
                    byte_start: f.byte_start,
                    byte_end: f.byte_end,
                },
                features: vec![FacetFeature::Tag { tag }],
            },
            DetectedFacetKind::Link { uri } => RichTextFacet {
                index: FacetByteSlice {
                    byte_start: f.byte_start,
                    byte_end: f.byte_end,
                },
                features: vec![FacetFeature::Link { uri }],
            },
        })
        .collect()
}

// ─── ActivityPub Rich Text (stub) ───
// When Track A delivers AP posting (A1.2), mentions will be converted to
// `<a href="actor_url">@handle</a>` in the `content` HTML field.
// Hashtags become `<a href="tag_url">#tag</a>`.
// URLs remain as `<a href="url">url</a>`.
// The `parse_facets_for_activitypub` function will live here.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_mention_basic() {
        let text = "Hello @alice.bsky.social!";
        let facets = detect_facets(text);
        assert_eq!(facets.len(), 1);
        assert_eq!(facets[0].byte_start, 6);
        assert_eq!(facets[0].byte_end, 24);
        match &facets[0].kind {
            DetectedFacetKind::Mention { handle } => assert_eq!(handle, "alice.bsky.social"),
            _ => panic!("Expected mention facet"),
        }
    }

    #[test]
    fn test_detect_mention_with_emoji_prefix() {
        let text = "🎉 Hello @alice.bsky.social";
        let facets = detect_facets(text);
        assert_eq!(facets.len(), 1);
        // 🎉 = 4 bytes, space = 1, Hello = 5, space = 1 → byte 11
        assert_eq!(facets[0].byte_start, 11);
        // @alice.bsky.social = 18 bytes → 11 + 18 = 29
        assert_eq!(facets[0].byte_end, 29);
        match &facets[0].kind {
            DetectedFacetKind::Mention { handle } => assert_eq!(handle, "alice.bsky.social"),
            _ => panic!("Expected mention facet"),
        }
    }

    #[test]
    fn test_detect_hashtag_basic() {
        let text = "Check out #bytescape today";
        let facets = detect_facets(text);
        assert_eq!(facets.len(), 1);
        assert_eq!(facets[0].byte_start, 10);
        assert_eq!(facets[0].byte_end, 20); // "#bytescape" = 10 bytes
        match &facets[0].kind {
            DetectedFacetKind::Tag { tag } => assert_eq!(tag, "bytescape"),
            _ => panic!("Expected tag facet"),
        }
    }

    #[test]
    fn test_detect_hashtag_at_start() {
        let text = "#rust is great";
        let facets = detect_facets(text);
        assert_eq!(facets.len(), 1);
        match &facets[0].kind {
            DetectedFacetKind::Tag { tag } => assert_eq!(tag, "rust"),
            _ => panic!("Expected tag facet"),
        }
    }

    #[test]
    fn test_detect_hashtag_no_digit_start() {
        let text = "Check #123 number";
        let facets = detect_facets(text);
        // #123 starts with a digit, should not match
        assert_eq!(facets.len(), 0);
    }

    #[test]
    fn test_detect_link_basic() {
        let text = "Visit https://example.com for more";
        let facets = detect_facets(text);
        assert_eq!(facets.len(), 1);
        assert_eq!(facets[0].byte_start, 6);
        match &facets[0].kind {
            DetectedFacetKind::Link { uri } => assert_eq!(uri, "https://example.com"),
            _ => panic!("Expected link facet"),
        }
    }

    #[test]
    fn test_detect_link_with_path() {
        let text = "See https://example.com/path/to/page?q=1&b=2 now";
        let facets = detect_facets(text);
        assert_eq!(facets.len(), 1);
        match &facets[0].kind {
            DetectedFacetKind::Link { uri } => {
                assert_eq!(uri, "https://example.com/path/to/page?q=1&b=2");
            }
            _ => panic!("Expected link facet"),
        }
    }

    #[test]
    fn test_detect_link_trailing_punctuation() {
        let text = "Go to https://example.com.";
        let facets = detect_facets(text);
        assert_eq!(facets.len(), 1);
        match &facets[0].kind {
            DetectedFacetKind::Link { uri } => assert_eq!(uri, "https://example.com"),
            _ => panic!("Expected link facet"),
        }
    }

    #[test]
    fn test_detect_multiple_facets() {
        let text = "@alice.bsky.social check out #bytescape at https://example.com";
        let facets = detect_facets(text);
        assert_eq!(facets.len(), 3);

        match &facets[0].kind {
            DetectedFacetKind::Mention { handle } => assert_eq!(handle, "alice.bsky.social"),
            _ => panic!("Expected mention"),
        }
        match &facets[1].kind {
            DetectedFacetKind::Tag { tag } => assert_eq!(tag, "bytescape"),
            _ => panic!("Expected tag"),
        }
        match &facets[2].kind {
            DetectedFacetKind::Link { uri } => assert_eq!(uri, "https://example.com"),
            _ => panic!("Expected link"),
        }
    }

    #[test]
    fn test_byte_offsets_with_unicode() {
        // é is 2 bytes in UTF-8
        let text = "café @alice.bsky.social";
        let facets = detect_facets(text);
        assert_eq!(facets.len(), 1);
        // c=1, a=1, f=1, é=2, space=1 → 6 bytes before @
        assert_eq!(facets[0].byte_start, 6);
        assert_eq!(facets[0].byte_end, 24);
    }

    #[test]
    fn test_byte_offsets_with_cjk() {
        // Each CJK character is 3 bytes in UTF-8
        let text = "你好 @alice.bsky.social";
        let facets = detect_facets(text);
        assert_eq!(facets.len(), 1);
        // 你=3, 好=3, space=1 → 7 bytes before @
        assert_eq!(facets[0].byte_start, 7);
        assert_eq!(facets[0].byte_end, 25);
    }

    #[test]
    fn test_no_facets_in_plain_text() {
        let text = "Just a normal post with no special syntax";
        let facets = detect_facets(text);
        assert_eq!(facets.len(), 0);
    }

    #[test]
    fn test_mention_without_dot_ignored() {
        let text = "Hey @someone how are you";
        let facets = detect_facets(text);
        assert_eq!(facets.len(), 0);
    }

    #[test]
    fn test_local_facets_no_resolution() {
        let text = "@alice.bsky.social check #rust at https://rust-lang.org";
        let facets = parse_facets_local(text);
        assert_eq!(facets.len(), 3);

        match &facets[0].features[0] {
            FacetFeature::Mention { did } => assert_eq!(did, "@alice.bsky.social"),
            _ => panic!("Expected mention"),
        }
    }

    #[test]
    fn test_facet_serialization_mention() {
        let facet = RichTextFacet {
            index: FacetByteSlice {
                byte_start: 6,
                byte_end: 24,
            },
            features: vec![FacetFeature::Mention {
                did: "did:plc:abc123".to_string(),
            }],
        };

        let json = serde_json::to_value(&facet).unwrap();
        assert_eq!(json["index"]["byteStart"], 6);
        assert_eq!(json["index"]["byteEnd"], 24);
        assert_eq!(
            json["features"][0]["$type"],
            "app.bsky.richtext.facet#mention"
        );
        assert_eq!(json["features"][0]["did"], "did:plc:abc123");
    }

    #[test]
    fn test_facet_serialization_tag() {
        let facet = RichTextFacet {
            index: FacetByteSlice {
                byte_start: 0,
                byte_end: 5,
            },
            features: vec![FacetFeature::Tag {
                tag: "rust".to_string(),
            }],
        };

        let json = serde_json::to_value(&facet).unwrap();
        assert_eq!(json["features"][0]["$type"], "app.bsky.richtext.facet#tag");
        assert_eq!(json["features"][0]["tag"], "rust");
    }

    #[test]
    fn test_facet_serialization_link() {
        let facet = RichTextFacet {
            index: FacetByteSlice {
                byte_start: 0,
                byte_end: 20,
            },
            features: vec![FacetFeature::Link {
                uri: "https://example.com".to_string(),
            }],
        };

        let json = serde_json::to_value(&facet).unwrap();
        assert_eq!(json["features"][0]["$type"], "app.bsky.richtext.facet#link");
        assert_eq!(json["features"][0]["uri"], "https://example.com");
    }

    #[test]
    fn test_multiple_mentions_in_text() {
        let text = "Hey @alice.bsky.social and @bob.test!";
        let facets = detect_facets(text);
        assert_eq!(facets.len(), 2);
        match &facets[0].kind {
            DetectedFacetKind::Mention { handle } => assert_eq!(handle, "alice.bsky.social"),
            _ => panic!("Expected mention"),
        }
        match &facets[1].kind {
            DetectedFacetKind::Mention { handle } => assert_eq!(handle, "bob.test"),
            _ => panic!("Expected mention"),
        }
    }

    #[test]
    fn test_hashtag_with_trailing_punctuation() {
        let text = "Love #rust!";
        let facets = detect_facets(text);
        assert_eq!(facets.len(), 1);
        match &facets[0].kind {
            DetectedFacetKind::Tag { tag } => assert_eq!(tag, "rust"),
            _ => panic!("Expected tag facet"),
        }
    }

    #[test]
    fn test_http_link() {
        let text = "Visit http://example.com today";
        let facets = detect_facets(text);
        assert_eq!(facets.len(), 1);
        match &facets[0].kind {
            DetectedFacetKind::Link { uri } => assert_eq!(uri, "http://example.com"),
            _ => panic!("Expected link facet"),
        }
    }

    #[test]
    fn test_mention_at_start_of_text() {
        let text = "@alice.bsky.social hello!";
        let facets = detect_facets(text);
        assert_eq!(facets.len(), 1);
        assert_eq!(facets[0].byte_start, 0);
        assert_eq!(facets[0].byte_end, 18);
    }

    #[test]
    fn test_hashtag_at_end_of_text() {
        let text = "Check out #bytescape";
        let facets = detect_facets(text);
        assert_eq!(facets.len(), 1);
        match &facets[0].kind {
            DetectedFacetKind::Tag { tag } => assert_eq!(tag, "bytescape"),
            _ => panic!("Expected tag facet"),
        }
    }

    #[test]
    fn test_link_at_end_of_text() {
        let text = "Visit https://example.com";
        let facets = detect_facets(text);
        assert_eq!(facets.len(), 1);
        match &facets[0].kind {
            DetectedFacetKind::Link { uri } => assert_eq!(uri, "https://example.com"),
            _ => panic!("Expected link facet"),
        }
    }
}
