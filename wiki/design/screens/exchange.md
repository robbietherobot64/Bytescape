# Screen Spec: Exchange (v2)

**Status:** Draft
**Date:** 2026-08-12
**Route:** `/exchange` (bytescape.social/exchange)
**Satisfies:** FR-21 (Marketplace); contributes to FR-15 (Reactions — listing favorites), FR-25 (notifications — listing inquiries), FR-28 (AppView — listing queries), FR-29 (cross-PDS search — listing search)

**Sources:**
- v1 implementation: none (v1 had no marketplace)
- v2 constraints: `wiki/product/prd.md` (FR-21 Marketplace), `wiki/architecture/database.md` (`exchange_listings`, `exchange_categories`, `users` tables), `wiki/architecture/decisions.md` (AD-00 AT Protocol only, AD-01 PDS-first write model, AD-01b dual PDS, AD-01c lexicons, AD-01d chat, AD-07 AtProtoClient trait)
- Format template: `wiki/design/screens/profile.md`

---

## 1. Overview

The Exchange is Bytescape's peer-to-peer marketplace — a Craigslist-meets-MySpace classifieds board where users post listings, browse by category, and contact sellers. FR-21 specifies title, description, price (free-form text, not a validated currency field), category, images, and status (active/sold/expired). Unlike traditional marketplaces, there is no checkout flow, no escrow, and no integrated payments — the Exchange is a **listing board**, not a storefront. Contact happens through Bytescape's existing DM system (FR-19) or through listing comments.

The Exchange is built on a custom lexicon (`com.bytescape.exchange.listing`) written to the user's PDS (AD-01, AD-01c), with a derived index in the AppView Postgres for search, filtering, and browse. This means listings are portable AT Protocol records — a user's marketplace history follows their DID, not the app.

### 1.1 What the Exchange shows

| Content | In the Exchange? | Why |
|---|---|---|
| Active listings from all users | ✅ | The marketplace body (FR-21) |
| Own listings (all statuses) | ✅ with management affordances | Owner sees edit/delete/mark-sold controls |
| Sold listings from other users | ❌ hidden from browse | Status filter excludes sold/expired by default; visible only on seller's "My Listings" view |
| Expired listings | ❌ hidden from browse | Same as sold; auto-expire after configurable period |
| Listings from muted accounts | ❌ | FR-26 consistency; muted sellers never appear |
| Free listings (price = "Free" / "Trade" / empty) | ✅ | Price is free-form text — "Free", "Trade", "OBO", "$50" all valid |

### 1.2 Data sources (v2)

| Data | Table / record | Notes |
|---|---|---|
| Listing body | `exchange_listings` | `uri` (AT Proto record URI), `author_did`, `title`, `description`, `price` (text), `category`, `image_urls` JSONB array, `status` (active/sold/expired), `view_count`, `created_at`, `updated_at` |
| Author identity | `appview_profiles` | `handle`, `display_name`, `avatar_url` — drives listing cards, seller info, search |
| Categories | `exchange_categories` (app DB) | Seed set: For Sale, Free, Housing, Jobs, Services, Community, ISO (In Search Of). Admin-manageable. |
| View counts | `exchange_listings.view_count` | Server-incremented on each listing detail view. No dedup — same as the profile visitor counter philosophy (round-3 decision). One row per view is not worth the storage; a simple counter suffices. |
| Listing favorites | App DB join or `app.bsky.feed.like` on listing URI | Favoriting a listing uses the standard AT Proto like record (`app.bsky.feed.like`) on the listing's URI — no custom record needed. The AppView indexes favorites. |
| Notifications | `notifications` (app DB) | Inquiry, favorite, and price-drop notifications use the existing notification infrastructure (FR-25) |

### 1.3 Dual PDS note (AD-01b)

Listings are `com.bytescape.exchange.listing` records on the user's PDS. The split matters for writes:

| | Bytescape-hosted (`did:web`) | Bluesky-hosted (`did:plc`) |
|---|---|---|
| Creating a listing | Signed commit to Bytescape PDS | XRPC write to bsky.social |
| Reading listings | Identical — AppView derived index | Identical |
| Editing/deleting | Direct PDS mutation | XRPC to bsky.social |
| Listing visibility | Indexed on the firehose → AppView | Same |

Frontend consequence: read is PDS-agnostic (AppView only). Write latency differs for Bluesky-hosted users (the listing appears after the relay indexes it, typically <2s — same pattern as feed posts). Read-after-write for Bytescape-hosted users means listings appear instantly after creation.

### 1.4 Frontend stack (kept)

React 19 + Vite + TypeScript. Same patterns as the rest of the app: `useState`/`useEffect`, localStorage caching (`bytescape_exchange_filters`, `bytescape_exchange_view`), and the custom DOM event bus (`bytescape:toast`, `bytescape:viewProfile`, `bytescape:openComposer`). No new state library.

