//! Shared fixtures for the Phase C AP interaction tests.
//!
//! Deterministic and isolated: every fixture id is random-suffixed, remote
//! signers are served from an in-test `ap_remote_actors` cache seed (no
//! network), and keys come from [`crate::ap::keys::generate_keypair`].

use axum::body::Body;
use axum::http::Request;
use sqlx::PgPool;

use crate::ap::{http_sig, keys};
use crate::db::users::UserRow;

/// Turn the AP surface on for the process (all AP tests need it; flipping the
/// flag to "true" is idempotent across parallel tests).
pub fn enable_ap_flags() {
    // Edition 2024: `env::set_var` is unsafe. Tests only ever flip the flags
    // to "true" (the value every AP test needs).
    unsafe {
        std::env::set_var("AP_ACTORS_ENABLED", "true");
        std::env::set_var("AP_INGEST_ENABLED", "true");
    }
}

/// Schema sync happens ONCE per test process: every DB test blocks here
/// before touching the database, so no fixture cleanup can race the
/// data-backfill migrations (051) the way per-call syncing allowed.
static MIGRATED: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();

/// Serializes tests that create DUE `ap_delivery_queue` rows. The delivery
/// worker (`process_due`) scans all due rows globally, so a concurrently
/// running delivery/retirement test would cross-process another test's rows
/// with the wrong sender outcome or drain state (observed: a `Permanent`
/// delivery test racing a retirement test's queued rows). Held across each
/// test's seed→process→assert→cleanup — mirroring the delivery tests' private
/// guard (d2486f1), generalized here when retirement staging started enqueueing
/// due rows of its own.
pub fn due_rows_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
        std::sync::LazyLock::new(tokio::sync::Mutex::default);
    &LOCK
}

pub async fn connect_pool() -> Option<PgPool> {
    if std::env::var("DATABASE_URL").is_err() {
        let _ = dotenvy::dotenv();
        let _ = dotenvy::from_filename("bytescape-server/.env");
    }
    match std::env::var("DATABASE_URL") {
        Ok(url) => {
            let pool = PgPool::connect(&url).await.expect("db connect");
            MIGRATED
                .get_or_init(|| async { crate::db::run_migrations(&pool).await })
                .await;
            Some(pool)
        }
        Err(_) => {
            eprintln!("DATABASE_URL unset — skipping DB integration test");
            None
        }
    }
}

pub fn test_state(pool: &PgPool) -> Option<crate::AppState> {
    let Ok(config) = crate::config::AppConfig::from_env() else {
        eprintln!("config incomplete — skipping AP DB test");
        return None;
    };
    Some(crate::AppState {
        config: std::sync::Arc::new(config),
        db: pool.clone(),
        og_cache: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
    })
}

/// Random 12-char fixture suffix.
pub fn unique() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

/// An AP signer identity: an actor URL + a locally generated RSA keypair.
pub struct TestSigner {
    pub actor_id: String,
    pub key_id: String,
    pub private_pem: String,
    pub public_pem: String,
}

/// Generate a signer for `actor_id` (remote-looking or local — the key is
/// resolved from wherever the test seeds it).
pub fn make_signer(actor_id: &str) -> TestSigner {
    let kp = keys::generate_keypair().expect("keygen");
    TestSigner {
        actor_id: actor_id.to_string(),
        key_id: format!("{actor_id}#main-key"),
        private_pem: kp.private_pem,
        public_pem: kp.public_pem,
    }
}

/// Seed `ap_remote_actors` so inbox verification resolves this signer's key
/// and inbox with zero network.
pub async fn seed_remote_actor(
    pool: &PgPool,
    signer: &TestSigner,
    inbox_url: &str,
    shared_inbox_url: Option<&str>,
) {
    seed_remote_actor_pem(pool, signer, inbox_url, shared_inbox_url, &signer.public_pem).await;
}

