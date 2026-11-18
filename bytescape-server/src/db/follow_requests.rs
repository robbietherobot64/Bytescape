//! Pending inbound follow requests (locked accounts).
//!
//! A `Follow` on a user with `manually_approves_followers = true` is recorded
//! here instead of auto-accepted. The owner approves (→ `Accept` sent,
//! `followers` row accepted) or denies (→ `Reject` sent, no relationship).
//! AT-side (Bluesky) follows use the same table with `network = 'atproto'` —
//! no protocol enforcement exists there (disclosed in UI); approval flips our
//! local `follows` row to accepted on approval.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct FollowRequestRow {
    pub id: Uuid,
    /// Follower actor URI (remote AP actor id, AT DID, or local actor id).
    pub fan_out: String,
    /// Target local user (the locked account).
    pub user_id: Uuid,
    pub inbox_url: Option<String>,
    pub follow_activity_id: Option<String>,
    /// 'activitypub' | 'atproto' | 'local'
    pub network: String,
    /// NULL = pending | 'approved' | 'denied'
    pub decision: Option<String>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

impl FollowRequestRow {
    /// Queue a pending request (upserts on the latest state; a repeat Follow
    /// while pending is a no-op, after a decision it re-opens as pending).
    pub async fn create<'e, E>(
        pool: E,
        fan_out: &str,
        user_id: Uuid,
        inbox_url: Option<&str>,
        follow_activity_id: Option<&str>,
        network: &str,
    ) -> Result<Self, sqlx::Error>
    where
        E: sqlx::PgExecutor<'e>,
    {
        sqlx::query_as(
            r#"
            INSERT INTO follow_requests (fan_out, user_id, inbox_url, follow_activity_id, network)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (fan_out, user_id) DO UPDATE
              SET inbox_url = EXCLUDED.inbox_url,
                  follow_activity_id = EXCLUDED.follow_activity_id,
                  network = EXCLUDED.network,
                  decision = NULL,
                  resolved_at = NULL,
                  created_at = NOW()
            RETURNING *
            "#,
        )
        .bind(fan_out)
        .bind(user_id)
        .bind(inbox_url)
        .bind(follow_activity_id)
        .bind(network)
        .fetch_one(pool)
        .await
    }

    pub async fn find_pending(
        pool: &PgPool,
        user_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM follow_requests WHERE user_id = $1 AND decision IS NULL ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
    }

    /// Resolve a pending request. Returns the row when it was pending and is
    /// now resolved (exactly-once resolution); None when already resolved or
    /// missing.
    pub async fn resolve(
        pool: &PgPool,
        request_id: Uuid,
        user_id: Uuid,
        decision: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as(
            r#"
            UPDATE follow_requests
            SET decision = $3, resolved_at = NOW()
            WHERE id = $1 AND user_id = $2 AND decision IS NULL
            RETURNING *
            "#,
        )
        .bind(request_id)
        .bind(user_id)
        .bind(decision)
        .fetch_optional(pool)
        .await
    }

    /// The most recent request for a (follower, target) pair — used by the
    /// follow-button pending state and by Undo cleanup.
    pub async fn find_latest(
        pool: &PgPool,
        fan_out: &str,
        user_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM follow_requests WHERE fan_out = $1 AND user_id = $2")
            .bind(fan_out)
            .bind(user_id)
            .fetch_optional(pool)
            .await
    }

    /// Drop a pending request (follower canceled / Undo Follow).
    pub async fn delete_pending(
        pool: &PgPool,
        fan_out: &str,
        user_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM follow_requests WHERE fan_out = $1 AND user_id = $2 AND decision IS NULL")
            .bind(fan_out)
            .bind(user_id)
            .execute(pool)
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ap::testutil::{connect_pool, unique};

    /// A pending request can be resolved exactly once (approve), and a repeat
    /// resolution returns None (already resolved).
    #[tokio::test]
    async fn follow_request_resolves_exactly_once() {
        let Some(pool) = connect_pool().await else { return };
        let username = format!("fr_{}", unique());
        let user = crate::db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");
        let follower = format!("https://remote.test/users/f_{}", unique());

        let req = FollowRequestRow::create(
            &pool, &follower, user.id, Some(&format!("{follower}/inbox")),
            Some(&format!("{follower}/activities/follow-1")), "activitypub",
        )
        .await
        .expect("create");
        assert_eq!(req.decision, None, "starts pending");

        let resolved = FollowRequestRow::resolve(&pool, req.id, user.id, "approved")
            .await
            .expect("resolve")
            .expect("was pending");
        assert_eq!(resolved.decision.as_deref(), Some("approved"));

        // Second resolve -> None (exactly-once).
        let again = FollowRequestRow::resolve(&pool, req.id, user.id, "denied")
            .await
            .expect("resolve again");
        assert!(again.is_none(), "already-resolved request cannot be re-resolved");

        let _ = sqlx::query("DELETE FROM follow_requests WHERE user_id = $1")
            .bind(user.id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id).execute(&pool).await;
    }

    /// Creating a request while one is pending is an upsert that keeps it
    /// pending (no duplicate row per (follower, target)).
    #[tokio::test]
    async fn follow_request_upserts_pending_without_duplicates() {
        let Some(pool) = connect_pool().await else { return };
        let username = format!("fu_{}", unique());
        let user = crate::db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");
        let follower = format!("https://remote.test/users/g_{}", unique());

        for _ in 0..2 {
            FollowRequestRow::create(
                &pool, &follower, user.id, Some(&format!("{follower}/inbox")),
                Some(&format!("{follower}/activities/follow-1")), "activitypub",
            )
            .await
            .expect("create");
        }
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM follow_requests WHERE fan_out = $1 AND user_id = $2",
        )
        .bind(&follower).bind(user.id)
        .fetch_one(&pool).await.expect("count");
        assert_eq!(count, 1, "one pending request per (follower, target)");

        let pending = FollowRequestRow::find_pending(&pool, user.id).await.expect("pending");
        assert_eq!(pending.len(), 1, "listed as pending");

        let _ = sqlx::query("DELETE FROM follow_requests WHERE user_id = $1")
            .bind(user.id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id).execute(&pool).await;
    }
}
