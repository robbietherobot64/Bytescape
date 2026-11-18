use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct TopicRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub title: String,
    pub content: String,
    pub content_html: Option<String>,
    pub tags: Vec<String>,
    pub community: Option<String>,
    pub pinned: bool,
    pub locked: bool,
    pub reply_count: i32,
    pub view_count: i32,
    pub last_reply_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ReplyRow {
    pub id: Uuid,
    pub topic_id: Uuid,
    pub user_id: Uuid,
    pub content: String,
    pub content_html: Option<String>,
    pub parent_reply_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl TopicRow {
    pub async fn create(
        pool: &PgPool,
        user_id: Uuid,
        title: &str,
        content: &str,
        community: Option<&str>,
        tags: &[String],
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO forum_topics (user_id, title, content, community, tags)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(title)
        .bind(content)
        .bind(community)
        .bind(tags)
        .fetch_one(pool)
        .await
    }

    pub async fn list(
        pool: &PgPool,
        community: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        if let Some(c) = community {
            sqlx::query_as(
                r#"
                SELECT * FROM forum_topics
                WHERE community = $1
                ORDER BY pinned DESC, COALESCE(last_reply_at, created_at) DESC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(c)
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
        } else {
            sqlx::query_as(
                r#"
                SELECT * FROM forum_topics
                ORDER BY pinned DESC, COALESCE(last_reply_at, created_at) DESC
                LIMIT $1 OFFSET $2
                "#,
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
        }
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM forum_topics WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await
    }

    pub async fn increment_view_count(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE forum_topics SET view_count = view_count + 1 WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }
}

impl ReplyRow {
    pub async fn create(
        pool: &PgPool,
        topic_id: Uuid,
        user_id: Uuid,
        content: &str,
        parent_id: Option<Uuid>,
    ) -> Result<Self, sqlx::Error> {
        let row = sqlx::query_as(
            r#"
            INSERT INTO forum_replies (topic_id, user_id, content, parent_reply_id)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#,
        )
        .bind(topic_id)
        .bind(user_id)
        .bind(content)
        .bind(parent_id)
        .fetch_one(pool)
        .await?;

        sqlx::query(
            r#"
            UPDATE forum_topics
            SET reply_count = reply_count + 1, last_reply_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(topic_id)
        .execute(pool)
        .await?;

        Ok(row)
    }

    pub async fn list_for_topic(
        pool: &PgPool,
        topic_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            r#"
            SELECT * FROM forum_replies
            WHERE topic_id = $1
            ORDER BY created_at ASC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(topic_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }
}
