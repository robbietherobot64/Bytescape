//! Live Mastodon follow import — read the account's home-instance follow graph
//! through the REST API at sign-up / link time.
//!
//! Why this exists: a Mastodon-signed-up Bytescape account acts AS its
//! home-instance actor (no local AP actor is minted), so its follow graph lives
//! on the home instance and never reaches us by federation. Without this import
//! a fresh sign-up showed zero follows until the user manually uploaded a
//! Mastodon archive (`api/import_mastodon.rs`) — the reported "0 follows
//! imported at signup" bug.
//!
//! What one run writes (delete-then-reimport, idempotent):
//! * `follows` — `follow_type = 'mastodon_import'`, `status = 'accepted'`,
//!   `followed_actor` = the CANONICAL AP actor id from WebFinger. The canonical
//!   form matters: the following feed tier matches `appview_posts.author_did`
//!   (a post's `attributedTo`) by exact equality
//!   (`api/feed/sources.rs::get_followed_ap_actors`), so storing the
//!   home-instance profile URL instead silently matches nothing.
//! * `imported_contacts` — `source = 'mastodon'`, `direction = 'following'`,
//!   `import_session_id = NULL`, so `ap::follow_feed::populate_interests` and
//!   the archive / RSS-candidate machinery keep reading one table.
//! * `social_graph_matches` — when a resolved actor belongs to a Bytescape
//!   account (`users.ap_actor_id`), so the existing "friends already here"
//!   surface lights up.
//!
//! Followers are deliberately NOT imported: `followers` rows need an inbox and
//! only mean something for an account with a minted AP actor (audience
//! expansion), which a Mastodon sign-up has by design. That import belongs to
//! identity-graph-unification Phase 1/3, once inbound followers are readable at
//! all; `oauth::mastodon::follower_accounts` is built and waiting for it.
//!
//! Everything here is best-effort: one unresolvable handle is skipped, never
//! fatal, and the run always leaves whatever it did write in place.

use sqlx::PgPool;
use uuid::Uuid;

use crate::oauth::mastodon::MastodonFollowAccount;

/// `follows.follow_type` for rows that come from a home-instance graph read —
/// distinct from the archive import (`imported_contacts` only) and from a
/// native follow made in Bytescape.
const FOLLOW_TYPE: &str = "mastodon_import";

/// Bound on WebFinger resolutions per run. Each resolution is a guarded HTTPS
/// round-trip (15s cap, `ap::remote::resolve_handle`); the cap keeps one huge
/// follow graph from turning a background task into a long tail of fetches.
const MAX_RESOLVES_PER_RUN: usize = 40;

/// Outcome of one import run (also what the caller persists as metadata).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ImportOutcome {
    /// Accounts the home instance listed (before the resolve cap).
    pub seen: usize,
    /// Accounts resolved to an actor id and written to `follows`.
    pub written: usize,
    /// Accounts skipped because WebFinger could not resolve them.
    pub skipped: usize,
    /// Written accounts that belong to a Bytescape user.
    pub matched_local: usize,
}

