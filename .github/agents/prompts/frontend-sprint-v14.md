# Frontend Sprint v14 — Feed Stability Overhaul, Image Fixes, Import UX & Groups Feed (Track B)

**Sprint Goal:** Fix the **CRITICAL feed stability issues** that have persisted for 2 sprints (image blink, gaps between posts, inconsistent sizing), fix Instagram images not loading on the feed, fix profile image previews, address small UX items from user feedback, and wire up the Groups feed tab.

**Context:** Sprint v13 delivered UX improvements from user feedback (3-column profile layout, AccountSwitcher, import animation, Connect Account button, platform abbreviations) but **did NOT address the P0 critical feed stability issues** (image blink, gaps, sizing). These have been reported since Sprint v12 (2026-03-28) and confirmed STILL CRITICAL in the latest user feedback (2026-04-03). The user says: "needs a fundamental ground up redesign of how the feed UI functions, so it's not so twitchy and is more stable overall."

The Groups feed tab was also scoped for v13 but not delivered — the Forums screen still shows only a mock directory with no federated content.

**HEAD:** `7ba4c4a` on `main`. **Build:** 0 tsc errors, 272 frontend tests pass.

---

## P0: Feed Stability Overhaul (🔥🔥🔥 CRITICAL — 2 SPRINTS OVERDUE)

**Priority:** 🔥🔥🔥 ABSOLUTE HIGHEST — the user has reported these across multiple sprints and explicitly says prior fixes were insufficient. No other work should begin until the feed feels stable during normal scrolling.

### 0.1 Image/UI Blink on Scroll

**Symptoms:** Images and overall UI "blink" when scrolling — go black for a split second when an image is hovered over. One part of the UI is affected by interactivity of another part. Prior fixes (synchronous cache init in `useBlobUrl`, removed hover effects, removed dark placeholder) were insufficient.

**Root cause investigation (try ALL of these):**

1. **CSS containment.** Add `contain: content` (or `contain: layout paint`) to every feed item wrapper. Without containment, a reflow in one item (e.g., hover state change) causes siblings to repaint. This is likely the #1 cause of "one part affecting another."

2. **VirtualizedFeedList re-rendering.** Check if `@tanstack/react-virtual` is unmounting/remounting items during scroll. When an item unmounts, its `BlobImage` component unmounts, clearing the blob URL from state. When it remounts, the blob URL must be re-resolved → black frame. **Fix:** Ensure the virtualizer has `overscan: 5` (render 5 extra items above/below viewport) and that `estimateSize` returns stable values.

3. **BlobImage lifecycle.** Check `useBlobUrl` — when the component unmounts, does it revoke the object URL? If yes, and the virtualizer remounts the same item, the URL is gone. **Fix:** Use a shared, persistent blob URL cache (module-level `Map`) that survives component unmount/remount cycles. Never revoke URLs for items that are in the current feed.

4. **CSS transitions on images.** Search for any `transition`, `animation`, `opacity`, or `transform` CSS on image containers or `.feed-entry-card`. Any transition triggered by scroll position changes could cause flicker. **Remove all transitions** from feed items during scroll.

5. **Hover effect isolation.** Even though prior hover effects were "removed," check for any remaining `:hover` rules on `.feed-entry-card` or parent containers that cause layout shifts. Hover on a card should NEVER change dimensions, padding, margin, or box-shadow in a way that affects neighboring items.

6. **`will-change: transform`** on feed item containers to promote them to their own compositor layer. This prevents repaints from propagating across items.

**Verification criteria:**
- Scroll the feed for 60 seconds at varying speeds — ZERO visual flicker
- Hover over images while scrolling — no black frames, no blink
- Hover on one card must NOT cause any visual change in adjacent cards
- Open DevTools → Rendering → Paint Flashing — feed items should NOT repaint when adjacent items are interacted with

### 0.2 Feed Gap & Image Sizing Inconsistency

**Symptoms:** Random extra whitespace between feed posts. Images sometimes render correctly, other times they're cut off and the post shrinks to 1/3 expected height.

**Root cause investigation:**

