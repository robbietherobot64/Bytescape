#![allow(dead_code)]
//! Database connection, migration runner, and entity modules.

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub mod users;
pub mod posts;
pub mod media;
pub mod follows;
pub mod follow_requests;
pub mod imports;
pub mod accounts;
pub mod subscriptions;
pub mod wall;
pub mod reactions;
pub mod event_comments;
pub mod matrix;
pub mod chat;
pub mod chat_profiles;
pub mod rss;
pub mod forums;
pub mod exchange;
pub mod cached_posts;
pub mod imported_contacts;
pub mod social_graph_matches;
pub mod archive_identities;
pub mod notifications;
pub mod cross_posts;
pub mod invite_codes;
pub mod posting_identities;

/// A connection pool for Postgres.
pub type DbPool = PgPool;

const MIGRATIONS_001: &str = include_str!("../../migrations/001_initial.sql");
const MIGRATIONS_002: &str = include_str!("../../migrations/002_content_integration.sql");
const MIGRATIONS_003: &str = include_str!("../../migrations/003_subscriptions_updated_at.sql");
const MIGRATIONS_004: &str = include_str!("../../migrations/004_events.sql");
const MIGRATIONS_005: &str = include_str!("../../migrations/005_profile_fields.sql");
const MIGRATIONS_006: &str = include_str!("../../migrations/006_wall.sql");
const MIGRATIONS_007: &str = include_str!("../../migrations/007_reactions.sql");
const MIGRATIONS_008: &str = include_str!("../../migrations/008_event_rsvps.sql");
const MIGRATIONS_009: &str = include_str!("../../migrations/009_events_cascade.sql");
const MIGRATIONS_010: &str = include_str!("../../migrations/010_event_comments.sql");
const MIGRATIONS_011: &str = include_str!("../../migrations/011_matrix.sql");
const MIGRATIONS_012: &str = include_str!("../../migrations/012_reactions_metadata.sql");
const MIGRATIONS_013: &str = include_str!("../../migrations/013_chat_cache.sql");
const MIGRATIONS_014: &str = include_str!("../../migrations/014_widget_layout.sql");
const MIGRATIONS_015: &str = include_str!("../../migrations/015_media_user_id.sql");
const MIGRATIONS_016: &str = include_str!("../../migrations/016_chat_profiles.sql");
const MIGRATIONS_017: &str = include_str!("../../migrations/017_refresh_tokens.sql");
const MIGRATIONS_018: &str = include_str!("../../migrations/018_email_verifications.sql");
const MIGRATIONS_019: &str = include_str!("../../migrations/019_soft_delete.sql");
const MIGRATIONS_020: &str = include_str!("../../migrations/020_rss_subscriptions.sql");
const MIGRATIONS_021: &str = include_str!("../../migrations/021_post_archive_fields.sql");
const MIGRATIONS_022: &str = include_str!("../../migrations/022_forums.sql");
const MIGRATIONS_023: &str = include_str!("../../migrations/023_exchange.sql");
const MIGRATIONS_024: &str = include_str!("../../migrations/024_cached_posts.sql");
const MIGRATIONS_025: &str = include_str!("../../migrations/025_custom_profile_fields.sql");
const MIGRATIONS_026: &str = include_str!("../../migrations/026_fix_fk_cascade.sql");
const MIGRATIONS_027: &str = include_str!("../../migrations/027_fix_delete_constraints.sql");
const MIGRATIONS_028: &str = include_str!("../../migrations/028_imported_contacts.sql");
const MIGRATIONS_029: &str = include_str!("../../migrations/029_social_graph_matches.sql");
const MIGRATIONS_030: &str = include_str!("../../migrations/030_archive_identities.sql");
const MIGRATIONS_031: &str = include_str!("../../migrations/031_notifications.sql");
const MIGRATIONS_032: &str = include_str!("../../migrations/032_add_reshared_post_id.sql");
const MIGRATIONS_033: &str = include_str!("../../migrations/033_add_follow_source.sql");
const MIGRATIONS_035: &str = include_str!("../../migrations/035_contact_category.sql");
const MIGRATIONS_036: &str = include_str!("../../migrations/036_cross_posts.sql");
const MIGRATIONS_037: &str = include_str!("../../migrations/037_cross_posts_external_id.sql");
const MIGRATIONS_038: &str = include_str!("../../migrations/038_rsa_keys.sql");
const MIGRATIONS_039: &str = include_str!("../../migrations/039_pds_tables.sql");
const MIGRATIONS_040: &str = include_str!("../../migrations/040_firehose_tables.sql");
const MIGRATIONS_041: &str = include_str!("../../migrations/041_oauth_tables.sql");
const MIGRATIONS_042: &str = include_str!("../../migrations/042_invite_codes.sql");
const MIGRATIONS_043: &str = include_str!("../../migrations/043_fix_original_platform_enum.sql");
const MIGRATIONS_044: &str = include_str!("../../migrations/044_appview_posts.sql");
const MIGRATIONS_045: &str = include_str!("../../migrations/045_appview_did_handle.sql");
const MIGRATIONS_046: &str = include_str!("../../migrations/046_idx_appview_posts_created_uri.sql");
const MIGRATIONS_047: &str = include_str!("../../migrations/047_idx_appview_posts_author_did_created.sql");
const MIGRATIONS_048: &str = include_str!("../../migrations/048_appview_engagement.sql");
const MIGRATIONS_049: &str = include_str!("../../migrations/049_muted_accounts.sql");
const MIGRATIONS_050: &str = include_str!("../../migrations/050_posting_identities.sql");
const MIGRATIONS_051: &str = include_str!("../../migrations/051_backfill_posting_identities.sql");
const MIGRATIONS_052: &str = include_str!("../../migrations/052_pds_accounts_user_id.sql");
const MIGRATIONS_053: &str = include_str!("../../migrations/053_drop_ap_tables.sql");
const MIGRATIONS_054: &str = include_str!("../../migrations/054_identity_fk_constraints.sql");
const MIGRATIONS_055: &str = include_str!("../../migrations/055_posts_v1_columns.sql");
const MIGRATIONS_056: &str = include_str!("../../migrations/056_users_show_nsfw.sql");
const MIGRATIONS_057: &str = include_str!("../../migrations/057_appview_posts_labels_has_media.sql");
const MIGRATIONS_058: &str = include_str!("../../migrations/058_feed_combos.sql");
const MIGRATIONS_059: &str = include_str!("../../migrations/059_published_feeds.sql");
const MIGRATIONS_060: &str = include_str!("../../migrations/060_subscriptions_pinned.sql");
const MIGRATIONS_061: &str = include_str!("../../migrations/061_appview_posts_created_at.sql");
const MIGRATIONS_062: &str = include_str!("../../migrations/062_appview_posts_generic_idx.sql");
const MIGRATIONS_063: &str = include_str!("../../migrations/063_appview_posts_author_profile.sql");
const MIGRATIONS_064: &str = include_str!("../../migrations/064_post_tags.sql");
const MIGRATIONS_065: &str = include_str!("../../migrations/065_index_nulls_order.sql");
const MIGRATIONS_066: &str = include_str!("../../migrations/066_feed_snapshots.sql");
const MIGRATIONS_068: &str = include_str!("../../migrations/068_topic_terms.sql");
const MIGRATIONS_069: &str = include_str!("../../migrations/069_stories.sql");
const MIGRATIONS_070: &str = include_str!("../../migrations/070_pds_blobs.sql");
const MIGRATIONS_071: &str = include_str!("../../migrations/071_status_live.sql");
const MIGRATIONS_072: &str = include_str!("../../migrations/072_status_created_at.sql");
const MIGRATIONS_073: &str = include_str!("../../migrations/073_did_handle_profile.sql");
const MIGRATIONS_074: &str = include_str!("../../migrations/074_schema_drift_catchup.sql");
const MIGRATIONS_075: &str = include_str!("../../migrations/075_did_handle_handle_idx.sql");
const MIGRATIONS_076: &str = include_str!("../../migrations/076_feed_sort_indexes.sql");
const MIGRATIONS_077: &str = include_str!("../../migrations/077_mastodon_import.sql");
const MIGRATIONS_078: &str = include_str!("../../migrations/078_users_onboarded.sql");
const MIGRATIONS_079: &str = include_str!("../../migrations/079_calendar_events.sql");
const MIGRATIONS_080: &str = include_str!("../../migrations/080_ap_actor_identity.sql");
const MIGRATIONS_081: &str = include_str!("../../migrations/081_calendar_topics.sql");
const MIGRATIONS_082: &str = include_str!("../../migrations/082_ap_interactions.sql");
const MIGRATIONS_083: &str = include_str!("../../migrations/083_ap_sources.sql");
const MIGRATIONS_084: &str = include_str!("../../migrations/084_appview_posts_ap_index.sql");
const MIGRATIONS_085: &str = include_str!("../../migrations/085_calendar_rsvps_author_index.sql");
const MIGRATIONS_086: &str = include_str!("../../migrations/086_ap_delivery_fifo.sql");
const MIGRATIONS_087: &str = include_str!("../../migrations/087_federated_account_lifecycle.sql");
const MIGRATIONS_088: &str = include_str!("../../migrations/088_orphan_cleanup.sql");
const MIGRATIONS_089: &str = include_str!("../../migrations/089_reaction_home_rest_metadata.sql");
const MIGRATIONS_090: &str = include_str!("../../migrations/090_reaction_home_status_id_fixup.sql");
const MIGRATIONS_091: &str = include_str!("../../migrations/091_pds_did_reidentify.sql");
const MIGRATIONS_092: &str = include_str!("../../migrations/092_reaction_retraction_window.sql");
const MIGRATIONS_093: &str = include_str!("../../migrations/093_firehose_commit_chain.sql");
const MIGRATIONS_094: &str = include_str!("../../migrations/094_settings_account_ia.sql");
const MIGRATIONS_095: &str = include_str!("../../migrations/095_ap_source_kinds.sql");
const MIGRATIONS_096: &str = include_str!("../../migrations/096_ap_source_priority.sql");
const MIGRATIONS_097: &str = include_str!("../../migrations/097_ap_groups.sql");
const MIGRATIONS_098: &str = include_str!("../../migrations/098_appview_reply_parent_idx.sql");
const MIGRATIONS_100: &str = include_str!("../../migrations/100_follow_requests.sql");
const MIGRATIONS_101: &str = include_str!("../../migrations/101_notifications_provenance.sql");
const MIGRATIONS_102: &str = include_str!("../../migrations/102_approve_followers_default.sql");
const MIGRATIONS_103: &str = include_str!("../../migrations/103_feed_cold_path_idx.sql");
const MIGRATIONS_105: &str = include_str!("../../migrations/105_ap_inbox_rejections.sql");
const MIGRATIONS_106: &str = include_str!("../../migrations/106_graph_visibility.sql");

