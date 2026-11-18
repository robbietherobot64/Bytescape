//! Federated account retirement — add-identity-realignment.
//!
//! When a Bytescape account dies (immediate delete or soft-delete expiry), its
//! federated presence must die with it: nothing it did on remote servers may
//! outlive it (the aus.social orphan-favourite class), and no local federated
//! rows may be orphaned. Owner decision 2026-11-14 (OQ3): deletion federates on
//! BOTH protocols — ActivityPub (this module) and AT Protocol (repo purge +
//! repo-root tombstone, orchestrated by `api::accounts::teardown_account`).
//!
//! Sequence per retiring minted AP actor (design.md Decisions 2 + 4):
//!   1. STAGE — standing Like/Follow/Announce get a full-embed `Undo` (the
//!      original activity is embedded as `object`: Mastodon resolves
//!      `Undo(Like)` via `value_or_id(object.object)` and silently drops bare
//!      {id,type} stubs), queued to the original's audience. The retracted
//!      originals are DETACHED from the account (`user_id` → NULL) so they stay
//!      dereferenceable until the retractions are delivered ("available until
//!      the retraction is delivered" — `identity` spec).
//!   2. FREEZE — the `Delete(Tombstone)` fan-out set (every inbox this actor
//!      ever delivered to + accepted followers' inboxes, sharedInbox-deduped)
//!      is captured NOW, while `followers` and delivery history still exist —
//!      both die with the account row.
//!   3. DRAIN — [`spawn_drain_worker`] waits until no queued delivery for this
//!      actor is still pending, then queues `Delete(Tombstone)` to the frozen
//!      set (FIFO behind the retractions: migration 086 claims by insertion
//!      order and the Delete is only enqueued once the retractions left
//!      `pending`), and on the next clean cycle finalizes: drops the staged key
//!      material and purges the actor's staged rows once the owning account is
//!      gone ("purged with the account"; a superseded actor of a live owner
//!      keeps its payloads dereferenceable). The registry row stays
//!      forever so the actor id keeps answering with a Tombstone.
//!
//! While `drained_at IS NULL` the actor id still serves its PUBLIC key: remote
//! signature verification fetches the signing actor's key document, so without
//! it the retractions would be rejected (and the remote favourite would become
//! a permanent orphan). Only public key material is staged in the clear; the
//! private key (encrypted at rest, same format as `users.ap_private_key_pem`)
//! lives only for the drain window so the delivery worker can sign.

use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::store::AS_PUBLIC;

/// How often the drain worker checks retiring actors.
const DRAIN_INTERVAL_SECS: u64 = 300;

