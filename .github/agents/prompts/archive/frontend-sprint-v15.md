# Frontend Sprint v15 — Feed Pipeline Rebuild (Track B)

**Sprint Goal:** Rebuild the feed's content retrieval and display pipeline from scratch per TECHNICAL_SPEC.md §6.2. This is a **4-phase build** with a mandatory memory gate after each phase. No phase begins until the previous phase passes its memory test.

**HEAD:** `75f6c01` on `main`. **Build:** 0 tsc errors, 272 frontend tests, cargo check clean.

**Reference:** TECHNICAL_SPEC.md §6.2 "Ground-Up Rebuild Specification (v2, 2026-04-04)"

---

## What We're Rebuilding vs What Stays

Read this section CAREFULLY. Do NOT delete or modify files in the "stays" list.

### STAYS (do not touch):
| File | Reason |
|------|--------|
| `src/screens/FeedScreen.tsx` | Three-column layout shell, tab bar, ComposeBox integration, pull-to-refresh, filter toolbar. **Will be modified** to wire new hook, but NOT deleted. |
| `src/screens/FeedScreen.css` | All feed styling stays. |
| `src/components/FeedEntryCard.tsx` | Individual card rendering, interactions, quote posts. Working correctly. |
| `src/components/feed/EngagementBar.tsx` | Like/reply/repost buttons. Working correctly. |
| `src/components/feed/GalleryOverlay.tsx` | Fullscreen image gallery. Working correctly. |
| `src/components/feed/MediaGrid.tsx` | Image grid layout within cards. Working correctly. |
| `src/components/feed/ThreadReplies.tsx` | Thread display. Working correctly. |
| `src/components/feed/helpers.ts` | Feed utility functions. Working correctly. |
| `src/components/feed/icons.tsx` | Feed icon components. Working correctly. |
| `src/components/feed/index.ts` | Barrel export. Working correctly. |
| `src/components/FeedRightPanel.tsx` | Discovery sidebar. Working correctly. |
| `src/components/FriendsPanel.tsx` | Friends sidebar. Working correctly. |
| `src/components/ComposeBox.tsx` | Post composer. Working correctly. |
| `src/components/Sidebar.tsx` | Navigation sidebar. Working correctly. |
| `src/components/BlobImage.tsx` | Used by vault/profile. Not feed-specific. |
| `src/components/VideoPlayer.tsx` | Used by vault viewer. Keep (with memory fixes from v14). |
| `src/utils/adapters.ts` | `adaptBlueskyPost`, `adaptBytescapeItem`, `deduplicateFeedEntries`. **Will be imported by new hook.** |
| `src/utils/content.ts` | Content utilities. Not feed-specific. |
| `src/types/index.ts` | Type definitions. Will add new types, not remove existing. |
| `src/hooks/useBlobUrl.ts` | Blob URL resolution. Used by vault. Keep. |

### BEING REBUILT (replace internals, keep file locations):
| File | What Changes |
|------|-------------|
| `src/hooks/useUnifiedFeed.ts` | **Complete rewrite.** New bounded-state hook with cursor pagination, 150-item cap, per-tab isolation, source toggles. Same export name so FeedScreen import doesn't break. |
| `src/components/feed/VirtualizedFeedList.tsx` | **Complete rewrite.** Replace @tanstack/react-virtual with CSS `content-visibility: auto` approach. Fallback to @tanstack/react-virtual if content-visibility insufficient. |
| `src/store/index.ts` | **Modify feed cache store only.** Replace `FeedCacheStore` internals with bounded cache (150-item cap per tab, 4-tab max). Do NOT modify NavStore, ThemeStore, or other stores. |

### BEING DELETED:
| File | Reason |
|------|--------|
| `src/utils/prefetchBlobUrls.ts` | Bulk prefetch is the opposite of lazy loading. New pipeline resolves blobs on-demand for viewport ± 3 only. |
| `src/utils/feedAnalytics.ts` | Duplicate image filtering. Will be replaced by dedup in the new hook. |
| `src/utils/VideoPlaybackManager.ts` | Singleton video lifecycle manager. Will be replaced by IntersectionObserver-based cleanup in the new rendering pipeline. |

---

