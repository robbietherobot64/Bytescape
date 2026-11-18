-- migration name: ap_source_priority (curated sources outrank discovered ones)
-- purpose: the poller's probe lane (never-successful hosts) is where a newly
--   added *kind* becomes observable. Discovery inserts hundreds of guessed
--   hosts, so without a rank a deliberately seeded source (PieFed, Mobilizon)
--   queues behind ~1,500 never-probed discoveries and may not be verified for
--   days. `priority` is that rank: seed_defaults pins the curated set to 1 at
--   every boot, discovery leaves it at 0.
--
-- Rebuildable: ap_sources is coordination state. The default makes this column
--   safe to add on any database; re-running is a no-op.

ALTER TABLE ap_sources
    ADD COLUMN IF NOT EXISTS priority INT NOT NULL DEFAULT 0;

-- Backfill the kinds that only exist because a discovery upgraded the row or
-- migration 095 reclassified it (they are not all in the Rust seed list).
UPDATE ap_sources SET priority = 1 WHERE kind IN ('piefed', 'mobilizon');
