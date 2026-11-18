//! Post-history backfill on fediverse sign-in (Phase B) — behind
//! `AP_INGEST_ENABLED`.
//!
//! When a user signs in with Mastodon (or any AP instance), their own AP
//! outbox is fetched and every activity on it is passed to
//! [`crate::ap::ingest::ingest_activity`], which upserts `appview_posts` /
//! `appview_stories` rows idempotently (`ON CONFLICT (uri) DO UPDATE`). The
//! ingest is additive: only `https://` AP rows are written; the AT (Bluesky)
//! read path is untouched.
//!
//! The triggers live in `api::oauth::{mastodon_callback, oauth_complete}` and
//! call [`spawn_history_backfill`]: fire-and-forget, so sign-in tokens are
//! returned synchronously and a failed backfill is only logged.

use serde_json::Value;

/// Bound on outbox pages fetched per backfill (Mastodon pages hold 20
/// activities → up to ~400 per sign-in), so the background task stays cheap
/// no matter how large the account's history is.
const MAX_PAGES: usize = 20;

const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

const ACCEPT: &str = "application/activity+json, \
                      application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\", \
                      application/json";

const USER_AGENT: &str = "Bytescape/1.0 (+activitypub-backfill)";

/// Fire-and-forget backfill of `acct`'s own AP outbox, spawned from the
/// Mastodon OAuth completion flow.
///
/// - No-op unless `AP_INGEST_ENABLED` is true (and again inside
///   [`backfill_user_history`] / `ingest_activity`).
/// - Never fails sign-in: the task runs in the background; errors are logged.
/// - `acct` may be a bare local username (`alice`); `instance` fills in the
///   missing domain so WebFinger can resolve it to an actor URL.
/// - `user_id` is the fetching user: their issued AP actor signs every AP GET
///   (authorized fetch); without one the fetches go out unsigned (warned).
pub fn spawn_history_backfill(
    pool: sqlx::PgPool,
    acct: &str,
    instance: &str,
    user_id: uuid::Uuid,
    jwt_secret: String,
) {
    if !crate::ap::ap_ingest_enabled() {
        return;
    }
    let handle = match normalize_handle(acct, instance) {
        Some(h) => h,
        None => {
            tracing::warn!("[AP] backfill skipped: cannot build a handle from acct {acct:?}");
            return;
        }
    };
    tokio::spawn(async move {
        let signer = crate::ap::keys::load_fetch_signer(&pool, user_id, &jwt_secret).await;
        tracing::warn!("[AP-DBG] backfill signer for {handle}: {}", if signer.is_some() { "SOME (signed)" } else { "NONE (unsigned)" });
        match backfill_handle(&pool, &handle, signer.as_ref()).await {
            Ok(n) => tracing::info!("[AP] history backfill for {handle}: {n} rows ingested"),
            Err(e) => tracing::warn!("[AP] history backfill failed for {handle}: {e}"),
        }
    });
}

/// Resolve `@user@instance` via WebFinger, then backfill that actor's outbox.
async fn backfill_handle(
    pool: &sqlx::PgPool,
    handle: &str,
    signer: Option<&(String, String)>,
) -> Result<usize, String> {
    let actor_url = actor_url_for_handle(handle, signer).await?;
    backfill_user_history(pool, &actor_url, signer, Some(handle)).await
}

/// Fetch `actor_url`'s own AP outbox and ingest every activity on it via
/// [`crate::ap::ingest::ingest_activity`].
///
/// Follows the collection's `first`/`next` pagination up to [`MAX_PAGES`]
/// pages (a later page failing keeps what was already ingested; the first
/// fetch failing is an error). Returns the total rows ingested — 0 when the
/// flag is off or nothing on the outbox was recognized as a post.
pub async fn backfill_user_history(
    pool: &sqlx::PgPool,
    actor_url: &str,
    signer: Option<&(String, String)>,
    handle_override: Option<&str>,
) -> Result<usize, String> {
    if !crate::ap::ap_ingest_enabled() {
        return Ok(0);
    }
    let client = http_client()?;
    let mut url = resolve_outbox(&client, actor_url, signer).await;
    let mut total = 0usize;
    let mut pages = 0usize;
    while pages < MAX_PAGES {
        let page = match get_json(&client, &url, signer).await {
            Ok(p) => p,
            Err(e) => {
                if pages == 0 {
                    return Err(e);
                }
                tracing::warn!("[AP] backfill stopped after {pages} pages: {e}");
                break;
            }
        };
        pages += 1;
        if let Some(items) = page.get("orderedItems").and_then(Value::as_array) {
            for item in items {
                match crate::ap::ingest::ingest_activity(pool, item, handle_override).await {
                    Ok(n) => total += n,
                    Err(e) => tracing::warn!("[AP] backfill entry skipped: {e}"),
                }
            }
        }
        // A bare OrderedCollection carries no items — hop to its first page;
        // a page with items hops via `next`.
        let hop = next_link(&page, "next")
            .or_else(|| next_link(&page, "first").filter(|_| page.get("orderedItems").is_none()));
        match hop {
            Some(next) => url = next,
            None => break,
        }
    }
    Ok(total)
}

