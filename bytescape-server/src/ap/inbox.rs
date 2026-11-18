//! ActivityPub inbox (Phase C): verified receive + idempotent dispatch.
//!
//! Order of operations is the security boundary:
//! 1. HTTP signature over the EXACT raw bytes (Digest/Date/host/
//!    `(request-target)` all mandatory signed) — before any JSON parsing or
//!    write ([`crate::ap::http_sig`]).
//! 2. Signer key resolution (local actor keys / `ap_remote_actors` cache,
//!    SSRF-guarded fetch on miss) + RSA verify.
//! 3. Signer-owns-actor check: the activity's `actor` (and any embedded
//!    object's `attributedTo`) must be the verified signer.
//! 4. ONE transaction: persist + dedupe by activity id, then effects —
//!    replays return success without duplicate effects. Like/Announce count
//!    changes commit atomically with their `ap_activities` interaction rows;
//!    Undo retires the signer-owned original exactly once (repeatable,
//!    clamped).

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::ap::{http_sig, ingest, keys, remote, store};
use crate::db::users::UserRow;
use crate::error::AppError;
use crate::AppState;

/// Max accepted inbox body size (also enforced by the route layer).
pub const MAX_BODY_BYTES: usize = 256 * 1024;

/// POST /@{username}/inbox — the actor's inbox.
pub async fn inbox_handler(
    State(state): State<AppState>,
    Path(username): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    process_inbox(state, Some(&username), method, uri, headers, body).await
}

/// POST /inbox — shared inbox (same processing, target resolved from the
/// activity itself).
pub async fn shared_inbox_handler(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    process_inbox(state, None, method, uri, headers, body).await
}

fn err_status(code: u16, msg: String) -> AppError {
    match code {
        401 => AppError::Auth(msg),
        403 => AppError::Forbidden(msg),
        404 => AppError::NotFound(msg),
        _ => AppError::BadRequest(msg),
    }
}

/// The verified signer of an inbox request.
struct Signer {
    actor_id: String,
    inbox_url: String,
}

async fn process_inbox(
    state: AppState,
    target_username: Option<&str>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    // Every rejection must be diagnosable from journald alone (owner QA
    // follow-up): path + reason + a brief payload digest — never the payload.
    // Behavior is unchanged: the error is returned exactly as before.
    let path = uri.path().to_string();
    let body_len = body.len();
    let digest = hex::encode(&Sha256::digest(body.as_ref())[..8]);
    let db = state.db.clone();
    match process_inbox_inner(state, target_username, method, uri, headers, body).await {
        Ok(resp) => Ok(resp),
        Err(e) => {
            tracing::warn!("[AP inbox] rejected POST {path}: {e} ({body_len} bytes, sha256:{digest})");
            // Journal-independent record: journald rate-limits under AppView
            // log storms and dropped ~15 rejection lines in the 2026-11-18
            // window exactly when diagnosis mattered. Best-effort — never
            // replaces the error return. Prune on write keeps the table
            // bounded without a scheduler (the runner has no journal/cron).
            let reason = e.to_string();
            let _ = sqlx::query(
                "INSERT INTO ap_inbox_rejections (path, reason, body_bytes, body_digest) \
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(&path)
            .bind(&reason)
            .bind(body_len as i32)
            .bind(&digest)
            .execute(&db)
            .await;
            let _ = sqlx::query(
                "DELETE FROM ap_inbox_rejections WHERE created_at < now() - interval '7 days'",
            )
            .execute(&db)
            .await;
            Err(e)
        }
    }
}

async fn process_inbox_inner(
    state: AppState,
    target_username: Option<&str>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    if !crate::ap::ap_actors_enabled() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    if body.len() > MAX_BODY_BYTES {
        return Err(AppError::BadRequest("inbox body too large".into()));
    }

    // The named inbox must belong to a live actor (the shared inbox resolves
    // targets from the activity instead).
    let target_user = match target_username {
        Some(u) => {
            let user = UserRow::find_ap_enabled_by_username(&state.db, u)
                .await
                .map_err(|e| AppError::Internal(format!("db: {e}")))?
                .ok_or_else(|| AppError::NotFound("no such actor".into()))?;
            Some(user)
        }
        None => None,
    };

    // Host must be ours — a correctly signed request aimed at another
    // instance must not be processed here. Compare hostnames (port-stripped on
    // both sides: dev DOMAIN may carry a port).
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let host_name = host.split(':').next().unwrap_or("");
    let expected_host = state
        .config
        .domain
        .split(':')
        .next()
        .unwrap_or(&state.config.domain);
    if host_name.is_empty() || !host_name.eq_ignore_ascii_case(expected_host) {
        return Err(err_status(400, format!("host mismatch: {host}")));
    }

    let path_and_query = uri
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| uri.path().to_string());

    // ── 1. Signature over the exact raw bytes — before any parsing/writes. ──
    let prepared = http_sig::prepare_verify(
        method.as_str(),
        &path_and_query,
        &headers,
        &body,
        chrono::Utc::now(),
    )
    .map_err(|e| err_status(e.status(), format!("signature check: {e}")))?;

    // ── 2. Resolve the signer's key and verify. ───────────────────────────
    let signer_actor = prepared
        .key_id
        .split('#')
        .next()
        .unwrap_or(&prepared.key_id)
        .to_string();
    if signer_actor.is_empty() {
        return Err(err_status(401, "unresolvable keyId".into()));
    }

    let mut remote_signer: Option<remote::RemoteActor> = None;
    let public_pem = {
        let local = UserRow::find_by_ap_actor_id(&state.db, &signer_actor)
            .await
            .map_err(|e| AppError::Internal(format!("db: {e}")))?
            .and_then(|u| u.ap_public_key_pem);
        match local {
            Some(pem) => Some(pem),
            None => {
                // Remote signer: cached key first, SSRF-guarded fetch on miss.
                // Signed fetch as the target actor when we know it.
                let fetch_signer: Option<(String, String)> = match &target_user {
                    Some(u) => keys::load_fetch_signer(&state.db, u.id, &state.config.jwt_secret).await,
                    None => None,
                };
                match remote::load_or_fetch_actor(
                    &state.db,
                    &signer_actor,
                    fetch_signer.as_ref().map(|(k, p)| (k.as_str(), p.as_str())),
                )
                .await
                {
                    Ok(actor) => {
                        let pem = actor.public_key_pem.clone();
                        remote_signer = Some(actor);
                        pem
                    }
                    // Transient (cache/DB) failures MUST go back as 5xx so
                    // the sender retries — a 401 here told Mastodon the
                    // signer was permanently unresolvable and the follow
                    // was silently dropped (owner QA 2026-11-18).
                    Err(remote::ResolveError::Transient(e)) => {
                        return Err(AppError::Internal(format!(
                            "signer key lookup failed (retry later): {e}"
                        )));
                    }
                    Err(remote::ResolveError::Unresolvable(e)) => {
                        return Err(err_status(401, format!("cannot resolve signer key: {e}")));
                    }
                }
            }
        }
    };
    let public_pem = public_pem.ok_or_else(|| {
        err_status(401, format!("signer {signer_actor} has no resolvable key"))
    })?;
    http_sig::verify_signature(&prepared.signing_string, &prepared.signature, &public_pem)
        .map_err(|e| err_status(e.status(), format!("signature check: {e}")))?;

    // Persist the resolved remote actor (cache) only now that the signature
    // verified — nothing unverified is ever written.
    let signer_inbox = if let Some(actor) = &remote_signer {
        let inbox = actor.inbox_url.clone();
        if let Err(e) = remote::upsert_actor(&state.db, actor).await {
            tracing::warn!("[AP] remote actor cache write failed for {signer_actor}: {e}");
        }
        inbox
    } else {
        // Local signer: inbox from the actor document convention.
        let row: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT ap_inbox FROM users WHERE ap_actor_id = $1 AND ap_enabled = true",
        )
        .bind(&signer_actor)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
        row.and_then(|(inbox,)| inbox)
            .unwrap_or_else(|| format!("{signer_actor}/inbox"))
    };

    // ── 3. Parse, then enforce signer ownership BEFORE any write. ─────────
    let activity: Value = serde_json::from_slice(&body)
        .map_err(|e| AppError::BadRequest(format!("invalid JSON body: {e}")))?;
    if !activity.is_object() {
        return Err(AppError::BadRequest("activity must be a JSON object".into()));
    }
    let activity_type = activity
        .get("type")
        .and_then(|t| t.as_str())
        .filter(|t| !t.is_empty())
        .ok_or_else(|| AppError::BadRequest("activity without type".into()))?
        .to_string();
    let activity_id = activity
        .get("id")
        .and_then(|i| i.as_str())
        .filter(|i| i.starts_with("http://") || i.starts_with("https://"))
        .ok_or_else(|| AppError::BadRequest("activity without absolute http(s) id".into()))?
        .to_string();

    // The activity must be authored by the verified signer…
    let claimed = activity
        .get("actor")
        .and_then(ingest::actor_url_str)
        .or_else(|| ingest::actor_of(&activity, None));
    let claimed = claimed.ok_or_else(|| {
        AppError::BadRequest("activity without actor/attributedTo".into())
    })?;
    if claimed != signer_actor {
        return Err(err_status(
            403,
            format!("activity actor {claimed} is not the signer {signer_actor}"),
        ));
    }
    // …and so must any embedded object's author (spoofed attributedTo).
    if let Some(obj) = activity.get("object").and_then(|o| o.as_object()) {
        if let Some(attr) = obj.get("attributedTo").and_then(ingest::actor_url_str) {
            if attr != signer_actor {
                return Err(err_status(
                    403,
                    format!("object attributedTo {attr} is not the signer {signer_actor}"),
                ));
            }
        }
    }

    let object_id = match activity.get("object") {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Object(o)) => o
            .get("id")
            .and_then(|i| i.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        _ => None,
    };

    // ── 4. Persist + dedupe, then effects — one transaction. ──────────────
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| AppError::Internal(format!("txn: {e}")))?;

    let inserted = store::insert_activity(
        &mut tx,
        &activity_id,
        &signer_actor,
        &activity_type,
        object_id.as_deref(),
        None,
        "in",
        &activity,
    )
    .await
    .map_err(|e| AppError::Internal(format!("activity store: {e}")))?;

    if !inserted {
        // Replay: the effects already happened (or are in flight with this
        // activity id) — acknowledge without duplicating anything.
        tx.rollback()
            .await
            .map_err(|e| AppError::Internal(format!("txn: {e}")))?;
        return Ok((StatusCode::ACCEPTED, Json(json!({ "ok": true, "replayed": true }))).into_response());
    }

    dispatch(
        &state,
        &mut tx,
        &Signer {
            actor_id: signer_actor.clone(),
            inbox_url: signer_inbox,
        },
        &activity,
        &activity_type,
        &activity_id,
        object_id.as_deref(),
    )
    .await?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(format!("txn: {e}")))?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "ok": true }))).into_response())
}

