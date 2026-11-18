-- migration name: ap_source_kinds (PieFed + Mobilizon registry kinds)
-- purpose: the registry's inferred `kind` had no value for PieFed or Mobilizon,
--   so both were polled with the wrong fetcher and could never yield:
--     * PieFed hosts were discovered as `lemmy` (their actor paths are /u/…)
--       and polled at /api/v3/post/list, which PieFed does not serve (404).
--     * Mobilizon hosts were discovered as `generic` and polled at
--       /api/v1/timelines/public, which is a GraphQL server (400).
--   Both kinds now have a real fetcher in ap/public_feed.rs. This migration
--   reclassifies the rows already in the registry and resets their fetch
--   state so they re-enter the queue immediately (fetched_at = NULL sorts
--   first in the probe lane) instead of waiting out the LRU order.
--
-- Rebuildable: ap_sources is coordination state; re-running is a no-op when
--   no row matches (idempotent by WHERE clause, safe on every boot).

UPDATE ap_sources
   SET kind = 'piefed',
       failures = 0,
       enabled = TRUE,
       fetched_at = NULL,
       updated_at = now()
 WHERE kind IN ('lemmy', 'generic')
   AND host LIKE '%piefed%';

UPDATE ap_sources
   SET kind = 'mobilizon',
       failures = 0,
       enabled = TRUE,
       fetched_at = NULL,
       updated_at = now()
 WHERE kind IN ('generic', 'mastodon')
   AND host LIKE '%mobilizon%';
