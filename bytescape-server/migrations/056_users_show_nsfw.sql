-- users.show_nsfw: NSFW content preference (feed spec OQ-8 amended 2026-10-11).
-- Default false: NSFW-marked posts are hidden from feeds entirely. When true,
-- they are shown (frontend keeps blur + click-to-reveal).
ALTER TABLE users ADD COLUMN IF NOT EXISTS show_nsfw BOOLEAN NOT NULL DEFAULT false;
