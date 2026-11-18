#![allow(dead_code)]
//! REST API for the frontend — routes registered here.

use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    Json,
    routing::get,
};
use serde::Deserialize;
use serde_json::json;
use crate::AppState;
pub mod matrix;
pub mod bluesky_chat;

pub mod auth;
pub mod profile;
pub mod feed;
pub mod feedgen;
pub mod vault;
pub mod identity;
pub mod oauth;
pub mod accounts;
pub mod subscriptions;
pub mod interest_sources;
pub mod prefs;
pub mod events;
pub mod posts;
pub mod rss;
pub mod rss_sync;
pub mod forums;
pub mod follow;
pub mod stories;
pub mod import;
pub mod import_facebook;
pub mod import_instagram;
pub mod import_mastodon;
pub mod og;
pub mod google_import;
pub mod exchange;
pub mod export;
pub mod notifications;
pub mod mute;
pub mod settings;
pub mod groups;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/.well-known/matrix/client", get(matrix::matrix_well_known))
        .route("/caddy/ask", get(caddy_ask))
        .route("/api/health", get(health_check))
        .merge(auth::routes())
        .merge(profile::routes())
        .merge(feed::routes())
        .merge(feedgen::routes())
        .merge(vault::routes())
        .merge(identity::routes())
        .merge(oauth::routes())
        .merge(accounts::routes())
        .merge(subscriptions::routes())
        .merge(events::routes())
        .merge(bluesky_chat::routes())
        .merge(posts::routes())
        .merge(rss::routes())
        .merge(import::routes())
        .merge(import_facebook::routes())
        .merge(import_instagram::routes())
        .merge(import_mastodon::routes())
        .merge(google_import::routes())
        .merge(forums::routes())
        .merge(follow::routes())
        .merge(stories::routes())
        .merge(exchange::routes())
        .merge(export::routes())
        .merge(notifications::routes())
        .merge(mute::routes())
        .merge(settings::routes())
        .merge(groups::routes())
}

pub async fn health_check(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    match sqlx::query("SELECT 1").fetch_one(&state.db).await {
        Ok(_) => (StatusCode::OK, Json(json!({"status": "ok", "database": "connected"}))),
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"status": "error", "message": e.to_string()}))),
    }
}

#[derive(Deserialize)]
struct CaddyAskParams {
    domain: String,
}

/// Caddy on-demand TLS `ask` endpoint. Returns 200 only for names we control:
/// DOMAIN plus TLS_ASK_EXTRA_DOMAINS and their single-label subdomains — the
/// set wildcard DNS already points at this box. Caddy refuses to issue a
/// certificate for any name this rejects.
pub async fn caddy_ask(
    State(state): State<AppState>,
    Query(params): Query<CaddyAskParams>,
) -> StatusCode {
    let mut apexes: Vec<&str> = vec![state.config.domain.as_str()];
    apexes.extend(state.config.tls_ask_extra_domains.iter().map(String::as_str));
    for apex in apexes {
        if params.domain == apex {
            return StatusCode::OK;
        }
        if let Some(label) = params.domain.strip_suffix(&format!(".{}", apex)) {
            if !label.is_empty() && !label.contains('.') {
                return StatusCode::OK;
            }
        }
    }
    StatusCode::NOT_FOUND
}