/// A retired actor registry row (the tombstone record that outlives the
/// account). Key material is present only until the drain completes.
pub struct RetiredActor {
    pub actor_id: String,
    pub username: String,
    pub public_key_pem: Option<String>,
    pub drained_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// `to`/`cc`/`bto`/`bcc` audience entries of an activity payload (mirrors the
/// bridge's private helper — kept local so retirement never depends on the
/// outbound bridge).
fn audience_of(payload: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for key in ["to", "cc", "bto", "bcc"] {
        match payload.get(key) {
            Some(Value::String(s)) => out.push(s.clone()),
            Some(Value::Array(a)) => {
                out.extend(a.iter().filter_map(|v| v.as_str().map(str::to_string)))
            }
            _ => {}
        }
    }
    out
}

/// Insert a retirement-staging activity. Deliberately `user_id = NULL`: these
/// rows must OUTLIVE the account (migration 087 cascades user-owned rows away,
/// which would strand the retractions before the delivery worker ran).
async fn insert_staging_activity(
    db: &PgPool,
    id: &str,
    actor_id: &str,
    activity_type: &str,
    object_id: Option<&str>,
    payload: &Value,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "INSERT INTO ap_activities (id, actor_id, activity_type, object_id, direction, payload, user_id) \
         VALUES ($1, $2, $3, $4, 'out', $5, NULL) \
         ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(actor_id)
    .bind(activity_type)
    .bind(object_id)
    .bind(payload)
    .execute(db)
    .await?;
    Ok(r.rows_affected() > 0)
}

/// Stage the retirement of one minted AP actor: registry row + full-embed
/// `Undo` retractions for every standing Like/Follow/Announce + the frozen
/// Delete fan-out set. Idempotent (registry upsert does nothing on repeat,
/// retraction loop is guarded by the exactly-once `undone_at` mark).
///
/// Must run BEFORE the account row is deleted: it reads `followers` and the
/// delivery history to freeze the fan-out set.
pub async fn stage_actor_retirement(
    db: &PgPool,
    user_id: Uuid,
) -> Result<Option<String>, String> {
    let user = crate::db::users::UserRow::find_by_id(db, user_id)
        .await
        .map_err(|e| format!("user lookup: {e}"))?
        .ok_or_else(|| "no such user".to_string())?;
    let Some(actor_id) = user.ap_actor_id.clone() else {
        // No minted AP actor (e.g. a post-realignment Mastodon signup acting
        // via its home instance): nothing of ours exists on the AP network.
        return Ok(None);
    };

    // 1) Freeze the Delete fan-out set while followers + history still exist.
    let mut inboxes: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let delivered: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT q.inbox_url FROM ap_delivery_queue q \
         JOIN ap_activities a ON a.id = q.activity_id \
         WHERE a.actor_id = $1",
    )
    .bind(&actor_id)
    .fetch_all(db)
    .await
    .map_err(|e| format!("delivery history: {e}"))?;
    for (inbox,) in delivered {
        if !inbox.is_empty() {
            inboxes.insert(inbox);
        }
    }
    let followers: Vec<(Option<String>, String)> = sqlx::query_as(
        "SELECT ra.shared_inbox_url, f.inbox_url FROM followers f \
         LEFT JOIN ap_remote_actors ra ON ra.actor_id = f.actor_id \
         WHERE f.user_id = $1 AND f.accepted = true",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
    .map_err(|e| format!("followers lookup: {e}"))?;
    for (shared, inbox) in followers {
        let inbox = shared.unwrap_or(inbox);
        if !inbox.is_empty() {
            inboxes.insert(inbox);
        }
    }

    // 2) Registry row: tombstone record + signing material for the drain
    //    window. ON CONFLICT DO NOTHING — a repeat stage keeps the first
    //    frozen set and key material.
    sqlx::query(
        "INSERT INTO retired_ap_actors \
           (actor_id, user_id, username, public_key_pem, private_key_pem, delete_inboxes) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (actor_id) DO NOTHING",
    )
    .bind(&actor_id)
    .bind(user_id)
    .bind(&user.username)
    .bind(&user.ap_public_key_pem)
    .bind(&user.ap_private_key_pem)
    .bind(serde_json::json!(inboxes.iter().collect::<Vec<_>>()))
    .execute(db)
    .await
    .map_err(|e| format!("retirement registry insert: {e}"))?;

    // 3) Retractions: full-embed Undo per standing Like/Follow/Announce. The
    //    exactly-once `undone_at` mark is the concurrency guard (same one the
    //    bridge relies on); only rows we mark ourselves are retracted here.
    //    In addition, an original that ALREADY references an Undo (e.g. a
    //    remediation insert carrying `undone_at = NULL` on the original) is
    //    excluded entirely — the shared "at most one Undo per original"
    //    invariant (`bridge::undo_already_queued`), honoured here by NOT
    //    EXISTS before the CAS so the guard short-circuits without marking.
    let standing: Vec<(String, String, Value)> = sqlx::query_as(
        "UPDATE ap_activities SET undone_at = now() \
         WHERE actor_id = $1 AND activity_type IN ('Like','Follow','Announce') \
           AND undone_at IS NULL \
           AND NOT EXISTS (SELECT 1 FROM ap_activities u \
                            WHERE u.object_id = ap_activities.id \
                              AND u.activity_type = 'Undo') \
         RETURNING id, activity_type, payload",
    )
    .bind(&actor_id)
    .fetch_all(db)
    .await
    .map_err(|e| format!("standing interactions: {e}"))?;
    for (original_id, _original_type, original_payload) in standing {
        // Detach the original from the account so it stays dereferenceable
        // until the retraction is delivered (it is purged at drain finalize).
        sqlx::query("UPDATE ap_activities SET user_id = NULL WHERE id = $1")
            .bind(&original_id)
            .execute(db)
            .await
            .map_err(|e| format!("original detach: {e}"))?;

        let undo_id = format!(
            "{}/activities/undo-{}",
            actor_id.trim_end_matches('/'),
            Uuid::new_v4().simple()
        );
        let mut payload = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": undo_id,
            "type": "Undo",
            "actor": actor_id,
        });
        for key in ["to", "cc"] {
            if let Some(v) = original_payload.get(key) {
                payload[key] = v.clone();
            }
        }
        // The FULL original is embedded: Mastodon's undo handler dispatches on
        // object.type and resolves the target via value_or_id(object.object) —
        // a bare {id,type} stub is silently dropped (the remote favourite
        // would persist forever: the bug this whole change exists to kill).
        payload["object"] = original_payload.clone();

        if insert_staging_activity(db, &undo_id, &actor_id, "Undo", Some(&original_id), &payload)
            .await
            .map_err(|e| format!("undo staging insert: {e}"))?
        {
            let audience = audience_of(&original_payload);
            let inboxes = super::store::audience_inboxes(db, &actor_id, &audience).await;
            for inbox in inboxes {
                super::delivery::enqueue(db, &undo_id, &inbox)
                    .await
                    .map_err(|e| format!("undo enqueue: {e}"))?;
            }
        }
    }

    tracing::info!(
        "[retire] staged retirement of {actor_id} (user {user_id}, fan-out {} inbox(es))",
        inboxes.len()
    );
    Ok(Some(actor_id))
}

