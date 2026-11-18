//! Mastodon GDPR archive import.
//!
//! Implements `specs/import/mastodon-archive/spec.md`:
//! * **R1** — `outbox.json` Create/Announce activities (plus bare Notes in the
//!   owner's own outbox) become [`ParsedPost`]s with `action_text` posted /
//!   replied / boosted; `following.json` / `followers.json` become
//!   [`ContactEntry`]s carrying `remote_handle` = `@user@instance`.
//! * **R2** — posts and contacts land in the same tables the Facebook and
//!   Instagram importers write to, with `source = "mastodon"`.
//! * **R3** — [`match_mastodon_contacts`] links local users by handle or
//!   instance and records `social_graph_matches`; unmatched contacts keep a
//!   populated `contact_handle` so they surface in the "may be on the network"
//!   list like every other source.
//! * **R4 / R5** — malformed entries are skipped with an error, counted into
//!   `total_posts`, recorded in `import_sessions.metadata` (`skipped`), and the
//!   session still completes.
//! * **R6** — archive only: this module makes **no** network calls; attachment
//!   URLs are recorded as references and never fetched.

use axum::{
    extract::{DefaultBodyLimit, Multipart, State},
    http::HeaderMap,
    routing::post,
    Json, Router,
};
use chrono::{DateTime, Utc};
use regex::{Captures, Regex};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::LazyLock;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::api::import::ContactEntry;
use crate::api::import_facebook::ParsedPost;
use crate::{auth, db, error::AppError, AppState};

// ─── HTML → text ────────────────────────────────────────────────────────────

static HTML_BR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<br\s*/?>").unwrap());
static HTML_P_CLOSE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)</p>").unwrap());
static HTML_TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());
static WS_RUN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]+").unwrap());
static NL_RUN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").unwrap());
static NUM_ENT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&#(\d{1,6});").unwrap());
static HEX_ENT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&#x([0-9a-fA-F]{1,6});").unwrap());

/// Convert ActivityPub HTML content (`<p>`, `<br>`, links) to plain text.
/// Entity decoding is ordered so `&amp;` is resolved last — decoding it first
/// would double-decode inputs like `&amp;lt;`.
pub(crate) fn html_to_text(html: &str) -> String {
    let s = HTML_BR_RE.replace_all(html, "\n");
    let s = HTML_P_CLOSE_RE.replace_all(&s, "\n");
    let s = HTML_TAG_RE.replace_all(&s, "");
    let mut out = s.into_owned();
    for (ent, ch) in [
        ("&nbsp;", " "),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&apos;", "'"),
        ("&rsquo;", "\u{2019}"),
        ("&lsquo;", "\u{2018}"),
        ("&hellip;", "\u{2026}"),
        ("&mdash;", "\u{2014}"),
    ] {
        out = out.replace(ent, ch);
    }
    let out = HEX_ENT_RE
        .replace_all(&out, |c: &Captures| {
            u32::from_str_radix(&c[1], 16)
                .ok()
                .and_then(char::from_u32)
                .map(|ch| ch.to_string())
                .unwrap_or_else(|| c[0].to_string())
        })
        .into_owned();
    let out = NUM_ENT_RE
        .replace_all(&out, |c: &Captures| {
            c[1]
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .map(|ch| ch.to_string())
                .unwrap_or_else(|| c[0].to_string())
        })
        .into_owned();
    let out = out.replace("&amp;", "&"); // last: see doc comment
    let out = WS_RUN_RE.replace_all(&out, " ");
    let out = NL_RUN_RE.replace_all(&out, "\n\n");
    out.trim().to_string()
}

// ─── Handle derivation ──────────────────────────────────────────────────────

/// Derive `(user, instance)` from an actor URL such as
/// `https://host/users/bob` or `https://host/@bob`.
pub(crate) fn handle_from_url(url: &str) -> Option<(String, String)> {
    let u = url::Url::parse(url).ok()?;
    let host = u.host_str()?.to_lowercase();
    let path = u.path();
    let user = if let Some(rest) = path.strip_prefix("/users/") {
        rest.split('/').next().unwrap_or("")
    } else if let Some(rest) = path.strip_prefix("/@") {
        rest.split('/').next().unwrap_or("")
    } else {
        ""
    };
    if user.is_empty() {
        None
    } else {
        Some((user.to_lowercase(), host))
    }
}

