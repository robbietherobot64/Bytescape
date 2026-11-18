//! Events — the calendar surface (specs: events/browse, events/participate).
//!
//! Reads come from the derived `calendar_*` index (events/ingestion). Writes
//! go to the participant's own PDS as `community.lexicon.calendar.*` records
//! via the stories dual-write path (`at_identity_for` / `write_record` /
//! `delete_author_record`), then write-through into the index — read-your-
//! writes, because our own firehose is not guaranteed to loop back through
//! the relay (spec: write-through after putRecord; no silent local rows).
//!
//! at-URIs travel in query strings and bodies, never path segments (`://`
//! and `/` make path params a routing trap — design decision 4). The legacy
//! `/api/events/{uuid}/...` routes are gone with this rewrite (breaking
//! internal; in-repo consumers only).

use axum::{
    extract::{Multipart, Query, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use uuid::Uuid;

use crate::api::feedgen::{topic_for_interest, TOPICS};
use crate::api::stories::{
    at_identity_for, delete_author_record, own_dids, record_cid, write_record, AtId,
};
use crate::calendar;
use crate::db;
use crate::db::accounts::LinkedAccountRow;
use crate::db::follows::FollowRow;
use crate::db::subscriptions::SubscriptionRow;
use crate::error::AppError;
use crate::{auth, AppState};

// ─── Response shapes ───────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AuthorView {
    pub did: String,
    pub handle: String,
    pub display_name: Option<String>,
    pub avatar: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LocationView {
    pub name: Option<String>,
    pub address: Option<String>,
}

/// The read shape both surfaces render (spec: events/browse detail fields).
/// `id` is the event's at-URI — the stable, portable key.
#[derive(Debug, Serialize)]
pub struct EventView {
    pub id: String,
    pub cid: String,
    pub name: String,
    pub description: Option<String>,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    pub mode: Option<String>,
    pub status: String,
    pub location: Option<LocationView>,
    pub image: Option<String>,
    pub uris: serde_json::Value,
    pub author: AuthorView,
    pub is_upcoming: bool,
    /// Viewer's own response in UI vocabulary (going/maybe/not_going).
    pub rsvp_status: Option<String>,
    pub rsvp_counts: calendar::RsvpCounts,
    pub likes_count: i64,
    pub replies_count: u32,
    pub reaction_counts: Option<HashMap<String, i64>>,
    pub user_reaction: Option<String>,
    /// Why this event is in the list: following RSVP, a subscribed feed, or a
    /// subscribed hashtag. Empty when the viewer has no matching source —
    /// lists only ever contain source-matched events.
    pub provenance: Vec<ProvenanceView>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ProvenanceView {
    /// "following" | "feeds" | "hashtags". "hashtags" chips carry the `#cats`
    /// display form and appear only on a literal hashtag match in the event
    /// text; topic membership shows as "feeds". There is no bare-word chip on
    /// this surface (events are long-form prose; a prose noun is not a tag).
    pub kind: String,
    pub label: String,
}

#[derive(Debug, Serialize)]
pub struct EventsFeedResponse {
    pub items: Vec<EventView>,
    pub has_more: bool,
}

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    status: Option<String>,
    limit: Option<i64>,
    q: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
    location: Option<String>,
    /// Source tab: home | following | feeds | hashtags. Home is the whole
    /// index; the others resolve the viewer's follow graph / subscriptions.
    source: Option<String>,
    /// Scope inside the tab: one source id from /api/events/sources. A
    /// followed DID, a feed subscription id, or a hashtag subscription id.
    followed_did: Option<String>,
    feed_id: Option<String>,
    hashtag_id: Option<String>,
    /// Keyset cursor: the last item's sort key (RFC3339, or absent when the
    /// cursor sits in the undated block of the "all" view).
    cursor_starts_at: Option<String>,
    cursor_uri: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UriQuery {
    uri: String,
}

#[derive(Debug, Deserialize)]
pub struct CommentsQuery {
    uri: String,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteCommentQuery {
    id: String,
}

#[derive(Debug, Deserialize)]
pub struct RsvpRequest {
    uri: String,
    /// UI vocabulary: going | maybe | not_going.
    status: String,
}

#[derive(Debug, Serialize)]
pub struct RsvpResponse {
    pub uri: String,
    pub status: String,
    pub rsvp_counts: calendar::RsvpCounts,
}

#[derive(Debug, Serialize)]
pub struct EventCommentView {
    pub id: String,
    pub content: String,
    pub created_at: String,
    pub author: AuthorView,
}

#[derive(Debug, Deserialize)]
pub struct CommentRequest {
    uri: String,
    content: String,
}

#[derive(Debug, Deserialize)]
pub struct ReactionRequest {
    uri: String,
    #[serde(default = "default_emoji")]
    emoji: String,
}

fn default_emoji() -> String {
    "❤️".to_string()
}

#[derive(Debug, Deserialize)]
pub struct CreateEventRequest {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    starts_at: Option<String>,
    #[serde(default)]
    ends_at: Option<String>,
    /// Short (inperson/virtual/hybrid) or full lexicon token.
    #[serde(default)]
    mode: Option<String>,
    /// Free-text venue/city; recorded in the record's locations as a named
    /// map link (the address sub-lexicon requires structured country data we
    /// cannot honestly invent from a string).
    #[serde(default)]
    location: Option<String>,
    /// Uploaded image URL → recorded as a `uris` entry `{uri, name:"image"}`.
    #[serde(default)]
    picture_url: Option<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/events",
            get(list_events).post(create_event).delete(delete_event_handler),
        )
        .route("/api/events/detail", get(event_detail))
        .route("/api/events/sources", get(event_sources))
        .route("/api/events/upload-image", post(upload_event_image))
        .route("/api/events/my-rsvps", get(my_rsvps))
        .route("/api/events/rsvp", post(rsvp_event).delete(remove_rsvp))
        .route("/api/events/reactions", post(add_reaction).delete(remove_reaction))
        .route(
            "/api/events/comments",
            get(list_comments).post(post_comment).delete(delete_comment),
        )
}

// ─── Shared plumbing ───────────────────────────────────────────────────

fn extract_user_id(headers: &HeaderMap, secret: &str) -> Result<Uuid, AppError> {
    let token = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Auth("Missing or invalid Authorization header".into()))?;
    let claims = auth::validate_token(token, secret)?;
    Uuid::parse_str(&claims.sub).map_err(|_| AppError::Auth("Invalid user ID in token".into()))
}

/// Optional viewer: (user_id, viewer DIDs). Anonymous browse is public
/// (spec: first page renders without sign-in); cookie sessions arrive as
/// injected Bearer headers, so this one path serves both.
fn optional_viewer(
    headers: &HeaderMap,
    secret: &str,
) -> Option<Uuid> {
    let token = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))?;
    let claims = auth::validate_token(token, secret).ok()?;
    Uuid::parse_str(&claims.sub).ok()
}

