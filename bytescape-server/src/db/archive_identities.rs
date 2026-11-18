use sqlx::FromRow;
use uuid::Uuid;

#[derive(FromRow)]
pub struct ArchiveIdentityRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub platform: String,
    pub handle: Option<String>,
    pub display_name: Option<String>,
    pub profile_url: Option<String>,
    pub import_session_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct NewArchiveIdentity {
    pub user_id: Uuid,
    pub platform: String,
    pub handle: Option<String>,
    pub display_name: Option<String>,
    pub profile_url: Option<String>,
    pub import_session_id: Option<Uuid>,
}

impl ArchiveIdentityRow {
    pub async fn create(
        db: &sqlx::PgPool,
        identity: &NewArchiveIdentity,
    ) -> Result<ArchiveIdentityRow, sqlx::Error> {
        sqlx::query_as::<_, ArchiveIdentityRow>(
            r#"INSERT INTO archive_identities (user_id, platform, handle, display_name, profile_url, import_session_id)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT DO NOTHING
            RETURNING *"#,
        )
        .bind(identity.user_id)
        .bind(&identity.platform)
        .bind(&identity.handle)
        .bind(&identity.display_name)
        .bind(&identity.profile_url)
        .bind(identity.import_session_id)
        .fetch_one(db)
        .await
    }

    pub async fn find_by_handle(
        db: &sqlx::PgPool,
        handle: &str,
    ) -> Vec<ArchiveIdentityRow> {
        sqlx::query_as::<_, ArchiveIdentityRow>(
            "SELECT * FROM archive_identities WHERE LOWER(handle) = LOWER($1)",
        )
        .bind(handle)
        .fetch_all(db)
        .await
        .unwrap_or_default()
    }

    pub async fn find_by_name(
        db: &sqlx::PgPool,
        name: &str,
    ) -> Vec<ArchiveIdentityRow> {
        sqlx::query_as::<_, ArchiveIdentityRow>(
            "SELECT * FROM archive_identities WHERE LOWER(display_name) = LOWER($1)",
        )
        .bind(name)
        .fetch_all(db)
        .await
        .unwrap_or_default()
    }

    pub async fn find_by_user(
        db: &sqlx::PgPool,
        user_id: Uuid,
    ) -> Vec<ArchiveIdentityRow> {
        sqlx::query_as::<_, ArchiveIdentityRow>(
            "SELECT * FROM archive_identities WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_all(db)
        .await
        .unwrap_or_default()
    }
}