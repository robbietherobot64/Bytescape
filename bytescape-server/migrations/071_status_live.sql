-- Live-status flag for app.bsky.actor.status records. Bluesky's "go live"
-- statuses carry the sentinel text "app.bsky.actor.status#live"; the human
-- text lives in the embed title. is_live marks those records so the UI can
-- label them honestly instead of inferring from text.
ALTER TABLE appview_status ADD COLUMN IF NOT EXISTS is_live boolean NOT NULL DEFAULT false;

-- Backfill existing sentinel rows.
UPDATE appview_status SET is_live = true WHERE status LIKE 'app.bsky.actor.status#%';