1. **VirtualizedFeedList `estimateSize`.** If it returns a fixed estimate (e.g., 400px) and the actual rendered size is 600px for image posts or 150px for text-only posts, the virtualizer creates gaps during scroll as it reconciles estimated vs. actual sizes. **Fix:** Use a `measureElement` callback ref to measure actual sizes, and cache measured sizes per item ID so they're stable on re-render.

2. **Image container pre-sizing.** Check `FeedEntryCard` image containers — do they have a fixed `aspect-ratio` or `min-height` BEFORE the blob URL resolves? If the container has 0 height before the image loads, then expands to 300px after load, the virtualizer's size estimate is wrong → gaps. **Fix:** Set `aspect-ratio: 4/3` and `min-height: 200px` on all image containers as a fallback before image load.

3. **BlobImage loading state.** Does `BlobImage` render an empty `<div>` with 0 height while the blob is resolving? This would cause the 1/3 height issue. **Fix:** BlobImage should always render a placeholder with the expected image dimensions (use `width`/`height` from the item's media metadata if available, else a standard aspect ratio).

4. **CSS `overflow: hidden` clipping.** Check if any parent container has `overflow: hidden` that clips images without a corresponding `min-height`. This could cause the "cut off" appearance.

5. **Re-measurement after image load.** After `BlobImage` loads and the container resizes, the virtualizer needs to be notified. Call `virtualizer.measureElement(ref)` after image `onLoad` fires.

**Verification criteria:**
- Scroll through 100+ posts — no unexplained gaps between posts
- All images render at their full expected size, never clipped to 1/3 height
- Text-only posts are compact, image posts are appropriately sized
- Window resize doesn't break feed layout

### 0.3 CSS Architecture Fixes

Apply these structural CSS changes to support the feed stability overhaul:

```css
/* Feed item containment — prevents cross-item reflow */
.feed-entry-card {
  contain: content;
  will-change: transform;
}

/* Image container pre-sizing */
.feed-entry-card .media-container {
  aspect-ratio: 4/3;
  min-height: 100px;
  overflow: hidden;
}

/* Remove ALL transitions from feed items during scroll */
.feed-entry-card,
.feed-entry-card * {
  transition: none !important;
}

/* If hover effects are needed, restrict to non-layout properties */
.feed-entry-card:hover {
  /* OK: box-shadow, outline */
  /* NOT OK: padding, margin, height, width, transform */
}
```

**Key files to modify:**
- `src/components/feed/VirtualizedFeedList.tsx` — overscan, estimateSize, measureElement
- `src/components/feed/FeedEntryCard.tsx` — image container pre-sizing
- `src/hooks/useBlobUrl.ts` — persistent blob URL cache
- `src/App.css` — CSS containment, transition removal

---

## P1: Image Loading Fixes (🔥 Critical)

**Priority:** 🔥 Critical — user-visible broken images make the app feel broken.

### 1.1 Instagram Images Not Loading on Feed (UF.23)

**Symptoms:** Instagram images do not load on the feed — just broken/empty placeholders. They load fine in the vault.

**Investigation path:**
1. Compare how the vault renders Instagram images vs. how the feed renders them
2. Check if Instagram items have `media_blobs` with valid blob hashes in the feed query results
3. Check if `useBlobUrl` handles Instagram source items differently or if the blob hash format differs
4. Check if the feed's `BlobImage` component is being called with the correct props for Instagram items
5. Try logging the blob URL resolution for an Instagram item in both contexts

**Fix:** Make blob resolution source-agnostic. If the vault can resolve it, the feed should too.

### 1.2 Profile Page Image Previews Broken

**Symptoms:** Image previews on the profile page show only a small box with an X — no image loads.

**Investigation path:**
1. Open DevTools → Network tab → filter for blob/image requests when viewing profile
2. Check if `BlobImage` is receiving the correct `blobHash` prop on the profile page
3. Check if the profile page's data query returns `media_blobs` / `media_paths` correctly
4. Check for CSS issues — is the image container styled with 0 width/height?

**Fix:** Ensure profile page images use the same `BlobImage` component and blob resolution path as the feed.

**Key files:**
- `src/screens/ProfileScreen.tsx`
- `src/hooks/useBlobUrl.ts`
- `src/components/feed/FeedEntryCard.tsx` (reference working implementation)

---

## P2: Small UX Fixes (Medium)

**Priority:** Medium — user-reported friction items that are quick to fix.

### 2.1 Username Suggestion Fix (UF.22)

**Symptoms:** Initial data import username suggestion concatenates the full handle + domain name together (e.g., `@usernamedomainnamesuffix`) rather than just the username portion.

**Fix:** In `ImportScreen.tsx`, find the username suggestion logic and modify it to:
1. Take the raw handle/username string
2. Split on `.` (period)
3. Use only the first segment (before the first period) as the suggestion
4. Strip any leading `@`

Example: `@user.bsky.social` → `user`

### 2.2 Import Button Styling (UF.12 remainder)

**Symptoms:** User wants the import button to be "big and green so it stands out."

**Fix:** In `ImportScreen.tsx`, style the main import/action button:
```css
.import-button-primary {
  background: #22c55e; /* bright green */
  color: white;
  font-size: 1.2rem;
  padding: 14px 32px;
  border-radius: 8px;
  border: none;
  cursor: pointer;
  font-weight: 600;
}
.import-button-primary:hover {
  background: #16a34a;
}
```

**Key files:**
- `src/screens/ImportScreen.tsx`
- `src/App.css` (button styles)

---

## P3: Wire Up Groups Feed Tab (High — Carried from v13)

**Priority:** High — the Groups feed tab exists in types/labels but the button never renders and data isn't connected. User feedback: "Forums - no content currently federated, just dummy directory."

### 3.1 Render the Tab Button

In `FeedScreen.tsx`, the `'groups'` tab value exists in `FeedTab` type and `FEED_TAB_LABELS` but is filtered out or not rendered.

**Fix:** Ensure the "Groups" tab button appears in the tab bar. Use the same styling and active-state logic as existing tabs.

### 3.2 Data Source

When "Groups" tab is active:
1. Call `invoke<BytescapeItem[]>('get_groups_feed', { limit: 50 })` (or use `useGroupsData` hook)
2. Display results using `VirtualizedFeedList`
3. Set `USE_MOCK = false` in `useGroupsData` since backend commands exist

### 3.3 Empty State

Verify the existing empty state shows:
- "No group subscriptions yet."
- Clickable "Browse Forums →" link navigating to ForumScreen

### 3.4 useGroupsData Cleanup

- Remove mock data path from `useGroupsData.ts`
- Replace `any[]` with proper types
- Add try/catch with user-friendly error messages on all `invoke()` calls

**Key files:**
- `src/screens/FeedScreen.tsx`
- `src/hooks/useGroupsData.ts`
- `src/store/index.ts`

---

## Constraints

- **Track boundary:** Only modify files in `src/`. Do NOT touch `src-tauri/src/` (backend files).
- **Build verification:** Run `npx tsc --noEmit` + `pnpm test -- --run` before EVERY commit. Zero errors, zero test regressions.
- **Shared files:** Before editing `types/index.ts`, `useUnifiedFeed.ts`, `App.tsx`, `store/index.ts`, run `git log --oneline -5 -- <filepath>` to check for recent agent modifications.
- **Commit format:** `feat(track-b): description` or `fix(track-b): description`.
- **No TODOs in code.** Implement fully.
- **P0 FIRST.** Do not begin P1/P2/P3 until the feed stability overhaul is complete and verified. The user has called this out for 2 sprints — it is the #1 priority.
- **Update `progress.md`** with deliverables when done.

---

## What This Sprint Does NOT Include (Future)

- **Share-to-Group menu** (sprint v15+) — depends on verified Groups feed
- **Group badge on feed cards** (sprint v15+)
- **Moderation UI** (sprint v15+) — depends on backend moderation commands
- **Frontend structural refactoring** (sprint v15+) — god-component splitting, store consolidation
- **Social module redesign** (sprint v15+)
- **Auto-login to linked platforms** (sprint v15+) — UF.19
- **Remote community browsing** (sprint v15+)
