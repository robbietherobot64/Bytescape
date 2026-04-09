# Frontend Sprint v18a — Profile Polish (Landing Page & Layout)

**Sprint**: v18a (Profile Polish — Part 1)  
**Track**: B (Frontend — React/TypeScript)  
**Date**: 2026-04-08  
**Previous sprint**: v17 (cross-platform repost, feed density — delivered)  
**HEAD at prompt time**: `3f35751`  
**Context**: Profile is the least polished part of the app. Must reach quality parity before go-live identity publishing.

---

## Sprint Context

The profile will be the first thing people see when visiting `bytescape.social/@{username}`. It needs to look and feel as polished as the rest of the app. This sprint addresses **landing page layout, identity display, button logic, and the wall/posts structure**.

### Key files

| File | Lines | Purpose |
|------|-------|--------|
| `src/screens/ProfileScreen.tsx` | 324 | Main profile view |
| `src/screens/ProfileScreen.css` | 229 | Profile styles |
| `src/components/ProfileBioSections.tsx` | 769 | Wall sections (About, Links, etc.) |
| `src/components/ProfileBioSections.css` | 422 | Wall section styles |
| `src/types/index.ts` | ~700 | Shared types |

---

## P0: Fix Username Display

### Problem
The profile hero shows `auth?.handle` which is the full Bluesky handle (e.g., `rabie.bsky.social`). It should show the universal Bytescape username — the part extracted from the `web_did`.

### Current code (ProfileScreen.tsx L179-L184)
```tsx
<h1 className="profile-display-name-large">
    {auth?.handle || 'Anonymous'}
</h1>
<div className="profile-did">{auth?.web_did || auth?.did}</div>
```

### Required change
Display the **Bytescape username** (from `auth.local_username` or extracted from `auth.web_did`). The `web_did` has the format `did:web:bytescape.social:{username}` — extract the last segment. Show the full handle as a secondary subtitle only.

```tsx
// Extract username from web_did: "did:web:bytescape.social:alice" → "alice"
const bytescapeUsername = auth?.local_username 
    || auth?.web_did?.split(':').pop() 
    || null;

<h1 className="profile-display-name-large">
    {bytescapeUsername || auth?.handle || 'Anonymous'}
</h1>
{auth?.handle && bytescapeUsername && (
    <div className="profile-handle-subtitle">@{auth.handle}</div>
)}
<div className="profile-did">{auth?.web_did || auth?.did}</div>
```

### Files to modify
- `src/screens/ProfileScreen.tsx` — username display logic in hero section

---

## P1: Auth-Guard the Customize Button

### Problem
The "Customize" button is always visible, even for non-logged-in users or visitors viewing someone else's profile.

### Current code (ProfileScreen.tsx L186-L207)
The `<button className="profile-edit-btn">` with "Customize" text is rendered unconditionally.

### Required change
Only render the customize/edit button when the profile belongs to the current user. Since this is currently the local-user-only profile, guard it with `auth` existence:

```tsx
{auth && (
    <button className="profile-edit-btn" ...>
        <BsPencilFill size={16} />
        <span>Customize</span>
    </button>
)}
```

### Future note
When we add public profile viewing for other users, this guard will need to compare `auth.did` against the profile owner's DID.

### Files to modify
- `src/screens/ProfileScreen.tsx` — wrap button in auth conditional

---

## P2: Merge Customize and Edit Bio Buttons

### Problem
User feedback: "The current customize button doesn't work, so let's replace it with the edit bio button, and keep all the functionality of that as a basic form of profile customization."

Currently:
- **"Customize" button** (ProfileScreen.tsx L186-L207) → navigates to ProfileBuilderScreen (GrapesJS drag-and-drop editor) — this is the advanced builder
- **"Edit Bio" button** (ProfileBioSections.tsx, inside the wall) → toggles inline section editing (add/remove/reorder sections, edit content)

### Required change
1. **Rename the hero button** from "Customize" to "Edit Profile"
2. Instead of navigating to the GrapesJS builder, this button should **toggle edit mode on the ProfileBioSections** directly.
3. **Keep** the GrapesJS builder accessible but move it to a secondary link (e.g., "Advanced Builder" link inside edit mode, or as an option in Settings).
4. The ProfileBioSections "Edit Bio" toolbar button becomes redundant — remove it, since the hero button now controls edit mode.

### Implementation approach
- Add a state variable `editMode` to ProfileScreen
- Pass it down as a prop to ProfileBioSections: `<ProfileBioSections editMode={editMode} ...>`
- ProfileBioSections currently manages its own `isEditing` state — convert it to use the prop
- The "Edit Bio" button inside ProfileBioSections toolbar should be removed (or hidden when `editMode` is controlled externally)
- Add a small "Advanced Builder →" link somewhere in edit mode for power users

### Files to modify
- `src/screens/ProfileScreen.tsx` — change button behavior, add editMode state
- `src/components/ProfileBioSections.tsx` — accept editMode prop, remove standalone Edit Bio button

---

## P3: Redesign Wall/Posts Layout

### Problem
User feedback: "Make the posts the default selection... Balance the posts width with the width of the other widgets of the profile, so they can all be displayed at the same time."

Currently:
- Default tab is `'wall'` (should be `'posts'`)
- Wall and Posts are mutually exclusive tabs at 960px max-width
- No side-by-side layout — posts take full width with no widgets visible

