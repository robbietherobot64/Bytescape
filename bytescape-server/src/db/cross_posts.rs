use sqlx::PgPool;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CrossPostRow {
    pub id: Uuid,
    pub post_id: Uuid,
    pub user_id: Uuid,
    pub platform: String,
    pub external_url: String,
    pub created_at: DateTime<Utc>,
    pub external_id: Option<String>,
    pub external_handle: Option<String>,
}

impl CrossPostRow {
    pub async fn create(
        pool: &PgPool,
        post_id: Uuid,
        user_id: Uuid,
        platform: &str,
        external_url: &str,
        external_id: Option<&str>,
        external_handle: Option<&str>,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO cross_posts (post_id, user_id, platform, external_url, external_id, external_handle)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(post_id)
        .bind(user_id)
        .bind(platform)
        .bind(external_url)
        .bind(external_id)
        .bind(external_handle)
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_post(
        pool: &PgPool,
        post_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM cross_posts WHERE post_id = $1 ORDER BY created_at DESC",
        )
        .bind(post_id)
        .fetch_all(pool)
        .await
    }

    pub async fn delete_by_post(
        pool: &PgPool,
        post_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM cross_posts WHERE post_id = $1")
            .bind(post_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Find all cross-posts for a user (used for dedup in aggregated feed)
    pub async fn find_by_user(
        pool: &PgPool,
        user_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM cross_posts WHERE user_id = $1 ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
    }
}
