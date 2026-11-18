use sqlx::PgPool;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ChatMessageRow {
    pub id: uuid::Uuid,
    pub platform: String,
    pub conversation_id: String,
    pub provider_account_id: Option<String>,
    pub message_id: String,
    pub sender_id: String,
    pub sender_name: Option<String>,
    pub sender_avatar: Option<String>,
    pub content: String,
    pub sent_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl ChatMessageRow {
    pub async fn upsert(
        pool: &PgPool,
        platform: &str,
        conversation_id: &str,
        provider_account_id: Option<&str>,
        message_id: &str,
        sender_id: &str,
        sender_name: Option<&str>,
        sender_avatar: Option<&str>,
        content: &str,
        sent_at: DateTime<Utc>,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO chat_messages
                (platform, conversation_id, provider_account_id, message_id, sender_id, sender_name, sender_avatar, content, sent_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (platform, message_id) DO UPDATE SET
                sender_name = EXCLUDED.sender_name,
                sender_avatar = EXCLUDED.sender_avatar,
                content = EXCLUDED.content,
                sent_at = EXCLUDED.sent_at
            RETURNING *
            "#
        )
        .bind(platform)
        .bind(conversation_id)
        .bind(provider_account_id)
        .bind(message_id)
        .bind(sender_id)
        .bind(sender_name)
        .bind(sender_avatar)
        .bind(content)
        .bind(sent_at)
        .fetch_one(pool)
        .await
    }

    pub async fn list_for_conversation(
        pool: &PgPool,
        platform: &str,
        conversation_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM chat_messages
             WHERE platform = $1 AND conversation_id = $2
             ORDER BY sent_at DESC LIMIT $3 OFFSET $4"
        )
        .bind(platform)
        .bind(conversation_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }

    pub async fn latest_for_platform(
        pool: &PgPool,
        platform: &str,
        provider_account_id: &str,
    ) -> Result<Vec<(String, i64)>, sqlx::Error> {
        sqlx::query_as(
            "SELECT conversation_id, COUNT(*) as cnt
             FROM chat_messages
             WHERE platform = $1 AND provider_account_id = $2
             GROUP BY conversation_id"
        )
        .bind(platform)
        .bind(provider_account_id)
        .fetch_all(pool)
        .await
    }
}
