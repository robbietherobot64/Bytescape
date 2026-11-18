//! Standing ActivityPub feed puller.
//!
//! Polls public timelines on the managed source set ([`crate::ap::sources`]) and
//! ingests them through the single write path [`crate::ap::ingest::ingest_activity`]
//! (idempotent upsert keyed by AP object URL). This is what puts ActivityPub
//! content — Mastodon, Pixelfed, Lemmy, PieFed, Mobilizon, and any other AP
//! software — into `appview_posts` / `calendar_events`, which the feed and the
//! events surface read.
//!
//! Three REST/GraphQL dialects are normalized to the AP shapes the ingest
//! dispatches on:
//!
//! * **Mastodon / Pixelfed / generic** — `GET /api/v1/timelines/public` (the
//!   *federated* timeline: posts from every instance the host knows about, so
//!   coverage grows across the fediverse). Boosts are un-wrapped to their
//!   original status.
//! * **Lemmy** — `GET /api/v3/post/list?sort=New`, mapped from Lemmy's
//!   `post`/`creator`/`community` shape to `Create { object: Note }`.
//! * **PieFed** — `GET /api/alpha/post/list?sort=New`; PieFed speaks the same
//!   `post_view` shape as Lemmy (title lives in `post.title`), so both share
//!   [`post_view_to_activity`]. PieFed has no `/api/v3` route — polling it as
//!   Lemmy 404s on every host.
//! * **Mobilizon** — no public timeline at all: `POST /api` GraphQL
//!   `searchEvents` lists public events, and each event's ActivityPub `Event`
//!   object is then fetched and ingested into `calendar_events`.
//!
//! Every ingested author reveals a live fediverse host, which is upserted into
//! the source registry ([`crate::ap::sources::upsert_discovered`]) so the set of
//! polled instances expands on its own. Fetching is SSRF-safe via
//! [`crate::ap::remote::{guard_url, guarded_client}`].

use serde_json::Value;

/// Hosts with content history polled per pass (the main lane).
///
/// Cycle math (audit 2026-11: 1,662 healthy sources, ~312 s per pass including
/// [`PACE_SECS`] pacing and [`PASS_INTERVAL_SECS`]): `ceil(1662 / 24) * 312 s
/// ≈ 6.0 h` per full sweep. At the previous value of 6 the same set took
/// ~24 h, and the pre-hygiene registry (3,169 rows, untried hosts competing
/// for the same slots) took ~46 h.
pub const SOURCES_PER_PASS: i64 = 24;
/// Never-successful hosts probed per pass (the probe lane). ~1,500 untried
/// hosts ÷ 4 ≈ 32 h to sweep once; a probe that yields content promotes the
/// host into the main lane, and a probe that keeps failing cools down into the
/// `reprobe` cycle instead of consuming these slots forever.
pub const PROBE_PER_PASS: i64 = 4;
/// Items requested from one source per pass.
pub const PER_SOURCE_LIMIT: usize = 40;
/// Politeness pause between host fetches.
const PACE_SECS: u64 = 2;
/// Gap between full passes over the source set (~5 min).
const PASS_INTERVAL_SECS: u64 = 300;
/// Re-probe cooled-down hosts every N passes (≈ hourly at the default cadence).
const REPROBE_EVERY: u32 = 12;

const USER_AGENT: &str = "Bytescape/1.0 (+https://bytescape.social)";

/// Fetch and ingest one source's public timeline. Returns rows ingested.
/// `Err(String)` on transport/shape failure (the caller records it as a miss).
pub async fn fetch_and_ingest_source(
    pool: &sqlx::PgPool,
    host: &str,
    kind: &str,
    limit: usize,
) -> Result<usize, String> {
    if limit == 0 {
        return Ok(0);
    }
    match kind {
        "lemmy" => fetch_lemmy(pool, host, limit).await,
        "piefed" => fetch_piefed(pool, host, limit).await,
        "mobilizon" => fetch_mobilizon(pool, host, limit).await,
        _ => fetch_mastodon_like(pool, host, limit).await,
    }
}

/// Mastodon/Pixelfed federated public timeline → activities → ingest.
async fn fetch_mastodon_like(
    pool: &sqlx::PgPool,
    host: &str,
    limit: usize,
) -> Result<usize, String> {
    let url = format!("https://{host}/api/v1/timelines/public?limit={limit}");
    let v = get_json(&url).await?;
    let statuses = v
        .as_array()
        .ok_or_else(|| format!("GET {url}: expected a status list"))?;
    let mut total = 0usize;
    for status in statuses.iter().take(limit) {
        // A boost carries the original post in `reblog`; index that instead of
        // the (usually empty) boost wrapper.
        let status = status
            .get("reblog")
            .filter(|r| r.is_object())
            .unwrap_or(status);
        let Some(activity) = super::follow_feed::create_activity_from_rest_status(status) else {
            continue;
        };
        if let Some(actor) = status.pointer("/account/url").and_then(Value::as_str) {
            let _ = super::sources::upsert_discovered(pool, actor).await;
        }
        if let Err(e) = super::ingest::ingest_activity(pool, &activity, None).await {
            tracing::debug!("[ap-pull] mastodon entry skipped: {e}");
        } else {
            total += 1;
        }
    }
    Ok(total)
}

