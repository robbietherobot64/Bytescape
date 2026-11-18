# Screen Spec: Vault — Archive Browser (v2)

**Status:** Draft
**Date:** 2026-08-12
**Route:** `/vault` (primary — owner's personal archive); `/@{handle}/vault` (secondary — shared link destination; read-only for visitors)
**Satisfies:** FR-5 (Archive Vault — primary surface), FR-12 (Media Screen — media gallery tab with lightbox). Contributes to FR-1/FR-2/FR-3 (import entry point CTA), FR-11 (reshare of archive posts), FR-13 (composer entry point for sharing), SM-5 (≥40% customize within 14 days — vault engagement drives return visits).

**Sources:**
- v1 implementation: `apps/web/src/screens/` (v1 had basic archive viewing within the profile feed; no dedicated vault screen)
- v2 constraints: `wiki/product/prd.md` (FR-5, FR-12; FR-1/FR-2/FR-3 import flow; FR-11 profile feed), `wiki/architecture/database.md` (`archive_posts`, `import_sessions` tables), `wiki/architecture/decisions.md` (AD-01b dual PDS, AD-01c lexicons, AD-07 AtProtoClient)

> **Scope decision:** The Vault is the dedicated screen for browsing imported archive content (`com.bytescape.archive.post` records). Archive posts do NOT appear in followers' feeds (FR-5) — the vault is the primary place users interact with their imported history. The media gallery tab within the vault satisfies FR-12's gallery-with-lightbox requirement for imported media. Live Bluesky post media appears in the profile feed's media filter (FR-12's "both imported and live" requirement for the general case) — the vault focuses on imported content only.

> **Dual route model:** `/vault` resolves to the signed-in user's vault (owner view with full affordances). `/@{handle}/vault` renders the same data read-only for visitors — archive posts are shareable as bytescape.social links (FR-5), so visitors may arrive at a specific post or the vault overview via a shared link. Unauthenticated visitors see public vault content with a login/signup CTA.

---

## 1. Overview

The Vault screen is the owner's personal archive browser: a dedicated surface for exploring imported history from Facebook, Instagram, and Threads (FR-1/FR-2/FR-3), separated from the public feed (FR-5). Archive posts are stored as `com.bytescape.archive.post` custom lexicon records on the user's PDS — they appear on the AT Protocol firehose but are ignored by Bluesky's AppView (standard custom-lexicon behavior). Bytescape's own AppView indexes them for this screen.

The vault serves two purposes:

| Purpose | Description | FR coverage |
|---|---|---|
| **Archive browser** | Browse all imported posts chronologically, filterable by source platform, content type, and date range | FR-5 |
| **Media gallery** | Grid view of imported media (photos, videos) with lightbox for full-screen viewing | FR-12 |

### 1.1 Audience

| Viewer | Sees | Can do |
|---|---|---|
| Owner (`/vault`) | Full archive + all controls | Browse, filter, search, share posts, open lightbox, start new import |
| Authenticated visitor (`/@{handle}/vault`) | Public archive content (read-only) | Browse, filter, open lightbox, share links, follow the owner |
| Unauthenticated visitor | Public archive content (read-only) | Browse, filter, open lightbox; login/signup CTA on share actions |

### 1.2 Data sources (v2)

| Data | Table / record | Notes |
|---|---|---|
| Archive posts | `archive_posts` (AppView) | `uri`, `author_did`, `source_platform`, `content_text`, `media_urls`, `original_ts`, `import_batch_id`. Indexed from `com.bytescape.archive.post` PDS records |
| Import sessions | `import_sessions` (App DB) | `id`, `user_id`, `platform`, `status`, `total_posts`, `imported_count`, `media_count`, `errors`. Drives the empty state CTA and import-progress banner |
| Author identity | `appview_profiles` | `handle`, `display_name`, `avatar_url` — used for share previews and visitor-facing vault header |
| Media blobs | PDS blob storage | Referenced by `archive_posts.media_urls`; served via PDS blob endpoint or Bytescape CDN proxy |

### 1.3 Dual PDS note (AD-01b)

Both user classes store archive posts the same way: `com.bytescape.archive.post` records on their respective PDS. The vault reads from the local AppView index regardless of PDS class.

