//! API — Account data export.
//!
//! GET /api/account/export — download all user data as a ZIP file.

use axum::{
    extract::State,
    http::{header, HeaderMap},
    Router,
    routing::get,
};
use uuid::Uuid;
use std::io::Write;

use crate::{error::AppError, AppState};

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/account/export", get(export_account_data))
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

async fn export_account_data(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<axum::response::Response, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let username: String = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch user: {}", e)))?;

    let profile = export_profile(&state.db, user_id).await?;
    let posts = export_table(&state.db, "SELECT * FROM posts WHERE user_id = $1", user_id).await?;
    let media = export_table(&state.db,
        "SELECT id, post_id, user_id, storage_url, thumbnail_url, mime_type, width, height, alt_text, file_size, content_hash, created_at FROM media WHERE user_id = $1",
        user_id).await?;
    let follows = export_table(&state.db, "SELECT * FROM follows WHERE follower_id = $1", user_id).await?;
    let followers = export_table(&state.db, "SELECT * FROM followers WHERE user_id = $1", user_id).await?;
    let linked_accounts = export_table(&state.db,
        "SELECT id, user_id, provider, external_id, external_username, token_type, scope, metadata, created_at, updated_at FROM linked_accounts WHERE user_id = $1",
        user_id).await?;
    let subscriptions = export_table(&state.db, "SELECT * FROM subscriptions WHERE user_id = $1", user_id).await?;
    let wall_posts = export_table(&state.db,
        "SELECT * FROM wall_posts WHERE profile_user_id = $1 OR author_user_id = $1", user_id).await?;
    let forum_topics = export_table(&state.db, "SELECT * FROM forum_topics WHERE user_id = $1", user_id).await?;
    let forum_replies = export_table(&state.db, "SELECT * FROM forum_replies WHERE user_id = $1", user_id).await?;
    let reactions = export_table(&state.db, "SELECT * FROM reactions WHERE user_id = $1 AND retracted_at IS NULL", user_id).await?;
    let events = export_table(&state.db, "SELECT * FROM events WHERE received_by = $1", user_id).await?;
    let event_rsvps = export_table(&state.db, "SELECT * FROM event_rsvps WHERE user_id = $1", user_id).await?;
    let exchange_listings = export_table(&state.db,
        "SELECT * FROM exchange_listings WHERE user_id = $1", user_id).await?;
    let import_sessions = export_table(&state.db,
        "SELECT id, user_id, source, status, total_posts, imported_posts, total_media, imported_media, started_at, completed_at, created_at FROM import_sessions WHERE user_id = $1",
        user_id).await?;

    let mut zip_buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_buf));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        macro_rules! add_json {
            ($zip:expr, $name:expr, $data:expr) => {
                $zip.start_file($name, options)
                    .map_err(|e| AppError::Internal(format!("ZIP error: {}", e)))?;
                let bytes = serde_json::to_string_pretty(&$data)
                    .map_err(|e| AppError::Internal(format!("JSON error: {}", e)))?;
                $zip.write_all(bytes.as_bytes())
                    .map_err(|e| AppError::Internal(format!("ZIP write error: {}", e)))?;
            };
        }

        add_json!(zip, "profile.json", profile);
        add_json!(zip, "posts.json", posts);
        add_json!(zip, "media.json", media);
        add_json!(zip, "follows.json", follows);
        add_json!(zip, "followers.json", followers);
        add_json!(zip, "linked_accounts.json", linked_accounts);
        add_json!(zip, "subscriptions.json", subscriptions);
        add_json!(zip, "wall_posts.json", wall_posts);
        add_json!(zip, "forum_topics.json", forum_topics);
        add_json!(zip, "forum_replies.json", forum_replies);
        add_json!(zip, "reactions.json", reactions);
        add_json!(zip, "events.json", events);
        add_json!(zip, "event_rsvps.json", event_rsvps);
        add_json!(zip, "exchange_listings.json", exchange_listings);
        add_json!(zip, "import_sessions.json", import_sessions);

        zip.finish()
            .map_err(|e| AppError::Internal(format!("ZIP finalize error: {}", e)))?;
    }

    let filename = format!(
        "bytescape-export-{}-{}.zip",
        username,
        chrono::Utc::now().format("%Y-%m-%d")
    );

    let body = axum::body::Body::from(zip_buf);
    Ok(axum::response::Response::builder()
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(body)
        .unwrap())
}

async fn export_profile(db: &sqlx::PgPool, user_id: Uuid) -> Result<serde_json::Value, AppError> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT id, username, display_name, bio, avatar_url, header_url, theme, custom_css, \
         location, website, pronouns, relationship_status, birthday, occupation, interests, \
         widget_layout, widget_themes, created_at, updated_at \
         FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(db)
    .await
    .map_err(|e| AppError::Internal(format!("Failed to fetch profile: {}", e)))?;

    Ok(serde_json::json!({
        "id": row.get::<Uuid, _>("id").to_string(),
        "username": row.get::<String, _>("username"),
        "display_name": row.get::<String, _>("display_name"),
        "bio": row.get::<String, _>("bio"),
        "avatar_url": row.get::<Option<String>, _>("avatar_url"),
        "header_url": row.get::<Option<String>, _>("header_url"),
        "theme": row.get::<String, _>("theme"),
        "custom_css": row.get::<String, _>("custom_css"),
        "location": row.get::<Option<String>, _>("location"),
        "website": row.get::<Option<String>, _>("website"),
        "pronouns": row.get::<Option<String>, _>("pronouns"),
        "relationship_status": row.get::<Option<String>, _>("relationship_status"),
        "birthday": row.get::<Option<chrono::NaiveDate>, _>("birthday"),
        "occupation": row.get::<Option<String>, _>("occupation"),
        "interests": row.get::<Option<Vec<String>>, _>("interests"),
        "widget_layout": row.get::<serde_json::Value, _>("widget_layout"),
        "widget_themes": row.get::<serde_json::Value, _>("widget_themes"),
        "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
        "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
    }))
}

async fn export_table(db: &sqlx::PgPool, query: &str, user_id: Uuid) -> Result<serde_json::Value, AppError> {
    use sqlx::{Row, Column};
    let rows = sqlx::query(query)
        .bind(user_id)
        .fetch_all(db)
        .await
        .map_err(|e| AppError::Internal(format!("Export query failed: {}", e)))?;

    let json_rows: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let mut map = serde_json::Map::new();
            for col in row.columns() {
                let col_name = col.name();
                let val: Option<serde_json::Value> = row.try_get(col_name).ok();
                map.insert(col_name.to_string(), val.unwrap_or(serde_json::Value::Null));
            }
            serde_json::Value::Object(map)
        })
        .collect();

    Ok(serde_json::Value::Array(json_rows))
}