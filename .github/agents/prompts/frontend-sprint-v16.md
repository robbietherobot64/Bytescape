# Frontend Sprint v16 — Media Full-Width Fix & Repost Audience UX (Track B)

**Sprint Goal:** (1) Fix images and videos to fill the full width of the post card — this has been broken for 20+ rounds and is the #1 user frustration. (2) Redesign the repost flow to support multi-platform audience selection and quote posts.

**HEAD:** `d0edd1c` on `main`. **Build:** 0 tsc errors, 273 frontend tests, cargo check clean.

**Reference:** User feedback: "Images improperly sized/centered" (20+ attempts failed). "Repost should prompt which platforms to post to, let you tweak audience, add quotes if allowed."

---

## CRITICAL CONTEXT: Why Previous Attempts Failed

**20+ commits have tried to fix media full-width. ALL failed.** Here's what was tried and why it didn't work:

| Approach | Commits | Why it failed |
|----------|---------|---------------|
| Negative margins on `.post-media-grid` (`margin: 0 -10px; width: calc(100% + 20px)`) to break out of `.feed-entry-body` padding | `99ea3e7` → `5722061` | `contain: layout style` on parent elements clips the negative margins in WebKitGTK |
| Absolute positioning on `img`/`video` with `inset: 0` | `aabd3e7` | WebKitGTK doesn't resolve percentage heights through `aspect-ratio` containers correctly |
| Moving `<MediaGrid>` outside `.feed-entry-body` to be direct child of `.feed-entry-card` | `9803005` | User reports still not fixed (unclear why — CSS looks correct but WebKitGTK may not honor it) |
| Various `object-fit`, `width: 100%`, explicit pixel dimensions | multiple | WebKitGTK behavior diverges from Chrome/Firefox for nested containment contexts |

**Key insight from investigation:** The `.feed-entry-card` has `contain: layout style` and `overflow: hidden`. The `.feed-card-wrapper` also has `contain: layout style`. This double containment creates an environment where WebKitGTK's layout engine behaves unpredictably for percentage-based sizing.

---

## P0: Fix Media Full-Width (FRESH APPROACH REQUIRED)

**Priority:** P0 — user's #1 frustration, explicitly deferred to frontend agent.

### Current DOM Chain (after commit `9803005`)

```
.feed-card-wrapper          ← contain: layout style; content-visibility: auto (inline)
  └── .feed-item-renderer   ← no width rules
      └── .feed-entry-card  ← contain: layout style; overflow: hidden; border: 2px solid
          ├── .feed-entry-header
          ├── .feed-entry-body    ← padding: 8px 10px
          │   ├── .post-text
          │   ├── .link-preview-card
          │   └── quoted post
          ├── .post-media-grid    ← NOW direct child of card (commit 9803005)
          │   └── .media-item    ← position: relative; aspect-ratio: 16/9
          │       └── .media-item-inner  ← position: absolute; inset: 0
          │           └── <img>/<video>  ← position: absolute; inset: 0; object-fit: cover
          └── EngagementBar
```

### Investigation Method (DO THIS FIRST)

Before writing ANY CSS, you MUST inspect the actual computed widths at runtime:

```bash
# Launch with WebKitGTK inspector enabled:
WEBKIT_INSPECTOR_SERVER=127.0.0.1:9222 pnpm tauri dev
# Then open Chrome/Firefox and navigate to http://127.0.0.1:9222
# Inspect computed widths of: .feed-entry-card, .post-media-grid, .media-item, img
```

Document the actual pixel values at each level. The fix depends on understanding WHERE the width breaks down.

### Approach Options (try in order)

**Option A: Remove containment, verify media works, add back incrementally**
1. Temporarily remove `contain: layout style` from BOTH `.feed-card-wrapper` and `.feed-entry-card`
2. Test if media fills full width
3. If yes → the containment is the problem. Add back `contain: style` only (not `layout`) and test again
4. If `contain: style` works → use that. If not → find alternative perf optimization