/// Resolve the optional viewer to (user_id, viewer DIDs). Anonymous browse is
/// public (spec: first page renders without sign-in); cookie sessions arrive
/// as injected Bearer headers, so this one path serves both.
async fn resolve_viewer(
    state: &AppState,
    headers: &HeaderMap,
) -> Option<(Uuid, Vec<String>)> {
    let uid = optional_viewer(headers, &state.config.jwt_secret)?;
    Some((uid, own_dids(&state.db, uid).await))
}

/// at://did/collection/rkey → parts (None when not a well-formed at-URI).
fn split_at_uri(uri: &str) -> Option<(String, String, String)> {
    let rest = uri.strip_prefix("at://")?;
    let mut parts = rest.splitn(3, '/');
    let did = parts.next()?.to_string();
    let collection = parts.next()?.to_string();
    let rkey = parts.next()?.to_string();
    if did.is_empty() || collection.is_empty() || rkey.is_empty() {
        return None;
    }
    Some((did, collection, rkey))
}

/// Deterministic RSVP rkey so a status change replaces the same record
/// (spec: response change replaces, never duplicates).
fn rsvp_rkey(event_uri: &str) -> String {
    let digest = Sha256::digest(event_uri.as_bytes());
    format!("rsvp-{}", hex::encode(&digest)[..32].to_string())
}

/// One row shape for every read path; `lat` supplies best-effort profile
/// display columns off the author's own posts (same posture as stories'
/// author_info — handle resolution is cached, never blocking).
const EVENT_SELECT: &str = r#"SELECT e.uri, e.cid, e.author_did,
       COALESCE(NULLIF(e.author_handle, ''), h.handle) AS author_handle, e.name,
       e.description, e.starts_at, e.ends_at, e.mode, e.status, e.topics, e.location_text,
       e.locations, e.uris, e.record_created_at,
       lat.author_display_name, lat.author_avatar
  FROM calendar_events e
  LEFT JOIN appview_did_handle h ON h.did = e.author_did
  LEFT JOIN LATERAL (
      SELECT p.author_display_name, p.author_avatar
      FROM appview_posts p
      WHERE p.author_did = e.author_did AND p.author_display_name IS NOT NULL
      ORDER BY p.created_at DESC
      LIMIT 1
  ) lat ON true"#;

#[derive(Debug, sqlx::FromRow)]
struct EventRowLite {
    uri: String,
    cid: String,
    author_did: String,
    author_handle: Option<String>,
    name: String,
    description: Option<String>,
    starts_at: Option<chrono::DateTime<chrono::Utc>>,
    ends_at: Option<chrono::DateTime<chrono::Utc>>,
    mode: Option<String>,
    status: String,
    topics: Vec<String>,
    location_text: Option<String>,
    locations: serde_json::Value,
    uris: serde_json::Value,
    record_created_at: Option<chrono::DateTime<chrono::Utc>>,
    author_display_name: Option<String>,
    author_avatar: Option<String>,
}

/// Best-effort author identities for a page of events: local DID→handle
/// cache first, then Bluesky's getProfiles (bounded, persisted). Events
/// indexed from the firehose carry a NULL handle; without this the UI falls
/// back to raw DIDs.
async fn resolve_event_identities(
    pool: &sqlx::PgPool,
    dids: &[String],
) -> HashMap<String, (String, Option<String>, Option<String>)> {
    let mut out: HashMap<String, (String, Option<String>, Option<String>)> = HashMap::new();
    if dids.is_empty() {
        return out;
    }
    let cached: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT did, handle, display_name, avatar FROM appview_did_handle
         WHERE did = ANY($1) AND handle IS NOT NULL",
    )
    .bind(dids)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (did, handle, display_name, avatar) in cached {
        out.insert(did, (handle, display_name, avatar));
    }
    let missing: Vec<String> = dids
        .iter()
        .filter(|d| !out.contains_key(*d))
        .cloned()
        .collect();
    if !missing.is_empty() {
        let resolved = crate::api::feed::sources::resolve_profiles_remote(pool, &missing).await;
        for (did, (handle, display_name, avatar)) in resolved {
            out.insert(did, (handle, display_name, avatar));
        }
    }
    out
}

