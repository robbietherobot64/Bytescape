//! API — Fediverse groups (Lemmy/PieFed communities over ActivityPub).
//!
//! GET    /api/groups                 — directory + search (`q`, `instance`, `kind`)
//! GET    /api/groups/{actor_id}      — one group (URL-encoded actor id)
//! GET    /api/groups/{actor_id}/posts — posts of one group (feed shape)
//! POST   /api/groups/{actor_id}/follow — follow a group (local + federated AP Follow)
//! DELETE /api/groups/{actor_id}/follow — unfollow (local + federated Undo Follow)
//! GET    /api/groups/subscriptions   — the caller's followed groups
//! POST   /api/groups/vote            — up/down vote on a group post (federated AP Like/Dislike)
//!
//! Group identity is the canonical AP actor id (`https://instance/c/name`) so
//! a later AT "spaces" layer can coexist (design D1 in
//! openspec/changes/groups-activitypub/design.md). All vote/follow writes are
//! real federated ActivityPub activities via `ap::bridge` — never local-only
//! state presented as federated (design D2).

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::feed::optional_viewer;
use crate::error::AppError;
use crate::{auth, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/groups", get(list_groups))
        .route("/api/groups/subscriptions", get(my_group_subscriptions))
        .route("/api/groups/vote", post(vote_post))
        // The group actor id is a full URL (`https://instance/c/name`), so it
        // travels as an `actor_id` query param rather than a path wildcard —
        // a `{*actor_id}` catch-all would swallow the `/posts` and `/follow`
        // sub-routes (axum route-conflict).
        .route("/api/groups/detail", get(get_group))
        .route("/api/groups/posts", get(group_posts))
        .route("/api/groups/follow", post(follow_group).delete(unfollow_group))
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

/// The group actor id (a full `https://instance/c/name` URL) travels as a
/// query param on the detail/posts/follow endpoints — a path wildcard would
/// collide with the sub-routes (see [`routes`]).
#[derive(Deserialize)]
struct ActorIdQuery {
    actor_id: String,
}

// ─── Views ─────────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
pub struct GroupView {
    pub actor_id: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub instance: String,
    pub kind: String,
    pub nsfw: bool,
    /// Posts indexed under this group (cheap correlated count, capped).
    pub post_count: i64,
    /// Whether the caller follows this group (subscriptions row) — `false`
    /// for anonymous callers.
    pub following: bool,
}

#[derive(Serialize, Clone)]
struct GroupPostCounts {
    upvotes: i32,
    downvotes: i32,
    comments: i32,
    /// `upvotes / max(1, upvotes + downvotes)`, rounded to 2 decimals.
    /// `None` when no votes exist at all — the UI renders `—`, never `0.00`
    /// (which would imply data) (design D3).
    ratio: Option<f64>,
    /// The caller's own standing vote on this post (`"up"`, `"down"`, `None`)
    /// — read back from `ap_activities`, never faked (design D2).
    my_vote: Option<String>,
}

// ─── Group directory + search ──────────────────────────────────────────────

#[derive(Deserialize)]
struct ListGroupsQuery {
    q: Option<String>,
    instance: Option<String>,
    kind: Option<String>,
    limit: Option<i64>,
}

async fn list_groups(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListGroupsQuery>,
) -> Result<Json<Vec<GroupView>>, AppError> {
    let viewer = optional_viewer(&headers, &state.config.jwt_secret)?;
    let limit = query.limit.unwrap_or(30).clamp(1, 100);
    let rows = search_groups(
        &state.db,
        query.q.as_deref().unwrap_or(""),
        query.instance.as_deref(),
        query.kind.as_deref(),
        limit,
    )
    .await?;
    Ok(Json(with_following(&state.db, rows, viewer).await))
}

/// Search `ap_groups` by name/title (`ILIKE '%q%'`), optionally scoped by
/// instance and kind. Pure function of its arguments — deterministic and
/// unit-testable via the SQL shaping tests below.
async fn search_groups(
    db: &PgPool,
    q: &str,
    instance: Option<&str>,
    kind: Option<&str>,
    limit: i64,
) -> Result<Vec<GroupView>, AppError> {
    let pattern = format!("%{}%", q.replace('%', "\\%").replace('_', "\\_"));
    let rows: Vec<GroupView> = sqlx::query_as(
        r#"SELECT g.actor_id, g.name, g.title, g.description, g.instance, g.kind, g.nsfw,
                  (SELECT COUNT(*) FROM appview_posts p WHERE p.group_actor_id = g.actor_id) AS post_count,
                  FALSE AS following
           FROM ap_groups g
           WHERE ($1 = '' OR g.name ILIKE $2 OR g.title ILIKE $2)
             AND ($3::text IS NULL OR g.instance = $3)
             AND ($4::text IS NULL OR g.kind = $4)
           ORDER BY post_count DESC, g.name
           LIMIT $5"#,
    )
    .bind(q)
    .bind(&pattern)
    .bind(instance)
    .bind(kind)
    .bind(limit)
    .fetch_all(db)
    .await
    .map_err(AppError::Database)?;
    Ok(rows)
}

/// Stamp `following` on a batch of groups for the viewer (one query).
async fn with_following(db: &PgPool, mut rows: Vec<GroupView>, viewer: Option<Uuid>) -> Vec<GroupView> {
    let Some(user_id) = viewer else { return rows };
    let ids: Vec<String> = rows.iter().map(|g| g.actor_id.clone()).collect();
    if ids.is_empty() {
        return rows;
    }
    let following: std::collections::HashSet<String> = sqlx::query_as::<_, (String,)>(
        "SELECT source_id FROM subscriptions \
         WHERE user_id = $1 AND source = 'ap_group' AND category = 'groups' AND enabled = true \
           AND source_id = ANY($2::text[])",
    )
    .bind(user_id)
    .bind(&ids)
    .fetch_all(db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|(id,)| id)
    .collect();
    for row in &mut rows {
        row.following = following.contains(&row.actor_id);
    }
    rows
}

/// One group by its URL-encoded actor id.
async fn get_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ActorIdQuery>,
) -> Result<Json<GroupView>, AppError> {
    let viewer = optional_viewer(&headers, &state.config.jwt_secret)?;
    let actor_id = q.actor_id;
    let rows = sqlx::query_as::<_, GroupView>(
        r#"SELECT g.actor_id, g.name, g.title, g.description, g.instance, g.kind, g.nsfw,
                  (SELECT COUNT(*) FROM appview_posts p WHERE p.group_actor_id = g.actor_id) AS post_count,
                  FALSE AS following
           FROM ap_groups g WHERE g.actor_id = $1"#,
    )
    .bind(&actor_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound("Group not found".into()))?;
    let mut rows = with_following(&state.db, vec![rows], viewer).await;
    Ok(Json(rows.remove(0)))
}

