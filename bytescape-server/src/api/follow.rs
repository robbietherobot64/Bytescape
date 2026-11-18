//! API — Following list with source badge info.

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{auth, db, error::AppError, AppState};

// ─────────────────────────────────────────────────────────────────────────────
// FollowService (identity-graph Phase 2) — one follow/unfollow write path with
// per-network adapters, keyed by (caller identity, target network):
//
//   AT target    → `createRecord` via the Bluesky-linked account
//   AP target    → home-REST follow for Mastodon-linked callers (their AP
//                  identity IS their home actor — "one account, one person";
//                  NO silent fallback to a minted actor, a stale grant is a
//                  visible re-link error) or the minted-actor AP `Follow`
//                  (`ap::bridge::ui_follow`) otherwise
//   local target → `follows` row
//
// Every write records `network` + `identity_used` so undo routes back through
// the same adapter: `follows.follow_type` is the network/adapter marker
// (`native` | `bluesky_import` | `ap` | `mastodon_import` — the last is the
// home-graph shape `follow_import` mirrors, so re-imports self-heal the row)
// and `follows.source_platform` carries the exact identity (AT DID, minted
// actor URL, home acct).
// ─────────────────────────────────────────────────────────────────────────────

/// Where the follow target lives, decided from the handle + the UI's source
/// label (the label comes from the post's network; the handle form wins on
/// conflicts — a `user@host` on our own domain is local no matter what).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TargetNetwork {
    /// Bluesky handle (AT Protocol).
    At(String),
    /// Remote fediverse actor, `user@host`.
    Ap(String),
    /// Local Bytescape username.
    Local(String),
}

pub(crate) fn classify_target(
    handle: &str,
    source: &str,
    domain: &str,
) -> Result<TargetNetwork, AppError> {
    let handle = handle.trim().trim_start_matches('@');
    let source = source.to_lowercase();
    if source.starts_with("bluesky") {
        return Ok(TargetNetwork::At(handle.to_string()));
    }
    if let Some((user, host)) = handle.split_once('@') {
        if user.is_empty() || host.is_empty() {
            return Err(AppError::BadRequest(format!("bad handle {handle}")));
        }
        if host.eq_ignore_ascii_case(domain) {
            return Ok(TargetNetwork::Local(user.to_string()));
        }
        return Ok(TargetNetwork::Ap(handle.to_string()));
    }
    let ap_source = ["ap", "activitypub", "mastodon", "lemmy", "pixelfed", "misskey"]
        .iter()
        .any(|p| source.starts_with(p));
    if ap_source {
        // The UI labelled the target remote but gave a bare name — remote
        // resolution needs the full handle@host; never guess a host.
        return Err(AppError::BadRequest(format!(
            "following {handle} needs the full handle@host"
        )));
    }
    Ok(TargetNetwork::Local(handle.to_string()))
}

/// The adapter a follow write goes through — the routing decision, isolated
/// so the (caller, target) matrix is unit-testable.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum Adapter {
    AtCreateRecord,
    ApHomeRest,
    ApMinted,
    LocalRow,
}

/// What the caller's linked identities can act as.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct CallerCaps {
    pub bluesky: bool,
    pub mastodon: bool,
}

/// Adapter selection. A Mastodon-linked caller's AP identity is its home
/// actor (design.md Decision 5) — the home-REST adapter wins and a stale
/// grant fails loudly; the minted path is only for callers without a
/// Mastodon link, so a follow is never silently attributed to a second
/// identity.
pub(crate) fn select_adapter(
    target: &TargetNetwork,
    caps: CallerCaps,
) -> Result<Adapter, AppError> {
    match target {
        TargetNetwork::At(_) if caps.bluesky => Ok(Adapter::AtCreateRecord),
        TargetNetwork::At(_) => Err(AppError::NotFound("No Bluesky account linked".into())),
        TargetNetwork::Ap(_) if caps.mastodon => Ok(Adapter::ApHomeRest),
        TargetNetwork::Ap(_) => Ok(Adapter::ApMinted),
        TargetNetwork::Local(_) => Ok(Adapter::LocalRow),
    }
}

