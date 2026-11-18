use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{AppState, error::AppError, db::notifications::NotificationRow};
use crate::auth;

fn extract_user_id(headers: &axum::http::HeaderMap, secret: &str) -> Result<uuid::Uuid, AppError> {
    let auth_header = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Auth("Missing or invalid Authorization header".into()))?;
    let claims = auth::validate_token(auth_header, secret)?;
    uuid::Uuid::parse_str(&claims.sub).map_err(|_| AppError::Auth("Invalid user ID in token".into()))
}

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/api/notifications", axum::routing::get(list_notifications))
        .route("/api/notifications/unread-count", axum::routing::get(unread_count))
        .route("/api/notifications/{id}/read", axum::routing::post(mark_read))
        .route("/api/notifications/read-all", axum::routing::post(mark_all_read))
        .route("/api/notifications/follow-requests", axum::routing::get(list_follow_requests))
        .route(
            "/api/notifications/follow-requests/{id}/approve",
            axum::routing::post(approve_follow_request),
        )
        .route(
            "/api/notifications/follow-requests/{id}/deny",
            axum::routing::post(deny_follow_request),
        )
}

#[derive(serde::Deserialize)]
struct ListQuery {
    /// Filter by notification type (like, repost, follow, reply, mention,
    /// profile_comment, follow_request).
    #[serde(rename = "type")]
    notification_type: Option<String>,
    /// Load-more cursor: created_at of the last item (exclusive).
    cursor: Option<String>,
    limit: Option<i64>,
}

async fn list_notifications(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<ListQuery>,
) -> Result<Json<Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let limit = query.limit.unwrap_or(20).clamp(1, 50);
    let before = match &query.cursor {
        Some(cursor) => Some(
            chrono::DateTime::parse_from_rfc3339(cursor)
                .map_err(|_| AppError::BadRequest("invalid cursor".into()))?
                .with_timezone(&chrono::Utc),
        ),
        None => None,
    };
    let notifications = NotificationRow::find_filtered(
        &state.db,
        user_id,
        query.notification_type.as_deref(),
        before,
        limit,
    )
    .await?;
    Ok(Json(json!({ "notifications": notifications })))
}

async fn unread_count(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let count = NotificationRow::find_unread_count(&state.db, user_id).await?;
    Ok(Json(json!({ "count": count })))
}

async fn mark_read(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    NotificationRow::mark_read(&state.db, id, user_id).await?;
    Ok(Json(json!({ "status": "read" })))
}

async fn mark_all_read(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    NotificationRow::mark_all_read(&state.db, user_id).await?;
    Ok(Json(json!({ "status": "all_read" })))
}

// ---------------------------------------------------------------------------
// Follow requests (Phase 4): list + approve/deny inline actions.
//
// Approve on an AP request sends a deterministic `Accept` (the same shape the
// inbox auto-accept path queues — see ap/inbox.rs Follow effect) and marks the
// `followers` row accepted. Deny sends a `Reject`. AT-side (network='atproto')
// requests have no protocol enforcement: approve flips our local `follows`
// row to accepted (UI-only gating, disclosed in the UI), deny deletes it.
// ---------------------------------------------------------------------------

async fn list_follow_requests(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let requests = crate::db::follow_requests::FollowRequestRow::find_pending(&state.db, user_id).await?;
    Ok(Json(json!({ "requests": requests })))
}

#[derive(serde::Deserialize)]
struct ResolveParams {
    id: Uuid,
}

async fn approve_follow_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(params): Path<ResolveParams>,
) -> Result<Json<Value>, AppError> {
    resolve_follow_request(state, headers, params.id, true).await
}

async fn deny_follow_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(params): Path<ResolveParams>,
) -> Result<Json<Value>, AppError> {
    resolve_follow_request(state, headers, params.id, false).await
}