**Option B: Use explicit pixel width via ResizeObserver**
1. Add a `ResizeObserver` on `.feed-entry-card` that measures its `clientWidth`
2. Pass the measured width as an inline style to `.post-media-grid`: `style={{ width: measuredWidth }}`
3. This bypasses all CSS percentage/containment issues — it's a direct pixel value

**Option C: Move media grid to absolute positioning within the card**
1. Make `.feed-entry-card` `position: relative` (it already has `overflow: hidden`)
2. Place `.post-media-grid` with `position: absolute; left: 0; right: 0;`
3. Use a spacer `<div>` with matching `aspect-ratio` to maintain card height

**Option D: Nuclear — remove all containment from feed cards permanently**
If containment is the issue and `contain: style` alone doesn't work:
1. Remove `contain: layout style` from `.feed-entry-card` and `.feed-card-wrapper`
2. Remove `will-change: transform` from `.feed-entry-card`
3. Keep `content-visibility: auto` on `.feed-card-wrapper` (this provides most of the perf benefit without `contain: layout`)
4. Test scrolling performance — if acceptable, ship it

### Key CSS Files

| File | Lines | What's there |
|------|-------|-------------|
| `src/App.css` | 1303-1420 | `.post-media-grid`, `.media-item`, `.media-item-inner`, `img`/`video` rules |
| `src/App.css` | 1148-1170 | `.feed-entry-card` containment, overflow, border |
| `src/screens/FeedScreen.css` | 96-110 | `.feed-card-wrapper` containment, image cap rules |
| `src/App.css` | 3884-3888 | `@media (min-width: 1025px)` responsive overrides |

### Key Component Files

| File | What it does |
|------|-------------|
| `src/components/FeedEntryCard.tsx` | Card renderer — `<MediaGrid>` is now a direct child of `.feed-entry-card` (after `.feed-entry-body`) |
| `src/components/feed/MediaGrid.tsx` | Grid renderer — uses `.media-item-inner` wrapper |
| `src/components/BlobImage.tsx` | Renders bare `<img>`, no className/style from MediaGrid |
| `src/components/feed/VirtualizedFeedList.tsx` | Sets `content-visibility: auto` + `containIntrinsicSize` inline on `.feed-card-wrapper` |

### Acceptance Criteria

1. Single-image posts: image fills the full width of the card (edge to edge, no gap on either side)
2. Multi-image grids (2, 3, 4): grid fills full card width, cells are evenly distributed
3. Videos: fill full card width in 16:9 aspect ratio
4. No horizontal overflow or scrollbar on any card
5. Works on the geocities theme (default) — verify on at least one other theme

---

## P1: Repost Audience Selection UX

**Priority:** P1 — new feature request.

### Current State

The repost flow currently works like this:
- Click repost button → if already reposted, undo immediately. If not, show dropdown: "Repost" / "Quote Repost"
- "Repost" → immediate repost to the SAME platform as the original post (Bluesky `repost_post` or Mastodon `mastodon_reblog`)
- "Quote Repost" → opens `QuoteRepostModal` for Bluesky quote posts only

**Backend commands that already exist:**
- `repost_post` / `unrepost_post` — Bluesky
- `mastodon_reblog` / `mastodon_unreblog` — Mastodon

### What the User Wants

When clicking repost, instead of immediately reposting to the same platform:
1. **Show an audience/platform selection modal** — let user choose which platforms to repost to
2. **Allow adding a quote/comment** if the target platform supports it (Bluesky supports quote posts, Mastodon supports quoting on some instances)
3. **Show which platforms the user has linked accounts for** — only enabled platforms are selectable

### Implementation Plan

#### P1.1: Create `RepostModal` Component

**File:** `src/components/RepostModal.tsx` (new)

This replaces the current inline dropdown menu. Design:

```
┌─────────────────────────────────┐
│  Repost                      ✕  │
├─────────────────────────────────┤
│                                 │
│  Share to:                      │
│  ☑ Bluesky (@handle.bsky)      │
│  ☑ Mastodon (@handle@instance)  │
│  ☐ Lemmy (not connected)       │
│                                 │
│  ┌─────────────────────────┐    │
│  │ Add a quote (optional)  │    │
│  │                         │    │
│  └─────────────────────────┘    │
│                                 │
│  [ Cancel ]       [ Repost ▸ ]  │
└─────────────────────────────────┘
```

Key behaviors:
- Pre-check the platform the original post came from
- Show all platforms the user has linked (read from app state / `invoke('get_connected_accounts')` or similar)
- Disabled/greyed out platforms the user hasn't connected, with "(not connected)" label
- Optional quote text field — when filled, uses quote-repost for platforms that support it
- "Repost" button sends to all checked platforms sequentially

#### P1.2: Wire into FeedEntryCard

**File:** `src/components/FeedEntryCard.tsx`

Replace the current `handleRepostClick` logic:

```typescript
// Current: shows inline dropdown (Repost / Quote Repost)
// New: always opens RepostModal, which handles platform selection + optional quote

const handleRepostClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!isInteractive) return;
    
    if (reposted) {
        // Undo — still immediate for the original platform
        handleDirectRepost(e);
    } else {
        // Open the new RepostModal
        setRepostModalOpen(true);
    }
};
```

#### P1.3: AudienceDrawer Integration

The `AudienceDrawer` component already exists and handles "share to platforms" for the Share button. The RepostModal can reuse its platform-listing logic. Check `src/components/AudienceDrawer.tsx` for the pattern.

### Acceptance Criteria

1. Clicking repost on ANY post (Bluesky or Mastodon) opens the RepostModal
2. User can select one or more platforms to repost to
3. Optional quote text is supported (uses quote-repost on Bluesky, regular boost + reply on Mastodon if needed)
4. Unlinked platforms shown but disabled
5. Undo repost still works (immediate, no modal)

---

## P2: Source Filter Re-Fetch

**Priority:** P2 — minor UX issue.

### Problem

The source filter dropdown (platform selector in the feed) is **client-side only**. Changing the source filter does NOT trigger a re-fetch — it just filters the existing cache. If the cache was populated with mixed sources, filtering to "Bluesky" can show very few or zero results.

### Fix

**File:** `src/hooks/useUnifiedFeed.ts` (line ~473)

Add `effectiveSourceFilter` to the `filterKeyRef` dependencies:

```typescript
// Current:
const key = JSON.stringify({ f: effectiveFeedUris, t: effectiveTags });

// Fixed:
const key = JSON.stringify({ f: effectiveFeedUris, t: effectiveTags, s: effectiveSourceFilter });
```

This ensures changing the source filter clears the stale cache and re-fetches.

---

## Verification

1. **Media width:** Load the feed, scroll through 10+ posts with images. Every image fills the full card width with no gaps.
2. **Repost modal:** Click repost on a Bluesky post → modal opens with platform checkboxes. Click repost on a Mastodon post → same modal.
3. **Source filter:** Select "Bluesky" in source dropdown → feed re-fetches (not just client-side filter).
4. **Build:** `npx tsc --noEmit` — 0 errors. `pnpm test -- --run` — all pass. `cargo check` — clean.

---

## Files Expected to be Modified

| File | Changes |
|------|---------|
| `src/App.css` | Media grid CSS — depends on which approach fixes it |
| `src/screens/FeedScreen.css` | Possibly `.feed-card-wrapper` containment changes |
| `src/components/FeedEntryCard.tsx` | Wire RepostModal, possibly MediaGrid DOM changes |
| `src/components/feed/MediaGrid.tsx` | Possibly add ResizeObserver or inline width |
| `src/components/feed/VirtualizedFeedList.tsx` | Possibly modify containment inline styles |
| `src/components/RepostModal.tsx` | **NEW** — platform selection + quote repost modal |
| `src/hooks/useUnifiedFeed.ts` | Add source filter to re-fetch trigger |
