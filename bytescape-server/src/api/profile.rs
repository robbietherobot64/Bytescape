//! API — Profile routes.

use axum::{
    extract::{Multipart, Path, Query, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use std::collections::HashSet;

use crate::api::feed::{normalize_bluesky_post, AuthorInfo, EmbedInfo, FeedPost, FeedResponse, MediaAttachment};
use crate::db;
use crate::error::AppError;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/profile/me", get(get_profile).put(update_profile))
        .route("/api/profile/me/avatar", post(upload_avatar))
        .route("/api/profile/me/header", post(upload_header))
        .route("/api/profile/{username}", get(public_profile))
        .route("/api/profile/{username}/feed", get(public_feed))
        .route("/api/profile/{username}/follow", post(follow_user))
        .route("/api/profile/{username}/unfollow", post(unfollow_user))
        .route("/api/profile/{username}/wall", get(list_wall).post(post_wall))
        .route("/api/profile/{username}/following", get(list_following))
        .route("/api/profile/{username}/followers", get(list_followers))
        .route(
            "/api/profile/{username}/followers/remove",
            post(remove_follower),
        )
        .route("/api/profile/{username}/friends", get(list_friends))
        .route("/api/profile/relationships", get(relationships))
}

#[derive(Debug, Serialize, Deserialize)]
struct WidgetConfig {
    id: String,
    widget_type: String,
    x: u8,
    y: u8,
    w: u8,
    h: u8,
    data: serde_json::Value,
    theme_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct WidgetTheme {
    id: String,
    name: String,
    background: Option<String>,
    border_color: Option<String>,
    border_radius: Option<String>,
    text_color: Option<String>,
    font_family: Option<String>,
    padding: Option<String>,
    shadow: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct CustomProfileField {
    field_key: String,
    label: Option<String>,
    content: Option<String>,
    visible: bool,
}

#[derive(Debug, Serialize)]
struct ProfileResponse {
    id: String,
    username: String,
    display_name: String,
    bio: String,
    avatar_url: Option<String>,
    header_url: Option<String>,
    theme: String,
    location: Option<String>,
    website: Option<String>,
    pronouns: Option<String>,
    relationship_status: Option<String>,
    birthday: Option<String>,
    occupation: Option<String>,
    interests: Vec<String>,
    onboarded: bool,
    nsfw_pref: String,
    widget_layout: Vec<WidgetConfig>,
    widget_themes: Vec<WidgetTheme>,
    custom_profile_fields: Vec<CustomProfileField>,
    created_at: String,
    posts_count: i64,
    followers_count: i32,
    following_count: i32,
    visitor_count: i64,
    is_own_profile: bool,
    is_following: bool,
    /// The viewer's follow toward this profile is a pending (requested)
    /// follow — the locked-account state (identity-graph P4/P5: follower-side
    /// UI shows Requested + cancel).
    follow_pending: bool,
}

#[derive(Debug, Deserialize)]
struct UpdateProfileRequest {
    display_name: Option<String>,
    bio: Option<String>,
    theme: Option<String>,
    custom_css: Option<String>,
    avatar_url: Option<String>,
    header_url: Option<String>,
    location: Option<String>,
    website: Option<String>,
    pronouns: Option<String>,
    relationship_status: Option<String>,
    birthday: Option<String>,
    occupation: Option<String>,
    interests: Option<Vec<String>>,
    nsfw_pref: Option<String>,
    widget_layout: Option<Vec<WidgetConfig>>,
    widget_themes: Option<Vec<WidgetTheme>>,
    custom_profile_fields: Option<Vec<CustomProfileField>>,
}

#[derive(Debug, Deserialize)]
struct FeedQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    source: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RelationshipsQuery {
    usernames: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WallQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct PostWallRequest {
    content: String,
}

#[derive(Debug, Serialize)]
struct WallPostView {
    id: String,
    author: AuthorInfo,
    content: String,
    created_at: String,
}

#[derive(Debug, Serialize)]
struct FollowingView {
    username: String,
    handle: String,
    display_name: Option<String>,
    avatar: Option<String>,
    is_local: bool,
    /// Which network the relationship lives on: `local` | `activitypub` |
    /// `atproto` (identity-graph Phase 3 provenance).
    network: String,
    /// Which of the profile owner's identities produced this row (local
    /// username, minted actor URL, AT DID, or home acct).
    identity_used: String,
    /// The VIEWER's follow state for this row (shared matcher — the same
    /// exact source-URL map the feed's follow badges use).
    following: bool,
    follow_pending: bool,
    /// Remove-follower affordance (own profile + the network permits it).
    can_remove: bool,
}

fn extract_user_id(headers: &HeaderMap, secret: &str) -> Result<Uuid, AppError> {
    let auth = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Auth("Missing or invalid Authorization header".into()))?;

    let claims = crate::auth::validate_token(auth, secret)?;
    Uuid::parse_str(&claims.sub).map_err(|_| AppError::Auth("Invalid user ID in token".into()))
}

fn extract_content_text(content: &Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(obj) = content.as_object()
        && let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
            return text.to_string();
        }
    content.to_string()
}

pub fn post_row_to_feed_post(row: &db::posts::PostRow, author_info: &AuthorInfo, original_post: Option<FeedPost>) -> FeedPost {
    let effective_author = if row.content_type == "event" {
        let handle = row.content.get("author_handle")
            .and_then(|v| v.as_str())
            .unwrap_or(&author_info.handle);
        let display_name = row.content.get("author_display_name")
            .and_then(|v| v.as_str());
        AuthorInfo {
            handle: handle.to_string(),
            display_name: display_name.map(|s| s.to_string()),
            avatar: None,
        }
    } else {
        author_info.clone()
    };

    let media = if row.content_type == "event" {
        row.content.get("media")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter().filter_map(|m| {
                    Some(MediaAttachment {
                        url: m["url"].as_str()?.to_string(),
                        thumbnail_url: None,
                        mime_type: Some("image/jpeg".to_string()),
                        width: None,
                        height: None,
                        alt_text: None,
                    })
                }).collect()
            })
            .unwrap_or_default()
    } else {
        row.content.get("media_urls")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter().filter_map(|u| {
                    let url = u.as_str()?.to_string();
                    let mime_type = if url.ends_with(".mp4") || url.ends_with(".mov") || url.ends_with(".webm") {
                        Some("video/mp4".to_string())
                    } else if url.ends_with(".gif") {
                        Some("image/gif".to_string())
                    } else if url.ends_with(".png") {
                        Some("image/png".to_string())
                    } else {
                        Some("image/jpeg".to_string())
                    };
                    Some(MediaAttachment {
                        url,
                        thumbnail_url: None,
                        mime_type,
                        width: None,
                        height: None,
                        alt_text: None,
                    })
                }).collect()
            })
            .unwrap_or_default()
    };

    let group_name = row.content.get("group_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let display_name = row.content.get("display_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Imported archive posts must say where they came from — "via Instagram
    // archive" — instead of reading as native Bytescape posts. The import
    // parsers stamp a generic action ("posted"/"boosted"/"replied") into
    // content.action_text, so for imported posts that generic placeholder is
    // replaced by the archive-origin label. Meaningful actions ("updated his
    // status", "added a photo") are kept alongside the origin.
    let source_label = match row.source.as_str() {
        "instagram" => "Instagram",
        "facebook" => "Facebook",
        "threads" => "Threads",
        "mastodon" => "Mastodon",
        other => other,
    };
    let stored_action = row.content.get("action_text")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let action_text = if row.is_imported {
        let generic = matches!(
            stored_action.as_deref(),
            None | Some("posted") | Some("boosted") | Some("replied")
        );
        if generic {
            Some(format!("via {} archive", source_label))
        } else {
            stored_action
        }
    } else {
        stored_action
    };

    let link_url = row.content.get("link_url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let link_title = row.content.get("link_title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let embeds: Vec<EmbedInfo> = if let Some(url) = link_url {
        vec![EmbedInfo {
            kind: "link".to_string(),
            url,
            title: link_title,
            description: None,
            thumbnail: None,
        }]
    } else {
        vec![]
    };

    FeedPost {
        id: row.id.to_string(),
        source: row.source.clone(),
        author: effective_author,
        content: extract_content_text(&row.content),
        content_type: row.content_type.clone(),
        media,
        embeds,
        likes_count: row.likes_count as u32,
        shares_count: row.shares_count as u32,
        replies_count: row.replies_count as u32,
        created_at: if row.is_imported { row.original_ts.to_rfc3339() } else { row.created_at.to_rfc3339() },
        url: row.source_url.clone(),
        thread_id: row.source_url.clone(),
        start_time: row.start_time.map(|t| t.to_rfc3339()),
        end_time: row.end_time.map(|t| t.to_rfc3339()),
        location: row.location.clone(),
        display_name,
        action_text,
        group_name,
        group: None,
        upvotes: None,
        downvotes: None,
        my_vote: None,
        reaction_counts: None,
        user_reaction: None,
        cid: None,
        author_url: None,
        reposted_by: None,
        boosted_by: None,
        quoted_post_uri: None,
        is_reshare: Some(row.reshared_post_id.is_some()),
        original_post: original_post.map(Box::new),
        is_own: None,
        is_following: None,
        follow_pending: None,
        cross_posts: vec![],
    matched_sources: vec![],
    labels: vec![],
    nsfw: false,
        author_external_id: None,
    }
}

async fn build_profile_response(
    pool: &sqlx::PgPool,
    user: &db::users::UserRow,
    viewer_id: Option<Uuid>,
    domain: &str,
) -> Result<ProfileResponse, AppError> {
    let local_posts_count = db::posts::PostRow::public_feed_count(pool, user.id, None).await?;

    // Native-graph follows only. Imported rows (follow_type='bluesky_import')
    // mirror the federated Bluesky graph and are counted by the linked
    // account's stats below — counting both double-counted the same people.
    let native_following: i32 = match sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM follows WHERE follower_id = $1 AND follow_type = 'native'"
    )
    .bind(user.id)
    .fetch_one(pool)
    .await
    {
        Ok(n) => n as i32,
        Err(e) => {
            tracing::error!("[PROFILE] user={} id={} native_following query error: {}", user.username, user.id, e);
            0
        }
    };

    let native_followers: i32 = match sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM follows WHERE followed_actor LIKE '%' || $1 || '%' AND follow_type = 'native'"
    )
    .bind(&user.username)
    .fetch_one(pool)
    .await
    {
        Ok(n) => n as i32,
        Err(e) => {
            tracing::error!("[PROFILE] user={} id={} native_followers query error: {}", user.username, user.id, e);
            0
        }
    };

    // Inbound AP followers (the `followers` table) — the count used to ignore
    // them entirely (identity-graph Phase 3: counts include inbound
    // followers).
    let inbound_ap_followers: i32 = match sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM followers WHERE user_id = $1 AND accepted"
    )
    .bind(user.id)
    .fetch_one(pool)
    .await
    {
        Ok(n) => n as i32,
        Err(e) => {
            tracing::error!("[PROFILE] user={} id={} inbound followers query error: {}", user.username, user.id, e);
            0
        }
    };

    // Federated stats: a Bluesky-linked account's real followers/following/
    // posts live on the network, not in local tables. When the local counts
    // are empty the profile showed 0/1/0 — aggregate the linked account's
    // numbers in (cached in linked_accounts.metadata, refreshed live from
    // the public AppView when missing or stale).
    let linked = linked_bluesky_stats(pool, user.id).await;
    // Posts are additive (local imports/composer posts are Bytescape-only);
    // followers/following add only the NATIVE graph on top of the federated
    // number — imported rows are already inside it.
    let posts_count = local_posts_count + linked.posts as i64;
    let followers_count = native_followers + linked.followers + inbound_ap_followers;
    let following_count = native_following + linked.following;

    let is_own_profile = viewer_id.map(|v| v == user.id).unwrap_or(false);

    let (is_following, follow_pending) = if let Some(vid) = viewer_id {
        let actor = format!("https://{}/@{}", domain, user.username);
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM follows WHERE follower_id = $1 AND followed_actor = $2",
        )
        .bind(vid)
        .bind(&actor)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
        (status.is_some(), status.as_deref() == Some("pending"))
    } else {
        (false, false)
    };

    let widget_layout: Vec<WidgetConfig> = serde_json::from_value(user.widget_layout.clone())
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to parse widget_layout for {}: {}", user.username, e);
            vec![]
        });
    let widget_themes: Vec<WidgetTheme> = serde_json::from_value(user.widget_themes.clone())
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to parse widget_themes for {}: {}", user.username, e);
            vec![]
        });

    let custom_profile_fields: Vec<CustomProfileField> =
        serde_json::from_value(user.custom_profile_fields.clone())
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to parse custom_profile_fields for {}: {}", user.username, e);
            vec![]
        });

    Ok(ProfileResponse {
        id: user.id.to_string(),
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        bio: user.bio.clone(),
        avatar_url: user.avatar_url.clone(),
        header_url: user.header_url.clone(),
        theme: user.theme.clone(),
        location: user.location.clone(),
        website: user.website.clone(),
        pronouns: user.pronouns.clone(),
        relationship_status: user.relationship_status.clone(),
        birthday: user.birthday.map(|d| d.to_string()),
        occupation: user.occupation.clone(),
        interests: user.interests.clone(),
        onboarded: user.onboarded,
        nsfw_pref: user.nsfw_pref.clone(),
        widget_layout,
        widget_themes,
        custom_profile_fields,
        created_at: user.created_at.to_rfc3339(),
        posts_count,
        followers_count,
        following_count,
        visitor_count: user.visitor_count,
        is_own_profile,
        is_following,
        follow_pending,
    })
}

