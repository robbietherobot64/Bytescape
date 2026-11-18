# Events — the standalone site

Events is Bytescape's first **standalone section site**: its own deployable
app, its own URL, same account and same data as the main app. It exists
because events are a content network of their own — and because every future
section (Classifieds, Groups) will follow the same pattern.

## Where it lives

- **Test host (live now):** `events.test.bytescape.social`
- **Production:** `events.bytescape.social` — deployed with the next
  production release (track it in [STATUS.md](../STATUS.md))
- **In the main app:** the Events tab shows the same events — one shared UI
  package (`packages/events-ui`) mounted by both frontends.

Sign-in is a one-click hop from your main Bytescape session; no separate
account.

## What you can do

- **Browse** upcoming events (chronological, cursor-paginated, location and
  time filters). Your feeds only surface events matched by the people, feeds,
  and hashtags you follow — nothing else
- **RSVP** — going or maybe, one status per person, changeable; counts show
  how many are going/maybe
- **Discuss** — comments and emoji reactions per event
- **Create** events (anyone signed in), **edit and delete** your own (delete
  removes it everywhere: index, comments, reactions)
- **Deep-link** any event — URLs are shareable and reopen the same event
- **Feed-style source tabs** — Home (everything from your sources), Following
  (events people you follow have RSVP'd to), Feeds (events matching the topics
  of feeds you follow), and Hashtags (events carrying a hashtag you follow —
  the exact tag or the bare word, never sibling tags: `#astrophotography` will
  not pull in `#astronomy` events). The right rail is
  the same **Your sources** panel as the main feed: click a source to open its
  tab scoped to that person, feed, or hashtag; close the tab (or select Home)
  to clear it.
- **Provenance chips** — every event says why it is there: Following, the feed
  that matched, or the hashtag that matched. No fallback: if none of your
  sources match an event, it doesn't show up in your feeds at all.

## Why the records matter

Events aren't rows locked in one app's database. Creating an event writes an
AT Protocol record — `community.lexicon.calendar.event` — to *your* PDS
repository, and an RSVP writes `community.lexicon.calendar.rsvp`. That means:

- **Interop:** events made on other AT Protocol event apps (atmo.rsvp,
  OpenMeet, the late Smoke Signal's archive) show up here, and events created
  here are readable there. The site is seeded with hundreds of real network
  events (calendar.city, atmosphere.community, …) via a bounded backfill plus
  a live relay arm.
- **Portability:** your events live in your repository, not in Bytescape's
  database. The database is a derived index that can be rebuilt.

## The side-module pattern

The Events site proves the shape of Bytescape's standalone sections:

- **Own deployable** — built and shipped independently (`apps/events`,
  `npm run deploy:*`), can be up while the main app deploys, and vice versa.
- **Shared code, not forks** — the component tree lives in
  `packages/events-ui`; both hosts mount it, so the sections never drift.
- **Protocol-native data** — section content is written as network records
  first, indexed second (see [Architecture](../architecture/README.md)).

Classifieds and Groups are planned to ride the same pattern when their ship
gates open ([MVP scope](../product/mvp-scope.md)).