/// Lemmy `/api/v3/post/list` → activities → ingest.
async fn fetch_lemmy(pool: &sqlx::PgPool, host: &str, limit: usize) -> Result<usize, String> {
    let url = format!("https://{host}/api/v3/post/list?limit={limit}&sort=New");
    let v = get_json(&url).await?;
    let posts = v
        .get("posts")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("GET {url}: expected a posts list"))?;
    ingest_post_views(pool, &url, posts, limit, "lemmy").await
}

/// PieFed `/api/alpha/post/list` → activities → ingest. PieFed has no
/// `/api/v3` route (verified: 404 on every host), but its `alpha` post list
/// speaks the same `post_view` shape as Lemmy — only the title field name
/// differs (`post.title`, handled by [`post_view_to_activity`]).
async fn fetch_piefed(pool: &sqlx::PgPool, host: &str, limit: usize) -> Result<usize, String> {
    let url = format!("https://{host}/api/alpha/post/list?limit={limit}&sort=New");
    let v = get_json(&url).await?;
    let posts = v
        .get("posts")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("GET {url}: expected a posts list"))?;
    ingest_post_views(pool, &url, posts, limit, "piefed").await
}

/// Shared Lemmy/PieFed ingest loop: bridge each `post_view` to an AP
/// `Create{Note}` (discovering the author's host on the way) and hand it to
/// the single ingest path. `kind` (`"lemmy"`/`"piefed"`) is stamped on each
/// bridged post_view so the group registry records which software produced
/// the community.
async fn ingest_post_views(
    pool: &sqlx::PgPool,
    url: &str,
    posts: &[Value],
    limit: usize,
    kind: &str,
) -> Result<usize, String> {
    let mut total = 0usize;
    for post_view in posts.iter().take(limit) {
        // Stamp the fetcher's software kind before bridging — the bridge
        // copies it into `_bytescape_group.kind` for the group registry.
        let mut post_view = post_view.clone();
        if post_view.is_object() {
            post_view["_bytescape_kind"] = Value::String(kind.to_string());
        }
        let Some(activity) = post_view_to_activity(&post_view) else {
            continue;
        };
        if let Some(actor) = activity
            .pointer("/object/attributedTo")
            .and_then(Value::as_str)
        {
            let _ = super::sources::upsert_discovered(pool, actor).await;
        }
        match super::ingest::ingest_activity(pool, &activity, None).await {
            Ok(n) => total += n,
            Err(e) => tracing::debug!("[ap-pull] {url}: post entry skipped: {e}"),
        }
    }
    Ok(total)
}

