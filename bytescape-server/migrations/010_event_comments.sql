-- Replay guard (added with 079): on databases already converted by
-- 079_calendar_events.sql, event_comments is re-keyed to event_uri — the
-- legacy event_id column is gone, and an unguarded index on it panics every
-- subsequent boot. Fresh databases take exactly the original path below
-- (legacy shape first; 079 converts it later in the same run).
CREATE TABLE IF NOT EXISTS event_comments (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id    UUID NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    content     TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'event_comments' AND column_name = 'event_id'
    ) THEN
        CREATE INDEX IF NOT EXISTS idx_event_comments_event_id ON event_comments(event_id);
    END IF;
END $$;
CREATE INDEX IF NOT EXISTS idx_event_comments_user_id ON event_comments(user_id);