## Phase 1: Data Hook + State Foundation

**Goal:** Replace `useUnifiedFeed` with a new implementation that enforces bounded state, cursor pagination, and per-tab isolation. No rendering changes yet — wire the new hook into the existing FeedScreen/VirtualizedFeedList.

### Phase 1.1: Define new types

**File:** `src/types/index.ts` — add these types (do not remove existing types):

```typescript
/** Backend response shape from all feed commands (matches Rust FeedPage). */
export interface FeedPage {
  items: FeedEntry[];
  cursor: string | null;
  has_more: boolean;
}

/** Per-tab state container. */
export interface FeedTabState {
  entries: FeedEntry[];
  cursor: string | null;
  has_more: boolean;
  fetchedAt: number;         // epoch ms
  scrollOffset: number;      // preserved scroll position
  sourceToggles: Record<string, boolean>;  // Following tab: per-platform toggles
}

/** Linked platform info for source toggles. */
export interface LinkedFeedSource {
  id: string;       // "bluesky", "mastodon", "pixelfed", "peertube"
  label: string;    // "Bluesky", "Mastodon", etc.
  handle: string;   // "@user.bsky.social"
  connected: boolean;
}
```

### Phase 1.2: Update FeedTab type

**File:** `src/hooks/useUnifiedFeed.ts` (or the new replacement)

The 'subscribed' tab stays as-is — it's a good catchall for feeds, hashtags, lists, etc. (anything that isn't a direct follow). Remove 'you' tab. Add 'rss' as a placeholder.

```typescript
// Remove 'you' tab, add 'rss' placeholder
export type FeedTab = 'all' | 'following' | 'subscribed' | 'groups' | 'rss';

export const FEED_TAB_LABELS: Record<FeedTab, string> = {
  all: 'All',
  following: 'Following',
  subscribed: 'Subscribed',
  groups: 'Groups',
  rss: 'RSS',
};
```

**Tab definitions:**
- **all** = Combined view of Following + Subscribed + Groups + RSS. NOT a global firehose — only YOUR connections/subscriptions.
- **following** = Accounts you follow across all linked platforms (Bluesky, Mastodon, Pixelfed, PeerTube). Source toggles filter by platform.
- **subscribed** = Bluesky custom feeds, Mastodon lists, subscribed hashtags (Bluesky + Mastodon). Anything you subscribe to that isn't a direct follow.
- **groups** = Lemmy communities, Piefed groups, Bytescape local groups. Future placeholder for now.
- **rss** = RSS/Atom feed subscriptions. Future placeholder — render empty state with "RSS feeds coming soon" message.

**IMPORTANT:** Remove 'you' tab references. Run `grep -rn "'you'" src/` to find and remove 'you' tab references. The 'subscribed' tab is NOT being renamed — keep all existing 'subscribed' references.

### Phase 1.3: Rewrite `useUnifiedFeed` hook

**File:** `src/hooks/useUnifiedFeed.ts` — complete rewrite.

**Core architecture:**

```typescript
const MAX_ENTRIES = 150;       // Hard cap per tab
const DEFAULT_PAGE_SIZE = 30;  // Configurable via settings

interface UseUnifiedFeedOptions {
  auth: AuthStatus | null;
  activeTab: FeedTab;
  selectedFeedUris: string[];
  pageSize?: number;
}

interface UseUnifiedFeedReturn {
  entries: FeedEntry[];
  isLoading: boolean;
  error: string | null;
  hasMore: boolean;
  loadMore: () => Promise<void>;
  refresh: () => Promise<void>;
  isLoadingMore: boolean;
  // Source toggles (Following tab)
  linkedSources: LinkedFeedSource[];
  sourceToggles: Record<string, boolean>;
  setSourceToggle: (sourceId: string, enabled: boolean) => void;
  // Metadata
  savedFeeds: GeneratorView[];
}
```

**State management rules:**
1. Each tab has its own `FeedTabState` in Zustand (not local state).
2. Only the active tab fetches data. Inactive tabs retain their state but do NOT fetch or auto-refresh.
3. `entries` array is hard-capped at `MAX_ENTRIES` (150). When `loadMore` is called and total would exceed 150, evict the oldest page (first `pageSize` entries).
4. Cursor is opaque — the hook passes it to the backend and receives the next cursor. No client-side offset tracking.
5. Auto-refresh: every 60 seconds, replace the current first page (do NOT grow state). Configurable: 30s / 60s / 5m / off.

