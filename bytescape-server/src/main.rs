//! bytescape.social — federated social server
#![allow(clippy::too_many_arguments)]

mod api;
mod ap;
mod appview;
pub mod auth;
mod calendar;
mod config;
pub mod constants;
mod db;
mod error;
pub mod identity;
mod oauth;
mod oauth_server;
mod pds;
mod secret;
mod tags;

use axum::Router;
use config::AppConfig;
use db::DbPool;
use std::sync::Arc;
use std::collections::HashMap;
use std::sync::Mutex;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db: DbPool,
    pub og_cache: Arc<Mutex<HashMap<String, serde_json::Value>>>,
}

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = AppConfig::from_env().expect("Failed to load config");
    let db = db::connect(&config.database_url).await;

    // SEC-1: seal any pre-rollout plaintext linked-account tokens at rest.
    // This Rust backfill IS the migration for existing rows (a .sql migration
    // cannot run chacha20poly1305). Idempotent; non-fatal (legacy plaintext
    // rows stay readable via the tolerant open in db::accounts).
    match db::accounts::encrypt_plaintext_tokens(&db, &config.jwt_secret).await {
        Ok(n) if n > 0 => {
            tracing::info!("linked_accounts: sealed {n} legacy plaintext token column(s) at rest")
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("linked_accounts token encryption backfill failed: {e}"),
    }

    let use_local = !config.s3_enabled;
    if use_local {
        std::fs::create_dir_all("./uploads/avatars").ok();
        std::fs::create_dir_all("./uploads/headers").ok();
        tracing::info!("Local file upload mode: ./uploads/");
    }

    let state = AppState {
        config: Arc::new(config.clone()),
        db: db.clone(),
        og_cache: Arc::new(Mutex::new(HashMap::new())),
    };

    {
        let db3 = db.clone();
        tokio::spawn(async move { clear_seed_events(&db3).await; });
        let db_topics = db.clone();
        tokio::spawn(async move {
            match crate::calendar::backfill_topics(&db_topics).await {
                Ok(n) if n > 0 => tracing::info!("calendar topics: repaired {n} events"),
                Ok(_) => {}
                Err(e) => tracing::warn!("calendar topics backfill failed: {e}"),
            }
        });
        crate::api::feed::sources::spawn_cache_warmer(db.clone());
        crate::api::feed::sources::spawn_aggregated_refresher(state.clone());
        if crate::ap::ap_ingest_enabled() {
            crate::ap::public_feed::spawn_ap_public_feed_worker(
                db.clone(),
                state.config.jwt_secret.clone(),
            );
        }
        crate::api::feedgen::sync_topic_terms(&db).await;
        crate::api::stories::spawn_story_transitions(db.clone());
    }

    api::bluesky_chat::start_background_sync(state.clone());

    // Start firehose consumer (writes derived index from firehose events)
    pds::firehose_consumer::spawn(db.clone(), std::sync::Arc::new(tokio::sync::Notify::new()));

    // Start AppView relay firehose consumer (indexes Bluesky network posts)
    // Disabled until we have users — set APPVIEW_CONSUMER=true to enable
    if std::env::var("APPVIEW_CONSUMER").as_deref() == Ok("true") {
        eprintln!("[AppView] Starting relay firehose consumer...");
        appview::consumer::start_consumer(db.clone());
        appview::handle_resolver::start_handle_resolver(db.clone());
    } else {
        eprintln!("[AppView] Relay firehose consumer disabled (set APPVIEW_CONSUMER=true to enable)");
    }

    // AP actor issuance for reclaimed ghost handles (Phase A). Behind
    // AP_ACTORS_ENABLED. `AP_RECLAIM_HANDLES` = comma-separated legacy handles
    // whose fediverse ghost actors (e.g. @lovingisliving@bytescape.social) must
    // be reconciled to a live, controlled actor at their canonical id.
    if crate::ap::ap_actors_enabled() {
        let handles: Vec<String> = std::env::var("AP_RECLAIM_HANDLES")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !handles.is_empty() {
            let state = state.clone();
            tokio::spawn(async move {
                match crate::ap::issue::reclaim_ghost_handles(&state, &handles).await {
                    Ok(issued) if !issued.is_empty() => {
                        eprintln!("[AP] Reclaimed actor(s) for: {}", issued.join(", "));
                    }
                    Ok(_) => {}
                    Err(e) => eprintln!("[AP] Reclaim failed: {e}"),
                }
            });
        }
    }

    // AP outbound delivery worker (Phase C): signed POSTs of queued
    // activities to remote inboxes. Queue state lives in Postgres, so pending
    // deliveries survive restarts.
    ap::delivery::spawn_worker(db.clone(), config.jwt_secret.clone());

    // Retirement drain worker (add-identity-realignment): delivers staged
    // retractions + Delete fan-outs, then finalizes tombstones.
    ap::retire::spawn_drain_worker(db.clone());

    // Re-register with the relay at startup and every 20 minutes (the
    // reference PDS cadence — `packages/pds/src/crawlers.ts` throttles
    // requestCrawl to `NOTIFY_THRESHOLD = 20 * 60e3`; host statuses drift, so
    // periodic re-requests keep us crawled). Startup call covers every
    // deploy/restart. `request_crawl` logs the relay's answer in detail and
    // never fails this loop.
    {
        let domain = config.domain.clone();
        tokio::spawn(async move {
            loop {
                crate::pds::account::request_crawl(&domain).await;
                tokio::time::sleep(std::time::Duration::from_secs(20 * 60)).await;
            }
        });
    }

    // One-shot did:web -> did:plc re-identity (group-5 migration): pre-realignment
    // minted `did:web` AT identities are invisible to the real Atmosphere
    // (the AppView drops their events — bluesky-social/atproto#4379), so
    // m→bsky likes never federate. Gated like the actor supersession below:
    // REIDENTIFY_DID_WEB=apply executes the run, dry-run values audit-log only,
    // absent = never runs. A typo'd value (e.g. `=true`) dry-runs fail-closed
    // BUT is warned about — it must not look configured while doing nothing
    // forever (R2-16).
    if let Some(raw_mode) = std::env::var("REIDENTIFY_DID_WEB").ok() {
        let mode = parse_reidentify_mode(Some(raw_mode));
        if let ReidentifyMode::Unknown(value) = &mode {
            tracing::warn!(
                "[reidentify] REIDENTIFY_DID_WEB={value:?} is not a recognized mode — dry-running; \
                 set it to 'apply' to execute the migration"
            );
        }
        let apply = mode == ReidentifyMode::Apply;
        let db = db.clone();
        let plc = config.plc_directory.clone();
        let domain = config.domain.clone();
        tokio::spawn(async move {
            match pds::reidentify::supersede_did_web(&db, &plc, &domain, apply, None).await {
                Ok(n) => {
                    tracing::info!(
                        "[reidentify] run ({}) migrated {n} account(s)",
                        if apply { "apply" } else { "dry-run" }
                    );
                    // One host-level crawl covers every re-pointed repo (same
                    // ping account mint fires). Fire-and-forget.
                    if apply && n > 0 {
                        pds::account::request_crawl(&domain).await;
                    }
                }
                Err(e) => tracing::warn!("[reidentify] run failed: {e}"),
            }
        });
    }

    // One-shot appview embed repair: posts ingested before the
    // recordWithMedia fix carry embeds = NULL (text-only, no images, no
    // quote). EMBED_BACKFILL=apply runs it, dry-run values fetch and count
    // without writing, absent = never runs. Same parser as the other
    // one-shots, so a typo'd value fail-closes to dry-run and is warned
    // about instead of silently doing nothing.
    if let Some(raw_mode) = std::env::var("EMBED_BACKFILL").ok() {
        let mode = parse_reidentify_mode(Some(raw_mode));
        if let ReidentifyMode::Unknown(value) = &mode {
            tracing::warn!(
                "[embed-backfill] EMBED_BACKFILL={value:?} is not a recognized mode — dry-running; \
                 set it to 'apply' to write the repaired embeds"
            );
        }
        let apply = mode == ReidentifyMode::Apply;
        let db = db.clone();
        tokio::spawn(async move {
            match appview::embed_backfill::run(&db, apply).await {
                Ok(stats) => tracing::info!(
                    "[embed-backfill] run ({}) done: {stats:?}",
                    if apply { "apply" } else { "dry-run" }
                ),
                Err(e) => tracing::warn!("[embed-backfill] run failed: {e}"),
            }
        });
    }

    // One-shot minted-actor supersession (tasks.md 3.3), opt-in and
    // audit-logged: RETIRE_MASTODON_MINTED_ACTORS=apply executes the run,
    // dry-run values audit-log only, absent = never runs. Same parser and
    // typo warning as REIDENTIFY_DID_WEB (R2-16): the shapes are trivially
    // identical ("apply" / anything else / absent), so one parse covers both.
    if let Some(raw_mode) = std::env::var("RETIRE_MASTODON_MINTED_ACTORS").ok() {
        let mode = parse_reidentify_mode(Some(raw_mode));
        if let ReidentifyMode::Unknown(value) = &mode {
            tracing::warn!(
                "[retire] RETIRE_MASTODON_MINTED_ACTORS={value:?} is not a recognized mode — dry-running; \
                 set it to 'apply' to execute the supersession"
            );
        }
        let apply = mode == ReidentifyMode::Apply;
        let state = state.clone();
        tokio::spawn(async move {
            match crate::ap::retire::supersede_minted_actors(&state, apply, None).await {
                Ok(n) => tracing::info!(
                    "[retire] supersession run ({}) staged {n} actor(s)",
                    if apply { "apply" } else { "dry-run" }
                ),
                Err(e) => tracing::warn!("[retire] supersession run failed: {e}"),
            }
        });
    }

    // Background task: process expired soft-deletions every hour
    {
        let db = db.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                // Federated retirement runs before the row delete (staged
                // rows outlive the delete and drain in the background).
                match crate::ap::retire::sweep_expired(&db).await {
                    Ok(count) => {
                        if count > 0 {
                            tracing::info!("Purged {} expired soft-deleted accounts", count);
                        }
                    }
                    Err(e) => tracing::error!("Failed to purge expired deletions: {}", e),
                }
            }
        });
    }

    let session_state = state.clone();
    let mut app = Router::new()
        .nest("/_matrix", api::matrix::routes())
        .merge(api::routes())
        .merge(pds::routes())
        .merge(appview::endpoints::routes())
        .merge(oauth_server::routes())
        .merge(ap::routes())
        .merge(api::og::routes())
        .with_state(state);

    if use_local {
        app = app.nest_service("/uploads", ServeDir::new("uploads"));
    }

    // Innermost layer: cookie → Bearer injection + Origin check for
    // cookie-authenticated writes (spec: events/session).
    let app = app
        .layer(axum::middleware::from_fn_with_state(
            session_state,
            api::auth::cookie_session_middleware,
        ))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let socket = tokio::net::TcpSocket::new_v4()
        .expect("Failed to create socket");
    socket.set_reuseaddr(true)
        .expect("Failed to set SO_REUSEADDR");
    #[cfg(target_os = "linux")]
    let _ = socket.set_reuseport(true);
    let bind_port = config.port;
    let addr: std::net::SocketAddr = format!("0.0.0.0:{}", bind_port).parse()
        .expect("invalid bind address");
    socket.bind(addr)
        .expect("Failed to bind port");

    let listener = socket.listen(1024)
        .expect("Failed to listen on configured port");

    tracing::info!("bytescape.social server starting on 0.0.0.0:{}", bind_port);
    axum::serve(listener, app)
        .await
        .expect("Server failed");
}

