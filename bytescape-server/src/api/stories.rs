//! API — Stories: scoped rail, explorer, own stories, seen state.
//!
//! All reads are flat-column queries over `appview_stories` / `appview_status`
//! with the social window enforced at query level: scheduled stories
//! (`active_from` in the future) and expired stories never leave here.

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use sqlx::PgPool;
use std::collections::HashMap;
use tracing::warn;
use uuid::Uuid;

use crate::api::feed::sources::{get_followed_handles, resolve_followed_dids};
use crate::api::follow::extract_user_id;
use crate::{error::AppError, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/stories/rail", get(rail_handler))
        .route("/api/stories/explorer", get(explorer_handler))
        .route("/api/stories/mine", get(mine_handler))
        .route("/api/stories/seen", post(seen_handler))
        .route("/api/stories/item", get(story_item_handler))
        .route("/api/stories", post(create_story_handler).delete(delete_story_handler))
        .route("/api/stories/status", post(set_status_handler))
        .route("/api/stories/archive/media", get(archive_media_handler))
        .route("/api/stories/archive", delete(archive_delete_handler))
}

// === Response shapes (flat sortable fields per spec) ===

#[derive(Debug, Serialize, Clone, Default)]
pub struct AuthorInfo {
    pub did: String,
    pub handle: String,
    pub display_name: Option<String>,
    pub avatar: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct StoryItem {
    pub kind: String, // "story" | "status"
    pub uri: String,
    pub author_did: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub active_from: Option<String>,
    pub pinned: bool,
    pub text: Option<String>,
    pub media: Option<serde_json::Value>,
    pub music_url: Option<String>,
    /// Bluesky "go live" status — label honestly and link out to the stream.
    pub is_live: bool,
    pub style: Option<serde_json::Value>,
    pub can_react: bool,
    pub can_reply: bool,
    pub like_count: i64,
    pub repost_count: i64,
    pub reply_count: i64,
    /// Only on /api/stories/mine: active | scheduled | never | archived.
    pub state: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StoryGroup {
    pub author: AuthorInfo,
    pub seen: bool,
    pub items: Vec<StoryItem>,
}

#[derive(Debug, Serialize)]
pub struct RailResponse {
    pub groups: Vec<StoryGroup>,
    /// Federated video moments in the viewer's interest/hashtag scope — the
    /// Loops-style rail's population source while stories grow (owner: the
    /// rail must POPULATE; video posts are what actually exist today).
    pub videos: Vec<crate::api::feed::FeedPost>,
}

#[derive(Debug, Serialize)]
pub struct ExplorerResponse {
    pub groups: Vec<StoryGroup>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct MineResponse {
    pub items: Vec<StoryItem>,
}

// === Circle and author resolution ===

/// The viewer's circle: followed + matched users' DIDs.
async fn circle_dids(db: &PgPool, user_id: Uuid, domain: &str) -> Vec<String> {
    let handles = get_followed_handles(db, user_id, domain).await;
    let (mut dids, _map) = resolve_followed_dids(db, &handles).await;

    // Matched friends (imported-contact matches) contribute their linked or
    // native DIDs.
    let matched: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT CASE WHEN user_a_id = $1 THEN user_b_id ELSE user_a_id END
         FROM social_graph_matches WHERE user_a_id = $1 OR user_b_id = $1",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
    .unwrap_or_default();

    if !matched.is_empty() {
        let linked: Vec<Option<String>> = sqlx::query_scalar(
            "SELECT external_id FROM linked_accounts WHERE user_id = ANY($1) AND external_id IS NOT NULL",
        )
        .bind(&matched)
        .fetch_all(db)
        .await
        .unwrap_or_default();
        let native: Vec<Option<String>> = sqlx::query_scalar(
            "SELECT did FROM pds_accounts WHERE user_id = ANY($1)",
        )
        .bind(&matched)
        .fetch_all(db)
        .await
        .unwrap_or_default();
        dids.extend(linked.into_iter().flatten());
        dids.extend(native.into_iter().flatten());
    }

    dids.sort();
    dids.dedup();
    dids
}

/// The viewer's own DIDs (linked Bluesky and/or native PDS).
pub(crate) async fn own_dids(db: &PgPool, user_id: Uuid) -> Vec<String> {
    let mut dids: Vec<String> = sqlx::query_scalar(
        "SELECT external_id FROM linked_accounts WHERE user_id = $1 AND external_id IS NOT NULL",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
    .unwrap_or_default();
    let native: Vec<String> = sqlx::query_scalar("SELECT did FROM pds_accounts WHERE user_id = $1")
        .bind(user_id)
        .fetch_all(db)
        .await
        .unwrap_or_default();
    dids.extend(native);
    dids.sort();
    dids.dedup();
    dids
}

/// Best-effort display info per author: the did→handle map plus the
/// denormalized profile columns off the author's posts. Handles fall back to
/// the DID (the display convention).
async fn author_info(db: &PgPool, dids: &[String]) -> HashMap<String, AuthorInfo> {
    let mut out: HashMap<String, AuthorInfo> = HashMap::new();
    for d in dids {
        out.insert(
            d.clone(),
            AuthorInfo { did: d.clone(), handle: d.clone(), display_name: None, avatar: None },
        );
    }
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT did, handle FROM appview_did_handle WHERE did = ANY($1)",
    )
    .bind(dids)
    .fetch_all(db)
    .await
    .unwrap_or_default();
    for (did, handle) in rows {
        if let Some(a) = out.get_mut(&did) {
            a.handle = handle;
        }
    }
    let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT DISTINCT ON (author_did) author_did, author_display_name, author_avatar
         FROM appview_posts WHERE author_did = ANY($1)",
    )
    .bind(dids)
    .fetch_all(db)
    .await
    .unwrap_or_default();
    for (did, display_name, avatar) in rows {
        if let Some(a) = out.get_mut(&did) {
            if a.display_name.is_none() {
                a.display_name = display_name;
            }
            if a.avatar.is_none() {
                a.avatar = avatar;
            }
        }
    }
    // Status-only authors have no ingested posts; fall back to the cached
    // profile, backfilling from Bluesky's public profile API when missing.
    let missing: Vec<String> = out
        .iter()
        .filter(|(_, a)| a.avatar.is_none() || a.display_name.is_none())
        .map(|(d, _)| d.clone())
        .collect();
    if !missing.is_empty() {
        let cached: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT did, display_name, avatar FROM appview_did_handle WHERE did = ANY($1) AND (display_name IS NOT NULL OR avatar IS NOT NULL)",
        )
        .bind(&missing)
        .fetch_all(db)
        .await
        .unwrap_or_default();
        for (did, display_name, avatar) in cached {
            if let Some(a) = out.get_mut(&did) {
                if a.display_name.is_none() {
                    a.display_name = display_name;
                }
                if a.avatar.is_none() {
                    a.avatar = avatar;
                }
            }
        }
        let still_missing: Vec<String> = out
            .iter()
            .filter(|(_, a)| a.avatar.is_none() || a.display_name.is_none())
            .map(|(d, _)| d.clone())
            .collect();
        if !still_missing.is_empty() {
            // Batch through the shared resolver (parallel, persisted, with a
            // negative cache) — the old serial fetch_bluesky_profile loop
            // paid an 8s timeout per missing author inside the request.
            let resolved =
                crate::api::feed::sources::resolve_profiles_remote(db, &still_missing).await;
            for (did, (handle, display_name, avatar)) in resolved {
                if let Some(a) = out.get_mut(&did) {
                    if a.handle == did {
                        a.handle = handle;
                    }
                    if a.display_name.is_none() {
                        a.display_name = display_name;
                    }
                    if a.avatar.is_none() {
                        a.avatar = avatar;
                    }
                }
            }
        }
    }
    out
}

/// Bytescape-side status-card backgrounds keyed by author DID.
async fn status_styles(db: &PgPool, dids: &[String]) -> HashMap<String, serde_json::Value> {
    let rows: Vec<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT la.external_id, u.status_style FROM users u
         JOIN linked_accounts la ON la.user_id = u.id
         WHERE la.external_id = ANY($1) AND u.status_style IS NOT NULL
         UNION ALL
         SELECT p.did, u.status_style FROM users u
         JOIN pds_accounts p ON p.user_id = u.id
         WHERE p.did = ANY($1) AND u.status_style IS NOT NULL",
    )
    .bind(dids)
    .bind(dids)
    .fetch_all(db)
    .await
    .unwrap_or_default();
    rows.into_iter().collect()
}

