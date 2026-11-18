//! Calendar records → the derived `calendar_events` / `calendar_rsvps` index.
//!
//! Shared by three callers (design decision 2): the relay firehose arm
//! (`appview/consumer.rs`), the local PDS firehose arm
//! (`pds/firehose_consumer.rs`), and the API write-through/delete paths.
//! Parse is JSON-native: the local consumer decodes records to
//! `serde_json::Value`, and the relay arm bridges its CBOR record through
//! [`cbor_to_json`] (each consumer owns its CAR walker; parse + SQL live here).
//!
//! Spec: `events/records` (schemas, dual write path, legacy read tolerance)
//! and `events/ingestion` (idempotent at-URI upsert, flat columns, no
//! phantom rows, no chat/social-graph namespaces).

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;

/// Canonical community-governed event NSID — the only event NSID we write.
pub const EVENT_NSID: &str = "community.lexicon.calendar.event";
/// Legacy Smoke Signal event NSIDs — read tolerance only, never written.
pub const LEGACY_EVENT_NSIDS: &[&str] =
    &["events.smokesignal.Event", "events.smokesignal.calendar.event"];
/// Canonical RSVP NSID — the only RSVP NSID we write.
pub const RSVP_NSID: &str = "community.lexicon.calendar.rsvp";

/// Every NSID whose records land in `calendar_events`.
pub const RECORD_NSIDS: &[&str] = &[EVENT_NSID, "events.smokesignal.Event", "events.smokesignal.calendar.event"];

/// Path prefixes (collection + "/") that carry event records on the firehose.
pub const EVENT_PATH_PREFIXES: &[&str] = &[
    "community.lexicon.calendar.event/",
    "events.smokesignal.Event/",
    "events.smokesignal.calendar.event/",
];
/// Path prefix carrying RSVP records.
pub const RSVP_PATH_PREFIX: &str = "community.lexicon.calendar.rsvp/";

/// Canonical status token for a scheduled event (lexicon default).
pub const EVENT_STATUS_SCHEDULED: &str = "community.lexicon.calendar.event#scheduled";
/// Status token for a cancelled event (Mobilizon `ical:status: CANCELLED`).
pub const EVENT_STATUS_CANCELLED: &str = "community.lexicon.calendar.event#cancelled";

const DEFAULT_EVENT_STATUS: &str = EVENT_STATUS_SCHEDULED;
const RSVP_GOING: &str = "community.lexicon.calendar.rsvp#going";
const KNOWN_RSVP_STATUSES: &[&str] = &[
    RSVP_GOING,
    "community.lexicon.calendar.rsvp#interested",
    "community.lexicon.calendar.rsvp#notgoing",
];

/// True when a firehose op path holds an event record (canonical or legacy).
pub fn is_event_path(path: &str) -> bool {
    EVENT_PATH_PREFIXES.iter().any(|p| path.starts_with(p))
}

/// True when a firehose op path holds an RSVP record.
pub fn is_rsvp_path(path: &str) -> bool {
    path.starts_with(RSVP_PATH_PREFIX)
}

// ─── Parse ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedEvent {
    pub name: String,
    pub description: Option<String>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub mode: Option<String>,
    pub status: String,
    pub location_text: Option<String>,
    pub locations: Value,
    pub uris: Value,
    pub record_created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedRsvp {
    pub subject_uri: String,
    pub status: String,
    pub record_created_at: Option<DateTime<Utc>>,
}

/// Postgres UTF-8 rejects NUL bytes — network records carry them sometimes.
fn clean(s: &str) -> String {
    s.replace('\u{0}', "")
}

fn parse_rfc3339(s: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|dt| dt.with_timezone(&Utc))
}

/// First non-null value among alias keys (legacy field-name tolerance).
fn get_alias<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|k| v.get(*k).filter(|x| !x.is_null()))
}

/// Optional timestamp: absent → None; present-but-invalid → reject the record.
fn opt_time(v: &Value, keys: &[&str]) -> Result<Option<DateTime<Utc>>, ()> {
    match get_alias(v, keys) {
        None => Ok(None),
        Some(Value::String(s)) => parse_rfc3339(s).map(Some).ok_or(()),
        Some(_) => Err(()),
    }
}

