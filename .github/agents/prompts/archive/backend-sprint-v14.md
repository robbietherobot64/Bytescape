# Backend Sprint v14 — EMERGENCY: Memory Leak Eradication (Track A)

**Sprint Goal:** **FIX THE MEMORY LEAK.** Nothing else matters this sprint. The user has reported 32+ GB memory usage when filtering the feed by video for THREE consecutive sprints. Commit `6d04f1e` claimed to fix it but **only fixed the cache-HIT path** (reads 12 header bytes instead of the full file for extension detection). The cache-MISS path — which is the actual leak — still reads entire video blobs (50–500 MB each) into memory via `sync.get_media(hash)`. This sprint is 100% dedicated to eradicating every memory leak source. **NO feature work. NO security fixes. NO connector fixes.**

**Context:** PM investigation identified **10 distinct memory leak sources** across backend and frontend. This prompt covers the 6 backend sources. The frontend prompt covers the remaining 4. Combined, these explain the observed 32+ GB memory consumption.

**HEAD:** `d242546` on `main`. **Build:** 0 tsc errors, 272 frontend tests, cargo check clean.

---

## The Root Cause (Read This First)

The primary leak is in `get_media_blob()` (commands/media.rs line ~202):

```rust
// Line ~246 — THE LEAK: reads ENTIRE blob into heap memory
let bytes = sync.get_media(hash).await   // ← calls reader.read_to_end()
    .map_err(|e| BytescapeError::BlobStorage(e))?;
```

`sync.get_media()` (sync.rs line ~327) calls `reader.read_to_end(&mut buf)` which materializes the ENTIRE blob in memory. For a 200 MB video, that is a 200 MB heap allocation. When the user filters by Video, `resolve_blob_urls_batch` triggers `get_media_blob` for every uncached video — 10-20 concurrent fetches × 50-500 MB = **0.5-10 GB instantaneous heap allocation**.

**The fix already exists but is not used:** `sync.export_media_to_file()` (sync.rs line ~340) streams blobs directly to disk with **zero memory buffering** via `self.blobs.export(hash, target)`. The `get_media_header()` method (sync.rs line ~349) reads only the first N bytes for MIME detection (32 bytes instead of 500 MB).

---

## P0.1: Fix `get_media_blob` Cache-Miss Path (🔥🔥🔥 THE PRIMARY LEAK)

**File:** `src-tauri/src/commands/media.rs`, function `get_media_blob` (line ~202)

**Current broken code (lines ~240-260):**
```rust
// Not cached — fetch from Iroh blob store
let sync_guard = state.sync_manager.lock().await;
let sync = sync_guard.as_ref()
    .ok_or_else(|| BytescapeError::SyncNotInitialized)?;
let hash = parse_hash_safe(&normalized_hash)?;

let bytes = sync.get_media(hash).await          // ← READS ENTIRE BLOB INTO MEMORY
    .map_err(|e| BytescapeError::BlobStorage(e))?;

let ext = detect_extension_from_bytes(&bytes);   // ← Only needs 12 bytes, has entire blob

std::fs::create_dir_all(&cache_dir)
    .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
let cache_path = cache_dir.join(format!("{}.{}", &normalized_hash, ext));
std::fs::write(&cache_path, &bytes)              // ← Writes from memory to disk (wasteful)
    .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
```

**Required fix — replace with streaming export:**
1. Use `sync.get_media_header(hash, 12)` to read only the first 12 bytes for extension detection
2. Use `sync.export_media_to_file(hash, &cache_path)` to stream directly to disk with zero memory buffering
3. The function signature stays the same (returns `Result<String, String>` — the cache file path)

**Pseudocode for the fix:**
```rust
// Not cached — stream from Iroh blob store directly to disk
let sync_guard = state.sync_manager.lock().await;
let sync = sync_guard.as_ref()
    .ok_or_else(|| BytescapeError::SyncNotInitialized)?;
let hash = parse_hash_safe(&normalized_hash)?;

// Step 1: Read ONLY header bytes for extension detection (12 bytes, not 500 MB)
let header = sync.get_media_header(hash, 12).await
    .map_err(|e| BytescapeError::BlobStorage(e))?;
let ext = detect_extension_from_bytes(&header);

// Step 2: Stream blob directly to cache file (zero memory buffering)
std::fs::create_dir_all(&cache_dir)
    .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
let cache_path = cache_dir.join(format!("{}.{}", &normalized_hash, ext));
sync.export_media_to_file(hash, &cache_path).await
    .map_err(|e| BytescapeError::BlobStorage(e))?;
```

