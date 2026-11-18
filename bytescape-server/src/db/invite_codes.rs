use sqlx::PgPool;
use uuid::Uuid;

pub struct InviteCodeRow {
    pub id: Uuid,
    pub code: String,
    pub max_uses: i32,
    pub use_count: i32,
    pub is_active: bool,
}

impl InviteCodeRow {
    /// Validate and consume an invite code. Returns Ok(()) if valid.
    pub async fn validate_and_consume(pool: &PgPool, code: &str, user_id: Uuid) -> Result<(), sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE invite_codes
               SET use_count = use_count + 1,
                   used_at = CASE WHEN use_count + 1 >= max_uses THEN now() ELSE used_at END,
                   used_by = CASE WHEN use_count + 1 >= max_uses THEN $2 ELSE used_by END,
                   is_active = CASE WHEN use_count + 1 >= max_uses THEN false ELSE is_active END
               WHERE code = $1 AND is_active = true AND use_count < max_uses"#,
        )
        .bind(code)
        .bind(user_id)
        .execute(pool)
        .await?;

        if result.rows_affected() > 0 {
            Ok(())
        } else {
            Err(sqlx::Error::RowNotFound)
        }
    }

    /// Check if a code is valid without consuming it.
    pub async fn is_valid(pool: &PgPool, code: &str) -> Result<bool, sqlx::Error> {
        let row: (bool,) = sqlx::query_as(
            r#"SELECT EXISTS(SELECT 1 FROM invite_codes WHERE code = $1 AND is_active = true AND use_count < max_uses)"#,
        )
        .bind(code)
        .fetch_one(pool)
        .await?;
        Ok(row.0)
    }

    /// Generate a new invite code.
    pub async fn create(pool: &PgPool, created_by: Option<Uuid>, max_uses: i32) -> Result<String, sqlx::Error> {
        let code = generate_code();
        sqlx::query(
            r#"INSERT INTO invite_codes (code, created_by, max_uses) VALUES ($1, $2, $3)"#,
        )
        .bind(&code)
        .bind(created_by)
        .bind(max_uses)
        .execute(pool)
        .await?;
        Ok(code)
    }

    /// List all invite codes.
    pub async fn list(pool: &PgPool) -> Result<Vec<InviteCodeInfo>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, i32, i32, bool, Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>)>(
            r#"SELECT code, max_uses, use_count, is_active, used_at FROM invite_codes ORDER BY created_at DESC"#,
        )
        .fetch_all(pool)
        .await?;
        Ok(rows.iter().map(|r| InviteCodeInfo {
            code: r.0.clone(),
            max_uses: r.1,
            use_count: r.2,
            is_active: r.3,
            used_at: r.4.map(|t| t.to_rfc3339()),
        }).collect())
    }
}

pub struct InviteCodeInfo {
    pub code: String,
    pub max_uses: i32,
    pub use_count: i32,
    pub is_active: bool,
    pub used_at: Option<String>,
}

fn generate_code() -> String {
    use rand::Rng;
    let chars: Vec<char> = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789".chars().collect();
    (0..16).map(|_| chars[rand::thread_rng().gen_range(0..chars.len())]).collect()
}