fn str_field(obj: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    obj.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Derive the canonical `@user@instance` handle from an actor reference —
/// either a plain URL string or an actor object (`preferredUsername`/`acct`
/// plus `id`/`url`). Returns `(handle, display_name?, profile_url?)`.
fn actor_to_handle(v: &Value) -> Option<(String, Option<String>, Option<String>)> {
    if let Some(s) = v.as_str() {
        let (user, host) = handle_from_url(s)?;
        return Some((format!("@{}@{}", user, host), Some(user), Some(s.to_string())));
    }
    let obj = v.as_object()?;
    let profile = str_field(obj, "id").or_else(|| str_field(obj, "url"));
    let name = str_field(obj, "name")
        .or_else(|| str_field(obj, "display_name"))
        .filter(|s| !s.is_empty());

    // `acct` may already be fully qualified: "bob@host"
    if let Some(acct) = str_field(obj, "acct") {
        if let Some((user, host)) = acct.split_once('@') {
            if !user.is_empty() && !host.is_empty() {
                return Some((
                    format!("@{}@{}", user.to_lowercase(), host.to_lowercase()),
                    name,
                    profile,
                ));
            }
        }
    }

    let username = str_field(obj, "preferredUsername")
        .or_else(|| str_field(obj, "username"))
        .or_else(|| str_field(obj, "acct"));
    let host = profile
        .as_deref()
        .and_then(|p| url::Url::parse(p).ok())
        .and_then(|u| u.host_str().map(str::to_lowercase));
    if let (Some(user), Some(host)) = (username, host) {
        return Some((format!("@{}@{}", user.to_lowercase(), host), name, profile));
    }

    // Last resort: parse the profile URL path directly.
    if let Some(p) = profile {
        if let Some((user, host)) = handle_from_url(&p) {
            return Some((format!("@{}@{}", user, host), name, Some(p)));
        }
    }
    None
}

// ─── Lossy JSON entry scanning ──────────────────────────────────────────────

enum RawEntry {
    Valid(Value),
    Invalid(String),
}

fn push_candidate(entries: &mut Vec<RawEntry>, candidate: &str) {
    match serde_json::from_str::<Value>(candidate) {
        Ok(v) => entries.push(RawEntry::Valid(v)),
        Err(e) => entries.push(RawEntry::Invalid(format!("invalid JSON: {}", e))),
    }
}

/// Unwrap an ActivityPub collection (`OrderedCollection`/`Collection`) to its
/// inner items. Prefers `orderedItems`, then `items`. Returns `None` for
/// non-collection values so the caller can treat them as a single entry.
fn collection_items(v: &Value) -> Option<Vec<Value>> {
    let obj = v.as_object()?;
    let ty = obj.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let is_collection = matches!(ty, "OrderedCollection" | "Collection" | "OrderedCollectionPage" | "CollectionPage");
    if !is_collection {
        return None;
    }
    if let Some(Value::Array(a)) = obj.get("orderedItems") {
        return Some(a.clone());
    }
    if let Some(Value::Array(a)) = obj.get("items") {
        return Some(a.clone());
    }
    // Empty collection → no items (still a collection, just empty).
    Some(Vec::new())
}

/// Parse a top-level JSON array leniently. Strict parse first; on failure,
/// scan for balanced `{…}` entries so one corrupt entry (R5) does not lose the
/// valid entries around it. Each element becomes `Valid` or `Invalid`.
fn scan_entries(data: &[u8]) -> Vec<RawEntry> {
    if let Ok(v) = serde_json::from_slice::<Value>(data) {
        return match v {
            Value::Array(a) => a.into_iter().map(RawEntry::Valid).collect(),
            // Mastodon/AP outbox & like/bookmark collections are wrapped in an
            // OrderedCollection (or Collection): { type, orderedItems: [...] }.
            // Unwrap to the inner activities, else the whole collection is seen
            // as one non-post entry and silently dropped (0 posts, 0 errors).
            other => match collection_items(&other) {
                Some(items) => items.into_iter().map(RawEntry::Valid).collect(),
                None => vec![RawEntry::Valid(other)],
            },
        };
    }
    let text = String::from_utf8_lossy(data);
    let bytes = text.as_bytes();
    let Some(open) = bytes.iter().position(|&b| b == b'[') else {
        return vec![RawEntry::Invalid("file is not valid JSON".to_string())];
    };

    let mut entries: Vec<RawEntry> = Vec::new();
    let mut in_string = false;
    let mut esc = false;
    let mut level: i32 = 1; // inside the outer array
    let mut obj_start: Option<usize> = None;
    let mut junk_start: Option<usize> = None;
    let mut i = open + 1;

    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' => {
                if level == 1 && obj_start.is_none() && junk_start.is_none() {
                    junk_start = Some(i); // bare string entry
                }
                in_string = true;
            }
            b'{' => {
                if level == 1 && obj_start.is_none() && junk_start.is_none() {
                    obj_start = Some(i);
                }
                level += 1;
            }
            b'[' => {
                if level == 1 && obj_start.is_none() && junk_start.is_none() {
                    junk_start = Some(i);
                }
                level += 1;
            }
            b'}' | b']' => {
                level -= 1;
                if level == 1 {
                    if let Some(s) = obj_start.take() {
                        push_candidate(&mut entries, &text[s..=i]);
                    }
                } else if level == 0 {
                    if let Some(s) = obj_start.take() {
                        push_candidate(&mut entries, &text[s..=i]);
                    } else if let Some(s) = junk_start.take() {
                        push_candidate(&mut entries, &text[s..i]);
                    }
                    break;
                }
            }
            b',' => {
                if level == 1 {
                    if let Some(s) = obj_start.take() {
                        push_candidate(&mut entries, &text[s..i]);
                    } else if let Some(s) = junk_start.take() {
                        push_candidate(&mut entries, &text[s..i]);
                    }
                }
            }
            _ => {
                if level == 1
                    && obj_start.is_none()
                    && junk_start.is_none()
                    && !b.is_ascii_whitespace()
                {
                    junk_start = Some(i);
                }
            }
        }
        i += 1;
    }

    // Truncated tail (unclosed object / trailing junk).
    if let Some(s) = obj_start.take() {
        push_candidate(&mut entries, &text[s..]);
    } else if let Some(s) = junk_start.take() {
        push_candidate(&mut entries, &text[s..]);
    }
    if entries.is_empty() {
        entries.push(RawEntry::Invalid("file is not valid JSON".to_string()));
    }
    entries
}

// ─── Outbox / activity parsing ──────────────────────────────────────────────

/// Result of parsing one activity file (spec R1).
#[derive(Default)]
pub struct MastodonActivityParse {
    pub posts: Vec<ParsedPost>,
    /// Errors for skipped entries (spec R5) — one message per malformed entry.
    pub errors: Vec<String>,
    /// Post-like entries encountered, including malformed ones — this is what
    /// R4 records as `total_posts` (malformed entries count toward the total).
    pub entries_seen: usize,
}

pub(crate) fn ts_of(v: &Value, keys: &[&str]) -> Option<DateTime<Utc>> {
    for k in keys {
        if let Some(s) = v.get(k).and_then(|x| x.as_str())
            && let Ok(dt) = DateTime::parse_from_rfc3339(s)
        {
            return Some(dt.with_timezone(&Utc));
        }
    }
    None
}

/// Attachment URLs from a Note (`attachment` AP form or `media_attachments`
/// REST form). Recorded as references only — never fetched (spec R6).
pub(crate) fn attachment_urls(note: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for key in ["attachment", "media_attachments"] {
        let Some(Value::Array(items)) = note.get(key) else {
            continue;
        };
        for item in items {
            let mut push = |s: &str| {
                if !s.is_empty() && seen.insert(s.to_string()) {
                    out.push(s.to_string());
                }
            };
            match item {
                Value::String(s) => push(s),
                Value::Object(o) => match o.get("url") {
                    Some(Value::String(s)) => push(s),
                    Some(Value::Array(links)) => {
                        for l in links {
                            if let Some(h) = l
                                .get("href")
                                .and_then(|x| x.as_str())
                                .or_else(|| l.get("url").and_then(|x| x.as_str()))
                            {
                                push(h);
                            }
                        }
                    }
                    _ => {
                        if let Some(h) = o.get("href").and_then(|x| x.as_str()) {
                            push(h);
                        }
                    }
                },
                _ => {}
            }
        }
    }
    out
}

/// Build a post from an embedded Note. `ts_sources` are tried in order for
/// `published`/`created_at` (boosts prefer the Announce timestamp).
pub(crate) fn post_from_note(
    note: &Value,
    ts_sources: &[&Value],
    is_boost: bool,
) -> Result<ParsedPost, String> {
    let original_ts = ts_sources
        .iter()
        .find_map(|src| ts_of(src, &["published", "created_at"]))
        .ok_or_else(|| "missing or unparseable timestamp".to_string())?;

    let content = note
        .get("content")
        .and_then(|v| v.as_str())
        .map(html_to_text)
        .unwrap_or_default();
    let media = attachment_urls(note);
    if content.trim().is_empty() && media.is_empty() {
        return Err("entry has no text, media, or link".to_string());
    }

    let nonempty = |key: &str| {
        note.get(key)
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    };
    let in_reply = nonempty("inReplyTo") || nonempty("in_reply_to_id");
    let action = if is_boost {
        "boosted"
    } else if in_reply {
        "replied"
    } else {
        "posted"
    };

    Ok(ParsedPost {
        content_text: content,
        display_name: None,
        action_text: Some(action.to_string()),
        group_name: None,
        link_url: None,
        link_title: None,
        original_ts,
        media_urls: media,
    })
}

