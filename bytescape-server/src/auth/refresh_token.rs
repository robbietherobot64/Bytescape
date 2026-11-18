use base64::Engine;
use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppError;

#[derive(Debug, sqlx::FromRow)]
pub struct RefreshTokenRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub family_id: Uuid,
    pub expires_at: chrono::DateTime<Utc>,
    pub revoked: bool,
}

pub struct RefreshTokenManager {
    pub db: PgPool,
}

impl RefreshTokenManager {
    pub async fn create(&self, user_id: Uuid) -> Result<(String, Uuid), AppError> {
        let raw = generate_token();
        let hash = hash_token(&raw);
        let family_id = Uuid::new_v4();
        let expires_at = Utc::now() + Duration::days(30);

        sqlx::query(
            r#"
            INSERT INTO refresh_tokens (user_id, token_hash, family_id, expires_at)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(user_id)
        .bind(&hash)
        .bind(family_id)
        .bind(expires_at)
        .execute(&self.db)
        .await?;

        Ok((raw, family_id))
    }

    pub async fn validate(&self, raw_token: &str) -> Result<RefreshTokenRecord, AppError> {
        let hash = hash_token(raw_token);
        let record = sqlx::query_as::<_, RefreshTokenRecord>(
            r#"
            SELECT id, user_id, family_id, expires_at, revoked
            FROM refresh_tokens
            WHERE token_hash = $1
            "#,
        )
        .bind(&hash)
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| AppError::Auth("Invalid refresh token".into()))?;

        if record.revoked {
            return Err(AppError::Auth("Refresh token revoked".into()));
        }
        if record.expires_at < Utc::now() {
            return Err(AppError::Auth("Refresh token expired".into()));
        }

        Ok(record)
    }

    pub async fn get_username(&self, user_id: Uuid) -> Result<String, AppError> {
        let user = sqlx::query_as::<_, (String,)>("SELECT username FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&self.db)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".into()))?;
        Ok(user.0)
    }

    pub async fn rotate(&self, raw_token: &str) -> Result<(String, Uuid), AppError> {
        let old = self.validate(raw_token).await?;

        sqlx::query(
            "UPDATE refresh_tokens SET revoked = TRUE, revoked_at = NOW() WHERE family_id = $1",
        )
        .bind(old.family_id)
        .execute(&self.db)
        .await?;

        self.create_with_family(old.user_id, old.family_id).await
    }

    async fn create_with_family(&self, user_id: Uuid, family_id: Uuid) -> Result<(String, Uuid), AppError> {
        let raw = generate_token();
        let hash = hash_token(&raw);
        let expires_at = Utc::now() + Duration::days(30);

        sqlx::query(
            r#"
            INSERT INTO refresh_tokens (user_id, token_hash, family_id, expires_at)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(user_id)
        .bind(&hash)
        .bind(family_id)
        .bind(expires_at)
        .execute(&self.db)
        .await?;

        Ok((raw, family_id))
    }

    pub async fn revoke_all_for_user(&self, user_id: Uuid) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE refresh_tokens SET revoked = TRUE, revoked_at = NOW() WHERE user_id = $1 AND revoked = FALSE",
        )
        .bind(user_id)
        .execute(&self.db)
        .await?;
        Ok(())
    }
}

fn generate_token() -> String {
    let bytes: [u8; 32] = rand::random();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}
