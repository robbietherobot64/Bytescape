-- Migration 093: firehose commit-chain metadata for relay-valid #commit frames.
--
-- The relay requires every #commit frame after the first to carry `prevData`
-- (the previous commit's `data` CID) and `since` (the previous rev) — indigo's
-- verify.go enforces `prevRepo != nil → prevData MUST be present`. Both are
-- derived at emit time from the previous commit block and persisted here so a
-- replayed event can carry them without re-deriving from block history.
-- DDL-only, idempotent (re-run every boot like every migration).

ALTER TABLE pds_firehose_seq ADD COLUMN IF NOT EXISTS prev_data text;
ALTER TABLE pds_firehose_seq ADD COLUMN IF NOT EXISTS since text;
