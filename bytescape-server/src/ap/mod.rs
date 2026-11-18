//! ActivityPub actor issuance & federation (Phase A–C).
//!
//! Every Bytescape account is a dual-protocol citizen: an AT Protocol identity
//! (did:web / did:plc, Ed25519) **and** an ActivityPub actor (RSA, for
//! HTTP-signatures), cross-linked via `alsoKnownAs`.
//!
//! Phase A scope (behind `AP_ACTORS_ENABLED`):
//! - [`keys`]: per-user RSA keypair generation + at-rest encryption
//! - [`webfinger`]: `/.well-known/webfinger` discovery
//! - [`actor`]: actor documents + canonical/alias id reconciliation (reclaim)
//!
//! Phase B (behind `AP_INGEST_ENABLED`): [`ingest`] maps remote Notes/Stories
//! into the appview indexes.
//!
//! Phase C (this surface):
//! - [`outbox`]: `GET /@{username}/outbox` — OrderedCollection of the actor's
//!   public outbound activities
//! - [`inbox`]: `POST /@{username}/inbox` (+ shared `/inbox`) — raw-body HTTP
//!   signature verification, activity dedupe, core Create/Like/Undo/Announce/
//!   Follow/Accept handling
//! - [`remote`]: SSRF-guarded actor resolution + `ap_remote_actors` cache
//! - [`store`]: shared activity/delivery persistence
//! - [`delivery`]: signed outbound delivery worker (bounded retries)
//! - [`bridge`]: native PDS writes → AP activities (called from `pds`)

pub mod actor;
pub mod backfill;
pub mod bridge;
pub mod collections;
pub mod delivery;
pub mod follow_feed;
pub mod follow_import;
pub mod http_sig;
pub mod inbox;
pub mod ingest;
pub mod issue;
pub mod keys;
pub mod notify;
pub mod outbox;
pub mod public_feed;
pub mod remote;
pub mod retire;
pub mod sources;
pub mod store;
pub mod webfinger;

#[cfg(test)]
pub(crate) mod testutil;

use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};

use crate::AppState;

/// Mount the AP routes. Registration is unconditional at the router level (so
/// paths are defined), but every handler checks `AP_ACTORS_ENABLED` and returns
/// 404 when disabled — the shipped app behaves exactly as before when the flag
/// is off. This keeps the bail seam simple: one flag, no route-patching.
pub fn routes() -> Router<AppState> {
    // Inbox endpoints get their own bounded-body sub-router: unauthenticated
    // remote input must never buffer unbounded bytes.
    let inbox_routes = Router::new()
        .route("/@{username}/inbox", post(inbox::inbox_handler))
        // Alias actor forms (webfinger `aliases` + actor `alsoKnownAs`):
        // remotes that cached a /users/ or /actors/ actor id deliver Follows
        // to the alias-form inbox URL. That POST must land exactly like the
        // canonical one — the dispatch resolves the target from the activity
        // object, which already reconciles alias forms. Owner QA 2026-11-17:
        // a Mastodon follow addressed to /users/{u}/inbox was silently 404'd
        // here and vanished without a row or a notification.
        .route("/users/{username}/inbox", post(inbox::inbox_handler))
        .route("/actors/{username}/inbox", post(inbox::inbox_handler))
        .route("/inbox", post(inbox::shared_inbox_handler))
        .layer(DefaultBodyLimit::max(inbox::MAX_BODY_BYTES));

    Router::new()
        // Canonical actor document + alias reconciliation (reclaim).
        .route("/@{username}", get(actor::actor_document))
        .route("/users/{username}", get(actor::alias_redirect))
        .route("/actors/{username}", get(actor::alias_redirect))
        // Interaction surface (Phase C).
        .route("/@{username}/outbox", get(outbox::outbox_handler))
        // Alias outbox forms: cached alias actor ids derive collection URLs
        // the same way they derive inbox URLs — serve them identically
        // (owner QA follow-up, same defect class as the alias inboxes).
        .route("/users/{username}/outbox", get(outbox::outbox_handler))
        .route("/actors/{username}/outbox", get(outbox::outbox_handler))
        // Follow-graph collections (identity-graph-unification Phase 1):
        // paginated OrderedCollections when the graph is public; the bare
        // hidden doc (no first/items/totalItems) when it is private.
        .route("/@{username}/followers", get(collections::followers_handler))
        .route("/@{username}/following", get(collections::following_handler))
        // Alias forms: cached alias actor ids derive collection URLs from
        // the id they cached — serve them identically (same defect class as
        // the alias outboxes/inboxes above).
        .route(
            "/users/{username}/followers",
            get(collections::followers_handler),
        )
        .route(
            "/users/{username}/following",
            get(collections::following_handler),
        )
        .route(
            "/actors/{username}/followers",
            get(collections::followers_handler),
        )
        .route(
            "/actors/{username}/following",
            get(collections::following_handler),
        )
        // Dereferenceable activity ids (Undo originals resolve through here).
        .route(
            "/@{username}/activities/{*rest}",
            get(actor::activity_document),
        )
        .merge(inbox_routes)
        // Discovery.
        .route("/.well-known/webfinger", get(webfinger::webfinger))
        .route("/.well-known/nodeinfo", get(webfinger::nodeinfo_pointer))
        .route("/nodeinfo/2.0", get(webfinger::nodeinfo))
        // Self-serve issuance (signed-in user mints/reclaims their own actor).
        .route("/api/ap/issue", get(issue::issue_self).post(issue::issue_self))
}

/// Is the AP actor surface enabled? Reads the feature flag from the
/// environment. `AP_ACTORS_ENABLED=true` turns issuance/serving on.
pub fn ap_actors_enabled() -> bool {
    std::env::var("AP_ACTORS_ENABLED").map(|v| v.eq_ignore_ascii_case("true")) == Ok(true)
}

/// Is AP feed ingestion enabled? Reads the feature flag from the
/// environment. `AP_INGEST_ENABLED=true` turns the Note/Story → `appview_*`
/// mappers on; with the flag off every ingest entry point is a no-op and the
/// shipped AT feed is unchanged.
pub fn ap_ingest_enabled() -> bool {
    std::env::var("AP_INGEST_ENABLED").map(|v| v.eq_ignore_ascii_case("true")) == Ok(true)
}