// === Assembly (pure) ===

fn rfc3339(ts: chrono::DateTime<chrono::Utc>) -> String {
    ts.to_rfc3339()
}

/// Order items within a group: pinned first (newest first among pinned), then
/// unpinned by recency. Status cards sort by their updated moment.
fn order_items(items: &mut Vec<StoryItem>) {
    items.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then_with(|| b.created_at.cmp(&a.created_at))
    });
}

/// Suppress mirror-duplicate status cards: a status whose text is identical
/// to an active story by the same author is that story's caption mirror.
fn dedupe_mirror(items: &mut Vec<StoryItem>) {
    let story_texts: Vec<String> = items
        .iter()
        .filter(|i| i.kind == "story")
        .filter_map(|i| i.text.clone())
        .collect();
    items.retain(|i| {
        i.kind != "status"
            || i.text
                .as_ref()
                .map(|t| !story_texts.contains(t))
                .unwrap_or(true)
    });
}

fn story_state(item: &StoryItem) -> &'static str {
    if item.state.as_deref() == Some("archived") {
        return "archived";
    }
    let now = chrono::Utc::now().to_rfc3339();
    if let Some(af) = &item.active_from {
        if af > &now {
            return "scheduled";
        }
    }
    match &item.expires_at {
        None => "never",
        Some(exp) if exp <= &now => "expired",
        Some(_) => "active",
    }
}

/// Group flat items by author. Groups order by most recent item; items are
/// ordered and mirror-deduped per spec. `seen_uris` marks fully-seen groups.
fn assemble_groups(
    items: Vec<StoryItem>,
    authors: &HashMap<String, AuthorInfo>,
    seen_uris: &std::collections::HashSet<String>,
) -> Vec<StoryGroup> {
    let mut by_author: HashMap<String, Vec<StoryItem>> = HashMap::new();
    for item in items {
        by_author.entry(item.author_did.clone()).or_default().push(item);
    }
    let mut groups: Vec<StoryGroup> = by_author
        .into_iter()
        .map(|(did, mut items)| {
            dedupe_mirror(&mut items);
            order_items(&mut items);
            let seen = !items.is_empty() && items.iter().all(|i| seen_uris.contains(&i.uri));
            StoryGroup {
                author: authors.get(&did).cloned().unwrap_or(AuthorInfo {
                    did: did.clone(),
                    handle: did,
                    display_name: None,
                    avatar: None,
                }),
                seen,
                items,
            }
        })
        .filter(|g| !g.items.is_empty())
        .collect();
    groups.sort_by(|a, b| {
        let ka = a.items.first().map(|i| i.created_at.clone()).unwrap_or_default();
        let kb = b.items.first().map(|i| i.created_at.clone()).unwrap_or_default();
        kb.cmp(&ka)
    });
    groups
}

// === Row fetching ===

#[derive(Debug, sqlx::FromRow)]
struct StoryRow {
    uri: String,
    author_did: String,
    created_at: chrono::DateTime<chrono::Utc>,
    active_from: Option<chrono::DateTime<chrono::Utc>>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pinned: bool,
    text: Option<String>,
    media: Option<serde_json::Value>,
    can_react: bool,
    can_reply: bool,
    like_count: i64,
    repost_count: i64,
    reply_count: i64,
    archived: bool,
    archive_media: Option<serde_json::Value>,
}

fn story_row_to_item(row: StoryRow) -> StoryItem {
    let mut item = StoryItem {
        kind: "story".into(),
        uri: row.uri,
        author_did: row.author_did,
        created_at: rfc3339(row.created_at),
        expires_at: row.expires_at.map(rfc3339),
        active_from: row.active_from.map(rfc3339),
        pinned: row.pinned,
        text: row.text,
        media: if row.archived { row.archive_media.or(row.media) } else { row.media },
        music_url: None,
        is_live: false,
        style: None,
        can_react: row.can_react,
        can_reply: row.can_reply,
        like_count: row.like_count,
        repost_count: row.repost_count,
        reply_count: row.reply_count,
        state: Some(if row.archived { "archived" } else { "active" }.into()),
    };
    if !row.archived {
        item.state = Some(story_state(&item).into());
    }
    item
}

const STORY_COLUMNS: &str = "uri, author_did, created_at, active_from, expires_at, pinned, text, media, can_react, can_reply, like_count, repost_count, reply_count, archived, archive_media";

/// Active stories (social window enforced at query level) for authors.
async fn fetch_active_stories(db: &PgPool, dids: &[String]) -> Result<Vec<StoryItem>, AppError> {
    let rows: Vec<StoryRow> = sqlx::query_as::<_, StoryRow>(&format!(
        "SELECT {STORY_COLUMNS} FROM appview_stories
         WHERE author_did = ANY($1)
           AND NOT archived
           AND (active_from IS NULL OR active_from <= now())
           AND (expires_at IS NULL OR expires_at > now())
         ORDER BY author_did, pinned DESC, created_at DESC"
    ))
    .bind(dids)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(story_row_to_item).collect())
}

/// All stories for authors regardless of window (own-stories view).
async fn fetch_all_stories(db: &PgPool, dids: &[String]) -> Result<Vec<StoryItem>, AppError> {
    let rows: Vec<StoryRow> = sqlx::query_as::<_, StoryRow>(&format!(
        "SELECT {STORY_COLUMNS} FROM appview_stories
         WHERE author_did = ANY($1)
         ORDER BY created_at DESC"
    ))
    .bind(dids)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(story_row_to_item).collect())
}

/// Active status cards (one per author, self-keyed) as story items.
async fn fetch_active_statuses(
    db: &PgPool,
    dids: &[String],
    styles: &HashMap<String, serde_json::Value>,
) -> Result<Vec<StoryItem>, AppError> {
    let rows: Vec<(String, Option<String>, Option<String>, chrono::DateTime<chrono::Utc>, i64, i64, i64, chrono::DateTime<chrono::Utc>, bool)> =
        sqlx::query_as(
            "SELECT author_did, status, music_url, expires_at, like_count, repost_count, reply_count, created_at, is_live
             FROM appview_status
             WHERE author_did = ANY($1) AND expires_at > now()
             ORDER BY updated_at DESC",
        )
        .bind(dids)
        .fetch_all(db)
        .await?;
    Ok(rows
        .into_iter()
        .map(|(did, status, music_url, expires_at, like, repost, reply, created, is_live)| StoryItem {
            kind: "status".into(),
            uri: format!("at://{}/app.bsky.actor.status/self", did),
            author_did: did.clone(),
            created_at: rfc3339(created),
            expires_at: Some(rfc3339(expires_at)),
            active_from: None,
            pinned: false,
            text: status.map(|s| {
                if s.starts_with("app.bsky.actor.status#") {
                    // Bluesky live-status sentinel — display the human label.
                    "Live now".to_string()
                } else {
                    s
                }
            }),
            media: None,
            music_url,
            is_live,
            style: styles.get(&did).cloned(),
            can_react: true,
            can_reply: true,
            like_count: like,
            repost_count: repost,
            reply_count: reply,
            state: Some("active".into()),
        })
        .collect())
}

async fn fetch_seen(db: &PgPool, viewer: &str, uris: &[String]) -> std::collections::HashSet<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT uri FROM appview_story_seen WHERE viewer_did = $1 AND uri = ANY($2)",
    )
    .bind(viewer)
    .bind(uris)
    .fetch_all(db)
    .await
    .unwrap_or_default()
    .into_iter()
    .collect()
}

// === Handlers ===

