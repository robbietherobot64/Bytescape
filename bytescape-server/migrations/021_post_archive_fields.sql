-- Add original_platform enum column + import_batch_id for archive grouping
DO $$ BEGIN
    CREATE TYPE post_platform AS ENUM (
        'bytescape', 'bluesky', 'mastodon', 'lemmy', 'google', 'facebook', 'instagram', 'x', 'rss', 'unknown'
    );
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

ALTER TABLE posts
  ADD COLUMN IF NOT EXISTS original_platform post_platform DEFAULT 'bytescape',
  ADD COLUMN IF NOT EXISTS import_batch_id UUID;

-- Index for fast profile feed filtering by platform + date
CREATE INDEX IF NOT EXISTS idx_posts_user_platform_ts ON posts(user_id, original_platform, original_ts DESC);
