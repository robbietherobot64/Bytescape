//! Signed outbound ActivityPub delivery (Phase C).
//!
//! A durable queue (`ap_delivery_queue`, unique per `(activity_id, inbox_url)`
//! so sharedInbox dedupe and repeated enqueues can never double-deliver) plus a
//! background worker that POSTs each stored activity to its inbox as exact
//! bytes with `application/activity+json` and a draft-cavage HTTP Signature
//! over `(request-target) host date digest` (see [`crate::ap::http_sig`]).
//!
//! Outcome policy:
//! * 2xx → `delivered`
//! * 429 / 5xx / transport error → retry: `attempts + 1`, bounded exponential
//!   backoff (base 30s, ×2, cap 6h, ±20% jitter; `Retry-After` wins when the
//!   server sends one); `attempts >= max_attempts` → `failed` (bounded)
//! * other 4xx → `failed` (terminal), `last_error` set
//!
//! Queue rows live in Postgres, so pending deliveries survive restarts: the
//! worker claims due rows (`status = 'pending' AND next_attempt_at <= now()`)
//! on every cycle. Activity ids are stable — retries re-send the identical
//! bytes under the identical id.

use futures::future::BoxFuture;
use serde_json::Value;
use sqlx::PgPool;

// ─── Sender abstraction ────────────────────────────────────────────────────

/// One ready-to-send delivery: exact body bytes plus the signed headers that
/// cover them.
#[derive(Clone)]
pub struct OutboundPost {
    pub inbox_url: String,
    /// Exact bytes sent; the `Digest` header covers exactly these.
    pub body: Vec<u8>,
    pub host: String,
    pub path_and_query: String,
    pub date: String,
    pub digest: String,
    /// Full draft-cavage `Signature` header value.
    pub signature: String,
}

/// Result of one delivery attempt.
#[derive(Clone)]
pub enum SendOutcome {
    Delivered,
    /// Transient failure — retry with bounded backoff. `retry_after` is the
    /// server's `Retry-After` ask, if any (it wins over the computed backoff).
    Retry {
        retry_after: Option<std::time::Duration>,
        reason: String,
    },
    /// Permanent failure — terminal, never retried.
    Permanent {
        reason: String,
    },
}

/// Pluggable transport so the worker's state machine is testable without
/// network. [`HttpSender`] is the production implementation.
pub trait DeliverySender: Send + Sync {
    fn send(&self, post: OutboundPost) -> BoxFuture<'static, Result<SendOutcome, String>>;
}

/// Production transport: POST the exact bytes with `Content-Type:
/// application/activity+json` and the prebuilt `Digest`/`Date`/`Host`/
/// `Signature` headers. SSRF-guarded client (HTTPS-only, private ranges
/// rejected, redirects capped and re-guarded — see [`crate::ap::remote`]).
pub struct HttpSender {
    client: reqwest::Client,
}

impl HttpSender {
    pub fn new() -> Result<Self, String> {
        let client = crate::ap::remote::guarded_client()?;
        Ok(Self { client })
    }
}

impl DeliverySender for HttpSender {
    fn send(&self, post: OutboundPost) -> BoxFuture<'static, Result<SendOutcome, String>> {
        let client = self.client.clone();
        Box::pin(async move {
            // SSRF guard on the INITIAL target: a hostile actor document
            // controls inbox_url (ap_remote_actors is populated from remote
            // docs), and guarded_client's redirect policy only re-guards hops
            // after the first. Err → the transport/retry path; a blocked
            // target exhausts its bounded retry budget into `failed`.
            crate::ap::remote::guard_url(&post.inbox_url)
                .map_err(|e| format!("delivery target rejected: {e}"))?;
            let req = client
                .post(&post.inbox_url)
                .header(reqwest::header::CONTENT_TYPE, "application/activity+json")
                .header(reqwest::header::HOST, &post.host)
                .header(reqwest::header::DATE, &post.date)
                .header("Digest", &post.digest)
                .header("Signature", &post.signature)
                .body(post.body);
            let resp = req
                .send()
                .await
                .map_err(|e| format!("POST {}: {e}", post.inbox_url))?;
            let status = resp.status();
            if status.is_success() {
                return Ok(SendOutcome::Delivered);
            }
            let retry_after = resp
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(parse_retry_after);
            if status.as_u16() == 429 || status.is_server_error() {
                return Ok(SendOutcome::Retry {
                    retry_after,
                    reason: format!("HTTP {status}"),
                });
            }
            let body = resp.text().await.unwrap_or_default();
            let body = &body[..body.len().min(200)];
            Ok(SendOutcome::Permanent {
                reason: format!("HTTP {status}: {body}"),
            })
        })
    }
}

