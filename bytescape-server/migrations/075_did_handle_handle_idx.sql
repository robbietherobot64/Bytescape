-- The circle resolution (resolve_followed_dids) looks up
-- appview_did_handle by handle on every stories/feed request; with only the
-- did primary key each lookup was a full scan of the 700k-row table and the
-- story rail took 4-11s. The handle index turns it into a point lookup.
CREATE INDEX IF NOT EXISTS idx_appview_did_handle_handle
    ON appview_did_handle (handle);
