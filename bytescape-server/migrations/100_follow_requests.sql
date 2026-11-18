-- identity-graph-unification Phase 4: locked accounts / follow requests.
--
-- 1. users.manually_approves_followers BOOLEAN NOT NULL DEFAULT false —
--    per-user account lock. Reflected verbatim in the AP actor document as
--    `manuallyApprovesFollowers`. AT Protocol has no locked-account mechanism,
--    so on the Bluesky side the flag gates our own ingest/UI only (disclosed).
-- 2. follow_requests — a pending inbound follow awaiting owner approval.
--    fan_out (webfinger-able follower URI), user_id (target local user).
--    Decision recorded: NULL = pending, 'approved', 'denied'.
--    Unlike `followers`, a remote actor may have at most one row per target
--    (UNIQUE(fan_out, user_id)) — approve/deny always resolves the latest.

ALTER TABLE users ADD COLUMN IF NOT EXISTS manually_approves_followers BOOLEAN NOT NULL DEFAULT FALSE;

CREATE TABLE IF NOT EXISTS follow_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    fan_out TEXT NOT NULL,               -- follower actor URI (remote or local actor id)
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    inbox_url TEXT,                      -- follower's delivery inbox (AP)
    follow_activity_id TEXT,             -- AP Follow id for Accept/Reject object (NULL for AT/local)
    network TEXT NOT NULL DEFAULT 'activitypub',  -- activitypub | atproto | local
    decision TEXT,                       -- NULL pending | approved | denied
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ,
    UNIQUE(fan_out, user_id)
);
CREATE INDEX IF NOT EXISTS idx_follow_requests_user ON follow_requests(user_id) WHERE decision IS NULL;