// ─── Retry timing ──────────────────────────────────────────────────────────

/// Parse a `Retry-After` value: delta-seconds or an HTTP-date (IMF-fixdate,
/// which is RFC 2822 with a literal `GMT` zone — `%Z` refuses to parse it, so
/// match the zone literally and fall back to RFC 2822).
fn parse_retry_after(v: &str) -> Option<std::time::Duration> {
    let v = v.trim();
    if let Ok(secs) = v.parse::<u64>() {
        return Some(std::time::Duration::from_secs(secs));
    }
    let when = chrono::DateTime::parse_from_str(v, "%a, %d %b %Y %H:%M:%S GMT")
        .or_else(|_| chrono::DateTime::parse_from_rfc2822(v))
        .ok()?;
    let delta = when.with_timezone(&chrono::Utc) - chrono::Utc::now();
    delta.to_std().ok()
}

/// Bounded exponential backoff: base 30s ×2 per attempt, capped at 6h, ±20%
/// jitter so a thundering herd cannot synchronize retries.
fn backoff_delay(attempts: i32) -> std::time::Duration {
    let exponent = attempts.saturating_sub(1).min(20) as u32;
    let base_secs = (30u64 << exponent).min(6 * 3600);
    let jitter = 0.8 + rand::random::<f64>() * 0.4;
    std::time::Duration::from_secs_f64(base_secs as f64 * jitter)
}

// ─── Worker state machine ──────────────────────────────────────────────────

const SCAN_LIMIT: i64 = 50;

