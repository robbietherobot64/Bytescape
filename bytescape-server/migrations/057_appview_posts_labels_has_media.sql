-- appview_posts completeness (2026-10-11): the firehose consumer INSERTs
-- labels + has_media, but the table (migration-044-era shape) lacked them —
-- every upsert failed with "column ... does not exist", leaving the appview
-- empty and NSFW label filters dead. aligns the table with consumer.rs.
ALTER TABLE appview_posts ADD COLUMN IF NOT EXISTS labels JSONB;
ALTER TABLE appview_posts ADD COLUMN IF NOT EXISTS has_media BOOLEAN DEFAULT false;
