# Frontend Sprint v19 — Profile Customization: Modular Widgets & Minimalist Default

**Sprint**: v19 (Profile Polish)  
**Track**: B (Frontend — React/TypeScript)  
**Date**: 2026-04-09  
**Previous sprint**: v18 reverted — profile UI restored to pre-v18 state per user feedback (commit `8f8070b`)  
**HEAD at prompt time**: `8f8070b`  
**Design philosophy**: Minimalist default layout. Modular widget system — easy to add, remove, rearrange. Users opt into complexity; they don't start with it.

---

## Sprint Context

The v18 profile overhaul was reverted because it butchered the profile — too much at once, layout felt wrong, customization was tacked on rather than integrated. This sprint rebuilds profile customization from the ground up with a different approach:

1. **Start minimal**: Clean default layout that looks good with zero configuration
2. **Modular widgets**: Each profile section is a self-contained widget the user can add/remove/reorder
3. **Opt-in complexity**: Themes, backgrounds, marquee, GIFs, etc. are available but never forced
4. **Component architecture**: Each widget type is independently renderable — easy to build, test, and extend

### Current state (post-revert)

| Component | File | State |
|-----------|------|-------|
| ProfileScreen | `screens/ProfileScreen.tsx` | Pre-v18: hero + Wall/Posts tabs + Bio sections + hardcoded marquee footer |
| ProfileScreen CSS | `screens/ProfileScreen.css` | Pre-v18: 228 lines, GeoCities theme |
| ProfileBioSections | `components/ProfileBioSections.tsx` | Pre-v18: 768 lines, 6 section types (about, connections, now-playing, guestbook, links, custom), self-contained edit mode |
| FeedEntryCard | `components/FeedEntryCard.tsx` | Pre-v18: no `onDelete` prop |
| ProfilePageNav | `components/ProfilePageNav.tsx` | ✅ Kept from v18 — tabbed page navigation |
| ProfilePageEditor | `components/ProfilePageEditor.tsx` | ✅ Kept from v18 — rich-text + blog editor |
| useProfilePages | `hooks/useProfilePages.ts` | ✅ Kept from v18 — page CRUD |
| ProfileSettings type | `types/index.ts` | ✅ Kept — theme, background, marquee fields defined |
| Backend commands | `commands/profile.rs` | ✅ Kept — settings persistence, pages CRUD, vault deletion |

### User direction

> "We want the profile to be highly customizable and easy to add or remove different components. However, it should start with a default layout that is minimalist and makes sense."

### Profile feedback items to address

From `profile-feedback` (prioritized):

| # | Item | Priority |
|---|------|----------|
| 1 | Username should be Bytescape handle (end of did:web), not Bluesky handle | P0 |
| 2 | Hide customize/edit buttons for non-logged-in users | P0 |
| 3 | Posts as default content (not hidden behind a tab), better space utilization | P0 |
| 4 | Replace broken "Customize" button with working edit mode | P0 |
| 5 | Post deletion on profile | P0 |
| 6 | Marquee as opt-in only (off by default), with customization options | P1 |
| 7 | Now Playing with Bandcamp/SoundCloud/Spotify/YouTube embeds | P1 |
| 8 | Links auto-populate from linked accounts | P1 |
| 9 | Profile inherits system theme (no separate picker) | P0 |
| 10 | Expanded connections (search, filter, expand all) | P2 |
| 11 | GIF collection section type | P2 |
| 12 | Profile pages / website-blog directory | P2 |

---

## Architecture: Widget System

### Core concept

The profile is a **widget container**. Each visual section is a **widget** with a standard interface:

```typescript
type WidgetType = 'posts' | 'about' | 'now-playing' | 'links' | 'connections' | 'guestbook' | 'gif-collection' | 'marquee' | 'custom';

interface ProfileWidget {
    id: string;                    // Unique instance ID
    type: WidgetType;              // Widget type from registry
    title: string;                 // User-editable display title
    column: 'main' | 'sidebar';   // Which column it lives in
    order: number;                 // Sort order within its column
    visible: boolean;              // Can be hidden without deleting
    config: Record<string, any>;   // Widget-specific settings
}
```

