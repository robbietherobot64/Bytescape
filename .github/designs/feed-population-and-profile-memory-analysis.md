# Feed Population & Profile Memory — Systems Analysis

**Date:** 2026-03-28  
**Context:** Post-P0 feed pipeline rework (`167110b`). RSS down from 17-25 GB to 5-7 GB on feed page.  
**Remaining issues:**  
1. Feed shows mostly user's own posts, handful of entries total  
2. Profile page + Instagram filter causes 24 GB RSS that persists after navigation

---

## Issue 1: Anemic Feed Population

### Root Cause Chain

Three bugs compound to produce a feed that's mostly the user's own posts:

```
Bug #1: Silent timeline failure
    → Bluesky/Mastodon API error is swallowed
    → Only vault items remain
    → Bug #2 kicks in ──┐
                        │
Bug #2: Vault filter too restrictive
    → list_my_items_paged() only returns items where:
       a) is_liked == true, OR
       b) author_did == owner_did (user's own), OR
       c) author_did is None (archive imports without DID)
    → All items from OTHER people are excluded
    → "All" tab vault component = only user's own posts
                        │
Bug #3: Empty feedUris on initial "All" tab fetch
    → get_timeline_unified called with tags:[], feedUris:[]
    → has_custom = false → custom feeds codepath skipped
    → Only timeline + vault (user's own) returned
    → "Load More" correctly passes savedFeeds URIs, but page 1 is impoverished
```

### Detailed Findings

#### F1 — Silent Timeline Failure (feed_aggregation.rs:388-392)

```rust
Err(e) => {
    eprintln!("get_timeline_unified: live timeline fetch FAILED: {}", e);
    // ← execution continues, returns Ok(...) with only vault items
}
```

When the Bluesky timeline API fails (expired session, rate limit, network error), the error is logged to stderr but **swallowed**. The function returns `Ok(UnifiedFeedResponse { items: [vault_only], ... })`. The frontend receives no error signal and renders the partial result as if it were the complete feed.

**This is the most likely direct cause of the "only my own posts" symptom.** If the Bluesky session token has expired, every feed load silently falls back to vault-only items.

**Fix:** Return error information alongside partial results:
- Add `warnings: Vec<String>` to `UnifiedFeedResponse`  
- Populate with specific failure messages per data source  
- Frontend displays a non-blocking banner: "⚠ Bluesky feed unavailable — showing cached content"  
- Trigger session refresh on timeline auth failure

#### F2 — Vault Filter Is "Mine-Only" (vault.rs:372-384)

`list_my_item_headers()` is the gatekeeper for all vault queries feeding into the "All" tab. Its filter:

```rust
filter(|item| {
    item.is_liked                                    // liked items
    || item.author_did.as_deref() == Some(owner_did) // user's own posts
    || item.author_did.is_none()                     // archive imports (no DID)
})
```

This excludes vault items authored by **other people** — items received via P2P sync, items imported from shared archives, items from followed accounts that got cached locally. The "All" tab is supposed to be a unified view, but its vault component only surfaces the user's own content.

**Fix:** For the "All" tab's vault query, use a broader filter (or no author filter at all). The `list_my_items_paged` function needs a mode that returns all vault items, not just "mine." Options:
- Add a `include_others: bool` parameter to `list_my_items_paged`
- Create a separate `list_all_vault_items_paged()` function
- Use different vault queries per tab: "You" tab = mine-only, "All" tab = everything

#### F3 — Empty feedUris/tags on Initial "All" Fetch (useUnifiedFeed.ts:352-353)

```typescript
provider.getTimelineUnified({
    tags: [],        // ← always empty on initial load
    feedUris: [],    // ← always empty on initial load
    ...
})
```

The `savedFeeds` data is fetched in the same `Promise.all` as `getTimelineUnified`, so it isn't available yet when the unified call fires. In the backend, `has_custom = !tags.is_empty() || !feed_uris.is_empty()` evaluates to `false`, so custom feeds are entirely skipped.

Compare with "Load More" (useUnifiedFeed.ts:597) which correctly passes `feedUris: customFeedUris(savedFeeds)` because by that point `savedFeeds` is populated in state.

