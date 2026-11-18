-- Query plans over appview_posts order by `created_at DESC NULLS LAST`; an
-- index declared plain `DESC` means NULLS FIRST in Postgres, and the order
-- mismatch keeps the planner in scan+sort mode (measured: 29-33s statements;
-- feeds are early-stop walks only when index order matches ORDER BY).
-- Create-only: the wrong-order indexes on the production box were replaced
-- out of band; migrations must never DROP/CREATE multi-million-row indexes
-- at boot.
CREATE INDEX IF NOT EXISTS idx_appview_posts_created_uri
    ON appview_posts (created_at DESC NULLS LAST, uri DESC NULLS LAST)
    WHERE (reply_parent IS NULL AND text IS NOT NULL AND text <> '');

CREATE INDEX IF NOT EXISTS idx_appview_posts_created
    ON appview_posts (created_at DESC NULLS LAST);

CREATE INDEX IF NOT EXISTS idx_appview_posts_author_did_created
    ON appview_posts (author_did, created_at DESC NULLS LAST)
    WHERE (reply_parent IS NULL AND text IS NOT NULL AND text <> '');

-- Resolver lookup (handle-missing posts) must be an index, never a scan.
CREATE INDEX IF NOT EXISTS idx_appview_posts_unresolved
    ON appview_posts (author_did)
    WHERE (author_handle IS NULL AND author_did IS NOT NULL);
