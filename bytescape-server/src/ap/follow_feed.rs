//! Follow surface + interest hashtags: pull cross-protocol (ActivityPub)
//! content into the feed (Phase B) — behind `AP_INGEST_ENABLED`.
//!
//! Two entry points:
//! * [`ingest_from_actor_outbox`] — fetch one AP actor's outbox and pass each
//!   activity to [`crate::ap::ingest::ingest_activity`] (bounded by `limit`).
//! * [`populate_interests`] — for a signed-in user: ingest from the AP
//!   actors behind their imported Mastodon contacts (`imported_contacts`,
//!   `source = 'mastodon'`) and best-effort pull their interest hashtags from
//!   the public tag timelines of instances those contacts live on.
//!
//! Everything here is additive: only `https://` AP rows are written, through
//! the ingest API, idempotently (`ON CONFLICT (uri) DO UPDATE` handled by
//! [`crate::ap::ingest::ingest_activity`]). Network failures never panic —
//! they stop the walk and keep whatever was ingested (`Ok(0)` / count-so-far).
//! With `AP_INGEST_ENABLED` off, both entry points no-op (`Ok(0)`).

// Integration seam: `populate_interests` / `ingest_from_actor_outbox` are
// called by the parent wiring (feed warm-up / sign-in), not from within this
// module yet.
#![allow(dead_code)]

use serde_json::Value;

/// Bound on contacts resolved per populate run, so one huge archive cannot
/// fan out into an unbounded number of outbox fetches. Matches the live
/// follow-import cap (`ap::follow_import::MAX_RESOLVES_PER_RUN`): a graph the
/// importer stored should be ingestible in one pass, not 12 contacts at a time.
const MAX_CONTACTS: usize = 40;
/// Activities ingested per followed contact's outbox.
const PER_CONTACT_LIMIT: usize = 10;
/// Outbox pages walked per actor (a Mastodon page holds 20 activities, so
/// `PER_CONTACT_LIMIT` normally completes on the first page).
const MAX_PAGES: usize = 3;
/// Interest hashtags pulled per populate run.
const MAX_INTERESTS: usize = 6;
/// Instances queried per hashtag (the user's own + their contacts' instances).
const MAX_TAG_INSTANCES: usize = 3;
/// Statuses pulled per hashtag-timeline query.
const TAG_TIMELINE_LIMIT: usize = 10;

const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

const ACCEPT: &str = "application/activity+json, \
                      application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\", \
                      application/json";

const USER_AGENT: &str = "Bytescape/1.0 (+activitypub-follow-feed)";

