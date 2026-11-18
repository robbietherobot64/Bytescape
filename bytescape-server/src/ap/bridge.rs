//! Native→AP bridge (Phase C): federate native Bytescape writes into
//! ActivityPub activities.
//!
//! The PDS write internals call [`after_write`] / [`after_delete`] **after**
//! the AT commit. These entry points never return errors to the AT write path:
//! every failure is logged and any affected `ap_delivery_queue` row is left
//! pending with `last_error` set — a federation hiccup can never fail or roll
//! back a native write.
//!
//! Mapping (native record → AP activity):
//! * `app.bsky.feed.post`   → `Create(Note)`
//! * `com.bytescape.story`  → `Create(Story)` (media via getBlob URLs)
//! * `app.bsky.feed.like`   → `Like`
//! * `app.bsky.feed.repost` → `Announce`
//! * delete of any stored original → `Undo` of that original
//! * re-put of a stored record → `Update` (id includes the record CID, so a
//!   same-content re-put is a no-op)
//!
//! Ids are stable: `{actor}/activities/{collection}/{rkey}` for the original,
//! `…/{rkey}-undo` for its Undo, `…/{rkey}-update-{cid}` for updates. The AT
//! record uri is stored as `ap_activities.local_uri` (UNIQUE) — the mapping
//! deletes resolve the exact original through. No public activity is ever
//! produced for private content.
//!
//! Sequence per write (acceptance order): store the activity (stable id) →
//! self-index the AP object via [`crate::ap::ingest::ingest_activity`] →
//! enqueue delivery to the audience's inboxes (sharedInbox-deduped by
//! [`crate::ap::store::audience_inboxes`]).
//!
//! The module also hosts the UI-driven outbound interactions the API surface
//! needs (Like/Undo/Follow/Undo on remote AP objects): [`ui_like`],
//! [`ui_unlike`], [`ui_follow`], [`ui_unfollow`].

use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::store::{self, AS_PUBLIC};

/// JSON-LD context every delivered activity must carry — Mastodon silently
/// drops activities without it (their processor compacts against `@context`).
const AS_CONTEXT: &str = "https://www.w3.org/ns/activitystreams";

// ─── Stable id helpers ─────────────────────────────────────────────────────

fn actor_base(actor_id: &str) -> String {
    actor_id.trim_end_matches('/').to_string()
}

/// `{actor}/activities/{collection}/{rkey}` — the original write activity.
fn write_activity_id(actor_id: &str, collection: &str, rkey: &str) -> String {
    format!("{}/activities/{}/{}", actor_base(actor_id), collection, rkey)
}

/// `…/{rkey}-undo` — the Undo of a stored write original.
fn undo_activity_id(actor_id: &str, collection: &str, rkey: &str) -> String {
    format!(
        "{}/activities/{}/{}-undo",
        actor_base(actor_id),
        collection,
        rkey
    )
}

/// `…/{rkey}-update-{cid}` — an Update of a stored write original. The CID is
/// content-derived, so a same-content re-put resolves to the same id and is a
/// deduped no-op.
fn update_activity_id(actor_id: &str, collection: &str, rkey: &str, cid: &str) -> String {
    format!(
        "{}/activities/{}/{}-update-{}",
        actor_base(actor_id),
        collection,
        rkey,
        cid
    )
}

/// `{actor}/activities/{kind}-{uuid}` — UI-driven one-off activities.
fn ui_activity_id(actor_id: &str, kind: &str) -> String {
    format!(
        "{}/activities/{}-{}",
        actor_base(actor_id),
        kind,
        Uuid::new_v4().simple()
    )
}

fn object_id(actor_id: &str, collection: &str, rkey: &str) -> String {
    format!(
        "{}/objects/{}/{}",
        actor_base(actor_id),
        collection,
        rkey
    )
}

fn followers_url(actor_id: &str) -> String {
    format!("{}/followers", actor_base(actor_id))
}

fn local_uri(did: &str, collection: &str, rkey: &str) -> String {
    format!("at://{}/{}/{}", did, collection, rkey)
}

// ─── Local actor resolution ────────────────────────────────────────────────

struct LocalActor {
    #[allow(dead_code)]
    user_id: Uuid,
    username: String,
    actor_id: String,
}

impl LocalActor {
    /// `@user@host` derived from the canonical actor URL (the host is ours).
    fn handle(&self, domain: &str) -> String {
        let host = url::Url::parse(&self.actor_id)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_else(|| domain.to_string());
        format!("@{}@{}", self.username, host)
    }

    fn host(&self, domain: &str) -> String {
        url::Url::parse(&self.actor_id)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_else(|| domain.to_string())
    }
}

/// Map a native DID to its AP-enabled local actor. `None` = no bridge for this
/// write (no PDS account, no user, or no issued AP actor) — silently skipped.
async fn local_actor(db: &PgPool, did: &str) -> Option<LocalActor> {
    let (user_id,): (Uuid,) = sqlx::query_as("SELECT user_id FROM pds_accounts WHERE did = $1")
        .bind(did)
        .fetch_optional(db)
        .await
        .ok()??;
    let user = crate::db::users::UserRow::find_by_id(db, user_id).await.ok()??;
    if !user.ap_enabled {
        return None;
    }
    let actor_id = user.ap_actor_id?;
    Some(LocalActor {
        user_id,
        username: user.username,
        actor_id,
    })
}

// ─── Payload helpers ───────────────────────────────────────────────────────

/// The `to`/`cc` audience entries of an activity payload.
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

/// Never bridge private content: a record declaring non-public visibility
/// produces no public AP activity.
fn is_private(record: &Value) -> bool {
    if let Some(v) = record.get("visibility").and_then(|v| v.as_str()) {
        return v != "public";
    }
    if record.get("public") == Some(&Value::Bool(false)) {
        return true;
    }
    false
}

fn published_of(record: &Value) -> String {
    record
        .get("createdAt")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339())
}

/// Story media as getBlob URLs from `com.bytescape.story.items`.
fn story_media_urls(host: &str, did: &str, record: &Value) -> Vec<String> {
    let mut urls = Vec::new();
    for item in record.get("items").and_then(Value::as_array).into_iter().flatten() {
        let media = item.get("image").or_else(|| item.get("video"));
        let cid = media
            .and_then(|m| m.get("ref"))
            .and_then(|r| r.get("$link"))
            .and_then(|c| c.as_str());
        if let Some(cid) = cid {
            urls.push(format!(
                "https://{host}/xrpc/com.atproto.sync.getBlob?did={did}&cid={cid}"
            ));
        }
    }
    urls
}

/// The `appview_posts` author of an AP object uri (the Like/Announce `to`
/// target). Falls back to the object uri itself when unknown.
async fn object_author(db: &PgPool, object_uri: &str) -> String {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT author_did FROM appview_posts WHERE uri = $1")
            .bind(object_uri)
            .fetch_optional(db)
            .await
            .ok()
            .flatten();
    row.map(|(a,)| a).unwrap_or_else(|| object_uri.to_string())
}

/// The audience target for an outbound Like on `object_uri`: the object's
/// REAL author actor, dereferenced at like time via the object's own
/// `attributedTo` (see [`crate::ap::remote::object_author_actor`]), with the
/// `appview_posts` hint as fallback. The hint is routinely wrong for
/// federation: it carries profile-page/API URLs that do not match the actor's
/// canonical id, and the last-resort fallback (the object URI itself) resolves
/// to nothing — "no known inbox … skipping" and the Like silently never lands.
/// The dereference is signed (authorized-fetch instances 401 unsigned GETs)
/// and caches the resolved actor, so the delivery enqueue below hits the
/// cache instead of re-fetching.
async fn dereferenced_author(db: &PgPool, actor_id: &str, object_uri: &str) -> String {
    let hint = object_author(db, object_uri).await;
    let signer = crate::ap::keys::actor_fetch_signer(db, actor_id).await;
    crate::ap::remote::object_author_actor(
        db,
        object_uri,
        &hint,
        signer.as_ref().map(|(k, p)| (k.as_str(), p.as_str())),
    )
    .await
}

