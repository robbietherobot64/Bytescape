use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{delete, get},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::feed::{AuthorInfo, FeedPost, FeedResponse};
use crate::{auth, db, error::AppError, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/rss/subscriptions", get(list_subscriptions).post(create_subscription))
        .route("/api/rss/subscriptions/{id}", delete(delete_subscription))
        .route("/api/rss/feed", get(rss_feed))
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
struct RssSubscriptionView {
    id: String,
    url: String,
    label: String,
    category: String,
    enabled: bool,
    last_fetched_at: Option<String>,
    fetch_error: Option<String>,
}

#[derive(Deserialize)]
struct CreateRssSubscriptionReq {
    url: String,
}

#[derive(Deserialize)]
struct RssFeedQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn list_subscriptions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<RssSubscriptionView>>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let rows = db::subscriptions::SubscriptionRow::find_by_category(&state.db, user_id, "rss")
        .await
        .map_err(AppError::Database)?;

    let subs: Vec<RssSubscriptionView> = rows
        .into_iter()
        .map(|r| RssSubscriptionView {
            id: r.id.to_string(),
            url: r.source_id,
            label: r.label,
            category: r.category,
            enabled: r.enabled,
            last_fetched_at: None,
            fetch_error: None,
        })
        .collect();

    Ok(Json(subs))
}

async fn create_subscription(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateRssSubscriptionReq>,
) -> Result<(StatusCode, Json<RssSubscriptionView>), AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    if req.url.trim().is_empty() {
        return Err(AppError::BadRequest("URL cannot be empty".into()));
    }

    // Convert common URLs to RSS feed URLs
    let url = normalize_to_rss_url(&req.url).await;

    // Check if this is a profile URL (Instagram, Threads, Facebook) — skip RSS validation
    let is_profile_url = url.contains("instagram.com")
        || url.contains("threads.net")
        || url.contains("facebook.com");

    let (label, has_rss) = if is_profile_url {
        // Extract username from URL for label
        let url_path = url.trim_end_matches('/');
        let username = url_path
            .rsplit('/')
            .next()
            .unwrap_or("unknown")
            .trim_start_matches('@')
            .trim_start_matches('_')
            .to_string();
        (username, false)
    } else {
        // Fetch and validate the feed (try RSS first, then Atom)
        let resp = reqwest::get(&url)
            .await
            .map_err(|e| AppError::BadRequest(format!("Failed to fetch URL: {}", e)))?;

        let body = resp.bytes()
            .await
            .map_err(|e| AppError::BadRequest(format!("Failed to read response: {}", e)))?;

        // Try RSS first
        let (label, has_rss) = if let Ok(channel) = rss::Channel::read_from(body.as_ref()) {
            let title_str = channel.title();
            let label = if title_str.is_empty() {
                url.trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .trim_start_matches("www.")
                    .to_string()
            } else {
                title_str.to_string()
            };
            (label, true)
        } else if let Ok(feed) = atom_syndication::Feed::read_from(body.as_ref()) {
            // Try Atom format (YouTube, some Mastodon instances)
            let title_str = feed.title();
            let label = if title_str.is_empty() {
                url.trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .trim_start_matches("www.")
                    .to_string()
            } else {
                title_str.to_string()
            };
            (label, true)
        } else {
            return Err(AppError::BadRequest("Not a valid RSS or Atom feed".into()));
        };
        (label, true)
    };

    let row = db::subscriptions::SubscriptionRow::create(
        &state.db,
        user_id,
        "rss",
        &url,
        &label,
        "📡",
        "rss",
    )
    .await
    .map_err(AppError::Database)?;

    // Store initial RSS/Atom items
    if has_rss {
        let resp = reqwest::get(&url).await.ok();
        if let Some(resp) = resp {
            if let Ok(body) = resp.bytes().await {
                // Try RSS first
                if let Ok(channel) = rss::Channel::read_from(body.as_ref()) {
                    for item in channel.items() {
                        let guid = item.guid().map(|g| g.value()).or_else(|| item.link()).unwrap_or("");
                        if guid.is_empty() {
                            continue;
                        }
                        let pub_date = item.pub_date()
                            .and_then(|d| chrono::DateTime::parse_from_rfc2822(d).ok())
                            .map(|d| d.with_timezone(&chrono::Utc))
                            .unwrap_or_else(chrono::Utc::now);

                        db::rss::RssFeedItemRow::create(
                            &state.db,
                            row.id,
                            user_id,
                            guid,
                            item.title(),
                            item.link(),
                            item.description(),
                            item.author(),
                            pub_date,
                            item.content(),
                        )
                        .await
                        .ok();
                    }
                } else if let Ok(feed) = atom_syndication::Feed::read_from(body.as_ref()) {
                    // Try Atom format (YouTube, some Mastodon instances)
                    for entry in feed.entries() {
                        let guid = entry.id();
                        if guid.is_empty() {
                            continue;
                        }
                        let pub_date = entry.published()
                            .map(|d| d.with_timezone(&chrono::Utc))
                            .unwrap_or_else(|| entry.updated().with_timezone(&chrono::Utc));

                        let title = entry.title().value.clone();
                        let link = entry.links().first().map(|l| l.href().to_string());
                        let content = entry.content().and_then(|c| c.value().map(|v| v.to_string()));
                        let summary = entry.summary().map(|s| s.value.clone());

                        db::rss::RssFeedItemRow::create(
                            &state.db,
                            row.id,
                            user_id,
                            guid,
                            Some(&title),
                            link.as_deref(),
                            summary.as_deref(),
                            None, // Atom doesn't have author field the same way
                            pub_date,
                            content.as_deref(),
                        )
                        .await
                        .ok();
                    }
                }
            }
        }
    }

    // Update last_fetched_at
    sqlx::query("UPDATE subscriptions SET last_fetched_at = NOW() WHERE id = $1")
        .bind(row.id)
        .execute(&state.db)
        .await
        .ok();

    Ok((
        StatusCode::CREATED,
        Json(RssSubscriptionView {
            id: row.id.to_string(),
            url: url,
            label: row.label,
            category: row.category,
            enabled: row.enabled,
            last_fetched_at: Some(chrono::Utc::now().to_rfc3339()),
            fetch_error: None,
        }),
    ))
}

