use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::connectors::Source;
use crate::sync::SyncManager;

const PUBLIC_XRPC: &str = "https://bsky.social/xrpc";
const PLC_DIRECTORY: &str = "https://plc.directory";

fn decode_base64url_json(input: &str) -> Option<serde_json::Value> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    let bytes = URL_SAFE_NO_PAD.decode(input).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn xrpc_from_access_jwt(access_jwt: &str) -> Option<String> {
    let token = access_jwt.trim();
    let mut parts = token.split('.');
    let _header = parts.next()?;
    let payload = parts.next()?;

    let payload_json = decode_base64url_json(payload)?;
    let iss = payload_json.get("iss")?.as_str()?.trim();
    if iss.is_empty() {
        return None;
    }

    Some(format!("{}/xrpc", iss.trim_end_matches('/')))
}

pub fn authenticated_xrpc_candidates(access_jwt: &str, prefer_private: bool) -> Vec<String> {
    let mut urls = Vec::new();

    if prefer_private {
        if let Some(pds_xrpc) = xrpc_from_access_jwt(access_jwt) {
            urls.push(pds_xrpc);
        }
        urls.push(PUBLIC_XRPC.to_string());
    } else {
        urls.push(PUBLIC_XRPC.to_string());
        if let Some(pds_xrpc) = xrpc_from_access_jwt(access_jwt) {
            if pds_xrpc != PUBLIC_XRPC {
                urls.push(pds_xrpc);
            }
        }
    }

    urls
}

// === Bluesky Feed API Response Types ===

#[derive(Debug, Deserialize)]
struct AuthorFeedResponse {
    feed: Vec<FeedItem>,
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FeedItem {
    post: FeedPost,
    #[serde(default)]
    reason: Option<Reason>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "$type")]
