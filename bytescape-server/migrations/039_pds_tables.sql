-- Sprint 1: PDS tables for AT Protocol repo storage

-- IPLD blocks (CBOR-encoded repo data, CID-addressed)
-- Each block belongs to a DID's repo and is tagged with the rev it was written at
CREATE TABLE IF NOT EXISTS pds_ipld_blocks (
    cid TEXT PRIMARY KEY,
    did TEXT NOT NULL,
    data BYTEA NOT NULL,
    rev TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_ipld_blocks_did ON pds_ipld_blocks(did);

-- Repo roots (current commit CID per user DID)
CREATE TABLE IF NOT EXISTS pds_repo_roots (
    did TEXT PRIMARY KEY,
    root_cid TEXT NOT NULL,
    rev TEXT NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- PDS accounts (DID, signing key, handle, status)
CREATE TABLE IF NOT EXISTS pds_accounts (
    did TEXT PRIMARY KEY,
    handle TEXT UNIQUE,
    signing_key_private BYTEA NOT NULL,
    signing_key_public TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);