**Data flow per tab:**

| Tab | Backend Command | Params |
|-----|----------------|--------|
| `all` | `get_timeline_unified` | `{ limit, cursor, source_filter: null }` |
| `following` | `get_timeline` | `{ limit, cursor, source_filter: activeToggles }` |
| `subscribed` | `get_unified_feed` | `{ limit, cursor, feed_uris: selectedFeedUris }` |
| `groups` | `get_groups_feed` | `{ limit, cursor }` |
| `rss` | (future) | Render empty placeholder: "RSS feeds coming soon" |

**Response handling:**
```typescript
const page = await invoke<FeedPage>('get_timeline', { limit: pageSize, cursor });
const adapted = page.items; // Backend now returns FeedEntry-compatible items
const deduped = deduplicateFeedEntries([...currentEntries, ...adapted]);
const capped = deduped.slice(-MAX_ENTRIES); // Keep newest, evict oldest
```

**Stale-while-revalidate:**
- On tab switch, if cached state exists and is <2 min old, render immediately + background refresh.
- If >2 min old, show loading state + foreground fetch.

### Phase 1.4: Update Zustand feed cache store

**File:** `src/store/index.ts` — modify `FeedCacheStore` only.

Replace the unbounded `cache: Record<string, FeedCacheEntry>` with a bounded store:

```typescript
interface FeedCacheState {
  tabs: Record<FeedTab, FeedTabState>;
  
  // Actions
  setTabEntries: (tab: FeedTab, entries: FeedEntry[], cursor: string | null, hasMore: boolean) => void;
  appendTabEntries: (tab: FeedTab, entries: FeedEntry[], cursor: string | null, hasMore: boolean) => void;
  evictOldest: (tab: FeedTab, pageSize: number) => void;
  getTabState: (tab: FeedTab) => FeedTabState;
  clearTab: (tab: FeedTab) => void;
  clearAllTabs: () => void;
  setScrollOffset: (tab: FeedTab, offset: number) => void;
  setSourceToggles: (tab: FeedTab, toggles: Record<string, boolean>) => void;
  
  // Metadata caches (bounded)
  savedFeeds: GeneratorView[];
  setSavedFeeds: (feeds: GeneratorView[]) => void;
}
```

**Bounded invariants:**
- Each tab's `entries` array: max 150 items. `appendTabEntries` enforces this by evicting oldest if over cap.
- Total tabs: 5 (all, following, subscribed, groups, rss). No dynamic tab keys.
- `savedFeeds`: max 100 items.

### Phase 1.5: Wire into FeedScreen

**File:** `src/screens/FeedScreen.tsx` — modify, do not rewrite.

1. Replace `useUnifiedFeed` import to use the new hook (same file path, new internals).
2. Remove `archiveSourceFilter`, `selectedHashtagsFilter`, `hashtagInput` state (moved into hook/store).
3. Replace `handleLoadMore` call with `loadMore()` from new hook.
4. Keep: ComposeBox, pull-to-refresh, tab bar, filter toolbar, three-column layout.
5. Update tab rendering to use new `FeedTab` type (remove 'you' tab, add 'rss' placeholder tab).
6. Wire source toggles into Following tab header.

### Phase 1 Memory Gate 🧪

**STOP. Do not proceed to Phase 2 until this passes.**

Test procedure:
1. `npx tsc --noEmit` — 0 errors
2. `pnpm test -- --run` — all tests pass (update/remove tests that reference deleted code)
3. Launch app with `pnpm tauri dev`
4. Switch between all 5 tabs (All, Following, Subscribed, Groups, RSS)
5. Load 3 pages on the All tab (click Load More twice)
6. Check: `entries.length` must be ≤150 at all times
7. Check: RSS memory must be <300 MB (no media loaded yet, just text data)
8. Check: tab switch renders cached data instantly, then background refreshes

**Commit:** `feat(track-b): rebuild feed data hook with bounded state and cursor pagination`

---

## Phase 2: Rendering Pipeline