/// Grace-expiry death path (the `purge_expired` sweeper's replacement): for
/// every expired soft-deleted account, stage its federated retirement first —
/// retraction rows deliberately outlive the row delete (user_id NULL) and the
/// drain worker delivers them afterwards — then delete the row. Delivery
/// failures never block deletion (spec: best-effort, recorded).
pub async fn sweep_expired(db: &PgPool) -> Result<u64, sqlx::Error> {
    let expired: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM users WHERE deleted_at IS NOT NULL AND deleted_at < NOW()",
    )
    .fetch_all(db)
    .await?;
    let mut purged = 0u64;
    for (user_id,) in expired {
        if let Err(e) = stage_actor_retirement(db, user_id).await {
            tracing::warn!(
                "[retire] staging for expired account {user_id} failed: {e} — purging anyway"
            );
        }
        // Home-REST favourites need the home actor's unfavourite, not an AP
        // Undo — sweep them while the link tokens still exist.
        match crate::api::feed::retract_home_rest_reactions(db, user_id).await {
            Ok(n) if n > 0 => tracing::info!("[retire] retracted {n} home-REST favourite(s) for expired {user_id}"),
            Ok(_) => {}
            Err(e) => tracing::warn!("[retire] home-REST retraction for expired {user_id} failed: {e} — purging anyway"),
        }
        purged += sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(db)
            .await?
            .rows_affected();
    }
    Ok(purged)
}

/// The owning account is gone (called right after the user row delete): purge
/// the kept payloads of any PREVIOUSLY retired actor whose drain already
/// completed — a superseded actor's payloads outlive retirement only while its
/// owner lives ("purged with the account"). Actors still draining purge at
/// finalize instead (owner gone by then).
pub async fn on_account_gone(db: &PgPool, user_id: Uuid) {
    let done: Vec<(String,)> = sqlx::query_as(
        "SELECT actor_id FROM retired_ap_actors \
         WHERE user_id = $1 AND drained_at IS NOT NULL",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
    .unwrap_or_default();
    for (actor_id,) in done {
        if let Err(e) =
            sqlx::query("DELETE FROM ap_activities WHERE actor_id = $1 AND user_id IS NULL")
                .bind(&actor_id)
                .execute(db)
                .await
        {
            tracing::warn!("[retire] purge of retired rows for {actor_id} failed: {e}");
        }
    }
}

/// One-shot migration runner (tasks.md 3.3): retire the minted AP actors of
/// Mastodon-signed-up users — they act as their home-instance actor now, and a
/// second dereferenceable "person" must not linger in remote caches. Gated by
/// `RETIRE_MASTODON_MINTED_ACTORS` (absent = never runs, `apply` = execute,
/// anything else = audit-only dry run) and audit-logged per actor.
/// `allow_usernames` limits a run to specific accounts (verification hook);
/// `None` = every eligible account. Returns how many actors were staged.
///
/// Per actor: stage the shared retraction+Delete routine
/// ([`stage_actor_retirement`]), then drop the minted keypair from `users` —
/// the staged copy signs the drain ([`crate::ap::delivery::signing_identity`]
/// falls back to it). The actor id serves its key document while draining and
/// a Tombstone afterwards; its published activity URLs stay dereferenceable
/// for the live owner (actors/spec.md) — see `drain_once`'s finalize gate.
/// Bluesky/email minted actors are untouched: eligibility requires a
/// `linked_accounts` row with `provider = 'mastodon'`.
pub async fn supersede_minted_actors(
    state: &crate::AppState,
    apply: bool,
    allow_usernames: Option<Vec<String>>,
) -> Result<u32, String> {
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT u.id, u.username FROM users u \
         JOIN linked_accounts la ON la.user_id = u.id AND la.provider = 'mastodon' \
         WHERE u.ap_actor_id IS NOT NULL \
           AND NOT EXISTS (SELECT 1 FROM retired_ap_actors r WHERE r.user_id = u.id) \
         ORDER BY u.username",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| format!("candidate query: {e}"))?;
    let mut staged = 0u32;
    for (user_id, username) in rows {
        if let Some(allow) = &allow_usernames {
            if !allow.contains(&username) {
                continue;
            }
        }
        if !apply {
            tracing::info!("[retire][dry-run] would retire minted actor of mastodon user {username}");
            continue;
        }
        match stage_actor_retirement(&state.db, user_id).await {
            Ok(Some(actor_id)) => {
                // Key material now lives in the retirement staging (the drain
                // signs with it); the minted keypair goes away per design
                // Decision 2 step 4. `ap_actor_id` is kept — the registry wins
                // serving, and it keeps identity resolution unambiguous.
                sqlx::query(
                    "UPDATE users SET ap_enabled = false, \
                     ap_private_key_pem = NULL, ap_public_key_pem = NULL WHERE id = $1",
                )
                .bind(user_id)
                .execute(&state.db)
                .await
                .map_err(|e| format!("key drop for {username}: {e}"))?;
                tracing::info!(
                    "[retire] superseded minted actor {actor_id} of mastodon user {username} (owner row kept)"
                );
                staged += 1;
            }
            Ok(None) => {
                tracing::info!("[retire] {username} has no minted actor; nothing to retire");
            }
            Err(e) => {
                tracing::warn!("[retire] supersession of {username} failed: {e} — continuing");
            }
        }
    }
    Ok(staged)
}

/// Queue the `Delete(Tombstone)` fan-out for one retiring actor (drain phase).
/// Idempotent: `delete_enqueued_at` marks the single enqueue moment.
async fn enqueue_actor_delete(db: &PgPool, row: &RetiredActorStaging) -> Result<(), sqlx::Error> {
    let delete_id = format!(
        "{}/activities/delete-{}",
        row.actor_id.trim_end_matches('/'),
        Uuid::new_v4().simple()
    );
    let payload = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": delete_id,
        "type": "Delete",
        "actor": row.actor_id,
        "object": {
            "id": row.actor_id,
            "type": "Tombstone",
            "deleted": chrono::Utc::now().to_rfc3339(),
        },
        "to": [AS_PUBLIC],
    });
    insert_staging_activity(db, &delete_id, &row.actor_id, "Delete", Some(&row.actor_id), &payload)
        .await?;
    for inbox in &row.delete_inboxes {
        super::delivery::enqueue(db, &delete_id, inbox).await?;
    }
    sqlx::query("UPDATE retired_ap_actors SET delete_enqueued_at = now() WHERE actor_id = $1")
        .bind(&row.actor_id)
        .execute(db)
        .await?;
    Ok(())
}

