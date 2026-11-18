//! ActivityPub follow-graph collections (identity-graph-unification Phase 1):
//! `GET /@{username}/followers` and `GET /@{username}/following`, served as
//! AS2 `OrderedCollection` + bounded `OrderedCollectionPage`s.
//!
//! Mastodon marks an account's collections hidden unless BOTH collections
//! carry `first` (mastodon/mastodon#35311) — that was the owner's "This user
//! has chosen to not make this information available" bug: the advertised
//! URLs 404'd, so Mastodon hid them.
//!
//! Visibility (`users.graph_visibility`, default `public`):
//! * `public` — full pagination: `totalItems`, `first`/`last` page entry
//!   points, numbered pages with `orderedItems`.
//! * `private` — the collection URL stays dereferenceable but answers with
//!   ONLY `{id, type}`: no `first`, no `items`, no `totalItems`. Absent
//!   `first` is exactly Mastodon's native hidden-collection marker, and with
//!   no `totalItems` no follow counts leak. Enforcement is backend-only this
//!   phase (the settings toggle and Mastodon hide_collections parity follow).
//!
//! Items are embedded actor summaries (`id` + `type` + display fields joined
//! from `ap_remote_actors`); consumers that only need identities read `id`
//! (Mastodon's `value_or_id` accepts embedded objects and bare strings alike).
//!
//! Served rows:
//! * followers — accepted inbound AP follows (`followers` rows).
//! * following — AP-representable outbound follows: `follows` rows whose
//!   `followed_actor` is a canonical http(s) actor URI — native/`ap`/
//!   `mastodon_import` rows qualify; Bluesky mirror rows
//!   (`bsky.app/profile/…`) are not AP actors and stay out, as do pending
//!   (not-yet-accepted) outbound follows.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::ap::outbox::jsonld;
use crate::db::users::UserRow;
use crate::error::AppError;
use crate::AppState;

/// Items per collection page (matches the outbox).
const PAGE_SIZE: i64 = 20;

#[derive(Debug, Deserialize)]
pub struct CollectionQuery {
    /// Absent → the collection document. `first`/`1`/`2`… → that page;
    /// `last` → the final page. Anything else falls back to the collection
    /// document (never an error — collection consumers link-hop blindly).
    pub page: Option<String>,
}

#[derive(Clone, Copy)]
enum Kind {
    Followers,
    Following,
}

impl Kind {
    fn segment(self) -> &'static str {
        match self {
            Kind::Followers => "followers",
            Kind::Following => "following",
        }
    }
}

enum PageSel {
    Collection,
    Page(i64),
    Last,
}

fn parse_page_sel(raw: Option<&str>) -> PageSel {
    match raw {
        None => PageSel::Collection,
        Some("first") => PageSel::Page(1),
        Some("last") => PageSel::Last,
        Some(s) => match s.parse::<i64>() {
            Ok(n) if n >= 1 => PageSel::Page(n),
            _ => PageSel::Collection,
        },
    }
}

/// GET /@{username}/followers (and the `/users/` `/actors/` alias forms —
/// cached alias actor ids derive collection URLs from the id they cached,
/// same as the outbox/inbox aliases).
pub async fn followers_handler(
    State(state): State<AppState>,
    Path(username): Path<String>,
    Query(query): Query<CollectionQuery>,
) -> Result<Response, AppError> {
    collection_handler(state, username, Kind::Followers, query).await
}

/// GET /@{username}/following (and the `/users/` `/actors/` alias forms).
pub async fn following_handler(
    State(state): State<AppState>,
    Path(username): Path<String>,
    Query(query): Query<CollectionQuery>,
) -> Result<Response, AppError> {
    collection_handler(state, username, Kind::Following, query).await
}

