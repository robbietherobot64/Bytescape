-- bytescape.social — Content integration schema
-- Linked external accounts + subscriptions + discovery

-- =============================================================================
-- Linked external accounts (Bluesky, Mastodon, etc.)
-- =============================================================================
CREATE TABLE IF NOT EXISTS linked_accounts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider        TEXT NOT NULL,                   -- 'bluesky', 'mastodon'
    external_id     TEXT NOT NULL,                   -- Bluesky DID, Mastodon acct URI
    external_username TEXT NOT NULL,                 -- @handle@domain or DID
    token_type      TEXT NOT NULL DEFAULT 'oauth',   -- 'oauth', 'app_password'
    access_token    TEXT NOT NULL,
    refresh_token   TEXT,
    token_expires_at TIMESTAMPTZ,
    scope           TEXT,
    dpop_private_key TEXT,                           -- hex-encoded Ed25519 for DPoP
    metadata        JSONB NOT NULL DEFAULT '{}',     -- extra provider-specific data
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, provider, external_id)
);

-- =============================================================================
-- User content subscriptions (hashtags, feeds, communities)
-- =============================================================================
CREATE TABLE IF NOT EXISTS subscriptions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source          TEXT NOT NULL,                   -- 'hashtag', 'bluesky_feed', 'mastodon_list', 'lemmy_community', 'piefed_community'
    source_id       TEXT NOT NULL,                   -- tag name, feed URI, community URL
    label           TEXT NOT NULL,                   -- display label
    icon            TEXT NOT NULL DEFAULT '#',
    category        TEXT NOT NULL DEFAULT 'hashtags', -- 'follows', 'feeds', 'hashtags', 'subscriptions'
    enabled         BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, source, source_id)
);

-- =============================================================================
-- Indexes
-- =============================================================================
CREATE INDEX IF NOT EXISTS idx_linked_accounts_user_id ON linked_accounts(user_id);
CREATE INDEX IF NOT EXISTS idx_linked_accounts_provider ON linked_accounts(provider);
CREATE INDEX IF NOT EXISTS idx_subscriptions_user_id ON subscriptions(user_id);
CREATE INDEX IF NOT EXISTS idx_subscriptions_category ON subscriptions(category);
