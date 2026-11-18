ALTER TABLE media
    ADD COLUMN IF NOT EXISTS user_id UUID REFERENCES users(id),
    ALTER COLUMN post_id DROP NOT NULL;

CREATE INDEX IF NOT EXISTS idx_media_user_id ON media(user_id);