/// Internal staging row shape for the drain worker.
struct RetiredActorStaging {
    actor_id: String,
    delete_inboxes: Vec<String>,
    delete_enqueued_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// One drain pass over all retiring actors. Returns how many actors advanced.
///
/// * No pending deliveries + Delete not yet enqueued → queue the Delete
///   fan-out (it lands behind the retractions in seq order).
/// * No pending deliveries + Delete enqueued (or nothing to send) → finalize:
///   drop staged key material, purge the actor's staged activity rows (their
///   queue rows cascade), and stamp `drained_at`. The registry row remains so
///   the actor id serves a Tombstone forever.
pub async fn drain_once(db: &PgPool) -> Result<u32, sqlx::Error> {
    let rows: Vec<(String, serde_json::Value, Option<chrono::DateTime<chrono::Utc>>)> =
        sqlx::query_as(
            "SELECT actor_id, delete_inboxes, delete_enqueued_at \
             FROM retired_ap_actors WHERE drained_at IS NULL",
        )
        .fetch_all(db)
        .await?;
    let mut advanced = 0u32;
    for (actor_id, delete_inboxes, delete_enqueued_at) in rows {
        let pending: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ap_delivery_queue q \
             JOIN ap_activities a ON a.id = q.activity_id \
             WHERE a.actor_id = $1 AND q.status = 'pending'",
        )
        .bind(&actor_id)
        .fetch_one(db)
        .await?;
        if pending.0 > 0 {
            continue; // retractions (or the Delete) still on their way out
        }
        if delete_enqueued_at.is_none() {
            let inboxes: Vec<String> = delete_inboxes
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            enqueue_actor_delete(
                db,
                &RetiredActorStaging {
                    actor_id: actor_id.clone(),
                    delete_inboxes: inboxes,
                    delete_enqueued_at: None,
                },
            )
            .await?;
            advanced += 1;
            continue;
        }
        // Finalize: keys and registry tombstone state. The staged rows purge
        // only once the owning account is gone (spec: "then purged with the
        // account"); a superseded actor of a LIVE owner keeps its published
        // payloads dereferenceable per actors/spec.md.
        sqlx::query(
            "UPDATE retired_ap_actors SET drained_at = now(), \
             private_key_pem = NULL, public_key_pem = NULL WHERE actor_id = $1",
        )
        .bind(&actor_id)
        .execute(db)
        .await?;
        let owner_gone: (bool,) = sqlx::query_as(
            "SELECT NOT EXISTS ( \
             SELECT 1 FROM users u JOIN retired_ap_actors r ON r.user_id = u.id \
             WHERE r.actor_id = $1)",
        )
        .bind(&actor_id)
        .fetch_one(db)
        .await?;
        if owner_gone.0 {
            sqlx::query("DELETE FROM ap_activities WHERE actor_id = $1 AND user_id IS NULL")
                .bind(&actor_id)
                .execute(db)
                .await?;
            tracing::info!("[retire] drain complete for {actor_id}: rows purged with the account");
        } else {
            tracing::info!("[retire] drain complete for {actor_id}: payloads kept for live owner");
        }
        advanced += 1;
    }
    Ok(advanced)
}

/// Background drain worker (spawned once from `main`). Queue state is in
/// Postgres, so retiring actors survive restarts mid-drain.
pub fn spawn_drain_worker(db: PgPool) {
    tokio::spawn(async move {
        tracing::info!("[retire] retirement drain worker started");
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(DRAIN_INTERVAL_SECS)).await;
            match drain_once(&db).await {
                Ok(0) => {}
                Ok(n) => tracing::info!("[retire] drain worker advanced {n} retiring actor(s)"),
                Err(e) => tracing::warn!("[retire] drain worker pass failed: {e}"),
            }
        }
    });
}