**Goal:** Replace `VirtualizedFeedList` internals with a memory-safe rendering approach. CSS `content-visibility: auto` preferred, @tanstack/react-virtual as fallback.

### Phase 2.1: Implement CSS content-visibility approach

**File:** `src/components/feed/VirtualizedFeedList.tsx` — rewrite internals.

**New approach — CSS native virtualization:**

```tsx
interface FeedListProps {
  entries: FeedEntry[];
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  onLoadMore?: () => void;
  hasMore: boolean;
  isLoadingMore: boolean;
  // Source toggles for Following tab
  sourceToggleBar?: React.ReactNode;
}

function FeedList({ entries, scrollContainerRef, onLoadMore, hasMore, isLoadingMore, sourceToggleBar }: FeedListProps) {
  return (
    <div className="feed-list">
      {sourceToggleBar}
      {entries.map(entry => (
        <FeedCardWrapper key={entry.reposted_by ? `repost:${entry.reposted_by}:${entry.id}` : entry.id} entry={entry} />
      ))}
      {isLoadingMore && <LoadingSpinner />}
      {hasMore && !isLoadingMore && (
        <button className="load-more-btn" onClick={onLoadMore}>Load More</button>
      )}
    </div>
  );
}

/** Wrapper that applies content-visibility: auto for native browser virtualization */
const FeedCardWrapper = React.memo(({ entry }: { entry: FeedEntry }) => (
  <div
    className="feed-card-wrapper"
    style={{
      contentVisibility: 'auto',
      containIntrinsicSize: 'auto 400px',  // estimated height
      contain: 'content',
    }}
  >
    <FeedEntryCard entry={entry} />
  </div>
));
```

**CSS additions** (in `FeedScreen.css`):
```css
.feed-card-wrapper {
  content-visibility: auto;
  contain-intrinsic-size: auto 400px;
  contain: content;
}
```

**Why this approach:**
- `content-visibility: auto` tells WebKitGTK to skip layout/paint/decode for off-screen cards
- This means off-screen `<img>` tags won't be decoded into RGBA bitmaps (the primary memory consumer)
- No JavaScript virtualization complexity (no height estimation, no scroll jump bugs)
- Cards stay in the DOM (preserving scroll position) but their rendering is browser-managed

**Key difference from old approach:** The old `VirtualizedFeedList` used `@tanstack/react-virtual` which unmounted/remounted components as they scrolled in/out. The new approach keeps all 150 items in the DOM but lets the browser skip rendering for off-screen ones. This is simpler and avoids the "flash of content" bug on fast scrolling.

### Phase 2.2: Remove blob prefetch

**Delete:** `src/utils/prefetchBlobUrls.ts`

The old `VirtualizedFeedList` called `prefetchBlobUrls` for visible items ± 5. The new approach does NOT prefetch. Instead, blob URLs are resolved lazily by `useBlobUrl` inside `BlobImage` when the card enters the viewport (triggered by `content-visibility: auto` making the card visible).

**Update any imports:** Search for `prefetchBlobUrls` and remove all references.

### Phase 2.3: Remove VideoPlaybackManager

**Delete:** `src/utils/VideoPlaybackManager.ts`

The old singleton tracked `<video>` elements globally. The new approach uses IntersectionObserver inside `VideoPlayer.tsx` (or the feed card wrapper) to pause/destroy video pipelines when cards scroll off-screen.

**Update imports:** Remove all `VideoPlaybackManager` imports from:
- `src/screens/FeedScreen.tsx`
- `src/components/FeedEntryCard.tsx`
- `src/components/VideoPlayer.tsx`
- Any other file

**Replace with IntersectionObserver in VideoPlayer.tsx:**
```typescript
useEffect(() => {
  if (!containerRef.current) return;
  const observer = new IntersectionObserver(
    ([entry]) => {
      if (!entry.isIntersecting && videoRef.current) {
        videoRef.current.pause();
        videoRef.current.removeAttribute('src');
        videoRef.current.load(); // Release GStreamer pipeline
      }
    },
    { threshold: 0 }
  );
  observer.observe(containerRef.current);
  return () => observer.disconnect();
}, []);
```

