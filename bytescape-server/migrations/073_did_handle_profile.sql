-- Cached profile fields for story rail authors. Status-only authors had no
-- ingested posts to read avatars from, so live-status rings rendered without
-- an avatar. author_info now falls back to these columns and backfills them
-- from Bluesky's public profile API.
ALTER TABLE appview_did_handle ADD COLUMN IF NOT EXISTS display_name text;
ALTER TABLE appview_did_handle ADD COLUMN IF NOT EXISTS avatar text;
