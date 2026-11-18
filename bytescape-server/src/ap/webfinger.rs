//! ActivityPub discovery — WebFinger + NodeInfo.
//!
//! WebFinger resolves `acct:{username}@{domain}` to the canonical actor id plus
//! its alias forms (reclaim reconciliation: any cached alias id is listed so
//! remote servers converge on the canonical actor).

use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::actor::{alias_actor_urls, canonical_actor_url};
use super::ap_actors_enabled;
use crate::db::users::UserRow;
use crate::AppState;

#[derive(Deserialize)]
pub struct WebfingerQuery {
    pub resource: String,
}

/// GET /.well-known/webfinger?resource=acct:{username}@{domain}
pub async fn webfinger(
    State(state): State<AppState>,
    Query(q): Query<WebfingerQuery>,
) -> Result<Response, crate::error::AppError> {
    if !ap_actors_enabled() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }

    let domain = &state.config.domain;
    let username = parse_acct(&q.resource, domain)
        .ok_or_else(|| crate::error::AppError::NotFound("unsupported resource".into()))?;

    let user = UserRow::find_ap_enabled_by_username(&state.db, &username)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("db: {e}")))?;

    // A retired actor's handle stops resolving as live the moment retirement
    // starts (add-identity-realignment) — remote signature verification fetches
    // the keyId URL directly, so WebFinger need not cooperate with the drain.
    if crate::ap::retire::load_retired_actor_by_username(&state.db, &username).await.is_some() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }

    let user = user
        .ok_or_else(|| crate::error::AppError::NotFound("no such actor".into()))?;

    let actor_id = user
        .ap_actor_id
        .clone()
        .unwrap_or_else(|| canonical_actor_url(domain, &user.username));

    // subject + self link → canonical actor; aliases listed so a cached alias id
    // still finds the canonical actor.
    let mut aliases = vec![actor_id.clone()];
    aliases.extend(alias_actor_urls(domain, &user.username));

    let doc: Value = json!({
        "subject": format!("acct:{}@{}", user.username, domain),
        "aliases": aliases,
        "links": [
            {
                "rel": "self",
                "type": "application/activity+json",
                "href": actor_id,
            },
            {
                "rel": "http://webfinger.net/rel/profile-page",
                "type": "text/html",
                "href": actor_id,
            },
        ],
    });

    Ok(([(header::CONTENT_TYPE, "application/jrd+json")], Json(doc)).into_response())
}

/// Parse an `acct:user@domain` (or bare `user@domain`) resource into the local
/// username, enforcing that the domain matches ours. Returns None otherwise.
fn parse_acct(resource: &str, domain: &str) -> Option<String> {
    let acct = resource.strip_prefix("acct:").unwrap_or(resource);
    let (user, host) = acct.split_once('@')?;
    if !host.eq_ignore_ascii_case(domain) {
        return None;
    }
    if user.is_empty() {
        return None;
    }
    Some(user.to_string())
}

/// GET /.well-known/nodeinfo — pointer to the NodeInfo document.
pub async fn nodeinfo_pointer(State(state): State<AppState>) -> Response {
    if !ap_actors_enabled() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let domain = &state.config.domain;
    let doc = json!({
        "links": [{
            "rel": "http://nodeinfo.diaspora.software/ns/schema/2.0",
            "href": format!("https://{}/nodeinfo/2.0", domain),
        }],
    });
    Json(doc).into_response()
}

/// GET /nodeinfo/2.0 — instance metadata.
pub async fn nodeinfo(State(state): State<AppState>) -> Response {
    if !ap_actors_enabled() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let domain = &state.config.domain;
    let doc = json!({
        "version": "2.0",
        "software": { "name": "bytescape", "version": "0.1.0" },
        "protocols": ["activitypub"],
        "services": { "inbound": [], "outbound": [] },
        "openRegistrations": true,
        "usage": { "users": { "total": 0 } },
        "metadata": { "domain": domain },
    });
    ([(header::CONTENT_TYPE, "application/json")], Json(doc)).into_response()
}
