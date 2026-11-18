-- bytescape.social — Social Graph: imported contacts from archives
CREATE TABLE IF NOT EXISTS imported_contacts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    contact_name    TEXT NOT NULL,
    contact_handle  TEXT,
    contact_platform_url TEXT,
    source          TEXT NOT NULL,
    import_session_id UUID REFERENCES import_sessions(id) ON DELETE SET NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_imported_contacts_user_id ON imported_contacts(user_id);
CREATE INDEX IF NOT EXISTS idx_imported_contacts_name ON imported_contacts(contact_name);
CREATE INDEX IF NOT EXISTS idx_imported_contacts_handle ON imported_contacts(contact_handle);