/// Best-effort searchable text from the locations union: venue/street/city
/// style strings, never URLs. Payload stays in `locations` untouched.
///
/// `pub(crate)` because the AP event mapper ([`crate::ap::ingest`]) builds the
/// same `locations` payload from a Mobilizon `location` and must derive its
/// `location_text` identically.
pub(crate) fn location_text(locations: &Value) -> Option<String> {
    let arr = locations.as_array()?;
    let mut parts: Vec<String> = Vec::new();
    for item in arr {
        let Some(obj) = item.as_object() else { continue };
        // name/street/locality/region/postalCode = community.lexicon.location.address
        // fields; description covers looser foreign shapes; "uri" deliberately
        // excluded — a map link is not location text.
        for key in ["name", "street", "locality", "region", "postalCode", "description"] {
            if let Some(s) = obj.get(key).and_then(Value::as_str) {
                let s = clean(s);
                if !s.is_empty() && !parts.contains(&s) {
                    parts.push(s);
                }
            }
        }
    }
    if parts.is_empty() { None } else { Some(parts.join(", ")) }
}

/// Parse an event record. `nsid` is the collection from the firehose path —
/// authoritative for strictness: canonical records require `createdAt`
/// (spec: invalid/missing required fields are rejected); legacy Smoke Signal
/// records are tolerant (no `createdAt`, schema.org-style time aliases).
pub fn parse_event(record: &Value, nsid: &str) -> Option<ParsedEvent> {
    let canonical = nsid == EVENT_NSID;

    let name = record
        .get("name")
        .and_then(Value::as_str)
        .map(|s| clean(s).trim().to_string())
        .filter(|s| !s.is_empty())?;

    let record_created_at = match get_alias(record, &["createdAt", "created_at"]) {
        None if canonical => return None, // canonical requires createdAt (spec)
        None => None,                     // legacy may omit it
        Some(Value::String(s)) => Some(parse_rfc3339(s)?),
        Some(_) => return None,
    };

    let starts_at = opt_time(record, &["startsAt", "startDate", "startTime"]).ok()?;
    let ends_at = opt_time(record, &["endsAt", "endDate", "endTime"]).ok()?;

    let description = record
        .get("description")
        .and_then(Value::as_str)
        .map(clean)
        .filter(|s| !s.is_empty());
    let mode = record
        .get("mode")
        .and_then(Value::as_str)
        .map(clean)
        .filter(|s| !s.is_empty());
    let status = record
        .get("status")
        .and_then(Value::as_str)
        .map(clean)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_EVENT_STATUS.to_string());

    let locations = record
        .get("locations")
        .filter(|v| v.is_array())
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let uris = record
        .get("uris")
        .filter(|v| v.is_array())
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));

    let location_text = location_text(&locations);

    Some(ParsedEvent {
        name,
        description,
        starts_at,
        ends_at,
        mode,
        status,
        location_text,
        locations,
        uris,
        record_created_at,
    })
}

/// Parse an RSVP record: required `subject` strongRef (an at-URI) and
/// `status` (known token; absent → lexicon default `#going`; unknown token →
/// reject so foreign vocabulary junk never lands).
pub fn parse_rsvp(record: &Value) -> Option<ParsedRsvp> {
    let subject_uri = record.get("subject")?.get("uri")?.as_str()?.to_string();
    if !subject_uri.starts_with("at://") {
        return None;
    }
    let status = match record.get("status").and_then(Value::as_str) {
        Some(s) => {
            if KNOWN_RSVP_STATUSES.contains(&s) {
                s.to_string()
            } else {
                return None;
            }
        }
        None => RSVP_GOING.to_string(),
    };
    // The rsvp lexicon defines no createdAt; tolerate one if a producer adds it.
    let record_created_at = match record.get("createdAt") {
        Some(Value::String(s)) => Some(parse_rfc3339(s)?),
        _ => None,
    };
    Some(ParsedRsvp {
        subject_uri,
        status,
        record_created_at,
    })
}

// ─── CBOR → JSON bridge (relay path) ──────────────────────────────────

