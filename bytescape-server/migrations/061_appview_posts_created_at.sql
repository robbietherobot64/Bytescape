-- Retention + recency ordering: the prune job and the feed ORDER BY both key
-- on created_at. (Created CONCURRENTLY in ops before first deploy; this is
-- the idempotent boot-time equivalent.)
CREATE INDEX IF NOT EXISTS idx_appview_posts_created_at ON appview_posts (created_at);