async fn rail_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<RailResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    // Federated video moments (Loops / Pixelfed / Mastodon video posts) from
    // the viewer's interest/hashtag scope — the rail populates from these even
    // when nobody in the circle posts stories yet.
    let ctx = crate::api::feed::sources::GroupScopeCtx::resolve(&state.db, Some(user_id)).await;
    let mut videos = crate::api::feed::sources::fetch_interest_video_posts(&state.db, &ctx, 12).await;
    // The rail is a casual surface: hidden-flagged content stays out unless
    // the viewer explicitly opted in.
    let show_nsfw: Option<String> =
        sqlx::query_scalar("SELECT nsfw_pref FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
    if show_nsfw.as_deref() != Some("include") {
        videos.retain(|p| !p.nsfw);
    }
    let dids = circle_dids(&state.db, user_id, &state.config.domain).await;
    if dids.is_empty() {
        return Ok(Json(RailResponse {
            groups: vec![],
            videos,
        }));
    }
    // Bounded catch-up for circle authors missing from the index (consumer
    // downtime, new user); short-TTL dedup keeps it cheap. Detached: the
    // sweep fetches over the network and must never gate the rail response.
    // One sweep at a time — every rail request used to spawn its own, and
    // stacked sweeps swamped the DB pool the response needs.
    static SWEEP_RUNNING: std::sync::LazyLock<std::sync::atomic::AtomicBool> =
        std::sync::LazyLock::new(|| std::sync::atomic::AtomicBool::new(false));
    if SWEEP_RUNNING
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .is_ok()
    {
        let db = state.db.clone();
        let dids = dids.clone();
        let port = state.config.port;
        tokio::spawn(async move {
            catchup_stories_for(&db, &dids, port).await;
            SWEEP_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
        });
    }
    let mut items = fetch_active_stories(&state.db, &dids).await?;
    let styles = status_styles(&state.db, &dids).await;
    items.extend(fetch_active_statuses(&state.db, &dids, &styles).await?);

    let authors = author_info(&state.db, &dids).await;
    let uris: Vec<String> = items.iter().map(|i| i.uri.clone()).collect();
    let seen_uris = fetch_seen(&state.db, &user_id.to_string(), &uris).await;

    Ok(Json(RailResponse {
        groups: assemble_groups(items, &authors, &seen_uris),
        videos,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ExplorerQuery {
    pub sort: Option<String>,   // "recent" (default) | "author"
    pub person: Option<String>, // handle/did/name substring
    pub kind: Option<String>,   // "both" (default) | "media" | "status"
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

async fn explorer_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ExplorerQuery>,
) -> Result<Json<ExplorerResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let dids = circle_dids(&state.db, user_id, &state.config.domain).await;
    if dids.is_empty() {
        return Ok(Json(ExplorerResponse { groups: vec![], total: 0 }));
    }
    let mut items = fetch_active_stories(&state.db, &dids).await?;
    let styles = status_styles(&state.db, &dids).await;
    items.extend(fetch_active_statuses(&state.db, &dids, &styles).await?);
    let authors = author_info(&state.db, &dids).await;
    let uris: Vec<String> = items.iter().map(|i| i.uri.clone()).collect();
    let seen_uris = fetch_seen(&state.db, &user_id.to_string(), &uris).await;

    let mut groups = assemble_groups(items, &authors, &seen_uris);

    // Kind filter drops non-matching items after assembly so mirror-deduped
    // status cards cannot resurrect through the filter.
    match query.kind.as_deref() {
        Some("media") => {
            for g in &mut groups {
                g.items.retain(|i| i.kind == "story");
            }
        }
        Some("status") => {
            for g in &mut groups {
                g.items.retain(|i| i.kind == "status");
            }
        }
        _ => {}
    }
    groups.retain(|g| !g.items.is_empty());

    // Person filter over handle / display name / did.
    if let Some(p) = query.person.as_deref().filter(|p| !p.trim().is_empty()) {
        let p = p.trim().to_lowercase();
        groups.retain(|g| {
            g.author.handle.to_lowercase().contains(&p)
                || g.author.did.to_lowercase().contains(&p)
                || g.author
                    .display_name
                    .as_deref()
                    .map(|n| n.to_lowercase().contains(&p))
                    .unwrap_or(false)
        });
    }

    // Author sort: display name (fallback handle) ascending.
    if query.sort.as_deref() == Some("author") {
        groups.sort_by(|a, b| {
            let ka = a.author.display_name.as_deref().unwrap_or(&a.author.handle);
            let kb = b.author.display_name.as_deref().unwrap_or(&b.author.handle);
            ka.to_lowercase().cmp(&kb.to_lowercase())
        });
    }

    let total = groups.len();
    let offset = query.offset.unwrap_or(0).max(0) as usize;
    let limit = query.limit.unwrap_or(30).clamp(1, 100) as usize;
    let groups: Vec<StoryGroup> = groups.into_iter().skip(offset).take(limit).collect();
    Ok(Json(ExplorerResponse { groups, total }))
}

async fn mine_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MineResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let dids = own_dids(&state.db, user_id).await;
    if dids.is_empty() {
        return Ok(Json(MineResponse { items: vec![] }));
    }
    let mut items = fetch_all_stories(&state.db, &dids).await?;
    for item in &mut items {
        item.state = Some(story_state(item).into());
    }
    let styles = status_styles(&state.db, &dids).await;
    items.extend(fetch_active_statuses(&state.db, &dids, &styles).await?);
    order_items(&mut items);
    Ok(Json(MineResponse { items }))
}

#[derive(Debug, Deserialize)]
pub struct SeenRequest {
    pub uri: String,
}

async fn seen_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SeenRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    if body.uri.is_empty() {
        return Err(AppError::BadRequest("uri required".into()));
    }
    sqlx::query(
        "INSERT INTO appview_story_seen (viewer_did, uri) VALUES ($1, $2)
         ON CONFLICT (viewer_did, uri) DO NOTHING",
    )
    .bind(user_id.to_string())
    .bind(&body.uri)
    .execute(&state.db)
    .await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

#[derive(Debug, Deserialize)]
pub struct StoryItemQuery {
    pub uri: String,
}

/// GET /api/stories/item — one active story (for reshare preview cards).
/// Social window enforced: scheduled/expired stories 404.
async fn story_item_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<StoryItemQuery>,
) -> Result<Json<StoryItem>, AppError> {
    let _user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let rows: Vec<StoryRow> = sqlx::query_as::<_, StoryRow>(&format!(
        "SELECT {STORY_COLUMNS} FROM appview_stories
         WHERE uri = $1 AND NOT archived
           AND (active_from IS NULL OR active_from <= now())
           AND (expires_at IS NULL OR expires_at > now())"
    ))
    .bind(&q.uri)
    .fetch_all(&state.db)
    .await?;
    rows.into_iter()
        .map(story_row_to_item)
        .next()
        .map(Json)
        .ok_or_else(|| AppError::NotFound("story not found".into()))
}

// === Write side: PDS records + self-index ===
//
// Records go to the author's PDS (native or linked) and the appview row is
// upserted directly — our own PDS firehose is not guaranteed to loop back
// through bsky.network, so self-indexing mirrors the PostRow::create pattern.

fn parse_rfc3339_opt(s: Option<&String>) -> Result<Option<chrono::DateTime<chrono::Utc>>, AppError> {
    match s {
        None => Ok(None),
        Some(s) if s.is_empty() => Ok(None),
        Some(s) => chrono::DateTime::parse_from_rfc3339(s)
            .map(|dt| Some(dt.with_timezone(&chrono::Utc)))
            .map_err(|_| AppError::BadRequest(format!("invalid timestamp: {}", s))),
    }
}

/// The record's CID as the firehose computes it (dag-cbor bytes → CIDv1).
pub(crate) fn record_cid(record: &serde_json::Value) -> String {
    let bytes = serde_ipld_dagcbor::to_vec(record).unwrap_or_default();
    let mut sha = sha2::Sha256::new();
    sha.update(&bytes);
    let hash = sha.finalize();
    lexicon_cid::Cid::new_v1(
        0x71,
        lexicon_cid::multihash::MultihashGeneric::wrap(0x12, &hash).unwrap(),
    )
    .to_string()
}

pub(crate) enum AtId {
    Native(String),
    Linked(crate::db::accounts::LinkedAccountRow),
}

/// Resolve the user's AT identity: native Bytescape PDS account first, then a
/// linked external account (same posture as post creation).
pub(crate) async fn at_identity_for(db: &PgPool, user_id: Uuid) -> Result<Option<AtId>, AppError> {
    let native: Option<(String,)> = sqlx::query_as("SELECT did FROM pds_accounts WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    if let Some((did,)) = native {
        return Ok(Some(AtId::Native(did)));
    }
    let acct = crate::db::accounts::LinkedAccountRow::find_by_user(db, user_id)
        .await?
        .into_iter()
        .find(|a| a.provider == "bluesky");
    Ok(acct.map(AtId::Linked))
}

/// Load archived story media for a reshare: read the private copy from disk
/// (the HTTP route is owner-auth'd and cannot be fetched server-side without
/// credentials). Returns None when the URL is not an archive-media URL.
async fn load_archive_media(
    db: &PgPool,
    user_id: Uuid,
    url: &str,
) -> Option<(Vec<u8>, String)> {
    let rest = url.split("/api/stories/archive/media?").nth(1)?;
    let mut uri = String::new();
    let mut cid = String::new();
    for pair in rest.split('&') {
        let mut kv = pair.splitn(2, '=');
        match (kv.next(), kv.next()) {
            (Some("uri"), Some(v)) => uri = urlencoding::decode(v).ok()?.into_owned(),
            (Some("cid"), Some(v)) => cid = urlencoding::decode(v).ok()?.into_owned(),
            _ => {}
        }
    }
    let (did, _collection, rkey) = split_at_uri(&uri)?;
    let own = own_dids(db, user_id).await;
    if !own.contains(&did) {
        return None;
    }
    let row: Option<(Option<serde_json::Value>,)> =
        sqlx::query_as("SELECT archive_media FROM appview_stories WHERE uri = $1 AND archived")
            .bind(&uri)
            .fetch_optional(db)
            .await
            .ok()
            .flatten();
    let entries = row
        .and_then(|(v,)| v)
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let entry = entries
        .iter()
        .find(|e| e.get("cid").and_then(|c| c.as_str()) == Some(cid.as_str()))?;
    let mime = entry
        .get("mime_type")
        .and_then(|m| m.as_str())
        .unwrap_or("image/jpeg")
        .to_string();
    let bytes = std::fs::read(format!("{}/{}", archive_dir(&did, &rkey), cid)).ok()?;
    Some((bytes, mime))
}

/// Load uploaded media bytes for a media_url produced by /api/media/upload.
async fn load_media_bytes(url: &str) -> Result<(Vec<u8>, String), AppError> {
    if let Some(key) = url.strip_prefix("/uploads/") {
        if key.contains("..") {
            return Err(AppError::BadRequest("invalid media path".into()));
        }
        let bytes = std::fs::read(format!("./uploads/{}", key))
            .map_err(|e| AppError::BadRequest(format!("media not found: {}", e)))?;
        let mime = mime_guess::from_path(key)
            .first_raw()
            .unwrap_or("application/octet-stream")
            .to_string();
        return Ok((bytes, mime));
    }
    let resp = reqwest::get(url)
        .await
        .map_err(|e| AppError::BadRequest(format!("media fetch failed: {}", e)))?;
    if !resp.status().is_success() {
        return Err(AppError::BadRequest(format!("media fetch returned {}", resp.status())));
    }
    let mime = resp
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::BadRequest(format!("media read failed: {}", e)))?
        .to_vec();
    Ok((bytes, mime))
}

