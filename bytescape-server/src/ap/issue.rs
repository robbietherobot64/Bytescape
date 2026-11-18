//! AP actor issuance — mint (or reclaim) a dual-protocol actor for an account.
//!
//! Issuance is idempotent and reclaim-safe:
//! - Generates an RSA keypair (if the account has none) and stores it encrypted.
//! - Sets the canonical actor id `https://{domain}/@{username}` (the reclaim id),
//!   inbox, and outbox.
//! - Marks the account `ap_enabled`, so the actor document + WebFinger serve it.
//!
//! Reclaim: for an account whose handle already existed as a fediverse ghost
//! actor (e.g. `@lovingisliving@bytescape.social`), issuing at the same canonical
//! id + serving the alias redirects reconciles the remote cache to this actor.

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json};
use uuid::Uuid;

use crate::ap::keys;
use crate::db::users::UserRow;
use crate::AppState;

/// Result of a successful issuance.
#[derive(Debug)]
pub struct IssuedActor {
    pub actor_id: String,
    pub reclaimed: bool,
}

/// Mint or reclaim the AP actor for a user. Idempotent: re-running for an
/// already-issued account keeps its existing keys and id (returns
/// `reclaimed = false` only on first issuance of a handle that had no actor).
pub async fn issue_actor_for_user(
    state: &AppState,
    user_id: uuid::Uuid,
) -> Result<IssuedActor, String> {
    let user = UserRow::find_by_id(&state.db, user_id)
        .await
        .map_err(|e| format!("db: {e}"))?
        .ok_or_else(|| "user not found".to_string())?;

    // "Issue actors only for accounts without an existing AP identity"
    // (actors/spec.md, realignment 2026-11-14): a Mastodon signup brings its
    // home-instance actor — minting one here would recreate the second
    // dereferenceable person this change exists to kill. Single chokepoint:
    // every issuance path (email/OAuth signup, oauth_complete, the reaction
    // repair in provision_ap_actor, ghost reclaim) funnels through here.
    if crate::db::accounts::LinkedAccountRow::find_by_user(&state.db, user_id)
        .await
        .map_err(|e| format!("db: {e}"))?
        .iter()
        .any(|la| la.provider == "mastodon")
    {
        return Err("account acts as its home-instance actor (mastodon signup) — no local AP actor is minted".into());
    }

    let domain = &state.config.domain;
    let actor_id = crate::ap::actor::canonical_actor_url(domain, &user.username);
    let inbox = format!("{}/inbox", actor_id);
    let outbox = format!("{}/outbox", actor_id);

    // Reuse an existing keypair; generate only if absent (idempotent reclaim
    // must not rotate keys — a key change breaks delivery continuity).
    let (private_pem, public_pem) = match (user.ap_private_key_pem.clone(), user.ap_public_key_pem.clone()) {
        (Some(enc_priv), Some(pub_pem)) => {
            let priv_pem = keys::private_key_pem(&enc_priv, &state.config.jwt_secret)?;
            (priv_pem, pub_pem)
        }
        _ => {
            let kp = keys::generate_keypair()?;
            let enc_priv = keys::encrypt_private_key(&kp.private_pem, &state.config.jwt_secret)?;
            (enc_priv, kp.public_pem)
        }
    };

    // Persist: if the account already carried an actor id (any form), this is a
    // reclaim/refresh; if not, it's a first issuance.
    let reclaimed = user.ap_actor_id.is_some() || user.ap_enabled;

    UserRow::set_ap_actor(
        &state.db,
        user_id,
        &actor_id,
        &inbox,
        &outbox,
        &private_pem,
        &public_pem,
    )
    .await
    .map_err(|e| format!("db: {e}"))?;

    Ok(IssuedActor { actor_id, reclaimed })
}

/// "This user's AP identity" per provider (design.md Decision 3, realignment
/// 2026-11-14): a Mastodon signup's identity IS its home-instance actor (the
/// canonical actor URL recorded at link time); Bluesky/email signups' identity
/// is their minted actor, once issued (`None` before issuance — never invent
/// an identity URL that serves nothing).
pub async fn ap_identity_for_user(
    db: &sqlx::PgPool,
    domain: &str,
    user: &UserRow,
) -> Option<String> {
    if let Ok(links) = crate::db::accounts::LinkedAccountRow::find_by_user(db, user.id).await {
        if let Some(home) = links.iter().find(|la| la.provider == "mastodon") {
            if let Some(url) = home
                .metadata
                .get("actor_url")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                return Some(url.to_string());
            }
            // No recorded URL (older link): derive the canonical home actor
            // URL from the `user@instance` identity.
            let instance = home.metadata.get("instance").and_then(|v| v.as_str())?;
            let handle = home.external_id.split('@').next().filter(|s| !s.is_empty())?;
            return Some(format!("https://{}/@{}", instance, handle));
        }
    }
    user.ap_actor_id.clone().or_else(|| {
        user.ap_enabled
            .then(|| crate::ap::actor::canonical_actor_url(domain, &user.username))
    })
}

/// Ensure the reclaimed ghost-actor accounts (e.g. `lovingisliving`) are issued
/// at their canonical id. Called on startup when `AP_ACTORS_ENABLED` is true, so
/// a known handle's ghost is reconciled to a live, controlled actor.
///
/// `handles` is the list of legacy handles to reclaim. Returns the handles that
/// were issued (whether newly or refreshed).
pub async fn reclaim_ghost_handles(
    state: &AppState,
    handles: &[String],
) -> Result<Vec<String>, String> {
    let mut issued = Vec::new();
    for handle in handles {
        match UserRow::find_by_username(&state.db, handle).await {
            Ok(Some(user)) => match issue_actor_for_user(state, user.id).await {
                Ok(_) => issued.push(handle.clone()),
                // A Mastodon-signup handle deliberately has no local actor
                // (realignment) — skip it, don't fail the whole reclaim batch.
                Err(e) if e.contains("home-instance actor") => {
                    tracing::info!("[AP] reclaim skip for {handle}: {e}");
                }
                Err(e) => return Err(e),
            },
            Ok(None) => {
                tracing::warn!("[AP] reclaim: no local user for handle {handle}; skipping");
            }
            Err(e) => return Err(format!("db looking up {handle}: {e}")),
        }
    }
    Ok(issued)
}