/// Total conversion of a DAG-CBOR record to JSON. Calendar records carry
/// only strings/arrays/maps/bools; bytes render as hex and tags unwrap, so
/// conversion never fails and never panics.
pub fn cbor_to_json(v: &serde_cbor::Value) -> Value {
    use serde_cbor::Value as C;
    match v {
        C::Null => Value::Null,
        C::Bool(b) => Value::Bool(*b),
        C::Integer(i) => i64::try_from(*i)
            .map(Value::from)
            .unwrap_or_else(|_| Value::String(i.to_string())),
        C::Float(f) => serde_json::Number::from_f64(*f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        C::Text(s) => Value::String(s.clone()),
        C::Bytes(b) => Value::String(hex::encode(b)),
        C::Array(a) => Value::Array(a.iter().map(cbor_to_json).collect()),
        C::Map(m) => Value::Object(
            m.iter()
                .filter_map(|(k, val)| match k {
                    C::Text(s) => Some((s.clone(), cbor_to_json(val))),
                    _ => None,
                })
                .collect(),
        ),
        C::Tag(_, inner) => cbor_to_json(inner),
        // Simple values (and any future variants) carry no record meaning.
        _ => Value::Null,
    }
}

// ─── Interest matching ─────────────────────────────────────────────────

/// Upper bound on an event's derived topics. Breadth is bounded so one long
/// description cannot join half the interest catalog; field priority decides
/// which claims survive.
const MAX_DERIVED_TOPICS: usize = 4;

/// Topic rkeys mentioned by an event's searchable text. Derived data — the
/// column is repaired by [`backfill_topics`], never authored.
///
/// Matching follows the canonical topic rule the main feed's tag index uses
/// (migrations 064/068): vocabulary **tags** match only as `#tag` hashtag
/// tokens (tag intent), **keywords** as whole words (prose). Matching tags as
/// bare words bled unrelated prose into feeds — "creative environment" claimed
/// `climate`, "art therapy" claimed `mental-health`. Both surfaces share
/// `crate::tags` so filtering cannot drift.
///
/// Only the event's name and description are searchable. `location_text` is
/// deliberately excluded: venue and institution names are geography, not
/// subject matter, and including them made "St. Joseph's Arts And Science
/// College" claim `science` for a basketball clinic (70 rows carried a topic
/// that came from the venue alone). The main feed has no location field — this
/// is what keeps the two surfaces equivalent.
///
/// At most [`MAX_DERIVED_TOPICS`] topics are kept, highest field priority
/// first (name > description hashtag > description keyword). Long marketing
/// descriptions otherwise claim a dozen topics from incidental nouns
/// ("running it in production", "policy execution path").
pub fn derive_topics(name: &str, description: Option<&str>) -> Vec<String> {
    let name_hay = format!(" {} ", name);
    let desc_hay = format!(" {} ", description.unwrap_or_default());
    // Field priority: a claim from the event's own name outranks a hashtag in
    // the description, which outranks a plain keyword buried in prose. Ties
    // keep the catalog's order, so the derived list is deterministic.
    let mut hits: Vec<(u8, usize, String)> = Vec::new();
    for (idx, topic) in crate::api::feedgen::TOPICS.iter().enumerate() {
        let in_name = topic
            .tags
            .iter()
            .any(|t| crate::tags::text_has_hashtag(&name_hay, t))
            || topic
                .keywords
                .iter()
                .any(|k| crate::tags::text_has_word(&name_hay, k));
        let has_tag = topic
            .tags
            .iter()
            .any(|t| crate::tags::text_has_hashtag(&desc_hay, t));
        let has_keyword = topic
            .keywords
            .iter()
            .any(|k| crate::tags::text_has_word(&desc_hay, k));
        let rank = if in_name {
            0
        } else if has_tag {
            1
        } else if has_keyword {
            2
        } else {
            continue;
        };
        hits.push((rank, idx, topic.rkey.to_string()));
    }
    hits.sort_by_key(|(rank, idx, _)| (*rank, *idx));
    hits.truncate(MAX_DERIVED_TOPICS);
    hits.into_iter().map(|(_, _, rkey)| rkey).collect()
}

#[cfg(test)]
mod topic_tests {
    use super::{derive_topics, MAX_DERIVED_TOPICS};

    /// Tags match only as `#tag` tokens — prose mentioning the tag word must
    /// not claim the topic (the Climate-feed bleed: "creative environment" is
    /// not a climate event).
    #[test]
    fn tags_match_only_as_hashtags() {
        assert!(!derive_topics("Resin Koi Pond Workshop", Some("a creative environment"))
            .contains(&"climate".to_string()));
        assert!(derive_topics("Resin Koi Pond Workshop", Some("our #environment pledge"))
            .contains(&"climate".to_string()));
        assert!(!derive_topics("Art Therapy Night", None).contains(&"mental-health".to_string()));
        assert!(!derive_topics("Block Party", Some("Garden Layout, Bengaluru"))
            .contains(&"gardening".to_string()));
    }

    /// Keywords still match as whole words in prose, and hashtags still
    /// derive their topic.
    #[test]
    fn keywords_match_as_words() {
        assert!(derive_topics("Board Games Night", None).contains(&"tabletop".to_string()));
        assert!(derive_topics("Board Games Night", Some("#boardgames"))
            .contains(&"tabletop".to_string()));
        assert!(!derive_topics("The skater landed it", None).contains(&"ska".to_string()));
    }

    /// The venue string is not searchable: a basketball clinic at
    /// "St. Joseph's Arts And Science College" is a basketball event, not a
    /// science event (the Science-feed bleed).
    #[test]
    fn location_text_is_not_searched() {
        let topics = derive_topics(
            "Women's Beginner Basketball Coaching",
            Some("See more details at https://sistersinsweat.com/bengaluru/products/Women-Bangalore-Basketball-Beginner"),
        );
        assert_eq!(topics, vec!["basketball".to_string()]);
    }

    /// At most four topics survive, and field priority decides which ones:
    /// a name claim outranks hashtags and prose keywords.
    #[test]
    fn derivation_caps_topics_by_field_priority() {
        let topics = derive_topics(
            "Science Man",
            Some("our running club meets to discuss architecture, politics and music"),
        );
        assert_eq!(topics.len(), MAX_DERIVED_TOPICS);
        assert!(topics.contains(&"science".to_string()), "name claim must survive: {topics:?}");

        // A description hashtag outranks a description keyword when the cap cuts.
        let topics = derive_topics(
            "#tabletop",
            Some("board games, running and architecture"),
        );
        assert!(topics.contains(&"tabletop".to_string()));
    }
}

// ─── Index writes (shared by both firehose arms + write-through) ───────

/// Idempotent event upsert keyed by the event's canonical URI (spec: replay
/// never duplicates; every row corresponds to a currently existing record).
/// The author handle comes from the local DID→handle map when known — never a
/// remote call. `source` records the protocol the row came from (`atproto` from
/// the firehose arms, `activitypub` from
/// [`crate::ap::ingest::parse_ap_event`]); the `uri` column is TEXT precisely
/// so at-URIs and AP object ids share the table.
pub async fn upsert_event(
    pool: &PgPool,
    uri: &str,
    cid: &str,
    author_did: &str,
    event: ParsedEvent,
    source: &str,
) -> Result<(), sqlx::Error> {
    let handle: Option<String> = sqlx::query_scalar(
        "SELECT handle FROM appview_did_handle WHERE did = $1 AND handle IS NOT NULL",
    )
    .bind(author_did)
    .fetch_optional(pool)
    .await?;

    insert_event(pool, uri, cid, author_did, handle.as_deref(), event, source).await
}

/// The row write behind [`upsert_event`], with the author handle supplied by
/// the caller. Split out so the AP ingest path — which already knows the
/// handle derived from the actor URL and may run inside the inbox transaction
/// (any `PgExecutor`) — does not pay for a DID→handle lookup that cannot hit
/// (remote AP actors are not in the AT handle map).
pub(crate) async fn insert_event(
    pool: impl sqlx::PgExecutor<'_>,
    uri: &str,
    cid: &str,
    author_did: &str,
    author_handle: Option<&str>,
    event: ParsedEvent,
    source: &str,
) -> Result<(), sqlx::Error> {
    let topics = derive_topics(&event.name, event.description.as_deref());

    sqlx::query(
        r#"INSERT INTO calendar_events
             (uri, cid, author_did, author_handle, name, description, starts_at, ends_at,
              mode, status, location_text, locations, uris, record_created_at, topics, source)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
           ON CONFLICT (uri) DO UPDATE SET
             cid = EXCLUDED.cid, author_did = EXCLUDED.author_did,
             author_handle = COALESCE(EXCLUDED.author_handle, calendar_events.author_handle),
             name = EXCLUDED.name, description = EXCLUDED.description,
             starts_at = EXCLUDED.starts_at, ends_at = EXCLUDED.ends_at,
             mode = EXCLUDED.mode, status = EXCLUDED.status,
             location_text = EXCLUDED.location_text, locations = EXCLUDED.locations,
             uris = EXCLUDED.uris, record_created_at = EXCLUDED.record_created_at,
             topics = EXCLUDED.topics, source = EXCLUDED.source,
             indexed_at = now()"#,
    )
    .bind(uri)
    .bind(cid)
    .bind(author_did)
    .bind(author_handle)
    .bind(&event.name)
    .bind(&event.description)
    .bind(event.starts_at)
    .bind(event.ends_at)
    .bind(&event.mode)
    .bind(&event.status)
    .bind(&event.location_text)
    .bind(&event.locations)
    .bind(&event.uris)
    .bind(event.record_created_at)
    .bind(&topics)
    .bind(source)
    .execute(pool)
    .await?;
    Ok(())
}

/// Recompute the derived `topics` column from the current vocabulary at
/// startup (derived index — never hand-edited). The vocabulary lives in code,
/// so a tag change (e.g. dropping "board") must invalidate every derived row;
/// only changed rows are written.
pub async fn backfill_topics(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let rows: Vec<(String, String, Option<String>, Vec<String>)> = sqlx::query_as(
        "SELECT uri, name, description, topics
           FROM calendar_events",
    )
    .fetch_all(pool)
    .await?;
    let mut repaired = 0u64;
    for (uri, name, description, current) in rows {
        let topics = derive_topics(&name, description.as_deref());
        if topics == current {
            continue;
        }
        sqlx::query("UPDATE calendar_events SET topics = $2 WHERE uri = $1")
            .bind(&uri)
            .bind(&topics)
            .execute(pool)
            .await?;
        repaired += 1;
    }
    Ok(repaired)
}

/// Idempotent RSVP upsert keyed by at-URI. Multiple RSVP rows per
/// (event, author) from foreign apps are kept — the rollup dedupes
/// latest-wins (spec: response change replaces for our writes; rollup
/// handles what we don't control).
pub async fn upsert_rsvp(
    pool: &PgPool,
    uri: &str,
    author_did: &str,
    rsvp: ParsedRsvp,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO calendar_rsvps (uri, event_uri, author_did, status, record_created_at)
           VALUES ($1, $2, $3, $4, $5)
           ON CONFLICT (uri) DO UPDATE SET
             status = EXCLUDED.status,
             record_created_at = EXCLUDED.record_created_at,
             indexed_at = now()"#,
    )
    .bind(uri)
    .bind(&rsvp.subject_uri)
    .bind(author_did)
    .bind(&rsvp.status)
    .bind(rsvp.record_created_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record deleted → index row gone, presentation cascade (spec: no phantom
/// rows; comments/reactions never render without the event).
pub async fn delete_event(pool: &PgPool, uri: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM event_comments WHERE event_uri = $1")
        .bind(uri)
        .execute(pool)
        .await?;
    // reactions.target is keyed by TEXT post_id — at-URIs for events.
    sqlx::query("DELETE FROM reactions WHERE post_id = $1")
        .bind(uri)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM calendar_events WHERE uri = $1")
        .bind(uri)
        .execute(pool)
        .await?;
    Ok(())
}

/// RSVP record deleted → rollup row gone.
pub async fn delete_rsvp(pool: &PgPool, uri: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM calendar_rsvps WHERE uri = $1")
        .bind(uri)
        .execute(pool)
        .await?;
    Ok(())
}

// ─── Rollup reads ─────────────────────────────────────────────────────

/// Per-status counts for one event, deduped latest-wins per author (spec:
/// foreign apps may leave multiple RSVP rows per author — each author counts
/// once, at their most recent response). The rsvp lexicon defines no
/// `createdAt`, so recency falls back to `indexed_at`; ties break on uri.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct RsvpCounts {
    pub going: i64,
    pub interested: i64,
    pub notgoing: i64,
}

impl RsvpCounts {
    pub fn total(&self) -> i64 {
        self.going + self.interested + self.notgoing
    }
}

pub async fn rsvp_counts(pool: &PgPool, event_uri: &str) -> Result<RsvpCounts, sqlx::Error> {
    let (going, interested, notgoing): (i64, i64, i64) = sqlx::query_as(
        r#"WITH latest AS (
             SELECT DISTINCT ON (author_did) status
             FROM calendar_rsvps
             WHERE event_uri = $1
             ORDER BY author_did,
                      COALESCE(record_created_at, indexed_at) DESC,
                      uri DESC
           )
           SELECT count(*) FILTER (WHERE status = $2),
                  count(*) FILTER (WHERE status = $3),
                  count(*) FILTER (WHERE status = $4)
           FROM latest"#,
    )
    .bind(event_uri)
    .bind(KNOWN_RSVP_STATUSES[0]) // #going
    .bind(KNOWN_RSVP_STATUSES[1]) // #interested
    .bind(KNOWN_RSVP_STATUSES[2]) // #notgoing
    .fetch_one(pool)
    .await?;
    Ok(RsvpCounts {
        going,
        interested,
        notgoing,
    })
}

/// The viewer's current RSVP status for an event (latest-wins), if any —
/// spec: own status resolves from their record across devices/surfaces.
pub async fn viewer_rsvp(
    pool: &PgPool,
    event_uri: &str,
    author_did: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT status FROM calendar_rsvps
           WHERE event_uri = $1 AND author_did = $2
           ORDER BY COALESCE(record_created_at, indexed_at) DESC, uri DESC
           LIMIT 1"#,
    )
    .bind(event_uri)
    .bind(author_did)
    .fetch_optional(pool)
    .await
}

