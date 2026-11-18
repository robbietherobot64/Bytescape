-- migration name: feed_cold_path_idx (feed-cold-path-idx-103 marker)
-- purpose: server-side feed cold paths scanned the ~3.1M-row appview_posts
--   table end-to-end (EXPLAIN on the test DB: 17-39s per query, sqlx slow
--   statements at 50-70s per first load). Every index below turns one of the
--   profiled scans into a bounded index walk — see the cold-path profiling
--   notes in the feed perf change:
--
--   1. idx_appview_posts_feed_recent — the discover / interest-fallback
--      recency queries order by `created_at DESC NULLS LAST` over the
--      feed-eligible subset; the existing created_at indexes are DESC
--      (NULLS FIRST) and cannot satisfy that ordering, so Postgres chose a
--      full parallel seq scan (27s). This is the NULLS LAST sibling of
--      idx_appview_posts_created_uri, partial over the feed-eligible rows.
--   2. idx_appview_posts_popular_rank — Popular sort orders by the
--      engagement score expression, then created_at DESC NULLS LAST. No
--      index could serve that two-key order, so every popular ranking over
--      a large candidate set was a scan + top-N sort (24-39s). Keys match
--      feed::sort::SortMode::order_by() exactly; uri/labels are included so
--      the platform-bucket candidate walk is index-only.
--   3. idx_appview_posts_group_recent — the Groups view's scope query walks
--      group-linked posts newest-first; idx_appview_posts_group's leading
--      group_actor_id column cannot order an all-groups walk (and again
--      NULLS LAST), forcing a bitmap scan + sort of the whole group subset.
--   4. idx_appview_posts_at_stale — the retention prune selects stale
--      `at://` rows via `created_at < now() - '5 days'`; the created_at
--      bitmap scan read ~90k heap pages to find 81 rows (37s) because the
--      aged rows are almost all https:// archive posts. Partial over the
--      prunable cohort: the age range only visits stale firehose rows.
--   5. idx_appview_post_tags_tag_created_nullslast — the per-tag walks
--      (hashtag tiers, search multi, AP interest tier) order by
--      `created_at DESC NULLS LAST`; idx_appview_post_tags_created is DESC
--      (NULLS FIRST) so the walks degraded to bitmap scans + sorts of every
--      row for the tag (4s+ per tag under load). Same key, exact ordering.
--
-- Rebuildable / idempotent: IF NOT EXISTS, additive; safe on every boot.

CREATE INDEX IF NOT EXISTS idx_appview_posts_feed_recent
    ON appview_posts (created_at DESC NULLS LAST)
    WHERE reply_parent IS NULL AND text IS NOT NULL AND text != '';

CREATE INDEX IF NOT EXISTS idx_appview_posts_popular_rank
    ON appview_posts (
        (COALESCE(like_count, 0) * 3 + COALESCE(repost_count, 0) * 2 + COALESCE(reply_count, 0) * 1) DESC,
        created_at DESC NULLS LAST
    )
    INCLUDE (uri, labels)
    WHERE reply_parent IS NULL AND text IS NOT NULL AND text != '';

CREATE INDEX IF NOT EXISTS idx_appview_posts_group_recent
    ON appview_posts (created_at DESC NULLS LAST)
    WHERE group_actor_id IS NOT NULL
      AND reply_parent IS NULL AND text IS NOT NULL AND text != '';

CREATE INDEX IF NOT EXISTS idx_appview_posts_at_stale
    ON appview_posts (created_at)
    WHERE uri LIKE 'at://%';

CREATE INDEX IF NOT EXISTS idx_appview_post_tags_tag_created_nullslast
    ON appview_post_tags (tag, created_at DESC NULLS LAST);