/// Federated Bluesky counts for a user's linked account(s).
///
/// Served from `linked_accounts.metadata` (`followers` / `following` /
/// `posts_count`, written at link time and refreshed here) — refreshed live
/// from the public Bluesky AppView when absent or older than
/// `STATS_TTL_SECS`. Network failure degrades to the cached values.
#[derive(Debug, Default, Clone, Copy)]
struct LinkedStats {
    followers: i32,
    following: i32,
    posts: i32,
}

const STATS_TTL_SECS: i64 = 3600;

async fn linked_bluesky_stats(pool: &sqlx::PgPool, user_id: Uuid) -> LinkedStats {
    let accounts = match crate::db::accounts::LinkedAccountRow::find_by_user(pool, user_id).await {
        Ok(a) => a,
        Err(e) => {
            tracing::warn!("[PROFILE] linked-account lookup failed for {}: {}", user_id, e);
            return LinkedStats::default();
        }
    };

    let mut total = LinkedStats::default();
    for acct in accounts.iter().filter(|a| a.provider == "bluesky") {
        let fresh = chrono::DateTime::parse_from_rfc3339(
            acct.metadata["stats_cached_at"].as_str().unwrap_or(""),
        )
        .map(|t| chrono::Utc::now().timestamp() - t.timestamp() < STATS_TTL_SECS)
        .unwrap_or(false);
        let has_all = ["followers", "following", "posts_count"]
            .iter()
            .all(|k| acct.metadata[*k].as_i64().is_some());

        let stats = if has_all && fresh {
            stats_from_metadata(&acct.metadata)
        } else if let Ok(live) = fetch_public_bsky_stats(&acct.external_id).await {
            let stats = LinkedStats {
                followers: live.0,
                following: live.1,
                posts: live.2,
            };
            persist_linked_stats(pool, user_id, &acct.external_id, &stats).await;
            stats
        } else {
            tracing::warn!(
                "[PROFILE] live Bluesky stats unavailable for {} — using cache",
                acct.external_username
            );
            stats_from_metadata(&acct.metadata)
        };

        total.followers += stats.followers;
        total.following += stats.following;
        total.posts += stats.posts;
    }
    total
}

fn stats_from_metadata(metadata: &serde_json::Value) -> LinkedStats {
    let get = |k: &str| metadata[k].as_i64().unwrap_or(0) as i32;
    LinkedStats {
        followers: get("followers"),
        following: get("following"),
        posts: get("posts_count"),
    }
}

/// Public AppView graph read (no auth): the Bluesky graph of any account is
/// public protocol data, so this is the fallback when the stored credential
/// cannot read the PDS (stale DPoP token, scope drift) — source (c) must
/// never silently disappear from a merged list.
async fn bsky_graph_public(
    did: &str,
    followers: bool,
) -> Result<Vec<(String, String, String, Option<String>)>, String> {
    let endpoint = if followers { "getFollowers" } else { "getFollows" };
    let url = format!(
        "https://public.api.bsky.app/xrpc/app.bsky.graph.{endpoint}?actor={}&limit=100",
        urlencoding::encode(did)
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("{endpoint}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("{endpoint}: HTTP {}", resp.status()));
    }
    let doc: serde_json::Value = resp.json().await.map_err(|e| format!("{endpoint}: {e}"))?;
    let key = if followers { "followers" } else { "follows" };
    let items = doc[key]
        .as_array()
        .ok_or_else(|| format!("{endpoint}: no {key} list"))?;
    Ok(items
        .iter()
        .filter_map(|it| {
            let did = it["did"].as_str()?.to_string();
            let handle = it["handle"].as_str()?.to_string();
            let display = it["displayName"].as_str().unwrap_or("").to_string();
            let avatar = it["avatar"].as_str().map(|s| s.to_string());
            Some((did, handle, display, avatar))
        })
        .collect())
}

/// Fetch (followers, following, posts) from the public Bluesky AppView —
/// no auth needed, works even when the stored DPoP token is stale.
async fn fetch_public_bsky_stats(did: &str) -> Result<(i32, i32, i32), String> {
    let url = format!(
        "https://api.bsky.app/xrpc/app.bsky.actor.getProfile?actor={}",
        urlencoding::encode(did)
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("read failed: {}", e))?;
    if !status.is_success() {
        return Err(format!("getProfile returned {}: {}", status.as_u16(), text));
    }
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("bad JSON: {}", e))?;
    let n = |k: &str| json[k].as_i64().unwrap_or(0) as i32;
    Ok((n("followersCount"), n("followsCount"), n("postsCount")))
}