| | Bytescape-hosted (`did:web`) | Bluesky-hosted (`did:plc`) |
|---|---|---|
| Archive records written to | Bytescape PDS | bsky.social PDS (via XRPC) |
| Vault reads from | Local AppView (`archive_posts`) | Same — local AppView |
| Media blobs stored on | Bytescape PDS blobs | bsky.social PDS blobs |
| Share links resolve via | bytescape.social/@{handle}/vault/{post-uri} | Same |

Frontend consequence: the vault UI is PDS-agnostic. The only difference is import-time latency (direct PDS write vs. XRPC), which is invisible once the import completes.

### 1.4 Frontend stack (kept)

React 19 + Vite + TypeScript with the v1 patterns retained per the 2026-08-09 stack decision: `useState`/`useEffect`, localStorage caching, and the custom DOM event bus (`bytescape:refreshFeed`, `bytescape:toast`, `bytescape:viewProfile`, `bytescape:navigate`). No new state library.

---

## 2. v1 Salvage Assessment

Verdicts: **KEEP** (reuse as-is or with trivial touch-ups), **REFINE** (keep concept and most code, improve), **REWORK** (keep concept, rebuild internals), **DROP** (obsolete; mine for parts), **DEFER** (concept preserved, design postponed).

Summary: **1 KEEP · 2 REFINE · 1 REWORK · 2 DROP** — v1 had archive viewing embedded in the profile feed but no dedicated vault screen. The salvage extracts usable patterns from the profile feed's archive rendering and the photo gallery lightbox.

| # | File (lines) | Verdict | One-line rationale |
|---|---|---|---|
| 1 | `ProfileFeed.tsx` (270) — archive post rendering | **REFINE** | Archive posts render with `📦 imported · {platform}` badges and source filtering; extract the post-card rendering pattern for the vault's `VaultPostCard`, but the vault gets its own data-fetch pipeline (archive-specific API, not the feed endpoint with filters) |
| 2 | `widgets/WidgetPhotoGallery.tsx` (90) — lightbox | **REFINE** | Grid + lightbox pattern is solid; keep the lightbox shell (fullscreen overlay, keyboard nav, swipe), replace base64-dataURL storage with blob-URL references from `archive_posts.media_urls`, add video support |
| 3 | `PostCard.tsx` (1276) — media pipeline | **REWORK** | The unified media/embed pipeline (image carousel, video player, lightbox) is the richest source for `VaultPostCard` media rendering; extract the image grid (≤4 thumbs), lightbox integration, and video player. The vault card is simpler than the feed card (no reactions, no reposts, no quote-post embeds) — keep media rendering, drop engagement UI |
| 4 | `FeedColumn.tsx` (468) — filter chips | **DROP** | Filter chip pattern (additive multi-select) is conceptually useful but the vault's filter bar is a different UI (dropdown selects, not sidebar checkboxes). Mine the chip-clear-each logic for the active filter display |
| 5 | `CategorySidebar.tsx` (303) — source filtering | **DROP** | Sidebar category model (Following/Feeds/Hashtags/Subscriptions) doesn't map to vault sources (Facebook/Instagram/Threads). The vault filter bar replaces this entirely |
| 6 | `MobileNav.tsx` (49) — bottom nav | **KEEP** | Bottom nav includes `☰ More` which surfaces the vault link; no changes needed |

### 2.1 v1 defects fixed by this spec

