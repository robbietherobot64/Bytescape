CREATE INDEX IF NOT EXISTS idx_appview_posts_created_uri
  ON appview_posts (created_at DESC, uri DESC)
  WHERE reply_parent IS NULL AND text IS NOT NULL AND text <> '';
