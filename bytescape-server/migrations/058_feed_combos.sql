-- Modular feed builder: user-combined topic feeds, published as real
-- app.bsky.feed.generator records (2026-10-14).
CREATE TABLE IF NOT EXISTS feed_combos (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    rkey TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    topics TEXT[] NOT NULL DEFAULT '{}',
    at_uri TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_feed_combos_user ON feed_combos(user_id);