/// Write refreshed stats back into linked_accounts.metadata so the next
/// profile view is served from cache.
async fn persist_linked_stats(pool: &sqlx::PgPool, user_id: Uuid, did: &str, stats: &LinkedStats) {
    let patch = serde_json::json!({
        "followers": stats.followers,
        "following": stats.following,
        "posts_count": stats.posts,
        "stats_cached_at": chrono::Utc::now().to_rfc3339(),
    });
    let _ = sqlx::query(
        "UPDATE linked_accounts
         SET metadata = metadata || $3,
             updated_at = now()
         WHERE user_id = $1 AND provider = 'bluesky' AND external_id = $2",
    )
    .bind(user_id)
    .bind(did)
    .bind(&patch)
    .execute(pool)
    .await
    .map_err(|e| tracing::warn!("[PROFILE] persisting linked stats failed: {}", e));
}

fn build_actor_url(domain: &str, username: &str) -> String {
    format!("https://{}/@{}", domain, username)
}

fn parse_local_username_from_actor_url(domain: &str, actor_url: &str) -> Option<String> {
    let prefix = format!("https://{}/@", domain);
    actor_url.strip_prefix(&prefix).map(|s| s.to_string())
}

pub(crate) async fn handle_image_upload(
    state: &AppState,
    multipart: &mut Multipart,
    folder: &str,
    max_bytes: usize,
) -> Result<String, AppError> {
    let mut file_bytes: Vec<u8> = Vec::new();
    let mut filename: Option<String> = None;
    let mut content_type: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Multipart error: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            filename = field.file_name().map(|s| s.to_string());
            content_type = field.content_type().map(|s| s.to_string());
            file_bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("Read error: {}", e)))?
                .to_vec();
            break;
        }
    }

    if file_bytes.is_empty() {
        return Err(AppError::BadRequest("No file provided".into()));
    }

    if file_bytes.len() > max_bytes {
        return Err(AppError::BadRequest(format!(
            "File too large: {} bytes (max {})",
            file_bytes.len(),
            max_bytes
        )));
    }

    let valid_types = [
        "image/jpeg",
        "image/png",
        "image/gif",
        "image/webp",
    ];
    let detected_mime = content_type
        .as_deref()
        .or_else(|| {
            filename
                .as_deref()
                .and_then(|f| mime_guess::from_path(f).first_raw())
        })
        .unwrap_or("application/octet-stream");

    if !valid_types.contains(&detected_mime) {
        return Err(AppError::BadRequest(format!(
            "Invalid content type: {}. Allowed: jpeg, png, gif, webp",
            detected_mime
        )));
    }

    let ext = detected_mime
        .strip_prefix("image/")
        .unwrap_or("jpg")
        .replace("jpeg", "jpg");
    let object_key = format!("{}/{}/{}.{}", folder, state.config.domain, Uuid::new_v4(), ext);

    if !state.config.s3_enabled {
        let file_path = format!("./uploads/{}", object_key);
        if let Some(parent) = std::path::Path::new(&file_path).parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&file_path, &file_bytes)
            .map_err(|e| AppError::Internal(format!("Failed to save file: {}", e)))?;
        let url = if state.config.s3_public_base_url.is_empty() {
            format!("/uploads/{}", object_key)
        } else {
            format!("{}/{}", state.config.s3_public_base_url, object_key)
        };
        Ok(url)
    } else {
        let credentials = aws_sdk_s3::config::Credentials::new(
            &state.config.s3_access_key,
            &state.config.s3_secret_key,
            None,
            None,
            "static",
        );
        let s3_config = aws_sdk_s3::Config::builder()
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .endpoint_url(&state.config.s3_endpoint)
            .region(aws_sdk_s3::config::Region::new("us-east-1"))
            .credentials_provider(credentials)
            .force_path_style(true)
            .build();
        let client = aws_sdk_s3::Client::from_conf(s3_config);

        let body = aws_sdk_s3::primitives::ByteStream::from(file_bytes);
        client
            .put_object()
            .bucket(&state.config.s3_bucket)
            .key(&object_key)
            .body(body)
            .content_type(detected_mime)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("S3 upload failed: {}", e)))?;

        let base = if state.config.s3_public_base_url.is_empty() {
            state.config.s3_endpoint.trim_end_matches('/').to_string()
        } else {
            state.config.s3_public_base_url.trim_end_matches('/').to_string()
        };
        Ok(format!("{}/{}/{}", base, state.config.s3_bucket, object_key))
    }
}

async fn get_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ProfileResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let user = db::users::UserRow::find_by_id(&state.db, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    // Visitor counter: the own profile (ProfileScreen loads via /api/profile/me)
    // is also a page-view — increment it here too, matching public_profile, so
    // the owner's Visitors stat counts as they reload their own page (Bug 4).
    sqlx::query("UPDATE users SET visitor_count = visitor_count + 1 WHERE id = $1")
        .bind(user.id)
        .execute(&state.db)
        .await?;

    let resp = build_profile_response(&state.db, &user, Some(user_id), &state.config.domain).await?;
    Ok(Json(resp))
}
async fn update_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<UpdateProfileRequest>,
) -> Result<Json<ProfileResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let current = db::users::UserRow::find_by_id(&state.db, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    // Clean cutover: the boolean `show_nsfw` is gone; `nsfw_pref` is a strict
    // three-state string. Reject unknown values rather than coerce them.
    if let Some(pref) = body.nsfw_pref.as_deref() {
        if crate::db::users::NsfwPref::parse(pref).is_none() {
            return Err(AppError::BadRequest(format!(
                "Invalid nsfw_pref '{}'. Must be 'exclude', 'blur', or 'show'",
                pref
            )));
        }
    }

    let display_name = body.display_name.unwrap_or(current.display_name);
    let bio = body.bio.unwrap_or(current.bio);
    let theme = body.theme.unwrap_or(current.theme);
    let custom_css = body.custom_css.unwrap_or(current.custom_css);

    let birthday = body.birthday.as_ref().and_then(|bd| {
        chrono::NaiveDate::parse_from_str(bd, "%Y-%m-%d").ok()
    });

    let widget_layout = body.widget_layout
        .as_ref()
        .map(|v| serde_json::to_value(v).unwrap_or(serde_json::Value::Array(vec![])));
    let widget_themes = body.widget_themes
        .as_ref()
        .map(|v| serde_json::to_value(v).unwrap_or(serde_json::Value::Array(vec![])));
    let custom_profile_fields = body.custom_profile_fields
        .as_ref()
        .map(|v| serde_json::to_value(v).unwrap_or(serde_json::Value::Array(vec![])));

    let updated = db::users::UserRow::update_profile(
        &state.db,
        user_id,
        &display_name,
        &bio,
        &theme,
        &custom_css,
        body.avatar_url.as_deref(),
        body.header_url.as_deref(),
        body.location.as_deref(),
        body.website.as_deref(),
        body.pronouns.as_deref(),
        body.relationship_status.as_deref(),
        birthday,
        body.occupation.as_deref(),
        body.interests.as_deref(),
        widget_layout.as_ref(),
        widget_themes.as_ref(),
        custom_profile_fields.as_ref(),
        body.nsfw_pref.as_deref(),
    )
    .await?;

    let resp = build_profile_response(&state.db, &updated, Some(user_id), &state.config.domain).await?;
    Ok(Json(resp))
}

async fn public_profile(
    State(state): State<AppState>,
    Path(username): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ProfileResponse>, AppError> {
    let user = match db::users::UserRow::find_by_username(&state.db, &username).await? {
        Some(u) => u,
        None => return remote_public_profile(&state, &username, headers).await,
    };

    // Visitor counter: every page load counts — owner included. It is a
    // "times this page has been loaded" hit counter, not unique-visitor
    // analytics, so own views increment too.
    sqlx::query("UPDATE users SET visitor_count = visitor_count + 1 WHERE id = $1")
        .bind(user.id)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;
    let user = db::users::UserRow::find_by_id(&state.db, user.id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("User {} not found", username)))?;

    let viewer_id = crate::api::feed::optional_viewer(&headers, &state.config.jwt_secret)?;
    let resp = build_profile_response(&state.db, &user, viewer_id, &state.config.domain).await?;
    Ok(Json(resp))
}

