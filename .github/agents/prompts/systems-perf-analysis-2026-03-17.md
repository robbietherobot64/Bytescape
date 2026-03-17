# Systems Performance Analysis: Feed Loading & Video Playback

> **Date:** 2026-03-17  
> **Agent:** Systems  
> **Scope:** Full-stack trace of feed loading latency and video playback stutter  
> **Symptom:** ~20-second feed loads, video stutter/choppy playback, multi-second "Loading native media" delays

---

## Executive Summary

After tracing the complete data path from Bluesky API → Rust backend → Tauri IPC → React frontend → WebKitGTK renderer, I identified **12 distinct latency sources in feed loading** and **6 issues in video playback**. The two biggest performance killers are:

1. **Full H.264 re-encode of every video** (FFmpeg `libx264 ultrafast`) — adds 5–20 seconds of CPU-blocking delay before any video frame renders. This is the "Loading native media" wait.
2. **Serial IPC call chains** — the frontend makes 2–3 sequential Tauri `invoke()` calls before rendering feed data, where parallelism is possible.

Both are fixable without architectural changes. The codebase already contains unused infrastructure (batch blob resolution, feed caching) that just needs wiring.

---

## Part 1: Feed Loading Pipeline

### Data Flow (Current)

```
┌──────────────────── FRONTEND (React) ────────────────────┐
│                                                           │
│  useUnifiedFeed.ts                                        │
│    ├─ [1] invoke('list_saved_feeds')          ← IPC #1   │
│    │      ↓ await (blocks)                                │
│    ├─ [2] invoke('list_followed_hashtags')    ← IPC #2   │
│    │      ↓ await (blocks)     [Subscribed tab only]      │
│    ├─ [3] invoke('get_unified_feed')          ← IPC #3   │
│    │      OR invoke('get_timeline_unified')               │
│    │      ↓ await (blocks)                                │
│    ├─ [4] For each post with media:                       │
│    │      invoke('get_media_blob')            ← IPC #N   │
│    │      (one per image — NOT batched)                   │
│    ├─ [5] For each video:                                 │
│    │      invoke('resolve_hls_video')         ← IPC + FFmpeg │
│    └─ [6] invoke('verify_bluesky_syndication')← IPC #M   │
│           (one per vault item — N+1 pattern)              │
│                                                           │
└───────────────────────────────────────────────────────────┘
         │ Tauri JSON IPC boundary
┌──────────────────── BACKEND (Rust) ───────────────────────┐
│                                                           │
│  get_timeline_unified()                                   │
│    ├─ tokio::join!(timeline, custom_feeds, vault)         │
│    │   ├─ get_timeline() → Bluesky XRPC (network)        │
│    │   ├─ get_unified_feed() → N parallel feed fetches    │
│    │   └─ list_my_items_paged() → redb (disk)            │
│    ├─ FeedCache (30s TTL, timeline only)                  │
│    ├─ Merge + dedup + sort                                │
│    └─ Serialize to JSON → IPC response                    │
│                                                           │
│  get_media_blob()                                         │
│    ├─ Check blob_cache/ on disk                           │
│    ├─ If miss: Iroh blob export (P2P)                     │
│    └─ Return file path                                    │
│                                                           │
│  resolve_hls_video()                                      │
│    ├─ Check MP4 cache on disk                             │
│    ├─ Strategy 1: FFmpeg native HLS → MP4 (full encode)   │
│    ├─ Strategy 2: Manual segment DL → concat → remux      │
│    │   (ALSO full encode, not just remux)                 │
│    └─ Return file path → Warp HTTP server                 │
│                                                           │
└───────────────────────────────────────────────────────────┘
```

### Latency Sources — Ranked by Impact