/// Process every due pending delivery once. Returns how many rows were
/// attempted. Injectable `sender` keeps this deterministic in tests.
pub async fn process_due(
    db: &PgPool,
    jwt_secret: &str,
    sender: &dyn DeliverySender,
) -> Result<u32, sqlx::Error> {
    // FIFO within due rows: `seq` is insertion order. The old
    // (next_attempt_at, id) tiebreak used gen_random_uuid() ids, so
    // same-instant inserts (rapid Like → Undo) were claimed in random order
    // and an Undo could leapfrog the Like it undoes. Retries stay governed by
    // `next_attempt_at` (their due time simply comes later in seq order).
    //
    // The claim: the lease push-forward (`next_attempt_at = now() + 5 min`) IS
    // the in-flight claim — concurrent runners (multi-replica or overlapping
    // passes) see the leased row as not-due and skip it
    // (`FOR UPDATE SKIP LOCKED` serializes the claiming UPDATEs themselves).
    // Every outcome below overwrites `next_attempt_at` (retry backoff) or sets
    // a terminal status, so the lease never sticks. Crash window: a runner
    // that dies mid-flight leaves the row leased — it is retried after the
    // 5-minute lease expires (at-least-once delivery, bounded ~5min stall).
    let mut rows: Vec<(uuid::Uuid, String, String, i32, i32, i64)> = sqlx::query_as(
        "UPDATE ap_delivery_queue \
         SET next_attempt_at = now() + interval '5 minutes', updated_at = now() \
         WHERE id IN (SELECT id FROM ap_delivery_queue \
                       WHERE status = 'pending' AND next_attempt_at <= now() \
                       ORDER BY seq ASC LIMIT $1 \
                       FOR UPDATE SKIP LOCKED) \
         RETURNING id, activity_id, inbox_url, attempts, max_attempts, seq",
    )
    .bind(SCAN_LIMIT)
    .fetch_all(db)
    .await?;
    // `RETURNING` row order is unspecified — restore the claimed FIFO order
    // (`seq`) before processing so a Like → Undo pair in one batch is always
    // sent Like-first (the existing insertion-order contract).
    rows.sort_unstable_by_key(|r| r.5);

    let mut processed = 0u32;
    for (row_id, activity_id, inbox_url, attempts, max_attempts, _seq) in rows {
        processed += 1;

        let activity: Option<(String, Value)> =
            sqlx::query_as("SELECT actor_id, payload FROM ap_activities WHERE id = $1")
                .bind(&activity_id)
                .fetch_optional(db)
                .await?;
        let Some((actor_id, payload)) = activity else {
            mark_failed(db, row_id, attempts + 1, "activity row missing").await?;
            continue;
        };

        let Some((key_id, privkey_pem)) = signing_identity(db, jwt_secret, &actor_id).await else {
            mark_failed(db, row_id, attempts + 1, "no AP signing key for actor").await?;
            continue;
        };

        let body = serde_json::to_vec(&payload).unwrap_or_default();
        let digest = crate::ap::http_sig::digest_header(&body);
        let date = crate::ap::http_sig::http_date();
        let Ok(parsed) = url::Url::parse(&inbox_url) else {
            mark_failed(db, row_id, attempts + 1, "unparseable inbox url").await?;
            continue;
        };
        let host = match parsed.port() {
            Some(port) => format!("{}:{port}", parsed.host_str().unwrap_or_default()),
            None => parsed.host_str().unwrap_or_default().to_string(),
        };
        let path_and_query = match parsed.query() {
            Some(q) => format!("{}?{q}", parsed.path()),
            None => parsed.path().to_string(),
        };
        let signature = match crate::ap::http_sig::sign_request(
            &privkey_pem,
            &key_id,
            "POST",
            &path_and_query,
            &[("host", &host), ("date", &date), ("digest", &digest)],
            &["(request-target)", "host", "date", "digest"],
        ) {
            Ok(sig) => sig,
            Err(e) => {
                mark_failed(db, row_id, attempts + 1, &format!("signing failed: {e}")).await?;
                continue;
            }
        };

        let post = OutboundPost {
            inbox_url: inbox_url.clone(),
            body,
            host,
            path_and_query,
            date,
            digest,
            signature,
        };
        let outcome = match sender.send(post).await {
            Ok(outcome) => outcome,
            Err(e) => SendOutcome::Retry {
                retry_after: None,
                reason: format!("transport: {e}"),
            },
        };

        match outcome {
            SendOutcome::Delivered => {
                sqlx::query(
                    "UPDATE ap_delivery_queue SET status = 'delivered', attempts = $2, \
                     last_error = NULL, updated_at = now() WHERE id = $1",
                )
                .bind(row_id)
                .bind(attempts + 1)
                .execute(db)
                .await?;
            }
            SendOutcome::Permanent { reason } => {
                mark_failed(db, row_id, attempts + 1, &reason).await?;
            }
            SendOutcome::Retry { retry_after, reason } => {
                let next = attempts + 1;
                if next >= max_attempts {
                    mark_failed(db, row_id, next, &format!("{reason}; attempts exhausted")).await?;
                } else {
                    let delay = retry_after.unwrap_or_else(|| backoff_delay(next));
                    let at = chrono::Utc::now()
                        + chrono::Duration::from_std(delay).unwrap_or(chrono::Duration::hours(6));
                    sqlx::query(
                        "UPDATE ap_delivery_queue SET attempts = $2, next_attempt_at = $3, \
                         last_error = $4, updated_at = now() WHERE id = $1",
                    )
                    .bind(row_id)
                    .bind(next)
                    .bind(at)
                    .bind(&reason[..reason.len().min(500)])
                    .execute(db)
                    .await?;
                }
            }
        }
    }
    Ok(processed)
}

/// Terminal: mark the queue row failed with its error (rows stay for audit).
async fn mark_failed(
    db: &PgPool,
    row_id: uuid::Uuid,
    attempts: i32,
    error: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE ap_delivery_queue SET status = 'failed', attempts = $2, \
         last_error = $3, updated_at = now() WHERE id = $1",
    )
    .bind(row_id)
    .bind(attempts)
    .bind(&error[..error.len().min(500)])
    .execute(db)
    .await?;
    Ok(())
}

