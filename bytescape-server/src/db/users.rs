//! User database operations.

use sqlx::PgPool;
use uuid::Uuid;

/// The three-state NSFW content preference (`users.nsfw_pref`).
///
/// - `Exclude` — NSFW content hidden entirely (SQL pre-filter + in-memory
///   policy drop). Default for new accounts.
/// - `Blur` — NSFW content is kept and marked (`nsfw = true`) so the frontend
///   can render its blur + click-to-reveal layer.
/// - `Show` — NSFW content is treated as clean (unmarked/unflagged).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NsfwPref {
    Exclude,
    Blur,
    Show,
}

impl NsfwPref {
    /// The canonical DB/API string form.
    pub fn as_str(&self) -> &'static str {
        match self {
            NsfwPref::Exclude => "exclude",
            NsfwPref::Blur => "blur",
            NsfwPref::Show => "show",
        }
    }

    /// Parse from the stored/API string. Unknown values fall back to `Exclude`
    /// (the safe default) when `strict` is false; `from_api` uses strict parsing
    /// so bad client input is rejected rather than silently coerced.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "exclude" => Some(NsfwPref::Exclude),
            "blur" => Some(NsfwPref::Blur),
            "show" => Some(NsfwPref::Show),
            _ => None,
        }
    }

    /// Lenient read from a stored column value (unknown -> `Exclude`).
    pub fn from_db(s: &str) -> Self {
        Self::parse(s).unwrap_or(NsfwPref::Exclude)
    }

    /// Snapshot-seed axis: `exclude` -> `n=0`, `blur`/`show` -> `n=1` (both
    /// keep marked items in the snapshot, so they share a key).
    pub fn snapshot_axis(&self) -> u8 {
        match self {
            NsfwPref::Exclude => 0,
            NsfwPref::Blur | NsfwPref::Show => 1,
        }
    }
}

/// Row shape for the `users` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    pub password_hash: String,
    pub public_key: String,
    pub encrypted_privkey: Option<String>,
    pub recovery_escrow: Option<String>,
    pub bio: String,
    pub avatar_url: Option<String>,
    pub header_url: Option<String>,
    pub theme: String,
    pub custom_css: String,
    pub location: Option<String>,
    pub website: Option<String>,
    pub pronouns: Option<String>,
    pub relationship_status: Option<String>,
    pub birthday: Option<chrono::NaiveDate>,
    pub occupation: Option<String>,
    pub interests: Vec<String>,
    pub widget_layout: serde_json::Value,
    pub widget_themes: serde_json::Value,
    pub custom_profile_fields: serde_json::Value,
    pub nsfw_pref: String,
    pub feed_prefs: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub status: String,
    pub visitor_count: i64,
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub onboarded: bool,
    // ActivityPub actor identity (Phase A). Nullable — set when an AP actor is
    // minted/reclaimed for the account (behind AP_ACTORS_ENABLED).
    pub ap_actor_id: Option<String>,
    pub ap_inbox: Option<String>,
    pub ap_outbox: Option<String>,
    pub ap_private_key_pem: Option<String>,
    pub ap_public_key_pem: Option<String>,
    pub ap_enabled: bool,
    pub manually_approves_followers: bool,
    /// Follow-graph visibility for the AP collections (migration 106):
    /// `public` (default) serves paginated followers/following; `private`
    /// serves them as Mastodon's hidden state (no `first`, no counts).
    pub graph_visibility: String,
}

impl UserRow {
    pub async fn set_onboarded(
        db: &sqlx::PgPool,
        user_id: uuid::Uuid,
        onboarded: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE users SET onboarded = $1 WHERE id = $2")
            .bind(onboarded)
            .bind(user_id)
            .execute(db)
            .await
            .map(|_| ())
    }

    pub async fn set_manually_approves_followers(
        db: &sqlx::PgPool,
        user_id: uuid::Uuid,
        locked: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE users SET manually_approves_followers = $1 WHERE id = $2")
            .bind(locked)
            .bind(user_id)
            .execute(db)
            .await
            .map(|_| ())
    }