/// Resolve a user's imported remote posts through the linked-identity map.
///
/// Match complete handles and actor URLs rather than prefix-filtering the
/// multi-million-row appview index. Mastodon links carry `metadata.instance`,
/// which lets us reconstruct the exact persisted forms for both handle and DID.
pub(crate) async fn identity_appview_posts(
    db: &sqlx::PgPool,
    user_id: Uuid,
    local_username: &str,
) -> Result<Vec<FeedPost>, AppError> {
    let mut identities: std::collections::HashSet<String> = std::collections::HashSet::new();
    let accounts = crate::db::accounts::LinkedAccountRow::find_by_user(db, user_id)
        .await
        .map_err(AppError::Database)?;
    for account in accounts {
        let instance = account
            .metadata
            .get("instance")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .trim_start_matches('@');
        for raw in [&account.external_username, &account.external_id] {
            let identity = raw.trim().trim_start_matches('@');
            if identity.is_empty() {
                continue;
            }
            identities.insert(identity.to_string());
            let username = identity.split('@').next().unwrap_or(identity);
            if !instance.is_empty() && !username.is_empty() {
                identities.insert(format!("{username}@{instance}"));
                identities.insert(format!("@{username}@{instance}"));
                identities.insert(format!("https://{instance}/users/{username}"));
                identities.insert(format!("https://{instance}/@{username}"));
            }
        }
    }
    identities.insert(local_username.trim().trim_start_matches('@').to_string());
    let idents: Vec<String> = identities.into_iter().collect();

    // Both equality predicates have btree indexes. The former OR/EXISTS LIKE
    // query degenerated into a created_at scan and hit the statement timeout.
    let mut tx = db.begin().await.map_err(AppError::Database)?;
    sqlx::query("SET LOCAL statement_timeout = '6s'")
        .execute(&mut *tx)
        .await
        .map_err(AppError::Database)?;
    let rows: Vec<(String, String, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<chrono::DateTime<chrono::Utc>>, Option<serde_json::Value>, Option<serde_json::Value>, Option<i32>, Option<i32>, Option<i32>)> =
        sqlx::query_as(
            r#"SELECT uri, cid, author_did, author_handle, author_avatar, author_display_name, text, created_at, embeds, labels, like_count, repost_count, reply_count
               FROM appview_posts
               WHERE (author_handle = ANY($1) OR author_did = ANY($1))
                 AND reply_parent IS NULL
               ORDER BY created_at DESC
               LIMIT 100"#,
        )
        .bind(&idents)
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::Database)?;

    let mut appview: Vec<FeedPost> = Vec::new();
    for (uri, cid, did, author_handle, avatar, display_name, text, created, embeds, labels, likes, reposts, replies) in rows {
        if let Some(p) = crate::api::feed::normalize::normalize_appview_post(
            uri, cid, did, author_handle, avatar, display_name, text, created, embeds,
            crate::api::feed::normalize::labels_from_json(&labels),
            likes.unwrap_or(0), reposts.unwrap_or(0), replies.unwrap_or(0),
        ) {
            appview.push(p);
        }
    }
    Ok(appview)
}

async fn public_feed(
    State(state): State<AppState>,
    Path(username): Path<String>,
    Query(query): Query<FeedQuery>,
    headers: HeaderMap,
) -> Result<Json<FeedResponse>, AppError> {
    let user = match db::users::UserRow::find_by_username(&state.db, &username).await? {
        Some(u) => u,
        None => return remote_public_feed(&state, &username, &query).await,
    };

    let viewer_id = crate::api::feed::optional_viewer(&headers, &state.config.jwt_secret)?;
    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.offset.unwrap_or(0);

    // Parse optional date filters
    let date_from = query.date_from.as_deref().and_then(|d| {
        chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .ok()
            .and_then(|nd| nd.and_hms_opt(0, 0, 0).map(|t| t.and_utc()))
    });
    let date_to = query.date_to.as_deref().and_then(|d| {
        chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .ok()
            .and_then(|nd| nd.and_hms_opt(23, 59, 59).map(|t| t.and_utc()))
    });
    let source_filter = query.source.as_deref();

    let mut all_posts: Vec<FeedPost> = Vec::new();

    // 1. Local posts
    let author_info = AuthorInfo {
        handle: format!("@{}", user.username),
        display_name: Some(user.display_name.clone()),
        avatar: user.avatar_url.clone(),
    };
    let fetch_limit = (limit + offset).min(100);
    let local_posts = db::posts::PostRow::public_feed(&state.db, user.id, fetch_limit, 0, source_filter, date_from, date_to).await?;
    for p in &local_posts {
        let original = if let Some(orig_id) = p.reshared_post_id {
            if let Some(orig_row) = db::posts::PostRow::find_by_id(&state.db, orig_id).await? {
                let orig_author = db::users::UserRow::find_by_id(&state.db, orig_row.user_id).await?;
                let orig_author_info = orig_author.as_ref().map(|u| AuthorInfo {
                    handle: format!("@{}", u.username),
                    display_name: Some(u.display_name.clone()),
                    avatar: u.avatar_url.clone(),
                }).unwrap_or_else(|| author_info.clone());
                Some(post_row_to_feed_post(&orig_row, &orig_author_info, None))
            } else { None }
        } else { None };
        all_posts.push(post_row_to_feed_post(p, &author_info, original));
    }

    // 2. External posts from linked accounts
    let accounts = crate::db::accounts::LinkedAccountRow::find_by_user(&state.db, user.id)
        .await
        .map_err(AppError::Database)?;

    let http = reqwest::Client::new();

    for acct in &accounts {
        match acct.provider.as_str() {
            "bluesky" => {
                let instance = crate::oauth::bluesky::get_pds_url(&acct.metadata);
                let mut bsky_posts: Vec<FeedPost> = Vec::new();

                // Try DPoP auth first
                if let Some(dpop_hex) = &acct.dpop_private_key
                    && !dpop_hex.is_empty()
                        && let Ok(dpop_bytes) = hex::decode(dpop_hex)
                            && let Ok(client) = crate::oauth::bluesky::BlueskyApiClient::new(
                                &acct.access_token, &dpop_bytes, &instance,
                            )
                                && let Ok(result) = client.get_author_feed(&acct.external_id, fetch_limit as u32).await
                                    && let Some(feed) = result["feed"].as_array() {
                                        for item in feed {
                                            if let Some(post) = normalize_bluesky_post(item) {
                                                bsky_posts.push(post);
                                            }
                                        }
                                    }

                // Fallback to Bearer auth
                if bsky_posts.is_empty() {
                    let url = format!(
                        "{}/xrpc/app.bsky.feed.getAuthorFeed?actor={}&limit={}",
                        instance, acct.external_id, fetch_limit
                    );
                    if let Ok(resp) = http.get(&url).header("Authorization", format!("Bearer {}", acct.access_token)).send().await
                        && let Ok(text) = resp.text().await
                            && let Ok(result) = serde_json::from_str::<serde_json::Value>(&text)
                                && let Some(feed) = result["feed"].as_array() {
                                    for item in feed {
                                        if let Some(post) = normalize_bluesky_post(item) {
                                            bsky_posts.push(post);
                                        }
                                    }
                                }
                }

                // Final fallback: the public AppView. Stored OAuth/DPoP
                // tokens go stale (expired access token, rotated key) and
                // the two paths above silently swallowed those errors,
                // leaving the profile feed empty. Public author feeds need
                // no auth, so federated posts still render.
                if bsky_posts.is_empty() {
                    match crate::oauth::bluesky::BlueskyPublicApi::get_author_feed(
                        &acct.external_id,
                        fetch_limit as u32,
                    )
                    .await
                    {
                        Ok(result) => {
                            if let Some(feed) = result["feed"].as_array() {
                                for item in feed {
                                    if let Some(post) = normalize_bluesky_post(item) {
                                        bsky_posts.push(post);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "[PROFILE] bluesky author feed unavailable for {} (dpop+bearer failed, public fallback failed): {}",
                                acct.external_username,
                                e
                            );
                        }
                    }
                }

                // Filter by source and date range
                if source_filter.map(|s| s != "bluesky").unwrap_or(false) {
                    bsky_posts.clear();
                }
                if date_from.is_some() || date_to.is_some() {
                    bsky_posts.retain(|p| {
                        let created = p.created_at.parse::<chrono::DateTime<chrono::Utc>>();
                        match created {
                            Ok(ts) => {
                                let mut keep = true;
                                if let Some(df) = date_from && ts < df { keep = false; }
                                if let Some(dt) = date_to && ts > dt { keep = false; }
                                keep
                            }
                            Err(_) => true,
                        }
                    });
                }
                all_posts.extend(bsky_posts);
            }
            _ => {}
        }
    }

    // 3. Imported remote posts from the appview index, resolved through the
    // linked-identity map (design doc: the local username is a label; the
    // linked remote actors are the identity).
    all_posts.extend(identity_appview_posts(&state.db, user.id, &user.username).await?);

    // 4. Deduplicate by post id
    let mut seen_ids = HashSet::new();
    all_posts.retain(|p| seen_ids.insert(p.id.clone()));

    // 5. Sort by created_at descending
    all_posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // 5. Set is_own for all posts on the user's profile
    let is_own_profile = viewer_id.map(|v| v == user.id).unwrap_or(false);
    if is_own_profile {
        for post in &mut all_posts {
            post.is_own = Some(true);
        }
    } else {
        // For external platform posts on someone else's profile, check if they're from linked accounts
        let viewer_accounts = if let Some(vid) = viewer_id {
            crate::db::accounts::LinkedAccountRow::find_by_user(&state.db, vid).await.unwrap_or_default()
        } else {
            vec![]
        };
        for post in &mut all_posts {
            for acct in &viewer_accounts {
                let provider = &acct.provider;
                if post.source.starts_with(provider) {
                    let post_handle = post.author.handle.trim_start_matches('@').to_lowercase();
                    let external_handle = acct.external_id.trim_start_matches('@').to_lowercase();
                    if post_handle == external_handle || post_handle.starts_with(&format!("{}@", external_handle)) {
                        post.is_own = Some(true);
                    }
                    let ext_user = acct.external_username.trim_start_matches('@').to_lowercase();
                    if post_handle == ext_user || post_handle.starts_with(&format!("{}@", ext_user)) {
                        post.is_own = Some(true);
                    }
                }
            }
        }
    }

    // 6. In-memory pagination
    let total = all_posts.len() as i64;
    let items: Vec<FeedPost> = all_posts.into_iter().skip(offset as usize).take(limit as usize).collect();
    let returned = items.len() as i64;
    let has_more = offset + returned < total;

    Ok(Json(FeedResponse {
        items,
        cursor: None,
        has_more,
        phase: None,
        platform_counts: None,
    }))
}

/// Remote (Bluesky network) profile: the username is not a local account —
/// resolve it through the public AppView so profile links for federated
/// authors work instead of 404ing.
async fn remote_public_profile(
    state: &AppState,
    username: &str,
    headers: HeaderMap,
) -> Result<Json<ProfileResponse>, AppError> {
    let url = format!(
        "https://api.bsky.app/xrpc/app.bsky.actor.getProfile?actor={}",
        urlencoding::encode(username)
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("profile lookup failed: {}", e)))?;
    if !resp.status().is_success() {
        return Err(AppError::NotFound(format!("User {} not found", username)));
    }
    let p: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("bad profile JSON: {}", e)))?;
    let handle = p["handle"].as_str().unwrap_or(username).to_string();

    // Follow state follows the same actor-URL convention as the follows
    // table (https://bsky.app/profile/<handle>).
    let mut is_following = false;
    if let Some(vid) = crate::api::feed::optional_viewer(&headers, &state.config.jwt_secret)? {
        let actor = format!("https://bsky.app/profile/{}", handle);
        is_following =
            crate::db::follows::FollowRow::find_by_follower_and_actor(&state.db, vid, &actor)
                .await
                .map_err(AppError::Database)?
                .is_some();
    }

    Ok(Json(ProfileResponse {
        id: p["did"].as_str().unwrap_or_default().to_string(),
        username: handle,
        display_name: p["displayName"].as_str().unwrap_or(username).to_string(),
        bio: p["description"].as_str().unwrap_or("").to_string(),
        avatar_url: p["avatar"].as_str().map(|s| s.to_string()),
        header_url: p["banner"].as_str().map(|s| s.to_string()),
        theme: "default".into(),
        location: None,
        website: None,
        pronouns: None,
        relationship_status: None,
        birthday: None,
        occupation: None,
        interests: vec![],
        onboarded: false,
        nsfw_pref: "exclude".into(),
        widget_layout: vec![],
        widget_themes: vec![],
        custom_profile_fields: vec![],
        created_at: p["indexedAt"].as_str().unwrap_or("").to_string(),
        posts_count: p["postsCount"].as_i64().unwrap_or(0),
        followers_count: p["followersCount"].as_i64().unwrap_or(0) as i32,
        following_count: p["followsCount"].as_i64().unwrap_or(0) as i32,
        visitor_count: 0,
        is_own_profile: false,
        is_following,
        follow_pending: false,
    }))
}