### Phase 2.4: Delete feedAnalytics

**Delete:** `src/utils/feedAnalytics.ts`

The `filterFacebookDuplicateImagePosts` function is no longer needed — deduplication is handled by `deduplicateFeedEntries` in the new hook.

**Update imports:** Remove from `FeedScreen.tsx`.

### Phase 2.5: Limit simultaneous decoded images

**Goal:** Even with `content-visibility: auto`, the browser may keep some off-screen images decoded. Add a CSS/JS safeguard:

In `FeedScreen.css`, add:
```css
/* Limit image decode memory by constraining maximum rendered dimensions */
.feed-card-wrapper img {
  max-width: 600px;
  max-height: 400px;
  object-fit: contain;
}
```

This ensures no feed image is decoded at >600×400 pixels = ~960KB per image (vs 4000×3000 = ~48MB for fullsize).

### Phase 2 Memory Gate 🧪

**STOP. Do not proceed to Phase 3 until this passes.**

Test procedure:
1. `npx tsc --noEmit` — 0 errors
2. `pnpm test -- --run` — all tests pass
3. Launch app, load 150 items (5 pages × 30)
4. Scroll through entire feed slowly
5. **Memory test:** RSS must stay under **500 MB** with 150 items loaded (mixed content including images)
6. **Image decode test:** Open DevTools → check `<img>` elements. Off-screen images should NOT be in the decoded bitmap cache (verify via WebKitGTK inspector if available, or observe memory staying flat during scroll)
7. **Scroll performance:** 60fps scrolling, no jank, no layout thrashing
8. Verify `prefetchBlobUrls.ts` and `VideoPlaybackManager.ts` are deleted
9. Verify no `VideoPlaybackManager` imports remain anywhere

**Commit:** `feat(track-b): replace virtualized feed list with content-visibility rendering`

---

## Phase 3: Media Pipeline

**Goal:** Ensure all media (images, video, blob-resolved items) loads lazily and within memory constraints.

### Phase 3.1: Lazy image loading enforcement

Verify that ALL `<img>` tags in feed cards have `loading="lazy"` and `decoding="async"`:

```tsx
<img
  src={thumbnailUrl}
  loading="lazy"
  decoding="async"
  alt={altText}
  style={{ maxWidth: 600, maxHeight: 400, objectFit: 'contain' }}
/>
```

**Files to check:**
- `src/components/feed/MediaGrid.tsx` — all image elements
- `src/components/FeedEntryCard.tsx` — author avatars, inline images
- `src/components/BlobImage.tsx` — blob-resolved images (used by vault items in feed)

### Phase 3.2: Video click-to-play with IntersectionObserver cleanup

**File:** `src/components/VideoPlayer.tsx`

Ensure:
1. Videos render as **poster frame only** until user clicks play
2. No `<video>` `src` is set until user clicks (prevents WebKitGTK from initializing GStreamer pipeline)
3. When the card scrolls off-screen, the `<video>` pipeline is destroyed via IntersectionObserver (from Phase 2.3)
4. Maximum **1** active video at a time — starting a new video pauses the previous one

**Single-video enforcement:**
```typescript
// Module-level singleton
let activeVideoElement: HTMLVideoElement | null = null;

function playVideo(video: HTMLVideoElement) {
  if (activeVideoElement && activeVideoElement !== video) {
    activeVideoElement.pause();
    activeVideoElement.removeAttribute('src');
    activeVideoElement.load();
  }
  activeVideoElement = video;
  video.src = resolvedUrl;
  video.play();
}
```

### Phase 3.3: Responsive video aspect ratio

**File:** `src/components/VideoPlayer.tsx` + `FeedScreen.css`

Implement the §6.2 responsive video spec:

```css
/* Desktop: pillarbox for vertical video */
@media (min-width: 769px) {
  .video-container {
    max-width: 100%;
    max-height: 500px;
    display: flex;
    justify-content: center;
    align-items: center;
    background: #000;
  }
  .video-container video {
    max-width: 100%;
    max-height: 500px;
    object-fit: contain;  /* Preserves aspect ratio, black bars fill remainder */
  }
}

/* Mobile: fill width */
@media (max-width: 768px) {
  .video-container {
    width: 100%;
    max-height: 80vh;
    background: #000;
  }
  .video-container video {
    width: 100%;
    max-height: 80vh;
    object-fit: contain;
  }
}
```

