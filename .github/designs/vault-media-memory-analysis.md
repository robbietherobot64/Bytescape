# Vault Media Memory Analysis
### Systems Advisory — March 28, 2026
### Root cause analysis: memory climb when filtering to user's own items

---

## Executive Summary

The feed now loads the correct number of items (feed population fixes confirmed working). The remaining memory issue is **isolated to vault media** — when the user filters to their own items, all media comes from the local Iroh blob store rather than remote HTTPS URLs, and the memory climb is caused by **three compounding factors**:

1. **Full-resolution images decoded by WebKitGTK** — imported photos are stored as-is (4000×3000+ originals) and served to the feed at their original resolution. WebKitGTK decodes each to an RGBA bitmap in its render pipeline: a single 12MP photo = **48 MB decoded**. With 15-20 feed items visible, each having 1-4 photos, that's **720 MB – 3.8 GB** of decoded bitmaps.

2. **GStreamer video pipelines** — each `<video>` element with a `src` creates a native GStreamer pipeline consuming **100–500 MB**. The `MAX_ACTIVE_PIPELINES = 6` cap still permits up to **3 GB** of native pipeline memory.

3. **WebKitGTK's `no-store` cache policy paradox** — the media server sends `Cache-Control: no-store` to prevent *video* double-buffering, but this also forces WebKitGTK to **re-decode images on every render** instead of caching the decoded bitmap. Each scroll-induced virtualizer remount re-fetches and re-decodes the full-resolution image.

The JS-side pipeline is **clean** — no Object URL leaks, capped string caches (500 entries), file paths over IPC (zero binary data), semaphored blob fetches. The memory growth is entirely in **WebKitGTK's native rendering layer** (image decode buffers + GStreamer pipelines), which is invisible to JS heap profiling.

---

## Pipeline Trace (Vault Items)

```
BytescapeItem (from redb)
  → FeedEntry.media_urls = ["blob:abc123", "blob:def456"]     ← full-res originals
  → FeedEntry.media_thumbnails = ["blob:thumb1", "blob:thumb2"] ← 256px WebP thumbs
  → FeedEntryCard
      → mediaSources = entry.media_urls.map(...)               ← FULL RESOLUTION
      → thumbnailSources = entry.media_thumbnails.map(...)     ← only used for video poster
      → MediaGrid
          → BlobImage src={mediaSources[i]}                    ← loads FULL-RES blob
          → useLazyBlobUrl → invoke('get_media_blob')
              → Rust: stream blob to blob_cache/ on disk       ← zero memory (✅)
              → Returns file path → blobCacheUrl() → http://127.0.0.1:{port}/{hash}.jpg
          → WebKitGTK HTTP fetch from local server
              → Server sends file with Cache-Control: no-store  ← forces re-decode (❌)
              → WebKitGTK decodes 4000×3000 JPEG → 48 MB RGBA  ← THE MEMORY COST
```

### Key Finding: Images use `mediaSources` (full-res), not `thumbnailSources` (256px thumbs)

The feed card renders full-resolution originals for **images**. Thumbnails are only used as `poster` frames for **videos**. This means a user's imported Facebook/Instagram photo archive — containing 2000×1500 to 4032×3024 phone photos — gets decoded at full resolution for a 300×200px feed card display.

**This is the single largest contributor.** The image data flowing through the pipeline is 10-50× larger than what's needed for display.

---

## Memory Cost Model

### Images (dominant cost)

| Resolution | JPEG Size | Decoded RGBA | Feed Display Size |
|-----------|-----------|-------------|-------------------|
| 1920×1080 | 300 KB | **8.3 MB** | 300×170 px |
| 3024×4032 | 2.5 MB | **48.8 MB** | 300×400 px |
| 4032×3024 | 3.0 MB | **48.8 MB** | 300×225 px |

With virtualizer showing **15 items** + **5 overscan** = 20 items, each with avg 2 images:
- **Low-res imported (1080p):** 40 images × 8.3 MB = **332 MB**
- **Phone photos (12MP):** 40 images × 48.8 MB = **1.95 GB**

With `no-store`, scrolling causes **re-decode** of already-viewed images, so memory temporarily spikes higher during scroll before GC can reclaim.

### Videos

| Component | Memory Per Pipeline | Max Concurrent | Total |
|-----------|-------------------|----------------|-------|
| GStreamer pipeline | 100–500 MB | 6 (MAX_ACTIVE) | 600 MB – 3 GB |

### Combined worst case (photo-heavy Instagram import)