Add `WidgetType` and `ProfileWidget` to `src/types/index.ts`.

### Widget registry (in ProfileScreen)

```typescript
const WIDGET_REGISTRY: Record<WidgetType, { label: string; emoji: string; defaultColumn: 'main' | 'sidebar'; singleton: boolean }> = {
    'posts':          { label: 'Posts',           emoji: '📝', defaultColumn: 'main',    singleton: true },
    'about':          { label: 'About Me',        emoji: '👋', defaultColumn: 'sidebar', singleton: true },
    'links':          { label: 'Links',           emoji: '🔗', defaultColumn: 'sidebar', singleton: true },
    'connections':    { label: 'Connections',      emoji: '👥', defaultColumn: 'sidebar', singleton: true },
    'now-playing':    { label: 'Now Playing',      emoji: '🎵', defaultColumn: 'sidebar', singleton: true },
    'guestbook':      { label: 'Guestbook',        emoji: '💬', defaultColumn: 'main',    singleton: true },
    'gif-collection': { label: 'GIF Collection',    emoji: '🖼️', defaultColumn: 'main',    singleton: false },
    'marquee':        { label: 'Marquee Ticker',    emoji: '📰', defaultColumn: 'main',    singleton: true },
    'custom':         { label: 'Custom Section',    emoji: '✨', defaultColumn: 'main',    singleton: false },
};
```

`singleton: true` = only one instance allowed. `singleton: false` = user can add multiples.

### Default widget set (new users / no saved layout)

```typescript
const DEFAULT_WIDGETS: ProfileWidget[] = [
    { id: 'posts',       type: 'posts',       title: 'Posts',       column: 'main',    order: 0, visible: true, config: {} },
    { id: 'about',       type: 'about',       title: 'About Me',   column: 'sidebar', order: 0, visible: true, config: {} },
    { id: 'links',       type: 'links',       title: 'Links',      column: 'sidebar', order: 1, visible: true, config: {} },
    { id: 'connections', type: 'connections',  title: 'Connections', column: 'sidebar', order: 2, visible: true, config: {} },
];
```

Minimal, sensible, clean. No marquee, no guestbook, no now-playing by default.

### Default visual layout

```
┌──────────────────────────────────────────────────────────┐
│  [Avatar]  username                                       │
│            did:web:bytescape.social:username      [Edit]  │
└──────────────────────────────────────────────────────────┘
┌────────────────────────────────┐ ┌──────────────────────┐
│  📝 Posts                      │ │  👋 About Me         │
│  [All] [🦋 Bsky] [📦 Archive] │ │  "Tell the world..." │
│                                │ ├──────────────────────┤
│  [post cards...]               │ │  🔗 Links            │
│                                │ │  🦋 @handle.bsky...  │
│                                │ │  🔗 mysite.com       │
│  [Load More]                   │ ├──────────────────────┤
│                                │ │  👥 Connections       │
└────────────────────────────────┘ │  42 following         │
                                   └──────────────────────┘
```

---

## P0: New Default Layout + Core Fixes

This is the structural rewrite. One commit, all core issues resolved.

### Step 1: ProfileScreen rewrite

Replace the Wall/Posts tab system with a 2-column widget layout.

**Key changes from current pre-v18 code:**
- Remove `ProfileTab` type and `profileTab` state (no more wall/posts tabs)
- Add `editMode` state (boolean) with working toggle button
- Add `widgets` state loaded from `get_profile_settings` (or default set)
- Add `deleteConfirm` state for post deletion confirmation
- Remove hardcoded marquee footer
- Show posts directly in main column (no tab required)
- Render sidebar widgets in right column using existing `ProfileBioSections` sub-renderers

**Username fix:**
```typescript
const displayName = auth?.local_username || auth?.handle || 'Anonymous';
```
Display `local_username` (the Bytescape handle). Fall back to `handle` only if not set.

