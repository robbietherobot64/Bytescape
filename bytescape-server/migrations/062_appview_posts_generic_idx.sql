-- Generic (non-post) records — events, listings, custom lexicons — get a
-- partial index: the interest-word matcher and the type filter query only
-- this small subset, and must never seq-scan the 4.7M-row post table.
-- Column order must match the queries' ORDER BY exactly (DESC NULLS LAST),
-- or Postgres cannot walk it in order and falls back to a seq scan.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_appview_posts_generic_created
    ON appview_posts (created_at DESC NULLS LAST)
    WHERE uri NOT LIKE '%/app.bsky.feed.post/%';