### Phase 3.4: Blob resolution for vault items (viewport-aware)

The new feed does NOT bulk-prefetch blob URLs. Instead, blob resolution happens lazily:

1. `BlobImage` (already uses `useBlobUrl`) resolves when the component mounts
2. With `content-visibility: auto`, components only mount/render when entering the viewport
3. Add a concurrency limit to blob resolution: max 5 concurrent `invoke('get_media_blob')` calls

**File:** `src/hooks/useBlobUrl.ts` — add concurrency limit:

```typescript
const BLOB_RESOLVE_CONCURRENCY = 5;
let activeBlobResolves = 0;
const pendingBlobResolves: Array<() => void> = [];

async function resolveWithLimit<T>(fn: () => Promise<T>): Promise<T> {
  if (activeBlobResolves >= BLOB_RESOLVE_CONCURRENCY) {
    await new Promise<void>(resolve => pendingBlobResolves.push(resolve));
  }
  activeBlobResolves++;
  try {
    return await fn();
  } finally {
    activeBlobResolves--;
    if (pendingBlobResolves.length > 0) {
      pendingBlobResolves.shift()!();
    }
  }
}
```

### Phase 3 Memory Gate 🧪

**STOP. Do not proceed to Phase 4 until this passes.**

Test procedure:
1. `npx tsc --noEmit` — 0 errors
2. `pnpm test -- --run` — all tests pass
3. Load 150 items with images from Bluesky CDN
4. **Memory test:** RSS must stay under **500 MB**
5. Scroll through feed — images should lazy-load as they enter viewport
6. Play a video → scroll it off-screen → verify GStreamer pipeline released (memory drops)
7. Play a second video → verify first video is automatically paused/destroyed
8. Load vault items with blob references → verify they resolve lazily, max 5 concurrent
9. **Vertical video test:** Find a 9:16 video → on desktop, verify black pillarbox bars → resize window to <768px, verify video fills width

**Commit:** `feat(track-b): implement lazy media pipeline with content-visibility and viewport cleanup`

---

## Phase 4: Source Toggles + Integration Polish

**Goal:** Wire up the Following tab source toggles, remove dead code, finalize the rebuild.

### Phase 4.1: Source toggle UI for Following tab

**File:** `src/screens/FeedScreen.tsx` (or a new `src/components/feed/SourceToggleBar.tsx`)

When `activeTab === 'following'`, render a row of toggle pills above the feed:

```tsx
<div className="source-toggle-bar">
  {linkedSources.map(source => (
    <button
      key={source.id}
      className={`source-toggle ${sourceToggles[source.id] ? 'active' : ''}`}
      onClick={() => setSourceToggle(source.id, !sourceToggles[source.id])}
      aria-pressed={sourceToggles[source.id]}
    >
      <SourceIcon id={source.id} />
      {source.label}
    </button>
  ))}
</div>
```

When toggles change, the hook re-fetches with `source_filter` param set to only enabled sources.

### Phase 4.2: Update FeedRightPanel and ContextPanel

These import `FeedTab` and `FEED_TAB_LABELS`. Update them to use the new types ('feeds' instead of 'subscribed', no 'you').

### Phase 4.3: Remove dead imports and code

Search the entire `src/` directory for:
- References to deleted files (`prefetchBlobUrls`, `feedAnalytics`, `VideoPlaybackManager`)
- References to removed tab value `'you'` (the 'subscribed' tab is KEPT — do not remove those references)
- References to old hook exports that no longer exist (`FEED_FILTER_ALL`, `ARCHIVE_PAGE_SIZE`, `customFeedUris` if unused)
- Unused imports in `FeedScreen.tsx` (the rewrite will have changed many)

### Phase 4.4: Update/add tests

**Files:** `src/` test files

1. **Update existing tests** that reference `FeedTab = 'you'` — remove those. 'subscribed' tests stay as-is.
2. **Add new tests:**
   - `useUnifiedFeed.test.ts`: test 150-item cap enforcement, cursor pagination, tab isolation
   - `FeedList.test.tsx`: test that content-visibility wrapper renders correctly
   - `SourceToggleBar.test.tsx`: test toggle state changes

