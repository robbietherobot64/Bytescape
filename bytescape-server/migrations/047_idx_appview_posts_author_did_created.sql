-- Composite partial index for fast social-graph seed feed queries.
-- Covers: WHERE author_did = ANY($1) AND reply_parent IS NULL AND text IS NOT NULL AND text <> ''
-- ordered by created_at DESC.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_appview_posts_author_did_created
  ON appview_posts (author_did, created_at DESC)
  WHERE reply_parent IS NULL AND text IS NOT NULL AND text <> '';