/// Attach counts/viewer state to a page of rows (one query per concern —
/// no N+1; author identity resolution is cached and bounded).
async fn hydrate(
    pool: &sqlx::PgPool,
    rows: Vec<EventRowLite>,
    viewer: Option<(Uuid, Vec<String>)>,
    provenance: Option<&ProvenanceContext>,
) -> Result<Vec<EventView>, AppError> {
    let uris: Vec<String> = rows.iter().map(|r| r.uri.clone()).collect();
    let author_dids: Vec<String> = rows.iter().map(|r| r.author_did.clone()).collect();
    let author_identities = resolve_event_identities(pool, &author_dids).await;

    let rsvp_counts = calendar::rsvp_counts_for_many(pool, &uris).await.unwrap_or_default();
    let own_rsvps = match &viewer {
        Some((_, dids)) if !dids.is_empty() => {
            calendar::viewer_rsvps_for(pool, &uris, dids).await.unwrap_or_default()
        }
        _ => HashMap::new(),
    };
    let reaction_counts = db::reactions::ReactionRow::counts_for_posts(pool, &uris)
        .await
        .unwrap_or_default();
    let user_reactions = match &viewer {
        Some((uid, _)) => {
            db::reactions::ReactionRow::user_reactions_for_posts(pool, &uris, *uid)
                .await
                .unwrap_or_default()
        }
        None => HashMap::new(),
    };
    let comment_counts = db::event_comments::EventCommentRow::counts_for_events(pool, &uris)
        .await
        .unwrap_or_default();

    // Latest RSVP per (event, followed author); only going/interested count
    // as Following provenance. One query for the whole page.
    let followed_going: std::collections::HashSet<String> = match provenance {
        Some(ctx) if !ctx.follows.is_empty() => {
            let follows: Vec<String> = ctx.follows.iter().cloned().collect();
            let rows: Vec<(String, String)> = sqlx::query_as(
                r#"SELECT DISTINCT ON (event_uri, author_did) event_uri, status
                   FROM calendar_rsvps
                   WHERE event_uri = ANY($1) AND author_did = ANY($2)
                   ORDER BY event_uri, author_did, record_created_at DESC NULLS LAST"#,
            )
            .bind(&uris)
            .bind(&follows)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            rows.into_iter()
                .filter(|(_, status)| status.ends_with("#going") || status.ends_with("#interested"))
                .map(|(uri, _)| uri)
                .collect()
        }
        _ => std::collections::HashSet::new(),
    };

    let now = chrono::Utc::now();
    let out = rows
        .into_iter()
        .map(|r| {
            let rsvp_counts = rsvp_counts.get(&r.uri).copied().unwrap_or_default();
            let rsvp_status = own_rsvps
                .get(&r.uri)
                .map(|s| calendar::rsvp_status_to_ui(s).to_string());
            let counts = reaction_counts.get(&r.uri).cloned().unwrap_or_default();
            let likes_count: i64 = counts.values().sum();
            let uris_array = r.uris.as_array().cloned().unwrap_or_default();
            let image = uris_array
                .iter()
                .find(|u| u.get("name").and_then(|n| n.as_str()) == Some("image"))
                .and_then(|u| u.get("uri").and_then(|v| v.as_str()))
                .map(|s| s.to_string());
            let location = {
                let name = r
                    .locations
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|l| l.get("name").and_then(|n| n.as_str()))
                    .map(|s| s.to_string());
                if name.is_none() && r.location_text.is_none() {
                    None
                } else {
                    Some(LocationView {
                        name,
                        address: r.location_text.clone(),
                    })
                }
            };
            // Honest attribution: resolved handle cache → DID fallback.
            let ident = author_identities.get(&r.author_did);
            let handle = r
                .author_handle
                .clone()
                .filter(|h| !h.is_empty())
                .or_else(|| ident.map(|(h, _, _)| h.clone()))
                .unwrap_or_else(|| r.author_did.clone());
            let display_name = r
                .author_display_name
                .clone()
                .or_else(|| ident.and_then(|(_, d, _)| d.clone()));
            let avatar = r
                .author_avatar
                .clone()
                .or_else(|| ident.and_then(|(_, _, a)| a.clone()));

            let mut provenance_view: Vec<ProvenanceView> = Vec::new();
            if followed_going.contains(&r.uri) {
                provenance_view.push(ProvenanceView {
                    kind: "following".into(),
                    label: "Following".into(),
                });
            }
            if let Some(ctx) = provenance {
                // One representative feed + one representative hashtag keeps
                // the card readable; the full source lists live in the rail.
                let feed_label = ctx.feeds.iter().find_map(|feed| {
                    let hit = feed
                        .topics
                        .iter()
                        .any(|t| r.topics.iter().any(|rt| rt == t));
                    hit.then(|| feed.label.clone())
                });
                if let Some(label) = feed_label {
                    provenance_view.push(ProvenanceView {
                        kind: "feeds".into(),
                        label,
                    });
                }
                let text = format!(
                    " {} {} ",
                    r.name.to_lowercase(),
                    r.description.as_deref().unwrap_or_default().to_lowercase()
                );
                // Chip truth is now the same rule as membership on this
                // surface: a `#cats` chip only when the literal hashtag is in
                // the text. Event descriptions are long-form prose, so a bare
                // word ("a weekly practice space") no longer earns a chip at
                // all — previously it produced a phantom non-# `interest` chip.
                if let Some(label) = literal_hashtag_chip(&text, &ctx.hashtags) {
                    provenance_view.push(ProvenanceView {
                        kind: "hashtags".into(),
                        label: label.to_string(),
                    });
                }
            }
            // No fallback chip: provenance names the viewer's matching
            // sources only. Unmatched events never reach a list, so an empty
            // provenance is only possible on deep-linked detail views.

            EventView {
                id: r.uri.clone(),
                cid: r.cid,
                name: r.name,
                description: r.description,
                starts_at: r.starts_at.map(|t| t.to_rfc3339()),
                ends_at: r.ends_at.map(|t| t.to_rfc3339()),
                mode: r.mode,
                status: r.status,
                location,
                image,
                uris: r.uris,
                author: AuthorView {
                    did: r.author_did,
                    handle,
                    display_name,
                    avatar,
                },
                is_upcoming: r.starts_at.map(|t| t > now).unwrap_or(false),
                rsvp_status,
                rsvp_counts,
                likes_count,
                replies_count: *comment_counts.get(&r.uri).unwrap_or(&0) as u32,
                reaction_counts: if counts.is_empty() { None } else { Some(counts) },
                user_reaction: user_reactions.get(&r.uri).cloned(),
                provenance: provenance_view,
            }
        })
        .collect();
    Ok(out)
}

/// Resolve the event exists → (author_did, cid) or 404.
async fn load_event_meta(
    pool: &sqlx::PgPool,
    uri: &str,
) -> Result<(String, String), AppError> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT author_did, cid FROM calendar_events WHERE uri = $1")
            .bind(uri)
            .fetch_optional(pool)
            .await
            .map_err(AppError::from)?;
    row.ok_or_else(|| AppError::NotFound("Event not found".into()))
}

