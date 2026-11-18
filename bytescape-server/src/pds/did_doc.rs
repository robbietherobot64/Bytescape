//! DID document hosting — instance + per-user.
//! Also serves /.well-known/atproto-did for AT Protocol handle resolution.

use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::{json, Value};

use crate::AppState;
use crate::pds::account::{PdsAccount, encode_public_key_multibase};

/// The one neutral 404 reason for every tombstone class (migrated / deleted /
/// never-existed) on the did.json and atproto-did routes: an unauthenticated
/// fetcher must not be able to distinguish account states from the response
/// body (R2-11). Status stays 404; routing unchanged.
const TOMBSTONE_404: &str = "not found";

/// The DID document for a stored PDS account: `id` is the account's STORED DID
/// (a minted `did:plc` since 5.4; a `did:web` row predating the realignment
/// serves its doc only until the `REIDENTIFY_DID_WEB` re-identify run
/// supersedes it — afterwards the old identity tombstones with a neutral 404
/// and only the new `did:plc` doc resolves),
/// never a derived `did:web` — the document must describe the identity the mint
/// actually created. `host` is the resolvable handle host (`{user}.{domain}`),
/// mirrored into `alsoKnownAs` as `at://{host}` (bidirectional with our
/// well-known handle resolution).
pub fn account_did_doc(account: &PdsAccount, host: &str, domain: &str) -> Value {
    let did_id = account.did.clone();
    let public_key_multibase = encode_public_key_multibase(&account.keypair);
    json!({
        "@context": ["https://www.w3.org/ns/did/v1", "https://w3id.org/security/multikey/v1"],
        "id": did_id,
        "alsoKnownAs": [format!("at://{}", host)],
        "verificationMethod": [{
            "id": format!("{}#signing-key", did_id),
            "type": "Multikey",
            "controller": did_id,
            "publicKeyMultibase": public_key_multibase,
        }],
        "service": [
            {
                "id": format!("{}#atproto_pds", did_id),
                "type": "AtprotoPersonalDataServer",
                "serviceEndpoint": format!("https://{}", domain),
            }
        ]
    })
}