/// Resolve the signing identity `(key_id, privkey_pem)` for a local actor URL
/// (via [`crate::ap::keys::load_fetch_signer`]). `None` when the actor is not
/// a local AP-enabled user or the key cannot be loaded — those rows fail
/// terminally with an explanatory `last_error`.
async fn signing_identity(
    db: &PgPool,
    jwt_secret: &str,
    actor_id: &str,
) -> Option<(String, String)> {
    let user = match crate::db::users::UserRow::find_by_ap_actor_id(db, actor_id).await {
        Ok(Some(u)) => Some(u),
        _ => {
            // Fallback for canonical `/@{username}` URLs not stored verbatim.
            let username = actor_id
                .rsplit('/')
                .next()
                .unwrap_or("")
                .trim_start_matches('@')
                .to_string();
            if username.is_empty() {
                None
            } else {
                crate::db::users::UserRow::find_ap_enabled_by_username(db, &username)
                    .await
                    .ok()
                    .flatten()
            }
        }
    };
    if let Some(user) = user {
        if let Some(signer) = crate::ap::keys::load_fetch_signer(db, user.id, jwt_secret).await {
            return Some(signer);
        }
        // Fall through to retirement staging: a superseded actor's keypair is
        // dropped from `users` at stage time, but its in-flight retractions
        // must still sign during the drain (the registry holds the staged key).
    }
    // Retirement staging (add-identity-realignment): the account row is gone
    // but its retractions/Delete must still deliver during the drain window —
    // signed with the staged key (same `{actor_id}#main-key` id the actor doc
    // served before retirement, so remote verification resolves it).
    let staged: Option<(Option<String>,)> =
        sqlx::query_as("SELECT private_key_pem FROM retired_ap_actors WHERE actor_id = $1")
            .bind(actor_id)
            .fetch_optional(db)
            .await
            .ok()
            .flatten();
    let (enc_pem,) = staged?;
    let enc_pem = enc_pem?;
    match crate::ap::keys::private_key_pem(&enc_pem, jwt_secret) {
        Ok(pem) => Some((format!("{}#main-key", actor_id.trim_end_matches('/')), pem)),
        Err(e) => {
            tracing::warn!("[AP] retired signer key load failed for {actor_id}: {e}");
            None
        }
    }
}

// ─── Enqueue API (used by the bridge / inbox) ──────────────────────────────

/// Enqueue one (activity, inbox) delivery. Idempotent: the
/// `UNIQUE(activity_id, inbox_url)` key makes repeats no-ops. Returns whether
/// a row was created.
pub async fn enqueue(
    pool: &PgPool,
    activity_id: &str,
    inbox_url: &str,
) -> Result<bool, sqlx::Error> {
    let inserted: Option<(uuid::Uuid,)> = sqlx::query_as(
        "INSERT INTO ap_delivery_queue (activity_id, inbox_url) VALUES ($1, $2) \
         ON CONFLICT (activity_id, inbox_url) DO NOTHING RETURNING id",
    )
    .bind(activity_id)
    .bind(inbox_url)
    .fetch_optional(pool)
    .await?;
    Ok(inserted.is_some())
}

// ─── Background worker ─────────────────────────────────────────────────────

const CYCLE_SECS: u64 = 15;

