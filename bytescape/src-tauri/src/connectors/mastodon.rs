//! # connectors/mastodon.rs — Mastodon / ActivityPub C2S Client
//!
//! Implements the client-to-server (C2S) portion of the Mastodon API, enabling
//! Bytescape to interact with any Mastodon-compatible fediverse instance
//! (Mastodon, Pleroma, Akkoma, GoToSocial, Lemmy, Misskey partial).
//!
//! ## How It Works
//!
//! 1. **OAuth2 Registration**: `register_app()` registers Bytescape as an OAuth2
//!    client with the target instance, storing the client ID/secret
//! 2. **Authentication**: `complete_auth()` exchanges the authorization code for
//!    access/refresh tokens, then `verify_credentials()` confirms the token works
//! 3. **Timeline Fetching**: `get_home_timeline()`, `get_hashtag_timeline()`,
//!    `get_account_statuses()` — all return data in Bluesky-compatible format
//!    for unified feed rendering
//! 4. **Actions**: Post, follow/unfollow, favourite/unfavourite, reblog/unreblog,
//!    follow/unfollow hashtags
//! 5. **DMs**: `get_conversations()` fetches direct message threads
//! 6. **Account Lookup**: `lookup_account()`, `get_following()`, `get_followers()`
//!
//! ## Bluesky-Compatible Output
//!
//! To avoid duplicating the entire feed rendering pipeline, Mastodon posts are
//! converted into `BlueskyImportedPost` via `status_to_bsky_post()`. This lets
//! the unified feed system treat Mastodon and Bluesky posts identically. For
//! vault storage, `status_to_bytescape_item()` converts directly to `BytescapeItem`.
//!
//! ## Rate Limiting
//!
//! All API calls go through `rate_limit_request()` which enforces a configurable
//! delay between requests to avoid hitting instance rate limits.
//!
//! ## Related Files
//! - `commands/mastodon.rs` — Tauri commands that wrap these functions
//! - `connectors/bluesky.rs` — `BlueskyImportedPost` type used for unified feeds
//! - `models.rs` — `BytescapeItem`, `VaultMessage` schemas
//! - `connectors/mod.rs` — `Source::Mastodon` enum variant

use serde::{Deserialize, Serialize};

use crate::models::{
    BytescapeItem,
    BytescapeType::{Status as BStatus, Gallery as BGallery},
    SyndicationStatus, Audience, VaultMessage,
};

/// Default redirect URI used during OAuth2 authorization.
pub const REDIRECT_URI: &str = "http://127.0.0.1:28461/mastodon/callback";

/// Default OAuth2 scopes requested by Bytescape.
pub const SCOPES: &str = "read write push";

// ═══════════════════════════════════════════════════════════════════
// SECTION 1: MASTODON API TYPES
// Request/response types mirroring the Mastodon REST API schemas.
// ═══════════════════════════════════════════════════════════════════

/// Registered OAuth2 application credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MastodonApp {
    pub instance: String,
    pub client_id: String,
    pub client_secret: String,
}

/// Authenticated user session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MastodonSession {
    pub instance: String,
    pub access_token: String,
    pub acct: String,
    pub display_name: String,
    pub avatar: String,
    pub account_id: String,
}

/// A Mastodon status (toot / post).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MastodonStatus {
    pub id: String,
    pub uri: String,
    pub url: Option<String>,
    pub content: String,
    pub created_at: String,
    pub visibility: String,
    pub account: MastodonAccount,
    #[serde(default)]
    pub media_attachments: Vec<MastodonMediaAttachment>,
    pub in_reply_to_id: Option<String>,
    pub reblog: Option<Box<MastodonStatus>>,
    #[serde(default)]
    pub favourites_count: i64,
    #[serde(default)]
    pub replies_count: i64,
    #[serde(default)]
    pub reblogs_count: i64,
    #[serde(default)]
    pub favourited: bool,
    #[serde(default)]
    pub reblogged: bool,
    #[serde(default)]
    pub sensitive: bool,
    pub spoiler_text: Option<String>,
}

/// A Mastodon user account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MastodonAccount {
    pub id: String,
    pub acct: String,
    pub display_name: String,
    pub url: String,
    pub avatar: String,
    #[serde(default)]
    pub header: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub followers_count: i64,
    #[serde(default)]
    pub following_count: i64,
    #[serde(default)]
    pub statuses_count: i64,
}

/// A media attachment on a status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MastodonMediaAttachment {
    pub id: String,
    #[serde(rename = "type")]
    pub media_type: String,
    pub url: String,
    pub preview_url: Option<String>,
    pub description: Option<String>,
}

/// Relationship between authenticated user and another account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MastodonRelationship {
    pub id: String,
    pub following: bool,
    pub followed_by: bool,
    pub blocking: bool,
    pub muting: bool,
    pub requested: bool,
}

/// A DM conversation entry (GET /api/v1/conversations).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MastodonConversation {
    pub id: String,
    pub unread: bool,
    pub accounts: Vec<MastodonAccount>,
    pub last_status: Option<MastodonStatus>,
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 2: OAUTH2 AUTHENTICATION
// App registration, authorization code exchange, and credential verification.
// ═══════════════════════════════════════════════════════════════════