async fn resolve_follow_request(
    state: AppState,
    headers: HeaderMap,
    request_id: Uuid,
    approve: bool,
) -> Result<Json<Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let decision = if approve { "approved" } else { "denied" };

    let request = crate::db::follow_requests::FollowRequestRow::resolve(
        &state.db,
        request_id,
        user_id,
        decision,
    )
    .await?
    .ok_or_else(|| AppError::NotFound("no such pending follow request".into()))?;

    if request.network == "activitypub" {
        // Queue a deterministic Accept or Reject to the follower's inbox.
        let user = crate::db::users::UserRow::find_by_id(&state.db, user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("user not found".into()))?;
        let local_actor_id = user
            .ap_actor_id
            .clone()
            .unwrap_or_else(|| crate::ap::actor::canonical_actor_url(&state.config.domain, &user.username));
        let follow_activity_id = request
            .follow_activity_id
            .clone()
            .unwrap_or_else(|| request.fan_out.clone());
        let response_id = format!(
            "{}/activities/{}-{}",
            local_actor_id,
            if approve { "accept" } else { "reject" },
            crate::ap::inbox::activity_hash(&follow_activity_id)
        );
        let response = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": response_id,
            "type": if approve { "Accept" } else { "Reject" },
            "actor": local_actor_id,
            "object": follow_activity_id,
            "to": [request.fan_out],
        });

        let mut tx = state.db.begin().await.map_err(|e| AppError::Internal(format!("txn: {e}")))?;
        let new = crate::ap::store::insert_activity(
            &mut tx,
            &response_id,
            &local_actor_id,
            if approve { "Accept" } else { "Reject" },
            Some(&follow_activity_id),
            None,
            "out",
            &response,
        )
        .await
        .map_err(|e| AppError::Internal(format!("activity store: {e}")))?;
        if new {
            let mut inboxes = crate::ap::store::audience_inboxes(
                &state.db,
                &local_actor_id,
                &[request.fan_out.clone()],
            )
            .await;
            if inboxes.is_empty() {
                if let Some(inbox) = &request.inbox_url {
                    inboxes.push(inbox.clone());
                }
            }
            crate::ap::store::enqueue_for_inboxes(&mut tx, &response_id, &inboxes)
                .await
                .map_err(|e| AppError::Internal(format!("enqueue: {e}")))?;
        }
        tx.commit().await.map_err(|e| AppError::Internal(format!("txn: {e}")))?;

        if approve {
            // The follow becomes accepted: record the inbound follower.
            sqlx::query(
                "INSERT INTO followers (actor_id, inbox_url, user_id, accepted) \
                 VALUES ($1, $2, $3, true) \
                 ON CONFLICT (actor_id, user_id) DO UPDATE \
                   SET inbox_url = EXCLUDED.inbox_url, accepted = true",
            )
            .bind(&request.fan_out)
            .bind(request.inbox_url.as_deref().unwrap_or(""))
            .bind(user_id)
            .execute(&state.db)
            .await
            .map_err(|e| AppError::Internal(format!("followers store: {e}")))?;
        } else {
            sqlx::query("DELETE FROM followers WHERE actor_id = $1 AND user_id = $2")
                .bind(&request.fan_out)
                .bind(user_id)
                .execute(&state.db)
                .await
                .map_err(|e| AppError::Internal(format!("followers delete: {e}")))?;
        }
    } else {
        // AT/local: no protocol message — flip the local follows row.
        if approve {
            sqlx::query(
                "UPDATE follows SET status = 'accepted' WHERE followed_actor = $1 AND follower_id = $2",
            )
            .bind(&request.fan_out)
            .bind(user_id)
            .execute(&state.db)
            .await
            .map_err(|e| AppError::Internal(format!("follows accept: {e}")))?;
        } else {
            sqlx::query("DELETE FROM follows WHERE followed_actor = $1 AND follower_id = $2")
                .bind(&request.fan_out)
                .bind(user_id)
                .execute(&state.db)
                .await
                .map_err(|e| AppError::Internal(format!("follows delete: {e}")))?;
        }
    }

    Ok(Json(json!({ "status": decision, "request_id": request_id })))
}