| Source | Memory |
|--------|--------|
| Decoded images (20 items × 2 photos × 48 MB) | **1.95 GB** |
| GStreamer pipelines (6 active) | **1.8 GB** |
| JS heap (string caches, React state) | **~50 MB** |
| Iroh/redb overhead | **~200 MB** |
| **Total** | **~4 GB** |

This matches the user's observed behavior — memory climbs specifically when filtering to their own items (which are all vault items with high-res imported media).

---

## Root Causes (Ranked by Impact)

### 🔴 P0: Full-resolution images served for feed card display

**File:** `MediaGrid.tsx` line 46 — `BlobImage src={mediaSources[i]}`
**File:** `FeedEntryCard.tsx` line 90 — `mediaSources = entry.media_urls.map(...)`

The `media_urls` field contains references to the **original full-resolution blob**. No intermediate "display-size" variant exists between the 256px thumbnail and the full original. The feed card renders the full original at ~300px display width, wasting 90-95% of the decoded pixel data.

**Fix:** Generate a **display-size variant** (max 1200px on longest edge) during import, or on-demand during blob resolution. Serve this for feed display; reserve the full original for the gallery overlay.

### 🔴 P0: Media server `no-store` cache policy prevents image caching

**File:** `media_server.rs` line 33 — `"cache-control", "no-store, no-cache, must-revalidate"`

This was added to prevent **video** double-buffering (GStreamer pipeline + HTTP cache = 2× memory). But it also affects **images**, forcing WebKitGTK to re-fetch and re-decode on every virtualizer remount. Since the virtualizer removes DOM nodes on scroll-out and recreates them on scroll-in, the same image gets decoded repeatedly.

**Fix:** Use content-type-aware cache headers: `no-store` for video, `max-age=300` for images. Or split the Warp route into two handlers based on file extension.

### 🟡 P1: No display-size image generation at import time

**File:** `connectors/facebook.rs` line 1503, `connectors/instagram.rs` line 452

Both import pipelines call `store_media_with_thumbnail(bytes, 256)` which generates only a 256×256 WebP thumbnail. There's no intermediate "display-size" variant (800-1200px) suitable for feed rendering. The gap between 256px thumb and 4032px original is enormous.

**Fix:** Add a second size tier: `store_media_with_display_variant(bytes, 1200)` that generates a JPEG/WebP at 1200px max dimension alongside the 256px thumb.

### 🟡 P1: GStreamer pipeline count still too high for vault browsing

The `MAX_ACTIVE_PIPELINES = 6` and `VideoPlaybackManager.MAX_REGISTERED = 6` caps were set for mixed-source feeds. When filtering to a video-heavy vault (Instagram reels), all 6 slots fill with video pipelines = up to 3 GB native memory.

**Fix:** When source filter is an archive source (facebook, instagram), reduce `MAX_ACTIVE_PIPELINES` to 2-3. Or: don't autoplay vault videos — show thumbnail with play button, only create pipeline on user click.

### 🟢 P2: Blob disk cache never evicted

**File:** `commands/media.rs` — `blob_cache/` directory

Every blob ever resolved stays on disk permanently. Not a RAM issue, but for users with large archives (10GB+ of imported media), this duplicates the entire media store. Eventually impacts disk space.

**Fix:** LRU eviction based on last-access time, capped at e.g. 2 GB disk usage.

---

## Recommended Architecture: Display-Size Media Tier

The fundamental fix is to introduce a **three-tier media system**:

```
Tier 1: Thumbnail (256px WebP)     ← exists today, used for VaultScreen grid
Tier 2: Display   (1200px JPEG)    ← NEW — used for feed cards, profile posts
Tier 3: Original  (full-res)       ← exists today, used for gallery overlay
```

### Implementation Design

#### Backend (Track A): `get_media_blob_display` command

```rust
/// Like get_media_blob, but returns a display-size (max 1200px) variant.
/// If the display variant doesn't exist in cache, generates it on-the-fly
/// from the original blob and caches the result.
///
/// Cache key: {hash}_display.{ext} in blob_cache/
#[tauri::command]
pub async fn get_media_blob_display(
    hash_str: String,
    max_dimension: Option<u32>,  // default 1200
    state: State<'_, AppState>,
) -> Result<String, String> {
    let max_dim = max_dimension.unwrap_or(1200);
    let display_key = format!("{}_display_{}", hash, max_dim);
    
    // Check display cache first
    // If miss: fetch original → resize → cache display variant → return path
    // Use existing blob_semaphore to limit concurrency
}
```

#### Backend (Track A): Content-aware cache headers in media server