// 067_text_trgm.sql (text trigram indexes) is registered as separate
// statements below: CREATE INDEX CONCURRENTLY cannot run inside a transaction
// block, and a multi-statement raw_sql is one implicit block.

/// Connect to Postgres and run migrations.
pub async fn connect(database_url: &str) -> DbPool {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(database_url)
        .await
        .expect("Failed to connect to Postgres");

    tracing::info!("Connected to Postgres");

    run_migrations(&pool).await;
    pool
}

/// Run the SQL migration file idempotently.
///
/// Serialized process-wide: the statements are individually idempotent but
/// the DDL set (CREATE TYPE …) is not safe to run concurrently — parallel
/// test setups that sync the schema would race the catalog.
pub async fn run_migrations(pool: &DbPool) {
    static MIGRATIONS_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = MIGRATIONS_LOCK.lock().await;
    sqlx::raw_sql(MIGRATIONS_001)
        .execute(pool)
        .await
        .expect("Migration 001 failed");

    sqlx::raw_sql(MIGRATIONS_002)
        .execute(pool)
        .await
        .expect("Migration 002 failed");

    sqlx::raw_sql(MIGRATIONS_003)
        .execute(pool)
        .await
        .expect("Migration 003 failed");

    sqlx::raw_sql(MIGRATIONS_004)
        .execute(pool)
        .await
        .expect("Migration 004 failed");

    sqlx::raw_sql(MIGRATIONS_005)
        .execute(pool)
        .await
        .expect("Migration 005 failed");

    sqlx::raw_sql(MIGRATIONS_006)
        .execute(pool)
        .await
        .expect("Migration 006 failed");

    sqlx::raw_sql(MIGRATIONS_007)
        .execute(pool)
        .await
        .expect("Migration 007 failed");

    sqlx::raw_sql(MIGRATIONS_008)
        .execute(pool)
        .await
        .expect("Migration 008 failed");

    sqlx::raw_sql(MIGRATIONS_009)
        .execute(pool)
        .await
        .expect("Migration 009 failed");

    sqlx::raw_sql(MIGRATIONS_010)
        .execute(pool)
        .await
        .expect("Migration 010 failed");

    sqlx::raw_sql(MIGRATIONS_011)
        .execute(pool)
        .await
        .expect("Migration 011 failed");

    sqlx::raw_sql(MIGRATIONS_012)
        .execute(pool)
        .await
        .expect("Migration 012 failed");

    sqlx::raw_sql(MIGRATIONS_013)
        .execute(pool)
        .await
        .expect("Migration 013 failed");

    sqlx::raw_sql(MIGRATIONS_014)
        .execute(pool)
        .await
        .expect("Migration 014 failed");

    sqlx::raw_sql(MIGRATIONS_015)
        .execute(pool)
        .await
        .expect("Migration 015 failed");

    sqlx::raw_sql(MIGRATIONS_016)
        .execute(pool)
        .await
        .expect("Migration 016 failed");

    sqlx::raw_sql(MIGRATIONS_017)
        .execute(pool)
        .await
        .expect("Migration 017 failed");

    sqlx::raw_sql(MIGRATIONS_018)
        .execute(pool)
        .await
        .expect("Migration 018 failed");

    sqlx::raw_sql(MIGRATIONS_019)
        .execute(pool)
        .await
        .expect("Migration 019 failed");

    sqlx::raw_sql(MIGRATIONS_020)
        .execute(pool)
        .await
        .expect("Migration 020 failed");

    sqlx::raw_sql(MIGRATIONS_021)
        .execute(pool)
        .await
        .expect("Migration 021 failed");

    sqlx::raw_sql(MIGRATIONS_022)
        .execute(pool)
        .await
        .expect("Migration 022 failed");

    sqlx::raw_sql(MIGRATIONS_023)
        .execute(pool)
        .await
        .expect("Migration 023 failed");

    sqlx::raw_sql(MIGRATIONS_024)
        .execute(pool)
        .await
        .expect("Migration 024 failed");

    sqlx::raw_sql(MIGRATIONS_025)
        .execute(pool)
        .await
        .expect("Migration 025 failed");

    sqlx::raw_sql(MIGRATIONS_026)
        .execute(pool)
        .await
        .expect("Migration 026 failed");

    sqlx::raw_sql(MIGRATIONS_027)
        .execute(pool)
        .await
        .expect("Migration 027 failed");

    sqlx::raw_sql(MIGRATIONS_028)
        .execute(pool)
        .await
        .expect("Migration 028 failed");

    sqlx::raw_sql(MIGRATIONS_029)
        .execute(pool)
        .await
        .expect("Migration 029 failed");

    sqlx::raw_sql(MIGRATIONS_030)
        .execute(pool)
        .await
        .expect("Migration 030 failed");

    sqlx::raw_sql(MIGRATIONS_031)
        .execute(pool)
        .await
        .expect("Migration 031 failed");

    sqlx::raw_sql(MIGRATIONS_032)
        .execute(pool)
        .await
        .expect("Migration 032 failed");

    sqlx::raw_sql(MIGRATIONS_033)
        .execute(pool)
        .await
        .expect("Migration 033 failed");

    sqlx::raw_sql(MIGRATIONS_035)
        .execute(pool)
        .await
        .expect("Migration 035 failed");

    sqlx::raw_sql(MIGRATIONS_036)
        .execute(pool)
        .await
        .expect("Migration 036 failed");

    sqlx::raw_sql(MIGRATIONS_037)
        .execute(pool)
        .await
        .expect("Migration 037 failed");

    sqlx::raw_sql(MIGRATIONS_038)
        .execute(pool)
        .await
        .expect("Migration 038 failed");

    sqlx::raw_sql(MIGRATIONS_039)
        .execute(pool)
        .await
        .expect("Migration 039 failed");

    sqlx::raw_sql(MIGRATIONS_040)
        .execute(pool)
        .await
        .expect("Migration 040 failed");

    sqlx::raw_sql(MIGRATIONS_041)
        .execute(pool)
        .await
        .expect("Migration 041 failed");

    sqlx::raw_sql(MIGRATIONS_042)
        .execute(pool)
        .await
        .expect("Migration 042 failed");

    sqlx::raw_sql(MIGRATIONS_043)
        .execute(pool)
        .await
        .expect("Migration 043 failed");

    sqlx::raw_sql(MIGRATIONS_044)
        .execute(pool)
        .await
        .expect("Migration 044 failed");

    sqlx::raw_sql(MIGRATIONS_045)
        .execute(pool)
        .await
        .expect("Migration 045 failed");

    sqlx::raw_sql(MIGRATIONS_046)
        .execute(pool)
        .await
        .expect("Migration 046 failed");

    sqlx::raw_sql(MIGRATIONS_047)
        .execute(pool)
        .await
        .expect("Migration 047 failed");

    sqlx::raw_sql(MIGRATIONS_048)
        .execute(pool)
        .await
        .expect("Migration 048 failed");

    sqlx::raw_sql(MIGRATIONS_049)
        .execute(pool)
        .await
        .expect("Migration 049 failed");

    sqlx::raw_sql(MIGRATIONS_050)
        .execute(pool)
        .await
        .expect("Migration 050 failed");

    sqlx::raw_sql(MIGRATIONS_051)
        .execute(pool)
        .await
        .expect("Migration 051 failed");

    sqlx::raw_sql(MIGRATIONS_052)
        .execute(pool)
        .await
        .expect("Migration 052 failed");

    sqlx::raw_sql(MIGRATIONS_053)
        .execute(pool)
        .await
        .expect("Migration 053 failed");

    sqlx::raw_sql(MIGRATIONS_054)
        .execute(pool)
        .await
        .expect("Migration 054 failed");

    sqlx::raw_sql(MIGRATIONS_055)
        .execute(pool)
        .await
        .expect("Migration 055 failed");

    sqlx::raw_sql(MIGRATIONS_056)
        .execute(pool)
        .await
        .expect("Migration 056 failed");

    sqlx::raw_sql(MIGRATIONS_057)
        .execute(pool)
        .await
        .expect("Migration 057 failed");

    sqlx::raw_sql(MIGRATIONS_058)
        .execute(pool)
        .await
        .expect("Migration 058 failed");

    sqlx::raw_sql(MIGRATIONS_059)
        .execute(pool)
        .await
        .expect("Migration 059 failed");

    sqlx::raw_sql(MIGRATIONS_060)
        .execute(pool)
        .await
        .expect("Migration 060 failed");

    sqlx::raw_sql(MIGRATIONS_061)
        .execute(pool)
        .await
        .expect("Migration 061 failed");

    sqlx::raw_sql(MIGRATIONS_062)
        .execute(pool)
        .await
        .expect("Migration 062 failed");

    sqlx::raw_sql(MIGRATIONS_063)
        .execute(pool)
        .await
        .expect("Migration 063 failed");

    sqlx::raw_sql(MIGRATIONS_064)
        .execute(pool)
        .await
        .expect("Migration 064 failed");

    sqlx::raw_sql(MIGRATIONS_065)
        .execute(pool)
        .await
        .expect("Migration 065 failed");

    sqlx::raw_sql(MIGRATIONS_066)
        .execute(pool)
        .await
        .expect("Migration 066 failed");

    sqlx::raw_sql(MIGRATIONS_068)
        .execute(pool)
        .await
        .expect("Migration 068 failed");

    sqlx::raw_sql(MIGRATIONS_069)
        .execute(pool)
        .await
        .expect("Migration 069 failed");

    sqlx::raw_sql(MIGRATIONS_070)
        .execute(pool)
        .await
        .expect("Migration 070 failed");

    sqlx::raw_sql(MIGRATIONS_071)
        .execute(pool)
        .await
        .expect("Migration 071 failed");

    sqlx::raw_sql(MIGRATIONS_072)
        .execute(pool)
        .await
        .expect("Migration 072 failed");

    sqlx::raw_sql(MIGRATIONS_073)
        .execute(pool)
        .await
        .expect("Migration 073 failed");

    sqlx::raw_sql(MIGRATIONS_074)
        .execute(pool)
        .await
        .expect("Migration 074 failed");

    sqlx::raw_sql(MIGRATIONS_075)
        .execute(pool)
        .await
        .expect("Migration 075 failed");

    sqlx::raw_sql(MIGRATIONS_076)
        .execute(pool)
        .await
        .expect("Migration 076 failed");

    sqlx::raw_sql(MIGRATIONS_077)
        .execute(pool)
        .await
        .expect("Migration 077 failed");

    sqlx::raw_sql(MIGRATIONS_078)
        .execute(pool)
        .await
        .expect("Migration 078 failed");

    sqlx::raw_sql(MIGRATIONS_079)
        .execute(pool)
        .await
        .expect("Migration 079 failed");

    sqlx::raw_sql(MIGRATIONS_080)
        .execute(pool)
        .await
        .expect("Migration 080 failed");

    sqlx::raw_sql(MIGRATIONS_081)
        .execute(pool)
        .await
        .expect("Migration 081 failed");

    sqlx::raw_sql(MIGRATIONS_082)
        .execute(pool)
        .await
        .expect("Migration 082 failed");

    sqlx::raw_sql(MIGRATIONS_083)
        .execute(pool)
        .await
        .expect("Migration 083 failed");

    sqlx::raw_sql(MIGRATIONS_084)
        .execute(pool)
        .await
        .expect("Migration 084 failed");

    sqlx::raw_sql(MIGRATIONS_085)
        .execute(pool)
        .await
        .expect("Migration 085 failed");
    sqlx::raw_sql(MIGRATIONS_086)
        .execute(pool)
        .await
        .expect("Migration 086 failed");
    sqlx::raw_sql(MIGRATIONS_087)
        .execute(pool)
        .await
        .expect("Migration 087 failed");
    sqlx::raw_sql(MIGRATIONS_088)
        .execute(pool)
        .await
        .expect("Migration 088 failed");
    sqlx::raw_sql(MIGRATIONS_089)
        .execute(pool)
        .await
        .expect("Migration 089 failed");

    sqlx::raw_sql(MIGRATIONS_090)
        .execute(pool)
        .await
        .expect("Migration 090 failed");

    sqlx::raw_sql(MIGRATIONS_091)
        .execute(pool)
        .await
        .expect("Migration 091 failed");

    sqlx::raw_sql(MIGRATIONS_092)
        .execute(pool)
        .await
        .expect("Migration 092 failed");

    sqlx::raw_sql(MIGRATIONS_093)
        .execute(pool)
        .await
        .expect("Migration 093 failed");

    sqlx::raw_sql(MIGRATIONS_094)
        .execute(pool)
        .await
        .expect("Migration 094 failed");

    sqlx::raw_sql(MIGRATIONS_095)
        .execute(pool)
        .await
        .expect("Migration 095 failed");

    sqlx::raw_sql(MIGRATIONS_096)
        .execute(pool)
        .await
        .expect("Migration 096 failed");

    sqlx::raw_sql(MIGRATIONS_097)
        .execute(pool)
        .await
        .expect("Migration 097 failed");

    sqlx::raw_sql(MIGRATIONS_098)
        .execute(pool)
        .await
        .expect("Migration 098 failed");

    sqlx::raw_sql(MIGRATIONS_100)
        .execute(pool)
        .await
        .expect("Migration 100 failed");

    sqlx::raw_sql(MIGRATIONS_101)
        .execute(pool)
        .await
        .expect("Migration 101 failed");

    sqlx::raw_sql(MIGRATIONS_102)
        .execute(pool)
        .await
        .expect("Migration 102 failed");

    sqlx::raw_sql(MIGRATIONS_103)
        .execute(pool)
        .await
        .expect("Migration 103 failed");

    sqlx::raw_sql(MIGRATIONS_105)
        .execute(pool)
        .await
        .expect("Migration 105 failed");

    sqlx::raw_sql(MIGRATIONS_106)
        .execute(pool)
        .await
        .expect("Migration 106 failed");

    sqlx::raw_sql("CREATE EXTENSION IF NOT EXISTS pg_trgm;")
        .execute(pool)
        .await
        .expect("Migration 067 (pg_trgm) failed");
    // Plain CREATE (not CONCURRENTLY): CONCURRENTLY cannot run in the
    // migration's transaction and deadlocks-adjacent-waits with any manual
    // build at boot. IF NOT EXISTS makes this a no-op once built; a fresh
    // or missing index costs one write-blocking build, once.
    sqlx::raw_sql(
        "CREATE INDEX IF NOT EXISTS idx_appview_posts_text_trgm \
         ON appview_posts USING gin (text gin_trgm_ops);",
    )
    .execute(pool)
    .await
    .expect("Migration 067 (text trgm) failed");
    sqlx::raw_sql(
        "CREATE INDEX IF NOT EXISTS idx_appview_posts_handle_trgm \
         ON appview_posts USING gin (author_handle gin_trgm_ops);",
    )
    .execute(pool)
    .await
    .expect("Migration 067 (handle trgm) failed");

    tracing::info!("Migrations applied successfully");
}