// ─── Group feed ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct GroupPostView {
    #[serde(flatten)]
    post: crate::api::feed::FeedPost,
    group: GroupHeader,
    counts: GroupPostCounts,
}

#[derive(Serialize, Clone)]
struct GroupHeader {
    actor_id: String,
    name: String,
    title: Option<String>,
    instance: String,
    kind: String,
    nsfw: bool,
    following: bool,
}

#[derive(Deserialize)]
struct GroupPostsQuery {
    actor_id: String,
    limit: Option<i64>,
}

async fn group_posts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<GroupPostsQuery>,
) -> Result<Json<crate::api::feed::FeedResponse>, AppError> {
    let viewer = optional_viewer(&headers, &state.config.jwt_secret)?;
    let actor_id = query.actor_id;
    let limit = query.limit.unwrap_or(30).clamp(1, 100);
    // Full FeedPost rows (body, media, embeds, group header, vote counts) via
    // the same normalization path as the groups feed source — the community
    // page renders real GroupPostCards, not a minimal stub projection.
    let mut posts = crate::api::feed::sources::fetch_appview_group(
        &state.db,
        &actor_id,
        limit,
        crate::api::feed::sort::SortMode::New,
    )
    .await;
    if let Some(uid) = viewer {
        stamp_my_votes(&state.db, uid, &mut posts).await;
    }
    // Fire-and-forget count refresh for the community page (item: stale votes).
    let uris: Vec<String> = posts.iter().map(|p| p.id.clone()).collect();
    let db = state.db.clone();
    tokio::spawn(async move {
        crate::ap::public_feed::refresh_counts_for_posts(&db, &uris).await;
    });
    Ok(Json(crate::api::feed::FeedResponse {
        cursor: None,
        has_more: posts.len() as i64 >= limit,
        items: posts,
        phase: None,
        platform_counts: None,
    }))
}