/// Apply the effects of a freshly stored inbound activity (inside the storing
/// transaction).
async fn dispatch(
    state: &AppState,
    tx: &mut sqlx::PgConnection,
    signer: &Signer,
    activity: &Value,
    activity_type: &str,
    activity_id: &str,
    object_id: Option<&str>,
) -> Result<(), AppError> {
    match activity_type {
        "Create" | "Note" | "Story" | "Photo" => {
            // Create → the existing ingest mappers (idempotent appview writes).
            ingest::ingest_activity(&mut *tx, activity, None)
                .await
                .map_err(|e| AppError::Internal(format!("ingest: {e}")))?;
            // Reply notification: Create with inReplyTo targeting a local post.
            if let Some(parent) = activity
                .get("object")
                .and_then(|o| o.get("inReplyTo"))
                .and_then(ingest::actor_url_str)
            {
                notify_post_interaction(
                    &mut *tx,
                    state,
                    signer,
                    &parent,
                    crate::ap::notify::TYPE_REPLY,
                    activity_id,
                )
                .await?;
            }
        }
        "Follow" => {
            let target = object_id.ok_or_else(|| {
                AppError::BadRequest("Follow without object".into())
            })?;
            let user = local_actor_user(state, target)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("no such actor {target}")))?;

            if user.manually_approves_followers {
                // Locked account: queue a pending request instead of auto-accept.
                crate::db::follow_requests::FollowRequestRow::create(
                    &mut *tx,
                    &signer.actor_id,
                    user.id,
                    Some(&signer.inbox_url),
                    Some(activity_id),
                    "activitypub",
                )
                .await
                .map_err(|e| AppError::Internal(format!("follow request store: {e}")))?;
                crate::ap::notify::notify(
                    &mut *tx,
                    user.id,
                    crate::ap::notify::TYPE_FOLLOW_REQUEST,
                    "Follow request",
                    Some(&format!(
                        "{} wants to follow you",
                        crate::ap::notify::remote_actor_ref(&state.db, &signer.actor_id).await.handle
                    )),
                    Some(json!({ "request_actor": signer.actor_id })),
                    activity_id,
                    &crate::ap::notify::remote_actor_ref(&state.db, &signer.actor_id).await,
                )
                .await
                .map_err(|e| AppError::Internal(format!("notification: {e}")))?;
                return Ok(());
            }

            // Store/update the follower; exactly one Accept per Follow id
            // (deterministic Accept id → dedupe at the activity store).
            sqlx::query(
                "INSERT INTO followers (actor_id, inbox_url, user_id, accepted) \
                 VALUES ($1, $2, $3, true) \
                 ON CONFLICT (actor_id, user_id) DO UPDATE \
                   SET inbox_url = EXCLUDED.inbox_url, accepted = true",
            )
            .bind(&signer.actor_id)
            .bind(&signer.inbox_url)
            .bind(user.id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(format!("followers store: {e}")))?;

            let local_actor_id = user
                .ap_actor_id
                .clone()
                .unwrap_or_else(|| crate::ap::actor::canonical_actor_url(&state.config.domain, &user.username));
            let accept_id = format!(
                "{}/activities/accept-{}",
                local_actor_id,
                activity_hash(activity_id)
            );
            let accept = json!({
                "@context": "https://www.w3.org/ns/activitystreams",
                "id": accept_id,
                "type": "Accept",
                "actor": local_actor_id,
                "object": activity,
                "to": [signer.actor_id],
            });
            let new = store::insert_activity(
                &mut *tx,
                &accept_id,
                &local_actor_id,
                "Accept",
                Some(activity_id),
                None,
                "out",
                &accept,
            )
            .await
            .map_err(|e| AppError::Internal(format!("accept store: {e}")))?;
            if new {
                let mut inboxes =
                    store::audience_inboxes(&state.db, &local_actor_id, &[signer.actor_id.clone()])
                        .await;
                if inboxes.is_empty() {
                    // Local or unresolved signer — still answer their inbox.
                    inboxes.push(signer.inbox_url.clone());
                }
                store::enqueue_for_inboxes(&mut *tx, &accept_id, &inboxes)
                    .await
                    .map_err(|e| AppError::Internal(format!("accept enqueue: {e}")))?;
            }
            crate::ap::notify::notify(
                &mut *tx,
                user.id,
                crate::ap::notify::TYPE_FOLLOW,
                "New follower",
                Some(&format!(
                    "{} followed you",
                    crate::ap::notify::remote_actor_ref(&state.db, &signer.actor_id).await.handle
                )),
                Some(json!({ "request_actor": signer.actor_id })),
                activity_id,
                &crate::ap::notify::remote_actor_ref(&state.db, &signer.actor_id).await,
            )
            .await
            .map_err(|e| AppError::Internal(format!("notification: {e}")))?;
        }
        "Accept" => {
            // Accept our outgoing Follow → activate the pending follows row.
            let follow_id = object_id
                .map(str::to_string)
                .or_else(|| {
                    activity
                        .get("object")
                        .and_then(|o| o.get("id"))
                        .and_then(|i| i.as_str())
                        .map(str::to_string)
                });
            let follow_id = match follow_id {
                Some(id) => id,
                None => return Ok(()),
            };
            let row: Option<(String, String)> = sqlx::query_as(
                "SELECT actor_id, object_id FROM ap_activities \
                 WHERE id = $1 AND activity_type = 'Follow' AND direction = 'out'",
            )
            .bind(&follow_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(format!("follow lookup: {e}")))?;
            let Some((follow_actor, followed)) = row else {
                return Ok(());
            };
            // The Accept must answer a Follow we sent for THIS signer.
            if followed != signer.actor_id {
                return Ok(());
            }
            let user = local_actor_user(state, &follow_actor).await?;
            if let Some(user) = user {
                sqlx::query(
                    "UPDATE follows SET status = 'accepted' \
                     WHERE follower_id = $1 AND followed_actor = $2 AND status = 'pending'",
                )
                .bind(user.id)
                .bind(&signer.actor_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::Internal(format!("follows accept: {e}")))?;
            }
        }
        "Like" => {
            if let Some(obj) = object_id {
                apply_count(&mut *tx, obj, Count::Like, 1).await?;
                notify_post_interaction(&mut *tx, state, signer, obj, crate::ap::notify::TYPE_LIKE, activity_id).await?;
            }
        }
        "Announce" => {
            if let Some(obj) = object_id {
                apply_count(&mut *tx, obj, Count::Repost, 1).await?;
                notify_post_interaction(&mut *tx, state, signer, obj, crate::ap::notify::TYPE_REPOST, activity_id).await?;
            }
        }
        "Undo" => {
            undo_effect(state, &mut *tx, signer, activity, object_id).await?;
        }
        // Everything else (Update, Delete, …) is stored but has no effect —
        // core handling covers Create/Follow/Accept/Like/Announce/Undo.
        _ => {}
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Count {
    Like,
    Repost,
}

