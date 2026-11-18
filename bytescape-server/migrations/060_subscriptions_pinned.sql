-- Pinned sources: feed-locally pinned tabs project onto savedFeedsPrefV2
-- (pinned feeds visible to Bluesky and other clients).
ALTER TABLE subscriptions ADD COLUMN IF NOT EXISTS pinned BOOLEAN NOT NULL DEFAULT false;