/// The local user (id, username) who owns an event, when the event author is a
/// local account. Used to route event-comment notifications.
async fn event_owner(
    pool: &sqlx::PgPool,
    uri: &str,
) -> Option<(Uuid, String)> {
    let author_did: Option<String> =
        sqlx::query_scalar("SELECT author_did FROM calendar_events WHERE uri = $1")
            .bind(uri)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    let author_did = author_did?;
    sqlx::query_as::<_, (Uuid, String)>(
        "SELECT u.id, u.username FROM users u \
         JOIN linked_accounts la ON la.user_id = u.id \
         WHERE la.provider = 'bluesky' AND la.external_id = $1 LIMIT 1",
    )
    .bind(&author_did)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

// ─── Read surfaces (spec: events/browse) ───────────────────────────────

async fn list_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<EventsQuery>,
) -> Result<Json<EventsFeedResponse>, AppError> {
    let viewer = resolve_viewer(&state, &headers).await;

    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let status = query.status.as_deref().unwrap_or("upcoming");

    // WHERE pieces. Base condition first (never starts with AND); time
    // filters bind against indexed columns only; the tsquery reuses the GIN
    // expression verbatim and location rides the trgm index (spec: filters
    // never degrade to unindexed scans).
    let mut wheres: Vec<String> = Vec::new();
    match status {
        "upcoming" => wheres.push("e.starts_at >= now()".to_string()),
        "past" => wheres.push("e.starts_at < now()".to_string()),
        _ => wheres.push("true".to_string()), // "all" — undated included
    }
    if let Some(ts) = query
        .cursor_starts_at
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
    {
        let _ = ts;
        match status {
            "upcoming" => wheres.push(
                "AND (e.starts_at, e.uri) > ($1::timestamptz, $2::text)".to_string(),
            ),
            "past" => wheres
                .push("AND (e.starts_at, e.uri) < ($1::timestamptz, $2::text)".to_string()),
            _ => {
                // "all" orders starts_at ASC NULLS LAST: a dated cursor still
                // has the whole undated block ahead of it.
                wheres.push(
                    "AND ((e.starts_at IS NOT NULL AND ((e.starts_at > $1::timestamptz) OR (e.starts_at = $1::timestamptz AND e.uri > $2::text))) OR e.starts_at IS NULL)"
                        .to_string(),
                );
            }
        }
    } else if query.cursor_uri.is_some() && status == "all" {
        // Cursor inside the undated block: after it, only later undated rows.
        wheres.push("AND e.starts_at IS NULL AND e.uri > $2::text".to_string());
    }
    if query.q.as_deref().is_some_and(|s| !s.trim().is_empty()) {
        wheres.push(
            "AND to_tsvector('english', coalesce(e.name,'') || ' ' || coalesce(e.description,'')) @@ plainto_tsquery('english', $3)"
                .to_string(),
        );
    }
    if query.date_from.is_some() {
        wheres.push("AND e.starts_at >= $4::timestamptz".to_string());
    }
    if query.date_to.is_some() {
        wheres.push("AND e.starts_at < $5::timestamptz".to_string());
    }
    if query.location.as_deref().is_some_and(|s| !s.trim().is_empty()) {
        wheres.push("AND e.location_text ILIKE '%' || $6::text || '%'".to_string());
    }
    // Source tab filter. Home = no additional predicate; the other tabs
    // resolve the viewer's graph/subscriptions server-side (the client only
    // sends a scope id from /api/events/sources).
    let source = query.source.as_deref().unwrap_or("home");
    let mut source_terms: Vec<String> = Vec::new();
    // Home binds ($8..$10): follows, feed topics, hashtag patterns. Set only
    // for the home/unknown branch below.
    let mut home_binds: Option<(Vec<String>, Vec<String>, Vec<String>)> = None;
    match source {
        "following" => {
            if let Some((uid, _)) = viewer.as_ref() {
                let mut follows = viewer_follow_dids(&state, *uid).await;
                if let Some(one) = query.followed_did.as_deref() {
                    if follows.iter().any(|d| d == one) {
                        follows = vec![one.to_string()];
                    } else {
                        follows.clear();
                    }
                }
                if follows.is_empty() {
                    wheres.push("AND false".to_string());
                } else {
                    // Latest RSVP per (event, author) wins — a changed response
                    // must not leave a stale going row matching.
                    wheres.push(
                        "AND EXISTS (SELECT 1 FROM (SELECT DISTINCT ON (r.event_uri, r.author_did) r.status FROM calendar_rsvps r WHERE r.event_uri = e.uri AND r.author_did = ANY($8::text[]) ORDER BY r.event_uri, r.author_did, r.record_created_at DESC NULLS LAST) latest WHERE latest.status IN ('community.lexicon.calendar.rsvp#going', 'community.lexicon.calendar.rsvp#interested'))"
                            .to_string(),
                    );
                    source_terms = follows;
                }
            } else {
                wheres.push("AND false".to_string());
            }
        }
        "feeds" => {
            if let Some((uid, _)) = viewer.as_ref() {
                let subs = SubscriptionRow::find_by_user_and_category(&state.db, *uid, "feeds")
                    .await
                    .unwrap_or_default();
                let subs: Vec<SubscriptionRow> = match query.feed_id.as_deref() {
                    Some(id) => subs.into_iter().filter(|s| s.id.to_string() == id).collect(),
                    None => subs,
                };
                let mut topics: Vec<String> = subs.iter().flat_map(feed_topic_ids).collect();
                topics.sort();
                topics.dedup();
                if topics.is_empty() {
                    wheres.push("AND false".to_string());
                } else {
                    wheres.push("AND e.topics && $8::text[]".to_string());
                    source_terms = topics;
                }
            } else {
                wheres.push("AND false".to_string());
            }
        }
        "hashtags" => {
            if let Some((uid, _)) = viewer.as_ref() {
                let subs = SubscriptionRow::find_by_user_and_category(&state.db, *uid, "hashtags")
                    .await
                    .unwrap_or_default();
                let subs: Vec<SubscriptionRow> = match query.hashtag_id.as_deref() {
                    Some(id) => subs.into_iter().filter(|s| s.id.to_string() == id).collect(),
                    None => subs,
                };
                let mut tags: Vec<String> = subs.iter().filter_map(subscription_tag).collect();
                tags.sort();
                tags.dedup();
                if tags.is_empty() {
                    wheres.push("AND false".to_string());
                } else {
                    // Literal `#tag` tokens only (crate::tags::sql_hashtag_pattern):
                    // event descriptions are long-form prose, so accepting the
                    // bare word made "a weekly practice space" claim `#space`
                    // (74 rows, none carrying the hashtag). This is the main
                    // feed's rule — its tag index only holds literal #tags.
                    // Never the topic family either: that bled #astrophotography
                    // into #astronomy events and #atproto into #activitypub.
                    wheres.push(
                        "AND (e.name ~* ANY($8::text[]) OR e.description ~* ANY($8::text[]))"
                            .to_string(),
                    );
                    source_terms = tags
                        .iter()
                        .map(|t| crate::tags::sql_hashtag_pattern(t))
                        .collect();
                }
            } else {
                wheres.push("AND false".to_string());
            }
        }
        _ => {
            // Home (and unknown sources): the union of the viewer's sources —
            // followed RSVPs, feed topic families, exact hashtags. Each side
            // of the OR is false on an empty array, so zero sources means an
            // empty list; unmatched network events never surface (spec:
            // source-only listings). Hashtags match a literal `#tag` token only
            // (crate::tags::sql_hashtag_pattern) — never a bare word and never a
            // topic family.
            let (follows, feed_topics, tag_patterns) = match viewer.as_ref() {
                Some((uid, _)) => {
                    let follows = viewer_follow_dids(&state, *uid).await;
                    let feed_subs =
                        SubscriptionRow::find_by_user_and_category(&state.db, *uid, "feeds")
                            .await
                            .unwrap_or_default();
                    let mut feed_topics: Vec<String> =
                        feed_subs.iter().flat_map(feed_topic_ids).collect();
                    feed_topics.sort();
                    feed_topics.dedup();
                    let hashtag_subs =
                        SubscriptionRow::find_by_user_and_category(&state.db, *uid, "hashtags")
                            .await
                            .unwrap_or_default();
                    let tag_patterns: Vec<String> = hashtag_subs
                        .iter()
                        .filter_map(subscription_tag)
                        .map(|t| crate::tags::sql_hashtag_pattern(&t))
                        .collect();
                    (follows, feed_topics, tag_patterns)
                }
                None => (Vec::new(), Vec::new(), Vec::new()),
            };
            wheres.push(
                "AND (EXISTS (SELECT 1 FROM (SELECT DISTINCT ON (r.event_uri, r.author_did) r.status FROM calendar_rsvps r WHERE r.event_uri = e.uri AND r.author_did = ANY($8::text[]) ORDER BY r.event_uri, r.author_did, r.record_created_at DESC NULLS LAST) latest WHERE latest.status IN ('community.lexicon.calendar.rsvp#going', 'community.lexicon.calendar.rsvp#interested')) OR e.topics && $9::text[] OR e.name ~* ANY($10::text[]) OR e.description ~* ANY($10::text[]))"
                    .to_string(),
            );
            home_binds = Some((follows, feed_topics, tag_patterns));
        }
    }

    let order = match status {
        "upcoming" => "ORDER BY e.starts_at ASC, e.record_created_at ASC NULLS LAST, e.uri ASC",
        "past" => "ORDER BY e.starts_at DESC, e.record_created_at DESC NULLS LAST, e.uri ASC",
        _ => "ORDER BY e.starts_at ASC NULLS LAST, e.record_created_at ASC NULLS LAST, e.uri ASC",
    };
    let sql = format!(
        "{} WHERE {} {} LIMIT $7",
        EVENT_SELECT,
        if wheres.is_empty() {
            "true".to_string()
        } else {
            wheres.join(" ")
        },
        order
    );

    let cursor_ts = query
        .cursor_starts_at
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));
    let cursor_uri = query.cursor_uri.clone().unwrap_or_default();
    let q_tsquery = query
        .q
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string());
    let date_from = query
        .date_from
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));
    let date_to = query
        .date_to
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));
    let location = query.location.clone().filter(|s| !s.trim().is_empty());

    let mut rows_query = sqlx::query_as::<_, EventRowLite>(&sql)
        .bind(cursor_ts)
        .bind(cursor_uri)
        .bind(q_tsquery)
        .bind(date_from)
        .bind(date_to)
        .bind(location)
        .bind(limit + 1);
    if let Some(binds) = &home_binds {
        rows_query = rows_query.bind(&binds.0).bind(&binds.1).bind(&binds.2);
    } else {
        rows_query = rows_query.bind(&source_terms);
    }
    let rows = rows_query
        .fetch_all(&state.db)
        .await
        .map_err(AppError::from)?;

    let has_more = rows.len() as i64 > limit;
    let rows: Vec<EventRowLite> = rows.into_iter().take(limit as usize).collect();

    let provenance = match viewer.as_ref() {
        Some((uid, _)) => Some(build_provenance_context(&state, *uid).await),
        None => None,
    };
    let items = hydrate(&state.db, rows, viewer, provenance.as_ref()).await?;
    Ok(Json(EventsFeedResponse { items, has_more }))
}

