-- identity-graph-unification Phase 1: follow-graph visibility.
--
-- users.graph_visibility ('public'|'private', default 'public') drives how
-- the AP actor's follow-graph collections are served:
--   * public  — /{actor}/followers and /{actor}/following serve paginated
--     OrderedCollections (first/last entry points, totalItems, page items).
--   * private — the collections stay dereferenceable but render only
--     {id, type}: no first, no items, no totalItems. Mastodon renders its
--     native hidden-collection state when `first` is absent
--     (mastodon/mastodon#35311) and no counts leak.
-- Backend enforcement only in this phase (the settings toggle and Mastodon
-- hide_collections parity land in later phases).
--
-- Idempotent like every file here: the runner has no journal and re-runs the
-- whole set on every boot under the migrations lock.
BEGIN;

ALTER TABLE users ADD COLUMN IF NOT EXISTS graph_visibility TEXT NOT NULL DEFAULT 'public';

ALTER TABLE users DROP CONSTRAINT IF EXISTS users_graph_visibility_check;
ALTER TABLE users ADD CONSTRAINT users_graph_visibility_check
    CHECK (graph_visibility IN ('public', 'private'));

COMMIT;
