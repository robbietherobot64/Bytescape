-- migration name: ap_sources (fediverse content-source registry for the AP feed)
-- purpose: the standing ActivityPub feed puller polls a broad, managed set of
--   fediverse instances (Mastodon / Pixelfed / Lemmy / generic) for public
--   content. Each instance is a row here with health/verification state so the
--   source set can evolve on its own ("as much of the fediverse as possible,
--   pruned by a verification criterion"). Discovered hosts (from ingested
--   posts) are upserted here, so the registry grows as we see more of the
--   network; repeatedly-failing hosts are cooled down and periodically re-probed.
--
-- Rebuildable: this is coordination state, not user data. Deleting a row just
--   makes the host a discovery candidate again.

CREATE TABLE IF NOT EXISTS ap_sources (
    host            TEXT PRIMARY KEY,                 -- instance hostname, e.g. "mastodon.social"
    kind            TEXT NOT NULL DEFAULT 'generic',  -- mastodon | pixelfed | lemmy | generic
    enabled         BOOLEAN NOT NULL DEFAULT TRUE,    -- false = cooled down / retired
    failures        INT NOT NULL DEFAULT 0,           -- consecutive fetch failures
    last_ok_at      TIMESTAMPTZ,                      -- last successful fetch
    last_items      INT NOT NULL DEFAULT 0,           -- items returned by the last fetch
    fetched_at      TIMESTAMPTZ,                      -- last fetch attempt (success or not)
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The poller picks the least-recently-fetched enabled source each pass.
CREATE INDEX IF NOT EXISTS idx_ap_sources_due
    ON ap_sources (enabled, fetched_at NULLS FIRST);
