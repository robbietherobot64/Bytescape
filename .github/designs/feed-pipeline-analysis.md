# Feed Pipeline Deep Dive — Systems Analysis

**Date:** 2025-01-28  
**Context:** 17–25 GB RSS observed when filtering feed by video, despite few posts loading  
**Previous fixes:** Frontend mitigations (`5befc2f`), streaming blob (`63159dc`) — both insufficient

---

## 1. Executive Summary

The memory problem is **not** a single leak — it is a **cascade of amplification loops** where 13 structural issues compound multiplicatively. Each one alone is tolerable; together they explain how ≤200 posts can inflate to 17–25 GB. The pipeline has **no backpressure**, **no viewport-awareness**, and **multiple data duplication points** that turn every 45-second refresh into a full replay of the cascade.

### Multiplier Model (rough order-of-magnitude)

| Stage | What happens | Memory amplification |
|-------|-------------|---------------------|
| Backend: `build_all_headers()` | Full-table JSON parse of every vault item → headers | `N_vault × avg_item_size` (one-time, cached) |
| Backend: `list_my_items_paged(200)` | Clone headers → filter → `bulk_get_items_by_ids` → parse 200 full `BytescapeItem` | 200 × 2–5 KB = ~1 MB |
| Backend: `BytescapeItem → FeedEntry` | Clone every field of every item into FeedEntry struct | 2× transient allocation (~2 MB) |
| IPC serialization | serde_json::to_string of entire `Vec<FeedEntry>` response | +1× as JSON string (~1–3 MB) |
| JS parse + React state | `JSON.parse()` + `setFeedEntries(deduped)` — old + new arrays alive simultaneously | 2× JS heap (~2–6 MB) |
| Zustand cache | `setCache(key, deduped, cursor)` — **same data stored a second time** | +1× (~1–3 MB) |
| **Stale-while-revalidate** | Cache hit → render stale → background fetch → **repeat entire pipeline** | **2× of everything above** |
| `prefetchBlobUrls(ALL entries)` | Batch IPC for all blob refs (not just viewport), fires **twice** per load | N_blobs × IPC + disk I/O |
| For uncached blobs: `get_media_blob` | Iroh export → disk write → file detection | per-blob: 1 semaphore permit, temp memory |
| For HLS videos: `resolve_hls_video` | FFmpeg subprocess → 13 MB MP4 on disk + ~200 MB process memory | **200 MB per concurrent FFmpeg** |
| `filterFacebookDuplicateImagePosts` | `new Map(entries)` + media URL index on **every render** | O(n) transient allocation |
| Auto-refresh (45s) | Re-runs **entire pipeline** from backend through all stages | Full cascade, every 45s |
| Syndication verification | N+1 IPC calls → if any changed → **another full fetchFeed** | Potential cascade trigger |
| WebKitGTK GStreamer pipelines | Video elements keep native pipelines alive even after DOM removal | 100–500 MB per zombie pipeline |

---

## 2. Data Flow Diagram