**Memory impact:** Reduces per-blob memory from 50-500 MB to ~12 bytes. This single fix eliminates the primary leak.

---

## P0.2: Fix `get_media_blob_bytes` Cache-Miss Path

**File:** `src-tauri/src/commands/media.rs`, function `get_media_blob_bytes` (line ~270)

**Same pattern as P0.1** — the cache-miss path (line ~300) calls `sync.get_media(hash).await` and loads the entire blob into memory. Then it returns the bytes via IPC, doubling memory (one copy in Rust, one serialized for IPC).

**Required fix:**
1. Stream to cache file first using `export_media_to_file` (same as P0.1)
2. Then read from the cache file to return bytes via IPC
3. Add a `tracing::warn!` if the file is >50 MB being sent over IPC

**Minimum fix:** Replace `sync.get_media(hash)` with export-to-file + `std::fs::read(&cache_path)`. This still loads the file into memory for IPC return, but at least the blob is not held in memory simultaneously with the Iroh read buffer.

---

## P0.3: Fix `resolve_blob_urls_batch` Concurrent Fetches

**File:** `src-tauri/src/commands/media.rs`, function `resolve_blob_urls_batch` (line ~1490+)

**Current broken pattern:** When uncached blobs are found, the batch resolver falls through to `get_media_blob()` in two places:
- Line ~1516: `match get_media_blob(trimmed.to_string(), state.clone()).await`
- Line ~1578: `match get_media_blob(trimmed.to_string(), state.clone()).await`

Even after P0.1 fixes the streaming, the batch function has no concurrency limit — it can trigger 20+ simultaneous blob exports, saturating disk I/O.

**Required fix:**
1. Add a semaphore to limit concurrent blob exports to **3** maximum
2. Use `tokio::sync::Semaphore` with 3 permits — store in `AppState` or use `std::sync::LazyLock`
3. Acquire a permit before each `get_media_blob` call in the batch, release after

**Implementation note:** Check if `once_cell` is already in Cargo.toml. If using Rust 1.80+, prefer `std::sync::LazyLock`. Otherwise add the semaphore to `AppState`.

---

## P0.4: Fix `backfill_media_mimes` Full Blob Reads

**File:** `src-tauri/src/lib.rs`, lines ~882-895 (inside the backfill spawn block)

**Current broken code:**
```rust
// Path hint inconclusive — fetch blob for magic bytes
match sync.get_media(hash).await {     // ← READS ENTIRE BLOB for MIME detection
    Ok(data) => {
        mimes.push(crate::commands::media::mime_from_magic_bytes(&data));
    }
    Err(_) => mimes.push("application/octet-stream".to_string()),
}
```

When the path hint is inconclusive (no file extension), the backfill reads the ENTIRE blob just to check magic bytes. For a 500 MB video, this reads 500 MB when only 32 bytes are needed.

**Required fix:** Replace `sync.get_media(hash)` with `sync.get_media_header(hash, 32)`:
```rust
match sync.get_media_header(hash, 32).await {
    Ok(header) => {
        mimes.push(crate::commands::media::mime_from_magic_bytes(&header));
    }
    Err(_) => mimes.push("application/octet-stream".to_string()),
}
```

**Memory impact:** Reduces from potentially gigabytes to a few hundred bytes total.

---

## P0.5: Add FFmpeg Process Timeout

**File:** `src-tauri/src/commands/media.rs`, all FFmpeg strategy functions

**Problem:** FFmpeg conversion processes use `.wait_with_output()` which blocks indefinitely. A hung FFmpeg process holds the source video in memory and the output buffer grows without bound.

**Investigation:** Search for all `.wait_with_output()` calls in media.rs. Each one needs a timeout wrapper.

**Required fix:**
1. Replace all `child.wait_with_output()` calls with a timeout wrapper:
   ```rust
   let output = tokio::time::timeout(
       std::time::Duration::from_secs(60),
       tokio::task::spawn_blocking(move || child.wait_with_output())
   ).await
   .map_err(|_| "FFmpeg timed out after 60s".to_string())?
   .map_err(|e| e.to_string())?
   .map_err(|e| e.to_string())?;
   ```
