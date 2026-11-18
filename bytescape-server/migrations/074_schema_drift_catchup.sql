-- Schema drift catch-up (2026-11-05). These columns existed on the
-- production database but were never captured in a migration — they were
-- applied manually at some point. Found by diffing a fresh boot
-- (test.bytescape.social staging) against production. Without them, any
-- fresh database fails at signup and profile reads.
ALTER TABLE users ADD COLUMN IF NOT EXISTS visitor_count BIGINT NOT NULL DEFAULT 0;
ALTER TABLE appview_posts ADD COLUMN IF NOT EXISTS embeds JSONB;
