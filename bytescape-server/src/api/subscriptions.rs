//! API — User subscription management.
//! GET    /api/subscriptions        — list all subscriptions
//! POST   /api/subscriptions        — create a subscription
//! PATCH  /api/subscriptions/:id    — update subscription (toggle enabled)
//! DELETE /api/subscriptions/:id    — delete a subscription

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{auth, error::AppError, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/subscriptions", get(list_subscriptions).post(create_subscription))
        .route("/api/subscriptions/bootstrap", post(bootstrap_subscriptions))
        .route("/api/subscriptions/{id}", patch(toggle_subscription).delete(delete_subscription))
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
struct SubscriptionView {
    id: String,
    source: String,
    source_id: String,
    label: String,
    icon: String,
    category: String,
    enabled: bool,
    pinned: bool,
}

#[derive(Deserialize)]
struct CreateSubscriptionReq {
    source: String,
    source_id: String,
    label: String,
    icon: Option<String>,
    category: Option<String>,
}

async fn list_subscriptions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SubscriptionView>>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let rows = crate::db::subscriptions::SubscriptionRow::find_by_user(&state.db, user_id)
        .await
        .map_err(AppError::Database)?;
    // Two-way feed sync with Bluesky: pull new saved feeds, drop ones the user
    // deleted on Bluesky. Runs every load so the sides stay converged. Then
    // push the reconciled set back so Bluesky reflects any Bytescape deletes.
    let _ = crate::api::prefs::reconcile_feeds(&state.db, user_id).await;
    let _ = crate::api::prefs::pull_if_empty(&state.db, user_id).await;
    let rows = crate::db::subscriptions::SubscriptionRow::find_by_user(&state.db, user_id)
        .await
        .map_err(AppError::Database)?;

    let subs: Vec<SubscriptionView> = rows
        .into_iter()
        .map(|r| SubscriptionView {
            id: r.id.to_string(),
            source: r.source,
            source_id: r.source_id,
            label: r.label,
            icon: r.icon,
            category: r.category,
            enabled: r.enabled,
            pinned: r.pinned,
        })
        .collect();

    Ok(Json(subs))
}

async fn create_subscription(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateSubscriptionReq>,
) -> Result<(StatusCode, Json<SubscriptionView>), AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    // Feed sources that look like AT URIs must be real feed generator records
    // (`at://did:…/app.bsky.feed.generator/<rkey>`) — malformed URIs are dead
    // on arrival and just produce an empty Feeds tab.
    if req.category.as_deref() == Some("feeds") && req.source_id.starts_with("at://") {
        let parts: Vec<&str> = req.source_id.trim_start_matches("at://").split('/').collect();
        let valid = parts.len() == 3
            && parts[0].starts_with("did:")
            && parts[1] == "app.bsky.feed.generator"
            && !parts[2].is_empty();
        if !valid {
            return Err(AppError::BadRequest(
                "Not a feed generator URI (expected at://did:…/app.bsky.feed.generator/<name>)".into(),
            ));
        }
    }

    let row = crate::db::subscriptions::SubscriptionRow::create(
        &state.db,
        user_id,
        &req.source,
        &req.source_id,
        &req.label,
        req.icon.as_deref().unwrap_or("#"),
        req.category.as_deref().unwrap_or("hashtags"),
    )
    .await
    .map_err(AppError::Database)?;

    crate::api::prefs::push_async(state.db.clone(), user_id);

    Ok((
        StatusCode::CREATED,
        Json(SubscriptionView {
            id: row.id.to_string(),
            source: row.source,
            source_id: row.source_id,
            label: row.label,
            icon: row.icon,
            category: row.category,
            enabled: row.enabled,
            pinned: row.pinned,
        }),
    ))
}

#[derive(Deserialize)]
struct UpdateSubscriptionReq {
    enabled: Option<bool>,
    pinned: Option<bool>,
}