async fn clear_seed_events(pool: &DbPool) {
    let _ = sqlx::query(
        "DELETE FROM events WHERE ap_object_id LIKE '%/seed-%'"
    )
    .execute(pool)
    .await;
}

/// Outcome of a one-shot migration flag (`REIDENTIFY_DID_WEB`,
/// `RETIRE_MASTODON_MINTED_ACTORS`). Gating semantics (unchanged):
/// `Absent` = the run never starts; `DryRun` = audit log only; `Apply` =
/// executes. `Unknown` dry-runs like `DryRun`, but keeps the typo visible to
/// the caller so it can be warned about — a typo'd `=true` "looks configured
/// and does nothing, forever" if it is silently folded into `DryRun` (R2-16).
///
/// Note: second-boot idempotency is NOT this parser's concern — it is
/// `supersede_did_web`'s `state='done'` filter (`pds/reidentify.rs`), which is
/// outside this file's scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReidentifyMode {
    /// Flag not set — the run never starts.
    Absent,
    /// Flag set to an explicit dry-run value — audit log only.
    DryRun,
    /// Flag set to `apply` (case-insensitive) — the irreversible run executes.
    Apply,
    /// Flag set to an unrecognized value — dry-runs (fail-closed) but the
    /// caller SHOULD warn naming the value; the variant carries it.
    Unknown(String),
}

