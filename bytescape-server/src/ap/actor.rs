//! ActivityPub actor documents + canonical/alias id reconciliation.
//!
//! Reclaim strategy: the canonical actor id is `https://{domain}/@{username}`.
//! The alias forms `/users/{username}` and `/actors/{username}` **301-redirect**
//! to the canonical `/@{username}`. Whichever id the fediverse already cached
//! for a re-issued (reclaimed) identity, a remote server re-fetching is
//! reconciled to the one canonical actor — so reclaim succeeds regardless of the
//! cached form. `alsoKnownAs` lists the aliases for explicitness.
//!
//! All handlers bail (404) unless `AP_ACTORS_ENABLED`.

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use serde_json::{json, Value};

use super::ap_actors_enabled;
use crate::db::users::UserRow;
use crate::AppState;

/// Canonical actor URL for a username on this domain.
pub fn canonical_actor_url(domain: &str, username: &str) -> String {
    format!("https://{}/@{}", domain, username)
}

/// The alias actor-id forms that must reconcile to the canonical id.
pub fn alias_actor_urls(domain: &str, username: &str) -> Vec<String> {
    vec![
        format!("https://{}/users/{}", domain, username),
        format!("https://{}/actors/{}", domain, username),
    ]
}

/// The `alsoKnownAs` list for an AP actor document (task 3.4): the alias actor
/// forms plus the account's ONE AT Protocol identity, so one person resolves
/// across both protocols. `at_did` is the identity-spec AT identity — an
/// existing Bluesky `did:plc` where the user linked one, else the stored PDS
/// DID (post-5.4 a minted `did:plc`; pre-existing rows keep `did:web`) — never
/// derived: deriving `did:web:{username}.{domain}` would cross-link a second,
/// wrong identity.
pub fn actor_also_known_as(domain: &str, username: &str, at_did: Option<&str>) -> Vec<String> {
    let mut aliases = alias_actor_urls(domain, username);
    if let Some(did) = at_did {
        aliases.push(format!("at://{}", did));
    }
    aliases
}

/// The actor document JSON — the exact shape `GET /@{username}` serves: the
/// canonical id, the alias-reconciled `alsoKnownAs` (alias forms + the ONE
/// resolved AT identity `at://{at_did}`, never a derived identity), the
/// `#main-key` key document, and the profile fields. Pure builder shared by
/// the serving handler and the `Update(actor)` fan-out (which embeds the FULL
/// document so remotes replace their cached copy from `object`).
pub(crate) fn actor_document_json(
    user: &UserRow,
    actor_id: &str,
    at_did: Option<&str>,
    domain: &str,
) -> Value {
    actor_document_json_with_lock(user, actor_id, at_did, domain, user.manually_approves_followers)
}

/// The actor document builder with an explicit locked-state parameter — the
/// follow-request flow re-renders the doc with the flag flipped before the
/// flag column is visible on the refetched row.
pub(crate) fn actor_document_json_with_lock(
    user: &UserRow,
    actor_id: &str,
    at_did: Option<&str>,
    domain: &str,
    manually_approves_followers: bool,
) -> Value {
    let inbox = user
        .ap_inbox
        .clone()
        .unwrap_or_else(|| format!("{}/inbox", actor_id.trim_end_matches('/')));
    json!({
        "@context": [
            "https://www.w3.org/ns/activitystreams",
            "https://w3id.org/security/v1",
        ],
        "id": actor_id,
        "type": "Person",
        "preferredUsername": user.username,
        "name": user.display_name,
        "summary": user.bio,
        "url": actor_id,
        "inbox": inbox,
        "outbox": user.ap_outbox.clone().unwrap_or_else(|| format!("{}/outbox", actor_id)),
        "endpoints": {
            "sharedInbox": format!("https://{}/inbox", domain),
        },
        "manuallyApprovesFollowers": manually_approves_followers,
        "followers": format!("{}/followers", actor_id),
        "following": format!("{}/following", actor_id),
        "alsoKnownAs": actor_also_known_as(domain, &user.username, at_did),
        "publicKey": {
            "id": format!("{}#main-key", actor_id),
            "owner": actor_id,
            "publicKeyPem": user.ap_public_key_pem.clone().unwrap_or_default(),
        },
        "icon": user.avatar_url.as_ref().map(|u| json!({
            "type": "Image",
            "url": u,
        })),
        "image": user.header_url.as_ref().map(|u| json!({
            "type": "Image",
            "url": u,
        })),
    })
}

