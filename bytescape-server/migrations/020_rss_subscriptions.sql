-- Extend subscriptions table for RSS feeds
ALTER TABLE subscriptions
  ADD COLUMN IF NOT EXISTS url TEXT,
  ADD COLUMN IF NOT EXISTS last_fetched_at TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS fetch_error TEXT;

-- Index for fast RSS subscription lookup
CREATE INDEX IF NOT EXISTS idx_subscriptions_url ON subscriptions(url);

-- Table for cached RSS feed items (acts as a normalized post cache)
CREATE TABLE IF NOT EXISTS rss_feed_items (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subscription_id UUID NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    guid            TEXT NOT NULL,
    title           TEXT,
    link            TEXT,
    description     TEXT,
    author          TEXT,
    pub_date        TIMESTAMPTZ NOT NULL,
    content         TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(subscription_id, guid)
);

CREATE INDEX IF NOT EXISTS idx_rss_items_subscription ON rss_feed_items(subscription_id);
CREATE INDEX IF NOT EXISTS idx_rss_items_user_date ON rss_feed_items(user_id, pub_date DESC);