async fn toggle_subscription(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateSubscriptionReq>,
) -> Result<Json<SubscriptionView>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let mut row = crate::db::subscriptions::SubscriptionRow::find_by_id(&state.db, id, user_id)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound("Subscription not found".into()))?;
    if let Some(enabled) = req.enabled {
        row = crate::db::subscriptions::SubscriptionRow::toggle(&state.db, id, user_id, enabled)
            .await
            .map_err(AppError::Database)?
            .ok_or_else(|| AppError::NotFound("Subscription not found".into()))?;
    }
    if let Some(pinned) = req.pinned {
        row = crate::db::subscriptions::SubscriptionRow::set_pinned(&state.db, id, user_id, pinned)
            .await
            .map_err(AppError::Database)?
            .ok_or_else(|| AppError::NotFound("Subscription not found".into()))?;
    }

    crate::api::prefs::push_async(state.db.clone(), user_id);

    Ok(Json(SubscriptionView {
        id: row.id.to_string(),
        source: row.source,
        source_id: row.source_id,
        label: row.label,
        icon: row.icon,
        category: row.category,
        enabled: row.enabled,
        pinned: row.pinned,
    }))
}

async fn delete_subscription(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let deleted = crate::db::subscriptions::SubscriptionRow::delete(&state.db, id, user_id)
        .await
        .map_err(AppError::Database)?;

    if deleted == 0 {
        return Err(AppError::NotFound("Subscription not found".into()));
    }

    // Push synchronously so the feed is gone from Bluesky before the client
    // reloads; otherwise reconcile_feeds on the next list would see it still
    // saved on Bluesky and re-add the just-deleted sub.
    let _ = crate::api::prefs::push(&state.db, user_id).await;

    Ok(Json(serde_json::json!({ "deleted": true })))
}

/// Remove one interest's source grouping: its topic feed rows (ANY row kind —
/// including a manually-picked feed with the same rkey) and its hashtag rows.
async fn remove_interest_grouping(
    db: &crate::db::DbPool,
    user_id: Uuid,
    interest_id: &str,
) -> Result<(), sqlx::Error> {
    let Some(topic) = crate::api::feedgen::topic_for_interest(interest_id) else {
        return Ok(());
    };
    let rows = crate::db::subscriptions::SubscriptionRow::find_by_user(db, user_id).await?;
    for row in rows {
        let rkey = row.source_id.rsplit('/').next().unwrap_or("");
        let tag = row
            .source_id
            .trim_start_matches("mastodon:")
            .trim_start_matches('#')
            .to_lowercase();
        let is_topic_feed =
            row.source_id.contains("app.bsky.feed.generator") && rkey == topic.rkey;
        let is_topic_tag = topic.tags.iter().any(|t| t.eq_ignore_ascii_case(&tag));
        if is_topic_feed || is_topic_tag {
            let _ =
                crate::db::subscriptions::SubscriptionRow::delete(db, row.id, user_id).await;
        }
    }
    Ok(())
}

/// Expand interest ids into subscriptions: one topic feed + its hashtags per
/// interest. Shared by the bootstrap endpoint and the prefs import.
pub(crate) async fn expand_interests(
    db: &crate::db::DbPool,
    user_id: Uuid,
    interest_ids: &[String],
) -> Result<Vec<crate::db::subscriptions::SubscriptionRow>, sqlx::Error> {
    let mut rows = Vec::new();
    let existing: std::collections::HashSet<String> =
        crate::db::subscriptions::SubscriptionRow::find_by_user(db, user_id)
            .await?
            .into_iter()
            .map(|r| r.source_id)
            .collect();
    let mut seen = std::collections::HashSet::new();
    let unique: Vec<String> = interest_ids
        .iter()
        .filter(|id| seen.insert((*id).clone()))
        .cloned()
        .collect();
    for (interest_id, source) in
        crate::api::interest_sources::get_all_sources_for_interests(&unique)
    {
        for hashtag in &source.mastodon_hashtags {
            let tag = hashtag.trim_start_matches('#');
            if existing.contains(&format!("mastodon:{}", tag)) || existing.contains(tag) {
                continue;
            }
            rows.push(
                crate::db::subscriptions::SubscriptionRow::create(
                    db,
                    user_id,
                    "bootstrap_hashtag",
                    &format!("mastodon:{}", tag),
                    &format!("#{}", tag),
                    "#",
                    "hashtags",
                )
                .await?,
            );
        }
        if let Some(feed_uri) = &source.bluesky_feed {
            let label = crate::api::feedgen::topic_for_interest(&interest_id)
                .map(|t| t.name.to_string())
                .unwrap_or_else(|| interest_id.clone());
            // Prefer the real record URI (publisher repo) so the row exports
            // into savedFeedsPrefV2 as a resolvable feed; the service form is
            // the fallback (content resolution is rkey-based either way).
            let rkey = feed_uri.rsplit('/').next().unwrap_or("");
            let uri = crate::api::feedgen::published_uri(db, rkey)
                .await
                .unwrap_or_else(|| feed_uri.clone());
            let same_rkey = existing.iter().any(|e| {
                e.contains("app.bsky.feed.generator") && e.rsplit('/').next() == Some(rkey)
            });
            if existing.contains(&uri) || existing.contains(feed_uri) || same_rkey {
                continue;
            }
            rows.push(
                crate::db::subscriptions::SubscriptionRow::create(
                    db,
                    user_id,
                    "bootstrap_feed",
                    &uri,
                    &label,
                    "📡",
                    "feeds",
                )
                .await?,
            );
        }
    }
    Ok(rows)
}