/// Fan out an `Update(actor)` because the actor's `alsoKnownAs` AT
/// cross-link changed (AT identity supersession). Runs inside the
/// reidentify cutover transaction (rolls back with it). No-op Ok(()) when
/// the user has no AP actor, or when their alsoKnownAs AT link is their
/// LINKED Bluesky account (the PDS-DID swap does not change the doc).
pub(crate) async fn fan_out_also_known_as_update(
    tx: &mut sqlx::PgConnection,
    user_id: uuid::Uuid,
    domain: &str,
    new_at_did: &str,
) -> Result<(), String> {
    // Everything through the caller's transaction — the typed row helpers take
    // `&PgPool` and would escape it.
    let user: UserRow = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| format!("user lookup: {e}"))?
        .ok_or_else(|| format!("no such user: {user_id}"))?;
    let Some(actor_id) = user.ap_actor_id.clone() else {
        // No minted AP actor (e.g. a post-realignment Mastodon signup acting
        // via its home instance): nothing of ours exists on the AP network.
        return Ok(());
    };
    if !user.ap_enabled {
        return Ok(());
    }
    // The rendered `at://` link is the LINKED Bluesky account's DID when one
    // exists (the `actor_document` precedence below) — the PDS-DID swap does
    // not change it, so the document is unchanged and there is nothing to
    // re-publish.
    let bluesky_linked: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM linked_accounts WHERE user_id = $1 AND provider = 'bluesky')",
    )
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| format!("linked bluesky lookup: {e}"))?;
    if bluesky_linked {
        return Ok(());
    }

    let activity_id = format!(
        "{}/activities/update-{}",
        actor_id.trim_end_matches('/'),
        uuid::Uuid::new_v4().simple()
    );
    let payload = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": activity_id,
        "type": "Update",
        "actor": actor_id,
        // The FULL actor document: remotes replace their cached copy from
        // `object`, so the new `at://{new_at_did}` cross-link takes effect
        // without re-resolving a link that now points at a tombstoned DID.
        "object": actor_document_json(&user, &actor_id, Some(new_at_did), domain),
    });

    // Delivery-history inbox set — the same query shape
    // `retire::stage_actor_retirement` freezes for its Delete fan-out: every
    // remote that ever received an activity from this actor may hold a cached
    // actor document. Read through the tx (the cutover's writes are invisible
    // to a pooled connection).
    let delivered: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT q.inbox_url FROM ap_delivery_queue q \
         JOIN ap_activities a ON a.id = q.activity_id \
         WHERE a.actor_id = $1",
    )
    .bind(&actor_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| format!("delivery history: {e}"))?;
    let inboxes: Vec<String> = delivered
        .into_iter()
        .map(|(inbox,)| inbox)
        // Same-server inboxes (the actor's own, the shared local inbox) are
        // never delivery targets — matches `store::audience_inboxes`' local
        // skip: no self-delivery.
        .filter(|inbox| {
            !inbox.is_empty()
                && !inbox.starts_with(&format!("https://{domain}/"))
                && !inbox.starts_with(&format!("http://{domain}/"))
        })
        .collect();

    crate::ap::store::insert_activity(
        &mut *tx,
        &activity_id,
        &actor_id,
        "Update",
        Some(&actor_id),
        None,
        "out",
        &payload,
    )
    .await
    .map_err(|e| format!("update activity insert: {e}"))?;
    crate::ap::store::enqueue_for_inboxes(&mut *tx, &activity_id, &inboxes)
        .await
        .map_err(|e| format!("update enqueue: {e}"))?;
    Ok(())
}

/// GET /@{username} — the ActivityPub actor document.
pub async fn actor_document(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Response, crate::error::AppError> {
    if !ap_actors_enabled() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }

    // Retirement registry first (add-identity-realignment): a superseded or
    // deleted actor's id keeps answering — its PUBLIC key document while the
    // retraction drain runs (remote signature verification fetches the signing
    // actor's key and would otherwise drop the Undo/Delete), a Tombstone after.
    if let Some(retired) = crate::ap::retire::load_retired_actor_by_username(&state.db, &username).await
    {
        let doc = crate::ap::retire::retired_actor_document(&retired);
        return Ok(([(header::CONTENT_TYPE, "application/activity+json")], Json(doc)).into_response());
    }

    let user = UserRow::find_ap_enabled_by_username(&state.db, &username)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("db: {e}")))?
        .ok_or_else(|| crate::error::AppError::NotFound("no such actor".into()))?;

    // The persisted actor id is authoritative (handles reclaim of an id that may
    // differ from the freshly-derived canonical form).
    let actor_id = user
        .ap_actor_id
        .clone()
        .unwrap_or_else(|| canonical_actor_url(&state.config.domain, &user.username));

    // The account's ONE cross-protocol AT identity (identity spec): an
    // existing Bluesky DID where the user linked one (their real identity on
    // the AT network), else the stored PDS DID (a minted did:plc since 5.4;
    // did:web only for pre-existing rows) — never derived.
    let bluesky_did: Option<String> = sqlx::query_scalar(
        "SELECT external_id FROM linked_accounts WHERE user_id = $1 AND provider = 'bluesky' LIMIT 1",
    )
    .bind(user.id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| crate::error::AppError::Internal(format!("db: {e}")))?;
    let pds_did: Option<String> = sqlx::query_scalar("SELECT did FROM pds_accounts WHERE user_id = $1")
        .bind(user.id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("db: {e}")))?;
    let at_did = bluesky_did.or(pds_did);

    let doc = actor_document_json(&user, &actor_id, at_did.as_deref(), &state.config.domain);

    Ok(([(header::CONTENT_TYPE, "application/activity+json")], Json(doc)).into_response())
}

