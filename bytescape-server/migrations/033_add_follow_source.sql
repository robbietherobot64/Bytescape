-- Add source column to follows table to track import-matched vs organic follows
ALTER TABLE follows ADD COLUMN IF NOT EXISTS source TEXT NOT NULL DEFAULT 'manual';
ALTER TABLE follows ADD COLUMN IF NOT EXISTS source_platform TEXT;