**Fix:** Two options:
1. **Restructure the call chain:** Fetch `savedFeeds` first, then pass the URIs to `getTimelineUnified`. Adds ~100ms latency for the sequential call.
2. **Move feed resolution to backend:** Have `get_timeline_unified` call `list_saved_feeds` internally when `feedUris` is empty, rather than relying on the frontend to pass them. The backend already has access to the saved feeds. This is the better fix — it makes the backend self-contained and eliminates the coordination bug entirely.

#### F4 — Vault Dedup Replaces Timeline Posts with Vault Copies

In `get_timeline_unified` (feed_aggregation.rs:394-426), vault items' share_history URIs are collected, and timeline entries matching those URIs are **removed** (`feed.retain` excludes matches). The vault version replaces them. Since vault items are filtered to "mine" (F2), this effectively replaces other people's posts from the timeline with the user's own syndicated copies, further reducing diversity.

### Feed Data Flow — Current vs. Expected

```
CURRENT (broken):
┌─────────────┐     ┌──────────────┐     ┌──────────────┐
│ Bluesky API │────►│ FAILS silent │     │ Custom feeds │
│ get_timeline│     │ (no error    │     │ NOT CALLED   │
│             │     │  to frontend)│     │ (empty URIs) │
└─────────────┘     └──────────────┘     └──────────────┘
                           │
                           ▼
                    ┌──────────────┐
                    │ Vault query  │
                    │ "mine only"  │──── only user's posts
                    └──────────────┘


EXPECTED (fixed):
┌─────────────┐     ┌──────────────┐     ┌──────────────┐
│ Bluesky API │     │ Custom feeds │     │ Vault items  │
│ get_timeline│     │ (backend     │     │ (ALL items,  │
│ (retry on   │     │  resolves    │     │  not just    │
│  auth fail) │     │  URIs if     │     │  "mine")     │
│             │     │  empty)      │     │              │
└──────┬──────┘     └──────┬───────┘     └──────┬───────┘
       │                   │                     │
       └───────────────────┴─────────────────────┘
                           │
                    ┌──────┴───────┐
                    │ Merge + dedup│
                    │ Sort by time │
                    │ 200+ entries │
                    └──────────────┘
                           │
              ┌────────────┼────────────┐
              │            │            │
         If any source failed:     All sources OK:
         Return items +            Return items
         warnings[]                (rich, diverse)
```

### Recommended Implementation (Priority Order)

| # | Fix | File(s) | Effort |
|---|-----|---------|--------|
| **1** | Backend resolves savedFeeds internally when `feedUris` is empty | `feed_aggregation.rs` | 1-2 hours |
| **2** | Add `warnings` field to `UnifiedFeedResponse` + surface in UI | `feed_aggregation.rs`, `useUnifiedFeed.ts`, `FeedScreen.tsx` | 2-4 hours |
| **3** | Retry/refresh Bluesky session on timeline failure before giving up | `feed_aggregation.rs`, `import.rs` | 2-3 hours |
| **4** | Broader vault filter for "All" tab (include non-owned items) | `vault.rs`, `feed_aggregation.rs` | 1-2 hours |
| **5** | Pass `has_custom` flag or feed URIs consistently in Load More | `useUnifiedFeed.ts` | 30 min |

---

## Issue 2: Profile Page Memory Leak (24 GB + No Release)

### Root Cause: Architectural Mismatch

The profile page has **zero memory management** while rendering the most media-dense content in the app (Instagram imports = nearly 100% media). Every protection added to FeedScreen is absent from ProfileScreen.

### The `display: none` Tab Retention Problem

`App.tsx` (line 726) renders ALL open tabs simultaneously:

```tsx
style={isActive ? ... : { display: 'none' }}
```

**When the user navigates away from Profile, the component is NOT unmounted.** It's hidden with CSS. This means:
- All React state (`items[]` — potentially hundreds of BytescapeItems) persists
- No `useEffect` cleanup functions fire
- All video `<video>` elements remain in the DOM with GStreamer pipelines alive
- All `IntersectionObserver` instances stop firing (elements not intersecting because hidden) but the observers stay allocated
- Blob URLs remain unreleased