/// Instance DID document at /.well-known/did.json
/// Uses a persisted instance signing key (stored in pds_accounts with reserved handle "_instance").
/// On wildcard subdomains, serves the per-user DID document based on Host header.
pub async fn instance_did_document(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Value>, crate::error::AppError> {
    let domain = state.config.domain.clone();

    // Check Host header for subdomain
    let host = headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or(&domain);
    let host = host.split(':').next().unwrap_or(host); // strip port

    // If on a subdomain, serve the user's DID document
    if host != domain {
        let handle = host
            .strip_suffix(&format!(".{}", domain))
            .unwrap_or(host);

        if let Ok(Some(account)) = PdsAccount::load_by_handle(&state.db, handle).await {
            // Retired `did:web` identity (pds::reidentify supersession): this
            // host resolves `did:web:{handle}.{domain}` but the account now
            // lives on a `did:plc` — serving its doc here would be a
            // mismatched-DID doc (the "imperfect resolution" class the
            // AppView drops events for). Same honest AT tombstone as a
            // deleted account: 404, one neutral message across every
            // tombstone class (R2-11). Handle resolution stays live via
            // /{handle}/.well-known/atproto-did.
            if !account.did.starts_with("did:web:") {
                return Err(crate::error::AppError::NotFound(TOMBSTONE_404.into()));
            }
            return Ok(Json(account_did_doc(&account, &host, &domain)));
        }
        // Unknown subdomain handle → honest AT tombstone: 404 (RepoNotFound/
        // HandleNotFound class), never the instance doc. A wildcard subdomain
        // must not serve the instance DID for a dead/never-existed handle
        // (add-identity-realignment: the did.json of a deleted account must
        // stop resolving). One neutral message across every tombstone class
        // (R2-11).
        return Err(crate::error::AppError::NotFound(TOMBSTONE_404.into()));
    }

    let did_id = format!("did:web:{}", domain);

    // Load or create the instance account (persists the signing key)
    let account = match PdsAccount::load_by_handle(&state.db, "_instance").await {
        Ok(Some(a)) => a,
        Ok(None) => {
            PdsAccount::create_instance(&state.db, &domain)
                .await
                .map_err(|e| crate::error::AppError::Internal(format!("Failed to create instance account: {e}")))?
        }
        Err(e) => return Err(crate::error::AppError::Internal(format!("Failed to load instance account: {e}"))),
    };

    let public_key_multibase = encode_public_key_multibase(&account.keypair);

    let doc = json!({
        "@context": ["https://www.w3.org/ns/did/v1", "https://w3id.org/security/multikey/v1"],
        "id": did_id,
        "alsoKnownAs": [format!("at://{}", domain)],
        "verificationMethod": [{
            "id": format!("{}#instance-key", did_id),
            "type": "Multikey",
            "controller": did_id,
            "publicKeyMultibase": public_key_multibase,
        }],
        "service": [
            {
                "id": format!("{}#atproto_pds", did_id),
                "type": "AtprotoPersonalDataServer",
                "serviceEndpoint": format!("https://{}", domain),
            }
        ]
    });

    Ok(Json(doc))
}

/// Per-user DID document at /{handle}.{domain}/.well-known/did.json
pub async fn user_did_document(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<Json<Value>, crate::error::AppError> {
    let domain = state.config.domain.clone();

    // Extract handle from the path (e.g. "alice.bytescape.social" → "alice")
    let handle = path
        .strip_suffix(&format!(".{}", domain))
        .unwrap_or(&path);

    let account = PdsAccount::load_by_handle(&state.db, handle)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("Failed to load account: {e}")))?
        .ok_or_else(|| crate::error::AppError::NotFound(TOMBSTONE_404.into()))?;

    // Retired `did:web` identity: this URL resolves `did:web:{path}`, but the
    // account now lives on a `did:plc` (pds::reidentify supersession). A doc
    // whose `id` doesn't match the requested DID is exactly the "imperfect
    // resolution" the AppView drops events for (bluesky-social/atproto#4379),
    // so retire the old identity with the same honest AT tombstone a deleted
    // account gets — 404, never a mismatched doc, one neutral message across
    // every tombstone class (R2-11). Handle → DID resolution
    // (/.well-known/atproto-did) keeps working and returns the live DID.
    if !account.did.starts_with("did:web:") {
        return Err(crate::error::AppError::NotFound(TOMBSTONE_404.into()));
    }

    let doc = account_did_doc(&account, &path, &domain);

    Ok(Json(doc))
}

