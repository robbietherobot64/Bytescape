CREATE TABLE IF NOT EXISTS posting_identities (
    user_id           UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    protocol          TEXT NOT NULL CHECK (protocol IN ('atproto', 'activitypub')),
    identity_type     TEXT NOT NULL DEFAULT 'bytescape' CHECK (identity_type IN ('bytescape', 'linked')),
    linked_account_id UUID REFERENCES linked_accounts(id) ON DELETE RESTRICT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, protocol),
    CONSTRAINT linked_identity_requires_account CHECK (
        (identity_type = 'bytescape' AND linked_account_id IS NULL) OR
        (identity_type = 'linked' AND linked_account_id IS NOT NULL)
    )
);