/// Register Bytescape as an OAuth2 application on the given instance.
///
/// Returns `(MastodonApp, authorize_url)`.
pub async fn register_app(
    http_client: &reqwest::Client,
    instance: &str,
) -> Result<(MastodonApp, String), String> {
    let base = normalize_instance(instance);

    let resp = http_client
        .post(format!("{}/api/v1/apps", base))
        .json(&serde_json::json!({
            "client_name": "Bytescape",
            "redirect_uris": REDIRECT_URI,
            "scopes": SCOPES,
            "website": "https://bytescape.social"
        }))
        .send()
        .await
        .map_err(|e| format!("App registration failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("App registration failed ({}): {}", status, body));
    }

    let json: serde_json::Value = resp.json().await
        .map_err(|e| format!("Invalid registration response: {}", e))?;

    let client_id = json["client_id"].as_str()
        .ok_or("Missing client_id in response")?
        .to_string();
    let client_secret = json["client_secret"].as_str()
        .ok_or("Missing client_secret in response")?
        .to_string();

    let app = MastodonApp {
        instance: base.clone(),
        client_id: client_id.clone(),
        client_secret,
    };

    let auth_url = format!(
        "{}/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&scope={}",
        base,
        urlencoding::encode(&client_id),
        urlencoding::encode(REDIRECT_URI),
        urlencoding::encode(SCOPES),
    );

    Ok((app, auth_url))
}

/// Exchange an authorization code for an access token, then verify credentials.
pub async fn complete_auth(
    http_client: &reqwest::Client,
    app: &MastodonApp,
    auth_code: &str,
) -> Result<MastodonSession, String> {
    let resp = http_client
        .post(format!("{}/oauth/token", app.instance))
        .json(&serde_json::json!({
            "grant_type": "authorization_code",
            "code": auth_code,
            "client_id": app.client_id,
            "client_secret": app.client_secret,
            "redirect_uri": REDIRECT_URI,
            "scope": SCOPES,
        }))
        .send()
        .await
        .map_err(|e| format!("Token exchange failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Token exchange failed ({}): {}", status, body));
    }

    let token_json: serde_json::Value = resp.json().await
        .map_err(|e| format!("Invalid token response: {}", e))?;

    let access_token = token_json["access_token"].as_str()
        .ok_or("Missing access_token in response")?
        .to_string();

    let account = verify_credentials(http_client, &app.instance, &access_token).await?;

    Ok(MastodonSession {
        instance: app.instance.clone(),
        access_token,
        acct: account.acct,
        display_name: account.display_name,
        avatar: account.avatar,
        account_id: account.id,
    })
}

/// Verify the access token and retrieve the authenticated user's account.
pub async fn verify_credentials(
    http_client: &reqwest::Client,
    instance: &str,
    access_token: &str,
) -> Result<MastodonAccount, String> {
    let resp = http_client
        .get(format!("{}/api/v1/accounts/verify_credentials", normalize_instance(instance)))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("Credential verification failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Credential verification failed ({}): {}", status, body));
    }

    resp.json::<MastodonAccount>().await
        .map_err(|e| format!("Invalid account response: {}", e))
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 3: TIMELINE & FEED FETCHING
// Home timeline, account statuses, and hashtag timelines.
// Results are returned in Bluesky-compatible format for unified rendering.
// ═══════════════════════════════════════════════════════════════════

/// Fetch the home timeline of the authenticated user.
pub async fn get_home_timeline(
    http_client: &reqwest::Client,
    session: &MastodonSession,
    limit: Option<i32>,
    max_id: Option<&str>,
) -> Result<Vec<BytescapeItem>, String> {
    let mut url = format!(
        "{}/api/v1/timelines/home?limit={}",
        session.instance,
        limit.unwrap_or(20).min(40),
    );
    if let Some(max) = max_id {
        url.push_str(&format!("&max_id={}", urlencoding::encode(max)));
    }

    let resp = rate_limit_request(
        http_client
            .get(&url)
            .bearer_auth(&session.access_token),
    ).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Timeline fetch failed ({}): {}", status, body));
    }

    let statuses: Vec<MastodonStatus> = resp.json().await
        .map_err(|e| format!("Invalid timeline response: {}", e))?;

    let instance = &session.instance;
    Ok(statuses
        .into_iter()
        .map(|s| status_to_bytescape_item(instance, &s))
        .collect())
}

/// Fetch the home timeline and return as BlueskyImportedPost for unified feed merging.
pub async fn get_home_timeline_as_bsky(
    http_client: &reqwest::Client,
    session: &MastodonSession,
    limit: Option<i32>,
) -> Result<Vec<crate::connectors::bluesky::BlueskyImportedPost>, String> {
    let url = format!(
        "{}/api/v1/timelines/home?limit={}",
        session.instance,
        limit.unwrap_or(20).min(40),
    );

    let resp = rate_limit_request(
        http_client
            .get(&url)
            .bearer_auth(&session.access_token),
    ).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Timeline fetch failed ({}): {}", status, body));
    }

    let statuses: Vec<MastodonStatus> = resp.json().await
        .map_err(|e| format!("Invalid timeline response: {}", e))?;

    Ok(statuses
        .iter()
        .map(|s| status_to_bsky_post(&session.instance, s))
        .collect())
}

