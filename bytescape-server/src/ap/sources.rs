//! Fediverse content-source registry for the standing ActivityPub feed puller.
//!
//! The AP feed is populated by polling public timelines on a broad, managed set
//! of fediverse instances (Mastodon / Pixelfed / Lemmy / PieFed / Mobilizon /
//! generic). Each instance is a row in `ap_sources` carrying health/verification
//! state so the source set can evolve on its own:
//!
//! * **Seed** — a broad initial list of well-known healthy instances
//!   ([`seed_defaults`]).
//! * **Discovery** — every ingested `https://…` post reveals its host, which is
//!   upserted here ([`upsert_discovered`]) with an inferred `kind`. The registry
//!   therefore grows to cover more of the fediverse as content flows in.
//! * **Verification** — a host that keeps failing fetches (or yields nothing)
//!   is cooled down after [`COOLDOWN_AFTER`] consecutive failures and
//!   periodically re-probed for recovery ([`reprobe`]). This is the "criterion
//!   of verification": reachability + content yield, tunable here.
//!
//! The poller takes the least-recently-fetched enabled sources each pass
//! ([`next_due`]) so load is spread across the set. Two lanes share the pass
//! budget: hosts that have produced content at least once (the main,
//! least-recently-fetched lane) and hosts that have **never** succeeded (the
//! probe lane). Discovery inflates the registry far faster than a single lane
//! can sweep it — on the 2026-11 audit the main lane held 1,662 healthy hosts
//! and 1,504 never-successful ones, which is why untried hosts no longer
//! consume main-lane slots (`next_due`).

use sqlx::Row;

/// Consecutive failures before a host is cooled down (`enabled = false`).
pub const COOLDOWN_AFTER: i32 = 5;

/// How many cooled-down hosts to re-probe per [`reprobe`] pass.
const REPROBE_BATCH: i64 = 8;

#[derive(Debug, Clone)]
pub struct ApSource {
    pub host: String,
    pub kind: String,
}

/// Insert a broad initial source set if absent. Idempotent (`ON CONFLICT DO
/// NOTHING`), so it is safe to run at startup. Well-known, high-uptime,
/// public-timeline-enabled instances across the three named networks plus
/// broader Mastodon-compatible ones.
pub async fn seed_defaults(pool: &sqlx::PgPool) -> Result<usize, sqlx::Error> {
    let defaults: &[(&str, &str)] = &[
        // Mastodon (and Mastodon-compatible)
        ("mastodon.social", "mastodon"),
        ("mstdn.social", "mastodon"),
        ("fosstodon.org", "mastodon"),
        ("hachyderm.io", "mastodon"),
        ("mastodon.online", "mastodon"),
        ("mas.to", "mastodon"),
        ("techhub.social", "mastodon"),
        ("geneseo.social", "mastodon"),
        // Pixelfed
        ("pixelfed.social", "pixelfed"),
        ("pixelfed.net", "pixelfed"),
        ("pixey.org", "pixelfed"),
        // Lemmy
        ("lemmy.world", "lemmy"),
        ("lemmy.ml", "lemmy"),
        ("sh.itjust.works", "lemmy"),
        ("beehaw.org", "lemmy"),
        ("sopuli.xyz", "lemmy"),
        ("programming.dev", "lemmy"),
        // PieFed (Lemmy-compatible forum software, different REST path)
        ("piefed.social", "piefed"),
        ("piefed.world", "piefed"),
        ("piefed.ca", "piefed"),
        // Mobilizon (events; GraphQL API + AP `Event` objects, no public
        // timeline — see `public_feed::fetch_mobilizon`)
        ("mobilizon.fr", "mobilizon"),
        ("mobilizon.it", "mobilizon"),
    ];
    let mut n = 0usize;
    for (host, kind) in defaults {
        // `priority = 1` pins the curated set: the probe lane verifies deliberate
        // sources before the discovery backlog, which is what makes a freshly
        // added kind (PieFed, Mobilizon) observable instead of buried behind
        // 1,500 never-probed hosts. The WHERE clause keeps the count honest —
        // only rows that actually gained priority are reported.
        let r = sqlx::query(
            "INSERT INTO ap_sources (host, kind, priority) VALUES ($1, $2, 1) \
             ON CONFLICT (host) DO UPDATE SET priority = 1 WHERE ap_sources.priority < 1",
        )
        .bind(host)
        .bind(kind)
        .execute(pool)
        .await?;
        n += r.rows_affected() as usize;
    }
    Ok(n)
}

/// Infer `kind` from a URL or bare host. Path-shape detection is more reliable
/// than host names (e.g. `sh.itjust.works` is Lemmy without "lemmy" in the name).
///
/// Order matters: PieFed feeds are Lemmy-shaped (`/u/`, `/c/`, `/p/`) and
/// Mobilizon events live under `/events/`, so both are checked before the
/// generic Lemmy/Pixelfed path shapes. Every kind here has a fetcher in
/// [`crate::ap::public_feed`]; a kind without one pollutes the registry with
/// permanently-failing rows.
pub fn infer_kind(url_or_host: &str) -> String {
    let lower = url_or_host.to_ascii_lowercase();
    // Mobilizon: event objects under /events/<uuid> and "mobilizon" hosts.
    if lower.contains("mobilizon") || lower.contains("/events/") {
        return "mobilizon".to_string();
    }
    // PieFed: its own hostnames; object paths mirror Lemmy's.
    if lower.contains("piefed") {
        return "piefed".to_string();
    }
    // Lemmy actor/object paths: /post/, /comment/, /c/, /u/, /m/.
    if lower.contains("/post/")
        || lower.contains("/comment/")
        || lower.contains("/c/")
        || lower.contains("/u/")
        || lower.contains("/m/")
        || lower.contains("lemmy")
    {
        return "lemmy".to_string();
    }
    if lower.contains("pixelfed") || lower.contains("/p/") || lower.contains("/stories/") {
        return "pixelfed".to_string();
    }
    if lower.contains("mastodon") || lower.contains("/users/") || lower.contains("/statuses/") {
        return "mastodon".to_string();
    }
    "generic".to_string()
}