**Auth-guarded edit mode:**
```typescript
{auth && (
    <button className="profile-edit-btn" onClick={() => setEditMode(!editMode)}>
        {editMode ? <><BsCheckLg /> Done</> : <><BsPencilFill /> Edit</>}
    </button>
)}
```
No button visible when not logged in. No broken "Customize" button.

### Step 2: FeedEntryCard `onDelete`

Add optional `onDelete?: (id: string) => void` prop. When present, show a small trash icon in the card header. ProfileScreen handles the confirmation dialog (inline bar above the card, same pattern as v18).

### Step 3: CSS rewrite

Rewrite `ProfileScreen.css` for the 2-column grid layout:

```css
.profile-content-grid {
    display: grid;
    grid-template-columns: 1fr 300px;
    gap: 16px;
    padding: 16px;
    max-width: 1100px;
    margin: 0 auto;
    width: 100%;
}

@media (max-width: 768px) {
    .profile-content-grid { grid-template-columns: 1fr; }
}
```

Keep existing hero styles (they look fine). Remove the marquee styles from default render. Add edit-mode control styles.

### Files to modify

| File | Changes |
|------|---------|
| `src/screens/ProfileScreen.tsx` | Rewrite: 2-column widget container, username fix, edit mode, post deletion |
| `src/screens/ProfileScreen.css` | Rewrite: 2-column grid, widget card, edit controls |
| `src/components/FeedEntryCard.tsx` | Add `onDelete` prop with trash icon |
| `src/types/index.ts` | Add `ProfileWidget`, `WidgetType` types |

### Migration from BioSections to Widgets

The current `ProfileBioSections` uses its own `ProfileSection` model (`get_widget_data`/`save_widget_data`). The new widget system uses `ProfileWidget[]` stored inside `ProfileSettings` via `get_profile_settings`/`save_profile_settings`.

**Strategy**: Keep `ProfileBioSections` as the content renderer for sidebar widgets. The new `ProfileScreen` manages layout; it renders each widget using the appropriate sub-component. For About, Links, Connections, Now Playing, Guestbook — delegate to extracted renderers from `ProfileBioSections`. For Posts — render directly in ProfileScreen (it already does this).

This avoids rewriting all the section content logic in one go. Over time, individual widget renderers can be extracted into their own component files.

---

## P1: Widget Customization Panel

### Theme: Inherit the system theme

**The profile does NOT have its own theme picker.** It inherits whatever system theme the user has active (`useTheme().activeTheme`). The app already has 4 themes (`geocities`, `win98`, `midnight`, `hacker`) with full CSS variable sets and custom override support via `useTheme`. The profile should use these CSS variables for all its colors, backgrounds, and accents — no profile-specific theming layer.

The hero gradient, card backgrounds, and accent colors all come from the active theme's CSS variables (`--bg-primary`, `--accent-purple`, etc.). If the user switches their system theme, the profile updates automatically.

### Edit mode widget controls

When `editMode === true`:

1. Each widget card shows a small toolbar: ↑↓ (reorder within column), ✕ (remove widget)
2. Bottom of each column shows an **"+ Add Widget"** button
3. Clicking "Add Widget" shows a dropdown of available types (singletons filtered if already present)

### Marquee widget

Not on by default. User adds it via "Add Widget" → "Marquee Ticker". Widget config:
- `text: string` — scrolling message
- `color: string` — text color
- `speed: 'slow' | 'medium' | 'fast'`

CSS animation renders it. Speed map: slow=20s, medium=12s, fast=6s.

### Files to modify

| File | Changes |
|------|---------|
| `src/screens/ProfileScreen.tsx` | Widget controls, add widget dropdown, marquee widget renderer |
| `src/screens/ProfileScreen.css` | Edit controls, widget toolbar, marquee animation |

---

## P2: Rich Widget Content

### Now Playing embeds

Auto-detect URL type and render inline embed:

- **Bandcamp**: `{artist}.bandcamp.com/(track|album)/{slug}` → Bandcamp embedded player
- **SoundCloud**: `soundcloud.com/{user}/{track}` → SoundCloud widget
- **Spotify**: `open.spotify.com/(track|album|playlist)/{id}` → Spotify embed
- **YouTube**: `youtube.com/watch?v={id}` or `youtu.be/{id}` → YouTube embed

