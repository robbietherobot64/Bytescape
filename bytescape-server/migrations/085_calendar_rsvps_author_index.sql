-- migration name: calendar_rsvps_author_index
-- purpose: the Events "Following" tab resolves events RSVP'd by the viewer's
--   follow graph. The existing (event_uri, author_did, record_created_at)
--   index serves per-event lookups; this index serves the author-side filter
--   (`author_did = ANY($follows)`) without a sequential scan of all RSVPs.

CREATE INDEX IF NOT EXISTS idx_calendar_rsvps_author
    ON calendar_rsvps (author_did);