enum Reason {
    #[serde(rename = "app.bsky.feed.defs#skeletonReasonRepost")]
    Repost {
        by: FeedAuthor,
        #[serde(rename = "indexedAt")]
        indexed_at: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct FeedPost {
    uri: String,
    cid: String,
    author: FeedAuthor,
    record: PostRecord,
    #[serde(default)]
    embed: Option<serde_json::Value>,
    #[serde(rename = "indexedAt")]
    indexed_at: Option<String>,
    #[serde(rename = "replyCount", default)]
    pub reply_count: Option<i64>,
    #[serde(rename = "repostCount", default)]
    pub repost_count: Option<i64>,
    #[serde(rename = "likeCount", default)]
    pub like_count: Option<i64>,
    #[serde(default)]
    pub viewer: Option<PostViewer>,
}

#[derive(Debug, Deserialize)]
struct PostViewer {
    #[serde(default)]
    pub like: Option<String>,
    #[serde(default)]
    pub repost: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FeedAuthor {
    did: String,
    handle: String,
    #[serde(rename = "displayName", default)]
    display_name: Option<String>,
    #[serde(default)]
    avatar: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PostRecord {
    #[serde(rename = "$type", default)]
    record_type: Option<String>,
    text: Option<String>,
    #[serde(rename = "createdAt", default)]
    created_at: Option<String>,
    #[serde(default)]
    embed: Option<PostEmbed>,
    #[serde(default)]
    facets: Option<Vec<Facet>>,
}

#[derive(Debug, Deserialize)]
struct Facet {
    features: Vec<FacetFeature>,
    index: FacetIndex,
}

#[derive(Debug, Deserialize)]
struct FacetIndex {
    #[serde(rename = "byteStart")]
    byte_start: usize,
    #[serde(rename = "byteEnd")]
    byte_end: usize,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "$type")]
enum FacetFeature {
    #[serde(rename = "app.bsky.richtext.facet#tag")]
    Tag {
        tag: String,
    },
    #[serde(rename = "app.bsky.richtext.facet#mention")]
    Mention {
        did: String,
    },
    #[serde(rename = "app.bsky.richtext.facet#link")]
    Link {
        uri: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct PostEmbed {
    #[serde(rename = "$type", default)]
    embed_type: Option<String>,
    images: Option<Vec<EmbedImage>>,
    external: Option<EmbedExternal>,
}

#[derive(Debug, Deserialize)]
struct EmbedImage {
    alt: Option<String>,
    image: Option<BlobRef>,
}

#[derive(Debug, Deserialize)]
struct BlobRef {
    #[serde(rename = "$type", default)]
    ref_type: Option<String>,
    #[serde(rename = "ref", default)]
    link: Option<serde_json::Value>,
    #[serde(rename = "mimeType", default)]
    mime_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EmbedExternal {
    uri: Option<String>,
    title: Option<String>,
    description: Option<String>,
}

// === Progress Event ===

#[derive(Debug, Serialize, Clone)]
pub struct ImportProgress {
    pub source: String,
    pub stage: String,
    pub posts_imported: usize,
    pub photos_downloaded: usize,
    pub current_item: String,
}

// === Parsed Post (used by lib.rs to create BytescapeItem) ===

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlueskyImportedPost {
    pub uri: String,
    pub cid: String,
    pub text: String,
    pub created_at: i64,
    pub image_urls: Vec<String>,
    pub image_local_paths: Vec<String>,
    pub author_did: String,
    pub author_handle: String,
    pub author_avatar: Option<String>,
    pub reply_count: Option<i64>,
    pub repost_count: Option<i64>,
    pub like_count: Option<i64>,
    pub video_url: Option<String>,
    pub video_thumbnail: Option<String>,
    pub external_url: Option<String>,
    pub external_title: Option<String>,
    pub external_description: Option<String>,
    pub external_thumb: Option<String>,
    // Repost info
    pub reposted_by: Option<String>, 
    pub reposted_by_handle: Option<String>,
    // Quoted content
    pub quoted_post: Option<Box<BlueskyImportedPost>>,
    // Viewer state (has current user liked/reposted)
    #[serde(default)]
    pub viewer_like: Option<String>,
    #[serde(default)]
    pub viewer_repost: Option<String>,
}

// === Social Graph & Feeds ===

#[derive(Debug, Deserialize)]
struct GetFollowsResponse {
    follows: Vec<ProfileView>,
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GetFollowersResponse {
    followers: Vec<ProfileView>,
    cursor: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProfileView {
    pub did: String,
    pub handle: String,
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GetPreferencesResponse {
    preferences: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct GetFeedGeneratorsResponse {
    feeds: Vec<GeneratorView>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GeneratorView {
    pub uri: String,
    pub cid: String,
    pub did: String,
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
}

// === DID Document (for PDS resolution) ===

#[derive(Debug, Deserialize)]
struct DidDocument {
    service: Option<Vec<DidService>>,
}

#[derive(Debug, Deserialize)]
struct DidService {
    #[serde(rename = "type")]
    service_type: String,
    #[serde(rename = "serviceEndpoint")]
    service_endpoint: String,
}

// === Public API ===

/// Fetch all of a user's Bluesky posts, downloading embedded images.
/// This calls the public `getAuthorFeed` API (no auth required for public posts)
/// and the authenticated PDS for image blobs.
// === Timeline Import (The "Us" Feed) ===

pub async fn import_bluesky_timeline(
    access_jwt: &str,
    media_dir: PathBuf,
    app_handle: &AppHandle,
    limit: Option<u8>,
    cursor: Option<String>,
    sync_manager: Option<Arc<SyncManager>>,
    download_media: bool,
) -> Result<(Vec<BlueskyImportedPost>, Option<String>), String> {
    let client = Client::new();
    let token = access_jwt.trim();
    let limit = limit.unwrap_or(50);

    let mut feed_res: Option<AuthorFeedResponse> = None;
    let mut last_error = String::new();

    for base in authenticated_xrpc_candidates(token, false) {
        let mut url = format!("{}/app.bsky.feed.getTimeline?limit={}", base, limit);
        if let Some(c) = cursor.as_ref() {
            url.push_str(&format!("&cursor={}", urlencoding::encode(c)));
        }

        let resp = client
            .get(&url)
            .bearer_auth(token)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if resp.status().is_success() {
            let parsed: AuthorFeedResponse = resp.json().await.map_err(|e| e.to_string())?;
            feed_res = Some(parsed);
            break;
        }

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        last_error = format!("Failed to fetch timeline from {}: {} {}", base, status, body);
        println!("{}", last_error);
    }

    let feed_res = feed_res.ok_or_else(|| {
        if last_error.is_empty() {
            "Failed to fetch timeline: unknown error".to_string()
        } else {
            last_error
        }
    })?;
    
    let mut imported_posts = Vec::new();

    for item in feed_res.feed {
        // Skip if not a post record (e.g. might be empty or wrong type)
        // For timeline, items are FeedViewPost, which contain 'post'. 
        // Our types match sufficiently (FeedItem has 'post').
        
        let post = item.post;
        
        // Flatten logic similar to feed import
        let (text, created_at) = if let Some(record) = post.record.text {
              (record, post.record.created_at.unwrap_or_default())  
        } else {
            // Check record type, sometimes it's just the struct directly if deserialized differently?
            // Actually our PostRecord struct has Option<String> for text.
            (post.record.text.clone().unwrap_or_default(), post.record.created_at.clone().unwrap_or_default())
        };

        // For timeline browsing, avoid local downloads by default.
        let (local_paths, remote_urls) = if download_media {
            download_media_from_embed_with_sync(&client, &post.embed, &media_dir, sync_manager.clone()).await
        } else {
            let mut image_urls: Vec<String> = Vec::new();
            if let Some(ref e) = post.embed {
                extract_image_urls_from_view(e, &mut image_urls);
            }
            (Vec::new(), image_urls)
        };

        let mut new_post = BlueskyImportedPost {
            uri: post.uri,
            cid: post.cid,
            author_did: post.author.did,
            author_handle: post.author.handle,
            author_avatar: post.author.avatar,
            text,
            created_at: chrono::DateTime::parse_from_rfc3339(&created_at)
                .map(|dt| dt.timestamp())
                .unwrap_or(0),
            image_urls: remote_urls,
            image_local_paths: local_paths,
            like_count: post.like_count,
            reply_count: post.reply_count,
            repost_count: post.repost_count,
            video_url: None, 
            video_thumbnail: None,
            external_url: None,
            external_title: None,
            external_description: None,
            external_thumb: None,
            reposted_by: None,
            reposted_by_handle: None,
            quoted_post: None,
            viewer_like: post.viewer.as_ref().and_then(|v| v.like.clone()),
            viewer_repost: post.viewer.as_ref().and_then(|v| v.repost.clone()),
        };

        if let Some(ref embed) = post.embed {
            extract_video_from_view(embed, &mut new_post.video_url, &mut new_post.video_thumbnail);
            extract_external_from_view(embed, &mut new_post.external_url, &mut new_post.external_title, &mut new_post.external_description, &mut new_post.external_thumb);
        }

        imported_posts.push(new_post);

        // Emit progress for every item to keep UI lively
        let _ = app_handle.emit("import-progress", ImportProgress {
            source: "bluesky".to_string(),
            stage: "timeline".to_string(),
            posts_imported: imported_posts.len(),
            photos_downloaded: 0, // Simplified for now
            current_item: "Processing timeline...".to_string(),
        });
    }

    Ok((imported_posts, feed_res.cursor))
}

pub async fn import_bluesky_feed(
    did: &str,
    _access_jwt: &str,
    media_dir: PathBuf,
    app_handle: &AppHandle,
    max_pages: Option<usize>,
    per_page_limit: Option<u8>,
    sync_manager: Option<Arc<SyncManager>>,
) -> Result<Vec<BlueskyImportedPost>, String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;
    std::fs::create_dir_all(&media_dir)
        .map_err(|e| format!("Failed to create media dir: {}", e))?;

    // Resolve the user's PDS for downloading blobs
    let _pds_url = resolve_pds(&client, did).await.unwrap_or_else(|_| "https://bsky.social".to_string());

    let mut all_posts: Vec<BlueskyImportedPost> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut page = 0;
    let limit = per_page_limit.unwrap_or(100);

    loop {
        page += 1;
        if let Some(max) = max_pages {
            if page > max {
                break;
            }
        }
        let _ = app_handle.emit("import-progress", ImportProgress {
            source: Source::Bluesky.as_str().to_string(),
            stage: "fetching".to_string(),
            posts_imported: all_posts.len(),
            photos_downloaded: all_posts.iter().map(|p| p.image_local_paths.len()).sum(),
            current_item: format!("Fetching page {}...", page),
        });

        // Fetch a page of the author's feed
        let mut url = format!(
            "{}/app.bsky.feed.getAuthorFeed?actor={}&limit={}",
            PUBLIC_XRPC, did, limit
        );
        if let Some(ref c) = cursor {
            url.push_str(&format!("&cursor={}", c));
        }

        let response = client.get(&url)
            .header("Authorization", format!("Bearer {}", _access_jwt))
            .send()
            .await
            .map_err(|e| format!("Feed fetch failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Feed API returned {}: {}", status, body));
        }

        let feed_response: AuthorFeedResponse = response.json()
            .await
            .map_err(|e| format!("Failed to parse feed: {}", e))?;

        println!("bluesky: Fetching page {}, found {} items", page, feed_response.feed.len());
        if feed_response.feed.is_empty() {
            break;
        }

        for item in &feed_response.feed {
            let post = &item.post;

            // Skip reposts (only import user's own posts)
            if post.author.did != did {
                println!("bluesky: Skipping post from {} (owner of feed is {})", post.author.did, did);
                continue;
            }

            let text = post.record.text.clone().unwrap_or_default();

            // Parse timestamp
            let created_at = post.record.created_at.as_ref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.timestamp())
                .unwrap_or(0);

            // Extract image URLs from the post's view embed
            let mut image_urls: Vec<String> = Vec::new();
            if let Some(ref embed) = post.embed {
                extract_image_urls_from_view(embed, &mut image_urls);
            }

            // Download images (use Iroh blobs if available)
            let mut image_local_paths: Vec<String> = Vec::new();
            for (i, img_url) in image_urls.iter().enumerate() {
                let ext = if img_url.contains(".png") { "png" } else { "jpg" };
                let filename = format!("bsky_{}_{}.{}", post.cid, i, ext);
                let dest = media_dir.join(&filename);

                match client.get(img_url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        if let Ok(bytes) = resp.bytes().await {
                            // Try to store in Iroh blobs if available
                            if let Some(ref sync) = sync_manager {
                                match sync.store_media_with_thumbnail(bytes.clone(), 256).await {
                                    Ok((media_hash, _thumb_hash)) => {
                                        image_local_paths.push(format!("blob:{}", media_hash));
                                        continue;
                                    }
                                    Err(e) => {
                                        eprintln!("bluesky: Failed to store in Iroh, falling back: {}", e);
                                    }
                                }
                            }
                            // Fallback to filesystem
                            if std::fs::write(&dest, &bytes).is_ok() {
                                let filename = dest.file_name().unwrap().to_string_lossy().to_string();
                                image_local_paths.push(format!("media/{}", filename));
                            }
                        }
                    }
                    _ => {
                        eprintln!("bluesky: Failed to download image: {}", img_url);
                    }
                }
            }

            let mut new_post = BlueskyImportedPost {
                uri: post.uri.clone(),
                cid: post.cid.clone(),
                text,
                created_at,
                image_urls,
                image_local_paths,
                author_did: post.author.did.clone(),
                author_handle: post.author.handle.clone(),
                author_avatar: post.author.avatar.clone(),
                reply_count: post.reply_count,
                repost_count: post.repost_count,
                like_count: post.like_count,
                video_url: None,
                video_thumbnail: None,
                external_url: None,
                external_title: None,
                external_description: None,
                external_thumb: None,
                reposted_by: None,
                reposted_by_handle: None,
                quoted_post: None,
                viewer_like: post.viewer.as_ref().and_then(|v| v.like.clone()),
                viewer_repost: post.viewer.as_ref().and_then(|v| v.repost.clone()),
            };

            if let Some(ref embed) = post.embed {
                extract_video_from_view(embed, &mut new_post.video_url, &mut new_post.video_thumbnail);
                extract_external_from_view(embed, &mut new_post.external_url, &mut new_post.external_title, &mut new_post.external_description, &mut new_post.external_thumb);
            }
            
            all_posts.push(new_post);
        }

        // Check for next page
        match feed_response.cursor {
            Some(c) if !c.is_empty() => cursor = Some(c),
            _ => break,
        }
    }

    // Final progress
    let _ = app_handle.emit("import-progress", ImportProgress {
        source: Source::Bluesky.as_str().to_string(),
        stage: "complete".to_string(),
        posts_imported: all_posts.len(),
        photos_downloaded: all_posts.iter().map(|p| p.image_local_paths.len()).sum(),
        current_item: format!("Done! {} posts imported.", all_posts.len()),
    });

    println!("bluesky: Imported {} posts with {} images",
        all_posts.len(),
        all_posts.iter().map(|p| p.image_local_paths.len()).sum::<usize>()
    );

    Ok(all_posts)
}

async fn download_media_from_embed(
    client: &Client,
    embed: &Option<serde_json::Value>,
    media_dir: &Path,
) -> (Vec<String>, Vec<String>) {
    // Fallback to filesystem-only version
    download_media_from_embed_with_sync(client, embed, media_dir, None).await
}

/// Download media from embed JSON, storing in Iroh blobs if SyncManager is available.
/// Returns (local_paths_or_hashes, remote_urls, blob_hashes, thumbnail_hashes)
async fn download_media_from_embed_with_sync(
    client: &Client,
    embed: &Option<serde_json::Value>,
    media_dir: &Path,
    sync_manager: Option<Arc<SyncManager>>,
) -> (Vec<String>, Vec<String>) {
    let mut image_urls: Vec<String> = Vec::new();
    if let Some(ref e) = embed {
        extract_image_urls_from_view(e, &mut image_urls);
    }

    let mut local_paths: Vec<String> = Vec::new();
    let mut remote_urls: Vec<String> = Vec::new();

    for (i, img_url) in image_urls.iter().enumerate() {
        let ext = if img_url.contains(".png") { "png" } else { "jpg" };
        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
        let filename = format!("bsky_import_{}_{}.{}", timestamp, i, ext);
        let dest = media_dir.join(&filename);
        println!("bluesky: Downloading image to: {:?}", dest);

        match client.get(img_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(bytes) = resp.bytes().await {
                    // Try to store in Iroh blobs if available
                    if let Some(ref sync) = sync_manager {
                        match sync.store_media_with_thumbnail(bytes.clone(), 256).await {
                            Ok((media_hash, thumb_hash)) => {
                                // Store blob hash with blob: prefix for identification
                                let hash_str = format!("blob:{}", media_hash);
                                local_paths.push(hash_str);
                                remote_urls.push(img_url.clone());
                                if let Some(th) = thumb_hash {
                                    println!("bluesky: Stored image in Iroh blob: {} (thumb: {})", media_hash, th);
                                } else {
                                    println!("bluesky: Stored image in Iroh blob: {} (no thumbnail)", media_hash);
                                }
                                continue;
                            }
                            Err(e) => {
                                eprintln!("bluesky: Failed to store in Iroh, falling back to filesystem: {}", e);
                            }
                        }
                    }
                    
                    // Fallback to filesystem storage
                    if std::fs::write(&dest, &bytes).is_ok() {
                        local_paths.push(dest.to_string_lossy().to_string());
                        remote_urls.push(img_url.clone());
                    }
                }
            }
            _ => {
                eprintln!("bluesky: Failed to download image: {}", img_url);
            }
        }
    }

    (local_paths, remote_urls)
}

/// Extract image thumbnail URLs from the post view embed JSON.
/// The `getAuthorFeed` response includes rendered views with `thumb` URLs.
pub fn extract_image_urls_from_view(embed: &serde_json::Value, urls: &mut Vec<String>) {
    // images view: { "$type": "app.bsky.embed.images#view", "images": [{ "thumb": "...", "fullsize": "..." }] }
    if let Some(images) = embed.get("images").and_then(|i| i.as_array()) {
        for img in images {
            // Prefer fullsize, fallback to thumb
            if let Some(url) = img.get("fullsize").and_then(|u| u.as_str()) {
                urls.push(url.to_string());
            } else if let Some(url) = img.get("thumb").and_then(|u| u.as_str()) {
                urls.push(url.to_string());
            }
        }
    }
    // record with media: { "$type": "app.bsky.embed.recordWithMedia#view", "media": { "images": [...] } }
    if let Some(media) = embed.get("media") {
        extract_image_urls_from_view(media, urls);
    }
}

fn extract_video_from_view(embed: &serde_json::Value, video_url: &mut Option<String>, video_thumbnail: &mut Option<String>) {
    // app.bsky.embed.video#view
    // { "$type": "app.bsky.embed.video#view", "playlist": "https://...", "thumbnail": "https://..." }
    
    // Check for direct video view
    if let Some(t) = embed.get("$type").and_then(|t| t.as_str()) {
        if t == "app.bsky.embed.video#view" {
            if let Some(playlist) = embed.get("playlist").and_then(|p| p.as_str()) {
                *video_url = Some(playlist.to_string());
            }
            if let Some(thumb) = embed.get("thumbnail").and_then(|t| t.as_str()) {
                *video_thumbnail = Some(thumb.to_string());
            }
            return;
        }
    }

    // Check for recordWithMedia (e.g. quote post with video, or just text with video?? No, usually it's just video or recordWithMedia)
    if let Some(media) = embed.get("media") {
        extract_video_from_view(media, video_url, video_thumbnail);
    }
}

fn extract_external_from_view(embed: &serde_json::Value, url: &mut Option<String>, title: &mut Option<String>, description: &mut Option<String>, thumb: &mut Option<String>) {
    // app.bsky.embed.external#view
    // { "$type": "app.bsky.embed.external#view", "external": { "uri": "...", "title": "...", "description": "...", "thumb": "..." } }
    
    if let Some(t) = embed.get("$type").and_then(|t| t.as_str()) {
         if t == "app.bsky.embed.external#view" {
             if let Some(ext) = embed.get("external") {
                 if let Some(u) = ext.get("uri").and_then(|s| s.as_str()) { *url = Some(u.to_string()); }
                 if let Some(t) = ext.get("title").and_then(|s| s.as_str()) { *title = Some(t.to_string()); }
                 if let Some(d) = ext.get("description").and_then(|s| s.as_str()) { *description = Some(d.to_string()); }
                 if let Some(th) = ext.get("thumb").and_then(|s| s.as_str()) { *thumb = Some(th.to_string()); }
             }
             return;
         }
    }

    // recordWithMedia
    if let Some(media) = embed.get("media") {
        extract_external_from_view(media, url, title, description, thumb);
    }
}

fn extract_quote_from_view(embed: &serde_json::Value) -> Option<Box<BlueskyImportedPost>> {
    // record view: { "$type": "app.bsky.embed.record#view", "record": { "$type": "app.bsky.embed.record#viewRecord", ... } }
    let record_view = if let Some(t) = embed.get("$type").and_then(|t| t.as_str()) {
        if t == "app.bsky.embed.record#view" {
            embed.get("record")
        } else if t == "app.bsky.embed.recordWithMedia#view" {
            embed.get("record").and_then(|r| r.get("record"))
        } else {
            None
        }
    } else {
        None
    };

    if let Some(rv) = record_view {
        if let Some(t) = rv.get("$type").and_then(|t| t.as_str()) {
            if t == "app.bsky.embed.record#viewRecord" {
                let author = rv.get("author")?;
                let value = rv.get("value")?; // This is the record content (PostRecord equivalent)
                
                let text = value.get("text").and_then(|t| t.as_str()).unwrap_or_default().to_string();
                let created_at = value.get("createdAt").and_then(|s| s.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.timestamp())
                    .unwrap_or(0);

                let mut image_urls = Vec::new();
                if let Some(embeds) = rv.get("embeds").and_then(|e| e.as_array()) {
                    for e in embeds {
                        extract_image_urls_from_view(e, &mut image_urls);
                    }
                }

                return Some(Box::new(BlueskyImportedPost {
                    uri: rv.get("uri").and_then(|u| u.as_str()).unwrap_or_default().to_string(),
                    cid: rv.get("cid").and_then(|u| u.as_str()).unwrap_or_default().to_string(),
                    text,
                    created_at,
                    image_urls,
                    image_local_paths: Vec::new(),
                    author_did: author.get("did").and_then(|u| u.as_str()).unwrap_or_default().to_string(),
                    author_handle: author.get("handle").and_then(|u| u.as_str()).unwrap_or_default().to_string(),
                    author_avatar: author.get("avatar").and_then(|u| u.as_str()).map(|u| u.to_string()),
                    reply_count: None,
                    repost_count: None,
                    like_count: None,
                    video_url: None,
                    video_thumbnail: None,
                    external_url: None,
                    external_title: None,
                    external_description: None,
                    external_thumb: None,
                    reposted_by: None,
                    reposted_by_handle: None,
                    quoted_post: None,
                    viewer_like: None,
                    viewer_repost: None,
                }));
            }
        }
    }
    None
}


/// Resolve a DID to its PDS URL via the PLC directory.
async fn resolve_pds(client: &Client, did: &str) -> Result<String, String> {
    let url = format!("{}/{}", PLC_DIRECTORY, did);
    let response = client.get(&url)
        .send()
        .await
        .map_err(|e| format!("PLC lookup failed: {}", e))?;

    let doc: DidDocument = response.json()
        .await
        .map_err(|e| format!("Failed to parse DID doc: {}", e))?;

    doc.service
        .and_then(|services| {
            services.iter()
                .find(|s| s.service_type == "AtprotoPersonalDataServer")
                .map(|s| s.service_endpoint.clone())
        })
        .ok_or_else(|| "No PDS found".to_string())
}
/// Fetch the user's timeline (home feed).
/// This calls the authenticated `getTimeline` API.
/// Returns posts with remote image URLs (does not download them).
pub async fn get_bluesky_timeline(
    access_jwt: &str,
    limit: u8,
    cursor: Option<String>,
) -> Result<(Vec<BlueskyImportedPost>, Option<String>), String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;

    let mut url = format!(
        "{}/app.bsky.feed.getTimeline?limit={}",
        PUBLIC_XRPC, limit
    );
    if let Some(ref c) = cursor {
        url.push_str(&format!("&cursor={}", c));
    }

    let token = access_jwt.trim();

    let response = client.get(&url)
        .bearer_auth(token)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Timeline fetch failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Timeline API returned {}: {}", status, body));
    }

    let feed_response: AuthorFeedResponse = response.json()
        .await
        .map_err(|e| format!("Failed to parse timeline: {}", e))?;

    println!("bluesky: Timeline found {} items", feed_response.feed.len());
    for (i, item) in feed_response.feed.iter().enumerate().take(3) {
        println!("bluesky: DEBUG Post {}: URI={}, CID={}, Text={:?}, Author={}", 
            i, item.post.uri, item.post.cid, item.post.record.text, item.post.author.handle);
    }

    let mut posts: Vec<BlueskyImportedPost> = Vec::new();

    for item in feed_response.feed {
        let post = item.post;
        let text = post.record.text.unwrap_or_default();

        let created_at = post.record.created_at.as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp())
            .unwrap_or(0);

        let mut image_urls: Vec<String> = Vec::new();
        let mut video_url: Option<String> = None;
        let mut video_thumbnail: Option<String> = None;
        let mut external_url: Option<String> = None;
        let mut external_title: Option<String> = None;
        let mut external_description: Option<String> = None;
        let mut external_thumb: Option<String> = None;
        let mut quoted_post: Option<Box<BlueskyImportedPost>> = None;

        if let Some(ref embed) = post.embed {
            extract_image_urls_from_view(embed, &mut image_urls);
            extract_video_from_view(embed, &mut video_url, &mut video_thumbnail);
            extract_external_from_view(embed, &mut external_url, &mut external_title, &mut external_description, &mut external_thumb);
            quoted_post = extract_quote_from_view(embed);
        }

        let mut reposted_by = None;
        let mut reposted_by_handle = None;
        if let Some(Reason::Repost { by, .. }) = item.reason {
            reposted_by = Some(by.did);
            reposted_by_handle = Some(by.handle);
        }

        let mut new_post = BlueskyImportedPost {
            uri: post.uri,
            cid: post.cid,
            text,
            created_at,
            image_urls,
            image_local_paths: Vec::new(),
            author_did: post.author.did,
            author_handle: post.author.handle,
            author_avatar: post.author.avatar,
            reply_count: post.reply_count,
            repost_count: post.repost_count,
            like_count: post.like_count,
            video_url,
            video_thumbnail: if video_thumbnail.is_some() { video_thumbnail } else { external_thumb.clone() },
            external_url,
            external_title,
            external_description,
            external_thumb,
            reposted_by,
            reposted_by_handle,
            quoted_post,
            viewer_like: post.viewer.as_ref().and_then(|v| v.like.clone()),
            viewer_repost: post.viewer.as_ref().and_then(|v| v.repost.clone()),
        };

        if let Some(ref embed) = post.embed {
            extract_video_from_view(embed, &mut new_post.video_url, &mut new_post.video_thumbnail);
        }
        
        posts.push(new_post);
    }

    Ok((posts, feed_response.cursor))
}

/// Search for posts on Bluesky.
pub async fn search_posts(
    access_jwt: &str,
    query: &str,
    limit: u8,
    cursor: Option<String>,
    required_tags: Option<Vec<String>>,
) -> Result<(Vec<BlueskyImportedPost>, Option<String>), String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;

    // Use the public search endpoint (assuming app.bsky.feed.searchPosts)
    // Note: searchPosts usually takes 'q' parameter.
    let fetch_limit = if required_tags.is_some() { 100 } else { limit };
    println!("bluesky: search_posts query='{}' limit={} fetch_limit={}", query, limit, fetch_limit);
    
    let _token = access_jwt.trim();
    let mut url = format!(
        "{}/app.bsky.feed.searchPosts?q={}&limit={}",
        PUBLIC_XRPC, urlencoding::encode(query), fetch_limit
    );
    println!("bluesky: search_posts fetching from URL: {}", url);
    if let Some(ref c) = cursor {
        url.push_str(&format!("&cursor={}", c));
    }

    let token = access_jwt.trim();

    let response = client.get(&url)
        .bearer_auth(token)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Search fetch failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Search API returned {}: {}", status, body));
    }

    // Search response structure matches AuthorFeedResponse (posts are in 'posts' field usually, but let's check docs or assume standard feed)
    // Actually searchPosts returns { posts: [...], cursor: ... } directly, not { feed: [{ post: ... }] }
    // Let's define a specific struct for search response.
    #[derive(Debug, Deserialize)]
    struct SearchResponse {
        posts: Vec<FeedPost>,
        cursor: Option<String>,
    }

    let search_response: SearchResponse = response.json()
        .await
        .map_err(|e| format!("Failed to parse search response: {}", e))?;

    println!("bluesky: search_posts received {} raw results", search_response.posts.len());

    let mut posts: Vec<BlueskyImportedPost> = Vec::new();
    let tags_set: Option<std::collections::HashSet<String>> = required_tags.map(|tags| {
        tags.iter().map(|t| t.to_lowercase()).collect()
    });

    for post in search_response.posts {
        // Strict Hashtag Filtering
        if let Some(ref required) = tags_set {
            let mut has_match = false;
            if let Some(ref facets) = post.record.facets {
                for facet in facets {
                    for feature in &facet.features {
                        if let FacetFeature::Tag { tag } = feature {
                            if required.contains(&tag.to_lowercase()) {
                                has_match = true;
                                break;
                            }
                        }
                    }
                    if has_match { break; }
                }
            }
            if !has_match {
                continue; 
            }
        }
        let text = post.record.text.unwrap_or_default();

        let created_at = post.record.created_at.as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp())
            .unwrap_or(0);

        let mut image_urls: Vec<String> = Vec::new();
        let mut video_url: Option<String> = None;
        let mut video_thumbnail: Option<String> = None;
        let mut external_url: Option<String> = None;
        let mut external_title: Option<String> = None;
        let mut external_description: Option<String> = None;
        let mut external_thumb: Option<String> = None;
        let mut quoted_post: Option<Box<BlueskyImportedPost>> = None;

        if let Some(ref embed) = post.embed {
            extract_image_urls_from_view(embed, &mut image_urls);
            extract_video_from_view(embed, &mut video_url, &mut video_thumbnail);
            extract_external_from_view(embed, &mut external_url, &mut external_title, &mut external_description, &mut external_thumb);
            quoted_post = extract_quote_from_view(embed);
        }

        let mut new_post = BlueskyImportedPost {
            uri: post.uri,
            cid: post.cid,
            text,
            created_at,
            image_urls,
            image_local_paths: Vec::new(),
            author_did: post.author.did,
            author_handle: post.author.handle,
            author_avatar: post.author.avatar,
            reply_count: post.reply_count,
            repost_count: post.repost_count,
            like_count: post.like_count,
            video_url,
            video_thumbnail: if video_thumbnail.is_some() { video_thumbnail } else { external_thumb.clone() },
            external_url,
            external_title,
            external_description,
            external_thumb,
            reposted_by: None,
            reposted_by_handle: None,
            quoted_post,
            viewer_like: post.viewer.as_ref().and_then(|v| v.like.clone()),
            viewer_repost: post.viewer.as_ref().and_then(|v| v.repost.clone()),
        };

        if let Some(ref embed) = post.embed {
            extract_video_from_view(embed, &mut new_post.video_url, &mut new_post.video_thumbnail);
            extract_external_from_view(embed, &mut new_post.external_url, &mut new_post.external_title, &mut new_post.external_description, &mut new_post.external_thumb);
        }
        
        posts.push(new_post);
    }

