# Frontend Sprint v19 — Profile Customization Reactivation

**Sprint**: v19 (Profile Customization)  
**Track**: B (Frontend — React/TypeScript)  
**Date**: 2026-04-10  
**HEAD at prompt time**: `1b59005`  
**Status**: Reissued because v19 was skipped while prompts drifted ahead to hosted-identity planning  

---

## Sprint Context

Hosted identity is **not** the active implementation target right now. The user explicitly wants the profile customization sprint revived and executed first.

The current profile surface still reflects the reverted pre-v18 baseline:

| Component | File | Current state |
|-----------|------|---------------|
| Profile screen | `src/screens/ProfileScreen.tsx` | Still uses top-level `wall/posts` tabs, broken `Customize` button, hardcoded marquee, no post deletion |
| Profile screen CSS | `src/screens/ProfileScreen.css` | Older layout, not the intended minimalist 2-column profile |
| Bio sections | `src/components/ProfileBioSections.tsx` | Basic editable sections, but no rich embeds, no linked-account auto-links, no GIF surface, no connections search/filter |
| Feed cards | `src/components/FeedEntryCard.tsx` | No profile delete affordance |
| Profile pages | `src/components/ProfilePageNav.tsx`, `src/components/ProfilePageEditor.tsx`, `src/hooks/useProfilePages.ts` | Present in the repo, but not wired into the current profile experience |

## Non-Negotiable Product Direction

1. The default profile should be **minimalist** and sensible out of the box.
2. The profile should inherit the **active app/system theme** the user already selected.
3. Do **not** add a separate profile theme picker or a parallel profile-only theming layer.
4. Customization should come from **modular profile components** the user can add, remove, hide, or reorder.
5. Optional creative features like marquees, backgrounds, embeds, GIF walls, and extra pages must be opt-in and must not clutter the default profile.

### User feedback to implement

From `profile-feedback` and chat:

- Replace the broken `Customize` button with a working customization/edit flow.
- Show the Bytescape handle / local username, not the Bluesky handle, as the main profile identity.
- Make posts the default/main surface and use space better so posts and profile widgets can be visible at the same time.
- Keep the wall / guestbook capability, but do not make it the dominant default view.
- Allow deleting profile posts, including local posts.
- Remove the default scrolling ticker; keep it only as an optional customization.
- Support richer creativity: embeds, GIF-heavy sections, linked-account auto-links, more navigable connections, and profile pages.

---

## P0: Minimalist System-Themed Profile Baseline

This is the first required implementation step. Do this before any richer customization work.

### Required behavior

1. Replace the current top-level `wall/posts` tab framing with a layout where **posts are visible by default**.
2. Use a balanced **2-column profile layout** so the main post feed and profile widgets are both on screen on desktop, with a clean stacked mobile fallback.
3. Replace the broken `Customize` button with a working **Edit / Customize** toggle.
4. Hide edit/customization controls when the viewer is not the logged-in local user.
5. Show `local_username` as the primary identity label.
6. Remove the hardcoded marquee from the default render.
7. Add delete affordances for profile posts.

### Layout rule

Treat the wall / guestbook as a secondary profile component, not the main page shell. If a wall toggle remains anywhere, it belongs with the wall / guestbook area itself, and **Posts** must be the default selection.

### Files expected in P0

| File | Work |
|------|------|
| `src/screens/ProfileScreen.tsx` | Main layout rewrite, username fix, working customize/edit mode, post deletion wiring |
| `src/screens/ProfileScreen.css` | Minimalist 2-column layout using existing theme variables, responsive stacking |
| `src/components/FeedEntryCard.tsx` | Optional `onDelete` affordance for profile-owned posts |

### Theme rule for P0

Use the app's existing theme system and CSS variables. The profile should visually inherit the current app theme automatically. Do not introduce profile-specific palette state, profile theme presets, or a theme picker modal.

---

## P1: Modular Section Customization

Once the baseline layout is solid, make the profile easy to customize without making it noisy by default.

### Goal

Move toward a modular section model where profile areas can be added, removed, hidden, or reordered.

This can be implemented either by:

1. introducing a flat widget/section registry, or
2. extending the existing `ProfileBioSections` model so it supports clearer layout placement and enable/disable behavior

Choose the simpler path that fits the current codebase. Do not over-engineer this into a nested page builder yet.

### Default section set

The default profile should stay sparse:

- Posts
- About
- Links
- Connections

These should be available but **not default-on**:

- Wall / guestbook
- Now Playing
- GIF collection / image button section
- Marquee / ticker
- Extra custom sections

### Constraints

- No separate profile theme picker.
- Optional backgrounds are acceptable only if they layer on top of the current app theme and remain subtle by default.
- Keep edit controls compact and obvious.

### Likely files

| File | Work |
|------|------|
| `src/screens/ProfileScreen.tsx` | Section layout ownership, add/remove/reorder controls |
| `src/components/ProfileBioSections.tsx` | Section visibility, ordering, richer per-section controls |
| `src/components/ProfileBioSections.css` | Minimalist section chrome and editor controls |
| `src/types/index.ts` | Only if a minimal shared section/widget type is necessary |

---

## P2: Rich Media and Creative Profile Components

After the core layout and modular editing are in place, implement the richer customization features the user asked for.

### Required items

1. **Now Playing embeds**
   - Support Bandcamp, SoundCloud, Spotify, and YouTube.
   - Autoplay must be opt-in only.

2. **Linked-account auto-links**
   - If the user has linked accounts and has not manually customized links yet, auto-populate the links section from linked accounts.

3. **GIF-first custom section**
   - Provide a section type for GIF buttons / GIF collections / image-button style content.

4. **Connections navigation improvements**
   - Search
   - Source filtering
   - Expand all / scrollable full list

### Likely files

| File | Work |
|------|------|
| `src/components/ProfileBioSections.tsx` | Embed detection, linked-account fallback, GIF section, connections search/filter |
| `src/components/ProfileBioSections.css` | Embed frames, GIF layout, expanded connection list |

---

## P3: Website / Blog Pages On Profile

The user wants the profile to function like a customizable small website.

Use the existing profile page files already in the repo and wire them into the active profile surface:

- `src/components/ProfilePageNav.tsx`
- `src/components/ProfilePageEditor.tsx`
- `src/hooks/useProfilePages.ts`

### Required behavior

1. The profile can have additional pages beyond the default home/profile surface.
2. The default home page remains minimalist.
3. Users can create page-style content for blog / website usage.
4. Page navigation should feel like an extension of the profile, not a separate application.

This is where richer Neocities-style behavior starts, but keep the first implementation grounded and maintainable.

---

## Guardrails

### Do

- Implement the direct user feedback first.
- Keep the default profile clean.
- Reuse existing theme variables and page components where practical.
- Split work into small commits that map to P0, P1, P2, and P3.

### Do not

- Do not jump ahead to hosted identity work in this sprint.
- Do not add a profile-specific theme picker.
- Do not make marquee / backgrounds / GIF widgets appear by default.
- Do not rebuild the entire profile architecture if a smaller extension of current components will get the result faster and more safely.

---

## Verification

Run all of these before handing back work:

```bash
cd bytescape && npx tsc --noEmit
cd bytescape && pnpm test -- --run
cd bytescape/src-tauri && cargo check
```

---

## Track Boundaries

- Track B owns `src/` UI work.
- `src/types/index.ts` is shared; keep any change minimal and coordinated.
- Do not move back into hosted-identity prompt scope until this profile sprint is actually implemented.