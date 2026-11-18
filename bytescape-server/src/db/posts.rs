//! Post database operations.

use sqlx::PgPool;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PostRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub source: String,
    pub source_url: Option<String>,
    pub ap_object_id: Option<String>,
    pub content: Value,
    pub content_type: String,
    pub community: Option<String>,
    pub reply_parent_id: Option<Uuid>,
    pub reshared_post_id: Option<Uuid>,
    pub visibility: String,
    pub original_ts: chrono::DateTime<chrono::Utc>,
    pub imported_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_imported: bool,
    pub import_session_id: Option<Uuid>,
    pub likes_count: i32,
    pub shares_count: i32,
    pub replies_count: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub location: Option<String>,
    pub original_platform: String,
    pub import_batch_id: Option<Uuid>,
}

impl PostRow {
    pub async fn create(
        pool: &PgPool,
        user_id: Uuid,
        source: &str,
        source_url: Option<&str>,
        ap_object_id: Option<&str>,
        content: &Value,
        content_type: &str,
        community: Option<&str>,
        reply_parent_id: Option<Uuid>,
        reshared_post_id: Option<Uuid>,
        original_ts: chrono::DateTime<chrono::Utc>,
        is_imported: bool,
        import_session_id: Option<Uuid>,
        visibility: &str,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO posts
                (user_id, source, source_url, ap_object_id, content, content_type,
                 community, reply_parent_id, reshared_post_id, original_ts, is_imported, import_session_id,
                 visibility)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(source)
        .bind(source_url)
        .bind(ap_object_id)
        .bind(content)
        .bind(content_type)
        .bind(community)
        .bind(reply_parent_id)
        .bind(reshared_post_id)
        .bind(original_ts)
        .bind(is_imported)
        .bind(import_session_id)
        .bind(visibility)
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM posts WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await
    }

    pub async fn find_by_ap_object_id(pool: &PgPool, ap_object_id: &str) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM posts WHERE ap_object_id = $1")
            .bind(ap_object_id)
            .fetch_optional(pool)
            .await
    }

    /// Paginated feed for a user.
    pub async fn feed(
        pool: &PgPool,
        user_id: Uuid,
        limit: i64,
        offset: i64,
        content_type: Option<&str>,
        source: Option<&str>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let mut sql = String::from(
            "SELECT p.* FROM posts p WHERE p.user_id = $1 AND p.visibility = 'public' "
        );
        let mut param_idx = 2;
        if let Some(_ct) = content_type {
            sql.push_str(&format!(" AND p.content_type = ${} ", param_idx));
            param_idx += 1;
        }
        if let Some(_s) = source {
            sql.push_str(&format!(" AND p.source = ${} ", param_idx));
            param_idx += 1;
        }
        sql.push_str(&format!("ORDER BY p.original_ts DESC LIMIT ${} OFFSET ${}", param_idx, param_idx + 1));

        let mut q = sqlx::query_as::<_, Self>(&sql).bind(user_id);
        if let Some(ct) = content_type {
            q = q.bind(ct);
        }
        if let Some(s) = source {
            q = q.bind(s);
        }
        q.bind(limit).bind(offset).fetch_all(pool).await
    }

    /// Public feed for a user's profile page.
    pub async fn public_feed(
        pool: &PgPool,
        user_id: Uuid,
        limit: i64,
        offset: i64,
        source_filter: Option<&str>,
        date_from: Option<chrono::DateTime<chrono::Utc>>,
        date_to: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let mut sql = String::from(
            "SELECT * FROM posts WHERE user_id = $1 AND visibility = 'public' "
        );
        let mut param_idx = 2;
        if let Some(_s) = source_filter {
            sql.push_str(&format!(" AND source = ${} ", param_idx));
            param_idx += 1;
        }
        if date_from.is_some() {
            sql.push_str(&format!(" AND original_ts >= ${} ", param_idx));
            param_idx += 1;
        }
        if date_to.is_some() {
            sql.push_str(&format!(" AND original_ts <= ${} ", param_idx));
            param_idx += 1;
        }
        sql.push_str(&format!("ORDER BY original_ts DESC LIMIT ${} OFFSET ${}", param_idx, param_idx + 1));

        let mut q = sqlx::query_as::<_, Self>(&sql).bind(user_id);
        if let Some(s) = source_filter {
            q = q.bind(s);
        }
        if let Some(df) = date_from {
            q = q.bind(df);
        }
        if let Some(dt) = date_to {
            q = q.bind(dt);
        }
        q.bind(limit).bind(offset).fetch_all(pool).await
    }

    /// Count public posts for a user (for outbox totalItems).
    pub async fn public_feed_count(
        pool: &PgPool,
        user_id: Uuid,
        source_filter: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        let mut sql = String::from(
            "SELECT COUNT(*) FROM posts WHERE user_id = $1 AND visibility = 'public' "
        );
        if let Some(_s) = source_filter {
            sql.push_str(" AND source = $2 ");
        }

        let mut q = sqlx::query_scalar::<_, i64>(&sql).bind(user_id);
        if let Some(s) = source_filter {
            q = q.bind(s);
        }
        q.fetch_one(pool).await
    }

    pub async fn delete(
        pool: &PgPool,
        id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM posts WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Delete a post by ID only (for federation: inbound Delete activities).
    pub async fn delete_by_id(pool: &PgPool, post_id: Uuid) -> Result<u64, sqlx::Error> {
        sqlx::query("DELETE FROM posts WHERE id = $1")
            .bind(post_id)
            .execute(pool)
            .await
            .map(|r| r.rows_affected())
    }
}