| # | Severity | Component | Issue | Est. Latency |
|---|----------|-----------|-------|-------------|
| L1 | 🔴 CRITICAL | `useUnifiedFeed.ts` | **Serial IPC chain**: Subscribed tab awaits `list_saved_feeds` → `list_followed_hashtags` → `get_unified_feed` sequentially. All tab awaits `list_saved_feeds` → `get_timeline_unified` sequentially. | +400–800ms |
| L2 | 🔴 CRITICAL | `useUnifiedFeed.ts` | **Redundant `list_saved_feeds`**: Called in TS, then called AGAIN inside `get_timeline_unified` (via `get_unified_feed`). Two network round-trips for the same data. | +200–500ms |
| L3 | 🔴 CRITICAL | `useBlobUrl.ts` | **1-at-a-time blob resolution**: Each image invokes `get_media_blob` individually. A feed page with 20 images = 20 serial IPC calls. Backend's `resolve_blob_urls_batch` EXISTS but is never called. | +500–2000ms |
| L4 | 🟠 HIGH | `feed_aggregation.rs` | **No cache for subscribed/custom feeds**: `FeedCache` only caches the timeline sub-query (30s TTL). Subscribed tab and custom feed fetches always hit Bluesky network. | +500–2000ms |
| L5 | 🟠 HIGH | `db.rs` | **Header cache cold start**: First load triggers full table scan + JSON deserialization of every vault item. Subsequent reads return a `Vec::clone()` of the entire collection. | +50–500ms |
| L6 | 🟡 MEDIUM | `useUnifiedFeed.ts` | **Syndication verification N+1**: After feed loads, calls `verify_bluesky_syndication` per vault item. 10 vault items = 10 additional IPC + network calls. | +200–1000ms |
| L7 | 🟡 MEDIUM | `useUnifiedFeed.ts` | **45-second auto-refresh**: Re-fetches the entire first page from the network. No incremental/delta update. | Recurring cost |
| L8 | 🟡 MEDIUM | `useUnifiedFeed.ts` | **Full dedup on every page**: `deduplicateFeedEntries` scans the entire accumulated feed array. O(n) per page load, growing with scroll. | +10–50ms/page |
| L9 | 🟡 MEDIUM | `media.rs:get_media_blob` | **Re-reads cached files**: On cache hit, still reads file bytes and re-writes them (to detect content type). Should just return the known path. | +5–20ms/image |
| L10 | 🟢 LOW | `bluesky.rs` | **`get_author_feed` creates new `Client`**: Uses `Client::new()` instead of `SHARED_CLIENT`, bypassing connection pooling. | +50–100ms |
| L11 | 🟢 LOW | `useUnifiedFeed.ts` | **`list_follows` called sequentially after feed**: Should be parallelized with the feed fetch. | +100–300ms |
| L12 | 🟢 LOW | IPC boundary | **JSON serialization overhead**: Large feed responses (~50 posts with embeds) serialize/deserialize through Tauri's JSON IPC. | +20–50ms |

### Cold Start Waterfall (All Tab)

```
Time →   0ms                     500ms                    1000ms                   1500ms                   2000ms+
         ├────────────────────────┼────────────────────────┼────────────────────────┼────────────────────────┤
IPC #1:  ▓▓▓▓ list_saved_feeds (150-300ms)
                                  │
IPC #2:  ░░░░░░░░░░░░░░░░░░░░░░░▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ get_timeline_unified (500-1500ms)
                                                            ├─ timeline API (300-800ms)
                                                            ├─ custom feeds (200-1000ms) ── parallel
                                                            └─ vault query (20-100ms) ───── parallel
                                                                                    │
RENDER:  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░▓ First paint
                                                                                    │
IMAGES:  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░▓▓▓▓▓▓▓▓ 20× get_media_blob (sequential)
                                                                                              +500-2000ms
```

**Worst case** (Subscribed tab, cold cache, many feeds): 3 serial IPC calls + N custom feed network requests + 20 blob resolutions = **3–5 seconds to interactive**.

The "~20 seconds" user experience likely includes video FFmpeg processing stacking on top of this feed loading — see Part 2.

---

## Part 2: Video Playback Pipeline

### Data Flow (Current)