---

## 2. v1 Salvage Assessment

v1 had no marketplace. Verdicts apply to reusable patterns and shared components from other v1 screens, not exchange-specific code.

Summary: **0 KEEP · 0 REFINE · 0 REWORK · 0 DROP · 0 DEFER** — greenfield screen.

| Source | What carries | Notes |
|---|---|---|
| `PostCard.tsx` (media pipeline) | **Pattern reuse** | Image carousel + lightbox from PostCard (1276 lines) is the reference implementation for listing image display. The listing card's image gallery reuses the same carousel component and lightbox modal. |
| `ComposerModal.tsx` (form shell) | **Pattern reuse** | Modal form shell, media upload flow, and error surfacing patterns carry into `ListingCreateForm`. |
| `CategorySidebar.tsx` (filter UI) | **Pattern reuse** | Category checkbox + filter-input pattern carries into `CategoryFilter`. |
| `NotificationBell.tsx` (toast/event) | **Pattern reuse** | Event bus toast pattern for listing creation confirmation, deletion, status changes. |
| `styles.css` (theme tokens) | **Direct reuse** | All CSS custom properties (`--bg-primary`, `--bg-secondary`, `--accent`, `--danger`, etc.), font stack (VT323/Share Tech Mono), and interaction tokens apply unchanged. |

### 2.1 Design tokens applied

The Exchange uses the same green-phosphor terminal system ratified by the profile and feed specs:

| Token | Exchange usage |
|---|---|
| `--bg-primary: #0a0a0a` | Screen background |
| `--bg-secondary: #111` | Card backgrounds, modal backgrounds |
| `--accent: #00ff41` | Category badges, active filters, status indicators, CTA buttons |
| `--accent-dim: #0a3a0a` | Card borders, input borders |
| `--danger: #ff3333` | Delete actions, expired status |
| `--text-primary: #00ff41` | Headings (VT323) |
| `--text-secondary: #aaa` | Body text, descriptions (Share Tech Mono) |
| `--border-dim: #1a1a1a` | Card grid borders, section dividers |
| `--transition: 0.15s` | Hover states, filter transitions |
| Fonts: VT323 headers, Share Tech Mono body | "EXCHANGE" heading, card titles in VT323; descriptions, prices, metadata in Share Tech Mono |
| `>` prompt prefix on CTAs | `> POST LISTING`, `> LOAD MORE`, `> SEARCH` |

---

## 3. Layout

The Exchange is a single-column content area within the app shell (TabBar + content), centered at max 1200px on desktop. No sidebar — the category filter and search are inline above the grid.

### 3.1 Desktop (>900px) — filter bar + card grid

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│ TAB BAR   [.tab-bar-left = 280px]  [📰 Feed] [👤 @you] [＋…]  [🏪 Exchange] [🔔] │
├──────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  EXCHANGE                                              [> POST LISTING]          │
│                                                                                  │
│  ┌─────────────────────────────────────────────────────────────────────────────┐ │
│  │ [🔍 Search listings…            ]  [All Categories ▾]  [Sort: Newest ▾]     │ │
│  ├─────────────────────────────────────────────────────────────────────────────┤ │
│  │ [For Sale] [Free] [Housing] [Jobs] [Services] [Community] [ISO]   [Clear]  │ │
│  └─────────────────────────────────────────────────────────────────────────────┘ │
│                                                                                  │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐                             │
│  │ ┌──────────┐ │ │ ┌──────────┐ │ │ ┌──────────┐ │                             │
│  │ │  image   │ │ │ │  image   │ │ │ │  image   │ │                             │
│  │ │  thumb   │ │ │ │  thumb   │ │ │ │  thumb   │ │                             │
│  │ └──────────┘ │ │ └──────────┘ │ │ └──────────┘ │                             │
│  │ VHS Player   │ │ Free Couch   │ │ Web Dev Svc  │                             │
│  │ $45          │ │ Free         │ │ $50/hr       │                             │
│  │ For Sale     │ │ Free         │ │ Services     │                             │
│  │ @mika · 2h   │ │ @jon · 1d    │ │ @skeeter · 3d│                             │
│  │ 👁 12 ♥ 3    │ │ 👁 45 ♥ 8    │ │ 👁 23 ♥ 5    │                             │
│  └──────────────┘ └──────────────┘ └──────────────┘                             │
│                                                                                  │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐                             │
│  │ …            │ │ …            │ │ …            │                             │
│  └──────────────┘ └──────────────┘ └──────────────┘                             │
│                                                                                  │
│                         [ > LOAD MORE LISTINGS ]                                 │
│                         (infinite-scroll sentinel)                               │
│                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────┘
 content max-width: 1200px, centered, padding 24px
 card grid: 3 columns, gap 16px
 each card: ~300px wide, image 4:3 ratio, rounded 6px
