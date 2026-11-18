//! API — Identity routes.

use axum::{Router, routing::get};
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/identity/did", get(super::super::identity::get_did_handler))
        .route("/api/identity/also-known-as", get(get_aka).post(update_aka))
}

async fn get_aka() -> &'static str { "TODO" }
async fn update_aka() -> &'static str { "TODO" }
