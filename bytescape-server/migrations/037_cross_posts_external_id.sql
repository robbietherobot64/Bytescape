-- Add external_id and external_handle columns to cross_posts for dedup matching
-- external_id: raw platform post ID (Mastodon status ID, Bluesky AT-URI, rkey)
-- external_handle: author handle on the external platform (for dedup matching against timeline posts)

ALTER TABLE cross_posts ADD COLUMN IF NOT EXISTS external_id TEXT;
ALTER TABLE cross_posts ADD COLUMN IF NOT EXISTS external_handle TEXT;
