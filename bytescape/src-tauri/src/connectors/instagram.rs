use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::AppHandle;

use crate::sync::SyncManager;
use crate::commands::import::ParseResult;

use crate::connectors::facebook::ParsedPost;

fn normalize_zip_media_uri(uri: &str) -> String {
    let mut s = uri.trim().replace('\\', "/");
    while s.starts_with("./") {
        s = s[2..].to_string();
    }
    s.trim_start_matches('/').to_string()
}

fn detect_top_level_prefix(file_names: &[String]) -> Option<String> {
    let first = file_names.first()?;
    let prefix = if let Some(slash_pos) = first.find('/') {
        &first[..=slash_pos]
    } else {
        return None;
    };

    let matching = file_names.iter().filter(|n| n.starts_with(prefix)).count();
    if matching > file_names.len() / 2 {
        Some(prefix.to_string())
    } else {
        None
    }
}

fn is_numbered_posts_file(basename: &str) -> bool {
    if !basename.starts_with("posts_") || !basename.ends_with(".json") {
        return false;
    }
    let number_part = &basename[6..basename.len() - 5];
    !number_part.is_empty() && number_part.chars().all(|c| c.is_ascii_digit())
}

fn post_from_single_media(media: InstagramMedia) -> InstagramPost {
    InstagramPost {
        title: media.title.clone(),
        creation_timestamp: media.creation_timestamp,
        media: vec![media],
    }
}