/// Remote profile feed: serve the author's public AppView feed. Reuses the
/// same Bluesky item normalizer as the linked-account path.
async fn remote_public_feed(
    _state: &AppState,
    username: &str,
    query: &FeedQuery,
) -> Result<Json<FeedResponse>, AppError> {
    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.offset.unwrap_or(0);
    let result = crate::oauth::bluesky::BlueskyPublicApi::get_author_feed(
        username,
        (limit + offset).min(100) as u32,
    )
    .await
    .map_err(|e| AppError::Internal(format!("author feed unavailable: {}", e)))?;

    let mut posts: Vec<FeedPost> = Vec::new();
    if let Some(feed) = result["feed"].as_array() {
        for item in feed {
            if let Some(post) = normalize_bluesky_post(item) {
                posts.push(post);
            }
        }
    }

    if query.source.as_deref().map(|s| s != "bluesky").unwrap_or(false) {
        posts.clear();
    }
    let date_from = query.date_from.as_deref().and_then(|d| {
        chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .ok()
            .and_then(|nd| nd.and_hms_opt(0, 0, 0).map(|t| t.and_utc()))
    });
    let date_to = query.date_to.as_deref().and_then(|d| {
        chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .ok()
            .and_then(|nd| nd.and_hms_opt(23, 59, 59).map(|t| t.and_utc()))
    });
    if date_from.is_some() || date_to.is_some() {
        posts.retain(|p| {
            p.created_at
                .parse::<chrono::DateTime<chrono::Utc>>()
                .map(|ts| {
                    date_from.map(|df| ts >= df).unwrap_or(true)
                        && date_to.map(|dt| ts <= dt).unwrap_or(true)
                })
                .unwrap_or(true)
        });
    }

    let total = posts.len() as i64;
    let items: Vec<FeedPost> = posts.into_iter().skip(offset as usize).take(limit as usize).collect();
    let has_more = (offset + items.len() as i64) < total;
    Ok(Json(FeedResponse {
        items,
        cursor: None,
        has_more,
        phase: None,
        platform_counts: None,
    }))
}

async fn upload_avatar(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let url = handle_image_upload(&state, &mut multipart, "avatars", 5 * 1024 * 1024).await?;
    db::users::UserRow::set_avatar(&state.db, user_id, &url).await?;
    Ok(Json(serde_json::json!({ "url": url })))
}

async fn upload_header(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let url = handle_image_upload(&state, &mut multipart, "headers", 10 * 1024 * 1024).await?;
    db::users::UserRow::set_header_url(&state.db, user_id, &url).await?;
    Ok(Json(serde_json::json!({ "url": url })))
}

async fn list_wall(
    State(state): State<AppState>,
    Path(username): Path<String>,
    Query(query): Query<WallQuery>,
) -> Result<Json<Vec<WallPostView>>, AppError> {
    let user = db::users::UserRow::find_by_username(&state.db, &username)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("User {} not found", username)))?;

    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    let posts = db::wall::WallPostRow::list_by_profile(&state.db, user.id, limit, offset).await?;

    let mut views = Vec::with_capacity(posts.len());
    for post in &posts {
        let author = db::users::UserRow::find_by_id(&state.db, post.author_user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Wall post author not found".into()))?;

        views.push(WallPostView {
            id: post.id.to_string(),
            author: AuthorInfo {
                handle: format!("@{}", author.username),
                display_name: Some(author.display_name.clone()),
                avatar: author.avatar_url.clone(),
            },
            content: post.content.clone(),
            created_at: post.created_at.to_rfc3339(),
        });
    }

    Ok(Json(views))
}

