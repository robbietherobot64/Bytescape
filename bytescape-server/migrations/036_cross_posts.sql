-- Cross-post tracking: stores the URL of each cross-posted copy on external platforms
-- so we can delete/edit them when the original Bytescape post is modified.

CREATE TABLE IF NOT EXISTS cross_posts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    post_id UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    platform TEXT NOT NULL, -- 'mastodon', 'bluesky', 'lemmy'
    external_url TEXT NOT NULL, -- the URL/URI on the external platform
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_cross_posts_post_id ON cross_posts(post_id);
CREATE INDEX IF NOT EXISTS idx_cross_posts_user_id ON cross_posts(user_id);
