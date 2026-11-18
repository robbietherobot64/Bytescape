use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PostingIdentityRow {
    pub user_id: Uuid,
    pub protocol: String,
    pub identity_type: String,
    pub linked_account_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl PostingIdentityRow {
    pub async fn find_by_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM posting_identities WHERE user_id = $1 ORDER BY protocol",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
    }

    pub async fn upsert(
        pool: &PgPool,
        user_id: Uuid,
        protocol: &str,
        identity_type: &str,
        linked_account_id: Option<Uuid>,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO posting_identities (user_id, protocol, identity_type, linked_account_id)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id, protocol)
            DO UPDATE SET
                identity_type = EXCLUDED.identity_type,
                linked_account_id = EXCLUDED.linked_account_id,
                updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(protocol)
        .bind(identity_type)
        .bind(linked_account_id)
        .fetch_one(pool)
        .await
    }

    pub async fn get_for_protocol(
        pool: &PgPool,
        user_id: Uuid,
        protocol: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM posting_identities WHERE user_id = $1 AND protocol = $2",
        )
        .bind(user_id)
        .bind(protocol)
        .fetch_optional(pool)
        .await
    }

    pub async fn reset_to_bytescape(
        pool: &PgPool,
        user_id: Uuid,
        protocol: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE posting_identities SET identity_type = 'bytescape', linked_account_id = NULL, updated_at = NOW() WHERE user_id = $1 AND protocol = $2",
        )
        .bind(user_id)
        .bind(protocol)
        .execute(pool)
        .await?;
        Ok(())
    }
}