async fn post_wall(
    State(state): State<AppState>,
    Path(username): Path<String>,
    headers: HeaderMap,
    Json(body): Json<PostWallRequest>,
) -> Result<Json<WallPostView>, AppError> {
    let viewer_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let profile_user = db::users::UserRow::find_by_username(&state.db, &username)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("User {} not found", username)))?;

    let is_mutual = if viewer_id == profile_user.id {
        true
    } else {
        let viewer_user = db::users::UserRow::find_by_id(&state.db, viewer_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Viewer not found".into()))?;

        let viewer_actor_url = build_actor_url(&state.config.domain, &viewer_user.username);
        let profile_actor_url = build_actor_url(&state.config.domain, &profile_user.username);

        let viewer_follows_profile = db::follows::FollowRow::find_by_follower_and_actor(
            &state.db, viewer_id, &profile_actor_url,
        )
        .await?
        .is_some();

        let profile_follows_viewer = db::follows::FollowRow::find_by_follower_and_actor(
            &state.db, profile_user.id, &viewer_actor_url,
        )
        .await?
        .is_some();

        viewer_follows_profile && profile_follows_viewer
    };

    if !is_mutual {
        return Err(AppError::Forbidden("Only mutual follows can post on this wall".into()));
    }

    let post = db::wall::WallPostRow::create(
        &state.db,
        profile_user.id,
        viewer_id,
        &body.content,
    )
    .await?;

    let author = db::users::UserRow::find_by_id(&state.db, viewer_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Viewer not found".into()))?;

    // Notify the profile owner of the wall post (spec: profile_comment).
    if profile_user.id != viewer_id {
        let _ = crate::ap::notify::notify(
            &state.db,
            profile_user.id,
            crate::ap::notify::TYPE_COMMENT,
            "New wall post",
            Some(&format!("@{} posted on your wall", author.username)),
            Some(serde_json::json!({ "wall_post_id": post.id.to_string() })),
            &format!("wall-post:{}", post.id),
            &crate::ap::notify::ActorRef::local(&author.username),
        )
        .await;
    }

    Ok(Json(WallPostView {
        id: post.id.to_string(),
        author: AuthorInfo {
            handle: format!("@{}", author.username),
            display_name: Some(author.display_name.clone()),
            avatar: author.avatar_url.clone(),
        },
        content: post.content.clone(),
        created_at: post.created_at.to_rfc3339(),
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Merged graph read (identity-graph Phase 3) — one row shape over every
// source of a person's follow graph:
//   (a) local `follows` rows (the Bytescape-side graph, all networks),
//   (b) inbound AP `followers` (+ `ap_remote_actors` display join),
//   (c) the live Bluesky graph of the linked account,
//   (d) the home-instance graph of a Mastodon-linked account (home-REST
//       proxy — FAIL-OPEN: proxy failure degrades to the stored rows, it
//       never errors the endpoint).
// Rows carry `network` + `identity_used` provenance; duplicates (mirror rows
// vs live graphs) collapse on one normalized identity key. The same person
// on two UNLINKED networks stays two rows with network badges (we cannot
// know they are one human — design.md Risks).
// ─────────────────────────────────────────────────────────────────────────────

/// Normalize one identity string to its dedupe key: local usernames, bsky
/// handles, and AP actor URLs (any alias form) collapse to `user@host` /
/// bare-handle shape so a canonical actor row and a home-instance acct row
/// for the same actor merge.
fn graph_identity_key(identity: &str) -> String {
    let s = identity.trim().trim_start_matches('@').to_lowercase();
    if let Some(rest) = s.strip_prefix("https://bsky.app/profile/") {
        return rest.trim_end_matches('/').to_string();
    }
    if s.starts_with("http") {
        if let Ok(u) = url::Url::parse(&s) {
            let host = u.host_str().unwrap_or("").to_string();
            let name = u
                .path()
                .rsplit('/')
                .next()
                .unwrap_or("")
                .trim_start_matches('@')
                .to_string();
            if !host.is_empty() && !name.is_empty() {
                return format!("{name}@{host}");
            }
        }
        return s;
    }
    s
}

fn follow_type_network(follow_type: &str) -> &'static str {
    match follow_type {
        "native" => "local",
        "bluesky_import" => "atproto",
        _ => "activitypub", // ap | mastodon_import | home_rest
    }
}

/// The merged graph response: rows plus the visibility disclosures the UI
/// surfaces (identity-graph Phase 6).
#[derive(Debug, Serialize)]
struct MergedGraphResponse {
    rows: Vec<FollowingView>,
    /// The profile's follow lists are hidden (`graph_visibility=private` and
    /// the viewer is not the owner) — rows withheld entirely.
    hidden: bool,
    /// The home instance hides its collections (`hide_collections`): the
    /// home source degraded to stored rows and the UI says so.
    home_graph_hidden: bool,
    /// The managing home instance for a Mastodon-linked account.
    home_instance: Option<String>,
}

/// Merge every graph source for `user`'s followers or following. `is_own`
/// enables the remove-follower affordance.
async fn merged_graph_rows(
    state: &AppState,
    user: &db::users::UserRow,
    viewer_id: Option<Uuid>,
    is_own: bool,
    following_side: bool,
) -> Result<MergedGraphResponse, AppError> {
    // Bytescape-side enforcement (the graph_visibility toggle): a private
    // graph serves nothing to anyone but its owner — the same rule the AP
    // collections enforce.
    if !is_own && user.graph_visibility != "public" {
        return Ok(MergedGraphResponse {
            rows: vec![],
            hidden: true,
            home_graph_hidden: false,
            home_instance: None,
        });
    }
    // The home instance's own collections state (authoritative; cached).
    let (home_instance, home_hide) = crate::api::follow::home_hide_state(state, user.id)
        .await
        .map(|(i, h)| (Some(i), h))
        .unwrap_or((None, None));
    let domain = state.config.domain.clone();
    let mut seen: HashSet<String> = HashSet::new();
    let mut result: Vec<FollowingView> = Vec::new();
    // Follow-state badges come from the SHARED matcher (the same exact
    // source-URL map the feed badges use) — never a fork.
    let follow_state = match viewer_id {
        Some(v) => crate::api::follow::viewer_follow_state_map(&state.db, v).await,
        None => Default::default(),
    };

    // (a) local `follows` rows.
    if following_side {
        let follows = db::follows::FollowRow::list_by_user(&state.db, user.id).await?;
        // Display data for remote rows from the actor cache — the same join
        // the inbound source uses.
        let mut remote_display: std::collections::HashMap<
            String,
            (Option<String>, Option<String>, Option<String>),
        > = Default::default();
        {
            let ids: Vec<String> = follows
                .iter()
                .map(|f| f.followed_actor.clone())
                .filter(|a| a.starts_with("http"))
                .collect();
            if !ids.is_empty() {
                let rows: Vec<(String, Option<String>, Option<String>, Option<String>)> =
                    sqlx::query_as(
                        "SELECT actor_id, preferred_username, display_name, avatar_url \
                         FROM ap_remote_actors WHERE actor_id = ANY($1)",
                    )
                    .bind(&ids)
                    .fetch_all(&state.db)
                    .await
                    .unwrap_or_default();
                for (id, p, d, a) in rows {
                    remote_display.insert(id, (p, d, a));
                }
            }
        }
        for follow in &follows {
            let is_local =
                follow.followed_actor.contains(&format!("https://{}/", domain));
            let target_username = if is_local {
                parse_local_username_from_actor_url(&domain, &follow.followed_actor)
            } else {
                None
            };
            let cached = remote_display.get(&follow.followed_actor);
            let (display_name, avatar) = match &target_username {
                Some(tu) => match db::users::UserRow::find_by_username(&state.db, tu).await? {
                    Some(t) => (Some(t.display_name), t.avatar_url),
                    None => (None, None),
                },
                None => match cached {
                    Some((_, d, a)) => (d.clone(), a.clone()),
                    None => (None, None),
                },
            };
            let uname = target_username.clone().unwrap_or_else(|| {
                if let Some((Some(p), _, _)) = cached.filter(|(p, _, _)| {
                    p.as_deref().is_some_and(|s| !s.is_empty())
                }) {
                    let host = follow
                        .followed_actor
                        .split("://")
                        .nth(1)
                        .and_then(|r| r.split('/').next())
                        .unwrap_or("");
                    return format!("{p}@{host}");
                }
                graph_identity_key(&follow.followed_actor)
            });
            if !seen.insert(graph_identity_key(&follow.followed_actor)) {
                continue;
            }
            let pending = follow.status == "pending";
            result.push(FollowingView {
                username: uname,
                handle: follow.followed_actor.clone(),
                display_name,
                avatar,
                is_local,
                network: follow_type_network(&follow.follow_type).to_string(),
                identity_used: follow
                    .source_platform
                    .clone()
                    .unwrap_or_else(|| user.username.clone()),
                following: follow_state.contains_key(&follow.followed_actor),
                follow_pending: follow_state
                    .get(&follow.followed_actor)
                    .copied()
                    .unwrap_or(pending),
                can_remove: false,
            });
        }
    } else {
        // Local followers: rows other local accounts aimed at this user's
        // actor forms (same shape the profile count uses).
        let actor_forms: Vec<String> = [
            format!("https://{}/@{}", domain, user.username),
            user.ap_actor_id.clone().unwrap_or_default(),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
        let rows: Vec<(Uuid, String, String, Option<String>, String, Option<String>)> =
            sqlx::query_as(
                "SELECT f.follower_id, u.username, u.display_name, u.avatar_url, \
                        f.follow_type, f.source_platform \
                 FROM follows f JOIN users u ON u.id = f.follower_id \
                 WHERE f.followed_actor = ANY($1) \
                 ORDER BY f.created_at DESC",
            )
            .bind(&actor_forms)
            .fetch_all(&state.db)
            .await
            .map_err(|e| AppError::Internal(format!("db: {e}")))?;
        for (_fid, username, display_name, avatar, follow_type, source_platform) in rows {
            if !seen.insert(username.to_lowercase()) {
                continue;
            }
            let handle = format!("https://{}/@{}", domain, username);
            result.push(FollowingView {
                username: username.clone(),
                handle: handle.clone(),
                display_name: Some(display_name),
                avatar,
                is_local: true,
                network: follow_type_network(&follow_type).to_string(),
                identity_used: source_platform.unwrap_or_else(|| username.clone()),
                following: follow_state.contains_key(&handle),
                follow_pending: follow_state.get(&handle).copied().unwrap_or(false),
                can_remove: is_own,
            });
        }

        // (b) inbound AP followers, display data from the actor cache.
        let rows: Vec<(String, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT f.actor_id, r.preferred_username, r.display_name, r.avatar_url \
             FROM followers f LEFT JOIN ap_remote_actors r ON r.actor_id = f.actor_id \
             WHERE f.user_id = $1 AND f.accepted \
             ORDER BY f.created_at DESC",
        )
        .bind(user.id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Internal(format!("db: {e}")))?;
        for (actor_id, preferred_username, display_name, avatar) in rows {
            let key = graph_identity_key(&actor_id);
            if !seen.insert(key.clone()) {
                continue;
            }
            let username = match preferred_username {
                Some(p) if !p.is_empty() => {
                    let host = actor_id
                        .split("://")
                        .nth(1)
                        .and_then(|r| r.split('/').next())
                        .unwrap_or("");
                    format!("{p}@{host}")
                }
                _ => key.clone(),
            };
            result.push(FollowingView {
                username,
                handle: actor_id.clone(),
                display_name,
                avatar,
                is_local: false,
                network: "activitypub".to_string(),
                identity_used: actor_id.clone(),
                following: follow_state.contains_key(&actor_id),
                follow_pending: follow_state.get(&actor_id).copied().unwrap_or(false),
                can_remove: is_own,
            });
        }
    }

    // (c) live Bluesky graph + (d) home-instance graph — both FAIL-OPEN:
    // a dead credential or proxy never errors the endpoint.
    let accounts = crate::db::accounts::LinkedAccountRow::find_by_user(&state.db, user.id)
        .await
        .map_err(AppError::Database)?;
    let http = reqwest::Client::new();
    for acct in &accounts {
        match acct.provider.as_str() {
            "bluesky" => {
                let pds = crate::oauth::bluesky::get_pds_url(&acct.metadata);
                let rows = if following_side {
                    crate::oauth::bluesky::get_follows(
                        &http,
                        &acct.external_id,
                        &acct.access_token,
                        &pds,
                    )
                    .await
                    .map(|v| {
                        v.into_iter()
                            .map(|(did, handle, display)| (did, handle, display, None))
                            .collect::<Vec<_>>()
                    })
                } else {
                    crate::oauth::bluesky::get_followers(
                        &http,
                        &acct.external_id,
                        &acct.access_token,
                        &pds,
                    )
                    .await
                };
                let rows = match rows {
                    Ok(rows) => Ok(rows),
                    // The stored credential could not read the PDS (stale
                    // token, scope drift) — the graph is public data; the
                    // public AppView keeps source (c) from silently dropping.
                    Err(_) => bsky_graph_public(&acct.external_id, !following_side).await,
                };
                if let Ok(rows) = rows {
                    for (did, handle, display, avatar) in rows {
                        if !seen.insert(graph_identity_key(&handle)) {
                            continue;
                        }
                        let bsky_url = format!("https://bsky.app/profile/{handle}");
                        result.push(FollowingView {
                            username: handle.clone(),
                            handle: did.clone(),
                            display_name: Some(display).filter(|d| !d.is_empty()),
                            avatar,
                            is_local: false,
                            network: "atproto".to_string(),
                            identity_used: acct.external_id.clone(),
                            following: follow_state.contains_key(&bsky_url),
                            follow_pending: follow_state.get(&bsky_url).copied().unwrap_or(false),
                            can_remove: false,
                        });
                    }
                }
            }
            "mastodon" => {
                let instance = match crate::api::feed::home_instance_of(acct) {
                    Ok(i) => i,
                    Err(_) => continue,
                };
                // Phase 6: a hidden home graph degrades to the stored rows —
                // the live proxy is skipped and the response discloses the
                // hidden state (the home value is authoritative).
                if home_hide == Some(true) {
                    continue;
                }
                let token =
                    match crate::oauth::mastodon::refresh_mastodon_token(&state.db, &http, acct)
                        .await
                    {
                        Ok(t) => t,
                        Err(_) => continue, // fail-open: stored rows stand alone
                    };
                let rows = if following_side {
                    crate::oauth::mastodon::following_accounts(&http, &instance, &token).await
                } else {
                    crate::oauth::mastodon::follower_accounts(&http, &instance, &token).await
                };
                if let Ok(rows) = rows {
                    for m in rows {
                        if !seen.insert(graph_identity_key(&m.acct)) {
                            continue;
                        }
                        result.push(FollowingView {
                            username: m.acct.clone(),
                            handle: m.url.clone().unwrap_or_else(|| m.acct.clone()),
                            display_name: Some(m.display_name).filter(|d| !d.is_empty()),
                            avatar: None,
                            is_local: false,
                            network: "activitypub".to_string(),
                            identity_used: acct.external_username.clone(),
                            following: false,
                            follow_pending: false,
                            can_remove: false,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    Ok(MergedGraphResponse {
        rows: result,
        hidden: false,
        home_graph_hidden: home_hide == Some(true),
        home_instance,
    })
}

/// GET /api/profile/{username}/followers — the merged follower list.
async fn list_followers(
    State(state): State<AppState>,
    Path(username): Path<String>,
    headers: HeaderMap,
) -> Result<Json<MergedGraphResponse>, AppError> {
    let user = db::users::UserRow::find_by_username(&state.db, &username)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("User {} not found", username)))?;
    let viewer_id = crate::api::feed::optional_viewer(&headers, &state.config.jwt_secret)?;
    let is_own = viewer_id == Some(user.id);
    merged_graph_rows(&state, &user, viewer_id, is_own, false).await.map(Json)
}

/// GET /api/profile/{username}/following — the merged following list.
async fn list_following(
    State(state): State<AppState>,
    Path(username): Path<String>,
    headers: HeaderMap,
) -> Result<Json<MergedGraphResponse>, AppError> {
    let user = db::users::UserRow::find_by_username(&state.db, &username)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("User {} not found", username)))?;
    let viewer_id = crate::api::feed::optional_viewer(&headers, &state.config.jwt_secret)?;
    let is_own = viewer_id == Some(user.id);
    merged_graph_rows(&state, &user, viewer_id, is_own, true).await.map(Json)
}

#[derive(Debug, serde::Deserialize)]
struct RemoveFollowerRequest {
    actor_id: String,
}

/// POST /api/profile/{username}/followers/remove — follower management on
/// one's own profile. The AP case retracts ON-NETWORK: an `Undo` of the
/// follower's stored `Follow` is queued to their inbox before the row goes;
/// local rows just delete; Bluesky/home graphs have no remove affordance
/// (the caller only offers `can_remove` rows).
async fn remove_follower(
    State(state): State<AppState>,
    Path(username): Path<String>,
    headers: HeaderMap,
    Json(req): Json<RemoveFollowerRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let viewer_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let user = db::users::UserRow::find_by_username(&state.db, &username)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("User {} not found", username)))?;
    if viewer_id != user.id {
        return Err(AppError::Forbidden("only the profile owner manages followers".into()));
    }

    let actor_forms: Vec<String> = [
        format!("https://{}/@{}", state.config.domain, user.username),
        user.ap_actor_id.clone().unwrap_or_default(),
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect();

    // Local follower row first (the actor_id is their actor URL).
    let deleted = sqlx::query(
        "DELETE FROM follows WHERE followed_actor = ANY($1) AND follower_id = \
         (SELECT id FROM users WHERE ap_actor_id = $2 OR username = $3)",
    )
    .bind(&actor_forms)
    .bind(&req.actor_id)
    .bind(
        req.actor_id
            .rsplit('/')
            .next()
            .unwrap_or("")
            .trim_start_matches('@'),
    )
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(format!("db: {e}")))?
    .rows_affected();
    if deleted > 0 {
        return Ok(Json(serde_json::json!({ "removed": true, "network": "local" })));
    }

    // Inbound AP follower: retract on-network (Undo of their Follow), then
    // drop the row.
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT actor_id, inbox_url FROM followers WHERE user_id = $1 AND actor_id = $2",
    )
    .bind(user.id)
    .bind(&req.actor_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(format!("db: {e}")))?;
    let Some((actor_id, follower_inbox)) = row else {
        return Err(AppError::NotFound("no such follower".into()));
    };

    let original: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM ap_activities \
         WHERE actor_id = $1 AND activity_type = 'Follow' AND direction = 'in' \
           AND (object_id = ANY($2) OR object_id LIKE '%' || $3 || '%') \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&actor_id)
    .bind(&actor_forms)
    .bind(&user.username)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(format!("db: {e}")))?;

    let our_actor = actor_forms[0].clone();
    let undo_id = format!(
        "{}/activities/remove-follower-{}",
        our_actor,
        uuid::Uuid::new_v4().simple()
    );
    let object_ref = original
        .map(|(id,)| id)
        .unwrap_or_else(|| format!("{actor_id}#follow"));
    let payload = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": undo_id,
        "type": "Undo",
        "actor": our_actor,
        "object": object_ref,
        "to": [actor_id],
    });
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| AppError::Internal(format!("db: {e}")))?;
    crate::ap::store::insert_activity(
        &mut tx,
        &undo_id,
        &our_actor,
        "Undo",
        Some(&object_ref),
        None,
        "out",
        &payload,
    )
    .await
    .map_err(|e| AppError::Internal(format!("db: {e}")))?;
    let mut inboxes =
        crate::ap::store::audience_inboxes(&state.db, &our_actor, &[actor_id.clone()]).await;
    if inboxes.is_empty() {
        // The actor cache is the normal resolver; the followers row's own
        // inbox is the floor so a retraction is never silently dropped.
        inboxes.push(follower_inbox.clone());
    }
    crate::ap::store::enqueue_for_inboxes(&mut *tx, &undo_id, &inboxes)
        .await
        .map_err(|e| AppError::Internal(format!("db: {e}")))?;
    sqlx::query("DELETE FROM followers WHERE user_id = $1 AND actor_id = $2")
        .bind(user.id)
        .bind(&actor_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(format!("db: {e}")))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(format!("db: {e}")))?;

    Ok(Json(serde_json::json!({
        "removed": true,
        "network": "activitypub",
        "undo_queued": !inboxes.is_empty(),
    })))
}

