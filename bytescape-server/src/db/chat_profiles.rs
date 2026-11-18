use sqlx::PgPool;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ChatProfileRow {
    pub platform: String,
    pub did: String,
    pub display_name: Option<String>,
    pub handle: Option<String>,
    pub avatar: Option<String>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl ChatProfileRow {
    pub async fn upsert(
        pool: &PgPool,
        platform: &str,
        did: &str,
        display_name: Option<&str>,
        handle: Option<&str>,
        avatar: Option<&str>,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO chat_profiles (platform, did, display_name, handle, avatar)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (platform, did) DO UPDATE SET
                display_name = COALESCE(EXCLUDED.display_name, chat_profiles.display_name),
                handle = COALESCE(EXCLUDED.handle, chat_profiles.handle),
                avatar = COALESCE(EXCLUDED.avatar, chat_profiles.avatar),
                updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(platform)
        .bind(did)
        .bind(display_name)
        .bind(handle)
        .bind(avatar)
        .fetch_one(pool)
        .await
    }

    pub async fn find(
        pool: &PgPool,
        platform: &str,
        did: &str,
    ) -> Result<Option<(String, String, Option<String>)>, sqlx::Error> {
        let row: Option<Self> = sqlx::query_as(
            "SELECT * FROM chat_profiles WHERE platform = $1 AND did = $2",
        )
        .bind(platform)
        .bind(did)
        .fetch_optional(pool)
        .await?;

        Ok(row.map(|r| {
            let name = r
                .display_name
                .filter(|s| !s.is_empty())
                .or_else(|| r.handle.clone().filter(|s| !s.is_empty()))
                .unwrap_or_else(|| r.did.clone());
            (name, r.handle.unwrap_or_default(), r.avatar)
        }))
    }
}
