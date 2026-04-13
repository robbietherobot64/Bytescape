# Frontend Sprint v21 — Profile Sign-off Punch List Before Hosted Publish

**Sprint**: v21 (Profile Sign-off Punch List)  
**Track**: B (Frontend — React/TypeScript)  
**Date**: 2026-04-13  
**Previous sprint**: v20 landed the major final-polish wins; this sprint closes the remaining profile issues blocking sign-off  
**HEAD at prompt time**: `afa784b`  
**Supersedes**: `frontend-sprint-v20.md`

---

## Sprint Context

The last sprint materially improved the profile and should be treated as completed work, not reopened baseline work.

Recent confirmed wins now in the codebase:

- media thumbnails plus full-size lightbox/gallery browsing
- Mastodon posts appearing on the profile Posts page
- per-module quick edit from normal owner view
- clearer connections UI with avatars, hover cards, and follow/follower distinction
- export parity improvements and general CSS cleanup

The profile is now in **punch-list** territory. The next frontend sprint should focus only on the issues still called out in `user-feedback`.

### Source-of-truth feedback still open

#### Critical priorities

1. The Media viewer needs an information panel showing provenance and context:
   - original post link if available
   - upload/post date
   - platform/source
   - relevant text or contextual metadata
2. Customization panels still have misaligned text and overlapping controls.
3. The two-column module layout still behaves like a shared grid instead of two independent columns.
4. Users need left/right controls to move modules across columns, not only vertical movement.

#### Lower-priority but still remaining

5. Listening To / Playlists paste behavior became unreliable again.
6. GIF Collection is still effectively a dead-end module.
7. Blog modules need a minimal path for linking external platforms like WordPress or Ghost.
8. Imported Facebook/Instagram posts still show `You` in preview when they should show the viewed profile identity.
9. `Nickname` should be renamed to `Display name` in the UI.

#### Cross-track dependency to watch

10. Mastodon posts now appear, but Mastodon images still do not render on the post page. Track A owns the likely root cause; Track B must verify the render path once backend parity is fixed.

---

## Sprint Goal

Finish the remaining profile UX and rendering issues so the next major PM prompt can credibly move on to hosted publishing work instead of another local-profile cleanup pass.

That means:

- media feels informative, not just viewable
- module layout feels intentional and controllable
- owner/preview identity labeling feels correct
- editing panels stop fighting the user
- the remaining “this does nothing” profile modules become minimally usable

Hosted publish UI remains out of scope until this sign-off pass is accepted.

---

## P0: Media Context Panel + Preview Identity Correctness

### 1. Add a provenance/context panel to the Media lightbox

Current state: the lightbox shows the file and navigation only.

Required outcome:

- add an information area beneath or beside the media viewport
- show the best available metadata for the selected media item:
  - source platform
  - original timestamp/date
  - linked post/source entry when available
  - short contextual text/snippet if available
- if backend support is missing for file-to-post provenance, coordinate with Track A immediately rather than inventing fake client-side metadata

### 2. Fix preview-mode author labeling for imported owner posts

Current state: imported posts can still show `You` in contexts where preview should look external.

Required outcome:

- in preview mode, owner-authored imported posts should display the viewed profile identity
- prefer the profile display name when present, otherwise the Bytescape username
- do not break the normal owner-view flow or interactive owner actions

### 3. Rename `Nickname` to `Display name`

Required outcome:

- update visible UI labels, placeholders, and relevant helper text
- preserve compatibility with current persisted data if the underlying field name remains unchanged for now

### Files likely involved

| File | Work |
|------|------|
| `src/screens/ProfileScreen.tsx` | Media info panel, preview identity handling, display-name labeling |
| `src/screens/ProfileScreen.css` | Lightbox info-panel layout and related styling |
| `src/components/FeedEntryCard.tsx` | Preview-facing author-name behavior if the fallback is still `You` |

---

## P1: Independent Columns + Better Layout Controls

Current state: module layout still behaves like a single shared two-column grid where taller boxes delay the next row for both sides.

Required outcome:

1. Make the left and right columns behave independently.
2. Allow modules to stack within available vertical space instead of waiting on the tallest neighboring box.
3. Add left/right movement controls so modules can be explicitly moved between columns.
4. Preserve clear up/down ordering within a column.

### Product intent

The user is asking for a classic customizable profile-page layout, not a generic masonry experiment. Keep the controls understandable and deterministic.

### Constraints

- do not regress existing edit flows
- do not make module ordering ambiguous
- mobile behavior should still collapse cleanly to one column

### Files likely involved

| File | Work |
|------|------|
| `src/screens/ProfileScreen.tsx` | Module placement model, left/right controls, ordering logic |
| `src/screens/ProfileScreen.css` | Column layout structure, responsive behavior, movement control styling |

---

## P2: Customization Popup Alignment Cleanup

This remains a sign-off issue from the user’s perspective.

Required outcome:

Audit and fix overlap/alignment problems in:

- customize dropdown
- floating customize popup
- radio/checkbox rows
- toggle and button clusters
- per-module style editor rows
- avatar popup and related small editors

### Constraints

- keep the current customization model intact
- do not reopen a large redesign just to solve alignment
- prefer predictable spacing, wrapping, and label alignment over cramming more controls into tight rows

### Files likely involved

| File | Work |
|------|------|
| `src/screens/ProfileScreen.css` | Primary alignment and spacing cleanup |
| `src/screens/ProfileScreen.tsx` | Small markup adjustments only where CSS alone is insufficient |

---

## P3: Reliability and Completeness Pass For Remaining Modules

### 1. Restore Listening To / Playlists paste reliability

Required outcome:

- verify paste works consistently in the relevant editors
- confirm the parser still handles URLs and iframe embed snippets
- remove any interaction bug where the editor stops accepting pasted content even though other popups still do

### 2. Make GIF Collection minimally usable

Current state: the user reports `Add GIF` effectively does nothing.

Required outcome:

- ensure the control actually results in editable/addable entries
- minimum acceptable first pass: linked GIF entry UI that visibly works
- if cheap, support choosing or uploading a local GIF as well
- do not promise a full curated old-web GIF library in this sprint unless it is actually implemented

### 3. Add a minimal external-blog linking path

Required outcome:

- the blog surface should have a reasonable first-pass path for linking an external blog home or article
- WordPress/Ghost support can be simple outbound linking for now
- do not build a full external blog ingestion system in this sprint

### Files likely involved

| File | Work |
|------|------|
| `src/screens/ProfileScreen.tsx` | Playlist/listening editor reliability, GIF module behavior, blog link support |
| `src/screens/ProfileScreen.css` | Supporting module/editor polish |

---

## Verification

Run all of these before handing back work:

```bash
cd bytescape && npx tsc --noEmit
cd bytescape && pnpm test -- --run
cd bytescape/src-tauri && cargo check
```

Also manually verify:

1. media lightbox shows contextual info for the selected media item
2. preview mode no longer labels imported owner posts as `You`
3. left/right module movement works and column packing behaves independently
4. customization controls no longer overlap at desktop and narrow widths
5. playlists/listening paste works again

---

## Track Boundaries

- Track B owns `src/` UI and CSS.
- Coordinate with Track A on the Mastodon-image issue and any backend support needed for media provenance.
- Do not jump ahead to hosted identity or `bytescape.social` publish UI in this sprint.