/// Spawn the delivery worker. Queue state is in Postgres, so pending
/// deliveries survive restarts and are picked up on the first cycle. Spawned
/// from `main` once at startup.
pub fn spawn_worker(db: PgPool, jwt_secret: String) {
    tokio::spawn(async move {
        tracing::info!("[AP] delivery worker started");
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(CYCLE_SECS)).await;
            let sender = match HttpSender::new() {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("[AP] delivery HTTP client unavailable this cycle: {e}");
                    continue;
                }
            };
            match process_due(&db, &jwt_secret, &sender).await {
                Ok(0) => {}
                Ok(n) => tracing::debug!("[AP] delivery worker processed {n} due item(s)"),
                Err(e) => tracing::warn!("[AP] delivery worker scan failed: {e}"),
            }
        }
    });
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ap::testutil::{connect_pool, make_local_actor};
    use parking_lot::Mutex;
    use std::collections::HashMap;

    /// Delivery tests (and ANY test that seeds due queue rows) serialize on
    /// `testutil::due_rows_lock`: `process_due` scans ALL due rows, so a
    /// concurrent test's worker would cross-process rows with the wrong
    /// `FakeSender` outcome (e.g. a `Permanent` sender terminating a `Retry`
    /// test's row). Held across each test's seed→process→assert→cleanup.
    use crate::ap::testutil::due_rows_lock as SERIAL;

    /// Deterministic transport: canned outcomes PER INBOX; records every post.
    /// `process_due` claims all due rows in the shared test DB globally, so rows
    /// from other suites (bridge/inbox fixtures, retirement staging) are
    /// routinely claimed alongside this test's row — keying outcomes per inbox
    /// makes those foreign sends take the fallback outcome instead of shifting
    /// this test's canned sequence (observed flake: foreign rows consuming a
    /// `Permanent`/`Retry` outcome and flipping the row to `delivered`).
    struct FakeSender {
        by_inbox: Mutex<HashMap<String, Vec<SendOutcome>>>,
        fallback: SendOutcome,
        sent: Mutex<Vec<OutboundPost>>,
    }

    impl FakeSender {
        fn new(outcomes: Vec<SendOutcome>) -> Self {
            Self {
                by_inbox: Mutex::new(HashMap::new()),
                fallback: outcomes.into_iter().next().unwrap_or(SendOutcome::Delivered),
                sent: Mutex::new(Vec::new()),
            }
        }

        /// Canned outcomes for one inbox only (`queue_for` order).
        fn new_for(inbox: &str, fallback: SendOutcome, outcomes: Vec<SendOutcome>) -> Self {
            let s = Self {
                by_inbox: Mutex::new(HashMap::new()),
                fallback,
                sent: Mutex::new(Vec::new()),
            };
            s.queue_for(inbox, outcomes);
            s
        }

        fn queue_for(&self, inbox: &str, outcomes: Vec<SendOutcome>) {
            self.by_inbox.lock().insert(inbox.to_string(), outcomes);
        }

        /// The posts recorded for one inbox (foreign suites' sends excluded).
        fn sent_for(&self, inbox: &str) -> Vec<OutboundPost> {
            self.sent
                .lock()
                .iter()
                .filter(|p| p.inbox_url == inbox)
                .cloned()
                .collect()
        }
    }

    impl DeliverySender for FakeSender {
        fn send(&self, post: OutboundPost) -> BoxFuture<'static, Result<SendOutcome, String>> {
            let outcome = self
                .by_inbox
                .lock()
                .get_mut(&post.inbox_url)
                .and_then(|queue| queue.pop())
                .unwrap_or_else(|| self.fallback.clone());
            self.sent.lock().push(post);
            Box::pin(async move { Ok(outcome) })
        }
    }

    /// The one recorded post for `inbox` (assertions are own-inbox scoped:
    /// foreign suites' sends are ignored).
    fn only_post(sender: &FakeSender, inbox: &str) -> OutboundPost {
        let mine = sender.sent_for(inbox);
        assert_eq!(mine.len(), 1, "exactly one post to {inbox}");
        mine.into_iter().next().unwrap()
    }

    /// Insert an activity + queue row directly (raw fixtures — the bridge's
    /// own store path is covered by bridge tests). `actor_id` must belong to a
    /// real AP-enabled local user: the worker signs with that actor's key.
    async fn seed_delivery(
        pool: &sqlx::PgPool,
        suffix: &str,
        inbox: &str,
        actor_id: &str,
    ) -> (String, uuid::Uuid) {
        let activity_id = format!("{actor_id}/activities/x/{suffix}");
        sqlx::query(
            "INSERT INTO ap_activities (id, actor_id, activity_type, object_id, direction, payload) \
             VALUES ($1, $2, 'Create', NULL, 'out', $3)",
        )
        .bind(&activity_id)
        .bind(actor_id)
        .bind(serde_json::json!({"id": activity_id, "type": "Create"}))
        .execute(pool)
        .await
        .expect("insert activity");
        let (row_id,): (uuid::Uuid,) = sqlx::query_as(
            "INSERT INTO ap_delivery_queue (activity_id, inbox_url) VALUES ($1, $2) RETURNING id",
        )
        .bind(&activity_id)
        .bind(inbox)
        .fetch_one(pool)
        .await
        .expect("insert queue row");
        (activity_id, row_id)
    }

    async fn cleanup(pool: &sqlx::PgPool, suffix: &str) {
        let _ = sqlx::query("DELETE FROM ap_delivery_queue WHERE activity_id LIKE $1")
            .bind(format!("%/x/{suffix}"))
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE id LIKE $1")
            .bind(format!("%/x/{suffix}"))
            .execute(pool)
            .await;
    }

    #[test]
    fn backoff_is_exponential_bounded_and_jittered() {
        let d1 = backoff_delay(1);
        let d2 = backoff_delay(2);
        assert!(d1 >= std::time::Duration::from_secs(24), "{d1:?}"); // 30s ±20%
        assert!(d1 <= std::time::Duration::from_secs(36), "{d1:?}");
        assert!(d2 >= std::time::Duration::from_secs(48), "{d2:?}"); // 60s ±20%
        assert!(d2 <= std::time::Duration::from_secs(72), "{d2:?}");
        let capped = backoff_delay(50);
        assert!(capped <= std::time::Duration::from_secs((6 * 3600) as u64 * 12 / 10), "{capped:?}");
    }

    #[test]
    fn parse_retry_after_seconds_and_http_date() {
        assert_eq!(
            parse_retry_after("120"),
            Some(std::time::Duration::from_secs(120))
        );
        let future = (chrono::Utc::now() + chrono::Duration::seconds(300))
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let parsed = parse_retry_after(&future).expect("http-date parses");
        assert!(parsed >= std::time::Duration::from_secs(290), "{parsed:?}");
        assert!(parse_retry_after("not a date").is_none());
    }

    /// 2xx completes the row; the exact body bytes carry a Digest and a
    /// Signature covering (request-target) host date digest.
    #[tokio::test]
    async fn delivery_2xx_marks_delivered_with_signed_exact_bytes() {
        let Some(pool) = connect_pool().await else {
            return;
        };
        let _serial = SERIAL().lock().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let inbox = format!("https://remote.test/users/a/inbox-{suffix}");
        let la = make_local_actor(&pool, "bytescape.test").await;
        let (activity_id, row_id) = seed_delivery(&pool, &suffix, &inbox, &la.actor_id).await;
        let sender = FakeSender::new_for(&inbox, SendOutcome::Delivered, vec![]);

        let n = process_due(&pool, "unused", &sender)
            .await
            .expect("process_due");
        assert!(n >= 1, "the due row was processed");

        let (status, attempts, last_error): (String, i32, Option<String>) = sqlx::query_as(
            "SELECT status, attempts, last_error FROM ap_delivery_queue WHERE id = $1",
        )
        .bind(row_id)
        .fetch_one(&pool)
        .await
        .expect("queue row");
        assert_eq!(status, "delivered");
        assert_eq!(attempts, 1);
        assert!(last_error.is_none());

        // The bytes sent are the stored activity's exact serialization and
        // the headers cover them (own-inbox scoped).
        let post = only_post(&sender, &inbox);
        assert_eq!(post.inbox_url, inbox);
        let payload: Value = serde_json::from_slice(&post.body).expect("body JSON");
        assert_eq!(payload["id"], activity_id);
        assert_eq!(post.digest, crate::ap::http_sig::digest_header(&post.body));
        assert!(post.signature.contains(r#"keyId=""#));
        assert!(post.signature.contains(r#"headers="(request-target) host date digest""#));

        cleanup(&pool, &suffix).await;
    }

    /// 429/5xx/transport → still pending with attempts+1 and a future
    /// next_attempt_at (Retry-After wins when given).
    #[tokio::test]
    async fn delivery_transient_retries_with_backoff() {
        let Some(pool) = connect_pool().await else {
            return;
        };
        let _serial = SERIAL().lock().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let la = make_local_actor(&pool, "bytescape.test").await;
        let (_, row_id) = seed_delivery(
            &pool,
            &suffix,
            &format!("https://remote.test/inbox-{suffix}"),
            &la.actor_id,
        )
        .await;
        let sender = FakeSender::new_for(
            &format!("https://remote.test/inbox-{suffix}"),
            SendOutcome::Delivered,
            vec![SendOutcome::Retry {
                retry_after: Some(std::time::Duration::from_secs(900)),
                reason: "HTTP 429".into(),
            }],
        );

        process_due(&pool, "unused", &sender).await.expect("process_due");

        let (status, attempts, next, last_error): (String, i32, chrono::DateTime<chrono::Utc>, Option<String>) =
            sqlx::query_as(
                "SELECT status, attempts, next_attempt_at, last_error FROM ap_delivery_queue WHERE id = $1",
            )
            .bind(row_id)
            .fetch_one(&pool)
            .await
            .expect("queue row");
        assert_eq!(status, "pending", "transient failures stay pending");
        assert_eq!(attempts, 1);
        assert!(next > chrono::Utc::now() + chrono::Duration::seconds(870), "Retry-After wins: {next}");
        assert!(next <= chrono::Utc::now() + chrono::Duration::seconds(930), "{next}");
        assert!(last_error.as_deref() == Some("HTTP 429"), "{last_error:?}");

        // Not due again yet → our row sees no second attempt (foreign due
        // rows may legitimately process in this pass; assert on OUR row).
        let before: (i32,) = sqlx::query_as("SELECT attempts FROM ap_delivery_queue WHERE id = $1")
            .bind(row_id)
            .fetch_one(&pool)
            .await
            .expect("queue row");
        let _ = process_due(&pool, "unused", &FakeSender::new(vec![]))
            .await
            .expect("process_due");
        let after: (i32,) = sqlx::query_as("SELECT attempts FROM ap_delivery_queue WHERE id = $1")
            .bind(row_id)
            .fetch_one(&pool)
            .await
            .expect("queue row");
        assert_eq!(after.0, before.0, "row is scheduled for the future — no second attempt");

        cleanup(&pool, &suffix).await;
    }

    /// Other 4xx is terminal regardless of remaining attempts.
    #[tokio::test]
    async fn delivery_permanent_4xx_is_terminal() {
        let Some(pool) = connect_pool().await else {
            return;
        };
        let _serial = SERIAL().lock().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let la = make_local_actor(&pool, "bytescape.test").await;
        let (_, row_id) = seed_delivery(
            &pool,
            &suffix,
            &format!("https://remote.test/inbox-{suffix}"),
            &la.actor_id,
        )
        .await;
        let sender = FakeSender::new_for(
            &format!("https://remote.test/inbox-{suffix}"),
            SendOutcome::Delivered,
            vec![SendOutcome::Permanent {
                reason: "HTTP 404: gone".into(),
            }],
        );

        process_due(&pool, "unused", &sender).await.expect("process_due");

        let (status, last_error): (String, Option<String>) = sqlx::query_as(
            "SELECT status, last_error FROM ap_delivery_queue WHERE id = $1",
        )
        .bind(row_id)
        .fetch_one(&pool)
        .await
        .expect("queue row");
        assert_eq!(status, "failed", "permanent failure is terminal");
        assert!(last_error.unwrap_or_default().contains("404"));

        cleanup(&pool, &suffix).await;
    }

    /// Retry budget is bounded: the attempt that hits max_attempts flips the
    /// row to failed even though the failure itself is transient.
    #[tokio::test]
    async fn delivery_retry_budget_is_bounded() {
        let Some(pool) = connect_pool().await else {
            return;
        };
        let _serial = SERIAL().lock().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let la = make_local_actor(&pool, "bytescape.test").await;
        let (_, row_id) = seed_delivery(
            &pool,
            &suffix,
            &format!("https://remote.test/inbox-{suffix}"),
            &la.actor_id,
        )
        .await;
        sqlx::query("UPDATE ap_delivery_queue SET attempts = 7, max_attempts = 8 WHERE id = $1")
            .bind(row_id)
            .execute(&pool)
            .await
            .expect("set attempts");
        let sender = FakeSender::new_for(
            &format!("https://remote.test/inbox-{suffix}"),
            SendOutcome::Delivered,
            vec![SendOutcome::Retry {
                retry_after: None,
                reason: "transport: boom".into(),
            }],
        );

        process_due(&pool, "unused", &sender).await.expect("process_due");

        let (status, attempts): (String, i32) =
            sqlx::query_as("SELECT status, attempts FROM ap_delivery_queue WHERE id = $1")
                .bind(row_id)
                .fetch_one(&pool)
                .await
                .expect("queue row");
        assert_eq!(status, "failed", "attempts exhausted");
        assert_eq!(attempts, 8);

        cleanup(&pool, &suffix).await;
    }

    /// Queue rows cannot outlive their activity (FK ON DELETE CASCADE) and
    /// the worker fails such rows terminally instead of retrying forever if it
    /// ever meets one.
    #[tokio::test]
    async fn delivery_queue_rows_cascade_with_their_activity() {
        let Some(pool) = connect_pool().await else {
            return;
        };
        let _serial = SERIAL().lock().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let (activity_id, row_id) = seed_delivery(
            &pool,
            &suffix,
            &format!("https://remote.test/inbox-{suffix}"),
            "https://bytescape.test/@fixture",
        )
        .await;

        // Deleting the activity cascades to its queue row.
        sqlx::query("DELETE FROM ap_activities WHERE id = $1")
            .bind(&activity_id)
            .execute(&pool)
            .await
            .expect("delete activity");
        let (count,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ap_delivery_queue WHERE id = $1",
        )
        .bind(row_id)
        .fetch_one(&pool)
        .await
            .expect("count");
        assert_eq!(count, 0, "queue row cascaded away with its activity");

        cleanup(&pool, &suffix).await;
    }

    /// Enqueue is idempotent per (activity, inbox) — sharedInbox dedupe.
    #[tokio::test]
    async fn enqueue_dedupes_per_activity_and_inbox() {
        let Some(pool) = connect_pool().await else {
            return;
        };
        let _serial = SERIAL().lock().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let (activity_id, _) = seed_delivery(
            &pool,
            &suffix,
            &format!("https://remote.test/shared-{suffix}"),
            "https://bytescape.test/@fixture",
        )
        .await;
        // Second enqueue of the same (activity, inbox) is a no-op…
        let again = enqueue(&pool, &activity_id, &format!("https://remote.test/shared-{suffix}"))
            .await
            .expect("enqueue");
        assert!(!again, "duplicate (activity, inbox) must not enqueue");
        // …while a distinct inbox gets its own row.
        let other = enqueue(&pool, &activity_id, &format!("https://other.test/inbox-{suffix}"))
            .await
            .expect("enqueue");
        assert!(other, "distinct inbox enqueues");

        let (count,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ap_delivery_queue WHERE activity_id = $1",
        )
        .bind(&activity_id)
        .fetch_one(&pool)
        .await
        .expect("count");
        assert_eq!(count, 2);

        cleanup(&pool, &suffix).await;
    }

    /// Two queue rows inserted back-to-back (same next_attempt_at) are claimed
    /// in INSERTION order, not in gen_random_uuid() order — a rapid Like →
    /// Undo must never let the Undo leapfrog the Like it undoes.
    #[tokio::test]
    async fn delivery_claims_due_rows_in_insertion_order() {
        let Some(pool) = connect_pool().await else {
            return;
        };
        let _serial = SERIAL().lock().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let inbox = format!("https://remote.test/users/a/inbox-{suffix}");
        let la = make_local_actor(&pool, "bytescape.test").await;

        // Four back-to-back inserts: uuid tiebreak would order these
        // randomly (1-in-24 chance of passing by luck), `seq` never does.
        let mut expected: Vec<String> = Vec::new();
        for i in 0..4 {
            let row_suffix = format!("{i}{suffix}");
            let (activity_id, _) = seed_delivery(&pool, &row_suffix, &inbox, &la.actor_id).await;
            expected.push(activity_id);
        }
        let sender = FakeSender::new_for(&inbox, SendOutcome::Delivered, vec![]);

        let _ = process_due(&pool, "unused", &sender).await.expect("process_due");
        assert_eq!(sender.sent_for(&inbox).len(), 4, "all four of our due rows processed");

        let sent: Vec<String> = sender
            .sent_for(&inbox)
            .iter()
            .map(|post| {
                let payload: Value = serde_json::from_slice(&post.body).expect("body JSON");
                payload["id"].as_str().expect("activity id").to_string()
            })
            .collect();
        assert_eq!(sent, expected, "due rows claimed in insertion order");

        for row_suffix in 0..4 {
            cleanup(&pool, &format!("{row_suffix}{suffix}")).await;
        }
    }

    /// CR-7: `process_due` must claim its rows atomically. Two concurrent
    /// passes over one due row must produce exactly ONE send — without an
    /// in-flight claim both passes SELECT the same row and double-POST it.
    /// The sender sleeps ~200ms per send, so with the plain SELECT both
    /// passes have fetched the row before either resolves.
    #[tokio::test]
    async fn concurrent_process_due_passes_send_each_due_row_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        /// Sleeps ~200ms per send (holding the row "in flight") and counts
        /// sends per inbox.
        struct SlowCountingSender {
            inner: FakeSender,
            inbox: String,
            sends: Arc<AtomicUsize>,
        }
        impl DeliverySender for SlowCountingSender {
            fn send(&self, post: OutboundPost) -> BoxFuture<'static, Result<SendOutcome, String>> {
                if post.inbox_url == self.inbox {
                    self.sends.fetch_add(1, Ordering::SeqCst);
                }
                let fut = self.inner.send(post);
                Box::pin(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    fut.await
                })
            }
        }

        let Some(pool) = connect_pool().await else {
            return;
        };
        let _serial = SERIAL().lock().await;
        let suffix = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let inbox = format!("https://remote.test/users/a/inbox-{suffix}");
        let la = make_local_actor(&pool, "bytescape.test").await;
        let (_, row_id) = seed_delivery(&pool, &suffix, &inbox, &la.actor_id).await;
        let sends = Arc::new(AtomicUsize::new(0));
        let sender = SlowCountingSender {
            inner: FakeSender::new_for(&inbox, SendOutcome::Delivered, vec![]),
            inbox: inbox.clone(),
            sends: sends.clone(),
        };

        // Two overlapping passes over the same due row.
        let (a, b) = tokio::join!(
            process_due(&pool, "unused", &sender),
            process_due(&pool, "unused", &sender),
        );
        a.expect("process_due a");
        b.expect("process_due b");

        assert_eq!(
            sends.load(Ordering::SeqCst),
            1,
            "the in-flight claim lets exactly one pass POST the row"
        );
        let (status, attempts): (String, i32) =
            sqlx::query_as("SELECT status, attempts FROM ap_delivery_queue WHERE id = $1")
                .bind(row_id)
                .fetch_one(&pool)
                .await
                .expect("queue row");
        assert_eq!(status, "delivered");
        assert_eq!(attempts, 1, "one attempt — the losing pass never touched the row");

        cleanup(&pool, &suffix).await;
    }
}