/// Notify the owner of a local post when a remote actor likes/reposts/replies
/// to it. Resolves the post's author (appview_posts by uri, then the mapped
/// at:// row) and routes one notification to their account. No-op when the
/// object isn't a post of a local user (e.g. a remote object).
async fn notify_post_interaction(
    tx: &mut sqlx::PgConnection,
    state: &AppState,
    signer: &Signer,
    object_uri: &str,
    notification_type: &str,
    activity_id: &str,
) -> Result<(), AppError> {
    // Find the post's author handle/did via appview_posts.
    let author: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT author_handle, author_did FROM appview_posts WHERE uri = $1 LIMIT 1",
    )
    .bind(object_uri)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(format!("post lookup: {e}")))?;
    let Some((author_handle, author_did)) = author else {
        return Ok(());
    };
    // Map the post author to the local owner. The bridge stores author_handle
    // in the `@user@domain` form (and author_did as the actor URL), so an
    // exact `username = author_handle` match silently dropped EVERY
    // interaction notification on bridged local posts — accept the bare
    // username (handle stripped) and fall back to the canonical actor match
    // (identity-graph P5 audit).
    let bare = author_handle
        .as_deref()
        .map(|h| {
            h.trim()
                .trim_start_matches('@')
                .split('@')
                .next()
                .unwrap_or("")
                .to_string()
        })
        .unwrap_or_default();
    let mut owner_id: Option<uuid::Uuid> = None;
    if !bare.is_empty() {
        owner_id = sqlx::query_scalar("SELECT id FROM users WHERE username = $1 LIMIT 1")
            .bind(&bare)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(format!("user lookup: {e}")))?;
    }
    if owner_id.is_none() {
        if let Some(did) = author_did.as_deref().filter(|s| s.starts_with("http")) {
            owner_id = sqlx::query_scalar(
                "SELECT id FROM users WHERE ap_actor_id = $1 OR $1 LIKE '%' || username || '%' LIMIT 1",
            )
            .bind(did)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(format!("user lookup: {e}")))?;
        }
    }
    let Some(owner_id) = owner_id else {
        return Ok(());
    };
    let title = match notification_type {
        crate::ap::notify::TYPE_LIKE => "New like",
        crate::ap::notify::TYPE_REPOST => "New repost",
        crate::ap::notify::TYPE_REPLY => "New reply",
        _ => "New interaction",
    };
    // Provenance-correct actor resolution: the row's handle comes from the
    // actor cache (never a raw URI or DID in the visible fields).
    let actor_ref = crate::ap::notify::remote_actor_ref(&state.db, &signer.actor_id).await;
    let body = match notification_type {
        crate::ap::notify::TYPE_LIKE => format!("{} liked your post", actor_ref.handle),
        crate::ap::notify::TYPE_REPOST => format!("{} reposted your post", actor_ref.handle),
        crate::ap::notify::TYPE_REPLY => format!("{} replied to your post", actor_ref.handle),
        _ => format!("{} interacted with your post", actor_ref.handle),
    };
    crate::ap::notify::notify(
        &mut *tx,
        owner_id,
        notification_type,
        title,
        Some(&body),
        Some(json!({ "subject_uri": object_uri })),
        activity_id,
        &actor_ref,
    )
    .await
    .map_err(|e| AppError::Internal(format!("notification: {e}")))?;
    Ok(())
}

/// Atomically move a target's interaction count (`GREATEST` clamps at zero).
/// Both keyed copies of a native post (the https AP object row and the at://
/// appview row it maps to via `ap_activities.local_uri`) are moved together.
async fn apply_count(
    tx: &mut sqlx::PgConnection,
    object: &str,
    count: Count,
    delta: i32,
) -> Result<(), AppError> {
    let col = match count {
        Count::Like => "like_count",
        Count::Repost => "repost_count",
    };
    let mapped = "(SELECT local_uri FROM ap_activities WHERE object_id = $1 \
                  AND local_uri IS NOT NULL ORDER BY created_at DESC LIMIT 1)";
    let posts_sql = format!(
        "UPDATE appview_posts SET {col} = GREATEST({col} + $2, 0) \
         WHERE uri = $1 OR uri = COALESCE({mapped}, '')"
    );
    sqlx::query(&posts_sql)
        .bind(object)
        .bind(delta)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(format!("count update: {e}")))?;
    let stories_sql = format!(
        "UPDATE appview_stories SET {col} = GREATEST({col} + $2, 0) \
         WHERE uri = $1 OR uri = COALESCE({mapped}, '')"
    );
    sqlx::query(&stories_sql)
        .bind(object)
        .bind(delta)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(format!("count update: {e}")))?;
    Ok(())
}