/// Post whose content is only a URL in the archive (older exports omit the
/// embedded Note — see spec R1 "text content where available").
fn post_from_url(
    url: &str,
    ts_sources: &[&Value],
    is_boost: bool,
) -> Result<ParsedPost, String> {
    let original_ts = ts_sources
        .iter()
        .find_map(|src| ts_of(src, &["published", "created_at"]))
        .ok_or_else(|| "missing or unparseable timestamp".to_string())?;
    Ok(ParsedPost {
        content_text: String::new(),
        display_name: None,
        action_text: Some(if is_boost { "boosted" } else { "posted" }.to_string()),
        group_name: None,
        link_url: Some(url.to_string()),
        link_title: None,
        original_ts,
        media_urls: Vec::new(),
    })
}

fn record_attempt(
    result: &mut MastodonActivityParse,
    label: &str,
    index: usize,
    attempt: Result<ParsedPost, String>,
) {
    result.entries_seen += 1;
    match attempt {
        Ok(post) => result.posts.push(post),
        Err(e) => result
            .errors
            .push(format!("{} entry {}: {}", label, index, e)),
    }
}

/// Parse one activity file (`outbox.json`, `likes.json`, `bookmarks.json`).
///
/// `allow_bare_notes` should only be true for the owner's own outbox: bare
/// Notes in likes/bookmarks are *other people's* statuses and must not be
/// imported as the archive owner's posts.
pub fn parse_mastodon_activities(
    data: &[u8],
    allow_bare_notes: bool,
    label: &str,
) -> MastodonActivityParse {
    let mut result = MastodonActivityParse::default();
    for (idx, raw) in scan_entries(data).into_iter().enumerate() {
        let n = idx + 1;
        let entry = match raw {
            RawEntry::Invalid(msg) => {
                result.entries_seen += 1;
                result
                    .errors
                    .push(format!("{} entry {}: {}", label, n, msg));
                continue;
            }
            RawEntry::Valid(v) => v,
        };
        let Some(obj) = entry.as_object() else {
            result.entries_seen += 1;
            result
                .errors
                .push(format!("{} entry {}: entry is not a JSON object", label, n));
            continue;
        };
        let ty = obj.get("type").and_then(|t| t.as_str());
        match ty {
            Some("Create") => {
                let activity = &entry;
                let attempt = match obj.get("object") {
                    None => Err("Create activity has no object".to_string()),
                    Some(v) if v.is_object() => post_from_note(v, &[v, activity], false),
                    Some(Value::String(u)) if !u.is_empty() => post_from_url(u, &[activity], false),
                    Some(_) => Err("Create activity object is neither a Note nor a URL".to_string()),
                };
                record_attempt(&mut result, label, n, attempt);
            }
            Some("Announce") => {
                let activity = &entry;
                let attempt = match obj.get("object") {
                    None => Err("Announce activity has no object".to_string()),
                    Some(v) if v.is_object() => post_from_note(v, &[activity, v], true),
                    Some(Value::String(u)) if !u.is_empty() => post_from_url(u, &[activity], true),
                    Some(_) => {
                        Err("Announce activity object is neither a Note nor a URL".to_string())
                    }
                };
                record_attempt(&mut result, label, n, attempt);
            }
            Some("Note") if allow_bare_notes => {
                record_attempt(&mut result, label, n, post_from_note(&entry, &[&entry], false));
            }
            Some(_) => {
                // Like / Follow / Delete / … — not a post; silently ignored.
            }
            None => {
                if obj.contains_key("content") {
                    if allow_bare_notes {
                        record_attempt(
                            &mut result,
                            label,
                            n,
                            post_from_note(&entry, &[&entry], false),
                        );
                    }
                    // Foreign bare status in likes/bookmarks — not ours.
                } else {
                    result.entries_seen += 1;
                    result.errors.push(format!(
                        "{} entry {}: unrecognized entry (missing type)",
                        label, n
                    ));
                }
            }
        }
    }
    result
}

// ─── Contact parsing ────────────────────────────────────────────────────────

/// Build a [`ContactEntry`] from one following/followers entry. Tries the
/// direction-specific field of a `Follow` activity first (`object` for
/// following, `actor` for followers), then the entry itself as an actor.
fn contact_from_entry(entry: &Value, direction: &str) -> Option<ContactEntry> {
    let primary_key = if direction == "following" { "object" } else { "actor" };
    let mut candidates: Vec<&Value> = Vec::new();
    if let Some(p) = entry.get(primary_key) {
        candidates.push(p);
    }
    candidates.push(entry);

    for c in candidates {
        if let Some((handle, name, url)) = actor_to_handle(c) {
            let user = handle
                .trim_start_matches('@')
                .split('@')
                .next()
                .unwrap_or("")
                .to_string();
            return Some(ContactEntry {
                contact_name: name.unwrap_or(user),
                contact_handle: Some(handle.clone()),
                contact_platform_url: url,
                source: "mastodon".to_string(),
                remote_handle: Some(handle),
                direction: Some(direction.to_string()),
            });
        }
    }
    None
}

fn parse_mastodon_contacts(data: &[u8], direction: &str) -> Vec<ContactEntry> {
    scan_entries(data)
        .into_iter()
        .filter_map(|raw| match raw {
            RawEntry::Valid(v) => contact_from_entry(&v, direction),
            RawEntry::Invalid(_) => None, // tolerated: contacts stay best-effort
        })
        .collect()
}

/// Parse `following.json` (direction "following").
pub fn parse_mastodon_following_json(data: &[u8]) -> Vec<ContactEntry> {
    parse_mastodon_contacts(data, "following")
}

/// Parse `followers.json` (direction "follower").
pub fn parse_mastodon_followers_json(data: &[u8]) -> Vec<ContactEntry> {
    parse_mastodon_contacts(data, "follower")
}

// ─── Owner identity (actor.json) ────────────────────────────────────────────

/// The archive owner's own identity, used for `archive_identities` so other
/// users' imports can match against it.
pub struct MastodonOwner {
    pub handle: String,
    pub display_name: Option<String>,
    pub profile_url: Option<String>,
}

/// Parse `actor.json` into the owner's normalized `@user@instance` handle.
pub fn parse_mastodon_actor(data: &[u8]) -> Option<MastodonOwner> {
    let v: Value = serde_json::from_slice(data).ok()?;
    let (handle, _, _) = actor_to_handle(&v)?;
    let obj = v.as_object()?;
    let display_name = str_field(obj, "name")
        .or_else(|| str_field(obj, "display_name"))
        .filter(|s| !s.is_empty());
    let profile_url = str_field(obj, "url").or_else(|| str_field(obj, "id"));
    Some(MastodonOwner {
        handle,
        display_name,
        profile_url,
    })
}

// ─── Whole-archive parsing ──────────────────────────────────────────────────

/// Everything extracted from a Mastodon archive ZIP (spec R1).
#[derive(Default)]
pub struct MastodonArchiveParse {
    pub posts: Vec<ParsedPost>,
    pub contacts: Vec<ContactEntry>,
    pub owner: Option<MastodonOwner>,
    pub errors: Vec<String>,
    /// Post-like activity entries seen across all walked files (spec R4).
    pub entries_seen: usize,
}

