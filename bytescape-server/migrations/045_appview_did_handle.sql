CREATE TABLE IF NOT EXISTS appview_did_handle (
    did TEXT PRIMARY KEY,
    handle TEXT NOT NULL,
    resolved_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