async fn event_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<UriQuery>,
) -> Result<Json<EventView>, AppError> {
    let sql = format!("{} WHERE e.uri = $1", EVENT_SELECT);
    let row = sqlx::query_as::<_, EventRowLite>(&sql)
        .bind(&query.uri)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound("Event not found".into()))?;

    let viewer = resolve_viewer(&state, &headers).await;
    let mut items = hydrate(&state.db, vec![row], viewer, None).await?;
    items
        .pop()
        .map(Json)
        .ok_or_else(|| AppError::NotFound("Event not found".into()))
}

async fn my_rsvps(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<EventsQuery>,
) -> Result<Json<EventsFeedResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let dids = own_dids(&state.db, user_id).await;
    if dids.is_empty() {
        return Err(AppError::BadRequest("no AT identity linked to this account".into()));
    }
    let limit = query.limit.unwrap_or(50).clamp(1, 100);

    // Latest RSVP per event for the viewer, newest first.
    let rsvp_uris: Vec<String> = sqlx::query_scalar(
        r#"SELECT DISTINCT ON (event_uri) event_uri
           FROM calendar_rsvps
           WHERE author_did = ANY($1)
           ORDER BY event_uri, COALESCE(record_created_at, indexed_at) DESC, uri DESC"#,
    )
    .bind(&dids)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::from)?;
    if rsvp_uris.is_empty() {
        return Ok(Json(EventsFeedResponse {
            items: Vec::new(),
            has_more: false,
        }));
    }

    let sql = format!("{} WHERE e.uri = ANY($1)", EVENT_SELECT);
    let mut rows = sqlx::query_as::<_, EventRowLite>(&sql)
        .bind(&rsvp_uris)
        .fetch_all(&state.db)
        .await
        .map_err(AppError::from)?;
    // Preserve the rsvp recency order from the first query.
    let pos: HashMap<&String, usize> = rsvp_uris.iter().enumerate().map(|(i, u)| (u, i)).collect();
    rows.sort_by_key(|r| pos.get(&r.uri).copied().unwrap_or(usize::MAX));
    rows.truncate(limit as usize);

    let items = hydrate(&state.db, rows, Some((user_id, dids)), None).await?;
    Ok(Json(EventsFeedResponse {
        items,
        has_more: false,
    }))
}

// ─── Source tabs (spec: events/browse — Home/Following/Feeds/Hashtags) ──

/// Map a feed subscription to the events topic index: the rkey in a
/// Bytescape topic feed's AT-URI, the feed's display name, or its label
/// tokens ("Cats & Dogs" → cats, dogs). Third-party feeds that don't map
/// contribute no topics — their Feeds row scopes to an empty list.
fn feed_topic_ids(sub: &SubscriptionRow) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let mut exact = false;
    if let Some(rkey) = sub.source_id.rsplit('/').next() {
        if let Some(t) = topic_for_interest(rkey) {
            ids.push(t.rkey.to_string());
            exact = true;
        }
    }
    let label = sub.label.trim();
    if let Some(t) = TOPICS.iter().find(|t| t.name.eq_ignore_ascii_case(label)) {
        ids.push(t.rkey.to_string());
        exact = true;
    }
    let normalized = label
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-");
    if let Some(t) = topic_for_interest(&normalized) {
        ids.push(t.rkey.to_string());
        exact = true;
    }
    // Only fall back to label tokens when no exact topic matched. Otherwise a
    // specific feed ("PC Gaming" → pc-gaming) also claims the broad "gaming"
    // topic and pollutes both the Feeds tab and provenance.
    if !exact {
        for token in label.to_lowercase().split(|c: char| !c.is_alphanumeric()) {
            if token.len() > 1 {
                if let Some(t) = topic_for_interest(token) {
                    ids.push(t.rkey.to_string());
                }
            }
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

fn subscription_tag(sub: &SubscriptionRow) -> Option<String> {
    // The label is the display form ("#cats"); source ids may carry a
    // provider prefix ("mastodon:cats"). Prefer the label, then the tail.
    let from_label = sub.label.trim().trim_start_matches('#').to_lowercase();
    let raw = if !from_label.is_empty() {
        from_label
    } else {
        let source = sub.source_id.trim().trim_start_matches('#').to_lowercase();
        source.rsplit(':').next().unwrap_or("").to_string()
    };
    if raw.is_empty() { None } else { Some(raw) }
}

/// Viewer-side provenance inputs, loaded once per list request. Matching
/// each event against these is in-memory, so the card chips cost no N+1.
struct ProvenanceContext {
    follows: std::collections::HashSet<String>,
    feeds: Vec<ProvenanceFeed>,
    hashtags: Vec<ProvenanceHashtag>,
}

struct ProvenanceFeed {
    label: String,
    topics: Vec<String>,
}

struct ProvenanceHashtag {
    label: String,
    tag: String,
}

/// The events chip truth: a hashtag subscription earns a chip only when the
/// event text carries the literal `#tag`. Chips and membership therefore share
/// one rule on this surface — a bare word in prose earns nothing. The main
/// feed keeps [`crate::tags::chip_claim`] for its own chips, unchanged.
fn literal_hashtag_chip<'a>(text: &str, hashtags: &'a [ProvenanceHashtag]) -> Option<&'a str> {
    hashtags
        .iter()
        .find(|hashtag| crate::tags::text_has_hashtag(text, &hashtag.tag))
        .map(|hashtag| hashtag.label.as_str())
}