/// Walk the archive ZIP: `outbox.json` (+ `likes.json` / `bookmarks.json`,
/// Create/Announce only), `following.json`, `followers.json`, `actor.json`.
pub fn parse_mastodon_archive(temp_path: &str) -> Result<MastodonArchiveParse, AppError> {
    let file = std::fs::File::open(temp_path)
        .map_err(|e| AppError::BadRequest(format!("Failed to open uploaded file: {}", e)))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| AppError::BadRequest(format!("Invalid ZIP: {}", e)))?;

    let mut result = MastodonArchiveParse::default();
    let mut saw_post_file = false;
    let mut wanted: HashSet<String> = [
        "outbox.json",
        "likes.json",
        "bookmarks.json",
        "following.json",
        "followers.json",
        "actor.json",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    for i in 0..archive.len() {
        let mut buf = Vec::new();
        let fname = {
            let mut entry = match archive.by_index(i) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let name = entry.name().to_string();
            let fname = name.rsplit('/').next().unwrap_or("").to_string();
            if !wanted.contains(&fname) {
                continue;
            }
            wanted.remove(&fname);
            if entry.read_to_end(&mut buf).is_err() {
                result
                    .errors
                    .push(format!("{}: failed to read entry", fname));
                continue;
            }
            fname
        };
        if buf.is_empty() {
            continue;
        }
        match fname.as_str() {
            "outbox.json" => {
                saw_post_file = true;
                let parsed = parse_mastodon_activities(&buf, true, "outbox.json");
                result.posts.extend(parsed.posts);
                result.errors.extend(parsed.errors);
                result.entries_seen += parsed.entries_seen;
            }
            "likes.json" | "bookmarks.json" => {
                saw_post_file = true;
                let parsed = parse_mastodon_activities(&buf, false, &fname);
                result.posts.extend(parsed.posts);
                result.errors.extend(parsed.errors);
                result.entries_seen += parsed.entries_seen;
            }
            "following.json" => {
                result.contacts.extend(parse_mastodon_following_json(&buf));
            }
            "followers.json" => {
                result.contacts.extend(parse_mastodon_followers_json(&buf));
            }
            "actor.json" => {
                if result.owner.is_none() {
                    result.owner = parse_mastodon_actor(&buf);
                }
            }
            _ => {}
        }
    }

    if !saw_post_file {
        result
            .errors
            .push("outbox.json not found in archive".to_string());
    }
    // Fill display_name on posts from the actor when available.
    if let Some(name) = result.owner.as_ref().and_then(|o| o.display_name.clone()) {
        for p in &mut result.posts {
            p.display_name = Some(name.clone());
        }
    }
    Ok(result)
}

/// Streaming-endpoint variant: sends SSE events through `tx` while parsing,
/// returning (posts, errors) for the caller to do DB inserts.
pub fn parse_mastodon_archive_sync(
    temp_path: &str,
    tx: &crate::api::import::StdSyncSender,
) -> (Vec<ParsedPost>, Vec<String>) {
    let send = |val: Value| {
        let _ = tx.send(val.to_string());
    };
    send(serde_json::json!({"type":"phase","phase":"scanning_archive","total":0}));
    match parse_mastodon_archive(temp_path) {
        Err(e) => (vec![], vec![e.to_string()]),
        Ok(parsed) => {
            send(serde_json::json!({"type":"phase","phase":"parsing_posts","total":parsed.posts.len()}));
            for (i, p) in parsed.posts.iter().enumerate() {
                send(serde_json::json!({
                    "type":"post",
                    "title": p.content_text.chars().take(100).collect::<String>(),
                    "timestamp": p.original_ts.to_rfc3339(),
                    "media_count": p.media_urls.len(),
                    "index": i + 1,
                }));
            }
            (parsed.posts, parsed.errors)
        }
    }
}

// ─── Contact matching (spec R3) ─────────────────────────────────────────────

/// Normalize a raw handle to canonical `@user@instance` (lowercased).
fn normalize_remote_handle(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_start_matches('@');
    let (user, instance) = trimmed.split_once('@')?;
    if user.is_empty() || instance.is_empty() || instance.contains('@') {
        return None;
    }
    Some(format!(
        "@{}@{}",
        user.trim().to_lowercase(),
        instance.trim().to_lowercase()
    ))
}

/// Link local users to the imported Mastodon contacts of `session_id`
/// (spec R3): exact archive-identity handle match → certain `handle_match`;
/// local username alignment → `handle_match`; identity instance alignment →
/// `instance_match`. Creates rows in both directions like the Facebook /
/// Instagram matcher does.
pub async fn match_mastodon_contacts(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    session_id: Uuid,
) -> Result<usize, sqlx::Error> {
    let rows: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
        r#"
        SELECT remote_handle, contact_handle
        FROM imported_contacts
        WHERE user_id = $1 AND import_session_id = $2
          AND (remote_handle IS NOT NULL OR contact_handle IS NOT NULL)
        "#,
    )
    .bind(user_id)
    .bind(session_id)
    .fetch_all(pool)
    .await?;

    // user id → (priority, match_type, confidence); lower priority wins.
    let mut candidates: HashMap<Uuid, (u8, &'static str, &'static str)> = HashMap::new();
    let mut consider = |id: Uuid, prio: u8, mt: &'static str, conf: &'static str| {
        match candidates.get(&id) {
            Some(existing) if existing.0 <= prio => {}
            _ => {
                candidates.insert(id, (prio, mt, conf));
            }
        }
    };

    for (remote, fallback) in rows {
        let Some(raw) = remote.or(fallback) else {
            continue;
        };
        let Some(norm) = normalize_remote_handle(&raw) else {
            continue;
        };
        let inner = norm.trim_start_matches('@').to_string();
        let Some((user, instance)) = inner.split_once('@') else {
            continue;
        };

        // 1) exact archive-identity handle (certain).
        let exact: Vec<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT DISTINCT u.id
            FROM users u
            JOIN archive_identities ai ON ai.user_id = u.id
            WHERE u.id <> $1 AND u.deleted_at IS NULL
              AND LOWER(LTRIM(ai.handle, '@')) = $2
            "#,
        )
        .bind(user_id)
        .bind(&inner)
        .fetch_all(pool)
        .await?;
        for (id,) in exact {
            consider(id, 1, "handle_match", "certain");
        }

        // 2) local username aligns with the handle's user part (likely).
        let by_username: Vec<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT DISTINCT id FROM users
            WHERE id <> $1 AND deleted_at IS NULL AND LOWER(username) = $2
            "#,
        )
        .bind(user_id)
        .bind(user)
        .fetch_all(pool)
        .await?;
        for (id,) in by_username {
            consider(id, 2, "handle_match", "likely");
        }

        // 2b) the handle IS a Bytescape account's canonical AP actor: the
        // archive stores `@user@host`, and a minted actor is
        // `https://{host}/@{user}` (alias form `…/users/{user}`). Resolved
        // offline via `users.ap_actor_id` — this matcher never touches the
        // network (spec R6: ZIP only), so only locally stored ids are compared.
        for candidate in [
            format!("https://{instance}/@{user}"),
            format!("https://{instance}/users/{user}"),
        ] {
            if let Ok(Some(target)) =
                db::users::UserRow::find_by_ap_actor_id(pool, &candidate).await
            {
                consider(target.id, 1, "handle_match", "certain");
            }
        }

        // 3) some archive identity lives on the same instance (likely).
        let by_instance: Vec<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT DISTINCT u.id
            FROM users u
            JOIN archive_identities ai ON ai.user_id = u.id
            WHERE u.id <> $1 AND u.deleted_at IS NULL
              AND LOWER(ai.handle) LIKE '%@' || LOWER($2)
            "#,
        )
        .bind(user_id)
        .bind(instance)
        .fetch_all(pool)
        .await?;
        for (id,) in by_instance {
            consider(id, 3, "instance_match", "likely");
        }
    }

    let mut matched = 0usize;
    for (id, (_, mt, conf)) in candidates {
        // RowNotFound from ON CONFLICT DO NOTHING means the pair already
        // existed — either way the users are linked, so count it.
        let _ = db::social_graph_matches::SocialGraphMatchRow::create(
            pool, user_id, id, mt, "mastodon", conf,
        )
        .await;
        let _ = db::social_graph_matches::SocialGraphMatchRow::create(
            pool, id, user_id, mt, "mastodon", conf,
        )
        .await;
        matched += 1;
    }
    Ok(matched)
}