```

**Own listings tab.** A toggle (pill buttons) appears next to the heading for authenticated users: `[All Listings] [My Listings]`. My Listings shows all of the user's own listings regardless of status, with status badges and management affordances (edit/delete/mark-sold). This is a client-side filter on `author_did === viewer.did` — no separate endpoint.

### 3.2 Tablet (551–900px) — 2-column grid

The filter bar stacks: search input full-width, then category chips + sort on a second row. Grid drops to 2 columns.

```
┌────────────────────────────────────────────────────┐
│ TAB BAR  [📰 Feed] [👤 @you] [🏪] [🔔] [☰]        │
├────────────────────────────────────────────────────┤
│                                                    │
│  EXCHANGE                    [> POST LISTING]      │
│                                                    │
│  [🔍 Search listings…                          ]   │
│  [All Categories ▾]  [Sort: Newest ▾]              │
│  [For Sale] [Free] [Housing] [Jobs] [Services] …   │
│                                                    │
│  ┌──────────────────────┐ ┌──────────────────────┐ │
│  │ ┌──────────────────┐ │ │ ┌──────────────────┐ │ │
│  │ │     image        │ │ │ │     image        │ │ │
│  │ └──────────────────┘ │ │ └──────────────────┘ │ │
│  │ VHS Player           │ │ Free Couch           │ │
│  │ $45 · For Sale       │ │ Free · Free          │ │
│  │ @mika · 👁 12 ♥ 3    │ │ @jon · 👁 45 ♥ 8     │ │
│  └──────────────────────┘ └──────────────────────┘ │
│                                                    │
│              [ > LOAD MORE LISTINGS ]              │
├────────────────────────────────────────────────────┤
│ 📰 Feed · 👤 Profile · ＋ · 💬 Chat · ☰ More       │
└────────────────────────────────────────────────────┘
 card grid: 2 columns, gap 12px
 bottom nav: 56px + safe-area
```

### 3.3 Mobile (≤550px) — single-column list

Search bar, compact category chips (horizontal scroll), and single-column card stack. "Post Listing" button moves to a floating action button (FAB) or the bottom nav `＋`.

```
┌─────────────────────────────┐
│ TAB BAR  [🏪 Exchange]  [🔔]│
├─────────────────────────────┤
│ EXCHANGE                     │
│ [🔍 Search…            ]    │
│ [All ▾] [Sort: Newest ▾]    │
│ [For Sale][Free][Housing]… → │  ← horizontal scroll chips
│                             │
│ ┌─────────────────────────┐ │
│ │ ┌─────────────────────┐ │ │
│ │ │      image          │ │ │
│ │ └─────────────────────┘ │ │
│ │ VHS Player              │ │
│ │ $45 · For Sale          │ │
│ │ @mika · 👁 12 · ♥ 3    │ │
│ └─────────────────────────┘ │
│ ┌─────────────────────────┐ │
│ │ Free Couch              │ │
│ │ Free · Free             │ │
│ │ @jon · 👁 45 · ♥ 8     │ │
│ └─────────────────────────┘ │
│      [ > LOAD MORE ]        │
├─────────────────────────────┤
│ 📰    👤    ＋    💬    ☰   │
│ Feed Prof   Post Chat More  │
└─────────────────────────────┘
 single column, gap 12px, padding 12px
 category chips: horizontal overflow scroll
 ＋ in bottom nav dispatches ListingCreateForm
```

### 3.4 Listing detail modal

Clicking any listing card opens a detail modal (not a new route — preserves scroll position and browse context). On desktop/tablet: centered modal, max 720px wide. On mobile: fullscreen overlay.

```
Desktop/Tablet (>550px):
┌──────────────────────────────────────────────────────────┐
│ ▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒ overlay rgba(0,0,0,.6) ▒▒▒▒▒▒▒▒▒▒▒▒▒▒│
│ ┌──────────────────────────────────────────────────┐ [✕] │
│ │ ┌────────────────────────────────────────────┐   │     │
│ │ │           IMAGE CAROUSEL / LIGHTBOX        │   │     │
│ │ │     [<]  [image 1 of 3]  [>]               │   │     │
│ │ └────────────────────────────────────────────┘   │     │
│ │                                                  │     │
│ │  VHS Player — Working Condition                  │     │
│ │  $45                                             │     │
│ │  For Sale · Posted 2h ago by @mika               │     │
│ │                                                  │     │
│ │  Description text goes here. Multi-line,         │     │
│ │  supports line breaks. Seller can describe       │     │
│ │  condition, specs, pickup details…               │     │
│ │                                                  │     │
│ │  👁 12 views · ♥ 3 favorites                     │     │
│ │                                                  │     │
│ │  [♥ Favorite]  [💬 Message Seller]               │     │
│ │                                                  │     │
│ │  ── Seller ──                                    │     │
│ │  [av] @mika · Member since 2025                  │     │
│ └──────────────────────────────────────────────────┘     │
└──────────────────────────────────────────────────────────┘