/// At most one Undo may ever reference `original_id` — queued, in flight, or
/// delivered. The `undone_at` mark serializes the common paths, but any
/// producer that inserts an Undo without it (remediation inserts, future
/// callers) must not be able to double-retract: a second Undo for the same
/// original is at best noise and at worst a remote-side toggle of the very
/// state being retracted.
async fn undo_already_queued(db: &PgPool, original_id: &str) -> Result<bool, String> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM ap_activities WHERE object_id = $1 AND activity_type = 'Undo' LIMIT 1",
    )
    .bind(original_id)
    .fetch_optional(db)
    .await
    .map_err(|e| format!("undo guard: {e}"))?;
    Ok(row.is_some())
}

/// Best-effort reversal of the `undone_at` CAS when the Undo row that justi-
/// fied it failed to become durable. The mark's exactly-once contract stands
/// only while a retraction actually exists; on failure we clear it so the
/// delete/unlike intent can be retried (`undo_already_queued` then also sees
/// nothing, keeping the two guards consistent).
async fn unmark_undone(db: &PgPool, original_id: &str) {
    if let Err(e) = sqlx::query("UPDATE ap_activities SET undone_at = NULL WHERE id = $1")
        .bind(original_id)
        .execute(db)
        .await
    {
        tracing::warn!("[AP bridge] unmark of {original_id} failed: {e}");
    }
}

/// Build the AP object (Note/Story) for a mapped content record. `None` for
/// unmapped/undecodable records.
fn build_object(
    actor: &LocalActor,
    domain: &str,
    did: &str,
    collection: &str,
    rkey: &str,
    record: &Value,
) -> Option<(Value, String)> {
    let obj_id = object_id(&actor.actor_id, collection, rkey);
    let published = published_of(record);
    let to = json!([AS_PUBLIC]);
    let cc = json!([followers_url(&actor.actor_id)]);
    let object = match collection {
        "app.bsky.feed.post" => json!({
            "id": obj_id,
            "type": "Note",
            "attributedTo": actor.actor_id,
            "content": record.get("text").and_then(Value::as_str).unwrap_or(""),
            "published": published,
            "to": to,
            "cc": cc,
        }),
        "com.bytescape.story" => {
            let media = story_media_urls(&actor.host(domain), did, record);
            if media.is_empty() {
                return None;
            }
            let attachment: Vec<Value> = media
                .into_iter()
                .map(|u| json!({"type": "Image", "url": u}))
                .collect();
            let mut o = json!({
                "id": obj_id,
                "type": "Story",
                "attributedTo": actor.actor_id,
                "published": published,
                "attachment": attachment,
                "to": to,
                "cc": cc,
            });
            if let Some(text) = record.get("text").and_then(Value::as_str).filter(|t| !t.trim().is_empty())
            {
                o["content"] = json!(text);
            }
            o
        }
        _ => return None,
    };
    Some((object, obj_id))
}

