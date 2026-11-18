ALTER TABLE appview_posts ADD COLUMN IF NOT EXISTS like_count INTEGER DEFAULT 0;
ALTER TABLE appview_posts ADD COLUMN IF NOT EXISTS repost_count INTEGER DEFAULT 0;
ALTER TABLE appview_posts ADD COLUMN IF NOT EXISTS reply_count INTEGER DEFAULT 0;
CREATE INDEX IF NOT EXISTS idx_appview_posts_engagement ON appview_posts (created_at DESC, like_count DESC, repost_count DESC, reply_count DESC);