#[derive(Deserialize)]
struct BootstrapSubscriptionsReq {
    interest_ids: Vec<String>,
}

async fn bootstrap_subscriptions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<BootstrapSubscriptionsReq>,
) -> Result<(StatusCode, Json<Vec<SubscriptionView>>), AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    // ONE state setter: the request is the complete interest list. Deselected
    // interests lose their whole source grouping (topic feed + hashtags, any
    // row kind) — "deselect an interest unsubscribes the grouping".
    let mut seen = std::collections::HashSet::new();
    let new_ids: Vec<String> = req
        .interest_ids
        .iter()
        .filter(|id| seen.insert((*id).clone()))
        .cloned()
        .collect();
    let rows = reconcile_interests(&state.db, user_id, &new_ids)
        .await
        .map_err(AppError::Database)?;
    let created: Vec<SubscriptionView> = rows
        .into_iter()
        .map(|row| SubscriptionView {
            id: row.id.to_string(),
            source: row.source,
            source_id: row.source_id,
            label: row.label,
            icon: row.icon,
            category: row.category,
            enabled: row.enabled,
            pinned: row.pinned,
        })
        .collect();

    Ok((StatusCode::OK, Json(created)))
}

/// Reconcile a user's interest set to exactly `new_ids`: drop deselected
/// groupings, clear stale bootstrap rows, re-expand into topic-feed + hashtag
/// subscriptions, persist `users.interests`, and mark the account onboarded.
/// Shared by `POST /api/subscriptions/bootstrap` and
/// `PATCH /api/settings/interests` (the Settings section is a second entry
/// point to the same picker). Returns the created subscription rows.
pub(crate) async fn reconcile_interests(
    db: &crate::db::DbPool,
    user_id: Uuid,
    new_ids: &[String],
) -> Result<Vec<crate::db::subscriptions::SubscriptionRow>, sqlx::Error> {
    let old_ids = crate::db::users::UserRow::find_by_id(db, user_id)
        .await?
        .map(|u| u.interests)
        .unwrap_or_default();
    for id in &old_ids {
        if !new_ids.contains(id) {
            remove_interest_grouping(db, user_id, id).await?;
        }
    }

    // Delete remaining bootstrap rows (stale kinds), then re-expand below.
    crate::db::subscriptions::SubscriptionRow::delete_bootstrap_subscriptions(db, user_id).await?;

    // One interest = one topic feed + its hashtags (the topic catalog in
    // feedgen is the single vocabulary).
    let rows = expand_interests(db, user_id, new_ids).await?;

    // Interests are set here — the client does not need a separate profile
    // write, and doing one first would hide the deselection delta above.
    crate::db::users::UserRow::set_interests(db, user_id, new_ids).await?;
    // Saving interests is the onboarding action: mark the account onboarded so
    // the frontend stops routing it through onboarding on subsequent loads.
    crate::db::users::UserRow::set_onboarded(db, user_id, true).await?;

    crate::api::prefs::push_async(db.clone(), user_id);
    Ok(rows)
}
