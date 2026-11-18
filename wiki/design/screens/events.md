# Screen Spec: Events (v2)

> **Implementation note (2026-11-09) — shipped layout supersedes §3.1/§4.2:**
> the events surface now mirrors the main feed instead of the filter-bar
> layout: a 600px reading column plus a 300px rail, centered. Status tabs,
> search, a collapsed **Filters** disclosure, and **+ New event** live in one
> toolbar row; the permanent left filter rail is gone. Card grid/list toggle
> (§§3.1–3.2) is not implemented; the single-column card list is the shipped
> view. *(The 2026-11-12 note below supersedes the rail layout: the rail is
> the feed's sources panel, and the "Your interests" panel is gone.)*

> **Implementation note (2026-11-12) — the events shell is the main feed:**
> the events surface renders the feed's exact tab strip (`feed-tabstrip` /
> `feed-tab`), action bar (`feed-actionbar`, `feed-sort-select`, accent
> create button), and right column (`sources-column` → `SourceSearch` +
> `SourcesPanel`) with the same "Your sources" markup, counts, search, delete
> flow, and populated Following / Feeds / Hashtags. Selecting a source opens
> a source tab exactly like the feed (close × removes it) and scopes the
> listing; closing it or selecting Home clears the scope. The right rail keeps
> **Your events** below the sources panel. The status/search/Filters/+New
> event bar sits directly below the tabs. `GET /api/events/sources` supplies
> the panel; `?source=` plus `followed_did` / `feed_id` / `hashtag_id` scopes
> the list.