    Ok((posts, search_response.cursor))
}


/// Fetch list of users the actor follows.
pub async fn get_follows(
    access_jwt: &str,
    actor: &str,
    limit: u8,
    cursor: Option<String>,
) -> Result<(Vec<ProfileView>, Option<String>), String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;
    let mut url = format!("{}/app.bsky.graph.getFollows?actor={}&limit={}", PUBLIC_XRPC, actor, limit);
    if let Some(c) = cursor {
        url.push_str(&format!("&cursor={}", c));
    }

    let response = client.get(&url)
        .header("Authorization", format!("Bearer {}", access_jwt))
        .send()
        .await
        .map_err(|e| format!("Follows fetch failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Follows API error: {}", response.status()));
    }

    let res: GetFollowsResponse = response.json().await
        .map_err(|e| format!("Parse follows failed: {}", e))?;

    Ok((res.follows, res.cursor))
}

/// Fetch saved feeds (including pinned timeline/custom feeds).
pub async fn get_saved_feeds(
    access_jwt: &str,
) -> Result<Vec<GeneratorView>, String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;
    
    // 1. Get Preferences (try PDS first for authenticated user prefs, then public endpoint)
    let token = access_jwt.trim();
    let mut res_opt: Option<GetPreferencesResponse> = None;
    let mut pref_err = String::new();

    for base in authenticated_xrpc_candidates(token, true) {
        let url = format!("{}/app.bsky.actor.getPreferences", base);
        let response = client.get(&url)
            .bearer_auth(token)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| format!("Preferences fetch failed: {}", e))?;

        if response.status().is_success() {
            let parsed: GetPreferencesResponse = response.json().await
                .map_err(|e| format!("Parse preferences failed: {}", e))?;
            res_opt = Some(parsed);
            break;
        }

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        pref_err = format!("Preferences API error from {}: {} {}", base, status, body);
        println!("{}", pref_err);
    }

    let res = res_opt.ok_or_else(|| {
        if pref_err.is_empty() {
            "Preferences API error: unknown".to_string()
        } else {
            pref_err
        }
    })?;
    println!("bluesky: get_saved_feeds raw preferences count: {}", res.preferences.len());
    for pref in &res.preferences {
        if let Some(t) = pref.get("$type").and_then(|t| t.as_str()) {
            println!("bluesky: preference found: {}", t);
        }
    }

    // 2. Extract Feed URIs
    fn collect_feed_uris(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(s) => {
                if s.starts_with("at://") && s.contains("/app.bsky.feed.generator/") {
                    out.push(s.to_string());
                }
            }
            serde_json::Value::Array(arr) => {
                for v in arr {
                    collect_feed_uris(v, out);
                }
            }
            serde_json::Value::Object(map) => {
                // common shapes in preferences
                if let Some(v) = map.get("value").and_then(|v| v.as_str()) {
                    if v.starts_with("at://") && v.contains("/app.bsky.feed.generator/") {
                        out.push(v.to_string());
                    }
                }
                if let Some(v) = map.get("uri").and_then(|v| v.as_str()) {
                    if v.starts_with("at://") && v.contains("/app.bsky.feed.generator/") {
                        out.push(v.to_string());
                    }
                }
                for v in map.values() {
                    collect_feed_uris(v, out);
                }
            }
            _ => {}
        }
    }

    let mut feed_uris = Vec::new();
    for pref in res.preferences {
        collect_feed_uris(&pref, &mut feed_uris);
    }

    // Deduplicate while preserving order
    let mut seen = std::collections::HashSet::new();
    feed_uris.retain(|uri| seen.insert(uri.clone()));

    if feed_uris.is_empty() {
        return Ok(Vec::new());
    }

    // 3. Resolve Feed Generators (best effort, tolerant to partial/invalid feeds)
    let feeds_url = format!("{}/app.bsky.feed.getFeedGenerators", PUBLIC_XRPC);
    let mut resolved_feeds: Vec<GeneratorView> = Vec::new();

    // Query in small batches to avoid 400/414 from a single bad URI poisoning all results.
    for chunk in feed_uris.chunks(10) {
        let params: Vec<(&str, String)> = chunk.iter().cloned().map(|uri| ("feeds", uri)).collect();

        let feed_resp = match client.get(&feeds_url)
            .header("Authorization", format!("Bearer {}", access_jwt))
            .query(&params)
            .send()
            .await {
                Ok(r) => r,
                Err(e) => {
                    println!("bluesky: getFeedGenerators batch request failed: {}", e);
                    continue;
                }
            };

        if !feed_resp.status().is_success() {
            println!("bluesky: getFeedGenerators batch returned {} (continuing)", feed_resp.status());
            continue;
        }

        match feed_resp.json::<GetFeedGeneratorsResponse>().await {
            Ok(gen_res) => resolved_feeds.extend(gen_res.feeds),
            Err(e) => println!("bluesky: getFeedGenerators parse failed: {}", e),
        }
    }

    // 4. Fallback for unresolved URIs: return synthetic feed entries so UI still works.
    let resolved_uris: std::collections::HashSet<String> = resolved_feeds.iter().map(|f| f.uri.clone()).collect();
    for uri in feed_uris {
        if resolved_uris.contains(&uri) {
            continue;
        }

        let trimmed = uri.trim_start_matches("at://");
        let mut parts = trimmed.split('/');
        let did = parts.next().unwrap_or_default().to_string();
        let _collection = parts.next();
        let rkey = parts.next().unwrap_or("feed");

        resolved_feeds.push(GeneratorView {
            uri: uri.clone(),
            cid: String::new(),
            did,
            display_name: Some(rkey.replace('-', " ")),
            description: None,
            avatar: None,
        });
    }

    Ok(resolved_feeds)
}