/// The actor document's declared `outbox`, falling back to `{actor}/outbox`.
/// A failed actor fetch only means the fallback is tried — the outbox fetch
/// itself reports the real error.
async fn resolve_outbox(
    client: &reqwest::Client,
    actor_url: &str,
    signer: Option<&(String, String)>,
) -> String {
    if let Ok(actor) = get_json(client, actor_url, signer).await
        && let Some(href) = next_link(&actor, "outbox")
    {
        return href;
    }
    format!("{}/outbox", actor_url.trim_end_matches('/'))
}

/// Resolve `user@host` to the canonical AP actor URL via WebFinger
/// (`/.well-known/webfinger` → the `rel="self"` activity+json link).
async fn actor_url_for_handle(
    handle: &str,
    signer: Option<&(String, String)>,
) -> Result<String, String> {
    let (user, domain) = handle
        .split_once('@')
        .ok_or_else(|| format!("not a user@host handle: {handle}"))?;
    let acct = format!("{user}@{domain}");
    let resource = urlencoding::encode(&acct);
    let url = format!("https://{domain}/.well-known/webfinger?resource=acct:{resource}");
    let doc = get_json(&http_client()?, &url, signer).await?;
    doc.get("links")
        .and_then(Value::as_array)
        .and_then(|links| {
            links.iter().find(|l| {
                let is_self = l.get("rel").and_then(Value::as_str) == Some("self");
                let is_ap = l.get("type").and_then(Value::as_str).is_some_and(|t| {
                    t.contains("activity+json") || t.contains("activitystreams")
                });
                is_self && is_ap
            })
        })
        .and_then(|l| l.get("href"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("webfinger for {handle}: no AP self link"))
}

/// `alice` + `mastodon.social` → `alice@mastodon.social`; `@bob@other.social`
/// passes through (leading `@` stripped). `None` when no domain can be built.
fn normalize_handle(acct: &str, instance: &str) -> Option<String> {
    let acct = acct.trim().trim_start_matches('@');
    if acct.is_empty() {
        return None;
    }
    if acct.contains('@') {
        return Some(acct.to_string());
    }
    let instance = instance.trim().trim_start_matches('@');
    if instance.is_empty() {
        return None;
    }
    Some(format!("{acct}@{instance}"))
}

/// `next`/`first`/`outbox` may be a bare URL string or an ActivityStreams
/// Link object (`{"href": …}`).
fn next_link(page: &Value, key: &str) -> Option<String> {
    match page.get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) => o.get("href").and_then(Value::as_str).map(str::to_string),
        _ => None,
    }
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| format!("http client: {e}"))
}

/// GET `url` and parse the JSON body.
///
/// When `signer` is `Some((key_id, privkey_pem))` the request carries a
/// draft-cavage `Signature` plus the `Host` / `Date` / `Accept` headers it
/// covers (see [`crate::ap::http_sig::signed_get_headers`]); `None` sends the
/// plain unsigned GET (the pre-signing behavior).
async fn get_json(
    client: &reqwest::Client,
    url: &str,
    signer: Option<&(String, String)>,
) -> Result<Value, String> {
    let mut req = client
        .get(url)
        .header(reqwest::header::ACCEPT, ACCEPT)
        .header(reqwest::header::USER_AGENT, USER_AGENT);
    for (name, value) in crate::ap::http_sig::signed_get_headers(
        url,
        signer.map(|(k, p)| (k.as_str(), p.as_str())),
        ACCEPT,
    ) {
        req = req.header(name, value);
    }
    let resp = req.send().await.map_err(|e| format!("GET {url}: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        tracing::warn!("[AP-DBG] GET {url} -> {status}; signer={} body={}", signer.is_some(), &body[..body.len().min(200)]);
        return Err(format!("GET {url}: HTTP {status}"));
    }
    resp.json().await.map_err(|e| format!("GET {url}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalize_handle_fills_missing_domain_and_passes_full_handles() {
        assert_eq!(
            normalize_handle("alice", "mastodon.social").as_deref(),
            Some("alice@mastodon.social")
        );
        assert_eq!(
            normalize_handle("@bob@other.social", "").as_deref(),
            Some("bob@other.social")
        );
        // No instance to fill in → cannot build a handle.
        assert_eq!(normalize_handle("alice", ""), None);
        assert_eq!(normalize_handle("", "mastodon.social"), None);
    }

    #[test]
    fn next_link_accepts_bare_urls_and_link_objects() {
        assert_eq!(
            next_link(&json!({ "next": "https://x.test/outbox?page=2" }), "next").as_deref(),
            Some("https://x.test/outbox?page=2")
        );
        assert_eq!(
            next_link(&json!({ "first": { "href": "https://x.test/outbox?page=1" } }), "first")
                .as_deref(),
            Some("https://x.test/outbox?page=1")
        );
        assert_eq!(next_link(&json!({}), "next"), None);
    }
}
