-- Migration 040: Firehose sequence + derived index + consumer offset

-- Firehose event log: every repo commit gets a sequence entry
CREATE TABLE IF NOT EXISTS pds_firehose_seq (
    seq BIGSERIAL PRIMARY KEY,
    did TEXT NOT NULL,
    commit_cid TEXT NOT NULL,
    prev_cid TEXT,
    rev TEXT NOT NULL,
    ops JSONB NOT NULL,
    blocks BYTEA NOT NULL,
    time TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_firehose_did ON pds_firehose_seq(did);
CREATE INDEX IF NOT EXISTS idx_firehose_time ON pds_firehose_seq(time);

-- Derived index: consumer writes here from firehose events
CREATE TABLE IF NOT EXISTS pds_indexed_records (
    id BIGSERIAL PRIMARY KEY,
    did TEXT NOT NULL,
    collection TEXT NOT NULL,
    rkey TEXT NOT NULL,
    cid TEXT NOT NULL,
    value_json JSONB NOT NULL,
    rev TEXT NOT NULL,
    indexed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(did, collection, rkey)
);
CREATE INDEX IF NOT EXISTS idx_indexed_did_col ON pds_indexed_records(did, collection);

-- Consumer offset: tracks last consumed seq for restart recovery
CREATE TABLE IF NOT EXISTS pds_consumer_offset (
    id INTEGER PRIMARY KEY DEFAULT 1,
    last_seq BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT single_row CHECK (id = 1)
);
INSERT INTO pds_consumer_offset (id, last_seq) VALUES (1, 0) ON CONFLICT DO NOTHING;