/// Fetch posts from a specific custom feed generator (e.g. #Cats, What's Hot).
pub async fn get_custom_feed(
    access_jwt: &str,
    feed_uri: &str,
    limit: u8,
    cursor: Option<String>,
) -> Result<(Vec<BlueskyImportedPost>, Option<String>), String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;
    println!("bluesky: get_custom_feed uri='{}' limit={}", feed_uri, limit);
    let is_following_feed = feed_uri.contains("app.bsky.feed.generator/following");
    if is_following_feed {
        println!("bluesky: Detected standard 'Following' feed URI: {}", feed_uri);
    }
    
    let encoded_feed = urlencoding::encode(feed_uri);
    let mut url = format!("{}/app.bsky.feed.getFeed?feed={}&limit={}", PUBLIC_XRPC, encoded_feed, limit);
    if let Some(c) = cursor {
        url.push_str(&format!("&cursor={}", urlencoding::encode(&c)));
    }

    let response = client.get(&url)
        .header("Authorization", format!("Bearer {}", access_jwt))
        .send()
        .await
        .map_err(|e| format!("Custom feed fetch failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Custom Feed API error: {}", response.status()));
    }

    let feed_response: AuthorFeedResponse = response.json().await
        .map_err(|e| format!("Parse custom feed failed: {}", e))?;

    let mut posts: Vec<BlueskyImportedPost> = Vec::new();
    for item in feed_response.feed {
        let post = item.post;
        let text = post.record.text.unwrap_or_default();
        let created_at = post.record.created_at.as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp())
            .unwrap_or(0);

        let mut image_urls: Vec<String> = Vec::new();
        if let Some(ref embed) = post.embed {
            extract_image_urls_from_view(embed, &mut image_urls);
        }

        let mut new_post = BlueskyImportedPost {
            uri: post.uri,
            cid: post.cid,
            text,
            created_at,
            image_urls,
            image_local_paths: Vec::new(),
            author_did: post.author.did,
            author_handle: post.author.handle,
            author_avatar: post.author.avatar,
            reply_count: post.reply_count,
            repost_count: post.repost_count,
            like_count: post.like_count,
            video_url: None,
            video_thumbnail: None,
            external_url: None,
            external_title: None,
            external_description: None,
            external_thumb: None,
            reposted_by: None,
            reposted_by_handle: None,
            quoted_post: None,
            viewer_like: post.viewer.as_ref().and_then(|v| v.like.clone()),
            viewer_repost: post.viewer.as_ref().and_then(|v| v.repost.clone()),
        };

        if let Some(ref embed) = post.embed {
            extract_video_from_view(embed, &mut new_post.video_url, &mut new_post.video_thumbnail);
            extract_external_from_view(embed, &mut new_post.external_url, &mut new_post.external_title, &mut new_post.external_description, &mut new_post.external_thumb);
        }

        posts.push(new_post);
    }

    Ok((posts, feed_response.cursor))
}