/// Fetch the authenticated user's own statuses as BlueskyImportedPost.
///
/// Mastodon API: `GET /api/v1/accounts/:id/statuses`
///
/// Automatically paginates via `max_id` to fetch up to `max_pages` pages
/// (default 10 = ~400 posts). Pass `max_id` to resume from a specific point.
pub async fn get_account_statuses_as_bsky(
    http_client: &reqwest::Client,
    session: &MastodonSession,
    limit: Option<i32>,
    max_id: Option<&str>,
    max_pages: Option<u32>,
) -> Result<(Vec<crate::connectors::bluesky::BlueskyImportedPost>, Option<String>), String> {
    let per_page = limit.unwrap_or(40).min(40);
    let pages = max_pages.unwrap_or(10) as usize;
    let mut all_posts = Vec::new();
    let mut current_max_id: Option<String> = max_id.map(|s| s.to_string());

    for _ in 0..pages {
        let mut url = format!(
            "{}/api/v1/accounts/{}/statuses?limit={}&exclude_replies=false&exclude_reblogs=false",
            session.instance,
            session.account_id,
            per_page,
        );
        if let Some(ref mid) = current_max_id {
            url.push_str(&format!("&max_id={}", urlencoding::encode(mid)));
        }

        let resp = rate_limit_request(
            http_client
                .get(&url)
                .bearer_auth(&session.access_token),
        ).await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Account statuses fetch failed ({}): {}", status, body));
        }

        let statuses: Vec<MastodonStatus> = resp.json().await
            .map_err(|e| format!("Invalid account statuses response: {}", e))?;

        if statuses.is_empty() {
            break;
        }

        // The last status ID becomes the cursor for the next page
        let last_id = statuses.last().map(|s| s.id.clone());

        all_posts.extend(
            statuses.iter().map(|s| status_to_bsky_post(&session.instance, s))
        );

        // If we got fewer than requested, there are no more pages
        if (statuses.len() as i32) < per_page {
            return Ok((all_posts, None));
        }

        current_max_id = last_id;
    }

    // Return the cursor for the next page (last status ID)
    let next_cursor = current_max_id;
    Ok((all_posts, next_cursor))
}

/// Fetch a public hashtag timeline as BlueskyImportedPost.
///
/// Mastodon API: `GET /api/v1/timelines/tag/:hashtag`
/// Does NOT require authentication on most instances (public endpoint).
pub async fn get_hashtag_timeline_as_bsky(
    http_client: &reqwest::Client,
    session: &MastodonSession,
    hashtag: &str,
    limit: Option<i32>,
) -> Result<Vec<crate::connectors::bluesky::BlueskyImportedPost>, String> {
    let tag = hashtag.trim_start_matches('#');
    let url = format!(
        "{}/api/v1/timelines/tag/{}?limit={}",
        session.instance,
        urlencoding::encode(tag),
        limit.unwrap_or(20).min(40),
    );

    let resp = rate_limit_request(
        http_client
            .get(&url)
            .bearer_auth(&session.access_token),
    ).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Hashtag timeline fetch failed ({}): {}", status, body));
    }

    let statuses: Vec<MastodonStatus> = resp.json().await
        .map_err(|e| format!("Invalid hashtag timeline response: {}", e))?;

    Ok(statuses
        .iter()
        .map(|s| status_to_bsky_post(&session.instance, s))
        .collect())
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 4: DIRECT MESSAGES
// ═══════════════════════════════════════════════════════════════════

/// Fetch DM conversations.
pub async fn get_conversations(
    http_client: &reqwest::Client,
    session: &MastodonSession,
    limit: Option<i32>,
) -> Result<Vec<MastodonConversation>, String> {
    let url = format!(
        "{}/api/v1/conversations?limit={}",
        session.instance,
        limit.unwrap_or(20).min(40),
    );

    let resp = http_client
        .get(&url)
        .bearer_auth(&session.access_token)
        .send()
        .await
        .map_err(|e| format!("Conversations fetch failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Conversations fetch failed ({}): {}", status, body));
    }

    resp.json().await
        .map_err(|e| format!("Invalid conversations response: {}", e))
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 5: ACTIONS (POST, FOLLOW, ENGAGE)
// Creating posts, following/unfollowing accounts, favouriting,
// reblogging, and managing hashtag subscriptions.
// ═══════════════════════════════════════════════════════════════════

/// Post a new status with the given visibility.
///
/// For DMs, set `visibility` to `"direct"` and mention the recipient
/// (e.g., `@user@instance.social`) in the status text.
pub async fn post_status(
    http_client: &reqwest::Client,
    session: &MastodonSession,
    content: &str,
    visibility: &str,
    in_reply_to_id: Option<&str>,
) -> Result<MastodonStatus, String> {
    let mut body = serde_json::json!({
        "status": content,
        "visibility": visibility,
    });
    if let Some(reply_id) = in_reply_to_id {
        body["in_reply_to_id"] = serde_json::Value::String(reply_id.to_string());
    }

    let resp = rate_limit_request(
        http_client
            .post(format!("{}/api/v1/statuses", session.instance))
            .bearer_auth(&session.access_token)
            .json(&body),
    ).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Post status failed ({}): {}", status, body));
    }

    resp.json().await
        .map_err(|e| format!("Invalid status response: {}", e))
}

// ──────────────────────────────────────────────────
// Follow / Unfollow
// ──────────────────────────────────────────────────

/// Follow a Mastodon account by its numeric account ID.
pub async fn follow_account(
    http_client: &reqwest::Client,
    session: &MastodonSession,
    account_id: &str,
) -> Result<MastodonRelationship, String> {
    let resp = http_client
        .post(format!("{}/api/v1/accounts/{}/follow", session.instance, account_id))
        .bearer_auth(&session.access_token)
        .send()
        .await
        .map_err(|e| format!("Follow failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Follow failed ({}): {}", status, body));
    }

    resp.json().await
        .map_err(|e| format!("Invalid relationship response: {}", e))
}

