//! Imported contacts from archive parsing.

use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ImportedContactRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub contact_name: String,
    pub contact_handle: Option<String>,
    pub contact_platform_url: Option<String>,
    pub source: String,
    pub import_session_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub category: Option<String>,
}

impl ImportedContactRow {
    pub async fn create(
        pool: &PgPool,
        user_id: Uuid,
        contact_name: &str,
        contact_handle: Option<&str>,
        contact_platform_url: Option<&str>,
        source: &str,
        import_session_id: Option<Uuid>,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO imported_contacts (user_id, contact_name, contact_handle, contact_platform_url, source, import_session_id)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(contact_name)
        .bind(contact_handle)
        .bind(contact_platform_url)
        .bind(source)
        .bind(import_session_id)
        .fetch_one(pool)
        .await
    }

    pub async fn create_batch(
        pool: &PgPool,
        contacts: &[NewContact],
    ) -> Result<u64, sqlx::Error> {
        if contacts.is_empty() {
            return Ok(0);
        }
        let mut count = 0u64;
        for c in contacts {
            let _ = sqlx::query(
                r#"
                INSERT INTO imported_contacts (user_id, contact_name, contact_handle, contact_platform_url, source, remote_handle, direction, import_session_id)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                "#,
            )
            .bind(c.user_id)
            .bind(&c.contact_name)
            .bind(c.contact_handle.as_deref())
            .bind(c.contact_platform_url.as_deref())
            .bind(&c.source)
            .bind(c.remote_handle.as_deref())
            .bind(c.direction.as_deref())
            .bind(c.import_session_id)
            .execute(pool)
            .await?;
            count += 1;
        }
        Ok(count)
    }

    pub async fn count_by_session(
        pool: &PgPool,
        session_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM imported_contacts WHERE import_session_id = $1",
        )
        .bind(session_id)
        .fetch_one(pool)
        .await
    }

    pub async fn count_by_session_and_user(
        pool: &PgPool,
        session_id: Uuid,
        user_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM imported_contacts WHERE import_session_id = $1 AND user_id = $2",
        )
        .bind(session_id)
        .bind(user_id)
        .fetch_one(pool)
        .await
    }

    pub async fn categorize_contacts(
        pool: &PgPool,
        session_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        // Mark matched contacts
        sqlx::query(
            r#"UPDATE imported_contacts SET category = 'matched'
               WHERE import_session_id = $1 AND user_id = $2
               AND EXISTS (
                   SELECT 1 FROM social_graph_matches
                   WHERE (user_a_id = $2 OR user_b_id = $2)
                   AND source = imported_contacts.source
               )"#,
        )
        .bind(session_id)
        .bind(user_id)
        .execute(pool)
        .await?;

        // Mark RSS candidates (Mastodon contacts with handles)
        sqlx::query(
            r#"UPDATE imported_contacts SET category = 'rss_candidate'
               WHERE import_session_id = $1 AND user_id = $2
               AND category = 'unreachable'
               AND source = 'mastodon'
               AND contact_handle IS NOT NULL"#,
        )
        .bind(session_id)
        .bind(user_id)
        .execute(pool)
        .await?;

        // Mark Reddit contacts as RSS candidates
        sqlx::query(
            r#"UPDATE imported_contacts SET category = 'rss_candidate'
               WHERE import_session_id = $1 AND user_id = $2
               AND category = 'unreachable'
               AND source = 'reddit'
               AND contact_handle IS NOT NULL"#,
        )
        .bind(session_id)
        .bind(user_id)
        .execute(pool)
        .await?;

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct NewContact {
    pub user_id: Uuid,
    pub contact_name: String,
    pub contact_handle: Option<String>,
    pub contact_platform_url: Option<String>,
    pub source: String,
    /// Canonical ActivityPub handle (`@user@instance`) — Mastodon archives only.
    pub remote_handle: Option<String>,
    /// Direction in the archive: "following" / "follower" (Mastodon only).
    pub direction: Option<String>,
    pub import_session_id: Option<Uuid>,
}
