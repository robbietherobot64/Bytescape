use axum::{extract::State, http::HeaderMap, Json};
use serde::{Deserialize, Serialize};

use crate::db;
use crate::error::AppError;
use crate::oauth;
use crate::AppState;

use super::CrossPostResult;
use super::extract_user_id;

// ---------------------------------------------------------------------------
// POST /api/posts/repost — Reshare an imported post as a new Bytescape post
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct RepostRequest {
    post_id: String,
    #[allow(dead_code)]
    source: Option<String>,
    #[allow(dead_code)]
    url: Option<String>,
    comment: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct RepostResponse {
    ok: bool,
    post_id: String,
    message: String,
}

/// Reshare an `appview_posts` original — regular AT feed posts
/// (`at://…app.bsky.feed.post`) and AP/group posts (`https://…`). The story
/// branch above only covers stories/statuses; everything else in
/// `appview_posts` reshares here as a quote of the original, mirroring the
/// story path (PDS record + appview upsert + the original's repost count).
async fn reshare_appview_original(
    state: &AppState,
    user_id: uuid::Uuid,
    req: &RepostRequest,
) -> Result<Json<RepostResponse>, AppError> {
    let (author_did, original_text): (String, Option<String>) = sqlx::query_as(
        "SELECT author_did, text FROM appview_posts WHERE uri = $1",
    )
    .bind(&req.post_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound("Post not found".into()))?;
    let _ = author_did;

    let original_text = original_text.unwrap_or_default();
    let user_comment = req.comment.as_deref().unwrap_or("").trim();
    let repost_text = if user_comment.is_empty() {
        if original_text.is_empty() {
            "Reshared".to_string()
        } else {
            format!("Reshared\n\n{}", original_text)
        }
    } else if original_text.is_empty() {
        user_comment.to_string()
    } else {
        format!("{}\n\n{}", user_comment, original_text)
    };
    let repost_text = cap_reshare_text(repost_text);

    // Write a real AT post quoting the original (mirrors the story path).
    let id = crate::api::stories::at_identity_for(&state.db, user_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("no AT identity linked to this account".into()))?;
    let did = match &id {
        crate::api::stories::AtId::Native(d) => d.clone(),
        crate::api::stories::AtId::Linked(a) => a.external_id.clone(),
    };
    let now = chrono::Utc::now();
    let record = serde_json::json!({
        "$type": "app.bsky.feed.post",
        "text": repost_text,
        "createdAt": now.to_rfc3339(),
    });
    let rkey = {
        use rsky_common::tid::Ticker;
        Ticker::new().next(None).0
    };
    crate::api::stories::write_record(&state.db, state, &id, "app.bsky.feed.post", &rkey, &record)
        .await?;
    let uri = format!("at://{}/app.bsky.feed.post/{}", did, rkey);
    let embeds = serde_json::json!([{"type": "quote", "uri": req.post_id}]);
    crate::appview::consumer::upsert_appview_post(
        &state.db,
        &uri,
        &crate::api::stories::record_cid(&record),
        &did,
        &repost_text,
        None,
        Some(&embeds),
        now,
    )
    .await?;
    crate::appview::consumer::bump_interaction_count(
        &state.db,
        &req.post_id,
        crate::appview::consumer::CountKind::Repost,
    )
    .await;
    Ok(Json(RepostResponse {
        ok: true,
        message: "Reshared".into(),
        post_id: uri,
    }))
}

pub(super) async fn repost_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RepostRequest>,
) -> Result<Json<RepostResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    // Story/status reshare: quote-post embedding the record so the reshare
    // rides every feed path; the consumer counts it as the record's repost.
    if req.post_id.starts_with("at://") {
        let story_uri = req.post_id.clone();
        let mut row: Option<(String, String)> = sqlx::query_as(
            "SELECT author_did, cid FROM appview_stories WHERE uri = $1",
        )
        .bind(&story_uri)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::Database)?;
        if row.is_none() {
            // Status cards (self-keyed records keyed by author DID) reshare
            // the same way — derive the author from the at:// URI.
            let author = story_uri
                .strip_prefix("at://")
                .and_then(|s| s.split('/').next())
                .unwrap_or("");
            row = sqlx::query_as(
                "SELECT author_did, cid FROM appview_status WHERE author_did = $1",
            )
            .bind(author)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::Database)?;
        }
        let (author_did, story_cid) = match row {
            Some(r) => r,
            // Not a story/status — a regular AT feed post. Fall through to the
            // appview_posts reshare path (covers AT feed posts + AP/group posts).
            None => return reshare_appview_original(&state, user_id, &req).await,
        };
        // Feed queries filter empty-text posts; carry the story's text like
        // post reshares preserve content (fallback label for text-less cards).
        let story_text: Option<String> = sqlx::query_scalar(
            "SELECT COALESCE((SELECT text FROM appview_stories WHERE uri = $1),
                             (SELECT status FROM appview_status WHERE author_did = $2))",
        )
        .bind(&story_uri)
        .bind(&author_did)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::Database)?
        .flatten();
        let post_text = story_text
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| "Reshared a story".to_string());
        let id = crate::api::stories::at_identity_for(&state.db, user_id)
            .await?
            .ok_or_else(|| AppError::BadRequest("no AT identity linked to this account".into()))?;
        let did = match &id {
            crate::api::stories::AtId::Native(d) => d.clone(),
            crate::api::stories::AtId::Linked(a) => a.external_id.clone(),
        };
        let now = chrono::Utc::now();
        let record = serde_json::json!({
            "$type": "app.bsky.feed.post",
            "text": post_text,
            "createdAt": now.to_rfc3339(),
            "embed": {
                "$type": "app.bsky.embed.record",
                "record": {"uri": story_uri, "cid": story_cid}
            },
        });
        let rkey = {
            use rsky_common::tid::Ticker;
            Ticker::new().next(None).0
        };
        crate::api::stories::write_record(
            &state.db, &state, &id, "app.bsky.feed.post", &rkey, &record,
        )
        .await?;
        let uri = format!("at://{}/app.bsky.feed.post/{}", did, rkey);
        let embeds = serde_json::json!([{"type": "quote", "uri": story_uri}]);
        crate::appview::consumer::upsert_appview_post(
            &state.db,
            &uri,
            &crate::api::stories::record_cid(&record),
            &did,
            &post_text,
            None,
            Some(&embeds),
            now,
        )
        .await?;
        crate::appview::consumer::bump_interaction_count(
            &state.db,
            &story_uri,
            crate::appview::consumer::CountKind::Repost,
        )
        .await;
        let _ = author_did;
        return Ok(Json(RepostResponse {
            ok: true,
            message: "Story reshared".into(),
            post_id: uri,
        }));
    }

    // AP/group posts (`https://…`) — reshare via the appview_posts path.
    if req.post_id.starts_with("https://") {
        return reshare_appview_original(&state, user_id, &req).await;
    }

    // Fetch the original post
    let original = sqlx::query_as::<_, db::posts::PostRow>(
        "SELECT * FROM posts WHERE id = $1"
    )
    .bind(uuid::Uuid::parse_str(&req.post_id).map_err(|_| AppError::BadRequest("Invalid post ID".into()))?)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound("Post not found".into()))?;

    // Get user info
    let user = db::users::UserRow::find_by_id(&state.db, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    // Build repost content — preserve original content and media
    let original_content = original.content.clone();
    let original_source = original.source.clone();
    let original_date = original.created_at.format("%B %d, %Y").to_string();

    // Build optional user comment
    let user_comment = req.comment.as_deref().unwrap_or("").trim();

    // Add attribution header to the reposted content
    let attribution = format!("Originally posted on {} on {}", capitalize(&original_source), original_date);
    let original_text = original_content["text"].as_str().unwrap_or("");

    let repost_text = if user_comment.is_empty() {
        if original_text.is_empty() {
            attribution
        } else {
            format!("{}\n\n{}", attribution, original_text)
        }
    } else {
        if original_text.is_empty() {
            format!("{}\n\n{}", user_comment, attribution)
        } else {
            format!("{}\n\n{}\n\n{}", user_comment, attribution, original_text)
        }
    };

    // Truncate to fit within 300 chars (Bluesky limit)
    let repost_text = cap_reshare_text(repost_text);

    // Build content JSON with original media
    let content_json = serde_json::json!({
        "text": repost_text,
        "mentions": [],
        "hashtags": [],
        "links": [],
        "media_urls": original_content["media_urls"].as_array().cloned().unwrap_or_default(),
    });

    // Create the repost as a new Bytescape post
    let post = db::posts::PostRow::create(
        &state.db,
        user_id,
        "bytescape",
        None,
        None,
        &content_json,
        "short",
        None,
        Some(original.id),
        None,
        chrono::Utc::now(),
        false,
        None,
        "public",
    )
    .await?;

    let http = reqwest::Client::new();

    // ── Cross-post/error accumulators ──
    let mut cross_posts: Vec<CrossPostResult> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    // ── A4-T14d: Load posting identities ──
    let posting_identities = db::posting_identities::PostingIdentityRow::find_by_user(&state.db, user_id)
        .await
        .unwrap_or_default();
    let at_identity = posting_identities.iter().find(|pi| pi.protocol == "atproto");

    // ── A4-T14a: AT Protocol path — PDS write or external Bluesky post ──
    let at_is_bytescape = at_identity.map_or(true, |pi| pi.identity_type == "bytescape");

    if at_is_bytescape {
        // identity_type = 'bytescape' → existing PDS write path
        // createRecord triggers firehose → AP bridge → AP delivery
        let pds_account = sqlx::query_as::<_, (String,)>(
            "SELECT did FROM pds_accounts WHERE user_id = $1"
        )
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();

        if let Some((did,)) = pds_account {
            let record = serde_json::json!({
                "$type": "app.bsky.feed.post",
                "text": &repost_text,
                "createdAt": chrono::Utc::now().to_rfc3339(),
            });

            let rkey = format!("{}", post.id);
            match crate::pds::create_record_internal(
                &state.db, &did, "app.bsky.feed.post", Some(&rkey), &record, Some(&state.config.domain),
            ).await {
                Ok((uri, cid, _rev)) => {
                    tracing::info!("PDS createRecord (repost): {} (cid={})", uri, cid);
                }
                Err(e) => {
                    tracing::warn!("PDS createRecord failed for repost (still stored in DB): {}", e);
                }
            }
        }
    } else if at_identity.is_some() {
        // A4-T14b: identity_type = 'linked' → post to external PDS via linked account's OAuth+DPoP
        tracing::info!("[bluesky_linked_repost] AT identity is linked, posting repost to external PDS");
        if let Some(linked_id) = at_identity.unwrap().linked_account_id {
            if let Ok(Some(acct)) = db::accounts::LinkedAccountRow::find_by_id(&state.db, linked_id).await {
                let pds = crate::oauth::bluesky::get_pds_url(&acct.metadata);
                let did = acct.metadata["did"].as_str().unwrap_or(&acct.external_id);

                // Validate token freshness before posting
                let access_token = if let Some(expires_at) = acct.token_expires_at {
                    if chrono::Utc::now() >= expires_at {
                        tracing::info!("[bluesky_linked_repost] Token expired at {}, attempting refresh", expires_at);
                        match oauth::bluesky::refresh_session(&http, &state.db, &acct).await {
                            Ok(new_token) => {
                                tracing::info!("[bluesky_linked_repost] Token refreshed successfully");
                                Some(new_token)
                            }
                            Err(e) => {
                                tracing::error!("[bluesky_linked_repost] Token refresh failed: {}", e);
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
                    if acct.refresh_token.is_none() {
                        tracing::warn!("[bluesky_linked_repost] No token_expires_at and no refresh_token — legacy account");
                    }
                    Some(acct.access_token.clone())
                };

                if let Some(access_token) = access_token {
                    let media_urls: Vec<String> = content_json["media_urls"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();

                    let create_result = if let Some(dpop_hex) = &acct.dpop_private_key {
                        if !dpop_hex.is_empty() {
                            match hex::decode(dpop_hex) {
                                Ok(dpop_seed) => {
                                    match oauth::bluesky::BlueskyApiClient::new(&access_token, &dpop_seed, &pds) {
                                        Ok(client) => client.create_record(did, &repost_text, &media_urls, None).await,
                                        Err(e) => Err(format!("Failed to create Bluesky client: {}", e)),
                                    }
                                }
                                Err(e) => Err(format!("Failed to decode DPoP key: {}", e)),
                            }
                        } else {
                            oauth::bluesky::create_post(&state.db, &acct, &repost_text, &media_urls, None).await
                        }
                    } else {
                        oauth::bluesky::create_post(&state.db, &acct, &repost_text, &media_urls, None).await
                    };

                    match create_result {
                        Ok(uri) => {
                            let external_handle = format!("@{}", acct.external_username);
                            if let Err(e) = db::cross_posts::CrossPostRow::create(&state.db, post.id, user_id, "bluesky", &uri, Some(&uri), Some(&external_handle)).await {
                                tracing::error!("[bluesky_linked_repost] Failed to store cross_post: {}", e);
                            }
                            cross_posts.push(CrossPostResult {
                                platform: "bluesky".into(),
                                ok: true,
                                url: Some(uri),
                                error: None,
                            });
                        }
                        Err(e) => {
                            tracing::error!("[bluesky_linked_repost] External PDS repost failed: {}", e);
                            errors.push(format!("Bluesky: {}", e));
                            cross_posts.push(CrossPostResult {
                                platform: "bluesky".into(),
                                ok: false,
                                url: None,
                                error: Some(e),
                            });
                        }
                    }
                }
            } else {
                tracing::warn!("[bluesky_linked_repost] Linked account {} not found", linked_id);
            }
        }
    }

    // AP federation repost removed — Sprint 1 (AP subtraction)

    Ok(Json(RepostResponse {
        ok: true,
        post_id: post.id.to_string(),
        message: "Reshared!".to_string(),
    }))
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Cap reshared text at ~300 chars on a CHAR boundary. The previous inline
/// `&text[..297]` byte-slice panicked on multibyte text (e.g. Japanese group
/// posts — "byte index 297 is not a char boundary"), surfacing as HTTP 502 so
/// repost/share silently did nothing (owner bug). Truncating by whole chars
/// never splits a UTF-8 character.
fn cap_reshare_text(text: String) -> String {
    const CAP: usize = 300;
    if text.chars().count() > CAP {
        let mut t: String = text.chars().take(CAP.saturating_sub(3)).collect();
        t.push_str("...");
        t
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::cap_reshare_text;

    #[test]
    fn reshare_cap_never_panics_on_multibyte_text() {
        // Regression for the owner bug: resharing a Japanese group post used to
        // panic (HTTP 502) because the byte slice split a multibyte character.
        let jp = "ん".repeat(400); // 400 chars / 1200 bytes — well past the cap
        let capped = cap_reshare_text(jp);
        assert!(capped.chars().count() <= 300, "capped to <= 300 chars");
        assert!(capped.ends_with("..."), "truncated with ellipsis");
    }

    #[test]
    fn reshare_cap_keeps_short_text_intact() {
        assert_eq!(cap_reshare_text("hello world".to_string()), "hello world");
    }

    #[test]
    fn reshare_cap_truncates_long_ascii() {
        let long = "a".repeat(400);
        let capped = cap_reshare_text(long);
        assert!(capped.chars().count() <= 300);
        assert!(capped.ends_with("..."));
    }
}
