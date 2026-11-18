-- Add reshared_post_id to track native reshares
ALTER TABLE posts ADD COLUMN IF NOT EXISTS reshared_post_id UUID REFERENCES posts(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_posts_reshared_post_id ON posts(reshared_post_id);
