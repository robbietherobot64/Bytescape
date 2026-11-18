CREATE TABLE IF NOT EXISTS appview_posts (
    uri TEXT PRIMARY KEY,
    cid TEXT NOT NULL,
    author_did TEXT NOT NULL,
    author_handle TEXT,
    text TEXT,
    reply_parent TEXT,
    reply_root TEXT,
    languages TEXT[],
    created_at TIMESTAMPTZ,
    indexed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_appview_posts_text ON appview_posts USING gin(to_tsvector('english', text));
CREATE INDEX IF NOT EXISTS idx_appview_posts_author ON appview_posts(author_did);
CREATE INDEX IF NOT EXISTS idx_appview_posts_created ON appview_posts(created_at DESC);

CREATE TABLE IF NOT EXISTS appview_cursor (
    id INT PRIMARY KEY DEFAULT 1,
    cursor BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
INSERT INTO appview_cursor (id, cursor) VALUES (1, 0) ON CONFLICT DO NOTHING;