```
┌─ VideoPlayer.tsx ─────────────────────────────────────────┐
│                                                           │
│  1. IntersectionObserver triggers when card scrolls in    │
│  2. invoke('resolve_hls_video', { playlistUrl })          │
│     ↓  ── blocks; shows "⏳ Loading native media..." ──   │
│                                                           │
│  ┌─ resolve_hls_video (Rust) ──────────────────────────┐  │
│  │                                                     │  │
│  │  Check MP4 cache → miss                             │  │
│  │  Check TS cache → miss                              │  │
│  │                                                     │  │
│  │  Strategy 1: FFmpeg native HLS → MP4                │  │
│  │    ffmpeg -i <m3u8_url>                             │  │
│  │           -c:v libx264 -preset ultrafast -crf 23    │  │
│  │           -c:a aac -b:a 128k                        │  │
│  │           -fps_mode cfr                             │  │
│  │           -movflags +faststart                      │  │
│  │    ──── FULL H.264 RE-ENCODE (5–20 sec CPU) ────   │  │
│  │                                                     │  │
│  │  If Strategy 1 fails:                               │  │
│  │  Strategy 2: Manual segment download                │  │
│  │    Download N segments (6 concurrent)               │  │
│  │    Concatenate → .ts file                           │  │
│  │    remux_ts_to_mp4() ← ALSO full re-encode          │  │
│  │    ──── ANOTHER 5–20 sec CPU ────                   │  │
│  │                                                     │  │
│  └─ Returns file path ────────────────────────────────┘  │
│                                                           │
│  3. Construct http://127.0.0.1:{port}/{filename}          │
│  4. NOW render <video src={url}> (zero progressive play)  │
│                                                           │
└───────────────────────────────────────────────────────────┘
```

### Issue Catalog

| # | Severity | Component | Issue | Impact |
|---|----------|-----------|-------|--------|
| V1 | 🔴 CRITICAL | `media.rs` | **Full H.264 re-encode** on both FFmpeg strategies. `libx264 ultrafast crf 23` re-encodes every video frame + `aac` re-encodes audio. The code comments say "AAC passthrough" but the actual flags are `-c:a aac -b:a 128k` (re-encode, not `-c:a copy`). | 5–20s delay per video, 100% CPU core |
| V2 | 🔴 CRITICAL | `VideoPlayer.tsx` | **No progressive playback**: `<video>` element is NOT rendered until FFmpeg completes AND returns. User stares at "⏳ Loading native media..." for the full encode duration. | Perceived wait = FFmpeg wall time |
| V3 | 🟠 HIGH | `media.rs` | **`ultrafast` preset inflates output**: CRF 23 with ultrafast produces files 2–3× larger than source because the encoder skips motion estimation. Playback bitrate is higher than necessary. | Larger files, more disk I/O, potential stutter on playback |
| V4 | 🟠 HIGH | `VideoPlayer.tsx` | **No FFmpeg cancellation**: If user scrolls past a video while it's encoding, the FFmpeg process runs to completion. Multiple videos visible = multiple concurrent FFmpeg processes. | CPU thrashing, delayed subsequent videos |
| V5 | 🟡 MEDIUM | `media.rs` | **Strategy 2 double-encode path**: Manual download concatenates segments → .ts → calls `remux_ts_to_mp4()` → full re-encode. The concatenated .ts already has clean timestamps from sequential download. A `-c copy` remux would suffice here. | Extra 5–20s on fallback path |
| V6 | 🟡 MEDIUM | Frontend | **Duplicate video instances**: When expanding a video (inline → gallery), both `<video>` elements remain in the DOM. Two decode pipelines running simultaneously. | Double memory/CPU, potential audio overlap |

### Why Full Re-encode Exists (Context)

The commit history and code comments reveal the reasoning: HLS streams from Bluesky have PTS (Presentation Timestamp) discontinuities at segment boundaries. Using `-c copy` (stream copy/remux) preserves these discontinuities, causing stutter in WebKitGTK's media pipeline which is less forgiving than Chrome/Firefox.

The backend agent chose to solve this with a full re-encode (`libx264 ultrafast`) to regenerate all timestamps. **This is the nuclear option** — it works, but at enormous CPU cost.

---

## Part 3: Recommended Fixes

### Priority 1: Video — Eliminate Full Re-encode (Biggest Win)

**Problem:** Every video incurs 5–20 seconds of CPU-blocking encode before first frame.

**Recommended approach — tiered strategy:**

```
Strategy A (preferred): Stream-copy with PTS fixup
  ffmpeg -y -fflags +genpts+discardcorrupt
         -avoid_negative_ts make_zero
         -i <input>
         -c:v copy                    ← NO re-encode
         -c:a copy                    ← NO re-encode  
         -bsf:v h264_mp4toannexb     ← fix NALU framing if needed
         -fps_mode cfr
         -movflags +faststart
         -f mp4 <output>

  Expected time: <1 second (I/O bound, not CPU bound)

Strategy B (fallback if A produces stutter):
  ffmpeg ... -c:v copy -c:a aac -b:a 128k ...
  
  Re-encode audio only (cheap), copy video. Fixes most AAC-in-TS
  issues while keeping video stream-copy speed.

Strategy C (current — last resort):
  Full libx264 re-encode. Only use if the specific stream
  fails Strategy A and B validation.
```

