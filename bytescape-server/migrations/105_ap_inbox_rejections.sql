-- Owner QA follow-up (2026-11-18): inbox rejections must be diagnosable even
-- when journald rate-limits under AppView log storms (observed: ~15 signed
-- probe rejections left no journald trace during the 02:21-02:55 UTC window
-- precisely when diagnosis mattered). The journald WARN stays; this table is
-- the queryable, journal-independent record: one row per rejected inbox POST
-- (path, reason, payload size + digest — never the payload). Rejection
-- volume is low outside storms; rows are pruned to a 7-day window at write
-- time so the table stays bounded without a scheduler.
BEGIN;

CREATE TABLE IF NOT EXISTS ap_inbox_rejections (
    id bigserial PRIMARY KEY,
    path text NOT NULL,
    reason text NOT NULL,
    body_bytes integer NOT NULL,
    body_digest text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_ap_inbox_rejections_created
    ON ap_inbox_rejections (created_at);

COMMIT;
