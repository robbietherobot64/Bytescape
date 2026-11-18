//! Remote AP actor resolution + SSRF-guarded fetching (Phase C).
//!
//! One place for every outbound network touch of the AP surface:
//! * [`guard_url`] — HTTPS-only (unless `AP_ALLOW_HTTP=true`), DNS-resolved
//!   loopback/private/link-local/CGNAT/reserved targets rejected.
//! * [`guarded_client`] — 15s timeout, redirects capped at 3 and re-guarded
//!   per hop.
//! * [`load_or_fetch_actor`] / [`resolve_handle`] — actor-doc + WebFinger
//!   resolution with a 24h `ap_remote_actors` cache. Fetching never writes;
//!   callers persist the resolved actor via [`upsert_actor`] only AFTER HTTP
//!   signature verification succeeded.

use serde_json::Value;
use std::net::{IpAddr, ToSocketAddrs};

/// Fediverse content negotiation for actor docs / WebFinger.
const ACCEPT: &str = "application/activity+json, \
                      application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\", \
                      application/json";
const USER_AGENT: &str = "bytescape-ap/0.1";
const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const MAX_REDIRECTS: usize = 3;
const CACHE_TTL: chrono::Duration = chrono::Duration::hours(24);

/// A resolved remote actor: delivery targets + the key used to verify its
/// HTTP signatures. Mirrors the `ap_remote_actors` columns.
#[derive(Debug, Clone)]
pub struct RemoteActor {
    pub actor_id: String,
    pub inbox_url: String,
    pub shared_inbox_url: Option<String>,
    pub public_key_pem: Option<String>,
    pub preferred_username: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

impl RemoteActor {
    /// The delivery inbox to use: the shared inbox when advertised (dedupe),
    /// otherwise the actor's own inbox.
    pub fn delivery_inbox(&self) -> &str {
        self.shared_inbox_url.as_deref().unwrap_or(&self.inbox_url)
    }
}

/// Is plain HTTP permitted (dev escape hatch)? `AP_ALLOW_HTTP=true`.
fn http_allowed() -> bool {
    std::env::var("AP_ALLOW_HTTP").map(|v| v.eq_ignore_ascii_case("true")) == Ok(true)
}

/// True for addresses that must never be fetched or delivered to: loopback,
/// private (RFC1918), link-local, CGNAT, unspecified, multicast, reserved.
pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_multicast()
                // 100.64.0.0/10 CGNAT + 240.0.0.0/4 reserved.
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64)
                || v4.octets()[0] >= 240
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // fc00::/7 unique-local + fe80::/10 link-local.
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                // IPv4-mapped: judge the embedded v4.
                || v6
                    .to_ipv4_mapped()
                    .map(|v4| is_blocked_ip(IpAddr::V4(v4)))
                    .unwrap_or(false)
        }
    }
}

/// SSRF guard for a URL we are about to fetch or deliver to: HTTPS (unless
/// `AP_ALLOW_HTTP=true`), non-empty host, and no resolved address in a blocked
/// range. DNS is resolved here so literal names like `localhost` cannot slip
/// through on the later connection.
pub fn guard_url(url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("bad url {url}: {e}"))?;
    let scheme = parsed.scheme();
    let ok_scheme = scheme == "https" || (scheme == "http" && http_allowed());
    if !ok_scheme {
        return Err(format!("refusing non-https url {url}"));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| format!("url without host: {url}"))?
        .to_string();
    let port = parsed.port_or_known_default().unwrap_or(443);

    // Literal host? Judge directly, no DNS.
    if let Ok(ip) = host.parse::<IpAddr>() {
        return if is_blocked_ip(ip) {
            Err(format!("refusing private address {ip} for {url}"))
        } else {
            Ok(())
        };
    }

    let addrs = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|e| format!("dns {host}: {e}"))?
        .collect::<Vec<_>>();
    if addrs.is_empty() {
        return Err(format!("dns {host}: no addresses"));
    }
    for addr in &addrs {
        if is_blocked_ip(addr.ip()) {
            return Err(format!("refusing private address {} for {url}", addr.ip()));
        }
    }
    Ok(())
}