/// Unfollow a Mastodon account by its numeric account ID.
pub async fn unfollow_account(
    http_client: &reqwest::Client,
    session: &MastodonSession,
    account_id: &str,
) -> Result<MastodonRelationship, String> {
    let resp = http_client
        .post(format!("{}/api/v1/accounts/{}/unfollow", session.instance, account_id))
        .bearer_auth(&session.access_token)
        .send()
        .await
        .map_err(|e| format!("Unfollow failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Unfollow failed ({}): {}", status, body));
    }

    resp.json().await
        .map_err(|e| format!("Invalid relationship response: {}", e))
}

// ──────────────────────────────────────────────────
// Favourite / Unfavourite
// ──────────────────────────────────────────────────

/// Favourite (like) a status on Mastodon.
pub async fn favourite_status(
    http_client: &reqwest::Client,
    session: &MastodonSession,
    status_id: &str,
) -> Result<MastodonStatus, String> {
    let resp = rate_limit_request(
        http_client
            .post(format!("{}/api/v1/statuses/{}/favourite", session.instance, status_id))
            .bearer_auth(&session.access_token),
    ).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Favourite failed ({}): {}", status, body));
    }

    resp.json().await
        .map_err(|e| format!("Invalid favourite response: {}", e))
}

/// Unfavourite (unlike) a status on Mastodon.
pub async fn unfavourite_status(
    http_client: &reqwest::Client,
    session: &MastodonSession,
    status_id: &str,
) -> Result<MastodonStatus, String> {
    let resp = rate_limit_request(
        http_client
            .post(format!("{}/api/v1/statuses/{}/unfavourite", session.instance, status_id))
            .bearer_auth(&session.access_token),
    ).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Unfavourite failed ({}): {}", status, body));
    }

    resp.json().await
        .map_err(|e| format!("Invalid unfavourite response: {}", e))
}

// ──────────────────────────────────────────────────
// Reblog / Unreblog
// ──────────────────────────────────────────────────

/// Reblog (repost) a status on Mastodon.
pub async fn reblog_status(
    http_client: &reqwest::Client,
    session: &MastodonSession,
    status_id: &str,
) -> Result<MastodonStatus, String> {
    let resp = rate_limit_request(
        http_client
            .post(format!("{}/api/v1/statuses/{}/reblog", session.instance, status_id))
            .bearer_auth(&session.access_token),
    ).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Reblog failed ({}): {}", status, body));
    }

    resp.json().await
        .map_err(|e| format!("Invalid reblog response: {}", e))
}

/// Unreblog (undo repost) a status on Mastodon.
pub async fn unreblog_status(
    http_client: &reqwest::Client,
    session: &MastodonSession,
    status_id: &str,
) -> Result<MastodonStatus, String> {
    let resp = rate_limit_request(
        http_client
            .post(format!("{}/api/v1/statuses/{}/unreblog", session.instance, status_id))
            .bearer_auth(&session.access_token),
    ).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Unreblog failed ({}): {}", status, body));
    }

    resp.json().await
        .map_err(|e| format!("Invalid unreblog response: {}", e))
}

/// Follow a hashtag on Mastodon (POST /api/v1/tags/:hashtag/follow).
pub async fn follow_hashtag(
    http_client: &reqwest::Client,
    session: &MastodonSession,
    hashtag: &str,
) -> Result<serde_json::Value, String> {
    let tag = hashtag.trim_start_matches('#');
    let resp = http_client
        .post(format!("{}/api/v1/tags/{}/follow", session.instance, tag))
        .bearer_auth(&session.access_token)
        .send()
        .await
        .map_err(|e| format!("Follow hashtag failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Follow hashtag failed ({}): {}", status, body));
    }

    resp.json().await
        .map_err(|e| format!("Invalid hashtag follow response: {}", e))
}

/// Unfollow a hashtag on Mastodon (POST /api/v1/tags/:hashtag/unfollow).
pub async fn unfollow_hashtag(
    http_client: &reqwest::Client,
    session: &MastodonSession,
    hashtag: &str,
) -> Result<serde_json::Value, String> {
    let tag = hashtag.trim_start_matches('#');
    let resp = http_client
        .post(format!("{}/api/v1/tags/{}/unfollow", session.instance, tag))
        .bearer_auth(&session.access_token)
        .send()
        .await
        .map_err(|e| format!("Unfollow hashtag failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Unfollow hashtag failed ({}): {}", status, body));
    }

    resp.json().await
        .map_err(|e| format!("Invalid hashtag unfollow response: {}", e))
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 6: ACCOUNT LOOKUP & SOCIAL GRAPH
// Finding accounts and listing follows/followers.
// ═══════════════════════════════════════════════════════════════════

/// Look up a Mastodon account by `acct` string (e.g., `user@instance.social`).
pub async fn lookup_account(
    http_client: &reqwest::Client,
    session: &MastodonSession,
    acct: &str,
) -> Result<MastodonAccount, String> {
    let url = format!(
        "{}/api/v1/accounts/lookup?acct={}",
        session.instance,
        urlencoding::encode(acct),
    );

    let resp = http_client
        .get(&url)
        .bearer_auth(&session.access_token)
        .send()
        .await
        .map_err(|e| format!("Account lookup failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Account lookup failed ({}): {}", status, body));
    }

    resp.json().await
        .map_err(|e| format!("Invalid account response: {}", e))
}

