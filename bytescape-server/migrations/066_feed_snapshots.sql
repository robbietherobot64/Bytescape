-- Serve-first feed: precomputed seed snapshots. The /api/feed/combined seed
-- branch serves one JSON row (built off-request) instead of recomputing the
-- whole feed pipeline per request. payload = the verbatim FeedResponse JSON
-- (items/cursor/has_more/phase). Key covers user + the query axes that change
-- the seed result (sort, ids, author, source_categories, types).
CREATE TABLE IF NOT EXISTS feed_snapshots (
    key TEXT PRIMARY KEY,
    payload JSONB NOT NULL,
    built_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_feed_snapshots_built_at ON feed_snapshots (built_at);