/// Viewer's follow state per target identity — ONE batch over `follows`,
/// keyed by exact source URL (value: row still pending). Shared by
/// `enrich_with_follow_status` (post follow badges) and the merged
/// followers/following rows so a post author, a comment author, and a graph
/// row by the same actor resolve identically.
pub(crate) async fn viewer_follow_state_map(
    db: &crate::db::DbPool,
    user_id: Uuid,
) -> std::collections::HashMap<String, bool> {
    let Ok(follows) = db::follows::FollowRow::list_by_user(db, user_id).await else {
        return Default::default();
    };
    follows
        .into_iter()
        .map(|f| (f.followed_actor, f.status == "pending"))
        .collect()
}

/// The home instance's collections-visibility for a Mastodon-linked account
/// (identity-graph Phase 6). The HOME value is authoritative: read via
/// `verify_credentials`, cached in `linked_accounts.metadata`
/// (`hide_collections` + `hide_checked_at`, 1h TTL). Returns
/// `(instance, Option<bool>)` where `None` = unknown (read failed or no
/// cached value); callers decide the conservative direction per surface.
pub(crate) async fn home_hide_state(
    state: &AppState,
    user_id: Uuid,
) -> Option<(String, Option<bool>)> {
    let acct = crate::api::feed::linked_mastodon_account(&state.db, user_id)
        .await
        .ok()??;
    let instance = crate::api::feed::home_instance_of(&acct).ok()?;
    let cached = acct.metadata.get("hide_collections").and_then(|v| v.as_bool());
    let checked = acct
        .metadata
        .get("hide_checked_at")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&chrono::Utc));
    let stale = checked
        .map(|t| t < chrono::Utc::now() - chrono::Duration::hours(1))
        .unwrap_or(true);
    let hide = if stale {
        let http = crate::api::feed::feed_http();
        let token =
            crate::oauth::mastodon::refresh_mastodon_token(&state.db, &http, &acct).await.ok()?;
        match crate::oauth::mastodon::verify_credentials(&http, &instance, &token).await {
            Ok(me) => {
                let _ = sqlx::query(
                    "UPDATE linked_accounts SET metadata = metadata || $2::jsonb WHERE id = $1",
                )
                .bind(acct.id)
                .bind(serde_json::json!({
                    "hide_collections": me.hide_collections,
                    "hide_checked_at": chrono::Utc::now().to_rfc3339(),
                }))
                .execute(&state.db)
                .await;
                Some(me.hide_collections)
            }
            Err(_) => cached,
        }
    } else {
        cached
    };
    Some((instance, hide))
}

/// What a follow/unfollow write did — mapped to each endpoint's historic
/// response shape at the edge.
#[derive(Debug, Serialize)]
pub struct FollowOutcome {
    /// `following` | `pending` | `already_following` | `none`.
    pub status: &'static str,
    /// `atproto` | `activitypub` | `local`.
    pub network: &'static str,
    /// The caller identity the write went out as (AT DID, minted actor URL,
    /// home acct, or local username).
    pub identity_used: String,
    /// Canonical target id (AT DID, AP actor URL, or local actor URL).
    pub actor: String,
    /// True when a federated write happened (row exists either way).
    pub federated: bool,
}

async fn caller_caps(
    state: &AppState,
    user_id: Uuid,
) -> Result<(CallerCaps, Option<db::accounts::LinkedAccountRow>, Option<db::accounts::LinkedAccountRow>), AppError> {
    let bsky = crate::db::accounts::LinkedAccountRow::find_by_provider(&state.db, user_id, "bluesky")
        .await
        .map_err(AppError::Database)?
        .into_iter()
        .next();
    let mastodon = crate::api::feed::linked_mastodon_account(&state.db, user_id).await?;
    Ok((
        CallerCaps {
            bluesky: bsky.is_some(),
            mastodon: mastodon.is_some(),
        },
        bsky,
        mastodon,
    ))
}