/// Get a Mastodon account by its numeric ID.
pub async fn get_account(
    http_client: &reqwest::Client,
    session: &MastodonSession,
    account_id: &str,
) -> Result<MastodonAccount, String> {
    let resp = http_client
        .get(format!("{}/api/v1/accounts/{}", session.instance, account_id))
        .bearer_auth(&session.access_token)
        .send()
        .await
        .map_err(|e| format!("Account fetch failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Account fetch failed ({}): {}", status, body));
    }

    resp.json().await
        .map_err(|e| format!("Invalid account response: {}", e))
}

// ──────────────────────────────────────────────────
// Following / Followers Lists
// ──────────────────────────────────────────────────

/// Fetch the accounts the authenticated user follows.
pub async fn get_following_list(
    http_client: &reqwest::Client,
    session: &MastodonSession,
    limit: Option<i32>,
) -> Result<Vec<MastodonAccount>, String> {
    let url = format!(
        "{}/api/v1/accounts/{}/following?limit={}",
        session.instance,
        session.account_id,
        limit.unwrap_or(80).min(80),
    );

    let resp = http_client
        .get(&url)
        .bearer_auth(&session.access_token)
        .send()
        .await
        .map_err(|e| format!("Following list fetch failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Following list fetch failed ({}): {}", status, body));
    }

    resp.json().await
        .map_err(|e| format!("Invalid following list response: {}", e))
}

/// Fetch the accounts that follow the authenticated user.
pub async fn get_followers_list(
    http_client: &reqwest::Client,
    session: &MastodonSession,
    limit: Option<i32>,
) -> Result<Vec<MastodonAccount>, String> {
    let url = format!(
        "{}/api/v1/accounts/{}/followers?limit={}",
        session.instance,
        session.account_id,
        limit.unwrap_or(80).min(80),
    );

    let resp = http_client
        .get(&url)
        .bearer_auth(&session.access_token)
        .send()
        .await
        .map_err(|e| format!("Followers list fetch failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Followers list fetch failed ({}): {}", status, body));
    }

    resp.json().await
        .map_err(|e| format!("Invalid followers list response: {}", e))
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 7: UTILITIES & FORMAT CONVERSION
// Instance URL normalization, Mastodon→Bluesky format adapters,
// Mastodon→BytescapeItem converters, and rate limiting.
// ═══════════════════════════════════════════════════════════════════

/// Normalize an instance URL (enforce https:// prefix, strip trailing slash).
/// Rejects plaintext HTTP to prevent credential leakage.
pub fn normalize_instance(instance: &str) -> String {
    let trimmed = instance.trim().trim_end_matches('/');
    if trimmed.starts_with("https://") {
        trimmed.to_string()
    } else if trimmed.starts_with("http://") {
        // Upgrade http to https — never send auth tokens in plaintext
        format!("https://{}", &trimmed["http://".len()..])
    } else {
        format!("https://{}", trimmed)
    }
}

/// Convert a Mastodon status to a BytescapeItem for unified feed display.
pub fn status_to_bytescape_item(instance: &str, status: &MastodonStatus) -> BytescapeItem {
    // If this is a reblog, use the inner status for content
    let effective = status.reblog.as_ref().map(|b| b.as_ref()).unwrap_or(status);

    let text_content = strip_html(&effective.content);

    // MEMORY-CRITICAL: Prefer preview_url (small variant, ~200-400px) over url
    // (full resolution). WebKitGTK decodes every image to RGBA bitmap in memory.
    let media_urls: Vec<String> = effective.media_attachments.iter()
        .map(|m| m.preview_url.clone().unwrap_or_else(|| m.url.clone()))
        .collect();

    let has_image = effective.media_attachments.iter()
        .any(|m| m.media_type == "image" || m.media_type == "gifv");
    let has_video = effective.media_attachments.iter()
        .any(|m| m.media_type == "video" || m.media_type == "audio");

    let item_type = if has_image || has_video {
        BGallery
    } else {
        BStatus
    };

    let preview_img = effective.media_attachments.iter()
        .find(|m| m.media_type == "image" || m.media_type == "gifv")
        .and_then(|m| m.preview_url.clone().or_else(|| Some(m.url.clone())));

    let timestamp = chrono::DateTime::parse_from_rfc3339(&effective.created_at)
        .map(|dt| dt.timestamp())
        .unwrap_or(0);

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let instance_host = instance.replace("https://", "").replace("http://", "");
    // Build display handle — include instance domain for local accounts
    let author_handle = if effective.account.display_name.is_empty() {
        if effective.account.acct.contains('@') {
            effective.account.acct.clone()
        } else {
            format!("{}@{}", effective.account.acct, instance_host)
        }
    } else {
        if effective.account.acct.contains('@') {
            format!("{} (@{})", effective.account.display_name, effective.account.acct)
        } else {
            format!("{} (@{}@{})", effective.account.display_name, effective.account.acct, instance_host)
        }
    };

    // Build proper acct: URI for the author (handles both local and remote accounts)
    let author_acct = if effective.account.acct.contains('@') {
        format!("acct:{}", effective.account.acct)
    } else {
        format!("acct:{}@{}", effective.account.acct, instance_host)
    };

    BytescapeItem {
        id: format!("https://{}/statuses/{}", instance_host, status.id),
        original_timestamp: timestamp,
        content: text_content,
        media_paths: media_urls,
        source: "mastodon".to_string(),
        path: format!("mastodon/{}/{}", instance_host, status.id),
        imported_at: now_secs,
        title: effective.spoiler_text.clone().filter(|s| !s.is_empty()),
        original_id: Some(status.id.clone()),
        author_did: Some(author_acct),
        author_handle: Some(author_handle),
        author_avatar: Some(effective.account.avatar.clone()),
        tagged_people: vec![],
        location: None,
        is_liked: effective.favourited,
        like_count: Some(effective.favourites_count),
        reply_count: Some(effective.replies_count),
        repost_count: Some(effective.reblogs_count),
        quote_count: None,
        item_type,
        url: effective.url.clone().or(Some(effective.uri.clone())),
        link_url: None,
        content_html: Some(effective.content.clone()),
        preview_img,
        media_blobs: vec![],
        thumbnail_blobs: vec![],
        media_mimes: vec![],
        syndication_status: SyndicationStatus::LocalOnly,
        syndicated_uri: Some(effective.uri.clone()),
        audiences: vec![match effective.visibility.as_str() {
            "public" | "unlisted" => Audience::Activitypub,
            _ => Audience::Local,
        }],
        syndicated_uris: Default::default(),
        syndicated_at: Default::default(),
        share_history: vec![],
        reposted_by_did: if status.reblog.is_some() {
            Some(if status.account.acct.contains('@') {
                format!("acct:{}", status.account.acct)
            } else {
                format!("acct:{}@{}", status.account.acct, instance_host)
            })
        } else {
            None
        },
        reposted_by_handle: if status.reblog.is_some() {
            Some(status.account.display_name.clone())
        } else {
            None
        },
        reply_parent_uri: effective.in_reply_to_id.clone(),
        reply_parent_handle: None,
        collab_doc_id: None,
    }
}