#[cfg(test)]
mod migration_coverage_tests {
    /// Every migrations/*.sql file must be registered in this module — either
    /// as a MIGRATIONS_* include_str! or (special-cased files) named in a
    /// comment. An unregistered migration boots fresh databases with a stale
    /// schema while existing ones keep working — the worst failure mode.
    #[test]
    fn every_migration_file_is_registered() {
        const SELF: &str = include_str!("mod.rs");
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let mut missing = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("migrations dir") {
            let name = entry.expect("entry").file_name().to_string_lossy().to_string();
            if !name.ends_with(".sql") {
                continue;
            }
            if !SELF.contains(&name) {
                missing.push(name);
            }
        }
        assert!(missing.is_empty(), "migrations not registered in db/mod.rs: {missing:?}");
    }
}

/// add-identity-realignment phase-1 migration verification (tasks 2.4 + 4.4).
/// Integration tests against the test database (skipped without DATABASE_URL),
/// mirroring the `ap::retire` test style.
#[cfg(test)]
mod identity_realignment_migration_tests {
    use serde_json::json;
    use sqlx::PgPool;
    use uuid::Uuid;

    use crate::ap::testutil::{connect_pool, unique};
    use crate::db::accounts::LinkedAccountRow;
    use crate::db::users::UserRow;