**Implementation plan:**
1. Try Strategy A first (stream copy). Validate output with `ffprobe` — check for PTS jumps > 100ms.
2. If validation fails, fall back to Strategy B (audio re-encode only).
3. If Strategy B also fails, fall back to current Strategy C (full re-encode).
4. Cache which strategy worked per-URL so repeats are fast.

**Expected improvement:** Video load time drops from 5–20s → <1s for ~90% of videos. The remaining 10% (corrupted streams) get the full re-encode as fallback.

### Priority 2: Wire `resolve_blob_urls_batch` (Already Built)

**Problem:** Frontend resolves images 1-at-a-time. Backend batch command exists but is unused.

**What exists:**
- `resolve_blob_urls_batch` command in [media.rs](bytescape/src-tauri/src/commands/media.rs#L1021) — takes `Vec<String>`, returns `HashMap<String, String>`
- Already registered in `lib.rs` invoke_handler

**What to change (Frontend only):**
- After feed data arrives in `useUnifiedFeed.ts`, collect all `blob:hash` references from post embeds
- Single `invoke('resolve_blob_urls_batch', { sources: [...] })` call
- Populate an in-memory URL map that `useBlobUrl` reads from (skip individual IPC)

**Expected improvement:** 20 blob resolutions go from 20 sequential IPC calls (~500–2000ms) → 1 IPC call (~50–100ms). **10–20× speedup on image loading.**

### Priority 3: Parallelize Frontend IPC Calls

**Problem:** `list_saved_feeds` → `get_unified_feed` runs sequentially. `list_saved_feeds` is also called redundantly (frontend calls it, then `get_timeline_unified` calls it again internally).

**Fix for All tab:**
```typescript
// CURRENT (serial, redundant):
const feedsData = await invoke('list_saved_feeds');  // IPC #1 — 150-300ms
const response = await invoke('get_timeline_unified', { feedUris });  // IPC #2 — calls list_saved_feeds again internally

// PROPOSED (parallel, deduplicated):
const [feedsData, response] = await Promise.all([
  invoke('list_saved_feeds'),
  invoke('get_timeline_unified', { feedUris: [], limit: 50, cursor: null, vaultOffset: 0 }),
]);
// OR: Remove the frontend list_saved_feeds call entirely — get_timeline_unified already handles it.
```

**Fix for Subscribed tab:**
```typescript
// CURRENT (3 serial calls):
const feedsData = await invoke('list_saved_feeds');     // IPC #1
const tags = await invoke('list_followed_hashtags');     // IPC #2
const [posts, cur] = await invoke('get_unified_feed', { tags, feedUris }); // IPC #3

// PROPOSED (parallel where possible):
const [feedsData, tags] = await Promise.all([
  invoke('list_saved_feeds'),
  invoke('list_followed_hashtags'),
]);
// feeds + tags needed for IPC #3, so only 2-step chain instead of 3
const [posts, cur] = await invoke('get_unified_feed', { tags, feedUris: feedsData.map(f => f.uri) });
```

**Expected improvement:** Shaves 200–500ms off initial load.

### Priority 4: Extend Feed Cache to Cover Subscribed/Custom

**Problem:** `FeedCache` only caches the "timeline" sub-query. Subscribed tab and custom feeds always hit the network.

**Fix:** Add cache keys for:
- `subscribed:{feed_uri}:{cursor}` — cache each custom feed response (30s TTL)
- `hashtag:{tag}:{cursor}` — cache hashtag feed responses

This is a backend-only change in `feed_aggregation.rs`.

**Expected improvement:** Subscribed tab loads drop from 1–3s → <200ms on cache hit (which is the common case with 45s auto-refresh).

### Priority 5: Progressive Video Playback

**Problem:** Even with stream-copy (Priority 1), there's still a moment where the video file is being written. Currently, `<video>` isn't rendered until the file is fully written.

**Approach:** 
- For HLS sources, render the `<video>` element immediately with a poster frame
- Start FFmpeg in background, emit Tauri event when complete
- On event, swap `src` to the local file
- User sees poster → instant swap to playable video (perceived zero wait)

**Alternative (simpler):** If WebKitGTK supports it, serve the TS file directly (no MP4 conversion) for immediate playback, then swap to MP4 when available. Most browsers handle MPEG-TS in `<video>` tags.

### Priority 6: Cancel FFmpeg on Scroll-Away

**Problem:** FFmpeg processes run to completion even when user scrolls past. Multiple concurrent encodes thrash CPU.

**Fix:**
- Track active FFmpeg process PIDs in a `HashMap<String, Child>` on `AppState`
- Expose `cancel_video_resolve(url: String)` command
- Frontend's `VideoPlayer` calls cancel in its `useEffect` cleanup
- Kill FFmpeg process on cancel, clean up partial files

---

## Part 4: Architecture Observations

### What the Backend Agent Got Right

1. **Parallel sub-queries in `get_timeline_unified`**: Uses `tokio::join!` to run timeline + custom feeds + vault simultaneously. This is correct architecture.
2. **Feed cache concept**: The `FeedCache` with TTL is the right pattern, just needs broader coverage.
3. **Warp media server**: Bypassing WebKitGTK's `asset://` protocol issues with a localhost HTTP server was a good engineering decision.
4. **Batch blob resolution command**: `resolve_blob_urls_batch` was designed correctly — it's just not wired up on the frontend side.
5. **IntersectionObserver lazy loading**: Both `VideoPlayer` and `useLazyBlobUrl` use viewport-aware loading. Good pattern.

### What Needs Architectural Attention

1. **Misleading code comments**: `ffmpeg_hls_to_mp4` comment says "Audio is copied (AAC passthrough)" but actual flags are `-c:a aac -b:a 128k` (re-encode). `remux_ts_to_mp4` function name says "remux" but does a full re-encode. These misleading names make it hard for other agents to understand true behavior.

2. **Cross-track gap**: The batch blob resolution is a textbook example of a Backend ↔ Frontend handoff gap. Backend built it (correctly), Frontend never adopted it. This is exactly the kind of issue the PM should catch in sprint planning.

3. **No observability**: There are `println!` debug statements but no structured timing metrics. Adding `Instant::now()` + elapsed timing to the feed and video paths would make performance regression detection trivial.

---

## Part 5: Estimated Impact

| Fix | Effort | Feed Load Impact | Video Impact |
|-----|--------|-----------------|--------------|
| P1: Stream-copy video | Backend: ~2 hours | — | **-5 to -20 seconds** per video |
| P2: Wire batch blob resolution | Frontend: ~1 hour | **-500 to -2000ms** | — |
| P3: Parallelize IPC calls | Frontend: ~30 min | **-200 to -500ms** | — |
| P4: Extend feed cache | Backend: ~1 hour | **-500 to -2000ms** (cache hit) | — |
| P5: Progressive video playback | Both: ~3 hours | — | **-1 to -3s perceived** |
| P6: FFmpeg cancellation | Both: ~2 hours | — | Prevents CPU thrashing |

**Combined expected improvement:**
- Feed first paint: from ~2–5s → ~0.5–1.5s (60–70% reduction)
- Video first frame: from ~5–20s → ~0.5–2s (90% reduction)
- Overall perceived "20 second" experience → ~2–3s

---

## Appendix: Source File References

| File | Lines | Key Functions |
|------|-------|---------------|
| `src/hooks/useUnifiedFeed.ts` | 150–260 | Serial IPC chain, tab-specific fetch logic |
| `src/hooks/useBlobUrl.ts` | 1–169 | Individual blob resolution (should use batch) |
| `src/components/VideoPlayer.tsx` | 1–185 | Video loading, FFmpeg integration point |
| `src-tauri/src/commands/media.rs` | 590–660 | `resolve_hls_video` — HLS cache + strategy dispatch |
| `src-tauri/src/commands/media.rs` | 795–835 | `ffmpeg_hls_to_mp4` — full re-encode flags |
| `src-tauri/src/commands/media.rs` | 845–875 | `remux_ts_to_mp4` — also full re-encode (misnamed) |
| `src-tauri/src/commands/media.rs` | 1021–1113 | `resolve_blob_urls_batch` — unused batch command |
| `src-tauri/src/commands/feed_aggregation.rs` | 190–350 | `get_timeline_unified` — parallel fetch + cache |
