-- Mastodon import (openspec: import-pipeline-verification)
-- * remote_handle: canonical @user@instance for ActivityPub contacts (R1/R2).
-- * direction: whether the archive owner followed or was followed by them (R1).
-- * import_sessions.metadata: skipped-entry errors recorded on a completed
--   session (R4/R5) — google_import already INSERTs this column, so this also
--   captures it for fresh databases.
ALTER TABLE imported_contacts ADD COLUMN IF NOT EXISTS remote_handle TEXT;
ALTER TABLE imported_contacts ADD COLUMN IF NOT EXISTS direction TEXT;
ALTER TABLE import_sessions ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}';
