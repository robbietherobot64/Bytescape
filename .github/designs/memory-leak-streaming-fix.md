# Memory Leak Fix: Streaming Blob Architecture

**Status:** Design Complete — Ready for Implementation  
**Priority:** CRITICAL — App consuming 17–25 GB RAM when video filter is active  
**Track:** Backend (Track A) — `src-tauri/src/`  
**Author:** Systems Agent  
**Date:** Session 6, Sprint v6

---

## 1. Problem Statement

When the user filters their feed by video, the app's memory climbs to **17–25 GB** and never recovers. A frontend-only fix (commit `5befc2f`) addressed GStreamer pipeline churn but did not touch the real culprit: the **Rust backend reads entire video blobs into heap memory**.

### Root Cause Chain

```
VideoPlayer mount
  → invoke('get_media_blob', hash)
    → media.rs: acquire blob_semaphore (4 permits)
    → media.rs: lock sync_manager Mutex
    → sync.rs: get_media(hash)
      → blobs.reader(hash).read_to_end(&mut Vec::new())   ← UNBOUNDED HEAP ALLOC
      → Bytes::from(buf)                                    ← vec consumed, Bytes alive
    → media.rs: detect_extension_from_bytes(&bytes)         ← bytes still alive
    → media.rs: std::fs::write(&cache_path, &bytes)         ← bytes STILL alive
    → media.rs: return Ok(path)                             ← bytes + sync_guard drop HERE
```

A single 500 MB video = 500 MB heap allocation. With 15 videos rendered concurrently (4 active via semaphore, rest queued), this compounds rapidly. Contributing amplifiers:

| Factor | Impact |
|--------|--------|
| `read_to_end()` into unbounded `Vec<u8>` | O(file_size) heap per call |
| `sync_manager` Mutex held across read+write | Serializes all blob access; bytes live until return |
| `bytes.to_vec()` in `get_media_blob_bytes` | **Doubles** memory (Bytes + Vec clone) |
| MIME backfill reads full blob for 12 magic bytes | Gratuitous multi-MB allocs on startup |
| glibc `malloc` fragmentation | OS never reclaims freed pages |

### Memory Math

```
15 videos × avg 200 MB each = 3 GB (concurrent)
× 2 (.to_vec() clone in get_media_blob_bytes) = 6 GB
+ MIME backfill (reads every blob on startup) = 10+ GB
+ malloc fragmentation (glibc keeps RSS high) = 17–25 GB observed
```

---

## 2. Solution Architecture

### Core Principle: **Never buffer a full blob in memory**

Three new SyncManager methods replace the monolithic `get_media()`:

```
┌─────────────────────────────────────────────────────────┐
│                    SyncManager API                       │
├─────────────────────────────────────────────────────────┤
│  CURRENT (LEAKS):                                       │
│  get_media(hash) → Bytes         ← reads entire blob    │
│                                                         │
│  NEW (STREAMING):                                       │
│  export_media_to_file(hash, path) → u64                 │
│    └─ iroh-blobs export() API: zero memory buffering    │
│                                                         │
│  get_media_header(hash, n) → Vec<u8>                    │
│    └─ reads only first n bytes (32 for ext detection)   │
│                                                         │
│  get_media(hash) → Bytes  [RETAINED for small blobs]    │
│    └─ kept for thumbnails, images <10MB, uploads        │
│    └─ add explicit size guard (reject >50MB reads)      │
└─────────────────────────────────────────────────────────┘
```

### Data Flow — After Fix

```
VideoPlayer mount
  → invoke('get_media_blob', hash)
    → media.rs: check disk cache (fast path, unchanged)
    → media.rs: acquire blob_semaphore
    → media.rs: lock sync_manager BRIEFLY (just to get blob store ref)
    → media.rs: drop sync_guard immediately
    → sync.rs: export_media_to_file(hash, temp_path)
      → iroh_blobs::api::Blobs::export(hash, temp_path).await
      → blob written directly to disk by iroh internals (ZERO heap alloc)
    → media.rs: read_header_detect_ext(temp_path)  ← reads 32 bytes only
    → media.rs: rename temp_path → final_cache_path
    → return Ok(final_cache_path)
```

