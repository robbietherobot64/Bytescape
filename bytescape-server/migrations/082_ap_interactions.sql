-- ActivityPub interaction surface (Phase C of add-activitypub-integration).
--
-- Persistent federation state for native Bytescape actors:
--  * ap_activities     — every inbound/outbound activity, deduped by id (the
--                        replay guard and the interaction-state record: Like /
--                        Announce / Follow originals live here until undone).
--  * ap_remote_actors  — resolved remote actor cache (inboxes + public keys).
--  * ap_delivery_queue — outbound delivery attempts with bounded retries.
--
-- All columns are additive; nothing here touches the AT surface.

-- ─── Activities ────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS ap_activities (
    id            TEXT PRIMARY KEY,             -- stable activity URI
    actor_id      TEXT NOT NULL,                -- AP actor URL that authored it
    activity_type TEXT NOT NULL,                -- Create / Update / Follow / Accept / Like / Announce / Undo
    object_id     TEXT,                         -- target object/activity URI
    local_uri     TEXT UNIQUE,                  -- at:// record URI for the undoable OUTBOUND original (consumed on delete)
    direction     TEXT NOT NULL,                -- 'in' | 'out'
    payload       JSONB NOT NULL,               -- exact activity JSON (delivered / served verbatim)
    undone_at     TIMESTAMPTZ,                  -- set once when an Undo retires the original
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Outbox paging: newest-first keyset over (created_at, id).
CREATE INDEX IF NOT EXISTS idx_ap_activities_outbox
    ON ap_activities (actor_id, created_at DESC, id DESC);

-- Undo/interaction lookups: original by (signer, object, type).
CREATE INDEX IF NOT EXISTS idx_ap_activities_actor_object
    ON ap_activities (actor_id, object_id, activity_type);

-- ─── Remote actors ─────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS ap_remote_actors (
    actor_id            TEXT PRIMARY KEY,       -- remote actor URL
    inbox_url           TEXT NOT NULL,          -- delivery target
    shared_inbox_url    TEXT,                   -- endpoints.sharedInbox when advertised
    public_key_pem      TEXT,                   -- publicKey.publicKeyPem (RSA, PKCS#1); NULL until a key is seen
    preferred_username  TEXT,
    display_name        TEXT,
    avatar_url          TEXT,
    fetched_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ─── Outbound delivery queue ───────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS ap_delivery_queue (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    activity_id     TEXT NOT NULL REFERENCES ap_activities(id) ON DELETE CASCADE,
    inbox_url       TEXT NOT NULL,
    attempts        INTEGER NOT NULL DEFAULT 0,
    max_attempts    INTEGER NOT NULL DEFAULT 8,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    status          TEXT NOT NULL DEFAULT 'pending',  -- pending | delivered | failed
    last_error      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- one row per (activity, inbox): sharedInbox dedupe and re-enqueue safety.
    UNIQUE (activity_id, inbox_url)
);

-- Worker's due scan: pending rows only, ordered by when they may run.
CREATE INDEX IF NOT EXISTS idx_ap_delivery_queue_due
    ON ap_delivery_queue (next_attempt_at) WHERE status = 'pending';