/// Fetch posts liked by the actor.
pub async fn get_actor_likes(
    access_jwt: &str,
    actor: &str,
    limit: u8,
    cursor: Option<String>,
) -> Result<(Vec<BlueskyImportedPost>, Option<String>), String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;
    let mut url = format!("{}/app.bsky.feed.getActorLikes?actor={}&limit={}", PUBLIC_XRPC, actor, limit);
    if let Some(c) = cursor {
        url.push_str(&format!("&cursor={}", c));
    }

    let response = client.get(&url)
        .header("Authorization", format!("Bearer {}", access_jwt))
        .send()
        .await
        .map_err(|e| format!("Likes fetch failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Likes API error: {}", response.status()));
    }

    let feed_response: AuthorFeedResponse = response.json().await
        .map_err(|e| format!("Parse likes failed: {}", e))?;

    let mut posts: Vec<BlueskyImportedPost> = Vec::new();
    for item in feed_response.feed {
        let post = item.post;
        let text = post.record.text.unwrap_or_default();
        let created_at = post.record.created_at.as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp())
            .unwrap_or(0);

        let mut image_urls: Vec<String> = Vec::new();
        if let Some(ref embed) = post.embed {
            extract_image_urls_from_view(embed, &mut image_urls);
        }

        let mut new_post = BlueskyImportedPost {
            uri: post.uri,
            cid: post.cid,
            text,
            created_at,
            image_urls,
            image_local_paths: Vec::new(),
            author_did: post.author.did,
            author_handle: post.author.handle,
            author_avatar: post.author.avatar,
            reply_count: post.reply_count,
            repost_count: post.repost_count,
            like_count: post.like_count,
            video_url: None,
            video_thumbnail: None,
            external_url: None,
            external_title: None,
            external_description: None,
            external_thumb: None,
            reposted_by: None,
            reposted_by_handle: None,
            quoted_post: None,
            viewer_like: post.viewer.as_ref().and_then(|v| v.like.clone()),
            viewer_repost: post.viewer.as_ref().and_then(|v| v.repost.clone()),
        };

        if let Some(ref embed) = post.embed {
            extract_video_from_view(embed, &mut new_post.video_url, &mut new_post.video_thumbnail);
            extract_external_from_view(embed, &mut new_post.external_url, &mut new_post.external_title, &mut new_post.external_description, &mut new_post.external_thumb);
        }

        posts.push(new_post);
    }

    Ok((posts, feed_response.cursor))
}