/// Re-encode a generated PKCS#1 public key PEM as SPKI (`BEGIN PUBLIC KEY`)
/// — the form Mastodon and virtually every AP implementation publish in
/// actor documents. Seeding a signer with this reproduces real remote
/// delivery key material deterministically.
pub fn spki_pem(pkcs1_public_pem: &str) -> String {
    use rsa::pkcs1::DecodeRsaPublicKey;
    use rsa::pkcs8::EncodePublicKey;
    let key = rsa::RsaPublicKey::from_pkcs1_pem(pkcs1_public_pem).expect("pkcs1 public");
    key.to_public_key_pem(rsa::pkcs8::LineEnding::LF)
        .expect("spki encode")
        .to_string()
}

/// [`seed_remote_actor`] with an explicit public key PEM (e.g. the SPKI
/// form — see [`spki_pem`]).
pub async fn seed_remote_actor_pem(
    pool: &PgPool,
    signer: &TestSigner,
    inbox_url: &str,
    shared_inbox_url: Option<&str>,
    public_pem: &str,
) {
    sqlx::query(
        "INSERT INTO ap_remote_actors \
           (actor_id, inbox_url, shared_inbox_url, public_key_pem, preferred_username, fetched_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, now(), now()) \
         ON CONFLICT (actor_id) DO UPDATE SET \
           inbox_url = $2, shared_inbox_url = $3, public_key_pem = $4, fetched_at = now(), updated_at = now()",
    )
    .bind(&signer.actor_id)
    .bind(inbox_url)
    .bind(shared_inbox_url)
    .bind(public_pem)
    .bind(signer.actor_id.rsplit('/').next().unwrap_or("remote"))
    .execute(pool)
    .await
    .expect("seed remote actor");
}

/// A local Bytescape account with an issued AP actor (plaintext test key at
/// rest — `keys::private_key_pem` accepts both formats).
pub struct TestLocalActor {
    pub user: UserRow,
    pub actor_id: String,
    pub username: String,
    pub private_pem: String,
}

pub async fn make_local_actor(pool: &PgPool, domain: &str) -> TestLocalActor {
    let username = format!("aptest_{}", unique());
    let user = UserRow::create_active(pool, &username, "", "x", "x")
        .await
        .expect("create user");
    let kp = keys::generate_keypair().expect("keygen");
    let actor_id = crate::ap::actor::canonical_actor_url(domain, &username);
    UserRow::set_ap_actor(
        pool,
        user.id,
        &actor_id,
        &format!("{actor_id}/inbox"),
        &format!("{actor_id}/outbox"),
        &kp.private_pem,
        &kp.public_pem,
    )
    .await
    .expect("issue AP actor");
    TestLocalActor {
        user,
        actor_id,
        username,
        private_pem: kp.private_pem,
    }
}

/// A signed inbox POST over the exact raw body bytes
/// (`(request-target) host date digest`).
pub fn signed_inbox_request(
    path: &str,
    body: &[u8],
    signer: &TestSigner,
    host: &str,
) -> Request<Body> {
    let date = http_sig::http_date();
    let digest = http_sig::digest_header(body);
    let signature = http_sig::sign_request(
        &signer.private_pem,
        &signer.key_id,
        "POST",
        path,
        &[("host", host), ("date", &date), ("digest", &digest)],
        &["(request-target)", "host", "date", "digest"],
    )
    .expect("sign inbox request");
    Request::builder()
        .method("POST")
        .uri(path)
        .header("host", host)
        .header("date", date)
        .header("digest", digest)
        .header("signature", signature)
        .header("content-type", "application/activity+json")
        .body(Body::from(body.to_vec()))
        .expect("inbox request")
}

/// Router with the AP routes mounted for oneshot dispatch.
pub fn ap_router(state: crate::AppState) -> axum::Router {
    axum::Router::new()
        .merge(crate::ap::routes())
        .with_state(state)
}