/// Parse a raw migration-flag value into its [`ReidentifyMode`]. Pure: the
/// env read stays at the call site so gating behavior (`var().ok()`: unset and
/// non-UTF8 both mean "never runs") is preserved exactly.
pub(crate) fn parse_reidentify_mode(value: Option<String>) -> ReidentifyMode {
    match value {
        None => ReidentifyMode::Absent,
        Some(v) if v.eq_ignore_ascii_case("apply") => ReidentifyMode::Apply,
        Some(v) if v.is_empty() || v.eq_ignore_ascii_case("dry-run") => ReidentifyMode::DryRun,
        Some(v) => ReidentifyMode::Unknown(v),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_reidentify_mode, ReidentifyMode};

    #[test]
    fn parse_reidentify_mode_absent_never_runs() {
        assert_eq!(parse_reidentify_mode(None), ReidentifyMode::Absent);
    }

    #[test]
    fn parse_reidentify_mode_apply_is_case_insensitive() {
        assert_eq!(parse_reidentify_mode(Some("apply".into())), ReidentifyMode::Apply);
        assert_eq!(parse_reidentify_mode(Some("APPLY".into())), ReidentifyMode::Apply);
        assert_eq!(parse_reidentify_mode(Some("Apply".into())), ReidentifyMode::Apply);
    }

    #[test]
    fn parse_reidentify_mode_explicit_dry_run_values() {
        assert_eq!(parse_reidentify_mode(Some(String::new())), ReidentifyMode::DryRun);
        assert_eq!(parse_reidentify_mode(Some("dry-run".into())), ReidentifyMode::DryRun);
        assert_eq!(parse_reidentify_mode(Some("DRY-RUN".into())), ReidentifyMode::DryRun);
    }

    /// The typo class must be VISIBLE in the return value — `=true`/`=yes`/`=1`
    /// "look configured" but must never be mistaken for a real mode (the
    /// `tracing::warn!` that names the value is code-reviewed at the call
    /// site; this test pins the data it needs reaching the caller).
    #[test]
    fn parse_reidentify_mode_typo_values_are_visible_as_unknown() {
        for typo in ["true", "yes", "1"] {
            assert_eq!(
                parse_reidentify_mode(Some(typo.into())),
                ReidentifyMode::Unknown(typo.to_string()),
                "typo {typo:?} must surface as Unknown(value), not fold into DryRun"
            );
        }
        assert_ne!(
            parse_reidentify_mode(Some("true".into())),
            ReidentifyMode::DryRun,
            "a typo'd value must be distinguishable from an intentional dry-run"
        );
    }
}
