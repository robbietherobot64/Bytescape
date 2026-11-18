-- Topic feeds matched posts with a text regex over a time window (12-60s:
-- the candidate set is every matching row in history). Extracted per-key
-- rows — the same architecture as hashtag search — make topic feeds an
-- index lookup. The trigger additionally extracts topic-keyword matches
-- as 'kw:<term>' rows; the vocabulary lives in Rust (feedgen TOPICS) and
-- is synced into these tables at server start.
CREATE TABLE IF NOT EXISTS appview_topic_terms (
    term text PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS appview_feed_settings (
    key   text PRIMARY KEY,
    value text NOT NULL
);

CREATE OR REPLACE FUNCTION appview_posts_extract_tags() RETURNS trigger AS $$
DECLARE kw_re text;
BEGIN
    IF NEW.text IS NOT NULL AND NEW.text LIKE '%#%' THEN
        INSERT INTO appview_post_tags (tag, uri, created_at)
        SELECT DISTINCT lower(m[1]), NEW.uri, NEW.created_at
        FROM regexp_matches(NEW.text, '#([A-Za-z0-9_]+)', 'g') AS m
        ON CONFLICT DO NOTHING;
    END IF;
    -- Topic-keyword matches (tag = 'kw:<term>') for the generated feeds.
    IF NEW.text IS NOT NULL THEN
        SELECT value INTO kw_re FROM appview_feed_settings WHERE key = 'kw_regex';
        IF kw_re IS NOT NULL THEN
            INSERT INTO appview_post_tags (tag, uri, created_at)
            SELECT DISTINCT 'kw:' || lower(m[1]), NEW.uri, NEW.created_at
            FROM regexp_matches(NEW.text, kw_re, 'gi') AS m
            ON CONFLICT DO NOTHING;
        END IF;
    END IF;
    RETURN NEW;
END
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS appview_posts_tags ON appview_posts;
CREATE TRIGGER appview_posts_tags
    AFTER INSERT OR UPDATE OF text ON appview_posts
    FOR EACH ROW EXECUTE FUNCTION appview_posts_extract_tags();

-- Historical keyword rows are backfilled out of band (one heavy scan):
--   INSERT INTO appview_post_tags (tag, uri, created_at)
--   SELECT DISTINCT 'kw:' || lower(m[1]), p.uri, p.created_at
--   FROM appview_posts p,
--        LATERAL regexp_matches(p.text, (SELECT value FROM appview_feed_settings
--                                        WHERE key = 'kw_regex'), 'gi') AS m
--   WHERE p.text IS NOT NULL AND p.created_at > now() - interval '7 days'
--   ON CONFLICT DO NOTHING;
