-- migration name: appview_reply_parent_idx (comment thread lookups)
-- purpose: GET /api/feed/comments filters `appview_posts WHERE reply_parent = $1`
--   to build a post's comment thread. Without an index on `reply_parent` that
--   is a full scan over the ~200k-row table — the owner reported "comments take
--   long to load" (item 2). This partial index (comment rows only) makes the
--   thread lookup an index walk.
--
-- Rebuildable / idempotent: IF NOT EXISTS, additive; safe on every boot.

CREATE INDEX IF NOT EXISTS idx_appview_posts_reply_parent
    ON appview_posts (reply_parent, created_at)
    WHERE reply_parent IS NOT NULL;
