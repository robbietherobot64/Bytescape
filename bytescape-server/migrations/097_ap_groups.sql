-- migration name: ap_groups (group registry + vote counts + group linkage)
-- purpose: the Lemmy/PieFed fetchers (ap/public_feed.rs) receive a full
--   `community {name, actor_id}` and `counts {upvotes, downvotes, comments}`
--   on every post_view, but only ever persisted a `kw:<name>` tag row — the
--   community identity itself was thrown away. This migration gives groups a
--   real home:
--     * ap_groups          — one row per community actor (canonical AP actor
--                            id as PK so a later AT layer can coexist).
--     * appview_posts.group_actor_id — nullable link post → group; NULL for
--                            non-forum AP content and all AT content.
--     * upvote_count / downvote_count / comment_count — the fetched vote
--                            counts, stored at ingest so the group post card
--                            can show a real upvote ratio.
--
-- Historical rows are NOT backfilled (design decision D4/D1 in
-- openspec/changes/groups-activitypub/design.md): the `kw:` community tags
-- were lowercased keyword matches with no actor id and can collide with real
-- keywords — backfilling from them would fabricate group membership. Old
-- posts simply have group_actor_id NULL and only appear in group views once
-- re-ingested by the standing poller.
--
-- Rebuildable: additive columns + IF NOT EXISTS throughout; re-running is a
-- no-op.

CREATE TABLE IF NOT EXISTS ap_groups (
    actor_id      TEXT PRIMARY KEY,          -- canonical AP actor id, e.g. https://lemmy.world/c/technology
    name          TEXT NOT NULL,             -- community.name (short name, e.g. "technology")
    title         TEXT,                      -- display title when the fetcher sends one
    description   TEXT,
    nsfw          BOOLEAN NOT NULL DEFAULT FALSE,
    instance      TEXT NOT NULL,             -- host of actor_id, e.g. lemmy.world
    kind          TEXT NOT NULL DEFAULT 'lemmy',  -- 'lemmy' | 'piefed'
    discovered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Group directory by instance; search over name/title uses the existing
-- pg_trgm extension (067) — a plain index here keeps the by-instance browse
-- cheap without a second trigram build.
CREATE INDEX IF NOT EXISTS idx_ap_groups_instance ON ap_groups (instance);
CREATE INDEX IF NOT EXISTS idx_ap_groups_name ON ap_groups (lower(name));

ALTER TABLE appview_posts ADD COLUMN IF NOT EXISTS group_actor_id TEXT;
ALTER TABLE appview_posts ADD COLUMN IF NOT EXISTS upvote_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE appview_posts ADD COLUMN IF NOT EXISTS downvote_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE appview_posts ADD COLUMN IF NOT EXISTS comment_count INTEGER NOT NULL DEFAULT 0;

-- Group feed + Subscriptions feed filter on (group_actor_id, recency).
CREATE INDEX IF NOT EXISTS idx_appview_posts_group ON appview_posts (group_actor_id, created_at DESC)
    WHERE group_actor_id IS NOT NULL;

-- Trigram index for group name/title search (ILIKE '%q%').
CREATE INDEX IF NOT EXISTS idx_ap_groups_name_trgm ON ap_groups USING gin (name gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_ap_groups_title_trgm ON ap_groups USING gin (title gin_trgm_ops);
