-- Regex topic/keyword searches (text ~* ...) were full-window scans over
-- millions of rows (measured: 22s for one topic feed). A trigram GIN index
-- makes every text ~* / ILIKE query index-backed and lets the created_at
-- ORDER BY terminate at LIMIT in milliseconds. Same for the author-handle
-- ILIKE search. CONCURRENTLY: this table is hot (firehose inserts) and
-- 10M+ rows — a plain CREATE would block writes for minutes.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_appview_posts_text_trgm
    ON appview_posts USING gin (text gin_trgm_ops);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_appview_posts_handle_trgm
    ON appview_posts USING gin (author_handle gin_trgm_ops);
