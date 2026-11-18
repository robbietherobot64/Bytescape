//! Outgoing follows / subscriptions.

use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FollowRow {
    pub id: Uuid,
    pub follower_id: Uuid,
    pub followed_actor: String,
    pub follow_type: String,
    pub label: Option<String>,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Which identity made this write (AT DID, minted actor URL, home acct) —
    /// the provenance column written by [`FollowRow::set_identity`].
    pub source_platform: Option<String>,
}

impl FollowRow {
    pub async fn create(
        pool: &PgPool,
        follower_id: Uuid,
        followed_actor: &str,
        follow_type: &str,
        label: Option<&str>,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO follows (follower_id, followed_actor, follow_type, label)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (follower_id, followed_actor) DO UPDATE
                SET status = 'pending', follow_type = $3, label = $4
            RETURNING *
            "#,
        )
        .bind(follower_id)
        .bind(followed_actor)
        .bind(follow_type)
        .bind(label)
        .fetch_one(pool)
        .await
    }

    pub async fn set_accepted(pool: &PgPool, follower_id: Uuid, followed_actor: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE follows SET status = 'accepted' WHERE follower_id = $1 AND followed_actor = $2")
            .bind(follower_id)
            .bind(followed_actor)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Record which identity made this write (`follows.source_platform`) so
    /// undo routes back through the same adapter and the merged read model can
    /// attribute the row (identity-graph Phase 2): the AT DID, the minted AP
    /// actor URL, or the home-instance acct — `NULL` for local-only follows.
    pub async fn set_identity(
        pool: &PgPool,
        follower_id: Uuid,
        followed_actor: &str,
        identity_used: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE follows SET source_platform = $3 WHERE follower_id = $1 AND followed_actor = $2",
        )
        .bind(follower_id)
        .bind(followed_actor)
        .bind(identity_used)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn list_by_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM follows WHERE follower_id = $1 ORDER BY created_at DESC")
            .bind(user_id)
            .fetch_all(pool)
            .await
    }

    pub async fn delete(
        pool: &PgPool,
        id: Uuid,
        follower_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let r = sqlx::query("DELETE FROM follows WHERE id = $1 AND follower_id = $2")
            .bind(id)
            .bind(follower_id)
            .execute(pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    pub async fn set_status(
        pool: &PgPool,
        id: Uuid,
        status: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE follows SET status = $1 WHERE id = $2")
            .bind(status)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn find_by_follower_and_actor(
        pool: &PgPool,
        follower_id: Uuid,
        followed_actor: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM follows WHERE follower_id = $1 AND followed_actor = $2",
        )
        .bind(follower_id)
        .bind(followed_actor)
        .fetch_optional(pool)
        .await
    }

    pub async fn delete_by_actor(
        pool: &PgPool,
        follower_id: Uuid,
        followed_actor: &str,
    ) -> Result<bool, sqlx::Error> {
        let r = sqlx::query(
            "DELETE FROM follows WHERE follower_id = $1 AND followed_actor = $2",
        )
        .bind(follower_id)
        .bind(followed_actor)
        .execute(pool)
        .await?;
        Ok(r.rows_affected() > 0)
    }

    pub async fn find_followed_actors(
        pool: &PgPool,
        follower_id: Uuid,
        actor_urls: &[String],
    ) -> Result<Vec<String>, sqlx::Error> {
        if actor_urls.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT followed_actor FROM follows WHERE follower_id = $1 AND followed_actor = ANY($2)",
        )
        .bind(follower_id)
        .bind(actor_urls)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }
}
