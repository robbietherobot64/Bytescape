-- home-REST 404 fix follow-up (2026-11): 089 wrote the post_id's numeric tail
-- into home_status_id — for foreign-host posts that tail is the REMOTE
-- server's status id, not the home instance's local one, so an unreact would
-- target a wrong (or nonexistent) home status. NULL the slot for rows whose
-- post_id host differs from the link's metadata.instance: home_rest_unreact
-- then falls back to the record-id slot (correct home ids for runtime-created
-- rows) or treats the row as never-resolved, and any re-resolve path starts
-- clean. Rows with a matching host keep their ids (same-host tails are the
-- home instance's own ids). 0 affected rows on bytescape_test at deploy time.
--
-- Idempotent: the boot runner re-runs every migration at each start; NULLing
-- an already-NULL slot is a no-op. The equality predicate targets EXACTLY
-- 089's writes (it stored substring(post_id tail)): runtime-resolved rows hold
-- the home-LOCAL search id, which differs from a foreign URI's tail, so their
-- correct ids are preserved.
UPDATE reactions r
SET metadata = r.metadata || jsonb_build_object('home_status_id', NULL)
FROM linked_accounts la
WHERE la.user_id = r.user_id
  AND la.provider = 'mastodon'
  AND r.metadata->>'like_identity' = 'home-rest'
  AND r.metadata->>'home_status_id' IS NOT NULL
  AND r.metadata->>'home_status_id' = substring(r.post_id from '/([0-9]+)/?$')
  AND split_part(substring(r.post_id from '^https?://([^/]+)'), ':', 1)
      <> split_part(coalesce(la.metadata->>'instance', ''), ':', 1);