**Memory used: 32 bytes** (header buffer) vs **500 MB** (current).

---

## 3. Detailed Fix Plan

### Fix 1: `sync.rs` — New Streaming Methods

**File:** `src-tauri/src/sync.rs`  
**Location:** After existing `get_media()` (line ~278)

#### 3.1a `export_media_to_file()`

Uses the iroh-blobs `Blobs::export()` API which writes a blob directly to a file path on disk without loading it into memory. Returns the byte count written.

```rust
/// Export a blob directly to a file on disk — ZERO memory buffering.
///
/// Uses iroh-blobs' internal export which streams from the blob store
/// to the target path without materializing the full blob in memory.
pub async fn export_media_to_file(&self, hash: Hash, target: &std::path::Path) -> Result<u64, String> {
    let size = self.blobs.export(hash, target)
        .await
        .map_err(|e| format!("Blob export failed for {}: {}", hash, e))?;
    Ok(size)
}
```

**Why `export()` instead of `tokio::io::copy(reader, file)`:** The export API is purpose-built for this — it handles partial blobs, concurrent reads, and uses the store's internal I/O scheduler. It's also a single async call vs managing reader/writer lifetimes.

#### 3.1b `get_media_header()`

Reads only the first N bytes using the blob reader. The reader implements `AsyncRead + AsyncSeek`, so a bounded `read()` only fetches the requested bytes.

```rust
/// Read only the first `n` bytes of a blob for MIME/extension detection.
///
/// For a 500 MB video, this reads 32 bytes instead of 500 MB.
pub async fn get_media_header(&self, hash: Hash, n: usize) -> Result<Vec<u8>, String> {
    use tokio::io::AsyncReadExt;
    let mut reader = self.blobs.reader(hash);
    let mut buf = vec![0u8; n];
    let bytes_read = reader.read(&mut buf).await.map_err(|e| e.to_string())?;
    buf.truncate(bytes_read);
    Ok(buf)
}
```

#### 3.1c `get_media()` — Add Size Guard

Keep the existing method for legitimate small-blob reads (thumbnails, images for upload), but add a hard ceiling to prevent accidental multi-GB reads:

```rust
/// Retrieve media bytes by hash. CAPPED at 50 MB — use export_media_to_file() for larger blobs.
pub async fn get_media(&self, hash: Hash) -> Result<Bytes, String> {
    use tokio::io::AsyncReadExt;
    let mut reader = self.blobs.reader(hash);
    let mut buf = Vec::new();
    // Safety cap: refuse to buffer more than 50 MB in memory
    let n = reader.read_to_end(&mut buf).await.map_err(|e| e.to_string())?;
    if n > 50 * 1024 * 1024 {
        return Err(format!("Blob too large for in-memory read ({} MB). Use export_media_to_file().", n / (1024 * 1024)));
    }
    Ok(Bytes::from(buf))
}
```

> **Note:** The 50 MB cap on `get_media()` means callers that handle large files (videos) MUST switch to `export_media_to_file()`. Callers that only handle small blobs (thumbnails, images <10 MB) can keep using `get_media()` unchanged.

---

### Fix 2: `media.rs:get_media_blob()` — Stream to Disk

**File:** `src-tauri/src/commands/media.rs`  
**Current code (lines 177–200):** Locks Mutex → reads full blob into `Bytes` → detects extension → writes to disk → returns path. Blob is alive for the entire scope.

**New code:**