    async fn reaction_metadata(
        pool: &PgPool,
        user_id: Uuid,
        post_id: &str,
    ) -> serde_json::Value {
        sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT metadata FROM reactions WHERE user_id = $1 AND post_id = $2",
        )
        .bind(user_id)
        .bind(post_id)
        .fetch_one(pool)
        .await
        .expect("reaction row")
    }

    /// Seed one `ap_activities` row (user_id left NULL) plus its delivery row.
    async fn seed_activity_with_queue(
        pool: &PgPool,
        id: &str,
        actor_id: &str,
        direction: &str,
        inbox_url: &str,
    ) {
        sqlx::query(
            "INSERT INTO ap_activities (id, actor_id, activity_type, direction, payload) \
             VALUES ($1, $2, 'Like', $3, $4)",
        )
        .bind(id)
        .bind(actor_id)
        .bind(direction)
        .bind(json!({"id": id, "type": "Like", "actor": actor_id}))
        .execute(pool)
        .await
        .expect("seed activity");
        sqlx::query("INSERT INTO ap_delivery_queue (activity_id, inbox_url) VALUES ($1, $2)")
            .bind(id)
            .bind(inbox_url)
            .execute(pool)
            .await
            .expect("seed queue row");
    }

    async fn activity_count(pool: &PgPool, id: &str) -> i64 {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM ap_activities WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .expect("activity count")
    }

    async fn queue_count(pool: &PgPool, id: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM ap_delivery_queue WHERE activity_id = $1",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("queue count")
    }

    /// Task 2.4: reaction metadata created under the old minted-actor identity
    /// by a Mastodon-signed-up user migrates to the home-REST identity (link
    /// instance + status id parsed from the post URI), so unreact dispatches
    /// the way the react was created. Other identities and already-migrated
    /// rows are untouched; the second run is a no-op (the `like_identity =
    /// 'ap'` predicate stops matching after the first).
    #[tokio::test]
    async fn m089_migrates_mastodon_ap_reaction_metadata_to_home_rest() {
        let Some(pool) = connect_pool().await else { return };
        let suffix = unique();

        let masto = UserRow::create_active(&pool, &format!("mig089m_{suffix}"), "", "x", "x")
            .await
            .expect("mastodon user");
        let bsky = UserRow::create_active(&pool, &format!("mig089b_{suffix}"), "", "x", "x")
            .await
            .expect("bluesky user");
        LinkedAccountRow::create(
            &pool,
            masto.id,
            "mastodon",
            &format!("z@{suffix}"),
            "z",
            "tok",
            None,
            None,
            None,
            Some("read write"),
            &json!({"instance": "mastodon.example"}),
        )
        .await
        .expect("mastodon link");
        LinkedAccountRow::create(
            &pool,
            bsky.id,
            "bluesky",
            &format!("did:plc:{suffix}"),
            "z",
            "tok",
            None,
            None,
            None,
            None,
            &json!({}),
        )
        .await
        .expect("bluesky link");

        let post_a = "https://mastodon.example/users/z/statuses/123";
        let post_b = "https://remote.test/users/z/statuses/456";
        let post_c = "https://mastodon.example/users/z/statuses/789";
        for (post_id, user_id, metadata) in [
            (post_a, masto.id, json!({"like_identity": "ap", "bluesky_like_rkey": "x"})),
            (post_b, bsky.id, json!({"like_identity": "ap", "bluesky_like_rkey": "y"})),
            (post_c, masto.id, json!({"like_identity": "home-rest", "marker": "keep"})),
        ] {
            sqlx::query(
                "INSERT INTO reactions (post_id, user_id, emoji, metadata) \
                 VALUES ($1, $2, '⭐', $3)",
            )
            .bind(post_id)
            .bind(user_id)
            .bind(metadata)
            .execute(&pool)
            .await
            .expect("seed reaction");
        }

        // The boot runner re-runs every migration at each start — run twice to
        // prove the second pass is a no-op.
        for _ in 0..2 {
            sqlx::raw_sql(include_str!(
                "../../migrations/089_reaction_home_rest_metadata.sql"
            ))
            .execute(&pool)
            .await
            .expect("migration 089");
        }

        // (a) Mastodon user's minted-actor reaction → home-REST identity.
        let a = reaction_metadata(&pool, masto.id, post_a).await;
        assert_eq!(a["like_identity"], json!("home-rest"), "(a) identity migrated");
        assert_eq!(a["home_instance"], json!("mastodon.example"), "(a) home instance from the link");
        assert_eq!(a["home_status_id"], json!("123"), "(a) status id parsed from the URI");
        assert_eq!(
            a["bluesky_like_rkey"],
            json!("x"),
            "(a) unrelated metadata keys survive the || merge"
        );

        // (b) Bluesky user's identical 'ap' metadata is not ours to migrate.
        let b = reaction_metadata(&pool, bsky.id, post_b).await;
        assert_eq!(
            b,
            json!({"like_identity": "ap", "bluesky_like_rkey": "y"}),
            "(b) non-mastodon reaction untouched"
        );

        // (c) Already home-rest with a marker key: never re-matched.
        let c = reaction_metadata(&pool, masto.id, post_c).await;
        assert_eq!(
            c,
            json!({"like_identity": "home-rest", "marker": "keep"}),
            "(c) already-migrated reaction untouched"
        );

        sqlx::query("DELETE FROM reactions WHERE user_id = ANY($1)")
            .bind(vec![masto.id, bsky.id])
            .execute(&pool)
            .await
            .expect("cleanup reactions");
        sqlx::query("DELETE FROM linked_accounts WHERE user_id = ANY($1)")
            .bind(vec![masto.id, bsky.id])
            .execute(&pool)
            .await
            .expect("cleanup links");
        sqlx::query("DELETE FROM users WHERE id = ANY($1)")
            .bind(vec![masto.id, bsky.id])
            .execute(&pool)
            .await
            .expect("cleanup users");
    }

    /// Task 4.4: 088's one-shot orphan cleanup removes ONLY outbound rows
    /// under a vanished local actor. Inbound remote history and
    /// registry-staged retirement rows are deliberately preserved (see the
    /// 088 header), and a second run deletes nothing new.
    #[tokio::test]
    async fn m088_orphan_cleanup_spares_inbound_and_registry_staged_rows() {
        let Some(pool) = connect_pool().await else { return };
        let suffix = unique();

        // (i) orphan outbound activity: direction 'out', user_id NULL, actor
        // mapping to no live user and no retirement-registry entry.
        let orphan_id = format!("https://gone.test/@ghost/activities/like-{suffix}");
        seed_activity_with_queue(
            &pool,
            &orphan_id,
            "https://gone.test/@ghost",
            "out",
            "https://gone.test/inbox",
        )
        .await;

        // (ii) inbound federated history of a remote actor (user_id NULL).
        let inbound_id = format!("https://remote.test/users/z/activities/like-{suffix}");
        seed_activity_with_queue(
            &pool,
            &inbound_id,
            "https://remote.test/users/z",
            "in",
            "https://remote.test/users/z/inbox",
        )
        .await;

        // (iii) registry-staged row: retired_ap_actors entry first (as the
        // retirement staging does), then its user-less activity + queue row.
        let staged_actor = format!("https://bytescape.test/@retired_{suffix}");
        sqlx::query("INSERT INTO retired_ap_actors (actor_id, username) VALUES ($1, $2)")
            .bind(&staged_actor)
            .bind(format!("retired_{suffix}"))
            .execute(&pool)
            .await
            .expect("registry row");
        let staged_id = format!("{staged_actor}/activities/undo-{suffix}");
        seed_activity_with_queue(
            &pool,
            &staged_id,
            &staged_actor,
            "out",
            "https://bytescape.test/inbox",
        )
        .await;

        // The boot runner re-runs 088 every start — twice proves idempotence.
        for _ in 0..2 {
            sqlx::raw_sql(include_str!("../../migrations/088_orphan_cleanup.sql"))
                .execute(&pool)
                .await
                .expect("migration 088");
        }

        // (i) gone: the activity and its delivery row.
        assert_eq!(activity_count(&pool, &orphan_id).await, 0, "(i) orphan activity purged");
        assert_eq!(queue_count(&pool, &orphan_id).await, 0, "(i) orphan queue row purged");
        // (ii)+(iii) present: 088 must never touch inbound history or
        // registry-staged rows (deliberate — see the 088 header).
        assert_eq!(activity_count(&pool, &inbound_id).await, 1, "(ii) inbound activity preserved");
        assert_eq!(queue_count(&pool, &inbound_id).await, 1, "(ii) inbound queue row preserved");
        assert_eq!(activity_count(&pool, &staged_id).await, 1, "(iii) staged activity preserved");
        assert_eq!(queue_count(&pool, &staged_id).await, 1, "(iii) staged queue row preserved");

        sqlx::query("DELETE FROM ap_delivery_queue WHERE activity_id = ANY($1)")
            .bind(vec![orphan_id.clone(), inbound_id.clone(), staged_id.clone()])
            .execute(&pool)
            .await
            .expect("cleanup queue rows");
        sqlx::query("DELETE FROM ap_activities WHERE id = ANY($1)")
            .bind(vec![orphan_id, inbound_id, staged_id])
            .execute(&pool)
            .await
            .expect("cleanup activities");
        sqlx::query("DELETE FROM retired_ap_actors WHERE actor_id = $1")
            .bind(&staged_actor)
            .execute(&pool)
            .await
            .expect("cleanup registry row");
    }

    async fn queue_seq(pool: &PgPool, row: Uuid) -> i64 {
        sqlx::query_scalar::<_, i64>("SELECT seq FROM ap_delivery_queue WHERE id = $1")
            .bind(row)
            .fetch_one(pool)
            .await
            .expect("queue seq")
    }

    /// CR-4: 086's seq backfill must (a) never let a same-`created_at` Undo
    /// leapfrog the Like it undoes and (b) be a strict no-op on re-run — the
    /// boot runner executes every migration at every start, and the old
    /// `AND q.seq IS DISTINCT FROM s.rn` re-ranked every row on each boot with
    /// a `row_number() OVER (ORDER BY created_at, id)` tiebreak on the RANDOM
    /// queue uuid, so a re-run could re-invert the exact Like → Undo pair 086
    /// exists to protect (the pre-086 leapfrog bug).
    #[tokio::test]
    async fn m086_seq_backfill_orders_same_created_at_pairs_and_reruns_are_noops() {
        let Some(pool) = connect_pool().await else { return };
        let suffix = unique();

        // Same-instant Like → Undo pair. The Like's queue id is the LARGEST
        // possible uuid and the Undo's the SMALLEST, so any
        // `ORDER BY created_at, id` tiebreak puts the Undo first — the exact
        // leapfrog 086 must prevent.
        let actor = format!("https://bytescape.test/@mig086_{suffix}");
        // Shield the fixture rows from a concurrently running 088 re-run (the
        // m088 test executes 088 against this shared DB; its orphan DELETE
        // spares registry-staged actors only). Pre-drained so the row is
        // invisible to `retire::drain_once`, whose return value counts every
        // undrained registry row globally (retirement tests assert on it).
        sqlx::query(
            "INSERT INTO retired_ap_actors (actor_id, username, drained_at) VALUES ($1, $2, now())",
        )
        .bind(&actor)
        .bind(format!("mig086_{suffix}"))
        .execute(&pool)
        .await
        .expect("registry shield");
        let like_id = format!("{actor}/activities/like-{suffix}");
        let undo_id = format!("{actor}/activities/undo-{suffix}");
        sqlx::query(
            "INSERT INTO ap_activities (id, actor_id, activity_type, object_id, direction, payload) \
             VALUES ($1, $2, 'Like', $3, 'out', $4)",
        )
        .bind(&like_id)
        .bind(&actor)
        .bind(format!("https://remote.test/users/z/statuses/{suffix}"))
        .bind(json!({"id": like_id, "type": "Like", "actor": actor}))
        .execute(&pool)
        .await
        .expect("seed like activity");
        sqlx::query(
            "INSERT INTO ap_activities (id, actor_id, activity_type, object_id, direction, payload) \
             VALUES ($1, $2, 'Undo', $3, 'out', $4)",
        )
        .bind(&undo_id)
        .bind(&actor)
        .bind(&like_id)
        .bind(json!({"id": undo_id, "type": "Undo", "object": like_id}))
        .execute(&pool)
        .await
        .expect("seed undo activity");

        let like_row = Uuid::from_u128(u128::MAX); // ffffffff-…-ffffffff
        let undo_row = Uuid::nil(); //                00000000-…-00000000
        sqlx::query("DELETE FROM ap_delivery_queue WHERE id = ANY($1)")
            .bind(vec![like_row, undo_row])
            .execute(&pool)
            .await
            .expect("stale-row pre-clean");
        // Identical created_at (the tiebreak hazard), Like inserted FIRST so
        // its auto-assigned seq precedes the Undo's (insertion order). The far
        // future next_attempt_at keeps the rows out of every claim scan.
        let created_at = chrono::Utc::now();
        for (row, activity_id) in [(like_row, &like_id), (undo_row, &undo_id)] {
            sqlx::query(
                "INSERT INTO ap_delivery_queue (id, activity_id, inbox_url, created_at, next_attempt_at) \
                 VALUES ($1, $2, $3, $4, $4 + interval '1 day')",
            )
            .bind(row)
            .bind(activity_id)
            .bind(format!("https://remote.test/inbox-{suffix}"))
            .bind(created_at)
            .execute(&pool)
            .await
            .expect("seed queue row");
        }

        let initial_like = queue_seq(&pool, like_row).await;
        let initial_undo = queue_seq(&pool, undo_row).await;
        assert!(
            initial_like < initial_undo,
            "insertion order: the Like precedes its Undo (Like={initial_like}, Undo={initial_undo})"
        );

        // The boot runner re-runs 086 every start — twice proves the second
        // (and every later) pass changes nothing.
        for run in 1..=2 {
            sqlx::raw_sql(include_str!("../../migrations/086_ap_delivery_fifo.sql"))
                .execute(&pool)
                .await
                .expect("migration 086");
            let (sl, su) = (queue_seq(&pool, like_row).await, queue_seq(&pool, undo_row).await);
            assert!(
                sl < su,
                "run {run}: the Undo must never leapfrog the Like it undoes (Like={sl}, Undo={su})"
            );
            assert_eq!(
                (sl, su),
                (initial_like, initial_undo),
                "run {run}: re-running 086 must never re-sequence already-sequenced rows"
            );
        }

        sqlx::query("DELETE FROM ap_delivery_queue WHERE id = ANY($1)")
            .bind(vec![like_row, undo_row])
            .execute(&pool)
            .await
            .expect("cleanup queue rows");
        sqlx::query("DELETE FROM ap_activities WHERE id = ANY($1)")
            .bind(vec![like_id, undo_id])
            .execute(&pool)
            .await
            .expect("cleanup activities");
        sqlx::query("DELETE FROM retired_ap_actors WHERE actor_id = $1")
            .bind(&actor)
            .execute(&pool)
            .await
            .expect("cleanup registry shield");
    }
}