fn parse_instagram_media_file(file_name: &str, raw_json: &str) -> Result<Vec<InstagramPost>, String> {
    let basename = PathBuf::from(file_name)
        .file_name()
        .and_then(|f| f.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    if is_numbered_posts_file(&basename) {
        let posts: Vec<InstagramPost> = serde_json::from_str(raw_json)
            .map_err(|e| format!("Failed to parse {}: {}", basename, e))?;
        return Ok(posts);
    }

    if basename == "reels.json" {
        #[derive(Debug, Deserialize)]
        struct ReelsRoot {
            #[serde(default)]
            ig_reels_media: Vec<InstagramPost>,
        }

        let root: ReelsRoot = serde_json::from_str(raw_json)
            .map_err(|e| format!("Failed to parse reels.json: {}", e))?;
        return Ok(root
            .ig_reels_media
            .into_iter()
            .filter(|p| !p.media.is_empty())
            .collect());
    }

    if basename == "stories.json" {
        #[derive(Debug, Deserialize)]
        struct StoriesRoot {
            #[serde(default)]
            ig_stories: Vec<InstagramMedia>,
        }

        let root: StoriesRoot = serde_json::from_str(raw_json)
            .map_err(|e| format!("Failed to parse stories.json: {}", e))?;
        return Ok(root
            .ig_stories
            .into_iter()
            .filter(|m| m.uri.is_some())
            .map(post_from_single_media)
            .collect());
    }

    if basename == "profile_photos.json" {
        #[derive(Debug, Deserialize)]
        struct ProfilePhotosRoot {
            #[serde(default)]
            ig_profile_picture: Vec<InstagramMedia>,
        }

        let root: ProfilePhotosRoot = serde_json::from_str(raw_json)
            .map_err(|e| format!("Failed to parse profile_photos.json: {}", e))?;
        return Ok(root
            .ig_profile_picture
            .into_iter()
            .filter(|m| m.uri.is_some())
            .map(post_from_single_media)
            .collect());
    }

    Ok(Vec::new())
}

/// =============================================================
/// Instagram Export JSON Schema Structs
/// =============================================================

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct InstagramPost {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub creation_timestamp: Option<i64>,
    #[serde(default)]
    pub media: Vec<InstagramMedia>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct InstagramMedia {
    pub uri: Option<String>,
    pub creation_timestamp: Option<i64>,
    pub title: Option<String>,
}

/// =============================================================
/// Parsed Output Structs
/// =============================================================

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ParsedInstagramPost {
    pub id: String,
    pub timestamp: i64,
    pub content: String,
    pub media_paths: Vec<String>,
}

/// =============================================================
/// Main Parser Function
/// =============================================================

pub fn parse_instagram_zip(
    zip_path: PathBuf,
    media_dir: PathBuf,
    _app_handle: &AppHandle,
    sync_manager: Option<Arc<SyncManager>>,
    rt_handle: tokio::runtime::Handle,
) -> Result<ParseResult, String> {
    let file = std::fs::File::open(&zip_path)
        .map_err(|e| format!("Failed to open ZIP: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Failed to read ZIP: {}", e))?;

    let mut parsed_posts = Vec::new();
    let mut total_photos = 0;
    let mut total_videos = 0;

    let mut file_names: Vec<String> = Vec::new();
    let mut entry_lookup: HashMap<String, String> = HashMap::new();
    let mut basename_lookup: HashMap<String, Vec<String>> = HashMap::new();

    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| format!("Failed reading ZIP entry {}: {}", i, e))?;
        let name = entry.name().replace('\\', "/");
        let lower = name.to_lowercase();
        file_names.push(name.clone());
        entry_lookup.insert(lower, name.clone());

        if let Some(base) = PathBuf::from(&name)
            .file_name()
            .and_then(|f| f.to_str())
            .map(|s| s.to_lowercase())
        {
            basename_lookup.entry(base).or_default().push(name.clone());
        }
    }

    let top_prefix = detect_top_level_prefix(&file_names);

    // 1. Find and parse all relevant media JSON files (posts_x, reels, stories, profile_photos).
    let mut media_json_files: Vec<String> = file_names
        .iter()
        .filter(|name| {
            let lower = name.to_lowercase();
            if !lower.ends_with(".json") {
                return false;
            }
            let in_media_export = lower.contains("/your_instagram_activity/media/")
                || lower.starts_with("your_instagram_activity/media/");
            if !in_media_export {
                return false;
            }

            let lower_path = PathBuf::from(&lower);
            let basename = lower_path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("");

            is_numbered_posts_file(basename)
                || basename == "reels.json"
                || basename == "stories.json"
                || basename == "profile_photos.json"
        })
        .cloned()
        .collect();

    media_json_files.sort();
    media_json_files.dedup();

    if media_json_files.is_empty() {
        return Err("Could not find Instagram media JSON files in ZIP (expected your_instagram_activity/media/posts_*.json, reels.json, stories.json)".to_string());
    }

    let mut posts: Vec<InstagramPost> = Vec::new();
    for json_file in media_json_files {
        let mut raw = String::new();
        let mut read_ok = false;

        if let Ok(mut entry) = archive.by_name(&json_file) {
            entry
                .read_to_string(&mut raw)
                .map_err(|e| format!("Failed to read {}: {}", json_file, e))?;
            read_ok = true;
        }

        if !read_ok {
            let key = json_file.to_lowercase();
            let Some(real_name) = entry_lookup.get(key.as_str()) else {
                continue;
            };
            let mut entry = archive
                .by_name(real_name)
                .map_err(|e| format!("Failed to open {}: {}", real_name, e))?;
            entry
                .read_to_string(&mut raw)
                .map_err(|e| format!("Failed to read {}: {}", real_name, e))?;
        }

        let mut parsed = parse_instagram_media_file(&json_file, &raw)?;
        posts.append(&mut parsed);
    }

    if posts.is_empty() {
        return Err("Instagram media files were found but no posts/media entries could be parsed".to_string());
    }

    // 2. Process each post
    for post in posts {
        let mut media_uris = Vec::new();
        let mut media_local_paths = Vec::new();
        let mut post_timestamp = post.creation_timestamp.unwrap_or(0);
        let mut post_content = post.title.clone().unwrap_or_default();

        for media in post.media {
            if let Some(uri) = media.uri {
                media_uris.push(uri.clone());

                // Extract media from ZIP with resilient lookup (normalized + prefixed + basename).
                let normalized_uri = normalize_zip_media_uri(&uri);
                let mut candidates: Vec<String> = vec![uri.clone()];
                if normalized_uri != uri {
                    candidates.push(normalized_uri.clone());
                }
                if let Some(prefix) = &top_prefix {
                    candidates.push(format!("{}{}", prefix, normalized_uri));
                }

                let mut found_media_bytes: Option<Vec<u8>> = None;

                for candidate in &candidates {
                    if let Ok(mut entry) = archive.by_name(candidate) {
                        let mut buffer = Vec::new();
                        if entry.read_to_end(&mut buffer).is_ok() && !buffer.is_empty() {
                            found_media_bytes = Some(buffer);
                            break;
                        }
                    }

                    let candidate_lower = candidate.to_lowercase();
                    if let Some(real_name) = entry_lookup.get(candidate_lower.as_str()) {
                        if let Ok(mut entry) = archive.by_name(real_name) {
                            let mut buffer = Vec::new();
                            if entry.read_to_end(&mut buffer).is_ok() && !buffer.is_empty() {
                                found_media_bytes = Some(buffer);
                                break;
                            }
                        }
                    }
                }

                if found_media_bytes.is_none() {
                    if let Some(base_lower) = PathBuf::from(&normalized_uri)
                        .file_name()
                        .and_then(|f| f.to_str())
                        .map(|s| s.to_lowercase())
                    {
                        if let Some(entry_names) = basename_lookup.get(&base_lower) {
                            for entry_name in entry_names {
                                if let Ok(mut entry) = archive.by_name(entry_name) {
                                    let mut buffer = Vec::new();
                                    if entry.read_to_end(&mut buffer).is_ok() && !buffer.is_empty() {
                                        found_media_bytes = Some(buffer);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(buffer) = found_media_bytes {
                    let lower_uri = uri.to_lowercase();
                    let clean_uri = lower_uri.split('?').next().unwrap_or(&lower_uri);
                    let clean_uri = clean_uri.split('#').next().unwrap_or(clean_uri);
                    let is_video = clean_uri.ends_with(".mp4")
                        || clean_uri.ends_with(".mov")
                        || clean_uri.ends_with(".m4v")
                        || clean_uri.ends_with(".webm")
                        || clean_uri.ends_with(".mkv")
                        || clean_uri.contains("/videos/")
                        || clean_uri.contains("_video");
                    if is_video {
                        total_videos += 1;
                    } else {
                        total_photos += 1;
                    }

                    // Add to Iroh when available; fallback to filesystem path.
                    if let Some(ref sync) = sync_manager {
                        let sync_clone = sync.clone();
                        let bytes = bytes::Bytes::from(buffer.clone());
                        let hash_result = rt_handle.block_on(async move {
                            sync_clone.store_media_with_thumbnail(bytes, 256).await
                        });

                        if let Ok((hash, _thumb)) = hash_result {
                            media_local_paths.push(format!("blob:{}", hash));
                            continue;
                        }
                    }

                    let file_name = PathBuf::from(&normalized_uri)
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| format!("instagram_media_{}", post_timestamp));
                    let out_path = media_dir.join(file_name);
                    if std::fs::create_dir_all(&media_dir).is_ok() && std::fs::write(&out_path, &buffer).is_ok() {
                        media_local_paths.push(out_path.to_string_lossy().to_string());
                    } else {
                        media_local_paths.push(String::new()); // sentinel to keep alignment
                    }
                } else {
                    media_local_paths.push(String::new()); // sentinel to keep alignment with media_uris
                }
            }

            // If the post didn't have a top-level timestamp/title, use the first media's
            if post_timestamp == 0 {
                post_timestamp = media.creation_timestamp.unwrap_or(0);
            }
            if post_content.is_empty() {
                post_content = media.title.unwrap_or_default();
            }
        }

        parsed_posts.push(ParsedPost {
            timestamp: post_timestamp,
            content: post_content,
            media_uris,
            media_local_paths,
            title: post.title,
            author_name: None,
            action_type: None,
            tagged_people: vec![],
            location: None,
        });
    }

    Ok(ParseResult {
        posts: parsed_posts,
        trackers: vec![],
        total_photos,
        total_videos,
        total_tracker_events: 0,
        source: "instagram".to_string(),
    })
}