/// Convert a Mastodon status to a VaultMessage for DM display.
pub fn status_to_vault_message(
    instance: &str,
    status: &MastodonStatus,
    owner_acct: &str,
) -> VaultMessage {
    let instance_host = instance.replace("https://", "").replace("http://", "");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();

    let sender_acct = if status.account.acct.contains('@') {
        format!("acct:{}", status.account.acct)
    } else {
        format!("acct:{}@{}", status.account.acct, instance_host)
    };
    let owner_acct_uri = if owner_acct.contains('@') {
        format!("acct:{}", owner_acct)
    } else {
        format!("acct:{}@{}", owner_acct, instance_host)
    };

    VaultMessage {
        id: format!("https://{}/statuses/{}", instance_host, status.id),
        owner_did: Some(owner_acct_uri),
        convo_id: format!("mastodon-dm:{}@{}", status.account.acct, instance_host),
        sender_did: sender_acct,
        sender_handle: if status.account.display_name.is_empty() {
            status.account.acct.clone()
        } else {
            status.account.display_name.clone()
        },
        text: strip_html(&status.content),
        sent_at: status.created_at.clone(),
        source: "mastodon".to_string(),
        imported_at: now.as_secs(),
        peer_did: None,
        peer_handle: None,
    }
}

/// Convert a Mastodon status to a BlueskyImportedPost for unified feed display.
///
/// This adapter lets Mastodon posts flow through the same frontend pipeline as
/// Bluesky posts without requiring frontend changes.
pub fn status_to_bsky_post(instance: &str, status: &MastodonStatus) -> crate::connectors::bluesky::BlueskyImportedPost {
    let effective = status.reblog.as_ref().map(|b| b.as_ref()).unwrap_or(status);

    let text_content = strip_html(&effective.content);
    let instance_host = instance.replace("https://", "").replace("http://", "");

    // MEMORY-CRITICAL: Prefer preview_url (small variant) over full-resolution url.
    // WebKitGTK decodes every <img> to a full RGBA bitmap in process memory.
    let image_urls: Vec<String> = effective.media_attachments.iter()
        .filter(|m| m.media_type == "image" || m.media_type == "gifv")
        .map(|m| m.preview_url.clone().unwrap_or_else(|| m.url.clone()))
        .collect();

    let video_url = effective.media_attachments.iter()
        .find(|m| m.media_type == "video")
        .map(|m| m.url.clone());
    let video_thumbnail = effective.media_attachments.iter()
        .find(|m| m.media_type == "video")
        .and_then(|m| m.preview_url.clone());

    let timestamp = chrono::DateTime::parse_from_rfc3339(&effective.created_at)
        .map(|dt| dt.timestamp())
        .unwrap_or(0);

    // Build display handle — avoid double-appending instance_host when acct
    // already contains an @ (remote accounts, e.g. user@mastodon.social)
    let author_handle = if effective.account.display_name.is_empty() {
        if effective.account.acct.contains('@') {
            effective.account.acct.clone()
        } else {
            format!("{}@{}", effective.account.acct, instance_host)
        }
    } else {
        if effective.account.acct.contains('@') {
            format!("{} (@{})", effective.account.display_name, effective.account.acct)
        } else {
            format!("{} (@{}@{})", effective.account.display_name, effective.account.acct, instance_host)
        }
    };

    let author_acct = if effective.account.acct.contains('@') {
        format!("acct:{}", effective.account.acct)
    } else {
        format!("acct:{}@{}", effective.account.acct, instance_host)
    };

    crate::connectors::bluesky::BlueskyImportedPost {
        uri: format!("https://{}/statuses/{}", instance_host, status.id),
        cid: status.id.clone(),
        text: text_content,
        created_at: timestamp,
        image_urls,
        image_local_paths: vec![],
        author_did: author_acct,
        author_handle,
        author_avatar: Some(effective.account.avatar.clone()),
        reply_count: Some(effective.replies_count),
        repost_count: Some(effective.reblogs_count),
        like_count: Some(effective.favourites_count),
        video_url,
        video_thumbnail,
        external_url: effective.url.clone(),
        external_title: None,
        external_description: None,
        external_thumb: None,
        reposted_by: if status.reblog.is_some() {
            Some(if status.account.acct.contains('@') {
                format!("acct:{}", status.account.acct)
            } else {
                format!("acct:{}@{}", status.account.acct, instance_host)
            })
        } else {
            None
        },
        reposted_by_handle: if status.reblog.is_some() {
            Some(if status.account.display_name.is_empty() {
                status.account.acct.clone()
            } else {
                status.account.display_name.clone()
            })
        } else {
            None
        },
        reply_parent_handle: None,
        reply_parent_uri: effective.in_reply_to_id.clone(),
        quoted_post: None,
        viewer_like: if effective.favourited { Some("liked".to_string()) } else { None },
        viewer_repost: if effective.reblogged { Some("reblogged".to_string()) } else { None },
    }
}

