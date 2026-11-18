-- identity-graph-unification Phase 5: notification dedupe + provenance.
--
-- 1. dedupe_key TEXT — the origin event id (AP/AT activity id, record uri, or
--    local event key). NULL for legacy import rows; unique per (user_id,
--    dedupe_key) so a redelivered federated interaction creates exactly one
--    notification row.
-- 2. actor_id TEXT — the acting actor (remote AP actor uri, AT DID, or local
--    username) so each row carries its actor + network provenance.
-- 3. network TEXT — 'activitypub' | 'atproto' | 'local'.

ALTER TABLE notifications ADD COLUMN IF NOT EXISTS dedupe_key TEXT;
ALTER TABLE notifications ADD COLUMN IF NOT EXISTS actor_id TEXT;
ALTER TABLE notifications ADD COLUMN IF NOT EXISTS network TEXT NOT NULL DEFAULT 'local';

CREATE UNIQUE INDEX IF NOT EXISTS idx_notifications_dedupe
    ON notifications(user_id, dedupe_key) WHERE dedupe_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_notifications_type ON notifications(user_id, type);
