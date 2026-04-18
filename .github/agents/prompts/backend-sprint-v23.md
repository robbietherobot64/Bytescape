# Backend Sprint v23 — Thumbnail Pipeline & Media Maintenance Stability

**Sprint**: v23 (Media Stability)  
**Track**: A (Backend — Rust)  
**Date**: 2026-04-18  
**Previous sprint**: v22 fixed media visibility and added blog-feed HTML support, but the latest user feedback reports renewed memory and playback regressions  
**HEAD at prompt time**: `3000da5`  

---

## Sprint Context

The current critical user feedback is now:

1. Memory leak is back and can freeze the full system.
2. Image playback / viewing is stuttery and janky again.

The media visibility bug from earlier in v22 appears to be resolved, but the current frontend implementation is still forced into a bad fallback path because lightweight previews are not guaranteed for every image.

### Audit findings

1. Frontend still invokes `vault_cleanup_repost_media` on media-tab open, even though this should not be part of the gallery query path.
2. Some image flows still lack a usable thumbnail, so the frontend falls back to `vault_get_file_data`.
3. Once the UI starts pulling original image bytes into thumbnail grids, memory pressure and playback jank become expected outcomes.

This sprint should make the backend support a stable rule:

> Thumbnail surfaces get lightweight preview data. Full image/file reads are reserved for explicit view/download actions.

---

## P0: Guarantee Lightweight Thumbnails For Image Surfaces (CRITICAL)

### Problem

Frontend grids are currently forced to fetch full file bytes when `thumbnail_hash` is missing. That means the backend is not providing a reliable lightweight preview path for all image files.

### Required outcome

Make the thumbnail API reliable enough that the frontend never needs full-image fallback in gallery/grid contexts.

### Acceptable implementation directions

Choose the simplest path that fits the current storage model:

1. `vault_get_thumbnail` generates and persists a thumbnail on demand when one does not already exist.
2. Import/upload paths eagerly create thumbnail metadata for all supported image files.
3. A small preview-generation helper is added and reused by both import-time and on-demand paths.

### Requirements

- Hard-cap preview size / dimensions.
- Cache or persist generated thumbnails so repeat requests stay cheap.
- Keep IPC payloads small enough for scrolling grids.
- Do not regress support for archive-imported or linked-account media.

### Files likely involved

| File | Work |
|------|------|
| `src-tauri/src/commands/vault.rs` | Make thumbnail command reliable for images without existing thumbnail metadata |
| `src-tauri/src/commands/import.rs` | Ensure imported images get thumbnail support consistently |
| `src-tauri/src/sync.rs` or related media helpers | Persist / register generated thumbnail artifacts if needed |

---

## P1: Move Repost/Reply Cleanup Out Of The UI Hot Path

### Problem

`vault_cleanup_repost_media` still exists as a heavy maintenance operation, and the frontend has been calling it during media-tab rendering. That is the wrong ownership boundary and the wrong timing.

### Required outcome

Cleanup, if still needed, becomes a backend-owned maintenance step that runs outside the gallery navigation path.

### Acceptable timing

- after import completes
- during app startup in a non-blocking task
- behind an explicit maintenance/admin trigger

### Requirements

- The media listing path must not depend on cleanup having just run.
- Cleanup must be safe and idempotent.
- Avoid destructive work unless the item is definitely a repost/reply artifact that should not appear as standalone gallery media.

### Files likely involved

| File | Work |
|------|------|
| `src-tauri/src/commands/vault.rs` | Cleanup timing / safety review |
| `src-tauri/src/lib.rs` or import completion path | Schedule maintenance away from UI interaction |
| `src-tauri/src/commands/import.rs` | Hook post-import maintenance only if needed |

---

## P2: Audit Large Image Read Paths

### Problem

If the UI is experiencing stutter even after thumbnail fixes, the next likely issue is that full-size image reads are too heavy or too frequent.

### Required backend review

Review the commands used by the UI for full image viewing and confirm:

1. full-file reads are only required for explicit actions like lightbox / modal view
2. there is no accidental thumbnail-grid code path that encourages large IPC transfers
3. any exported/decoded image path is bounded and does not create avoidable duplicate buffers

Add lightweight diagnostic logging where helpful so performance issues can be validated with real archive data.

### Likely files

| File | Work |
|------|------|
| `src-tauri/src/commands/vault.rs` | Review full-data commands and logging |
| `src-tauri/src/media_server.rs` or related helpers | Verify image preview/full-view behavior if relevant |

---

## P3: Blog Backend Work Is Not The Priority

The lower-priority blog feedback is about **frontend default behavior** now, not backend data shape. `body_html` already landed in v22.

Do not spend this sprint expanding blog backend features unless frontend uncovers a concrete missing field that blocks default-inline behavior.

---

## Guardrails

### Do

- Treat memory stability as the highest priority.
- Prefer bounded preview generation over shipping original files through IPC.
- Keep support for archive-imported and linked-account media intact.
- Coordinate with frontend on any API change needed to eliminate full-image thumbnail fallback.

### Do not

- Do not modify React frontend files.
- Do not reopen hosted-identity work.
- Do not add unrelated feature work while the app can still freeze from media browsing.

---

## Verification

Run all of these before handing back work:

```bash
cd bytescape/src-tauri && cargo check
cd bytescape && npx tsc --noEmit
cd bytescape && pnpm test -- --run
```

Behavioral verification:

1. Archive-imported and linked-account images still appear in media galleries.
2. Image thumbnail requests remain lightweight even when original media is large.
3. Repeated gallery opens do not require destructive cleanup to show the right media.
4. Full image reads are reserved for explicit full-view actions.

---

## Track Boundaries

- Track A owns `src-tauri/src/` and backend media behavior.
- Keep any shared contract changes minimal and coordinated.
- Frontend owns the blog default-inline toggle behavior in this sprint.