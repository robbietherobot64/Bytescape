//! Wall post database operations.

use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WallPostRow {
    pub id: Uuid,
    pub profile_user_id: Uuid,
    pub author_user_id: Uuid,
    pub content: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl WallPostRow {
    pub async fn create(
        pool: &PgPool,
        profile_user_id: Uuid,
        author_user_id: Uuid,
        content: &str,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO wall_posts (profile_user_id, author_user_id, content) VALUES ($1, $2, $3) RETURNING *"
        )
        .bind(profile_user_id)
        .bind(author_user_id)
        .bind(content)
        .fetch_one(pool)
        .await
    }

    pub async fn list_by_profile(
        pool: &PgPool,
        profile_user_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM wall_posts WHERE profile_user_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        )
        .bind(profile_user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }
}
