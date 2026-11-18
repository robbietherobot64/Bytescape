# Screen Spec: Following (v2)

**Status:** Draft
**Date:** 2026-08-12
**Route:** `/following` (also reachable via the profile nav widget "👤 Following" page and the feed sidebar's Following category header)
**Satisfies:** FR-10 (Feed with Per-Source Control — source management surface), FR-4 (Social Graph Reconstruction — follow/unfollow actions); contributes to FR-26 (Mute Lists — mute entry point), FR-25 (Notifications — follow-related notifications)

**Sources:**
- v1 implementation: `apps/web/src/screens/FollowingScreen.tsx` (183 lines), `src/components/FeedLayout.tsx` (source sidebar sections), `src/components/CategorySidebar.tsx` (303 lines — per-category items/chips)
- v2 constraints: `wiki/product/prd.md` (FR-10, FR-4, FR-26), `wiki/architecture/database.md` (appview_follows, muted_accounts, import_sessions), `wiki/architecture/decisions.md` (AD-00 AT Protocol only, AD-01b dual PDS, AD-01c lexicons, AD-07 AtProtoClient)

---

## 1. Overview

The Following screen is the dedicated surface for managing every source that populates the user's feed: people (follows), custom feed generators (feeds), hashtags, and subscriptions. It is the full-screen counterpart to the feed sidebar's inline source controls — where the sidebar offers quick toggles and filters, the Following screen provides bulk management, search within the list, and a consolidated view of the social graph.

The screen serves authenticated users only (unauthenticated visitors see a login redirect). It displays four tabbed lists — Following (people), Feeds (custom feed generators), Hashtags, and Subscriptions — each with search, follow/unfollow actions, and bulk operations. The Following tab is the default and carries the richest data: avatars, handles, display names, mutual-follow markers, and account-type badges.

### 1.1 Dual PDS rendering (AD-01b)

Both user classes see the same Following screen with identical data sources. The follow graph is unified in `appview_follows` regardless of PDS origin:

| | Bytescape-hosted (`did:web`) | Bluesky-hosted (`did:plc`) |
|---|---|---|
| Follow records | `app.bsky.graph.follow` on Bytescape PDS | `app.bsky.graph.follow` on bsky.social PDS |
| Reads | Identical — AppView queries `appview_follows` | Identical |
| Writes (follow) | Direct PDS write → indexed | XRPC to bsky.social → indexed via relay |
| Writes (unfollow) | Direct PDS delete → indexed | XRPC delete → indexed via relay |

Frontend consequence: the screen is PDS-agnostic. Follow/unfollow actions call the unified API; the PDS split is invisible to the UI. Account-type badges (`📦 bytescape home` / `🦋 bluesky`) render from `appview_profiles.user_type` for informational context only.

### 1.2 Data sources (v2)

| Data | Table / record | Notes |
|---|---|---|
| Follow graph (who follows whom) | `appview_follows` | `UNIQUE(actor_did, subject_did)`. Mutual = both directions exist |
| Profile info (handle, display_name, avatar) | `appview_profiles` | Drives the Following list rows |
| Custom feed subscriptions | App DB (subscriptions table, v1 shape carried forward) | Feed generator URI + user DID |
| Hashtag subscriptions | App DB (subscriptions table) | Hashtag string + user DID |
| Subscription sources | App DB (subscriptions table) | Platform-specific subscription records |
| Muted accounts | `muted_accounts` | `user_id`, `muted_did` — mute entry point from the Following list |
| Import sessions | `import_sessions` | Status display for in-progress imports (relevant to Subscriptions tab) |

### 1.3 What changed vs. v1 at a glance

1. **Bluesky-only** — v1 FollowingScreen had five platform tabs (Bluesky, Mastodon, Lemmy, RSS, All). v2 collapses to four *source-type* tabs (Following, Feeds, Hashtags, Subscriptions) since AD-00 eliminates Mastodon/Lemmy/RSS.
2. **Mutual markers** — v1 showed follows as flat lists with no mutual indicator. v2 queries both directions of `appview_follows` and renders a `⇄ mutual` badge.
3. **Bulk operations** — v1 had no bulk select/unfollow. v2 adds checkbox selection + "Unfollow selected" / "Unsubscribe selected" actions.
4. **Search within list** — v1 had no in-list search. v2 adds a search bar that filters the current tab's items client-side (instant) with a "Search for more…" server fallback.
5. **Account-type badges** — v2 adds `📦`/`🦋` badges distinguishing Bytescape-hosted vs. Bluesky-hosted accounts (consistent with the profile spec's R-4).
6. **Feed sidebar integration** — the Following screen is accessible both as a standalone route (`/following`) and as the expanded view of the profile nav's "👤 Following" page (profile spec 3.4).

---

## 2. v1 Salvage Assessment

Verdicts: **KEEP** (reuse as-is or with trivial touch-ups), **REFINE** (keep concept and most code, improve), **REWORK** (keep concept, rebuild internals), **DROP** (obsolete; mine for parts), **DEFER** (concept preserved, design work postponed to Phase 2).

Summary: **2 KEEP · 3 REFINE · 2 REWORK · 2 DROP · 1 DEFER** — consistent with the v1 FollowingScreen being a thin shell around a five-platform model that no longer applies. The data-fetching patterns and list rendering survive; the platform tabs and layout are rebuilt.

| # | File (lines) | Verdict | One-line rationale |
|---|---|---|---|
| 1 | `FollowingScreen.tsx` (183) | **REWORK** | Five platform tabs (Bluesky/Mastodon/Lemmy/RSS/All) are the v2 four source-type tabs (Following/Feeds/Hashtags/Subscriptions); layout and state management rebuilt, but the fetch + list rendering pattern carries |
| 2 | `CategorySidebar.tsx` (303) — Following section | **REFINE** | The sidebar's Following category already renders avatar rows with ✕ remove — reuse as the Following tab's item component; add mutual badge, account badge, bulk select |
| 3 | `FeedLayout.tsx` (755) — source management logic | **REFINE** | The feed layout's follow/unfollow handlers and subscription CRUD are the API layer this screen delegates to; extract into a shared hook |
| 4 | `PostCard.tsx` (1276) — follow-on-card | **KEEP** | The follow button on post cards uses the same follow API; the button component carries over |
| 5 | `SocialPanel.tsx` (203) — Friends list | **REFINE** | The mutuals list in the social panel is the same data (mutual follows) rendered differently; the mutual-detection join carries as a utility |
| 6 | `MobileNav.tsx` (49) | **KEEP** | Bottom nav entry for Following (under "More" menu on mobile) — thin, no changes needed |
| 7 | `NotificationBell.tsx` (199) — follow notifications | **KEEP** | Follow notifications already route correctly; no changes needed |
| 8 | `src/styles.css` — Following section | **DROP** | v1 Following styles are tightly coupled to the five-platform layout; rewrite against the v2 tab/list structure |
| 9 | `src/hooks/useFollow.ts` (if exists) | **REWORK** | Extract and unify follow/unfollow/mute logic from FeedLayout + FollowingScreen + PostCard into a single `useFollowActions` hook |
| 10 | `src/types/following.ts` (if exists) | **DROP** | v1 type definitions include platform-specific fields (Mastodon instance, Lemmy community); replace with v2 types |

### 2.1 v1 defects fixed by this spec

1. **Five-platform model is dead** — v1 tabs (Bluesky/Mastodon/Lemmy/RSS/All) reference APIs and data sources that don't exist in the v2 Bluesky-only architecture. Fixed: four source-type tabs matching FR-10's actual source categories.
2. **No mutual indicator** — v1 renders all follows identically with no way to distinguish mutual connections from one-way follows. Fixed: mutual badge (`⇄`) derived from bidirectional `appview_follows` lookup.
3. **No bulk operations** — v1 requires one-by-one unfollow/unsubscribe. Fixed: checkbox selection per tab + bulk action bar.
4. **No search within list** — users with large follow lists (hundreds of accounts) must scroll to find a specific follow. Fixed: client-side instant filter with server fallback for "Search for more…".
5. **No account-type context** — v1 shows handles with no indication of whether the account is Bytescape-hosted or Bluesky-hosted. Fixed: `📦`/`🦋` badge consistent with profile spec R-4.
6. **Flat list with no visual hierarchy** — v1 renders all items identically regardless of relationship strength. Fixed: mutual follows sorted to top with visual accent, one-way follows below.

---

## 3. Layout

### 3.1 Desktop (>900px) — centered list with tab bar

```
┌────────────────────────────────────────────────────────────────────────────┐
│ TAB BAR   [📰 Feed] [👤 @you] [Following]                [🔔 3] [☰]       │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│   ┌────────────────────────────────────────────────────────────────────┐   │
│   │ FOLLOWING                                                          │   │
│   ├────────────────────────────────────────────────────────────────────┤   │
│   │                                                                    │   │
│   │  [👤 Following] [📡 Feeds] [# Hashtags] [🔔 Subscriptions]         │   │
│   │  ───────────────────────────────────────────────────────────────── │   │
│   │                                                                    │   │
│   │  > Search following…                          [☑ Select] [▾ Bulk] │   │
│   │  ───────────────────────────────────────────────────────────────── │   │
│   │                                                                    │   │
│   │  ── Mutuals (12) ─────────────────────────────────────────────── │   │
│   │  ┌──────────────────────────────────────────────────────────────┐ │   │
│   │  │ [av] @mika.dev    Mika        ⇄ mutual  🦋  [✓] [Following▾]│ │   │
│   │  │ [av] @jon.gg      Jon         ⇄ mutual  📦  [✓] [Following▾]│ │   │
│   │  │ [av] @skeeter     Skeeter     ⇄ mutual  🦋  [✓] [Following▾]│ │   │
│   │  └──────────────────────────────────────────────────────────────┘ │   │
│   │                                                                    │   │
│   │  ── Following (38) ──────────────────────────────────────────── │   │
│   │  ┌──────────────────────────────────────────────────────────────┐ │   │
│   │  │ [av] @newsbot     News Bot              🦋  [✓] [Following▾]│ │   │
│   │  │ [av] @photography Photo Daily           📦  [✓] [Following▾]│ │   │
│   │  └──────────────────────────────────────────────────────────────┘ │   │
│   │                                                                    │   │
│   │         [ > LOAD MORE ]                                            │   │
│   │                                                                    │   │
│   └────────────────────────────────────────────────────────────────────┘   │
│                                                                            │
│   max-width: 720px, centered · full-height scrollable                      │
└────────────────────────────────────────────────────────────────────────────┘
```

Notes:
- Content area is max 720px, centered — the list doesn't stretch to full viewport width on ultra-wide monitors.
- Tab bar is the app chrome (shared with Feed, Profile, etc.); the Following screen is one tab.
- Source-type tabs (Following / Feeds / Hashtags / Subscriptions) are within the screen's own card.
- "Mutuals" section is always pinned to the top of the Following tab with a count; "Following" (non-mutual) below it.
- Bulk action bar appears at the bottom when ≥1 item is selected: `[Unfollow selected (3)] [Cancel selection]`.

### 3.2 Tablet (551–900px) — same layout, tighter padding

```
┌──────────────────────────────────────────────────────────┐
│ TAB BAR  [📰 Feed] [👤 @you] [Following]      [🔔] [☰]  │
├──────────────────────────────────────────────────────────┤
│                                                          │
│ ┌──────────────────────────────────────────────────────┐ │
│ │ FOLLOWING                                            │ │
│ ├──────────────────────────────────────────────────────┤ │
│ │ [👤 Following] [📡 Feeds] [# Hashtags] [🔔 Subscr.]  │ │
│ │ ──────────────────────────────────────────────────── │ │
│ │ > Search following…              [☑ Select] [▾ Bulk]│ │
│ │ ──────────────────────────────────────────────────── │ │
│ │                                                      │ │
│ │ ── Mutuals (12) ──────────────────────────────────── │ │
│ │ [av] @mika.dev  Mika   ⇄ mutual  🦋  [✓] [Following▾]│ │
│ │ [av] @jon.gg    Jon    ⇄ mutual  📦  [✓] [Following▾]│ │
│ │ …                                                    │ │
│ │                                                      │ │
│ │ ── Following (38) ────────────────────────────────── │ │
│ │ [av] @newsbot   News Bot         🦋  [✓] [Following▾]│ │
│ │ …                                                    │ │
│ │         [ > LOAD MORE ]                              │ │
│ └──────────────────────────────────────────────────────┘ │
│                                                          │
├──────────────────────────────────────────────────────────┤
│ 📰 Feed · 👤 Profile · ＋ · 💬 Chat · ☰ More            │
└──────────────────────────────────────────────────────────┘
 card: full-width with 16px margin · bottom nav 56px + safe-area
```

### 3.3 Mobile (≤550px) — full-width, compact rows

```
┌─────────────────────────────┐
│ TAB BAR  [Following]  [🔔][☰]│
├─────────────────────────────┤
│ ┌───────────────────────────┐│
│ │ FOLLOWING                 ││
│ ├───────────────────────────┤│
│ │[👤][📡][#][🔔]            ││  ← tab icons, scrollable row
│ │Following Feeds Hash Subs  ││
│ │───────────────────────────││
│ │> Search…    [☑][▾ Bulk]   ││
│ │───────────────────────────││
│ │                           ││
│ │── Mutuals (12) ──────────││
│ │[av] @mika.dev  ⇄ 🦋 [▾]  ││
│ │[av] @jon.gg    ⇄ 📦 [▾]  ││
│ │                           ││
│ │── Following (38) ────────││
│ │[av] @newsbot      🦋 [▾] ││
│ │[av] @photography  📦 [▾] ││
│ │                           ││
│ │     [ > LOAD MORE ]       ││
│ └───────────────────────────┘│
│                              │
│ 📰   👤   ＋   💬   ☰       │
└─────────────────────────────┘
 full-width · bottom nav 56px + safe-area
 display_name hidden (handle only) to save space
 action button is a compact [▾] dropdown (Following ▾) instead of inline text
```

Mobile differences:
- Source-type tabs collapse to icon-only row with text labels below (scrollable horizontally if needed).
- Display name hidden on ≤550px — only handle + badges shown to maximize list density.
- Action button changes from `[Following ▾]` text to a compact `[▾]` icon dropdown.
- Bulk action bar pins to bottom, above the bottom nav.

---

## 4. Component Breakdown

### 4.1 Screen shell

**`FollowingScreen` (REWORK)**
- Purpose: resolve the current user's follow graph, manage tab state, coordinate search and bulk operations, lay out the screen.
- Props: viewer context from auth; optional `tab` query param for deep-linking (default: `following`).
- State: `activeTab: 'following' | 'feeds' | 'hashtags' | 'subscriptions'`, `searchQuery: string`, `selectedItems: Set<string>`, `bulkMode: boolean`, `loading`, `error`.
- Loads via `GET /api/following` (follow graph + profile data), `GET /api/subscriptions` (feeds/hashtags/subscriptions).
- Stack: React 19 + Vite + TypeScript, useState/useEffect, shared `useFollowActions` hook (see 4.7).
- Renders inside the app chrome (TabBar, bottom nav on mobile). The screen is a standalone route AND a nav-page widget inside the profile (profile spec 3.4) — the component accepts an optional `embedded` prop that suppresses the route-level title bar when rendered inside the profile nav widget.

### 4.2 Tab bar

**`FollowingTabs` (REWORK from v1 platform tabs)**
- Purpose: switch between the four source types. Replaces v1's Bluesky/Mastodon/Lemmy/RSS/All platform tabs.
- Props: `activeTab`, `onTabChange`, `counts: { following, feeds, hashtags, subscriptions }`.
- Tabs: `👤 Following {n}`, `📡 Feeds {n}`, `# Hashtags {n}`, `🔔 Subscriptions {n}`.
- Rendering: horizontal pill/tab bar on desktop/tablet; icon-only scrollable row on mobile (3.3).
- Accessibility: `role="tablist"`, `role="tab"` per item, `aria-selected`, `aria-controls` pointing to the list panel, arrow-key navigation between tabs.
- Active tab: 2px bottom border `--accent` + accent text color. Inactive: `--text-secondary` text, no border.

### 4.3 Search bar

**`FollowingSearchBar` (NEW)**
- Purpose: filter the current tab's items instantly, with a server fallback for broader search.
- Props: `placeholder`, `value`, `onChange`, `onServerSearch`, `loading`.
- Behavior:
  - Client-side instant filter: as the user types, filter the loaded list by handle, display name, or tag (depending on active tab). No debounce — the list is already in memory.
  - Server fallback: when client-side results are empty and query ≥3 characters, show a "Search for more…" link that triggers `onServerSearch` → fetches from the AppView search API → results merge into the list with a "Search results" section header.
  - Clear: `✕` button or Escape key resets the query.
- Rendering: 36px input, `> Search following…` placeholder (changes per tab: "Search feeds…", "Search hashtags…", "Search subscriptions…"), green phosphor `--accent` focus ring.
- Accessibility: `aria-label="Search following list"`, `role="searchbox"`, live region announces result count.

### 4.4 Following list

**`FollowingList` (REWORK)**
- Purpose: render the paginated, searchable, bulk-selectable list of followed accounts.
- Props: `items: FollowingItem[]`, `loading`, `error`, `hasMore`, `onLoadMore`, `selectedItems`, `onToggleSelect`, `onUnfollow`, `onMute`, `searchQuery`.
- State (local): `confirmDelete: FollowingItem | null` (unfollow confirmation target).
- Structure: splits items into two sections:
  - **Mutuals** (pinned top): items where `isMutual === true`, sorted by most-recently-followed. Section header: `── Mutuals ({count}) ──`.
  - **Following** (below): remaining items, sorted by most-recently-followed. Section header: `── Following ({count}) ──`.
- Each item renders as a `FollowingItem` row.
- Empty state (tab has items but search filtered to none): "No results for '{query}'" with a clear-search link.
- Pagination: infinite-scroll sentinel + `> LOAD MORE` button (dual affordance, matching feed spec pattern).

**`FollowingItem` (REFINE from CategorySidebar's source item)**
- Purpose: a single followed account row with identity, badges, and actions.
- Props: `item: FollowingItem`, `selected`, `onToggleSelect`, `onUnfollow`, `onMute`, `bulkMode`.
- Renders:
  - **Avatar**: 36px circle, loaded from `appview_profiles.avatar_url`; fallback = first letter of handle on `--accent-dim` background.
  - **Handle**: `@handle` in Share Tech Mono, primary text color. Linked to `/@{handle}`.
  - **Display name** (desktop/tablet only): lighter weight, `--text-secondary`.
  - **Mutual badge**: `⇄ mutual` in small accent text, only if `isMutual`.
  - **Account badge**: `📦` (bytescape-hosted) or `🦋` (bluesky-hosted), from `user_type`.
  - **Action area** (right-aligned):
    - Normal mode: `[Following ▾]` dropdown button → menu with "Unfollow", "Mute", "View profile".
    - Bulk mode: checkbox replacing the dropdown.
- Unfollow action: calls `onUnfollow(item)` → confirmation dialog (4.6) → API call → item fades out (opacity 200ms) + count decrements.
- Hover: 2px left border `--accent` highlight (consistent with feed sidebar SourceItem).
- Accessibility: `role="row"`, action button `aria-label="Actions for @{handle}"`, checkbox `aria-label="Select @{handle}"`, focus-visible outline.

### 4.5 Feed source list, hashtag list, subscription list

**`FeedSourceList` (NEW)**
- Purpose: list subscribed custom feed generators with unsubscribe actions.
- Data: from `GET /api/subscriptions?type=feed`.
- Renders: feed name, creator handle, subscriber count (if available), `[Subscribed ▾]` action.
- Structure: flat list (no mutuals split), search filters by feed name.
- Empty state: "No feed subscriptions yet. Discover feeds on Bluesky or from your feed sidebar."

**`HashtagList` (NEW)**
- Purpose: list followed hashtags with unfollow actions.
- Data: from `GET /api/subscriptions?type=hashtag`.
- Renders: `#hashtag` in accent text, post count (if available), `[Following ▾]` action.
- Structure: flat list, search filters by hashtag text.
- Empty state: "No hashtags followed yet. Add hashtags from your feed sidebar."

**`SubscriptionList` (NEW)**
- Purpose: list platform-level subscriptions (RSS feeds, if/when FR-8 ships; future subscription types).
- Data: from `GET /api/subscriptions?type=subscription`.
- Renders: subscription name, source URL/icon, status badge (active/error), `[Subscribed ▾]` action.
- Structure: flat list with status grouping (active first, errored second with warning icon).
- Empty state: "No subscriptions yet. Subscriptions will be available as new source types are added."
- **Note:** this tab's content depends on the Subscriptions model decision (feed spec OQ-2). If Subscriptions remain undefined at v2.0 launch, this tab renders its empty state with a "Coming soon" note and is non-functional — DEFER, not DROP.

### 4.6 Confirmation dialog

**`UnfollowConfirmation` (NEW — shared pattern with feed spec's DeleteConfirmationPopup)**
- Purpose: prevent accidental unfollow/unsubscribe with an explicit confirmation step.
- Spec: centered modal, 320px max, `--bg-secondary`, 8px radius, shadow `0 8px 32px rgba(0,0,0,.5)`.
- Content: title ("Unfollow @{handle}?" / "Unsubscribe from {feed_name}?" / "Remove #{hashtag}?"), the item's avatar/name quoted, one line ("Their posts will no longer appear in your feed.").
- Buttons: `[Cancel]` secondary (default focus, `autoFocus`) + `[Unfollow]` / `[Unsubscribe]` / `[Remove]` danger-styled (`--danger`).
- Behavior: Cancel gets default focus (Enter = Cancel); focus trap; Escape = Cancel; Tab cycles the two buttons; overlay click does nothing (consistent with feed spec D4 — "forces explicit choice").
- After confirm: API call → item fades out (opacity 200ms) → count decrements → toast "Unfollowed @{handle}" on the event bus.

### 4.7 Shared hook

**`useFollowActions` (REWORK — extracted from FeedLayout + FollowingScreen + PostCard)**
- Purpose: unified follow/unfollow/mute API wrapper used by the Following screen, the feed sidebar, post-card follow buttons, and the profile header.
- Returns: `{ follow(did), unfollow(did), mute(did), unmute(did), isFollowing(did), isMutual(did), isMuted(did), loading }`.
- Handles: optimistic UI updates (follow → immediate state flip, rollback on error), toast dispatch on success/failure, `appview_follows` cache invalidation.
- Consumed by: `FollowingScreen`, `SourceSidebar`, `PostCard`, `ProfileHeaderBar`.

---

## 5. States

### 5.1 Loading

- **Initial load**: skeleton rows (3–5 shimmer placeholders matching the item row shape — avatar circle + handle bar + badge placeholders), tab bar visible with counts as `—`.
- **Tab switch**: if data for the new tab hasn't been fetched yet, show skeleton rows; if previously loaded, show cached data immediately and refresh in background (stale-while-revalidate pattern).
- **Load more**: the `> LOAD MORE` button shows a spinner inline; existing rows remain visible.

### 5.2 Populated

- **Following tab**: Mutuals section (if any mutuals exist) pinned to top with accent border, Following section below. Both sorted by most-recently-followed.
- **Feeds tab**: flat list of subscribed feeds, sorted alphabetically.
- **Hashtags tab**: flat list of followed hashtags, sorted alphabetically.
- **Subscriptions tab**: grouped by status (active first, errored second).
- Counts in tab headers reflect the actual number of items per source type.

### 5.3 Empty (not following anyone)

- Full-screen centered empty state (not just the list area):
  - Illustration: a simple terminal-style ASCII art or glyph (e.g., `> _` blinking cursor).
  - Title: "You're not following anyone yet."
  - Body: "Follow people from your feed, search for accounts, or import your social graph to get started."
  - CTAs: `[> EXPLORE FEED]` (routes to `/`), `[> IMPORT GRAPH]` (routes to import flow, FR-1/2/3).
- Tab-specific empty states (when one tab is empty but others aren't): see component-level empty states in 4.5.

### 5.4 Filtered empty (search returns nothing)

- Inline in the list area:
  - Text: "No results for '{query}'." in `--text-secondary`.
  - Link: `[Clear search]` resets the input.
  - If query ≥3 chars: `[Search all of Bytescape →]` link that routes to the global search with the query.

### 5.5 Error

- Inline in the list area (not full-screen):
  - Icon: `⚠` in `--danger`.
  - Title: "Couldn't load your following list."
  - Body: brief error message from the API.
  - CTA: `[> RETRY]` button styled with `--accent` (matches feed spec error pattern).
- Network error during follow/unfollow: toast "Action failed — please try again" on the event bus, no state change (optimistic update rolled back).

### 5.6 Edge cases

- **Self-follow**: impossible at the AT Protocol level; the UI never shows the viewer's own account in the Following list.
- **Deleted/suspended accounts**: render with a ghost avatar, handle struck through (`~~@handle~~`), badge `⚠ account unavailable`, unfollow action still functional.
- **Very long lists (1000+ follows)**: pagination at 50 items per page, infinite-scroll + button dual affordance. Client-side search operates on loaded items only; "Search for more…" triggers server search for items not yet loaded.
- **Rapid follow/unfollow**: debounced — the action button disables for 500ms after click to prevent double-fire. Optimistic UI shows the new state immediately.
- **Concurrent tab and sidebar changes**: unfollowing from the Following screen immediately reflects in the feed sidebar (shared `useFollowActions` hook invalidates the `appview_follows` cache).

---

## 6. Interactions

### 6.1 Unfollow a single account

1. User clicks `[Following ▾]` on a `FollowingItem` row.
2. Dropdown menu appears: "Unfollow", "Mute", "View profile".
3. User clicks "Unfollow".
4. `UnfollowConfirmation` modal appears (4.6): "Unfollow @{handle}?", Cancel (focused) + Unfollow.
5. User clicks Unfollow (or presses Enter since Cancel is focused → dismisses).
6. On confirm: item fades out (200ms), count decrements, toast "Unfollowed @{handle}".
7. Feed sidebar updates (shared cache invalidation).

### 6.2 Follow back (mutual creation)

- Not applicable on the Following screen (you're viewing who *you* follow). Mutual status updates automatically when the other person follows you — the badge appears on next data refresh or via WebSocket push if available.

### 6.3 Bulk unfollow

1. User clicks `[☑ Select]` to enter bulk mode.
2. Checkboxes appear on every item; `[Select all]` and `[Select none]` links appear in the header.
3. User checks 3 accounts.
4. Bottom bulk action bar appears: `[Unfollow selected (3)]` `[Cancel selection]`.
5. User clicks "Unfollow selected (3)".
6. Single `UnfollowConfirmation`: "Unfollow 3 accounts?" with list of handles quoted.
7. On confirm: all 3 fade out simultaneously, counts decrement, toast "Unfollowed 3 accounts".
8. Bulk mode auto-exits when selection is empty.

### 6.4 Mute from the Following list

1. User clicks `[Following ▾]` → "Mute".
2. Immediate action (no confirmation dialog — consistent with feed spec mute behavior: mute is soft and reversible).
3. Toast: "Muted @{handle}. Their posts won't appear in your feed."
4. Item remains in the Following list but gains a `🔇 muted` badge; the action changes to "Unmute".

### 6.5 Search within list

1. User types in the search bar.
2. Client-side filtering: list narrows instantly as characters are typed (handle + display name match, case-insensitive).
3. If no client-side results and query ≥3 chars: "Search for more…" link appears below the search bar.
4. Click "Search for more…" → server search fires → results appear in a "Search results" section above the regular list.
5. Clear: `✕` button or Escape key resets the query and restores the full list.

### 6.6 Tab switching

1. User clicks a tab (e.g., "Feeds").
2. Tab activates (accent bottom border), list area transitions (200ms crossfade or instant swap).
3. Search bar placeholder updates ("Search feeds…").
4. Bulk selection clears on tab switch.
5. Previously loaded tab data is cached in memory; switching back shows cached data immediately, refreshes in background.

### 6.7 Keyboard navigation

- **Tab bar**: arrow keys move between tabs, Enter/Space activates.
- **List items**: arrow keys move focus between rows, Enter opens the action dropdown.
- **Action dropdown**: arrow keys navigate menu items, Enter selects, Escape closes.
- **Bulk mode**: Space toggles checkbox on focused item.
- **Search bar**: Escape clears query and returns focus to the list.
- **Focus trap**: modal dialogs (unfollow confirmation) trap focus; Tab cycles between Cancel and Confirm buttons.

### 6.8 Accessibility

- All interactive elements have `aria-label` attributes.
- Action dropdowns use `aria-haspopup="menu"`, `aria-expanded`.
- Mutual badge: `aria-label="mutual follow"`.
- Account badge: `aria-label="bytescape account"` / `aria-label="bluesky account"`.
- Unfollow confirmation: `role="dialog"`, `aria-modal="true"`, `aria-labelledby` pointing to the title.
- Live region for search result count: "Showing 12 of 50 following".
- Bulk selection: `aria-label="Select @{handle}"` on each checkbox, `aria-label="Select all"` / `aria-label="Deselect all"` on header controls.
- `prefers-reduced-motion`: fade transitions (item removal, tab switch) become instant; skeleton shimmer stops.

---

## 7. Responsive Behavior

| Breakpoint | Layout | Key changes |
|---|---|---|
| **Desktop (>900px)** | Centered card, max 720px | Full tab bar with text labels, display names shown, `[Following ▾]` text action button, mutuals section with accent border |
| **Tablet (551–900px)** | Full-width card with 16px margin | Same as desktop but tighter padding; tab text labels may truncate on narrow tablets; bottom nav visible |
| **Mobile (≤550px)** | Full-width, no margin | Icon-only tab row (scrollable), display names hidden (handle only), compact `[▾]` action button, bulk action bar pins above bottom nav, search bar full-width |

Additional responsive notes:
- **Drawer integration**: when accessed from the feed sidebar's Following category header (not as a standalone route), the Following list renders inside the sidebar drawer on tablet/mobile with its own scroll container. The standalone route renders as a full screen.
- **Bottom nav**: visible on ≤900px. "Following" is accessible via More menu on mobile, or as a direct tab entry.
- **Safe area**: bulk action bar respects `env(safe-area-inset-bottom)` on mobile to avoid overlap with the bottom nav.

---

## 8. Open Questions

| # | Question | Impact | Resolution target |
|---|---|---|---|
| **OQ-1** | Should the Following screen show follower counts alongside each account? The feed sidebar shows counts per *source category* but not per account. Adding follower counts per row enriches the list but requires an additional data fetch per batch. | Component props, API response shape | Before v2.0 implementation |
| **OQ-2** | What is the Subscriptions tab's content model? The feed spec's OQ-2 (Subscriptions = Lemmy/PieFed communities or something else?) gates this tab's functionality. If Subscriptions remain undefined at launch, the tab renders an empty "Coming soon" state. | Entire Subscriptions tab | Gated on feed spec OQ-2 resolution |
| **OQ-3** | Should muting be available as a bulk action alongside unfollow? Currently bulk mode only offers unfollow/unsubscribe. Adding bulk mute is cheap but increases the confirmation dialog complexity (different consequences for unfollow vs. mute). | Bulk action bar, confirmation dialog | During implementation |
| **OQ-4** | Should the Following screen support *reordering* (drag-to-sort) the list? Users with large follow lists may want to pin important accounts to the top. This is a significant UX addition not in v1. | Component complexity, persistence model | Phase 2 — not v2.0 |
| **OQ-5** | Should "View profile" from the action dropdown open in a new tab/modal or navigate away from the Following screen? Navigation away loses the user's scroll position and search context. | Interaction model, routing | Before v2.0 implementation |

**Resolved by this spec (not open questions):** four source-type tabs (not five platform tabs) matching FR-10; mutual markers derived from bidirectional `appview_follows`; bulk unfollow with confirmation; client-side search with server fallback; account-type badges; Bluesky-only per AD-00.
