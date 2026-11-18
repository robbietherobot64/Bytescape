use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;
use serde_json::Value;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct NotificationRow {
    pub id: Uuid,
    pub user_id: Uuid,
    #[sqlx(rename = "type")]
    pub notification_type: String,
    pub title: String,
    pub body: Option<String>,
    pub data: Option<Value>,
    pub read: bool,
    pub created_at: DateTime<Utc>,
    /// Origin event id (AP/AT activity id, record uri, local key) for dedupe.
    pub dedupe_key: Option<String>,
    /// Acting actor: remote AP actor uri, AT DID, or local username.
    pub actor_id: Option<String>,
    /// 'activitypub' | 'atproto' | 'local'
    pub network: String,
}

impl NotificationRow {
    pub async fn create(
        pool: &PgPool,
        user_id: Uuid,
        notification_type: &str,
        title: &str,
        body: Option<&str>,
        data: Option<Value>,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO notifications (user_id, type, title, body, data)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(notification_type)
        .bind(title)
        .bind(body)
        .bind(data)
        .fetch_one(pool)
        .await
    }

    /// Create a provenance-carrying notification, deduped by `dedupe_key`
    /// (one row per (user, origin event)). Returns the row, or None when the
    /// event was already recorded (redelivery / replay).
    pub async fn create_deduped<'e, E>(
        pool: E,
        user_id: Uuid,
        notification_type: &str,
        title: &str,
        body: Option<&str>,
        data: Option<Value>,
        dedupe_key: &str,
        actor_id: Option<&str>,
        network: &str,
    ) -> Result<Option<Self>, sqlx::Error>
    where
        E: sqlx::PgExecutor<'e>,
    {
        sqlx::query_as(
            r#"
            INSERT INTO notifications (user_id, type, title, body, data, dedupe_key, actor_id, network)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id, dedupe_key) WHERE dedupe_key IS NOT NULL DO NOTHING
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(notification_type)
        .bind(title)
        .bind(body)
        .bind(data)
        .bind(dedupe_key)
        .bind(actor_id)
        .bind(network)
        .fetch_optional(pool)
        .await
    }

    pub async fn find_unread_count(pool: &PgPool, user_id: Uuid) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND read = FALSE",
        )
        .bind(user_id)
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_user(pool: &PgPool, user_id: Uuid, limit: i64) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM notifications WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(pool)
        .await
    }

    /// Newest-first listing with optional type filter and created_at cursor
    /// (exclusive, ISO timestamp) for load-more pagination.
    pub async fn find_filtered(
        pool: &PgPool,
        user_id: Uuid,
        notification_type: Option<&str>,
        before: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM notifications \
             WHERE user_id = $1 \
               AND ($2::text IS NULL OR type = $2) \
               AND ($3::timestamptz IS NULL OR created_at < $3) \
             ORDER BY created_at DESC LIMIT $4",
        )
        .bind(user_id)
        .bind(notification_type)
        .bind(before)
        .bind(limit)
        .fetch_all(pool)
        .await
    }

    pub async fn mark_read(pool: &PgPool, notification_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE notifications SET read = TRUE WHERE id = $1 AND user_id = $2",
        )
        .bind(notification_id)
        .bind(user_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn mark_all_read(pool: &PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE notifications SET read = TRUE WHERE user_id = $1 AND read = FALSE")
            .bind(user_id)
            .execute(pool)
            .await?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ap::testutil::{connect_pool, unique};

    /// Dedupe by (user_id, dedupe_key): a redelivered federated event creates
    /// exactly one row. Distinct dedupe keys create distinct rows.
    #[tokio::test]
    async fn create_deduped_is_idempotent_per_origin_event() {
        let Some(pool) = connect_pool().await else { return };
        let username = format!("nd_{}", unique());
        let user = crate::db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");
        let key = format!("ap-activity-{}", unique());

        let first = NotificationRow::create_deduped(
            &pool, user.id, "like", "New like", Some("a liked"), None, &key,
            Some("https://remote.test/users/a"), "activitypub",
        )
        .await
        .expect("first insert");
        assert!(first.is_some(), "first event inserts a row");

        // Replay (same dedupe key) -> no second row.
        let second = NotificationRow::create_deduped(
            &pool, user.id, "like", "New like", Some("a liked"), None, &key,
            Some("https://remote.test/users/a"), "activitypub",
        )
        .await
        .expect("replay insert");
        assert!(second.is_none(), "replayed event deduped to no new row");

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE user_id = $1")
            .bind(user.id)
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(count, 1, "exactly one notification per origin event");

        // A different event (different key) inserts again.
        let third = NotificationRow::create_deduped(
            &pool, user.id, "like", "New like", Some("b liked"), None,
            &format!("ap-activity-{}", unique()),
            Some("https://remote.test/users/b"), "activitypub",
        )
        .await
        .expect("third insert");
        assert!(third.is_some(), "a distinct event inserts a new row");

        let _ = sqlx::query("DELETE FROM notifications WHERE user_id = $1")
            .bind(user.id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id).execute(&pool).await;
    }

    /// Each notification carries actor + network provenance.
    #[tokio::test]
    async fn notifications_carry_actor_and_network_provenance() {
        let Some(pool) = connect_pool().await else { return };
        let username = format!("np_{}", unique());
        let user = crate::db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");
        let key = format!("at-{}", unique());

        let row = NotificationRow::create_deduped(
            &pool, user.id, "follow", "New follower", Some("bob followed"), None, &key,
            Some("did:plc:bob"), "atproto",
        )
        .await
        .expect("insert")
        .expect("row");
        assert_eq!(row.network, "atproto");
        assert_eq!(row.actor_id.as_deref(), Some("did:plc:bob"));

        // Type filter returns only matching rows.
        let likes = NotificationRow::find_filtered(&pool, user.id, Some("like"), None, 20)
            .await
            .expect("filter");
        assert!(likes.is_empty(), "no like rows for this user");
        let follows = NotificationRow::find_filtered(&pool, user.id, Some("follow"), None, 20)
            .await
            .expect("filter");
        assert_eq!(follows.len(), 1, "the follow row is returned by its type filter");

        let _ = sqlx::query("DELETE FROM notifications WHERE user_id = $1")
            .bind(user.id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id).execute(&pool).await;
    }
}
