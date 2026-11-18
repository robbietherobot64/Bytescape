-- Feed sort modes had no covering indexes: the 'new' sort orders by
-- indexed_at (unindexed — full sort of the 4.1M-row appview_posts per
-- query, 30s+ under the cache warmers' concurrent passes) and 'popular'
-- orders by an engagement expression. Both now walk an index and stop at
-- LIMIT. The 067 comment's tradeoff applies: plain CREATE, IF NOT EXISTS,
-- one write-blocking build, once.
CREATE INDEX IF NOT EXISTS idx_appview_posts_indexed_at
    ON appview_posts (indexed_at DESC NULLS LAST);

CREATE INDEX IF NOT EXISTS idx_appview_posts_popular_score
    ON appview_posts ((COALESCE(like_count, 0) * 3 + COALESCE(repost_count, 0) * 2 + COALESCE(reply_count, 0)) DESC, created_at DESC NULLS LAST);