/// Stamp the viewer's standing federated vote (Like → "up", Dislike → "down")
/// on each post, so the group card lights the right vote button (D2).
async fn stamp_my_votes(
    db: &PgPool,
    user_id: Uuid,
    posts: &mut [crate::api::feed::FeedPost],
) {
    let actor_id: String = match sqlx::query_as::<_, (Option<String>,)>(
        "SELECT ap_actor_id FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await
    {
        Ok(Some((Some(a),))) => a,
        _ => return,
    };
    let uris: Vec<String> = posts.iter().map(|p| p.id.clone()).collect();
    if uris.is_empty() {
        return;
    }
    let Ok(votes) = sqlx::query_as::<_, (String, String)>(
        "SELECT DISTINCT ON (object_id) object_id, activity_type FROM ap_activities \
         WHERE actor_id = $1 AND activity_type IN ('Like','Dislike') AND undone_at IS NULL \
           AND object_id = ANY($2::text[]) \
         ORDER BY object_id, created_at DESC, id DESC",
    )
    .bind(&actor_id)
    .bind(&uris)
    .fetch_all(db)
    .await
    else {
        return;
    };
    let map: std::collections::HashMap<String, String> = votes.into_iter().collect();
    for p in posts.iter_mut() {
        if let Some(t) = map.get(&p.id) {
            p.my_vote = Some(if t == "Like" { "up" } else { "down" }.to_string());
        }
    }
}

/// Raw `(uri, upvotes, downvotes, comment_count)` rows for one group, newest
/// first. Split out so the shaping logic (`build_group_posts`) is unit-
/// testable without a DB.
async fn fetch_group_posts(
    db: &PgPool,
    actor_id: &str,
    limit: i64,
) -> Result<Vec<(String, i32, i32, i32)>, AppError> {
    let rows = sqlx::query_as::<_, (String, i32, i32, i32)>(
        "SELECT uri, upvote_count, downvote_count, comment_count \
         FROM appview_posts \
         WHERE group_actor_id = $1 AND reply_parent IS NULL \
         ORDER BY created_at DESC NULLS LAST LIMIT $2",
    )
    .bind(actor_id)
    .bind(limit)
    .fetch_all(db)
    .await
    .map_err(AppError::Database)?;
    Ok(rows)
}

async fn group_header(db: &PgPool, actor_id: &str) -> Result<GroupHeader, AppError> {
    let row = sqlx::query_as::<_, (String, String, Option<String>, String, String, bool)>(
        "SELECT actor_id, name, title, instance, kind, nsfw FROM ap_groups WHERE actor_id = $1",
    )
    .bind(actor_id)
    .fetch_optional(db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound("Group not found".into()))?;
    let (actor_id, name, title, instance, kind, nsfw) = row;
    Ok(GroupHeader {
        actor_id,
        name,
        title,
        instance,
        kind,
        nsfw,
        following: false,
    })
}

/// `(upvotes, downvotes, comments, my_vote)` per post uri for one group.
/// `my_vote` is read back from `ap_activities` — the standing Like/Dislike by
/// the viewer's actor (design D2: never fake a vote).
async fn vote_counts_for(
    db: &PgPool,
    actor_id: &str,
    viewer: Option<Uuid>,
    limit: i64,
) -> Result<std::collections::HashMap<String, GroupPostCounts>, AppError> {
    // Bounded to the same recency window as `fetch_group_posts` so a huge
    // community never scans its entire history here (the counts map only needs
    // to cover the posts actually returned).
    let rows = sqlx::query_as::<_, (String, i32, i32, i32)>(
        "SELECT uri, upvote_count, downvote_count, comment_count \
         FROM appview_posts WHERE group_actor_id = $1 AND reply_parent IS NULL \
         ORDER BY created_at DESC NULLS LAST LIMIT $2",
    )
    .bind(actor_id)
    .bind(limit)
    .fetch_all(db)
    .await
    .map_err(AppError::Database)?;
    let mut map = std::collections::HashMap::new();
    for (uri, up, down, comments) in rows {
        map.insert(
            uri,
            GroupPostCounts {
                upvotes: up,
                downvotes: down,
                comments,
                ratio: ratio(up, down),
                my_vote: None,
            },
        );
    }
    if let Some(user_id) = viewer {
        // One query for the viewer's standing votes across the group's posts.
        let actor: Option<(Option<String>,)> =
            sqlx::query_as("SELECT ap_actor_id FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(db)
                .await
                .map_err(AppError::Database)?;
        if let Some((Some(actor_id),)) = actor {
            let votes = sqlx::query_as::<_, (String, String)>(
                "SELECT DISTINCT ON (object_id) object_id, activity_type \
                 FROM ap_activities \
                 WHERE actor_id = $1 AND activity_type IN ('Like', 'Dislike') \
                   AND undone_at IS NULL AND object_id = ANY($2::text[]) \
                 ORDER BY object_id, created_at DESC, id DESC",
            )
            .bind(&actor_id)
            .bind(map.keys().cloned().collect::<Vec<_>>())
            .fetch_all(db)
            .await
            .map_err(AppError::Database)?;
            for (uri, activity_type) in votes {
                if let Some(entry) = map.get_mut(&uri) {
                    entry.my_vote = Some(match activity_type.as_str() {
                        "Like" => "up".to_string(),
                        "Dislike" => "down".to_string(),
                        _ => continue,
                    });
                }
            }
        }
    }
    Ok(map)
}

/// Shape group posts: attach the group header and the per-post vote counts.
/// Pure — the only inputs are the fetched rows, the header, and the counts
/// map, so it is directly unit-testable.
fn build_group_posts(
    rows: Vec<(String, i32, i32, i32)>,
    group: GroupHeader,
    counts: std::collections::HashMap<String, GroupPostCounts>,
) -> Vec<GroupPostView> {
    rows.into_iter()
        .filter_map(|(uri, up, down, comments)| {
            let counts = counts.get(&uri).cloned().unwrap_or(GroupPostCounts {
                upvotes: up,
                downvotes: down,
                comments,
                ratio: ratio(up, down),
                my_vote: None,
            });
            // The post body itself is rendered by the shared PostCard path on
            // the client; here we only need the identity + counts. A minimal
            // FeedPost-shaped stub keeps the response type stable for the
            // frontend (`GroupPostView.post` carries id/url/counts).
            let post = crate::api::feed::FeedPost {
                id: uri.clone(),
                source: "group".to_string(),
                author: crate::api::feed::AuthorInfo {
                    handle: String::new(),
                    display_name: None,
                    avatar: None,
                },
                content: String::new(),
                content_type: "short".to_string(),
                media: vec![],
                embeds: vec![],
                likes_count: up.max(0) as u32,
                shares_count: 0,
                replies_count: comments.max(0) as u32,
                created_at: String::new(),
                url: Some(uri),
                thread_id: None,
                start_time: None,
                end_time: None,
                location: None,
                display_name: None,
                action_text: None,
                group_name: Some(group.name.clone()),
                group: None,
                upvotes: None,
                downvotes: None,
                my_vote: None,
                reaction_counts: None,
                user_reaction: None,
                cid: None,
                author_url: None,
                reposted_by: None,
                boosted_by: None,
                quoted_post_uri: None,
                is_reshare: Some(false),
                original_post: None,
                is_own: None,
                is_following: None,
                follow_pending: None,
                cross_posts: vec![],
                matched_sources: vec![],
                labels: vec![],
                nsfw: false,
                author_external_id: None,
            };
            Some(GroupPostView {
                post,
                group: group.clone(),
                counts,
            })
        })
        .collect()
}

/// Upvote ratio: `upvotes / max(1, upvotes + downvotes)`, rounded to 2
/// decimals. `None` when there are no votes at all (design D3 — `0.00` would
/// imply data where there is none).
pub fn ratio(upvotes: i32, downvotes: i32) -> Option<f64> {
    let total = (upvotes.max(0) + downvotes.max(0)) as f64;
    if total == 0.0 {
        return None;
    }
    let r = upvotes.max(0) as f64 / total;
    Some((r * 100.0).round() / 100.0)
}

// ─── Subscriptions (follow/unfollow) ───────────────────────────────────────

#[derive(Deserialize)]
struct FollowReq {
    /// The group's canonical AP actor id.
    actor_id: String,
    /// Group name/title hint for the subscription label (optional; falls back
    /// to the registry name).
    label: Option<String>,
}

#[derive(Serialize)]
struct FollowResp {
    following: bool,
    /// `true` when a federated AP `Follow` was queued to the community inbox;
    /// `false` when the follow is local-only (no AP actor on the account yet)
    /// — the UI shows an honest tooltip for this case (design D2).
    federated: bool,
    actor_id: String,
}

async fn follow_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<FollowReq>,
) -> Result<Json<FollowResp>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let actor_id = body.actor_id;
    let group = group_header(&state.db, &actor_id).await?;
    let label = body
        .label
        .or_else(|| group.title.clone())
        .unwrap_or_else(|| group.name.clone());
    // Local subscription row first: the Subscriptions feed works immediately,
    // even before a remote Accept lands (design D4).
    crate::db::subscriptions::SubscriptionRow::create(
        &state.db,
        user_id,
        "ap_group",
        &actor_id,
        &label,
        "👥",
        "groups",
    )
    .await
    .map_err(AppError::Database)?;
    // Federated Follow to the community inbox — queued via the existing
    // signed-POST pipeline (design D2). Best-effort: a failure here never
    // blocks the local subscription.
    let federated = match crate::ap::bridge::ui_follow(
        &state.db,
        &state.config.jwt_secret,
        user_id,
        &actor_id,
    )
    .await
    {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!("[groups] federated follow of {actor_id} failed (local sub kept): {e}");
            false
        }
    };
    Ok(Json(FollowResp {
        following: true,
        federated,
        actor_id,
    }))
}