async fn collection_handler(
    state: AppState,
    username: String,
    kind: Kind,
    query: CollectionQuery,
) -> Result<Response, AppError> {
    if !crate::ap::ap_actors_enabled() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let user = UserRow::find_ap_enabled_by_username(&state.db, &username)
        .await
        .map_err(|e| AppError::Internal(format!("db: {e}")))?
        .ok_or_else(|| AppError::NotFound("no such actor".into()))?;
    let actor_id = user.ap_actor_id.clone().unwrap_or_else(|| {
        crate::ap::actor::canonical_actor_url(&state.config.domain, &user.username)
    });
    let url = format!("{actor_id}/{}", kind.segment());

    // Private graphs disclose nothing — including on page URLs: the hidden
    // document answers every request shape identically.
    if user.graph_visibility != "public" {
        return Ok(jsonld(hidden_collection_doc(&url)));
    }

    // Phase 6: when the home instance hides its collections, our AP surface
    // must not overstate it — the home-derived rows (mastodon_import /
    // home_rest mirrors) drop out of the served graph until the home says
    // otherwise.
    let home_hidden = crate::api::follow::home_hide_state(&state, user.id)
        .await
        .map(|(_, h)| h == Some(true))
        .unwrap_or(false);
    let home_filter: &str = if home_hidden {
        " AND follow_type NOT IN ('mastodon_import', 'home_rest')"
    } else {
        ""
    };

    let total = count_rows(&state, user.id, kind, home_filter).await?;
    let pages = ((total + PAGE_SIZE - 1) / PAGE_SIZE).max(1);

    let page = match parse_page_sel(query.page.as_deref()) {
        PageSel::Collection => return Ok(jsonld(collection_doc(&url, total, pages))),
        PageSel::Page(n) => n.min(pages),
        PageSel::Last => pages,
    };

    let offset = (page - 1) * PAGE_SIZE;
    let rows = page_rows(&state, user.id, kind, offset, home_filter).await?;
    let has_next = (offset + rows.len() as i64) < total;
    let items = rows.iter().map(item_for).collect();
    Ok(jsonld(page_doc(&url, page, items, has_next)))
}

/// One served identity: the actor's canonical id plus cached display data.
struct Row {
    actor_id: String,
    display_name: Option<String>,
    preferred_username: Option<String>,
    avatar_url: Option<String>,
}

fn item_for(row: &Row) -> Value {
    let mut item = json!({ "id": row.actor_id, "type": "Person" });
    if let Some(name) = row.display_name.as_deref().filter(|s| !s.is_empty()) {
        item["name"] = json!(name);
    }
    if let Some(uname) = row.preferred_username.as_deref().filter(|s| !s.is_empty()) {
        item["preferredUsername"] = json!(uname);
    }
    if let Some(avatar) = row.avatar_url.as_deref().filter(|s| !s.is_empty()) {
        item["icon"] = json!({ "type": "Image", "url": avatar });
    }
    item
}

fn collection_doc(url: &str, total: i64, pages: i64) -> Value {
    json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": url,
        "type": "OrderedCollection",
        "totalItems": total,
        "first": { "id": format!("{url}?page=1"), "type": "OrderedCollectionPage" },
        "last": { "id": format!("{url}?page={pages}"), "type": "OrderedCollectionPage" },
    })
}

/// The hidden rendering for `graph_visibility != 'public'`: identity only.
/// No `first` (Mastodon's hidden marker), no items, no `totalItems` counts.
fn hidden_collection_doc(url: &str) -> Value {
    json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": url,
        "type": "OrderedCollection",
    })
}

fn page_doc(url: &str, page: i64, items: Vec<Value>, has_next: bool) -> Value {
    let mut doc = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{url}?page={page}"),
        "type": "OrderedCollectionPage",
        "partOf": url,
        "orderedItems": items,
    });
    if has_next {
        doc["next"] = json!(format!("{url}?page={}", page + 1));
    }
    if page > 1 {
        doc["prev"] = json!(format!("{url}?page={}", page - 1));
    }
    doc
}

