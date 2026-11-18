-- migration name: calendar_topics (interest matching for the events surface)
-- purpose: community.lexicon.calendar.event records carry no tags field, so
--   the derived index computes topic rkeys from name/description/location at
--   index time (the feed's topic vocabulary is the single source). This lets
--   "your interests" filter the events surface — a Ska subscriber sees ska
--   events (spec: events/browse + feed interests translation).
--
-- Rebuildable: topics is derived data; a missing value is repaired by
--   calendar::backfill_topics at startup, never hand-edited.

ALTER TABLE calendar_events
    ADD COLUMN IF NOT EXISTS topics TEXT[] NOT NULL DEFAULT '{}';

CREATE INDEX IF NOT EXISTS idx_calendar_events_topics
    ON calendar_events USING GIN (topics);