/// HTTP client for AP fetch/delivery: bounded timeout, bounded redirects,
/// each redirect hop re-validated by [`guard_url`].
pub fn guarded_client() -> Result<reqwest::Client, String> {
    let policy = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() > MAX_REDIRECTS {
            return attempt.error("too many redirects");
        }
        let url = attempt.url().to_string();
        match guard_url(&url) {
            Ok(()) => attempt.follow(),
            Err(e) => attempt.error(e),
        }
    });
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .redirect(policy)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| format!("http client: {e}"))
}

/// GET JSON with AP content negotiation and (optionally) a signed GET.
async fn get_json(url: &str, signer: Option<(&str, &str)>) -> Result<Value, String> {
    guard_url(url)?;
    let client = guarded_client()?;
    let mut req = client
        .get(url)
        .header(reqwest::header::ACCEPT, ACCEPT);
    for (name, value) in crate::ap::http_sig::signed_get_headers(url, signer, ACCEPT) {
        req = req.header(name, value);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("fetch {url}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("fetch {url}: HTTP {}", resp.status()));
    }
    let v: Value = resp
        .json()
        .await
        .map_err(|e| format!("fetch {url}: body: {e}"))?;
    if !v.is_object() {
        return Err(format!("fetch {url}: not a JSON object"));
    }
    Ok(v)
}

fn href_of(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Object(o) => o
            .get("href")
            .or_else(|| o.get("url"))
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        _ => None,
    }
}

/// Parse an actor document (already fetched) into a [`RemoteActor`].
/// The doc's `id` must equal the URL it was fetched from, and a `publicKey.owner`
/// must match the actor — a key can only sign for its owner.
pub fn actor_from_doc(doc: &Value, actor_url: &str) -> Result<RemoteActor, String> {
    let id = doc
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("actor doc {actor_url}: missing id"))?;
    if id != actor_url {
        return Err(format!("actor doc {actor_url}: id mismatch ({id})"));
    }
    let inbox_url = href_of(doc.get("inbox"))
        .ok_or_else(|| format!("actor doc {actor_url}: missing inbox"))?;
    let shared_inbox_url = doc
        .get("endpoints")
        .and_then(|e| e.get("sharedInbox"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let mut public_key_pem = None;
    if let Some(pk) = doc.get("publicKey") {
        if let Some(owner) = pk.get("owner").and_then(|v| v.as_str()) {
            if owner != id {
                return Err(format!(
                    "actor doc {actor_url}: key owner mismatch ({owner})"
                ));
            }
        }
        public_key_pem = pk
            .get("publicKeyPem")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string);
    }

    Ok(RemoteActor {
        actor_id: id.to_string(),
        inbox_url,
        shared_inbox_url,
        public_key_pem,
        preferred_username: doc
            .get("preferredUsername")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        display_name: doc
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        avatar_url: href_of(doc.get("icon")),
    })
}

/// Fetch + parse an actor document. Reads only — callers persist via
/// [`upsert_actor`] once the signer is verified.
pub async fn fetch_actor(
    actor_url: &str,
    signer: Option<(&str, &str)>,
) -> Result<RemoteActor, String> {
    let doc = get_json(actor_url, signer).await?;
    actor_from_doc(&doc, actor_url)
}

/// The cached `ap_remote_actors` row for `actor_id`, if any.
pub async fn load_cached_actor(
    pool: &sqlx::PgPool,
    actor_id: &str,
) -> Option<RemoteActor> {
    let row: (
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT actor_id, inbox_url, shared_inbox_url, public_key_pem, \
                preferred_username, display_name, avatar_url \
         FROM ap_remote_actors WHERE actor_id = $1",
    )
    .bind(actor_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| tracing::warn!("[AP] remote actor cache read failed for {actor_id}: {e}"))
    .ok()??;
    Some(RemoteActor {
        actor_id: row.0,
        inbox_url: row.1,
        shared_inbox_url: row.2,
        public_key_pem: row.3,
        preferred_username: row.4,
        display_name: row.5,
        avatar_url: row.6,
    })
}