/// The registry row for a retired actor id (serving decision: key document
/// while draining, Tombstone after).
pub async fn load_retired_actor(db: &PgPool, actor_id: &str) -> Option<RetiredActor> {
    let row: Option<(String, String, Option<String>, Option<chrono::DateTime<chrono::Utc>>)> =
        sqlx::query_as(
            "SELECT actor_id, username, public_key_pem, drained_at \
             FROM retired_ap_actors WHERE actor_id = $1",
        )
        .bind(actor_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten();
    row.map(|(actor_id, username, public_key_pem, drained_at)| RetiredActor {
        actor_id,
        username,
        public_key_pem,
        drained_at,
    })
}

/// Registry lookup by username (the actor routes address actors as
/// `/@{username}`).
pub async fn load_retired_actor_by_username(db: &PgPool, username: &str) -> Option<RetiredActor> {
    let row: Option<(String, String, Option<String>, Option<chrono::DateTime<chrono::Utc>>)> =
        sqlx::query_as(
            "SELECT actor_id, username, public_key_pem, drained_at \
             FROM retired_ap_actors WHERE username = $1",
        )
        .bind(username)
        .fetch_optional(db)
        .await
        .ok()
        .flatten();
    row.map(|(actor_id, username, public_key_pem, drained_at)| RetiredActor {
        actor_id,
        username,
        public_key_pem,
        drained_at,
    })
}

/// The document a retired actor id serves: its PUBLIC key document while the
/// drain runs (remote signature verification needs it to accept the
/// retractions), a Tombstone afterwards.
pub fn retired_actor_document(r: &RetiredActor) -> Value {
    match (&r.drained_at, &r.public_key_pem) {
        (None, Some(public_key_pem)) => json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://w3id.org/security/v1",
            ],
            "id": r.actor_id,
            "type": "Person",
            "preferredUsername": r.username,
            "inbox": format!("{}/inbox", r.actor_id.trim_end_matches('/')),
            "outbox": format!("{}/outbox", r.actor_id.trim_end_matches('/')),
            "followers": format!("{}/followers", r.actor_id.trim_end_matches('/')),
            "publicKey": {
                "id": format!("{}#main-key", r.actor_id.trim_end_matches('/')),
                "owner": r.actor_id,
                "publicKeyPem": public_key_pem,
            },
            // Honest marker: the person is gone; the document exists only so
            // in-flight retractions verify.
            "deleted": true,
        }),
        _ => json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": r.actor_id,
            "type": "Tombstone",
            "deleted": chrono::Utc::now().to_rfc3339(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ap::testutil::{connect_pool, enable_ap_flags, make_local_actor};

    async fn cleanup(pool: &PgPool, actor_id: &str, user_id: Uuid) {
        let _ = sqlx::query("DELETE FROM ap_delivery_queue WHERE activity_id IN (SELECT id FROM ap_activities WHERE actor_id = $1)")
            .bind(actor_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id = $1")
            .bind(actor_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM retired_ap_actors WHERE actor_id = $1")
            .bind(actor_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM followers WHERE user_id = $1")
            .bind(user_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(pool)
            .await;
    }

    /// Seed one standing outbound Like under `actor_id`, user-owned (the
    /// normal shape `insert_activity` produces). The target author is seeded
    /// as a known remote actor so `audience_inboxes` resolves an inbox for the
    /// Like's `to` — without it the staged Undo has no delivery rows and the
    /// drain sees "nothing pending", letting the Delete leapfrog its Undo.
    async fn seed_standing_like(pool: &PgPool, actor_id: &str, user_id: Uuid, suffix: &str) -> String {
        let author = crate::ap::testutil::make_signer("https://remote.test/users/z");
        crate::ap::testutil::seed_remote_actor(
            pool,
            &author,
            "https://remote.test/users/z/inbox",
            None,
        )
        .await;
        let id = format!("{actor_id}/activities/like-{suffix}");
        let payload = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": id,
            "type": "Like",
            "actor": actor_id,
            "object": format!("https://remote.test/users/z/statuses/{suffix}"),
            "to": ["https://remote.test/users/z"],
        });
        sqlx::query(
            "INSERT INTO ap_activities (id, actor_id, activity_type, object_id, direction, payload, user_id) \
             VALUES ($1, $2, 'Like', $3, 'out', $4, $5)",
        )
        .bind(&id)
        .bind(actor_id)
        .bind(format!("https://remote.test/users/z/statuses/{suffix}"))
        .bind(&payload)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed like");
        id
    }

    /// Task 4.1 acceptance: staging embeds the FULL original in the Undo
    /// (Mastodon's undo_like rule), marks the original undone exactly once,
    /// detaches it for dereference, freezes the fan-out set, and the staged
    /// rows survive the account's deletion (user_id NULL beats the cascade).
    #[tokio::test]
    async fn staging_retracts_survives_account_delete_and_drains() {
        let _due = crate::ap::testutil::due_rows_lock().lock().await;
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let la = make_local_actor(&pool, "bytescape.test").await;
        let suffix = crate::ap::testutil::unique();
        let like_id = seed_standing_like(&pool, &la.actor_id, la.user.id, &suffix).await;

        // Delivery history for the tombstone fan-out freeze (3.1): the
        // original Like reached the author's inbox before retirement.
        crate::ap::delivery::enqueue(&pool, &like_id, "https://remote.test/users/z/inbox")
            .await
            .expect("seed delivery history");
        sqlx::query("UPDATE ap_delivery_queue SET status = 'delivered' WHERE activity_id = $1")
            .bind(&like_id)
            .execute(&pool)
            .await
            .expect("history delivered");

        let staged = stage_actor_retirement(&pool, la.user.id)
            .await
            .expect("stage");
        assert_eq!(staged.as_deref(), Some(la.actor_id.as_str()));
        // 3.1: the frozen tombstone audience equals the delivery-history set.
        let (frozen,): (Value,) =
            sqlx::query_as("SELECT delete_inboxes FROM retired_ap_actors WHERE actor_id = $1")
                .bind(&la.actor_id)
                .fetch_one(&pool)
                .await
                .expect("frozen fan-out");
        assert_eq!(
            frozen,
            json!(["https://remote.test/users/z/inbox"]),
            "tombstone audience equals the delivery-history set"
        );

        // The Undo embeds the full original — object.object must be present.
        let (undo_id, undo_payload): (String, Value) = sqlx::query_as(
            "SELECT id, payload FROM ap_activities \
             WHERE actor_id = $1 AND activity_type = 'Undo' AND object_id = $2",
        )
        .bind(&la.actor_id)
        .bind(&like_id)
        .fetch_one(&pool)
        .await
        .expect("staged undo");
        assert_eq!(undo_payload["object"]["id"], json!(like_id), "Undo embeds the exact original");
        assert_eq!(
            undo_payload["object"]["object"],
            json!(format!("https://remote.test/users/z/statuses/{suffix}")),
            "Undo.object.object carries the Like target (Mastodon resolves it)"
        );

        // Original marked undone + detached (dereferenceable past deletion).
        let (undone_at, owner): (Option<chrono::DateTime<chrono::Utc>>, Option<Uuid>) =
            sqlx::query_as("SELECT undone_at, user_id FROM ap_activities WHERE id = $1")
                .bind(&like_id)
                .fetch_one(&pool)
                .await
                .expect("original row");
        assert!(undone_at.is_some(), "original marked undone exactly once");
        assert!(owner.is_none(), "retracted original detached for dereference");

        // Account dies — staged rows must survive the 087 cascade.
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(la.user.id)
            .execute(&pool)
            .await
            .expect("user delete");
        let survivors: (i64,) =
            sqlx::query_as("SELECT count(*) FROM ap_activities WHERE actor_id = $1")
                .bind(&la.actor_id)
                .fetch_one(&pool)
                .await
                .expect("survivor count");
        assert_eq!(survivors.0, 2, "original + staged Undo outlive the account");
        let registry = load_retired_actor(&pool, &la.actor_id).await;
        assert!(registry.is_some(), "registry row outlives the account");

        // Drain: pending Undo blocks the Delete fan-out…
        let advanced = drain_once(&pool).await.expect("drain pass 1");
        assert_eq!(advanced, 0, "pending retraction blocks the Delete enqueue");
        let deletes: (i64,) =
            sqlx::query_as("SELECT count(*) FROM ap_activities WHERE actor_id = $1 AND activity_type = 'Delete'")
                .bind(&la.actor_id)
                .fetch_one(&pool)
                .await
                .expect("delete count");
        assert_eq!(deletes.0, 0, "no Delete while retractions are pending");

        // …once delivered, the Delete goes out…
        sqlx::query(
            "UPDATE ap_delivery_queue SET status = 'delivered' \
             WHERE activity_id IN (SELECT id FROM ap_activities WHERE actor_id = $1)",
        )
        .bind(&la.actor_id)
        .execute(&pool)
        .await
        .expect("mark delivered");
        let advanced = drain_once(&pool).await.expect("drain pass 2");
        assert_eq!(advanced, 1, "Delete fan-out enqueued once retractions drained");
        let staged_delete = load_retired_actor(&pool, &la.actor_id).await.expect("registry");
        assert!(staged_delete.drained_at.is_none(), "still draining the Delete");

        // …and the next clean pass finalizes: rows purged, tombstone served.
        sqlx::query(
            "UPDATE ap_delivery_queue SET status = 'delivered' \
             WHERE activity_id IN (SELECT id FROM ap_activities WHERE actor_id = $1)",
        )
        .bind(&la.actor_id)
        .execute(&pool)
        .await
        .expect("mark delivered 2");
        let advanced = drain_once(&pool).await.expect("drain pass 3");
        assert_eq!(advanced, 1, "finalize");
        let final_row = load_retired_actor(&pool, &la.actor_id).await.expect("registry");
        assert!(final_row.drained_at.is_some(), "drained — tombstone state");
        assert!(final_row.public_key_pem.is_none(), "key material dropped at drain");
        let doc = retired_actor_document(&final_row);
        assert_eq!(doc["type"], json!("Tombstone"), "drained actor serves a Tombstone");
        let remaining: (i64,) =
            sqlx::query_as("SELECT count(*) FROM ap_activities WHERE actor_id = $1")
                .bind(&la.actor_id)
                .fetch_one(&pool)
                .await
                .expect("purge count");
        assert_eq!(remaining.0, 0, "staged rows purged after drain");

        cleanup(&pool, &la.actor_id, la.user.id).await;
    }

    /// Tasks 3.3/3.4: the gated one-shot run retires Mastodon-signup minted
    /// actors (retraction staged, keypair dropped, owner row kept) and never
    /// touches Bluesky/email minted actors. Dry-run stages nothing.
    #[tokio::test]
    async fn supersede_retires_only_mastodon_minted_actors() {
        let _due = crate::ap::testutil::due_rows_lock().lock().await;
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let Some(state) = crate::ap::testutil::test_state(&pool) else { return };
        let la = make_local_actor(&pool, "bytescape.test").await;
        let plain = make_local_actor(&pool, "bytescape.test").await;
        let suffix = crate::ap::testutil::unique();
        // `la` acts as its home-instance actor from now on: it has a mastodon link.
        crate::db::accounts::LinkedAccountRow::create(
            &pool,
            la.user.id,
            "mastodon",
            &format!("{}@piaille.fr", la.username),
            &la.username,
            "tok",
            None,
            None,
            None,
            Some("read write:favourites"),
            &json!({
                "instance": "piaille.fr",
                "actor_url": format!("https://piaille.fr/@{}", la.username),
            }),
        )
        .await
        .expect("link mastodon");
        let like_id = seed_standing_like(&pool, &la.actor_id, la.user.id, &suffix).await;

        // Gated: dry-run (audit log only) stages nothing.
        let dry = supersede_minted_actors(
            &state,
            false,
            Some(vec![la.username.clone(), plain.username.clone()]),
        )
        .await
        .expect("dry run");
        assert_eq!(dry, 0, "dry-run stages nothing");
        assert!(load_retired_actor(&pool, &la.actor_id).await.is_none());

        let staged = supersede_minted_actors(
            &state,
            true,
            Some(vec![la.username.clone(), plain.username.clone()]),
        )
        .await
        .expect("apply run");
        assert_eq!(staged, 1, "only the mastodon-linked actor retires (3.3)");

        // 3.3: keypair dropped from the owner row, retraction staged, owner kept.
        let me = crate::db::users::UserRow::find_by_id(&pool, la.user.id)
            .await
            .expect("reload")
            .expect("row");
        assert!(!me.ap_enabled, "minted actor no longer serves live");
        assert!(me.ap_private_key_pem.is_none() && me.ap_public_key_pem.is_none(), "keypair dropped");
        assert!(load_retired_actor(&pool, &la.actor_id).await.is_some(), "registry row staged");
        let (undos,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ap_activities WHERE actor_id = $1 AND activity_type = 'Undo'",
        )
        .bind(&la.actor_id)
        .fetch_one(&pool)
        .await
        .expect("undo count");
        assert_eq!(undos, 1, "standing interaction retracted");
        let (marked,): (Option<chrono::DateTime<chrono::Utc>>,) =
            sqlx::query_as("SELECT undone_at FROM ap_activities WHERE id = $1")
                .bind(&like_id)
                .fetch_one(&pool)
                .await
                .expect("original");
        assert!(marked.is_some(), "original marked for retraction");

        // 3.4: the Bluesky/email minted actor is untouched.
        let untouched = crate::db::users::UserRow::find_by_id(&pool, plain.user.id)
            .await
            .expect("reload")
            .expect("row");
        assert!(untouched.ap_enabled, "bluesky/email actor untouched");
        assert!(untouched.ap_private_key_pem.is_some(), "bluesky/email keys unchanged");

        for actor in [&la, &plain] {
            cleanup(&pool, &actor.actor_id, actor.user.id).await;
        }
    }

    /// Task 4.2 acceptance: the grace-expiry death path runs the same
    /// retirement before the row delete — retraction rows outlive the account
    /// (user_id NULL) and drain in the background.
    #[tokio::test]
    async fn expiry_sweep_stages_retirement_before_delete() {
        let _due = crate::ap::testutil::due_rows_lock().lock().await;
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let la = make_local_actor(&pool, "bytescape.test").await;
        let suffix = crate::ap::testutil::unique();
        let _like = seed_standing_like(&pool, &la.actor_id, la.user.id, &suffix).await;

        crate::db::users::UserRow::soft_delete(
            &pool,
            la.user.id,
            chrono::Utc::now() - chrono::Duration::hours(1),
        )
        .await
        .expect("soft delete");

        let purged = sweep_expired(&pool).await.expect("sweep");
        assert!(purged >= 1, "expired account purged");

        assert!(
            crate::db::users::UserRow::find_by_id(&pool, la.user.id).await.expect("reload").is_none(),
            "account row deleted at expiry"
        );
        assert!(
            load_retired_actor(&pool, &la.actor_id).await.is_some(),
            "retirement staged BEFORE the row delete"
        );
        let (undos,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ap_activities WHERE actor_id = $1 AND activity_type = 'Undo'",
        )
        .bind(&la.actor_id)
        .fetch_one(&pool)
        .await
        .expect("undo count");
        assert_eq!(undos, 1, "retraction rows created and outlive the account");
        let (owner,): (Option<Uuid>,) =
            sqlx::query_as("SELECT user_id FROM ap_activities WHERE actor_id = $1 AND activity_type = 'Undo'")
                .bind(&la.actor_id)
                .fetch_one(&pool)
                .await
                .expect("staged row");
        assert!(owner.is_none(), "staged rows detached (survive the cascade)");

        // "Created AND delivered": the drain delivers the retraction and the
        // Delete fan-out, then finalizes — zero rows left (purged with the
        // account).
        sqlx::query(
            "UPDATE ap_delivery_queue SET status = 'delivered' \
             WHERE activity_id IN (SELECT id FROM ap_activities WHERE actor_id = $1)",
        )
        .bind(&la.actor_id)
        .execute(&pool)
        .await
        .expect("deliver retraction");
        drain_once(&pool).await.expect("drain (Delete enqueue)");
        sqlx::query(
            "UPDATE ap_delivery_queue SET status = 'delivered' \
             WHERE activity_id IN (SELECT id FROM ap_activities WHERE actor_id = $1)",
        )
        .bind(&la.actor_id)
        .execute(&pool)
        .await
        .expect("deliver delete");
        drain_once(&pool).await.expect("drain (finalize)");
        let (left,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM ap_activities WHERE actor_id = $1")
                .bind(&la.actor_id)
                .fetch_one(&pool)
                .await
                .expect("leftover rows");
        assert_eq!(left, 0, "retraction + Delete delivered, rows purged with the account");

        cleanup(&pool, &la.actor_id, la.user.id).await;
    }

    /// Task 4.4 acceptance: user-owned federated rows cascade with the account
    /// (no orphans), while a draining actor's staged rows deliberately do not.
    #[tokio::test]
    async fn user_owned_activities_cascade_with_account() {
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let la = make_local_actor(&pool, "bytescape.test").await;
        let suffix = crate::ap::testutil::unique();
        let _ = seed_standing_like(&pool, &la.actor_id, la.user.id, &suffix).await;

        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(la.user.id)
            .execute(&pool)
            .await
            .expect("user delete");
        let remaining: (i64,) =
            sqlx::query_as("SELECT count(*) FROM ap_activities WHERE actor_id = $1")
                .bind(&la.actor_id)
                .fetch_one(&pool)
                .await
                .expect("count");
        assert_eq!(remaining.0, 0, "user-owned rows cascade — the orphan gap is closed");

        cleanup(&pool, &la.actor_id, la.user.id).await;
    }

    /// Task 4.3 acceptance: soft-delete + restore emit nothing federated.
    #[tokio::test]
    async fn soft_delete_and_restore_emit_no_federation() {
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let la = make_local_actor(&pool, "bytescape.test").await;
        let suffix = crate::ap::testutil::unique();
        let _ = seed_standing_like(&pool, &la.actor_id, la.user.id, &suffix).await;

        crate::db::users::UserRow::soft_delete(
            &pool,
            la.user.id,
            chrono::Utc::now() + chrono::Duration::days(30),
        )
        .await
        .expect("soft delete");
        crate::db::users::UserRow::restore(&pool, la.user.id)
            .await
            .expect("restore")
            .expect("restored row");

        let acts: (i64,) =
            sqlx::query_as("SELECT count(*) FROM ap_activities WHERE actor_id = $1")
                .bind(&la.actor_id)
                .fetch_one(&pool)
                .await
                .expect("count");
        assert_eq!(acts.0, 1, "only the pre-existing like — no Undo/Delete staged");
        assert!(
            load_retired_actor(&pool, &la.actor_id).await.is_none(),
            "no retirement staged on soft-delete/restore"
        );

        cleanup(&pool, &la.actor_id, la.user.id).await;
    }

    /// CR-5: the retirement retraction loop must honour the shared "at most
    /// one Undo per original" invariant (`bridge::undo_already_queued`). An
    /// original that already carries a referencing Undo — e.g. a remediation
    /// insert whose `undone_at` mark was never set — must NOT get a second
    /// Undo at retirement (a double Undo can toggle remote state back on).
    #[tokio::test]
    async fn staging_never_double_undoes_a_remediation_shaped_original() {
        let _due = crate::ap::testutil::due_rows_lock().lock().await;
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let la = make_local_actor(&pool, "bytescape.test").await;
        let suffix = crate::ap::testutil::unique();
        let like_id = seed_standing_like(&pool, &la.actor_id, la.user.id, &suffix).await;

        // Remediation-shaped Undo: inserted WITHOUT the `undone_at` mark
        // (exactly the shape the committed bridge test constructs — the
        // corrected-Undo tooling does this). `undone_at` on the original
        // stays NULL.
        let pre_undo = format!("{}/activities/undo-remed{suffix}", la.actor_id.trim_end_matches('/'));
        sqlx::query(
            "INSERT INTO ap_activities (id, actor_id, activity_type, object_id, direction, payload, user_id) \
             VALUES ($1, $2, 'Undo', $3, 'out', $4, $5)",
        )
        .bind(&pre_undo)
        .bind(&la.actor_id)
        .bind(&like_id)
        .bind(serde_json::json!({"id": pre_undo, "type": "Undo", "object": {"id": like_id, "type": "Like"}}))
        .bind(la.user.id)
        .execute(&pool)
        .await
        .expect("seed pre-existing undo");

        let staged = stage_actor_retirement(&pool, la.user.id)
            .await
            .expect("stage");
        assert_eq!(staged.as_deref(), Some(la.actor_id.as_str()));

        // Exactly ONE Undo references the original: the remediation one. The
        // row is excluded from the standing query entirely (same short-circuit
        // as `undo_already_queued` before the bridge's CAS), so its `undone_at`
        // is left alone — the remediation Undo IS the retraction.
        let (undo_count,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ap_activities WHERE object_id = $1 AND activity_type = 'Undo'",
        )
        .bind(&like_id)
        .fetch_one(&pool)
        .await
        .expect("undo count");
        assert_eq!(undo_count, 1, "exactly one Undo may ever reference one original");
        let (undone_at,): (Option<chrono::DateTime<chrono::Utc>>,) =
            sqlx::query_as("SELECT undone_at FROM ap_activities WHERE id = $1")
                .bind(&like_id)
                .fetch_one(&pool)
                .await
                .expect("original row");
        assert!(
            undone_at.is_none(),
            "the guard short-circuits before the CAS — no mark, no second Undo"
        );

        cleanup(&pool, &la.actor_id, la.user.id).await;
    }
}