```rust
// media_server.rs — split route by content type
let route = warp::fs::dir(cache_dir.clone())
    .map(|file: warp::fs::File| {
        let path = file.path().to_string_lossy().to_string();
        let is_video = path.ends_with(".mp4") || path.ends_with(".mov") 
                    || path.ends_with(".webm") || path.ends_with(".mkv");
        
        if is_video {
            // Videos: no-store to prevent double-buffering with GStreamer
            warp::reply::with_header(file, "cache-control", "no-store")
        } else {
            // Images: allow WebKitGTK to cache decoded bitmaps for 5 minutes
            warp::reply::with_header(file, "cache-control", "max-age=300, immutable")
        }
    });
```

#### Frontend (Track B): Use display variant in feed, original in gallery

```typescript
// In FeedEntryCard or a new helper:
// For feed cards, resolve blob: URLs through the display-size command
const mediaSources = entry.media_urls.map(src => {
    if (src.startsWith('blob:') && kind !== 'video') {
        return src.replace('blob:', 'blob-display:'); // Signal to useBlobUrl
    }
    return src;
});

// In useBlobUrl:
// 'blob-display:' prefix → invoke('get_media_blob_display', { hashStr, maxDimension: 1200 })
// 'blob:' prefix → invoke('get_media_blob', { hashStr }) — full resolution
```

#### Backend (Track A): Generate display variant during import (optimization)

Add a third output to `store_media_with_thumbnail`:

```rust
pub async fn store_media_with_variants(
    &self, 
    data: Bytes, 
    thumb_size: u32, 
    display_size: u32,
) -> Result<(Hash, Option<Hash>, Option<Hash>), String> {
    let original_hash = self.store_media(data.clone()).await?;
    let thumb_hash = Self::generate_webp_thumbnail(&data, thumb_size)?;
    let display_hash = Self::generate_display_variant(&data, display_size)?;
    // ... store all three
    Ok((original_hash, thumb_hash, display_hash))
}
```

This pre-generates the display variant at import time, avoiding the on-demand resize cost. The `BytescapeItem` schema would need a `display_blobs: Vec<String>` field (or reuse `thumbnail_blobs` for the larger size and add `thumb_blobs` for the 256px version).

---

## Implementation Priority

| Priority | Fix | Expected Memory Reduction | Effort |
|----------|-----|---------------------------|--------|
| **P0** | Content-aware cache headers (images: `max-age=300`, videos: `no-store`) | 30-50% — eliminates image re-decode on scroll | 30 min |
| **P0** | On-demand display variant generation (`get_media_blob_display`) | 80-95% per image — 48 MB → 2-5 MB decoded | 1-2 days |
| **P1** | Pre-generate display variants at import time | Eliminates on-demand resize latency | 2-3 days |
| **P1** | Frontend: route feed images through display variant, gallery through original | Completes the display-tier pipeline | 3-4 hours |
| **P1** | Reduce video pipeline cap for vault-only browsing (6 → 2) | 2-3 GB → 0.5-1 GB for video-heavy vaults | 1-2 hours |
| **P2** | Blob disk cache LRU eviction | Disk space only (not RAM) | 1 day |

### Expected Result After P0 Fixes

| Scenario | Before | After |
|----------|--------|-------|
| 20 visible phone photos (12MP) | 1.95 GB decoded | **~80 MB decoded** (1200px variants) |
| + 6 video pipelines | +1.8 GB | +0.6 GB (capped at 2-3) |
| + scroll re-decode overhead | +500 MB transient | **0** (images cached by WebKitGTK) |
| **Total RSS for vault browsing** | **~4.5 GB** | **~1 GB** |

---

## Files Changed

| File | Track | Change |
|------|-------|--------|
| `src-tauri/src/media_server.rs` | A | Content-aware cache headers |
| `src-tauri/src/commands/media.rs` | A | New `get_media_blob_display` command |
| `src-tauri/src/sync.rs` | A | `generate_display_variant()` helper |
| `src-tauri/src/lib.rs` | A | Register `get_media_blob_display` command |
| `src-tauri/src/connectors/facebook.rs` | A | Use `store_media_with_variants` at import |
| `src-tauri/src/connectors/instagram.rs` | A | Use `store_media_with_variants` at import |
| `src-tauri/src/models/mod.rs` | Shared | Add `display_blobs: Vec<String>` to `BytescapeItem` |
| `src/hooks/useBlobUrl.ts` | B | Handle `blob-display:` prefix |
| `src/components/FeedEntryCard.tsx` | B | Route feed images through display variant |
| `src/components/feed/GalleryOverlay.tsx` | B | Route gallery images through full original |
| `src/utils/prefetchBlobUrls.ts` | B | Prefetch display variants, not originals |