/// Bridge one Lemmy/PieFed `post_view` into the AP `Create { object: Note }`
/// shape the ingest dispatches on.
///
/// Title + body become the note content (the feed renders the first line as
/// the title); the thumbnail / external URL becomes an attachment; the
/// community plus any explicit tags become `Hashtag` entries (which the ingest
/// writes into the tag index, so subscribed interests match communities); the
/// software's `nsfw` flag becomes `sensitive` (which the ingest maps to the
/// NSFW label).
///
/// Lemmy spells the title `post.name`, PieFed spells it `post.title`; every
/// other field used here is identical across the two.
fn post_view_to_activity(post_view: &Value) -> Option<Value> {
    let post = post_view.get("post")?;
    let creator = post_view.get("creator");
    let ap_id = post.get("ap_id").and_then(Value::as_str)?;
    let actor = creator
        .and_then(|c| c.get("actor_id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if actor.is_empty() {
        return None;
    }

    let title = post
        .get("name")
        .or_else(|| post.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let body = post.get("body").and_then(Value::as_str).unwrap_or("");
    let content = match (title.is_empty(), body.is_empty()) {
        (false, false) => format!("{title}\n\n{body}"),
        (false, true) => title.to_string(),
        (true, false) => body.to_string(),
        (true, true) => String::new(),
    };

    // Media: thumbnail or the external link target. A link-only post keeps its
    // URL as an attachment so it is not dropped as empty — the ingest records
    // it as a `link` embed (never an image), so the feed renders a link card.
    let mut attachment = Vec::new();
    if let Some(thumb) = post.get("thumbnail_url").and_then(Value::as_str) {
        if !thumb.is_empty() {
            attachment.push(serde_json::json!({ "type": "Image", "url": thumb }));
        }
    }
    if attachment.is_empty() {
        if let Some(url) = post.get("url").and_then(Value::as_str) {
            if !url.is_empty() {
                attachment.push(serde_json::json!({ "type": "Link", "url": url }));
            }
        }
    }

    // Tags: the community (rendered as a #community tag) plus any explicit
    // tags. PieFed carries `post.tags` as a comma-separated string; tolerate
    // an array too.
    let mut tags = Vec::new();
    if let Some(comm) = post_view
        .pointer("/community/name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        tags.push(serde_json::json!({ "type": "Hashtag", "name": format!("#{comm}") }));
    }
    let explicit: Vec<String> = match post.get("tags") {
        Some(Value::String(s)) => s
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect(),
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|t| t.as_str())
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect(),
        _ => Vec::new(),
    };
    for tag in explicit {
        let name = format!("#{}", tag.trim_start_matches('#'));
        if !tags.iter().any(|t| t.get("name").and_then(Value::as_str) == Some(name.as_str())) {
            tags.push(serde_json::json!({ "type": "Hashtag", "name": name }));
        }
    }

    let published = post.get("published").and_then(Value::as_str).unwrap_or("");
    if published.is_empty() {
        return None;
    }

    // Community/group + fetcher vote counts, carried as private in-process
    // keys (`_bytescape_group`, `_bytescape_counts`) that `parse_note` reads
    // back and `ingest_note` persists. Never serialized to the wire: this
    // Value goes straight from the fetcher bridge to the ingest parser in the
    // same process.
    let community = post_view.get("community");
    let group = community
        .and_then(|c| c.get("actor_id").and_then(Value::as_str))
        .filter(|s| !s.is_empty())
        .zip(
            community
                .and_then(|c| c.get("name").and_then(Value::as_str))
                .filter(|s| !s.is_empty()),
        )
        .map(|(actor_id, name)| {
            // Instance is the host of the community actor id.
            let instance = actor_id
                .split("://")
                .nth(1)
                .and_then(|rest| rest.split('/').next())
                .unwrap_or("")
                .to_string();
            serde_json::json!({
                "actor_id": actor_id,
                "name": name,
                "title": community.and_then(|c| c.get("title").and_then(Value::as_str)).unwrap_or(""),
                "instance": instance,
                // PieFed is detected by the fetcher URL shape; the generic
                // Lemmy/PieFed path here cannot tell them apart, so the
                // caller passes the kind through post_view when it knows it
                // (`_bytescape_kind` set by the fetcher) and falls back to
                // "lemmy" (both are Lemmy-compatible forum software).
                "kind": post_view.get("_bytescape_kind").and_then(Value::as_str).unwrap_or("lemmy"),
                "nsfw": community.and_then(|c| c.get("nsfw").and_then(Value::as_bool)).unwrap_or(false),
            })
        });
    let counts = post_view.get("counts").map(|c| {
        serde_json::json!({
            "upvotes": c.get("upvotes").and_then(Value::as_i64).unwrap_or(0),
            "downvotes": c.get("downvotes").and_then(Value::as_i64).unwrap_or(0),
            "comments": c.get("comments").and_then(Value::as_i64).unwrap_or(0),
        })
    });

    let mut object = serde_json::json!({
        "type": "Note",
        "id": ap_id,
        "url": ap_id,
        "attributedTo": actor,
        "content": content,
        "published": published,
        "sensitive": post.get("nsfw").and_then(Value::as_bool).unwrap_or(false),
        "attachment": attachment,
        "tag": tags,
    });
    if let Some(g) = group {
        object["_bytescape_group"] = g;
    }
    if let Some(c) = counts {
        object["_bytescape_counts"] = c;
    }

    Some(serde_json::json!({
        "type": "Create",
        "actor": actor,
        "object": object,
    }))
}

/// Fetch a Lemmy/PieFed post's comment thread and ingest each comment with
/// `reply_parent = post_uri` so it attaches to the post's thread. Called
/// lazily by the comments endpoint when a group post has no indexed comments
/// yet (design D5). Idempotent (`ingest_activity` upserts); bounded to
/// [`COMMENTS_FETCH_LIMIT`] comments per request.
///
/// The post URL (`https://host/post/{id}` or `https://host/post/{id}/slug`) is
/// resolved to `(host, post_id)`; the comment list is fetched from the
/// software's REST API (Lemmy `/api/v3/comment/list`, PieFed
/// `/api/alpha/comment/list`), and each `comment_view.comment` is bridged to
/// a `Create { object: Note }` whose `inReplyTo` is the post URL (top-level
/// comments) or the parent comment URL (nested replies — `comment.path`
/// carries the thread path).
pub async fn fetch_and_ingest_comments(
    pool: &sqlx::PgPool,
    post_url: &str,
) -> Result<usize, String> {
    let Some((host, post_id)) = post_url_to_host_and_id(post_url) else {
        return Err(format!("not a fetchable post URL: {post_url}"));
    };
    let mut total = 0usize;
    // Fetch ALL comment pages (bounded) — a single page misses deep threads.
    for page in 1..=COMMENTS_MAX_PAGES {
        // Lemmy serves /api/v3, PieFed serves /api/alpha. Try Lemmy first.
        let v = match get_json(&format!(
            "https://{host}/api/v3/comment/list?post_id={post_id}&type_=All&sort=New&limit={COMMENTS_FETCH_LIMIT}&page={page}"
        ))
        .await
        {
            Ok(v) => Some(v),
            Err(_) => get_json(&format!(
                "https://{host}/api/alpha/comment/list?post_id={post_id}&type_=All&sort=New&limit={COMMENTS_FETCH_LIMIT}&page={page}"
            ))
            .await
            .ok(),
        };
        let Some(v) = v else { break };
        let Some(comments) = v.get("comments").and_then(Value::as_array) else {
            break;
        };
        if comments.is_empty() {
            break;
        }
        for cv in comments.iter().take(COMMENTS_FETCH_LIMIT) {
            let Some(comment) = cv.get("comment") else { continue };
            let Some(activity) = comment_view_to_activity(comment, cv, post_url) else {
                continue;
            };
            match super::ingest::ingest_activity(pool, &activity, None).await {
                Ok(n) => total += n,
                Err(e) => tracing::debug!("[ap-pull] {host} comment skipped: {e}"),
            }
        }
        if comments.len() < COMMENTS_FETCH_LIMIT {
            break; // last page
        }
    }
    Ok(total)
}

/// Comments fetched per lazy thread pull (politeness + latency bound).
const COMMENTS_FETCH_LIMIT: usize = 50;
/// Max comment pages followed per thread pull (bounded).
const COMMENTS_MAX_PAGES: usize = 5;

/// `(host, numeric_or_slug post id)` from a Lemmy/PieFed post URL. Handles
/// `https://host/post/{id}` and `https://host/post/{id}/slug`.
fn post_url_to_host_and_id(post_url: &str) -> Option<(String, String)> {
    let rest = post_url.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/')?;
    let host = host.to_string();
    // /post/{id}[/slug] or /post/{id}/{slug} — take the segment after /post/.
    let mut segs = path.split('/').skip_while(|s| *s != "post").skip(1);
    let id = segs.next()?.to_string();
    if id.is_empty() {
        return None;
    }
    Some((host, id))
}

/// Bridge one Lemmy/PieFed `comment_view.comment` (+ its `creator`) into a
/// `Create { object: Note }` whose `inReplyTo` is the post URL (or the parent
/// comment for nested replies). `path` is Lemmy's thread path
/// (`0.{post_id}.{comment_id}` / `0.{post_id}.{parent_id}.{comment_id}`) —
/// the second-to-last segment (after the post id) is the parent comment.
fn comment_view_to_activity(comment: &Value, cv: &Value, post_url: &str) -> Option<Value> {
    let ap_id = comment.get("ap_id").and_then(Value::as_str)?;
    let actor = cv
        .pointer("/creator/actor_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    if actor.is_empty() {
        return None;
    }
    let content = comment.get("content").and_then(Value::as_str).unwrap_or("");
    let published = comment.get("published").and_then(Value::as_str).unwrap_or("");
    if published.is_empty() {
        return None;
    }
    // Parent: nested replies have a `path` of `0.<post>.<parent…>.<self>`;
    // top-level comments have `0.<post>.<self>`. Use the parent comment URL
    // when nested, else the post URL.
    let in_reply_to = comment
        .get("path")
        .and_then(Value::as_str)
        .and_then(|path| {
            let segs: Vec<&str> = path.split('.').collect();
            // path = [0, post_id, ...parents..., self]; the parent is the
            // segment before the last when there are >3 segments.
            if segs.len() > 3 {
                let parent_id = segs[segs.len() - 2];
                // The parent is a comment (same /comment/ collection as self),
                // so derive the base from the comment's own ap_id — NOT the
                // post URL (which would build /post/<comment_id>).
                let base = ap_id.rsplit_once('/').map(|(b, _)| b).unwrap_or(post_url);
                Some(format!("{base}/{parent_id}"))
            } else {
                None
            }
        })
        .unwrap_or_else(|| post_url.to_string());
    Some(serde_json::json!({
        "type": "Create",
        "actor": actor,
        "object": {
            "type": "Note",
            "id": ap_id,
            "url": ap_id,
            "attributedTo": actor,
            "content": content,
            "published": published,
            "inReplyTo": in_reply_to,
        }
    }))
}

/// Events requested from one Mobilizon instance per pass. Each event costs one
/// AP fetch, so this is the politeness bound that matters (not the timeline
/// `limit`, which is sized for one batched request).
const MOBILIZON_EVENTS_PER_PASS: usize = 12;

/// Mobilizon `/api` GraphQL `searchEvents` → each event's AP object → ingest.
///
/// Mobilizon has **no public timeline**: `/api/v1/timelines/public` answers
/// `400 No query document supplied` (it is a GraphQL server), so the
/// Mastodon-shaped fetcher can never work for it. Events are listed over
/// GraphQL (upcoming only — `beginsOn` is an inclusive lower bound), then each
/// event's canonical URL is fetched as ActivityPub and ingested, where the
/// `Event` arm of [`crate::ap::ingest::ingest_activity`] writes
/// `calendar_events` with `source='activitypub'`.
async fn fetch_mobilizon(pool: &sqlx::PgPool, host: &str, limit: usize) -> Result<usize, String> {
    let url = format!("https://{host}/api");
    let count = limit.min(MOBILIZON_EVENTS_PER_PASS);
    let begins_on = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    // `count` and `begins_on` are server-controlled (a usize and an RFC3339
    // timestamp) — no caller input reaches the query string.
    let query = format!(
        "query {{ searchEvents(term: \"\", beginsOn: \"{begins_on}\", page: 1, limit: {count}) \
         {{ elements {{ title url }} }} }}"
    );
    let v = post_json(&url, &serde_json::json!({ "query": query })).await?;
    if let Some(errors) = v.get("errors") {
        return Err(format!("POST {url}: graphql errors: {errors}"));
    }
    let elements = v
        .pointer("/data/searchEvents/elements")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("POST {url}: expected searchEvents elements"))?;

    let mut total = 0usize;
    for element in elements.iter().take(count) {
        let Some(event_url) = element
            .get("url")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let object = match get_json_as(event_url, "application/activity+json").await {
            Ok(o) => o,
            Err(e) => {
                tracing::debug!("[ap-pull] mobilizon event fetch skipped: {e}");
                continue;
            }
        };
        if let Some(actor) = object.get("attributedTo").and_then(|a| {
            a.as_str()
                .or_else(|| a.get("id").and_then(Value::as_str))
        }) {
            let _ = super::sources::upsert_discovered(pool, actor).await;
        }
        match super::ingest::ingest_activity(pool, &object, None).await {
            Ok(n) => total += n,
            Err(e) => tracing::debug!("[ap-pull] mobilizon event skipped: {e}"),
        }
    }
    Ok(total)
}