async fn delete_subscription(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let deleted = db::subscriptions::SubscriptionRow::delete(&state.db, id, user_id)
        .await
        .map_err(AppError::Database)?;

    if deleted == 0 {
        return Err(AppError::NotFound("RSS subscription not found".into()));
    }

    // Cascade deletes rss_feed_items via FK, but also ensure cleanup
    db::rss::RssFeedItemRow::delete_by_subscription(&state.db, id)
        .await
        .ok();

    Ok(Json(serde_json::json!({ "deleted": true })))
}

async fn rss_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RssFeedQuery>,
) -> Result<Json<FeedResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.offset.unwrap_or(0);

    let items = db::rss::RssFeedItemRow::feed_for_user_with_labels(&state.db, user_id, limit, offset)
        .await
        .map_err(AppError::Database)?;

    let posts: Vec<FeedPost> = items
        .into_iter()
        .map(|item| {
            let author_name = item.author.clone().unwrap_or_else(|| item.subscription_label.clone());
            let content_text = item.description.clone()
                .or_else(|| item.content.clone())
                .or_else(|| item.title.clone())
                .unwrap_or_else(|| "(no content)".to_string());
            FeedPost {
                id: format!("rss:{}", item.link.as_deref().unwrap_or(&item.title.clone().unwrap_or_default())),
                source: "rss".to_string(),
                author: AuthorInfo {
                    handle: author_name.clone(),
                    display_name: Some(author_name),
                    avatar: None,
                },
                content: content_text,
                content_type: "link".to_string(),
                media: vec![],
                embeds: {
                    let mut embeds = vec![];
                    if let Some(ref link) = item.link {
                        embeds.push(super::feed::EmbedInfo {
                            kind: "link".to_string(),
                            url: link.clone(),
                            title: item.title.clone(),
                            description: None,
                            thumbnail: None,
                        });
                    }
                    embeds
                },
                likes_count: 0,
                shares_count: 0,
                replies_count: 0,
                created_at: item.pub_date.to_rfc3339(),
                url: item.link.clone(),
                thread_id: None,
                start_time: None,
                end_time: None,
                location: None,
                reaction_counts: None,
                user_reaction: None,
                cid: None,
                author_url: None,
                reposted_by: None,
                boosted_by: None,
                quoted_post_uri: None,
                is_reshare: Some(false),
                original_post: None,
                display_name: None,
                action_text: None,
                group_name: None, group: None, upvotes: None, downvotes: None, my_vote: None,
                is_own: None,
                is_following: None,
                follow_pending: None,
                cross_posts: vec![],
    matched_sources: vec![],
    labels: vec![],
    nsfw: false,
        author_external_id: None,
            }
        })
        .collect();

    let returned = posts.len() as i64;
    let has_more = returned >= limit;

    Ok(Json(FeedResponse {
        items: posts,
        cursor: None,
        has_more,
        phase: None,
        platform_counts: None,
    }))
}

