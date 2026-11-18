-- migration name: appview_posts_ap_index (fast fediverse-content lookup)
-- purpose: the home feed surfaces recent ActivityPub rows (`https://…` URIs,
--   ingested by the standing puller from Mastodon/Pixelfed/Lemmy/etc.) as a
--   labeled "Fediverse" tier. Filtering `uri LIKE 'https://%'` over the ~1.7M
--   firehose rows would be a full scan; this partial index covers only the
--   AP rows (a small, growing set) so the query stays instant.
--
-- Rebuildable: derived from the row contents; drop + recreate any time.

CREATE INDEX IF NOT EXISTS idx_appview_posts_ap_recent
    ON appview_posts (created_at DESC)
    WHERE uri LIKE 'https://%';
