-- bytescape.social — Social Graph: confirmed two-way matches
CREATE TABLE IF NOT EXISTS social_graph_matches (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_a_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_b_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    match_type      TEXT NOT NULL,
    source          TEXT NOT NULL,
    confidence      TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    notified        BOOLEAN NOT NULL DEFAULT FALSE,
    UNIQUE(user_a_id, user_b_id)
);
CREATE INDEX IF NOT EXISTS idx_social_graph_matches_a ON social_graph_matches(user_a_id);
CREATE INDEX IF NOT EXISTS idx_social_graph_matches_b ON social_graph_matches(user_b_id);