/// Persist a resolved actor (upsert; `fetched_at` bumped on refresh).
pub async fn upsert_actor(pool: &sqlx::PgPool, a: &RemoteActor) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO ap_remote_actors \
           (actor_id, inbox_url, shared_inbox_url, public_key_pem, preferred_username, display_name, avatar_url, fetched_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, now(), now()) \
         ON CONFLICT (actor_id) DO UPDATE SET \
           inbox_url = EXCLUDED.inbox_url, \
           shared_inbox_url = EXCLUDED.shared_inbox_url, \
           public_key_pem = COALESCE(EXCLUDED.public_key_pem, ap_remote_actors.public_key_pem), \
           preferred_username = COALESCE(EXCLUDED.preferred_username, ap_remote_actors.preferred_username), \
           display_name = COALESCE(EXCLUDED.display_name, ap_remote_actors.display_name), \
           avatar_url = COALESCE(EXCLUDED.avatar_url, ap_remote_actors.avatar_url), \
           fetched_at = now(), updated_at = now()",
    )
    .bind(&a.actor_id)
    .bind(&a.inbox_url)
    .bind(&a.shared_inbox_url)
    .bind(&a.public_key_pem)
    .bind(&a.preferred_username)
    .bind(&a.display_name)
    .bind(&a.avatar_url)
    .execute(pool)
    .await
    .map(|_| ())
}

fn cache_is_fresh(fetched_at: chrono::DateTime<chrono::Utc>) -> bool {
    chrono::Utc::now() - fetched_at < CACHE_TTL
}

/// Resolve an actor's delivery targets for an *outbound* activity, fetching
/// on cache miss. Unlike [`load_or_fetch_actor`] this is alias-tolerant:
/// audience entries arrive in the form the producer stored (Mastodon
/// `/@user`), while the served actor doc carries the canonical id
/// (`/users/user`). Accepts a doc whose `id` is same-origin with the fetched
/// URL and caches under both keys so neither form re-fetches.
///
/// Returns `None` on any failure — delivery targeting is best-effort and must
/// never fail the write that triggered it. `signer` authorizes the fetch:
/// authorized-fetch instances (e.g. vivaldi.net) answer unsigned actor GETs
/// with 401, so the dereference path must sign or it resolves nothing there.
pub async fn resolve_delivery_actor(
    pool: &sqlx::PgPool,
    actor_url: &str,
    signer: Option<(&str, &str)>,
) -> Option<RemoteActor> {
    if let Some(a) = load_cached_actor(pool, actor_url).await {
        return Some(a);
    }
    let doc = get_json(actor_url, signer).await.ok()?;
    let doc_id = doc
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(actor_url);
    // Alias tolerance with origin pinning: the doc may name a canonical id
    // (Mastodon serves the same actor at `/@user` and `/users/user`) but never
    // an id on another origin.
    if doc_id != actor_url && !same_origin(doc_id, actor_url) {
        tracing::warn!("[AP] actor doc {actor_url} claims foreign id {doc_id}; refusing");
        return None;
    }
    // Parse against the doc's own id (strict checks: key ownership etc.).
    let actor = actor_from_doc(&doc, doc_id).ok()?;
    // Cache under the canonical id and, when different, under the alias the
    // caller asked about.
    if upsert_actor(pool, &actor).await.is_err() {
        return Some(actor); // resolution succeeded; cache write is best-effort
    }
    if doc_id != actor_url {
        let mut alias = actor.clone();
        alias.actor_id = actor_url.to_string();
        let _ = upsert_actor(pool, &alias).await;
    }
    Some(actor)
}

/// The `attributedTo` of a fetched document: a string, the first usable entry
/// of an array (string entries, or link/actor objects via `href`/`url`/`id`),
/// `None` when absent.
fn attributed_to_of(doc: &Value) -> Option<String> {
    let one = |v: &Value| -> Option<String> {
        match v {
            Value::String(s) if !s.is_empty() => Some(s.clone()),
            Value::Object(o) => o
                .get("href")
                .or_else(|| o.get("url"))
                .or_else(|| o.get("id"))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            _ => None,
        }
    };
    match doc.get("attributedTo") {
        Some(Value::Array(a)) => a.iter().find_map(one),
        other => other.and_then(one),
    }
}