async fn unfollow_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ActorIdQuery>,
) -> Result<Json<FollowResp>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let actor_id = q.actor_id;
    let row = crate::db::subscriptions::SubscriptionRow::find_by_user(&state.db, user_id)
        .await
        .map_err(AppError::Database)?
        .into_iter()
        .find(|r| r.source == "ap_group" && r.source_id == actor_id);
    if let Some(row) = row {
        crate::db::subscriptions::SubscriptionRow::delete(&state.db, row.id, user_id)
            .await
            .map_err(AppError::Database)?;
    }
    // Federated Undo Follow (best-effort — a local-only follow has nothing
    // to undo and `ui_unfollow` no-ops).
    let federated = crate::ap::bridge::ui_unfollow(&state.db, user_id, &actor_id)
        .await
        .unwrap_or(false);
    Ok(Json(FollowResp {
        following: false,
        federated,
        actor_id,
    }))
}

#[derive(Serialize)]
struct SubscriptionGroupView {
    actor_id: String,
    name: String,
    title: Option<String>,
    instance: String,
    kind: String,
    label: String,
}

async fn my_group_subscriptions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SubscriptionGroupView>>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let subs = crate::db::subscriptions::SubscriptionRow::find_by_user_and_category(
        &state.db,
        user_id,
        "groups",
    )
    .await
    .map_err(AppError::Database)?;
    let actor_ids: Vec<String> = subs
        .iter()
        .filter(|s| s.source == "ap_group" && s.enabled)
        .map(|s| s.source_id.clone())
        .collect();
    if actor_ids.is_empty() {
        return Ok(Json(vec![]));
    }
    let groups = sqlx::query_as::<_, (String, String, Option<String>, String, String)>(
        "SELECT actor_id, name, title, instance, kind FROM ap_groups WHERE actor_id = ANY($1::text[])",
    )
    .bind(&actor_ids)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;
    let label_for = |actor_id: &str| -> String {
        subs.iter()
            .find(|s| s.source_id == actor_id)
            .map(|s| s.label.clone())
            .unwrap_or_default()
    };
    Ok(Json(
        groups
            .into_iter()
            .map(|(actor_id, name, title, instance, kind)| SubscriptionGroupView {
                label: label_for(&actor_id),
                actor_id,
                name,
                title,
                instance,
                kind,
            })
            .collect(),
    ))
}

