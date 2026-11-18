//! Media attachment database operations.

use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MediaRow {
    pub id: Uuid,
    pub post_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub storage_url: String,
    pub thumbnail_url: Option<String>,
    pub mime_type: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub alt_text: Option<String>,
    pub location_lat: Option<f64>,
    pub location_lng: Option<f64>,
    pub taken_at: Option<chrono::DateTime<chrono::Utc>>,
    pub file_size: Option<i64>,
    pub content_hash: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl MediaRow {
    pub async fn create(
        pool: &PgPool,
        post_id: Uuid,
        storage_url: &str,
        thumbnail_url: Option<&str>,
        mime_type: &str,
        width: Option<i32>,
        height: Option<i32>,
        alt_text: Option<&str>,
        taken_at: Option<chrono::DateTime<chrono::Utc>>,
        file_size: Option<i64>,
        content_hash: Option<&str>,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO media
                (post_id, storage_url, thumbnail_url, mime_type, width, height,
                 alt_text, taken_at, file_size, content_hash)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING *
            "#,
        )
        .bind(post_id)
        .bind(storage_url)
        .bind(thumbnail_url)
        .bind(mime_type)
        .bind(width)
        .bind(height)
        .bind(alt_text)
        .bind(taken_at)
        .bind(file_size)
        .bind(content_hash)
        .fetch_one(pool)
        .await
    }

    /// Create a media row linked to a user (no post association yet).
    pub async fn create_for_user(
        pool: &PgPool,
        user_id: Uuid,
        storage_url: &str,
        thumbnail_url: Option<&str>,
        mime_type: &str,
        width: Option<i32>,
        height: Option<i32>,
        alt_text: Option<&str>,
        file_size: Option<i64>,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO media
                (user_id, storage_url, thumbnail_url, mime_type, width, height,
                 alt_text, file_size)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(storage_url)
        .bind(thumbnail_url)
        .bind(mime_type)
        .bind(width)
        .bind(height)
        .bind(alt_text)
        .bind(file_size)
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_post_id(pool: &PgPool, post_id: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM media WHERE post_id = $1 ORDER BY created_at")
            .bind(post_id)
            .fetch_all(pool)
            .await
    }

    pub async fn vault(
        pool: &PgPool,
        user_id: Uuid,
        limit: i64,
        offset: i64,
        _source_filter: Option<&str>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"
            SELECT m.* FROM media m
            LEFT JOIN posts p ON m.post_id = p.id
            WHERE m.user_id = $1 OR p.user_id = $1
            ORDER BY m.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }
}
