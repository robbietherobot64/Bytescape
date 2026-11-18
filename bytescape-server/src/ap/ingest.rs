//! ActivityPub feed-ingestion mappers (Phase B).
//!
//! Maps remote AP objects into the derived indexes the feed and events surfaces
//! already read:
//! * `Note` (and the other text-bearing types in [`NOTE_TYPES`]) →
//!   `appview_posts` — uri = the AP object URL, `author_did` = the AP actor
//!   URL, `author_handle` derived from the actor URL's host+path (the feed
//!   handle-resolver requires a non-NULL handle). `inReplyTo` becomes
//!   `reply_parent`/`reply_root` so comments stay out of the top-level feed;
//!   `sensitive`/`nsfw`/content-warning `summary` become `labels`;
//!   `tag` hashtags are written to `appview_post_tags` (the same rows the
//!   064/068 text trigger writes) so subscribed interests match objects whose
//!   tags never appear inline (Lemmy communities, Mastodon `tags[]`).
//! * `Event` (Mobilizon) → `calendar_events` via
//!   [`crate::calendar::insert_event`] with `source = 'activitypub'`.
//! * Pixelfed `Story`/`Photo` → `appview_stories` — `media` = JSON array of
//!   media URLs, 24h lifetime from `published`.
//!
//! Every entry point is a no-op unless `AP_INGEST_ENABLED=true` (see
//! [`crate::ap::ap_ingest_enabled`]). Writes are additive and idempotent
//! (`ON CONFLICT (uri) DO UPDATE`): only `https://` AP rows are written; the
//! AT (Bluesky) rows and the AT read path are never touched.
//!
//! HTML stripping, timestamp parsing, attachment extraction, and Note → post
//! parsing are reused from [`crate::api::import_mastodon`] — not duplicated.

use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};

use crate::api::import_mastodon::{
    attachment_urls, handle_from_url, html_to_text, post_from_note, ts_of,
};

// ─── Parsed objects ────────────────────────────────────────────────────────

/// A parsed ActivityPub `Note`, ready to be written to `appview_posts`.
#[derive(Debug, Clone)]
pub struct ApNote {
    /// Canonical object URL — becomes `appview_posts.uri`.
    pub object_url: String,
    /// `attributedTo` — the remote AP actor URL; becomes `author_did`.
    pub actor_url: String,
    /// Plain text (HTML stripped).
    pub content: String,
    /// `published` timestamp — becomes `created_at`.
    pub published: DateTime<Utc>,
    /// Media URLs (images/video/audio attachments) — become `image` embeds.
    pub media_urls: Vec<String>,
    /// External URLs (attachment entries of type `Link`, or a title-only
    /// object's own `url`) — become `link` embeds, never images.
    pub link_urls: Vec<String>,
    /// `tag` entries of type `Hashtag` (leading `#` stripped, deduped).
    /// Written to `appview_post_tags` by [`ingest_note`] so subscribed
    /// interests match tags that never appear inline in the text (Lemmy
    /// communities, Mastodon `tags[]`).
    pub hashtags: Vec<String>,
    /// `inReplyTo` — the direct parent's object URL, if any.
    pub reply_parent: Option<String>,
    /// Thread root, best-effort: the object's `context` when the server sends
    /// one, else the parent. AP exposes only the direct parent, so this is a
    /// two-level approximation until a thread walk exists.
    pub reply_root: Option<String>,
    /// Moderation labels derived from `sensitive`/`nsfw`/CW `summary`.
    pub labels: Vec<String>,
    /// Community/group this note was posted to (Lemmy/PieFed `post_view`
    /// bridging — `community {name, actor_id}`). `None` for direct posts
    /// (Mastodon/Pixelfed/generic). The canonical AP actor id of the group is
    /// kept so a later AT layer can coexist (design D1).
    pub group: Option<ApGroup>,
    /// Vote/reply counts as reported at fetch time (`post_view.counts` on the
    /// Lemmy/PieFed pullers; `0`/`None` everywhere else). Stored at ingest so
    /// the group post card can show a real upvote ratio without a second
    /// fetch (design D3).
    pub upvotes: Option<i32>,
    pub downvotes: Option<i32>,
    /// Comment count from the fetcher's `counts.comments`. Distinct from
    /// `reply_count` (which the AP inbox bumps on inbound replies) — this is
    /// the remote's own count at fetch time.
    pub fetched_comments: Option<i32>,
}

/// A community/group reference captured at fetch time (Lemmy/PieFed
/// `post_view.community`), ready for `ap_groups` upsert + post linkage.
#[derive(Debug, Clone)]
pub struct ApGroup {
    /// Canonical AP actor id of the community, e.g.
    /// `https://lemmy.world/c/technology`.
    pub actor_id: String,
    /// Short name (`community.name`, e.g. `technology`).
    pub name: String,
    /// Display title when the fetcher sends one (`community.title`).
    pub title: Option<String>,
    /// Host of `actor_id`, e.g. `lemmy.world`.
    pub instance: String,
    /// Which fetcher produced it (`lemmy` / `piefed`).
    pub kind: String,
    /// Community NSFW flag when present.
    pub nsfw: bool,
}

/// A parsed ActivityPub `Event` (Mobilizon), ready for `calendar_events`.
#[derive(Debug, Clone)]
pub struct ApEvent {
    /// Canonical object URL — becomes `calendar_events.uri`.
    pub object_url: String,
    /// `attributedTo` — the event author's actor URL; becomes `author_did`.
    pub actor_url: String,
    /// The event row, already in the calendar's lexicon-neutral shape.
    pub event: crate::calendar::ParsedEvent,
}

/// A parsed Pixelfed `Story`/`Photo`, ready for `appview_stories`.
#[derive(Debug, Clone)]
pub struct ApStory {
    /// Canonical object URL — becomes `appview_stories.uri`.
    pub object_url: String,
    /// `attributedTo` — the remote AP actor URL; becomes `author_did`.
    pub actor_url: String,
    /// `published` timestamp — becomes `created_at`/`active_from`.
    pub published: DateTime<Utc>,
    /// Plain text caption, if any (HTML stripped).
    pub text: Option<String>,
    /// Media URLs — becomes the `media` JSON array.
    pub media_urls: Vec<String>,
}

// ─── Parsing ───────────────────────────────────────────────────────────────

/// Split `v` into `(object, wrapping Create activity)`. A bare object passes
/// through with `None`. Returns `None` for a `Create` whose `object` is a bare
/// reference (string URL) — resolving it would need a fetch this module never
/// does.
fn unwrap_create(v: &Value) -> Option<(&Value, Option<&Value>)> {
    if v.get("type").and_then(|t| t.as_str()) == Some("Create") {
        match v.get("object") {
            Some(obj @ Value::Object(_)) => Some((obj, Some(v))),
            _ => None,
        }
    } else {
        Some((v, None))
    }
}

/// An actor reference as a plain URL string — either a string or an embedded
/// actor object (`id`/`url`).
pub(crate) fn actor_url_str(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Object(o) => o
            .get("id")
            .or_else(|| o.get("url"))
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        _ => None,
    }
}

/// The actor URL of an object (`attributedTo`), falling back to the wrapping
/// activity's `actor`/`attributedTo`.
pub(crate) fn actor_of(obj: &Value, wrapper: Option<&Value>) -> Option<String> {
    obj.get("attributedTo")
        .and_then(actor_url_str)
        .or_else(|| {
            wrapper.and_then(|w| {
                w.get("actor")
                    .or_else(|| w.get("attributedTo"))
                    .and_then(actor_url_str)
            })
        })
}

