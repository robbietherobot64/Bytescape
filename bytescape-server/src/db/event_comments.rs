use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

/// Event comments are Bytescape-side rows (the calendar lexicons define no
/// comment type) attached to the event's at-URI — they render only through
/// `calendar_events` and are removed when the event record is deleted
/// (spec: events/participate).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventCommentRow {
    pub id: Uuid,
    pub event_uri: String,
    pub user_id: Uuid,
    pub content: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl EventCommentRow {
    pub async fn create(
        pool: &PgPool,
        event_uri: &str,
        user_id: Uuid,
        content: &str,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO event_comments (event_uri, user_id, content)
            VALUES ($1, $2, $3)
            RETURNING *
            "#,
        )
        .bind(event_uri)
        .bind(user_id)
        .bind(content)
        .fetch_one(pool)
        .await
    }

    pub async fn list_for_event(
        pool: &PgPool,
        event_uri: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            r#"
            SELECT * FROM event_comments
            WHERE event_uri = $1
            ORDER BY created_at ASC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(event_uri)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }

    pub async fn delete(
        pool: &PgPool,
        comment_id: Uuid,
        user_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        sqlx::query("DELETE FROM event_comments WHERE id = $1 AND user_id = $2")
            .bind(comment_id)
            .bind(user_id)
            .execute(pool)
            .await
            .map(|r| r.rows_affected())
    }

    pub async fn counts_for_events(
        pool: &PgPool,
        event_uris: &[String],
    ) -> Result<HashMap<String, i64>, sqlx::Error> {
        if event_uris.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT event_uri, COUNT(*)::int8 FROM event_comments WHERE event_uri = ANY($1) GROUP BY event_uri",
        )
        .bind(event_uris)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().collect())
    }
}