/// Convert a MastodonAccount to a ProfileView for the friends list.
pub fn account_to_profile_view(instance: &str, account: &MastodonAccount) -> crate::connectors::bluesky::ProfileView {
    let instance_host = instance.replace("https://", "").replace("http://", "");
    let acct_uri = if account.acct.contains('@') {
        format!("acct:{}", account.acct)
    } else {
        format!("acct:{}@{}", account.acct, instance_host)
    };

    crate::connectors::bluesky::ProfileView {
        did: acct_uri,
        handle: if account.acct.contains('@') {
            account.acct.clone()
        } else {
            format!("{}@{}", account.acct, instance_host)
        },
        display_name: if account.display_name.is_empty() {
            None
        } else {
            Some(account.display_name.clone())
        },
        avatar: if account.avatar.is_empty() { None } else { Some(account.avatar.clone()) },
        description: if account.note.is_empty() { None } else { Some(strip_html(&account.note)) },
    }
}

/// Restore a Mastodon session directly from the keychain, without the
/// in-memory session cache (used by cross-module code like `get_timeline`).
pub fn get_session_from_keychain(
    keychain: &crate::keychain_encrypted::EncryptedKeychain,
) -> Result<MastodonSession, String> {
    let json_str = keychain.get_mastodon_session()
        .map_err(|e| format!("Keychain error: {}", e))?
        .ok_or("No Mastodon session stored")?;
    let blob: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| format!("Parse error: {}", e))?;
    let session: MastodonSession = serde_json::from_value(
        blob.get("session").cloned().ok_or("Missing session key in Mastodon keychain blob")?
    ).map_err(|e| format!("Deserialize error: {}", e))?;
    Ok(session)
}

/// Execute a request builder with rate-limit retry.
///
/// If the response is 429 Too Many Requests, wait for the duration
/// specified in `Retry-After` (or a 5s default) and retry once.
/// Also checks `X-RateLimit-Remaining` and logs when approaching limits.
pub async fn rate_limit_request(
    request_builder: reqwest::RequestBuilder,
) -> Result<reqwest::Response, String> {
    let cloned = request_builder.try_clone()
        .ok_or("Could not clone request for retry")?;

    let resp = request_builder.send().await
        .map_err(|e| format!("Request failed: {}", e))?;

    // Check rate limit headers
    if let Some(remaining) = resp.headers().get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u32>().ok())
    {
        if remaining < 10 {
            #[cfg(debug_assertions)]
            println!("[mastodon] Rate limit remaining: {}", remaining);
        }
    }

    if resp.status().as_u16() == 429 {
        let retry_after = resp.headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(5);

        let wait = retry_after.min(30); // Cap at 30s
        #[cfg(debug_assertions)]
        println!("[mastodon] Rate limited, retrying after {}s", wait);

        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;

        let retry_resp = cloned.send().await
            .map_err(|e| format!("Retry request failed: {}", e))?;

        if retry_resp.status().as_u16() == 429 {
            return Err("Rate limited by Mastodon instance — try again later".to_string());
        }

        return Ok(retry_resp);
    }

    Ok(resp)
}

