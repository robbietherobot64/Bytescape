# ATProto events integration — investigation notes (2026-10-26)

> **Status update (2026-11-08): LANDED** — the "proper standalone build"
> exists: `openspec/changes/add-events-standalone-site`. Network events now
> flow through a dedicated relay arm (`appview/consumer.rs` → `calendar_events`)
> plus a local-PDS arm and write-through; the standalone site is live on
> `events.test.bytescape.social` (prod block ready in `deploy/Caddyfile`).
> Historical catch-up lives in `ops/backfill-calendar-{authors.py,from-appview,from-local}`.
> The pitfalls below were honored by that build and stay as the record.

Status: **reverted** (side track; the feed is back to regular Bluesky posts).
Kept here so the legitimate events build can start from knowledge, not archaeology.

## Why it was reverted

The integration worked in the end (real Smoke Signal/OpenMeet events matched
interests and appeared in the feed) but the path there accumulated scaffolding:
special-case lanes, floor rules, ingest gates and fallbacks bolted onto the
feed pipeline. Decision: do events properly as their own module later.

## What we know now (the valuable part)

### The events network

- **Smoke Signal is sunset (July 2026)** — read-only archive at
  smokesignal.events. **OpenMeet** (platform.openmeet.net) and **atmo.rsvp**
  are the active apps; both write the same shared lexicons.
- Events are **records in user PDS repos**, not platform content. NSIDs:
  `community.lexicon.calendar.event` / `.rsvp` (current), legacy
  `events.smokesignal.Event`. Whatever app writes them, they cross the
  firehose — integration should target the lexicon, never the app.
- ~257 event records and ~40k other custom-lexicon records were ingested into
  `appview_posts` during the experiment and are **still there, inert** —
  useful seed data for the real build. The `events` module's own table
  (native Bytescape events) was empty and also never fed the timeline.

### What the real build should reuse

- **Classification**: `classify_content()` in `api/feed/normalize.rs` maps a
  record's collection NSID + embed shape to a type bucket (event/market/live/
  audio/blog/list/text/other). Its non-post branch is kept in the codebase as
  the classification base; no custom records are ingested today.
- **Topic matching**: interests match events by **word stems**, not hashtags —
  via the existing `idx_appview_posts_text` tsvector GIN index
  (`to_tsvector('english', text) @@ to_tsquery('english', ...)`). "climate"
  finds "Climate book club". Never regex-scan for this.
- **Indexes**: `idx_appview_posts_generic_created` (migration 062) covers
  generic records; note its column order must match the query's
  `ORDER BY ... DESC NULLS LAST` exactly or the planner ignores it.
- Sparse content classes need **guaranteed lanes** (fetch LIMIT and feed-floor
  both) or the post/doc flood crowds them out of every page.

### Pitfalls that cost the most time (do not repeat)

1. A `batch.retain(|p| !p.matched_sources.is_empty())` in the hashtag tier
   silently discarded every word-matched record (no literal `#tag` in prose).
2. Concatenating matcher lanes let the first lane own the whole `take(limit)`.
3. Top-level OR in a SQL WHERE drops the planner's index paths — one 4.7M-row
   seq scan per request.
4. Rendering records whose authors have no handle as raw DIDs produced
   DID-only feeds ("usernames only show up as DIDs").
5. Ingesting records with no text and no media (podping pings, likes, streaks,
   sync records) = pure noise: unrenderable, unmatchable, and they wreck every
   scan. 75,779 such rows were purged.
6. **Privacy**: custom-namespace chat (`place.stream.chat.message`) slipped
   past a `chat.`-prefix exclusion; 1,726 rows were purged. Rule: chat in any
   namespace is never feed content. Also never ingest social-graph records.

## Revert scope (2026-10-26)

Removed: wide-net custom-record ingestion (consumer), the generic-record
matchers/lanes (sources), the native-events feed tier, the events floor lane,
the event chips, the DID handle fallback, the feedgen handle exception.
Kept (independent features): content-type filter + dropdown, popular-sort
selection fix, network-true counts, chronology/chip fixes, cold-window perf
work, honest type-filter empty state, NUL-byte ingest sanitize.
