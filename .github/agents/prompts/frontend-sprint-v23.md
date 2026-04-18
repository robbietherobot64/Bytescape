# Frontend Sprint v23 — Media Memory Stabilization & Default Inline Blogs

**Sprint**: v23 (Media Stability + Blog Default Behavior)  
**Track**: B (Frontend — React/TypeScript)  
**Date**: 2026-04-18  
**Previous sprint**: v22 fixed media visibility and decomposed major ProfileScreen modules, but the latest user feedback reports new performance regressions  
**HEAD at prompt time**: `3000da5`  

---

## Sprint Context

The latest `user-feedback` changes the priority again:

### Critical priorities

1. Memory leak is back and can freeze the whole system.
2. Image playback / image viewing has become stuttery and janky again.

### Lower priority

3. Blog posts with links should default to a rich inline iframe view. Users may opt out to a plain link, but inline should be the default presentation.

### Audit findings from the current code

The current frontend implementation strongly suggests the new regressions are self-inflicted by thumbnail-loading behavior:

1. `ProfileScreen.tsx` still calls `vault_cleanup_repost_media` on every media-tab load.
2. `ProfileScreen.tsx` fetches thumbnails with `Promise.all(...)` across the whole gallery and falls back to `vault_get_file_data` when no thumbnail exists.
3. The vault picker does the same full-image fallback for up to 60 images.
4. `VaultScreen.tsx` also falls back to `vault_get_file_data` for grid thumbnails.
5. `BlogModule.tsx` still defaults to text view because `blogIframeView` starts empty / false.

This sprint is about **stability first**. No more polish or architecture work until media memory and playback are under control.

---

## P0: Remove Full-Image Fallback From Thumbnail Paths (CRITICAL)

### Problem

Thumbnail views are currently pulling full image bytes into JS when `thumbnail_hash` is missing. That is exactly the wrong tradeoff for a scrolling grid and is the leading suspect for both memory blow-up and janky rendering.

### Required frontend behavior

In all thumbnail/grid contexts, do **not** call `vault_get_file_data` as a fallback.

Affected surfaces:

- `src/screens/ProfileScreen.tsx` media tab
- `src/screens/ProfileScreen.tsx` vault picker
- `src/screens/VaultScreen.tsx` file grid/list thumbnails

### Acceptable fallback behavior

If no lightweight thumbnail is available yet:

1. show a placeholder tile, or
2. show a low-information preview state like `No preview yet`, or
3. use a backend-generated lightweight thumbnail path if Track A provides one during this sprint

What is **not** acceptable:

- fetching the original full image into thumbnail slots
- eagerly loading dozens or hundreds of full images in parallel

### Files to modify

| File | Work |
|------|------|
| `src/screens/ProfileScreen.tsx` | Remove full-file fallback from media gallery thumbnails and vault picker thumbnails |
| `src/screens/VaultScreen.tsx` | Remove full-file fallback from `FileThumbnail` |
| `src/components/profile/MediaGallery.tsx` | Adjust placeholder / empty-preview behavior if needed |

---

## P1: Fix Blob URL Lifetime & Bounded Thumbnail Loading (CRITICAL)

### Problem

Even when blob URLs are revoked in a few local cases, the current flow still creates many URLs in bulk and replaces thumbnail maps without a full cleanup pass. Combined with full-image fallback, this is a credible leak pattern.

### Required fixes

1. Revoke old media thumbnail URLs before replacing `mediaThumbs` with a new batch.
2. Revoke old vault-picker thumbnail URLs before replacing `vaultPickerThumbs`.
3. Add cleanup on view exit / unmount, not just on explicit delete or modal close.
4. Stop using unbounded `Promise.all(...)` over the full media set.
5. Load thumbnails in bounded batches or a capped initial window.

### Target behavior

- Opening the media tab repeatedly should not cause steady blob-memory growth.
- Opening the vault picker repeatedly should not retain old thumbnail URLs.
- Large vaults should remain responsive instead of stalling while every image thumbnail is fetched at once.

### Files to modify

| File | Work |
|------|------|
| `src/screens/ProfileScreen.tsx` | Blob URL cleanup, bounded thumbnail loading |
| `src/screens/VaultScreen.tsx` | Keep cache bounded and avoid heavy fallback behavior |

---

## P2: Remove Media Cleanup From Frontend Navigation Path

### Problem

The v22 frontend prompt explicitly required removing the unconditional `vault_cleanup_repost_media` call from media-tab load. Current code still calls it on every media-tab open.

This is both a consistency failure and a likely performance problem.

### Required fix

Remove the frontend call from the media-tab path entirely.

The frontend should:

- open the media tab
- list files
- fetch lightweight thumbnails

It should **not** trigger backend maintenance or destructive cleanup while rendering a gallery.

If maintenance is still needed, that belongs to Track A and should run after import or in background app lifecycle hooks, not in the render path.

### Files to modify

| File | Work |
|------|------|
| `src/screens/ProfileScreen.tsx` | Remove `vault_cleanup_repost_media` invocation from `loadMedia()` |

---

## P3: Default Blog Links To Inline Iframe View

### Problem

The blog iframe feature landed, but it is opt-in. The new feedback is explicit: linked blog posts should render inline by default, with an opt-out for users who want link-only behavior.

### Required behavior

For any blog entry with `external_url`:

1. Default expanded view to the iframe mode.
2. Keep a visible control to switch to a plain text / link mode.
3. Keep `Open in browser` available.
4. Preserve sandbox restrictions.
5. Do not inject raw HTML directly into the DOM.

### UX direction

The default should feel like a blog post window, not a URL preview.

If an embed fails because the site blocks framing:

- clearly show the fallback state
- offer the external open action
- keep the opt-out toggle coherent

### Files to modify

| File | Work |
|------|------|
| `src/components/profile/BlogModule.tsx` | Default iframe view for linked entries, add opt-out copy/controls |
| `src/screens/ProfileScreen.css` or module CSS | Any small CSS support needed for default-inline presentation |

---

## P4: Add Regression Coverage Where Cheap

At minimum, add or update tests for the behavior that changed in this sprint when practical:

1. Blog entries with `external_url` default to inline mode.
2. Thumbnail components do not use full-file fetches in grid contexts.

If the thumbnail-fetch behavior is awkward to unit test directly, document the gap in the commit notes and rely on manual verification.

---

## Guardrails

### Do

- Prioritize memory and playback stability over visual polish.
- Keep changes surgical and local to the media/blog surfaces.
- Reuse existing theme/system styling; no new theming work.
- Validate repeatedly after each focused fix.

### Do not

- Do not modify Rust files in `src-tauri/src/`.
- Do not continue ProfileScreen decomposition in this sprint.
- Do not add new profile features unrelated to the feedback above.
- Do not fetch full image payloads for thumbnail grids.

---

## Verification

Run all of these before handing back work:

```bash
cd bytescape && npx tsc --noEmit
cd bytescape && pnpm test -- --run
cd bytescape/src-tauri && cargo check
```

Behavioral verification:

1. Open the profile media tab repeatedly on a large archive-backed vault; memory should remain bounded.
2. Open the vault screen on a large image set; scrolling should stay responsive.
3. Large images should only load fully in explicit view actions like lightbox / file modal, not in grid thumbnails.
4. Blog posts with external URLs should open inline by default.

---

## Track Boundaries

- Track B owns `src/` only.
- Track B does not modify `src-tauri/src/` in this sprint.
- If backend support is required for lightweight thumbnails, document the dependency and coordinate through the active backend sprint.