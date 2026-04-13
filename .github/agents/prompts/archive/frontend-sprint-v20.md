# Frontend Sprint v20 — Profile Final Polish & Pre-Web Readiness

**Sprint**: v20 (Profile Final Polish)  
**Track**: B (Frontend — React/TypeScript)  
**Date**: 2026-04-11  
**Previous sprint**: v19 delivered the bulk of the profile platform; this sprint closes the last meaningful UX gaps before hosted publishing work begins  
**HEAD at prompt time**: `8010b45`  
**Supersedes**: the earlier hosted-identity UI prompt that jumped ahead of profile acceptance

---

## Sprint Context

The profile is no longer in a "build from scratch" state. Current code already includes:

- modular home/profile modules
- custom left-nav pages
- preview mode
- playlist and listening modules with embed support
- blog modules and custom page CRUD
- connection avatars and clickable links
- avatar editing and profile export primitives

Recent profile delivery history includes commits:

- `1748048` — profile feedback batch 1
- `a985d69` — media blob thumbnails
- `30e5bb9` — custom nav pages + media/page CRUD
- `ac8885b` — theme presets + per-module style editor + background presets + preview mode
- `b76246c` — playlists + blogs module work
- `86fdfba` — connections avatars + clickable items + embeds + blog editor
- `1830382` — Mastodon posts import + Mastodon connections support
- `8010b45` — boot-time Mastodon import wiring

This sprint is the **final local polish pass** before hosted web publishing. Do not reopen already-solved architectural work unless a remaining feedback item proves it is still broken.

### Source-of-truth feedback still open

From `user-feedback`:

#### Critical

1. Media thumbnails render, but clicking them does not open a full-size interactive image/viewer.
2. Mastodon posts are still not showing on the user's profile post history page.
3. Popup windows / toggle boxes / customization controls still suffer from text overlap and alignment problems.

#### Lower priority but still relevant for sign-off

4. Connections terminology is unclear: following vs followers vs contacts.
5. Connections avatars still need consistency where data is available.
6. Listening To / Playlists paste behavior seems flaky.
7. Preview mode does not yet feel useful enough.
8. Logged-in owner flow needs direct per-module edit affordances from normal viewing mode.
9. Blogs need at least a minimal path toward linking WordPress / Ghost / external blog systems.

---

## Sprint Goal

Make the profile feel finished enough that the next major phase can be **hosted publishing**, not more local profile cleanup.

That means:

- no dead-end media interactions
- no obvious broken or confusing customization controls
- no major gaps between imported profile data and the profile post history
- owner editing flow feels fast and intentional

Hosted identity / `bytescape.social` publish UI is **not** the active frontend goal in this sprint.

---

## P0: Media Viewer + Post History Correctness

### 1. Make the Media section interactive

Current state: the Media nav renders thumbnails/placeholders only.

Required outcome:

- clicking a media thumbnail opens a full-size viewer / lightbox
- images should be viewable at usable size
- video items should be playable from the media section as well
- navigation between media items is a plus if it is cheap to reuse existing patterns

### Guidance

- Prefer reusing an existing viewer/lightbox/modal pattern from elsewhere in the app instead of inventing a new one.
- Do not leave the profile media grid as a dead-end visual gallery.

### 2. Verify Mastodon post history on the profile

Current state: Mastodon import work has landed, and boot sync now calls `import_mastodon_posts`, but the user still reports missing Mastodon posts on profile history.

Required outcome:

- confirm the Posts area of the profile actually displays imported Mastodon posts
- if the frontend is filtering or labeling the source incorrectly, fix it
- if the bug is backend-owner or import related, coordinate with Track A immediately rather than papering over it in UI

### Files likely involved

| File | Work |
|------|------|
| `src/screens/ProfileScreen.tsx` | Media viewer state, click handlers, profile post history validation/fixes |
| `src/screens/ProfileScreen.css` | Media viewer / lightbox styling, media grid polish |

---

## P1: Editing UX and Preview Utility

The user is explicitly asking for faster, more module-local editing.

### Required behavior

1. In normal logged-in profile view, each module should expose a compact edit affordance.
2. Clicking that affordance should jump directly into editing that specific module.
3. Preview mode should hide owner-only editing chrome and feel like an external visitor view.
4. Any author label in preview should reflect the viewed profile identity, not a generic self-label like `You`.

### Important product intent

The point is not to add more UI everywhere. The point is to make editing **faster** when the owner already knows which module they want to change.

### Files likely involved

| File | Work |
|------|------|
| `src/screens/ProfileScreen.tsx` | Per-module quick edit flow, preview behavior cleanup |
| `src/screens/ProfileScreen.css` | Visible but restrained quick-edit control styling |

---

## P2: Customization Panel Polish

Current feedback says the customization surfaces are much better, but still have overlapping text and alignment issues.

### Required outcome

Audit and clean up layout issues in:

- customize dropdown
- floating customize popup panels
- per-module style editor rows
- playlist/listening/blog editors
- avatar popup
- toggle/button clusters where text overlaps controls

This is visual polish, but it matters because these are the core profile-creation controls.

### Constraints

- keep the system-theme inheritance model
- do not reopen the larger theme architecture
- avoid re-skinning the whole profile while fixing spacing/alignment

### Files likely involved

| File | Work |
|------|------|
| `src/screens/ProfileScreen.css` | Alignment, spacing, wrapping, responsive control layout |
| `src/screens/ProfileScreen.tsx` | Minor markup adjustments only where CSS alone is insufficient |

---

## P3: Connection and Content Clarification

These are lower priority than P0-P2, but they are part of making the profile understandable enough for launch.

### Required items

1. Clarify what `contacts` means relative to `following` and `followers`.
2. Improve labels or help text so the user understands what is being counted.
3. Ensure connection avatars render wherever the data is already available.
4. Verify Listening To / Playlists input behavior, especially paste.
5. Add the smallest reasonable external-blog linking path if it is cheap:
   - acceptable first pass: blog module entries can include an outbound URL for WordPress / Ghost / external blog home
   - do not build a full external blog ingestion system in this sprint

### Files likely involved

| File | Work |
|------|------|
| `src/screens/ProfileScreen.tsx` | Labels, help text, editor input reliability, optional blog link support |
| `src/screens/ProfileScreen.css` | Supporting UI polish |

---

## Out of Scope

- hosted identity Settings UI
- `publish_identity` / `check_publish_status` / `set_publish_secret`
- `bytescape.social` web publish panel
- full external blog embed/import architecture
- new profile theme system redesign

Those come next, after this sprint is accepted.

---

## Verification

Run all of these before handing back work:

```bash
cd bytescape && npx tsc --noEmit
cd bytescape && pnpm test -- --run
cd bytescape/src-tauri && cargo check
```

In addition to automated verification, manually exercise:

1. media thumbnail click-through in the profile Media section
2. Mastodon posts visible in profile Posts history
3. per-module quick edit from normal logged-in view
4. customize popup and style controls at desktop and narrow width

---

## Track Boundaries

- Track B owns `src/` UI and CSS.
- Coordinate with Track A immediately if Mastodon profile history turns out to be an import/ownership issue rather than a display issue.
- Do not move back into hosted-identity frontend work until this polish gate is accepted.
