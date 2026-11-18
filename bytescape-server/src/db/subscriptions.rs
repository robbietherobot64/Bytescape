use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SubscriptionRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub source: String,
    pub source_id: String,
    pub label: String,
    pub icon: String,
    pub category: String,
    pub enabled: bool,
    pub pinned: bool,
    #[allow(dead_code)]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl SubscriptionRow {
    pub async fn create(
        pool: &PgPool,
        user_id: Uuid,
        source: &str,
        source_id: &str,
        label: &str,
        icon: &str,
        category: &str,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO subscriptions (user_id, source, source_id, label, icon, category)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (user_id, source, source_id)
            DO UPDATE SET enabled = true
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(source)
        .bind(source_id)
        .bind(label)
        .bind(icon)
        .bind(category)
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM subscriptions WHERE user_id = $1 ORDER BY category, label",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_user_and_category(
        pool: &PgPool,
        user_id: Uuid,
        category: &str,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM subscriptions WHERE user_id = $1 AND category = $2 ORDER BY label",
        )
        .bind(user_id)
        .bind(category)
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_user_enabled(
        pool: &PgPool,
        user_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM subscriptions WHERE user_id = $1 AND enabled = true ORDER BY category, label",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_source(pool: &PgPool, user_id: Uuid, source: &str) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM subscriptions WHERE user_id = $1 AND source = $2 ORDER BY label",
        )
        .bind(user_id)
        .bind(source)
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_category(
        pool: &PgPool,
        user_id: Uuid,
        category: &str,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM subscriptions WHERE user_id = $1 AND category = $2 ORDER BY label",
        )
        .bind(user_id)
        .bind(category)
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_id(
        pool: &PgPool,
        id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM subscriptions WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user_id)
            .fetch_optional(pool)
            .await
    }

    pub async fn set_pinned(
        pool: &PgPool,
        id: Uuid,
        user_id: Uuid,
        pinned: bool,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as(
            "UPDATE subscriptions SET pinned = $1 WHERE id = $2 AND user_id = $3 RETURNING *",
        )
        .bind(pinned)
        .bind(id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn toggle(
        pool: &PgPool,
        id: Uuid,
        user_id: Uuid,
        enabled: bool,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as(
            "UPDATE subscriptions SET enabled = $1 WHERE id = $2 AND user_id = $3 RETURNING *",
        )
        .bind(enabled)
        .bind(id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn delete(pool: &PgPool, id: Uuid, user_id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM subscriptions WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn delete_by_source(pool: &PgPool, user_id: Uuid, source: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM subscriptions WHERE user_id = $1 AND source = $2 AND category = 'follows'"
        )
        .bind(user_id)
        .bind(source)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn delete_by_source_any(pool: &PgPool, user_id: Uuid, source: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM subscriptions WHERE user_id = $1 AND source = $2"
        )
        .bind(user_id)
        .bind(source)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn delete_bootstrap_subscriptions(pool: &PgPool, user_id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"DELETE FROM subscriptions
               WHERE user_id = $1
                 AND source IN ('bootstrap_hashtag', 'bluesky_search', 'bootstrap_feed')"#
        )
        .bind(user_id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }
}