// ─── Ingestion (spec R2 / R4) ───────────────────────────────────────────────

/// Public entry point for the unified import — processes a temp ZIP file path.
/// Returns `(imported_posts, session_id, errors)`.
pub async fn import_mastodon_archive_from_path(
    state: &AppState,
    user_id: Uuid,
    temp_path: &str,
) -> Result<(usize, String, Vec<String>), AppError> {
    let session = db::imports::ImportRow::create(&state.db, user_id, "mastodon")
        .await
        .map_err(AppError::Database)?;
    db::imports::ImportRow::start(&state.db, session.id).await.ok();

    // Phase 1: synchronous parse (spec R6 — no network, ZIP only).
    let parsed = match parse_mastodon_archive(temp_path) {
        Ok(p) => p,
        Err(e) => {
            let _ = db::imports::ImportRow::fail(&state.db, session.id, &e.to_string()).await;
            return Err(e);
        }
    };
    let mut errors = parsed.errors;

    // Phase 2: DB inserts.
    let total_media: i32 = parsed.posts.iter().map(|p| p.media_urls.len()).sum::<usize>() as i32;
    let mut imported: i32 = 0;
    let mut imported_media: i32 = 0;
    for post in &parsed.posts {
        let content_obj = serde_json::json!({
            "text": post.content_text,
            "display_name": post.display_name,
            "action_text": post.action_text,
            "group_name": post.group_name,
            "link_url": post.link_url,
            "link_title": post.link_title,
            "media_urls": post.media_urls,
        });
        match db::posts::PostRow::create(
            &state.db,
            user_id,
            "mastodon",
            None,
            None,
            &content_obj,
            "short",
            None,
            None,
            None,
            post.original_ts,
            true,
            Some(session.id),
            "public",
        )
        .await
        {
            Ok(_) => {
                imported += 1;
                imported_media += post.media_urls.len() as i32;
            }
            Err(e) => errors.push(format!("DB insert error: {}", e)),
        }
    }

    // Store the owner identity so other users' imports can match against it.
    if let Some(owner) = &parsed.owner {
        let _ = db::archive_identities::ArchiveIdentityRow::create(
            &state.db,
            &db::archive_identities::NewArchiveIdentity {
                user_id,
                platform: "mastodon".to_string(),
                handle: Some(owner.handle.clone()),
                display_name: owner.display_name.clone(),
                profile_url: owner.profile_url.clone(),
                import_session_id: Some(session.id),
            },
        )
        .await;
    }

    // Store contacts (spec R1/R2), then match them (spec R3) and categorize.
    if !parsed.contacts.is_empty() {
        let new_contacts: Vec<db::imported_contacts::NewContact> = parsed
            .contacts
            .iter()
            .map(|c| db::imported_contacts::NewContact {
                user_id,
                contact_name: c.contact_name.clone(),
                contact_handle: c.contact_handle.clone(),
                contact_platform_url: c.contact_platform_url.clone(),
                source: c.source.clone(),
                remote_handle: c.remote_handle.clone(),
                direction: c.direction.clone(),
                import_session_id: Some(session.id),
            })
            .collect();
        if let Err(e) =
            db::imported_contacts::ImportedContactRow::create_batch(&state.db, &new_contacts).await
        {
            errors.push(format!("contact insert error: {}", e));
        }
        if let Err(e) = match_mastodon_contacts(&state.db, user_id, session.id).await {
            errors.push(format!("contact matching error: {}", e));
        }
        if let Err(e) = db::imported_contacts::ImportedContactRow::categorize_contacts(
            &state.db,
            session.id,
            user_id,
        )
        .await
        {
            eprintln!("[mastodon import] Warning: failed to categorize contacts: {}", e);
        }
    }

    // Phase 3: session completion (spec R4 — completed even with skips).
    let total = parsed.entries_seen as i32;
    if total > 0 {
        if errors.is_empty() {
            db::imports::ImportRow::complete(&state.db, session.id, total, total_media)
                .await
                .ok();
        } else {
            db::imports::ImportRow::complete_with_errors(
                &state.db,
                session.id,
                total,
                imported,
                total_media,
                imported_media,
                &errors,
            )
            .await
            .ok();
        }
    } else {
        let msg = if errors.is_empty() {
            "No posts found in archive.".to_string()
        } else {
            format!("No posts found in archive. Errors: {}", errors.join("; "))
        };
        db::imports::ImportRow::fail(&state.db, session.id, &msg).await.ok();
    }

    Ok((imported as usize, session.id.to_string(), errors))
}

// ─── Direct endpoint ────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/import/mastodon", post(import_mastodon_archive))
        .layer(DefaultBodyLimit::disable())
}

fn extract_user_id(headers: &HeaderMap, secret: &str) -> Result<Uuid, AppError> {
    let auth = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Auth("Missing or invalid Authorization header".into()))?;
    let claims = auth::validate_token(auth, secret)?;
    Uuid::parse_str(&claims.sub).map_err(|_| AppError::Auth("Invalid user ID in token".into()))
}

#[derive(Serialize)]
struct ImportResponse {
    imported: usize,
    session_id: String,
    errors: Vec<String>,
}

