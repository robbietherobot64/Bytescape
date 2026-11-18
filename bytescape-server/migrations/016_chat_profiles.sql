CREATE TABLE IF NOT EXISTS chat_profiles (
    platform      TEXT NOT NULL,
    did           TEXT NOT NULL,
    display_name  TEXT,
    handle        TEXT,
    avatar        TEXT,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (platform, did)
);