```rust
// Not cached — acquire blob semaphore
let _permit = /* ... unchanged ... */;

// Get a CLONE of the blob store handle — release Mutex immediately.
// iroh_blobs::api::Store is a lightweight handle (Arc internally), not the data.
let blob_store = {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;
    sync.blobs().clone()  // Clone the API handle, NOT blob data
};
// sync_guard dropped here — Mutex released BEFORE any I/O

let hash = parse_hash_safe(&normalized_hash)?;

// Stream blob directly to a temp file — ZERO memory buffering
std::fs::create_dir_all(&cache_dir)
    .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
let temp_path = cache_dir.join(format!("{}.tmp", &normalized_hash));
let _size = blob_store.export(hash, &temp_path)
    .await
    .map_err(|e| BytescapeError::BlobStorage(format!("{}", e)))?;

// Detect extension from just the file header (32 bytes, not full file)
let ext = read_header_detect_ext(&temp_path);

// Rename to final cached path (atomic on same filesystem)
let cache_path = cache_dir.join(format!("{}.{}", &normalized_hash, ext));
std::fs::rename(&temp_path, &cache_path)
    .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;

Ok(cache_path.to_string_lossy().to_string())
```

**Key changes:**
1. **Mutex scope:** Lock → clone API handle → drop lock. Total lock time: microseconds (was: seconds for large blobs).
2. **Memory:** Zero — `export()` writes directly from blob store to disk.
3. **Extension detection:** `read_header_detect_ext()` reads only 32 bytes from the temp file.