async fn list_friends(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<Vec<FollowingView>>, AppError> {
    let user = db::users::UserRow::find_by_username(&state.db, &username)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("User {} not found", username)))?;

    let mut seen = HashSet::new();
    let mut result = Vec::new();
    let user_actor_url = build_actor_url(&state.config.domain, &user.username);

    // 1. Local mutual follows (bidirectional AP follows)
    let follows = db::follows::FollowRow::list_by_user(&state.db, user.id).await?;
    for follow in &follows {
        let is_local = follow.followed_actor.contains(&format!("https://{}/", state.config.domain));
        if !is_local {
            continue;
        }

        let target_username = match parse_local_username_from_actor_url(&state.config.domain, &follow.followed_actor) {
            Some(u) => u,
            None => continue,
        };

        let target_user = match db::users::UserRow::find_by_username(&state.db, &target_username).await? {
            Some(u) => u,
            None => continue,
        };

        let target_follows_back = db::follows::FollowRow::find_by_follower_and_actor(
            &state.db, target_user.id, &user_actor_url,
        )
        .await?
        .is_some();

        if !target_follows_back {
            continue;
        }

        if seen.insert(target_user.username.clone()) {
            result.push(FollowingView {
                username: target_user.username.clone(),
                handle: follow.followed_actor.clone(),
                display_name: Some(target_user.display_name),
                avatar: target_user.avatar_url,
                is_local: true,
                network: "local".to_string(),
                identity_used: follow
                    .source_platform
                    .clone()
                    .unwrap_or_else(|| user.username.clone()),
                following: true,
                follow_pending: false,
                can_remove: false,
            });
        }
    }

    // 2. Bluesky mutuals (intersection of follows and followers)
    let accounts = crate::db::accounts::LinkedAccountRow::find_by_user(&state.db, user.id)
        .await
        .map_err(AppError::Database)?;

    let http = reqwest::Client::new();

    for acct in &accounts {
        if acct.provider != "bluesky" {
            continue;
        }
        let pds = crate::oauth::bluesky::get_pds_url(&acct.metadata);

        let bsky_follows = crate::oauth::bluesky::get_follows(
            &http, &acct.external_id, &acct.access_token, &pds,
        )
        .await
        .unwrap_or_default();
        let bsky_followers = crate::oauth::bluesky::get_followers(
            &http, &acct.external_id, &acct.access_token, &pds,
        )
        .await
        .unwrap_or_default();

        let follows_set: HashSet<String> = bsky_follows.iter().map(|(did, _, _)| did.clone()).collect();
        for (did, handle, display, avatar) in &bsky_followers {
            if follows_set.contains(did)
                && seen.insert(handle.clone()) {
                    result.push(FollowingView {
                        username: handle.clone(),
                        handle: did.clone(),
                        display_name: Some(display.clone()).filter(|d| !d.is_empty()),
                        avatar: avatar.clone(),
                        is_local: false,
                        network: "atproto".to_string(),
                        identity_used: acct.external_id.clone(),
                        following: true,
                        follow_pending: false,
                        can_remove: false,
                    });
                }
        }
    }

    Ok(Json(result))
}