/// Resolve `user@host` to its canonical actor via the WebFinger cache and
/// keep the cache warm (same contract as `ap::bridge::ui_follow`).
async fn resolve_ap_actor(
    state: &AppState,
    user_id: Uuid,
    handle: &str,
) -> Result<crate::ap::remote::RemoteActor, AppError> {
    let signer =
        crate::ap::keys::load_fetch_signer(&state.db, user_id, &state.config.jwt_secret).await;
    let remote = crate::ap::remote::resolve_handle(
        &state.db,
        signer.as_ref().map(|(k, p)| (k.as_str(), p.as_str())),
        handle,
    )
    .await
    .map_err(|e| AppError::Internal(format!("AP follow resolve failed: {e}")))?;
    crate::ap::remote::upsert_actor(&state.db, &remote)
        .await
        .map_err(|e| AppError::Internal(format!("upsert_actor: {e}")))?;
    Ok(remote)
}

/// Route one follow write through the right adapter and record
/// `network` + `identity_used` on the row.
pub async fn follow(
    state: &AppState,
    user_id: Uuid,
    handle: &str,
    source: &str,
) -> Result<FollowOutcome, AppError> {
    let target = classify_target(handle, source, &state.config.domain)?;
    let (caps, bsky, mastodon) = caller_caps(state, user_id).await?;
    let adapter = select_adapter(&target, caps)?;
    let http = crate::api::feed::feed_http();

    match adapter {
        Adapter::AtCreateRecord => {
            let handle = match &target {
                TargetNetwork::At(h) => h.clone(),
                _ => unreachable!("adapter/target mismatch"),
            };
            let account = bsky.ok_or_else(|| AppError::NotFound("No Bluesky account linked".into()))?;
            let subject_did = crate::oauth::bluesky::resolve_handle(&http, &handle)
                .await
                .map_err(|e| AppError::Internal(format!("Bluesky resolve failed: {e}")))?;
            crate::oauth::bluesky::follow_user(&state.db, &account, &subject_did)
                .await
                .map_err(|e| AppError::Internal(format!("Bluesky follow failed: {e}")))?;
            // The story circle and feed read the follows table, not the repo:
            // without this row a UI follow stays invisible until the next
            // OAuth re-import rebuilds rows from the repo records.
            let actor_url = format!("https://bsky.app/profile/{}", handle);
            db::follows::FollowRow::create(&state.db, user_id, &actor_url, "bluesky_import", None)
                .await?;
            db::follows::FollowRow::set_accepted(&state.db, user_id, &actor_url).await?;
            db::follows::FollowRow::set_identity(&state.db, user_id, &actor_url, &account.external_id)
                .await?;
            Ok(FollowOutcome {
                status: "following",
                network: "atproto",
                identity_used: account.external_id.clone(),
                actor: subject_did,
                federated: true,
            })
        }
        Adapter::ApHomeRest => {
            let handle = match &target {
                TargetNetwork::Ap(h) => h.clone(),
                _ => unreachable!("adapter/target mismatch"),
            };
            let acct = mastodon.ok_or_else(|| {
                AppError::NotFound("No Mastodon account linked".into())
            })?;
            // Consent gate BEFORE any write: a stale grant is the visible
            // instruction-focused re-link error, and no row is created.
            crate::oauth::mastodon::ensure_follows_scope(acct.scope.as_deref())
                .map_err(crate::api::feed::mastodon_error)?;
            let instance = crate::api::feed::home_instance_of(&acct)?;
            let token =
                crate::oauth::mastodon::refresh_mastodon_token(&state.db, &http, &acct)
                    .await
                    .map_err(crate::api::feed::mastodon_error)?;
            let account_id =
                crate::oauth::mastodon::lookup_account_id(&http, &instance, &token, &handle)
                    .await
                    .map_err(crate::api::feed::mastodon_error)?;
            let rel = crate::oauth::mastodon::home_follow_account(&http, &instance, &token, &account_id)
                .await
                .map_err(crate::api::feed::mastodon_error)?;
            // The row mirrors the home-graph shape `follow_import` writes
            // (canonical actor id), so re-imports self-heal it and the merged
            // read model sees one person.
            let remote = resolve_ap_actor(state, user_id, &handle).await?;
            let actor_url = remote.actor_id.clone();
            db::follows::FollowRow::create(&state.db, user_id, &actor_url, "mastodon_import", None)
                .await?;
            if rel["following"].as_bool() != Some(false) {
                db::follows::FollowRow::set_accepted(&state.db, user_id, &actor_url).await?;
            }
            let identity = acct.external_username.clone();
            db::follows::FollowRow::set_identity(&state.db, user_id, &actor_url, &identity).await?;
            Ok(FollowOutcome {
                status: if rel["requested"].as_bool() == Some(true) {
                    "pending"
                } else {
                    "following"
                },
                network: "activitypub",
                identity_used: identity,
                actor: actor_url,
                federated: true,
            })
        }
        Adapter::ApMinted => {
            let handle = match &target {
                TargetNetwork::Ap(h) => h.clone(),
                _ => unreachable!("adapter/target mismatch"),
            };
            // Our own actor sends the Follow; the remote Accept activates the
            // row (ap::bridge::ui_follow owns row + delivery bookkeeping).
            let actor_id = crate::ap::bridge::ui_follow(
                &state.db,
                &state.config.jwt_secret,
                user_id,
                &handle,
            )
            .await
            .map_err(|e| AppError::Internal(format!("AP follow failed: {e}")))?;
            let mine = crate::db::users::UserRow::find_by_id(&state.db, user_id)
                .await?
                .map(|u| {
                    u.ap_actor_id.clone().unwrap_or_else(|| {
                        crate::ap::actor::canonical_actor_url(&state.config.domain, &u.username)
                    })
                })
                .unwrap_or_default();
            if !mine.is_empty() {
                db::follows::FollowRow::set_identity(&state.db, user_id, &actor_id, &mine).await?;
            }
            Ok(FollowOutcome {
                status: "pending",
                network: "activitypub",
                identity_used: mine,
                actor: actor_id,
                federated: true,
            })
        }
        Adapter::LocalRow => {
            let username = match &target {
                TargetNetwork::Local(u) => u.clone(),
                _ => unreachable!("adapter/target mismatch"),
            };
            let target_user = db::users::UserRow::find_by_username(&state.db, &username)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("User {} not found", username)))?;
            let actor_url =
                format!("https://{}/@{}", state.config.domain, target_user.username);
            let existing = db::follows::FollowRow::find_by_follower_and_actor(
                &state.db,
                user_id,
                &actor_url,
            )
            .await?;
            if existing.is_some() {
                return Ok(FollowOutcome {
                    status: "already_following",
                    network: "local",
                    identity_used: String::new(),
                    actor: actor_url,
                    federated: false,
                });
            }
            db::follows::FollowRow::create(&state.db, user_id, &actor_url, "native", None).await?;
            db::follows::FollowRow::set_accepted(&state.db, user_id, &actor_url).await?;
            Ok(FollowOutcome {
                status: "following",
                network: "local",
                identity_used: String::new(),
                actor: actor_url,
                federated: false,
            })
        }
    }
}