/// GET /@{username}/activities/{rest} — dereferenceable activity ids. Serves
/// the stored `ap_activities.payload` verbatim with `application/activity+json`
/// so remote servers can resolve activity references (Undo originals). Unknown
/// activity or a URL not owned by the named actor → 404.
pub async fn activity_document(
    State(state): State<AppState>,
    Path((username, rest)): Path<(String, String)>,
) -> Result<Response, crate::error::AppError> {
    if !ap_actors_enabled() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    // Retired actor first (add-identity-realignment): published activity URLs
    // stay dereferenceable after retirement for retraction processing — even
    // after the owning account row is gone (the registry knows the actor id).
    let actor_id = match crate::ap::retire::load_retired_actor_by_username(&state.db, &username).await
    {
        Some(retired) => retired.actor_id,
        None => {
            let user = UserRow::find_ap_enabled_by_username(&state.db, &username)
                .await
                .map_err(|e| crate::error::AppError::Internal(format!("db: {e}")))?
                .ok_or_else(|| crate::error::AppError::NotFound("no such actor".into()))?;
            user.ap_actor_id
                .clone()
                .unwrap_or_else(|| canonical_actor_url(&state.config.domain, &user.username))
        }
    };
    let activity_id = format!("{}/activities/{}", actor_id.trim_end_matches('/'), rest);

    let payload: Option<(Value,)> =
        sqlx::query_as("SELECT payload FROM ap_activities WHERE id = $1")
            .bind(&activity_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| crate::error::AppError::Internal(format!("db: {e}")))?;
    let Some((payload,)) = payload else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };

    Ok(([(header::CONTENT_TYPE, "application/activity+json")], Json(payload)).into_response())
}

