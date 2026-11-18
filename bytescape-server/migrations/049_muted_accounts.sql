CREATE TABLE IF NOT EXISTS muted_accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    muted_did TEXT,
    muted_handle TEXT,
    muted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    muted_type TEXT NOT NULL DEFAULT 'mute',  -- 'mute' or 'block'
    UNIQUE(user_id, muted_did)
);

CREATE INDEX IF NOT EXISTS idx_muted_accounts_user ON muted_accounts(user_id);
CREATE INDEX IF NOT EXISTS idx_muted_accounts_user_did ON muted_accounts(user_id, muted_did);
