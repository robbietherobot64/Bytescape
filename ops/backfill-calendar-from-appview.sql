-- Backfill 11.1 — one-time re-key of event-classified rows already in
-- appview_posts into the derived calendar index (spec: events/ingestion,
-- "pre-existing records are usable at launch").
--
-- Re-runnable: ON CONFLICT DO NOTHING. appview_posts has no type column —
-- classification is by the record's collection in its at-URI.
--
-- History: the reverted 2026-10 events experiment ingested ~257 such rows;
-- they were purged when the experiment was reverted (docs/events-integration-
-- notes.md). Expected effect on a database that went through that purge:
-- 0 rows re-keyed — launch content then comes from this backfill's sibling
-- scripts (local records, author allowlist) plus the live firehose arm.

INSERT INTO calendar_events (
    uri, cid, author_did, author_handle, name, description,
    starts_at, ends_at, locations, uris, record_created_at, source
)
SELECT
    p.uri,
    p.cid,
    p.author_did,
    p.author_handle,
    left(p.text, 500),
    p.text,
    NULL,                 -- appview_posts does not carry record-level times
    NULL,
    '[]'::jsonb,
    '[]'::jsonb,
    p.created_at,
    'atproto'
FROM appview_posts p
WHERE (p.uri LIKE '%/community.lexicon.calendar.event/%'
       OR p.uri LIKE '%/events.smokesignal.Event/%'
       OR p.uri LIKE '%/events.smokesignal.calendar.event/%')
  AND p.text IS NOT NULL AND p.text <> ''
ON CONFLICT (uri) DO NOTHING;