// ─── Votes ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct VoteReq {
    post_uri: String,
    /// `"up"` or `"down"`; any other value clears the caller's vote (undo).
    vote: String,
}

#[derive(Serialize)]
struct VoteResp {
    upvotes: i32,
    downvotes: i32,
    ratio: Option<f64>,
    my_vote: Option<String>,
    /// Whether a federated activity was dispatched (`Like`/`Dislike`/`Undo`).
    federated: bool,
}

async fn vote_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<VoteReq>,
) -> Result<Json<VoteResp>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    // Real federated votes (design D2). An account with no AP actor yet gets
    // one minted on demand (same policy as the existing reaction surface).
    crate::api::feed::provision_ap_actor(&state, user_id).await?;
    // Retraction window (migration 092): a vote landing within 30s of a
    // retraction of the same object can only be the trailing half of a
    // double-click race — never a fresh vote. Skip re-federation and report
    // the standing state honestly (same contract as the emoji reaction path).
    if req.vote != "clear"
        && crate::api::feed::recently_retracted(&state.db, user_id, &req.post_uri).await?
    {
        let my_vote = crate::ap::bridge::ui_vote_state(&state.db, user_id, &req.post_uri)
            .await
            .map_err(|e| AppError::Internal(format!("AP vote state: {e}")))?
            .map(|v| v.to_string());
        let (up, down) = sqlx::query_as::<_, (i32, i32)>(
            "SELECT upvote_count, downvote_count FROM appview_posts WHERE uri = $1",
        )
        .bind(&req.post_uri)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::Database)?
        .unwrap_or((0, 0));
        return Ok(Json(VoteResp {
            upvotes: up,
            downvotes: down,
            ratio: ratio(up, down),
            my_vote,
            federated: false,
        }));
    }
    let (federated, my_vote) = match req.vote.as_str() {
        "up" => {
            crate::ap::bridge::ui_unlike(&state.db, user_id, &req.post_uri)
                .await
                .map_err(|e| AppError::Internal(format!("AP undislike: {e}")))?;
            crate::ap::bridge::ui_undislike(&state.db, user_id, &req.post_uri)
                .await
                .map_err(|e| AppError::Internal(format!("AP undislike: {e}")))?;
            crate::ap::bridge::ui_like(&state.db, user_id, &req.post_uri)
                .await
                .map_err(|e| AppError::Internal(format!("AP like: {e}")))?;
            (true, Some("up".to_string()))
        }
        "down" => {
            crate::ap::bridge::ui_unlike(&state.db, user_id, &req.post_uri)
                .await
                .map_err(|e| AppError::Internal(format!("AP unlike: {e}")))?;
            crate::ap::bridge::ui_dislike(&state.db, user_id, &req.post_uri)
                .await
                .map_err(|e| AppError::Internal(format!("AP dislike: {e}")))?;
            (true, Some("down".to_string()))
        }
        _ => {
            let a = crate::ap::bridge::ui_unlike(&state.db, user_id, &req.post_uri)
                .await
                .map_err(|e| AppError::Internal(format!("AP unlike: {e}")))?;
            let b = crate::ap::bridge::ui_undislike(&state.db, user_id, &req.post_uri)
                .await
                .map_err(|e| AppError::Internal(format!("AP undislike: {e}")))?;
            (a || b, None)
        }
    };
    // Read the post's stored counts back (fetched-at-ingest numbers are the
    // honest ratio; a local vote only lights `my_vote`, it never inflates the
    // ratio — the remote's own counts are what show).
    let (up, down) = sqlx::query_as::<_, (i32, i32)>(
        "SELECT upvote_count, downvote_count FROM appview_posts WHERE uri = $1",
    )
    .bind(&req.post_uri)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .unwrap_or((0, 0));
    Ok(Json(VoteResp {
        upvotes: up,
        downvotes: down,
        ratio: ratio(up, down),
        my_vote,
        federated,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratio_math() {
        assert_eq!(ratio(7, 3), Some(0.7));
        assert_eq!(ratio(1, 1), Some(0.5));
        assert_eq!(ratio(0, 0), None, "no votes → no ratio (never 0.00)");
        assert_eq!(ratio(0, 5), Some(0.0), "all downvotes is a real 0.0");
        assert_eq!(ratio(-2, -3), None, "negatives clamp to zero total");
        assert_eq!(ratio(2, 1), Some(0.67), "rounded to 2 decimals");
    }

    #[test]
    fn parse_group_ref_requires_actor_id_and_name() {
        let good = serde_json::json!({
            "actor_id": "https://lemmy.world/c/technology",
            "name": "technology",
            "title": "Technology",
            "instance": "lemmy.world",
            "kind": "lemmy",
            "nsfw": false
        });
        let parsed = crate::ap::ingest::parse_group_ref(&good).expect("parses");
        assert_eq!(parsed.actor_id, "https://lemmy.world/c/technology");
        assert_eq!(parsed.name, "technology");
        assert_eq!(parsed.instance, "lemmy.world");
        assert!(!parsed.nsfw);

        assert!(crate::ap::ingest::parse_group_ref(&serde_json::json!({"name": "x"})).is_none());
        assert!(
            crate::ap::ingest::parse_group_ref(&serde_json::json!({"actor_id": "", "name": "x"}))
                .is_none()
        );
    }
}