/// POST/GET /api/ap/issue — self-serve issuance: the signed-in user mints (or
/// reclaims) their own AP actor. Returns the canonical actor id.
pub async fn issue_self(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, crate::error::AppError> {
    if !crate::ap::ap_actors_enabled() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let issued = issue_actor_for_user(&state, user_id)
        .await
        .map_err(crate::error::AppError::Internal)?;

    Ok(Json(serde_json::json!({
        "actor_id": issued.actor_id,
        "reclaimed": issued.reclaimed,
    }))
    .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ap::testutil::{connect_pool, enable_ap_flags, test_state, unique};

    /// Actors/spec.md "Mastodon sign-up requests issuance" + tasks.md 1.3/1.4:
    /// a Mastodon-signed-up user never gets a local AP actor (issuance refuses
    /// at the single chokepoint), its AP identity resolves to the HOME actor,
    /// while email signups keep minting (unchanged spec for them).
    #[tokio::test]
    async fn mastodon_signup_gets_no_local_actor() {
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let Some(state) = test_state(&pool) else { return };

        let username = format!("iss_{}", unique());
        let user = crate::db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");
        crate::db::accounts::LinkedAccountRow::create(
            &pool,
            user.id,
            "mastodon",
            &format!("{username}@piaille.fr"),
            &username,
            "tok",
            None,
            None,
            None,
            Some("read write:favourites"),
            &serde_json::json!({
                "instance": "piaille.fr",
                "actor_url": format!("https://piaille.fr/@{username}"),
            }),
        )
        .await
        .expect("link mastodon");

        let err = issue_actor_for_user(&state, user.id)
            .await
            .expect_err("mint must refuse for a mastodon signup");
        assert!(err.contains("home-instance actor"), "refusal names the reason: {err}");

        let fresh = crate::db::users::UserRow::find_by_id(&pool, user.id)
            .await
            .expect("reload")
            .expect("row");
        assert!(!fresh.ap_enabled, "no AP actor was minted");
        assert!(fresh.ap_actor_id.is_none(), "no actor id was minted");
        assert!(fresh.ap_private_key_pem.is_none(), "no RSA key was minted");

        // Task 1.4: "this user's AP identity" is the home-instance actor.
        let identity = ap_identity_for_user(&pool, "bytescape.test", &fresh).await;
        assert_eq!(
            identity.as_deref(),
            Some(format!("https://piaille.fr/@{username}").as_str()),
        );

        // Email signup (no pre-existing AP identity) keeps its minted actor.
        let email_name = format!("iss_{}", unique());
        let email_user = crate::db::users::UserRow::create_active(&pool, &email_name, "", "x", "x")
            .await
            .expect("create email user");
        issue_actor_for_user(&state, email_user.id)
            .await
            .expect("mint for email signup");
        let minted = crate::db::users::UserRow::find_by_id(&pool, email_user.id)
            .await
            .expect("reload")
            .expect("row");
        assert!(minted.ap_enabled && minted.ap_actor_id.is_some());
        let minted_identity = ap_identity_for_user(&pool, &state.config.domain, &minted).await;
        assert_eq!(minted_identity, minted.ap_actor_id, "bluesky/email identity is the minted actor");

        // Task 1.4, third provider: a Bluesky signup's identity is ALSO the
        // minted actor (their real bsky presence lives on the AT side; the
        // minted actor is their fediverse identity).
        let sky_name = format!("iss_{}", unique());
        let sky_user = crate::db::users::UserRow::create_active(&pool, &sky_name, "", "x", "x")
            .await
            .expect("create bluesky user");
        crate::db::accounts::LinkedAccountRow::create(
            &pool,
            sky_user.id,
            "bluesky",
            "did:plc:testsky",
            &sky_name,
            "tok",
            None,
            None,
            None,
            None,
            &serde_json::json!({ "did": "did:plc:testsky" }),
        )
        .await
        .expect("link bluesky");
        issue_actor_for_user(&state, sky_user.id)
            .await
            .expect("mint for bluesky signup");
        let sky = crate::db::users::UserRow::find_by_id(&pool, sky_user.id)
            .await
            .expect("reload")
            .expect("row");
        let sky_identity = ap_identity_for_user(&pool, &state.config.domain, &sky).await;
        assert_eq!(sky_identity, sky.ap_actor_id, "bluesky identity is the minted actor");

        for uid in [user.id, email_user.id, sky_user.id] {
            let _ = sqlx::query("DELETE FROM users WHERE id = $1").bind(uid).execute(&pool).await;
        }
    }
}

/// Extract the authenticated user's id from a Bearer JWT (same convention as
/// `api::profile::extract_user_id`).
fn extract_user_id(headers: &HeaderMap, secret: &str) -> Result<Uuid, crate::error::AppError> {
    let auth = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| crate::error::AppError::Auth("Missing bearer token".into()))?;
    let claims = crate::auth::validate_token(auth, secret)?;
    Uuid::parse_str(&claims.sub)
        .map_err(|_| crate::error::AppError::Auth("Invalid user id in token".into()))
}