/// GET /.well-known/atproto-did — AT Protocol handle resolution.
/// Reads the Host header to determine which handle is being resolved.
/// On main domain: returns instance DID.
/// On wildcard subdomain (e.g. lovingisliving.bytescape.social): returns that user's DID.
pub async fn atproto_did_instance(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, crate::error::AppError> {
    let domain = state.config.domain.clone();

    // Check Host header for subdomain
    let host = headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or(&domain);
    let host = host.split(':').next().unwrap_or(host); // strip port

    if host == domain {
        // Main domain — return instance DID
        let did = format!("did:web:{}", domain);
        return Ok(axum::response::Response::builder()
            .header("content-type", "text/plain")
            .body(axum::body::Body::from(did))
            .unwrap());
    }

    // Subdomain — extract handle
    let handle = host
        .strip_suffix(&format!(".{}", domain))
        .unwrap_or(host);

    match PdsAccount::load_by_handle(&state.db, handle).await {
        Ok(Some(account)) => Ok(axum::response::Response::builder()
            .header("content-type", "text/plain")
            .body(axum::body::Body::from(account.did))
            .unwrap()),
        Ok(None) => Err(crate::error::AppError::NotFound(TOMBSTONE_404.into())),
        Err(e) => Err(crate::error::AppError::Internal(format!("Failed to load account: {e}"))),
    }
}

/// GET /{handle}/.well-known/atproto-did — AT Protocol handle resolution (subdomain).
/// Returns the DID for the given handle.
pub async fn atproto_did(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<axum::response::Response, crate::error::AppError> {
    let domain = state.config.domain.clone();

    // If the path is the main domain, return instance DID
    if path == domain {
        let did = format!("did:web:{}", domain);
        return Ok(axum::response::Response::builder()
            .header("content-type", "text/plain")
            .body(axum::body::Body::from(did))
            .unwrap());
    }

    // Extract handle from subdomain path (e.g. "lovingisliving.bytescape.social" → "lovingisliving")
    let handle = path
        .strip_suffix(&format!(".{}", domain))
        .unwrap_or(&path);

    // Verify the account exists
    match PdsAccount::load_by_handle(&state.db, handle).await {
        Ok(Some(account)) => Ok(axum::response::Response::builder()
            .header("content-type", "text/plain")
            .body(axum::body::Body::from(account.did))
            .unwrap()),
        Ok(None) => Err(crate::error::AppError::NotFound(TOMBSTONE_404.into())),
        Err(e) => Err(crate::error::AppError::Internal(format!("Failed to load account: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;
    use secp256k1::{Keypair, Secp256k1};
    use secp256k1::rand::rngs::OsRng;

    /// The document describes the STORED identity — `id` is the stored did
    /// (did:plc since 5.4, did:web for pre-existing rows), never a derived
    /// `did:web:{user}.{domain}`, and `alsoKnownAs` claims the resolvable
    /// handle host.
    #[test]
    fn account_did_doc_serves_the_stored_identity() {
        let secp = Secp256k1::new();
        let keypair = Keypair::new(&secp, &mut OsRng);
        let account = PdsAccount {
            did: "did:plc:storedidentity".to_string(),
            handle: "alice".to_string(),
            keypair,
            status: "active".to_string(),
        };
        let doc = account_did_doc(&account, "alice.bytescape.test", "bytescape.test");
        assert_eq!(doc["id"], json!("did:plc:storedidentity"));
        assert_eq!(doc["alsoKnownAs"], json!(["at://alice.bytescape.test"]));
        assert_eq!(doc["verificationMethod"][0]["controller"], json!("did:plc:storedidentity"));
        assert_eq!(
            doc["verificationMethod"][0]["publicKeyMultibase"],
            json!(encode_public_key_multibase(&account.keypair)),
            "the document carries the account's REAL signing key"
        );
        assert_eq!(doc["service"][0]["id"], json!("did:plc:storedidentity#atproto_pds"));
    }

    /// Byte-exact body `error.rs` renders for a NotFound: `{"error":"not_found","message":<msg>}`.
    /// Comparing these bytes across account classes is what detects any
    /// migration-state oracle (R2-11).
    async fn not_found_body(err: crate::error::AppError) -> Vec<u8> {
        let resp = err.into_response();
        assert_eq!(resp.status(), axum::http::StatusCode::NOT_FOUND);
        let (_parts, body) = resp.into_parts();
        let bytes = axum::body::to_bytes(body, usize::MAX)
            .await
            .expect("read error body");
        bytes.to_vec()
    }

    fn retired_key() -> Vec<u8> {
        [0x33u8; 32].to_vec()
    }

    /// Seed a `pds_accounts` row. `did` starting `did:web:` = still-serving
    /// pre-realignment identity; `did:plc:` = migrated (the gate is
    /// `!did.starts_with("did:web:")` — no 091 claim row required).
    async fn seed_account(pool: &sqlx::PgPool, username: &str, did: &str, key: &[u8]) -> uuid::Uuid {
        let user = crate::db::users::UserRow::create_active(pool, username, "", "x", "x")
            .await
            .expect("create fixture user");
        sqlx::query(
            "INSERT INTO pds_accounts (did, handle, signing_key_private, signing_key_public, status, user_id) \
             VALUES ($1, $2, $3, 'test-pub', 'active', $4)",
        )
        .bind(did)
        .bind(username)
        .bind(key)
        .bind(user.id)
        .execute(pool)
        .await
        .expect("seed pds account");
        user.id
    }

    async fn fetch_user_did_doc(state: &crate::AppState, username: &str) -> Result<Json<Value>, crate::error::AppError> {
        user_did_document(
            axum::extract::State(state.clone()),
            axum::extract::Path(format!("{username}.test.bytescape.social")),
        )
        .await
    }

    async fn fetch_instance_did_doc(state: &crate::AppState, username: &str) -> Result<Json<Value>, crate::error::AppError> {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("host", format!("{username}.test.bytescape.social").parse().unwrap());
        instance_did_document(axum::extract::State(state.clone()), headers).await
    }

    async fn fetch_user_atproto_did(state: &crate::AppState, username: &str) -> Result<axum::response::Response, crate::error::AppError> {
        atproto_did(
            axum::extract::State(state.clone()),
            axum::extract::Path(format!("{username}.test.bytescape.social")),
        )
        .await
    }

    async fn fetch_instance_atproto_did(state: &crate::AppState, username: &str) -> Result<axum::response::Response, crate::error::AppError> {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("host", format!("{username}.test.bytescape.social").parse().unwrap());
        atproto_did_instance(axum::extract::State(state.clone()), headers).await
    }

    /// Retired `did:web` identity (reidentify supersession): the doc routes
    /// must 404 rather than serve a doc whose `id` mismatches the requested
    /// `did:web:{host}` DID — a mismatched doc is the "imperfect resolution"
    /// class the AppView drops events for (bluesky-social/atproto#4379), and
    /// the 404 body must carry the one neutral message (no migration-state
    /// oracle — R2-11).
    #[tokio::test]
    async fn retired_did_web_doc_routes_tombstone_rather_than_serve_mismatched_doc() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else {
            eprintln!("[test] retired_did_web_doc_routes_tombstone_rather_than_serve_mismatched_doc skipped: DATABASE_URL unset — asserted nothing");
            return;
        };
        let suffix = crate::ap::testutil::unique();
        let username = format!("retired_{suffix}");
        seed_account(&pool, &username, &format!("did:plc:reidentified{suffix}"), &retired_key()).await;

        let mut config = crate::config::AppConfig::from_env().expect("test config");
        config.domain = "test.bytescape.social".to_string();
        let state = crate::AppState {
            config: std::sync::Arc::new(config),
            db: pool.clone(),
            og_cache: Default::default(),
        };

        // Per-user route: /{handle}.{domain}/.well-known/did.json
        let res = fetch_user_did_doc(&state, &username).await;
        assert!(
            matches!(res, Err(crate::error::AppError::NotFound(_))),
            "retired did:web doc must tombstone, got {res:?}"
        );
        let msg = match res {
            Err(crate::error::AppError::NotFound(msg)) => msg,
            _ => unreachable!(),
        };
        assert_eq!(msg, "not found", "the 404 message must be the one neutral string, got {msg:?}");

        // Wildcard-subdomain route: Host = {handle}.{domain}
        let res = fetch_instance_did_doc(&state, &username).await;
        assert!(
            matches!(res, Err(crate::error::AppError::NotFound(_))),
            "retired did:web wildcard doc must tombstone, got {res:?}"
        );
    }

    /// R2-11: an unauthenticated fetcher of the did.json routes must NOT be
    /// able to distinguish "migrated to did:plc" from "deleted" from "never
    /// existed" — every 404 out of did_doc.rs carries one identical body.
    /// Drives all three classes —
    ///   migrated:  `pds_accounts` row with a `did:plc` DID (the tombstone gate
    ///              is `!account.did.starts_with("did:web:")`; no 091 claim row shape required),
    ///   deleted:   `pds_accounts` row purged with its owner (`purge_native_repo`
    ///              deletes the row and soft-deletes the user) — the tombstone
    ///              route's :134 no-account path,
    ///   unknown:   a handle that never existed —
    /// and asserts byte-identical 404 bodies across all three on every did.json
    /// surface (per-user + wildcard-subdomain Host routing).
    ///
    /// The atproto-did routes resolve handles per protocol (the migrated
    /// account answers 200 with its live `did:plc` — public-by-protocol, not an
    /// oracle; the review says "no new DID … leak[s]"), so only their
    /// no-account 404s (deleted + unknown) are compared; the migrated answer is
    /// asserted as a control.
    #[tokio::test]
    async fn tombstone_404_body_is_identical_for_migrated_deleted_and_unknown() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else {
            eprintln!("[test] tombstone_404_body_is_identical_for_migrated_deleted_and_unknown skipped: DATABASE_URL unset — asserted nothing");
            return;
        };
        let suffix = crate::ap::testutil::unique();

        // Class 1 — migrated: did:plc account (did_doc's `!did:web:` gate).
        let migrated = format!("mig_{suffix}");
        seed_account(&pool, &migrated, &format!("did:plc:migrated{suffix}"), &retired_key()).await;

        // Class 2 — deleted: the `pds_accounts` row is gone with its owner
        // (`purge_native_repo` deletes the row + soft-deletes the user), so
        // every route takes the :134 no-account path.
        let deleted = format!("del_{suffix}");
        let deleted_uid =
            seed_account(&pool, &deleted, &format!("did:web:del_{suffix}.test.bytescape.social"), &retired_key()).await;
        sqlx::query("DELETE FROM pds_accounts WHERE handle = $1")
            .bind(&deleted)
            .execute(&pool)
            .await
            .expect("purge fixture pds account");
        sqlx::query("UPDATE users SET deleted_at = NOW(), status = 'deleted' WHERE id = $1")
            .bind(deleted_uid)
            .execute(&pool)
            .await
            .expect("soft-delete fixture user");

        // Class 3 — unknown: handle that never existed.
        let unknown = format!("unk_{suffix}");

        let mut config = crate::config::AppConfig::from_env().expect("test config");
        config.domain = "test.bytescape.social".to_string();
        let state = crate::AppState {
            config: std::sync::Arc::new(config),
            db: pool.clone(),
            og_cache: Default::default(),
        };

        let mut did_json_bodies = Vec::new();
        let mut atproto_did_bodies = Vec::new();
        for name in [&migrated, &deleted, &unknown] {
            let r = fetch_user_did_doc(&state, name).await;
            assert!(
                matches!(r, Err(crate::error::AppError::NotFound(_))),
                "did.json ({name}) must 404, got {r:?}"
            );
            did_json_bodies.push(not_found_body(r.unwrap_err()).await);

            let r = fetch_instance_did_doc(&state, name).await;
            assert!(
                matches!(r, Err(crate::error::AppError::NotFound(_))),
                "wildcard did.json ({name}) must 404, got {r:?}"
            );
            assert_eq!(
                not_found_body(r.unwrap_err()).await,
                did_json_bodies[0],
                "wildcard-subdomain did.json 404 body must be byte-identical for {name}"
            );
        }

        // atproto-did: deleted + unknown share the no-account 404 (byte-identical
        // bodies); migrated resolves 200 to its live DID — protocol-mandated
        // handle resolution (public-by-protocol), so it is the control below.
        for name in [&deleted, &unknown] {
            let r = fetch_user_atproto_did(&state, name).await;
            assert!(
                matches!(r, Err(crate::error::AppError::NotFound(_))),
                "atproto-did ({name}) must 404, got {r:?}"
            );
            atproto_did_bodies.push(not_found_body(r.unwrap_err()).await);

            let r = fetch_instance_atproto_did(&state, name).await;
            assert!(
                matches!(r, Err(crate::error::AppError::NotFound(_))),
                "wildcard atproto-did ({name}) must 404, got {r:?}"
            );
            assert_eq!(
                not_found_body(r.unwrap_err()).await,
                atproto_did_bodies[0],
                "wildcard-subdomain atproto-did 404 body must be byte-identical for {name}"
            );
        }

        for (class, body) in ["migrated", "deleted", "unknown"].iter().zip(&did_json_bodies) {
            assert_eq!(
                body, &did_json_bodies[0],
                "did.json 404 body must be byte-identical for {class}: {body:?}"
            );
            // R2-11 negative pin (advisor decision 2026-11-14): the body must
            // never disclose migration state — no "retired"/"migrated" wording
            // may ever leak through a tombstone 404.
            let text = String::from_utf8_lossy(body);
            assert!(
                !text.contains("retired") && !text.contains("migrated"),
                "the 404 body must not disclose migration state: {text:?}"
            );
        }
        for (class, body) in ["deleted", "unknown"].iter().zip(&atproto_did_bodies) {
            assert_eq!(
                body, &atproto_did_bodies[0],
                "atproto-did 404 body must be byte-identical for {class}: {body:?}"
            );
        }

        // Controls — the fixtures are really the three distinct classes:
        // (a) migrated resolves its live DID via atproto-did (200 — protocol
        //     handle resolution, the documented exception to "indistinguishable").
        let r = fetch_user_atproto_did(&state, &migrated).await;
        assert!(r.is_ok(), "migrated account's atproto-did must resolve its live DID, got {r:?}");
        // (b) a live did:web account still serves its doc (the tombstone gate
        //     must not over-reach onto un-migrated accounts).
        let live = format!("live_{suffix}");
        seed_account(&pool, &live, &format!("did:web:live_{suffix}.test.bytescape.social"), &retired_key()).await;
        let r = fetch_user_did_doc(&state, &live).await;
        assert!(r.is_ok(), "un-migrated did:web account must keep serving its doc, got {r:?}");
    }
}