**Dependency:** Requires `sync.blobs()` to return a clonable handle. The `iroh_blobs::api::Store` is `Clone` (it's an `Arc<...>` internally), so `blobs().clone()` is essentially free.

> **Alternative considered:** Instead of cloning the blob store, we could add `export_media_to_file()` on `SyncManager` and call it while holding the Mutex. But that keeps the Mutex locked during the entire export I/O, which serializes all blob access. The clone approach is strictly better.

---

### Fix 3: `media.rs:get_media_blob_bytes()` — Eliminate `.to_vec()` Clone

**File:** `src-tauri/src/commands/media.rs`  
**Current code (line ~269):** `Ok(tauri::ipc::Response::new(bytes.to_vec()))` — doubles memory.

**New code:** For the Iroh-fetch path, write to cache first (like `get_media_blob`), then read from disk. This means the blob never exists as an in-memory buffer:

```rust
// Not cached — export to disk first, then read from cache
let blob_store = {
    let sync_guard = state.sync_manager.lock().await;
    let sync = sync_guard.as_ref()
        .ok_or_else(|| BytescapeError::SyncNotInitialized)?;
    sync.blobs().clone()
};

let hash = parse_hash_safe(&normalized_hash)?;

std::fs::create_dir_all(&cache_dir)
    .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
let temp_path = cache_dir.join(format!("{}.tmp", &normalized_hash));
let size = blob_store.export(hash, &temp_path)
    .await
    .map_err(|e| BytescapeError::BlobStorage(format!("{}", e)))?;

// Enforce IPC size limit (50 MB) — large files should use media server URL
if size > 50 * 1024 * 1024 {
    let ext = read_header_detect_ext(&temp_path);
    let cache_path = cache_dir.join(format!("{}.{}", &normalized_hash, ext));
    let _ = std::fs::rename(&temp_path, &cache_path);
    return Err(format!(
        "File too large for IPC ({}MB). Use media server URL: http://127.0.0.1:{}/{}.{}",
        size / (1024 * 1024),
        state.media_port,
        &normalized_hash,
        ext
    ));
}

let ext = read_header_detect_ext(&temp_path);
let cache_path = cache_dir.join(format!("{}.{}", &normalized_hash, ext));
let _ = std::fs::rename(&temp_path, &cache_path);

// Read from disk for IPC response — single copy, bounded by 50 MB guard above
let data = std::fs::read(&cache_path)
    .map_err(|e| BytescapeError::FileSystem(e.to_string()))?;
Ok(tauri::ipc::Response::new(data))
```

**Memory:** Worst case 50 MB (the IPC guard), vs previous unbounded + doubled.

---

### Fix 4: MIME Backfill — Header-Only Reads

**File:** `src-tauri/src/commands/media.rs` (lines ~496) and `src-tauri/src/lib.rs` (line ~891)

**Current code:** `sync.get_media(hash).await` reads full blob just to pass first 12 bytes to `mime_from_magic_bytes()`.

**New code:** Use the header method:

```rust
// In backfill_media_mimes and lib.rs startup backfill:
match sync.get_media_header(hash, 32).await {
    Ok(header) => {
        mimes.push(mime_from_magic_bytes(&header));
    }
    Err(_) => {
        let idx = mimes.len();
        let hint = item.media_paths.get(idx).map(|s: &String| s.as_str()).unwrap_or("");
        mimes.push(mime_from_path_hint(hint));
    }
}
```

**Memory:** 32 bytes per blob, vs potentially 500 MB per blob. For a backfill of 100 items × 3 blobs each, this saves ~150 GB of transient allocations.

---

### Fix 5: Remaining `get_media()` Callers — Risk Assessment

These callers use `get_media()` but are **lower risk** because they handle small blobs or are infrequent:

| Caller | File | Risk | Action |
|--------|------|------|--------|
| `vault_get_file_data` | vault.rs:199 | **HIGH** — returns `bytes.to_vec()` of arbitrary vault files | Switch to `export_media_to_file()` → return file path, or add size guard |
| `vault_get_thumbnail` | vault.rs:224 | LOW — thumbnails are ~50 KB (generated as WebP) | Keep `get_media()` — well within 50 MB cap |
| `syndicate_to_bluesky` | syndicate.rs:64 | MEDIUM — images only (videos skipped), but images can be 20+ MB | Keep for now — images are within 50 MB cap; optimize later if needed |
| `create_post` | posting.rs:233 | MEDIUM — same as syndicate | Keep for now |
| `send_reply` | interactions.rs:205 | MEDIUM — attachments for replies | Keep for now |
| `import_item_media_into_vault` | import.rs:1070 | **HIGH** — imports arbitrary media including videos | Switch to `export_media_to_file()` → process from disk |
| `diagnose_media_ref` | media.rs:409 | LOW — diagnostic command, manual invocation only | Keep `get_media()` |

**Phase 1 (this sprint):** Fix the 4 hot-path items (Fixes 1–4) that cause the 17–25 GB leak.  
**Phase 2 (next sprint):** Migrate `vault_get_file_data` and `import_item_media_into_vault` to streaming.

---

## 4. Implementation Sequence

```
Step 1: sync.rs — Add export_media_to_file() and get_media_header()
         Add 50 MB safety cap to existing get_media()
         (No callers break — get_media still exists)

Step 2: media.rs:get_media_blob() — Switch to blob_store.export()
         Drop Mutex before I/O

Step 3: media.rs:get_media_blob_bytes() — Switch to export + disk read
         Eliminate .to_vec() clone

Step 4: media.rs:backfill_media_mimes() — Switch to get_media_header()
         lib.rs startup backfill — Same change

Step 5: cargo check — verify compilation
Step 6: pnpm test -- --run — verify frontend tests pass
Step 7: Commit: fix(track-a): stream blobs to disk instead of buffering in memory
```

---

## 5. Risk Analysis

| Risk | Mitigation |
|------|------------|
| `blobs().clone()` might not give a usable handle outside the Mutex | `iroh_blobs::api::Store` is `Clone` (Arc-based). Verified in source. |
| `export()` might fail for blobs not yet fully downloaded | Same failure mode as current `get_media()` — returns error. No regression. |
| 50 MB cap on `get_media()` breaks large-blob callers | Only `vault_get_file_data` and `import_item_media_into_vault` could hit this. Both should migrate to streaming (Phase 2). For now, most vault files are images <10 MB. |
| `export()` is slower than in-memory `read_to_end()` | Unlikely — both do disk I/O. Export avoids the extra copy through userspace memory. Net effect should be *faster*. |
| Temp files left behind on crash | Use `.tmp` extension. Can be cleaned on startup. Low risk. |
| `read_header_detect_ext` fails on empty temp file | Export returns size; check `size == 0` before header read. |

---

## 6. Memory Impact Estimates

| Scenario | Before | After |
|----------|--------|-------|
| Single 500 MB video fetch | 500 MB heap (1 GB with `.to_vec()`) | 32 bytes (header) |
| 15 concurrent video filter | 7.5 GB min (15 GB with clones) | ~480 bytes |
| MIME backfill, 300 blobs | ~30 GB transient | ~9.6 KB |
| Steady-state feed browsing | 2–4 GB (blob churn + fragmentation) | <100 MB |

---

## 7. Diagram — Before vs After

### Before (Current)
```
┌──────────────────┐     ┌───────────────────────┐     ┌──────────────┐
│   Frontend/IPC   │────▶│   media.rs commands    │────▶│  sync.rs     │
│                  │     │                        │     │  get_media() │
│                  │     │  ┌──────────────────┐  │     │              │
│                  │     │  │ Mutex lock held   │  │     │  ┌────────┐ │
│                  │     │  │ for entire I/O    │  │     │  │Vec<u8> │ │
│                  │     │  ├──────────────────┤  │     │  │(500 MB)│ │
│                  │     │  │ bytes in memory   │  │     │  └────┬───┘ │
│                  │     │  │ detect_ext(bytes) │  │     │       │     │
│                  │     │  │ fs::write(bytes)  │  │     │       │     │
│                  │◀────│  │ bytes.to_vec()    │  │◀────│       │     │
│                  │     │  │ (DOUBLES memory)  │  │     │       │     │
│  17-25 GB RSS    │     │  └──────────────────┘  │     └───────┘─────┘
└──────────────────┘     └───────────────────────┘
```

### After (Streaming)
```
┌──────────────────┐     ┌───────────────────────┐     ┌──────────────┐
│   Frontend/IPC   │────▶│   media.rs commands    │     │  sync.rs     │
│                  │     │                        │     │              │
│                  │     │  ┌──────────────────┐  │     │  export()    │
│                  │     │  │ Mutex: clone +    │  │     │     │       │
│                  │     │  │ drop (μseconds)   │  │     │     │       │
│                  │     │  ├──────────────────┤  │     │     ▼       │
│                  │     │  │ export(hash,tmp)──│──│────▶│  ┌──────┐  │
│                  │     │  │    (ZERO memory)  │  │     │  │ Disk │  │
│                  │     │  │ read_header(32B)  │  │     │  │ I/O  │  │
│                  │     │  │ rename(tmp→final) │  │     │  └──────┘  │
│                  │◀────│  │ return path       │  │     │            │
│  <100 MB RSS     │     │  └──────────────────┘  │     └────────────┘
└──────────────────┘     └───────────────────────┘
         │
         ▼
  ┌──────────────┐
  │ Warp media   │  (streams from disk — already correct)
  │ server       │
  └──────────────┘
```

---

## 8. Validation Criteria

1. **Build passes:** `cargo check` — 0 errors
2. **Tests pass:** `pnpm test -- --run` — all green
3. **Memory test:** Filter feed by video → RSS stays under 500 MB (was 17–25 GB)
4. **Functional:** Videos still load and play after the change
5. **MIME detection:** Extensions still correctly detected (from file headers, not full reads)
6. **Batch resolve:** `resolve_blob_urls_batch` still resolves uncached blobs

---

## 9. Files Modified (Track A Boundary Check)

| File | Track | Authorized? |
|------|-------|-------------|
| `src-tauri/src/sync.rs` | Track A | ✅ |
| `src-tauri/src/commands/media.rs` | Track A | ✅ |
| `src-tauri/src/lib.rs` | Shared (command registration + startup) | ✅ (MIME backfill only) |

No frontend files modified. No shared type files modified. Track boundaries respected.
