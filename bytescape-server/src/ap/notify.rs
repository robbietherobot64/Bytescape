//! Notification writers (Phase 5).
//!
//! One insert per social event, deduped by origin id (`dedupe_key`): a
//! redelivered federated activity never creates a second row. Each row carries
//! the acting actor and its network ('activitypub' | 'atproto' | 'local').

use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::notifications::NotificationRow;

/// The acting actor's display identity for notification rendering.
#[derive(Debug, Clone)]
pub struct ActorRef {
    /// Stable id: remote AP actor uri, AT DID, or local username.
    pub id: String,
    /// Display handle (best-effort; falls back to the id).
    pub handle: String,
    pub network: &'static str,
}

impl ActorRef {
    pub fn activitypub(actor_uri: &str) -> Self {
        Self { id: actor_uri.to_string(), handle: actor_uri.to_string(), network: "activitypub" }
    }
    pub fn atproto(did: &str, handle: &str) -> Self {
        Self { id: did.to_string(), handle: handle.to_string(), network: "atproto" }
    }
    pub fn local(username: &str) -> Self {
        Self { id: username.to_string(), handle: username.to_string(), network: "local" }
    }
}

/// The acting remote AP actor with its display handle resolved from the
/// remote actor cache (`preferred_username`), falling back to the last path
/// segment of the actor uri, then the uri itself. Notifications must show
/// `@name`, never the raw actor URL (owner QA: the row key and dedupe stay
/// the stable actor uri — only the display handle is resolved).
pub async fn remote_actor_ref(pool: &PgPool, actor_uri: &str) -> ActorRef {
    let cached: Option<String> = sqlx::query_scalar(
        "SELECT preferred_username FROM ap_remote_actors WHERE actor_id = $1",
    )
    .bind(actor_uri)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let handle = cached
        .or_else(|| {
            actor_uri
                .rsplit('/')
                .next()
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| actor_uri.to_string());
    ActorRef { id: actor_uri.to_string(), handle, network: "activitypub" }
}

/// Notification type vocabulary (spec: like, repost, follow, reply, mention,
/// profile_comment + follow_request for the locked-account request flow).
pub const TYPE_LIKE: &str = "like";
pub const TYPE_REPOST: &str = "repost";
pub const TYPE_FOLLOW: &str = "follow";
pub const TYPE_FOLLOW_REQUEST: &str = "follow_request";
pub const TYPE_REPLY: &str = "reply";
pub const TYPE_MENTION: &str = "mention";
pub const TYPE_COMMENT: &str = "profile_comment";

/// Create a notification, deduped by `dedupe_key`. Returns the row, or None
/// when this event was already recorded.
#[allow(clippy::too_many_arguments)]
pub async fn notify<'e, E>(
    pool: E,
    user_id: Uuid,
    notification_type: &str,
    title: &str,
    body: Option<&str>,
    data: Option<Value>,
    dedupe_key: &str,
    actor: &ActorRef,
) -> Result<Option<NotificationRow>, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    let mut data = data.unwrap_or_else(|| Value::Object(Default::default()));
    if let Value::Object(map) = &mut data {
        map.insert("actor_handle".into(), Value::String(actor.handle.clone()));
        map.insert("network".into(), Value::String(actor.network.to_string()));
    }
    NotificationRow::create_deduped(
        pool,
        user_id,
        notification_type,
        title,
        body,
        Some(data),
        dedupe_key,
        Some(&actor.id),
        actor.network,
    )
    .await
}

/// The local user who owns an AP actor id (used by inbox writers to route
/// notifications to the right account).
pub async fn local_user_for_actor(
    pool: &PgPool,
    actor_id: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM users WHERE ap_actor_id = $1 AND ap_enabled = true",
    )
    .bind(actor_id)
    .fetch_optional(pool)
    .await
}