/// Same rollup for a page of events: one query, grouped counts per event.
pub async fn rsvp_counts_for_many(
    pool: &PgPool,
    event_uris: &[String],
) -> Result<HashMap<String, RsvpCounts>, sqlx::Error> {
    if event_uris.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        r#"SELECT DISTINCT ON (event_uri, author_did) event_uri, author_did, status
           FROM calendar_rsvps
           WHERE event_uri = ANY($1)
           ORDER BY event_uri, author_did,
                    COALESCE(record_created_at, indexed_at) DESC, uri DESC"#,
    )
    .bind(event_uris)
    .fetch_all(pool)
    .await?;
    let mut out: HashMap<String, RsvpCounts> = HashMap::new();
    for (event_uri, _, status) in rows {
        let counts = out.entry(event_uri).or_default();
        match status.as_str() {
            s if s == KNOWN_RSVP_STATUSES[0] => counts.going += 1,
            s if s == KNOWN_RSVP_STATUSES[1] => counts.interested += 1,
            s if s == KNOWN_RSVP_STATUSES[2] => counts.notgoing += 1,
            _ => {}
        }
    }
    Ok(out)
}

/// The viewer's current status across a page of events (latest-wins each).
/// A user's identity may span several DIDs (native + linked), so this takes
/// the full set.
pub async fn viewer_rsvps_for(
    pool: &PgPool,
    event_uris: &[String],
    viewer_dids: &[String],
) -> Result<HashMap<String, String>, sqlx::Error> {
    if event_uris.is_empty() || viewer_dids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(String, String)> = sqlx::query_as(
        r#"SELECT DISTINCT ON (event_uri) event_uri, status
           FROM calendar_rsvps
           WHERE event_uri = ANY($1) AND author_did = ANY($2)
           ORDER BY event_uri, COALESCE(record_created_at, indexed_at) DESC, uri DESC"#,
    )
    .bind(event_uris)
    .bind(viewer_dids)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

/// UI response → lexicon token (spec: going→#going, maybe→#interested,
/// not_going→#notgoing). None = unknown UI value.
pub fn rsvp_status_to_lexicon(ui: &str) -> Option<&'static str> {
    match ui {
        "going" => Some(KNOWN_RSVP_STATUSES[0]),
        "maybe" => Some(KNOWN_RSVP_STATUSES[1]),
        "not_going" => Some(KNOWN_RSVP_STATUSES[2]),
        _ => None,
    }
}