/// Hashtag names from a Note's `tag` array (leading `#` stripped, order
/// preserved, deduped). Non-Hashtag tags (Mention, Emoji) are skipped.
fn hashtag_names(note: &Value) -> Vec<String> {
    let Some(Value::Array(tags)) = note.get("tag") else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for t in tags {
        let Some(obj) = t.as_object() else { continue };
        let is_hashtag = obj
            .get("type")
            .and_then(|x| x.as_str())
            .is_some_and(|ty| ty.eq_ignore_ascii_case("Hashtag"));
        if !is_hashtag {
            continue;
        }
        let Some(name) = obj.get("name").and_then(|x| x.as_str()) else {
            continue;
        };
        let name = name.trim().trim_start_matches('#');
        if !name.is_empty() && seen.insert(name.to_string()) {
            out.push(name.to_string());
        }
    }
    out
}

/// The text-bearing AP object types that map to a feed post. `Note` is the
/// common case (Mastodon/Pixelfed posts, Lemmy comments); `Question` (Mastodon
/// polls), `Article` (Mastodon long-form / Lemmy articles), and
/// `Page`/`Audio`/`Video` (Lemmy link/media posts) carry the same post shape.
const NOTE_TYPES: [&str; 6] = ["Note", "Question", "Article", "Page", "Audio", "Video"];

/// Split an object's `attachment`/`media_attachments` into
/// `(media, links)`: entries whose `type` is `Link` are **external URLs** (a
/// link card), everything else is media. Recording a link as an image made
/// every Lemmy link post render a broken `<img>` against an HTML page.
fn attachment_parts(obj: &Value) -> (Vec<String>, Vec<String>) {
    let mut media: Vec<String> = Vec::new();
    let mut links: Vec<String> = Vec::new();
    let push = |out: &mut Vec<String>, s: &str| {
        if !s.is_empty() && !out.iter().any(|x| x == s) {
            out.push(s.to_string());
        }
    };
    for key in ["attachment", "media_attachments"] {
        let Some(Value::Array(items)) = obj.get(key) else {
            continue;
        };
        for item in items {
            let is_link = item
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|t| t.eq_ignore_ascii_case("Link"));
            let urls = match item {
                Value::String(s) => vec![s.clone()],
                Value::Object(_) => own_media_urls(item),
                _ => Vec::new(),
            };
            for url in urls {
                push(if is_link { &mut links } else { &mut media }, &url);
            }
        }
    }
    (media, links)
}

/// Moderation labels from the object's own flags.
///
/// * `sensitive` (Mastodon/Pixelfed/Lemmy via [`super::public_feed`]) or
///   `nsfw` → `nsfw`.
/// * A `Note`'s non-empty `summary` is Mastodon's content warning. The feed's
///   policy already ranks CW alongside NSFW (`"!warn"` is in
///   `filter::NSFW_LABELS`), so it is recorded under that token rather than
///   silently dropped. Gated on `Note` so an `Article`'s summary/description
///   is never mistaken for a warning.
fn labels_of(obj: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let flag = obj
        .get("sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || obj.get("nsfw").and_then(Value::as_bool).unwrap_or(false);
    if flag {
        out.push("nsfw".to_string());
    }
    let is_note = obj
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|t| t.eq_ignore_ascii_case("Note"));
    let has_cw = obj
        .get("summary")
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|s| !s.is_empty());
    if is_note && has_cw {
        out.push("!warn".to_string());
    }
    out
}

/// `inReplyTo` as an object URL (string or embedded `{id}` reference).
fn reply_parent_of(obj: &Value) -> Option<String> {
    obj.get("inReplyTo").and_then(actor_url_str)
}