The embed detection logic from v18's `detectEmbed()` was solid — reuse that pattern.

### Links auto-populate

When user has no custom links and has linked accounts (`list_linked_accounts`), auto-populate links widget with entries. Show "Auto-detected from linked accounts:" label.

### GIF Collection widget

Stores array of GIF URLs in `config.gifs`. Displays in a responsive grid. Editor: URL inputs with add/remove (same pattern as links editor).

### Files to modify

| File | Changes |
|------|---------|
| `src/components/ProfileBioSections.tsx` | Now Playing embed detection, links auto-populate, GIF collection renderer |
| `src/components/ProfileBioSections.css` | Embed iframe sizing, GIF grid, auto-label |

---

## P3: Profile Pages Integration

Wire the existing ProfilePageNav + ProfilePageEditor into the profile.

- Page tab bar below hero (only visible when pages exist or in edit mode)
- Clicking a page hides the widget grid and shows page content
- "Home" tab returns to widgets
- Edit mode: "Add Page" button with type selector (rich-text, blog, gallery)
- Gallery page type: simple image URL grid (basic implementation)

### Files to modify

| File | Changes |
|------|---------|
| `src/screens/ProfileScreen.tsx` | Wire ProfilePageNav + ProfilePageEditor, page routing state |
| `src/components/ProfilePageEditor.tsx` | Add gallery page type renderer |

---

## P4: Connections Expansion

1. "Show all" button → scrollable full list (max-height with overflow)
2. Search input filtering by handle text
3. Source filter tabs: All / Bluesky / P2P / Mastodon
4. Source icon per connection badge

### Files to modify

| File | Changes |
|------|---------|
| `src/components/ProfileBioSections.tsx` | Connections search, filter, expand |
| `src/components/ProfileBioSections.css` | Expanded list, search bar styles |

---

## P5: Avatar Image

1. Edit mode → clicking avatar shows URL input popup
2. Store as `avatar_url` in ProfileSettings
3. Render circular image with accent border; fallback to letter avatar on error
4. Add `avatar_url?: string` to ProfileSettings type

### Files to modify

| File | Changes |
|------|---------|
| `src/types/index.ts` | Add `avatar_url` to ProfileSettings |
| `src/screens/ProfileScreen.tsx` | Avatar image rendering + edit overlay |
| `src/screens/ProfileScreen.css` | Avatar image, edit overlay styles |

---

## Implementation Guidance

### What NOT to do (lessons from v18)

- **Don't combine everything in one massive commit.** Use the recommended commit sequence below.
- **Don't force features on by default.** Every customization feature is opt-in via the widget system.
- **Don't add a separate profile theme system.** The profile inherits the active system theme from `useTheme()`. CSS variables (`--bg-primary`, `--accent-purple`, etc.) drive all profile colors.
- **Don't inline CSS extensively.** Use classes. The pre-v18 code had too many inline style objects.
- **Don't remove features without providing the replacement in the same commit.**
- **Don't over-engineer.** The widget system is a flat array, not a nested tree.

### Recommended commit sequence

1. **P0**: New default layout + username fix + edit mode + post deletion (the big structural change)
2. **P1**: Widget customization panel + marquee opt-in (profile inherits system theme — no separate theme picker)
3. **P2**: Rich widget content (embeds, auto-links, GIFs)
4. **P3**: Profile pages integration
5. **P4+P5**: Connections expansion + avatar (can combine)

Each commit must pass: `npx tsc --noEmit` (0 errors) + `pnpm test -- --run` (all pass).

---

## Verification

Before each commit:

```bash
cd bytescape
npx tsc --noEmit                     # 0 errors
pnpm test -- --run                   # all tests pass
```

Document all changes in `progress.md`.

---

## Track Boundaries

- **Track B owns**: `src/` (all React/TypeScript, CSS)
- **Shared files** (require PM coordination): `src/types/index.ts`
- Check `git log --oneline -5 -- <filepath>` before editing any shared file
