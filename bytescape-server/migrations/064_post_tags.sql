-- Hashtag search index. Text ILIKE scans over appview_posts recheck tens of
-- thousands of rows per query (measured: 47s for three tags against the
-- firehose table). Extracted tags turn tag search into an index lookup.
CREATE TABLE IF NOT EXISTS appview_post_tags (
    tag        text NOT NULL,
    uri        text NOT NULL,
    created_at timestamptz,
    PRIMARY KEY (tag, uri)
);

CREATE INDEX IF NOT EXISTS idx_appview_post_tags_created
    ON appview_post_tags (tag, created_at DESC);

-- Keep tags in step with the firehose ingest inside the database, so the
-- consumer needs no changes and re-ingests stay idempotent.
CREATE OR REPLACE FUNCTION appview_posts_extract_tags() RETURNS trigger AS $$
BEGIN
    IF NEW.text IS NOT NULL AND NEW.text LIKE '%#%' THEN
        INSERT INTO appview_post_tags (tag, uri, created_at)
        SELECT DISTINCT lower(m[1]), NEW.uri, NEW.created_at
        FROM regexp_matches(NEW.text, '#([A-Za-z0-9_]+)', 'g') AS m
        ON CONFLICT DO NOTHING;
    END IF;
    RETURN NEW;
END
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS appview_posts_tags ON appview_posts;
CREATE TRIGGER appview_posts_tags
    AFTER INSERT OR UPDATE OF text ON appview_posts
    FOR EACH ROW EXECUTE FUNCTION appview_posts_extract_tags();

-- Historical rows are backfilled out of band (one heavy scan):
--   INSERT INTO appview_post_tags (tag, uri, created_at)
--   SELECT DISTINCT lower(m[1]), p.uri, p.created_at
--   FROM appview_posts p, LATERAL regexp_matches(p.text, '#([A-Za-z0-9_]+)', 'g') AS m
--   WHERE p.text IS NOT NULL AND p.text LIKE '%#%'
--   ON CONFLICT DO NOTHING;
