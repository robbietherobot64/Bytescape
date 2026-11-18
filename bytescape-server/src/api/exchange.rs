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
pub struct ListListingsQuery {
    category: Option<String>,
    listing_type: Option<String>,
    offset: Option<i64>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateListingBody {
    title: String,
    description: String,
    listing_type: Option<String>,
    category: Option<String>,
    price_cents: Option<i64>,
    currency: Option<String>,
    location: Option<String>,
    tags: Option<Vec<String>>,
    image_urls: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct ListingResponse {
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub description: String,
    pub listing_type: String,
    pub category: String,
    pub price_cents: Option<i64>,
    pub currency: Option<String>,
    pub location: Option<String>,
    pub tags: Vec<String>,
    pub image_urls: Vec<String>,
    pub status: String,
    pub view_count: i32,
    pub created_at: String,
    pub updated_at: String,
    pub author: Option<AuthorView>,
}

#[derive(Debug, Serialize)]
pub struct AuthorView {
    pub handle: String,
    pub display_name: String,
    pub avatar: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListListingsResponse {
    pub items: Vec<ListingResponse>,
    pub has_more: bool,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub id: String,
    pub listing_id: String,
    pub sender_id: String,
    pub recipient_id: String,
    pub content: String,
    pub read: bool,
    pub created_at: String,
    pub sender: Option<AuthorView>,
    pub recipient: Option<AuthorView>,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageBody {
    content: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStatusBody {
    status: String,
}

#[derive(Debug, Deserialize)]
pub struct ListMessagesQuery {
    offset: Option<i64>,
    limit: Option<i64>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/exchange/listings", get(list_listings).post(create_listing))
        .route("/api/exchange/listings/{id}", get(get_listing).put(update_listing_status).delete(delete_listing))
        .route("/api/exchange/listings/{id}/messages", get(list_messages).post(send_message))
        .route("/api/exchange/messages/inbox", get(list_inbox))
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

fn listing_to_response(listing: &db::exchange::ListingRow, author: Option<AuthorView>) -> ListingResponse {
    ListingResponse {
        id: listing.id.to_string(),
        user_id: listing.user_id.to_string(),
        title: listing.title.clone(),
        description: listing.description.clone(),
        listing_type: listing.listing_type.clone(),
        category: listing.category.clone(),
        price_cents: listing.price_cents,
        currency: listing.currency.clone(),
        location: listing.location.clone(),
        tags: listing.tags.clone(),
        image_urls: listing.image_urls.clone(),
        status: listing.status.clone(),
        view_count: listing.view_count,
        created_at: listing.created_at.to_rfc3339(),
        updated_at: listing.updated_at.to_rfc3339(),
        author,
    }
}

async fn list_listings(
    State(state): State<AppState>,
    Query(query): Query<ListListingsQuery>,
) -> Result<Json<ListListingsResponse>, AppError> {
    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.offset.unwrap_or(0);

    let listings = db::exchange::ListingRow::list(
        &state.db,
        query.category.as_deref(),
        query.listing_type.as_deref(),
        limit + 1,
        offset,
    )
    .await?;

    let has_more = listings.len() as i64 > limit;
    let items = listings.into_iter().take(limit as usize).collect::<Vec<_>>();

    let mut response_items = Vec::new();
    for listing in items {
        let author = fetch_author(&state.db, listing.user_id).await.ok();
        response_items.push(listing_to_response(&listing, author));
    }

    Ok(Json(ListListingsResponse {
        items: response_items,
        has_more,
    }))
}

async fn create_listing(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateListingBody>,
) -> Result<Json<ListingResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let listing_type = body.listing_type.unwrap_or_else(|| "offer".to_string());
    let category = body.category.unwrap_or_else(|| "other".to_string());

    if !matches!(listing_type.as_str(), "offer" | "want") {
        return Err(AppError::BadRequest("listing_type must be 'offer' or 'want'".into()));
    }

    let row = db::exchange::ListingRow::create(
        &state.db,
        user_id,
        &body.title,
        &body.description,
        &listing_type,
        &category,
        body.price_cents,
        body.currency.as_deref(),
        body.location.as_deref(),
        &body.tags.unwrap_or_default(),
        &body.image_urls.unwrap_or_default(),
    )
    .await?;

    let author = fetch_author(&state.db, user_id).await.ok();
    Ok(Json(listing_to_response(&row, author)))
}

async fn get_listing(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ListingResponse>, AppError> {
    let listing_id = Uuid::parse_str(&id)
        .map_err(|_| AppError::BadRequest("Invalid listing ID".into()))?;

    let listing = db::exchange::ListingRow::find_by_id(&state.db, listing_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Listing not found".into()))?;

    let _ = db::exchange::ListingRow::increment_view_count(&state.db, listing_id).await;

    let maybe_user_id = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|token| auth::validate_token(token, &state.config.jwt_secret).ok())
        .and_then(|claims| Uuid::parse_str(&claims.sub).ok());

    let author = fetch_author(&state.db, listing.user_id).await.ok();
    let mut resp = listing_to_response(&listing, author);

    // Only show description + contact info if the listing is active or the viewer is the owner
    if listing.status != "active" && maybe_user_id != Some(listing.user_id) {
        resp.description = String::new();
    }

    Ok(Json(resp))
}

async fn update_listing_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<UpdateStatusBody>,
) -> Result<Json<ListingResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let listing_id = Uuid::parse_str(&id)
        .map_err(|_| AppError::BadRequest("Invalid listing ID".into()))?;

    if !matches!(body.status.as_str(), "active" | "sold" | "expired" | "pending") {
        return Err(AppError::BadRequest("status must be 'active', 'sold', 'expired', or 'pending'".into()));
    }

    let listing = db::exchange::ListingRow::update_status(&state.db, listing_id, user_id, &body.status)
        .await?
        .ok_or_else(|| AppError::NotFound("Listing not found or not owned by you".into()))?;

    let author = fetch_author(&state.db, user_id).await.ok();
    Ok(Json(listing_to_response(&listing, author)))
}

async fn delete_listing(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let listing_id = Uuid::parse_str(&id)
        .map_err(|_| AppError::BadRequest("Invalid listing ID".into()))?;

    db::exchange::ListingRow::delete(&state.db, listing_id, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Listing not found or not owned by you".into()))?;

    Ok(Json(serde_json::json!({ "deleted": true })))
}

async fn send_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(listing_id): Path<String>,
    Json(body): Json<SendMessageBody>,
) -> Result<Json<MessageResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let listing_uuid = Uuid::parse_str(&listing_id)
        .map_err(|_| AppError::BadRequest("Invalid listing ID".into()))?;

    if body.content.trim().is_empty() {
        return Err(AppError::BadRequest("Message content cannot be empty".into()));
    }

    let listing = db::exchange::ListingRow::find_by_id(&state.db, listing_uuid)
        .await?
        .ok_or_else(|| AppError::NotFound("Listing not found".into()))?;

    if listing.user_id == user_id {
        return Err(AppError::BadRequest("Cannot message your own listing".into()));
    }

    let row = db::exchange::MessageRow::create(
        &state.db,
        listing_uuid,
        user_id,
        listing.user_id,
        &body.content,
    )
    .await?;

    let sender = fetch_author(&state.db, user_id).await.ok();
    let recipient = fetch_author(&state.db, listing.user_id).await.ok();

    Ok(Json(MessageResponse {
        id: row.id.to_string(),
        listing_id: row.listing_id.to_string(),
        sender_id: row.sender_id.to_string(),
        recipient_id: row.recipient_id.to_string(),
        content: row.content,
        read: row.read,
        created_at: row.created_at.to_rfc3339(),
        sender,
        recipient,
    }))
}

async fn list_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(listing_id): Path<String>,
    Query(query): Query<ListMessagesQuery>,
) -> Result<Json<Vec<MessageResponse>>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let listing_uuid = Uuid::parse_str(&listing_id)
        .map_err(|_| AppError::BadRequest("Invalid listing ID".into()))?;

    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    let rows = db::exchange::MessageRow::list_for_listing(&state.db, listing_uuid, user_id, limit, offset).await?;

    let mut messages = Vec::new();
    for row in rows {
        let sender = fetch_author(&state.db, row.sender_id).await.ok();
        let recipient = fetch_author(&state.db, row.recipient_id).await.ok();

        // Mark messages as read if user is the recipient
        if row.recipient_id == user_id && !row.read {
            let _ = db::exchange::MessageRow::mark_read(&state.db, row.id, user_id).await;
        }

        messages.push(MessageResponse {
            id: row.id.to_string(),
            listing_id: row.listing_id.to_string(),
            sender_id: row.sender_id.to_string(),
            recipient_id: row.recipient_id.to_string(),
            content: row.content,
            read: row.read,
            created_at: row.created_at.to_rfc3339(),
            sender,
            recipient,
        });
    }

    Ok(Json(messages))
}

async fn list_inbox(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListMessagesQuery>,
) -> Result<Json<Vec<MessageResponse>>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    let rows = db::exchange::MessageRow::list_inbox(&state.db, user_id, limit, offset).await?;

    let mut messages = Vec::new();
    for row in rows {
        let sender = fetch_author(&state.db, row.sender_id).await.ok();
        let recipient = fetch_author(&state.db, row.recipient_id).await.ok();

        messages.push(MessageResponse {
            id: row.id.to_string(),
            listing_id: row.listing_id.to_string(),
            sender_id: row.sender_id.to_string(),
            recipient_id: row.recipient_id.to_string(),
            content: row.content,
            read: row.read,
            created_at: row.created_at.to_rfc3339(),
            sender,
            recipient,
        });
    }

    Ok(Json(messages))
}