    /// Set the follow-graph visibility (`public` | `private`). Invalid values
    /// are rejected here and again by the migration-106 CHECK.
    pub async fn set_graph_visibility(
        db: &sqlx::PgPool,
        user_id: uuid::Uuid,
        visibility: &str,
    ) -> Result<(), sqlx::Error> {
        if visibility != "public" && visibility != "private" {
            return Err(sqlx::Error::Protocol(
                format!("invalid graph_visibility: {visibility}").into(),
            ));
        }
        sqlx::query("UPDATE users SET graph_visibility = $1 WHERE id = $2")
            .bind(visibility)
            .bind(user_id)
            .execute(db)
            .await
            .map(|_| ())
    }

    pub async fn set_interests(
        db: &sqlx::PgPool,
        user_id: uuid::Uuid,
        interests: &[String],
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE users SET interests = $1 WHERE id = $2")
            .bind(interests)
            .bind(user_id)
            .execute(db)
            .await
            .map(|_| ())
    }

    /// Set the app theme id (validated by the caller against the roster).
    pub async fn set_theme(
        db: &sqlx::PgPool,
        user_id: uuid::Uuid,
        theme: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE users SET theme = $1, updated_at = NOW() WHERE id = $2")
            .bind(theme)
            .bind(user_id)
            .execute(db)
            .await
            .map(|_| ())
    }

    /// Set the three-state NSFW preference.
    pub async fn set_nsfw_pref(
        db: &sqlx::PgPool,
        user_id: uuid::Uuid,
        pref: NsfwPref,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE users SET nsfw_pref = $1, updated_at = NOW() WHERE id = $2")
            .bind(pref.as_str())
            .bind(user_id)
            .execute(db)
            .await
            .map(|_| ())
    }

    /// Merge the feed-preferences patch (`{"sort": ..., "source_mix": ...}`)
    /// into `users.feed_prefs`, leaving unspecified keys untouched. Keys set to
    /// JSON `null` are removed.
    pub async fn merge_feed_prefs(
        db: &sqlx::PgPool,
        user_id: uuid::Uuid,
        patch: &serde_json::Value,
    ) -> Result<serde_json::Value, sqlx::Error> {
        let merged: serde_json::Value = sqlx::query_scalar(
            "UPDATE users SET feed_prefs = COALESCE(feed_prefs, '{}'::jsonb) || $2, \
             updated_at = NOW() WHERE id = $1 RETURNING COALESCE(feed_prefs, '{}'::jsonb)",
        )
        .bind(user_id)
        .bind(patch)
        .fetch_one(db)
        .await?;
        Ok(merged)
    }

    /// Create a new user (status = 'pending' if email verification required).
    pub async fn create(
        pool: &PgPool,
        username: &str,
        email: &str,
        password_hash: &str,
        public_key: &str,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO users (username, display_name, email, password_hash, public_key, status)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(username)
        .bind(username) // display_name defaults to username
        .bind(email)
        .bind(password_hash)
        .bind(public_key)
        .bind("pending")
        .fetch_one(pool)
        .await
    }

    /// Create a new user in active state (for OAuth signup or when email verification is skipped).
    pub async fn create_active(
        pool: &PgPool,
        username: &str,
        email: &str,
        password_hash: &str,
        public_key: &str,
    ) -> Result<Self, sqlx::Error> {
        // OAuth-only accounts have no email; store NULL (not '') so the
        // UNIQUE(email) constraint doesn't reject the 2nd+ user.
        let email_opt: Option<&str> = if email.is_empty() { None } else { Some(email) };
        sqlx::query_as(
            r#"
            INSERT INTO users (username, display_name, email, password_hash, public_key, status)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(username)
        .bind(username)
        .bind(email_opt)
        .bind(password_hash)
        .bind(public_key)
        .bind("active")
        .fetch_one(pool)
        .await
    }

    /// Activate a pending user (email verified).
    pub async fn activate_user(pool: &PgPool, user_id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as(
            r#"
            UPDATE users SET status = 'active', updated_at = NOW()
            WHERE id = $1 AND status = 'pending'
            RETURNING *
            "#,
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await
    }

    /// Find a user by ID.
    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await
    }

    /// Find a user by email.
    pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM users WHERE email = $1")
            .bind(email)
            .fetch_optional(pool)
            .await
    }