2. On timeout, kill the child process explicitly
3. Clean up any partial output files on timeout

**Also fix `cancel_video_resolve`** (line ~683): Currently stores PIDs and uses shell `kill` command (Linux-only). Store `Child` handles instead (or `Arc<Mutex<Option<Child>>>`) and call `.kill()` directly — this is cross-platform and more reliable.

---

## P0.6: Add `blob_cache/` Eviction Policy

**File:** `src-tauri/src/commands/media.rs` (new function)

**Problem:** The `blob_cache/` directory grows without bound. The docstring at line ~199 acknowledges this as tech debt. Over time, cached video files accumulate gigabytes of disk space.

**Required fix — add LRU eviction:**
1. Create a function `enforce_blob_cache_limit(cache_dir: &Path, max_bytes: u64)`
2. List all files in `blob_cache/`, sort by last access time (or modified time)
3. If total size exceeds `max_bytes` (default: 500 MB), delete oldest files until under limit
4. Call this function after each successful `get_media_blob` cache write (debounced — at most once per 60 seconds using an `Instant` check)
5. Also call on app startup (in the delayed background task area)

---

## P0.7: Add Memory Guard Logging

**File:** `src-tauri/src/commands/media.rs`

Add `tracing::warn!` or `println!` logging for any operation that would allocate large memory:
1. In `get_media_blob` and `get_media_blob_bytes`: if the blob size (from header or export) exceeds 50 MB, log a warning
2. In `resolve_blob_urls_batch`: log the total number of uncached blobs that need Iroh export
3. In `backfill_media_mimes`: log when falling through to blob fetch (should be rare after P0.4 fix)

This provides observability for future memory issues.

---

## Also Check: Other `sync.get_media()` Call Sites

Search the entire `src-tauri/src/` for all remaining `sync.get_media(` calls. Each one is a potential full-blob memory load. Known call sites beyond what is already covered above:

| File | Line | Context | Risk |
|------|------|---------|------|
| `commands/media.rs` ~462 | Video preview | Medium — single blob |
| `commands/media.rs` ~560 | HLS fallback | Medium — single blob |
| `commands/vault.rs` ~249 | Vault export | Low — user-initiated |
| `commands/vault.rs` ~274 | Vault batch | Medium — multiple blobs |
| `commands/interactions.rs` ~327 | Attachment fetch | Low — typically small |
| `commands/posting.rs` ~313 | Post attachment | Low — user-initiated |
| `commands/import.rs` ~1219 | Import validation | Low — runs once |
| `sync.rs` ~505 | Consistency check | High — iterates all blobs |
| `sync.rs` ~535, ~713 | Sync operations | Medium |

For each HIGH/MEDIUM risk call site: if the blob could be a video (>10 MB), replace with streaming export or header-only read as appropriate. Low-risk sites (user-initiated, single small blob) can be left for now.

---

## Verification Checklist

After implementing all fixes:

1. **Memory test:** Launch app → filter feed by Video → monitor RSS memory for 60 seconds. Must stay under 2 GB.
2. **Streaming verification:** Add a temporary `println!` in `get_media_blob` cache-miss path to confirm `export_media_to_file` is called (not `get_media`). Remove after verification.
3. **Batch concurrency:** Filter by Video with 20+ uncached videos → verify no more than 3 concurrent blob exports
4. **FFmpeg timeout:** Test with a large video file (>100 MB) to verify FFmpeg completes or times out within 60s
5. **Cache eviction:** Import several large videos, verify `blob_cache/` does not exceed 500 MB
6. **Build:** `cargo check` — 0 errors, `cargo test` — all pass

---

## Constraints

- **Track boundary:** Only modify files in `src-tauri/src/`. Do NOT touch `src/` (frontend files).
- **Build verification:** Run `cargo check` + `cargo test` before EVERY commit. Zero errors.
- **Commit format:** `fix(track-a): description`
- **No TODOs in code.** Implement fully.
- **MEMORY LEAK ONLY.** Do not fix any other bugs, add any features, or address any other tech debt. Every line of code you write this sprint must reduce memory usage.
- **Update `progress.md`** with deliverables when done.

---

## What This Sprint Does NOT Include

Everything else. No AP security. No moderation commands. No connector fixes. No encoding fixes. No feature work of any kind. The ONLY goal is to make the app use <2 GB of RAM under all conditions.