async fn import_mastodon_archive(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<ImportResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let temp_path = format!("/tmp/bytescape_mastodon_{}.zip", Uuid::new_v4());
    let mut temp_file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create temp file: {}", e)))?;

    let mut found_field = false;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Multipart error: {}", e)))?
    {
        if field.name() == Some("file") {
            found_field = true;
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|e| AppError::BadRequest(format!("Stream error: {}", e)))?
            {
                temp_file
                    .write_all(&chunk)
                    .await
                    .map_err(|e| AppError::Internal(format!("Write error: {}", e)))?;
            }
            break;
        }
    }

    if !found_field {
        tokio::fs::remove_file(&temp_path).await.ok();
        return Err(AppError::BadRequest("No file provided".into()));
    }
    temp_file
        .sync_all()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to flush temp file: {}", e)))?;

    let result = import_mastodon_archive_from_path(&state, user_id, &temp_path).await;
    tokio::fs::remove_file(&temp_path).await.ok();

    let (imported, session_id, errors) = result?;
    Ok(Json(ImportResponse {
        imported,
        session_id,
        errors,
    }))
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::feed::FeedResponse;
    use crate::api::import_instagram::fixtures::{build_zip, FixtureZip};

    /// Instance reserved for fixtures (`.test` is RFC 2606 reserved, so no
    /// real accounts can collide with these handles).
    const INST: &str = "mstdn-fixture.test";

    /// R1 fixture: 3 outbox activities (post / reply / boost) + 2 following +
    /// 1 follower contacts. With `with_malformed`, the reply is replaced by a
    /// structurally broken Create entry (R4/R5: 3 entries, 1 malformed).
    fn mastodon_fixture(match_user: &str, with_malformed: bool) -> FixtureZip {
        let alice = "https://mastodon.example/users/alice";

        let create_post = serde_json::json!({
            "type": "Create",
            "actor": alice,
            "published": "2025-01-10T12:00:00.000Z",
            "object": {
                "id": "https://mastodon.example/users/alice/statuses/1",
                "type": "Note",
                "content": "<p>Hello world</p>",
                "published": "2025-01-10T12:00:00.000Z",
                "inReplyTo": Value::Null,
                "attachment": [{
                    "type": "Image",
                    "url": "https://files.mastodon.example/pic.png",
                }],
            },
        });
        let reply = serde_json::json!({
            "type": "Create",
            "actor": alice,
            "published": "2025-01-11T09:30:00.000Z",
            "object": {
                "id": "https://mastodon.example/users/alice/statuses/2",
                "type": "Note",
                "content": "<p>@ghost_dave_zx9 nice thread</p>",
                "published": "2025-01-11T09:30:00.000Z",
                "inReplyTo": format!("https://{}/users/ghost_dave_zx9/statuses/9", INST),
                "attachment": [],
            },
        });
        let malformed = serde_json::json!({ "type": "Create" }); // R5: no object
        let announce = serde_json::json!({
            "type": "Announce",
            "actor": alice,
            "published": "2025-01-12T18:45:00.000Z",
            "object": {
                "id": "https://mastodon.example/users/alice/statuses/3/activity",
                "type": "Note",
                "content": "<p>Boosted original text</p>",
                "published": "2025-01-09T07:00:00.000Z",
                "attachment": [],
            },
        });

        let outbox = if with_malformed {
            serde_json::json!([create_post, malformed, announce])
        } else {
            serde_json::json!([create_post, reply, announce])
        };

        // One Follow activity (object = target URL) and one bare actor object —
        // both shapes appear in real exports.
        let following = serde_json::json!([
            {
                "type": "Follow",
                "actor": alice,
                "object": format!("https://{}/users/{}", INST, match_user),
                "published": "2025-01-05T00:00:00.000Z",
            },
            {
                "id": "https://nowhere-fixture.test/users/ghost_nobody_zx9",
                "preferredUsername": "ghost_nobody_zx9",
                "name": "Ghost Nobody",
                "url": "https://nowhere-fixture.test/@ghost_nobody_zx9",
            },
        ]);
        let followers = serde_json::json!([{
            "type": "Follow",
            "actor": format!("https://{}/users/ghost_dave_zx9", INST),
            "object": alice,
        }]);
        let actor = serde_json::json!({
            "id": "https://mastodon.example/users/alice",
            "type": "Person",
            "preferredUsername": "alice",
            "name": "Alice Fixture",
            "url": "https://mastodon.example/@alice",
        });

        build_zip(
            "mastodon",
            vec![
                ("outbox.json".to_string(), outbox.to_string().into_bytes()),
                ("following.json".to_string(), following.to_string().into_bytes()),
                ("followers.json".to_string(), followers.to_string().into_bytes()),
                ("actor.json".to_string(), actor.to_string().into_bytes()),
            ],
        )
    }

    fn ts(s: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(s).unwrap().timestamp()
    }

    /// Spec R1: 3 posts (posted / replied / boosted), known timestamps, one
    /// media reference; 3 contacts (2 following, 1 follower) with
    /// `remote_handle` populated.
    #[test]
    fn mastodon_fixture_parses_posts_replies_boosts_and_contacts() {
        let fixture = mastodon_fixture("alice_local", false);
        let parsed =
            parse_mastodon_archive(fixture.path().to_str().unwrap()).expect("parse fixture");

        assert!(parsed.errors.is_empty(), "errors: {:?}", parsed.errors);
        assert_eq!(parsed.entries_seen, 3, "3 post-like activities");
        assert_eq!(parsed.posts.len(), 3, "all 3 activities become posts");

        let actions: Vec<&str> = parsed
            .posts
            .iter()
            .map(|p| p.action_text.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(actions, vec!["posted", "replied", "boosted"]);

        assert_eq!(parsed.posts[0].content_text, "Hello world");
        assert_eq!(parsed.posts[0].original_ts.timestamp(), ts("2025-01-10T12:00:00Z"));
        assert_eq!(
            parsed.posts[0].media_urls,
            vec!["https://files.mastodon.example/pic.png"],
            "attachment URL recorded as a reference (R6: not fetched)"
        );

        assert_eq!(parsed.posts[1].original_ts.timestamp(), ts("2025-01-11T09:30:00Z"));
        // Boost timestamp is the Announce time, not the original post time.
        assert_eq!(parsed.posts[2].original_ts.timestamp(), ts("2025-01-12T18:45:00Z"));
        assert_eq!(parsed.posts[2].content_text, "Boosted original text");

        // Owner identity from actor.json.
        let owner = parsed.owner.as_ref().expect("actor.json parsed");
        assert_eq!(owner.handle, "@alice@mastodon.example");
        assert_eq!(owner.display_name.as_deref(), Some("Alice Fixture"));

        // Contacts: 2 following + 1 follower, remote_handle populated (R1).
        assert_eq!(parsed.contacts.len(), 3, "contacts: {:?}", parsed.contacts);
        let directions: Vec<&str> = parsed
            .contacts
            .iter()
            .map(|c| c.direction.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(directions, vec!["following", "following", "follower"]);

        let handles: HashSet<&str> = parsed
            .contacts
            .iter()
            .map(|c| c.remote_handle.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(
            handles,
            [
                format!("@alice_local@{}", INST),
                "@ghost_nobody_zx9@nowhere-fixture.test".to_string(),
                format!("@ghost_dave_zx9@{}", INST),
            ]
            .iter()
            .map(|s| s.as_str())
            .collect::<HashSet<&str>>(),
            "remote_handle carries @user@instance: {:?}",
            parsed.contacts
        );
        // contact_handle mirrors remote_handle so RSS derivation (R2) and the
        // unmatched-contact list (R3) work exactly like other sources.
        for c in &parsed.contacts {
            assert_eq!(c.contact_handle, c.remote_handle);
            assert_eq!(c.source, "mastodon");
        }
        // Bare actor object keeps its display name.
        let nobody = parsed
            .contacts
            .iter()
            .find(|c| c.remote_handle.as_deref() == Some("@ghost_nobody_zx9@nowhere-fixture.test"))
            .expect("bare actor contact");
        assert_eq!(nobody.contact_name, "Ghost Nobody");
    }

    /// Spec R5: one entry with invalid JSON must not abort the import — the
    /// valid entries around it are recovered and the bad one is reported.
    #[test]
    fn invalid_json_entry_is_skipped_and_neighbors_recovered() {
        let raw = format!(
            r#"[{{"type":"Create","actor":"https://mastodon.example/users/alice","published":"2025-01-10T12:00:00.000Z","object":{{"type":"Note","content":"<p>First post</p>","published":"2025-01-10T12:00:00.000Z"}}}}, {{"type": "Create", "object": {{"content": }}}}, {{"type":"Create","actor":"https://mastodon.example/users/alice","published":"2025-01-11T12:00:00.000Z","object":{{"type":"Note","content":"<p>Third post</p>","published":"2025-01-11T12:00:00.000Z"}}}}]"#
        );
        let parsed = parse_mastodon_activities(raw.as_bytes(), true, "outbox.json");

        assert_eq!(
            parsed.posts.len(),
            2,
            "entries around the corrupt one survive: {:?}",
            parsed.errors
        );
        assert_eq!(parsed.entries_seen, 3, "malformed entry counts toward total (R4)");
        assert_eq!(parsed.errors.len(), 1, "one skipped-entry error: {:?}", parsed.errors);
        assert!(parsed.errors[0].contains("outbox.json entry 2"));
        assert_eq!(parsed.posts[0].content_text, "First post");
        assert_eq!(parsed.posts[1].content_text, "Third post");
    }

    /// Spec R4/R5 structural case: a Create activity missing its object is
    /// skipped, counted, and reported.
    #[test]
    fn malformed_create_entry_is_skipped_and_counted() {
        let raw = serde_json::json!([
            {
                "type": "Create",
                "published": "2025-01-10T12:00:00.000Z",
                "object": { "type": "Note", "content": "<p>ok</p>", "published": "2025-01-10T12:00:00.000Z" }
            },
            { "type": "Create" }
        ]);
        let parsed = parse_mastodon_activities(raw.to_string().as_bytes(), true, "outbox.json");
        assert_eq!(parsed.posts.len(), 1);
        assert_eq!(parsed.entries_seen, 2);
        assert_eq!(parsed.errors.len(), 1);
        assert!(parsed.errors[0].contains("no object"));
    }

    /// Likes/bookmarks contain *other people's* statuses — only Create /
    /// Announce activities may be extracted there, bare Notes must not be
    /// attributed to the archive owner.
    #[test]
    fn likes_file_never_imports_foreign_bare_notes() {
        let raw = serde_json::json!([
            {
                "type": "Like",
                "actor": "https://mastodon.example/users/alice",
                "object": { "type": "Note", "content": "<p>Someone else's post</p>", "published": "2025-01-10T12:00:00.000Z" }
            },
            { "type": "Note", "content": "<p>Foreign bare status</p>", "published": "2025-01-10T12:00:00.000Z" }
        ]);
        let parsed = parse_mastodon_activities(raw.to_string().as_bytes(), false, "likes.json");
        assert_eq!(parsed.posts.len(), 0, "no foreign posts imported");
        assert_eq!(parsed.entries_seen, 0, "likes are not post attempts");
        assert!(parsed.errors.is_empty());
    }

    // R: a real Mastodon outbox.json is an OrderedCollection
    // { type, orderedItems: [...] }, not a bare array. The parser must unwrap it,
    // else the whole collection is treated as one non-post entry and dropped
    // (0 posts, 0 errors — the "Import failed" symptom on real archives).
    #[test]
    fn ordered_collection_outbox_is_unwrapped_to_posts() {
        let inner = serde_json::json!([
            {
                "type": "Create",
                "object": { "type": "Note", "content": "<p>Hello</p>", "published": "2025-01-10T12:00:00.000Z" }
            },
            {
                "type": "Announce",
                "object": { "type": "Note", "content": "<p>Boosted</p>", "published": "2025-01-11T09:30:00.000Z" }
            }
        ]);
        let wrapped = serde_json::json!({
            "type": "OrderedCollection",
            "id": "https://mastodon.example/users/alice/outbox",
            "totalItems": 2,
            "orderedItems": inner,
        });
        let parsed = parse_mastodon_activities(wrapped.to_string().as_bytes(), true, "outbox.json");
        assert_eq!(parsed.posts.len(), 2, "OrderedCollection unwrapped: {:?}", parsed.errors);
        assert!(parsed.errors.is_empty(), "errors: {:?}", parsed.errors);

        // `items` (plain Collection) is also unwrapped.
        let coll = serde_json::json!({
            "type": "Collection",
            "items": [ { "type": "Note", "content": "<p>Bare</p>", "published": "2025-01-12T12:00:00.000Z" } ],
        });
        let parsed2 = parse_mastodon_activities(coll.to_string().as_bytes(), true, "outbox.json");
        assert_eq!(parsed2.posts.len(), 1, "Collection unwrapped");
    }

    fn unique_suffix() -> String {
        Uuid::new_v4().simple().to_string()[..12].to_string()
    }

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

    async fn cleanup_user(pool: &sqlx::PgPool, user_id: Uuid) {
        let _ = sqlx::query("DELETE FROM social_graph_matches WHERE user_a_id = $1 OR user_b_id = $1")
            .bind(user_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM imported_contacts WHERE user_id = $1")
            .bind(user_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM archive_identities WHERE user_id = $1")
            .bind(user_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM posts WHERE user_id = $1")
            .bind(user_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM import_sessions WHERE user_id = $1")
            .bind(user_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(pool)
            .await;
    }

    /// End-to-end (spec R2 + R4 clean path + R3 matching + wall render):
    /// posts land in the shared table with `action_text`, the session completes
    /// with matching counts, contacts carry `remote_handle`/`direction`, the
    /// local user matching a contact gets a `social_graph_matches` row, and the
    /// profile wall renders all three posts.
    #[tokio::test]
    async fn mastodon_import_ingests_session_contacts_and_matching() {
        let Some(pool) = connect_pool().await else { return };
        let Ok(config) = crate::config::AppConfig::from_env() else {
            eprintln!("config incomplete — skipping mastodon DB test");
            return;
        };
        let state = AppState {
            config: std::sync::Arc::new(config),
            db: pool.clone(),
            og_cache: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
        };

        let imp_suffix = unique_suffix();
        let username = format!("mstdnimp_{}", imp_suffix);
        let match_user = format!("mstdn_{}", unique_suffix());
        let importer = db::users::UserRow::create_active(
            &pool,
            &username,
            &format!("{}@test.local", username),
            "x",
            "x",
        )
        .await
        .expect("create importer");
        let matched = db::users::UserRow::create_active(
            &pool,
            &match_user,
            &format!("{}@test.local", match_user),
            "x",
            "x",
        )
        .await
        .expect("create match user");

        let fixture = mastodon_fixture(&match_user, false);
        let (imported, session_id, errors) = import_mastodon_archive_from_path(
            &state,
            importer.id,
            fixture.path().to_str().unwrap(),
        )
        .await
        .expect("import should succeed");
        assert!(errors.is_empty(), "import errors: {errors:?}");
        assert_eq!(imported, 3, "all fixture posts imported");

        // R4: session records accurate counts and completes.
        let session = db::imports::ImportRow::find_by_id(&pool, session_id.parse().unwrap())
            .await
            .expect("session query")
            .expect("session row exists");
        assert_eq!(session.source, "mastodon");
        assert_eq!(session.status, "completed");
        assert_eq!(session.total_posts, 3);
        assert_eq!(session.imported_posts, 3);
        assert_eq!(session.total_media, 1);
        assert!(session.completed_at.is_some());

        // R2: posts in the shared table with source mastodon + action_text.
        let rows: Vec<(String, String, Option<Uuid>)> = sqlx::query_as(
            r#"SELECT source, content->>'action_text', import_session_id FROM posts
               WHERE user_id = $1 AND is_imported = true"#,
        )
        .bind(importer.id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 3);
        let mut actions: Vec<&str> = rows.iter().map(|r| r.1.as_str()).collect();
        actions.sort();
        assert_eq!(actions, ["boosted", "posted", "replied"]);
        for (source, _, sid) in &rows {
            assert_eq!(source, "mastodon");
            assert_eq!(sid.unwrap().to_string(), session_id);
        }

        // R1/R2: contacts stored with remote_handle + direction.
        let stored: Vec<(String, String)> = sqlx::query_as(
            r#"SELECT remote_handle::text, direction::text FROM imported_contacts
               WHERE user_id = $1 AND import_session_id = $2"#,
        )
        .bind(importer.id)
        .bind(session.id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(stored.len(), 3, "contacts stored: {stored:?}");
        let handles: HashSet<String> = stored.iter().map(|(h, _)| h.clone()).collect();
        assert!(handles.contains(&format!("@{}@{}", match_user, INST)));
        assert!(handles.contains("@ghost_nobody_zx9@nowhere-fixture.test"));
        assert!(handles.contains(&format!("@ghost_dave_zx9@{}", INST)));
        let following = stored.iter().filter(|(_, d)| d == "following").count();
        let follower = stored.iter().filter(|(_, d)| d == "follower").count();
        assert_eq!((following, follower), (2, 1));

        // R3: exactly the username-aligned local user is matched; the
        // unmatched contact has no match row and keeps its handle (so it
        // surfaces in the "may be on the network" list).
        let others: Vec<Uuid> = sqlx::query_scalar(
            r#"SELECT DISTINCT CASE WHEN user_a_id = $1 THEN user_b_id ELSE user_a_id END
               FROM social_graph_matches
               WHERE (user_a_id = $1 OR user_b_id = $1) AND source = 'mastodon'"#,
        )
        .bind(importer.id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(others, vec![matched.id], "only the aligned user matches");

        // Wall render: newest-first with original timestamps. Generic stored
        // actions are rendered as the archive-origin label on walls (see the
        // Instagram wall test for that pinned display behavior).
        let app = axum::Router::new()
            .merge(crate::api::profile::routes())
            .with_state(state.clone());
        let request = axum::http::Request::builder()
            .uri(format!("/api/profile/{}/feed", username))
            .body(axum::body::Body::empty())
            .unwrap();
        let response = tower::ServiceExt::oneshot(app, request)
            .await
            .expect("feed request");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let feed: FeedResponse = serde_json::from_slice(&body).expect("feed JSON");
        assert_eq!(feed.items.len(), 3, "wall shows all imported posts");
        let expected: Vec<String> = ["2025-01-12T18:45:00Z", "2025-01-11T09:30:00Z", "2025-01-10T12:00:00Z"]
            .iter()
            .map(|s| chrono::DateTime::parse_from_rfc3339(s)
                .unwrap()
                .with_timezone(&Utc)
                .to_rfc3339())
            .collect();
        for (i, item) in feed.items.iter().enumerate() {
            assert_eq!(item.created_at, expected[i], "item {i} timestamp");
            assert_eq!(item.source, "mastodon");
        }
        let media = &feed.items[2].media;
        assert_eq!(media.len(), 1, "attachment reference renders as media");
        assert_eq!(media[0].url, "https://files.mastodon.example/pic.png");

        cleanup_user(&pool, importer.id).await;
        cleanup_user(&pool, matched.id).await;
    }

    /// Spec R4/R5: 3 outbox entries, 1 malformed → session completes with
    /// total_posts = 3, imported_posts = 2, and the skipped entry recorded in
    /// session metadata.
    #[tokio::test]
    async fn mastodon_partial_import_completes_with_skipped_in_metadata() {
        let Some(pool) = connect_pool().await else { return };
        let Ok(config) = crate::config::AppConfig::from_env() else {
            eprintln!("config incomplete — skipping mastodon DB test");
            return;
        };
        let state = AppState {
            config: std::sync::Arc::new(config),
            db: pool.clone(),
            og_cache: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
        };

        let username = format!("mstdnfail_{}", unique_suffix());
        let importer = db::users::UserRow::create_active(
            &pool,
            &username,
            &format!("{}@test.local", username),
            "x",
            "x",
        )
        .await
        .expect("create importer");

        let fixture = mastodon_fixture("unused_local", true);
        let (imported, session_id, errors) = import_mastodon_archive_from_path(
            &state,
            importer.id,
            fixture.path().to_str().unwrap(),
        )
        .await
        .expect("import should succeed");
        assert_eq!(imported, 2, "malformed entry skipped: {errors:?}");
        assert_eq!(errors.len(), 1, "one skip error: {errors:?}");
        assert!(errors[0].contains("outbox.json entry 2"));

        let session = db::imports::ImportRow::find_by_id(&pool, session_id.parse().unwrap())
            .await
            .expect("session query")
            .expect("session row exists");
        assert_eq!(session.source, "mastodon");
        assert_eq!(session.status, "completed", "does not stay in started/failed");
        assert_eq!(session.total_posts, 3, "malformed entry counts toward total");
        assert_eq!(session.imported_posts, 2);
        assert!(session.error_message.is_some(), "error carried on the session");

        let metadata: serde_json::Value = sqlx::query_scalar(
            "SELECT metadata FROM import_sessions WHERE id = $1",
        )
        .bind(session.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let skipped = metadata
            .get("skipped")
            .and_then(|v| v.as_array())
            .expect("metadata.skipped array");
        assert_eq!(skipped.len(), 1, "skipped entry recorded: {metadata}");
        assert_eq!(
            metadata.get("error_count").and_then(|v| v.as_u64()),
            Some(1)
        );

        cleanup_user(&pool, importer.id).await;
    }
}
