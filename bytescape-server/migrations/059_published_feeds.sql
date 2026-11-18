-- Real record URIs of published topic feed generators, captured at publish
-- time. Generator records live in the publisher's repo
-- (at://{publisher-did}/app.bsky.feed.generator/{rkey}) while the feed service
-- (did:web:feeds.bytescape.social) serves getFeed — so the resolvable at-URI
-- is a fact to record, not to reconstruct. Backfilled by idempotent re-publish.
CREATE TABLE IF NOT EXISTS published_feeds (
    rkey TEXT PRIMARY KEY,
    at_uri TEXT NOT NULL,
    published_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