/// Read the home-instance `following` list and mirror it into `follows` +
/// `imported_contacts` + `social_graph_matches`, replacing any previous
/// `mastodon_import` rows for this user.
///
/// `access_token` is the stored (unsealed) home-instance OAuth token: the
/// `read` grant covers the whole walk, nothing is written to the home instance.
/// `jwt_secret` resolves the user's AP signing identity for signed WebFinger
/// lookups (absent actor → unsigned lookups, still allowed).
pub async fn import_follows(
    pool: &PgPool,
    user_id: Uuid,
    instance: &str,
    access_token: &str,
    jwt_secret: &str,
) -> Result<ImportOutcome, String> {
    if instance.trim().is_empty() || access_token.trim().is_empty() {
        return Err("follow import needs a home instance and a token".to_string());
    }
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let accounts = crate::oauth::mastodon::following_accounts(&http, instance, access_token)
        .await
        .map_err(|e| format!("following list: {e}"))?;

    if let Err(e) = sqlx::query("DELETE FROM follows WHERE follower_id = $1 AND follow_type = $2")
        .bind(user_id)
        .bind(FOLLOW_TYPE)
        .execute(pool)
        .await
    {
        return Err(format!("clearing previous imports: {e}"));
    }
    // Live-import contacts only: archive rows carry an `import_session_id` and
    // belong to a different flow.
    if let Err(e) = sqlx::query(
        "DELETE FROM imported_contacts WHERE user_id = $1 AND source = 'mastodon' \
         AND direction = 'following' AND import_session_id IS NULL",
    )
    .bind(user_id)
    .execute(pool)
    .await
    {
        return Err(format!("clearing previous contacts: {e}"));
    }

    let signer = crate::ap::keys::load_fetch_signer(pool, user_id, jwt_secret).await;
    let signer = signer.as_ref().map(|(k, p)| (k.as_str(), p.as_str()));

    let mut outcome = ImportOutcome {
        seen: accounts.len(),
        ..Default::default()
    };
    let mut contacts: Vec<crate::db::imported_contacts::NewContact> = Vec::new();

    for account in accounts.iter().take(MAX_RESOLVES_PER_RUN) {
        let actor = match crate::ap::remote::resolve_handle(pool, signer, &account.acct).await {
            Ok(actor) => actor,
            Err(e) => {
                tracing::debug!(acct = %account.acct, error = %e, "[FOLLOW-IMPORT] resolve failed, skipped");
                outcome.skipped += 1;
                continue;
            }
        };

        // Two ways a row can already exist: the same actor came from an earlier
        // run (deleted above, so free) or the user follows them natively. A
        // native follow is the stronger statement — never downgrade it to an
        // import.
        let inserted = sqlx::query(
            "INSERT INTO follows (follower_id, followed_actor, follow_type, status, label) \
             VALUES ($1, $2, $3, 'accepted', $4) \
             ON CONFLICT (follower_id, followed_actor) DO UPDATE \
                 SET follow_type = 'mastodon_import', \
                     status = 'accepted', \
                     label = COALESCE(EXCLUDED.label, follows.label) \
                 WHERE follows.follow_type <> 'native'",
        )
        .bind(user_id)
        .bind(&actor.actor_id)
        .bind(FOLLOW_TYPE)
        .bind(&account.display_name)
        .execute(pool)
        .await;
        match inserted {
            Ok(_) => outcome.written += 1,
            Err(e) => {
                tracing::warn!(actor = %actor.actor_id, error = %e, "[FOLLOW-IMPORT] follow insert failed");
                continue;
            }
        }

        contacts.push(crate::db::imported_contacts::NewContact {
            user_id,
            contact_name: account.display_name.clone(),
            contact_handle: Some(remote_handle(account)),
            contact_platform_url: account.url.clone(),
            source: "mastodon".to_string(),
            remote_handle: Some(remote_handle(account)),
            direction: Some("following".to_string()),
            import_session_id: None,
        });

        // A payload-free exact match: the resolved actor id IS the canonical
        // identity of a Bytescape account with a minted actor.
        match crate::db::users::UserRow::find_by_ap_actor_id(pool, &actor.actor_id).await {
            Ok(Some(target)) if target.id != user_id => {
                let _ = crate::db::social_graph_matches::SocialGraphMatchRow::create(
                    pool, user_id, target.id, "handle_match", "mastodon", "certain",
                )
                .await;
                let _ = crate::db::social_graph_matches::SocialGraphMatchRow::create(
                    pool, target.id, user_id, "handle_match", "mastodon", "certain",
                )
                .await;
                outcome.matched_local += 1;
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(
                actor = %actor.actor_id,
                error = %e,
                "[FOLLOW-IMPORT] local user lookup failed"
            ),
        }
    }

    if let Err(e) =
        crate::db::imported_contacts::ImportedContactRow::create_batch(pool, &contacts).await
    {
        // Contacts feed content ingestion; the follows rows are already in.
        tracing::warn!(user_id = %user_id, error = %e, "[FOLLOW-IMPORT] contact insert failed");
    }

    Ok(outcome)
}

/// Persist the run's counts on the Mastodon link so `GET /api/accounts` and the
/// sign-up UI can report them. Non-fatal by contract: metadata is a display
/// convenience, never the source of truth for the graph.
pub async fn persist_outcome(pool: &PgPool, user_id: Uuid, outcome: &ImportOutcome) {
    let patch = serde_json::json!({
        "mastodon_following_imported": outcome.written,
        "mastodon_following_seen": outcome.seen,
        "mastodon_following_imported_at": chrono::Utc::now().to_rfc3339(),
    });
    if let Err(e) = sqlx::query(
        "UPDATE linked_accounts SET metadata = metadata || $2::jsonb, updated_at = now() \
         WHERE user_id = $1 AND provider = 'mastodon'",
    )
    .bind(user_id)
    .bind(patch)
    .execute(pool)
    .await
    {
        tracing::warn!(user_id = %user_id, error = %e, "[FOLLOW-IMPORT] metadata write failed");
    }
}

/// Import the graph, persist the count, then pull the imported contacts'
/// content into the feed. Fire-and-forget: every caller is a request handler
/// whose response must not wait on dozen of WebFinger round-trips.
pub fn spawn_follow_import(
    pool: PgPool,
    user_id: Uuid,
    instance: String,
    access_token: String,
    jwt_secret: String,
) {
    tokio::spawn(async move {
        match import_follows(&pool, user_id, &instance, &access_token, &jwt_secret).await {
            Ok(outcome) => {
                tracing::info!(
                    user_id = %user_id,
                    seen = outcome.seen,
                    written = outcome.written,
                    skipped = outcome.skipped,
                    matched = outcome.matched_local,
                    "[FOLLOW-IMPORT] Mastodon follows imported"
                );
                persist_outcome(&pool, user_id, &outcome).await;
            }
            Err(e) => tracing::warn!(
                user_id = %user_id,
                instance = %instance,
                error = %e,
                "[FOLLOW-IMPORT] Mastodon follow import failed"
            ),
        }
        spawn_interest_ingest(pool, user_id, jwt_secret);
    });
}

/// Pull the (now stored) contacts' outbox content + interest hashtags into the
/// feed. No-op when `AP_INGEST_ENABLED` is off (`populate_interests` self-gates).
pub fn spawn_interest_ingest(pool: PgPool, user_id: Uuid, jwt_secret: String) {
    tokio::spawn(async move {
        if let Err(e) = crate::ap::follow_feed::populate_interests(&pool, user_id, &jwt_secret).await {
            tracing::warn!("[AP] populate_interests failed: {e}");
        }
    });
}

/// One home-instance follow → the handle the rest of the system matches on
/// (`@user@instance`, the archive importer's `remote_handle` form).
fn remote_handle(account: &MastodonFollowAccount) -> String {
    format!("@{}", account.acct)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_handle_matches_the_archive_form() {
        let account = MastodonFollowAccount {
            acct: "eff@mastodon.social".to_string(),
            url: Some("https://mastodon.social/@eff".to_string()),
            display_name: "EFF".to_string(),
        };
        // The archive importer stores `@user@host` in `remote_handle`; live
        // import must land in the same shape or `populate_interests` would
        // read two conventions.
        assert_eq!(remote_handle(&account), "@eff@mastodon.social");
    }
}
