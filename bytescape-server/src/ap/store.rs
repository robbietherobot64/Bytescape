//! Shared persistence helpers for the AP interaction surface (Phase C).
//!
//! Thin data-access layer over the Phase C tables — the inbox, the native
//! bridge, and the UI entry points all store activities and queue deliveries
//! through here so dedupe semantics stay identical everywhere:
//! * [`insert_activity`] — `ON CONFLICT DO NOTHING` (any unique column): the
//!   returned `false` means "already stored" (replay), never an error.
//! * [`enqueue_for_inboxes`] — unique per `(activity_id, inbox_url)`.
//! * [`audience_inboxes`] — resolves an activity's `to`/`cc` to concrete
//!   delivery inboxes: expands the sender's followers collection, skips
//!   `as:Public` and local actors, dedupes via `sharedInbox`.

use serde_json::Value;
use sqlx::PgPool;

/// The ActivityStreams Public collection — never a delivery target.
pub const AS_PUBLIC: &str = "https://www.w3.org/ns/activitystreams#Public";

/// Store one activity. Returns `true` when the row was newly inserted,
/// `false` when an activity with the same id (or the same `local_uri` undoable
/// original) already exists — the replay/dedupe signal.
pub async fn insert_activity(
    tx: &mut sqlx::PgConnection,
    id: &str,
    actor_id: &str,
    activity_type: &str,
    object_id: Option<&str>,
    local_uri: Option<&str>,
    direction: &str,
    payload: &Value,
) -> Result<bool, sqlx::Error> {
    // Bare ON CONFLICT DO NOTHING covers BOTH unique columns (id PK and the
    // local_uri undoable-original slot): one undoable original per record.
    // user_id is derived from the owning local actor (NULL for remote actors
    // and for retirement-staging rows): migration 087's ON DELETE CASCADE is
    // what guarantees "federated artifacts do not outlive their account".
    let r = sqlx::query(
        "INSERT INTO ap_activities (id, actor_id, activity_type, object_id, local_uri, direction, payload, user_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, (SELECT id FROM users WHERE ap_actor_id = $2)) \
         ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(actor_id)
    .bind(activity_type)
    .bind(object_id)
    .bind(local_uri)
    .bind(direction)
    .bind(payload)
    .execute(&mut *tx)
    .await?;
    Ok(r.rows_affected() > 0)
}

/// Queue one delivery row per inbox. Returns how many rows were newly created.
pub async fn enqueue_for_inboxes(
    tx: &mut sqlx::PgConnection,
    activity_id: &str,
    inboxes: &[String],
) -> Result<u32, sqlx::Error> {
    let mut created = 0u32;
    for inbox in inboxes {
        let r = sqlx::query(
            "INSERT INTO ap_delivery_queue (activity_id, inbox_url) VALUES ($1, $2) \
             ON CONFLICT (activity_id, inbox_url) DO NOTHING",
        )
        .bind(activity_id)
        .bind(inbox)
        .execute(&mut *tx)
        .await?;
        created += r.rows_affected() as u32;
    }
    Ok(created)
}

/// Resolve an activity audience (`to` + `cc` entries, plain strings or
/// embedded objects) to the unique inbox URLs to deliver to.
///
/// * `as:Public` (and empties) are skipped — never a delivery target.
/// * `{actor_id}/followers` expands to the actor's accepted followers, each
///   mapped to `COALESCE(shared_inbox, followers.inbox_url)`.
/// * Local actors are skipped (same server, no delivery).
/// * Remote actors resolve through `ap_remote_actors`
///   (`COALESCE(shared_inbox_url, inbox_url)`); unknown ones are skipped with
///   a warning — callers pre-resolve actors they intend to deliver to.
pub async fn audience_inboxes(
    pool: &PgPool,
    actor_id: &str,
    audience: &[String],
) -> Vec<String> {
    let followers_collection = format!("{actor_id}/followers");
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let push = |inbox: String, out: &mut Vec<String>, seen: &mut std::collections::HashSet<String>| {
        if !inbox.is_empty() && seen.insert(inbox.clone()) {
            out.push(inbox);
        }
    };

    for entry in audience {
        if entry.is_empty() || entry == AS_PUBLIC {
            continue;
        }
        if *entry == followers_collection
            || (entry.starts_with(&format!("{actor_id}/")) && entry.ends_with("/followers"))
        {
            // Expand the sender's followers collection. Two followers sharing
            // a sharedInbox collapse to one delivery (COALESCE + dedupe).
            let rows: Vec<(Option<String>, String)> = sqlx::query_as(
                "SELECT ra.shared_inbox_url, f.inbox_url \
                 FROM followers f \
                 LEFT JOIN ap_remote_actors ra ON ra.actor_id = f.actor_id \
                 WHERE f.user_id = (SELECT id FROM users WHERE ap_actor_id = $1) \
                   AND f.accepted = true",
            )
            .bind(actor_id)
            .fetch_all(pool)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("[AP] followers expansion failed for {actor_id}: {e}");
                Vec::new()
            });
            for (shared, inbox) in rows {
                push(shared.unwrap_or(inbox), &mut out, &mut seen);
            }
            continue;
        }

        // Local actor → same server, nothing to deliver.
        let is_local: Option<(String,)> = sqlx::query_as(
            "SELECT ap_actor_id FROM users WHERE ap_actor_id = $1 AND ap_enabled = true",
        )
        .bind(entry)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        if is_local.is_some() {
            continue;
        }

        let row: Option<(Option<String>, String)> = sqlx::query_as(
            "SELECT shared_inbox_url, inbox_url FROM ap_remote_actors WHERE actor_id = $1",
        )
        .bind(entry)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        match row {
            Some((shared, inbox)) => push(shared.unwrap_or(inbox), &mut out, &mut seen),
            None => {
                // Fetch-on-miss: activities name actors we may never have seen
                // (liking a stranger's post names its author). Resolving here
                // is what makes cross-protocol delivery actually reach the
                // remote server — before this, every Like to an uncached
                // author died at "no known inbox ...; skipping". The fetch is
                // signed with the acting actor's key (authorized-fetch
                // instances 401 unsigned GETs). Failures stay non-fatal and
                // are skipped exactly as before.
                if entry.starts_with("http") {
                    let signer = crate::ap::keys::actor_fetch_signer(pool, actor_id).await;
                    if let Some(a) = crate::ap::remote::resolve_delivery_actor(
                        pool,
                        entry,
                        signer.as_ref().map(|(k, p)| (k.as_str(), p.as_str())),
                    )
                    .await
                    {
                        push(a.delivery_inbox().to_string(), &mut out, &mut seen);
                        continue;
                    }
                }
                tracing::warn!("[AP] no known inbox for audience actor {entry}; skipping")
            }
        }
    }
    out
}