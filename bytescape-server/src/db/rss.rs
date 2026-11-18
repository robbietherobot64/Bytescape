use sqlx::PgPool;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RssFeedItemRow {
    pub id: Uuid,
    pub subscription_id: Uuid,
    pub user_id: Uuid,
    pub guid: String,
    pub title: Option<String>,
    pub link: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub pub_date: DateTime<Utc>,
    pub content: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RssFeedItemWithLabel {
    pub id: Uuid,
    pub subscription_id: Uuid,
    pub user_id: Uuid,
    pub guid: String,
    pub title: Option<String>,
    pub link: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub pub_date: DateTime<Utc>,
    pub content: Option<String>,
    pub created_at: DateTime<Utc>,
    pub subscription_label: String,
    pub subscription_url: Option<String>,
}

impl RssFeedItemRow {
    pub async fn create(
        pool: &PgPool,
        subscription_id: Uuid,
        user_id: Uuid,
        guid: &str,
        title: Option<&str>,
        link: Option<&str>,
        description: Option<&str>,
        author: Option<&str>,
        pub_date: DateTime<Utc>,
        content: Option<&str>,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO rss_feed_items (subscription_id, user_id, guid, title, link, description, author, pub_date, content)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (subscription_id, guid) DO UPDATE SET
                title = EXCLUDED.title,
                description = EXCLUDED.description,
                content = EXCLUDED.content,
                pub_date = EXCLUDED.pub_date
            RETURNING *
            "#
        )
        .bind(subscription_id).bind(user_id).bind(guid).bind(title).bind(link).bind(description).bind(author).bind(pub_date).bind(content)
        .fetch_one(pool)
        .await
    }

    pub async fn feed_for_user(pool: &PgPool, user_id: Uuid, limit: i64, offset: i64) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM rss_feed_items WHERE user_id = $1 ORDER BY pub_date DESC LIMIT $2 OFFSET $3"
        )
        .bind(user_id).bind(limit).bind(offset)
        .fetch_all(pool)
        .await
    }

    pub async fn feed_for_user_with_labels(pool: &PgPool, user_id: Uuid, limit: i64, offset: i64) -> Result<Vec<RssFeedItemWithLabel>, sqlx::Error> {
        sqlx::query_as(
            r#"SELECT i.*, s.label as subscription_label, s.url as subscription_url
            FROM rss_feed_items i
            JOIN subscriptions s ON i.subscription_id = s.id
            WHERE i.user_id = $1 AND s.enabled = true
            ORDER BY i.pub_date DESC LIMIT $2 OFFSET $3"#
        )
        .bind(user_id).bind(limit).bind(offset)
        .fetch_all(pool)
        .await
    }

    pub async fn delete_by_subscription(pool: &PgPool, subscription_id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM rss_feed_items WHERE subscription_id = $1")
            .bind(subscription_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }
}