/// Strip HTML tags from Mastodon content, converting `<br>` to newlines
/// and decoding common HTML entities.
pub fn strip_html(html: &str) -> String {
    let result = html
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n")
        .replace("</li>", "\n");

    // Strip remaining tags
    let mut output = String::with_capacity(result.len());
    let mut in_tag = false;
    for ch in result.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }

    // Decode entities
    output = output
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'");

    output.trim().to_string()
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 8: TESTS
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html_basic() {
        assert_eq!(strip_html("<p>Hello world</p>"), "Hello world");
    }

    #[test]
    fn test_strip_html_br_tags() {
        assert_eq!(strip_html("Line 1<br>Line 2"), "Line 1\nLine 2");
        assert_eq!(strip_html("Line 1<br/>Line 2"), "Line 1\nLine 2");
    }

    #[test]
    fn test_strip_html_entities() {
        assert_eq!(strip_html("A &amp; B &lt;3"), "A & B <3");
    }

    #[test]
    fn test_strip_html_links() {
        assert_eq!(
            strip_html(r#"<p>Check <a href="https://example.com">this</a></p>"#),
            "Check this"
        );
    }

    #[test]
    fn test_normalize_instance() {
        assert_eq!(normalize_instance("mastodon.social"), "https://mastodon.social");
        assert_eq!(normalize_instance("https://mastodon.social/"), "https://mastodon.social");
        assert_eq!(normalize_instance("https://mastodon.social"), "https://mastodon.social");
    }

    #[test]
    fn test_status_to_bytescape_item_text_only() {
        let status = MastodonStatus {
            id: "123".into(),
            uri: "https://mastodon.social/users/alice/statuses/123".into(),
            url: Some("https://mastodon.social/@alice/123".into()),
            content: "<p>Hello fediverse!</p>".into(),
            created_at: "2025-01-15T12:00:00Z".into(),
            visibility: "public".into(),
            account: MastodonAccount {
                id: "1".into(),
                acct: "alice".into(),
                display_name: "Alice".into(),
                url: "https://mastodon.social/@alice".into(),
                avatar: "https://mastodon.social/avatar.png".into(),
                header: String::new(),
                note: String::new(),
                followers_count: 100,
                following_count: 50,
                statuses_count: 42,
            },
            media_attachments: vec![],
            in_reply_to_id: None,
            reblog: None,
            favourites_count: 5,
            replies_count: 2,
            reblogs_count: 1,
            favourited: false,
            reblogged: false,
            sensitive: false,
            spoiler_text: None,
        };

        let item = status_to_bytescape_item("https://mastodon.social", &status);
        assert_eq!(item.source, "mastodon");
        assert_eq!(item.content, "Hello fediverse!");
        assert!(matches!(item.item_type, BStatus));
        assert!(item.media_paths.is_empty());
    }

    #[test]
    fn test_status_to_bytescape_item_with_media() {
        let status = MastodonStatus {
            id: "456".into(),
            uri: "https://mastodon.social/users/bob/statuses/456".into(),
            url: None,
            content: "<p>Photo!</p>".into(),
            created_at: "2025-01-15T12:00:00Z".into(),
            visibility: "unlisted".into(),
            account: MastodonAccount {
                id: "2".into(),
                acct: "bob@example.org".into(),
                display_name: "Bob".into(),
                url: "https://example.org/@bob".into(),
                avatar: "https://example.org/bob.png".into(),
                header: String::new(),
                note: String::new(),
                followers_count: 0,
                following_count: 0,
                statuses_count: 0,
            },
            media_attachments: vec![MastodonMediaAttachment {
                id: "m1".into(),
                media_type: "image".into(),
                url: "https://mastodon.social/media/photo.jpg".into(),
                preview_url: Some("https://mastodon.social/media/photo_s.jpg".into()),
                description: Some("A photo".into()),
            }],
            in_reply_to_id: None,
            reblog: None,
            favourites_count: 0,
            replies_count: 0,
            reblogs_count: 0,
            favourited: false,
            reblogged: false,
            sensitive: false,
            spoiler_text: None,
        };

        let item = status_to_bytescape_item("https://mastodon.social", &status);
        assert!(matches!(item.item_type, BGallery));
        assert_eq!(item.media_paths.len(), 1);
        assert_eq!(item.preview_img, Some("https://mastodon.social/media/photo_s.jpg".into()));
    }

    #[test]
    fn test_status_to_vault_message() {
        let status = MastodonStatus {
            id: "789".into(),
            uri: "https://mastodon.social/users/charlie/statuses/789".into(),
            url: None,
            content: "<p>Hey there!</p>".into(),
            created_at: "2025-01-15T14:00:00Z".into(),
            visibility: "direct".into(),
            account: MastodonAccount {
                id: "3".into(),
                acct: "charlie@mastodon.social".into(),
                display_name: "Charlie".into(),
                url: "https://mastodon.social/@charlie".into(),
                avatar: String::new(),
                header: String::new(),
                note: String::new(),
                followers_count: 0,
                following_count: 0,
                statuses_count: 0,
            },
            media_attachments: vec![],
            in_reply_to_id: None,
            reblog: None,
            favourites_count: 0,
            replies_count: 0,
            reblogs_count: 0,
            favourited: false,
            reblogged: false,
            sensitive: false,
            spoiler_text: None,
        };

        let msg = status_to_vault_message("https://mastodon.social", &status, "me@mastodon.social");
        assert_eq!(msg.source, "mastodon");
        assert_eq!(msg.text, "Hey there!");
        assert!(msg.convo_id.contains("charlie@mastodon.social"));
    }

    #[test]
    fn test_mastodon_session_serialization() {
        let session = MastodonSession {
            instance: "https://mastodon.social".into(),
            access_token: "token_123".into(),
            acct: "alice".into(),
            display_name: "Alice".into(),
            avatar: "https://mastodon.social/avatar.png".into(),
            account_id: "1".into(),
        };
        let json = serde_json::to_string(&session).unwrap();
        let restored: MastodonSession = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.acct, "alice");
    }

    #[test]
    fn test_mastodon_app_serialization() {
        let app = MastodonApp {
            instance: "https://mastodon.social".into(),
            client_id: "client_abc".into(),
            client_secret: "secret_xyz".into(),
        };
        let json = serde_json::to_string(&app).unwrap();
        let restored: MastodonApp = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.client_id, "client_abc");
    }
}