### Phase 4.5: Infinite scroll opt-in

Add a setting in the feed settings (or a toggle in the feed toolbar) that enables infinite scroll:

```typescript
// In the hook:
const infiniteScrollEnabled = useFeedSettings().infiniteScroll; // default: false

// In the FeedList:
{infiniteScrollEnabled ? (
  <InfiniteScrollTrigger onIntersect={loadMore} hasMore={hasMore} />
) : (
  hasMore && <button onClick={loadMore}>Load More</button>
)}
```

`InfiniteScrollTrigger` is a tiny component that uses IntersectionObserver on a sentinel div at the bottom of the feed.

### Phase 4 Memory Gate 🧪 (FINAL)

**This is the definitive acceptance test for the entire rebuild.**

Test procedure:
1. `npx tsc --noEmit` — 0 errors
2. `pnpm test -- --run` — all tests pass (including new tests)
3. **Full workflow test:**
   - Launch app
   - All tab: load 5 pages (150 items). Scroll through entire feed.
   - Following tab: toggle source filters. Verify filtering works.
   - Subscribed tab: select a custom feed. Load 2 pages.
   - RSS tab: verify placeholder renders correctly.
   - Groups tab: verify placeholder or content loads.
   - Switch between all tabs rapidly (10 times).
4. **Memory ceiling:** RSS must stay under **500 MB** at all times during the above workflow
5. **Memory recovery:** After switching away from a media-heavy tab and back, memory should not grow monotonically
6. **No regressions:**
   - Like/reply/repost still works on Bluesky posts
   - Quote post still works
   - ComposeBox publishes correctly
   - Pull-to-refresh works
   - Images load lazily
   - Videos are click-to-play with proper aspect ratios
   - Vault items resolve blob URLs lazily
7. **Performance:**
   - Tab switch: <100ms to render cached data
   - Load More: <500ms
   - Scroll: 60fps

**Commit:** `feat(track-b): complete feed pipeline rebuild with source toggles and integration polish`

---

## Verification Commands

Run these at every phase gate:

```bash
# TypeScript
cd bytescape && npx tsc --noEmit

# Tests
cd bytescape && pnpm test -- --run

# Memory check (while app is running)
ps aux | grep bytescape | grep -v grep | awk '{print $6/1024 " MB", $11}'

# Dead code check
grep -rn "prefetchBlobUrls\|VideoPlaybackManager\|feedAnalytics" src/ --include="*.ts" --include="*.tsx"

# Deleted tab references ('you' only — 'subscribed' stays)
grep -rn "'you'" src/ --include="*.ts" --include="*.tsx" | grep -i feedtab
```

---

## Constraints

- **Track boundary:** Only modify files in `src/`. Do NOT touch `src-tauri/src/` (backend files).
- **Build verification:** Run `npx tsc --noEmit` + `pnpm test -- --run` before EVERY commit. Zero errors.
- **Shared files:** Before editing `types/index.ts` or `store/index.ts`, run `git log --oneline -5 -- <filepath>` to check for recent agent modifications.
- **Commit format:** `feat(track-b): description`
- **No TODOs in code.** Implement fully.
- **Memory gates are mandatory.** Do not skip them. Do not proceed to the next phase if the current phase fails its memory gate. Fix the issue first.
- **Do not modify FeedEntryCard.tsx** unless absolutely necessary for type changes. Card rendering is working. The rebuild is the pipeline, not the cards.
- **Backward compatibility with backend:** The new hook should gracefully handle both old response formats (tuples) and new `FeedPage` format. Check `typeof response` or use try/catch to support both during the transition period while the backend sprint is in progress.
- **Update `progress.md`** with deliverables after each phase commit.

---

## What This Sprint Does NOT Include

- Backend changes (see `backend-sprint-v15.md`)
- Card redesign (FeedEntryCard, EngagementBar, MediaGrid are staying)
- New content types (Audio player, Document preview — future sprint)
- Profile screen changes
- Chat screen changes
- Settings UI for feed configuration (future sprint, can use hardcoded defaults for now)
- Sidebar/navigation changes
