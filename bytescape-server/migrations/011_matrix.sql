-- Matrix Events (messages, room state, membership changes)
CREATE TABLE IF NOT EXISTS matrix_events (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    room_id         TEXT NOT NULL,
    event_id        TEXT NOT NULL UNIQUE,
    sender          TEXT NOT NULL, -- e.g., @alice:bytescape.social
    event_type      TEXT NOT NULL, -- e.g., m.room.message, m.room.member
    state_key       TEXT,          -- only for state events
    content         JSONB NOT NULL,
    origin_server_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::bigint,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_matrix_events_room_ordered ON matrix_events(room_id, origin_server_ts DESC);
CREATE INDEX IF NOT EXISTS idx_matrix_events_type ON matrix_events(event_type);

-- Matrix Room Membership (lightweight state store)
CREATE TABLE IF NOT EXISTS matrix_room_members (
    room_id         TEXT NOT NULL,
    user_id         TEXT NOT NULL,
    membership      TEXT NOT NULL DEFAULT 'join', -- join, invite, leave, ban
    display_name    TEXT,
    avatar_url      TEXT,
    joined_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (room_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_matrix_room_members_user ON matrix_room_members(user_id);

-- Sync Tokens (opaque since tokens for /sync)
CREATE TABLE IF NOT EXISTS matrix_sync_tokens (
    id              BIGSERIAL PRIMARY KEY,
    token           TEXT NOT NULL UNIQUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