> **Implementation note (2026-11-12) — one topic rule on every surface:**
> topic derivation follows the main feed's canonical rule (`combined_regex`,
> migrations 064/068): vocabulary **tags** match only as `#tag` tokens,
> **keywords** as whole words. Bare-word tag matching was the Climate-feed
> bleed ("creative environment" claimed `climate`, "art therapy" claimed
> `mental-health`, "Garden Layout" — a place name — claimed `gardening`).
> Recall lives in `keywords` (e.g. `skateboard` was added so "Skateboard
> meetup" derives skateboarding). Over-broad bare words get dropped the way
> `board` was: `space` is absent as a bare keyword ("adorn your space",
> venue "Evvah space"), keeping `#space` + phrases like "outer space".
> The main feed's `kw:` keyword rows were dead on arrival — the trigger
> regex captured the boundary instead of the term (709K garbage rows);
> `kw_regex` now captures the term in group 1 and `sync_topic_terms`
> re-derives the 7-day read window out of band (marker-guarded).

> **Implementation note (2026-11-12) — exact hashtag matching + card provenance:**
> the Hashtags tab matches the subscribed tag as an exact token — `#tag` or
> the bare word `tag`, boundary-checked (`crate::tags`, shared with the main
> feed's chip matchers so both surfaces filter identically). A tag NEVER
> expands to its topic family: the earlier family fallback bled
> `#astrophotography` into `#astronomy` and `#atproto` into `#activitypub`
> because both sibling tags share one derived topic rkey.
> The Feeds mapping no longer falls back to broad label tokens once a specific
> feed topic matched ("PC Gaming" no longer claims every `gaming` event).
> Listings are source-only: only events matched by a followed RSVP, a feed
> topic family, or a hashtag family ever appear (Home = the union); unmatched
> events stay reachable only by deep link. Cards carry source-provenance
> chips: one per kind — Following, a feed label, a hashtag label — never a
> fallback. Matching runs once per list against the viewer's follows and
> subscriptions; no per-card queries.

> **Implementation note (2026-11-12) — cards are post-shaped:** the listing
> card reads like a feed post: title/date/location/author, the description
> clamped to four lines with **Read more**, then the media slot — the event
> image (16:9 cover) when `uris` carries one, otherwise an OpenGraph link
> preview of the event's external link (domain-only fallback when the metadata
> endpoint refuses). No interest chips; the source-provenance chips above name the match.

**Status:** Draft
**Date:** 2026-08-12
**Route:** `/events`
**Satisfies:** FR-17 (Events) — primary surface; contributes to FR-15 (Reactions and Engagement — emoji reactions on events), FR-25 (Notifications — event-related notifications), FR-28 (AppView — event indexing surface). Event records also surface on FR-11 (Profile Feed) when created by followed accounts.

**Sources:**
- v1 implementation: none (v1 had no events feature)
- v2 constraints: `wiki/product/prd.md` (FR-17), `wiki/architecture/database.md` (`events`, `event_rsvps` tables), `wiki/architecture/decisions.md` (AD-00 AT Protocol only, AD-01 PDS-first writes, AD-01b dual PDS, AD-01c lexicons — `community.lexicon.calendar.event` + `community.lexicon.calendar.rsvp`)
- Interop references: [atmo.rsvp](https://github.com/flo-bit/atmo-events), [Smoke Signal](https://smokesignal.events)

> **No v1 salvage.** Bytescape v1 had no events feature. This is a greenfield screen. Design is informed by the atmo.rsvp reference implementation and Smoke Signal's event-indexing model — both use the same `community.lexicon.calendar.event` and `community.lexicon.calendar.rsvp` lexicons that Bytescape adopts (AD-01c).

> **Lexicon interop.** Events written by Bytescape users are AT Protocol records on their PDS, indexed by the Bytescape AppView, and discoverable by any app that understands `community.lexicon.calendar.*` — atmo.rsvp, Smoke Signal, and future AT Protocol event apps. Conversely, Bytescape indexes events from the broader AT Protocol network (Smoke Signal currently indexes 14,120+ events). The screen is a window into the whole ecosystem, not just Bytescape-originated events.

---

## 1. Overview

The Events screen is Bytescape's event discovery, creation, and interaction surface. Users browse upcoming events from across the AT Protocol network, create their own events, RSVP, and engage with comments and emoji reactions. Events are `community.lexicon.calendar.event` records stored on the author's PDS (AD-01); RSVPs are `community.lexicon.calendar.rsvp` records on the attendee's PDS — both federate via the firehose and are interoperable with atmo.rsvp and Smoke Signal.

The screen serves three audiences:

| Viewer | Sees | Can do |
|---|---|---|
| Authenticated user | Full event listing + create affordance | Create events, RSVP, comment, react |
| Authenticated visitor (viewing specific event) | Event detail with RSVP status | RSVP, comment, react, share |
| Unauthenticated visitor | Public event listing | View events; create/RSVP prompts route to login |

### 1.1 Dual PDS rendering (AD-01b)

| | Bytescape-hosted (`did:web`) | Bluesky-hosted (`did:plc`) |
|---|---|---|
| Create event | Signed commit to Bytescape PDS + direct index insert (read-after-write) | XRPC write to bsky.social with OAuth token; indexed on relay arrival |
| RSVP | Signed commit to Bytescape PDS | XRPC write to bsky.social |
| Read events | Identical — AppView queries only | Identical |
| Comments / reactions | Standard `app.bsky.feed.post` as replies to event URI, or `app.bsky.feed.like` / emoji reaction records — same dual-PDS write model as the feed | Same |

Frontend consequence: the events UI is PDS-agnostic. The create/RSVP forms call the same API; the backend routes to the correct PDS. Read-after-write (AD-01) makes latency invisible for Bytescape-hosted users.

### 1.2 Data sources (v2)

| Data | Table / record | Notes |
|---|---|---|
| Event records | `events` table (AppView index of `community.lexicon.calendar.event`) | `uri`, `cid`, `author_did`, `title`, `description`, `start_time`, `end_time`, `location` JSONB `{name, address, lat, lng}`, `image_url`, `created_at` |
| RSVP records | `event_rsvps` table (AppView index of `community.lexicon.calendar.rsvp`) | `uri`, `cid`, `actor_did`, `event_uri` FK → events, `status` (`going`, `interested`, `not_going`), `UNIQUE(actor_did, event_uri)` |
| Author profiles | `appview_profiles` | `handle`, `display_name`, `avatar_url` — drives event card author display |
| Comments on events | `appview_posts` (replies to event AT URI) | Event comments are `app.bsky.feed.post` records with `reply_parent` pointing at the event URI — same threading model as post replies (FR-15) |
| Emoji reactions | `appview_likes` + emoji reaction records | Same engagement surface as feed posts (FR-15) |
| Viewer's RSVP state | `event_rsvps` lookup by `(actor_did, event_uri)` | One RSVP per user per event; update replaces the record |

### 1.3 Frontend stack (kept)

React 19 + Vite + TypeScript with the v1 patterns retained per the 2026-08-09 stack decision: `useState`/`useEffect`, localStorage caching (`bytescape_events_sort`, `bytescape_events_view`), and the custom DOM event bus (`bytescape:toast`, `bytescape:viewProfile`, `bytescape:openComposer`, `bytescape:refreshEvents`). No new state library.

---

## 2. v1 Salvage Assessment

**No v1 implementation exists.** The events feature is greenfield — there are no files to salvage.

| # | Source | Verdict | Rationale |
|---|---|---|---|
| 1 | atmo.rsvp (reference impl) | **REFERENCE** | Open-source AT Protocol event app using the same lexicons. Studied for API patterns, RSVP flow, and event record shape — not copied. atmo.rsvp's UI is React + Tailwind, not aligned with Bytescape's terminal aesthetic |
| 2 | Smoke Signal (event index) | **REFERENCE** | Indexes 14,120+ events from the AT Protocol network. Confirms that `community.lexicon.calendar.event` is actively used in the ecosystem and that Bytescape events will be discoverable |
| 3 | v1 PostCard.tsx engagement patterns | **KEEP (pattern reuse)** | Emoji reactions, comment threading, share dialog from the feed's PostCard (1,276 lines) — event comments and reactions reuse these patterns. The reaction picker and toggle-off behavior carry directly |

### 2.1 Defects avoided by this spec (greenfield — no bugs to fix, but design pitfalls prevented)

1. **No event spam in feeds** — events are NOT delivered to followers' feeds. They appear only on the Events screen and on the author's profile (if they place an events widget). This prevents the "calendar spam" problem that plagued Facebook event invites.
2. **RSVP is not a notification bomb** — RSVPs do not auto-notify all followers. Only the event author receives a notification (FR-25). Attendees see RSVP counts, not individual RSVP notifications.
3. **No "invite" model at launch** — v1 Facebook had aggressive event invites. Bytescape v2 has no invite mechanism; users discover events organically (browse, search, follow). This is deliberate simplicity, not a gap.
4. **Location is structured JSONB, not free text** — `{name, address, lat, lng}` enables future map integration and location filtering without a schema migration.

---

## 3. Layout

The events screen has two modes: **listing** (browse/discover events) and **detail** (single event view via modal or direct route). The listing is the default; detail opens as a modal overlay on desktop/tablet and as a fullscreen view on mobile.

### 3.1 Desktop (>900px) — listing with detail modal

```
┌────────────────────────────────────────────────────────────────────────────┐
│ TAB BAR   [.tab-bar-left = 280px]  [📰 Feed] [👤 @you] [＋…]    [🔔] [☰]  │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│  ┌─ SEARCH / FILTER BAR ─────────────────────────────────────────────────┐ │
│  │ > Search events...          [📅 Upcoming ▾] [📍 Location ▾] [🔍]      │ │
│  └──────────────────────────────────────────────────────────────────────┘ │
│                                                                            │
│  ┌─ VIEW TOGGLE ────────────┐  ┌─ CREATE BUTTON ───────────────────────┐ │
│  │ [▦ Grid] [☰ List]        │  │             [> CREATE EVENT]           │ │
│  └──────────────────────────┘  └────────────────────────────────────────┘ │
│                                                                            │
│  ┌─ EVENT CARDS (Grid View — default) ──────────────────────────────────┐ │
│  │ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐     │ │
│  │ │ [image]     │ │ [image]     │ │ [image]     │ │ [image]     │     │ │
│  │ │             │ │             │ │             │ │             │     │ │
│  │ │ AUG 15      │ │ AUG 18      │ │ AUG 22      │ │ SEP 01      │     │ │
│  │ │ Title...    │ │ Title...    │ │ Title...    │ │ Title...    │     │ │
│  │ │ 📍 Location │ │ 📍 Location │ │ 📍 Location │ │ 📍 Online   │     │ │
│  │ │ 👤 Author   │ │ 👤 Author   │ │ 👤 Author   │ │ 👤 Author   │     │ │
│  │ │ ✅ 12 going │ │ 🤔 5 maybe  │ │ ✅ 3 going  │ │ ✅ 28 going │     │ │
│  │ └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘     │ │
│  │ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐     │ │
│  │ │ ...         │ │ ...         │ │ ...         │ │ ...         │     │ │
│  │ └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘     │ │
│  └──────────────────────────────────────────────────────────────────────┘ │
│                          [ > LOAD MORE EVENTS ]                            │
│                                                                            │
│ ┌─ DETAIL MODAL (overlay, opens on card click) ───────────────────────┐   │
│ │ ┌──────────────────────────────────────────────────────────────────┐ │   │
│ │ │ [event image — full width banner]                          [✕]  │ │   │
│ │ │                                                                  │ │   │
│ │ │  EVENT TITLE                                          [Share ↗] │ │   │
│ │ │  📅 Saturday, August 15, 2026 · 7:00 PM – 10:00 PM              │ │   │
│ │ │  📍 Venue Name, 123 Street, City                                │ │   │
│ │ │                                                                  │ │   │
│ │ │  Description text...                                             │ │   │
│ │ │                                                                  │ │   │
│ │ │  Created by [av] @handle · 3 days ago                            │ │   │
│ │ │                                                                  │ │   │
│ │ │  ┌────────────────────────────────────────────────────────────┐  │ │   │
│ │ │  │ [✅ Going]    [🤔 Maybe]    [❌ Not Going]    [Clear RSVP] │  │ │   │
│ │ │  └────────────────────────────────────────────────────────────┘  │ │   │
│ │ │                                                                  │ │   │
│ │ │  ✅ 12 Going  ·  🤔 5 Maybe  ·  ❌ 2 Not Going                  │ │   │
│ │ │                                                                  │ │   │
│ │ │  ┌─ COMMENTS ──────────────────────────────────────────────┐    │ │   │
│ │ │  │ [av] @mika · 2h                                         │    │ │   │
│ │ │  │ "Can't wait for this!"  ❤️ 3                             │    │ │   │
│ │ │  │                                                          │    │ │   │
│ │ │  │ [av] @jon · 1d                                          │    │ │   │
│ │ │  │ "Anyone want to carpool?"  🤔 1                          │    │ │   │
│ │ │  │                                                          │    │ │   │
│ │ │  │ > Add a comment...                               [Post]  │    │ │   │
│ │ │  └──────────────────────────────────────────────────────────┘    │ │   │
│ │ │                                                                  │ │   │
│ │ │  ┌─ REACTIONS ─────────────────────────────────────────────┐    │ │   │
│ │ │  │ ❤️ 5  👍 3  🎉 2  😂 1           [😀 +]                  │    │ │   │
│ │ │  └──────────────────────────────────────────────────────────┘    │ │   │
│ │ └──────────────────────────────────────────────────────────────────┘ │   │
│ └──────────────────────────────────────────────────────────────────────┘   │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
```

### 3.2 List view alternative

```
┌──────────────────────────────────────────────────────────────────────────┐
│ > Search events...        [📅 Upcoming ▾] [📍 Location ▾]  [▦] [☰] [🔍] │
├──────────────────────────────────────────────────────────────────────────┤
│ ┌────────────────────────────────────────────────────────────────────┐   │
│ │ [thumb] │ AUG 15 · 7:00 PM  │ Title of Event         │ ✅ 12    │   │
│ │  80×80  │ 📍 Venue, City    │ @author · 3 days ago    │ 🤔 5    │   │
│ │         │                   │                         │ [RSVP ▾] │   │
│ ├─────────┼───────────────────┼─────────────────────────┼──────────┤   │
│ │ [thumb] │ AUG 18 · 2:00 PM  │ Another Event           │ ✅ 8     │   │
│ │  80×80  │ 📍 Park, Town     │ @author · 1 week ago    │ 🤔 2    │   │
│ │         │                   │                         │ [RSVP ▾] │   │
│ ├─────────┼───────────────────┼─────────────────────────┼──────────┤   │
│ │ ...     │                   │                         │          │   │
│ └────────────────────────────────────────────────────────────────────┘   │
│                      [ > LOAD MORE EVENTS ]                               │
└──────────────────────────────────────────────────────────────────────────┘
```

### 3.3 Tablet (551–900px) — simplified grid, detail as sheet

Grid drops to 2 columns. Detail modal becomes a bottom sheet (slides up, drag-to-dismiss).

```
┌──────────────────────────────────────────────────────┐
│ TAB BAR  [📰 Feed] [👤 @you]              [🔔] [☰]   │
├──────────────────────────────────────────────────────┤
│ > Search events...     [📅 ▾] [📍 ▾] [▦] [☰]  [🔍]  │
│                                                      │
│ ┌─────────────┐ ┌─────────────┐                      │
│ │ [image]     │ │ [image]     │                      │
│ │ AUG 15      │ │ AUG 18      │                      │
│ │ Title...    │ │ Title...    │                      │
│ │ 📍 Location │ │ 📍 Location │                      │
│ │ ✅ 12 going │ │ 🤔 5 maybe  │                      │
│ └─────────────┘ └─────────────┘                      │
│ ┌─────────────┐ ┌─────────────┐                      │
│ │ ...         │ │ ...         │                      │
│ └─────────────┘ └─────────────┘                      │
│              [ > LOAD MORE EVENTS ]                   │
├──────────────────────────────────────────────────────┤
│ 📰 Feed · 👤 Profile · ＋ · 💬 Chat · ☰ More         │  ← bottom nav
└──────────────────────────────────────────────────────┘

Detail sheet (slides up from bottom, 80vh max):
┌──────────────────────────────────────────────────────┐
│ ─────── (drag handle) ───────                   [✕]  │
│ [event image]                                        │
│ TITLE                                                │
│ 📅 Date · 📍 Location                                │
│ Description...                                       │
│ Created by @handle                                   │
│ [✅ Going] [🤔 Maybe] [❌ Not Going]                  │
│ ✅ 12 · 🤔 5 · ❌ 2                                  │
│ ┌─ Comments ─────────────────────────────────────┐   │
│ │ ...                                             │   │
│ └─────────────────────────────────────────────────┘   │
│ ┌─ Reactions ────────────────────────────────────┐   │
│ │ ❤️ 5 👍 3 🎉 2           [😀 +]                 │   │
│ └─────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────┘
```

### 3.4 Mobile (≤550px) — single column, fullscreen detail

Single column cards. Detail opens as a fullscreen view (pushes route to `/events/{uri}`, or modal fullscreen overlay). Create form is also fullscreen.

```
┌──────────────────────────────┐
│ TAB BAR  [📰] [👤] [＋] [💬] [☰] │
├──────────────────────────────┤
│ > Search events...    [🔍]   │
│ [📅 Upcoming ▾] [📍 ▾]       │
│ [▦ Grid] [☰ List]            │
│                              │
│ ┌──────────────────────────┐ │
│ │ [image — full width]     │ │
│ │                          │ │
│ │ AUG 15 · 7:00 PM        │ │
│ │ Title of Event           │ │
│ │ 📍 Venue Name, City      │ │
│ │ 👤 @author               │ │
│ │ ✅ 12 going · 🤔 5 maybe │ │
│ │ [RSVP ▾]                 │ │
│ └──────────────────────────┘ │
│ ┌──────────────────────────┐ │
│ │ [image — full width]     │ │
│ │ ...                      │ │
│ └──────────────────────────┘ │
│     [ > LOAD MORE EVENTS ]   │
├──────────────────────────────┤
│ 📰    👤    ＋    💬    ☰   │  ← bottom nav
└──────────────────────────────┘

Detail (fullscreen):
┌──────────────────────────────┐
│ ← Back                       │
│ [image — full width banner]  │
│                              │
│ EVENT TITLE                  │
│ 📅 Sat, Aug 15 · 7–10 PM    │
│ 📍 Venue, City               │
│                              │
│ Description text...          │
│                              │
│ 👤 @handle · 3 days ago      │
│                              │
│ [✅ Going] [🤔 Maybe] [❌ No]│
│                              │
│ ✅ 12 · 🤔 5 · ❌ 2         │
│                              │
│ ┌─ Comments ───────────────┐ │
│ │ ...                      │ │
│ └──────────────────────────┘ │
│ ┌─ Reactions ──────────────┐ │
│ │ ❤️ 5 👍 3    [😀 +]      │ │
│ └──────────────────────────┘ │
└──────────────────────────────┘
```

---

## 4. Component Breakdown

### 4.1 Screen shell

**`EventsScreen` (NEW)**
- Purpose: mount the events listing with search/filter bar, view toggle, event cards, and detail modal. Hold global state for the events list.
- Props: viewer context from auth.
- State: `events: Event[]`, `status: loading | error | ready`, `cursor`, `hasMore`, `viewMode: 'grid' | 'list'` (persisted to `localStorage.bytescape_events_view`), `sortMode: 'upcoming' | 'past' | 'all'`, `searchQuery`, `locationFilter`, `selectedEvent: Event | null` (detail modal), `showCreateForm: boolean`.
- Loads via `GET /api/events?sort=upcoming&search=...&location=...&cursor=...` — the AppView endpoint queries the `events` table.
- **Stack decision:** React 19 + Vite + TypeScript with the existing patterns — `useState`/`useEffect`, localStorage caching, and the custom DOM event bus. Same approach as feed and profile.
- Detail modal: renders `EventDetailModal` when `selectedEvent` is set. On mobile (≤550px), detail navigates to `/events/{encodedUri}` as a fullscreen route instead of a modal (better back-button behavior).

### 4.2 Search and filter bar

**`EventSearchBar` (NEW)**
- Purpose: text search across event titles and descriptions, plus filter controls.
- Props: `searchQuery`, `onSearchChange`, `sortMode`, `onSortChange`, `locationFilter`, `onLocationChange`.
- State (local): `inputValue` (debounced, 300ms), `locationInputValue`.
- Layout: horizontal bar with search input, sort dropdown, location dropdown/input, and search icon button.
- Search: `> Search events...` placeholder with `>` prompt prefix (terminal aesthetic). Debounced 300ms → API call with `search` param.
- Sort dropdown: `[📅 Upcoming ▾]` (default), `[📅 Past]`, `[📅 All]`. "Upcoming" filters to `start_time >= now()`; "Past" filters to `start_time < now()`; "All" shows everything.
- Location filter: `[📍 Location ▾]` dropdown with free-text input. Filters by `location.name` substring match server-side. Future: geo-proximity when `lat/lng` data is populated (not in v2.0 scope).
- Accessibility: search input has `aria-label="Search events"`, sort/location dropdowns have `aria-label` on the `<select>` element, focus ring on all interactive elements.

**`EventFilterBar` (NEW)**
- Purpose: secondary filter row for category/tag filters (placeholder for post-launch expansion).
- Props: `activeFilters`, `onFilterChange`.
- Renders only when filters are active. Pill chips matching the feed's `FilterChipBar` pattern.
- v2.0 ships with sort + search + location only; category filters are a follow-up once event categorization is standardized in the lexicon.

### 4.3 Event cards

**`EventCard` (NEW)**
- Purpose: display one event in grid or list view. Clickable → opens detail modal.
- Props: `event: Event`, `viewMode: 'grid' | 'list'`, `viewerRsvp: RsvpStatus | null`, `onSelect(event)`.
- State (local): none (stateless display component).
- Grid card layout:
  - Event image (or gradient placeholder with first-letter monogram if no image).
  - Date badge: month abbreviation + day number, positioned top-left over the image (`AUG 15`), styled in VT323 font.
  - Title (truncated to 2 lines with ellipsis).
  - Location line: `📍 {location.name}` (or "Online" if no physical location, or hidden if no location data).
  - Author line: avatar + `@handle`.
  - RSVP summary: `✅ {going_count} going` (or `🤔 {interested_count} maybe` if no going attendees yet — show the most populated status).
- List card layout:
  - Horizontal row: thumbnail (80×80), date/time column, title + author column, RSVP counts + inline RSVP dropdown.
- Hover: subtle border highlight (`--accent` at 30% opacity) + cursor pointer.
- Source badge (optional): if the event was created by a Bluesky-hosted user and originated outside Bytescape, a small `🦋` or `🌍` badge indicates external origin. Bytescape-originated events have no badge.
- Accessibility: `role="article"`, `aria-label` with event title + date + location, keyboard-focusable (`tabindex="0"`), Enter/Space opens detail.

### 4.4 Event detail modal

**`EventDetailModal` (NEW)**
- Purpose: full event view with RSVP, comments, and reactions.
- Props: `event: Event`, `viewerRsvp: RsvpStatus | null`, `onClose`, `onRsvpChange(status)`.
- State (local): `activeTab: 'details' | 'comments'`, `comments: Comment[]`, `commentCursor`, `commentsLoading`, `reactions: ReactionSummary[]`.
- Layout: overlay on desktop (600px max-width, centered, dark backdrop), bottom sheet on tablet (80vh, drag handle), fullscreen on mobile (pushed route).
- Content sections (top to bottom):
  1. **Image banner** — full-width, `object-fit: cover`, max-height 300px. Close button (`✕`) top-right overlay.
  2. **Title** — VT323 font, large. Share button top-right (`↗` copies AT URI link or opens share dialog).
  3. **Date/time** — `📅 {formatted_date} · {start_time} – {end_time}`. Uses `Intl.DateTimeFormat` for locale-aware formatting. If no `end_time`, show start time only.
  4. **Location** — `📍 {location.name}, {location.address}`. If `lat/lng` present, renders a "View on map" link (opens in new tab to OpenStreetMap — no proprietary map dependency).
  5. **Description** — rendered as markdown (same renderer as post content). Supports links, bold, italic, line breaks.
  6. **Author** — avatar + display name + `@handle` + relative time (`3 days ago`). Clickable → profile navigation.
  7. **RSVP buttons** — see 4.5.
  8. **RSVP summary** — `✅ 12 Going · 🤔 5 Maybe · ❌ 2 Not Going`. Clickable counts open a list of attendees (future: v2.1).
  9. **Comments** — threaded list of `app.bsky.feed.post` replies to the event URI. Each comment: avatar, handle, relative time, text, reaction count, reply affordance. `> Add a comment...` input at bottom with Post button. Login-required to comment.
  10. **Reactions** — emoji reaction summary bar (same component as feed PostCard reactions). Reaction picker (`😀 +` button), toggle-off on existing reactions.
- Focus trap: Tab cycles within the modal. Escape closes. First focusable element receives focus on open (the close button or the first RSVP button).
- Scroll: modal body scrolls independently; backdrop does not scroll (`overflow: hidden` on body while modal is open).

### 4.5 RSVP interaction

**`RsvpButton` (NEW)**
- Purpose: three-state RSVP toggle (Going / Maybe / Not Going) with clear option.
- Props: `currentStatus: 'going' | 'interested' | 'not_going' | null`, `onStatusChange(status: RsvpStatus | null)`, `disabled: boolean`.
- State (local): `pending: boolean` (optimistic UI guard).
- Layout: three button row — `✅ Going`, `🤔 Maybe`, `❌ Not Going`. Active state: filled background (`--accent` for Going, `--accent-dim` for Maybe, `--danger-dim` for Not Going) + border. Inactive: ghost/outlined.
- A fourth "clear" affordance appears when any status is active: `✕ Clear RSVP` text button below or beside the row. Clicking clears the RSVP (deletes the `community.lexicon.calendar.rsvp` record from the PDS).
- Behavior:
  - Click active status → no-op (already set).
  - Click different status → optimistic UI update, API call to create/update RSVP record on PDS. On failure: revert to previous status + toast error.
  - Click clear → optimistic removal, API call to delete RSVP record. On failure: restore previous status + toast error.
  - Login-required: if viewer is unauthenticated, clicking any RSVP button redirects to login with return URL.
- RSVP record shape (matching `community.lexicon.calendar.rsvp`):
  ```
  {
    $type: "community.lexicon.calendar.rsvp",
    event: { uri: event.uri, cid: event.cid },
    status: "going" | "interested" | "notGoing",
    createdAt: ISO timestamp
  }
  ```
  Note: lexicon uses `interested` (not "maybe") and `notGoing` (not "not_going") — the UI translates between display labels and lexicon values.
- Accessibility: `role="radiogroup"` on the button container, `role="radio"` + `aria-checked` on each button, `aria-label="RSVP status"`.

### 4.6 Event creation

**`EventCreateForm` (NEW)**
- Purpose: create a new `community.lexicon.calendar.event` record on the user's PDS.
- Props: `onClose`, `onCreated(event)`.
- State (local): `title`, `description`, `startDate`, `startTime`, `endDate`, `endTime`, `locationName`, `locationAddress`, `locationLat`, `locationLng`, `imageFile`, `imagePreview`, `submitting`, `errors: Record<string, string>`.
- Layout: fullscreen on mobile, modal (600px max-width) on desktop/tablet. Same modal shell as the detail modal.
- Fields:
  1. **Title** — text input, required, max 300 characters. `> Event name` placeholder.
  2. **Description** — textarea, optional, max 3000 characters. Supports basic markdown. Character counter.
  3. **Start date/time** — date picker + time picker, required. `<input type="date">` + `<input type="time">` styled to match terminal theme. `aria-label="Event start date"` and `aria-label="Event start time"`.
  4. **End date/time** — date picker + time picker, optional. Same styling. Only shown/validated if start is set.
  5. **Location name** — text input, optional. `> Venue name` placeholder.
  6. **Location address** — text input, optional. `> Address` placeholder.
  7. **Image** — file upload (drag-and-drop zone or click-to-browse). Accepts `image/jpeg`, `image/png`, `image/webp`. Max 5MB. Preview shown after selection. Stored as PDS blob.
- Validation:
  - Title required, non-empty.
  - Start date/time required and must be in the future (for "upcoming" events; past events allowed for import scenarios — soft warning, not block).
  - End date/time, if set, must be after start.
  - Image file type and size checks client-side before upload.
- Submit:
  - Creates `community.lexicon.calendar.event` record on the user's PDS (signed commit for Bytescape-hosted, XRPC for Bluesky-hosted — AD-01b).
  - Record shape:
    ```
    {
      $type: "community.lexicon.calendar.event",
      title: string,
      description?: string,
      startTime: ISO timestamp,
      endTime?: ISO timestamp,
      location?: { name?: string, address?: string, lat?: number, lng?: number },
      createdAt: ISO timestamp
    }
    ```
  - Image uploaded as a PDS blob first, then referenced in the record.
  - On success: close form, prepend event to the listing (read-after-write), toast "Event created!".
  - On failure: display error toast, keep form open with values preserved.
- Accessibility: all inputs labeled, form has `aria-label="Create new event"`, submit button has clear text (`> CREATE EVENT`), error messages linked to inputs via `aria-describedby`.

### 4.7 Comments on events

**`EventComments` (NEW — reuses PostCard comment patterns)**
- Purpose: threaded comments on an event, displayed inside `EventDetailModal`.
- Props: `eventUri: string`, `comments: Comment[]`, `cursor`, `hasMore`, `loading`, `onLoadMore`, `onComment(text)`.
- State (local): `inputValue`.
- Comments are `app.bsky.feed.post` records with `replyParent` pointing at the event's AT URI. Same threading model as post replies.
- Each comment: avatar, display name, `@handle`, relative timestamp, text body, reaction count, reply button.
- New comment input: `> Add a comment...` textarea with Post button. Login-required. On submit: optimistic append + API call. On failure: remove optimistic comment + toast error.
- Pagination: "Load more" button for older comments (same cursor pattern as feed).
- Accessibility: comments list has `aria-label="Event comments"`, input has `aria-label="Add a comment"`.

### 4.8 Reactions on events

Event reactions reuse the same emoji reaction system as feed posts (FR-15). Reaction records are standard AT Protocol engagement records. The reaction bar and picker are shared components — the same `ReactionBar` + `ReactionPicker` used on `PostCard` in the feed spec.

- Reaction summary bar: `❤️ 5  👍 3  🎉 2  😂 1` — each count clickable to toggle your own reaction.
- `😀 +` button opens the emoji picker (same picker as feed PostCard).
- Toggle-off: clicking an emoji you already reacted to removes your reaction.
- Optimistic UI with rollback on failure.

---

## 5. States

### 5.1 Loading

**Initial load (listing):**
- Skeleton cards: 6 shimmer placeholders in the current view mode (grid: 6 cards with image/title/text blocks shimmering; list: 6 rows with thumbnail/text blocks shimmering). Green-phosphor shimmer effect (`--accent` at low opacity, animated left-to-right) — same skeleton pattern as the feed.
- Search bar and filter controls are interactive immediately (no skeleton on the chrome).

**Detail modal load:**
- If the event data is already in the listing (card click), the modal opens instantly with the cached data. Comments load asynchronously with their own skeleton.
- If navigating directly to `/events/{uri}` (shared link), the full modal skeleton renders: image banner shimmer, title/text blocks, RSVP button placeholders.

**Load more:**
- Bottom of listing shows `> LOAD MORE EVENTS` button. On click: button text changes to `⟳ Loading...`, cards append on success. Infinite-scroll sentinel also triggers auto-load when scrolled into view (same dual pattern as feed).

### 5.2 Empty

**No events at all (first visit, no network events indexed):**
```
┌──────────────────────────────────────────────┐
│                                              │
│          ┌─────────────────────┐             │
│          │   📅                │             │
│          │   No events yet     │             │
│          │                     │             │
│          │   Be the first to   │             │
│          │   create one!       │             │
│          │                     │             │
│          │  [> CREATE EVENT]   │             │
│          └─────────────────────┘             │
│                                              │
└──────────────────────────────────────────────┘
```
- Calendar icon (📅) centered, VT323 font for heading, Share Tech Mono for body text.
- Primary CTA: `> CREATE EVENT` button. Only shown to authenticated users; unauthenticated visitors see "Log in to create an event" linking to login.

**Search/filter returns no results (filtered-empty):**
```
┌──────────────────────────────────────────────┐
│                                              │
│          ┌─────────────────────┐             │
│          │   🔍                │             │
│          │   No events found   │             │
│          │                     │             │
│          │   Try a different   │             │
│          │   search or filter  │             │
│          │                     │             │
│          │  [> CLEAR FILTERS]  │             │
│          └─────────────────────┘             │
│                                              │
└──────────────────────────────────────────────┘
```
- `> CLEAR FILTERS` button resets search query, sort, and location filter.

### 5.3 Error

**Network/API error:**
```
┌──────────────────────────────────────────────┐
│                                              │
│          ┌─────────────────────┐             │
│          │   ⚠️                │             │
│          │   Something went    │             │
│          │   wrong             │             │
│          │                     │             │
│          │   Could not load    │             │
│          │   events.           │             │
│          │                     │             │
│          │  [> RETRY]          │             │
│          └─────────────────────┘             │
│                                              │
└──────────────────────────────────────────────┘
```
- `> RETRY` button retries the API call.
- Toast notification on the event bus: `bytescape:toast` with error message.

**Create form submission error:**
- Toast: "Failed to create event. Please try again."
- Form stays open with all values preserved.
- Field-level validation errors shown inline with red text + `aria-describedby` linking.

**RSVP failure:**
- Optimistic UI reverts to previous state.
- Toast: "Failed to update RSVP. Please try again."

### 5.4 Edge cases

| Case | Behavior |
|---|---|
| Event with no image | Gradient placeholder with first-letter monogram in VT323. Gradient uses a seeded hue derived from the event title hash (deterministic, same event always gets the same color). |
| Event with no end time | Date/time line shows start only: `📅 Sat, Aug 15 · 7:00 PM` |
| Event with no location | Location line hidden. In card view, the location slot is omitted (not shown as "No location"). |
| Event with online-only location | If `location.name` contains "online", "virtual", "zoom", "discord", etc. (case-insensitive), render `🌐 Online` instead of `📍`. |
| Event in the past | Cards show a subtle `ENDED` badge overlay. Detail modal shows date normally. RSVP buttons remain functional (for past events, "Going" confirms attendance). Sort mode "Upcoming" excludes past events by default. |
| Very long title | Truncated to 2 lines with ellipsis in cards. Full title shown in detail modal. |
| Very long description | Truncated to 4 lines in cards (if description is shown there at all — cards show title + metadata only). Full description in detail modal with scroll. |
| Event by deleted/suspended user | Card renders with `[deleted]` author handle. RSVP buttons disabled. Detail modal shows "This event's author is no longer available." |
| Duplicate RSVP (race condition) | `UNIQUE(actor_did, event_uri)` constraint in `event_rsvps` prevents duplicates. API handles upsert: if a row exists, update `status`; if not, insert. Frontend sends one request at a time (pending guard in `RsvpButton`). |
| Event with 0 RSVPs | Summary line hidden (not "0 Going"). RSVP buttons always visible. |
| Concurrent RSVP updates | Last-write-wins on the PDS record. The `UNIQUE` constraint + upsert on the AppView ensures consistency. Frontend's optimistic UI shows the latest local state; if a stale write wins server-side, the next refresh corrects it. |
| Very long location address | Truncated to 1 line in cards. Full address in detail modal. |

---

## 6. Interactions

### 6.1 Key flows

**Flow 1: Discover and RSVP to an event**
1. User navigates to `/events` (via nav menu, mobile bottom nav "More", or direct URL).
2. Events load in grid view (default), sorted by "Upcoming".
3. User scrolls or searches for a specific event.
4. User clicks an event card → detail modal opens (desktop) or navigates to fullscreen detail (mobile).
5. User reviews event details (title, date, location, description, author).
6. User clicks `✅ Going` → optimistic UI update, RSVP record created on PDS.
7. RSVP summary updates: `✅ 13 going`.
8. User closes modal → returns to listing. The card's RSVP count reflects the update.

**Flow 2: Create a new event**
1. Authenticated user clicks `> CREATE EVENT` button in the listing header.
2. Event create form opens (modal on desktop, fullscreen on mobile).
3. User fills in: title, description, start date/time, end date/time (optional), location (optional), image (optional).
4. User clicks `> CREATE EVENT` submit button.
5. Client-side validation runs. If errors: inline messages appear, form stays open.
6. Image (if any) uploaded as PDS blob → blob reference returned.
7. Event record created on user's PDS → AT URI returned.
8. Direct index insert into `events` table (read-after-write).
9. Form closes. Toast: "Event created!"
10. New event appears at the top of the listing (if sorted by "Upcoming" and the event is in the future).

**Flow 3: Comment on an event**
1. User opens event detail modal.
2. Scrolls to comments section.
3. Types a comment in the `> Add a comment...` input.
4. Clicks `Post` (or presses Enter — configurable, same as feed comments).
5. Comment creates an `app.bsky.feed.post` record with `replyParent` = event URI.
6. Optimistic comment appears at the top of the comments list.
7. On failure: comment removed + toast error.

**Flow 4: React to an event**
1. User opens event detail modal.
2. Scrolls to reactions bar.
3. Clicks an existing emoji to toggle their reaction (on/off).
4. Or clicks `😀 +` to open the picker and select a new emoji.
5. Reaction record created/updated/deleted on PDS.
6. Counts update optimistically.

**Flow 5: Share an event**
1. User clicks the share button (`↗`) in the detail modal header.
2. Copy-to-clipboard: event AT URI (bytescape.social link format: `bytescape.social/events/{encodedUri}`).
3. Toast: "Link copied to clipboard!"
4. Future: native share sheet on mobile (Web Share API).

### 6.2 Keyboard navigation

| Key | Action |
|---|---|
| `Tab` / `Shift+Tab` | Cycle focus through cards, filter controls, buttons |
| `Enter` / `Space` | Open event detail (on focused card), activate button |
| `Escape` | Close detail modal, close create form, dismiss search results |
| `Arrow keys` | Navigate between event cards in grid (left/right) or list (up/down) |
| `/` | Focus search input (when not in an input field) |

### 6.3 Accessibility

- All event cards: `role="article"`, `aria-label="{title}, {date}, {location}"`.
- Detail modal: `role="dialog"`, `aria-modal="true"`, `aria-label="Event: {title}"`.
- Focus trap in modal: Tab cycles through close button, RSVP buttons, comment input, reaction picker.
- RSVP buttons: `role="radiogroup"` with `role="radio"` + `aria-checked`.
- Date/time pickers: native `<input type="date">` and `<input type="time">` with explicit `aria-label`.
- Search input: `aria-label="Search events"`.
- View toggle: `role="group"` with `aria-label="View mode"`, toggle buttons have `aria-pressed`.
- `prefers-reduced-motion`: disables skeleton shimmer animation, disables modal slide-in transition (instant show/hide), disables card hover transitions.
- Live regions: `aria-live="polite"` on RSVP summary for screen reader announcements when counts change.
- Color contrast: all text meets WCAG AA 4.5:1 against `--bg-primary: #0a0a0a`. RSVP status colors (`--accent`, `--accent-dim`, `--danger-dim`) are verified at 12px/600 weight minimum (same accessibility floor as the feed spec).

---

## 7. Responsive Behavior

| Breakpoint | Grid cards | Detail view | Filter bar | Create form |
|---|---|---|---|---|
| **Desktop (>900px)** | 3–4 columns (auto-fill, `minmax(280px, 1fr)`) | Centered modal overlay, 600px max-width | Full horizontal bar with all controls inline | Centered modal, 600px max-width |
| **Tablet (551–900px)** | 2 columns | Bottom sheet (80vh, drag handle) | Compressed: search + sort on one line, location wraps below | Modal, 500px max-width |
| **Mobile (≤550px)** | 1 column (full-width cards) | Fullscreen view (pushed route `/events/{uri}`) | Stacked: search on its own line, filters below | Fullscreen view |

### 7.1 Grid/list toggle behavior

- Default: grid view. Persisted to `localStorage.bytescape_events_view`.
- Grid: cards with image dominant, compact metadata below.
- List: horizontal rows with thumbnail, date/time, title, author, RSVP counts inline.
- Toggle is visible at all breakpoints. On mobile, list view may be preferred for scanning; grid view for browsing with images.

### 7.2 Bottom navigation integration

The events screen is reachable via:
- Desktop: navigation menu (if the user has added Events to their nav widget, or the default nav includes it).
- Mobile: bottom nav → "More" → Events. Or direct URL `/events`.

Events does NOT get a dedicated bottom nav slot (5 slots: Feed, Profile, ＋, Chat, More). It lives under "More" alongside other secondary screens.

---

## 8. Open Questions

| # | Question | Default if unresolved | Impact |
|---|---|---|---|
| **OQ-1** | Should events have a dedicated nav slot in the bottom nav on mobile, or stay under "More"? | Under "More" — events are not high-frequency enough for a dedicated slot at launch. Revisit if event creation/attendance becomes a top-3 activity. | Low — easy to promote later |
| **OQ-2** | Should the Events screen show events from the broader AT Protocol network (Smoke Signal, atmo.rsvp) or only Bytescape-originated events? | **Broader network** — the AppView indexes all `community.lexicon.calendar.event` records from the firehose, not just Bytescape users' events. This matches the interop ethos (AD-01c) and makes the screen immediately populated with Smoke Signal's 14,120+ events. A "Bytescape only" filter can be added later. | High — determines initial content volume |
| **OQ-3** | Should event creators be able to edit/update events after creation, or is creation final? | Creation is final in v2.0 — the AT Protocol record is immutable once committed. To "update" an event, the creator would delete and recreate. A future update could use record versioning if the lexicon supports it. | Medium — UX gap for organizers |
| **OQ-4** | Should there be a "My Events" view showing events the user created or RSVP'd to? | Yes, but as a tab/filter in the listing (not a separate screen). A "My Events" toggle at the top of the listing switches to a filtered view of the user's events and RSVPs. Deferred to v2.1. | Low — filter addition |
| **OQ-5** | Should event attendees be visible (clickable RSVP count → attendee list)? | Deferred to v2.1. The RSVP summary counts are shown; clicking them to see individual attendees is a follow-up. | Low — feature addition |
| **OQ-6** | How should recurring events be handled? | Not in v2.0. The `community.lexicon.calendar.event` lexicon does not define recurrence. If recurrence is needed, it would be a series of individual event records linked by a `seriesId` field — a lexicon extension. | Low — future feature |
| **OQ-7** | Should event notifications (new RSVPs, comments) integrate with FR-25 notification system? | Yes — event author receives notifications for RSVPs and comments. RSVP'd users receive notifications for event updates (if edit is implemented) and new comments. Notification types: `event_rsvp`, `event_comment`. Implementation follows FR-25 notification model. | Medium — notification type additions |

**Resolved by this spec (not questions):** Events use `community.lexicon.calendar.event` + `community.lexicon.calendar.rsvp` lexicons (AD-01c); dual-PDS write model matches feed/profile (AD-01b); events are NOT delivered to feeds (no calendar spam); RSVP is per-user-per-event (UNIQUE constraint); comments are standard post replies; reactions are standard engagement records; grid/list toggle with persisted preference; modal detail on desktop, bottom sheet on tablet, fullscreen on mobile.