### Required changes

#### 3a. Default to Posts tab
```tsx
const [profileTab, setProfileTab] = useState<ProfileTab>('posts');
```

#### 3b. Move Wall/Posts toggle
User feedback: "Move Wall/Posts toggle directly on top of the guestbook, and rename the window to 'wall'."

Instead of having Wall/Posts as the **primary** toggle at the top, make a 2-column layout:
- **Left/Main column** (~65%): Posts feed (always visible)
- **Right/Sidebar column** (~35%): Wall sections (About, Now Playing, Links, Guestbook, Connections)

The Guestbook gets a **Wall/Posts toggle** placed directly above it that switches between the guestbook view and a mini-post-wall (where visitors can write).

#### 3c. 2-Column Layout
Use the existing CSS class `profile-content-grid` (already defined in ProfileScreen.css L84-L90 as `grid-template-columns: 240px 1fr`) but update proportions:

```css
.profile-content-grid {
    display: grid;
    grid-template-columns: 1fr 340px;
    gap: 16px;
    max-width: 1200px;
    margin: 16px auto;
    padding: 0 16px;
}

/* Responsive: stack on narrow viewports */
@media (max-width: 900px) {
    .profile-content-grid {
        grid-template-columns: 1fr;
    }
}
```

#### 3d. Layout structure
```
┌─────────────────────────────────────────────────┐
│ Hero Banner (username, DID, avatar, edit button) │
├─────────────────────────┬───────────────────────┤
│ Posts Feed               │ About Me              │
│ (source filters)         │ Now Playing           │
│ (paginated cards)        │ Links                 │
│                          │ Guestbook             │
│                          │ Connections            │
│                          │                       │
│ [Load More]              │                       │
└─────────────────────────┴───────────────────────┘
```

### Files to modify
- `src/screens/ProfileScreen.tsx` — restructure to 2-column, default to posts
- `src/screens/ProfileScreen.css` — update grid layout
- `src/components/ProfileBioSections.tsx` — render as sidebar column (may need layout adjustments for narrower width)
- `src/components/ProfileBioSections.css` — adjust for sidebar context (single-column within sidebar instead of 3-column grid)

---

## P4: Post Deletion on Profile

### Problem
User feedback: "Allow for deleting posts on your profile, including local posts."

Currently, `FeedEntryCard` is rendered without any delete callback on the profile.

### Required changes

1. **Add delete handler** to profile posts:
```tsx
<FeedEntryCard
    entry={adaptBytescapeItem(item)}
    resolveMediaPath={resolveMediaPath}
    mediaDir={mediaDir}
    onDelete={() => handleDeletePost(item.id)}
/>
```

2. **Implement `handleDeletePost`**: Check if `FeedEntryCard` already accepts an `onDelete` prop. If not, add it. The handler should:
   - Call `invoke('delete_vault_post', { id: item.id })` (or whatever the existing vault delete command is)
   - On success, remove the item from local state
   - Show a confirmation dialog first ("Delete this post? This cannot be undone.")

3. **Backend**: Check if `delete_vault_post` command exists in `src-tauri/src/`. If not, this is a dependency on the Backend sprint (Track A).

### Files to modify
- `src/screens/ProfileScreen.tsx` — add delete handler and pass to FeedEntryCard
- `src/components/FeedEntryCard.tsx` — ensure onDelete prop is supported and renders a delete button
- Potentially `src-tauri/src/commands/` — if delete command doesn't exist (coordinate with Track A)

---

## P5: Remove Default Marquee Ticker

### Problem
User feedback: "Lose the tacky scrolling ticker at the bottom, but keep it as an option for customization."

### Current code (ProfileScreen.tsx L313-L317)
```tsx
<div className="profile-footer">
    <div className="profile-marquee">
        ⭐ Thanks for visiting my page! ...
    </div>
</div>
```

### Required changes
1. **Remove** the hardcoded marquee from the default profile layout
2. **Keep** the marquee as a configurable option in ProfileBioSections:
   - Add a new section type `'marquee'` to `ProfileSection.type`
   - Users can add it via "Add Section" → "⭐ Marquee Banner"
   - Customizable: message text, font, color, speed
   - Default visibility: `false` (not shown unless user enables it)
3. The marquee in the GrapesJS builder (`marquee-banner` block) already exists — this is about the default profile only

### Files to modify
- `src/screens/ProfileScreen.tsx` — remove footer marquee section
- `src/components/ProfileBioSections.tsx` — add 'marquee' section type with customization
- `src/components/ProfileBioSections.css` — marquee section styles

---

## Verification

Before committing:

```bash
cd bytescape
npx tsc --noEmit        # 0 errors
pnpm test -- --run      # all tests pass
```

### Manual verification
1. Profile shows Bytescape username (not Bluesky handle) in hero
2. "Customize" button hidden when not logged in
3. "Edit Profile" button toggles inline section editing
4. Posts visible by default with sidebar widgets alongside
5. Posts can be deleted with confirmation
6. Marquee ticker removed from default view

---

## Track Boundaries
- **Track B owns**: `src/` (all React/TypeScript, CSS)
- **Do NOT modify**: `src-tauri/src/` (Track A)
- **Shared file**: `src/types/index.ts` — check `git log --oneline -5 -- src/types/index.ts` before editing
