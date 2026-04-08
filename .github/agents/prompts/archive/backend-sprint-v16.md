# Backend Sprint v16 — Video Playback Stability & Memory Safety (Track A)

**Sprint Goal:** Eliminate video playback stutter, reduce GStreamer pipeline memory footprint, fix CPU spike on launch, and harden the media server for smooth video delivery across all sources (Bluesky HLS, vault blobs, Mastodon, Instagram).

**HEAD:** `d0edd1c` on `main`. **Build:** 0 tsc errors, 273 frontend tests, cargo check clean.

**Reference:** User feedback (persistent): "Video playback is twitchy and stuttery across all video sources. Memory leak to 32+ GB when filtering by video. CPU spikes to 100% on app launch."

---

## Context: Current Video Architecture

The video pipeline has 5 layers:

| Layer | File | Role |
|-------|------|------|
| Frontend component | `src/components/VideoPlayer.tsx` (426 lines) | Click-to-play gated `<video>` with IntersectionObserver lifecycle |
| Frontend utility | `src/utils/mediaServer.ts` (54 lines) | Caches Warp server port, constructs localhost URLs |
| Backend media commands | `src-tauri/src/commands/media.rs` (1731 lines) | Blob export, HLS→MP4 conversion via FFmpeg, MIME detection |
| Backend file server | `src-tauri/src/media_server.rs` (110 lines) | Warp HTTP server on localhost serving `blob_cache/` directory |
| Backend registration | `src-tauri/src/lib.rs` | Starts Warp server at init, stores port in AppState |

**How it works today:**
- HTML5 `<video>` tag rendered in WebKitGTK, which uses GStreamer internally
- Click-to-play: `src` is only set after user clicks (prevents passive pipeline accumulation)
- Global singleton: only 1 active video at a time, previous pipeline destroyed
- HLS videos (Bluesky): backend downloads `.m3u8` segments → remuxes to MP4 via FFmpeg → serves via localhost Warp
- Vault blobs: backend exports to `blob_cache/` → serves via Warp
- Direct URLs: passthrough to `<video src>`
- Dual IntersectionObserver: tears down pipeline when scrolled 500px out of viewport

**Known problems:**
1. Video is stuttery/twitchy across ALL sources — GStreamer pipeline scheduling in WebKitGTK
2. Memory leak to 32+ GB when filtering by video — pipeline teardown is slow or incomplete
3. CPU spikes to ~100% on app launch — likely startup media scanning or eager resolution
4. Instagram videos never load (but Instagram images work in vault)
5. HLS→MP4 FFmpeg conversion can take several seconds, creating a dead period after click

---

## P0: Fix Video Stutter (GStreamer Pipeline Optimization)

**Priority:** P0 — user's top complaint, every session.

### P0.1: Media Server Response Headers

**File:** `src-tauri/src/media_server.rs`

Current state: Warp server sets `Cache-Control: no-store` for videos. This means every seek/rebuffer causes a new HTTP request and potentially a new GStreamer demuxer initialization.

**Fix:** Change video cache headers to allow in-memory caching:
```rust
// Current (forces re-fetch on every seek):
.header("Cache-Control", "no-store")

// Fix: Allow browser to cache the video blob in memory (already local, no security risk):
.header("Cache-Control", "public, max-age=3600, immutable")
.header("Accept-Ranges", "bytes")  // Enable range requests for seeking
```

Also ensure the Warp server supports **HTTP Range requests** (byte-range serving). Without range request support, WebKitGTK must download the entire file before seeking, which causes stutter on any timeline scrub. Warp's `warp::fs::file()` supports ranges natively — verify this is being used instead of custom body streaming.

### P0.2: FFmpeg Remux Quality

**File:** `src-tauri/src/commands/media.rs` — `resolve_hls_video` function

Current approach uses a 3-tier fallback (stream copy → audio re-encode → full re-encode → raw `.ts`). Issues:
1. **Stream copy** (`-c copy`) can produce MP4 files with missing moov atoms if FFmpeg exits early
2. **Raw `.ts` fallback** may not play in WebKitGTK (TS container support is spotty)
3. The remux doesn't add `-movflags +faststart`, so the moov atom is at the end → WebKitGTK must download entire file before playback starts

**Fix:**
```rust
// Add to ALL FFmpeg MP4 output commands:
.arg("-movflags").arg("+faststart")

// For the stream-copy path, add:
.arg("-fflags").arg("+genpts")  // Generate presentation timestamps for smoother playback
```

### P0.3: Preload Optimization

**File:** `src/components/VideoPlayer.tsx` (frontend change, but backend-relevant)