async fn build_provenance_context(state: &AppState, user_id: Uuid) -> ProvenanceContext {
    let follows = viewer_follow_dids(state, user_id).await.into_iter().collect();
    let feed_subs = SubscriptionRow::find_by_user_and_category(&state.db, user_id, "feeds")
        .await
        .unwrap_or_default();
    let feeds = feed_subs
        .iter()
        .map(|s| ProvenanceFeed {
            label: s.label.clone(),
            topics: feed_topic_ids(s),
        })
        .filter(|f| !f.topics.is_empty())
        .collect();
    let hashtag_subs = SubscriptionRow::find_by_user_and_category(&state.db, user_id, "hashtags")
        .await
        .unwrap_or_default();
    let hashtags = hashtag_subs
        .iter()
        .filter_map(|s| {
            let tag = subscription_tag(s)?;
            Some(ProvenanceHashtag {
                label: s.label.clone(),
                tag,
            })
        })
        .collect();
    ProvenanceContext {
        follows,
        feeds,
        hashtags,
    }
}

/// The viewer's real follow graph: Bluesky follows (cached, refreshed) plus
/// local follows resolved to DIDs. The Following tab and the sources panel
/// share this one definition.
async fn viewer_follow_dids(state: &AppState, user_id: Uuid) -> Vec<String> {
    let http = crate::api::feed::feed_http();
    let mut accounts = LinkedAccountRow::find_by_user(&state.db, user_id)
        .await
        .unwrap_or_default();
    for acct in accounts.iter_mut() {
        if let Some(tok) = crate::api::feed::refresh_if_expired(&state.db, &http, acct).await {
            acct.access_token = tok;
        }
    }
    let mut dids: std::collections::HashSet<String> =
        match accounts.iter().find(|a| a.provider == "bluesky") {
            Some(acct) => crate::oauth::bluesky::get_viewer_follows(
                &http,
                &acct.access_token,
                &acct.external_id,
                acct.dpop_private_key.as_deref(),
            )
            .await
            .unwrap_or_default(),
            None => Default::default(),
        };
    let local = FollowRow::list_by_user(&state.db, user_id)
        .await
        .unwrap_or_default();
    let handles: Vec<String> = local
        .iter()
        .filter_map(|f| {
            let actor = f.followed_actor.trim();
            let handle = if actor.starts_with("did:") {
                actor.to_string()
            } else {
                actor.rsplit('/').next().unwrap_or("").to_lowercase()
            };
            if handle.is_empty() { None } else { Some(handle) }
        })
        .collect();
    if !handles.is_empty() {
        let (local_dids, _) =
            crate::api::feed::sources::resolve_followed_dids(&state.db, &handles).await;
        for did in local_dids {
            dids.insert(did);
        }
    }
    let mut out: Vec<String> = dids.into_iter().collect();
    out.sort();
    out
}

#[derive(Debug, Serialize)]
pub struct SourcePerson {
    pub did: String,
    pub handle: String,
    pub display_name: Option<String>,
    pub avatar: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SourceFeed {
    pub id: String,
    pub label: String,
    pub source_id: String,
    pub icon: String,
    pub topics: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SourceHashtag {
    pub id: String,
    pub label: String,
    pub source_id: String,
    pub icon: String,
    pub tag: String,
}

#[derive(Debug, Serialize)]
pub struct EventSourcesResponse {
    pub following: Vec<SourcePerson>,
    /// Total follows (the panel list is capped; the tab filter uses all).
    pub following_total: usize,
    pub feeds: Vec<SourceFeed>,
    pub hashtags: Vec<SourceHashtag>,
}

/// The right-rail browsing panel: the viewer's follows, feed subscriptions,
/// and hashtag subscriptions. Anonymous → empty groups.
async fn event_sources(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<EventSourcesResponse>, AppError> {
    let Some(user_id) = optional_viewer(&headers, &state.config.jwt_secret) else {
        return Ok(Json(EventSourcesResponse {
            following: Vec::new(),
            following_total: 0,
            feeds: Vec::new(),
            hashtags: Vec::new(),
        }));
    };
    let mut dids = viewer_follow_dids(&state, user_id).await;
    let following_total = dids.len();
    dids.truncate(200);
    let identities = resolve_event_identities(&state.db, &dids).await;
    let mut following: Vec<SourcePerson> = dids
        .iter()
        .map(|did| {
            let (handle, display_name, avatar) = identities
                .get(did)
                .cloned()
                .unwrap_or_else(|| (did.clone(), None, None));
            SourcePerson {
                did: did.clone(),
                handle,
                display_name,
                avatar,
            }
        })
        .collect();
    following.sort_by(|a, b| a.handle.to_lowercase().cmp(&b.handle.to_lowercase()));
    following.truncate(200);

    let feeds = SubscriptionRow::find_by_user_and_category(&state.db, user_id, "feeds")
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|s| {
            let topics = feed_topic_ids(&s);
            SourceFeed {
                id: s.id.to_string(),
                label: s.label,
                source_id: s.source_id,
                icon: s.icon,
                topics,
            }
        })
        .collect();
    let hashtags = SubscriptionRow::find_by_user_and_category(&state.db, user_id, "hashtags")
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|s| {
            let tag = subscription_tag(&s)?;
            Some(SourceHashtag {
                id: s.id.to_string(),
                label: s.label,
                source_id: s.source_id,
                icon: s.icon,
                tag,
            })
        })
        .collect();

    Ok(Json(EventSourcesResponse {
        following,
        following_total,
        feeds,
        hashtags,
    }))
}

// ─── Write surfaces (spec: events/participate, events/records) ─────────

async fn upload_event_image(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    let _user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let url = crate::api::profile::handle_image_upload(
        &state,
        &mut multipart,
        "events",
        10 * 1024 * 1024,
    )
    .await?;
    Ok(Json(serde_json::json!({ "url": url })))
}

async fn create_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateEventRequest>,
) -> Result<Json<EventView>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    // Field-level validation before any write (spec: validation errors
    // reject with a message and write no record).
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    let parse_ts = |label: &str, v: Option<&String>| -> Result<Option<String>, AppError> {
        match v.map(|s| s.trim()).filter(|s| !s.is_empty()) {
            None => Ok(None),
            Some(s) => chrono::DateTime::parse_from_rfc3339(s)
                .map(|dt| Some(dt.with_timezone(&chrono::Utc).to_rfc3339()))
                .map_err(|_| AppError::BadRequest(format!("{} must be RFC3339", label))),
        }
    };
    let starts_at = parse_ts("starts_at", body.starts_at.as_ref())?;
    let ends_at = parse_ts("ends_at", body.ends_at.as_ref())?;
    if let (Some(s), Some(e)) = (&starts_at, &ends_at) {
        if e < s {
            return Err(AppError::BadRequest(
                "end time must be after start time".into(),
            ));
        }
    }
    let mode = body
        .mode
        .as_deref()
        .map(|m| m.trim())
        .filter(|m| !m.is_empty())
        .map(|m| {
            if m.starts_with(calendar::EVENT_NSID) {
                Ok(m.to_string())
            } else {
                match m {
                    "inperson" | "virtual" | "hybrid" => {
                        Ok(format!("{}#{}", calendar::EVENT_NSID, m))
                    }
                    _ => Err(AppError::BadRequest(
                        "mode must be inperson, virtual, or hybrid".into(),
                    )),
                }
            }
        })
        .transpose()?;

    let id = at_identity_for(&state.db, user_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("no AT identity linked to this account".into()))?;
    let author_did = match &id {
        AtId::Native(d) => d.clone(),
        AtId::Linked(a) => a.external_id.clone(),
    };

    let now = chrono::Utc::now();
    let mut record = serde_json::json!({
        "$type": calendar::EVENT_NSID,
        "name": name,
        "createdAt": now.to_rfc3339(),
    });
    if let Some(d) = body.description.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        record["description"] = serde_json::Value::String(d.replace('\u{0}', ""));
    }
    if let Some(s) = starts_at {
        record["startsAt"] = serde_json::Value::String(s);
    }
    if let Some(e) = ends_at {
        record["endsAt"] = serde_json::Value::String(e);
    }
    if let Some(m) = mode {
        record["mode"] = serde_json::Value::String(m);
    }
    let mut locations: Vec<serde_json::Value> = Vec::new();
    if let Some(loc) = body.location.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        // event#uri {uri, name}: a named map-search link — the address
        // sub-lexicon demands structured country data a free-text venue
        // cannot honestly provide, so the string travels as the display name.
        locations.push(serde_json::json!({
            "uri": format!("https://www.openstreetmap.org/search?query={}", urlencoding::encode(loc)),
            "name": loc.replace('\u{0}', ""),
        }));
    }
    if let Some(pic) = body.picture_url.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        record["uris"] = serde_json::json!([
            {"uri": pic.replace('\u{0}', ""), "name": "image"}
        ]);
    }
    if !locations.is_empty() {
        record["locations"] = serde_json::Value::Array(locations);
    }

    let rkey = {
        use rsky_common::tid::Ticker;
        Ticker::new().next(None).0
    };
    write_record(&state.db, &state, &id, calendar::EVENT_NSID, &rkey, &record).await?;
    let uri = format!("at://{}/{}/{}", author_did, calendar::EVENT_NSID, rkey);

    // Write-through (spec: read-your-writes — no firehose wait).
    let parsed = calendar::parse_event(&record, calendar::EVENT_NSID)
        .ok_or_else(|| AppError::Internal("built record failed its own parse".into()))?;
    calendar::upsert_event(&state.db, &uri, &record_cid(&record), &author_did, parsed, "atproto").await
        .map_err(AppError::from)?;

    // Respond with the same hydrated shape the list/detail surfaces serve.
    let sql = format!("{} WHERE e.uri = $1", EVENT_SELECT);
    let row = sqlx::query_as::<_, EventRowLite>(&sql)
        .bind(&uri)
        .fetch_one(&state.db)
        .await
        .map_err(AppError::from)?;
    let viewer_dids = own_dids(&state.db, user_id).await;
    let mut items = hydrate(&state.db, vec![row], Some((user_id, viewer_dids)), None).await?;
    items
        .pop()
        .map(Json)
        .ok_or_else(|| AppError::Internal("created event missing from index".into()))
}