/// SSRF-safe JSON GET (HTTPS-only, private ranges rejected, redirects
/// re-guarded, bounded timeout).
async fn get_json(url: &str) -> Result<Value, String> {
    get_json_as(url, "application/json").await
}

/// SSRF-safe JSON GET with an explicit `Accept` header — AP object endpoints
/// need `application/activity+json` to answer with the JSON-LD document
/// instead of the HTML page.
async fn get_json_as(url: &str, accept: &str) -> Result<Value, String> {
    super::remote::guard_url(url)?;
    let client = super::remote::guarded_client()?;
    let resp = client
        .get(url)
        .header(reqwest::header::ACCEPT, accept)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .map_err(|e| format!("GET {url}: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("GET {url}: HTTP {status}"));
    }
    resp.json().await.map_err(|e| format!("GET {url}: {e}"))
}

/// SSRF-safe JSON POST (same guardrails as [`get_json_as`]).
async fn post_json(url: &str, body: &Value) -> Result<Value, String> {
    super::remote::guard_url(url)?;
    let client = super::remote::guarded_client()?;
    let resp = client
        .post(url)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("POST {url}: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("POST {url}: HTTP {status}"));
    }
    resp.json().await.map_err(|e| format!("POST {url}: {e}"))
}

/// The standing worker: seed the source registry, then loop — pull the due
/// sources (main lane + probe lane, see [`crate::ap::sources::next_due`]),
/// record health, periodically re-probe cooled-down hosts, and wait out the
/// pass interval. Spawned once from `main`.
pub fn spawn_ap_public_feed_worker(db: sqlx::PgPool, jwt_secret: String) {
    tokio::spawn(async move {
        match super::sources::seed_defaults(&db).await {
            Ok(n) if n > 0 => tracing::info!("[ap-pull] seeded {n} fediverse sources"),
            Ok(_) => {}
            Err(e) => tracing::warn!("[ap-pull] source seed failed: {e}"),
        }
        let _ = jwt_secret; // reserved for signed fetches when actors are issued
        let mut since_reprobe: u32 = 0;
        loop {
            match super::sources::next_due(&db, SOURCES_PER_PASS, PROBE_PER_PASS).await {
                Ok(sources) => {
                    for s in sources {
                        match fetch_and_ingest_source(&db, &s.host, &s.kind, PER_SOURCE_LIMIT).await
                        {
                            Ok(n) => {
                                let _ = super::sources::record_ok(&db, &s.host, n as i32).await;
                                if n > 0 {
                                    tracing::info!("[ap-pull] {}/{} → {} rows", s.kind, s.host, n);
                                }
                            }
                            Err(e) => {
                                let _ = super::sources::record_fail(&db, &s.host).await;
                                tracing::debug!("[ap-pull] {} failed: {e}", s.host);
                            }
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(PACE_SECS)).await;
                    }
                }
                Err(e) => tracing::warn!("[ap-pull] source query failed: {e}"),
            }
            since_reprobe += 1;
            if since_reprobe >= REPROBE_EVERY {
                since_reprobe = 0;
                match super::sources::reprobe(&db).await {
                    Ok(n) if n > 0 => tracing::info!("[ap-pull] re-probed {n} cooled sources"),
                    _ => {}
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(PASS_INTERVAL_SECS)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Lemmy `/api/v3/post/list` entry (title lives in `post.name`).
    fn lemmy_post_view() -> Value {
        serde_json::json!({
            "post": {
                "ap_id": "https://lemmy.world/post/55353427",
                "name": "Down With Data Centers",
                "body": "Global weekend of action.",
                "url": "https://example.com/article",
                "thumbnail_url": "",
                "published": "2026-11-15T21:15:41.081Z",
                "nsfw": false
            },
            "creator": {"actor_id": "https://lemmy.world/u/alice"},
            "community": {"name": "technology"}
        })
    }

    /// A PieFed `/api/alpha/post/list` entry — same shape, but the title is
    /// `post.title`, tags are a comma string, and `nsfw` is a real flag.
    fn piefed_post_view() -> Value {
        serde_json::json!({
            "post": {
                "ap_id": "https://piefed.social/c/piefed_meta@piefed.social/p/2603915/piefed-v1-9-is-released",
                "title": "PieFed v1.9 is released",
                "body": "## New features",
                "url": "https://codeberg.org/rimu/pyfedi/releases/tag/v1.9.0",
                "thumbnail_url": "https://media.piefed.social/posts/T1/yX/T1yX6Wq9GQZK8at.webp",
                "published": "2026-11-15T00:10:49.199Z",
                "tags": "PieFed, Release",
                "nsfw": true
            },
            "creator": {"actor_id": "https://piefed.social/u/rimu"},
            "community": {"name": "piefed_meta"}
        })
    }

    fn tag_names(activity: &Value) -> Vec<String> {
        activity["object"]["tag"]
            .as_array()
            .expect("tag array")
            .iter()
            .filter_map(|t| t.get("name").and_then(Value::as_str))
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn post_view_maps_lemmy_title_and_community_tag() {
        let a = post_view_to_activity(&lemmy_post_view()).expect("lemmy post maps");
        assert_eq!(a["object"]["type"], "Note");
        assert_eq!(a["object"]["id"], "https://lemmy.world/post/55353427");
        assert_eq!(
            a["object"]["attributedTo"], "https://lemmy.world/u/alice",
            "actor comes from creator.actor_id"
        );
        assert_eq!(
            a["object"]["content"], "Down With Data Centers\n\nGlobal weekend of action.",
            "title + body become the note content"
        );
        // No thumbnail → the external URL is a Link attachment (never an image).
        assert_eq!(a["object"]["attachment"][0]["type"], "Link");
        assert_eq!(a["object"]["attachment"][0]["url"], "https://example.com/article");
        assert_eq!(a["object"]["sensitive"], false);
        assert_eq!(tag_names(&a), vec!["#technology"]);
    }

    #[test]
    fn post_view_maps_piefed_title_tags_and_nsfw() {
        let a = post_view_to_activity(&piefed_post_view()).expect("piefed post maps");
        assert_eq!(
            a["object"]["content"], "PieFed v1.9 is released\n\n## New features",
            "PieFed spells the title `post.title`"
        );
        assert_eq!(
            a["object"]["sensitive"], true,
            "the software's nsfw flag becomes sensitive (→ labels at ingest)"
        );
        assert_eq!(
            tag_names(&a),
            vec!["#piefed_meta", "#PieFed", "#Release"],
            "community + the comma-separated `post.tags`"
        );
        // Thumbnail wins as media; the external URL is not recorded twice.
        assert_eq!(a["object"]["attachment"][0]["type"], "Image");
        assert_eq!(
            a["object"]["published"], "2026-11-15T00:10:49.199Z"
        );
    }

    #[test]
    fn post_view_survives_a_title_only_link_post() {
        let mut pv = lemmy_post_view();
        pv["post"]["body"] = serde_json::json!("");
        pv["post"]["url"] = serde_json::json!("https://example.com/only-link");
        let a = post_view_to_activity(&pv).expect("link post maps");
        assert_eq!(a["object"]["content"], "Down With Data Centers");
        assert_eq!(
            a["object"]["attachment"][0]["url"], "https://example.com/only-link"
        );
    }

    /// Every kind in the registry must have a fetcher; `infer_kind` is the only
    /// producer of kinds, so an unmapped value means permanently-dead hosts.
    #[test]
    fn every_inferred_kind_has_a_fetcher() {
        for (sample, expected) in [
            ("https://lemmy.world/post/1", "lemmy"),
            ("https://piefed.social/u/rimu", "piefed"),
            ("https://mobilizon.fr/events/abc", "mobilizon"),
            ("https://pixelfed.social/p/alice/1", "pixelfed"),
            ("https://mastodon.social/users/alice/statuses/1", "mastodon"),
            ("https://sh.itjust.works/post/1", "lemmy"),
            ("https://example.test/something", "generic"),
        ] {
            assert_eq!(
                super::super::sources::infer_kind(sample),
                expected,
                "kind inference for {sample}"
            );
        }
    }
}

/// Refresh like/vote/comment counts for a batch of post URIs from their home
/// instances (owner bug: Mastodon likes showed 0 while the source had N).
/// Bounded to 25 per call. Handles BOTH shapes:
/// * Lemmy/PieFed `/post/{id}` → REST `post_view/counts` (upvotes/downvotes/
///   comments); `like_count` (the ❤️) tracks the upvotes.
/// * Mastodon/AP `/statuses/…` (and any `https://` object) → the AP object's
///   `likes.totalItems` (❤️) / `replies.totalItems`.
/// Updates `like_count` AND `upvote_count`/`downvote_count`/`comment_count`, so
/// the heart + votes never render a stale 0. Called fire-and-forget (async
/// spawn) so the first render is never blocked (refresh-on-view).
pub async fn refresh_counts_for_posts(pool: &sqlx::PgPool, uris: &[String]) -> usize {
    let mut updated = 0usize;
    for uri in uris.iter().take(25) {
        if refresh_one_counts(pool, uri).await {
            updated += 1;
        }
    }
    updated
}

/// Refresh one post's counts. `like_count` = the ❤️ count; the 👍/👎 vote
/// columns carry upvotes/downvotes. Returns whether a row was updated.
async fn refresh_one_counts(pool: &sqlx::PgPool, uri: &str) -> bool {
    // Lemmy / PieFed: REST post counts. BOTH take `id=` on the single-post
    // endpoint — `post_id=` is silently ignored (`couldnt_find_post` from
    // Lemmy, a validation error from PieFed), which made every group count
    // refresh a no-op (owner: "vote counts and comment threads are not
    // tracked for group content"). `id` first; the legacy names stay as a
    // fallback for forks.
    if let Some((host, post_id)) = post_url_to_host_and_id(uri) {
        let v = match get_json(&format!("https://{host}/api/v3/post?id={post_id}")).await {
            Ok(v) => Some(v),
            Err(_) => match get_json(&format!("https://{host}/api/alpha/post?id={post_id}")).await {
                Ok(v) => Some(v),
                Err(_) => {
                    get_json(&format!("https://{host}/api/v3/post?post_id={post_id}"))
                        .await
                        .ok()
                }
            },
        };
        if let Some(c) = v
            .as_ref()
            .and_then(|v| v.pointer("/post_view/counts").or_else(|| v.get("counts")))
        {
            let up = c.get("upvotes").and_then(Value::as_i64).unwrap_or(0) as i32;
            let down = c.get("downvotes").and_then(Value::as_i64).unwrap_or(0) as i32;
            let com = c
                .get("comments")
                .or_else(|| c.get("comment_count"))
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32;
            // ❤️ (like) tracks the upvote total on forum posts.
            return update_counts(pool, uri, up, up, down, com).await;
        }
    }
    // Mastodon / AP object: `likes.totalItems` is the ❤️ count.
    if uri.starts_with("https://") {
        if let Ok(obj) = get_json_as(uri, "application/activity+json").await {
            let likes = obj
                .pointer("/likes/totalItems")
                .or_else(|| obj.get("likeCount"))
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32;
            let replies = obj
                .pointer("/replies/totalItems")
                .or_else(|| obj.get("replyCount"))
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32;
            return update_counts(pool, uri, likes, likes, 0, replies).await;
        }
    }
    false
}

/// Persist refreshed counts (`like_count`, `upvote_count`, `downvote_count`,
/// `comment_count`) for `uri`.
async fn update_counts(
    pool: &sqlx::PgPool,
    uri: &str,
    like: i32,
    up: i32,
    down: i32,
    comment: i32,
) -> bool {
    sqlx::query(
        "UPDATE appview_posts SET like_count = $1, upvote_count = $2, downvote_count = $3, comment_count = $4 WHERE uri = $5",
    )
    .bind(like)
    .bind(up)
    .bind(down)
    .bind(comment)
    .bind(uri)
    .execute(pool)
    .await
    .is_ok()
}

/// Map one Lemmy/PieFed `comment_view` to a renderable comment item (no DB
/// work) — used by the render-before-ingest fast path.
fn comment_view_to_item(cv: &Value) -> Option<crate::api::feed::CommentItem> {
    let comment = cv.get("comment")?;
    let creator = cv.get("creator");
    let id = comment.get("ap_id").and_then(Value::as_str)?.to_string();
    let actor_id = creator
        .and_then(|c| c.get("actor_id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let avatar = creator.and_then(|c| c.get("avatar")).and_then(Value::as_str);
    // Identical handle shape to the indexed-rows path (`@user@host` from the
    // actor URL) — a thread must not change author labels between the
    // render-before-ingest fetch and the durable-ingest re-read (owner:
    // "comments load and thread identically").
    let handle = ap_handle_local(actor_id);
    Some(crate::api::feed::CommentItem {
        id,
        author_handle: handle,
        author_avatar: avatar.map(str::to_string),
        content: comment
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        created_at: comment
            .get("published")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        parent_id: None,
        likes_count: comment
            .get("likes")
            .or_else(|| comment.get("score"))
            .and_then(Value::as_i64)
            .unwrap_or(0) as u32,
        media: vec![],
        embeds: vec![],
        author_url: (!actor_id.is_empty()).then(|| actor_id.to_string()),
        author_following: false,
        author_resolved: true,
    })
}

/// `@user@host` from an AP actor URL (local mirror of the feed module's
/// helper, kept here so the fetch path need not reach across modules).
fn ap_handle_local(actor_id: &str) -> String {
    let host = actor_id
        .split("://")
        .nth(1)
        .and_then(|r| r.split('/').next())
        .unwrap_or("");
    let name = actor_id.rsplit('/').next().unwrap_or("");
    if host.is_empty() || name.is_empty() {
        return actor_id.to_string();
    }
    format!("@{}@{}", name, host)
}

/// Fetch a post's comment thread and return renderable items IMMEDIATELY
/// (owner item 2: "render-before-ingest where safe… first screen fast, no long
/// spinner"). The durable DB ingest runs in the background from the same fetch.
pub async fn fetch_comment_items(
    pool: &sqlx::PgPool,
    post_url: &str,
) -> Result<Vec<crate::api::feed::CommentItem>, String> {
    let Some((host, post_id)) = post_url_to_host_and_id(post_url) else {
        return Err(format!("not a fetchable post URL: {post_url}"));
    };
    let mut items = Vec::new();
    let mut activities: Vec<Value> = Vec::new();
    for page in 1..=COMMENTS_MAX_PAGES {
        let v = match get_json(&format!(
            "https://{host}/api/v3/comment/list?post_id={post_id}&type_=All&sort=New&limit={COMMENTS_FETCH_LIMIT}&page={page}"
        ))
        .await
        {
            Ok(v) => Some(v),
            Err(_) => {
                get_json(&format!(
                    "https://{host}/api/alpha/comment/list?post_id={post_id}&type_=All&sort=New&limit={COMMENTS_FETCH_LIMIT}&page={page}"
                ))
                .await
                .ok()
            }
        };
        let Some(v) = v else { break };
        let Some(comments) = v.get("comments").and_then(Value::as_array) else {
            break;
        };
        if comments.is_empty() {
            break;
        }
        for cv in comments.iter().take(COMMENTS_FETCH_LIMIT) {
            if let Some(item) = comment_view_to_item(cv) {
                items.push(item);
            }
            if let Some(comment) = cv.get("comment") {
                if let Some(activity) = comment_view_to_activity(comment, cv, post_url) {
                    activities.push(activity);
                }
            }
        }
        if comments.len() < COMMENTS_FETCH_LIMIT {
            break;
        }
    }
    // Background the durable ingest (writes appview_posts + tags) from the same
    // fetch — the first screen renders without waiting on DB writes.
    let db = pool.clone();
    tokio::spawn(async move {
        for activity in activities {
            let _ = super::ingest::ingest_activity(&db, &activity, None).await;
        }
    });
    Ok(items)
}
