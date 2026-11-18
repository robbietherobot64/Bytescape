-- Minimal blob store for the self-hosted PDS. Story records reference media
-- as real AT Protocol blobs (records follow the DID across PDSes — that
-- portability requires actual blobs, not app-hosted URLs).
CREATE TABLE IF NOT EXISTS pds_blobs (
    did        text NOT NULL,
    cid        text NOT NULL,
    mime_type  text NOT NULL,
    size       bigint NOT NULL,
    bytes      bytea NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (did, cid)
);

-- Archive-transition retry accounting: failures log + retry with a cap,
-- after which the row is reduced to metadata and logged as stuck.
ALTER TABLE appview_stories ADD COLUMN IF NOT EXISTS transition_attempts int NOT NULL DEFAULT 0;
