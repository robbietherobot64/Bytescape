-- migration name: calendar_events (derived calendar index)
-- purpose: events become community.lexicon.calendar.* records on user PDSes;
--   the DB is a derived index keyed by at-URI (spec: events/ingestion).
--   Replaces the ActivityPub-era events/event_rsvps/event_comments(UUID) tables.
--
-- Pre-drop inspection (2026-11-08, dev DB): events=0, event_rsvps=0,
--   event_comments=0 — all three legacy tables empty, so no export snapshot
--   was produced (snapshot condition: rows existed).
--
-- Note: `source` defaults to 'atproto'; ActivityPub-sourced rows (Mobilizon/
--   Gancio ingest) land later with source='activitypub' — uri stays TEXT so
--   both at-URIs and AP object ids fit without schema change.

-- ── Drop legacy (children first; sole FKs: event_rsvps.event_id,
--    event_comments.event_id → events.id) ─────────────────────────────
-- event_comments keeps its NAME but changes shape: drop it only while it
-- still has the legacy event_id column, or every subsequent boot would wipe
-- user comments (create-table idempotency does not protect a name reuse).
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'event_comments' AND column_name = 'event_id'
    ) THEN
        DROP TABLE event_comments;
    END IF;
END $$;
DROP TABLE IF EXISTS event_rsvps;
DROP TABLE IF EXISTS events;

-- ── Derived event index ──────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS calendar_events (
    uri              TEXT PRIMARY KEY,          -- at://did/collection/rkey
    cid              TEXT NOT NULL,
    author_did       TEXT NOT NULL,
    author_handle    TEXT,                      -- nullable cache; NULL = unresolved (render honestly)
    name             TEXT NOT NULL,
    description      TEXT,
    starts_at        TIMESTAMPTZ,
    ends_at          TIMESTAMPTZ,
    mode             TEXT,                      -- community.lexicon.calendar.event#inperson|virtual|hybrid
    status           TEXT NOT NULL DEFAULT 'community.lexicon.calendar.event#scheduled',
    location_text    TEXT,                      -- extracted searchable location text
    locations        JSONB NOT NULL DEFAULT '[]', -- payload only, never a sort key
    uris             JSONB NOT NULL DEFAULT '[]', -- payload only; image rides here as {uri,name:"image"}
    record_created_at TIMESTAMPTZ,              -- the record's createdAt field
    source           TEXT NOT NULL DEFAULT 'atproto',
    indexed_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_calendar_events_starts_at
    ON calendar_events (starts_at);
CREATE INDEX IF NOT EXISTS idx_calendar_events_author
    ON calendar_events (author_did);
-- Search matches this EXACT expression (name + description, english config);
-- queries must reuse coalesce(name,'') || ' ' || coalesce(description,'')
-- or the planner will not use this index (documented OR/scan pitfalls).
CREATE INDEX IF NOT EXISTS idx_calendar_events_search
    ON calendar_events USING GIN
        (to_tsvector('english', coalesce(name, '') || ' ' || coalesce(description, '')));
-- Location filter runs ILIKE; gin_trgm_ops keeps it index-backed instead of
-- a table scan. The extension is also ensured at boot by the 067 block —
-- but 079 runs BEFORE it, so ensure here too (fresh-database ordering).
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE INDEX IF NOT EXISTS idx_calendar_events_location_trgm
    ON calendar_events USING GIN (location_text gin_trgm_ops);

-- ── RSVP rollup source rows ──────────────────────────────────────────
CREATE TABLE IF NOT EXISTS calendar_rsvps (
    uri              TEXT PRIMARY KEY,          -- the rsvp record's at-URI
    event_uri        TEXT NOT NULL,
    author_did       TEXT NOT NULL,
    status           TEXT NOT NULL,             -- #going | #interested | #notgoing
    record_created_at TIMESTAMPTZ,
    indexed_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Latest-wins rollup per (event, author): DISTINCT ON needs this order.
CREATE INDEX IF NOT EXISTS idx_calendar_rsvps_event_author
    ON calendar_rsvps (event_uri, author_did, record_created_at DESC);
-- Count queries: per-event counts by status.
CREATE INDEX IF NOT EXISTS idx_calendar_rsvps_counts
    ON calendar_rsvps (event_uri, status);

-- ── Event comments, re-keyed to the event's at-URI ───────────────────
-- (Comments stay Bytescape-side: the calendar lexicons define no comment
--  type. Orphan rows are never served — detail joins through calendar_events;
--  the ingestion delete path also removes them.)
CREATE TABLE IF NOT EXISTS event_comments (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_uri   TEXT NOT NULL,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    content     TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_event_comments_event_uri
    ON event_comments (event_uri, created_at);
CREATE INDEX IF NOT EXISTS idx_event_comments_user_id
    ON event_comments (user_id);
