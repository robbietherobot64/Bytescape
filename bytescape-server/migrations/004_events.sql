CREATE TABLE IF NOT EXISTS events (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    ap_object_id    TEXT UNIQUE NOT NULL,
    attributed_to   TEXT NOT NULL,
    name            TEXT NOT NULL,
    content         TEXT,
    content_html    TEXT,
    start_time      TIMESTAMPTZ NOT NULL,
    end_time        TIMESTAMPTZ,
    location        JSONB,
    source_url      TEXT,
    picture_url     TEXT,
    source          TEXT NOT NULL,
    received_by     UUID NOT NULL REFERENCES users(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_events_start_time ON events(start_time);
CREATE INDEX IF NOT EXISTS idx_events_ap_object_id ON events(ap_object_id);