/// Exact served size — the same predicates as [`page_rows`].
async fn count_rows(
    state: &AppState,
    user_id: uuid::Uuid,
    kind: Kind,
    home_filter: &str,
) -> Result<i64, AppError> {
    let sql = match kind {
        Kind::Followers => {
            "SELECT COUNT(*) FROM followers WHERE user_id = $1 AND accepted".to_string()
        }
        Kind::Following => {
            format!(
                "SELECT COUNT(*) FROM follows \
                 WHERE follower_id = $1 AND status = 'accepted' \
                   AND follow_type <> 'bluesky_import' \
                   AND followed_actor LIKE 'http%' \
                   AND followed_actor NOT LIKE '%bsky.app/profile/%'{home_filter}"
            )
        }
    };
    sqlx::query_scalar(&sql)
        .bind(user_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Internal(format!("db: {e}")))
}

/// One page of rows, newest follows first (stable on the uuid tiebreak),
/// with display data joined from the `ap_remote_actors` cache.
async fn page_rows(
    state: &AppState,
    user_id: uuid::Uuid,
    kind: Kind,
    offset: i64,
    home_filter: &str,
) -> Result<Vec<Row>, AppError> {
    let sql = match kind {
        Kind::Followers => {
            "SELECT f.actor_id, r.display_name, r.preferred_username, r.avatar_url \
             FROM followers f LEFT JOIN ap_remote_actors r ON r.actor_id = f.actor_id \
             WHERE f.user_id = $1 AND f.accepted \
             ORDER BY f.created_at DESC, f.id DESC LIMIT $2 OFFSET $3"
                .to_string()
        }
        Kind::Following => {
            format!(
                "SELECT f.followed_actor, r.display_name, r.preferred_username, r.avatar_url \
                 FROM follows f LEFT JOIN ap_remote_actors r ON r.actor_id = f.followed_actor \
                 WHERE f.follower_id = $1 AND f.status = 'accepted' \
                   AND f.follow_type <> 'bluesky_import' \
                   AND f.followed_actor LIKE 'http%' \
                   AND f.followed_actor NOT LIKE '%bsky.app/profile/%'{home_filter} \
                 ORDER BY f.created_at DESC, f.id DESC LIMIT $2 OFFSET $3"
            )
        }
    };
    let rows: Vec<(String, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(&sql)
            .bind(user_id)
            .bind(PAGE_SIZE)
            .bind(offset)
            .fetch_all(&state.db)
            .await
            .map_err(|e| AppError::Internal(format!("db: {e}")))?;
    Ok(rows
        .into_iter()
        .map(|(actor_id, display_name, preferred_username, avatar_url)| Row {
            actor_id,
            display_name,
            preferred_username,
            avatar_url,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::ServiceExt;

    use crate::ap::testutil::{connect_pool, enable_ap_flags, make_local_actor, test_state, unique};
    use crate::db::users::UserRow;

    #[test]
    fn hidden_collection_discloses_nothing() {
        let doc = hidden_collection_doc("https://social.test/@a/followers");
        let obj = doc.as_object().expect("object");
        assert_eq!(
            obj.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["@context", "id", "type"],
            "hidden collection carries ONLY id/type — no first, items, or counts: {doc}"
        );
        assert!(doc.get("first").is_none(), "no first: {doc}");
        assert!(doc.get("totalItems").is_none(), "no totalItems: {doc}");
    }

    #[test]
    fn public_collection_carries_first_on_both_entry_points() {
        let doc = collection_doc("https://social.test/@a/followers", 45, 3);
        assert_eq!(doc["type"], "OrderedCollection");
        assert_eq!(doc["totalItems"], 45);
        assert_eq!(doc["first"]["id"], "https://social.test/@a/followers?page=1");
        assert_eq!(doc["last"]["id"], "https://social.test/@a/followers?page=3");
    }

    #[test]
    fn page_links_follow_position() {
        let doc = page_doc("https://social.test/@a/following", 1, vec![], true);
        assert_eq!(doc["partOf"], "https://social.test/@a/following");
        assert_eq!(doc["next"], "https://social.test/@a/following?page=2");
        assert!(doc.get("prev").is_none(), "first page has no prev: {doc}");

        let doc = page_doc("https://social.test/@a/following", 3, vec![], false);
        assert_eq!(doc["prev"], "https://social.test/@a/following?page=2");
        assert!(doc.get("next").is_none(), "last page has no next: {doc}");
    }

    #[test]
    fn items_carry_id_plus_cached_display_data() {
        let bare = item_for(&Row {
            actor_id: "https://remote.test/users/x".into(),
            display_name: None,
            preferred_username: None,
            avatar_url: None,
        });
        assert_eq!(bare["id"], "https://remote.test/users/x");
        assert_eq!(bare["type"], "Person");
        assert!(bare.get("name").is_none());

        let full = item_for(&Row {
            actor_id: "https://remote.test/users/x".into(),
            display_name: Some("X".into()),
            preferred_username: Some("x".into()),
            avatar_url: Some("https://remote.test/a.png".into()),
        });
        assert_eq!(full["name"], "X");
        assert_eq!(full["preferredUsername"], "x");
        assert_eq!(full["icon"]["url"], "https://remote.test/a.png");
    }

    async fn get_doc(app: &axum::Router, path: &str, domain: &str) -> Value {
        let req = Request::builder()
            .method("GET")
            .uri(path)
            .header("host", domain)
            .body(Body::empty())
            .expect("request");
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status().as_u16(),
            200,
            "GET {path} serves the collection"
        );
        let bytes = to_bytes(resp.into_body(), 2 * 1024 * 1024)
            .await
            .expect("body");
        serde_json::from_slice(&bytes).expect("activity+json document")
    }

    /// DB integration (skips without DATABASE_URL): a public graph serves
    /// paginated collections with `first` on BOTH entry points — the exact
    /// mastodon/mastodon#35311 requirement the owner's bug tripped.
    #[tokio::test]
    async fn public_graph_serves_paginated_collections_with_first() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = crate::ap::routes().with_state(state);

        let target = make_local_actor(&pool, &domain).await;
        let s = unique();

        // Two inbound followers.
        for i in 0..2 {
            let actor = format!("https://remote.test/users/foll_{s}_{i}");
            sqlx::query(
                "INSERT INTO followers (actor_id, inbox_url, user_id, accepted) \
                 VALUES ($1, $2, $3, true)",
            )
            .bind(&actor)
            .bind(format!("{actor}/inbox"))
            .bind(target.user.id)
            .execute(&pool)
            .await
            .expect("seed follower");
        }
        // One accepted AP follow, plus rows that must NOT be served: a
        // pending (unaccepted) follow and a Bluesky mirror row.
        for (ty, status, actor) in [
            ("ap", "accepted", format!("https://remote.test/users/foll_{s}_0")),
            ("ap", "pending", format!("https://remote.test/users/pend_{s}")),
            (
                "bluesky_import",
                "accepted",
                format!("https://bsky.app/profile/bsky_{s}.test"),
            ),
        ] {
            sqlx::query(
                "INSERT INTO follows (follower_id, followed_actor, follow_type, status) \
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(target.user.id)
            .bind(actor)
            .bind(ty)
            .bind(status)
            .execute(&pool)
            .await
            .expect("seed follow");
        }

        let coll = get_doc(&app, &format!("/@{}/followers", target.username), &domain).await;
        assert_eq!(coll["type"], "OrderedCollection");
        assert_eq!(coll["totalItems"], 2);
        let followers_url = coll["id"].as_str().unwrap().to_string();
        assert!(
            coll["first"]["id"].as_str().unwrap().ends_with("?page=1"),
            "followers carry first: {coll}"
        );

        let coll = get_doc(&app, &format!("/@{}/following", target.username), &domain).await;
        assert_eq!(
            coll["totalItems"], 1,
            "pending + Bluesky rows are not served: {coll}"
        );
        assert!(
            coll["first"]["id"].as_str().unwrap().ends_with("?page=1"),
            "following carry first: {coll}"
        );

        let page = get_doc(
            &app,
            &format!("/@{}/followers?page=1", target.username),
            &domain,
        )
        .await;
        assert_eq!(page["type"], "OrderedCollectionPage");
        assert_eq!(page["partOf"], followers_url);
        let items = page["orderedItems"].as_array().expect("items");
        assert_eq!(items.len(), 2);
        assert!(items[0]["id"]
            .as_str()
            .unwrap()
            .starts_with("https://remote.test/users/foll_"));
        assert!(page.get("next").is_none(), "under one page: {page}");

        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(target.user.id)
            .execute(&pool)
            .await;
    }

    /// DB integration: `graph_visibility = 'private'` answers with the bare
    /// hidden document on every request shape — no first, no items, no
    /// counts, even when a page is requested directly.
    #[tokio::test]
    async fn private_graph_serves_the_hidden_state() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = crate::ap::routes().with_state(state);

        let target = make_local_actor(&pool, &domain).await;
        let actor = format!("https://remote.test/users/priv_{}", unique());
        sqlx::query(
            "INSERT INTO followers (actor_id, inbox_url, user_id, accepted) VALUES ($1, $2, $3, true)",
        )
        .bind(&actor)
        .bind(format!("{actor}/inbox"))
        .bind(target.user.id)
        .execute(&pool)
        .await
        .expect("seed follower");

        UserRow::set_graph_visibility(&pool, target.user.id, "private")
            .await
            .expect("flip to private");

        for path in [
            format!("/@{}/followers", target.username),
            format!("/@{}/following", target.username),
            format!("/@{}/followers?page=1", target.username),
        ] {
            let doc = get_doc(&app, &path, &domain).await;
            assert_eq!(doc["type"], "OrderedCollection");
            assert!(doc.get("first").is_none(), "no first on {path}: {doc}");
            assert!(doc.get("last").is_none(), "no last on {path}: {doc}");
            assert!(doc.get("totalItems").is_none(), "no counts on {path}: {doc}");
            assert!(doc.get("items").is_none(), "no items on {path}: {doc}");
            assert!(doc.get("orderedItems").is_none(), "no items on {path}: {doc}");
        }

        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(target.user.id)
            .execute(&pool)
            .await;
    }

    /// Phase 6: when the home instance hides its collections, the served AP
    /// following collection must not overstate it — home-derived rows drop
    /// out until the home says otherwise.
    #[tokio::test]
    async fn hidden_home_graph_excludes_home_rows_from_collections() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = crate::ap::routes().with_state(state.clone());
        let target = make_local_actor(&pool, &domain).await;

        // A home link that reports hide_collections=true.
        let (base, _heads, thread) = crate::oauth::mastodon::stub_http(
            "HTTP/1.1 200 OK",
            r#"{"id":"9","username":"o","acct":"o","hide_collections":true}"#,
            1,
        );
        crate::db::accounts::LinkedAccountRow::create(
            &pool,
            target.user.id,
            "mastodon",
            &format!("collhome_{}", unique()),
            "collhome",
            "tok",
            Some("refresh"),
            Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            None,
            Some("read"),
            &serde_json::json!({ "instance": base }),
        )
        .await
        .expect("link home");

        let s = unique();
        // One home-derived row (must drop) + one minted-AP row (must stay).
        for (ty, actor) in [
            ("mastodon_import", format!("https://probe.ap.test/users/home_{s}")),
            ("ap", format!("https://probe.ap.test/users/minted_{s}")),
        ] {
            crate::db::follows::FollowRow::create(&pool, target.user.id, &actor, ty, None)
                .await
                .expect("row");
            crate::db::follows::FollowRow::set_accepted(&pool, target.user.id, &actor)
                .await
                .expect("accept");
        }

        let doc = get_doc(&app, &format!("/@{}/following", target.username), &domain).await;
        assert_eq!(doc["totalItems"], 1, "the home row is excluded: {doc}");
        let page = get_doc(
            &app,
            &format!("/@{}/following?page=1", target.username),
            &domain,
        )
        .await;
        let items = page["orderedItems"].as_array().expect("items");
        assert_eq!(items.len(), 1);
        assert!(
            items[0]["id"].as_str().unwrap().contains("minted_"),
            "the minted-AP row survives: {page}"
        );
        let _ = thread.join();

        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(target.user.id)
            .execute(&pool)
            .await;
    }
}
