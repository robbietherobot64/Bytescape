# Frontend Sprint v14 — EMERGENCY: Memory Leak Eradication (Track B)

**Sprint Goal:** **FIX THE MEMORY LEAK — FRONTEND SIDE.** The user has reported 32+ GB memory usage when filtering the feed by video for THREE consecutive sprints. The backend is fixing the primary leak (full-blob reads into memory), but the frontend has **4 distinct leak sources** that compound the problem: unreleased GStreamer pipelines, excessive overscan for video feeds, unbounded URL caches, and missing video element cleanup. This sprint is 100% dedicated to eradicating every frontend memory contributor. **NO feature work. NO styling fixes. NO new UI.**

**Context:** PM investigation identified **10 distinct memory leak sources** across backend and frontend. The backend prompt covers 6 sources (streaming blob export, batch concurrency, FFmpeg timeouts, cache eviction). This prompt covers the remaining 4 frontend sources. The backend fixes will be most effective when paired with these frontend fixes.

**HEAD:** `d242546` on `main`. **Build:** 0 tsc errors, 272 frontend tests pass.

---

## Why The Frontend Matters For This Leak

Even after the backend switches to streaming blob exports (zero-copy to disk), the frontend still creates **GStreamer video pipelines** for every visible `<video>` element. Each GStreamer pipeline allocates 100-500 MB of memory for video decoding buffers. With an overscan of 5, filtering by Video creates 8-13 simultaneous pipelines (visible + overscan) = **0.8-6.5 GB of decoder memory**.

When the user changes filters (e.g., from Video back to All), these pipelines are NOT released — FeedScreen does NOT trigger cleanup on filter change. They accumulate until the tab is switched or the app is restarted.

---

## P0.1: Release GStreamer Pipelines on Filter Change (🔥🔥🔥 PRIMARY FRONTEND LEAK)

**File:** `src/screens/FeedScreen.tsx`

**Problem:** When the user switches content filters (e.g., Video → All, or All → Images), the previous set of video elements remain in memory. Their GStreamer decoding pipelines are not released. Each pipeline holds 100-500 MB.

**Context:** `PeerProfileScreen.tsx` already handles this correctly — check how it cleans up video elements when the user navigates away. FeedScreen needs equivalent cleanup.

**Investigation steps:**
1. Search FeedScreen.tsx for `mediaFilter` state changes — what happens when `setMediaFilter` is called?
2. Check if VirtualizedFeedList unmounts video items on filter change, or just hides them
3. Check VideoPlayer.tsx — does it clean up the `<video>` element's `src` on unmount?

**Required fix:**
1. Add a `useEffect` that triggers when `mediaFilter` changes
2. In the effect cleanup, find and pause/destroy all active `<video>` elements in the feed container
3. Clear the `videoUrlCache` on filter change (see P0.3 below)
4. Force the virtualizer to reset (re-measure all items) after filter change, which will cause old video items to unmount

**Implementation approach:**
```typescript
// In FeedScreen, after mediaFilter state:
useEffect(() => {
  // On filter change, pause all playing videos and clear their sources
  // to release GStreamer pipelines
  const feedContainer = document.querySelector('.feed-container'); // or use a ref
  if (feedContainer) {
    const videos = feedContainer.querySelectorAll('video');
    videos.forEach(video => {
      video.pause();
      video.removeAttribute('src');
      video.load(); // forces release of media resources
    });
  }
}, [mediaFilter]);
```

**Verification:** Switch between Video and All filters 10 times. Memory should return to baseline after each switch, not accumulate.

---

## P0.2: Reduce Overscan for Video-Dense Feeds

**File:** `src/components/feed/VirtualizedFeedList.tsx`

**Problem:** The virtualizer renders extra items above and below the viewport ("overscan"). The current default is likely 5 (set in Sprint v14 P0.1). For text posts, this is fine. For video posts, each overscan item creates a GStreamer pipeline (100-500 MB). With overscan=5 and a video filter, that is 5 extra pipelines = 0.5-2.5 GB of wasted memory for items not even visible.

**Required fix:**
1. Accept a `contentType` or `isVideoHeavy` prop in VirtualizedFeedList
2. When the content is video-heavy (Video filter active), set overscan to **1**
3. For all other content types, keep the current overscan value
4. Pass `mediaFilter` or a derived boolean from FeedScreen → VirtualizedFeedList

**Implementation:**
```typescript
// In VirtualizedFeedList props:
interface VirtualizedFeedListProps {
  // ... existing props
  contentType?: string; // 'video' | 'all' | 'images' | etc.
}

// In the virtualizer setup:
const overscan = props.contentType === 'video' ? 1 : 5;
```

**Memory impact:** Reduces video pipeline count from 8-13 to 4-6 (visible + 1 overscan each direction).

---

## P0.3: Add Eviction to `videoUrlCache`

**File:** `src/components/VideoPlayer.tsx`

**Problem:** `videoUrlCache` (line ~32) is a module-global `Map<string, string>` that **never clears**. Every video URL resolved during the session is cached forever. Over a long session with hundreds of videos, this map grows to hold hundreds of entries. While each entry is small (just a URL string), the issue is that it prevents garbage collection of associated resources and grows without bound.

**Required fix:**
1. Add a maximum size to the cache (100 entries)
2. When the cache exceeds the max, delete the oldest entries (FIFO eviction)
3. Add a `clearVideoUrlCache()` export that FeedScreen can call on filter change (see P0.1)