#[cfg(test)]
#[path = "profile.tests.rs"]
mod merged_graph_tests;

async fn follow_user(
    State(state): State<AppState>,
    Path(username): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let viewer_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    // Local username OR `handle@host` — the FollowService routes to the
    // right network adapter (local row, minted AP Follow, or — for
    // Mastodon-linked callers — a home-instance follow). The historic
    // response shape is preserved; the outcome fields are additive.
    let outcome = crate::api::follow::follow(&state, viewer_id, &username, "").await?;
    let status = match outcome.status {
        "already_following" => "already_following",
        _ => "accepted",
    };
    Ok(Json(serde_json::json!({
        "status": status,
        "network": outcome.network,
        "identity_used": outcome.identity_used,
        "actor": outcome.actor,
        "federated": outcome.federated,
        "pending": outcome.status == "pending",
    })))
}

async fn unfollow_user(
    State(state): State<AppState>,
    Path(username): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let viewer_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    // Unfollow routes back through the adapter that made the follow (the
    // follows row's follow_type is the undo key).
    let outcome = crate::api::follow::unfollow(&state, viewer_id, &username, "").await?;
    Ok(Json(serde_json::json!({
        "status": "unfollowed",
        "network": outcome.network,
        "actor": outcome.actor,
        "federated": outcome.federated,
    })))
}

async fn relationships(
    State(state): State<AppState>,
    Query(query): Query<RelationshipsQuery>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let viewer_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let usernames: Vec<String> = query
        .usernames
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if usernames.is_empty() {
        return Ok(Json(serde_json::json!({})));
    }

    let mut actor_urls: Vec<String> = Vec::new();
    let mut name_to_url: Vec<(String, String)> = Vec::new();

    for name in &usernames {
        let url = build_actor_url(&state.config.domain, name);
        actor_urls.push(url.clone());
        name_to_url.push((name.clone(), url));
    }

    let followed = db::follows::FollowRow::find_followed_actors(
        &state.db, viewer_id, &actor_urls,
    )
    .await?;

    let followed_set: std::collections::HashSet<&str> =
        followed.iter().map(|s| s.as_str()).collect();

    let mut result = serde_json::Map::new();
    for (name, url) in &name_to_url {
        result.insert(name.clone(), serde_json::Value::Bool(followed_set.contains(url.as_str())));
    }

    Ok(Json(serde_json::Value::Object(result)))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn connect_pool() -> Option<sqlx::PgPool> {
        if std::env::var("DATABASE_URL").is_err() {
            let _ = dotenvy::dotenv();
            let _ = dotenvy::from_filename("bytescape-server/.env");
        }
        match std::env::var("DATABASE_URL") {
            Ok(url) => {
                let pool = sqlx::PgPool::connect(&url).await.expect("db connect");
                // Fail fast rather than hang if the shared test DB is under
                // write contention (e.g. a live appview consumer rewriting
                // appview_posts). Keeps the test deterministic.
                let _ = sqlx::query("SET statement_timeout = '20s'").execute(&pool).await;
                Some(pool)
            }
            Err(_) => {
                eprintln!("DATABASE_URL unset — skipping DB integration test");
                None
            }
        }
    }

    /// Regression: the profile resolves imported remote posts through the
    /// linked-identity map. A post attributed to the REMOTE identity
    /// (e.g. `lovingisliving`) must appear on the profile of the local user
    /// (e.g. `xxlovingislivingxx`) whose linked_accounts row names that remote
    /// identity — even though the local username differs. This is the identity
    /// resolution from docs/design/identity-linkage.md.
    #[tokio::test]
    async fn profile_resolves_posts_via_linked_identity() {
        let Some(pool) = connect_pool().await else { return };

        // Distinctive markers so the test is isolated and idempotent.
        let suffix = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let local_username = format!("proftest_{suffix}");
        let remote_handle = format!("remoteperson_{suffix}");
        let post_uri = format!("https://mastodon.test/users/{remote_handle}/statuses/{suffix}");

        // Local user (the profile owner).
        let pw = crate::auth::hash_password("x").expect("hash");
        let user = crate::db::users::UserRow::create_active(&pool, &local_username, "", &pw, "pk")
            .await
            .expect("create user");

        // Linked identity: the local user's remote identity is `remote_handle`.
        crate::db::accounts::LinkedAccountRow::create(
            &pool, user.id, "mastodon", &remote_handle, &remote_handle, "at", None, None, None, None,
            &serde_json::json!({"instance": "mastodon.test"}),
        )
        .await
        .expect("link identity");

        // An imported remote post attributed to the REMOTE handle (not the
        // local username) — the exact mismatch that used to hide history.
        sqlx::query(
            r#"INSERT INTO appview_posts (uri, cid, author_did, author_handle, text, created_at)
               VALUES ($1, $2, $3, $4, $5, now())"#,
        )
        .bind(&post_uri)
        .bind(format!("cid_{suffix}"))
        .bind(format!("https://mastodon.test/users/{remote_handle}"))
        .bind(format!("{remote_handle}@mastodon.test"))
        .bind("imported remote post")
        .execute(&pool)
        .await
        .expect("insert post");

        // The resolution must find the post via the linked identity even though
        // the local username (`local_username`) is not on the post at all.
        let posts = identity_appview_posts(&pool, user.id, &local_username).await.expect("resolve linked identity");
        assert!(
            posts.iter().any(|p| p.id == post_uri),
            "linked-identity resolution must surface the remote-attributed post; got {} posts",
            posts.len()
        );

        // Cleanup (best-effort).
        let _ = sqlx::query("DELETE FROM appview_posts WHERE uri = $1").bind(&post_uri).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM linked_accounts WHERE user_id = $1").bind(user.id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1").bind(user.id).execute(&pool).await;
    }
}
