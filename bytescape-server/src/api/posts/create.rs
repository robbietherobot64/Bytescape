use axum::{extract::State, http::HeaderMap, Json};
use serde::{Deserialize, Serialize};

use crate::db;
use crate::error::AppError;
use crate::oauth;
use crate::AppState;

use super::CrossPostResult;
use super::extract_user_id;

// ---------------------------------------------------------------------------
// POST /api/posts/create
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct CreatePostRequest {
    content: String,
    visibility: String,
    platforms: Vec<String>,
    media_urls: Vec<String>,
    link_url: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct CreatePostResponse {
    ok: bool,
    post: PostResult,
    cross_posts: Vec<CrossPostResult>,
    errors: Vec<String>,
}

#[derive(Serialize)]
struct PostResult {
    id: String,
    url: Option<String>,
    content: String,
    visibility: String,
    created_at: String,
}

pub(super) async fn create_post_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreatePostRequest>,
) -> Result<Json<CreatePostResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    // Validate content length (max 300 chars — Bluesky's limit)
    if req.content.len() > 300 {
        return Err(AppError::BadRequest(
            "Content exceeds 300 character limit".into(),
        ));
    }

    if req.content.trim().is_empty() && req.media_urls.is_empty() {
        return Err(AppError::BadRequest(
            "Post must have text or media".into(),
        ));
    }

    // Validate visibility
    let visibility = match req.visibility.as_str() {
        "public" | "followers" | "friends" => req.visibility.clone(),
        _ => return Err(AppError::BadRequest("Invalid visibility. Must be: public, followers, or friends".into())),
    };

    let http = reqwest::Client::new();

    // Get user info for the author display
    let user = db::users::UserRow::find_by_id(&state.db, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    // Build content JSON
    let content_json = serde_json::json!({
        "text": req.content,
        "mentions": [],
        "hashtags": [],
        "links": [],
        "media_urls": req.media_urls,
    });

    // Store post in Bytescape DB
    let post = db::posts::PostRow::create(
        &state.db,
        user_id,
        "bytescape",
        None,
        None,
        &content_json,
        "short",
        None,
        None,
        None,
        chrono::Utc::now(),
        false,
        None,
        &visibility,
    )
    .await?;

    // ── Cross-post/error accumulators (available to all identity paths) ──
    let mut cross_posts: Vec<CrossPostResult> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    // ── Load posting identities ──
    let posting_identities = db::posting_identities::PostingIdentityRow::find_by_user(&state.db, user_id)
        .await
        .unwrap_or_default();
    let at_identity = posting_identities.iter().find(|pi| pi.protocol == "atproto");

    // ── AT Protocol path: PDS write or external Bluesky post ──
    let at_is_bytescape = at_identity.map_or(true, |pi| pi.identity_type == "bytescape");

    if at_is_bytescape {
        // identity_type = 'bytescape' → existing PDS write path
        // createRecord triggers firehose → AP bridge → AP delivery
        if visibility == "public" {
            let pds_account = sqlx::query_as::<_, (String,)>(
                "SELECT did FROM pds_accounts WHERE user_id = $1"
            )
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

            if let Some((did,)) = pds_account {
                let post_created = chrono::Utc::now();
                let record = serde_json::json!({
                    "$type": "app.bsky.feed.post",
                    "text": &req.content,
                    "createdAt": post_created.to_rfc3339(),
                });

                let rkey = format!("{}", post.id);
                match crate::pds::create_record_internal(
                    &state.db, &did, "app.bsky.feed.post", Some(&rkey), &record, Some(&state.config.domain),
                ).await {
                    Ok((uri, cid, _rev)) => {
                        tracing::info!("PDS createRecord: {} (cid={})", uri, cid);
                        // Self-index for feeds: our PDS writes do not loop
                        // back through the network firehose (same reason as
                        // story reshares self-index).
                        if let Err(e) = crate::appview::consumer::upsert_appview_post(
                            &state.db,
                            &uri,
                            &cid,
                            &did,
                            &req.content,
                            None,
                            None,
                            post_created,
                        )
                        .await
                        {
                            tracing::warn!("feed self-index failed for {}: {}", uri, e);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("PDS createRecord failed (post still stored in DB): {}", e);
                    }
                }
            }
        }
    } else if req.platforms.iter().any(|p| p == "bluesky") {
        // identity_type = 'linked' → post to external PDS via linked account's OAuth+DPoP
        // Skip Bytescape PDS write. Store cross_post record.
        tracing::info!("[bluesky_linked] AT identity is linked, posting to external PDS");
        if let Some(linked_id) = at_identity.unwrap().linked_account_id {
            if let Ok(Some(acct)) = db::accounts::LinkedAccountRow::find_by_id(&state.db, linked_id).await {
                let pds = crate::oauth::bluesky::get_pds_url(&acct.metadata);
                let did = acct.metadata["did"].as_str().unwrap_or(&acct.external_id);
                tracing::info!("[bluesky_linked] posting to external PDS: did={}, pds={}", did, pds);

                // A4-T15: Validate token freshness before posting
                let access_token = if let Some(expires_at) = acct.token_expires_at {
                    if chrono::Utc::now() >= expires_at {
                        // Token expired — attempt refresh
                        tracing::info!("[bluesky_linked] Token expired at {}, attempting refresh", expires_at);
                        match oauth::bluesky::refresh_session(&http, &state.db, &acct).await {
                            Ok(new_token) => {
                                tracing::info!("[bluesky_linked] Token refreshed successfully");
                                Some(new_token)
                            }
                            Err(e) => {
                                tracing::error!("[bluesky_linked] Token refresh failed: {}", e);
                                let msg = if acct.refresh_token.is_none() {
                                    "Bluesky token expired and no refresh token available. Please re-link your account.".to_string()
                                } else {
                                    format!("Bluesky token refresh failed: {}. Please re-link your account.", e)
                                };
                                errors.push(msg);
                                cross_posts.push(CrossPostResult {
                                    platform: "bluesky".into(),
                                    ok: false,
                                    url: None,
                                    error: Some("Token expired, re-link required".into()),
                                });
                                None
                            }
                        }
                    } else {
                        Some(acct.access_token.clone())
                    }
                } else {
                    // No token expiry stored — may be pre-DPoP account without refresh token
                    if acct.refresh_token.is_none() {
                        tracing::warn!("[bluesky_linked] No token_expires_at and no refresh_token — legacy account");
                    }
                    Some(acct.access_token.clone())
                };

                if let Some(access_token) = access_token {
                let create_result = if let Some(dpop_hex) = &acct.dpop_private_key {
                    if !dpop_hex.is_empty() {
                        match hex::decode(dpop_hex) {
                            Ok(dpop_seed) => {
                                match oauth::bluesky::BlueskyApiClient::new(&access_token, &dpop_seed, &pds) {
                                    Ok(client) => client.create_record(did, &req.content, &req.media_urls, req.link_url.as_deref()).await,
                                    Err(e) => Err(format!("Failed to create Bluesky client: {}", e)),
                                }
                            }
                            Err(e) => Err(format!("Failed to decode DPoP key: {}", e)),
                        }
                    } else {
                        oauth::bluesky::create_post(&state.db, &acct, &req.content, &req.media_urls, req.link_url.as_deref()).await
                    }
                } else {
                    oauth::bluesky::create_post(&state.db, &acct, &req.content, &req.media_urls, req.link_url.as_deref()).await
                };

                match create_result {
                    Ok(uri) => {
                        let external_handle = format!("@{}", acct.external_username);
                        if let Err(e) = db::cross_posts::CrossPostRow::create(&state.db, post.id, user_id, "bluesky", &uri, Some(&uri), Some(&external_handle)).await {
                            tracing::error!("[bluesky_linked] Failed to store cross_post: {}", e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("[bluesky_linked] External PDS post failed: {}", e);
                    }
                }
                } // end if let Some(access_token)
            } else {
                tracing::warn!("[bluesky_linked] Linked account {} not found", linked_id);
            }
        }
    }

    // ── Load linked accounts (used for cross-posting) ──
    let accounts = db::accounts::LinkedAccountRow::find_by_user(&state.db, user_id)
        .await
        .map_err(AppError::Database)?;

    // AP federation delivery removed — Sprint 1 (AP subtraction)

    // ── Cross-post to external platforms (non-authoritative accounts only) ──

    // Cross-post to Bluesky
    // Always cross-post to linked Bluesky accounts (the PDS write goes to Bytescape's PDS,
    // not the user's external Bluesky account — cross-post bridges that gap)
    if req.platforms.iter().any(|p| p == "bluesky") {
        tracing::info!("[bluesky_crosspost] attempting Bluesky cross-post");
        if let Some(acct) = accounts.iter().find(|a| a.provider == "bluesky") {
            let pds = crate::oauth::bluesky::get_pds_url(&acct.metadata);
            let did = acct.metadata["did"]
                .as_str()
                .unwrap_or(&acct.external_id);
            tracing::info!("[bluesky_crosspost] found account: did={}, pds={}, external_username={}, dpop_key_present={}", did, pds, acct.external_username, acct.dpop_private_key.is_some());

            let create_result = if let Some(dpop_hex) = &acct.dpop_private_key {
                if !dpop_hex.is_empty() {
                    tracing::info!("[bluesky_crosspost] using DPoP auth");
                    match hex::decode(dpop_hex) {
                        Ok(dpop_seed) => {
                            tracing::info!("[bluesky_crosspost] decoded dpop key ({} bytes), creating client to {}", dpop_seed.len(), pds);
                            match oauth::bluesky::BlueskyApiClient::new(
                                &acct.access_token, &dpop_seed, &pds,
                            ) {
                                Ok(client) => {
                                    tracing::info!("[bluesky_crosspost] client created, calling create_record");
                                    client.create_record(did, &req.content, &req.media_urls, req.link_url.as_deref()).await
                                }
                                Err(e) => Err(format!("Failed to create Bluesky client: {}", e)),
                            }
                        }
                        Err(e) => Err(format!("Failed to decode DPoP key: {}", e)),
                    }
                } else {
                    tracing::info!("[bluesky_crosspost] empty dpop key, falling back to Bearer");
                    oauth::bluesky::create_post(&state.db, &acct, &req.content, &req.media_urls, req.link_url.as_deref()).await
                }
            } else {
                tracing::info!("[bluesky_crosspost] no dpop key, falling back to Bearer");
                oauth::bluesky::create_post(&state.db, &acct, &req.content, &req.media_urls, req.link_url.as_deref()).await
            };

            match create_result {
                Ok(uri) => {
                    let external_handle = format!("@{}", acct.external_username);
                    match db::cross_posts::CrossPostRow::create(
                        &state.db, post.id, user_id, "bluesky", &uri,
                        Some(&uri), Some(&external_handle),
                    ).await {
                        Ok(_) => tracing::info!("[cross_post] Stored bluesky cross_post for post {}", post.id),
                        Err(e) => tracing::error!("[cross_post] Failed to store bluesky cross_post: {}", e),
                    }
                    cross_posts.push(CrossPostResult {
                        platform: "bluesky".into(),
                        ok: true,
                        url: Some(uri),
                        error: None,
                    });
                }
                Err(e) => {
                    let err_msg = format!("Bluesky cross-post failed: {}", e);
                    tracing::error!("[bluesky_crosspost] {}", err_msg);
                    errors.push(err_msg.clone());
                    cross_posts.push(CrossPostResult {
                        platform: "bluesky".into(),
                        ok: false,
                        url: None,
                        error: Some(e),
                    });
                }
            }
        } else {
            errors.push("No Bluesky account linked".into());
            cross_posts.push(CrossPostResult {
                platform: "bluesky".into(),
                ok: false,
                url: None,
                error: Some("No Bluesky account linked".into()),
            });
        }
    }

    // Cross-post to the linked Mastodon account's HOME INSTANCE — M-linked
    // users post through their home instance (design.md Decision 1): the status
    // is written to their instance's REST API as their real home actor. The
    // earlier code only cross-posted to Bluesky, so a post targeting Mastodon
    // silently never reached the home instance (owner bug).
    if req.platforms.iter().any(|p| p == "mastodon") {
        tracing::info!("[mastodon_crosspost] attempting Mastodon home-instance post");
        let http = reqwest::Client::new();
        if let Some(acct) = accounts.iter().find(|a| a.provider == "mastodon") {
            match super::mastodon_home_instance(acct) {
                Err(e) => {
                    let m = format!("Mastodon cross-post failed: {}", e);
                    tracing::error!("[mastodon_crosspost] {}", m);
                    errors.push(m.clone());
                    cross_posts.push(CrossPostResult { platform: "mastodon".into(), ok: false, url: None, error: Some(m) });
                }
                Ok(instance) => {
                    // Status text = content + link (the home instance auto-links).
                    let status_text = match &req.link_url {
                        Some(l) if !l.trim().is_empty() => format!("{}\n\n{}", req.content, l),
                        _ => req.content.clone(),
                    };
                    match oauth::mastodon::post_status(&http, &instance, &acct.access_token, &status_text, "public").await {
                        Ok(v) => {
                            let status_id = v["id"].as_str().unwrap_or("").to_string();
                            let status_url = v["url"].as_str().unwrap_or("").to_string();
                            let external_handle = format!("@{}", acct.external_username);
                            if !status_id.is_empty() {
                                if let Err(e) = db::cross_posts::CrossPostRow::create(
                                    &state.db, post.id, user_id, "mastodon", &status_id,
                                    Some(&status_url), Some(&external_handle),
                                ).await {
                                    tracing::error!("[cross_post] Failed to store mastodon cross_post: {}", e);
                                }
                            }
                            cross_posts.push(CrossPostResult { platform: "mastodon".into(), ok: true, url: Some(status_url), error: None });
                        }
                        Err(e) => {
                            let m = format!("Mastodon cross-post failed: {}", e);
                            tracing::error!("[mastodon_crosspost] {}", m);
                            errors.push(m.clone());
                            cross_posts.push(CrossPostResult { platform: "mastodon".into(), ok: false, url: None, error: Some(e.to_string()) });
                        }
                    }
                }
            }
        } else {
            errors.push("No Mastodon account linked".into());
            cross_posts.push(CrossPostResult { platform: "mastodon".into(), ok: false, url: None, error: Some("No Mastodon account linked".into()) });
        }
    }

    let post_url = Some(format!("/@{}/{}", user.username, post.id));

    Ok(Json(CreatePostResponse {
        ok: true,
        post: PostResult {
            id: post.id.to_string(),
            url: post_url,
            content: req.content,
            visibility,
            created_at: post.created_at.to_rfc3339(),
        },
        cross_posts,
        errors,
    }))
}
