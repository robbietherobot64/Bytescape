use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReactionRow {
    pub id: Uuid,
    pub post_id: String,
    pub user_id: Uuid,
    pub emoji: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub metadata: serde_json::Value,
}

impl ReactionRow {
    /// Upsert a reaction for a user on a post (each user can only have one reaction per post).
    pub async fn upsert(
        pool: &PgPool,
        post_id: &str,
        user_id: Uuid,
        emoji: &str,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO reactions (post_id, user_id, emoji)
            VALUES ($1, $2, $3)
            ON CONFLICT (post_id, user_id)
            DO UPDATE SET emoji = EXCLUDED.emoji, retracted_at = NULL, updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(post_id)
        .bind(user_id)
        .bind(emoji)
        .fetch_one(pool)
        .await
    }

    /// Get counts per emoji for a list of posts.
    pub async fn counts_for_posts(
        pool: &PgPool,
        post_ids: &[String],
    ) -> Result<HashMap<String, HashMap<String, i64>>, sqlx::Error> {
        let rows: Vec<(String, String, i64)> = sqlx::query_as(
            r#"
            SELECT post_id, emoji, COUNT(*) as cnt
            FROM reactions
            WHERE post_id = ANY($1) AND retracted_at IS NULL
            GROUP BY post_id, emoji
            "#,
        )
        .bind(post_ids)
        .fetch_all(pool)
        .await?;

        let mut result: HashMap<String, HashMap<String, i64>> = HashMap::new();
        for (post_id, emoji, cnt) in rows {
            result
                .entry(post_id)
                .or_default()
                .insert(emoji, cnt);
        }
        Ok(result)
    }

    /// Get the current user's reaction for a single post.
    pub async fn find(
        pool: &PgPool,
        post_id: &str,
        user_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as(
            r#"
            SELECT * FROM reactions WHERE post_id = $1 AND user_id = $2 AND retracted_at IS NULL
            "#,
        )
        .bind(post_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
    }

    /// Soft-delete a user's reaction on a post (each user can only have one
    /// reaction per post). The row flips to its retraction shadow
    /// (`retracted_at` = now, emoji cleared) instead of disappearing — the
    /// 30s repeat-click window needs it (CR-2/CR-10: ap_activities' shadow
    /// covers only the minted-AP cohort, and at:// has none at all). Shadows
    /// older than a day are opportunistically purged here (the window is 30s).
    /// Returns true if a row was removed.
    pub async fn delete(
        pool: &PgPool,
        post_id: &str,
        user_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let n = sqlx::query(
            r#"
            UPDATE reactions SET retracted_at = now(), emoji = '', updated_at = now()
            WHERE post_id = $1 AND user_id = $2 AND retracted_at IS NULL
            "#,
        )
        .bind(post_id)
        .bind(user_id)
        .execute(pool)
        .await?
        .rows_affected();
        sqlx::query("DELETE FROM reactions WHERE retracted_at < now() - interval '1 day'")
            .execute(pool)
            .await?;
        Ok(n)
    }

    /// Was this user's reaction of `post_id` removed within the last 30
    /// seconds? The shadow half of the legacy double-click collapse
    /// (see `api/feed/mod.rs::recently_retracted`) — identity-agnostic, so
    /// home-REST (CR-2) and at:// (CR-10) reactions are covered too.
    pub async fn recently_retracted(
        pool: &PgPool,
        user_id: Uuid,
        post_id: &str,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM reactions \
             WHERE user_id = $1 AND post_id = $2 \
               AND retracted_at > now() - interval '30 seconds')",
        )
        .bind(user_id)
        .bind(post_id)
        .fetch_one(pool)
        .await
    }

    /// Get the current user's reaction for a list of posts.
    pub async fn user_reactions_for_posts(
        pool: &PgPool,
        post_ids: &[String],
        user_id: Uuid,
    ) -> Result<HashMap<String, String>, sqlx::Error> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT post_id, emoji
            FROM reactions
            WHERE post_id = ANY($1) AND user_id = $2 AND retracted_at IS NULL
            "#,
        )
        .bind(post_ids)
        .bind(user_id)
        .fetch_all(pool)
        .await?;

        Ok(rows.into_iter().collect())
    }

    /// All standing reactions of one user (full rows incl. metadata) — the
    /// deletion-time home-REST retraction sweep walks these (retraction
    /// shadows excluded).
    pub async fn all_for_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            r#"
            SELECT * FROM reactions WHERE user_id = $1 AND retracted_at IS NULL
            "#,
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
    }

    /// Find a reaction by user + post, returning the full row including metadata.
    pub async fn find_by_user_post_platform(
        pool: &PgPool,
        user_id: Uuid,
        post_id: &str,
        _platform: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as(
            r#"
            SELECT * FROM reactions WHERE user_id = $1 AND post_id = $2 AND retracted_at IS NULL
            "#,
        )
        .bind(user_id)
        .bind(post_id)
        .fetch_optional(pool)
        .await
    }
}

/// Update the metadata JSONB for a user's reaction on a post.
pub async fn update_metadata(
    pool: &PgPool,
    user_id: Uuid,
    post_id: &str,
    _platform: &str,
    metadata: serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE reactions SET metadata = metadata || $1::jsonb, updated_at = NOW()
        WHERE user_id = $2 AND post_id = $3 AND retracted_at IS NULL
        "#,
    )
    .bind(metadata)
    .bind(user_id)
    .bind(post_id)
    .execute(pool)
    .await?;
    Ok(())
}
