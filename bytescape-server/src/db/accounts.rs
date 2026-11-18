//! `linked_accounts` — home-instance OAuth grants (Mastodon/Bluesky).
//!
//! SEC-1 (CWE-312): `access_token`, `refresh_token` and `dpop_private_key` are
//! **sealed at rest** with chacha20poly1305 (`crate::secret`, `enc:v1:` format;
//! key = `Sha256("bytescape.linked_accounts.v1" || JWT_SECRET)`). Writes seal
//! before binding ([`LinkedAccountRow::create`]); reads decrypt in the manual
//! [`sqlx::FromRow`] impl below — the single decode point shared by every
//! `query_as::<LinkedAccountRow>` in the crate (including raw `SELECT *`
//! sites), so the struct keeps plaintext fields and zero token consumers
//! change. Rows written before the rollout stay readable (`crate::secret::open`
//! passes legacy plaintext through) and are sealed at startup by
//! [`encrypt_plaintext_tokens`].

use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct LinkedAccountRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub provider: String,
    pub external_id: String,
    pub external_username: String,
    pub token_type: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub scope: Option<String>,
    pub dpop_private_key: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Decode `linked_accounts` rows, decrypting the sealed token columns on the
/// way out. Manual impl (not `derive(FromRow)`) so this stays the ONE read
/// choke point for every `query_as` decode of the struct.
impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for LinkedAccountRow {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        let secret = jwt_secret();
        let decode_err = |col: &str, e: String| -> sqlx::Error {
            sqlx::Error::ColumnDecode { index: col.to_string(), source: e.into() }
        };
        // Tolerant open: legacy plaintext rows pass through unchanged.
        let open = |col: &str| -> Result<String, sqlx::Error> {
            let v: String = row.try_get(col)?;
            crate::secret::open(&v, &secret)
                .map_err(|e| decode_err(col, format!("linked_accounts.{col}: {e}")))
        };
        let open_opt = |col: &str| -> Result<Option<String>, sqlx::Error> {
            let v: Option<String> = row.try_get(col)?;
            v.map(|v| {
                crate::secret::open(&v, &secret)
                    .map_err(|e| decode_err(col, format!("linked_accounts.{col}: {e}")))
            })
            .transpose()
        };
        Ok(LinkedAccountRow {
            id: row.try_get("id")?,
            user_id: row.try_get("user_id")?,
            provider: row.try_get("provider")?,
            external_id: row.try_get("external_id")?,
            external_username: row.try_get("external_username")?,
            token_type: row.try_get("token_type")?,
            access_token: open("access_token")?,
            refresh_token: open_opt("refresh_token")?,
            token_expires_at: row.try_get("token_expires_at")?,
            scope: row.try_get("scope")?,
            dpop_private_key: open_opt("dpop_private_key")?,
            metadata: row.try_get("metadata")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}

/// `JWT_SECRET` — the AEAD root secret (`crate::secret`). `FromRow` cannot
/// reach `AppState`, so it is read from the environment; every deployment
/// (and the test env loaded from `.env`) has it — `config.rs` makes it a
/// mandatory env with no default. Falls back to `""` only in degenerate
/// configs, which makes sealed values fail to decrypt loudly rather than
/// silently serve garbage credentials.
pub(crate) fn jwt_secret() -> String {
    std::env::var("JWT_SECRET").unwrap_or_default()
}

impl LinkedAccountRow {
    /// Insert-or-update a link. `access_token`, `refresh_token` and
    /// `dpop_private_key` are sealed before binding — only `enc:v1:` ciphertext
    /// ever reaches the database (NULL stays NULL). The returned row (via
    /// `RETURNING *` + [`FromRow`]) carries the plaintext fields back to the
    /// caller.
    pub async fn create(
        pool: &PgPool,
        user_id: Uuid,
        provider: &str,
        external_id: &str,
        external_username: &str,
        access_token: &str,
        refresh_token: Option<&str>,
        token_expires_at: Option<chrono::DateTime<chrono::Utc>>,
        dpop_private_key: Option<&str>,
        scope: Option<&str>,
        metadata: &serde_json::Value,
    ) -> Result<Self, sqlx::Error> {
        let secret = jwt_secret();
        let access_token = crate::secret::seal(access_token, &secret);
        let refresh_token = refresh_token.map(|t| crate::secret::seal(t, &secret));
        let dpop_private_key = dpop_private_key.map(|k| crate::secret::seal(k, &secret));
        sqlx::query_as(
            r#"
            INSERT INTO linked_accounts
                (user_id, provider, external_id, external_username, access_token, refresh_token,
                 token_expires_at, dpop_private_key, scope, metadata)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (user_id, provider, external_id)
            DO UPDATE SET
                access_token = EXCLUDED.access_token,
                refresh_token = EXCLUDED.refresh_token,
                token_expires_at = EXCLUDED.token_expires_at,
                dpop_private_key = EXCLUDED.dpop_private_key,
                scope = EXCLUDED.scope,
                metadata = EXCLUDED.metadata,
                updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(provider)
        .bind(external_id)
        .bind(external_username)
        .bind(access_token)
        .bind(refresh_token)
        .bind(token_expires_at)
        .bind(dpop_private_key)
        .bind(scope)
        .bind(metadata)
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM linked_accounts WHERE user_id = $1 ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_provider(
        pool: &PgPool,
        user_id: Uuid,
        provider: &str,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM linked_accounts WHERE user_id = $1 AND provider = $2 ORDER BY created_at DESC",
        )
        .bind(user_id)
        .bind(provider)
        .fetch_all(pool)
        .await
    }

    pub async fn find_all_by_provider(
        pool: &PgPool,
        provider: &str,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM linked_accounts WHERE provider = $1 ORDER BY created_at DESC",
        )
        .bind(provider)
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM linked_accounts WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await
    }

    /// Count a user's links for one provider — the one-per-network guard
    /// (at most one Bluesky link and one Mastodon link per Bytescape account).
    pub async fn count_by_user_provider(
        pool: &PgPool,
        user_id: Uuid,
        provider: &str,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM linked_accounts WHERE user_id = $1 AND provider = $2",
        )
        .bind(user_id)
        .bind(provider)
        .fetch_one(pool)
        .await
    }

    pub async fn delete(pool: &PgPool, id: Uuid, user_id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM linked_accounts WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }
}

/// Seal any legacy plaintext `linked_accounts` token columns in place and
/// return the number of rows re-bound. Idempotent: a row/column already
/// carrying the `enc:v1:` marker (or NULL) is skipped, so a second run
/// changes 0 rows.
///
/// **This Rust backfill IS the migration for existing rows** — a `.sql`
/// migration cannot run chacha20poly1305 (the seal key is derived in-process
/// from `JWT_SECRET`), so the encryption of pre-existing plaintext rows
/// happens here, at startup (`main.rs`, right after pool setup), instead of
/// in a migration file. Rows written after the rollout are already sealed by
/// [`LinkedAccountRow::create`]; this never decrypts and never touches a
/// sealed value.
pub async fn encrypt_plaintext_tokens(pool: &PgPool, jwt_secret: &str) -> Result<u64, String> {
    let cols = ["access_token", "refresh_token", "dpop_private_key"];
    let mut changed = 0u64;
    for col in cols {
        // Static identifiers only — `col` is a literal from the list above.
        let rows = sqlx::query_as::<_, (Uuid, String)>(&format!(
            "SELECT id, {col} FROM linked_accounts \
             WHERE {col} IS NOT NULL AND {col} NOT LIKE 'enc:v1:%'"
        ))
        .fetch_all(pool)
        .await
        .map_err(|e| format!("encrypt_plaintext_tokens: select {col}: {e}"))?;
        for (id, plain) in rows {
            let sealed = crate::secret::seal(&plain, jwt_secret);
            sqlx::query(&format!(
                "UPDATE linked_accounts SET {col} = $1 WHERE id = $2"
            ))
            .bind(&sealed)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|e| format!("encrypt_plaintext_tokens: update {col} for {id}: {e}"))?;
            changed += 1;
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ap::testutil::{connect_pool, unique};

    /// SEC-1: home-instance OAuth secrets are sealed at rest (chacha20poly1305,
    /// `enc:v1:` format) — a raw SELECT sees ciphertext while every read path
    /// hands consumers the original plaintext. Covers access_token,
    /// refresh_token and dpop_private_key.
    #[tokio::test]
    async fn linked_account_tokens_are_encrypted_at_rest() {
        let Some(pool) = connect_pool().await else {
            return;
        };
        let suffix = unique();
        let user =
            crate::db::users::UserRow::create_active(&pool, &format!("sec1_{suffix}"), "", "x", "x")
                .await
                .expect("create user");
        let access = format!("at-{suffix}");
        let refresh = format!("rt-{suffix}");
        let dpop = format!("dpop-{suffix}");
        let row = LinkedAccountRow::create(
            &pool,
            user.id,
            "mastodon",
            &format!("sec1_{suffix}@home.test"),
            "sec1",
            &access,
            Some(&refresh),
            None,
            Some(&dpop),
            Some("read write:favourites"),
            &serde_json::json!({}),
        )
        .await
        .expect("create linked account");

        // At rest: raw SQL sees sealed ciphertext, never the plaintext secrets.
        let (raw_at, raw_rt, raw_dpop): (String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT access_token, refresh_token, dpop_private_key FROM linked_accounts WHERE id = $1",
        )
        .bind(row.id)
        .fetch_one(&pool)
        .await
        .expect("raw select");
        for (name, stored, plain) in [
            ("access_token", raw_at.as_str(), access.as_str()),
            ("refresh_token", raw_rt.as_deref().unwrap_or(""), refresh.as_str()),
            ("dpop_private_key", raw_dpop.as_deref().unwrap_or(""), dpop.as_str()),
        ] {
            assert!(stored.starts_with("enc:v1:"), "{name} not sealed at rest: {stored}");
            assert_ne!(stored, plain, "{name} stored as plaintext");
        }

        // Through the read path: the original plaintext comes back.
        let read = LinkedAccountRow::find_by_provider(&pool, user.id, "mastodon")
            .await
            .expect("read back")
            .into_iter()
            .find(|a| a.id == row.id)
            .expect("row");
        assert_eq!(read.access_token, access);
        assert_eq!(read.refresh_token.as_deref(), Some(refresh.as_str()));
        assert_eq!(read.dpop_private_key.as_deref(), Some(dpop.as_str()));

        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await;
    }

    /// SEC-1 backfill: legacy plaintext rows are sealed in place and keep
    /// reading back as the original plaintext; a second run is a no-op
    /// (idempotent).
    #[tokio::test]
    async fn encrypt_plaintext_tokens_migrates_legacy_rows_and_is_idempotent() {
        let Some(pool) = connect_pool().await else {
            return;
        };
        let suffix = unique();
        let user =
            crate::db::users::UserRow::create_active(&pool, &format!("sec1m_{suffix}"), "", "x", "x")
                .await
                .expect("create user");
        let access = format!("legacy-at-{suffix}");
        let refresh = format!("legacy-rt-{suffix}");
        let dpop = format!("legacy-dpop-{suffix}");
        // Legacy-shape row: plaintext secrets, exactly what pre-rollout code wrote.
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO linked_accounts \
             (id, user_id, provider, external_id, external_username, token_type, \
              access_token, refresh_token, dpop_private_key, scope, metadata) \
             VALUES ($1, $2, 'mastodon', $3, 'legacy', 'oauth', $4, $5, $6, 'read', '{}')",
        )
        .bind(id)
        .bind(user.id)
        .bind(format!("sec1m_{suffix}@home.test"))
        .bind(&access)
        .bind(&refresh)
        .bind(&dpop)
        .execute(&pool)
        .await
        .expect("insert legacy row");

        let secret = std::env::var("JWT_SECRET").unwrap_or_default();
        let changed = encrypt_plaintext_tokens(&pool, &secret).await.expect("backfill");
        assert!(changed >= 3, "the three plaintext columns must be sealed (changed={changed})");

        // Stored values are sealed now — and read back as the originals.
        let (raw_at, raw_rt, raw_dpop): (String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT access_token, refresh_token, dpop_private_key FROM linked_accounts WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("raw select");
        for (name, stored, plain) in [
            ("access_token", raw_at.as_str(), access.as_str()),
            ("refresh_token", raw_rt.as_deref().unwrap_or(""), refresh.as_str()),
            ("dpop_private_key", raw_dpop.as_deref().unwrap_or(""), dpop.as_str()),
        ] {
            assert!(stored.starts_with("enc:v1:"), "{name} not sealed after backfill: {stored}");
            assert_ne!(stored, plain, "{name} still plaintext after backfill");
        }
        let read = LinkedAccountRow::find_by_id(&pool, id)
            .await
            .expect("read back")
            .expect("row");
        assert_eq!(read.access_token, access);
        assert_eq!(read.refresh_token.as_deref(), Some(refresh.as_str()));
        assert_eq!(read.dpop_private_key.as_deref(), Some(dpop.as_str()));

        // Second run: nothing left to migrate.
        let again = encrypt_plaintext_tokens(&pool, &secret).await.expect("second backfill");
        assert_eq!(again, 0, "backfill must be idempotent");

        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await;
    }
}
