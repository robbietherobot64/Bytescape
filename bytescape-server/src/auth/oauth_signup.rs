use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppError;

/// Payload stored in a short-lived temp token for OAuth-only signup.
/// This is NOT a login token — it holds provisional OAuth provider info
/// until the user picks a username to complete account creation.
#[derive(Debug, Serialize, Deserialize)]
pub struct TempTokenClaims {
    /// Random token ID for revocation
    pub jti: String,
    /// OAuth provider name ("bluesky")
    pub provider: String,
    /// External user ID from the provider
    pub external_id: String,
    /// Access token from the provider
    pub access_token: String,
    /// Refresh token from the provider (optional)
    pub refresh_token: Option<String>,
    /// Hex-encoded DPoP private key seed for authenticated provider calls.
    /// Must be stored in linked_accounts.dpop_private_key — NOT the access
    /// token, which is a JWT and will fail hex-decode at call sites.
    pub dpop_private_key: Option<String>,
    /// The OAuth scope the provider granted (Mastodon: recorded so read-only
    /// grants can be told apart from favourites-write grants).
    #[serde(default)]
    pub scope: Option<String>,
    /// Metadata JSON
    pub metadata: serde_json::Value,
    pub iat: i64,
    pub exp: i64,
}

/// Create a short-lived JWT (5 min) holding OAuth provider info for signup completion.
#[allow(clippy::too_many_arguments)]
pub fn create_temp_token(
    secret: &str,
    provider: &str,
    external_id: &str,
    access_token: &str,
    refresh_token: Option<String>,
    dpop_private_key: Option<String>,
    scope: Option<String>,
    metadata: serde_json::Value,
) -> Result<String, AppError> {
    let now = Utc::now().timestamp();
    let claims = TempTokenClaims {
        jti: Uuid::new_v4().to_string(),
        provider: provider.to_string(),
        external_id: external_id.to_string(),
        access_token: access_token.to_string(),
        refresh_token,
        dpop_private_key,
        scope,
        metadata,
        iat: now,
        exp: now + 300, // 5 minutes
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("Temp token creation failed: {}", e)))
}

/// Validate and decode a temp token.
pub fn validate_temp_token(token: &str, secret: &str) -> Result<TempTokenClaims, AppError> {
    decode::<TempTokenClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map(|d| d.claims)
    .map_err(|e| AppError::BadRequest(format!("Invalid or expired temp token: {}", e)))
}

/// Consume a temp token and create the user account with linked provider.
/// Returns the created user and linked account.
pub async fn consume_temp_token(
    pool: &PgPool,
    token: &str,
    username: &str,
    public_key: &str,
    secret: &str,
    require_invite: bool,
    invite_code: &str,
    domain: &str,
    plc_base: &str,
) -> Result<(crate::db::users::UserRow, crate::db::accounts::LinkedAccountRow), AppError> {
    let claims = validate_temp_token(token, secret)?;

    // Validate username
    if username.len() < 2 || username.len() > 32 {
        return Err(AppError::BadRequest("Username must be 2-32 characters".into()));
    }
    if !username.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Err(AppError::BadRequest("Username may only contain letters, numbers, hyphens, underscores".into()));
    }

    // Check username availability
    if crate::db::users::UserRow::find_by_username(pool, username).await?.is_some() {
        return Err(AppError::Conflict("Username already taken".into()));
    }

    // Create user with a random password (not usable for login — OAuth-only account)
    let random_password = Uuid::new_v4().to_string();
    let password_hash = crate::auth::hash_password(&random_password)?;

    // Invite gate at the creation chokepoint — every OAuth signup path
    // funnels through here (callback + oauth-complete).
    if require_invite {
        if invite_code.is_empty() {
            return Err(AppError::BadRequest("Invite code required".into()));
        }
        if !crate::db::invite_codes::InviteCodeRow::is_valid(pool, invite_code).await? {
            return Err(AppError::BadRequest("Invalid or expired invite code".into()));
        }
    }
    let user = crate::db::users::UserRow::create_active(pool, username, "", &password_hash, public_key).await?;
    if require_invite && !invite_code.is_empty() {
        let _ = crate::db::invite_codes::InviteCodeRow::validate_and_consume(pool, invite_code, user.id).await;
    }

    // Publish the chosen username as a live AT Protocol handle (did:plc) so it
    // resolves on the atmosphere (.well-known/atproto-did) — same as email
    // signup (api/auth.rs). Without this the handle is unresolvable and the
    // user can't be found or attributed on Bluesky/the wider network. Non-fatal:
    // a failed issuance never blocks OAuth signup.
    match crate::pds::account::PdsAccount::create(pool, username, domain, Some(user.id), plc_base).await {
        Ok(account) => {
            tracing::info!("[OAUTH] Created PDS account: did={} handle={}", account.did, account.handle);
        }
        Err(e) => {
            tracing::warn!("[OAUTH] PDS account creation failed (user still created): {}", e);
        }
    }

    // Link the OAuth provider
    let display_name = claims.metadata.get("display_name")
        .and_then(|v| v.as_str())
        .unwrap_or(username);

    let linked = crate::db::accounts::LinkedAccountRow::create(
        pool,
        user.id,
        &claims.provider,
        &claims.external_id,
        display_name,
        &claims.access_token,
        claims.refresh_token.as_deref(),
        None,
        claims.dpop_private_key.as_deref(),
        claims.scope.as_deref(),
        &claims.metadata,
    )
    .await?;

    Ok((user, linked))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Task 1.3 acceptance (chokepoint half): `consume_temp_token` for a
    /// Mastodon signup stores the home-actor reference + granted scope on the
    /// linked row and mints NO local AP actor (no `ap_actor_id`, no RSA key).
    #[tokio::test]
    async fn mastodon_signup_records_home_actor_and_mints_nothing() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else { return };
        let suffix = crate::ap::testutil::unique();
        let token = create_temp_token(
            "test-secret",
            "mastodon",
            &format!("ctt_{suffix}@piaille.fr"),
            "tok-abc",
            None,
            None,
            Some("read write:favourites".to_string()),
            serde_json::json!({
                "instance": "piaille.fr",
                "display_name": "CTT",
                "actor_url": format!("https://piaille.fr/@ctt_{suffix}"),
            }),
        )
        .expect("temp token");

        // Mint points at a closed port: this test's subject is the link row +
        // the no-AP-actor gate, and mint failure is non-fatal by design — a
        // real directory call here would write a public did:plc per test run.
        let (user, linked) = consume_temp_token(
            &pool,
            &token,
            &format!("ctt_{suffix}"),
            "pk",
            "test-secret",
            false,
            "",
            "bytescape.test",
            "http://127.0.0.1:9",
        )
        .await
        .expect("consume");

        assert_eq!(linked.provider, "mastodon");
        assert_eq!(linked.scope.as_deref(), Some("read write:favourites"), "granted scope recorded");
        assert_eq!(
            linked.metadata.get("actor_url").and_then(|v| v.as_str()),
            Some(format!("https://piaille.fr/@ctt_{suffix}").as_str()),
            "home-actor reference recorded at link time"
        );
        assert!(user.ap_actor_id.is_none(), "no minted AP actor for a mastodon signup");
        assert!(user.ap_private_key_pem.is_none(), "no minted RSA key for a mastodon signup");

        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await;
    }
}