/// Undo handling: retire the signer-owned original exactly once and reverse
/// its effect. Repeat and foreign Undos are no-ops; counts clamp at zero.
async fn undo_effect(
    state: &AppState,
    tx: &mut sqlx::PgConnection,
    signer: &Signer,
    activity: &Value,
    object_id: Option<&str>,
) -> Result<(), AppError> {
    // Primary: Undo.object is the original activity (by id). Fallback: the
    // embedded original's (actor, object, type) when we track it under its own
    // id only.
    let mut original: Option<(String, String, String, Option<String>, Option<chrono::DateTime<chrono::Utc>>)> =
        None;
    if let Some(target) = object_id {
        original = sqlx::query_as(
            "SELECT id, actor_id, activity_type, object_id, undone_at \
             FROM ap_activities WHERE id = $1",
        )
        .bind(target)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(format!("undo lookup: {e}")))?;
    }
    if original.is_none() {
        if let Some(embedded) = activity.get("object").and_then(|o| o.as_object()) {
            if let (Some(etype), Some(eobj)) = (
                embedded.get("type").and_then(|t| t.as_str()),
                embedded.get("object").and_then(ingest::actor_url_str),
            ) {
                original = sqlx::query_as(
                    "SELECT id, actor_id, activity_type, object_id, undone_at \
                     FROM ap_activities \
                     WHERE actor_id = $1 AND object_id = $2 AND activity_type = $3 \
                       AND direction = 'in' AND undone_at IS NULL \
                     ORDER BY created_at DESC LIMIT 1",
                )
                .bind(&signer.actor_id)
                .bind(&eobj)
                .bind(etype)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| AppError::Internal(format!("undo lookup: {e}")))?;
            }
        }
    }

    let Some((orig_id, orig_actor, orig_type, orig_object, undone_at)) = original else {
        return Ok(());
    };
    // Foreign originals are never touched; already-undone originals are
    // repeat no-ops.
    if orig_actor != signer.actor_id || undone_at.is_some() {
        return Ok(());
    }
    let n = sqlx::query("UPDATE ap_activities SET undone_at = now() WHERE id = $1 AND undone_at IS NULL")
        .bind(&orig_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(format!("undo mark: {e}")))?
        .rows_affected();
    if n != 1 {
        return Ok(());
    }

    match orig_type.as_str() {
        "Like" => {
            if let Some(obj) = &orig_object {
                apply_count(&mut *tx, obj, Count::Like, -1).await?;
            }
        }
        "Announce" => {
            if let Some(obj) = &orig_object {
                apply_count(&mut *tx, obj, Count::Repost, -1).await?;
            }
        }
        "Follow" => {
            // The signer withdrew their Follow → stop following us.
            if let Some(obj) = &orig_object {
                if let Some(user) = local_actor_user(state, obj).await? {
                    sqlx::query(
                        "DELETE FROM followers WHERE actor_id = $1 AND user_id = $2",
                    )
                    .bind(&signer.actor_id)
                    .bind(user.id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| AppError::Internal(format!("followers delete: {e}")))?;
                }
            }
        }
        "Create" => {
            // The signer withdrew their Create → remove the ingested object.
            if let Some(obj) = &orig_object {
                sqlx::query("DELETE FROM appview_posts WHERE uri = $1")
                    .bind(obj)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| AppError::Internal(format!("undo create: {e}")))?;
                sqlx::query("DELETE FROM appview_stories WHERE uri = $1")
                    .bind(obj)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| AppError::Internal(format!("undo create: {e}")))?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// The local user behind an AP actor id (issued actors only — a disabled or
/// unissued account is not an actor).
async fn local_actor_user(state: &AppState, actor_id: &str) -> Result<Option<UserRow>, AppError> {
    if let Some(user) = UserRow::find_by_ap_actor_id(&state.db, actor_id)
        .await
        .map_err(|e| AppError::Internal(format!("db: {e}")))?
    {
        return Ok(Some(user));
    }
    // Remotes cache alias actor URLs — we publish /users/{name} and
    // /actors/{name} alongside the canonical /@{name} — and echo them back as
    // the Follow object. Reconcile our own alias forms (including trailing
    // slashes) to the canonical identity instead of aborting the activity
    // before the follower row, Accept, and notification writer run.
    local_actor_alias_user(state, actor_id).await
}

/// Resolve one of our published alias actor URL forms to its local user. The
/// input must be exactly one of [`crate::ap::actor::canonical_actor_url`] /
/// [`crate::ap::actor::alias_actor_urls`] for the parsed username (modulo a
/// trailing slash); anything else is not this server's actor and resolves to
/// None.
async fn local_actor_alias_user(
    state: &AppState,
    actor_id: &str,
) -> Result<Option<UserRow>, AppError> {
    let base = format!("https://{}/", state.config.domain);
    let Some(path) = actor_id.strip_prefix(&base) else {
        return Ok(None);
    };
    let path = path.trim_end_matches('/');
    let (form, username) = if let Some(u) = path.strip_prefix('@') {
        ("@", u)
    } else if let Some(u) = path.strip_prefix("users/") {
        ("users/", u)
    } else if let Some(u) = path.strip_prefix("actors/") {
        ("actors/", u)
    } else {
        return Ok(None);
    };
    if username.is_empty() || username.contains('/') {
        return Ok(None);
    }
    let candidate = format!("{base}{form}{username}");
    let published = crate::ap::actor::canonical_actor_url(&state.config.domain, username);
    let published = std::iter::once(&published)
        .chain(crate::ap::actor::alias_actor_urls(&state.config.domain, username).iter())
        .any(|p| p == &candidate);
    if !published {
        return Ok(None);
    }
    UserRow::find_ap_enabled_by_username(&state.db, username)
        .await
        .map_err(|e| AppError::Internal(format!("db: {e}")))
}

/// Deterministic short hash for derived activity ids (Accept/Reject).
pub(crate) fn activity_hash(input: &str) -> String {
    let mut h = Sha256::new();
    h.update(input.as_bytes());
    let digest = h.finalize();
    let hex = hex::encode(digest);
    hex[..32].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ap::testutil::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    /// POST a signed body to the router; returns (status, json body).
    async fn post_signed(
        app: &axum::Router,
        path: &str,
        body: &Value,
        signer: &TestSigner,
        host: &str,
    ) -> (u16, Value) {
        let raw = serde_json::to_vec(body).expect("serialize");
        let req = signed_inbox_request(path, &raw, signer, host);
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        let status = resp.status().as_u16();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap_or_default();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    fn create_note(signer: &TestSigner, suffix: &str) -> (String, Value) {
        let note_id = format!("{}/statuses/{suffix}", signer.actor_id);
        let activity_id = format!("{}/activities/create-{suffix}", signer.actor_id);
        let activity = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": activity_id,
            "type": "Create",
            "actor": signer.actor_id,
            "to": ["https://www.w3.org/ns/activitystreams#Public"],
            "object": {
                "id": note_id,
                "type": "Note",
                "attributedTo": signer.actor_id,
                "content": format!("<p>hello {suffix}</p>"),
                "published": "2026-11-10T12:00:00Z"
            }
        });
        (note_id, activity)
    }

    /// (like_count, repost_count) for an appview post.
    async fn counts(pool: &sqlx::PgPool, note: &str) -> (i32, i32) {
        sqlx::query_as("SELECT like_count, repost_count FROM appview_posts WHERE uri = $1")
            .bind(note)
            .fetch_one(pool)
            .await
            .expect("counts")
    }

    #[tokio::test]
    async fn signed_create_indexes_once_and_replay_is_noop() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = ap_router(state);

        let target = make_local_actor(&pool, &domain).await;
        let s = unique();
        let signer = make_signer(&format!("https://remote.test/users/bob_{s}"));
        seed_remote_actor(&pool, &signer, &format!("{}/inbox", signer.actor_id), None).await;

        let (note_id, activity) = create_note(&signer, &s);
        let activity_id = activity["id"].as_str().unwrap().to_string();
        let path = format!("/@{}/inbox", target.username);

        let (status, body) = post_signed(&app, &path, &activity, &signer, &domain).await;
        assert_eq!(status, 202, "signed Create accepted: {body}");

        // Exactly one indexed Note and one stored activity…
        let posts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM appview_posts WHERE uri = $1")
            .bind(&note_id)
            .fetch_one(&pool)
            .await
            .expect("count posts");
        assert_eq!(posts, 1, "Note indexed exactly once");

        // …and the replay (same activity id, fresh signature) changes nothing.
        let (status2, body2) = post_signed(&app, &path, &activity, &signer, &domain).await;
        assert_eq!(status2, 202, "replay acknowledged");
        assert_eq!(body2["replayed"], json!(true), "replay flagged");

        let posts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM appview_posts WHERE uri = $1")
            .bind(&note_id)
            .fetch_one(&pool)
            .await
            .expect("count posts");
        assert_eq!(posts, 1, "replay does not duplicate the index");
        let acts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ap_activities WHERE id = $1")
            .bind(&activity_id)
            .fetch_one(&pool)
            .await
            .expect("count activities");
        assert_eq!(acts, 1, "one activity row per id");

        // Cleanup.
        let _ = sqlx::query("DELETE FROM appview_posts WHERE uri = $1")
            .bind(&note_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id = $1")
            .bind(&signer.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM ap_remote_actors WHERE actor_id = $1")
            .bind(&signer.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(target.user.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn unsigned_bad_digest_and_spoofed_requests_write_nothing() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = ap_router(state);

        let target = make_local_actor(&pool, &domain).await;
        let s = unique();
        let signer = make_signer(&format!("https://remote.test/users/eve_{s}"));
        seed_remote_actor(&pool, &signer, &format!("{}/inbox", signer.actor_id), None).await;

        let (note_id, activity) = create_note(&signer, &s);
        let activity_id = activity["id"].as_str().unwrap().to_string();
        let path = format!("/@{}/inbox", target.username);
        let raw = serde_json::to_vec(&activity).expect("serialize");

        // 1. Unsigned request → rejected.
        let unsigned = Request::builder()
            .method("POST")
            .uri(&path)
            .header("host", &domain)
            .body(Body::from(raw.clone()))
            .expect("request");
        let resp = app.clone().oneshot(unsigned).await.expect("oneshot");
        assert_ne!(resp.status().as_u16(), 202, "unsigned request rejected");

        // 2. Body tampered after signing (Digest mismatch) → rejected.
        let mut tampered = raw.clone();
        tampered.push(b' ');
        // Signature and headers cover the clean bytes; swap in the tampered body.
        let clean_sig_req = signed_inbox_request(&path, &raw, &signer, &domain);
        let (parts, _) = clean_sig_req.into_parts();
        let req = Request::from_parts(parts, Body::from(tampered));
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status().as_u16(), 400, "digest mismatch rejected");

        // 3. Spoofed attributedTo (object authored by someone else) → 403.
        let mut spoofed = activity.clone();
        spoofed["object"]["attributedTo"] = json!("https://remote.test/users/someone-else");
        let (status, _) = post_signed(&app, &path, &spoofed, &signer, &domain).await;
        assert_eq!(status, 403, "spoofed attributedTo rejected");

        // Nothing was written for any of them.
        let acts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ap_activities WHERE id = $1")
            .bind(&activity_id)
            .fetch_one(&pool)
            .await
            .expect("count activities");
        assert_eq!(acts, 0, "rejected requests store no activity");
        let posts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM appview_posts WHERE uri = $1")
            .bind(&note_id)
            .fetch_one(&pool)
            .await
            .expect("count posts");
        assert_eq!(posts, 0, "rejected requests index nothing");

        // Cleanup.
        let _ = sqlx::query("DELETE FROM ap_remote_actors WHERE actor_id = $1")
            .bind(&signer.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(target.user.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn follow_queues_exactly_one_accept() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = ap_router(state);

        let target = make_local_actor(&pool, &domain).await;
        // Migration 102 made approve-only the default; this test exercises the
        // auto-accept path.
        crate::db::users::UserRow::set_manually_approves_followers(&pool, target.user.id, false)
            .await
            .expect("unlock account");
        let s = unique();
        let signer = make_signer(&format!("https://remote.test/users/carol_{s}"));
        let shared = format!("https://remote.test/inbox");
        seed_remote_actor(&pool, &signer, &format!("{}/inbox", signer.actor_id), Some(&shared)).await;

        let follow_id = format!("{}/activities/follow-{s}", signer.actor_id);
        let follow = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": follow_id,
            "type": "Follow",
            "actor": signer.actor_id,
            "object": target.actor_id,
        });
        let path = format!("/@{}/inbox", target.username);

        let (status, _) = post_signed(&app, &path, &follow, &signer, &domain).await;
        assert_eq!(status, 202, "Follow accepted");

        // Follower recorded as accepted.
        let follower: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM followers WHERE actor_id = $1 AND user_id = $2 AND accepted = true",
        )
        .bind(&signer.actor_id)
        .bind(target.user.id)
        .fetch_one(&pool)
        .await
        .expect("count followers");
        assert_eq!(follower, 1, "follower stored");

        // Exactly one Accept, queued exactly once (to the shared inbox).
        let accepts: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ap_activities WHERE activity_type = 'Accept' AND actor_id = $1 AND object_id = $2",
        )
        .bind(&target.actor_id)
        .bind(&follow_id)
        .fetch_one(&pool)
        .await
        .expect("count accepts");
        assert_eq!(accepts, 1, "exactly one Accept");
        let queued: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ap_delivery_queue q JOIN ap_activities a ON a.id = q.activity_id \
             WHERE a.activity_type = 'Accept' AND a.actor_id = $1",
        )
        .bind(&target.actor_id)
        .fetch_one(&pool)
        .await
        .expect("count queue rows");
        assert_eq!(queued, 1, "exactly one Accept delivery");
        let inbox: String = sqlx::query_scalar(
            "SELECT q.inbox_url FROM ap_delivery_queue q JOIN ap_activities a ON a.id = q.activity_id \
             WHERE a.activity_type = 'Accept' AND a.actor_id = $1",
        )
        .bind(&target.actor_id)
        .fetch_one(&pool)
        .await
        .expect("accept inbox");
        assert_eq!(inbox, shared, "delivered to the shared inbox");

        // Replay: no second Accept, no second delivery.
        let (status2, _) = post_signed(&app, &path, &follow, &signer, &domain).await;
        assert_eq!(status2, 202, "replay acknowledged");
        let accepts: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ap_activities WHERE activity_type = 'Accept' AND actor_id = $1",
        )
        .bind(&target.actor_id)
        .fetch_one(&pool)
        .await
        .expect("count accepts");
        assert_eq!(accepts, 1, "replay queues no second Accept");
        let queued: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ap_delivery_queue q JOIN ap_activities a ON a.id = q.activity_id \
             WHERE a.activity_type = 'Accept' AND a.actor_id = $1",
        )
        .bind(&target.actor_id)
        .fetch_one(&pool)
        .await
        .expect("count queue rows");
        assert_eq!(queued, 1, "replay queues no second delivery");

        // Cleanup (queue rows cascade from ap_activities).
        let _ = sqlx::query("DELETE FROM followers WHERE actor_id = $1")
            .bind(&signer.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id IN ($1, $2)")
            .bind(&signer.actor_id)
            .bind(&target.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM ap_remote_actors WHERE actor_id = $1")
            .bind(&signer.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(target.user.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn like_announce_undo_counts_are_atomic_idempotent_and_clamped() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = ap_router(state);

        let target = make_local_actor(&pool, &domain).await;
        let s = unique();
        let signer = make_signer(&format!("https://remote.test/users/dave_{s}"));
        seed_remote_actor(&pool, &signer, &format!("{}/inbox", signer.actor_id), None).await;
        let other = make_signer(&format!("https://remote.test/users/frank_{s}"));
        seed_remote_actor(&pool, &other, &format!("{}/inbox", other.actor_id), None).await;

        let path = format!("/@{}/inbox", target.username);

        // The target Note (indexed via a signed Create from the signer).
        let (note_id, create) = create_note(&signer, &s);
        let (status, _) = post_signed(&app, &path, &create, &signer, &domain).await;
        assert_eq!(status, 202);

        // Like increments exactly once, even with a duplicate delivery.
        let like_id = format!("{}/activities/like-{s}", signer.actor_id);
        let like = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": like_id,
            "type": "Like",
            "actor": signer.actor_id,
            "object": note_id,
            "to": [target.actor_id],
        });
        let (status, _) = post_signed(&app, &path, &like, &signer, &domain).await;
        assert_eq!(status, 202);
        assert_eq!(counts(&pool, &note_id).await, (1, 0), "Like increments once");
        let (status, _) = post_signed(&app, &path, &like, &signer, &domain).await;
        assert_eq!(status, 202);
        assert_eq!(counts(&pool, &note_id).await, (1, 0), "duplicate Like is a no-op");

        // Announce increments repost_count exactly once.
        let ann_id = format!("{}/activities/announce-{s}", signer.actor_id);
        let ann = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": ann_id,
            "type": "Announce",
            "actor": signer.actor_id,
            "object": note_id,
        });
        let (status, _) = post_signed(&app, &path, &ann, &signer, &domain).await;
        assert_eq!(status, 202);
        let (status, _) = post_signed(&app, &path, &ann, &signer, &domain).await;
        assert_eq!(status, 202);
        assert_eq!(counts(&pool, &note_id).await, (1, 1), "Announce increments once");

        // Undo decrements exactly once; repeat Undo is a no-op.
        let undo_id = format!("{}/activities/undo-like-{s}", signer.actor_id);
        let undo = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": undo_id,
            "type": "Undo",
            "actor": signer.actor_id,
            "object": like_id,
        });
        let (status, _) = post_signed(&app, &path, &undo, &signer, &domain).await;
        assert_eq!(status, 202);
        assert_eq!(counts(&pool, &note_id).await, (0, 1), "Undo decrements once");

        let undo2_id = format!("{}/activities/undo-like-2-{s}", signer.actor_id);
        let undo2 = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": undo2_id,
            "type": "Undo",
            "actor": signer.actor_id,
            "object": like_id,
        });
        let (status, _) = post_signed(&app, &path, &undo2, &signer, &domain).await;
        assert_eq!(status, 202);
        assert_eq!(counts(&pool, &note_id).await, (0, 1), "repeat Undo is a no-op");

        // Foreign Undo (different signer) never touches the original.
        let foreign_id = format!("{}/activities/undo-like-{s}", other.actor_id);
        let foreign = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": foreign_id,
            "type": "Undo",
            "actor": other.actor_id,
            "object": like_id,
        });
        let (status, _) = post_signed(&app, &path, &foreign, &other, &domain).await;
        assert_eq!(status, 202);
        let undone: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT undone_at FROM ap_activities WHERE id = $1")
                .bind(&like_id)
                .fetch_one(&pool)
                .await
                .expect("like row");
        assert!(undone.is_some(), "original retired exactly once");
        let foreign_count: i32 = sqlx::query_scalar("SELECT like_count FROM appview_posts WHERE uri = $1")
            .bind(&note_id)
            .fetch_one(&pool)
            .await
            .expect("counts");
        assert_eq!(foreign_count, 0, "foreign Undo changes nothing");

        // Counts clamp at zero: an Undo of an un-counted Like must not go < 0.
        let stray_like = format!("{}/activities/like-stray-{s}", signer.actor_id);
        sqlx::query(
            "INSERT INTO ap_activities (id, actor_id, activity_type, object_id, direction, payload) \
             VALUES ($1, $2, 'Like', $3, 'in', $4)",
        )
        .bind(&stray_like)
        .bind(&signer.actor_id)
        .bind(&note_id)
        .bind(json!({"id": stray_like, "type": "Like", "actor": signer.actor_id, "object": note_id}))
        .execute(&pool)
        .await
        .expect("seed stray like");
        sqlx::query("UPDATE appview_posts SET like_count = 0 WHERE uri = $1")
            .bind(&note_id)
            .execute(&pool)
            .await
            .expect("zero like_count");
        let undo3_id = format!("{}/activities/undo-stray-{s}", signer.actor_id);
        let undo3 = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": undo3_id,
            "type": "Undo",
            "actor": signer.actor_id,
            "object": stray_like,
        });
        let (status, _) = post_signed(&app, &path, &undo3, &signer, &domain).await;
        assert_eq!(status, 202);
        let clamped: i32 = sqlx::query_scalar("SELECT like_count FROM appview_posts WHERE uri = $1")
            .bind(&note_id)
            .fetch_one(&pool)
            .await
            .expect("counts");
        assert_eq!(clamped, 0, "decrement clamps at zero");

        // Undo of the Announce reverses the repost count too.
        let undo_ann_id = format!("{}/activities/undo-ann-{s}", signer.actor_id);
        let undo_ann = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": undo_ann_id,
            "type": "Undo",
            "actor": signer.actor_id,
            "object": ann_id,
        });
        let (status, _) = post_signed(&app, &path, &undo_ann, &signer, &domain).await;
        assert_eq!(status, 202);
        assert_eq!(counts(&pool, &note_id).await, (0, 0), "Announce undone");

        // Cleanup.
        let _ = sqlx::query("DELETE FROM appview_posts WHERE uri = $1")
            .bind(&note_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id IN ($1, $2)")
            .bind(&signer.actor_id)
            .bind(&other.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM ap_remote_actors WHERE actor_id IN ($1, $2)")
            .bind(&signer.actor_id)
            .bind(&other.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(target.user.id)
            .execute(&pool)
            .await;
    }

    /// Locked account: an inbound Follow queues a pending follow_request and
    /// sends NO Accept; the actor doc advertises manuallyApprovesFollowers.
    #[tokio::test]
    async fn follow_on_locked_account_queues_request_without_accept() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = ap_router(state);

        let target = make_local_actor(&pool, &domain).await;
        // Lock the account.
        crate::db::users::UserRow::set_manually_approves_followers(&pool, target.user.id, true)
            .await
            .expect("lock account");

        let s = unique();
        let signer = make_signer(&format!("https://remote.test/users/lk_{s}"));
        seed_remote_actor(&pool, &signer, &format!("{}/inbox", signer.actor_id), None).await;

        let follow_id = format!("{}/activities/follow-{s}", signer.actor_id);
        let follow = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": follow_id,
            "type": "Follow",
            "actor": signer.actor_id,
            "object": target.actor_id,
        });
        let path = format!("/@{}/inbox", target.username);
        let (status, _) = post_signed(&app, &path, &follow, &signer, &domain).await;
        assert_eq!(status, 202, "Follow accepted (queued)");

        // A pending follow_request exists; no followers row accepted; no Accept sent.
        let requests: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM follow_requests WHERE fan_out = $1 AND user_id = $2 AND decision IS NULL",
        )
        .bind(&signer.actor_id)
        .bind(target.user.id)
        .fetch_one(&pool).await.expect("count requests");
        assert_eq!(requests, 1, "locked account queues a pending follow_request");

        let accepted: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM followers WHERE actor_id = $1 AND user_id = $2 AND accepted = true",
        )
        .bind(&signer.actor_id)
        .bind(target.user.id)
        .fetch_one(&pool).await.expect("count followers");
        assert_eq!(accepted, 0, "no accepted follower row while pending");

        let accepts: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ap_activities WHERE activity_type = 'Accept' AND actor_id = $1",
        )
        .bind(&target.actor_id)
        .fetch_one(&pool).await.expect("count accepts");
        assert_eq!(accepts, 0, "no Accept is sent while the request is pending");

        // Cleanup.
        let _ = sqlx::query("DELETE FROM follow_requests WHERE user_id = $1")
            .bind(target.user.id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM ap_remote_actors WHERE actor_id = $1")
            .bind(&signer.actor_id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id IN ($1, $2)")
            .bind(&signer.actor_id).bind(&target.actor_id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(target.user.id).execute(&pool).await;
    }

    /// (follow_notifications, follow_request_notifications) written for a user.
    async fn notif_counts(pool: &sqlx::PgPool, user_id: uuid::Uuid) -> (i64, i64) {
        let follow: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND type = 'follow'",
        )
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("count follow notifications");
        let request: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND type = 'follow_request'",
        )
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("count follow_request notifications");
        (follow, request)
    }

    /// A signed Follow activity from `actor` to `target`, unique per `suffix`.
    fn follow_json(actor: &str, target: &str, suffix: &str) -> Value {
        json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": format!("{actor}/activities/follow-{suffix}"),
            "type": "Follow",
            "actor": actor,
            "object": target,
        })
    }

    /// Accepted follower rows for (remote actor, local user).
    async fn follower_count(pool: &sqlx::PgPool, actor: &str, user_id: uuid::Uuid) -> i64 {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM followers WHERE actor_id = $1 AND user_id = $2 AND accepted = true",
        )
        .bind(actor)
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("count followers")
    }

    /// The follow_requests row id for (remote actor, local user).
    async fn request_id(pool: &sqlx::PgPool, actor: &str, user_id: uuid::Uuid) -> uuid::Uuid {
        sqlx::query_scalar(
            "SELECT id FROM follow_requests WHERE fan_out = $1 AND user_id = $2",
        )
        .bind(actor)
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("request id")
    }

    /// The decision on a follow request (None = pending).
    async fn request_decision(pool: &sqlx::PgPool, id: uuid::Uuid) -> Option<String> {
        sqlx::query_scalar("SELECT decision FROM follow_requests WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .expect("request decision")
    }

    /// POST to the real /api/notifications handlers with a user JWT.
    async fn api_post(app: &axum::Router, path: &str, token: &str) -> (u16, Value) {
        let req = axum::http::Request::builder()
            .method("POST")
            .uri(path)
            .header("authorization", format!("Bearer {token}"))
            .header("host", "bytescape.test")
            .body(axum::body::Body::from("{}"))
            .expect("api request");
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        let status = resp.status().as_u16();
        let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024)
            .await
            .unwrap_or_default();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    /// Every inbound follow on an unlocked account announces itself: exactly
    /// one TYPE_FOLLOW notification with the signer's provenance — including
    /// replays (dedupe is per activity id).
    #[tokio::test]
    async fn follow_writes_follow_notification() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = ap_router(state);

        let target = make_local_actor(&pool, &domain).await;
        crate::db::users::UserRow::set_manually_approves_followers(&pool, target.user.id, false)
            .await
            .expect("unlock account");
        let s = unique();
        let signer = make_signer(&format!("https://remote.test/users/notify_{s}"));
        seed_remote_actor(&pool, &signer, &format!("{}/inbox", signer.actor_id), None).await;

        let follow_id = format!("{}/activities/follow-{s}", signer.actor_id);
        let follow = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": follow_id,
            "type": "Follow",
            "actor": signer.actor_id,
            "object": target.actor_id,
        });
        let path = format!("/@{}/inbox", target.username);
        let (status, _) = post_signed(&app, &path, &follow, &signer, &domain).await;
        assert_eq!(status, 202, "Follow accepted");

        let (follows, requests) = notif_counts(&pool, target.user.id).await;
        assert_eq!(follows, 1, "accepted inbound follow writes one follow notification");
        assert_eq!(requests, 0, "no request notification for an unlocked account");

        // Provenance: the acting remote actor and its network.
        let (actor_id, network, request_actor): (String, String, String) = sqlx::query_as(
            "SELECT actor_id, network, data->>'request_actor' FROM notifications \
             WHERE user_id = $1 AND type = 'follow'",
        )
        .bind(target.user.id)
        .fetch_one(&pool)
        .await
        .expect("notification provenance");
        assert_eq!(actor_id, signer.actor_id, "notification carries the remote signer");
        assert_eq!(network, "activitypub", "notification carries the AP network");
        assert_eq!(request_actor, signer.actor_id, "data.request_actor is the signer");

        // Replay: still exactly one notification.
        let (status2, _) = post_signed(&app, &path, &follow, &signer, &domain).await;
        assert_eq!(status2, 202, "replay acknowledged");
        let (follows, _) = notif_counts(&pool, target.user.id).await;
        assert_eq!(follows, 1, "replay writes no second notification");

        // Cleanup (notifications + follower cascade from the user row).
        let _ = sqlx::query("DELETE FROM notifications WHERE user_id = $1")
            .bind(target.user.id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM followers WHERE actor_id = $1")
            .bind(&signer.actor_id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id IN ($1, $2)")
            .bind(&signer.actor_id).bind(&target.actor_id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM ap_remote_actors WHERE actor_id = $1")
            .bind(&signer.actor_id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(target.user.id).execute(&pool).await;
    }

    /// The owner-QA gap: remotes cache alias actor URLs (we publish
    /// `/users/{name}` and `/actors/{name}` alongside the canonical `/@{name}`)
    /// and send the Follow's `object` in that cached form. The follow must
    /// resolve to the same local user and announce itself exactly the same —
    /// not 404 out before the writer runs.
    #[tokio::test]
    async fn follow_to_alias_object_url_writes_notification() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = ap_router(state);

        let target = make_local_actor(&pool, &domain).await;
        crate::db::users::UserRow::set_manually_approves_followers(&pool, target.user.id, false)
            .await
            .expect("unlock account");

        // Two alias deliveries from two remote actors: the /users/ form (the
        // classic cached Mastodon id) and the /actors/ form with a trailing
        // slash (normalization noise seen in the wild).
        let path = format!("/@{}/inbox", target.username);
        for form in [
            format!("https://{}/users/{}", domain, target.username),
            format!("https://{}/actors/{}/", domain, target.username),
        ] {
            let sub = unique();
            let remote = make_signer(&format!("https://remote.test/users/alias_{sub}"));
            seed_remote_actor(&pool, &remote, &format!("{}/inbox", remote.actor_id), None).await;
            let follow = json!({
                "@context": "https://www.w3.org/ns/activitystreams",
                "id": format!("{}/activities/follow-{sub}", remote.actor_id),
                "type": "Follow",
                "actor": remote.actor_id,
                "object": form,
            });
            let (status, body) = post_signed(&app, &path, &follow, &remote, &domain).await;
            assert_eq!(status, 202, "alias-object Follow accepted for {form}, got {body}");

            let accepted: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM followers WHERE actor_id = $1 AND user_id = $2 AND accepted = true",
            )
            .bind(&remote.actor_id)
            .bind(target.user.id)
            .fetch_one(&pool)
            .await
            .expect("count followers");
            assert_eq!(accepted, 1, "alias-object follow resolves to the local user ({form})");

            let (follows, _) = notif_counts(&pool, target.user.id).await;
            assert_eq!(follows, 1, "alias-object follow writes its follow notification ({form})");

            let _ = sqlx::query("DELETE FROM ap_remote_actors WHERE actor_id = $1")
                .bind(&remote.actor_id).execute(&pool).await;
            let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id IN ($1, $2)")
                .bind(&remote.actor_id).bind(&target.actor_id).execute(&pool).await;
            let _ = sqlx::query("DELETE FROM followers WHERE actor_id = $1")
                .bind(&remote.actor_id).execute(&pool).await;
            let _ = sqlx::query("DELETE FROM notifications WHERE user_id = $1")
                .bind(target.user.id).execute(&pool).await;
        }

        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(target.user.id).execute(&pool).await;
    }

    /// Locked account: each new follow activity announces a follow_request;
    /// a re-follow after a deny re-opens the request and re-notifies.
    #[tokio::test]
    async fn follow_on_locked_account_notifies_and_refollow_renotifies() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = ap_router(state);

        let target = make_local_actor(&pool, &domain).await;
        crate::db::users::UserRow::set_manually_approves_followers(&pool, target.user.id, true)
            .await
            .expect("lock account");
        let s = unique();
        let signer = make_signer(&format!("https://remote.test/users/lknotify_{s}"));
        seed_remote_actor(&pool, &signer, &format!("{}/inbox", signer.actor_id), None).await;

        let path = format!("/@{}/inbox", target.username);
        let follow1 = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": format!("{}/activities/follow-{s}-1", signer.actor_id),
            "type": "Follow",
            "actor": signer.actor_id,
            "object": target.actor_id,
        });
        let (status, _) = post_signed(&app, &path, &follow1, &signer, &domain).await;
        assert_eq!(status, 202, "locked Follow queued");
        let (follows, requests) = notif_counts(&pool, target.user.id).await;
        assert_eq!((follows, requests), (0, 1), "locked follow writes a follow_request notification");

        // Deny the request the way the API does, then re-follow with a fresh
        // activity id: the request must re-open and re-notify.
        let _ = sqlx::query(
            "UPDATE follow_requests SET decision = 'denied', resolved_at = now() \
             WHERE fan_out = $1 AND user_id = $2",
        )
        .bind(&signer.actor_id)
        .bind(target.user.id)
        .execute(&pool)
        .await;
        let follow2 = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": format!("{}/activities/follow-{s}-2", signer.actor_id),
            "type": "Follow",
            "actor": signer.actor_id,
            "object": target.actor_id,
        });
        let (status, _) = post_signed(&app, &path, &follow2, &signer, &domain).await;
        assert_eq!(status, 202, "re-follow queued");
        let pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM follow_requests WHERE fan_out = $1 AND user_id = $2 AND decision IS NULL",
        )
        .bind(&signer.actor_id)
        .bind(target.user.id)
        .fetch_one(&pool)
        .await
        .expect("count pending");
        assert_eq!(pending, 1, "re-follow re-opens the request as pending");
        let (_, requests) = notif_counts(&pool, target.user.id).await;
        assert_eq!(requests, 2, "re-follow writes a fresh follow_request notification");

        // Replay of the second activity adds nothing.
        let (status, _) = post_signed(&app, &path, &follow2, &signer, &domain).await;
        assert_eq!(status, 202, "replay acknowledged");
        let (_, requests) = notif_counts(&pool, target.user.id).await;
        assert_eq!(requests, 2, "replay writes no third notification");

        // Cleanup.
        let _ = sqlx::query("DELETE FROM notifications WHERE user_id = $1")
            .bind(target.user.id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM follow_requests WHERE user_id = $1")
            .bind(target.user.id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM ap_remote_actors WHERE actor_id = $1")
            .bind(&signer.actor_id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id IN ($1, $2)")
            .bind(&signer.actor_id).bind(&target.actor_id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(target.user.id).execute(&pool).await;
    }

    /// Owner QA 2026-11-17: follows delivered to an alias-FORM inbox URL
    /// (`/users/{u}/inbox`, `/actors/{u}/inbox` — remotes cache our published
    /// alias actor ids and derive the inbox from them) must land exactly like
    /// the canonical one. Before the route existed these POSTs 404'd and the
    /// follow vanished: no activity row, no request, no notification.
    #[tokio::test]
    async fn follow_to_alias_form_inbox_is_accepted_and_notifies() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = ap_router(state);

        let target = make_local_actor(&pool, &domain).await;
        let paths = [
            format!("/users/{}/inbox", target.username),
            format!("/actors/{}/inbox", target.username),
        ];
        for (i, path) in paths.iter().enumerate() {
            let s = unique();
            let signer = make_signer(&format!("https://remote.test/users/aliasinbox_{s}"));
            seed_remote_actor(&pool, &signer, &format!("{}/inbox", signer.actor_id), None).await;
            let follow = json!({
                "@context": "https://www.w3.org/ns/activitystreams",
                "id": format!("{}/activities/follow-{s}", signer.actor_id),
                "type": "Follow",
                "actor": signer.actor_id,
                "object": target.actor_id,
            });
            let (status, body) = post_signed(&app, path, &follow, &signer, &domain).await;
            assert_eq!(status, 202, "alias inbox {path} accepted the Follow, got {body}");

            // Locked by default (migration 102): every delivered follow
            // queues a pending request and writes its notification.
            let requests: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM follow_requests WHERE fan_out = $1 AND user_id = $2 AND decision IS NULL",
            )
            .bind(&signer.actor_id)
            .bind(target.user.id)
            .fetch_one(&pool)
            .await
            .expect("count requests");
            assert_eq!(requests, 1, "alias inbox {path} queued the follow request");

            let notifs: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND type = 'follow_request'",
            )
            .bind(target.user.id)
            .fetch_one(&pool)
            .await
            .expect("count notifications");
            assert_eq!(notifs, (i + 1) as i64, "alias inbox {path} wrote its notification");

            let _ = sqlx::query("DELETE FROM ap_remote_actors WHERE actor_id = $1")
                .bind(&signer.actor_id).execute(&pool).await;
        }

        let _ = sqlx::query("DELETE FROM notifications WHERE user_id = $1")
            .bind(target.user.id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM follow_requests WHERE user_id = $1")
            .bind(target.user.id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id = $1")
            .bind(&target.actor_id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(target.user.id).execute(&pool).await;
    }

    /// THE production repro (owner verdict 2026-11-18 "still not getting any
    /// notifications for follow requests"): a Follow signed over the standard
    /// Mastodon wire format with the actor's SPKI public key (`BEGIN PUBLIC
    /// KEY` — what Mastodon and virtually every AP implementation publish in
    /// actor documents) must verify. The public-key parse accepted PKCS#1
    /// (`BEGIN RSA PUBLIC KEY`) only, and its failure surfaced as the generic
    /// "signature verification failed" seen in the live rejection logs:
    /// every real remote delivery died while self-keyed tests passed.
    #[tokio::test]
    async fn mastodon_spki_signed_follow_is_accepted_and_notifies() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = ap_router(state);

        let target = make_local_actor(&pool, &domain).await;
        let s = unique();
        let signer = make_signer(&format!("https://mastodon.social/users/owner_{s}"));
        // The remote publishes its key as SPKI — the real-world format.
        let spki = spki_pem(&signer.public_pem);
        assert!(spki.contains("BEGIN PUBLIC KEY"), "SPKI form seeded");
        seed_remote_actor_pem(
            &pool,
            &signer,
            &format!("{}/inbox", signer.actor_id),
            None,
            &spki,
        )
        .await;

        let follow = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": format!("{}/activities/follow-{s}", signer.actor_id),
            "type": "Follow",
            "actor": signer.actor_id,
            "object": target.actor_id,
        });
        let path = format!("/@{}/inbox", target.username);
        let (status, body) = post_signed(&app, &path, &follow, &signer, &domain).await;
        assert_eq!(
            status, 202,
            "SPKI-keyed Mastodon-format Follow must be accepted, got {body}"
        );

        // Locked by default: a pending request + its notification.
        let requests: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM follow_requests WHERE fan_out = $1 AND user_id = $2 AND decision IS NULL",
        )
        .bind(&signer.actor_id)
        .bind(target.user.id)
        .fetch_one(&pool)
        .await
        .expect("count requests");
        assert_eq!(requests, 1, "SPKI-keyed follow queues the request");
        let (follows, reqs) = notif_counts(&pool, target.user.id).await;
        assert_eq!((follows, reqs), (0, 1), "SPKI-keyed follow writes the notification");

        // Actor resolution: the notification carries the remote's display
        // handle (preferred_username from the actor cache), not the raw URL.
        let handle: String = sqlx::query_scalar(
            "SELECT data->>'actor_handle' FROM notifications WHERE user_id = $1 AND type = 'follow_request'",
        )
        .bind(target.user.id)
        .fetch_one(&pool)
        .await
        .expect("notification handle");
        assert_eq!(
            handle,
            format!("owner_{s}"),
            "notification resolves the remote display handle"
        );

        let _ = sqlx::query("DELETE FROM notifications WHERE user_id = $1")
            .bind(target.user.id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM follow_requests WHERE user_id = $1")
            .bind(target.user.id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id = $1")
            .bind(&target.actor_id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM ap_remote_actors WHERE actor_id = $1")
            .bind(&signer.actor_id).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(target.user.id).execute(&pool).await;
    }

    /// The whole follow matrix through the real delivery + approval paths:
    /// unlocked auto-accept, locked → approve (real API), locked → deny
    /// (real API), re-follow after deny. This is the owner flow end to end:
    /// signed Follow → request + notification → the UI's approve/deny.
    #[tokio::test]
    async fn follow_matrix_locked_unlocked_approve_deny_refollow() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let inbox_app = ap_router(state.clone());
        let api_app = axum::Router::new()
            .merge(crate::api::notifications::routes())
            .with_state(state.clone());

        // ── Unlocked: auto-accept → follower row + follow notification.
        let unlocked = make_local_actor(&pool, &domain).await;
        crate::db::users::UserRow::set_manually_approves_followers(&pool, unlocked.user.id, false)
            .await
            .expect("unlock");
        let s1 = unique();
        let r1 = make_signer(&format!("https://remote.test/users/mx_open_{s1}"));
        seed_remote_actor(&pool, &r1, &format!("{}/inbox", r1.actor_id), None).await;
        let f1 = follow_json(&r1.actor_id, &unlocked.actor_id, &s1);
        let (st, body) = post_signed(
            &inbox_app,
            &format!("/@{}/inbox", unlocked.username),
            &f1,
            &r1,
            &domain,
        )
        .await;
        assert_eq!(st, 202, "unlocked follow accepted: {body}");
        assert_eq!(follower_count(&pool, &r1.actor_id, unlocked.user.id).await, 1, "unlocked: follower row");
        assert_eq!(notif_counts(&pool, unlocked.user.id).await, (1, 0), "unlocked: follow notification");

        // ── Locked (the migration-102 default) + approve via the real API.
        let target = make_local_actor(&pool, &domain).await;
        let s2 = unique();
        let r2 = make_signer(&format!("https://remote.test/users/mx_appr_{s2}"));
        seed_remote_actor(&pool, &r2, &format!("{}/inbox", r2.actor_id), None).await;
        let f2 = follow_json(&r2.actor_id, &target.actor_id, &s2);
        let (st, body) = post_signed(
            &inbox_app,
            &format!("/@{}/inbox", target.username),
            &f2,
            &r2,
            &domain,
        )
        .await;
        assert_eq!(st, 202, "locked follow queued: {body}");
        assert_eq!(follower_count(&pool, &r2.actor_id, target.user.id).await, 0, "locked: no follower yet");
        assert_eq!(notif_counts(&pool, target.user.id).await, (0, 1), "locked: follow_request notification");
        let req2 = request_id(&pool, &r2.actor_id, target.user.id).await;
        let token = crate::auth::create_token(target.user.id, &target.username, &state.config.jwt_secret)
            .expect("jwt");
        let (st, body) = api_post(
            &api_app,
            &format!("/api/notifications/follow-requests/{req2}/approve"),
            &token,
        )
        .await;
        assert_eq!(st, 200, "approve endpoint: {body}");
        assert_eq!(follower_count(&pool, &r2.actor_id, target.user.id).await, 1, "approve: follower row");
        assert_eq!(request_decision(&pool, req2).await.as_deref(), Some("approved"), "approve: decision");

        // ── Locked + deny via the real API.
        let s3 = unique();
        let r3 = make_signer(&format!("https://remote.test/users/mx_deny_{s3}"));
        seed_remote_actor(&pool, &r3, &format!("{}/inbox", r3.actor_id), None).await;
        let f3 = follow_json(&r3.actor_id, &target.actor_id, &s3);
        let (st, body) = post_signed(
            &inbox_app,
            &format!("/@{}/inbox", target.username),
            &f3,
            &r3,
            &domain,
        )
        .await;
        assert_eq!(st, 202, "second locked follow queued: {body}");
        let req3 = request_id(&pool, &r3.actor_id, target.user.id).await;
        let (st, body) = api_post(
            &api_app,
            &format!("/api/notifications/follow-requests/{req3}/deny"),
            &token,
        )
        .await;
        assert_eq!(st, 200, "deny endpoint: {body}");
        assert_eq!(follower_count(&pool, &r3.actor_id, target.user.id).await, 0, "deny: still no follower");
        assert_eq!(request_decision(&pool, req3).await.as_deref(), Some("denied"), "deny: decision");

        // ── Re-follow after deny: request re-opens + a fresh notification.
        let f4 = follow_json(&r3.actor_id, &target.actor_id, &format!("{s3}b"));
        let (st, body) = post_signed(
            &inbox_app,
            &format!("/@{}/inbox", target.username),
            &f4,
            &r3,
            &domain,
        )
        .await;
        assert_eq!(st, 202, "re-follow after deny queued: {body}");
        assert_eq!(request_decision(&pool, req3).await, None, "re-follow: request pending again");
        assert_eq!(notif_counts(&pool, target.user.id).await, (0, 3), "one request notification per delivery");

        // Cleanup.
        let actors = vec![r1.actor_id.clone(), r2.actor_id.clone(), r3.actor_id.clone()];
        let _ = sqlx::query("DELETE FROM ap_remote_actors WHERE actor_id = ANY($1)")
            .bind(&actors).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id = ANY($1)")
            .bind(&actors).execute(&pool).await;
        for owner in [unlocked.user.id, target.user.id] {
            let _ = sqlx::query("DELETE FROM notifications WHERE user_id = $1")
                .bind(owner).execute(&pool).await;
            let _ = sqlx::query("DELETE FROM follow_requests WHERE user_id = $1")
                .bind(owner).execute(&pool).await;
            let _ = sqlx::query("DELETE FROM followers WHERE user_id = $1")
                .bind(owner).execute(&pool).await;
        }
        let _ = sqlx::query("DELETE FROM users WHERE id = ANY($1)")
            .bind(vec![unlocked.user.id, target.user.id]).execute(&pool).await;
    }

    /// Owner QA follow-up: rejections leave a journal-INDEPENDENT record —
    /// journald rate-limits under AppView log storms and dropped rejection
    /// lines in the 2026-11-18 window. The row carries path + reason + a
    /// payload digest, never the payload.
    #[tokio::test]
    async fn rejected_delivery_is_recorded_in_ap_inbox_rejections() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = ap_router(state);

        let target = make_local_actor(&pool, &domain).await;
        let path = format!("/@{}/inbox", target.username);

        // Unsigned POST → the reject path.
        let req = axum::http::Request::builder()
            .method("POST")
            .uri(&path)
            .header("host", &domain)
            .header("content-type", "application/activity+json")
            .body(axum::body::Body::from("{\"type\":\"Follow\"}"))
            .expect("request");
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status().as_u16(), 401, "unsigned delivery rejected");

        // The record: path + signature reason + digest, journal-independent.
        let (count, reason): (i64, String) = sqlx::query_as(
            "SELECT count(*), min(reason) FROM ap_inbox_rejections WHERE path = $1",
        )
        .bind(&path)
        .fetch_one(&pool)
        .await
        .expect("rejection record");
        assert!(count >= 1, "rejection recorded in ap_inbox_rejections");
        assert!(
            reason.contains("signature"),
            "record carries the rejection reason, got: {reason}"
        );
        let digest: String = sqlx::query_scalar(
            "SELECT body_digest FROM ap_inbox_rejections WHERE path = $1 LIMIT 1",
        )
        .bind(&path)
        .fetch_one(&pool)
        .await
        .expect("digest");
        assert_eq!(digest.len(), 16, "8-byte sha256 hex prefix recorded");

        let _ = sqlx::query("DELETE FROM ap_inbox_rejections WHERE path = $1")
            .bind(&path).execute(&pool).await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(target.user.id).execute(&pool).await;
    }
}