# Backend Sprint v21 — Mastodon Media Parity & Profile Provenance Support

**Sprint**: v21 (Profile Sign-off Support)  
**Track**: A (Backend — Rust)  
**Date**: 2026-04-13  
**Previous sprint**: v20 closed the major profile-history visibility gaps; this sprint fixes the remaining backend-side issues blocking profile sign-off  
**HEAD at prompt time**: `afa784b`  
**Supersedes**: `backend-sprint-v20.md`

---

## Sprint Context

The previous backend sprint materially improved the profile surface and should be treated as completed work.

Recent confirmed backend wins now in the codebase:

- Mastodon own-post import command exists and is wired into boot sync
- owner-scoped vault queries now match all owner identities, fixing Mastodon post visibility
- replies are filtered out of profile/vault history
- profile export now includes more of the current profile model, including layout/guestbook coverage

The remaining backend work is now narrower and more concrete.

### Source-of-truth feedback still open

#### Critical priorities

1. Mastodon posts now appear on the profile Posts page, but Mastodon images on those posts do not render.
2. The Media lightbox now exists, but the user wants provenance/context information for the selected media item.

#### Lower-priority but still relevant

3. Imported Facebook/Instagram posts still surface as `You` in preview-facing contexts.
4. The frontend wants to relabel `Nickname` as `Display name` while preserving profile compatibility.
5. Some remaining frontend punch-list items may expose small backend gaps once work starts (for example GIF uploads or richer blog-link persistence), but these should be handled only if the gap is real and minimal.

---

## Sprint Goal

Finish the backend-side data and metadata gaps that still keep the profile from being locally “done,” so the next PM phase can move to hosted publishing with a clean conscience.

Do not spend this sprint on hosted identity, publish servers, or resolver endpoints.

---

## P0: Mastodon Media Must Render In Profile Post History

### Problem statement

Current state:

- Mastodon own posts now appear through the profile-history path
- attached Mastodon images still do not render on the profile post page

This strongly suggests the remaining issue is now **media parity**, not post visibility.

### Required outcome

Trace the full path and fix the root cause:

1. Mastodon own-post import fetches and stores media consistently.
2. Imported Mastodon items retain the blob/path/MIME metadata needed by the frontend adapter.
3. The same profile Posts path used by the frontend surfaces Mastodon images, not text-only cards.
4. The fix is verified against the actual profile-history query path, not just against import counts.

### Strong fault lines to inspect first

- `import_mastodon_posts`
- shared save/import helpers used by Mastodon items
- how media blobs, thumbnail blobs, media paths, and MIME hints are stamped for Mastodon content
- any source-specific behavior that diverges from Bluesky/Facebook/Instagram media handling

### Files likely involved

| File | Work |
|------|------|
| `src-tauri/src/commands/import.rs` | Mastodon media import/save path parity |
| `src-tauri/src/commands/vault.rs` | Any profile-history query behavior needed for verification |
| `src-tauri/src/db/vault.rs` | Only if item/media metadata persistence needs adjustment |

---

## P1: Provide Media Provenance/Context Support For The Profile Lightbox

The new frontend ask is not just “show the media,” but “show where it came from.”

### Required outcome

Provide or expose the smallest reusable backend support needed so the frontend can show context for a selected media item.

Preferred metadata to make available when possible:

- source platform
- original timestamp
- owning/source post or item ID
- original/source URL if one exists
- short text/title/snippet associated with the source item

### Guidance

- prefer reusing existing vault data and query paths over inventing a second media index
- if extending an existing media/vault command is enough, do that
- if a new narrowly-scoped command is necessary, keep it surgical and profile-focused

### Files likely involved

| File | Work |
|------|------|
| `src-tauri/src/commands/vault.rs` | Media-to-item context lookup or extension of existing media commands |
| `src-tauri/src/db/vault.rs` | Supporting query helpers if needed |
| `src-tauri/src/types` / command structs | Only if a response model needs to be exposed cleanly |

---

## P2: Author Identity Parity For Preview-Facing Renders

The frontend can patch visible labels, but first confirm whether imported owner-authored items are missing author identity metadata that would make preview renders sane by default.

### Required outcome

- audit imported owner-authored Facebook/Instagram items that surface with blank/weak author labeling
- if the data gap is backend-side, stamp enough owner identity information to support external/preview rendering without breaking the local owner flow
- keep compatibility with existing profile settings while the UI shifts from `Nickname` to `Display name`

### Constraint

Do not create source-specific hacks that only work for one render surface if a more consistent owner-metadata fix is available.

---

## P3: Only Minimal Backend Support For Any Remaining Frontend Blockers

If the frontend punch-list work reveals a real backend blocker, implement the smallest support required and no more.

Examples of acceptable scope:

- tiny persisted shape adjustment for new profile metadata
- narrow helper for GIF/file-backed module assets if a real upload path is needed
- small profile export update if new persisted fields are introduced this sprint

Examples out of scope:

- hosted publish server
- public profile hosting
- resolver/DID publication endpoints
- full external blog ingestion architecture
- large new asset-library systems

---

## Verification

Run all of these before handing back work:

```bash
cd bytescape/src-tauri && cargo check
cd bytescape && npx tsc --noEmit
cd bytescape && pnpm test -- --run
```

Also do behavior-level validation that:

1. imported Mastodon posts in the same profile-history path now render attached images
2. the frontend has enough data to show media provenance/context without hardcoded guesses
3. imported owner-authored posts no longer depend on a generic `You` fallback in preview-facing paths if the root cause was backend metadata

---

## Track Boundaries

- Track A owns `src-tauri/src/`.
- Coordinate with Track B on the exact UI data shape needed for the media info panel.
- Do not jump ahead to hosted publish or web-server work until this sign-off sprint is accepted.