/// Upload media as a PDS blob on whichever PDS hosts the identity.
async fn upload_story_blob(
    db: &PgPool,
    id: &AtId,
    bytes: &[u8],
    mime: &str,
) -> Result<(String, i64), AppError> {
    match id {
        AtId::Native(did) => crate::pds::put_blob_internal(db, did, bytes, mime).await,
        AtId::Linked(acct) => {
            let resp = crate::oauth::bluesky::repo_upload_blob(db, acct, bytes.to_vec(), mime)
                .await
                .map_err(|e| AppError::Internal(format!("blob upload failed: {}", e)))?;
            let blob = resp.get("blob").cloned().unwrap_or(resp);
            let cid = blob
                .get("ref")
                .and_then(|r| r.get("$link"))
                .and_then(|l| l.as_str())
                .unwrap_or("")
                .to_string();
            let size = blob.get("size").and_then(|s| s.as_i64()).unwrap_or(bytes.len() as i64);
            if cid.is_empty() {
                return Err(AppError::Internal("blob upload returned no cid".into()));
            }
            Ok((cid, size))
        }
    }
}

/// Create (or replace, for repeated/self-keyed rkeys) a record on the
/// author's PDS. Create-first, falling back to put semantics when the key
/// already holds a value.
pub(crate) async fn write_record(
    db: &PgPool,
    state: &AppState,
    id: &AtId,
    collection: &str,
    rkey: &str,
    record: &serde_json::Value,
) -> Result<(), AppError> {
    match id {
        AtId::Native(did) => {
            match crate::pds::create_record_internal(
                db, did, collection, Some(rkey), record, Some(&state.config.domain),
            )
            .await
            {
                Ok(_) => Ok(()),
                Err(e) if e.to_string().contains("already a value") => {
                    crate::pds::put_record_internal(db, did, collection, rkey, record).await?;
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        AtId::Linked(acct) => {
            let body = serde_json::json!({
                "repo": acct.external_id,
                "collection": collection,
                "rkey": rkey,
                "record": record,
            });
            match crate::oauth::bluesky::repo_write(db, acct, "com.atproto.repo.createRecord", &body)
                .await
            {
                Ok(_) => Ok(()),
                // Existing key → put semantics (replace), mirroring the
                // native path — self-keyed records (event RSVPs, status
                // cards) must replace, never duplicate.
                Err(e) if e.contains("RecordAlreadyExists") || e.contains("already a value") => {
                    crate::oauth::bluesky::repo_write(db, acct, "com.atproto.repo.putRecord", &body)
                        .await
                        .map(|_| ())
                        .map_err(|e| AppError::Internal(format!("external putRecord failed: {}", e)))
                }
                Err(e) if e.contains("ScopeMissingError") => Err(AppError::BadRequest(
                    "Your Bluesky authorization is missing write permissions. Reconnect your Bluesky account (Accounts → Add → Bluesky) to grant them."
                        .into(),
                )),
                Err(e) => Err(AppError::Internal(format!(
                    "external createRecord failed: {}",
                    e
                ))),
            }
        }
    }
}

/// Delete a record from the author's PDS if we hold write authorization.
/// Returns whether the record is gone.
pub(crate) async fn delete_author_record(
    db: &PgPool,
    did: &str,
    collection: &str,
    rkey: &str,
) -> bool {
    match crate::pds::delete_record_internal(db, did, collection, rkey).await {
        Ok(_) => return true,
        Err(AppError::NotFound(_)) => {} // not a native account — try linked
        Err(e) => {
            warn!("[STORIES] native deleteRecord failed for {}/{}: {}", did, rkey, e);
            return false;
        }
    }
    let acct: Option<crate::db::accounts::LinkedAccountRow> =
        sqlx::query_as("SELECT * FROM linked_accounts WHERE external_id = $1 LIMIT 1")
            .bind(did)
            .fetch_optional(db)
            .await
            .ok()
            .flatten();
    if let Some(acct) = acct {
        return crate::oauth::bluesky::delete_record(db, &acct, collection, rkey)
            .await
            .map_err(|e| {
                warn!("[STORIES] external deleteRecord failed for {}/{}: {}", did, rkey, e);
                e
            })
            .is_ok();
    }
    false
}

#[derive(Debug, Deserialize)]
pub struct CreateStoryRequest {
    pub media_urls: Vec<String>,
    pub alts: Option<Vec<String>>,
    pub durations: Option<Vec<Option<i64>>>,
    pub text: Option<String>,
    pub expires_at: Option<String>,
    pub active_from: Option<String>,
    pub pinned: Option<bool>,
}

/// POST /api/stories — publish a com.bytescape.story to the author's PDS and
/// self-index it. A caption also mirrors into the author's status (24h) for
/// cross-app visibility.
async fn create_story_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateStoryRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    if req.media_urls.is_empty() {
        return Err(AppError::BadRequest("at least one media item required".into()));
    }
    let id = at_identity_for(&state.db, user_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("no AT identity linked to this account".into()))?;

    let active_from = parse_rfc3339_opt(req.active_from.as_ref())?;
    let expires_at = parse_rfc3339_opt(req.expires_at.as_ref())?;

    let mut record_items = Vec::new();
    let mut media_items = Vec::new();
    for (i, url) in req.media_urls.iter().enumerate() {
        if i == 6 {
            break;
        }
        let (bytes, mime) = match load_archive_media(&state.db, user_id, url).await {
            Some(pair) => pair,
            None => load_media_bytes(url).await?,
        };
        let (cid, size) = upload_story_blob(&state.db, &id, &bytes, &mime).await?;
        let kind = if mime.starts_with("video") { "video" } else { "image" };
        let alt = req.alts.as_ref().and_then(|a| a.get(i)).cloned().unwrap_or_default();
        let duration = req.durations.as_ref().and_then(|d| d.get(i).copied()).flatten().map(|d| d.clamp(1, 300));
        record_items.push(serde_json::json!({
            kind: {"$type": "blob", "ref": {"$link": cid}, "mimeType": mime, "size": size},
            "alt": alt,
            "duration": duration,
        }));
        // The index serves the flat media shape the consumer produces.
        media_items.push(serde_json::json!({
            "type": kind, "cid": cid, "mime_type": mime, "alt": alt, "duration": duration,
        }));
    }

    let now = chrono::Utc::now();
    let text = req.text.as_deref().map(|t| t.replace('\u{0}', "")).filter(|t| !t.is_empty());
    let record = serde_json::json!({
        "$type": "com.bytescape.story",
        "createdAt": now.to_rfc3339(),
        "expiresAt": expires_at.map(|t| t.to_rfc3339()),
        "activeFrom": active_from.map(|t| t.to_rfc3339()),
        "pinned": req.pinned.unwrap_or(false),
        "text": text,
        "items": record_items,
        "canReact": true,
        "canReply": true,
    });
    let rkey = {
        use rsky_common::tid::Ticker;
        Ticker::new().next(None).0
    };
    let author_did = match &id {
        AtId::Native(d) => d.clone(),
        AtId::Linked(a) => a.external_id.clone(),
    };
    write_record(&state.db, &state, &id, "com.bytescape.story", &rkey, &record).await?;
    let uri = format!("at://{}/com.bytescape.story/{}", author_did, rkey);

    // Self-index (firehose loopback is not guaranteed for our own PDS).
    let media = serde_json::Value::Array(media_items);
    crate::appview::consumer::upsert_story_row(
        &state.db,
        &uri,
        &record_cid(&record),
        &author_did,
        now,
        active_from,
        expires_at,
        req.pinned.unwrap_or(false),
        text.clone(),
        &media,
        true,
        true,
    )
    .await?;

    // Consume the transient upload rows: the story's media lives in the
    // author's PDS repo now, and every composer attempt leaves a full-size
    // `media` row that would otherwise clutter the Vault as an orphan file.
    for url in &req.media_urls {
        let Some(key) = url.strip_prefix("/uploads/") else {
            continue;
        };
        let deleted: Option<(String,)> = sqlx::query_as(
            "DELETE FROM media WHERE storage_url = $1 AND user_id = $2 AND post_id IS NULL RETURNING storage_url",
        )
        .bind(url)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await?;
        if deleted.is_some() {
            let _ = std::fs::remove_file(format!("./uploads/{}", key));
        }
    }

    // Caption mirror into the status (24h, cross-app visibility). Same-text
    // dedupe keeps it from doubling in our own rail. Skipped while a live
    // status is active: the mirror writes an app.bsky.actor.status record to
    // the PDS, which the firehose consumer re-upserts unconditionally, so a
    // record-level skip is the only complete guard.
    let live_status = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM appview_status WHERE author_did = $1 AND is_live AND expires_at > now())",
    )
    .bind(&author_did)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);
    if let (Some(caption), false) = (text.clone(), live_status) {
        let status_record = serde_json::json!({
            "$type": "app.bsky.actor.status",
            "status": caption,
            "createdAt": now.to_rfc3339(),
            "durationMinutes": 1440,
        });
        if let Err(e) = write_record(&state.db, &state, &id, "app.bsky.actor.status", "self", &status_record).await {
            warn!("[STORIES] caption mirror failed for {}: {}", uri, e);
        } else {
            let expires = now + chrono::Duration::minutes(1440);
            let _ = sqlx::query(
                "INSERT INTO appview_status (author_did, cid, status, expires_at, is_live)
                 VALUES ($1, $2, $3, $4, false)
                 ON CONFLICT (author_did) DO UPDATE SET
                   cid = EXCLUDED.cid, status = EXCLUDED.status, expires_at = EXCLUDED.expires_at, is_live = false,
                   updated_at = now(), created_at = now(), like_count = 0, repost_count = 0, reply_count = 0
                 WHERE appview_status.is_live = false
                    OR appview_status.expires_at < now()",
            )
            .bind(&author_did)
            .bind(record_cid(&status_record))
            .bind(&caption)
            .bind(expires)
            .execute(&state.db)
            .await;
        }
    }

    Ok(Json(serde_json::json!({"uri": uri})))
}

