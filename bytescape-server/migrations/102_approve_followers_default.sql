-- Owner QA 2026-11-17: approve-only is the out-of-the-box behavior on the
-- fediverse (Mastodon follows require approval; the user may opt into
-- auto-approve in Settings).
--
-- Distinction, documented deliberately:
--   * DEFAULT true — accounts created without an explicit choice are locked:
--     inbound follows are held for approval out of the box.
--   * One-time backfill below — every pre-existing account is flipped to
--     locked as well: all current values are the never-chosen migration-100
--     default (false), and the owner explicitly asked for approve-only out of
--     the box. Nothing is force-flipped after the backfill's single run.
--   * From then on the stored column value is the user's explicit choice
--     (Settings → "Approve followers automatically" / "Review follow
--     requests") and always wins.
-- Bluesky/AT keeps its disclosed UI-only gating (not network-enforceable
-- there); the AP actor advertises manuallyApprovesFollowers and enforces it.
--
-- This migration file is executed on EVERY boot (the runner has no journal),
-- so the backfill carries its own one-time guard: the UPDATE runs only while
-- its bookkeeping row is absent, and the row is written in the same
-- transaction — exactly-once across restarts, and an explicit
-- manually_approves_followers = false is never re-flipped afterwards.
BEGIN;

ALTER TABLE users ALTER COLUMN manually_approves_followers SET DEFAULT true;

CREATE TABLE IF NOT EXISTS app_one_time_backfills (
    name text PRIMARY KEY,
    applied_at timestamptz NOT NULL DEFAULT now()
);

UPDATE users SET manually_approves_followers = true
WHERE NOT EXISTS (
    SELECT 1 FROM app_one_time_backfills WHERE name = '102_approve_followers_backfill'
);

INSERT INTO app_one_time_backfills (name) VALUES ('102_approve_followers_backfill')
ON CONFLICT (name) DO NOTHING;

COMMIT;