/// The Like/Announce target uri (`subject.uri` of the native record).
fn subject_uri(record: &Value) -> Option<String> {
    record
        .get("subject")
        .and_then(|s| s.get("uri"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

// ─── Store → self-index → enqueue ──────────────────────────────────────────

/// Phase 1+2: store the activity (stable id) and self-index its object, in one
/// transaction so the derived feed row can never drift from the activity row.
/// Returns whether the activity row was newly inserted.
async fn store_and_index(
    db: &PgPool,
    actor: &LocalActor,
    domain: &str,
    id: &str,
    activity_type: &str,
    object_id: Option<&str>,
    local_uri: Option<&str>,
    payload: &Value,
) -> Result<bool, String> {
    let mut tx = db.begin().await.map_err(|e| format!("begin: {e}"))?;
    let inserted = store::insert_activity(
        &mut *tx,
        id,
        &actor.actor_id,
        activity_type,
        object_id,
        local_uri,
        "out",
        payload,
    )
    .await
    .map_err(|e| format!("insert_activity: {e}"))?;
    if let Err(e) = crate::ap::ingest::ingest_activity(&mut *tx, payload, Some(&actor.handle(domain)))
        .await
    {
        tracing::warn!("[AP bridge] self-index of {id} failed: {e}");
    }
    tx.commit().await.map_err(|e| format!("commit: {e}"))?;
    Ok(inserted)
}

/// Phase 3: enqueue delivery of `id` to every audience inbox (sharedInbox-
/// deduped). Failures are logged and persisted to the pending rows' last_error;
/// they never propagate.
async fn enqueue_audience(db: &PgPool, actor_id: &str, id: &str, audience: &[String]) {
    let inboxes = store::audience_inboxes(db, actor_id, audience).await;
    if inboxes.is_empty() {
        return;
    }
    let mut tx = match db.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::warn!("[AP bridge] enqueue begin failed for {id}: {e}");
            return;
        }
    };
    match store::enqueue_for_inboxes(&mut *tx, id, &inboxes).await {
        Ok(n) => {
            if let Err(e) = tx.commit().await {
                tracing::warn!("[AP bridge] enqueue commit failed for {id}: {e}");
                persist_failure(db, id, &format!("enqueue commit: {e}")).await;
            } else if n > 0 {
                tracing::debug!("[AP bridge] queued {id} to {n} inbox(es)");
            }
        }
        Err(e) => {
            let _ = tx.rollback().await;
            tracing::warn!("[AP bridge] enqueue failed for {id}: {e}");
            persist_failure(db, id, &format!("enqueue: {e}")).await;
        }
    }
}

/// Persist a bridge failure on the activity's pending delivery rows so the
/// error survives the request (rows stay pending and are retried by the
/// delivery worker).
async fn persist_failure(db: &PgPool, activity_id: &str, error: &str) {
    let error = &error[..error.len().min(500)];
    let _ = sqlx::query(
        "UPDATE ap_delivery_queue SET last_error = $1, updated_at = now() \
         WHERE activity_id = $2 AND status = 'pending'",
    )
    .bind(error)
    .bind(activity_id)
    .execute(db)
    .await;
}

// ─── Write hooks (called from pds write internals, AFTER commit) ───────────

/// Bridge one native record write (createRecord/putRecord) into AP.
/// Never fails — errors are logged and persisted per above.
pub async fn after_write(
    db: &PgPool,
    domain: &str,
    did: &str,
    collection: &str,
    rkey: &str,
    record: &Value,
    record_cid: &str,
) {
    if let Err(e) = after_write_inner(db, domain, did, collection, rkey, record, record_cid).await {
        tracing::warn!("[AP bridge] after_write at://{did}/{collection}/{rkey} failed: {e}");
    }
}

async fn after_write_inner(
    db: &PgPool,
    domain: &str,
    did: &str,
    collection: &str,
    rkey: &str,
    record: &Value,
    record_cid: &str,
) -> Result<(), String> {
    if !crate::ap::ap_actors_enabled() {
        return Ok(());
    }
    // Only the mapped collections federate; everything else is AT-only.
    let mapped = matches!(
        collection,
        "app.bsky.feed.post" | "com.bytescape.story" | "app.bsky.feed.like" | "app.bsky.feed.repost"
    );
    if !mapped || is_private(record) {
        return Ok(());
    }
    let Some(actor) = local_actor(db, did).await else {
        return Ok(());
    };

    let create_id = write_activity_id(&actor.actor_id, collection, rkey);
    let lu = local_uri(did, collection, rkey);
    let stored: Option<(String,)> = sqlx::query_as("SELECT id FROM ap_activities WHERE local_uri = $1")
        .bind(&lu)
        .fetch_optional(db)
        .await
        .map_err(|e| format!("local_uri lookup: {e}"))?;

    let is_update = stored.is_some();
    let id = if is_update {
        update_activity_id(&actor.actor_id, collection, rkey, record_cid)
    } else {
        create_id.clone()
    };

    let (activity_type, object_ref, payload, audience) = match collection {
        "app.bsky.feed.post" | "com.bytescape.story" => {
            let (object, obj_id) =
                build_object(&actor, domain, did, collection, rkey, record)
                    .ok_or_else(|| format!("undecodable {collection} record"))?;
            if is_update {
                (
                    "Update".to_string(),
                    obj_id.clone(),
                    json!({
                        "@context": AS_CONTEXT,
                        "id": id,
                        "type": "Update",
                        "actor": actor.actor_id,
                        "object": object,
                        "to": [AS_PUBLIC],
                        "cc": [followers_url(&actor.actor_id)],
                    }),
                    vec![AS_PUBLIC.to_string(), followers_url(&actor.actor_id)],
                )
            } else {
                (
                    "Create".to_string(),
                    obj_id.clone(),
                    json!({
                        "@context": AS_CONTEXT,
                        "id": id,
                        "type": "Create",
                        "actor": actor.actor_id,
                        "object": object,
                        "published": published_of(record),
                        "to": [AS_PUBLIC],
                        "cc": [followers_url(&actor.actor_id)],
                    }),
                    vec![AS_PUBLIC.to_string(), followers_url(&actor.actor_id)],
                )
            }
        }
        "app.bsky.feed.like" | "app.bsky.feed.repost" => {
            let target = subject_uri(record).ok_or_else(|| "like/repost without subject.uri")?;
            let is_like = collection == "app.bsky.feed.like";
            let activity_type = if is_like { "Like" } else { "Announce" };
            // Likes target the object's REAL author (dereferenced attributedTo,
            // see `dereferenced_author`) — never the status URL, which resolves
            // to no inbox and silently drops the Like.
            let author = if is_like {
                dereferenced_author(db, &actor.actor_id, &target).await
            } else {
                object_author(db, &target).await
            };
            let (to, cc) = if is_like {
                (
                    json!([author.clone()]),
                    json!([AS_PUBLIC, followers_url(&actor.actor_id)]),
                )
            } else {
                (
                    json!([AS_PUBLIC]),
                    json!([followers_url(&actor.actor_id)]),
                )
            };
            let audience = if is_like {
                vec![
                    author,
                    AS_PUBLIC.to_string(),
                    followers_url(&actor.actor_id),
                ]
            } else {
                vec![AS_PUBLIC.to_string(), followers_url(&actor.actor_id)]
            };
            (
                activity_type.to_string(),
                target.clone(),
                json!({
                    "@context": AS_CONTEXT,
                    "id": id,
                    "type": activity_type,
                    "actor": actor.actor_id,
                    "object": target,
                    "to": to,
                    "cc": cc,
                }),
                audience,
            )
        }
        _ => return Ok(()),
    };

    let inserted = store_and_index(
        db,
        &actor,
        domain,
        &id,
        &activity_type,
        Some(&object_ref),
        if is_update { None } else { Some(&lu) },
        &payload,
    )
    .await?;
    if inserted {
        tracing::info!("[AP bridge] {activity_type} {id} stored (local_uri={lu})");
    }
    enqueue_audience(db, &actor.actor_id, &id, &audience).await;
    Ok(())
}

/// Bridge one native record delete (deleteRecord) into an `Undo` of the
/// stored original. Repeat deletes are no-ops (`undone_at` is set exactly
/// once). Never fails.
pub async fn after_delete(
    db: &PgPool,
    domain: &str,
    did: &str,
    collection: &str,
    rkey: &str,
) {
    if let Err(e) = after_delete_inner(db, domain, did, collection, rkey).await {
        tracing::warn!("[AP bridge] after_delete at://{did}/{collection}/{rkey} failed: {e}");
    }
}

async fn after_delete_inner(
    db: &PgPool,
    domain: &str,
    did: &str,
    collection: &str,
    rkey: &str,
) -> Result<(), String> {
    if !crate::ap::ap_actors_enabled() {
        return Ok(());
    }
    let lu = local_uri(did, collection, rkey);
    let stored: Option<(String, String, Value)> =
        sqlx::query_as("SELECT id, activity_type, payload FROM ap_activities WHERE local_uri = $1")
            .bind(&lu)
            .fetch_optional(db)
            .await
            .map_err(|e| format!("local_uri lookup: {e}"))?;
    let Some((original_id, _original_type, original_payload)) = stored else {
        return Ok(()); // never federated — nothing to undo
    };
    // Exactly-once per original, structurally: if any Undo already references
    // this original (queued, in flight, or delivered), never emit a second.
    if undo_already_queued(db, &original_id).await? {
        return Ok(());
    }

    // Resolve the actor BEFORE the `undone_at` mark: without it no Undo can
    // be staged, so marking here would strand the retraction permanently
    // (every retry no-ops once `undone_at` is set). Leave the original intact
    // so a retried delete (e.g. after the account mapping appears) still
    // emits the Undo.
    let Some(actor) = local_actor(db, did).await else {
        return Ok(());
    };

    // Mark the original undone exactly once: a repeat delete (or a replayed
    // Undo) finds the flag already set and emits no second Undo.
    let marked: Option<(String,)> = sqlx::query_as::<_, (String,)>(
        "UPDATE ap_activities SET undone_at = now() \
         WHERE id = $1 AND undone_at IS NULL RETURNING id",
    )
    .bind(&original_id)
    .fetch_optional(db)
    .await
    .map_err(|e| format!("undone_at mark: {e}"))?;
    if marked.is_none() {
        return Ok(());
    }

    let id = undo_activity_id(&actor.actor_id, collection, rkey);
    let audience = audience_of(&original_payload);
    // Undo embeds the FULL original activity as its object (see
    // `ui_undo_original`: a bare {id, type} stub is dropped by Mastodon).
    let mut payload = json!({
        "@context": AS_CONTEXT,
        "id": id,
        "type": "Undo",
        "actor": actor.actor_id,
    });
    for key in ["to", "cc"] {
        if let Some(v) = original_payload.get(key) {
            payload[key] = v.clone();
        }
    }
    payload["object"] = original_payload;

    if let Err(e) = store_and_index(
        db,
        &actor,
        domain,
        &id,
        "Undo",
        Some(&original_id),
        None,
        &payload,
    )
    .await
    {
        // The CAS mark promises "an Undo exists". If the Undo row never
        // became durable, undo the mark (best-effort) so a retried delete
        // re-attempts the retraction instead of no-op'ing forever.
        unmark_undone(db, &original_id).await;
        return Err(e);
    }
    tracing::info!("[AP bridge] Undo {id} stored for {lu}");
    enqueue_audience(db, &actor.actor_id, &id, &audience).await;
    Ok(())
}

// ─── UI-driven outbound interactions (API surface) ─────────────────────────

/// The signed-in user's AP actor, or an error string when they have none.
async fn actor_for_user(db: &PgPool, user_id: Uuid) -> Result<LocalActor, String> {
    if !crate::ap::ap_actors_enabled() {
        return Err("ActivityPub is disabled".into());
    }
    let user = crate::db::users::UserRow::find_by_id(db, user_id)
        .await
        .map_err(|e| format!("db: {e}"))?
        .ok_or_else(|| "no such user".to_string())?;
    if !user.ap_enabled {
        return Err("no AP actor issued for this account".into());
    }
    let actor_id = user
        .ap_actor_id
        .ok_or_else(|| "no AP actor issued for this account".to_string())?;
    Ok(LocalActor {
        user_id,
        username: user.username,
        actor_id,
    })
}

/// Most recent stored activity of `activity_type` on `object_uri` by this
/// actor that is not undone (the exact original an Undo must reference).
async fn active_original(
    db: &PgPool,
    actor_id: &str,
    object_uri: &str,
    activity_type: &str,
) -> Result<Option<(String, Value)>, String> {
    let row: Option<(String, Value)> = sqlx::query_as(
        "SELECT id, payload FROM ap_activities \
         WHERE actor_id = $1 AND object_id = $2 AND activity_type = $3 AND undone_at IS NULL \
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(actor_id)
    .bind(object_uri)
    .bind(activity_type)
    .fetch_optional(db)
    .await
    .map_err(|e| format!("original lookup: {e}"))?;
    Ok(row)
}

/// Like a remote AP object (`object_uri` is its id). Idempotent: an
/// outstanding Like is returned, never duplicated. Returns the Like activity
/// id.
pub async fn ui_like(db: &PgPool, user_id: Uuid, object_uri: &str) -> Result<String, String> {
    let actor = actor_for_user(db, user_id).await?;
    // Resolved once for both the idempotent and the fresh path: the audience
    // must name the object's real author actor, not the hint's profile URL.
    let author = dereferenced_author(db, &actor.actor_id, object_uri).await;
    if let Some((id, payload)) = active_original(db, &actor.actor_id, object_uri, "Like").await? {
        // Self-heal: pre-fix likes were stored but never queued (audience
        // resolution skipped unknown actors, silently). Re-queue the original
        // audience on the idempotent path — enqueue is ON CONFLICT DO NOTHING,
        // so this costs nothing for already-delivered likes — plus the
        // dereferenced author, which older rows predate.
        let mut audience = audience_of(&payload);
        if !audience.contains(&author) {
            audience.push(author.clone());
        }
        enqueue_audience(db, &actor.actor_id, &id, &audience).await;
        return Ok(id);
    }
    let id = ui_activity_id(&actor.actor_id, "like");
    let payload = json!({
        "@context": AS_CONTEXT,
        "id": id,
        "type": "Like",
        "actor": actor.actor_id,
        "object": object_uri,
        "to": [author.clone()],
        "cc": [AS_PUBLIC, followers_url(&actor.actor_id)],
    });
    let domain = "";
    store_and_index(db, &actor, domain, &id, "Like", Some(object_uri), None, &payload).await?;
    let audience = vec![
        author,
        AS_PUBLIC.to_string(),
        followers_url(&actor.actor_id),
    ];
    enqueue_audience(db, &actor.actor_id, &id, &audience).await;
    // Record the like locally too (owner bug: the outbound like was only
    // federated, so the local count stayed 0 until a source re-fetch). The
    // refresh-from-source then reconciles to the authoritative total.
    let _ = sqlx::query("UPDATE appview_posts SET like_count = like_count + 1 WHERE uri = $1")
        .bind(object_uri)
        .execute(db)
        .await;
    Ok(id)
}

/// Undo the outstanding Like of `object_uri`, if any. Repeatable: no active
/// Like → no-op. Returns whether an Undo was queued.
pub async fn ui_unlike(db: &PgPool, user_id: Uuid, object_uri: &str) -> Result<bool, String> {
    let undone = ui_undo_original(db, user_id, object_uri, "Like").await?;
    if undone {
        // Mirror the local bump from ui_like (clamp at zero).
        let _ = sqlx::query(
            "UPDATE appview_posts SET like_count = GREATEST(like_count - 1, 0) WHERE uri = $1",
        )
        .bind(object_uri)
        .execute(db)
        .await;
    }
    Ok(undone)
}

/// Dislike a remote AP object — the downvote arm of the group vote buttons
/// (design D2). A real AP `Dislike` activity, stored and delivered through
/// the exact same path as [`ui_like`]: an audience of the object's real
/// author (dereferenced), `as:Public`, and the actor's followers collection.
///
/// Idempotent like `ui_like`: an outstanding Dislike is returned, never
/// duplicated. Mixing a Dislike with a standing Like is the caller's concern
/// (the UI flips one way or the other and calls [`ui_unlike`]/[`ui_undislike`]
/// first) — this function only ever keeps ONE standing Dislike per
/// (actor, object).
///
/// Returns the Dislike activity id.
pub async fn ui_dislike(db: &PgPool, user_id: Uuid, object_uri: &str) -> Result<String, String> {
    let actor = actor_for_user(db, user_id).await?;
    let author = dereferenced_author(db, &actor.actor_id, object_uri).await;
    if let Some((id, payload)) = active_original(db, &actor.actor_id, object_uri, "Dislike").await? {
        // Self-heal path, mirroring ui_like: re-queue the audience (enqueue
        // is ON CONFLICT DO NOTHING) plus the dereferenced author.
        let mut audience = audience_of(&payload);
        if !audience.contains(&author) {
            audience.push(author.clone());
        }
        enqueue_audience(db, &actor.actor_id, &id, &audience).await;
        return Ok(id);
    }
    let id = ui_activity_id(&actor.actor_id, "dislike");
    let payload = json!({
        "@context": AS_CONTEXT,
        "id": id,
        "type": "Dislike",
        "actor": actor.actor_id,
        "object": object_uri,
        "to": [author.clone()],
        "cc": [AS_PUBLIC, followers_url(&actor.actor_id)],
    });
    let domain = "";
    store_and_index(db, &actor, domain, &id, "Dislike", Some(object_uri), None, &payload).await?;
    let audience = vec![
        author,
        AS_PUBLIC.to_string(),
        followers_url(&actor.actor_id),
    ];
    enqueue_audience(db, &actor.actor_id, &id, &audience).await;
    Ok(id)
}

/// Undo the outstanding Dislike of `object_uri`, if any. Repeatable: no
/// active Dislike → no-op. Returns whether an Undo was queued.
pub async fn ui_undislike(db: &PgPool, user_id: Uuid, object_uri: &str) -> Result<bool, String> {
    ui_undo_original(db, user_id, object_uri, "Dislike").await
}

/// The user's standing vote on an AP object, read back from `ap_activities`:
/// the most recent non-undone `Like`/`Dislike` by their actor on this object.
/// `None` = no standing vote. Used to light the group post card's up/down
/// buttons honestly (design D2 — never fake a federated vote).
pub async fn ui_vote_state(
    db: &PgPool,
    user_id: Uuid,
    object_uri: &str,
) -> Result<Option<&'static str>, String> {
    let actor = actor_for_user(db, user_id).await?;
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT activity_type FROM ap_activities \
         WHERE actor_id = $1 AND object_id = $2 \
           AND activity_type IN ('Like', 'Dislike') AND undone_at IS NULL \
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(&actor.actor_id)
    .bind(object_uri)
    .fetch_optional(db)
    .await
    .map_err(|e| format!("vote state lookup: {e}"))?;
    Ok(row.and_then(|(t,)| match t.as_str() {
        "Like" => Some("up"),
        "Dislike" => Some("down"),
        _ => None,
    }))
}

/// Comment on a remote AP object — the reply arm of the group comment
/// composer. `object_uri` is the post's id (a Lemmy/PieFed post URL);
/// `parent_comment` names the comment being replied to in nested threads.
///
/// Mints a `Create(Note)` with `inReplyTo` and delivers it to the post's real
/// author (dereferenced) AND its community — remote thread implementations
/// (Lemmy/PieFed) only attach a reply to the thread when the community is in
/// the audience. `store_and_index` self-indexes the note (its `inReplyTo`
/// becomes `reply_parent`), so the thread shows the reply immediately.
///
/// Returns the Create activity id.
pub async fn ui_comment(
    db: &PgPool,
    user_id: Uuid,
    object_uri: &str,
    parent_comment: Option<&str>,
    content: &str,
) -> Result<String, String> {
    let actor = actor_for_user(db, user_id).await?;
    // The audience must name the object's real author actor, not the hint's
    // profile URL (same contract as ui_like).
    let author = dereferenced_author(db, &actor.actor_id, object_uri).await;
    // The community (group) the post belongs to — without it in `to`, Lemmy
    // and PieFed never thread the reply.
    let community: Option<String> = sqlx::query_scalar(
        "SELECT group_actor_id FROM appview_posts WHERE uri = $1 AND group_actor_id IS NOT NULL",
    )
    .bind(object_uri)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    let rkey = Uuid::new_v4().simple().to_string();
    let note_id = object_id(&actor.actor_id, "comments", &rkey);
    let activity_id = ui_activity_id(&actor.actor_id, "comment");
    let in_reply_to = parent_comment.unwrap_or(object_uri);
    let mut to = vec![author.clone()];
    if let Some(c) = community.as_deref() {
        to.push(c.to_string());
    }
    let cc = vec![AS_PUBLIC.to_string(), followers_url(&actor.actor_id)];
    let payload = json!({
        "@context": AS_CONTEXT,
        "id": activity_id,
        "type": "Create",
        "actor": actor.actor_id,
        "to": to,
        "cc": cc,
        "object": {
            "id": note_id,
            "type": "Note",
            "attributedTo": actor.actor_id,
            "content": content,
            "inReplyTo": in_reply_to,
            "published": chrono::Utc::now().to_rfc3339(),
            "to": to,
            "cc": cc,
        },
    });
    let domain = "";
    store_and_index(
        db,
        &actor,
        domain,
        &activity_id,
        "Create",
        Some(&note_id),
        None,
        &payload,
    )
    .await?;
    let mut audience = vec![
        author,
        AS_PUBLIC.to_string(),
        followers_url(&actor.actor_id),
    ];
    if let Some(c) = community {
        audience.push(c);
    }
    enqueue_audience(db, &actor.actor_id, &activity_id, &audience).await;
    // Mirror the local bump from ui_like: the reply is real the moment it is
    // queued — the refresh-from-source then reconciles to the authoritative
    // total.
    let _ = sqlx::query(
        "UPDATE appview_posts SET comment_count = comment_count + 1 WHERE uri = $1",
    )
    .bind(object_uri)
    .execute(db)
    .await;
    Ok(activity_id)
}

/// Follow a remote actor by `acct` (`user@host`): resolve via WebFinger/actor
/// fetch (SSRF+timeout bounded in [`crate::ap::remote`]), store a pending
/// `follows` row, and queue the Follow. The remote's Accept (inbox) activates
/// the row. Returns the resolved actor id.
pub async fn ui_follow(
    db: &PgPool,
    jwt_secret: &str,
    user_id: Uuid,
    acct: &str,
) -> Result<String, String> {
    let actor = actor_for_user(db, user_id).await?;
    let signer = crate::ap::keys::load_fetch_signer(db, user_id, jwt_secret).await;
    let remote = crate::ap::remote::resolve_handle(
        db,
        signer.as_ref().map(|(k, p)| (k.as_str(), p.as_str())),
        acct,
    )
    .await?;
    // The guarded fetch is the verification: cache the resolved actor.
    crate::ap::remote::upsert_actor(db, &remote)
        .await
        .map_err(|e| format!("upsert_actor: {e}"))?;

    // Pending follow row (Accept from the inbox activates it).
    let existing = crate::db::follows::FollowRow::find_by_follower_and_actor(
        db,
        user_id,
        &remote.actor_id,
    )
    .await
    .map_err(|e| format!("follows lookup: {e}"))?;
    if existing.is_none() {
        crate::db::follows::FollowRow::create(db, user_id, &remote.actor_id, "ap", None)
            .await
            .map_err(|e| format!("follows create: {e}"))?;
    }

    if active_original(db, &actor.actor_id, &remote.actor_id, "Follow")
        .await?
        .is_some()
    {
        return Ok(remote.actor_id);
    }
    let id = ui_activity_id(&actor.actor_id, "follow");
    let payload = json!({
        "@context": AS_CONTEXT,
        "id": id,
        "type": "Follow",
        "actor": actor.actor_id,
        "object": remote.actor_id,
        "to": [remote.actor_id],
    });
    let domain = "";
    store_and_index(db, &actor, domain, &id, "Follow", Some(&remote.actor_id), None, &payload)
        .await?;
    // Pass the remote's ACTOR ID, not its inbox URL: store::audience_inboxes
    // resolves audience entries as actor ids (followers-collection, local-skip,
    // then ap_remote_actors WHERE actor_id = entry → resolve_delivery_actor).
    // An inbox URL misses every branch and the enqueue silently produced NO
    // delivery row, so minted-actor UI follows were a silent black hole (the
    // Follow stayed `pending` and the remote never received it). The cached
    // ap_remote_actors row (already resolved in `remote`) resolves this actor id
    // to its shared-or-inbox endpoint.
    enqueue_audience(db, &actor.actor_id, &id, &[remote.actor_id.clone()]).await;
    Ok(remote.actor_id)
}

/// Unfollow a remote actor: queue an `Undo` of the exact stored Follow and
/// drop the `follows` row. No stored Follow → stale row cleanup only.
/// Returns whether an Undo was queued.
pub async fn ui_unfollow(db: &PgPool, user_id: Uuid, target_actor: &str) -> Result<bool, String> {
    let undone = ui_undo_original(db, user_id, target_actor, "Follow").await?;
    crate::db::follows::FollowRow::delete_by_actor(db, user_id, target_actor)
        .await
        .map_err(|e| format!("follows delete: {e}"))?;
    Ok(undone)
}

/// Shared Undo flow for UI activities: find the actor's outstanding original
/// on `object_uri`, mark it undone exactly once, store an `Undo` copying the
/// original's to/cc, and queue it.
async fn ui_undo_original(
    db: &PgPool,
    user_id: Uuid,
    object_uri: &str,
    activity_type: &str,
) -> Result<bool, String> {
    let actor = actor_for_user(db, user_id).await?;
    let Some((original_id, original_payload)) =
        active_original(db, &actor.actor_id, object_uri, activity_type).await?
    else {
        return Ok(false);
    };
    // Exactly-once per original, structurally: an Undo already queued or in
    // flight for this original means the retraction exists — a repeat unlike
    // must never queue a second one (double-Undo can toggle remote state).
    if undo_already_queued(db, &original_id).await? {
        return Ok(false);
    }
    let marked: Option<(String,)> = sqlx::query_as::<_, (String,)>(
        "UPDATE ap_activities SET undone_at = now() \
         WHERE id = $1 AND undone_at IS NULL RETURNING id",
    )
    .bind(&original_id)
    .fetch_optional(db)
    .await
    .map_err(|e| format!("undone_at mark: {e}"))?;
    if marked.is_none() {
        return Ok(false);
    }

    let id = ui_activity_id(&actor.actor_id, "undo");
    let audience = audience_of(&original_payload);
    // The Undo embeds the FULL original activity as its object: Mastodon's
    // Undo handler dispatches on `object.type` and resolves the target via
    // `value_or_id(object.object)` — a bare {id, type} stub has no nested
    // `object`, so the Undo is silently dropped (the remote favourite stays).
    let mut payload = json!({
        "@context": AS_CONTEXT,
        "id": id,
        "type": "Undo",
        "actor": actor.actor_id,
    });
    for key in ["to", "cc"] {
        if let Some(v) = original_payload.get(key) {
            payload[key] = v.clone();
        }
    }
    payload["object"] = original_payload;
    let domain = "";
    if let Err(e) =
        store_and_index(db, &actor, domain, &id, "Undo", Some(&original_id), None, &payload).await
    {
        // Same contract as `after_delete_inner`: the mark promised a durable
        // Undo — un-mark (best-effort) and propagate so the caller can retry.
        unmark_undone(db, &original_id).await;
        return Err(e);
    }
    enqueue_audience(db, &actor.actor_id, &id, &audience).await;
    Ok(true)
}
// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ap::remote::RemoteActor;
    use crate::ap::testutil::{connect_pool, enable_ap_flags, make_local_actor, unique};

    /// A local user + issued AP actor + a `pds_accounts` row so
    /// `after_write`/`after_delete` can map the DID to the account.
    async fn fixture(pool: &PgPool, suffix: &str) -> (crate::ap::testutil::TestLocalActor, String) {
        let la = make_local_actor(pool, "bytescape.test").await;
        let did = format!("did:web:bytescape.test:aptest_{suffix}");
        sqlx::query(
            "INSERT INTO pds_accounts (did, handle, signing_key_private, signing_key_public, status, user_id) \
             VALUES ($1, $2, $3, 'test-pub', 'active', $4)",
        )
        .bind(&did)
        .bind(format!("aptest_{suffix}@bytescape.test"))
        .bind(&b"\x00"[..])
        .bind(la.user.id)
        .execute(pool)
        .await
        .expect("seed pds account");
        (la, did)
    }

    /// Seed one accepted follower + its cached remote actor (the cached
    /// sharedInbox drives delivery dedupe).
    async fn add_follower(
        pool: &PgPool,
        user_id: Uuid,
        suffix: &str,
        name: &str,
        inbox: &str,
        shared: Option<&str>,
    ) -> String {
        let actor = format!("https://{name}.test/users/{name}_{suffix}");
        sqlx::query(
            "INSERT INTO followers (actor_id, inbox_url, user_id, accepted) VALUES ($1, $2, $3, true)",
        )
        .bind(&actor)
        .bind(inbox)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed follower");
        crate::ap::remote::upsert_actor(
            pool,
            &RemoteActor {
                actor_id: actor.clone(),
                inbox_url: inbox.to_string(),
                shared_inbox_url: shared.map(str::to_string),
                public_key_pem: None,
                preferred_username: None,
                display_name: None,
                avatar_url: None,
            },
        )
        .await
        .expect("seed remote actor");
        actor
    }

    async fn cleanup(pool: &PgPool, user_id: Uuid, actor_id: &str, did: &str, suffix: &str) {
        let _ = sqlx::query("DELETE FROM ap_delivery_queue WHERE activity_id LIKE $1")
            .bind(format!("{actor_id}/activities/%"))
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id = $1")
            .bind(actor_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM appview_posts WHERE uri LIKE $1")
            .bind(format!("{actor_id}/objects/%"))
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM followers WHERE user_id = $1").bind(user_id).execute(pool).await;
        let _ = sqlx::query("DELETE FROM ap_remote_actors WHERE actor_id LIKE $1")
            .bind(format!("%_{suffix}"))
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM pds_accounts WHERE did = $1").bind(did).execute(pool).await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1").bind(user_id).execute(pool).await;
    }

    async fn count_activities(pool: &PgPool, actor_id: &str, activity_type: &str) -> i64 {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ap_activities WHERE actor_id = $1 AND activity_type = $2",
        )
        .bind(actor_id)
        .bind(activity_type)
        .fetch_one(pool)
        .await
        .expect("count activities");
        n
    }

    /// create → one Create row with the stable id + local_uri mapping, one
    /// self-indexed `appview_posts` row, and one queue row per UNIQUE follower
    /// inbox (sharedInbox dedupes). Re-writing is a no-op.
    #[tokio::test]
    async fn bridge_create_stores_self_indexes_and_dedupes_shared_inbox() {
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let suffix = unique();
        let (la, did) = fixture(&pool, &suffix).await;

        // Two followers share one sharedInbox; the third has its own.
        add_follower(&pool, la.user.id, &suffix, "alice", "https://a.test/inbox", Some("https://shared.test/inbox")).await;
        add_follower(&pool, la.user.id, &suffix, "bob", "https://b.test/inbox", Some("https://shared.test/inbox")).await;
        add_follower(&pool, la.user.id, &suffix, "carol", "https://c.test/inbox", None).await;

        let rkey = format!("rk{suffix}");
        let record = serde_json::json!({
            "$type": "app.bsky.feed.post",
            "text": "hello federation",
            "createdAt": "2026-11-12T10:00:00Z",
        });
        after_write(&pool, "bytescape.test", &did, "app.bsky.feed.post", &rkey, &record, "cid1").await;

        let expected_id = write_activity_id(&la.actor_id, "app.bsky.feed.post", &rkey);
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT id, activity_type, direction, local_uri FROM ap_activities WHERE id = $1",
        )
        .bind(&expected_id)
        .fetch_all(&pool)
        .await
        .expect("activity row");
        assert_eq!(rows.len(), 1, "Create stored once under the stable id");
        let (id, ty, direction, lu) = &rows[0];
        assert_eq!(id, &expected_id);
        assert_eq!(ty, "Create");
        assert_eq!(direction, "out");
        assert_eq!(lu, &format!("at://{did}/app.bsky.feed.post/{rkey}"), "local_uri maps the AT record");

        // Self-index: the AP object landed in appview_posts via ingest_activity.
        let obj_uri = object_id(&la.actor_id, "app.bsky.feed.post", &rkey);
        let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM appview_posts WHERE uri = $1")
            .bind(&obj_uri)
            .fetch_one(&pool)
            .await
            .expect("appview row");
        assert_eq!(n, 1, "AP object self-indexed exactly once");

        // Delivery: unique inboxes only — a.test + b.test share one sharedInbox.
        let mut inboxes: Vec<String> = sqlx::query_as::<_, (String,)>(
            "SELECT inbox_url FROM ap_delivery_queue WHERE activity_id = $1 ORDER BY inbox_url",
        )
        .bind(&expected_id)
        .fetch_all(&pool)
        .await
        .expect("queue rows")
        .into_iter()
        .map(|(u,)| u)
        .collect();
        inboxes.sort();
        assert_eq!(
            inboxes,
            vec!["https://c.test/inbox".to_string(), "https://shared.test/inbox".to_string()],
            "one queue row per unique inbox, sharedInbox deduped"
        );

        // Replay: same stable id → no second activity, no second queue rows.
        after_write(&pool, "bytescape.test", &did, "app.bsky.feed.post", &rkey, &record, "cid1").await;
        assert_eq!(count_activities(&pool, &la.actor_id, "Create").await, 1);
        let (q,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ap_delivery_queue WHERE activity_id = $1",
        )
        .bind(&expected_id)
        .fetch_one(&pool)
        .await
        .expect("queue count");
        assert_eq!(q, 2, "queue rows deduped on replay");

        cleanup(&pool, la.user.id, &la.actor_id, &did, &suffix).await;
    }

    /// like → Like row + queued delivery; delete → stable-id Undo, original
    /// `undone_at` set, and a repeat delete emits no second Undo.
    #[tokio::test]
    async fn bridge_like_then_delete_emits_exactly_one_undo() {
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let suffix = unique();
        let (la, did) = fixture(&pool, &suffix).await;

        // The liked object is a remote AP post whose author is cached.
        let target = format!("https://z.test/users/z/statuses/{suffix}");
        sqlx::query(
            "INSERT INTO appview_posts (uri, cid, author_did, author_handle, text, created_at) \
             VALUES ($1, $1, $2, $3, 'remote post', now())",
        )
        .bind(&target)
        .bind(format!("https://z.test/users/z_{suffix}"))
        .bind(format!("z_{suffix}@z.test"))
        .execute(&pool)
        .await
        .expect("seed target post");
        let author = add_follower(&pool, la.user.id, &suffix, "z", "https://z.test/inbox", None).await;
        // (add_follower's actor is z_{suffix}; the post author must match.)
        sqlx::query("UPDATE appview_posts SET author_did = $1 WHERE uri = $2")
            .bind(&author)
            .bind(&target)
            .execute(&pool)
            .await
            .expect("align author");

        let rkey = format!("lk{suffix}");
        let record = serde_json::json!({
            "$type": "app.bsky.feed.like",
            "subject": {"uri": target, "cid": "bafk-target"},
            "createdAt": "2026-11-12T11:00:00Z",
        });
        after_write(&pool, "bytescape.test", &did, "app.bsky.feed.like", &rkey, &record, "cid2").await;

        let like_id = write_activity_id(&la.actor_id, "app.bsky.feed.like", &rkey);
        let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM ap_activities WHERE id = $1")
            .bind(&like_id)
            .fetch_one(&pool)
            .await
            .expect("like row");
        assert_eq!(n, 1, "Like stored under the stable id");
        let (q,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ap_delivery_queue WHERE activity_id = $1",
        )
        .bind(&like_id)
        .fetch_one(&pool)
        .await
        .expect("like queue");
        assert_eq!(q, 1, "Like queued to the object author's inbox");

        // Delete → Undo of the stored original.
        after_delete(&pool, "bytescape.test", &did, "app.bsky.feed.like", &rkey).await;
        let undo_id = undo_activity_id(&la.actor_id, "app.bsky.feed.like", &rkey);
        let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM ap_activities WHERE id = $1")
            .bind(&undo_id)
            .fetch_one(&pool)
            .await
            .expect("undo row");
        assert_eq!(n, 1, "Undo stored under the stable -undo id");
        // The Undo embeds the FULL original Like as its object (a bare
        // {id, type} stub is silently dropped by Mastodon).
        let (undo_payload,): (Value,) = sqlx::query_as(
            "SELECT payload FROM ap_activities WHERE id = $1",
        )
        .bind(&undo_id)
        .fetch_one(&pool)
        .await
        .expect("undo payload");
        let embedded = &undo_payload["object"];
        assert_eq!(embedded["type"], "Like", "Undo.object carries the original activity type");
        assert_eq!(embedded["id"], like_id, "Undo.object is the exact original Like");
        assert_eq!(embedded["object"], target, "Undo.object embeds the Like target so remotes can resolve it");
        let (undone_at,): (Option<chrono::DateTime<chrono::Utc>>,) = sqlx::query_as(
            "SELECT undone_at FROM ap_activities WHERE id = $1",
        )
        .bind(&like_id)
        .fetch_one(&pool)
        .await
        .expect("original row");
        assert!(undone_at.is_some(), "original marked undone exactly once");

        // Repeat delete: no second Undo.
        after_delete(&pool, "bytescape.test", &did, "app.bsky.feed.like", &rkey).await;
        assert_eq!(count_activities(&pool, &la.actor_id, "Undo").await, 1, "no second Undo on repeat delete");

        let _ = sqlx::query("DELETE FROM appview_posts WHERE uri = $1")
            .bind(&target)
            .execute(&pool)
            .await;
        cleanup(&pool, la.user.id, &la.actor_id, &did, &suffix).await;
    }

    /// put twice → one Create + one Update; a same-content re-put resolves to
    /// the same Update id and is a no-op.
    #[tokio::test]
    async fn bridge_put_twice_create_then_update_same_content_noop() {
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let suffix = unique();
        let (la, did) = fixture(&pool, &suffix).await;

        let rkey = format!("pt{suffix}");
        let record = serde_json::json!({
            "$type": "app.bsky.feed.post",
            "text": "edited once",
            "createdAt": "2026-11-12T12:00:00Z",
        });
        // First write (create).
        after_write(&pool, "bytescape.test", &did, "app.bsky.feed.post", &rkey, &record, "cid3").await;
        // Second write (put, same content + cid).
        after_write(&pool, "bytescape.test", &did, "app.bsky.feed.post", &rkey, &record, "cid3").await;
        // Third write: same-content re-put is a no-op.
        after_write(&pool, "bytescape.test", &did, "app.bsky.feed.post", &rkey, &record, "cid3").await;

        assert_eq!(count_activities(&pool, &la.actor_id, "Create").await, 1);
        assert_eq!(count_activities(&pool, &la.actor_id, "Update").await, 1);
        let expected_update = update_activity_id(&la.actor_id, "app.bsky.feed.post", &rkey, "cid3");
        let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM ap_activities WHERE id = $1")
            .bind(&expected_update)
            .fetch_one(&pool)
            .await
            .expect("update row");
        assert_eq!(n, 1, "one Update row with the cid-stable id");
        // The local_uri mapping still points at the Create original.
        let (mapped,): (String,) =
            sqlx::query_as("SELECT id FROM ap_activities WHERE local_uri = $1")
                .bind(format!("at://{did}/app.bsky.feed.post/{rkey}"))
                .fetch_one(&pool)
                .await
                .expect("local_uri mapping");
        assert_eq!(mapped, write_activity_id(&la.actor_id, "app.bsky.feed.post", &rkey));

        cleanup(&pool, la.user.id, &la.actor_id, &did, &suffix).await;
    }

    /// Private content never produces a public AP activity.
    #[tokio::test]
    async fn bridge_skips_private_content() {
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let suffix = unique();
        let (la, did) = fixture(&pool, &suffix).await;

        let rkey = format!("pv{suffix}");
        let record = serde_json::json!({
            "$type": "app.bsky.feed.post",
            "text": "friends only",
            "visibility": "followers",
            "createdAt": "2026-11-12T13:00:00Z",
        });
        after_write(&pool, "bytescape.test", &did, "app.bsky.feed.post", &rkey, &record, "cid4").await;
        assert_eq!(count_activities(&pool, &la.actor_id, "Create").await, 0, "private record stays unbridged");

        cleanup(&pool, la.user.id, &la.actor_id, &did, &suffix).await;
    }

    /// UI reactions on an AP object: idempotent Like, then an Undo of the
    /// exact original; a second unlike is a no-op.
    #[tokio::test]
    async fn ui_like_and_unlike_round_trip() {
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let suffix = unique();
        let la = make_local_actor(&pool, "bytescape.test").await;

        let target = format!("https://z.test/users/z/statuses/ui{suffix}");
        let author = add_follower(&pool, la.user.id, &suffix, "z", "https://z.test/inbox", None).await;
        sqlx::query(
            "INSERT INTO appview_posts (uri, cid, author_did, author_handle, text, created_at) \
             VALUES ($1, $1, $2, $3, 'ap post', now())",
        )
        .bind(&target)
        .bind(&author)
        .bind(format!("z_{suffix}@z.test"))
        .execute(&pool)
        .await
        .expect("seed target post");

        let like1 = ui_like(&pool, la.user.id, &target).await.expect("ui_like");
        let like2 = ui_like(&pool, la.user.id, &target).await.expect("ui_like repeat");
        assert_eq!(like1, like2, "outstanding Like is returned, never duplicated");
        assert_eq!(count_activities(&pool, &la.actor_id, "Like").await, 1);
        let (q,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ap_delivery_queue WHERE activity_id = $1",
        )
        .bind(&like1)
        .fetch_one(&pool)
        .await
        .expect("like queue");
        assert_eq!(q, 1, "Like queued to the author's inbox");

        let undone = ui_unlike(&pool, la.user.id, &target).await.expect("ui_unlike");
        assert!(undone, "first unlike queues the Undo");
        assert_eq!(count_activities(&pool, &la.actor_id, "Undo").await, 1);

        // Regression (unlike not removing remote favourite): the Undo must
        // embed the FULL original Like as its object — Mastodon resolves the
        // target via value_or_id(object.object) and drops a bare {id,type}
        // stub with a silent no-op.
        let (undo_payload,): (Value,) = sqlx::query_as(
            "SELECT payload FROM ap_activities WHERE actor_id = $1 AND activity_type = 'Undo'",
        )
        .bind(&la.actor_id)
        .fetch_one(&pool)
        .await
        .expect("undo payload");
        let embedded = &undo_payload["object"];
        assert_eq!(embedded["type"], "Like", "Undo.object carries the original activity type");
        assert_eq!(embedded["id"], like1, "Undo.object is the exact original Like");
        assert_eq!(embedded["object"], target, "Undo.object embeds the Like target so remotes can resolve it");
        assert_eq!(embedded["actor"], la.actor_id, "Undo.object embeds the original actor");

        let again = ui_unlike(&pool, la.user.id, &target).await.expect("ui_unlike repeat");
        assert!(!again, "repeat unlike is a no-op");
        assert_eq!(count_activities(&pool, &la.actor_id, "Undo").await, 1);

        let _ = sqlx::query("DELETE FROM appview_posts WHERE uri = $1")
            .bind(&target)
            .execute(&pool)
            .await;
        cleanup(&pool, la.user.id, &la.actor_id, "did:web:none", &suffix).await;
    }

    /// Exactly-once Undo, structurally: when an Undo already references the
    /// original — even one inserted OUTSIDE the `undone_at` mark (remediation
    /// inserts, future producers) — no path may queue a second. A double Undo
    /// is at best noise and at worst toggles remote state back on.
    #[tokio::test]
    async fn ui_unlike_never_queues_a_second_undo_for_one_original() {
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let suffix = unique();
        let la = make_local_actor(&pool, "bytescape.test").await;

        let target = format!("https://z.test/users/z/statuses/guard{suffix}");
        let author = add_follower(&pool, la.user.id, &suffix, "z", "https://z.test/inbox", None).await;
        sqlx::query(
            "INSERT INTO appview_posts (uri, cid, author_did, author_handle, text, created_at) \
             VALUES ($1, $1, $2, $3, 'ap post', now())",
        )
        .bind(&target)
        .bind(&author)
        .bind(format!("z_{suffix}@z.test"))
        .execute(&pool)
        .await
        .expect("seed target post");

        let like_id = ui_like(&pool, la.user.id, &target).await.expect("ui_like");

        // Remediation-shaped Undo: inserted without marking `undone_at`
        // (exactly what the corrected-Undo tooling does). The guard must see
        // it and refuse every further retraction attempt.
        let pre_undo = format!("{}/activities/undo-remed{suffix}", la.actor_id);
        sqlx::query(
            "INSERT INTO ap_activities (id, actor_id, activity_type, object_id, direction, payload) \
             VALUES ($1, $2, 'Undo', $3, 'out', $4)",
        )
        .bind(&pre_undo)
        .bind(&la.actor_id)
        .bind(&like_id)
        .bind(serde_json::json!({"id": pre_undo, "type": "Undo", "object": {"id": like_id, "type": "Like"}}))
        .execute(&pool)
        .await
        .expect("seed pre-existing undo");

        let queued = ui_unlike(&pool, la.user.id, &target).await.expect("ui_unlike");
        assert!(!queued, "an existing Undo makes every repeat unlike a no-op");
        assert_eq!(count_activities(&pool, &la.actor_id, "Undo").await, 1, "never a second Undo");
        // Native delete path applies the same structural guard.
        let did = format!("did:web:guard{suffix}");
        after_delete(&pool, "bytescape.test", &did, "app.bsky.feed.like", &suffix).await;
        assert_eq!(count_activities(&pool, &la.actor_id, "Undo").await, 1, "delete path guarded too");

        let _ = sqlx::query("DELETE FROM appview_posts WHERE uri = $1")
            .bind(&target)
            .execute(&pool)
            .await;
        cleanup(&pool, la.user.id, &la.actor_id, "did:web:none", &suffix).await;
    }

    /// CR-6: `after_delete` must never mark the original undone unless the
    /// Undo row becomes durable. With no `pds_accounts` mapping for the DID
    /// (actor unresolvable) the original is left untouched — and once the
    /// mapping appears, a retried delete emits the Undo instead of no-op'ing
    /// forever (the pre-fix code marked `undone_at` BEFORE resolving the actor
    /// and inserting, stranding the retraction permanently).
    #[tokio::test]
    async fn after_delete_without_actor_mapping_retries_until_the_undo_lands() {
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let suffix = unique();
        let la = make_local_actor(&pool, "bytescape.test").await;
        let did = format!("did:web:bytescape.test:nomap_{suffix}");
        let collection = "app.bsky.feed.like";
        let rkey = format!("lk{suffix}");

        // Stored federated original (the shape `after_write` leaves) whose DID
        // maps to NO pds_accounts row yet.
        let like_id = write_activity_id(&la.actor_id, collection, &rkey);
        let lu = local_uri(&did, collection, &rkey);
        let payload = serde_json::json!({
            "@context": AS_CONTEXT,
            "id": like_id,
            "type": "Like",
            "actor": la.actor_id,
            "object": format!("https://remote.test/users/z/statuses/{suffix}"),
            "to": ["https://remote.test/users/z"],
        });
        sqlx::query(
            "INSERT INTO ap_activities \
               (id, actor_id, activity_type, object_id, local_uri, direction, payload, user_id) \
             VALUES ($1, $2, 'Like', $3, $4, 'out', $5, $6)",
        )
        .bind(&like_id)
        .bind(&la.actor_id)
        .bind(format!("https://remote.test/users/z/statuses/{suffix}"))
        .bind(&lu)
        .bind(&payload)
        .bind(la.user.id)
        .execute(&pool)
        .await
        .expect("seed stored original");

        // (a) Unresolvable actor: nothing marked, nothing staged.
        after_delete(&pool, "bytescape.test", &did, collection, &rkey).await;
        let (undone_at,): (Option<chrono::DateTime<chrono::Utc>>,) = sqlx::query_as(
            "SELECT undone_at FROM ap_activities WHERE id = $1",
        )
        .bind(&like_id)
        .fetch_one(&pool)
        .await
        .expect("original row");
        assert!(undone_at.is_none(), "(a) unresolvable actor must leave the original unmarked");
        assert_eq!(
            count_activities(&pool, &la.actor_id, "Undo").await,
            0,
            "(a) no Undo can be staged without an actor"
        );

        // (b) The account mapping appears — the SAME delete intent retried
        // must now land the Undo (pre-fix the mark made every retry a no-op).
        sqlx::query(
            "INSERT INTO pds_accounts (did, handle, signing_key_private, signing_key_public, status, user_id) \
             VALUES ($1, $2, $3, 'test-pub', 'active', $4)",
        )
        .bind(&did)
        .bind(format!("nomap_{suffix}@bytescape.test"))
        .bind(&b"\x00"[..])
        .bind(la.user.id)
        .execute(&pool)
        .await
        .expect("seed pds account");
        after_delete(&pool, "bytescape.test", &did, collection, &rkey).await;

        let undo_id = undo_activity_id(&la.actor_id, collection, &rkey);
        assert_eq!(
            count_activities(&pool, &la.actor_id, "Undo").await,
            1,
            "(b) retried delete emits the Undo once the actor resolves"
        );
        let (undo_object,): (String,) = sqlx::query_as(
            "SELECT object_id FROM ap_activities WHERE id = $1",
        )
        .bind(&undo_id)
        .fetch_one(&pool)
        .await
        .expect("undo row");
        assert_eq!(undo_object, like_id, "(b) the Undo references the original");
        let (undone_at,): (Option<chrono::DateTime<chrono::Utc>>,) = sqlx::query_as(
            "SELECT undone_at FROM ap_activities WHERE id = $1",
        )
        .bind(&like_id)
        .fetch_one(&pool)
        .await
        .expect("original row");
        assert!(undone_at.is_some(), "(b) marked only alongside the durable Undo");

        cleanup(&pool, la.user.id, &la.actor_id, &did, &suffix).await;
    }
}