/// Parse a text-bearing AP object — bare or wrapped in a `Create` — into an
/// [`ApNote`]. Reuses `post_from_note` from the Mastodon archive importer for
/// the HTML strip, timestamp, and attachment extraction. Title/link-bearing
/// objects (Lemmy `Page`, Mastodon `Article`) are normalized so their title or
/// link becomes the note content.
///
/// `None` = not a text-bearing object, missing `id`/`attributedTo`, missing
/// timestamp, or an entry with no text and no media (the validity gate).
pub fn parse_note(v: &Value) -> Option<ApNote> {
    let (obj, wrapper) = unwrap_create(v)?;
    let ty = obj.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if !NOTE_TYPES.contains(&ty) {
        return None;
    }
    let object_url = obj
        .get("id")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    let actor_url = actor_of(obj, wrapper)?;

    // Normalize title/link-bearing objects (Lemmy Page/Audio/Video, Mastodon
    // Article) onto the shape `post_from_note` reads: `content` carries the
    // title+body so a bare link post is not dropped as empty.
    let body_html = obj
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let title = obj
        .get("name")
        .or_else(|| obj.get("summary"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let link = obj
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let combined = if !title.is_empty() && !body_html.is_empty() {
        format!("{title}\n\n{body_html}")
    } else if !body_html.is_empty() {
        body_html
    } else if !title.is_empty() {
        title
    } else {
        link
    };
    let mut norm = obj.clone();
    if !combined.trim().is_empty() {
        norm["content"] = Value::String(combined);
    }

    // The Note's own `published` first; the wrapping Create's as fallback.
    let mut ts_sources: Vec<&Value> = vec![&norm];
    if let Some(w) = wrapper {
        ts_sources.push(w);
    }
    let parsed = post_from_note(&norm, &ts_sources, false).ok()?;

    // Media vs external links (see `attachment_parts`). A title-only `Page`
    // whose body is its own URL keeps that URL as the link card.
    let (media_urls, mut link_urls) = attachment_parts(obj);
    if link_urls.is_empty() {
        if let Some(own) = obj.get("url").and_then(Value::as_str) {
            let own = own.trim();
            if !own.is_empty() && own != object_url && own.starts_with("http") {
                link_urls.push(own.to_string());
            }
        }
    }

    let reply_parent = reply_parent_of(obj);
    // AP exposes only the direct parent; `context`, when the server sends one,
    // is the thread root.
    let reply_root = obj
        .get("context")
        .and_then(actor_url_str)
        .or_else(|| reply_parent.clone());

    // Community/group + fetcher vote counts. Carried on the bridged Note as
    // the private `_bytescape_group` / `_bytescape_counts` keys by
    // `post_view_to_activity` (Lemmy/PieFed pullers) — absent on Notes that
    // came in over the wire (Mastodon et al.), where group is None and counts
    // are unknown. Never serialized back out: these keys only exist between
    // the fetcher bridge and this parser, both in-process.
    let group = obj.get("_bytescape_group").and_then(parse_group_ref);
    let (upvotes, downvotes, fetched_comments) = obj
        .get("_bytescape_counts")
        .map(parse_counts)
        .unwrap_or((None, None, None));

    Some(ApNote {
        object_url,
        actor_url,
        content: parsed.content_text,
        published: parsed.original_ts,
        media_urls,
        link_urls,
        hashtags: hashtag_names(obj),
        reply_parent,
        reply_root,
        labels: labels_of(obj),
        group,
        upvotes,
        downvotes,
        fetched_comments,
    })
}

/// Read the `_bytescape_group` community reference (shape set by
/// `post_view_to_activity`): `{actor_id, name, title, instance, kind, nsfw}`.
/// `None` when any required field is missing or empty — a group without a
/// canonical actor id is not a group we can register.
pub fn parse_group_ref(v: &Value) -> Option<ApGroup> {
    let actor_id = v.get("actor_id").and_then(Value::as_str)?.trim();
    if actor_id.is_empty() {
        return None;
    }
    let name = v.get("name").and_then(Value::as_str)?.trim();
    if name.is_empty() {
        return None;
    }
    let instance = v.get("instance").and_then(Value::as_str).unwrap_or("").trim();
    Some(ApGroup {
        actor_id: actor_id.to_string(),
        name: name.to_string(),
        title: v
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        instance: instance.to_string(),
        kind: v
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("lemmy")
            .to_string(),
        nsfw: v.get("nsfw").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// Read `_bytescape_counts` `{upvotes, downvotes, comments}` — clamped at
/// zero, `None` when a count is absent or not a number.
fn parse_counts(v: &Value) -> (Option<i32>, Option<i32>, Option<i32>) {
    let num = |key: &str| -> Option<i32> {
        v.get(key)
            .and_then(Value::as_i64)
            .map(|n| n.clamp(0, i32::MAX as i64) as i32)
    };
    (num("upvotes"), num("downvotes"), num("comments"))
}

/// Media URLs of an object that *is* its own media (Pixelfed `Photo`): its
/// `url`/`href` — a plain string or a link array.
fn own_media_urls(obj: &Value) -> Vec<String> {
    for key in ["url", "href"] {
        let urls: Vec<String> = match obj.get(key) {
            Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
            Some(Value::Array(links)) => links
                .iter()
                .filter_map(|l| match l {
                    Value::String(s) => Some(s.clone()),
                    Value::Object(o) => o
                        .get("href")
                        .or_else(|| o.get("url"))
                        .and_then(|x| x.as_str())
                        .map(str::to_string),
                    _ => None,
                })
                .filter(|s| !s.is_empty())
                .collect(),
            _ => Vec::new(),
        };
        if !urls.is_empty() {
            return urls;
        }
    }
    Vec::new()
}

/// Parse a Pixelfed `Story` (or bare `Photo`) — bare or wrapped in a `Create`
/// — into an [`ApStory`]. Requires at least one media URL, mirroring the AT
/// story parser which rejects zero-media records.
///
/// `None` = not a Story/Photo, missing `id`/`attributedTo`/timestamp, or no
/// usable media.
pub fn parse_story(v: &Value) -> Option<ApStory> {
    let (obj, wrapper) = unwrap_create(v)?;
    let ty = obj.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if ty != "Story" && ty != "Photo" {
        return None;
    }
    let object_url = obj
        .get("id")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    let actor_url = actor_of(obj, wrapper)?;
    let published = ts_of(obj, &["published", "created_at"])
        .or_else(|| wrapper.and_then(|w| ts_of(w, &["published", "created_at"])))?;
    let mut media_urls = attachment_urls(obj);
    if media_urls.is_empty() {
        media_urls = own_media_urls(obj);
    }
    if media_urls.is_empty() {
        return None;
    }
    let text = obj
        .get("content")
        .and_then(|c| c.as_str())
        .map(html_to_text)
        .filter(|s| !s.trim().is_empty());
    Some(ApStory {
        object_url,
        actor_url,
        published,
        text,
        media_urls,
    })
}

/// Normalize a Mobilizon `location` (a `Place` with a nested schema.org-style
/// `address`) into the flat field union `calendar::location_text` searches and
/// the calendar surface already stores. The raw `address` object is kept
/// alongside the flattened keys so no payload is lost.
fn ap_location_payload(loc: &Value) -> Value {
    let Some(obj) = loc.as_object() else {
        return loc.clone();
    };
    let mut out = serde_json::Map::new();
    out.insert("type".to_string(), json!("Place"));
    if let Some(name) = obj.get("name") {
        out.insert("name".to_string(), name.clone());
    }
    if let Some(addr) = obj.get("address").and_then(Value::as_object) {
        out.insert("address".to_string(), Value::Object(addr.clone()));
        for (from, to) in [
            ("streetAddress", "street"),
            ("addressLocality", "locality"),
            ("addressRegion", "region"),
            ("postalCode", "postalCode"),
        ] {
            if let Some(v) = addr.get(from) {
                out.insert(to.to_string(), v.clone());
            }
        }
    }
    if let Some(desc) = obj.get("description") {
        out.insert("description".to_string(), desc.clone());
    }
    Value::Object(out)
}

/// Calendar mode tokens (community lexicon).
const EVENT_MODE_PREFIX: &str = "community.lexicon.calendar.event#";

/// Parse a Mobilizon `Event` — bare or wrapped in a `Create` — into an
/// [`ApEvent`] ready for `calendar_events`.
///
/// Mapping (verified against a live `mobilizon.fr` event document):
/// `name` → name (required); `content` → description (HTML stripped);
/// `startTime`/`endTime` (aliases `startDate`/`endDate`, `startsAt`/`endsAt`)
/// → `starts_at`/`ends_at`; `isOnline` + a `location` → `mode`;
/// `ical:status`/`status` `CANCELLED` → cancelled; `location` → the `locations`
/// payload plus derived `location_text`; image attachments → `uris`
/// (`{uri, name:"image"}`, the shape the calendar surface already reads);
/// `published` → `record_created_at`; `attributedTo` → author.
///
/// `None` = not an `Event`, missing `id`/`attributedTo`, or missing `name`
/// (`calendar_events.name` is NOT NULL — a nameless event is not an event).
pub fn parse_ap_event(v: &Value) -> Option<ApEvent> {
    let (obj, wrapper) = unwrap_create(v)?;
    let ty = obj.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if !ty.eq_ignore_ascii_case("Event") {
        return None;
    }
    let object_url = obj
        .get("id")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    let actor_url = actor_of(obj, wrapper)?;
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();

    let description = obj
        .get("content")
        .and_then(Value::as_str)
        .map(html_to_text)
        .filter(|s| !s.trim().is_empty());
    let starts_at = ts_of(obj, &["startTime", "startDate", "startsAt"]);
    let ends_at = ts_of(obj, &["endTime", "endDate", "endsAt"]);
    let record_created_at = ts_of(obj, &["published", "created_at"]);

    let locations = match obj.get("location") {
        Some(Value::Array(items)) => {
            Value::Array(items.iter().map(ap_location_payload).collect())
        }
        Some(loc @ Value::Object(_)) => json!([ap_location_payload(loc)]),
        _ => json!([]),
    };
    let has_place = locations.as_array().is_some_and(|a| !a.is_empty());
    let is_online = obj
        .get("isOnline")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mode = match (is_online, has_place) {
        (true, true) => Some(format!("{EVENT_MODE_PREFIX}hybrid")),
        (true, false) => Some(format!("{EVENT_MODE_PREFIX}virtual")),
        (false, true) => Some(format!("{EVENT_MODE_PREFIX}inperson")),
        (false, false) => None,
    };

    let raw_status = obj
        .get("ical:status")
        .or_else(|| obj.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let status = if raw_status.eq_ignore_ascii_case("CANCELLED") {
        crate::calendar::EVENT_STATUS_CANCELLED
    } else {
        crate::calendar::EVENT_STATUS_SCHEDULED
    };

    let (media, _links) = attachment_parts(obj);
    let uris = Value::Array(
        media
            .iter()
            .map(|u| json!({ "uri": u, "name": "image" }))
            .collect(),
    );
    let location_text = crate::calendar::location_text(&locations);

    Some(ApEvent {
        object_url,
        actor_url,
        event: crate::calendar::ParsedEvent {
            name,
            description,
            starts_at,
            ends_at,
            mode,
            status: status.to_string(),
            location_text,
            locations,
            uris,
            record_created_at,
        },
    })
}

// ─── Handle derivation ─────────────────────────────────────────────────────

/// Actor path segments that name a *collection*, not a user: `/u/alice`,
/// `/c/technology`, `/m/name`, `/a/name` (Lemmy/PieFed), `/users/alice` and
/// `/accounts/alice` (Mastodon/Misskey/GTS). Reading the first segment as the
/// username collapsed every user on an instance onto one handle — on the
/// 2026-11 audit `@u@lemmy.world` covered 214 distinct actors over 404 rows.
const ACTOR_PATH_PREFIXES: &[&str] = &["u", "c", "m", "a", "users", "accounts"];

/// `@user@instance` from an actor URL's host+path. The common `/users/name`
/// and `/@name` shapes go through the shared `handle_from_url`; otherwise the
/// first path segment is skipped when it names a collection
/// ([`ACTOR_PATH_PREFIXES`]) and the next one is used. Returns `None` when no
/// username can be extracted — callers must then skip the handle write
/// (`appview_posts.author_handle` must stay non-NULL, and a wrong handle is
/// worse than none).
fn derive_handle(actor_url: &str) -> Option<String> {
    if let Some((user, host)) = handle_from_url(actor_url) {
        return Some(format!("@{user}@{host}"));
    }
    let u = url::Url::parse(actor_url).ok()?;
    let host = u.host_str()?.to_lowercase();
    let mut segments = u
        .path()
        .trim_start_matches('/')
        .split('/')
        .map(|s| s.trim_start_matches('@'))
        .filter(|s| !s.is_empty());
    let first = segments.next()?;
    let name = if ACTOR_PATH_PREFIXES
        .iter()
        .any(|p| first.eq_ignore_ascii_case(p))
    {
        segments.next()?
    } else {
        first
    };
    if name.is_empty() {
        None
    } else {
        Some(format!("@{}@{}", name.to_lowercase(), host))
    }
}

// ─── Ingestion ─────────────────────────────────────────────────────────────

/// Upsert one [`ApNote`] into `appview_posts` **and** its hashtags into
/// `appview_post_tags`, in one statement so a single `PgExecutor` (pool or
/// transaction) suffices. Idempotent (`ON CONFLICT (uri) DO UPDATE`);
/// interaction counts are preserved, like every other upsert into the table.
///
/// * `embeds` carries media as `{"type":"image","url":…}` and external URLs as
///   `{"type":"link","url":…}` — the read path maps the latter to a link card
///   instead of an `<img>` against an HTML page.
/// * `has_media` stays false for link-only posts (a link is not media).
/// * `labels` records the object's NSFW/CW flags (NULL when clean).
/// * `reply_parent`/`reply_root` keep comments out of the top-level feed.
/// * tag rows use the same `(tag, uri, created_at)` shape and lowercasing as
///   the 064/068 text trigger, so interest matching sees one vocabulary.
///
/// No-op when `AP_INGEST_ENABLED` is off, or when no handle can be derived
/// from `actor_url` (a non-NULL `author_handle` is required by the feed's
/// handle-resolver).
pub async fn ingest_note(
    pool: impl sqlx::PgExecutor<'_>,
    note: &ApNote,
    handle_override: Option<&str>,
) -> Result<(), sqlx::Error> {
    if !crate::ap::ap_ingest_enabled() {
        return Ok(());
    }
    let Some(handle) = handle_override
        .map(|h| h.to_string())
        .or_else(|| derive_handle(&note.actor_url))
    else {
        return Ok(());
    };
    // AP objects have no CID: the object URL is the stable id (same value as
    // uri). Interaction counts (like_count etc.) are schema defaults.
    let mut embeds: Vec<Value> = Vec::with_capacity(note.media_urls.len() + note.link_urls.len());
    embeds.extend(
        note.media_urls
            .iter()
            .map(|u| json!({ "type": "image", "url": u })),
    );
    embeds.extend(
        note.link_urls
            .iter()
            .map(|u| json!({ "type": "link", "url": u })),
    );
    let embeds = (!embeds.is_empty()).then(|| json!(embeds));
    let has_media = !note.media_urls.is_empty();
    let labels = (!note.labels.is_empty()).then(|| json!(note.labels));
    let tags: Vec<String> = note
        .hashtags
        .iter()
        .map(|t| t.trim_start_matches('#').trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    // Group registry upsert + post→group link (design D1): the community
    // reference comes from the Lemmy/PieFed fetchers; direct posts carry
    // `group = None` and get a NULL `group_actor_id`. Done in one statement
    // (`write_group_and_post`) so a single `PgExecutor` (pool or transaction)
    // suffices — `ingest_note`'s executor must not be used twice.
    let group_actor_id = note.group.as_ref().map(|g| g.actor_id.clone());
    write_group_and_post(
        pool,
        note,
        handle,
        &embeds,
        has_media,
        &labels,
        &tags,
        group_actor_id.as_deref(),
    )
    .await
    .map(|_| ())
}

/// Persist one note's group reference (if any) and its `appview_posts` row +
/// tag rows. Split out of [`ingest_note`] purely so the executor is used once;
/// the SQL semantics are documented at the call site.
#[allow(clippy::too_many_arguments)]
async fn write_group_and_post(
    pool: impl sqlx::PgExecutor<'_>,
    note: &ApNote,
    handle: String,
    embeds: &Option<Value>,
    has_media: bool,
    labels: &Option<Value>,
    tags: &[String],
    group_actor_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    let group = note.group.as_ref();
    // A present `group` always has a non-empty `actor_id` (`post_view_to_activity`
    // refuses to build one without it). The `g` CTE uses `INSERT … SELECT …
    // WHERE` (the only valid way to conditionally skip an INSERT) so an empty
    // actor id can never write a junk registry row — `INSERT … VALUES` has no
    // WHERE clause.
    sqlx::query(
        r#"WITH g AS (
             INSERT INTO ap_groups (actor_id, name, title, instance, kind, nsfw, updated_at)
             SELECT $17, $18, $19, $20, $21, $22, now()
             WHERE $17 <> ''
             ON CONFLICT (actor_id) DO UPDATE SET
               name = EXCLUDED.name,
               title = COALESCE(EXCLUDED.title, ap_groups.title),
               instance = EXCLUDED.instance,
               kind = EXCLUDED.kind,
               nsfw = EXCLUDED.nsfw,
               updated_at = now()
           ), upserted AS (
             INSERT INTO appview_posts (uri, cid, author_did, author_handle, text, reply_parent, reply_root, created_at, embeds, has_media, labels, group_actor_id, upvote_count, downvote_count, comment_count)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
             ON CONFLICT (uri) DO UPDATE SET
               text = EXCLUDED.text,
               author_handle = COALESCE(EXCLUDED.author_handle, appview_posts.author_handle),
               embeds = COALESCE(EXCLUDED.embeds, appview_posts.embeds),
               has_media = appview_posts.has_media OR EXCLUDED.has_media,
               reply_parent = COALESCE(EXCLUDED.reply_parent, appview_posts.reply_parent),
               reply_root = COALESCE(EXCLUDED.reply_root, appview_posts.reply_root),
               labels = COALESCE(EXCLUDED.labels, appview_posts.labels),
               group_actor_id = COALESCE(EXCLUDED.group_actor_id, appview_posts.group_actor_id),
               -- Vote counts are refreshed on re-ingest only when the fetcher
               -- actually reported them (`EXCLUDED.* <> 0`): a post re-pulled
               -- from a source that omits counts must not zero real numbers.
               upvote_count = CASE WHEN EXCLUDED.upvote_count <> 0 THEN EXCLUDED.upvote_count ELSE appview_posts.upvote_count END,
               downvote_count = CASE WHEN EXCLUDED.downvote_count <> 0 THEN EXCLUDED.downvote_count ELSE appview_posts.downvote_count END,
               comment_count = CASE WHEN EXCLUDED.comment_count <> 0 THEN EXCLUDED.comment_count ELSE appview_posts.comment_count END,
               indexed_at = now()
             RETURNING uri
           )
           INSERT INTO appview_post_tags (tag, uri, created_at)
           SELECT DISTINCT t, $1, $8
             FROM unnest($16::text[]) AS t
           ON CONFLICT DO NOTHING"#,
    )
    .bind(&note.object_url)
    .bind(&note.object_url)
    .bind(&note.actor_url)
    .bind(handle)
    .bind(note.content.replace('\u{0}', ""))
    .bind(note.reply_parent.as_deref())
    .bind(note.reply_root.as_deref())
    .bind(note.published)
    .bind(embeds.as_ref())
    .bind(has_media)
    .bind(labels.as_ref())
    .bind(group_actor_id)
    .bind(note.upvotes.unwrap_or(0))
    .bind(note.downvotes.unwrap_or(0))
    .bind(note.fetched_comments.unwrap_or(0))
    .bind(tags)
    .bind(group.map(|g| g.actor_id.as_str()).unwrap_or(""))
    .bind(group.map(|g| g.name.as_str()).unwrap_or(""))
    .bind(group.and_then(|g| g.title.as_deref()).unwrap_or(""))
    .bind(group.map(|g| g.instance.as_str()).unwrap_or(""))
    .bind(group.map(|g| g.kind.as_str()).unwrap_or(""))
    .bind(group.map(|g| g.nsfw).unwrap_or(false))
    .execute(pool)
    .await
    .map(|_| ())
}

/// Upsert one [`ApEvent`] into `calendar_events` with
/// `source = 'activitypub'`. The handle is derived from the actor URL (remote
/// AP actors are not in the AT DID→handle map, so there is nothing to look up).
///
/// No-op when `AP_INGEST_ENABLED` is off.
pub async fn ingest_event(
    pool: impl sqlx::PgExecutor<'_>,
    event: &ApEvent,
) -> Result<(), sqlx::Error> {
    if !crate::ap::ap_ingest_enabled() {
        return Ok(());
    }
    let handle = derive_handle(&event.actor_url);
    crate::calendar::insert_event(
        pool,
        &event.object_url,
        &event.object_url,
        &event.actor_url,
        handle.as_deref(),
        event.event.clone(),
        "activitypub",
    )
    .await
}

/// Upsert one [`ApStory`] into `appview_stories`. Live from `published`,
/// expiring 24h later (the Pixelfed/AT story lifetime). Idempotent; the
/// `ON CONFLICT` clause mirrors `consumer.rs::upsert_story_row` so interaction
/// counts survive re-ingests.
///
/// No-op when `AP_INGEST_ENABLED` is off.
pub async fn ingest_story(pool: impl sqlx::PgExecutor<'_>, story: &ApStory) -> Result<(), sqlx::Error> {
    if !crate::ap::ap_ingest_enabled() {
        return Ok(());
    }
    let media = json!(story.media_urls);
    let expires_at = story.published + Duration::hours(24);
    sqlx::query(
        r#"INSERT INTO appview_stories (uri, cid, author_did, created_at, active_from, expires_at, pinned, text, media, can_react, can_reply)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
           ON CONFLICT (uri) DO UPDATE SET
             cid = EXCLUDED.cid, text = EXCLUDED.text, media = EXCLUDED.media,
             active_from = EXCLUDED.active_from, expires_at = EXCLUDED.expires_at,
             pinned = EXCLUDED.pinned, can_react = EXCLUDED.can_react, can_reply = EXCLUDED.can_reply,
             updated_at = now()"#,
    )
    .bind(&story.object_url)
    .bind(&story.object_url)
    .bind(&story.actor_url)
    .bind(story.published)
    .bind(story.published)
    .bind(expires_at)
    .bind(false)
    .bind(story.text.as_deref().filter(|s| !s.trim().is_empty()))
    .bind(&media)
    .bind(true)
    .bind(true)
    .execute(pool)
    .await
    .map(|_| ())
}

/// Ingest one AP activity (or bare object) — the shared entry point for the
/// inbox receiver and the outbox poller.
///
/// Dispatch: `Create` → its object (`Note` → [`ingest_note`], `Event` →
/// [`ingest_event`], `Story`/`Photo` → [`ingest_story`]); a bare
/// `Note`/`Event`/`Story`/`Photo` → ingested directly.
///
/// Returns the number of rows ingested: `0` when `AP_INGEST_ENABLED` is off,
/// the activity type is unrecognized, or the object fails to parse; `Err` on a
/// DB failure (the message is the `sqlx::Error` display).
pub async fn ingest_activity(
    pool: impl sqlx::PgExecutor<'_>,
    activity: &Value,
    handle_override: Option<&str>,
) -> Result<usize, String> {
    if !crate::ap::ap_ingest_enabled() {
        return Ok(0);
    }
    match activity.get("type").and_then(|t| t.as_str()).unwrap_or("") {
        "Create" | "Note" | "Question" | "Article" | "Page" | "Audio" | "Video" => {
            if let Some(note) = parse_note(activity) {
                // Skip early when no handle is derivable: ingest_note would
                // no-op, and the count must reflect actual writes.
                if handle_override.is_none() && derive_handle(&note.actor_url).is_none() {
                    return Ok(0);
                }
                ingest_note(pool, &note, handle_override).await.map_err(|e| e.to_string())?;
                return Ok(1);
            }
            if let Some(story) = parse_story(activity) {
                ingest_story(pool, &story).await.map_err(|e| e.to_string())?;
                return Ok(1);
            }
            if let Some(event) = parse_ap_event(activity) {
                ingest_event(pool, &event).await.map_err(|e| e.to_string())?;
                return Ok(1);
            }
            Ok(0)
        }
        "Story" | "Photo" => {
            if let Some(story) = parse_story(activity) {
                ingest_story(pool, &story).await.map_err(|e| e.to_string())?;
                return Ok(1);
            }
            Ok(0)
        }
        "Event" => {
            if let Some(event) = parse_ap_event(activity) {
                ingest_event(pool, &event).await.map_err(|e| e.to_string())?;
                return Ok(1);
            }
            Ok(0)
        }
        _ => Ok(0),
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    async fn connect_pool() -> Option<sqlx::PgPool> {
        if std::env::var("DATABASE_URL").is_err() {
            let _ = dotenvy::dotenv();
            let _ = dotenvy::from_filename("bytescape-server/.env");
        }
        match std::env::var("DATABASE_URL") {
            Ok(url) => Some(sqlx::PgPool::connect(&url).await.expect("db connect")),
            Err(_) => {
                eprintln!("DATABASE_URL unset — skipping DB integration test");
                None
            }
        }
    }

    /// A realistic Mastodon `Create(Note)` with hashtag + image attachment.
    fn create_note_activity(suffix: &str) -> Value {
        serde_json::json!({
            "id": format!("https://mastodon.test/users/alice/statuses/{suffix}/activity"),
            "type": "Create",
            "actor": "https://mastodon.test/users/alice",
            "published": "2026-11-01T12:00:00Z",
            "to": ["https://www.w3.org/ns/activitystreams#Public"],
            "object": {
                "id": format!("https://mastodon.test/users/alice/statuses/{suffix}"),
                "type": "Note",
                "attributedTo": "https://mastodon.test/users/alice",
                "content": "<p>Hello <strong>fediverse</strong> &amp; friends!</p>",
                "published": "2026-11-01T12:00:00Z",
                "tag": [
                    {"type": "Hashtag", "href": "https://mastodon.test/tags/cats", "name": "cats"},
                    {"type": "Mention", "href": "https://mastodon.test/users/bob", "name": "@bob@mastodon.test"}
                ],
                "attachment": [
                    {"type": "Image", "mediaType": "image/jpeg", "url": "https://mastodon.test/media/abc123.jpeg"}
                ]
            }
        })
    }

    /// A Lemmy comment arriving as a BARE Note (no Create wrapper) — the shape
    /// `ingest_from_actor_outbox`'s `activities_from_outbox` can hand to
    /// `ingest_activity` when an outbox item is the object itself. `inReplyTo`
    /// must still become `reply_parent`. Regression: 374 live comment rows had
    /// `reply_parent = NULL`, keeping comments off every post's thread.
    #[test]
    fn parse_note_bare_comment_captures_in_reply_to() {
        let note = serde_json::json!({
            "type": "Note",
            "id": "https://lemmy.world/comment/26860538",
            "attributedTo": "https://lemmy.world/u/alice",
            "content": "<p>a reply</p>",
            "published": "2026-11-16T12:00:00Z",
            "inReplyTo": "https://thelemmy.club/post/58857491"
        });
        let parsed = parse_note(&note).expect("bare comment parses");
        assert_eq!(
            parsed.reply_parent.as_deref(),
            Some("https://thelemmy.club/post/58857491"),
            "bare Note inReplyTo must become reply_parent"
        );
    }

    #[test]
    fn parse_note_accepts_lemmy_page_with_title_and_link() {
        // A Lemmy link post is a `Page` with a title + external URL and no body.
        let activity = serde_json::json!({
            "type": "Create",
            "actor": "https://lemmy.world/u/alice",
            "published": "2026-11-01T12:00:00Z",
            "object": {
                "id": "https://lemmy.world/post/123",
                "type": "Page",
                "attributedTo": "https://lemmy.world/u/alice",
                "name": "Cool link",
                "url": "https://example.com/article",
                "published": "2026-11-01T12:00:00Z"
            }
        });
        let note = parse_note(&activity).expect("Lemmy Page parses");
        assert_eq!(note.object_url, "https://lemmy.world/post/123");
        assert_eq!(note.actor_url, "https://lemmy.world/u/alice");
        assert!(
            note.content.contains("Cool link"),
            "title becomes content: {:?}",
            note.content
        );
    }

    #[test]
    fn parse_note_from_create_and_bare_note() {
        let activity = create_note_activity("1088");
        let note = parse_note(&activity).expect("Create+Note parses");
        assert_eq!(
            note.object_url,
            "https://mastodon.test/users/alice/statuses/1088"
        );
        assert_eq!(note.actor_url, "https://mastodon.test/users/alice");
        assert_eq!(note.content, "Hello fediverse & friends!");
        assert_eq!(note.published.to_rfc3339(), "2026-11-01T12:00:00+00:00");
        assert_eq!(
            note.media_urls,
            vec!["https://mastodon.test/media/abc123.jpeg"]
        );
        assert_eq!(note.hashtags, vec!["cats"]);

        // The bare Note parses to the same row.
        let bare = parse_note(&activity["object"]).expect("bare Note parses");
        assert_eq!(bare.object_url, note.object_url);
        assert_eq!(bare.content, note.content);
        assert_eq!(bare.published, note.published);
        assert_eq!(bare.media_urls, note.media_urls);
        assert_eq!(bare.hashtags, note.hashtags);

        // Non-Note objects are rejected.
        assert!(parse_note(&serde_json::json!({"type": "Like", "id": "https://mastodon.test/x"})).is_none());
    }

    #[test]
    fn parse_story_from_story_and_photo() {
        let story = serde_json::json!({
            "id": "https://pixelfed.test/users/alice/stories/9",
            "type": "Story",
            "attributedTo": "https://pixelfed.test/users/alice",
            "published": "2026-11-02T08:30:00Z",
            "content": "<p>beach day</p>",
            "attachment": [
                {"type": "Image", "mediaType": "image/jpeg", "url": "https://pixelfed.test/media/s9.jpeg"}
            ]
        });
        let parsed = parse_story(&story).expect("Story parses");
        assert_eq!(parsed.object_url, "https://pixelfed.test/users/alice/stories/9");
        assert_eq!(parsed.actor_url, "https://pixelfed.test/users/alice");
        assert_eq!(parsed.media_urls, vec!["https://pixelfed.test/media/s9.jpeg"]);
        assert_eq!(parsed.text.as_deref(), Some("beach day"));
        assert_eq!(parsed.published.to_rfc3339(), "2026-11-02T08:30:00+00:00");

        // Bare Photo: the object itself carries the media (no attachment).
        let photo = serde_json::json!({
            "id": "https://pixelfed.test/storage/1.jpg",
            "type": "Photo",
            "attributedTo": "https://pixelfed.test/users/alice",
            "published": "2026-11-02T09:00:00+00:00",
            "url": "https://pixelfed.test/media/1.jpg"
        });
        let parsed = parse_story(&photo).expect("Photo parses");
        assert_eq!(parsed.media_urls, vec!["https://pixelfed.test/media/1.jpg"]);
        assert_eq!(parsed.text, None);

        // A media-less object is rejected (mirrors the AT story parser).
        assert!(parse_story(&serde_json::json!({
            "id": "https://pixelfed.test/stories/empty",
            "type": "Story",
            "attributedTo": "https://pixelfed.test/users/alice",
            "published": "2026-11-02T08:30:00Z"
        }))
        .is_none());
    }

    /// End-to-end dispatch: `Create(Note)` and a bare `Note` both land one
    /// `appview_posts` row each with a non-NULL `author_handle`; re-ingesting
    /// is idempotent (one row per uri). Skips without `DATABASE_URL`.
    #[tokio::test]
    async fn ingest_activity_ingests_create_note_and_bare_note() {
        let Some(pool) = connect_pool().await else { return };
        // Edition 2024: `env::set_var` is unsafe. Tests only ever flip the
        // flag to "true" (the value every AP-ingest test needs).
        unsafe { std::env::set_var("AP_INGEST_ENABLED", "true") };

        let suffix = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let create = create_note_activity(&suffix);
        let create_uri = format!("https://mastodon.test/users/alice/statuses/{suffix}");
        let bare_uri = format!("https://mastodon.test/users/bob/statuses/{suffix}");
        let bare = serde_json::json!({
            "id": bare_uri,
            "type": "Note",
            "attributedTo": "https://mastodon.test/users/bob",
            "content": "<p>bare note body</p>",
            "published": "2026-11-03T09:00:00Z"
        });

        let n1 = ingest_activity(&pool, &create, None).await.expect("ingest Create(Note)");
        let n2 = ingest_activity(&pool, &create, None)
            .await
            .expect("re-ingest Create(Note)");
        let n3 = ingest_activity(&pool, &bare, None).await.expect("ingest bare Note");

        let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
            r#"SELECT uri, author_handle, text FROM appview_posts
               WHERE uri IN ($1, $2)"#,
        )
        .bind(&create_uri)
        .bind(&bare_uri)
        .fetch_all(&pool)
        .await
        .expect("select ingested rows");

        // Clean up before asserting so a failed assert leaves no fixture rows.
        let _ = sqlx::query("DELETE FROM appview_posts WHERE uri IN ($1, $2)")
            .bind(&create_uri)
            .bind(&bare_uri)
            .execute(&pool)
            .await;

        assert_eq!(n1, 1, "Create(Note) ingests");
        assert_eq!(n2, 1, "idempotent re-ingest upserts the same row");
        assert_eq!(n3, 1, "bare Note ingests");
        assert_eq!(rows.len(), 2, "one row per uri despite the re-ingest");

        let (_, handle, _) = rows
            .iter()
            .find(|r| r.0 == create_uri)
            .expect("Create(Note) row present");
        assert_eq!(
            handle.as_deref(),
            Some("@alice@mastodon.test"),
            "author_handle derived from the actor URL"
        );

        let (_, _, text) = rows
            .iter()
            .find(|r| r.0 == bare_uri)
            .expect("bare Note row present");
        assert_eq!(text.as_deref(), Some("bare note body"));
    }

    // ─── Handle derivation (R3) ───────────────────────────────────────────

    #[test]
    fn derive_handle_skips_actor_collection_segments() {
        // Lemmy/PieFed actor paths are `/u/{name}`, not `/{name}`.
        assert_eq!(
            derive_handle("https://lemmy.world/u/alice").as_deref(),
            Some("@alice@lemmy.world")
        );
        assert_eq!(
            derive_handle("https://piefed.social/u/Bob").as_deref(),
            Some("@bob@piefed.social")
        );
        // Communities are addressed the same way — never collapse to `@c@host`.
        assert_eq!(
            derive_handle("https://lemmy.ml/c/technology").as_deref(),
            Some("@technology@lemmy.ml")
        );
        assert_eq!(
            derive_handle("https://lemmy.ml/m/news").as_deref(),
            Some("@news@lemmy.ml")
        );
        // Mastodon-shaped actors keep working.
        assert_eq!(
            derive_handle("https://mastodon.test/users/bob").as_deref(),
            Some("@bob@mastodon.test")
        );
        assert_eq!(
            derive_handle("https://pixelfed.test/@alice").as_deref(),
            Some("@alice@pixelfed.test")
        );
        // A bare single-segment path still parses.
        assert_eq!(
            derive_handle("https://example.test/alice").as_deref(),
            Some("@alice@example.test")
        );
        // The collapse itself: two users on one instance must not share a handle.
        assert_ne!(
            derive_handle("https://lemmy.world/u/alice"),
            derive_handle("https://lemmy.world/u/bob")
        );
        // Nothing derivable.
        assert_eq!(derive_handle("https://lemmy.world/u/"), None);
    }

    // ─── Reply / labels / link embeds (R5, R6, R7) ────────────────────────

    #[test]
    fn parse_note_maps_reply_labels_and_link_attachments() {
        let activity = serde_json::json!({
            "type": "Create",
            "actor": "https://lemmy.world/u/alice",
            "object": {
                "id": "https://lemmy.world/comment/42",
                "type": "Note",
                "attributedTo": "https://lemmy.world/u/alice",
                "content": "<p>a reply with a link</p>",
                "published": "2026-11-01T12:00:00Z",
                "inReplyTo": "https://lemmy.world/post/41",
                "context": "https://lemmy.world/post/40",
                "summary": "spoilers",
                "attachment": [
                    {"type": "Link", "url": "https://example.com/article"},
                    {"type": "Image", "url": "https://lemmy.world/pictrs/image/a.png"}
                ]
            }
        });
        let note = parse_note(&activity).expect("comment parses");
        assert_eq!(
            note.reply_parent.as_deref(),
            Some("https://lemmy.world/post/41"),
            "inReplyTo becomes reply_parent"
        );
        assert_eq!(
            note.reply_root.as_deref(),
            Some("https://lemmy.world/post/40"),
            "context wins as the thread root"
        );
        assert_eq!(
            note.link_urls,
            vec!["https://example.com/article".to_string()],
            "Link attachments are links, not media"
        );
        assert_eq!(
            note.media_urls,
            vec!["https://lemmy.world/pictrs/image/a.png".to_string()],
            "Image attachments stay media"
        );
        assert_eq!(note.labels, vec!["!warn".to_string()], "Note summary is a CW");

        // `sensitive` adds the nsfw label; the root falls back to the parent.
        let sensitive = serde_json::json!({
            "id": "https://lemmy.world/post/9",
            "type": "Note",
            "attributedTo": "https://lemmy.world/u/alice",
            "content": "flagged",
            "published": "2026-11-01T12:00:00Z",
            "sensitive": true,
            "inReplyTo": "https://lemmy.world/post/8"
        });
        let flagged = parse_note(&sensitive).expect("sensitive note parses");
        assert_eq!(flagged.labels, vec!["nsfw".to_string()]);
        assert_eq!(
            flagged.reply_root.as_deref(),
            Some("https://lemmy.world/post/8"),
            "no context → the parent is the best-effort root"
        );
        assert!(flagged.link_urls.is_empty());

        // A clean top-level Note carries no reply and no labels.
        let plain = parse_note(&create_note_activity("labels-clean")).expect("plain note parses");
        assert!(plain.reply_parent.is_none());
        assert!(plain.labels.is_empty());
    }

    // ─── AP Event mapping (R4) ────────────────────────────────────────────

    /// The live `mobilizon.fr` document shape (see the 2026-11 audit), trimmed
    /// to the fields the mapper reads.
    fn mobilizon_event(id: &str) -> Value {
        serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": id,
            "type": "Event",
            "name": "Flowers in the Park: Discover the Art of Floristry",
            "startTime": "2027-11-14T08:00:00Z",
            "endTime": "2027-11-14T20:45:00Z",
            "attributedTo": "https://mobilizon.fr/@luxy",
            "actor": "https://mobilizon.fr/@luxy",
            "published": "2026-10-16T16:47:27Z",
            "content": "<p># Flowers in the Park</p><p>Join us for a beautiful day.</p>",
            "isOnline": false,
            "joinMode": "free",
            "ical:status": "CONFIRMED",
            "tag": [],
            "attachment": [{
                "type": "Document",
                "mediaType": "image/jpeg",
                "name": "Banner",
                "url": "https://mobilizon.fr/media/banner.jpg"
            }],
            "location": {
                "type": "Place",
                "name": "Parc Léi",
                "address": {
                    "streetAddress": "1 rue de la Forêt",
                    "addressLocality": "Luxembourg",
                    "addressCountry": "LU"
                }
            },
            "to": ["https://www.w3.org/ns/activitystreams#Public"]
        })
    }

    #[test]
    fn parse_ap_event_maps_mobilizon_fields() {
        let parsed = parse_ap_event(&mobilizon_event("https://mobilizon.fr/events/e1"))
            .expect("Event parses");
        assert_eq!(parsed.object_url, "https://mobilizon.fr/events/e1");
        assert_eq!(parsed.actor_url, "https://mobilizon.fr/@luxy");
        let ev = parsed.event;
        assert_eq!(ev.name, "Flowers in the Park: Discover the Art of Floristry");
        assert_eq!(
            ev.starts_at.map(|t| t.to_rfc3339()),
            Some("2027-11-14T08:00:00+00:00".to_string())
        );
        assert_eq!(
            ev.ends_at.map(|t| t.to_rfc3339()),
            Some("2027-11-14T20:45:00+00:00".to_string())
        );
        assert_eq!(
            ev.mode.as_deref(),
            Some("community.lexicon.calendar.event#inperson")
        );
        assert_eq!(ev.status, crate::calendar::EVENT_STATUS_SCHEDULED);
        assert_eq!(
            ev.record_created_at.map(|t| t.to_rfc3339()),
            Some("2026-10-16T16:47:27+00:00".to_string())
        );
        let description = ev.description.expect("content becomes description");
        assert!(description.contains("Join us for a beautiful day"), "{description:?}");
        assert!(description.contains("Flowers in the Park"));
        assert_eq!(
            ev.location_text.as_deref(),
            Some("Parc Léi, 1 rue de la Forêt, Luxembourg"),
            "nested schema.org address is flattened for location_text"
        );
        let uris = ev.uris.as_array().expect("uris is an array");
        assert_eq!(uris.len(), 1);
        assert_eq!(uris[0]["name"], "image");
        assert_eq!(uris[0]["uri"], "https://mobilizon.fr/media/banner.jpg");

        // Wrapped in a Create (the inbox shape) parses identically.
        let wrapped = serde_json::json!({
            "type": "Create",
            "actor": "https://mobilizon.fr/@luxy",
            "object": mobilizon_event("https://mobilizon.fr/events/e2")
        });
        assert_eq!(
            parse_ap_event(&wrapped).expect("Create(Event) parses").object_url,
            "https://mobilizon.fr/events/e2"
        );
    }

    #[test]
    fn parse_ap_event_maps_online_cancelled_and_rejects_non_events() {
        let online = serde_json::json!({
            "id": "https://mobilizon.test/events/2",
            "type": "Event",
            "attributedTo": "https://mobilizon.test/@a",
            "name": "Online Jam",
            "startTime": "2026-12-01T18:00:00Z",
            "isOnline": true,
            "ical:status": "CANCELLED"
        });
        let parsed = parse_ap_event(&online).expect("online event parses");
        assert_eq!(
            parsed.event.mode.as_deref(),
            Some("community.lexicon.calendar.event#virtual")
        );
        assert_eq!(parsed.event.status, crate::calendar::EVENT_STATUS_CANCELLED);
        assert_eq!(parsed.event.description, None);
        assert!(parsed.event.starts_at.is_some());

        // Online + a place is a hybrid.
        let mut hybrid = online.clone();
        hybrid["isOnline"] = serde_json::json!(true);
        hybrid["location"] = serde_json::json!({"type": "Place", "name": "Café"});
        assert_eq!(
            parse_ap_event(&hybrid).unwrap().event.mode.as_deref(),
            Some("community.lexicon.calendar.event#hybrid")
        );

        // A nameless Event is not an event; a Note is not an Event.
        let mut nameless = online.clone();
        nameless.as_object_mut().unwrap().remove("name");
        assert!(parse_ap_event(&nameless).is_none());
        assert!(parse_ap_event(&create_note_activity("not-an-event")).is_none());
    }

    // ─── DB: tag index, labels, reply parent (R2, R5, R6) ────────────────

    /// One `Create(Note)` must write the hashtag into `appview_post_tags` (the
    /// same rows the 064/068 text trigger writes), the flags into `labels`, and
    /// `inReplyTo` into `reply_parent`. Skips without `DATABASE_URL`.
    #[tokio::test]
    async fn ingest_note_writes_tag_labels_and_reply_parent() {
        let Some(pool) = connect_pool().await else { return };
        unsafe { std::env::set_var("AP_INGEST_ENABLED", "true") };

        let suffix = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let uri = format!("https://lemmy.world/comment/{suffix}");
        let activity = serde_json::json!({
            "type": "Create",
            "actor": "https://lemmy.world/u/alice",
            "object": {
                "id": uri,
                "type": "Note",
                "attributedTo": "https://lemmy.world/u/alice",
                "content": "<p>hello #Fediverse</p>",
                "published": "2026-11-01T12:00:00Z",
                "inReplyTo": "https://lemmy.world/post/1",
                "sensitive": true,
                // Community tag from the puller's mapping — never inline text.
                "tag": [{"type": "Hashtag", "name": "#programmer_humor"}]
            }
        });

        let n = ingest_activity(&pool, &activity, None)
            .await
            .expect("ingest Create(Note)");

        let row: (Option<String>, Option<serde_json::Value>, Option<String>) = sqlx::query_as(
            "SELECT reply_parent, labels, author_handle FROM appview_posts WHERE uri = $1",
        )
        .bind(&uri)
        .fetch_one(&pool)
        .await
        .expect("ingested row");
        // `kw:` rows come from the topic-keyword trigger over the local DB's
        // vocabulary; this test is about the hashtag rows.
        let tags: Vec<String> = sqlx::query_scalar(
            "SELECT tag FROM appview_post_tags \
              WHERE uri = $1 AND tag NOT LIKE 'kw:%' ORDER BY tag",
        )
        .bind(&uri)
        .fetch_all(&pool)
        .await
        .expect("tag rows");

        // Clean up before asserting so a failed assert leaves no fixture rows.
        let _ = sqlx::query("DELETE FROM appview_post_tags WHERE uri = $1")
            .bind(&uri)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM appview_posts WHERE uri = $1")
            .bind(&uri)
            .execute(&pool)
            .await;

        assert_eq!(n, 1);
        assert_eq!(row.2.as_deref(), Some("@alice@lemmy.world"), "Lemmy actor handle");
        assert_eq!(
            row.0.as_deref(),
            Some("https://lemmy.world/post/1"),
            "inReplyTo stored as reply_parent (keeps comments out of the feed tier)"
        );
        assert_eq!(row.1, Some(serde_json::json!(["nsfw"])));
        assert_eq!(
            tags,
            vec!["fediverse".to_string(), "programmer_humor".to_string()],
            "both the inline hashtag (trigger) and the tag array (ingest) land"
        );
    }

    // ─── DB: Mobilizon Event → calendar_events (R4) ──────────────────────

    /// A bare `Event` must land in `calendar_events` with `source =
    /// 'activitypub'` and its real start time. Skips without `DATABASE_URL`.
    #[tokio::test]
    async fn ingest_ap_event_writes_activitypub_calendar_row() {
        let Some(pool) = connect_pool().await else { return };
        unsafe { std::env::set_var("AP_INGEST_ENABLED", "true") };

        let suffix = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let uri = format!("https://mobilizon.test/events/{suffix}");
        let event = mobilizon_event(&uri);

        let n = ingest_activity(&pool, &event, None)
            .await
            .expect("ingest Event");

        let row: (String, Option<String>, Option<String>, String, Option<String>) = sqlx::query_as(
            "SELECT name, author_handle, starts_at::text, source, mode FROM calendar_events WHERE uri = $1",
        )
        .bind(&uri)
        .fetch_one(&pool)
        .await
        .expect("calendar row");
        let _ = sqlx::query("DELETE FROM calendar_events WHERE uri = $1")
            .bind(&uri)
            .execute(&pool)
            .await;

        assert_eq!(n, 1, "Event ingests");
        assert_eq!(row.0, "Flowers in the Park: Discover the Art of Floristry");
        assert_eq!(row.1.as_deref(), Some("@luxy@mobilizon.fr"));
        assert_eq!(row.2.as_deref(), Some("2027-11-14 08:00:00+00"));
        assert_eq!(row.3, "activitypub", "AP events are tagged as such");
        assert_eq!(
            row.4.as_deref(),
            Some("community.lexicon.calendar.event#inperson")
        );
    }
}