#[derive(Debug, Deserialize)]
pub struct SetStatusRequest {
    pub text: String,
    pub duration_minutes: Option<i64>,
    pub embed_uri: Option<String>,
    pub style: Option<serde_json::Value>,
}

/// POST /api/stories/status — set the author's status (self-keyed replace).
async fn set_status_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SetStatusRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let text = req.text.replace('\u{0}', "");
    if text.trim().is_empty() {
        return Err(AppError::BadRequest("text required".into()));
    }
    let minutes = req.duration_minutes.unwrap_or(1440).clamp(1, 1440);
    let id = at_identity_for(&state.db, user_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("no AT identity linked to this account".into()))?;

    let now = chrono::Utc::now();
    let mut record = serde_json::json!({
        "$type": "app.bsky.actor.status",
        "status": text,
        "createdAt": now.to_rfc3339(),
        "durationMinutes": minutes,
    });
    if let Some(uri) = req.embed_uri.as_deref().filter(|u| !u.is_empty()) {
        record["embed"] = serde_json::json!({
            "$type": "app.bsky.embed.external",
            "external": {"uri": uri, "title": "", "description": ""}
        });
    }
    write_record(&state.db, &state, &id, "app.bsky.actor.status", "self", &record).await?;

    let author_did = match &id {
        AtId::Native(d) => d.clone(),
        AtId::Linked(a) => a.external_id.clone(),
    };
    let expires = now + chrono::Duration::minutes(minutes);
    // New cid = new status instance: counts reset (the consumer arm enforces
    // the same rule when other apps replace the record).
    sqlx::query(
        "INSERT INTO appview_status (author_did, cid, status, music_url, expires_at, is_live)
         VALUES ($1, $2, $3, $4, $5, false)
         ON CONFLICT (author_did) DO UPDATE SET
           cid = EXCLUDED.cid, status = EXCLUDED.status, music_url = EXCLUDED.music_url, is_live = false,
           expires_at = EXCLUDED.expires_at, updated_at = now(), created_at = now(),
           like_count = 0, repost_count = 0, reply_count = 0",
    )
    .bind(&author_did)
    .bind(record_cid(&record))
    .bind(&text)
    .bind(req.embed_uri.as_deref().filter(|u| !u.is_empty()))
    .bind(expires)
    .execute(&state.db)
    .await?;

    if let Some(style) = req.style {
        sqlx::query("UPDATE users SET status_style = $1 WHERE id = $2")
            .bind(style)
            .bind(user_id)
            .execute(&state.db)
            .await?;
    }

    Ok(Json(serde_json::json!({"ok": true})))
}

/// Parse an at:// URI into (did, collection, rkey).
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

#[derive(Debug, Deserialize)]
pub struct DeleteStoryRequest {
    pub uri: String,
}

