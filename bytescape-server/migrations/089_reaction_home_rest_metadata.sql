-- add-identity-realignment task 2.4: reaction metadata identity migration.
--
-- Reactions created under the old minted-actor identity ('like_identity' =
-- 'ap') by Mastodon-signed-up users move to the home-REST identity so unreact
-- dispatches the way the react was created (Decision 1: one account, one
-- person). Only rows whose user has a provider = 'mastodon' linked account
-- migrate; the old minted-actor Like itself is covered by the retirement
-- retraction (Decision 2). Bluesky/email signups keep their minted-actor
-- reactions and are not touched here.
--
-- home_instance comes from the link's metadata.instance; home_status_id is
-- the numeric id parsed from the canonical status URI, NULL when the URI
-- carries none (the runtime resolver looks those up). Other metadata keys
-- (e.g. bluesky_like_rkey) are preserved by the jsonb || merge.
--
-- Idempotent: the boot runner re-runs EVERY migration at each start, so this
-- is naturally idempotent — the like_identity = 'ap' predicate stops matching
-- as soon as a row has been migrated (and never matched already-migrated
-- 'home-rest' rows to begin with).
UPDATE reactions r
SET metadata = r.metadata || jsonb_build_object(
        'like_identity', 'home-rest',
        'home_instance', la.metadata->>'instance',
        'home_status_id', substring(r.post_id from '/([0-9]+)/?$')
    )
FROM users u
JOIN linked_accounts la ON la.user_id = u.id AND la.provider = 'mastodon'
WHERE r.user_id = u.id
  AND r.metadata->>'like_identity' = 'ap';
