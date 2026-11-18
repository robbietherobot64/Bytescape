-- bytescape.social — Postgres schema
-- Run by `db::run_migrations` on startup.
-- Uses IF NOT EXISTS for idempotent migrations.

-- =============================================================================
-- Users
-- =============================================================================
CREATE TABLE IF NOT EXISTS users (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username        TEXT NOT NULL UNIQUE,
    display_name    TEXT NOT NULL DEFAULT '',
    email           TEXT UNIQUE,
    password_hash   TEXT NOT NULL,
    public_key      TEXT NOT NULL,               -- hex-encoded Ed25519
    encrypted_privkey TEXT,                      -- client-side key, server stores for sync
    recovery_escrow  TEXT,                       -- blind escrow bundle (JSON)
    bio             TEXT NOT NULL DEFAULT '',
    avatar_url      TEXT,
    header_url      TEXT,
    theme           TEXT NOT NULL DEFAULT 'default',
    custom_css      TEXT NOT NULL DEFAULT '',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =============================================================================
-- AP Actors (one per user, created on signup)
-- =============================================================================
CREATE TABLE IF NOT EXISTS ap_actors (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    username        TEXT NOT NULL,
    domain          TEXT NOT NULL DEFAULT 'bytescape.social',
    public_key      TEXT NOT NULL,
    also_known_as   JSONB NOT NULL DEFAULT '[]',
    inbox_count     INTEGER NOT NULL DEFAULT 0,
    outbox_count    INTEGER NOT NULL DEFAULT 0,
    followers_count INTEGER NOT NULL DEFAULT 0,
    following_count INTEGER NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(username, domain)
);

-- =============================================================================
-- Import sessions (tracks migration progress)
-- =============================================================================
CREATE TABLE IF NOT EXISTS import_sessions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source          TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending',
    total_posts     INTEGER NOT NULL DEFAULT 0,
    imported_posts  INTEGER NOT NULL DEFAULT 0,
    total_media     INTEGER NOT NULL DEFAULT 0,
    imported_media  INTEGER NOT NULL DEFAULT 0,
    error_message   TEXT,
    started_at      TIMESTAMPTZ,
    completed_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =============================================================================
-- Posts (unified — both imported and native)
-- =============================================================================
CREATE TABLE IF NOT EXISTS posts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source          TEXT NOT NULL,
    source_url      TEXT,
    ap_object_id    TEXT UNIQUE,
    content         JSONB NOT NULL,              -- structured content blocks
    content_type    TEXT NOT NULL DEFAULT 'microblog',
    community       TEXT,
    reply_parent_id UUID REFERENCES posts(id),
    visibility      TEXT NOT NULL DEFAULT 'public',
    original_ts     TIMESTAMPTZ NOT NULL,
    imported_at     TIMESTAMPTZ,
    is_imported     BOOLEAN NOT NULL DEFAULT false,
    import_session_id UUID REFERENCES import_sessions(id),
    likes_count     INTEGER NOT NULL DEFAULT 0,
    shares_count    INTEGER NOT NULL DEFAULT 0,
    replies_count   INTEGER NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =============================================================================
-- Media attachments
-- =============================================================================
CREATE TABLE IF NOT EXISTS media (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    post_id         UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    storage_url     TEXT NOT NULL,
    thumbnail_url   TEXT,
    mime_type       TEXT NOT NULL,
    width           INTEGER,
    height          INTEGER,
    alt_text        TEXT,
    location_lat    DOUBLE PRECISION,
    location_lng    DOUBLE PRECISION,
    taken_at        TIMESTAMPTZ,
    file_size       BIGINT,
    content_hash    TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =============================================================================
-- Follows / Subscriptions (outgoing)
-- =============================================================================
CREATE TABLE IF NOT EXISTS follows (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    follower_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    followed_actor  TEXT NOT NULL,
    follow_type     TEXT NOT NULL DEFAULT 'person',
    label           TEXT,
    ap_kind         TEXT,
    status          TEXT NOT NULL DEFAULT 'pending',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(follower_id, followed_actor)
);

-- =============================================================================
-- Followers (incoming from AP — cached for actor document)
-- =============================================================================
CREATE TABLE IF NOT EXISTS followers (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_id        TEXT NOT NULL,
    inbox_url       TEXT NOT NULL,
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    accepted        BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(actor_id, user_id)
);

-- =============================================================================
-- Import sessions (tracks migration progress)
-- =============================================================================
CREATE TABLE IF NOT EXISTS import_sessions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source          TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending',
    total_posts     INTEGER NOT NULL DEFAULT 0,
    imported_posts  INTEGER NOT NULL DEFAULT 0,
    total_media     INTEGER NOT NULL DEFAULT 0,
    imported_media  INTEGER NOT NULL DEFAULT 0,
    error_message   TEXT,
    started_at      TIMESTAMPTZ,
    completed_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =============================================================================
-- Outbound AP delivery queue
-- =============================================================================
CREATE TABLE IF NOT EXISTS delivery_queue (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    activity        JSONB NOT NULL,
    inbox_url       TEXT NOT NULL,
    retries         INTEGER NOT NULL DEFAULT 0,
    max_retries     INTEGER NOT NULL DEFAULT 5,
    last_error      TEXT,
    status          TEXT NOT NULL DEFAULT 'pending',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    next_retry_at   TIMESTAMPTZ
);

-- =============================================================================
-- Indexes
-- =============================================================================
CREATE INDEX IF NOT EXISTS idx_posts_user_id ON posts(user_id);
CREATE INDEX IF NOT EXISTS idx_posts_original_ts ON posts(original_ts DESC);
CREATE INDEX IF NOT EXISTS idx_posts_source ON posts(source);
CREATE INDEX IF NOT EXISTS idx_posts_content_type ON posts(content_type);
CREATE INDEX IF NOT EXISTS idx_media_post_id ON media(post_id);
CREATE INDEX IF NOT EXISTS idx_import_sessions_user_id ON import_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_follows_follower_id ON follows(follower_id);
CREATE INDEX IF NOT EXISTS idx_followers_user_id ON followers(user_id);
CREATE INDEX IF NOT EXISTS idx_delivery_queue_status ON delivery_queue(status)
    WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_delivery_queue_next_retry ON delivery_queue(next_retry_at)
    WHERE next_retry_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ap_actors_user_id ON ap_actors(user_id);