/// DELETE /api/stories — real deletion: record from the PDS where authorized,
/// index row, and any archive copy. Nothing survives the author's explicit
/// deletion.
async fn delete_story_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DeleteStoryRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let (did, collection, rkey) = split_at_uri(&req.uri)
        .ok_or_else(|| AppError::BadRequest("invalid at:// uri".into()))?;
    if collection != "com.bytescape.story" && collection != "app.bsky.actor.status" {
        return Err(AppError::BadRequest("not a story or status record".into()));
    }
    let own = own_dids(&state.db, user_id).await;
    if !own.contains(&did) {
        return Err(AppError::Forbidden("not your record".into()));
    }

    delete_author_record(&state.db, &did, &collection, &rkey).await;

    if collection == "com.bytescape.story" {
        sqlx::query("DELETE FROM appview_stories WHERE uri = $1")
            .bind(&req.uri)
            .execute(&state.db)
            .await?;
        remove_archive_dir(&did, &rkey);
    } else {
        sqlx::query("DELETE FROM appview_status WHERE author_did = $1")
            .bind(&did)
            .execute(&state.db)
            .await?;
    }
    Ok(Json(serde_json::json!({"ok": true})))
}

// === Private archive storage ===

fn archive_dir(did: &str, rkey: &str) -> String {
    let safe = |s: &str| s.replace([':', '/', '\\'], "_");
    format!("./private_stories/{}/{}", safe(did), safe(rkey))
}

/// Purge every Bytescape-authored content record for a DID from its PDS and
/// the local index. Used on account deletion. Per the deletion policy:
///  - `com.bytescape.story` (Bytescape-only) and the mirrored
///    `app.bsky.actor.status` are ALWAYS deleted — these are Bytescape features
///    and must not survive account deletion;
///  - standard Bluesky records (`app.bsky.feed.post`, etc.) are left alone for
///    linked accounts (they belong to the user's Bluesky presence) but are
///    handled by the native-repo wipe for native accounts.
/// Best-effort: individual record failures are logged and skipped.
pub(crate) async fn purge_author_content(db: &PgPool, did: &str) {
    // Delete story records (com.bytescape.story) — both the PDS record and the
    // local index row + private media archive. The rkey is parsed from the uri
    // (appview_stories has no rkey column).
    let story_uris: Vec<(String,)> = sqlx::query_as::<_, (String,)>(
        "SELECT uri FROM appview_stories WHERE author_did = $1",
    )
    .bind(did)
    .fetch_all(db)
    .await
    .unwrap_or_default();
    for (uri,) in story_uris {
        let rkey = split_at_uri(&uri)
            .map(|(_, _, rkey)| rkey)
            .unwrap_or_else(|| uri.rsplit('/').next().unwrap_or_default().to_string());
        delete_author_record(db, did, "com.bytescape.story", &rkey).await;
        let _ = sqlx::query("DELETE FROM appview_stories WHERE uri = $1")
            .bind(&uri)
            .execute(db)
            .await;
        remove_archive_dir(did, &rkey);
    }

    // Delete the mirrored status record (app.bsky.actor.status/self).
    delete_author_record(db, did, "app.bsky.actor.status", "self").await;
    let _ = sqlx::query("DELETE FROM appview_status WHERE author_did = $1")
        .bind(did)
        .execute(db)
        .await;
}

fn remove_archive_dir(did: &str, rkey: &str) {
    let _ = std::fs::remove_dir_all(archive_dir(did, rkey));
}

#[derive(Debug, Deserialize)]
pub struct ArchiveMediaQuery {
    pub uri: String,
    pub cid: String,
}

/// GET /api/stories/archive/media — owner-only serving of archived media from
/// app-controlled storage. No visitor path.
async fn archive_media_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ArchiveMediaQuery>,
) -> Result<axum::response::Response, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let (did, _collection, rkey) = split_at_uri(&q.uri)
        .ok_or_else(|| AppError::BadRequest("invalid at:// uri".into()))?;
    let own = own_dids(&state.db, user_id).await;
    if !own.contains(&did) {
        return Err(AppError::Forbidden("archive is owner-only".into()));
    }
    let row: Option<(Option<serde_json::Value>,)> =
        sqlx::query_as("SELECT archive_media FROM appview_stories WHERE uri = $1 AND archived")
            .bind(&q.uri)
            .fetch_optional(&state.db)
            .await?;
    let entries = row
        .and_then(|(v,)| v)
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let entry = entries
        .iter()
        .find(|e| e.get("cid").and_then(|c| c.as_str()) == Some(q.cid.as_str()))
        .ok_or_else(|| AppError::NotFound("archive media not found".into()))?;
    let mime = entry
        .get("mime_type")
        .and_then(|m| m.as_str())
        .unwrap_or("application/octet-stream")
        .to_string();
    let path = format!("{}/{}", archive_dir(&did, &rkey), q.cid);
    let bytes =
        std::fs::read(&path).map_err(|_| AppError::NotFound("archive media not found".into()))?;
    axum::response::Response::builder()
        .header(axum::http::header::CONTENT_TYPE, mime)
        .body(axum::body::Body::from(bytes))
        .map_err(|e| AppError::Internal(format!("response build failed: {}", e)))
}

/// DELETE /api/stories/archive — permanent deletion of an archive item: the
/// stored media and the row are destroyed.
async fn archive_delete_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DeleteStoryRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let (did, _collection, rkey) = split_at_uri(&req.uri)
        .ok_or_else(|| AppError::BadRequest("invalid at:// uri".into()))?;
    let own = own_dids(&state.db, user_id).await;
    if !own.contains(&did) {
        return Err(AppError::Forbidden("not your record".into()));
    }
    sqlx::query("DELETE FROM appview_stories WHERE uri = $1")
        .bind(&req.uri)
        .execute(&state.db)
        .await?;
    remove_archive_dir(&did, &rkey);
    Ok(Json(serde_json::json!({"ok": true})))
}

// === Expiry transition job (4.2) ===

/// Fetch a blob for archival: our own store first, then the author's linked
/// PDS, then the network entryway. Videos run to the full blob limit, so the
/// fetch timeout is generous and the mime is taken from the response.
async fn fetch_blob_for_archive(db: &PgPool, did: &str, cid: &str) -> Option<(String, Vec<u8>)> {
    if let Ok(Some(row)) = crate::pds::get_blob_internal(db, did, cid).await {
        return Some(row);
    }
    let acct: Option<crate::db::accounts::LinkedAccountRow> =
        sqlx::query_as("SELECT * FROM linked_accounts WHERE external_id = $1 LIMIT 1")
            .bind(did)
            .fetch_optional(db)
            .await
            .ok()
            .flatten()
            .filter(|a: &crate::db::accounts::LinkedAccountRow| a.provider == "bluesky");
    let mut bases: Vec<String> = Vec::new();
    if let Some(a) = &acct {
        bases.push(crate::oauth::bluesky::get_pds_url(&a.metadata));
    }
    bases.push("https://bsky.network".to_string());
    for base in bases {
        let url = format!(
            "{}/xrpc/com.atproto.sync.getBlob?did={}&cid={}",
            base,
            urlencoding::encode(did),
            urlencoding::encode(cid)
        );
        let Ok(resp) = reqwest::Client::new()
            .get(&url)
            .timeout(std::time::Duration::from_secs(300))
            .send()
            .await
        else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        let mime = resp
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        if let Ok(bytes) = resp.bytes().await {
            return Some((mime, bytes.to_vec()));
        }
    }
    None
}

const TRANSITION_BATCH: i64 = 20;
const TRANSITION_MAX_ATTEMPTS: i32 = 5;

