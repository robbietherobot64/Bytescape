-- Backfill 11.2 — local-PDS calendar records into the derived index.
-- pds_indexed_records already holds every local collection (generic arm);
-- this lifts calendar rows out of it. Local creates are also write-through +
-- local-firehose-arm indexed, so the expected delta on a current database is
-- 0 — the scan matters for records indexed before the calendar arm existed.
--
-- Re-runnable: ON CONFLICT DO NOTHING.

INSERT INTO calendar_events (
    uri, cid, author_did, name, description, starts_at, ends_at,
    mode, status, locations, uris, record_created_at, source
)
SELECT
    format('at://%s/%s/%s', r.did, r.collection, r.rkey),
    r.cid,
    r.did,
    left(r.value_json->>'name', 500),
    r.value_json->>'description',
    NULLIF(r.value_json->>'startsAt', '')::timestamptz,
    NULLIF(r.value_json->>'endsAt', '')::timestamptz,
    r.value_json->>'mode',
    COALESCE(r.value_json->>'status', 'community.lexicon.calendar.event#scheduled'),
    COALESCE(
        CASE WHEN jsonb_typeof(r.value_json->'locations') = 'array'
             THEN r.value_json->'locations' ELSE '[]'::jsonb END,
        '[]'::jsonb
    ),
    COALESCE(
        CASE WHEN jsonb_typeof(r.value_json->'uris') = 'array'
             THEN r.value_json->'uris' ELSE '[]'::jsonb END,
        '[]'::jsonb
    ),
    NULLIF(r.value_json->>'createdAt', '')::timestamptz,
    'atproto'
FROM pds_indexed_records r
WHERE r.collection IN (
        'community.lexicon.calendar.event',
        'events.smokesignal.Event',
        'events.smokesignal.calendar.event'
      )
  AND r.value_json ? 'name'
ON CONFLICT (uri) DO NOTHING;