### ProfileScreen vs. FeedScreen — Protection Comparison

| Protection | FeedScreen ✅ | ProfileScreen ❌ |
|---|---|---|
| **Virtualization** (limits DOM nodes) | `VirtualizedFeedList` (~50 nodes) | Flat `items.map()` (unlimited) |
| **State cap** (limits array size) | `MAX_FEED_ENTRIES = 300` | No cap — unbounded `[...prev, ...new]` |
| **Video pipeline release** on filter | `VideoPlaybackManager.releaseAll()` | Nothing |
| **Video pipeline release** on tab switch | `VideoPlaybackManager.releaseAll()` | Nothing |
| **Viewport-aware prefetch** | Visible + 5 lookahead only | Eagerly loads all media for all items |
| **Video budget** | Pruned at 6 registered elements | No management |

### Instagram Filter Cascade

Instagram imports are nearly 100% media — posts, reels, stories. When the user filters to Instagram on the Profile Posts tab:

```
1. list_vault_posts_paged(source: "instagram") returns 20 items
   → Every item has media_urls populated
   
2. Each item renders FeedEntryCard → BlobImage/VideoPlayer
   → Each video creates a GStreamer pipeline (100-500 MB native memory)
   → Each image resolves via useBlobUrl → IPC → disk read
   
3. User clicks "Load More" (3 times)
   → 80 FeedEntryCards now in DOM, no virtualization
   → 30+ video reels × 200 MB avg = ~6 GB just from GStreamer
   → 50+ images × blob resolution
   
4. User navigates away from Profile
   → display: none hides the tab
   → Components NOT unmounted
   → GStreamer pipelines NOT released
   → 6+ GB stuck in native memory
   
5. User does other things, maybe returns to profile
   → Previous items still in state + new items loaded
   → GStreamer pipelines accumulate further
   → 12-24 GB total
```

### Data Flow Diagram — Profile Memory Accumulation

```
┌──────────────────────────────────────────────────────────┐
│  ProfileScreen (display: none when not active)           │
│                                                          │
│  items: BytescapeItem[]  ← UNBOUNDED, never released     │
│                                                          │
│  items.map(item =>                                       │
│    <FeedEntryCard>       ← ALL in DOM (no virtualization)│
│      <BlobImage>         ← blob URL per image            │
│      <VideoPlayer>       ← GStreamer pipeline per video  │
│        IntersectionObs   ← stops firing under display:none│
│    </FeedEntryCard>                                      │
│  )                                                       │
│                                                          │
│  On "Load More":                                         │
│    setItems(prev => [...prev, ...new])  ← grows forever  │
│                                                          │
│  On source filter change:                                │
│    setItems([])  ← resets, BUT old items' video elements │
│    are still being torn down by GStreamer async           │
│    NO VideoPlaybackManager.releaseAll() call             │
│                                                          │
│  On navigate away:                                       │
│    display: none  ← NOTHING cleaned up                   │
│                                                          │
└──────────────────────────────────────────────────────────┘
         │
         ▼  NATIVE MEMORY (invisible to JS profiler)
┌──────────────────────────────────────────────────────────┐
│  GStreamer Pipelines (WebKitGTK)                         │
│                                                          │
│  Pipeline #1  ████████████████████  200 MB               │
│  Pipeline #2  ████████████████████  300 MB               │
│  Pipeline #3  ████████████████████  150 MB               │
│  ...                                                     │
│  Pipeline #20 ████████████████████  400 MB               │
│                                                          │
│  Total: 4-8 GB native memory, never released             │
│  (display:none prevents IntersectionObserver cleanup)    │
└──────────────────────────────────────────────────────────┘
```

### VaultScreen Also Affected

`VaultScreen` has a similar but less severe pattern:
- Loads ALL files at once via `vault_list_files()` (no pagination)
- Module-level `thumbCache` is capped at 200 entries ✅ (already fixed)
- But thumbnails for all visible files are resolved eagerly
- No virtualization of the file grid

### Recommended Implementation (Priority Order)