**Implementation — simple LRU with insertion order:**
```typescript
const VIDEO_URL_CACHE_MAX = 100;
const videoUrlCache = new Map<string, string>();

function setVideoUrlCache(key: string, value: string) {
  if (videoUrlCache.size >= VIDEO_URL_CACHE_MAX) {
    // Delete oldest entry (first key in Map iteration order)
    const firstKey = videoUrlCache.keys().next().value;
    if (firstKey !== undefined) {
      videoUrlCache.delete(firstKey);
    }
  }
  videoUrlCache.set(key, value);
}

export function clearVideoUrlCache() {
  videoUrlCache.clear();
}
```

Replace all `videoUrlCache.set(src, ...)` calls with `setVideoUrlCache(src, ...)`.

---

## P0.4: Add Eviction to `resolvedUrlCache`

**File:** `src/hooks/useBlobUrl.ts`

**Problem:** `resolvedUrlCache` (line ~45) is a module-global `Map<string, string>` that grows without bound. The docstring at line ~29 explicitly acknowledges this as tech debt: "resolvedUrlCache is a process-global, unbounded Map with no eviction."

**Required fix:**
1. Add a maximum size to the cache (500 entries — larger than video cache because images are cheaper)
2. When the cache exceeds the max, evict oldest entries (FIFO)
3. Export a `clearResolvedUrlCache()` function for use in filter changes

**Implementation — same pattern as P0.3:**
```typescript
const RESOLVED_URL_CACHE_MAX = 500;
export const resolvedUrlCache = new Map<string, string>();

export function cacheResolvedUrl(hash: string, url: string): void {
  if (resolvedUrlCache.size >= RESOLVED_URL_CACHE_MAX) {
    const firstKey = resolvedUrlCache.keys().next().value;
    if (firstKey !== undefined) {
      resolvedUrlCache.delete(firstKey);
    }
  }
  resolvedUrlCache.set(hash, url);
}

export function clearResolvedUrlCache(): void {
  resolvedUrlCache.clear();
}
```

---

## P0.5: Ensure `<video>` Elements Release Resources on Unmount

**File:** `src/components/VideoPlayer.tsx`

**Investigation:** Check the VideoPlayer component's cleanup logic. When a VideoPlayer unmounts (e.g., scrolled out of the virtualizer's render window), does it:
1. Call `video.pause()`?
2. Set `video.src = ''` or `video.removeAttribute('src')`?
3. Call `video.load()` to force the browser/WebView to release the media source?

If any of these are missing, GStreamer pipelines persist even after the component unmounts.

**Required fix:**
```typescript
useEffect(() => {
  return () => {
    // Cleanup on unmount — release GStreamer pipeline
    if (videoRef.current) {
      videoRef.current.pause();
      videoRef.current.removeAttribute('src');
      videoRef.current.load(); // forces pipeline release
    }
  };
}, []);
```

Also check for any `useEffect` that sets `video.src` — ensure its cleanup function removes the src.

---

## P0.6: Add Memory Diagnostics (Developer Aid)

**File:** `src/screens/FeedScreen.tsx` (or a new util)

Add a development-mode memory diagnostic that logs:
1. Number of active `<video>` elements in the DOM
2. Size of `videoUrlCache`
3. Size of `resolvedUrlCache`
4. Current `mediaFilter` value

Log this on every filter change. This helps verify the fixes are working and provides a diagnostic tool for future memory issues.

```typescript
if (process.env.NODE_ENV === 'development') {
  console.log('[Memory] Filter changed to:', mediaFilter);
  console.log('[Memory] Active <video> elements:', document.querySelectorAll('video').length);
  console.log('[Memory] videoUrlCache size:', videoUrlCache.size);
  console.log('[Memory] resolvedUrlCache size:', resolvedUrlCache.size);
}
```

---

## Verification Checklist

After implementing all fixes:

1. **GStreamer cleanup:** Switch from Video filter to All filter 5 times. Check that `<video>` element count in DOM returns to 0 (or near 0) after each switch. Memory should not grow monotonically.
2. **Overscan reduction:** With Video filter active, check how many `<video>` elements are in the DOM. Should be ≤ viewport items + 2 (1 overscan each direction), NOT viewport + 10.
3. **Cache eviction:** Scroll through 200+ items. `resolvedUrlCache.size` should cap at 500. `videoUrlCache.size` should cap at 100.
4. **Unmount cleanup:** Open DevTools, scroll a video into view (check it plays), scroll it far out of view. The video element should be unmounted and its GStreamer pipeline released.
5. **Combined test:** Launch app → filter by Video → scroll for 30 seconds → switch to All → switch to Images → switch to Video. Total memory should stay under 3 GB (combined with backend fixes, target is <2 GB).
6. **Build:** `npx tsc --noEmit` — 0 errors, `pnpm test -- --run` — all pass.

---

## Constraints

- **Track boundary:** Only modify files in `src/`. Do NOT touch `src-tauri/src/` (backend files).
- **Build verification:** Run `npx tsc --noEmit` + `pnpm test -- --run` before EVERY commit. Zero errors, zero test regressions.
- **Shared files:** Before editing `types/index.ts`, `useUnifiedFeed.ts`, `App.tsx`, `store/index.ts`, run `git log --oneline -5 -- <filepath>` to check for recent agent modifications.
- **Commit format:** `fix(track-b): description`
- **No TODOs in code.** Implement fully.
- **MEMORY LEAK ONLY.** Do not fix any other bugs, add any features, or address any styling issues. Every line of code you write this sprint must reduce memory usage.
- **Update `progress.md`** with deliverables when done.

---

## What This Sprint Does NOT Include

Everything else. No image sizing fixes. No Mastodon duplicate fix. No Instagram abbreviation. No new UI. No feature work of any kind. The ONLY goal is to reduce frontend memory contribution to <1 GB under all conditions.