/// The REAL author actor of a remote object, dereferenced at like time.
///
/// Why: `appview_posts.author_did` carries profile-page/API URLs that often do
/// not match the actor's canonical id (a minds profile `…/jahmind` vs the
/// served actor `…/api/activitypub/users/588529`), and the fallback of using
/// the *object URI* as the audience entry resolves to nothing — "no known
/// inbox … skipping" and the Like silently never reaches anyone. Fetching the
/// object's own `attributedTo` and resolving that actor yields the true
/// delivery target. Failures fall back to `hint` unchanged (best-effort: a
/// hint is never worse than the pre-fix behavior).
///
/// The fetch is signed (`signer`): authorized-fetch instances reject unsigned
/// GETs and would resolve nothing.
pub async fn object_author_actor(
    pool: &sqlx::PgPool,
    object_uri: &str,
    hint: &str,
    signer: Option<(&str, &str)>,
) -> String {
    if let Ok(doc) = get_json(object_uri, signer).await {
        if let Some(author) = attributed_to_of(&doc) {
            return match resolve_delivery_actor(pool, &author, signer).await {
                Some(a) => a.actor_id,
                None => author,
            };
        }
    }
    hint.to_string()
}

/// Same scheme + host + port?
fn same_origin(a: &str, b: &str) -> bool {
    match (url::Url::parse(a), url::Url::parse(b)) {
        (Ok(a), Ok(b)) => {
            a.scheme() == b.scheme() && a.host() == b.host() && a.port_or_known_default() == b.port_or_known_default()
        }
        _ => false,
    }
}

/// Why actor resolution failed. A transient (infrastructure) failure MUST
/// surface as retryable (5xx): misclassifying it as unresolvable (401) makes
/// remote senders stop retrying and drops the activity permanently — a pool
/// blip read as "cannot resolve signer key: dns …" cost real follows
/// (identity-spec's prod window, 2026-11-18 02:21–02:55 UTC).
#[derive(Debug)]
pub enum ResolveError {
    /// Infrastructure failure (DB/cache lookup) — retry later.
    Transient(String),
    /// The actor itself could not be resolved (fetch/DNS/parse).
    Unresolvable(String),
}

/// Resolve an actor by id: fresh cache first (24h, must carry a key), else
/// fetch. Never writes — persist with [`upsert_actor`] after verification.
pub async fn load_or_fetch_actor(
    pool: &sqlx::PgPool,
    actor_id: &str,
    signer: Option<(&str, &str)>,
) -> Result<RemoteActor, ResolveError> {
    let cached: Option<(RemoteActor, chrono::DateTime<chrono::Utc>)> = {
        let row = match sqlx::query_as::<
            _,
            (
                String,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                chrono::DateTime<chrono::Utc>,
            ),
        >(
            "SELECT actor_id, inbox_url, shared_inbox_url, public_key_pem, \
                    preferred_username, display_name, avatar_url, fetched_at \
             FROM ap_remote_actors WHERE actor_id = $1",
        )
        .bind(actor_id)
        .fetch_optional(pool)
        .await
        {
            Ok(row) => row,
            Err(e) => {
                // A DB error is NOT a cache miss: fail retryably instead of
                // falling through to a fetch that would misreport the signer.
                tracing::warn!(
                    "[AP remote] transient cache lookup failure for {actor_id}: {e}"
                );
                return Err(ResolveError::Transient(format!(
                    "remote actor cache lookup failed: {e}"
                )));
            }
        };
        row.map(|r| {
            (
                RemoteActor {
                    actor_id: r.0,
                    inbox_url: r.1,
                    shared_inbox_url: r.2,
                    public_key_pem: r.3,
                    preferred_username: r.4,
                    display_name: r.5,
                    avatar_url: r.6,
                },
                r.7,
            )
        })
    };
    if let Some((a, fetched_at)) = cached {
        if cache_is_fresh(fetched_at) && a.public_key_pem.is_some() {
            return Ok(a);
        }
    }
    fetch_actor(actor_id, signer)
        .await
        .map_err(ResolveError::Unresolvable)
}