/// Route one unfollow through the adapter that made the follow: the row's
/// `follow_type` is the undo key (`ap` → AP Undo, `mastodon_import` →
/// home-REST unfollow, `bluesky_import` → AT record delete, `native` →
/// row delete). No row → best-effort stale cleanup by handle shape.
pub async fn unfollow(
    state: &AppState,
    user_id: Uuid,
    handle: &str,
    source: &str,
) -> Result<FollowOutcome, AppError> {
    let target = classify_target(handle, source, &state.config.domain)?;
    let (caps, bsky, _mastodon) = caller_caps(state, user_id).await?;
    let _ = select_adapter(&target, caps)?; // same capability gates as follow
    let http = crate::api::feed::feed_http();
    let handle = handle.trim().trim_start_matches('@');

    match target {
        TargetNetwork::At(_) => {
            let account =
                bsky.ok_or_else(|| AppError::NotFound("No Bluesky account linked".into()))?;
            let subject_did = crate::oauth::bluesky::resolve_handle(&http, handle)
                .await
                .map_err(|e| AppError::Internal(format!("Bluesky resolve failed: {e}")))?;
            // Find the follow record's rkey in the repo, then delete it.
            let pds_url = crate::oauth::bluesky::get_pds_url(&account.metadata);
            let list_url = format!(
                "{}/xrpc/com.atproto.repo.listRecords?repo={}&collection=app.bsky.graph.follow&limit=100",
                pds_url,
                urlencoding::encode(&account.external_id)
            );
            let list_resp = http
                .get(&list_url)
                .header("Authorization", format!("Bearer {}", account.access_token))
                .send()
                .await
                .map_err(|e| AppError::Internal(format!("Bluesky listRecords failed: {e}")))?;
            let list_json: serde_json::Value = list_resp.json().await.map_err(|e| {
                AppError::Internal(format!("Bluesky listRecords parse failed: {e}"))
            })?;
            let follow_rkey = list_json["records"]
                .as_array()
                .and_then(|records| {
                    records.iter().find_map(|r| {
                        let record_subject = r["value"]["subject"].as_str()?;
                        if record_subject == subject_did {
                            r["uri"]
                                .as_str()
                                .and_then(|uri| uri.rsplit('/').next())
                                .map(|s| s.to_string())
                        } else {
                            None
                        }
                    })
                })
                .ok_or_else(|| AppError::NotFound("Follow record not found".into()))?;
            crate::oauth::bluesky::unfollow_user(&state.db, &account, &follow_rkey)
                .await
                .map_err(|e| AppError::Internal(format!("Bluesky unfollow failed: {e}")))?;
            let actor_url = format!("https://bsky.app/profile/{}", handle);
            db::follows::FollowRow::delete_by_actor(&state.db, user_id, &actor_url).await?;
            Ok(FollowOutcome {
                status: "none",
                network: "atproto",
                identity_used: account.external_id.clone(),
                actor: subject_did,
                federated: true,
            })
        }
        TargetNetwork::Ap(_) => {
            // Undo key = the row's follow_type when a row exists.
            let rows = db::follows::FollowRow::list_by_user(&state.db, user_id)
                .await
                .unwrap_or_default();
            let row = rows
                .iter()
                .find(|r| crate::api::feed::ap_actor_matches_handle(&r.followed_actor, handle));
            let follow_type = row.map(|r| r.follow_type.clone());
            if follow_type.as_deref() == Some("mastodon_import") {
                // Home-graph follow → undo on the home instance.
                let acct = crate::api::feed::linked_mastodon_account(&state.db, user_id)
                    .await?
                    .ok_or_else(|| {
                        AppError::BadRequest(
                            "this follow lives on your Mastodon home instance — re-link to manage it"
                                .into(),
                        )
                    })?;
                crate::oauth::mastodon::ensure_follows_scope(acct.scope.as_deref())
                    .map_err(crate::api::feed::mastodon_error)?;
                let instance = crate::api::feed::home_instance_of(&acct)?;
                let token =
                    crate::oauth::mastodon::refresh_mastodon_token(&state.db, &http, &acct)
                        .await
                        .map_err(crate::api::feed::mastodon_error)?;
                let account_id =
                    crate::oauth::mastodon::lookup_account_id(&http, &instance, &token, handle)
                        .await
                        .map_err(crate::api::feed::mastodon_error)?;
                crate::oauth::mastodon::home_unfollow_account(&http, &instance, &token, &account_id)
                    .await
                    .map_err(crate::api::feed::mastodon_error)?;
                let actor_url = row.map(|r| r.followed_actor.clone()).unwrap_or_default();
                if !actor_url.is_empty() {
                    db::follows::FollowRow::delete_by_actor(&state.db, user_id, &actor_url).await?;
                }
                return Ok(FollowOutcome {
                    status: "none",
                    network: "activitypub",
                    identity_used: acct.external_username.clone(),
                    actor: actor_url,
                    federated: true,
                });
            }
            // Minted-actor follow (or unknown): resolve and queue the exact
            // Undo through the bridge; unresolvable remotes fall back to
            // best-effort stale-row cleanup by handle shape.
            let signer =
                crate::ap::keys::load_fetch_signer(&state.db, user_id, &state.config.jwt_secret)
                    .await;
            let actor_id = match crate::ap::remote::resolve_handle(
                &state.db,
                signer.as_ref().map(|(k, p)| (k.as_str(), p.as_str())),
                handle,
            )
            .await
            {
                Ok(a) => a.actor_id,
                Err(e) => {
                    tracing::warn!("[UNFOLLOW] AP actor resolve failed for {handle}: {e}");
                    for row in rows {
                        if row.follow_type == "ap"
                            && crate::api::feed::ap_actor_matches_handle(&row.followed_actor, handle)
                        {
                            let _ =
                                db::follows::FollowRow::delete(&state.db, row.id, user_id).await;
                        }
                    }
                    return Ok(FollowOutcome {
                        status: "none",
                        network: "activitypub",
                        identity_used: String::new(),
                        actor: String::new(),
                        federated: false,
                    });
                }
            };
            let queued =
                crate::ap::bridge::ui_unfollow(&state.db, user_id, &actor_id)
                    .await
                    .map_err(|e| AppError::Internal(format!("AP unfollow failed: {e}")))?;
            Ok(FollowOutcome {
                status: "none",
                network: "activitypub",
                identity_used: String::new(),
                actor: actor_id,
                federated: queued,
            })
        }
        TargetNetwork::Local(username) => {
            let target_user = db::users::UserRow::find_by_username(&state.db, &username)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("User {} not found", username)))?;
            let actor_url =
                format!("https://{}/@{}", state.config.domain, target_user.username);
            db::follows::FollowRow::delete_by_actor(&state.db, user_id, &actor_url).await?;
            Ok(FollowOutcome {
                status: "none",
                network: "local",
                identity_used: String::new(),
                actor: actor_url,
                federated: false,
            })
        }
    }
}