/// One transition sweep: expired deadline stories get their media copied to
/// private storage FIRST (verify the copy), then the public record is deleted
/// where we hold write authorization, then the row is reduced to metadata.
pub async fn run_story_transitions(db: &PgPool) -> Result<(), AppError> {
    let rows: Vec<(String, String, Option<serde_json::Value>, i32)> = sqlx::query_as(
        "SELECT uri, author_did, media, transition_attempts FROM appview_stories
         WHERE NOT archived AND expires_at IS NOT NULL AND expires_at < now()
           AND transition_attempts < $1
         ORDER BY expires_at LIMIT $2",
    )
    .bind(TRANSITION_MAX_ATTEMPTS)
    .bind(TRANSITION_BATCH)
    .fetch_all(db)
    .await?;

    for (uri, did, media, attempts) in rows {
        let Some((_d, _c, rkey)) = split_at_uri(&uri) else {
            warn!("[STORIES] transition: unparseable uri {}", uri);
            continue;
        };
        let entries: Vec<serde_json::Value> = media
            .and_then(|m| m.as_array().cloned())
            .unwrap_or_default();
        let dir = archive_dir(&did, &rkey);
        let mut archive_entries = Vec::new();
        let mut copy_ok = true;
        for e in &entries {
            let cid = match e.get("cid").and_then(|c| c.as_str()) {
                Some(c) => c.to_string(),
                None => continue,
            };
            match fetch_blob_for_archive(db, &did, &cid).await {
                Some((mime, bytes)) => {
                    if std::fs::create_dir_all(&dir).is_err() {
                        copy_ok = false;
                        break;
                    }
                    let path = format!("{}/{}", dir, cid);
                    if std::fs::write(&path, &bytes).is_err() {
                        copy_ok = false;
                        break;
                    }
                    let mut entry = e.clone();
                    entry["mime_type"] = serde_json::Value::String(mime);
                    archive_entries.push(entry);
                }
                None => {
                    warn!("[STORIES] transition: blob {} for {} unreachable", cid, uri);
                    copy_ok = false;
                    break;
                }
            }
        }
        if !copy_ok {
            sqlx::query(
                "UPDATE appview_stories SET transition_attempts = transition_attempts + 1 WHERE uri = $1",
            )
            .bind(&uri)
            .execute(db)
            .await?;
            continue;
        }

        // Persist the private archive item, then remove the public record.
        sqlx::query(
            "UPDATE appview_stories SET archive_media = $1, transition_attempts = transition_attempts + 1 WHERE uri = $2",
        )
        .bind(serde_json::Value::Array(archive_entries))
        .bind(&uri)
        .execute(db)
        .await?;

        if delete_author_record(db, &did, "com.bytescape.story", &rkey).await {
            sqlx::query("UPDATE appview_stories SET archived = true WHERE uri = $1")
                .bind(&uri)
                .execute(db)
                .await?;
        } else if attempts + 1 >= TRANSITION_MAX_ATTEMPTS {
            // Copy is safe; the record could not be removed (no/expired write
            // auth). Reduce to metadata and surface for follow-up.
            sqlx::query("UPDATE appview_stories SET archived = true WHERE uri = $1")
                .bind(&uri)
                .execute(db)
                .await?;
            warn!("[STORIES] STUCK expiry: record {} not deletable after {} attempts", uri, attempts + 1);
        }
    }
    Ok(())
}

/// Background sweep over expired deadline stories.
pub fn spawn_story_transitions(db: PgPool) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(600));
        loop {
            tick.tick().await;
            if let Err(e) = run_story_transitions(&db).await {
                tracing::warn!("[STORIES] transition sweep failed: {}", e);
            }
        }
    });
}

// === Catch-up for pre-index stories (5.1) ===

static CATCHUP_TRIED: std::sync::LazyLock<std::sync::Mutex<HashMap<String, std::time::Instant>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// Parse a listRecords story JSON into index fields (mirrors the record
/// schema rules; malformed entries are skipped).
fn story_from_json(v: &serde_json::Value) -> Option<(String, chrono::DateTime<chrono::Utc>, Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>, bool, Option<String>, serde_json::Value, bool, bool)> {
    let created = parse_rfc3339_opt(v.get("createdAt").and_then(|t| t.as_str()).map(|s| s.to_string()).as_ref()).ok().flatten()?;
    let active_from = parse_rfc3339_opt(v.get("activeFrom").and_then(|t| t.as_str()).map(|s| s.to_string()).as_ref()).ok().flatten();
    let expires_at = parse_rfc3339_opt(v.get("expiresAt").and_then(|t| t.as_str()).map(|s| s.to_string()).as_ref()).ok().flatten();
    let pinned = v.get("pinned").and_then(|b| b.as_bool()).unwrap_or(false);
    let text = v.get("text").and_then(|t| t.as_str()).map(|s| s.replace('\u{0}', "")).filter(|s| !s.is_empty());
    let can_react = v.get("canReact").and_then(|b| b.as_bool()).unwrap_or(true);
    let can_reply = v.get("canReply").and_then(|b| b.as_bool()).unwrap_or(true);
    let mut items = Vec::new();
    for it in v.get("items")?.as_array()? {
        if items.len() == 6 {
            break;
        }
        let blob = ["image", "video", "blob"].iter().find_map(|k| it.get(*k))?;
        let cid = blob.get("ref")?.get("$link")?.as_str()?.to_string();
        let mime = blob.get("mimeType").and_then(|m| m.as_str()).unwrap_or("").to_string();
        let kind = if mime.starts_with("video") { "video" } else { "image" };
        let alt = it.get("alt").and_then(|a| a.as_str()).unwrap_or("");
        let duration = it.get("duration").and_then(|d| d.as_i64()).map(|d| d.clamp(1, 300));
        items.push(serde_json::json!({
            "type": kind, "cid": cid, "mime_type": mime, "alt": alt, "duration": duration,
        }));
    }
    if items.is_empty() {
        return None;
    }
    Some((String::new(), created, active_from, expires_at, pinned, text, serde_json::Value::Array(items), can_react, can_reply))
}

/// Bounded catch-up: for circle authors with zero indexed stories, list their
/// PDS records once per TTL window and upsert them. Native authors are served
/// by our own PDS; external ones via the bsky.social proxy.
pub async fn catchup_stories_for(db: &PgPool, dids: &[String], self_port: u16) {
    if dids.is_empty() {
        return;
    }
    let have: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT author_did FROM appview_stories WHERE author_did = ANY($1)",
    )
    .bind(dids)
    .fetch_all(db)
    .await
    .unwrap_or_default();
    let missing: Vec<&String> = dids.iter().filter(|d| !have.contains(d)).collect();
    if !missing.is_empty() {
        warn!(
            "[STORIES] catch-up: {} circle authors missing from index, sweeping up to 40",
            missing.len()
        );
    }

    // Sweep a real slice per pass and fetch in parallel. The old shape (3
    // serial authors, 5s each) starved the crawl — with hundreds of circle
    // authors the target story never got reached, so its ring "just wasn't
    // there". The CATCHUP_TRIED cooldown makes each pass advance through
    // fresh authors instead of re-trying the same head of the list.
    let mut budget = 40usize;
    let mut sweep: Vec<String> = Vec::new();
    // Missing authors first (first-time fetch), then known ones so their NEW
    // stories arrive too — skipping authors already in the index left their
    // fresh stories unfetched forever. The CATCHUP_TRIED cooldown bounds every
    // author to one fetch per 10min either way.
    for did in missing
        .iter()
        .copied()
        .chain(dids.iter().filter(|d| have.contains(d)))
    {
        if budget == 0 {
            break;
        }
        {
            let mut tried = match CATCHUP_TRIED.lock() {
                Ok(t) => t,
                Err(_) => return,
            };
            if tried
                .get(did)
                .map(|t| t.elapsed() < std::time::Duration::from_secs(600))
                .unwrap_or(false)
            {
                continue;
            }
            tried.insert(did.clone(), std::time::Instant::now());
        }
        budget -= 1;
        sweep.push(did.clone());
    }
    // Fetch in bounded parallel batches: a fully parallel sweep opens 40
    // concurrent DB round-trips and starves the pool the rail response
    // itself needs (queueing behind it cost 7-11s per request). Five at a
    // time keeps the pool healthy while still sweeping far faster than the
    // old serial crawl.
    for batch in sweep.chunks(5) {
        let futs = batch.iter().cloned().map(|did| catchup_one(db, did, self_port));
        futures::future::join_all(futs).await;
    }
}