/// GET /users/{username} and /actors/{username} — 301 to the canonical `/@{username}`.
/// This is the reclaim reconciliation: every cached alias id resolves to one actor.
pub async fn alias_redirect(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Response, crate::error::AppError> {
    if !ap_actors_enabled() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    // Only redirect if this account actually serves an AP actor — don't promote
    // non-actor handles to a canonical actor that would 404.
    let exists = UserRow::find_ap_enabled_by_username(&state.db, &username)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("db: {e}")))?
        .is_some();
    if !exists {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }

    Ok(Redirect::permanent(&canonical_actor_url(&state.config.domain, &username)).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ap::testutil::{ap_router, connect_pool, enable_ap_flags, make_local_actor, test_state};

    /// Task 3.4 acceptance (updated for 5.4): the actor doc's `alsoKnownAs`
    /// carries the account's ONE AT identity (`at://did:plc:…` — an existing
    /// linked Bluesky DID where present, else the stored PDS DID) alongside
    /// the alias actor forms — never a derived did:web — so one person
    /// resolves across both protocols.
    #[test]
    fn also_known_as_includes_at_identity() {
        let aka = actor_also_known_as("bytescape.social", "alice", Some("did:plc:testsky"));
        assert!(
            aka.contains(&"at://did:plc:testsky".to_string()),
            "alsoKnownAs must carry the stored AT identity: {aka:?}"
        );
        assert!(aka.contains(&"https://bytescape.social/users/alice".to_string()));
        assert!(aka.contains(&"https://bytescape.social/actors/alice".to_string()));
        // No stored PDS DID (no pds account) -> no cross-link, and never a
        // derived did:web identity.
        let bare = actor_also_known_as("bytescape.social", "alice", None);
        assert!(!bare.iter().any(|a| a.starts_with("at://")), "no derived AT id: {bare:?}");
    }

    /// The cross-link resolution: the actor doc's `alsoKnownAs` carries the
    /// account's ONE AT identity — an existing linked Bluesky DID (their real
    /// identity on the AT network) takes precedence over the minted PDS DID;
    /// with no Bluesky link the minted DID is it.
    #[tokio::test]
    async fn actor_doc_cross_link_prefers_the_real_bluesky_identity() {
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let Some(state) = test_state(&pool) else { return };
        let la = make_local_actor(&pool, "bytescape.test").await;
        let suffix = crate::ap::testutil::unique();

        // Minted PDS DID (e.g. a did:plc mint at signup).
        let minted_did = format!("did:plc:minted{suffix}");
        sqlx::query(
            "INSERT INTO pds_accounts (did, handle, signing_key_private, signing_key_public, status, user_id) \
             VALUES ($1, $2, $3, 'test-pub', 'active', $4)",
        )
        .bind(&minted_did)
        .bind(&la.username)
        .bind(&[0x21u8; 32][..])
        .bind(la.user.id)
        .execute(&pool)
        .await
        .expect("seed minted pds row");

        // A linked Bluesky account: the user's REAL identity on the AT network.
        let real_did = format!("did:plc:realbsky{suffix}");
        crate::db::accounts::LinkedAccountRow::create(
            &pool,
            la.user.id,
            "bluesky",
            &real_did,
            "Alice",
            "tok",
            None,
            None,
            None,
            None,
            &serde_json::json!({}),
        )
        .await
        .expect("seed bluesky link");

        let fetch_aka = |state: crate::AppState, user: String| async move {
            let app = crate::ap::testutil::ap_router(state);
            let req = axum::http::Request::builder()
                .method("GET")
                .uri(format!("/@{user}"))
                .body(axum::body::Body::empty())
                .expect("actor request");
            let resp = tower::ServiceExt::oneshot(app, req).await.expect("oneshot");
            assert_eq!(resp.status().as_u16(), 200);
            let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.expect("body");
            let doc: Value = serde_json::from_slice(&bytes).expect("doc JSON");
            doc["alsoKnownAs"].clone()
        };

        let aka = fetch_aka(state.clone(), la.username.clone()).await;
        assert!(
            aka.to_string().contains(&format!("at://{real_did}")),
            "linked Bluesky DID is THE AT identity: {aka}"
        );
        assert!(
            !aka.to_string().contains(&minted_did),
            "exactly one AT identity — the superseded mint does not cross-link: {aka}"
        );

        // R2-4: the extracted document builder (the exact JSON the Update
        // fan-out embeds) renders exactly ONE `at://` identity — the resolved
        // one, linked Bluesky DID first — so the handler refactor is
        // behavior-neutral.
        let at_identities = |doc: &Value| -> Vec<String> {
            doc["alsoKnownAs"]
                .as_array()
                .expect("aka array")
                .iter()
                .filter_map(|a| a.as_str())
                .filter(|s| s.starts_with("at://"))
                .map(str::to_string)
                .collect()
        };
        let user = UserRow::find_by_id(&pool, la.user.id)
            .await
            .expect("refetch user")
            .expect("user row");
        let actor_id = user.ap_actor_id.clone().expect("minted actor id");
        let doc = actor_document_json(&user, &actor_id, Some(&real_did), "bytescape.test");
        assert_eq!(
            at_identities(&doc),
            vec![format!("at://{real_did}")],
            "builder: linked Bluesky DID wins — exactly one AT identity"
        );
        assert_eq!(doc["id"], Value::String(actor_id.clone()), "builder: the actor document id");

        // No Bluesky link → the minted PDS DID is the AT identity.
        let _ = sqlx::query("DELETE FROM linked_accounts WHERE user_id = $1 AND provider = 'bluesky'")
            .bind(la.user.id)
            .execute(&pool)
            .await;
        let aka = fetch_aka(state.clone(), la.username.clone()).await;
        assert!(
            aka.to_string().contains(&format!("at://{minted_did}")),
            "minted PDS DID carries the cross-link with no Bluesky link: {aka}"
        );
        let doc = actor_document_json(&user, &actor_id, Some(&minted_did), "bytescape.test");
        assert_eq!(
            at_identities(&doc),
            vec![format!("at://{minted_did}")],
            "builder: no Bluesky link — the PDS DID carries exactly one AT identity"
        );

        for table in ["pds_firehose_seq", "pds_indexed_records", "pds_repo_roots", "pds_accounts"] {
            let _ = sqlx::query(&format!("DELETE FROM {table} WHERE did = ANY($1)"))
                .bind(&[minted_did.clone()])
                .execute(&pool)
                .await;
        }
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(la.user.id)
            .execute(&pool)
            .await;
    }

    /// Activity ids must be dereferenceable: GET /@{username}/activities/{rest}
    /// returns the stored payload verbatim (application/activity+json), and an
    /// unknown activity is a 404 — remote servers resolve Undo originals here.
    #[tokio::test]
    async fn activity_url_serves_stored_payload_or_404() {
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let Some(state) = test_state(&pool) else { return };
        let la = make_local_actor(&pool, "bytescape.test").await;
        let suffix = crate::ap::testutil::unique();

        let rest = format!("like-{suffix}");
        let activity_id = format!("{}/activities/{}", la.actor_id, rest);
        let payload = serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": activity_id,
            "type": "Like",
            "actor": la.actor_id,
            "object": format!("https://remote.test/users/z/statuses/{suffix}"),
        });
        sqlx::query(
            "INSERT INTO ap_activities (id, actor_id, activity_type, object_id, direction, payload) \
             VALUES ($1, $2, 'Like', $3, 'out', $4)",
        )
        .bind(&activity_id)
        .bind(&la.actor_id)
        .bind(format!("https://remote.test/users/z/statuses/{suffix}"))
        .bind(&payload)
        .execute(&pool)
        .await
        .expect("seed activity");

        let app = crate::ap::testutil::ap_router(state.clone());
        let req = axum::http::Request::builder()
            .method("GET")
            .uri(format!("/@{}/activities/{}", la.username, rest))
            .body(axum::body::Body::empty())
            .expect("activity request");
        let resp = tower::ServiceExt::oneshot(app, req).await.expect("oneshot");
        assert_eq!(resp.status().as_u16(), 200, "stored activity must be dereferenceable");
        assert_eq!(
            resp.headers().get("content-type").and_then(|v| v.to_str().ok()),
            Some("application/activity+json"),
        );
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.expect("body");
        let doc: Value = serde_json::from_slice(&bytes).expect("body JSON");
        assert_eq!(doc, payload, "GET returns the stored payload verbatim");

        // Unknown activity → 404 (never a partial/stub document).
        let app = crate::ap::testutil::ap_router(state);
        let req = axum::http::Request::builder()
            .method("GET")
            .uri(format!("/@{}/activities/like-missing-{suffix}", la.username))
            .body(axum::body::Body::empty())
            .expect("missing activity request");
        let resp = tower::ServiceExt::oneshot(app, req).await.expect("oneshot");
        assert_eq!(resp.status().as_u16(), 404, "unknown activity id must 404");

        let _ = sqlx::query("DELETE FROM ap_delivery_queue WHERE activity_id = $1")
            .bind(&activity_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id = $1")
            .bind(&la.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(la.user.id)
            .execute(&pool)
            .await;
    }

    /// Task 3.2 acceptance: a retired actor id serves its key document while
    /// the drain runs (remote signature verification window), a Tombstone
    /// afterwards; its handle stops resolving via WebFinger the moment
    /// retirement starts; published activity URLs stay dereferenceable for the
    /// live owner ("available until the retraction is delivered").
    #[tokio::test]
    async fn retired_actor_serving_serves_key_doc_then_tombstone() {
        let _due = crate::ap::testutil::due_rows_lock().lock().await;
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let Some(state) = test_state(&pool) else { return };
        let la = make_local_actor(&pool, "bytescape.test").await;
        let suffix = crate::ap::testutil::unique();
        let payload = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": format!("{}/activities/like-{}", la.actor_id.trim_end_matches('/'), suffix),
            "type": "Like",
            "actor": la.actor_id,
            "object": format!("https://remote.test/users/z/statuses/{suffix}"),
            "to": ["https://remote.test/users/z"],
        });
        sqlx::query(
            "INSERT INTO ap_activities (id, actor_id, activity_type, object_id, direction, payload) \
             VALUES ($1, $2, 'Like', $3, 'out', $4)",
        )
        .bind(payload["id"].as_str().unwrap())
        .bind(&la.actor_id)
        .bind(format!("https://remote.test/users/z/statuses/{suffix}"))
        .bind(&payload)
        .execute(&pool)
        .await
        .expect("seed activity");

        crate::ap::retire::stage_actor_retirement(&pool, la.user.id)
            .await
            .expect("stage");

        // Draining: key-bearing document (the actor is dying but its
        // retractions are in flight — verification fetches publicKey).
        let app = ap_router(state.clone());
        let req = axum::http::Request::builder()
            .method("GET")
            .uri(format!("/@{}", la.username))
            .body(axum::body::Body::empty())
            .expect("actor request");
        let resp = tower::ServiceExt::oneshot(app, req).await.expect("oneshot");
        assert_eq!(resp.status().as_u16(), 200);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.expect("body");
        let doc: Value = serde_json::from_slice(&bytes).expect("JSON");
        assert_eq!(doc["type"], json!("Person"), "draining actor serves a key document");
        assert!(
            doc["publicKey"]["publicKeyPem"].as_str().is_some_and(|p| !p.is_empty()),
            "key document carries the public key for signature verification"
        );

        // WebFinger stops resolving the handle the moment retirement starts.
        let app = ap_router(state.clone());
        let req = axum::http::Request::builder()
            .method("GET")
            .uri(format!(
                "/.well-known/webfinger?resource=acct:{}@{}",
                la.username, state.config.domain
            ))
            .body(axum::body::Body::empty())
            .expect("webfinger request");
        let resp = tower::ServiceExt::oneshot(app, req).await.expect("oneshot");
        assert_eq!(resp.status().as_u16(), 404, "retired handle must not resolve as live");

        // Published activity URLs stay dereferenceable (superseded actor, live
        // owner — payloads outlive retirement for retraction processing).
        let app = ap_router(state.clone());
        let req = axum::http::Request::builder()
            .method("GET")
            .uri(format!("/@{}/activities/like-{}", la.username, suffix))
            .body(axum::body::Body::empty())
            .expect("activity request");
        let resp = tower::ServiceExt::oneshot(app, req).await.expect("oneshot");
        assert_eq!(resp.status().as_u16(), 200, "published activity stays dereferenceable");

        // Drain to completion: the staged Undo has a real queue row (the
        // remote-author fixture resolves its inbox) — emulate the delivery
        // worker's success before each pass so the drain advances through
        // retraction → Delete fan-out → finalize.
        for _ in 0..3 {
            sqlx::query(
                "UPDATE ap_delivery_queue SET status = 'delivered' \
                 WHERE status = 'pending' AND activity_id IN \
                 (SELECT id FROM ap_activities WHERE actor_id = $1)",
            )
            .bind(&la.actor_id)
            .execute(&pool)
            .await
            .expect("deliver staged rows");
            crate::ap::retire::drain_once(&pool).await.expect("drain pass");
        }

        // End state: Tombstone on the actor id, activity URLs still served.
        let app = ap_router(state.clone());
        let req = axum::http::Request::builder()
            .method("GET")
            .uri(format!("/@{}", la.username))
            .body(axum::body::Body::empty())
            .expect("actor request");
        let resp = tower::ServiceExt::oneshot(app, req).await.expect("oneshot");
        assert_eq!(resp.status().as_u16(), 200);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.expect("body");
        let doc: Value = serde_json::from_slice(&bytes).expect("JSON");
        assert_eq!(doc["type"], json!("Tombstone"), "drained actor serves a Tombstone");

        let app = ap_router(state);
        let req = axum::http::Request::builder()
            .method("GET")
            .uri(format!("/@{}/activities/like-{}", la.username, suffix))
            .body(axum::body::Body::empty())
            .expect("activity request");
        let resp = tower::ServiceExt::oneshot(app, req).await.expect("oneshot");
        assert_eq!(resp.status().as_u16(), 200, "superseded payloads kept for the live owner");

        let _ = sqlx::query("DELETE FROM ap_delivery_queue WHERE activity_id IN (SELECT id FROM ap_activities WHERE actor_id = $1)")
            .bind(&la.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id = $1")
            .bind(&la.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM retired_ap_actors WHERE actor_id = $1")
            .bind(&la.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(la.user.id)
            .execute(&pool)
            .await;
    }

    /// Seed one prior outbound activity + delivery-queue row to a known remote
    /// inbox — the delivery-history set the fan-out must reach (the same set
    /// `retire::stage_actor_retirement` freezes). Returns the remote inbox.
    async fn seed_delivery_history(
        pool: &sqlx::PgPool,
        actor_id: &str,
        suffix: &str,
    ) -> String {
        let remote_id = format!("https://remote.test/users/z-{suffix}");
        let inbox = format!("{remote_id}/inbox");
        let author = crate::ap::testutil::make_signer(&remote_id);
        crate::ap::testutil::seed_remote_actor(pool, &author, &inbox, None).await;
        let like_id = format!("{actor_id}/activities/like-{suffix}");
        let payload = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": like_id,
            "type": "Like",
            "actor": actor_id,
            "object": format!("https://remote.test/users/z-{suffix}/statuses/{suffix}"),
            "to": [remote_id],
        });
        sqlx::query(
            "INSERT INTO ap_activities (id, actor_id, activity_type, object_id, direction, payload) \
             VALUES ($1, $2, 'Like', $3, 'out', $4)",
        )
        .bind(&like_id)
        .bind(actor_id)
        .bind(format!("https://remote.test/users/z-{suffix}/statuses/{suffix}"))
        .bind(&payload)
        .execute(pool)
        .await
        .expect("seed prior outbound activity");
        sqlx::query(
            "INSERT INTO ap_delivery_queue (activity_id, inbox_url, status) VALUES ($1, $2, 'delivered')",
        )
        .bind(&like_id)
        .bind(&inbox)
        .execute(pool)
        .await
        .expect("seed delivery history row");
        inbox
    }

    /// R2-4: the cutover's Update(actor) fan-out reaches the delivery-history
    /// inbox set and re-publishes the full actor document with the NEW AT
    /// cross-link — remotes replace their cached doc from `object`.
    #[tokio::test]
    async fn fan_out_update_reaches_the_delivery_history_inbox_set() {
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let la = make_local_actor(&pool, "bytescape.test").await;
        let suffix = crate::ap::testutil::unique();
        // Host A: a plain remote inbox the actor delivered to before → must
        // receive the Update.
        let inbox_a = seed_delivery_history(&pool, &la.actor_id, &suffix).await;
        let new_at_did = format!("did:plc:newtest{suffix}");

        // Host B: a remote advertising BOTH a personal and a shared inbox; the
        // prior delivery went to its sharedInbox. sharedInbox collapses within
        // a host (exactly ONE queue row for B) but must NOT collapse distinct
        // hosts (A and B stay separate rows).
        let remote_b = format!("https://remote-b.test/users/z-{suffix}");
        let inbox_b = format!("{remote_b}/inbox");
        let shared_b = "https://remote-b.test/inbox".to_string();
        let author_b = crate::ap::testutil::make_signer(&remote_b);
        crate::ap::testutil::seed_remote_actor(&pool, &author_b, &inbox_b, Some(&shared_b)).await;
        let like_b = format!("{}/activities/like-b-{suffix}", la.actor_id.trim_end_matches('/'));
        let payload_b = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": like_b,
            "type": "Like",
            "actor": la.actor_id,
            "object": format!("{remote_b}/statuses/{suffix}"),
            "to": [remote_b],
        });
        sqlx::query(
            "INSERT INTO ap_activities (id, actor_id, activity_type, object_id, direction, payload) \
             VALUES ($1, $2, 'Like', $3, 'out', $4)",
        )
        .bind(&like_b)
        .bind(&la.actor_id)
        .bind(format!("{remote_b}/statuses/{suffix}"))
        .bind(&payload_b)
        .execute(&pool)
        .await
        .expect("seed host-B activity");
        sqlx::query(
            "INSERT INTO ap_delivery_queue (activity_id, inbox_url, status) VALUES ($1, $2, 'delivered')",
        )
        .bind(&like_b)
        .bind(&shared_b)
        .execute(&pool)
        .await
        .expect("seed host-B delivery history");

        // Host C "delivered" to OUR OWN inbox, and one row went to this
        // server's shared inbox — history noise: no self-delivery, no local
        // delivery of the Update.
        let self_inbox = format!("{}/inbox", la.actor_id);
        let remote_c = format!("https://remote-c.test/users/z-{suffix}");
        let author_c = crate::ap::testutil::make_signer(&remote_c);
        crate::ap::testutil::seed_remote_actor(&pool, &author_c, &self_inbox, None).await;
        let like_c = format!("{}/activities/like-c-{suffix}", la.actor_id.trim_end_matches('/'));
        let payload_c = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": like_c,
            "type": "Like",
            "actor": la.actor_id,
            "object": format!("{remote_c}/statuses/{suffix}"),
            "to": [remote_c],
        });
        sqlx::query(
            "INSERT INTO ap_activities (id, actor_id, activity_type, object_id, direction, payload) \
             VALUES ($1, $2, 'Like', $3, 'out', $4)",
        )
        .bind(&like_c)
        .bind(&la.actor_id)
        .bind(format!("{remote_c}/statuses/{suffix}"))
        .bind(&payload_c)
        .execute(&pool)
        .await
        .expect("seed host-C activity");
        for inbox in [self_inbox.as_str(), "https://bytescape.test/inbox"] {
            sqlx::query(
                "INSERT INTO ap_delivery_queue (activity_id, inbox_url, status) VALUES ($1, $2, 'delivered')",
            )
            .bind(&like_c)
            .bind(inbox)
            .execute(&pool)
            .await
            .expect("seed local delivery history");
        }

        let mut tx = pool.begin().await.expect("begin cutover tx");
        fan_out_also_known_as_update(&mut tx, la.user.id, "bytescape.test", &new_at_did)
            .await
            .expect("fan-out inside the cutover tx");
        tx.commit().await.expect("commit");

        // (a) the Update activity exists, embedding the FULL actor document
        // with the new cross-link and the actor's own id as `object.id`.
        let (payload,): (Value,) = sqlx::query_as(
            "SELECT payload FROM ap_activities WHERE actor_id = $1 AND activity_type = 'Update'",
        )
        .bind(&la.actor_id)
        .fetch_one(&pool)
        .await
        .expect("Update activity row");
        assert_eq!(payload["type"], json!("Update"), "activity type");
        assert_eq!(payload["actor"], Value::String(la.actor_id.clone()), "activity actor");
        assert!(
            payload["object"]["alsoKnownAs"]
                .to_string()
                .contains(&format!("at://{new_at_did}")),
            "re-published doc carries the NEW AT cross-link: {}",
            payload["object"]
        );
        assert_eq!(
            payload["object"]["id"], Value::String(la.actor_id.clone()),
            "re-published doc is THIS actor"
        );
        assert_eq!(payload["object"]["type"], json!("Person"), "full document, not a stub");

        // (b) …and the Update is queued to the delivery-history inbox set with
        // sharedInbox semantics pinned: exactly ONE row per remote HOST
        // (host B's sharedInbox — never its personal inbox too), distinct
        // hosts NOT collapsed together, and no self-delivery / no local
        // delivery.
        let (update_id,): (String,) = sqlx::query_as(
            "SELECT id FROM ap_activities WHERE actor_id = $1 AND activity_type = 'Update'",
        )
        .bind(&la.actor_id)
        .fetch_one(&pool)
        .await
        .expect("Update id");
        let queued_rows: Vec<(String,)> = sqlx::query_as(
            "SELECT inbox_url FROM ap_delivery_queue WHERE activity_id = $1 ORDER BY inbox_url",
        )
        .bind(&update_id)
        .fetch_all(&pool)
        .await
        .expect("queue lookup");
        let queued: Vec<String> = queued_rows.into_iter().map(|(u,)| u).collect();
        let mut expected = vec![inbox_a.clone(), shared_b.clone()];
        expected.sort();
        assert_eq!(
            queued, expected,
            "Update reaches every distinct remote HOST exactly once \
             (sharedInbox collapses within a host only), never self/local"
        );
        assert!(
            !queued.contains(&inbox_b),
            "host B is served at its sharedInbox — the personal inbox is NOT additionally queued"
        );
        assert!(
            !queued.contains(&self_inbox) && !queued.iter().any(|u| u.starts_with("https://bytescape.test/")),
            "no self-delivery / no local delivery of the Update"
        );

        let _ = sqlx::query("DELETE FROM ap_delivery_queue WHERE activity_id IN (SELECT id FROM ap_activities WHERE actor_id = $1)")
            .bind(&la.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id = $1")
            .bind(&la.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(la.user.id)
            .execute(&pool)
            .await;
    }

    /// R2-4: no-op without an AP actor — a user with `ap_actor_id` NULL has
    /// nothing of ours on the AP network, and an issued-but-disabled actor
    /// (`ap_enabled = false`) is not served either: neither produces an
    /// Update.
    #[tokio::test]
    async fn fan_out_update_is_noop_without_ap_actor() {
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };

        // (1) plain account, never issued an AP actor.
        let username = format!("aptest_{}", crate::ap::testutil::unique());
        let bare = UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create user");
        let mut tx = pool.begin().await.expect("begin");
        fan_out_also_known_as_update(&mut tx, bare.id, "bytescape.test", "did:plc:newtest")
            .await
            .expect("no-op with no AP actor");
        tx.commit().await.expect("commit");
        let (rows,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ap_activities WHERE actor_id = $1",
        )
        .bind(crate::ap::actor::canonical_actor_url("bytescape.test", &username))
        .fetch_one(&pool)
        .await
        .expect("activity count");
        assert_eq!(rows, 0, "no AP actor → no Update");

        // (2) issued actor, but the surface is disabled for the account.
        let la = make_local_actor(&pool, "bytescape.test").await;
        sqlx::query("UPDATE users SET ap_enabled = false WHERE id = $1")
            .bind(la.user.id)
            .execute(&pool)
            .await
            .expect("disable ap");
        let mut tx = pool.begin().await.expect("begin");
        fan_out_also_known_as_update(&mut tx, la.user.id, "bytescape.test", "did:plc:newtest")
            .await
            .expect("no-op with ap_enabled = false");
        tx.commit().await.expect("commit");
        let (rows,): (i64,) = sqlx::query_as("SELECT count(*) FROM ap_activities WHERE actor_id = $1")
            .bind(&la.actor_id)
            .fetch_one(&pool)
            .await
            .expect("activity count");
        assert_eq!(rows, 0, "ap_enabled = false → no Update");

        let _ = sqlx::query("DELETE FROM users WHERE id = ANY($1)")
            .bind(&[bare.id, la.user.id][..])
            .execute(&pool)
            .await;
    }

    /// R2-4: no-op when the `at://` cross-link is the LINKED Bluesky account —
    /// the PDS-DID swap does not change the rendered document (the
    /// `actor_document` precedence), so nothing needs re-publishing.
    #[tokio::test]
    async fn fan_out_update_is_noop_with_linked_bluesky() {
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let la = make_local_actor(&pool, "bytescape.test").await;
        let suffix = crate::ap::testutil::unique();
        crate::db::accounts::LinkedAccountRow::create(
            &pool,
            la.user.id,
            "bluesky",
            &format!("did:plc:realbsky{suffix}"),
            "Alice",
            "tok",
            None,
            None,
            None,
            None,
            &serde_json::json!({}),
        )
        .await
        .expect("seed bluesky link");

        let mut tx = pool.begin().await.expect("begin");
        fan_out_also_known_as_update(&mut tx, la.user.id, "bytescape.test", "did:plc:newtest")
            .await
            .expect("no-op with linked Bluesky");
        tx.commit().await.expect("commit");

        let (rows,): (i64,) = sqlx::query_as("SELECT count(*) FROM ap_activities WHERE actor_id = $1")
            .bind(&la.actor_id)
            .fetch_one(&pool)
            .await
            .expect("activity count");
        assert_eq!(rows, 0, "linked Bluesky DID is the cross-link — no Update");

        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(la.user.id)
            .execute(&pool)
            .await;
    }
}
