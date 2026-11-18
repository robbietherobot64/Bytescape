-- Real post time for statuses. The story rail displayed updated_at as the
-- entry age, so any firehose touch (like counts, mirrors) reset the age of a
-- status that had been posted hours earlier. created_at is set on insert and
-- preserved across same-record touches; it resets only on a genuine
-- replacement (new status record).
ALTER TABLE appview_status ADD COLUMN IF NOT EXISTS created_at timestamptz NOT NULL DEFAULT now();
UPDATE appview_status SET created_at = updated_at;
