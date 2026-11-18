-- cached_posts: normalized cache of external timeline posts for instant feed loading
CREATE TABLE IF NOT EXISTS cached_posts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source          TEXT NOT NULL,        -- 'bluesky', 'mastodon', 'lemmy', 'rss'
    external_id     TEXT NOT NULL,
    author          TEXT NOT NULL,
    author_handle   TEXT,                -- e.g. @user@instance.com
    avatar          TEXT,
    content         TEXT NOT NULL,
    html_content    TEXT,
    media_urls      TEXT[] DEFAULT '{}',
    source_url      TEXT,
    created_at      TIMESTAMPTZ NOT NULL,
    fetched_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, source, external_id)
);

CREATE INDEX IF NOT EXISTS idx_cached_posts_user_time
    ON cached_posts(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_cached_posts_user_source_time
    ON cached_posts(user_id, source, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_cached_posts_fetched_at
    ON cached_posts(fetched_at);