/// Lexicon token → UI value (inverse of [`rsvp_status_to_lexicon`]).
pub fn rsvp_status_to_ui(lexicon: &str) -> &'static str {
    match lexicon {
        s if s == KNOWN_RSVP_STATUSES[1] => "maybe",
        s if s == KNOWN_RSVP_STATUSES[2] => "not_going",
        _ => "going",
    }
}

// ─── Tests (spec: reject invalid, parse canonical + legacy) ───────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_canonical_event() {
        let v = json!({
            "$type": EVENT_NSID,
            "name": "Climate book club",
            "description": "Bring your own book\n\u{0}no NULs please",
            "createdAt": "2026-11-01T10:00:00.000Z",
            "startsAt": "2026-11-15T18:30:00.000Z",
            "endsAt": "2026-11-15T20:00:00.000Z",
            "mode": "community.lexicon.calendar.event#inperson",
            "status": "community.lexicon.calendar.event#scheduled",
            "locations": [{
                "$type": "community.lexicon.location.address",
                "name": "The Reading Room",
                "street": "12 Main St",
                "locality": "Berlin",
                "region": "BE",
                "postalCode": "10115",
                "country": "DE"
            }],
            "uris": [{"uri": "https://example.org/flyer.png", "name": "image"}],
            "rsvpExpected": true
        });
        let e = parse_event(&v, EVENT_NSID).expect("canonical event parses");
        assert_eq!(e.name, "Climate book club");
        assert_eq!(e.record_created_at, parse_rfc3339("2026-11-01T10:00:00.000Z"));
        assert_eq!(e.starts_at, parse_rfc3339("2026-11-15T18:30:00.000Z"));
        assert_eq!(e.ends_at, parse_rfc3339("2026-11-15T20:00:00.000Z"));
        assert_eq!(e.mode.as_deref(), Some("community.lexicon.calendar.event#inperson"));
        assert_eq!(e.status, "community.lexicon.calendar.event#scheduled");
        assert_eq!(
            e.location_text.as_deref(),
            Some("The Reading Room, 12 Main St, Berlin, BE, 10115")
        );
        assert!(e.locations.is_array() && e.uris.is_array());
        // NUL bytes never reach Postgres
        assert!(!e.description.as_deref().unwrap().contains('\u{0}'));
    }

    #[test]
    fn parses_minimal_event() {
        let v = json!({"$type": EVENT_NSID, "name": "Minimal", "createdAt": "2026-11-01T10:00:00Z"});
        let e = parse_event(&v, EVENT_NSID).expect("minimal event is valid (spec: lexicon requires only name+createdAt)");
        assert!(e.starts_at.is_none() && e.ends_at.is_none());
        assert!(e.location_text.is_none());
        assert_eq!(e.status, DEFAULT_EVENT_STATUS);
    }

    #[test]
    fn rejects_invalid_canonical_events() {
        // missing createdAt (canonical required)
        assert!(parse_event(&json!({"name": "x"}), EVENT_NSID).is_none());
        // malformed createdAt
        assert!(parse_event(
            &json!({"name": "x", "createdAt": "yesterday"}), EVENT_NSID
        ).is_none());
        // missing / empty name
        assert!(parse_event(&json!({"createdAt": "2026-11-01T10:00:00Z"}), EVENT_NSID).is_none());
        assert!(parse_event(
            &json!({"name": "  ", "createdAt": "2026-11-01T10:00:00Z"}), EVENT_NSID
        ).is_none());
        // malformed startsAt → reject, never index without it (spec)
        assert!(parse_event(
            &json!({"name": "x", "createdAt": "2026-11-01T10:00:00Z", "startsAt": "soon"}),
            EVENT_NSID
        ).is_none());
    }

    #[test]
    fn parses_legacy_event_without_created_at() {
        let v = json!({
            "$type": "events.smokesignal.Event",
            "name": "Old meetup",
            "description": "From the archive",
            "startDate": "2025-06-01T17:00:00.000Z",
            "endDate": "2025-06-01T19:00:00.000Z",
            "location": {"name": "Old venue"}
        });
        let e = parse_event(&v, "events.smokesignal.Event")
            .expect("legacy event parses without createdAt");
        assert!(e.record_created_at.is_none());
        assert_eq!(e.starts_at, parse_rfc3339("2025-06-01T17:00:00.000Z"));
        assert_eq!(e.ends_at, parse_rfc3339("2025-06-01T19:00:00.000Z"));
        assert_eq!(e.name, "Old meetup");
    }

    #[test]
    fn location_text_skips_urls() {
        let locs = json!([
            {"uri": "https://maps.example/x", "name": "Venue"},
            {"$type": "community.lexicon.location.geo", "lat": 1.5, "long": 2.5}
        ]);
        let t = location_text(&locs).expect("venue name extracted");
        assert_eq!(t, "Venue");
        assert!(!t.contains("https://"));
    }

    #[test]
    fn parses_rsvp_statuses() {
        let base = "at://did:plc:abc/community.lexicon.calendar.event/3abc";
        // known tokens
        for status in KNOWN_RSVP_STATUSES {
            let v = json!({"$type": RSVP_NSID, "subject": {"uri": base, "cid": "bafy"}, "status": status});
            assert_eq!(parse_rsvp(&v).expect("known status").status, *status);
        }
        // absent status → lexicon default #going
        let v = json!({"$type": RSVP_NSID, "subject": {"uri": base, "cid": "bafy"}});
        assert_eq!(parse_rsvp(&v).expect("default status").status, RSVP_GOING);
        // unknown token rejected
        let v = json!({"subject": {"uri": base}, "status": "community.lexicon.calendar.rsvp#maybe"});
        assert!(parse_rsvp(&v).is_none());
        // non-at subject rejected (spec: strongRef to the event's at-URI)
        let v = json!({"subject": {"uri": "https://example.com/e/1"}, "status": RSVP_GOING});
        assert!(parse_rsvp(&v).is_none());
        // missing subject rejected
        assert!(parse_rsvp(&json!({"status": RSVP_GOING})).is_none());
    }

    #[test]
    fn path_gates_match_collections() {
        assert!(is_event_path("community.lexicon.calendar.event/3abc"));
        assert!(is_event_path("events.smokesignal.Event/3abc"));
        assert!(is_event_path("events.smokesignal.calendar.event/3abc"));
        assert!(!is_event_path("community.lexicon.calendar.rsvp/3abc"));
        assert!(is_rsvp_path("community.lexicon.calendar.rsvp/3abc"));
        // chat namespaces never reach the calendar tables (spec)
        assert!(!is_event_path("place.stream.chat.message/3abc"));
        assert!(!is_event_path("app.bsky.graph.follow/3abc"));
    }

    #[test]
    fn cbor_bridge_preserves_calendar_shapes() {
        use serde_cbor::Value as C;
        let cbor = C::Map(
            [
                (C::Text("name".into()), C::Text("Meetup".into())),
                (C::Text("rsvpExpected".into()), C::Bool(true)),
                (
                    C::Text("uris".into()),
                    C::Array(vec![C::Map([(
                        C::Text("uri".into()),
                        C::Text("https://x.test/i.png".into()),
                    )].into_iter().collect())]),
                ),
            ]
            .into_iter()
            .collect(),
        );
        let v = cbor_to_json(&cbor);
        assert_eq!(v["name"], "Meetup");
        assert_eq!(v["rsvpExpected"], true);
        assert_eq!(v["uris"][0]["uri"], "https://x.test/i.png");
    }

    /// DB integration: counts dedupe latest-wins per author and own-status
    /// resolves from the newest record (spec: RSVP rollup). Skips without
    /// DATABASE_URL.
    #[tokio::test]
    async fn rsvp_rollup_latest_wins() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("DATABASE_URL unset — skipping DB integration test");
            return;
        };
        let pool = PgPool::connect(&url).await.expect("db connect");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let did_a = format!("did:plc:ctestrolla{}", stamp);
        let did_b = format!("did:plc:ctestrollb{}", stamp);
        let event_uri = format!("at://{}/community.lexicon.calendar.event/ev", did_a);
        let t0 = parse_rfc3339("2026-11-01T12:00:00Z").unwrap();
        let t1 = t0 + chrono::Duration::hours(1);

        // author A: going first, then interested (later — wins)
        upsert_rsvp(
            &pool,
            &format!("at://{}/community.lexicon.calendar.rsvp/r1", did_a),
            &did_a,
            ParsedRsvp {
                subject_uri: event_uri.clone(),
                status: RSVP_GOING.to_string(),
                record_created_at: Some(t0),
            },
        )
        .await
        .unwrap();
        upsert_rsvp(
            &pool,
            &format!("at://{}/community.lexicon.calendar.rsvp/r2", did_a),
            &did_a,
            ParsedRsvp {
                subject_uri: event_uri.clone(),
                status: KNOWN_RSVP_STATUSES[1].to_string(), // #interested
                record_created_at: Some(t1),
            },
        )
        .await
        .unwrap();
        // author B: going
        upsert_rsvp(
            &pool,
            &format!("at://{}/community.lexicon.calendar.rsvp/r3", did_b),
            &did_b,
            ParsedRsvp {
                subject_uri: event_uri.clone(),
                status: RSVP_GOING.to_string(),
                record_created_at: Some(t0),
            },
        )
        .await
        .unwrap();

        // Three stored rows, two authors → counts of two, latest status wins.
        let counts = rsvp_counts(&pool, &event_uri).await.unwrap();
        assert_eq!(counts.going, 1, "author B going");
        assert_eq!(counts.interested, 1, "author A latest = interested, not the earlier going");
        assert_eq!(counts.notgoing, 0);
        assert_eq!(counts.total(), 2);

        assert_eq!(
            viewer_rsvp(&pool, &event_uri, &did_a).await.unwrap().as_deref(),
            Some(KNOWN_RSVP_STATUSES[1]),
            "own status resolves to the newest response"
        );
        assert_eq!(
            viewer_rsvp(&pool, &event_uri, "did:plc:stranger").await.unwrap(),
            None
        );

        // Cleanup.
        let _ = sqlx::query("DELETE FROM calendar_rsvps WHERE event_uri = $1")
            .bind(&event_uri)
            .execute(&pool)
            .await;
    }

    #[test]
    fn topic_derivation_is_word_bounded() {
        // Interests are topics: a Ska subscriber sees ska events, not tasks.
        assert!(derive_topics("Ska Night at the Roxy", None).contains(&"ska".to_string()));
        assert!(derive_topics("Live tonight #ska", None).contains(&"ska".to_string()));
        // "task" and "skateboard" contain "ska" but are not ska events.
        assert!(!derive_topics("Community task force", None).contains(&"ska".to_string()));
        let skate = derive_topics("Skateboard meetup", None);
        assert!(skate.contains(&"skateboarding".to_string()));
        assert!(!skate.contains(&"ska".to_string()));
    }

    #[test]
    fn topic_derivation_reads_description() {
        let topics = derive_topics("Record fair", Some("punk and hardcore crates"));
        assert!(topics.contains(&"punk".to_string()));
        assert!(topics.contains(&"hardcore".to_string()));
        assert!(!topics.contains(&"ska".to_string()));
    }
}
