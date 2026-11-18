use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db;
use crate::error::AppError;
use crate::{auth, AppState};

#[derive(Debug, Deserialize)]
pub struct ListTopicsQuery {
    community: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTopicBody {
    title: String,
    content: String,
    community: Option<String>,
    tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct AddReplyBody {
    content: String,
    parent_reply_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct ListRepliesQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AuthorView {
    pub handle: String,
    pub display_name: String,
    pub avatar: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TopicResponse {
    pub id: String,
    pub title: String,
    pub content: String,
    pub content_html: Option<String>,
    pub tags: Vec<String>,
    pub community: Option<String>,
    pub pinned: bool,
    pub locked: bool,
    pub reply_count: i32,
    pub view_count: i32,
    pub last_reply_at: Option<String>,
    pub created_at: String,
    pub author: AuthorView,
}

#[derive(Debug, Serialize)]
pub struct ReplyResponse {
    pub id: String,
    pub topic_id: String,
    pub content: String,
    pub content_html: Option<String>,
    pub parent_reply_id: Option<String>,
    pub created_at: String,
    pub author: AuthorView,
}

#[derive(Debug, Serialize)]
pub struct TopicListResponse {
    pub items: Vec<TopicResponse>,
    pub has_more: bool,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/forums/topics", get(list_topics).post(create_topic))
        .route("/api/forums/topics/{id}", get(get_topic))
        .route("/api/forums/topics/{id}/replies", get(list_replies).post(add_reply))
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

async fn fetch_author(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<AuthorView, AppError> {
    let row = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
        "SELECT username, display_name, avatar_url FROM users WHERE id = $1"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from)?
    .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    Ok(AuthorView {
        handle: row.0.clone(),
        display_name: row.1.unwrap_or(row.0),
        avatar: row.2,
    })
}

async fn topic_to_response(
    pool: &sqlx::PgPool,
    topic: &db::forums::TopicRow,
) -> TopicResponse {
    let author = fetch_author(pool, topic.user_id).await.unwrap_or(AuthorView {
        handle: "unknown".into(),
        display_name: "Unknown".into(),
        avatar: None,
    });

    TopicResponse {
        id: topic.id.to_string(),
        title: topic.title.clone(),
        content: topic.content.clone(),
        content_html: topic.content_html.clone(),
        tags: topic.tags.clone(),
        community: topic.community.clone(),
        pinned: topic.pinned,
        locked: topic.locked,
        reply_count: topic.reply_count,
        view_count: topic.view_count,
        last_reply_at: topic.last_reply_at.map(|t| t.to_rfc3339()),
        created_at: topic.created_at.to_rfc3339(),
        author,
    }
}

async fn list_topics(
    State(state): State<AppState>,
    Query(query): Query<ListTopicsQuery>,
) -> Result<Json<TopicListResponse>, AppError> {
    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.offset.unwrap_or(0);

    let topics = db::forums::TopicRow::list(
        &state.db,
        query.community.as_deref(),
        limit,
        offset,
    )
    .await?;

    let total_count: i64 = if query.community.is_some() {
        sqlx::query_scalar("SELECT COUNT(*) FROM forum_topics WHERE community = $1")
            .bind(&query.community)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM forum_topics")
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
    };

    let mut items = Vec::new();
    for topic in topics {
        items.push(topic_to_response(&state.db, &topic).await);
    }

    let returned = items.len() as i64;
    let has_more = offset + returned < total_count;

    Ok(Json(TopicListResponse { items, has_more }))
}

async fn create_topic(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateTopicBody>,
) -> Result<Json<TopicResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let tags = body.tags.unwrap_or_default();
    let topic = db::forums::TopicRow::create(
        &state.db,
        user_id,
        &body.title,
        &body.content,
        body.community.as_deref(),
        &tags,
    )
    .await?;

    let resp = topic_to_response(&state.db, &topic).await;
    Ok(Json(resp))
}

async fn get_topic(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TopicResponse>, AppError> {
    let topic_id = Uuid::parse_str(&id)
        .map_err(|_| AppError::BadRequest("Invalid topic ID".into()))?;

    let topic = db::forums::TopicRow::find_by_id(&state.db, topic_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Topic not found".into()))?;

    let _ = db::forums::TopicRow::increment_view_count(&state.db, topic_id).await;

    let resp = topic_to_response(&state.db, &topic).await;
    Ok(Json(resp))
}

async fn add_reply(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(topic_id): Path<String>,
    Json(body): Json<AddReplyBody>,
) -> Result<Json<ReplyResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let topic_uuid = Uuid::parse_str(&topic_id)
        .map_err(|_| AppError::BadRequest("Invalid topic ID".into()))?;

    // Verify topic exists
    db::forums::TopicRow::find_by_id(&state.db, topic_uuid)
        .await?
        .ok_or_else(|| AppError::NotFound("Topic not found".into()))?;

    let reply = db::forums::ReplyRow::create(
        &state.db,
        topic_uuid,
        user_id,
        &body.content,
        body.parent_reply_id,
    )
    .await?;

    let author = fetch_author(&state.db, user_id).await?;

    Ok(Json(ReplyResponse {
        id: reply.id.to_string(),
        topic_id: reply.topic_id.to_string(),
        content: reply.content,
        content_html: reply.content_html,
        parent_reply_id: reply.parent_reply_id.map(|id| id.to_string()),
        created_at: reply.created_at.to_rfc3339(),
        author,
    }))
}

async fn list_replies(
    State(state): State<AppState>,
    Path(topic_id): Path<String>,
    Query(query): Query<ListRepliesQuery>,
) -> Result<Json<Vec<ReplyResponse>>, AppError> {
    let topic_uuid = Uuid::parse_str(&topic_id)
        .map_err(|_| AppError::BadRequest("Invalid topic ID".into()))?;

    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.offset.unwrap_or(0);

    let rows = db::forums::ReplyRow::list_for_topic(&state.db, topic_uuid, limit, offset).await?;

    let mut replies = Vec::new();
    for row in rows {
        let author = fetch_author(&state.db, row.user_id).await.unwrap_or(AuthorView {
            handle: "unknown".into(),
            display_name: "Unknown".into(),
            avatar: None,
        });

        replies.push(ReplyResponse {
            id: row.id.to_string(),
            topic_id: row.topic_id.to_string(),
            content: row.content,
            content_html: row.content_html,
            parent_reply_id: row.parent_reply_id.map(|id| id.to_string()),
            created_at: row.created_at.to_rfc3339(),
            author,
        });
    }

    Ok(Json(replies))
}
