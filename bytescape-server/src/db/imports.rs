//! Import session tracking.

use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ImportRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub source: String,
    pub status: String,
    pub total_posts: i32,
    pub imported_posts: i32,
    pub total_media: i32,
    pub imported_media: i32,
    pub error_message: Option<String>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl ImportRow {
    pub async fn create(
        pool: &PgPool,
        user_id: Uuid,
        source: &str,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO import_sessions (user_id, source)
            VALUES ($1, $2)
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(source)
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM import_sessions WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await
    }

    pub async fn list_by_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM import_sessions WHERE user_id = $1 ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
    }

    pub async fn start(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE import_sessions
            SET status = 'running', started_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn set_progress(
        pool: &PgPool,
        id: Uuid,
        imported_posts: i32,
        imported_media: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE import_sessions
            SET imported_posts = $1, imported_media = $2
            WHERE id = $3
            "#,
        )
        .bind(imported_posts)
        .bind(imported_media)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn complete(
        pool: &PgPool,
        id: Uuid,
        total_posts: i32,
        total_media: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE import_sessions
            SET status = 'completed', total_posts = $1, total_media = $2,
                imported_posts = $1, imported_media = $2, completed_at = NOW()
            WHERE id = $3
            "#,
        )
        .bind(total_posts)
        .bind(total_media)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn fail(pool: &PgPool, id: Uuid, error: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE import_sessions
            SET status = 'failed', error_message = $1, completed_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(error)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Complete an import that saw more entries than it could import (skipped
    /// malformed entries). Sets distinct total/imported counts and records the
    /// skipped-entry errors in session metadata (import-pipeline-verification
    /// R4/R5).
    pub async fn complete_with_errors(
        pool: &PgPool,
        id: Uuid,
        total_posts: i32,
        imported_posts: i32,
        total_media: i32,
        imported_media: i32,
        errors: &[String],
    ) -> Result<(), sqlx::Error> {
        let metadata = serde_json::json!({
            "skipped": errors,
            "error_count": errors.len(),
        });
        sqlx::query(
            r#"
            UPDATE import_sessions
            SET status = 'completed', total_posts = $1, imported_posts = $2,
                total_media = $3, imported_media = $4,
                error_message = $5,
                metadata = metadata || $6::jsonb,
                completed_at = NOW()
            WHERE id = $7
            "#,
        )
        .bind(total_posts)
        .bind(imported_posts)
        .bind(total_media)
        .bind(imported_media)
        .bind(if errors.is_empty() { None } else { Some(errors.join("; ")) })
        .bind(metadata)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }
}