/// Extract the activity objects from an outbox / collection-page document.
///
/// Prefers `orderedItems`, falls back to `items`, and also accepts a bare
/// JSON array (some servers return one). Non-object entries (IRI references,
/// stray scalars) are dropped — [`crate::ap::ingest::ingest_activity`] only
/// consumes already-parsed AP objects. Pure and DB-free by design so the
/// outbox walk is unit-testable without network.
pub fn activities_from_outbox(v: &Value) -> Vec<Value> {
    let items = match v {
        Value::Array(a) => a,
        Value::Object(_) => match v.get("orderedItems").or_else(|| v.get("items")) {
            Some(Value::Array(a)) => a,
            _ => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    items.iter().filter(|a| a.is_object()).cloned().collect()
}

/// Fetch `actor_url`'s AP outbox and ingest up to `limit` activities from it
/// via [`crate::ap::ingest::ingest_activity`].
///
/// Walks `orderedItems` / `items`, following `next` / `first` pagination up
/// to [`MAX_PAGES`] pages. `actor_url` may be an actor document, a profile
/// URL that content-negotiates to one, or an already-resolved collection
/// URL. Fetch failure stops the walk and returns what was ingested so far
/// (`Ok(0)` on the first fetch); only a client-build failure is an `Err`.
/// No-op (`Ok(0)`) when `AP_INGEST_ENABLED` is off or `actor_url` is not
/// `https://`.
///
/// `signer` (`(key_id, privkey_pem)`) signs each AP GET for authorized-fetch
/// instances; `None` fetches unsigned.
pub async fn ingest_from_actor_outbox(
    pool: &sqlx::PgPool,
    actor_url: &str,
    limit: usize,
    signer: Option<&(String, String)>,
) -> Result<usize, String> {
    if !crate::ap::ap_ingest_enabled() || limit == 0 {
        return Ok(0);
    }
    if !actor_url.starts_with("https://") {
        return Ok(0);
    }
    let client = http_client()?;

    // The fetched document may itself be a collection page (a contact's
    // profile URL can point straight at one); otherwise read the actor's
    // declared `outbox`, falling back to its first page, then `{actor}/outbox`.
    let (mut url, mut page) = match get_json(&client, actor_url, signer).await {
        Ok(doc)
            if doc
                .get("orderedItems")
                .or_else(|| doc.get("items"))
                .is_some_and(Value::is_array) =>
        {
            (String::new(), Some(doc))
        }
        Ok(doc) => (
            link_href(&doc, "outbox")
                .or_else(|| link_href(&doc, "first"))
                .unwrap_or_else(|| format!("{}/outbox", actor_url.trim_end_matches('/'))),
            None,
        ),
        Err(_) => (format!("{}/outbox", actor_url.trim_end_matches('/')), None),
    };

    let mut total = 0usize;
    let mut walked = 0usize;
    let mut pages = 0usize;
    while pages < MAX_PAGES && walked < limit {
        let current = match page.take() {
            Some(p) => p,
            None => match get_json(&client, &url, signer).await {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!("[AP] follow-feed outbox fetch {url} failed: {e}");
                    break;
                }
            },
        };
        pages += 1;
        for activity in activities_from_outbox(&current)
            .into_iter()
            .take(limit - walked)
        {
            walked += 1;
            match crate::ap::ingest::ingest_activity(pool, &activity, None).await {
                Ok(n) => total += n,
                Err(e) => tracing::warn!("[AP] follow-feed entry skipped: {e}"),
            }
        }
        if walked >= limit {
            break;
        }
        // A bare collection carries no inline items — hop to its first page;
        // a page with items hops via `next`.
        let hop = link_href(&current, "next").or_else(|| {
            link_href(&current, "first")
                .filter(|_| current.get("orderedItems").is_none() && current.get("items").is_none())
        });
        match hop {
            Some(next) => url = next,
            None => break,
        }
    }
    Ok(total)
}

/// Pull cross-protocol content for one user: ingest recent activities from
/// the AP actors behind their imported Mastodon contacts, then best-effort
/// pull their interest hashtags from the public tag timelines of the
/// instances those contacts (and the user's own linked Mastodon account)
/// live on.
///
/// Deterministic core first (contacts' outboxes — resolvable, bounded), then
/// the open-ended hashtag pass (every step best-effort: a failed webfinger,
/// outbox, or timeline fetch is logged and skipped, never fatal). Returns the
/// total rows ingested; `Ok(0)` when the flag is off or nothing was found.
///
/// `user_id` + `jwt_secret` resolve the fetching user's AP signing identity
/// (see [`crate::ap::keys::load_fetch_signer`]); without an issued actor every
/// fetch goes out unsigned (warned).
pub async fn populate_interests(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    jwt_secret: &str,
) -> Result<usize, String> {
    if !crate::ap::ap_ingest_enabled() {
        return Ok(0);
    }
    let user = crate::db::users::UserRow::find_by_id(pool, user_id)
        .await
        .map_err(|e| format!("load user {user_id}: {e}"))?
        .ok_or_else(|| format!("user {user_id} not found"))?;
    let signer = crate::ap::keys::load_fetch_signer(pool, user_id, jwt_secret).await;
    let signer = signer.as_ref();
    let mut total = 0usize;

    // ── Follow surface: the AP actors behind imported Mastodon contacts ──
    let (actor_urls, mut instances) = ap_contacts(pool, user_id, signer).await;
    for actor_url in &actor_urls {
        match ingest_from_actor_outbox(pool, actor_url, PER_CONTACT_LIMIT, signer).await {
            Ok(n) => total += n,
            Err(e) => tracing::warn!("[AP] follow-feed outbox {actor_url} failed: {e}"),
        }
    }

    // ── Interest hashtags: public tag timelines on known instances ────────
    if let Some(instance) = own_mastodon_instance(pool, user_id).await {
        push_unique(&mut instances, instance);
    }
    let tags: Vec<String> = user
        .interests
        .iter()
        .filter_map(|raw| normalize_tag(raw))
        .take(MAX_INTERESTS)
        .collect();
    for tag in &tags {
        for instance in instances.iter().take(MAX_TAG_INSTANCES) {
            match ingest_tag_timeline(pool, instance, tag, TAG_TIMELINE_LIMIT, signer).await {
                Ok(n) => total += n,
                Err(e) => tracing::debug!("[AP] tag timeline {tag}@{instance} skipped: {e}"),
            }
        }
    }
    Ok(total)
}

/// Resolve the user's imported AP contacts (`imported_contacts`,
/// `source = 'mastodon'`) to `(actor_urls, instance_hosts)`, capped at
/// [`MAX_CONTACTS`].
///
/// An actor URL comes from WebFingering the contact's `remote_handle`
/// (canonical `@user@instance` from the archive); when WebFinger fails or the
/// handle is missing, the contact's `contact_platform_url` (an `https://`
/// profile URL) is used directly — `ingest_from_actor_outbox` content-
/// negotiates it to the actor document. Instance hosts are collected from
/// both fields for the hashtag pass. All failures degrade to skipping that
/// contact. `signer` signs the WebFinger lookups (see [`webfinger_actor`]).
async fn ap_contacts(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    signer: Option<&(String, String)>,
) -> (Vec<String>, Vec<String>) {
    let rows: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
        r#"SELECT remote_handle, contact_platform_url
           FROM imported_contacts
           WHERE user_id = $1 AND source = 'mastodon'
             AND (remote_handle IS NOT NULL OR contact_platform_url IS NOT NULL)"#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_else(|e| {
        tracing::warn!("[AP] follow-feed contact load failed: {e}");
        Vec::new()
    });

    let mut actors = Vec::new();
    let mut instances = Vec::new();
    for (remote_handle, platform_url) in rows.into_iter().take(MAX_CONTACTS) {
        if let Some(host) = remote_handle.as_deref().and_then(domain_of_handle) {
            push_unique(&mut instances, host);
        }
        if let Some(host) = platform_url.as_deref().and_then(host_of_url) {
            push_unique(&mut instances, host);
        }
        let actor = match remote_handle
            .as_deref()
            .and_then(normalize_handle)
            .as_deref()
        {
            Some(handle) => match webfinger_actor(handle, signer).await {
                Ok(url) => Some(url),
                Err(e) => {
                    tracing::debug!("[AP] follow-feed webfinger {handle} failed: {e}");
                    platform_url
                        .as_deref()
                        .filter(|u| u.starts_with("https://"))
                        .map(str::to_string)
                }
            },
            None => platform_url
                .as_deref()
                .filter(|u| u.starts_with("https://"))
                .map(str::to_string),
        };
        if let Some(url) = actor {
            push_unique(&mut actors, url);
        }
    }
    (actors, instances)
}