/// Fetch one author's story records and upsert them into the index.
async fn catchup_one(db: &PgPool, did: String, self_port: u16) {
    let native: Option<(String,)> = sqlx::query_as("SELECT did FROM pds_accounts WHERE did = $1")
        .bind(&did)
        .fetch_optional(db)
        .await
        .unwrap_or(None);
    let base = if native.is_some() {
        format!("http://127.0.0.1:{}", self_port)
    } else {
        // Fetch from the author's actual PDS — bsky.social only serves
        // repos it hosts, so asking it about foreign dids 400s and the
        // story never enters the index.
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map(|c| c)
            .unwrap_or_default();
        crate::oauth::bluesky::resolve_pds_endpoint(&http, &did)
            .await
            .unwrap_or_else(|_| "https://bsky.social".to_string())
    };
    let url = format!(
        "{}/xrpc/com.atproto.repo.listRecords?repo={}&collection=com.bytescape.story&limit=50",
        base, did
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await;
    match resp {
        Ok(resp) if resp.status().is_success() => {
                match resp.json::<serde_json::Value>().await {
                    Ok(body) => {
                        if let Some(records) = body.get("records").and_then(|r| r.as_array()) {
                            for rec in records {
                                // listRecords entries nest the record under "value".
                                let value = rec.get("value").unwrap_or(rec);
                                match story_from_json(value) {
                                    Some((_, created, active_from, expires_at, pinned, text, items, can_react, can_reply)) => {
                                        let rkey = rec
                                            .get("uri")
                                            .and_then(|u| u.as_str())
                                            .and_then(|u| u.rsplit('/').next())
                                            .unwrap_or("")
                                            .to_string();
                                        let uri = format!("at://{}/com.bytescape.story/{}", did, rkey);
                                        if let Err(e) = crate::appview::consumer::upsert_story_row(
                                            db, &uri, "", &did, created, active_from, expires_at,
                                            pinned, text, &items, can_react, can_reply,
                                        )
                                        .await
                                        {
                                            warn!("[STORIES] catch-up upsert failed for {}: {}", uri, e);
                                        }
                                    }
                                    None => warn!("[STORIES] catch-up: malformed story record from {}", did),
                                }
                            }
                        }
                    }
                    Err(e) => warn!("[STORIES] catch-up: bad listRecords body from {}: {}", did, e),
                }
        }
        Ok(resp) => warn!("[STORIES] catch-up: listRecords {} for {}", resp.status(), did),
        Err(e) => warn!("[STORIES] catch-up: listRecords fetch failed for {}: {}", did, e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: &str, did: &str, uri: &str, created: &str, text: Option<&str>) -> StoryItem {
        StoryItem {
            kind: kind.into(),
            uri: uri.into(),
            author_did: did.into(),
            created_at: created.into(),
            expires_at: Some("2099-01-01T00:00:00+00:00".into()),
            active_from: None,
            pinned: false,
            text: text.map(|t| t.into()),
            media: None,
            music_url: None,
            is_live: false,
            style: None,
            can_react: true,
            can_reply: true,
            like_count: 0,
            repost_count: 0,
            reply_count: 0,
            state: Some("active".into()),
        }
    }

    #[test]
    fn groups_order_and_dedupe() {
        let items = vec![
            item("story", "did:plc:a", "at://did:plc:a/com.bytescape.story/1", "2026-11-01T10:00:00+00:00", Some("flyer")),
            item("story", "did:plc:a", "at://did:plc:a/com.bytescape.story/2", "2026-11-02T10:00:00+00:00", None),
            // Caption mirror of story 1 — must be suppressed.
            item("status", "did:plc:a", "at://did:plc:a/app.bsky.actor.status/self", "2026-11-01T10:05:00+00:00", Some("flyer")),
            item("story", "did:plc:b", "at://did:plc:b/com.bytescape.story/9", "2026-11-03T10:00:00+00:00", None),
        ];
        let authors = HashMap::new();
        let seen = std::collections::HashSet::new();
        let groups = assemble_groups(items, &authors, &seen);
        assert_eq!(groups.len(), 2);
        // did:plc:b has the most recent item overall.
        assert_eq!(groups[0].author.did, "did:plc:b");
        let a = &groups[1];
        assert_eq!(a.items.len(), 2, "mirror status suppressed");
        assert_eq!(a.items[0].uri, "at://did:plc:a/com.bytescape.story/2");
    }

    #[test]
    fn pinned_items_lead_the_group() {
        let mut p = item("story", "did:plc:a", "at://did:plc:a/com.bytescape.story/p", "2026-11-01T00:00:00+00:00", None);
        p.pinned = true;
        let items = vec![
            item("story", "did:plc:a", "at://did:plc:a/com.bytescape.story/n", "2026-11-02T00:00:00+00:00", None),
            p,
        ];
        let groups = assemble_groups(items, &HashMap::new(), &std::collections::HashSet::new());
        assert_eq!(groups[0].items[0].uri, "at://did:plc:a/com.bytescape.story/p");
    }

    #[test]
    fn state_classification() {
        let mut i = item("story", "d", "u", "2026-11-01T00:00:00+00:00", None);
        assert_eq!(story_state(&i), "active");
        i.expires_at = None;
        assert_eq!(story_state(&i), "never");
        i.active_from = Some("2099-01-01T00:00:00+00:00".into());
        assert_eq!(story_state(&i), "scheduled");
    }

    #[test]
    fn catchup_parses_listrecords_story() {
        let v = serde_json::json!({
            "uri": "at://did:plc:x/com.bytescape.story/abc",
            "value": {
                "createdAt": "2026-11-01T12:00:00Z",
                "expiresAt": "2026-11-02T12:00:00Z",
                "text": "hi",
                "pinned": true,
                "items": [{"image": {"ref": {"$link": "bafy1"}, "mimeType": "image/jpeg"}, "alt": "a", "duration": 5}]
            }
        });
        // listRecords wraps the record in "value".
        let rec = v.get("value").unwrap();
        let (_u, created, _af, exp, pinned, text, items, cr, cp) = story_from_json(rec).unwrap();
        assert_eq!(created.to_rfc3339(), "2026-11-01T12:00:00+00:00");
        assert_eq!(exp.unwrap().to_rfc3339(), "2026-11-02T12:00:00+00:00");
        assert!(pinned && cr && cp);
        assert_eq!(text.as_deref(), Some("hi"));
        assert_eq!(items[0]["cid"], "bafy1");
        // Malformed (no items) is rejected.
        assert!(story_from_json(&serde_json::json!({"createdAt": "2026-11-01T12:00:00Z", "items": []})).is_none());
    }

    /// Revoked/no-auth case: the transition hides the story, retries, and at
    /// the cap reduces the row to metadata with a stuck warning. Media copy
    /// is skipped when blobs are unreachable (no partial public state).
    #[tokio::test]
    async fn transition_caps_without_write_auth() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("DATABASE_URL unset — skipping DB integration test");
            return;
        };
        let pool = PgPool::connect(&url).await.expect("db connect");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let did = format!("did:plc:noauth{}", stamp);
        let uri = format!("at://{}/com.bytescape.story/x1", did);

        // Expired story with a media entry pointing at an unreachable blob.
        sqlx::query(
            "INSERT INTO appview_stories (uri, cid, author_did, created_at, expires_at, media)
             VALUES ($1, 'c', $2, now() - interval '2 days', now() - interval '1 hour',
                     '[{\"type\":\"image\",\"cid\":\"bafyUNREACHABLE\",\"mime_type\":\"image/jpeg\",\"alt\":\"\"}]')",
        )
        .bind(&uri)
        .bind(&did)
        .execute(&pool)
        .await
        .unwrap();

        // Read-time filter hides it immediately (the guarantee).
        let visible: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM appview_stories WHERE uri = $1 AND NOT archived
             AND (expires_at IS NULL OR expires_at > now())",
        )
        .bind(&uri)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(visible, 0, "expired story hidden at read time");

        // Sweep until the attempts cap trips (copy fails each round — the
        // blob is unreachable).
        for _ in 0..5 {
            run_story_transitions(&pool).await.expect("sweep");
        }
        let (archived, attempts): (bool, i32) = sqlx::query_as(
            "SELECT archived, transition_attempts FROM appview_stories WHERE uri = $1",
        )
        .bind(&uri)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(attempts, 5, "each sweep recorded an attempt");
        assert!(!archived, "no archive without a verified media copy");
        // Cleanup.
        sqlx::query("DELETE FROM appview_stories WHERE uri = $1")
            .bind(&uri)
            .execute(&pool)
            .await
            .unwrap();
    }
}