async fn rsvp_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RsvpRequest>,
) -> Result<Json<RsvpResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let status = calendar::rsvp_status_to_lexicon(&body.status)
        .ok_or_else(|| AppError::BadRequest("status must be going, maybe, or not_going".into()))?;
    let (author_did, event_cid) = load_event_meta(&state.db, &body.uri).await?;
    let _ = author_did;

    let id = at_identity_for(&state.db, user_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("no AT identity linked to this account".into()))?;

    // Spec: strongRef (uri + cid) to the event; deterministic rkey replaces.
    let record = serde_json::json!({
        "$type": calendar::RSVP_NSID,
        "subject": {"uri": body.uri, "cid": event_cid},
        "status": status,
    });
    let rkey = rsvp_rkey(&body.uri);
    write_record(&state.db, &state, &id, calendar::RSVP_NSID, &rkey, &record).await?;
    let rsvp_uri = match &id {
        AtId::Native(d) => format!("at://{}/{}/{}", d, calendar::RSVP_NSID, rkey),
        AtId::Linked(a) => format!("at://{}/{}/{}", a.external_id, calendar::RSVP_NSID, rkey),
    };

    calendar::upsert_rsvp(
        &state.db,
        &rsvp_uri,
        match &id {
            AtId::Native(d) => d.as_str(),
            AtId::Linked(a) => a.external_id.as_str(),
        },
        calendar::ParsedRsvp {
            subject_uri: body.uri.clone(),
            status: status.to_string(),
            record_created_at: Some(chrono::Utc::now()),
        },
    )
    .await
    .map_err(AppError::from)?;

    let counts = calendar::rsvp_counts(&state.db, &body.uri).await?;
    Ok(Json(RsvpResponse {
        uri: rsvp_uri,
        status: calendar::rsvp_status_to_ui(status).to_string(),
        rsvp_counts: counts,
    }))
}

async fn remove_rsvp(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<UriQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let id = at_identity_for(&state.db, user_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("no AT identity linked to this account".into()))?;
    let (did, rkey) = match &id {
        AtId::Native(d) => (d.clone(), rsvp_rkey(&query.uri)),
        AtId::Linked(a) => (a.external_id.clone(), rsvp_rkey(&query.uri)),
    };
    // Deletes only ever target the caller's own repo — a foreign RSVP's
    // index row cannot be touched through this path.
    let gone = delete_author_record(&state.db, &did, calendar::RSVP_NSID, &rkey).await;
    if gone {
        calendar::delete_rsvp(&state.db, &format!("at://{}/{}/{}", did, calendar::RSVP_NSID, rkey))
            .await
            .map_err(AppError::from)?;
    }
    Ok(Json(serde_json::json!({ "deleted": gone })))
}

/// Author-only: the record must exist, the caller's DID set must contain the
/// record's author, and the delete hits the author's PDS first — the index
/// row follows only when the record is really gone (no phantom state).
async fn delete_event_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<UriQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let (did, collection, rkey) = split_at_uri(&query.uri)
        .ok_or_else(|| AppError::BadRequest("uri must be an at:// URI".into()))?;
    let (author_did, _) = load_event_meta(&state.db, &query.uri).await?;

    let viewer_dids = own_dids(&state.db, user_id).await;
    if !viewer_dids.contains(&author_did) {
        // Spec: denied regardless of any local row — DID match only.
        return Err(AppError::Forbidden(
            "Only the event's author can delete it".into(),
        ));
    }
    let gone = delete_author_record(&state.db, &did, &collection, &rkey).await;
    if !gone {
        return Err(AppError::Internal(
            "could not delete the record on the author's PDS".into(),
        ));
    }
    calendar::delete_event(&state.db, &query.uri).await.map_err(AppError::from)?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