    /// Find a user by username.
    pub async fn find_by_username(pool: &PgPool, username: &str) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM users WHERE username = $1")
            .bind(username)
            .fetch_optional(pool)
            .await
    }

    /// Update profile fields.
    pub async fn update_profile(
        pool: &PgPool,
        id: Uuid,
        display_name: &str,
        bio: &str,
        theme: &str,
        custom_css: &str,
        avatar_url: Option<&str>,
        header_url: Option<&str>,
        location: Option<&str>,
        website: Option<&str>,
        pronouns: Option<&str>,
        relationship_status: Option<&str>,
        birthday: Option<chrono::NaiveDate>,
        occupation: Option<&str>,
        interests: Option<&[String]>,
        widget_layout: Option<&serde_json::Value>,
        widget_themes: Option<&serde_json::Value>,
        custom_profile_fields: Option<&serde_json::Value>,
        nsfw_pref: Option<&str>,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            UPDATE users
            SET display_name = COALESCE($1, display_name),
                bio = COALESCE($2, bio),
                theme = COALESCE($3, theme),
                custom_css = COALESCE($4, custom_css),
                avatar_url = COALESCE($5, avatar_url),
                header_url = COALESCE($6, header_url),
                location = COALESCE($7, location),
                website = COALESCE($8, website),
                pronouns = COALESCE($9, pronouns),
                relationship_status = COALESCE($10, relationship_status),
                birthday = COALESCE($11, birthday),
                occupation = COALESCE($12, occupation),
                interests = COALESCE($13, interests),
                -- Saving interests is the onboarding action: mark the account
                -- onboarded so the frontend stops routing it through onboarding.
                onboarded = CASE WHEN $13 IS NOT NULL THEN true ELSE onboarded END,
                widget_layout = COALESCE($14, widget_layout),
                widget_themes = COALESCE($15, widget_themes),
                custom_profile_fields = COALESCE($16, custom_profile_fields),
                nsfw_pref = COALESCE($17, nsfw_pref),
                updated_at = NOW()
            WHERE id = $18
            RETURNING *
            "#,
        )
        .bind(display_name)
        .bind(bio)
        .bind(theme)
        .bind(custom_css)
        .bind(avatar_url)
        .bind(header_url)
        .bind(location)
        .bind(website)
        .bind(pronouns)
        .bind(relationship_status)
        .bind(birthday)
        .bind(occupation)
        .bind(interests)
        .bind(widget_layout)
        .bind(widget_themes)
        .bind(custom_profile_fields)
        .bind(nsfw_pref)
        .bind(id)
        .fetch_one(pool)
        .await
    }

    /// Store the encrypted private key (from client-side key generation).
    pub async fn store_encrypted_privkey(
        pool: &PgPool,
        id: Uuid,
        encrypted_privkey: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE users SET encrypted_privkey = $1 WHERE id = $2")
            .bind(encrypted_privkey)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Store the recovery escrow bundle.
    pub async fn store_recovery_escrow(
        pool: &PgPool,
        id: Uuid,
        escrow: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE users SET recovery_escrow = $1 WHERE id = $2")
            .bind(escrow)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Persist the minted/reclaimed ActivityPub actor identity for a user.
    /// Called once per account when issuance runs (reclaim or fresh), behind
    /// AP_ACTORS_ENABLED. `ap_private_key_pem` must already be encrypted at rest.
    #[allow(clippy::too_many_arguments)]
    pub async fn set_ap_actor(
        pool: &PgPool,
        id: Uuid,
        ap_actor_id: &str,
        ap_inbox: &str,
        ap_outbox: &str,
        ap_private_key_pem: &str,
        ap_public_key_pem: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE users SET ap_actor_id=$1, ap_inbox=$2, ap_outbox=$3, \
             ap_private_key_pem=$4, ap_public_key_pem=$5, ap_enabled=true, updated_at=now() \
             WHERE id=$6",
        )
        .bind(ap_actor_id)
        .bind(ap_inbox)
        .bind(ap_outbox)
        .bind(ap_private_key_pem)
        .bind(ap_public_key_pem)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Find a user by their canonical AP actor id (e.g. `https://…/@alice`).
    /// Used by WebFinger, actor-doc fetch, and alias reconciliation.
    pub async fn find_by_ap_actor_id(
        pool: &PgPool,
        ap_actor_id: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM users WHERE ap_actor_id = $1 AND ap_enabled = true")
            .bind(ap_actor_id)
            .fetch_optional(pool)
            .await
    }

    /// Find a user by username for AP actor serving. Scoped to `ap_enabled` so a
    /// disabled/unissued account never serves an actor document.
    pub async fn find_ap_enabled_by_username(
        pool: &PgPool,
        username: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM users WHERE username = $1 AND ap_enabled = true")
            .bind(username)
            .fetch_optional(pool)
            .await
    }

    /// Set the avatar URL.
    pub async fn set_avatar(
        pool: &PgPool,
        id: Uuid,
        url: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE users SET avatar_url = $1 WHERE id = $2")
            .bind(url)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Set the header URL.
    pub async fn set_header_url(
        pool: &PgPool,
        id: Uuid,
        url: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE users SET header_url = $1 WHERE id = $2")
            .bind(url)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Soft-delete a user (sets deleted_at and marks status as 'deleted').
    pub async fn soft_delete(pool: &PgPool, user_id: Uuid, delete_at: chrono::DateTime<chrono::Utc>) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as(
            r#"
            UPDATE users SET deleted_at = $2, status = 'deleted', updated_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(delete_at)
        .fetch_optional(pool)
        .await
    }

    /// Restore a soft-deleted user (clears deleted_at and sets status to 'active').
    pub async fn restore(pool: &PgPool, user_id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as(
            r#"
            UPDATE users SET deleted_at = NULL, status = 'active', updated_at = NOW()
            WHERE id = $1 AND deleted_at IS NOT NULL
            RETURNING *
            "#,
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await
    }

    /// Count total users.
    pub async fn count(pool: &PgPool) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(pool)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Migration 102: accounts created without an explicit choice are
    /// approve-only out of the box — the column default is true and the
    /// create paths never write the column. An explicit user choice
    /// (Settings → follow approvals) sticks afterwards.
    #[tokio::test]
    async fn new_users_default_to_approve_only() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else { return };
        let username = format!("aptest_{}", crate::ap::testutil::unique());
        let user = UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");
        assert!(
            user.manually_approves_followers,
            "migration 102: new accounts hold follows for approval by default"
        );

        UserRow::set_manually_approves_followers(&pool, user.id, false)
            .await
            .expect("explicit choice");
        let stored: bool =
            sqlx::query_scalar("SELECT manually_approves_followers FROM users WHERE id = $1")
                .bind(user.id)
                .fetch_one(&pool)
                .await
                .expect("read stored choice");
        assert!(!stored, "an explicit choice to auto-approve wins over the default");

        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await;
    }

    /// Migration 106: the follow graph is public by default (Bluesky norms),
    /// an explicit `private` choice sticks, and invalid values are rejected.
    #[tokio::test]
    async fn graph_visibility_defaults_to_public_and_flips() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else { return };
        let username = format!("aptest_{}", crate::ap::testutil::unique());
        let user = UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");
        assert_eq!(
            user.graph_visibility, "public",
            "migration 106: the follow graph is public by default"
        );

        UserRow::set_graph_visibility(&pool, user.id, "private")
            .await
            .expect("explicit private");
        let stored: String =
            sqlx::query_scalar("SELECT graph_visibility FROM users WHERE id = $1")
                .bind(user.id)
                .fetch_one(&pool)
                .await
                .expect("read stored visibility");
        assert_eq!(stored, "private", "an explicit private choice sticks");

        assert!(
            UserRow::set_graph_visibility(&pool, user.id, "sometimes").await.is_err(),
            "values outside public|private are rejected"
        );

        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await;
    }
}