/// Record a host revealed by ingested content. Idempotent upsert; keeps any
/// existing health state but refreshes `kind` when it was a generic guess.
pub async fn upsert_discovered(pool: &sqlx::PgPool, url_or_host: &str) -> Result<(), sqlx::Error> {
    let Some(host) = host_of(url_or_host) else {
        return Ok(());
    };
    let kind = infer_kind(url_or_host);
    sqlx::query(
        "INSERT INTO ap_sources (host, kind) VALUES ($1, $2) \
         ON CONFLICT (host) DO UPDATE SET \
           kind = CASE WHEN ap_sources.kind = 'generic' THEN EXCLUDED.kind ELSE ap_sources.kind END, \
           updated_at = now()",
    )
    .bind(host)
    .bind(kind)
    .execute(pool)
    .await?;
    Ok(())
}

/// The sources to poll this pass, in two lanes.
///
/// * **Main lane** (`limit`): hosts that have yielded content at least once
///   (`last_ok_at IS NOT NULL`), least-recently-fetched first. This is the set
///   whose cycle time matters for feed freshness.
/// * **Probe lane** (`probe_limit`): hosts that have **never** succeeded
///   (`last_ok_at IS NULL`) — freshly seeded kinds and discovered hosts. They
///   are tried on a small per-pass budget instead of competing with healthy
///   hosts for main-lane slots; a probe that succeeds promotes the host to the
///   main lane, and one that keeps failing cools down through [`record_fail`]
///   into the [`reprobe`] cycle.
///
/// Both lanes order deterministically (`host` breaks ties) so a queue that a
/// migration or a seed just refreshed cannot be reordered arbitrarily. The
/// probe lane additionally prefers `priority = 1` — the curated seed set — so
/// a deliberately added source is verified immediately instead of queueing
/// behind the discovery backlog (`upsert_discovered` leaves priority 0).
///
/// Discovery grows the registry orders of magnitude faster than a single lane
/// can sweep it (audit 2026-11: 1,662 healthy vs 1,504 never-successful hosts),
/// so an untried host in the main lane would starve real content.
pub async fn next_due(
    pool: &sqlx::PgPool,
    limit: i64,
    probe_limit: i64,
) -> Result<Vec<ApSource>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT host, kind FROM ( \
           (SELECT host, kind, 0 AS lane, fetched_at FROM ap_sources \
             WHERE enabled = TRUE AND last_ok_at IS NOT NULL \
             ORDER BY fetched_at NULLS FIRST, last_items ASC, host ASC \
             LIMIT $1) \
           UNION ALL \
           (SELECT host, kind, 1 AS lane, fetched_at FROM ap_sources \
             WHERE enabled = TRUE AND last_ok_at IS NULL \
             ORDER BY fetched_at NULLS FIRST, priority DESC, host ASC \
             LIMIT $2) \
         ) lanes \
         ORDER BY lane, fetched_at NULLS FIRST",
    )
    .bind(limit)
    .bind(probe_limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ApSource {
            host: r.get("host"),
            kind: r.get("kind"),
        })
        .collect())
}

/// Mark a successful fetch: reset the failure streak and record yield + time.
pub async fn record_ok(pool: &sqlx::PgPool, host: &str, items: i32) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE ap_sources SET failures = 0, last_ok_at = now(), last_items = $2, \
           fetched_at = now(), enabled = TRUE, updated_at = now() WHERE host = $1",
    )
    .bind(host)
    .bind(items)
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark a failed fetch: bump the failure streak and cool the host down once it
/// crosses [`COOLDOWN_AFTER`].
pub async fn record_fail(pool: &sqlx::PgPool, host: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE ap_sources SET failures = failures + 1, fetched_at = now(), \
           enabled = (failures + 1 < $2), updated_at = now() WHERE host = $1",
    )
    .bind(host)
    .bind(COOLDOWN_AFTER)
    .execute(pool)
    .await?;
    Ok(())
}

/// Re-probe a sample of cooled-down hosts by clearing their failure streak and
/// re-enabling them, so a transiently-down instance recovers instead of being
/// retired forever. Run this on a slower cadence than the poller.
pub async fn reprobe(pool: &sqlx::PgPool) -> Result<usize, sqlx::Error> {
    let r = sqlx::query(
        "WITH cooled AS (SELECT host FROM ap_sources WHERE enabled = FALSE LIMIT $1) \
         UPDATE ap_sources SET enabled = TRUE, failures = 0, updated_at = now() \
         WHERE host IN (SELECT host FROM cooled)",
    )
    .bind(REPROBE_BATCH)
    .execute(pool)
    .await?;
    Ok(r.rows_affected() as usize)
}

/// Host of an `http(s)://host/...` URL or a bare host, lowercased.
fn host_of(url_or_host: &str) -> Option<String> {
    let s = url_or_host.trim();
    if let Some(rest) = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")) {
        let host = rest.split(['/', '?', '#']).next()?;
        return Some(host.to_ascii_lowercase());
    }
    // Bare host (no scheme, no path).
    if !s.is_empty() && !s.contains('/') {
        return Some(s.to_ascii_lowercase());
    }
    None
}