/// Notify the local owner of an at:// record when a Bluesky/AT actor interacts
/// with it (like, repost, reply). Resolves the record's author DID to a local
/// user and writes one deduped notification. `record_uri` is the liked/replied
/// record (`at://did/collection/rkey`); `acting_did` is the interacting user.
pub async fn notify_at_record_interaction(
    pool: &PgPool,
    record_uri: &str,
    acting_did: &str,
    notification_type: &str,
    dedupe_key: &str,
) -> Result<(), sqlx::Error> {
    let Some((owner_id, _owner_name)) = owner_of_at_record(pool, record_uri).await? else {
        return Ok(());
    };
    let handle = actor_handle(pool, acting_did).await?;
    let actor = ActorRef::atproto(acting_did, &handle);
    let title = match notification_type {
        TYPE_LIKE => "New like",
        TYPE_REPOST => "New repost",
        TYPE_REPLY => "New reply",
        _ => "New interaction",
    };
    let verb = match notification_type {
        TYPE_LIKE => "liked your post",
        TYPE_REPOST => "reposted your post",
        TYPE_REPLY => "replied to your post",
        _ => "interacted with your post",
    };
    notify(
        pool,
        owner_id,
        notification_type,
        title,
        Some(&format!("{} {}", handle, verb)),
        Some(serde_json::json!({ "subject_uri": record_uri })),
        dedupe_key,
        &actor,
    )
    .await?;
    Ok(())
}

/// Notify the local account owner of an inbound Bluesky/AT follow. AT has no
/// locked accounts — when the target is locked the follow is held as a pending
/// request locally (UI-only gating, disclosed in UI copy).
pub async fn notify_at_follow(
    pool: &PgPool,
    target_did: &str,
    acting_did: &str,
    follow_record_uri: &str,
) -> Result<(), sqlx::Error> {
    let Some(owner_id) = local_user_for_did(pool, target_did).await? else {
        return Ok(());
    };
    let locked: bool = sqlx::query_scalar(
        "SELECT manually_approves_followers FROM users WHERE id = $1",
    )
    .bind(owner_id)
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    let handle = actor_handle(pool, acting_did).await?;
    let actor = ActorRef::atproto(acting_did, &handle);
    if locked {
        crate::db::follow_requests::FollowRequestRow::create(
            pool,
            acting_did,
            owner_id,
            None,
            Some(follow_record_uri),
            "atproto",
        )
        .await?;
        notify(
            pool,
            owner_id,
            TYPE_FOLLOW_REQUEST,
            "Follow request",
            Some(&format!("{} wants to follow you", handle)),
            Some(serde_json::json!({ "request_actor": acting_did })),
            follow_record_uri,
            &actor,
        )
        .await?;
    } else {
        notify(
            pool,
            owner_id,
            TYPE_FOLLOW,
            "New follower",
            Some(&format!("{} followed you", handle)),
            Some(serde_json::json!({ "request_actor": acting_did })),
            follow_record_uri,
            &actor,
        )
        .await?;
    }
    Ok(())
}

/// The local user (id, username) who owns an at:// record (by the record's
/// repo DID). Used to route notifications to the right account with a display
/// name for the actor.
pub async fn owner_of_at_record(
    pool: &PgPool,
    record_uri: &str,
) -> Result<Option<(Uuid, String)>, sqlx::Error> {
    let repo_did = record_uri
        .strip_prefix("at://")
        .and_then(|s| s.split('/').next())
        .unwrap_or("");
    if repo_did.is_empty() {
        return Ok(None);
    }
    sqlx::query_as::<_, (Uuid, String)>(
        "SELECT u.id, u.username FROM users u \
         JOIN linked_accounts la ON la.user_id = u.id \
         WHERE la.provider = 'bluesky' AND la.external_id = $1 LIMIT 1",
    )
    .bind(repo_did)
    .fetch_optional(pool)
    .await
}

/// The local user linked to a Bluesky/AT DID.
async fn local_user_for_did(
    pool: &PgPool,
    did: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM linked_accounts WHERE provider = 'bluesky' AND external_id = $1 LIMIT 1",
    )
    .bind(did)
    .fetch_optional(pool)
    .await
}

/// Best-effort display handle for an AT DID (appview_did_handle cache).
async fn actor_handle(pool: &PgPool, did: &str) -> Result<String, sqlx::Error> {
    let handle: Option<String> = sqlx::query_scalar(
        "SELECT handle FROM appview_did_handle WHERE did = $1",
    )
    .bind(did)
    .fetch_optional(pool)
    .await?;
    Ok(handle.unwrap_or_else(|| did.to_string()))
}
