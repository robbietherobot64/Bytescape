-- Archive owner identities — stores the archive owner's own handle/name per platform
CREATE TABLE IF NOT EXISTS archive_identities (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    platform        TEXT NOT NULL,
    handle          TEXT,
    display_name    TEXT,
    profile_url     TEXT,
    import_session_id UUID REFERENCES import_sessions(id) ON DELETE SET NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_archive_identities_user ON archive_identities(user_id);
CREATE INDEX IF NOT EXISTS idx_archive_identities_handle ON archive_identities(handle);
CREATE INDEX IF NOT EXISTS idx_archive_identities_name ON archive_identities(display_name);