-- Add category column to imported_contacts for social graph restoration
-- Categories: 'matched', 'rss_candidate', 'unreachable' (default)

ALTER TABLE imported_contacts ADD COLUMN IF NOT EXISTS category TEXT DEFAULT 'unreachable';
CREATE INDEX IF NOT EXISTS idx_imported_contacts_category ON imported_contacts(category);