#[cfg(test)]
#[path = "follow.tests.rs"]
mod service_tests;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/following", get(following_handler))
}

pub(crate) fn extract_user_id(headers: &HeaderMap, secret: &str) -> Result<Uuid, AppError> {
    let auth = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Auth("Missing or invalid Authorization header".into()))?;
    let claims = auth::validate_token(auth, secret)?;
    Uuid::parse_str(&claims.sub).map_err(|_| AppError::Auth("Invalid user ID in token".into()))
}

#[derive(Debug, Deserialize)]
pub struct FollowListQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct FollowUserView {
    pub id: String,
    pub name: String,
    pub handle: String,
    pub avatar: Option<String>,
    pub source: String,
    pub followed_at: String,
}

#[derive(Debug, Serialize)]
pub struct FollowListResponse {
    pub items: Vec<FollowUserView>,
    pub has_more: bool,
}

async fn following_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<FollowListQuery>,
) -> Result<Json<FollowListResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let follows = db::follows::FollowRow::list_by_user(&state.db, user_id).await?;

    let mut items: Vec<FollowUserView> = Vec::new();
    for f in &follows {
        if f.status != "accepted" {
            continue;
        }
        let username = f.followed_actor.rsplit('/').next().unwrap_or(&f.followed_actor);
        let target_user = db::users::UserRow::find_by_username(&state.db, username.trim_start_matches('@'))
            .await
            .map_err(AppError::Database)?;

        let source = if f.follow_type.contains("import") { "import".to_string() } else { "bytescape".to_string() };

        if let Some(u) = target_user {
            items.push(FollowUserView {
                id: u.id.to_string(),
                name: u.display_name,
                handle: format!("@{}", u.username),
                avatar: u.avatar_url,
                source,
                followed_at: f.created_at.to_rfc3339(),
            });
        } else {
            items.push(FollowUserView {
                id: f.followed_actor.clone(),
                name: username.to_string(),
                handle: format!("@{}", username),
                avatar: None,
                source,
                followed_at: f.created_at.to_rfc3339(),
            });
        }
    }

    // Also include RSS subscriptions
    if let Ok(rss_subs) = db::subscriptions::SubscriptionRow::find_by_category(&state.db, user_id, "rss").await {
        for sub in &rss_subs {
            items.push(FollowUserView {
                id: sub.id.to_string(),
                name: sub.label.clone(),
                handle: sub.source_id.clone(),
                avatar: None,
                source: "rss".to_string(),
                followed_at: sub.created_at.to_rfc3339(),
            });
        }
    }

    // Sort by followed_at descending
    items.sort_by(|a, b| b.followed_at.cmp(&a.followed_at));

    let limit = query.limit.unwrap_or(50).min(500) as usize;
    let has_more = items.len() > limit;
    items.truncate(limit);

    Ok(Json(FollowListResponse { items, has_more }))
}