The `<video>` element currently uses `preload="metadata"`. In WebKitGTK, even `preload="metadata"` can trigger a partial GStreamer pipeline allocation. Consider:
1. Use `preload="none"` combined with a poster image to eliminate any pipeline creation before play
2. OR pre-fetch the first 256KB of the video file during the visibility observer phase and check if the container is valid MP4 (valid moov atom) before setting `src`

### P0.4: CPU Spike on Launch

Investigate what happens at app startup that causes CPU to spike:
1. Check `src-tauri/src/lib.rs` — are any media-related commands called eagerly at startup?
2. Check if `media_server.rs` does any filesystem scanning at startup
3. Check if the frontend's `useUnifiedFeed` eagerly resolves video URLs on initial feed load
4. Profile with `RUST_LOG=debug` and look for media-related log messages in the first 5 seconds

**Fix:** Any eager media work should be deferred to idle time or triggered only on user interaction.

---

## P1: Memory Leak — Video Pipeline Teardown

**Priority:** P1 — causes 32+ GB memory consumption.

### P1.1: Audit `blob_cache/` Cleanup

**File:** `src-tauri/src/commands/media.rs`

Every video resolve writes to `blob_cache/`. If these files are never cleaned up, disk usage grows unbounded and WebKitGTK may memory-map them.

**Fix:** Implement a cache eviction strategy:
```rust
// On app startup and periodically (every 30 minutes):
fn cleanup_blob_cache(cache_dir: &Path, max_size_mb: u64) {
    // 1. List all files in blob_cache/
    // 2. Sort by last-access time
    // 3. Delete oldest files until total size < max_size_mb
    // Suggested max: 500 MB
}
```

### P1.2: FFmpeg Process Leak

**File:** `src-tauri/src/commands/media.rs`

Check that ALL FFmpeg child processes are properly killed when:
- The user scrolls past a converting video (`cancel_video_resolve` command exists but verify it works)
- The app is shutting down
- A new video resolve is requested for the same source

**Verify:** After the sprint, `ps aux | grep ffmpeg` should show ZERO orphan processes during normal usage.

### P1.3: Instagram Video Resolution

**Files:** `src-tauri/src/connectors/instagram.rs` (if exists), `src-tauri/src/commands/media.rs`

Instagram videos don't load at all. Investigate:
1. What format are Instagram video URLs in the archive data?
2. Are they being resolved through the blob pipeline or direct URL?
3. Does the media server handle their MIME type correctly?
4. Are Instagram video entries being created with correct `media_kinds: ["video"]`?

---

## P2: Remaining `sync.get_media()` Cleanup

**Priority:** P2 — carry-over from v15.

These medium-risk `sync.get_media()` call sites still load full blobs into heap memory:

| File | Line (approx) | Context | Fix |
|------|------|---------|-----|
| `commands/syndicate.rs:128` | Post syndication | Stream from cache file instead of heap |
| `commands/vault.rs:249` | Single vault export | Export to cache, then read file |
| `commands/vault.rs:274` | Batch vault export | Add semaphore, export each to cache |
| `commands/interactions.rs:327` | Attachment fetch | Add 10 MB size guard |

Use the established pattern from v14:
```rust
let cache_path = cache_dir.join(format!("{}.{}", hash_str, ext));
if !cache_path.exists() {
    sync.export_media_to_file(hash, &cache_path).await?;
}
let bytes = std::fs::read(&cache_path)?;
```

---

## Verification

After all changes:

1. **Stutter test:** Play 5 different Bluesky videos consecutively. Each should start within 1 second of click and play without visible frame drops.
2. **Memory test:** Open the feed, filter by video, scroll through 20+ video entries. `htop` RSS should stay under 2 GB.
3. **CPU test:** Cold launch the app. CPU should drop below 30% within 10 seconds of the window appearing.
4. **Seek test:** Play a video, seek to middle, seek back. No stutter, no re-download.
5. **Cleanup test:** `ps aux | grep ffmpeg` should show 0 orphan processes. `du -sh blob_cache/` should not grow unbounded.
6. **Build:** `cargo check` — 0 errors. `npx tsc --noEmit` — 0 errors. `pnpm test -- --run` — all pass.

---

## Files Expected to be Modified

| File | Changes |
|------|---------|
| `src-tauri/src/media_server.rs` | Cache headers, range request support |
| `src-tauri/src/commands/media.rs` | FFmpeg flags (`-movflags +faststart`, `-fflags +genpts`), cache cleanup, process leak audit |
| `src-tauri/src/lib.rs` | Defer any eager media work at startup |
| `src-tauri/src/commands/syndicate.rs` | Replace `sync.get_media()` with cache-file pattern |
| `src-tauri/src/commands/vault.rs` | Replace `sync.get_media()` with cache-file pattern |
| `src-tauri/src/commands/interactions.rs` | Add size guard on attachment fetch |