```
USER OPENS FEED TAB
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│  useUnifiedFeed.fetchFeed(tab, background=false)            │
│  ┌──────────────────────────┐                               │
│  │ 1. Check Zustand cache   │─── HIT ──┐                   │
│  │    (maxAge: 120s)        │           │                   │
│  └──────────────────────────┘           ▼                   │
│                                  setFeedEntries(cached)     │
│                                  background = true ──┐      │
│                                                      │      │
│  ┌──────────────────────────┐◄───────────────────────┘      │
│  │ 2. Tauri IPC call        │   (ALWAYS runs, even on hit)  │
│  │    get_timeline_unified  │                               │
│  └──────────────────────────┘                               │
│         │                                                   │
└─────────┼───────────────────────────────────────────────────┘
          │  IPC boundary
          ▼
┌─────────────────────────────────────────────────────────────┐
│  RUST: get_timeline_unified                                 │
│                                                             │
│  ┌───────────────┐  ┌──────────────┐  ┌─────────────────┐  │
│  │ Vault future  │  │ Timeline fut │  │ Custom feeds fut│  │
│  │               │  │              │  │                 │  │
│  │ headers_all   │  │ feed_cache?  │  │ N × get_feed()  │  │
│  │  = build_all_ │  │  30s TTL     │  │                 │  │
│  │    headers()  │  │  ↓ miss      │  │                 │  │
│  │  (full table  │  │ bsky API     │  │                 │  │
│  │   JSON parse) │  │ (50 posts)   │  │                 │  │
│  │               │  │              │  │                 │  │
│  │ → filter mine │  │              │  │                 │  │
│  │ → page(200)   │  │              │  │                 │  │
│  │ → bulk_get    │  │              │  │                 │  │
│  │   (parse 200  │  │              │  │                 │  │
│  │    full items)│  │              │  │                 │  │
│  │ → to_FeedEntry│  │→ to_FeedEntry│  │→ to_FeedEntry   │  │
│  │   (clone all  │  │  (clone all  │  │  (clone all     │  │
│  │    fields)    │  │   fields)    │  │   fields)       │  │
│  └───────┬───────┘  └──────┬───────┘  └────────┬────────┘  │
│          └─────────────────┼───────────────────┘            │
│                            ▼                                │
│               tokio::join! (parallel)                       │
│                            │                                │
│          ┌─────────────────┼─────────────────┐              │
│          │ Merge + vault dedup (HashSet URI)  │              │
│          │ Source filter                      │              │
│          │ Sort by timestamp                  │              │
│          │ Final dedup (HashSet<String> seen) │              │
│          └─────────────────┬─────────────────┘              │
│                            │                                │
│          serde_json::to_value(UnifiedFeedResponse)          │
│          (entire Vec<FeedEntry> serialized)                  │
│                            │                                │
└────────────────────────────┼────────────────────────────────┘
                             │ IPC return (JSON blob)
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  JAVASCRIPT: useUnifiedFeed.fetchFeed (continued)           │
│                                                             │
│  JSON.parse(response)                                       │
│  ↓                                                          │
│  deduplicateFeedEntries(items) → builds Map for dedup       │
│  ↓                                                          │
│  setFeedEntries(deduped)  ←── React state (copy 1)         │
│  setCache(key, deduped)   ←── Zustand store (copy 2)       │
│  ↓                                                          │
│  prefetchBlobUrls(deduped) ←── resolves ALL blob refs      │
│       │                        (not just viewport)          │
│       │                                                     │
│       ▼                                                     │
│  IPC: resolve_blob_urls_batch(N sources)                    │
│    ├── cache hit: return http://127.0.0.1:PORT/file.ext     │
│    └── cache miss: get_media_blob(hash) → Iroh export       │
│                    (semaphore-limited to 4 concurrent)       │
│                    For HLS: FFmpeg → 200 MB process memory   │
│                                                             │
│  Results → cacheResolvedUrl(hash, url)                      │
│            (LRU map, max 500 entries)                       │
│                                                             │
│  ┌──────────────────────────────────────────────┐           │
│  │ FeedScreen.tsx render:                        │           │
│  │  filteredEntries = useMemo(() => {            │           │
│  │    filterFacebookDuplicateImagePosts(entries)  │           │
│  │      → builds Map of ALL entries by ID        │           │
│  │      → builds Map of ALL media URLs           │           │
│  │    then filter by mediaFilter                 │           │
│  │  }, [feedEntries, mediaFilter])               │           │
│  └──────────────────────────────────────────────┘           │
│                                                             │
│  ┌──────────────────────────────────────────────┐           │
│  │ EVERY 45 SECONDS (auto-refresh):              │           │
│  │  fetchFeed(tab, background=true)              │           │
│  │  → Full pipeline replay from Rust through JS  │           │
│  └──────────────────────────────────────────────┘           │
│                                                             │
│  ┌──────────────────────────────────────────────┐           │
│  │ SYNDICATION VERIFICATION (after every load):  │           │
│  │  for each vault entry with active syndications│           │
│  │    → IPC: verify_syndications(id)             │           │
│  │    → if changed: fetchFeed(tab, true)         │           │
│  │    → ANOTHER full pipeline run!               │           │
│  └──────────────────────────────────────────────┘           │
│                                                             │
│  ┌──────────────────────────────────────────────┐           │
│  │ LOAD MORE (unbounded accumulation):           │           │
│  │  setFeedEntries(prev => dedup([...prev,       │           │
│  │    ...newEntries]))                           │           │
│  │  → React state grows without cap              │           │
│  │  → appendCache has 500-entry cap, but         │           │
│  │    handleLoadMore calls setCache not append   │           │
│  └──────────────────────────────────────────────┘           │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 3. Finding Details

### F1 — CRITICAL: Stale-While-Revalidate Doubles Every Feed Load

**Location:** [useUnifiedFeed.ts](bytescape/src/hooks/useUnifiedFeed.ts#L165-L185)

**Mechanism:** When the user switches tabs or the feed loads:
1. `getCache(cacheKey)` returns a hit → `setFeedEntries(cached.entries)` + `background = true`
2. Even with a cache hit, the function continues and makes the full Tauri IPC call
3. On response: `setFeedEntries(deduped)` replaces state, `setCache(key, deduped)` updates Zustand
4. `prefetchBlobUrls(deduped)` fires **again** (it already fired for the stale data)

**Impact:** Every cache-hit feed load executes the **full backend pipeline twice** and triggers **two rounds of blob prefetching**. During the brief window where both old and new `FeedEntry[]` arrays exist in the JS heap, memory is 2× what it should be.

**Evidence:** Lines 165-185 show the cache-hit path setting `background = true` but continuing execution to the full IPC call. Lines 350-375 show `prefetchBlobUrls` is called unconditionally after each tab path.

---

### F2 — CRITICAL: feedEntries React State Grows Unbounded on Load More

**Location:** [useUnifiedFeed.ts](bytescape/src/hooks/useUnifiedFeed.ts#L493-L570)

**Mechanism:** `handleLoadMore` calls:
```
setFeedEntries(prev => deduplicateFeedEntries([...prev, ...newEntries]))
```
This spreads the old array + new entries into a fresh array. The Zustand `appendCache` has a 500-entry cap, but `handleLoadMore` calls `setFeedEntries` directly on React state without any cap. Also, `deduplicateFeedEntries` creates a transient `Map` of all entries during dedup, so **three copies** exist momentarily: `prev`, `[...prev, ...newEntries]`, and the dedup Map.

**Impact:** After 5 "Load More" operations with 200 entries each, React state holds 1000+ FeedEntry objects (each 2-5 KB in JS) = 2-5 MB in React state alone, plus the same in Zustand cache.

---

### F3 — CRITICAL: Full Vault JSON Parse on Header Cache Miss

**Location:** [db/vault.rs](bytescape/src-tauri/src/db/vault.rs#L331-L370)

**Mechanism:** `build_all_headers()` iterates **every row** of `ITEMS_TABLE`, calling `serde_json::from_str` to parse the **full BytescapeItem** (30+ fields including content, media_blobs, share_history, tagged_people, etc.) just to extract a `BytescapeItemHeader` (13 fields).

The headers **are** cached via `header_cache: RwLock<Option<Vec<BytescapeItemHeader>>>`, so this is a one-time cost per cache invalidation. But **every `save_item` / `upsert_item` / `delete_item` calls `invalidate_header_cache()`**, forcing a full re-parse on the next read.

During import or active posting, headers may be invalidated multiple times per minute, each triggering a full-table parse.

**Impact per rebuild:** For a vault with 5000 items at average 3 KB JSON each = 15 MB parsed + 5000 headers allocated. The full BytescapeItem deserialized per row is immediately dropped (only `.header()` is kept), but the JSON parsing allocator churn is significant.

**Evidence:** Lines 331-348 show the full table scan with `serde_json::from_str` for every row.

---

### F4 — HIGH: BytescapeItem → FeedEntry Conversion Clones Everything

**Location:** [feed_aggregation.rs](bytescape/src-tauri/src/commands/feed_aggregation.rs#L104-L200)

**Mechanism:** The conversion function maps every `BytescapeItem` field into a `FeedEntry`, cloning strings, Vecs, Options, and nested structs. Both the source `BytescapeItem` and the target `FeedEntry` exist simultaneously during the map operation over the Vec. The `FeedEntry` struct carries **all** fields (30+) regardless of what the feed card actually displays.

**What the feed card needs:** id, timestamp, content (first 300 chars), author handle/avatar, media_urls (for thumbnails), source, item_type, like_count, reply_count.

**What FeedEntry carries additionally:** full content body, share_history (Vec of SyndicationEvent), audiences (Vec), tagged_people, location, link metadata, quoted_post (recursive FeedEntry), syndicated_uri, media_mimes, media_thumbnails, reposted_by, reply_parent details.

**Impact:** ~40-60% of serialized FeedEntry data is unused by the feed card renderer.

---

### F5 — HIGH: Auto-Refresh Every 45 Seconds Replays Full Cascade

**Location:** [useUnifiedFeed.ts](bytescape/src/hooks/useUnifiedFeed.ts#L448-L455)

**Mechanism:** A `setInterval(45000)` calls `fetchFeed(activeTab, true)` — a **background** fetch that runs the complete pipeline: Rust vault scan + network API + IPC serialize + JS parse + state update + Zustand cache + prefetchBlobUrls. The `document.hidden` guard skips this when the tab is backgrounded, but **not** when the user is actively using the app on a different screen within the same window.

**Impact:** Over a 10-minute session, this triggers ~13 full pipeline replays. Combined with F1 (stale-while-revalidate), each auto-refresh may run the backend pipeline and blob prefetch **twice**.

---

### F6 — HIGH: prefetchBlobUrls Resolves ALL Blobs, Not Viewport

**Location:** [prefetchBlobUrls.ts](bytescape/src/utils/prefetchBlobUrls.ts#L72-L88)

**Mechanism:** `prefetchBlobUrls(entries)` collects **every** blob reference from **every** entry — `media_urls`, `media_thumbnails`, `link_thumbnail`, `author_avatar`, and `quoted_post.media_urls`. It skips entries already in `resolvedUrlCache` (max 500), but for uncached blobs it fires `resolve_blob_urls_batch` which iterates sequentially and for misses calls `get_media_blob()` (Iroh export, semaphore-gated to 4).

**Impact:** A feed with 200 entries × 2 media refs each = 400 blob resolutions. Even with cache hits, the IPC call serializes a 400-element HashMap response. For cache misses, 4-at-a-time Iroh exports run, each writing to disk.

**Interaction with F1:** Because stale-while-revalidate fires the pipeline twice, blob prefetch fires twice per feed load — once for stale entries (likely cached, cheap), once for fresh entries (may include new blobs, expensive).

---

### F7 — MEDIUM: Syndication Verification N+1 Pattern

**Location:** [useUnifiedFeed.ts](bytescape/src/hooks/useUnifiedFeed.ts#L457-L482)

**Mechanism:** After every feed load, an effect filters entries with active syndications (`share_history.some(e => e.active)`) and fires individual `verify_syndications` IPC calls. The guard (`verifiedIdsRef`) prevents re-verification of the same ID within a session, **but** `verifiedIdsRef.current.clear()` is called on every non-background fetch, resetting the guard. If any verification returns `changed`, it triggers `fetchFeed(activeTab, true)` — another full pipeline run.

**Impact:** N vault posts with active syndications = N sequential IPC calls after every foreground feed load. Each "changed" response triggers a cascade.

---

### F8 — MEDIUM: filterFacebookDuplicateImagePosts Runs Every Render

**Location:** [FeedScreen.tsx](bytescape/src/screens/FeedScreen.tsx#L223-L240), [feedAnalytics.ts](bytescape/src/utils/feedAnalytics.ts#L25-L90)

**Mechanism:** Inside the `useMemo`, `filterFacebookDuplicateImagePosts` builds:
1. A `Map<string, FeedEntry>` of all entries by ID
2. A `Map<string, string>` of media URLs to winner IDs
3. Iterates all entries twice (scoring + filtering)

This runs on every change to `feedEntries` or `mediaFilter`.

**Impact:** For 500 entries, creates ~500-entry Maps twice, iterates 1000 times. The transient allocation is O(n), and since `feedEntries` changes on every auto-refresh (F5), this runs every 45 seconds.

---

### F9 — MEDIUM: Same Data Stored in React State AND Zustand Cache

**Location:** [useUnifiedFeed.ts](bytescape/src/hooks/useUnifiedFeed.ts#L190-L375), [store/index.ts](bytescape/src/store/index.ts#L360-L410)

**Mechanism:** After every fetch:
1. `setFeedEntries(deduped)` → React state holds the array
2. `setCache(cacheKey, deduped, cursor)` → Zustand store holds a **second reference** to the same array

JavaScript garbage collection won't free the array as long as either reference exists. The Zustand cache also prunes entries >2 minutes old on `setCache`, but **different cache keys** for different tab+filter combos coexist. Switching between 3 tabs creates 3 cache entries.

**Impact:** 3 active tab caches × 200 entries × 3–5 KB each = 1.8–3 MB in Zustand alone, plus the same in React state for the active tab.

---

### F10 — HIGH: WebKitGTK GStreamer Pipeline Zombies

**Location:** VideoPlaybackManager.ts, VideoPlayer.tsx

**Mechanism:** WebKitGTK manages video playback through GStreamer pipelines that are **native-process allocated** — they don't show up in JS heap but consume RSS. When a `<video>` element is removed from DOM (e.g., scrolling past a feed card), the GStreamer pipeline teardown is asynchronous and can take seconds. If new videos mount before old pipelines finish teardown, pipelines accumulate.

The `VideoPlaybackManager` tracks `activeVideoCount` and prunes at a threshold of 30, but:
- The threshold of 30 is too high (30 × 100–500 MB = 3–15 GB just in pipelines)
- `releaseAll()` is only called on mediaFilter change, not on tab switches or scroll events
- The counting mechanism uses module-level globals that can desync from actual DOM state

**Impact:** This is likely the single largest contributor to the 17–25 GB RSS number. Even 10 zombie pipelines × 500 MB = 5 GB of native memory invisible to JS profiling.

---

### F11 — HIGH: resolve_hls_video Downloads Full Videos Eagerly

**Location:** [media.rs](bytescape/src-tauri/src/commands/media.rs#L692-L830)

**Mechanism:** HLS videos (common on Bluesky) are downloaded via FFmpeg to produce a single MP4 file. Each FFmpeg process:
- Consumes ~200 MB of process memory
- Produces a 10–30 MB MP4 on disk
- Is gated by `video_semaphore` (2 permits), but queued operations still wait in memory
- Has a 120-second timeout, during which the process stays alive

**Interaction with F6:** `prefetchBlobUrls` resolves ALL entries' media. If 10 entries have HLS video, 10 `resolve_hls_video` calls are queued. 2 run at a time, each at ~200 MB process memory. While 2 run, 8 wait in Tokio's task queue holding their future state.

**Impact:** 5 concurrent HLS conversions = ~1 GB process memory + 50–150 MB disk writes.

---

### F12 — MEDIUM: FeedEntry Struct Is Oversized for Feed Display

**Location:** [types/index.ts](bytescape/src/types/index.ts#L179-L230)

The `FeedEntry` interface has 35+ fields. A minimal "feed card" rendering needs ~12 fields. The remaining 23+ fields (share_history, audiences, tagged_people, location, link metadata, syndication status, owner_replies, viewer_like/repost) are carried through the entire pipeline — serialized in Rust, sent over IPC, parsed in JS, stored in state, stored in cache — even though they're only needed when a user clicks into a detail view.

---

### F13 — LOW: deduplicateFeedEntries Creates O(n) Transient Maps

Builds a `Map<string, FeedEntry>` on every call. Called after every fetch and every load-more. Low individual impact but contributes to GC pressure.

---

## 4. The Cascade Under Video Filter

When the user selects the **Video** media filter, the cascade is especially severe:

1. `fetchFeed` runs → backend returns 200+ mixed entries
2. `prefetchBlobUrls` resolves ALL 200+ entries' media (not just videos)
3. `filteredEntries` in `useMemo` filters down to only video entries — maybe 10–30
4. Those 10–30 videos render, each mounting a `<video>` element
5. For HLS videos, `resolve_hls_video` fires via useBlobUrl → FFmpeg processes spawn
6. User scrolls → old video cards unmount → GStreamer pipelines linger
7. New video cards mount → new GStreamer pipelines start
8. 45-second auto-refresh → full pipeline + new prefetch + re-render
9. Previous `filteredEntries` useMemo result is GC'd, but GStreamer pipelines from the old render are still tearing down
10. Repeat from step 1 every 45 seconds

After 3 minutes: ~4 refresh cycles × 10 video pipelines × (some zombie ratio) = potentially 20–30 accumulated GStreamer pipelines.

---

## 5. Proposed Rework Architecture

### Phase 1 — Viewport-Aware Lazy Loading (Biggest Win, Surgical)

**Problem addressed:** F6, F10, F11  
**Principle:** Only resolve media for entries that are actually visible

1. **Remove `prefetchBlobUrls` from fetchFeed.** Instead, each `FeedEntryCard` lazily resolves its own media via `useBlobUrl` (which already has an in-memory LRU cache).
2. **Viewport-aware blob prefetch:** Use the virtualized list's visible range to batch-prefetch only the ~10–20 visible entries + 5 entries of lookahead. This replaces eager-all-at-once with progressive loading.
3. **Video pipeline budget:** Reduce VideoPlaybackManager threshold from 30 → 3. Only 3 video pipelines may be active simultaneously. The 4th pauses the oldest. Release pipelines on any scroll that moves the video out of viewport.
4. **Defer HLS resolution:** Don't call `resolve_hls_video` until the user actually clicks play. Show a thumbnail + play button instead.

**Expected memory reduction:** 70–80% of the video filter case. Eliminates the N×200MB FFmpeg pileup and N×500MB GStreamer pileup.

### Phase 2 — Lightweight Feed Envelope (Structural)

**Problem addressed:** F4, F12, IPC overhead

1. **Define `FeedEnvelope`** — a minimal struct with only the ~12 fields needed for feed card rendering:
   ```
   FeedEnvelope { id, timestamp, content_preview (300 chars max), author_handle, author_avatar, source, item_type, media_urls, media_kinds, like_count, reply_count, repost_count }
   ```
2. **Rust side:** `get_timeline_unified` returns `Vec<FeedEnvelope>` instead of `Vec<FeedEntry>`. Full `FeedEntry` is fetched on-demand when user clicks into detail view (new command: `get_feed_entry_detail(id)`).
3. **Header-only vault query:** Instead of `build_all_headers()` parsing full JSON, add a dedicated `ITEM_HEADERS_TABLE` that stores pre-computed headers (updated on write). Eliminates the full-table-parse on cache miss.

**Expected memory reduction:** 40–60% of per-entry IPC and JS heap cost. A 200-entry feed of FeedEnvelopes ≈ 100 KB instead of 500 KB–1 MB of full FeedEntries.

### Phase 3 — Eliminate Duplicate State + Refresh Amplification

**Problem addressed:** F1, F2, F5, F7, F9

1. **Remove stale-while-revalidate:** Replace with simple cache-or-fetch. If cache is fresh (< 30s), use it. If stale, fetch fresh and replace. No background double-fetch.
2. **Delta refresh:** Auto-refresh should only fetch entries newer than the latest timestamp in current state. Backend command: `get_timeline_since(timestamp)` returns only new items. Frontend prepends them.
3. **Cap React state:** Enforce MAX_FEED_ENTRIES (300) in React state, matching Zustand. Discard oldest entries on overflow.
4. **Batch syndication verification:** Replace N+1 individual IPC calls with a single `verify_syndications_batch(ids)` call. Don't trigger full fetchFeed on change — just update the affected entries in-place.
5. **Unified state:** Store feed data in Zustand only, use a selector in the component. Eliminates the React state + Zustand duplication.

**Expected memory reduction:** 30–50% of JS heap from eliminating double-state, double-fetch, and unbounded growth.

### Phase 4 — Backend Optimization (Performance)

**Problem addressed:** F3

1. **Indexed header table:** Create `ITEM_HEADERS_TABLE` in redb. On every `save_item` / `upsert_item`, write both the full item to `ITEMS_TABLE` and the header to `ITEM_HEADERS_TABLE`. `list_all_item_headers` reads only from the lightweight table (no JSON parse of full items).
2. **Cursor-based vault pagination:** Instead of "offset into sorted headers", use a timestamp cursor. The backend query becomes: "give me N headers where timestamp < cursor", which is O(page_size) instead of O(total_items) for the sort+skip.
3. **Streaming IPC for large responses:** For feed responses >50 entries, use Tauri's streaming response protocol (if available in v2) instead of buffering the entire JSON blob.

---

## 6. Priority Ordering

| Priority | Phase | Estimated effort | Memory reduction |
|----------|-------|-----------------|-----------------|
| **P0** | Phase 1: Viewport-aware lazy loading | 2–3 days | 70–80% of video case |
| **P1** | Phase 3: Eliminate duplicate state + refresh | 1–2 days | 30–50% of JS heap |
| **P2** | Phase 2: Lightweight FeedEnvelope | 2–3 days | 40–60% of IPC/state |
| **P3** | Phase 4: Backend optimization | 1–2 days | Better for large vaults |

Phase 1 alone should bring the video-filter case from 17–25 GB down to 3–5 GB. Combined with Phase 3, it should land around 1–3 GB for typical usage.

---

## 7. Root Cause Summary

The memory problem is not "videos are big" — it's that the feed pipeline has **no concept of what the user is actually looking at**. It eagerly fetches, converts, serializes, deserializes, caches, and prefetches **everything** for **every entry**, then does it all again every 45 seconds, while also holding two copies of the data in JS and leaking native GStreamer pipelines that are invisible to JavaScript memory profiling.

The fix is not patching individual leaks — it's redesigning the pipeline to be **lazy, viewport-aware, and backpressure-respecting**.