| # | Fix | Impact | Effort |
|---|-----|--------|--------|
| **1** | Add `VirtualizedFeedList` to ProfileScreen Posts tab | Caps DOM nodes from unlimited → ~50. Single biggest win. | 2-3 hours |
| **2** | Add `VideoPlaybackManager.releaseAll()` on profile source filter change | Prevents pipeline accumulation when switching instagram → bluesky → instagram | 15 min |
| **3** | Add `MAX_PROFILE_ENTRIES` cap to ProfileScreen state | Prevents unbounded state growth on Load More | 15 min |
| **4** | Tab visibility lifecycle — release pipelines when tab becomes `display: none` | Prevents zombie pipelines when navigating away from any screen | 1-2 hours |
| **5** | Optional: Lazy-unmount inactive tabs after timeout | After N seconds of `display: none`, actually unmount the component tree to release all resources. Re-mount on re-activation. | 4-6 hours |

### Fix #4 Detail: Tab Visibility Lifecycle

The `display: none` pattern is intentional for instant tab switching, but it creates a memory retention problem. The cleanest fix is a **tab visibility callback**:

```
App.tsx detects tab switch
  → Fires onTabDeactivated(tabId) callback
  → Each screen component (FeedScreen, ProfileScreen, etc.) responds:
    - VideoPlaybackManager.releaseAll() 
    - Pause any active video/audio
    - Optionally trim state (clear items beyond first page)
```

This preserves instant tab switching (React state remains) while releasing the expensive native resources (GStreamer pipelines, blob URLs).

### Fix #5 Detail: Lazy Unmount (More Aggressive)

For screens like ProfileScreen that accumulate large state, consider:

```tsx
// In App.tsx tab rendering:
const [mountedTabs, setMountedTabs] = useState(new Set());

// Mount tab when activated, unmount after 60s of inactivity
useEffect(() => {
    mountedTabs.add(activeTabId);
    const timer = setTimeout(() => {
        for (const id of mountedTabs) {
            if (id !== activeTabId) mountedTabs.delete(id);
        }
    }, 60000);
    return () => clearTimeout(timer);
}, [activeTabId]);

// Only render mounted tabs
{tabs.filter(t => mountedTabs.has(t.id)).map(tab => ...)}
```

This trades instant switching (slight re-mount delay for idle tabs) for guaranteed resource cleanup. The 60-second window means recently-used tabs stay mounted for fast return.

---

## Combined Priority Matrix

| Priority | Issue | Fix | Expected Impact |
|---|---|---|---|
| **P0** | Feed: silent timeline failure | Add warnings + session retry | Rich feed when auth is valid |
| **P0** | Feed: empty feedUris on initial "All" | Backend resolves feeds internally | Custom feed content appears on page 1 |
| **P1** | Profile: no virtualization | Add VirtualizedFeedList to ProfileScreen | DOM nodes: unlimited → ~50 |
| **P1** | Profile: no video cleanup | Add releaseAll() on filter + tab deactivation | GStreamer pipelines released |
| **P1** | Profile: unbounded state | Add MAX_PROFILE_ENTRIES cap | State capped at ~200 items |
| **P2** | Feed: vault "mine-only" filter | Broader filter for "All" tab | Vault diversity in feed |
| **P2** | Profile: tab visibility lifecycle | onTabDeactivated callback in App.tsx | Cross-screen resource cleanup |
| **P3** | Profile: lazy unmount idle tabs | Unmount after 60s inactivity | Guaranteed full cleanup |

---

## Summary

The feed population problem is a **data pipeline bug** — three independent failures (silent API errors, empty parameters, restrictive vault filter) compound to show only the user's own posts. The fix is straightforward: make the backend self-sufficient for feed resolution and surface failures to the user.

The profile memory problem is an **architectural gap** — ProfileScreen was never given the memory protections that FeedScreen received. It renders media-dense Instagram content with no virtualization, no pipeline management, no state caps, and no cleanup on navigation. The `display: none` tab retention pattern (designed for instant switching) prevents component unmount, so nothing self-cleans.

Both issues are addressable with surgical fixes (P0/P1 items above). The broader architectural lesson: **every screen that renders media needs the same memory discipline as FeedScreen** — virtualization, pipeline budgets, state caps, and explicit cleanup on deactivation.