1. **No dedicated vault screen** — v1 mixed archive posts into the profile feed with source filter chips (which were removed in v2 per D7 — archive posts never appear in followers' feeds). The vault is the proper surface for archive browsing.
2. **Archive posts unreachable outside profile** — v1 archive content was only visible on the profile feed widget. The vault at `/vault` gives archive browsing its own URL, its own state, and its own shareable links.
3. **No media gallery for imported content** — v1's WidgetPhotoGallery was profile-only and stored base64 data. The vault's media tab provides a proper gallery over imported media with blob-URL references.
4. **Lightbox missing video support** — v1 lightbox handled images only. The vault lightbox supports both images and videos from imported archives.
5. **No date-range browsing** — v1 had no date filtering for archive posts. The vault filter bar includes a date-range picker for browsing by time period.

---

## 3. Layout

### 3.1 Desktop (>900px) — filter bar + content area

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ TAB BAR  [.tab-bar-left = 280px]  [📰 Feed] [👤 @you] [＋…]     [🔔 3] [☰] │
├──────────────────────────────────────────────────────────────────────────────┤
│ ┌─ VAULT HEADER ───────────────────────────────────────────────────────────┐ │
│ │ 📦 VAULT                                              [📦 New Import]   │ │
│ │ 2,847 archived posts · 1,203 media · Facebook, Instagram                │ │
│ └──────────────────────────────────────────────────────────────────────────┘ │
│ ┌─ FILTER BAR ────────────────────────────────────────────────────────────┐ │
│ │ [All Platforms ▾] [All Types ▾] [Date Range ▾]    [▦ Grid | ☰ List]    │ │
│ │                     active: [# Facebook ✕] [# Images ✕] [Clear all]    │ │
│ └──────────────────────────────────────────────────────────────────────────┘ │
│ ┌─ CONTENT ───────────────────────────────────────────────────────────────┐ │
│ │                                                                         │ │
│ │  LIST VIEW (Posts tab — default):                                       │ │
│ │  ┌─ post card ────────────────────────────────────────────────────────┐ │ │
│ │  │ [av] Marcus  @marcus      📦 Facebook · Jan 15, 2019              │ │ │
│ │  │ "Throwback to our trip to Barcelona! The architecture was..."       │ │ │
│ │  │ ┌─────┐ ┌─────┐ ┌─────┐                                          │ │ │
│ │  │ │ img │ │ img │ │ img │                                          │ │ │
│ │  │ └─────┘ └─────┘ └─────┘                                          │ │ │
│ │  │                                              [🔗 Share] [⋯]        │ │ │
│ │  └────────────────────────────────────────────────────────────────────┘ │ │
│ │  ┌─ post card ────────────────────────────────────────────────────────┐ │ │
│ │  │ [av] Marcus  @marcus      📦 Instagram · Mar 22, 2020             │ │ │
│ │  │ "Golden hour at the beach 🌅"                                     │ │ │
│ │  │ ┌─────────────────┐                                               │ │ │
│ │  │ │     image       │                                               │ │ │
│ │  │ └─────────────────┘                                               │ │ │
│ │  │                                              [🔗 Share] [⋯]        │ │ │
│ │  └────────────────────────────────────────────────────────────────────┘ │ │
│ │                                                                         │ │
│ │  GRID VIEW (Media tab):                                                │ │
│ │  ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐                                │ │
│ │  │ img  │ │ img  │ │ vid ▶│ │ img  │                                │ │
│ │  ├──────┤ ├──────┤ ├──────┤ ├──────┤                                │ │
│ │  │ img  │ │ img  │ │ img  │ │ vid ▶│                                │ │
│ │  ├──────┤ ├──────┤ ├──────┤ ├──────┤                                │ │
│ │  │ img  │ │ img  │ │ img  │ │ img  │                                │ │
│ │  └──────┘ └──────┘ └──────┘ └──────┘                                │ │
│ │                                                                         │ │
│ │            [ > LOAD MORE ]  (infinite-scroll sentinel)                 │ │
│ └─────────────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Tablet (551–900px) — compressed filter bar

```
┌────────────────────────────────────────────────────┐
│ TAB BAR  [📰 Feed] [👤 @you]         [🔔] [☰]     │
├────────────────────────────────────────────────────┤
│ ┌─ VAULT HEADER ──────────────────────────────────┐│
│ │ 📦 VAULT                         [📦 New Import]││
│ │ 2,847 posts · 1,203 media                       ││
│ └─────────────────────────────────────────────────┘│
│ ┌─ FILTER BAR ────────────────────────────────────┐│
│ │ [All Platforms ▾] [All Types ▾] [Date ▾]  [▦|☰]││
│ └─────────────────────────────────────────────────┘│
│ ┌─ POST CARD ─────────────────────────────────────┐│
│ │ 📦 Facebook · Jan 15, 2019                      ││
│ │ "Throwback to Barcelona…"                       ││
│ │ ┌──────┐ ┌──────┐ ┌──────┐                     ││
│ │ └──────┘ └──────┘ └──────┘                     ││
│ │                             [🔗 Share] [⋯]      ││
│ └─────────────────────────────────────────────────┘│
│ ┌─ POST CARD ─────────────────────────────────────┐│
│ │ …                                               ││
│ └─────────────────────────────────────────────────┘│
│            [ > LOAD MORE ]                         │
├────────────────────────────────────────────────────┤
│ 📰 Feed · 👤 Profile · ＋ · 💬 Chat · ☰ More      │
└────────────────────────────────────────────────────┘
```

- Filter bar: all three dropdowns inline on one row (they fit at 551px).
- Media grid: 3 columns.
- Post cards: avatars hidden on tablet (name + handle inline); media thumbs at 3 per row max.

### 3.3 Mobile (≤550px) — stacked filters, fullscreen lightbox

```
┌─────────────────────────────┐
│ TAB BAR  [📦 Vault]  [☰]   │
├─────────────────────────────┤
│ ┌─ VAULT HEADER ───────────┐│
│ │ 📦 VAULT    [📦 Import]  ││
│ │ 2,847 posts · 1,203 media││
│ └──────────────────────────┘│
│ ┌─ FILTER BAR ─────────────┐│
│ │ [Platforms ▾] [Types ▾]  ││
│ │ [Date Range ▾]    [▦|☰]  ││
│ └──────────────────────────┘│
│ ┌─ POST CARD ──────────────┐│
│ │ 📦 Facebook · Jan 2019   ││
│ │ "Throwback to…"          ││
│ │ ┌────┐ ┌────┐           ││
│ │ └────┘ └────┘           ││
│ │          [🔗] [⋯]       ││
│ └──────────────────────────┘│
│       [ > LOAD MORE ]       │
├─────────────────────────────┤
│ 📰    👤    ＋    💬    ☰   │
└─────────────────────────────┘
```

- Filter bar: two rows (2 dropdowns + 1 dropdown + view toggle).
- Media grid: 2 columns.
- Post cards: simplified — platform + date on one line, text truncated to 3 lines with "Show more", max 2 media thumbs with "+N" overflow indicator.
- Lightbox: fullscreen (100vw × 100vh), swipe between items, tap to dismiss controls.

---

## 4. Component Breakdown

### 4.1 Screen shell

**`VaultScreen` (NEW)**
- Purpose: resolve the target user (self or visitor), fetch archive data, hold filter/view state, orchestrate the vault layout.
- Props: `handle` (optional route param — absent for `/vault`, present for `/@{handle}/vault`), viewer context from auth.
- State: `posts: ArchivePost[]`, `cursor`, `hasMore`, `loading`, `error`, `filters: VaultFilters`, `viewMode: 'list' | 'grid'`, `importSessions: ImportSession[]`, `lightbox: { open: boolean, index: number, items: MediaItem[] }`.
- Data fetch: `GET /api/vault/{handle}?platform=&type=&date_from=&date_to=&cursor=&limit=20` — the AppView's archive query endpoint. Owner route (`/vault`) resolves handle from auth session.
- Responsive: renders the filter bar + content area; layout shifts handled by CSS (not separate render branches — v1 profile lesson).
- Event bus: listens for `bytescape:importComplete` (new import finished → refetch posts + update header stats), `bytescape:navigate` (deep-link to a specific post or media item).

### 4.2 Vault header

**`VaultHeader` (NEW)**
- Purpose: vault title, aggregate stats, import entry point.
- Props: `totalPosts`, `totalMedia`, `platforms: string[]`, `isOwner`, `activeImport: ImportSession | null`.
- Renders:
  - `📦 VAULT` title (VT323, uppercase, accent color).
  - Stats line: `{n} archived posts · {n} media · {platform list}` (Share Tech Mono, dim text).
  - **Owner only:** `[📦 New Import]` button → navigates to `/import` (or opens the import modal if inline import is chosen — see OQ-2).
  - **Active import banner** (conditional): if `importSessions` has a `status: 'processing'` entry, show a progress bar with `{imported_count} / {total_posts} posts · {media_count} media · {errors} errors` and a pulsing accent border. Dismissible; reappears on vault refresh if still processing.

### 4.3 Filter bar

**`VaultFilterBar` (NEW)**
- Purpose: filter archive content by platform, content type, and date range; switch between list and grid views.
- Props: `filters: VaultFilters`, `onFilterChange`, `viewMode`, `onViewModeChange`, `resultCount`, `isOwner`.
- `VaultFilters = { platform: 'all' | 'facebook' | 'instagram' | 'threads', contentType: 'all' | 'text' | 'image' | 'video' | 'event', dateFrom: string | null, dateTo: string | null }`.
- Subcomponents:

**`FilterDropdown`** — reusable select with accent-styled dropdown. Keyboard-navigable (Arrow keys, Enter to select, Escape to close). Each dropdown renders a labeled trigger (`[All Platforms ▾]`) and a scrollable option list. Active selection highlighted with accent background.

**`DatePickerRange`** — two linked date inputs (From / To). Presets: "All time", "Last year", "Last month", "Custom range". Custom range opens a simple calendar grid (month-by-month, green-accent selected range). Accessible: labeled inputs, keyboard-navigable calendar.

**`ActiveFilterChips`** — rendered below the dropdowns when any filter is active. Pills: `[# Facebook ✕]`, `[# Images ✕]`, `[Jan 2019 – Dec 2020 ✕]`. `✕` removes that filter; `[Clear all]` resets all. `aria-live="polite"` region announces filter changes: "Filtered to Facebook posts with images, 342 results".

**`ViewModeToggle`** — `[▦ Grid]` / `[☰ List]` segmented toggle. Default: List (posts). Grid mode shows the media gallery. Persists to `localStorage.bytescape_vault_view`.

### 4.4 Post list (List view)

**`VaultPostList` (NEW)**
- Purpose: reverse-chronological list of archive posts with infinite scroll.
- Props: `posts`, `loading`, `hasMore`, `onLoadMore`, `onOpenLightbox`, `onShare`, `isOwner`.
- Renders skeleton shimmer cards during initial load (3 placeholders matching card shape). Empty state delegates to `VaultEmptyState`. Error state: accent-bordered error box with `> RETRY` button.
- Infinite scroll: IntersectionObserver sentinel at the bottom + `[> LOAD MORE]` button (both — the button is the fallback and the retro signature, matching the feed pattern).

**`VaultPostCard` (NEW)**
- Purpose: render a single archive post with media, metadata, and actions.
- Props: `post: ArchivePost`, `onOpenLightbox(startIndex)`, `onShare`, `isOwner`.
- Layout:
  - **Header row:** avatar (from `appview_profiles`), display name, handle, platform badge (`📦 Facebook` / `📦 Instagram` / `📦 Threads` — colored per platform: Facebook `#1877f2`, Instagram `#e4405f`, Threads `#000`), original date.
  - **Body:** `content_text` with paragraph breaks preserved. "Show more" truncation at 5 lines (~400 chars).
  - **Media grid:** image/video thumbnails in a responsive grid (≤4 items: 2×2; 1 item: full width; 5+: 2×3 with "+N" overlay on the last). Click opens lightbox at that index. Video items show a `▶` overlay.
  - **Actions row:** `[🔗 Share]` (copies bytescape.social link to clipboard + toast), `[⋯]` menu (Copy link, Open original date, View import batch). No reactions, no reposts, no comment threading — archive posts are read-only browse items (FR-5), not live social objects.
- Kept from v1 PostCard: media grid rendering logic (image aspect ratio handling, 2×2 grid with `object-fit: cover`), lightbox trigger pattern. Dropped: engagement UI (likes, reposts, reactions, comments, quote-posts), embed pipeline (YouTube/SoundCloud/etc.), follow-on-card.

### 4.5 Media grid (Grid view)

**`VaultMediaGrid` (NEW)**
- Purpose: masonry-style grid of all imported media, filterable by source and type.
- Props: `mediaItems: MediaItem[]`, `loading`, `hasMore`, `onLoadMore`, `onOpenLightbox(index)`, `filters`.
- `MediaItem = { uri: string, url: string, type: 'image' | 'video', thumbnailUrl: string, postUri: string, platform: string, originalDate: string }`.
- Layout: CSS grid with `auto-fill` and `minmax(200px, 1fr)` on desktop; 3 columns on tablet; 2 columns on mobile. Each cell is a square thumbnail with platform badge overlay (bottom-left) and a video `▶` indicator (center) for video items. Hover: subtle accent border + slight scale (1.02, 150ms ease).
- Infinite scroll: same pattern as the post list (sentinel + button).
- Kept from v1 WidgetPhotoGallery: grid cell aspect ratio handling, hover effects. Added: video support, platform badges, infinite scroll (v1 gallery was a fixed widget with limited items).

### 4.6 Lightbox

**`MediaLightbox` (NEW)**
- Purpose: full-screen media viewer with keyboard/touch navigation.
- Props: `items: MediaItem[]`, `startIndex`, `onClose`, `onNavigate(index)`.
- Renders:
  - Full-viewport overlay (`position: fixed; inset: 0; z-index: 1000`) with `rgba(0,0,0,0.95)` background.
  - Centered media container (max 90vw × 85vh, object-fit: contain).
  - **Images:** rendered as `<img>` with lazy loading; pinch-zoom on touch devices.
  - **Videos:** rendered as `<video>` with native controls (play/pause, volume, fullscreen, progress). Autoplay on open (muted); user can unmute.
  - **Navigation:** left/right arrow buttons (48px touch targets), keyboard arrows, swipe on touch.
  - **Info bar** (bottom): platform badge, original date, `{index + 1} / {total}` counter, `[🔗 Share]` button, `[✕ Close]` button.
  - **Post context** (expandable): tap/click the info bar to expand and show the post's text content and a link back to the full post card.
- Accessibility: focus trap (Tab cycles through close, prev, next, share, info), `role="dialog"`, `aria-modal="true"`, `aria-label="Media viewer {n} of {total}"`, Escape to close, `prefers-reduced-motion` disables transitions and zoom animations.
- Kept from v1 WidgetPhotoGallery lightbox: overlay pattern, keyboard nav, swipe. Added: video support, info bar with post context, pinch-zoom, focus trap.

### 4.7 Empty state

**`VaultEmptyState` (NEW)**
- Purpose: guide users who have no imported content toward the import flow.
- Props: `isOwner`, `onStartImport`.
- **Owner, no imports:** Full empty state with VT323 heading (`> YOUR VAULT IS EMPTY`), body text explaining what the vault is and why importing matters, and a prominent `[📦 Import Your History]` CTA button that routes to `/import`. Secondary link: "What is archive import?" → help/faq.
- **Owner, all filtered out:** Lighter empty state: "No posts match your filters. [Clear filters]" — filter reset button clears all active filters.
- **Visitor, empty vault:** "This user hasn't imported any content yet." + follow button if not already following.
- **Unauthenticated:** "Sign in to import your own archive." + login/signup CTA.

### 4.8 Subcomponents

**`PlatformBadge` (NEW)**
- Purpose: colored tag identifying the source platform.
- Props: `platform: 'facebook' | 'instagram' | 'threads'`.
- Renders: `[📦 Facebook]` / `[📦 Instagram]` / `[📦 Threads]` with platform-specific background color (Facebook `#1877f2`, Instagram `#e4405f`, Threads `#333`) and white text, 11px Share Tech Mono, 4px radius, 2px horizontal padding. `aria-label="Imported from {platform}"`.

**`SkeletonCard` (NEW)**
- Purpose: shimmer placeholder matching the VaultPostCard shape during loading.
- Renders: gray rectangles for avatar (32px circle), name line (80px × 12px), body (3 lines × random width), media area (2×2 grid of gray squares). CSS `@keyframes shimmer` animation (left-to-right gradient sweep, 1.5s infinite).

---

## 5. States

| State | Trigger | What renders |
|---|---|---|
| **Loading** (initial) | First vault load | VaultHeader skeleton + 3 `SkeletonCard` shimmer placeholders. Filter bar visible but disabled. |
| **Loading** (pagination) | "Load more" triggered | Existing posts remain; 1 `SkeletonCard` appended at the bottom with spinner. |
| **Empty — no imports** | User has zero `import_sessions` with `status: 'completed'` | `VaultEmptyState` (owner variant): prominent CTA to `/import`. Header shows "0 archived posts". |
| **Empty — filtered** | Active filters produce zero results | `VaultEmptyState` (filtered variant): "No posts match your filters" + clear-filters button. Previous content hidden, not replaced with skeleton. |
| **Populated** | Posts exist and no filter mismatch | Full post list or media grid per view mode. Header shows live stats. |
| **Error** | API fetch fails (network, 500, timeout) | Accent-bordered error box with error message and `[> RETRY]` button. Previous content stays if paginating; replaced if initial load. |
| **Active import** | `import_sessions` has a `processing` entry | Progress banner in header: `{imported} / {total} posts · {media} media` with animated accent progress bar. Posts already imported appear in the list below the banner. Banner auto-refreshes every 5s via polling or SSE. |
| **Import complete (toast)** | `bytescape:importComplete` event fires | Toast notification: "Import complete! {n} posts imported." Vault refetches automatically. |
| **Lightbox open** | User clicks media item | `MediaLightbox` overlay takes focus. Background content inert (`aria-hidden="true"`). Scroll locked on body. |
| **Sharing** | User clicks `[🔗 Share]` | Copy bytescape.social link to clipboard → toast: "Link copied!". Share menu (future: native share sheet on mobile) deferred to OQ-3. |
| **Visitor — owner-only features hidden** | Non-owner viewing `/@{handle}/vault` | "New Import" button hidden. Post `[⋯]` menu limited to "Copy link" only (no "View import batch"). Filter bar and lightbox fully functional. |
| **Unauthenticated** | No auth session | Same as visitor, plus login/signup CTA in the vault header area. Share actions prompt login. |

---

## 6. Interactions

### 6.1 Filter flow

1. User clicks a filter dropdown (e.g., "All Platforms").
2. Dropdown opens with options: All, Facebook, Instagram, Threads.
3. User selects "Facebook" → dropdown label updates to "Facebook"; active filter chip `[# Facebook ✕]` appears below.
4. `VaultScreen` refetches with `?platform=facebook`. Loading state shown briefly (skeleton or spinner); results update in place.
5. User adds more filters (type: Images, date range: 2019–2020). Each adds a chip; each triggers a refetch with combined params.
6. User clicks `✕` on a chip → that filter clears, refetch with remaining filters.
7. User clicks "Clear all" → all filters reset to defaults, full refetch.
8. View mode toggle (List/Grid) does not refetch — it re-renders the same data in the alternate layout. Grid mode filters to media-containing posts only.

### 6.2 Lightbox flow

1. User clicks a media thumbnail in a post card (list view) or a grid cell (grid view).
2. `MediaLightbox` opens at the clicked index. Focus moves to the close button.
3. Left/right arrows or keyboard arrows navigate between items. Swipe works on touch.
4. For videos: click to play/pause, native controls for volume/progress/fullscreen.
5. Info bar shows platform, date, counter. Tap to expand for post text context.
6. `[🔗 Share]` copies the direct link to the media item (bytescape.social/@{handle}/vault/media/{uri}).
7. `[✕]` or Escape closes the lightbox. Focus returns to the triggering element.

### 6.3 Share flow

1. User clicks `[🔗 Share]` on a post card or in the lightbox.
2. Link format: `bytescape.social/@{handle}/vault/{post-uri}` (deep link to the specific archive post within the vault).
3. Link copied to clipboard → toast: "Link copied!" (via `bytescape:toast` event bus).
4. On mobile: if `navigator.share` is available, offer the native share sheet as an alternative to clipboard copy (see OQ-3).
5. Visitor clicking a share link arrives at `/@{handle}/vault` with the specific post highlighted/scrolled-to (deep link via URI fragment or query param).

### 6.4 Keyboard navigation

| Key | Context | Action |
|---|---|---|
| `Tab` / `Shift+Tab` | Vault page | Cycles through filter dropdowns, view toggle, post cards, action buttons |
| `Arrow keys` | Lightbox open | Navigate between media items |
| `Escape` | Lightbox open | Close lightbox, return focus to trigger |
| `Escape` | Filter dropdown open | Close dropdown |
| `Enter` / `Space` | Post card focused | Toggle "Show more" on truncated text |
| `Enter` / `Space` | Media thumbnail focused | Open lightbox at that item |

### 6.5 Accessibility

- All filter dropdowns: `aria-label`, `role="listbox"`, `role="option"` on items, `aria-selected` on active option.
- Active filter chips: `aria-live="polite"` region announces filter changes with result count.
- Lightbox: `role="dialog"`, `aria-modal="true"`, `aria-label="Media viewer {n} of {total}"`, focus trap, `prefers-reduced-motion` disables animations.
- Post cards: `aria-label="Archive post from {platform}, {date}"`.
- Media grid cells: `aria-label="{type} from {platform}, {date}"`, `role="button"`.
- Platform badges: `aria-label="Imported from {platform}"`.
- "Show more" toggle: `aria-expanded` attribute.
- Loading skeletons: `aria-busy="true"` on the content container, `aria-hidden="true"` on skeleton elements (decorative).

---

## 7. Responsive Behavior

| Breakpoint | Filter bar | Post cards | Media grid | Lightbox |
|---|---|---|---|---|
| **Desktop (>900px)** | All dropdowns inline, one row. View toggle right-aligned. | Full card: avatar, name, handle, platform badge, date, full text, media grid (≤4 thumbs, 2×2), actions row. | 4+ columns (`auto-fill, minmax(200px, 1fr)`). | Max 90vw × 85vh. Arrow buttons visible. Info bar with post context expandable. |
| **Tablet (551–900px)** | All dropdowns inline (they fit). View toggle right-aligned. | Compact card: no avatar (name + handle inline), platform badge + date on one line, text truncated to 3 lines, media max 3 thumbs. | 3 columns. | Max 95vw × 90vh. Arrow buttons visible. Info bar compact. |
| **Mobile (≤550px)** | Two rows: row 1 = Platforms + Types dropdowns; row 2 = Date range + view toggle. | Minimal card: platform + date on one line, text truncated to 2 lines with "Show more", max 2 media thumbs with "+N" overlay, actions as icon-only buttons. | 2 columns. | Fullscreen (100vw × 100vh). Swipe-only navigation (arrow buttons hidden). Tap to toggle controls. Info bar overlays bottom. |

**General responsive rules:**
- Filter bar: no horizontal scroll. Dropdowns use `flex-wrap: wrap` on mobile to stack naturally.
- Post list: single column at all breakpoints (the list is always one card per row).
- Media grid: CSS Grid with `auto-fill` + `minmax(200px, 1fr)` for fluid column count. On mobile, explicit 2-column override for consistent sizing.
- Lightbox: progressive enhancement — desktop gets arrows + keyboard; mobile gets swipe + tap gestures; both get the focus trap and close button.
- Bottom nav: always 56px + safe-area padding at ≤900px (matches the feed spec pattern). "Vault" accessible via `☰ More` menu, or directly if the user has pinned it.

---

## 8. Open Questions

| # | Question | Impact | Default if unresolved |
|---|---|---|---|
| **OQ-1** | Should the vault include a text search within archive post content? The AppView `archive_posts` table has `content_text` — a full-text search index is possible. | Adds a search input to the filter bar; increases API scope. | No search in v2.0 — filters (platform, type, date) are sufficient for initial browsing. Text search added when user feedback requests it. |
| **OQ-2** | Should the "New Import" button open a modal (inline) or navigate to `/import`? The import flow (FR-1/FR-2/FR-3) involves ZIP upload + SSE progress — it may be too complex for a modal. | UX flow for starting imports from the vault. | Navigate to `/import` (separate screen) — the import flow has its own multi-step state and SSE stream that warrant a dedicated route. |
| **OQ-3** | Should share actions use the native Web Share API on mobile (`navigator.share`) instead of clipboard copy? | Share UX on mobile devices. | Offer both: clipboard copy as default + native share sheet if available. The native share option appears as a second button in the share action or as a `[📤 Share via…]` option in the `⋯` menu. |
| **OQ-4** | Should archive posts be editable (e.g., add tags, notes, or corrections to imported content)? | Data model change — adds mutable metadata to `archive_posts` or a companion table. | Not in v2.0. Archive posts are treated as immutable historical records. Editability is a Phase 2 feature if users request it. |
| **OQ-5** | How should the vault handle very large archives (10,000+ posts)? Virtual scrolling for the post list and media grid may be needed to avoid DOM bloat. | Performance at scale. | Infinite scroll with 20-item pages in v2.0. Virtual scrolling (react-window or similar) added if real-world archives cause performance issues. The AppView pagination (cursor-based) already limits the per-request payload. |
| **OQ-6** | Should the vault show import error details (from `import_sessions.errors`)? Some posts may fail to import — the user should know what was skipped and why. | UX for import transparency. | Show a collapsible "Import details" section in the active-import banner and in a post-import summary. Errors listed as `{filename}: {error_message}` with a count. Deferred to the import screen spec for full detail — the vault shows the summary only. |
| **OQ-7** | Should there be a "Delete archive" bulk action to remove all imported content? | Data management, destructive action. | Not in v2.0. Individual post deletion (from the `⋯` menu) is the only destructive action in the vault. Bulk delete is high-risk and gated behind a confirmation flow that needs dedicated design. |