/// Resolve `user@host` to its actor via WebFinger (`rel=self` activity+json
/// link), cache-first on (preferred_username, host).
pub async fn resolve_handle(
    pool: &sqlx::PgPool,
    signer: Option<(&str, &str)>,
    acct: &str,
) -> Result<RemoteActor, String> {
    let acct = acct.trim().trim_start_matches('@');
    let (user, host) = acct
        .split_once('@')
        .ok_or_else(|| format!("handle must be user@host, got {acct}"))?;
    if user.is_empty() || host.is_empty() {
        return Err(format!("handle must be user@host, got {acct}"));
    }

    // Cache: same username on the same host, fetched within the TTL.
    let cached: Option<(String,)> = sqlx::query_as(
        "SELECT actor_id FROM ap_remote_actors \
         WHERE preferred_username = $1 AND actor_id LIKE 'http%' AND fetched_at > $2 \
         ORDER BY fetched_at DESC LIMIT 1",
    )
    .bind(user)
    .bind(chrono::Utc::now() - CACHE_TTL)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    if let Some((actor_id,)) = cached {
        let same_host = url::Url::parse(&actor_id)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.eq_ignore_ascii_case(host)))
            .unwrap_or(false);
        if same_host {
            if let Some(a) = load_cached_actor(pool, &actor_id).await {
                return Ok(a);
            }
        }
    }

    let resource = format!("acct:{user}@{host}");
    let wf_url = format!(
        "https://{host}/.well-known/webfinger?resource={}",
        urlencoding::encode(&resource)
    );
    let doc = get_json(&wf_url, signer).await?;
    let links = doc
        .get("links")
        .and_then(|l| l.as_array())
        .ok_or_else(|| format!("webfinger {acct}: no links"))?;
    let actor_url = links
        .iter()
        .find(|l| {
            l.get("rel").and_then(|r| r.as_str()) == Some("self")
                && l.get("type")
                    .and_then(|t| t.as_str())
                    .map(|t| {
                        t.contains("activity+json") || t.contains("ld+json")
                    })
                    .unwrap_or(false)
        })
        .and_then(|l| l.get("href").and_then(|h| h.as_str()))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("webfinger {acct}: no activity+json self link"))?
        .to_string();
    fetch_actor(&actor_url, signer).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_rejects_private_and_non_https_targets() {
        // Non-https is refused outright.
        assert!(guard_url("http://93.184.216.34/x").is_err());
        // Loopback / private / link-local / CGNAT / reserved, v4 + v6.
        for url in [
            "https://127.0.0.1/x",
            "https://10.0.0.1/x",
            "https://172.16.0.9/x",
            "https://192.168.1.5/x",
            "https://169.254.169.254/latest/meta-data",
            "https://100.64.0.1/x",
            "https://240.0.0.1/x",
            "https://[::1]/x",
            "https://[fc00::1]/x",
            "https://[fe80::1]/x",
            "https://[::ffff:127.0.0.1]/x",
        ] {
            assert!(guard_url(url).is_err(), "must reject {url}");
        }
    }

    #[test]
    fn guard_allows_public_targets() {
        // Literal public addresses (no DNS in tests).
        assert!(guard_url("https://93.184.216.34/x").is_ok());
        assert!(guard_url("https://8.8.8.8/x").is_ok());
    }

    #[test]
    fn actor_from_doc_enforces_id_and_key_ownership() {
        let doc = serde_json::json!({
            "id": "https://remote.test/users/alice",
            "type": "Person",
            "preferredUsername": "alice",
            "name": "Alice",
            "inbox": "https://remote.test/users/alice/inbox",
            "endpoints": {"sharedInbox": "https://remote.test/inbox"},
            "publicKey": {
                "id": "https://remote.test/users/alice#main-key",
                "owner": "https://remote.test/users/alice",
                "publicKeyPem": "PEM"
            },
            "icon": {"type": "Image", "url": "https://remote.test/a.png"}
        });
        let a = actor_from_doc(&doc, "https://remote.test/users/alice").expect("parse");
        assert_eq!(a.actor_id, "https://remote.test/users/alice");
        assert_eq!(a.inbox_url, "https://remote.test/users/alice/inbox");
        assert_eq!(a.shared_inbox_url.as_deref(), Some("https://remote.test/inbox"));
        assert_eq!(a.delivery_inbox(), "https://remote.test/inbox");
        assert_eq!(a.public_key_pem.as_deref(), Some("PEM"));
        assert_eq!(a.preferred_username.as_deref(), Some("alice"));
        assert_eq!(a.avatar_url.as_deref(), Some("https://remote.test/a.png"));

        // id mismatch → rejected.
        assert!(actor_from_doc(&doc, "https://remote.test/users/bob").is_err());

        // Key owned by someone else → rejected.
        let mut spoofed = doc.clone();
        spoofed["publicKey"]["owner"] = serde_json::json!("https://remote.test/users/bob");
        assert!(actor_from_doc(&spoofed, "https://remote.test/users/alice").is_err());
    }

    #[test]
    fn attributed_to_of_extracts_the_real_actor_id() {
        // Plain string.
        assert_eq!(
            attributed_to_of(&serde_json::json!({"attributedTo": "https://remote.test/users/z"})),
            Some("https://remote.test/users/z".to_string())
        );
        // Array: first usable entry wins (link objects via `href`).
        assert_eq!(
            attributed_to_of(&serde_json::json!({
                "attributedTo": [
                    {"type": "Link", "href": "https://remote.test/users/z"},
                    "https://remote.test/users/y"
                ]
            })),
            Some("https://remote.test/users/z".to_string())
        );
        // Embedded actor object (Mastodon serves the actor inline).
        assert_eq!(
            attributed_to_of(&serde_json::json!({
                "attributedTo": {"id": "https://remote.test/users/z", "type": "Person"}
            })),
            Some("https://remote.test/users/z".to_string())
        );
        // Absent / empty → None (caller keeps its hint).
        assert_eq!(attributed_to_of(&serde_json::json!({"type": "Note"})), None);
        assert_eq!(attributed_to_of(&serde_json::json!({"attributedTo": ""})), None);
    }

    #[tokio::test]
    async fn object_author_actor_keeps_the_hint_when_dereference_fails() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else {
            return;
        };
        // Loopback is SSRF-guarded, so the object fetch fails deterministically
        // and offline: the dereference is best-effort and must never degrade
        // below the pre-fix audience hint (unsigned AND signed callers).
        let hint = "https://remote.test/users/z";
        let got = object_author_actor(&pool, "https://127.0.0.1/statuses/1", hint, None).await;
        assert_eq!(got, hint, "failed dereference falls back to the hint");
        let got = object_author_actor(
            &pool,
            "https://127.0.0.1/statuses/1",
            hint,
            Some(("https://remote.test/users/y#main-key", "PEM")),
        )
        .await;
        assert_eq!(got, hint, "signed caller degrades the same way");
    }

    /// Owner QA 2026-11-18 (identity-spec's prod window): a DB error during
    /// the cache lookup is NOT a cache miss. It must classify as Transient
    /// (the inbox maps that to a retryable 5xx) — never fall through to a
    /// fetch that misreports the signer as unresolvable DNS via 401, which
    /// makes the remote sender stop retrying and drops the follow silently.
    #[tokio::test]
    async fn cache_db_error_is_transient_not_a_silent_miss() {
        // A pool that can never serve a query = the saturation blip, offline
        // and deterministic (acquire times out → sqlx error).
        let dead = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(std::time::Duration::from_millis(300))
            .connect_lazy("postgres://127.0.0.1:1/dead")
            .expect("lazy pool");
        let err = load_or_fetch_actor(&dead, "https://probe.ap.test/users/x", None)
            .await
            .expect_err("dead pool must not resolve an actor");
        assert!(
            matches!(err, ResolveError::Transient(_)),
            "DB failure must classify Transient, got: {err:?}"
        );
    }
}