Mobile (≤550px):
┌─────────────────────────────┐
│ ← Back            [share]   │
├─────────────────────────────┤
│ ┌─────────────────────────┐ │
│ │   IMAGE CAROUSEL        │ │
│ │   swipeable, full-width │ │
│ └─────────────────────────┘ │
│                             │
│ VHS Player — Working Cond.  │
│ $45                         │
│ For Sale · 2h ago           │
│                             │
│ Description text…           │
│                             │
│ 👁 12 views · ♥ 3 favorites │
│                             │
│ [♥ Favorite] [💬 Message]   │
│                             │
│ ── Seller ──                │
│ [av] @mika · Since 2025     │
└─────────────────────────────┘
 fullscreen on mobile, scrollable
```

**Owner view** replaces `[💬 Message Seller]` with `[✏️ Edit] [🗑️ Delete] [✓ Mark Sold]`.

---

## 4. Component Breakdown

### 4.1 Screen shell

**`ExchangeScreen` (NEW)**
- Purpose: mount the Exchange browse surface; manage listing state, filters, and modals.
- Props: viewer context from auth (optional — unauthenticated users can browse).
- State: `listings: Listing[]`, `status: loading | error | ready`, `filters: { search, category, sort, ownOnly }`, `cursor`, `hasMore`, `selectedListing: Listing | null` (detail modal), `showCreateForm: boolean`.
- Loads via `GET /api/exchange/listings` with query params: `search`, `category`, `sort` (newest/price-asc/price-desc/most-viewed), `cursor`, `limit`.
- **Stack:** React 19 + Vite + TypeScript. useState/useEffect for state, localStorage for persisted filter prefs (`bytescape_exchange_filters`, `bytescape_exchange_view`), event bus for toasts.
- Listens for `bytescape:openExchangeCreate` (fired by bottom nav `＋` when Exchange is active, or by the `> POST LISTING` button).

### 4.2 Filter bar

**`ListingSearchBar` (NEW)**
- Purpose: text search across listing titles and descriptions.
- Props: `value`, `onChange`, `placeholder`.
- State (local): debounced value (300ms), `searching` indicator.
- Behavior: input with `🔍` icon prefix, `> Search listings…` placeholder. Debounced 300ms — fires `onChange` after the user stops typing. Clear button (✕) appears when non-empty. Keyboard: Enter submits immediately (bypasses debounce), Escape clears.
- Accessibility: `aria-label="Search listings"`, `role="searchbox"`, announces result count via `aria-live` region ("42 listings found").

**`CategoryFilter` (NEW)**
- Purpose: category chip bar + dropdown for category filtering.
- Props: `categories: string[]`, `selected: string | null`, `onSelect(category)`.
- State (local): none (controlled by parent).
- Behavior: pill chips for each category. Clicking a chip filters to that category (toggle off if already selected → "All"). Active chip: `rgba(0,255,65,0.12)` bg + `--accent` border + accent text. On mobile (≤550px), chips wrap with horizontal overflow scroll. Dropdown variant (`[All Categories ▾]`) available as an alternative or supplement for many categories.
- Categories: For Sale, Free, Housing, Jobs, Services, Community, ISO (In Search Of). Seed set from `exchange_categories` table; admin-extendable.
- Accessibility: `role="group"` with `aria-label="Filter by category"`, each chip is a toggle button with `aria-pressed`.

**`SortSelect` (NEW)**
- Purpose: sort order control.
- Props: `value: SortMode`, `onChange`.
- Modes: Newest (default), Price: Low → High, Price: High → Low, Most Viewed.
- Renders as a `<select>` styled to match the terminal theme, or a segmented toggle on desktop.
- Accessibility: native `<select>` with `<label>` — inherits browser accessibility.

### 4.3 Listing cards

**`ListingCard` (NEW)**
- Purpose: browse-grid card for one listing.
- Props: `listing: Listing`, `onClick`, `isOwn: boolean`.
- State: none (presentational).
- Layout: image thumbnail (4:3 ratio, first from `image_urls`, placeholder if none), title (VT323, 1rem), price (Share Tech Mono, accent color if present), category badge (pill), seller avatar + handle + relative time, view count + favorite count.
- Kept patterns: image placeholder uses the same initials-fallback as profile avatars. Relative time uses the same `Intl.RelativeTimeFormat` pattern as PostCard.
- Own-listing indicator: subtle left border `--accent` on the card when `isOwn`.
- Status badge (when in "My Listings" view): `ListingStatusBadge` overlay on the image.
- Interactions: click → opens `ListingDetailModal`. Keyboard: Enter/Space on focused card.
- Accessibility: `role="article"`, `aria-label="{title} — {price} — {category}"`, focusable with visible focus ring.

**`ListingStatusBadge` (NEW)**
- Purpose: visual status indicator on listing cards and detail view.
- Props: `status: 'active' | 'sold' | 'expired'`.
- Renders: pill badge over the card image (bottom-left corner). Active = `--accent` bg with dark text. Sold = `--bg-secondary` bg with `--accent` text + `✓` icon. Expired = `--danger` bg with white text.
- Accessibility: `aria-label="Status: {status}"` on the badge element.

### 4.4 Listing detail

**`ListingDetailModal` (NEW)**
- Purpose: full listing view with images, description, seller info, and actions.
- Props: `listing: Listing`, `isOwn: boolean`, `isFavorited: boolean`, `onClose`, `onFavorite`, `onMessage`, `onEdit`, `onDelete`, `onMarkSold`.
- State (local): `lightboxOpen`, `lightboxIndex`, `deleteConfirmOpen`.
- Layout: see 3.4 wireframe. Image carousel at top (reuses PostCard's carousel + lightbox component), then title, price, category badge, timestamp, description (preserves line breaks), stats row (views + favorites), action buttons, seller card at bottom.
- Kept patterns: image carousel from PostCard (swipe on mobile, arrow keys on desktop, dot indicators). Focus trap inside the modal. Escape closes. Overlay click closes (unlike the DeleteConfirmationPopup — this is a view modal, not a destructive confirmation).
- **Owner actions:** Edit (opens `ListingCreateForm` in edit mode), Delete (opens `DeleteConfirmationPopup` — same shared component as feed/profile), Mark Sold (instant status change via API → card badge updates → toast "Marked as sold").
- **Visitor actions:** Favorite (toggle — standard AT Proto like on the listing URI), Message Seller (opens DM composer pre-filled with listing title as context).
- **Unauthenticated:** Favorite and Message redirect to login. View and browse are unrestricted (FR-21 — marketplace is publicly browsable).
- View count: incremented server-side on modal open (every load, no dedup — same philosophy as the profile visitor counter).
- Accessibility: `role="dialog"`, `aria-modal="true"`, `aria-labelledby` pointing to the listing title. Close button has `aria-label="Close listing detail"`. Image carousel has `aria-roledescription="carousel"` with slide labels.

### 4.5 Listing creation

**`ListingCreateForm` (NEW)**
- Purpose: create a new listing or edit an existing one.
- Props: `existingListing?: Listing` (edit mode), `onSubmit`, `onCancel`.
- State (local): `title`, `description`, `price`, `category`, `images: File[]`, `existingImages: string[]`, `submitting`, `errors: FieldErrors`, `dragActive`.
- Rendered as: a modal on desktop/tablet (max 640px, centered), fullscreen overlay on mobile. Same shell pattern as ComposerModal.
- Fields:
  - **Title** (required, max 100 chars) — text input, `aria-label="Listing title"`.
  - **Description** (required, max 2000 chars) — textarea, `aria-label="Listing description"`. Supports line breaks.
  - **Price** (optional, free-form text, max 50 chars) — text input with placeholder "Free, $45, Trade, OBO…". No currency validation — this is intentional (FR-21: free-form price). `aria-label="Price"`.
  - **Category** (required) — select dropdown from `exchange_categories`. `aria-label="Category"`.
  - **Images** (optional, max 8) — drag-and-drop zone + file picker button. Previews shown as a reorderable thumbnail strip. Each thumbnail has a ✕ remove button. Reuses the media upload pipeline from ComposerModal.
- Validation: title and category required. Client-side with inline error messages below each field. `aria-describedby` links each input to its error. `aria-live="polite"` on the error region.
- Submit: `> POST LISTING` button (create) / `> SAVE CHANGES` (edit). Loading spinner on the button during submission. On success: modal closes, new listing appears in the grid (optimistic insert for Bytescape-hosted users via read-after-write; deferred insert for Bluesky-hosted users after relay indexing), toast "Listing posted!".
- On error: toast "Failed to post listing — {reason}", form stays open with data preserved.
- Accessibility: focus trap in modal, all fields labeled, error announcements, `prefers-reduced-motion` disables drag animation.

### 4.6 Shared components reused

| Component | Source | Used for |
|---|---|---|
| `DeleteConfirmationPopup` | Feed spec 4.2 (shared) | Delete listing confirmation — same spec: centered modal, Cancel default focus, overlay does nothing |
| Image carousel + lightbox | `PostCard.tsx` | Listing detail image gallery — swipe, arrows, dots, lightbox fullscreen |
| Media upload pipeline | `ComposerModal.tsx` | Listing creation image upload — drag-and-drop, file picker, preview strip |
| Toast system | Event bus (`bytescape:toast`) | All mutation confirmations |

---

## 5. States

### 5.1 Loading (initial)

3 skeleton cards in the grid (shimmer animation matching card dimensions). Search bar and category chips render immediately (no skeleton for controls). Skeleton uses `--bg-secondary` base with a shimmer gradient sweep at 1.5s interval. Matches the feed spec's skeleton first-paint pattern.

```
┌──────────────┐ ┌──────────────┐ ┌──────────────┐
│ ░░░░░░░░░░░░ │ │ ░░░░░░░░░░░░ │ │ ░░░░░░░░░░░░ │
│ ░░░░░░░░░░░░ │ │ ░░░░░░░░░░░░ │ │ ░░░░░░░░░░░░ │
│              │ │              │ │              │
│ ░░░░░░░░░░░░ │ │ ░░░░░░░░░░░░ │ │ ░░░░░░░░░░░░ │
│ ░░░░         │ │ ░░░░         │ │ ░░░░         │
│ ░ · ░░       │ │ ░ · ░░       │ │ ░ · ░░       │
│ ░░ · ░░      │ │ ░░ · ░░      │ │ ░░ · ░░      │
└──────────────┘ └──────────────┘ └──────────────┘
```

### 5.2 Populated

Grid of listing cards. Each card shows image, title, price, category badge, seller info, view/favorite counts. Infinite scroll sentinel + `> LOAD MORE LISTINGS` button at the bottom (both — same pattern as feed). Active filters shown as chip bar above the grid when any filter is applied.

### 5.3 Empty (no listings exist)

Centered empty state in the grid area:

```
┌──────────────────────────────────┐
│                                  │
│         🏪                       │
│                                  │
│   No listings yet.               │
│   Be the first to post one!      │
│                                  │
│   [ > POST LISTING ]             │
│                                  │
└──────────────────────────────────┘
```

Copy: "No listings yet. Be the first to post one!" with a `> POST LISTING` CTA. For unauthenticated users: "No listings yet. Sign in to post the first one!" with a `> SIGN IN` CTA that routes to login (FR-27).

### 5.4 Filtered empty (filters produce no results)

Same layout as populated state but with a message in place of the grid:

```
┌──────────────────────────────────┐
│                                  │
│   🔍 No listings match your      │
│   filters.                       │
│                                  │
│   Try a different category or    │
│   clear your search.             │
│                                  │
│   [ > CLEAR FILTERS ]            │
│                                  │
└──────────────────────────────────┘
```

"Clear Filters" button resets all filters to defaults. The search bar and category chips remain visible and functional — the user can adjust without navigating away.

### 5.5 Error

Network/API error during listing fetch:

```
┌──────────────────────────────────┐
│                                  │
│   ⚠ Failed to load listings.     │
│                                  │
│   [ > RETRY ]                    │
│                                  │
└──────────────────────────────────┘
```

`> RETRY` button re-fires the current query. Toast also surfaces: "Failed to load listings — {error message}". The search bar and filters remain visible (user can change params before retrying).

### 5.6 Edge cases

| Case | Behavior |
|---|---|
| Listing with no images | Placeholder image: dark `--bg-secondary` rectangle with a `🏪` icon centered, matching card dimensions |
| Listing with very long title | Truncated to 2 lines with CSS `line-clamp: 2`, ellipsis. Full title visible in detail modal. |
| Listing with very long description | Truncated in card (not shown at all — card shows title, price, category, seller only). Full description in detail modal. |
| Price field empty | Card shows no price line. Detail modal shows no price. Not treated as "Free" — that's a deliberate price entry. |
| Price field = "Free" | Displayed in accent color as "Free". |
| Seller account deleted | Listing remains in the index (orphaned). Seller shows as "[deleted]". Listing can be reported/flagged (future). |
| Listing from muted seller | Excluded from all browse results. Not silently hidden — the mute system handles this at the query layer (FR-26). |
| Very many listings (>1000) | Infinite scroll with cursor pagination. No "page 2" — continuous scroll with the `> LOAD MORE` fallback button. |
| Concurrent edit (two tabs) | Last-write-wins on the PDS. No conflict detection. Toast warns if the save fails due to a CID mismatch (optimistic concurrency on the AT Proto record). |
| Image upload fails | Per-image error indicator on the thumbnail. Other images proceed. User can retry or remove the failed image. Listing can be submitted with partial images. |
| Rate limiting | Toast: "Slow down — too many listings. Try again in a minute." Server returns 429 with retry-after header. |

---

## 6. Interactions

### 6.1 Key flows

**Browse and discover**
1. User lands on `/exchange`. Skeleton cards show while listings load.
2. Listings appear in a grid, sorted by Newest (default). User can filter by category (chip click), search (text input), or sort (dropdown).
3. Clicking a listing card opens the detail modal. View count increments server-side.
4. In the detail modal, user can favorite (toggle heart), message seller (opens DM), or close (✕ / Escape / overlay click).
5. Infinite scroll loads more listings as the user scrolls. `> LOAD MORE` button is the fallback.

**Create a listing**
1. User clicks `> POST LISTING` or the bottom nav `＋` (when Exchange is active).
2. `ListingCreateForm` modal opens. Focus lands on the title input.
3. User fills title (required), description (required), price (optional free-form), category (required), images (optional, drag-drop or file picker).
4. `> POST LISTING` submits. On success: modal closes, listing appears in grid, toast "Listing posted!". On error: form stays open, inline errors + toast.
5. For Bluesky-hosted users, the listing may take up to 2s to appear in the grid (relay indexing). A toast explains: "Listing posted — it may take a moment to appear."

**Edit a listing**
1. Owner opens their listing detail → clicks `[✏️ Edit]`.
2. `ListingCreateForm` opens in edit mode, pre-filled with existing data.
3. Owner modifies fields, submits. On success: modal closes, card updates in place, toast "Listing updated!".

**Delete a listing**
1. Owner opens their listing detail → clicks `[🗑️ Delete]`.
2. `DeleteConfirmationPopup` appears (shared component). Cancel default focus. Overlay does nothing.
3. Confirm → API deletes the PDS record + removes from AppView index. Card fades out of the grid (opacity 200ms). Toast "Listing deleted."

**Mark as sold**
1. Owner opens their listing detail → clicks `[✓ Mark Sold]`.
2. Instant: API updates listing status to `sold`. Card gets `ListingStatusBadge` ("Sold"). Toast "Marked as sold!".
3. Sold listings are hidden from public browse but remain visible in "My Listings" view.

**Message seller**
1. Visitor opens a listing detail → clicks `[💬 Message Seller]`.
2. DM composer opens (FR-19) with the listing title as context (pre-filled message: "Hey, I'm interested in your listing: {title}").
3. If unauthenticated → redirect to login (FR-27).

**Favorite a listing**
1. Visitor clicks the `[♥ Favorite]` button in the detail modal (or on the card, if favoriting from the grid is enabled).
2. Toggle: creates/removes an `app.bsky.feed.like` record on the listing's URI. Count updates optimistically.
3. If unauthenticated → redirect to login.

### 6.2 Keyboard navigation

| Key | Context | Action |
|---|---|---|
| `Tab` | Grid | Moves focus between cards |
| `Enter` / `Space` | Focused card | Opens detail modal |
| `Escape` | Detail modal | Closes modal, returns focus to the card |
| `Escape` | Create form | Closes form (with unsaved-changes guard if data entered) |
| `←` / `→` | Image carousel in detail modal | Previous/next image |
| `Tab` | Detail modal | Cycles through focusable elements (close, favorite, message, seller link) |
| `Enter` | Search input | Submit search immediately (bypass debounce) |
| `Escape` | Search input | Clear search |

### 6.3 Accessibility

- All interactive elements have visible focus rings (`outline: 2px solid --accent`).
- Listing cards are `role="article"` with descriptive `aria-label`.
- Category filter chips are toggle buttons with `aria-pressed`.
- Search input has `aria-label` and is wrapped in a `role="search"` landmark.
- Detail modal is `aria-modal="true"` with `role="dialog"` and `aria-labelledby` pointing to the listing title.
- Image carousel has `aria-roledescription="carousel"` and `aria-label` per slide.
- `prefers-reduced-motion`: disables shimmer animation, carousel auto-play (if any), and modal transitions (instant show/hide).
- Price input in the create form: `aria-describedby` links to a helper text: "Enter any text — '$45', 'Free', 'Trade', 'OBO', or leave blank".
- Form validation errors announced via `aria-live="polite"`.
- Status badges have `aria-label="Status: active/sold/expired"`.

---

## 7. Responsive Behavior

### 7.1 Breakpoints

| Breakpoint | Grid columns | Filter layout | Detail view | Create form |
|---|---|---|---|---|
| Desktop (>900px) | 3 columns | Single row: search + dropdown + sort | Centered modal, max 720px | Centered modal, max 640px |
| Tablet (551–900px) | 2 columns | Stacked: search full-width, chips + sort below | Centered modal, max 600px | Centered modal, max 560px |
| Mobile (≤550px) | 1 column | Stacked: search full-width, horizontal-scroll chips, sort | Fullscreen overlay | Fullscreen overlay |

### 7.2 Layout changes

- **Card grid:** CSS Grid with `auto-fill` and `minmax(280px, 1fr)`. Naturally adapts from 3 → 2 → 1 columns without explicit breakpoint rules for the grid itself. Gap: 16px desktop, 12px tablet/mobile.
- **Filter bar:** On desktop, search + category dropdown + sort dropdown are inline (flexbox row). On tablet, search is full-width, chips and sort are on the next row. On mobile, chips are horizontally scrollable (`overflow-x: auto`, `-webkit-overflow-scrolling: touch`, scrollbar hidden).
- **Category chips:** On desktop, all chips visible in a wrap. On mobile, horizontal scroll with fade-edge gradient hint (right edge) indicating more content.
- **Detail modal:** Desktop/tablet = centered dialog with overlay. Mobile = fullscreen (100vw × 100vh, scrollable body). Transition: 200ms ease-out (or instant with `prefers-reduced-motion`).
- **Create form:** Same responsive treatment as detail modal. On mobile, form fields stack vertically with full-width inputs. Image upload zone is full-width.
- **"Post Listing" CTA:** On desktop/tablet, it's a button in the header row. On mobile, it moves to the bottom nav `＋` (consistent with the feed composer pattern).
- **Listing cards:** Image aspect ratio stays 4:3 at all breakpoints. On mobile, cards are full-width single-column. Title, price, and seller info remain the same layout (vertical stack within the card).

### 7.3 Safe areas

Bottom nav respects `safe-area-inset-bottom` on notched devices (56px + env(safe-area-inset-bottom)). Modal close button respects `safe-area-inset-top` on mobile fullscreen overlays.

---

## 8. Open Questions

| # | Question | Options | Default if unresolved |
|---|---|---|---|
| OQ-1 | **Listing expiry policy** — should listings auto-expire after a period (30/60/90 days)? If so, can the seller renew? | A) Auto-expire after 90 days with renewal email/notification. B) No auto-expiry; seller must manually close/delete. C) Auto-expire after configurable period (default 90 days) with a "Renew" button in My Listings. | **C** — auto-expire at 90 days, renewal is one click from My Listings, notification 7 days before expiry. Prevents stale listings from cluttering browse. |
| OQ-2 | **Listing reports/flags** — can users flag listings as inappropriate? What's the moderation flow? | A) Flag button on every listing → admin queue (Phase 2). B) Flag button + community moderation (enough flags auto-hides). C) No flagging in v2.0; admin can delete from the DB. | **C** for v2.0 — admin-only deletion from the DB. Flag button + moderation queue is Phase 2 (simpler launch). |
| OQ-3 | **Favorites visibility** — should a user's favorited listings be visible to others (like a wishlist)? | A) Public favorites tab on profile. B) Private — only the user sees their favorites. C) Favorites are just a notification trigger (price drop, listing update) — no browse list. | **B** — private favorites. The user's favorites list is accessible from the Exchange screen (a "Favorites" toggle next to "My Listings") but not visible to visitors. Phase 2 can add public wishlists. |
| OQ-4 | **Location field** — should listings have an optional location (city, zip, "local pickup only")? FR-21 doesn't specify it, but marketplace UX expects it. | A) Add a free-text `location` field to `exchange_listings`. B) No location field; sellers include it in the description. C) Structured location (city/state/country) with autocomplete. | **A** — free-text `location` field (optional, max 100 chars). Shown on the card as a 📍 chip next to the category. Structured location is Phase 2. Requires a DB migration to add `location TEXT` to `exchange_listings`. |
| OQ-5 | **Price display for ISO listings** — ISO (In Search Of) listings have no meaningful price. How should the card render? | A) Hide the price line entirely for ISO listings. B) Show "ISO" in place of price. C) Show whatever the user typed (might be a budget range). | **A** — hide the price line for ISO category. The card title + category badge make the intent clear. If the user typed a budget in the price field, it shows. |
| OQ-6 | **Image limits and sizing** — max image count (8?), max file size, accepted formats, thumbnail generation. | A) Max 8 images, 10MB each, JPEG/PNG/WebP, server-side thumbnails at 400px/800px. B) Max 4 images, 5MB each, JPEG/PNG only. C) Follow the same limits as PostCard media upload. | **C** — align with PostCard's media upload limits (whatever the v1/v2 media pipeline supports). Server-side thumbnail generation at two sizes: card (400px wide) and detail (800px wide). |

**Resolved by this spec (not questions):** price is free-form text (FR-21, no currency validation); view count has no dedup (profile visitor counter philosophy); listings are AT Protocol records on the user's PDS (AD-01, AD-01c); marketplace is a listing board, not a storefront (no checkout, no escrow); muted sellers excluded from browse (FR-26); unauthenticated browse allowed; detail view is a modal, not a route (preserves scroll position).
