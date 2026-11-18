-- Author profile display data for indexed posts: avatars and display names
-- were never stored, so DB-sourced feed posts rendered without profile
-- pictures (API-sourced posts had them via normalize_bluesky_api_post).
ALTER TABLE appview_posts
  ADD COLUMN IF NOT EXISTS author_avatar TEXT,
  ADD COLUMN IF NOT EXISTS author_display_name TEXT;