/// The instance host of the user's own linked Mastodon account — the best
/// hashtag-timeline source for a fediverse-signed-in user. Prefers the
/// `instance` stored in `linked_accounts.metadata` at OAuth completion;
/// falls back to the domain of `external_id` (`@user@instance`).
async fn own_mastodon_instance(pool: &sqlx::PgPool, user_id: uuid::Uuid) -> Option<String> {
    let row: Option<(String, Value)> = sqlx::query_as(
        "SELECT external_id, metadata FROM linked_accounts \
         WHERE user_id = $1 AND provider = 'mastodon' LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .ok()?;
    let (external_id, metadata) = row?;
    metadata
        .get("instance")
        .and_then(Value::as_str)
        .map(|s| s.to_ascii_lowercase())
        .or_else(|| domain_of_handle(&external_id))
}

/// Pull one hashtag's public timeline from `instance`
/// (`/api/v1/timelines/tag/{tag}`) and ingest each status through
/// [`crate::ap::ingest::ingest_activity`], bridged into an AP `Create`+`Note`
/// by [`create_activity_from_rest_status`]. Best-effort: transport and shape
/// failures are `Err` for the caller to skip; per-entry ingest failures are
/// logged and the rest continue.
async fn ingest_tag_timeline(
    pool: &sqlx::PgPool,
    instance: &str,
    tag: &str,
    limit: usize,
    signer: Option<&(String, String)>,
) -> Result<usize, String> {
    if limit == 0 {
        return Ok(0);
    }
    let client = http_client()?;
    let url = format!(
        "https://{instance}/api/v1/timelines/tag/{}?limit={limit}",
        urlencoding::encode(tag)
    );
    let mut req = client
        .get(&url)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, USER_AGENT);
    for (name, value) in crate::ap::http_sig::signed_get_headers(
        &url,
        signer.map(|(k, p)| (k.as_str(), p.as_str())),
        "application/json",
    ) {
        req = req.header(name, value);
    }
    let resp = req.send().await.map_err(|e| format!("GET {url}: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("GET {url}: HTTP {status}"));
    }
    let v: Value = resp.json().await.map_err(|e| format!("GET {url}: {e}"))?;
    let statuses = v
        .as_array()
        .ok_or_else(|| format!("GET {url}: expected a status list"))?;
    let mut total = 0usize;
    for status in statuses.iter().take(limit) {
        // A boost carries the original post in `reblog`; index that instead
        // of the (usually empty) boost wrapper.
        let status = status.get("reblog").filter(|r| r.is_object()).unwrap_or(status);
        let Some(activity) = create_activity_from_rest_status(status) else {
            continue;
        };
        match crate::ap::ingest::ingest_activity(pool, &activity, None).await {
            Ok(n) => total += n,
            Err(e) => tracing::warn!("[AP] follow-feed tag entry skipped: {e}"),
        }
    }
    Ok(total)
}

/// Bridge one Mastodon REST status into the AP shape
/// `Create { object: Note }` that [`crate::ap::ingest::ingest_activity`]
/// dispatches on. Maps the REST fields onto their AP counterparts:
/// `uri` → `id`/`url`, `account.url` → `attributedTo`, `created_at` →
/// `published`, `media_attachments[].url` → `attachment`, `tags[].name` →
/// `tag`, and — so the ingest can label them — `sensitive` → `sensitive`
/// plus `spoiler_text` → `summary` (Mastodon's content warning; the ingest
/// maps the pair to the `nsfw` / `!warn` labels). `None` when the status
/// lacks the identity fields the ingest needs.
pub(crate) fn create_activity_from_rest_status(status: &Value) -> Option<Value> {
    let uri = status.get("uri").and_then(Value::as_str)?;
    let actor = status.pointer("/account/url").and_then(Value::as_str)?;
    let published = status.get("created_at").and_then(Value::as_str)?;
    let content = status.get("content").and_then(Value::as_str).unwrap_or("");
    let attachment: Vec<Value> = status
        .get("media_attachments")
        .and_then(Value::as_array)
        .map(|media| {
            media
                .iter()
                .filter_map(|m| {
                    let url = m.get("url").or_else(|| m.get("remote_url")).and_then(Value::as_str)?;
                    Some(serde_json::json!({ "type": "Image", "url": url }))
                })
                .collect()
        })
        .unwrap_or_default();
    let tag: Vec<Value> = status
        .get("tags")
        .and_then(Value::as_array)
        .map(|tags| {
            tags
                .iter()
                .filter_map(|t| {
                    let name = t.get("name").and_then(Value::as_str)?;
                    Some(serde_json::json!({ "type": "Hashtag", "name": format!("#{name}") }))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(serde_json::json!({
        "type": "Create",
        "object": {
            "type": "Note",
            "id": uri,
            "url": uri,
            "attributedTo": actor,
            "content": content,
            "published": published,
            "sensitive": status.get("sensitive").and_then(Value::as_bool).unwrap_or(false),
            // Mastodon's CW. Empty means "no warning"; `labels_of` only
            // treats a non-empty Note summary as a warning.
            "summary": status.get("spoiler_text").and_then(Value::as_str).unwrap_or(""),
            "attachment": attachment,
            "tag": tag,
            // Reply linkage: `in_reply_to_uri` is the parent's object URL (a
            // post or another comment) and `in_reply_to_id` is the thread root
            // hint. Without these the ingest writes `reply_parent = NULL`, so
            // replies surface as top-level posts and never attach to their
            // thread (the "comments don't show under a post" bug).
            "inReplyTo": status.get("in_reply_to_uri").and_then(Value::as_str),
            "context": status.get("in_reply_to_uri").and_then(Value::as_str),
        }
    }))
}

/// Resolve `user@host` to the canonical AP actor URL via WebFinger
/// (`/.well-known/webfinger` → the `rel="self"` activity+json link).
async fn webfinger_actor(
    handle: &str,
    signer: Option<&(String, String)>,
) -> Result<String, String> {
    let (user, domain) = handle
        .split_once('@')
        .ok_or_else(|| format!("not a user@host handle: {handle}"))?;
    let resource = urlencoding::encode(&format!("{user}@{domain}")).to_string();
    let url = format!("https://{domain}/.well-known/webfinger?resource=acct:{resource}");
    let doc = get_json(&http_client()?, &url, signer).await?;
    let href = doc
        .get("links")
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
        .and_then(|l| link_href(l, "href"))
        .ok_or_else(|| format!("webfinger for {handle}: no AP self link"))?;
    if !href.starts_with("https://") {
        return Err(format!("webfinger for {handle}: non-https actor {href}"));
    }
    Ok(href)
}

/// `@alice@instance` / `alice@instance` → `alice@instance`; a bare `alice`
/// with no domain → `None` (cannot be WebFingered without one).
fn normalize_handle(raw: &str) -> Option<String> {
    let h = raw.trim().trim_start_matches('@');
    if h.is_empty() {
        return None;
    }
    let (user, domain) = h.split_once('@')?;
    if user.is_empty() || domain.is_empty() {
        return None;
    }
    Some(format!("{user}@{domain}"))
}

/// The domain of `@user@instance` (any leading `@` tolerated), lowercased.
fn domain_of_handle(raw: &str) -> Option<String> {
    normalize_handle(raw)
        .and_then(|h| h.rsplit_once('@').map(|(_, d)| d.to_ascii_lowercase()))
}

/// Host of an `http(s)://host/...` URL (lowercased), `None` otherwise.
fn host_of_url(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

/// Normalize an onboarding interest to a fetchable hashtag: trim, drop a
/// leading `#`, lowercase; `None` for empty or URL-hostile input.
fn normalize_tag(raw: &str) -> Option<String> {
    let t = raw.trim().trim_start_matches('#').trim().to_lowercase();
    if t.is_empty() || t.chars().any(|c| c.is_whitespace() || "/?#&".contains(c)) {
        return None;
    }
    Some(t)
}

fn push_unique(list: &mut Vec<String>, value: String) {
    if !list.contains(&value) {
        list.push(value);
    }
}

/// `next`/`first`/`outbox`/`href` may be a bare URL string or an
/// ActivityStreams Link object (`{"href": …}`).
fn link_href(page: &Value, key: &str) -> Option<String> {
    match page.get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) => o.get("href").and_then(Value::as_str).map(str::to_string),
        _ => None,
    }
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .user_agent(USER_AGENT)
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
        .header(reqwest::header::ACCEPT, ACCEPT);
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
        return Err(format!("GET {url}: HTTP {status}"));
    }
    resp.json().await.map_err(|e| format!("GET {url}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// R6: a Mastodon REST status carries its NSFW flag and CW as
    /// `sensitive` / `spoiler_text`; both must survive the bridge into the AP
    /// shape the ingest reads (otherwise 77k+ AP rows carry no labels at all).
    #[test]
    fn rest_status_bridges_sensitive_and_spoiler_text() {
        let status = json!({
            "uri": "https://mastodon.test/users/alice/statuses/1",
            "account": { "url": "https://mastodon.test/@alice" },
            "created_at": "2026-11-01T12:00:00Z",
            "content": "<p>body</p>",
            "sensitive": true,
            "spoiler_text": "cw"
        });
        let activity = super::create_activity_from_rest_status(&status).expect("bridges");
        assert_eq!(activity["object"]["sensitive"], true);
        assert_eq!(activity["object"]["summary"], "cw");
        let note = crate::ap::ingest::parse_note(&activity).expect("parses");
        assert_eq!(note.labels, vec!["nsfw".to_string(), "!warn".to_string()]);

        // A clean status stays clean: empty spoiler_text is not a warning.
        let clean = json!({
            "uri": "https://mastodon.test/users/alice/statuses/2",
            "account": { "url": "https://mastodon.test/@alice" },
            "created_at": "2026-11-01T12:00:00Z",
            "content": "<p>body</p>",
            "sensitive": false,
            "spoiler_text": ""
        });
        let activity = super::create_activity_from_rest_status(&clean).expect("bridges");
        let note = crate::ap::ingest::parse_note(&activity).expect("parses");
        assert!(note.labels.is_empty());
    }

    #[test]
    fn outbox_walk_prefers_ordered_items_and_drops_non_objects() {
        let page = json!({
            "type": "OrderedCollectionPage",
            "orderedItems": [
                {"type": "Create", "id": "https://x.test/1"},
                "https://x.test/skipped-iri",
                42,
                {"type": "Note", "id": "https://x.test/2"}
            ]
        });
        let acts = activities_from_outbox(&page);
        assert_eq!(acts.len(), 2);
        assert_eq!(acts[0]["id"], "https://x.test/1");
        assert_eq!(acts[1]["id"], "https://x.test/2");
    }

    #[test]
    fn outbox_walk_falls_back_to_items_and_bare_arrays() {
        let page = json!({"type": "CollectionPage", "items": [{"type": "Create"}]});
        assert_eq!(activities_from_outbox(&page).len(), 1);

        let bare = json!([{"type": "Note"}, {"type": "Create"}]);
        assert_eq!(activities_from_outbox(&bare).len(), 2);

        assert!(activities_from_outbox(&json!({})).is_empty());
        assert!(activities_from_outbox(&json!({"orderedItems": "not-an-array"})).is_empty());
        assert!(activities_from_outbox(&json!("just a string")).is_empty());
    }

    #[test]
    fn link_href_handles_strings_and_link_objects() {
        assert_eq!(
            link_href(&json!({"next": "https://x.test/2"}), "next").as_deref(),
            Some("https://x.test/2")
        );
        assert_eq!(
            link_href(&json!({"first": {"href": "https://x.test/1"}}), "first").as_deref(),
            Some("https://x.test/1")
        );
        assert_eq!(link_href(&json!({}), "outbox"), None);
        assert_eq!(link_href(&json!({"next": 7}), "next"), None);
    }

    #[test]
    fn rest_status_bridges_into_create_note() {
        let status = json!({
            "uri": "https://host.test/users/alice/statuses/1",
            "created_at": "2026-01-02T03:04:05.000Z",
            "content": "<p>hello</p>",
            "account": {"url": "https://host.test/@alice"},
            "media_attachments": [{"url": "https://host.test/media/1.png"}],
            "tags": [{"name": "rust"}]
        });
        let act = create_activity_from_rest_status(&status).expect("wrapped");
        assert_eq!(act["type"], "Create");
        let note = &act["object"];
        assert_eq!(note["type"], "Note");
        assert_eq!(note["id"], "https://host.test/users/alice/statuses/1");
        assert_eq!(note["url"], "https://host.test/users/alice/statuses/1");
        assert_eq!(note["attributedTo"], "https://host.test/@alice");
        assert_eq!(note["content"], "<p>hello</p>");
        assert_eq!(note["published"], "2026-01-02T03:04:05.000Z");
        assert_eq!(note["attachment"][0]["url"], "https://host.test/media/1.png");
        assert_eq!(note["tag"][0]["name"], "#rust");

        // Missing identity fields → not wrappable.
        assert!(create_activity_from_rest_status(&json!({"uri": "https://x"})).is_none());
        assert!(create_activity_from_rest_status(&json!({})).is_none());
    }

    #[test]
    fn handles_hosts_and_tags_normalize() {
        assert_eq!(
            normalize_handle("@alice@mastodon.social").as_deref(),
            Some("alice@mastodon.social")
        );
        assert_eq!(
            normalize_handle("alice@mastodon.social").as_deref(),
            Some("alice@mastodon.social")
        );
        assert_eq!(normalize_handle("alice"), None);
        assert_eq!(normalize_handle("@"), None);

        assert_eq!(
            domain_of_handle("@Alice@Mastodon.Social").as_deref(),
            Some("mastodon.social")
        );
        assert_eq!(domain_of_handle("alice"), None);

        assert_eq!(
            host_of_url("https://Mastodon.Social/@bob").as_deref(),
            Some("mastodon.social")
        );
        assert_eq!(host_of_url("http://host.test:8080/x").as_deref(), Some("host.test:8080"));
        assert_eq!(host_of_url("not a url"), None);

        assert_eq!(normalize_tag(" #Rust "), Some("rust".to_string()));
        assert_eq!(normalize_tag("art"), Some("art".to_string()));
        assert_eq!(normalize_tag(""), None);
        assert_eq!(normalize_tag("#"), None);
        assert_eq!(normalize_tag("a/b"), None);
    }
}