/// Fetch a unified feed of followed hashtags AND saved feeds.
/// Approaches:
/// 1. Fetch Hashtag Search (limit N)
/// 2. Fetch Saved Feeds (fetch list, then fetch top N from each)
#[derive(serde::Serialize, serde::Deserialize, Default, Debug)]
pub struct UnifiedCursor {
    pub hashtag_cursor: Option<String>,
    pub feed_cursors: std::collections::HashMap<String, String>,
}

/// 3. Merge and Sort by Date
pub async fn get_unified_feed(
    access_jwt: &str,
    tags: Vec<String>, // Followed hashtags
    feed_uris: Option<Vec<String>>, // Optional specific feeds to fetch
    limit: u8,
    cursor: Option<String>,
) -> Result<(Vec<BlueskyImportedPost>, Option<String>), String> {
    // 1. Fetch Hashtag Posts
    let parsed_cursor: UnifiedCursor = cursor
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default();

    let tag_query = tags.join(" OR ");
    let strict_tags = if tags.is_empty() { None } else { Some(tags.clone().iter().map(|t| t.trim_start_matches('#').to_string()).collect()) };
    
    println!("UnifiedFeed: Starting fetch. Tags: {:?}, Feeds: {:?}, Limit: {}", tags, feed_uris, limit);

    let hashtag_future = async {
        if tags.is_empty() { 
            println!("UnifiedFeed: No hashtags to fetch.");
            return Ok((Vec::new(), None)); 
        }
        println!("UnifiedFeed: Fetching hashtags query: {}", tag_query);
        match search_posts(access_jwt, &tag_query, limit, parsed_cursor.hashtag_cursor.clone(), strict_tags).await {
            Ok((posts, cursor)) => {
                println!("UnifiedFeed: Fetched {} hashtag posts", posts.len());
                Ok((posts, cursor))
            },
            Err(e) => {
                println!("UnifiedFeed: Hashtag fetch error: {}", e);
                Err(e)
            },
        }
    };

    // 2. Resolve Feed URIs
    let uris_future = async {
        if let Some(provided_uris) = feed_uris {
            if !provided_uris.is_empty() {
                println!("UnifiedFeed: Using {} provided URIs", provided_uris.len());
                Ok(provided_uris)
            } else {
                println!("UnifiedFeed: Provided feed URI list was empty, falling back to saved feeds");
                match get_saved_feeds(access_jwt).await {
                    Ok(views) => {
                        let uris: Vec<String> = views.into_iter()
                            .map(|v| v.uri)
                            .filter(|uri| !uri.contains("app.bsky.feed.generator/following"))
                            .collect();
                        println!("UnifiedFeed: Fallback saved feeds count: {}", uris.len());
                        Ok(uris)
                    },
                    Err(e) => {
                        println!("UnifiedFeed: Saved feeds fallback error: {}", e);
                        Err(e)
                    },
                }
            }
        } else {
            println!("UnifiedFeed: Fetching all saved feeds...");
            match get_saved_feeds(access_jwt).await {
                Ok(views) => {
                    let uris: Vec<String> = views.into_iter()
                        .map(|v| v.uri)
                        .filter(|uri| !uri.contains("app.bsky.feed.generator/following"))
                        .collect();
                    println!("UnifiedFeed: Found {} custom saved feeds (filtered): {:?}", uris.len(), uris);
                    Ok(uris)
                },
                Err(e) => {
                    println!("UnifiedFeed: Saved feeds list error: {}", e);
                    Err(e)
                },
            }
        }
    };

    let (hashtag_res, uris_res) = tokio::join!(hashtag_future, uris_future);
    
    // Check for critical Auth errors first
    if let Err(e) = &hashtag_res {
        if e.contains("ExpiredToken") || e.contains("401") || e.contains("Token has expired") {
            return Err(e.clone());
        }
    }
    if let Err(e) = &uris_res {
        if e.contains("ExpiredToken") || e.contains("401") || e.contains("Token has expired") {
            return Err(e.clone());
        }
    }

    let mut all_posts = Vec::new();
    let mut next_cursor = UnifiedCursor::default();

    // Add Hashtag posts
    if let Ok((posts, cursor)) = hashtag_res {
        all_posts.extend(posts);
        next_cursor.hashtag_cursor = cursor;
    }

    // 3. Fetch Posts from Each Saved Feed (Parallel)
    if let Ok(uris) = uris_res {
        println!("UnifiedFeed: Fetching content from {} saved feeds in parallel...", uris.len());
        let mut feed_futures = Vec::new();
        let mut feed_count = 0usize;
        let mut feed_success_count = 0usize;
        for uri in uris {
            feed_count += 1;
            let jwt = access_jwt.to_string();
            let u = uri.clone();
            let feed_cursor = parsed_cursor.feed_cursors.get(&u).cloned();
            feed_futures.push(tokio::spawn(async move {
                let res = get_custom_feed(&jwt, &u, limit, feed_cursor).await;
                match res {
                    Ok((posts, cursor)) => {
                        println!("UnifiedFeed: Feed content retrieved: {} ({} posts)", u, posts.len());
                        Ok((u, posts, cursor))
                    },
                    Err(e) => {
                        println!("UnifiedFeed: Feed content fetch failed: {} - {}", u, e);
                        Err(e)
                    }
                }
            }));
        }

        // Wait for all feeds
        for (i, future) in feed_futures.into_iter().enumerate() {
            match future.await {
                Ok(Ok((uri, posts, cursor))) => {
                    println!("UnifiedFeed: Merging {} posts from feed #{}", posts.len(), i);
                    feed_success_count += 1;
                    all_posts.extend(posts);
                    if let Some(c) = cursor {
                        next_cursor.feed_cursors.insert(uri, c);
                    }
                },
                Ok(Err(e)) => println!("UnifiedFeed: Future joined but returned error for feed #{}: {}", i, e),
                Err(e) => println!("UnifiedFeed: Future join panicked for feed #{}: {}", i, e),
            }
        }

        if feed_count > 0 && feed_success_count == 0 && all_posts.is_empty() {
            println!("UnifiedFeed: all subscribed feed fetches failed; returning empty result instead of hard error");
        }
    }

    println!("UnifiedFeed: Total raw posts before dedup: {}", all_posts.len());

    // 4. Deduplicate (by URI)
    all_posts.sort_by(|a, b| b.created_at.cmp(&a.created_at)); // Sort desc first
    all_posts.dedup_by(|a, b| a.uri == b.uri);

    println!("UnifiedFeed: Total posts after dedup: {}", all_posts.len());

    // 5. Sort by Date Descending
    all_posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // 6. Truncate
    all_posts.truncate(100); 

    let next_cursor_str = if next_cursor.hashtag_cursor.is_some() || !next_cursor.feed_cursors.is_empty() {
        serde_json::to_string(&next_cursor).ok()
    } else {
        None
    };

    Ok((all_posts, next_cursor_str))
}

