-- Stories: flat derived index for com.bytescape.story records and
-- app.bsky.actor.status status cards. Every sortable key is a column;
-- media/blob payloads live in JSONB that is never a sort key. Expired
-- deadline stories are transitioned to the private archive: the row stays
-- as metadata (archived = true) with archive_media pointing at the
-- app-controlled copy. appview_story_seen tracks per-viewer seen state.
CREATE TABLE IF NOT EXISTS appview_stories (
    uri           text PRIMARY KEY,
    cid           text NOT NULL,
    author_did    text NOT NULL,
    created_at    timestamptz NOT NULL,
    active_from   timestamptz,
    expires_at    timestamptz,
    pinned        boolean NOT NULL DEFAULT false,
    text          text,
    media         jsonb,
    can_react     boolean NOT NULL DEFAULT true,
    can_reply     boolean NOT NULL DEFAULT true,
    archived      boolean NOT NULL DEFAULT false,
    archive_media jsonb,
    like_count    bigint NOT NULL DEFAULT 0,
    repost_count  bigint NOT NULL DEFAULT 0,
    reply_count   bigint NOT NULL DEFAULT 0,
    updated_at    timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_appview_stories_author_created
    ON appview_stories (author_did, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_appview_stories_created
    ON appview_stories (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_appview_stories_active
    ON appview_stories (created_at DESC) WHERE NOT archived;

-- Status cards: one self-keyed app.bsky.actor.status per account. Expiry is
-- computed at ingest (createdAt + durationMinutes, default 24h). Interactions
-- target the record URI; counts reset when the self-keyed record is replaced
-- (new cid = new status instance).
CREATE TABLE IF NOT EXISTS appview_status (
    author_did   text PRIMARY KEY,
    cid          text NOT NULL DEFAULT '',
    status       text,
    music_url    text,
    expires_at   timestamptz NOT NULL,
    like_count   bigint NOT NULL DEFAULT 0,
    repost_count bigint NOT NULL DEFAULT 0,
    reply_count  bigint NOT NULL DEFAULT 0,
    updated_at   timestamptz NOT NULL DEFAULT now()
);

-- Per-viewer seen state for story groups.
CREATE TABLE IF NOT EXISTS appview_story_seen (
    viewer_did text NOT NULL,
    uri        text NOT NULL,
    seen_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (viewer_did, uri)
);

-- Status-card backgrounds: Bytescape-side presentation (profile
-- customization precedent), overwritten with each status publish.
ALTER TABLE users ADD COLUMN IF NOT EXISTS status_style JSONB;