/// Convert common platform URLs to their RSS feed equivalents.
/// YouTube channel URLs → RSS feed, Reddit profiles → RSS, Mastodon profiles → Atom, etc.
async fn normalize_to_rss_url(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');

    // YouTube: @handle, channel/ID, c/name → RSS feed
    if url.contains("youtube.com") || url.contains("youtu.be") {
        // Already an RSS feed
        if url.contains("feeds/videos.xml") {
            return url.to_string();
        }
        // Try to extract channel ID from page HTML
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .timeout(std::time::Duration::from_secs(10))
            .build().unwrap_or(reqwest::Client::new());
        eprintln!("[rss-normalize] Fetching YouTube page: {}", url);
        match client.get(url).send().await {
            Ok(resp) => {
                match resp.text().await {
                    Ok(body) => {
                        eprintln!("[rss-normalize] YouTube page fetched, length: {}", body.len());
                        // Look for channel_id in page source
                        if let Some(start) = body.find("\"channelId\":\"") {
                            let rest = &body[start + 13..];
                            if let Some(end) = rest.find('"') {
                                let channel_id = &rest[..end];
                                let rss_url = format!("https://www.youtube.com/feeds/videos.xml?channel_id={}", channel_id);
                                eprintln!("[rss-normalize] YouTube channel ID found: {} → {}", channel_id, rss_url);
                                return rss_url;
                            }
                        }
                        // Fallback: try externalId (some channels use this instead)
                        if let Some(start) = body.find("\"externalId\":\"") {
                            let rest = &body[start + 14..];
                            if let Some(end) = rest.find('"') {
                                let channel_id = &rest[..end];
                                let rss_url = format!("https://www.youtube.com/feeds/videos.xml?channel_id={}", channel_id);
                                eprintln!("[rss-normalize] YouTube externalId found: {} → {}", channel_id, rss_url);
                                return rss_url;
                            }
                        }
                        eprintln!("[rss-normalize] YouTube channelId not found in page body (length={})", body.len());
                    }
                    Err(e) => eprintln!("[rss-normalize] YouTube response read failed: {}", e),
                }
            }
            Err(e) => eprintln!("[rss-normalize] YouTube fetch failed: {}", e),
        }
    }

    // Reddit: user/subreddit URL → RSS
    if url.contains("reddit.com") && !url.contains(".rss") {
        return format!("{}/.rss", url);
    }

    // Mastodon: profile URL → Atom feed
    if url.contains("mastodon.social") || url.contains("mas.to") || url.contains("fosstodon.org") {
        // https://instance/@user → https://instance/users/user.atom
        if let Some(at_pos) = url.rfind("/@") {
            let instance = &url[..at_pos];
            let username = &url[at_pos + 2..];
            return format!("{}/users/{}.atom", instance, username);
        }
    }

    // Default: return as-is
    eprintln!("[rss-normalize] No conversion for URL: {}", url);
    url.to_string()
}