/// Import all likes for the actor.
pub async fn import_bluesky_likes(
    actor: &str,
    access_jwt: &str,
    media_dir: PathBuf,
    app_handle: &AppHandle,
    sync_manager: Option<Arc<SyncManager>>,
) -> Result<Vec<BlueskyImportedPost>, String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;
    std::fs::create_dir_all(&media_dir)
        .map_err(|e| format!("Failed to create media dir: {}", e))?;

    let mut all_posts: Vec<BlueskyImportedPost> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut page = 0;

    loop {
        page += 1;
        let _ = app_handle.emit("import-progress", ImportProgress {
            source: Source::Bluesky.as_str().to_string(),
            stage: "fetching_likes".to_string(),
            posts_imported: all_posts.len(),
            photos_downloaded: 0,
            current_item: format!("Fetching likes page {}...", page),
        });

        let mut url = format!(
            "{}/app.bsky.feed.getActorLikes?actor={}&limit=100",
            PUBLIC_XRPC, actor
        );
        if let Some(ref c) = cursor {
            url.push_str(&format!("&cursor={}", c));
        }

        let response = client.get(&url)
            .header("Authorization", format!("Bearer {}", access_jwt))
            .send()
            .await
            .map_err(|e| format!("Likes fetch failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Likes API returned {}: {}", status, body));
        }

        let feed_response: AuthorFeedResponse = response.json().await
            .map_err(|e| format!("Failed to parse likes: {}", e))?;

        if feed_response.feed.is_empty() {
            break;
        }

        for item in &feed_response.feed {
            let post = &item.post;
            let text = post.record.text.clone().unwrap_or_default();
            
            let created_at = post.record.created_at.as_ref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.timestamp())
                .unwrap_or(0);

            let mut image_urls: Vec<String> = Vec::new();
            if let Some(ref embed) = post.embed {
                extract_image_urls_from_view(embed, &mut image_urls);
            }

            let mut image_local_paths: Vec<String> = Vec::new();
            for (i, img_url) in image_urls.iter().enumerate() {
                 let ext = if img_url.contains(".png") { "png" } else { "jpg" };
                 let hash = hex::encode(md5::compute(img_url.as_bytes()).0);
                 let filename = format!("bsky_like_{}_{}.{}", hash, i, ext);
                 let dest = media_dir.join(&filename);

                 // Try to fetch and store
                 if let Ok(resp) = client.get(img_url).send().await {
                     if resp.status().is_success() {
                         if let Ok(bytes) = resp.bytes().await {
                             // Try Iroh blobs first
                             if let Some(ref sync) = sync_manager {
                                 match sync.store_media_with_thumbnail(bytes.clone(), 256).await {
                                     Ok((media_hash, _)) => {
                                         image_local_paths.push(format!("blob:{}", media_hash));
                                         continue;
                                     }
                                     Err(e) => {
                                         eprintln!("bluesky: Iroh store failed, falling back: {}", e);
                                     }
                                 }
                             }
                             // Fallback to filesystem
                             let _ = std::fs::write(&dest, &bytes);
                             image_local_paths.push(format!("media/{}", filename));
                             continue;
                         }
                     }
                 }
                 // If download failed but file exists from previous run
                 if dest.exists() {
                     image_local_paths.push(format!("media/{}", filename));
                 }
            }

            let mut new_post = BlueskyImportedPost {
                uri: post.uri.clone(),
                cid: post.cid.clone(),
                text,
                created_at,
                image_urls,
                image_local_paths,
                author_did: post.author.did.clone(),
                author_handle: post.author.handle.clone(),
                author_avatar: post.author.avatar.clone(),
                reply_count: post.reply_count,
                repost_count: post.repost_count,
                like_count: post.like_count,
                video_url: None,
                video_thumbnail: None,
                external_url: None,
                external_title: None,
                external_description: None,
                external_thumb: None,
                reposted_by: None,
                reposted_by_handle: None,
                quoted_post: None,
                viewer_like: post.viewer.as_ref().and_then(|v| v.like.clone()),
                viewer_repost: post.viewer.as_ref().and_then(|v| v.repost.clone()),
            };

            if let Some(ref embed) = post.embed {
                extract_video_from_view(embed, &mut new_post.video_url, &mut new_post.video_thumbnail);
                extract_external_from_view(embed, &mut new_post.external_url, &mut new_post.external_title, &mut new_post.external_description, &mut new_post.external_thumb);
            }

            all_posts.push(new_post);
        }

        match feed_response.cursor {
            Some(c) if !c.is_empty() => cursor = Some(c),
            _ => break,
        }
    }

    Ok(all_posts)
}