// ─── Reactions + comments (retained, re-keyed to at-URIs) ──────────────

async fn add_reaction(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ReactionRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let (_author_did, cid) = load_event_meta(&state.db, &body.uri).await?;

    // Route the reaction through the same shared core as the main feed (user
    // direction 2026-11-13: "this logic holds across both the main feed and
    // the events feed") — URI-scheme routed, on-demand identity,
    // cross-protocol. Converged: one Like stands regardless of emoji changes.
    let is_negative = crate::api::feed::is_negative_reaction(&body.emoji);
    let existing_rkey = db::reactions::ReactionRow::find(&state.db, &body.uri, user_id)
        .await?
        .and_then(|r| r.metadata["bluesky_like_rkey"].as_str().map(str::to_string));
    let result = crate::api::feed::sync_federated_like(
        "",
        &state,
        &body.uri,
        is_negative,
        user_id,
        Some(&cid),
        existing_rkey.as_deref(),
    )
    .await?;
    db::reactions::ReactionRow::upsert(&state.db, &body.uri, user_id, &body.emoji).await?;
    if let Some((rkey, identity)) = result {
        db::reactions::update_metadata(
            &state.db,
            user_id,
            &body.uri,
            "bluesky",
            serde_json::json!({"bluesky_like_rkey": rkey, "like_identity": identity}),
        )
        .await?;
    }
    let counts =
        db::reactions::ReactionRow::counts_for_posts(&state.db, std::slice::from_ref(&body.uri))
            .await
            .unwrap_or_default();
    let user_reaction = db::reactions::ReactionRow::user_reactions_for_posts(
        &state.db,
        std::slice::from_ref(&body.uri),
        user_id,
    )
    .await
    .unwrap_or_default();
    Ok(Json(serde_json::json!({
        "reaction_counts": counts.get(&body.uri).cloned(),
        "user_reaction": user_reaction.get(&body.uri).cloned(),
    })))
}

async fn remove_reaction(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<UriQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    // Undo the federated reaction first — it reads the metadata row that
    // remove is about to delete.
    crate::api::feed::perform_platform_unreact("", &state, &query.uri, false, None, user_id).await?;
    db::reactions::ReactionRow::delete(&state.db, &query.uri, user_id).await?;
    let counts =
        db::reactions::ReactionRow::counts_for_posts(&state.db, std::slice::from_ref(&query.uri))
            .await
            .unwrap_or_default();
    let user_reaction = db::reactions::ReactionRow::user_reactions_for_posts(
        &state.db,
        std::slice::from_ref(&query.uri),
        user_id,
    )
    .await
    .unwrap_or_default();
    Ok(Json(serde_json::json!({
        "reaction_counts": counts.get(&query.uri).cloned(),
        "user_reaction": user_reaction.get(&query.uri).cloned(),
    })))
}

async fn comment_author(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> AuthorView {
    sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT username, display_name FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map(|(username, display_name)| AuthorView {
        did: String::new(),
        handle: username.clone(),
        display_name: display_name.or(Some(username)),
        avatar: None,
    })
    .unwrap_or(AuthorView {
        did: String::new(),
        handle: "unknown".into(),
        display_name: Some("Unknown".into()),
        avatar: None,
    })
}

async fn list_comments(
    State(state): State<AppState>,
    Query(query): Query<CommentsQuery>,
) -> Result<Json<Vec<EventCommentView>>, AppError> {
    load_event_meta(&state.db, &query.uri).await?;
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);
    let rows =
        db::event_comments::EventCommentRow::list_for_event(&state.db, &query.uri, limit, offset)
            .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let author = comment_author(&state.db, row.user_id).await;
        out.push(EventCommentView {
            id: row.id.to_string(),
            content: row.content,
            created_at: row.created_at.to_rfc3339(),
            author,
        });
    }
    Ok(Json(out))
}

async fn post_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CommentRequest>,
) -> Result<Json<EventCommentView>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    load_event_meta(&state.db, &body.uri).await?;
    let content = body.content.trim().replace('\u{0}', "");
    if content.is_empty() {
        return Err(AppError::BadRequest("content is required".into()));
    }
    let row =
        db::event_comments::EventCommentRow::create(&state.db, &body.uri, user_id, &content)
            .await?;
    let author = comment_author(&state.db, user_id).await;

    // Notify the event owner of the comment (spec: comment on owner's content).
    if let Some((owner_id, owner_name)) = event_owner(&state.db, &body.uri).await {
        if owner_id != user_id {
            // Provenance: the ACTOR of a comment notification is the
            // commenter, never the recipient (identity-graph P5 audit).
            let _ = crate::ap::notify::notify(
                &state.db,
                owner_id,
                crate::ap::notify::TYPE_COMMENT,
                "New event comment",
                Some(&format!("{} commented on your event", author.handle)),
                Some(serde_json::json!({ "event_uri": body.uri })),
                &format!("event-comment:{}", row.id),
                &crate::ap::notify::ActorRef::local(&author.handle),
            )
            .await;
        }
    }
    Ok(Json(EventCommentView {
        id: row.id.to_string(),
        content: row.content,
        created_at: row.created_at.to_rfc3339(),
        author,
    }))
}

async fn delete_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DeleteCommentQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let comment_id =
        Uuid::parse_str(&query.id).map_err(|_| AppError::BadRequest("Invalid comment ID".into()))?;
    db::event_comments::EventCommentRow::delete(&state.db, comment_id, user_id).await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

#[cfg(test)]
mod provenance_tests {
    use super::{literal_hashtag_chip, ProvenanceHashtag};

    fn subs(labels: &[&str]) -> Vec<ProvenanceHashtag> {
        labels
            .iter()
            .map(|label| ProvenanceHashtag {
                label: label.to_string(),
                tag: label.trim_start_matches('#').to_lowercase(),
            })
            .collect()
    }

    /// Chip truth on events is literal: the word "space" in prose earns no
    /// chip; only the `#space` token does.
    #[test]
    fn chips_require_a_literal_hashtag() {
        let hashtags = subs(&["#space"]);
        assert_eq!(
            literal_hashtag_chip(" a weekly practice space for everyone ", &hashtags),
            None
        );
        assert_eq!(
            literal_hashtag_chip(" join us #space tonight ", &hashtags),
            Some("#space")
        );
    }

    /// No subscription, no chip; and a longer hashtag never claims a shorter
    /// subscription.
    #[test]
    fn chips_are_token_exact() {
        assert_eq!(literal_hashtag_chip("join us #space tonight", &[]), None);
        let hashtags = subs(&["#tech"]);
        assert_eq!(literal_hashtag_chip("dancing to #techno", &hashtags), None);
        assert_eq!(literal_hashtag_chip("see #tech_x", &hashtags), None);
        assert_eq!(literal_hashtag_chip("see #tech", &hashtags), Some("#tech"));
    }
}