/// Helper to fetch ALL follows for an actor.
pub async fn get_all_follows(
    client: &Client,
    actor: &str,
    access_jwt: &str,
) -> Result<Vec<ProfileView>, String> {
    let mut all_follows = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let mut url = format!("{}/app.bsky.graph.getFollows?actor={}&limit=100", PUBLIC_XRPC, actor);
        if let Some(ref c) = cursor {
            url.push_str(&format!("&cursor={}", c));
        }

        let response = client.get(&url)
            .header("Authorization", format!("Bearer {}", access_jwt))
            .send()
            .await
            .map_err(|e| format!("Follows fetch failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Follows API error: {}", response.status()));
        }

        let res: GetFollowsResponse = response.json().await
            .map_err(|e| format!("Parse follows failed: {}", e))?;

        all_follows.extend(res.follows);

        match res.cursor {
            Some(c) if !c.is_empty() => cursor = Some(c),
            _ => break,
        }
    }

    Ok(all_follows)
}

/// Helper to fetch ALL followers for an actor.
pub async fn get_all_followers(
    client: &Client,
    actor: &str,
    access_jwt: &str,
) -> Result<Vec<ProfileView>, String> {
    let mut all_followers = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let mut url = format!("{}/app.bsky.graph.getFollowers?actor={}&limit=100", PUBLIC_XRPC, actor);
        if let Some(ref c) = cursor {
            url.push_str(&format!("&cursor={}", c));
        }

        let response = client.get(&url)
            .header("Authorization", format!("Bearer {}", access_jwt))
            .send()
            .await
            .map_err(|e| format!("Followers fetch failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Followers API error: {}", response.status()));
        }

        let res: GetFollowersResponse = response.json().await
            .map_err(|e| format!("Parse followers failed: {}", e))?;

        all_followers.extend(res.followers);

        match res.cursor {
            Some(c) if !c.is_empty() => cursor = Some(c),
            _ => break,
        }
    }

    Ok(all_followers)
}

/// Import recent posts from ALL users the actor follows.
pub async fn import_follows_posts(
    actor: &str,
    access_jwt: &str,
    media_dir: PathBuf,
    app_handle: &AppHandle,
    sync_manager: Option<Arc<SyncManager>>,
) -> Result<Vec<BlueskyImportedPost>, String> {
    let client = Client::new();
    
    // 1. Get all follows
    let _ = app_handle.emit("import-progress", ImportProgress {
        source: Source::Bluesky.as_str().to_string(),
        stage: "fetching_follows".to_string(),
        posts_imported: 0,
        photos_downloaded: 0,
        current_item: "Fetching your follow list...".to_string(),
    });

    let follows = get_all_follows(&client, actor, access_jwt).await?;
    println!("Found {} follows", follows.len());

    let mut all_posts = Vec::new();
    let total_follows = follows.len();

    // 2. Iterate and import
    for (i, follow) in follows.iter().enumerate() {
        let _ = app_handle.emit("import-progress", ImportProgress {
            source: Source::Bluesky.as_str().to_string(),
            stage: "fetching_follows_posts".to_string(),
            posts_imported: all_posts.len(),
            photos_downloaded: 0,
            current_item: format!("Scanning {} ({}/{})", 
                follow.display_name.as_deref().unwrap_or(&follow.handle), 
                i + 1, total_follows
            ),
        });

        println!("bluesky: Importing feed for {} ({})", follow.handle, follow.did);
        match import_bluesky_feed(&follow.did, access_jwt, media_dir.clone(), app_handle, Some(1), Some(20), sync_manager.clone()).await {
            Ok(mut posts) => {
                println!("bluesky: Found {} posts for {}", posts.len(), follow.handle);
                all_posts.append(&mut posts);
            },
            Err(e) => eprintln!("Failed to import feed for {}: {}", follow.handle, e),
        }
    }

    Ok(all_posts)
}

/// Import all follows as Contacts
pub async fn import_bluesky_follows_as_contacts(
    actor: &str,
    access_jwt: &str,
    db_manager: Arc<tokio::sync::Mutex<Option<crate::db::DbManager>>>,
    _app_handle: &AppHandle,
) -> Result<usize, String> {
    let client = Client::new();
    let follows = get_all_follows(&client, actor, access_jwt).await?;
    
    let db_guard: tokio::sync::MutexGuard<'_, Option<crate::db::DbManager>> = db_manager.lock().await;
    let db = db_guard.as_ref().ok_or("Database not initialized")?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::from_secs(0))
        .as_secs();

    let mut count = 0;
    for follow in follows {
        let contact = crate::db::Contact {
            did: follow.did,
            handle: follow.handle,
            p2p_capable: false, // Default to false for Bluesky imports
            node_id: None,
            last_polled_at: now,
        };
        if let Ok(_) = db.save_contact(&contact).await {
            count += 1;
        }
    }
    
    Ok(count)
}

// === BlueskyClient: Authenticated HTTP helper for PDS interactions ===

/// A lightweight authenticated client for Bluesky PDS operations.
/// Wraps `reqwest::Client` with the user's access JWT for blob uploads and record creation.
pub struct BlueskyClient {
    http: Client,
    access_jwt: String,
}

impl BlueskyClient {
    /// Create a new BlueskyClient with an already-validated access JWT.
    ///
    /// Prefer this over `new()` — call `state.get_fresh_access_jwt()` first
    /// to ensure the token is valid and auto-refreshed.
    pub fn with_jwt(access_jwt: &str) -> Self {
        Self {
            http: Client::new(),
            access_jwt: access_jwt.to_string(),
        }
    }

    /// Create a new BlueskyClient from AppState (legacy, no auto-refresh).
    pub fn new(state: &crate::AppState) -> Self {
        let access_jwt = state
            .keychain
            .get_access_jwt()
            .ok()
            .flatten()
            .unwrap_or_default();

        Self {
            http: Client::new(),
            access_jwt,
        }
    }

    /// Upload a binary blob (image/video) to the Bluesky PDS.
    pub async fn post_blob(
        &self,
        url: &str,
        mime: &str,
        data: Vec<u8>,
    ) -> Result<reqwest::Response, String> {
        self.http
            .post(url)
            .header("Authorization", format!("Bearer {}", self.access_jwt))
            .header("Content-Type", mime)
            .body(data)
            .send()
            .await
            .map_err(|e| format!("Blob upload request failed: {}", e))
    }

    /// Create a record (e.g. a post) on the Bluesky PDS.
    pub async fn create_record(
        &self,
        url: &str,
        body: serde_json::Value,
    ) -> Result<reqwest::Response, String> {
        self.http
            .post(url)
            .header("Authorization", format!("Bearer {}", self.access_jwt))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Create record request failed: {}", e))
    }

    /// Delete a record (e.g. unlike, unrepost) on the Bluesky PDS.
    pub async fn delete_record(
        &self,
        url: &str,
        body: serde_json::Value,
    ) -> Result<reqwest::Response, String> {
        self.http
            .post(url)
            .header("Authorization", format!("Bearer {}", self.access_jwt))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Delete record request failed: {}", e))
    }

    /// Fetch a resource via authenticated GET request.
    pub async fn get_resource(
        &self,
        url: &str,
    ) -> Result<reqwest::Response, String> {
        self.http
            .get(url)
            .header("Authorization", format!("Bearer {}", self.access_jwt))
            .send()
            .await
            .map_err(|e| format!("GET request failed: {}", e))
    }
}
