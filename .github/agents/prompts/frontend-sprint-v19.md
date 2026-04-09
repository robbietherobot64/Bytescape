# Frontend Sprint v19 — Profile Customization Deepening

**Sprint**: v19 (Profile Polish: Creative Expression)  
**Track**: B (Frontend — React/TypeScript)  
**Date**: 2026-04-09  
**Previous sprint**: v18a/b/c (profile screen overhaul — 2-col layout, themes, embeds, pages — delivered)  
**HEAD at prompt time**: `b7fca4e`  
**Rationale**: Make the profile visually compelling before the hosted go-live (v20). Users need creative tools to express themselves.

---

## Sprint Context

Sprint v18 delivered the profile's structural overhaul: 2-column layout, 6 preset themes, Now Playing embeds, connections grid, profile pages with blog support, and post deletion. This sprint deepens the customization options — custom backgrounds, marquee ticker, avatar images, GIF sections, custom colors, expanded connections, and richer page editing. The goal is old-web creative expression: make it feel like a personal homepage.

### User feedback driving this sprint

From `profile-feedback`:

> "Overall increase of customization options. What creative ideas can we come up with that allows users to be creative on their profile? Custom backgrounds? Custom themes? Etc."

> "Lose the tacky scrolling ticker at the bottom, but keep it as an option for customization that users can change the message, font, color, etc."

> "What sort of native gif embedding can I do on these sections? Can I have a section full of gif buttons as a custom section? This is the type of creativity we want to allow on the profile, a lot of old web functionality."

> "Connections should be more navigable, searchable, filterable by source, expandable to view all connections"

> "We want the user to utilize their profile as a website/blog. So how can we implement a directory system on the profile that allows users to have a navigable site of pages that they customize?"

### What already exists

| Component | File | Status |
|-----------|------|--------|
| 6 preset themes | `ProfileScreen.tsx` (PROFILE_THEMES) | ✅ Working |
| ProfileSettings type fields | `types/index.ts` (704-713) | ✅ Fields defined for background + marquee |
| Profile settings persistence | `save_profile_settings` / `get_profile_settings` | ✅ JSON blob — any new fields just work |
| Bio sections CRUD | `ProfileBioSections.tsx` | ✅ 5 section types + custom |
| Section types | about, connections, now-playing, guestbook, links, custom | ✅ |
| Profile pages | `ProfilePageNav.tsx`, `ProfilePageEditor.tsx`, `useProfilePages.ts` | ✅ Rich-text + blog |
| Theme picker dropdown | `ProfileScreen.tsx` hero area | ✅ |
| Edit mode toggle | `ProfileScreen.tsx` hero actions | ✅ |

### What's defined but not implemented

The `ProfileSettings` interface already has these fields, but no UI controls or rendering logic:
- `background_type?: string` — "default" | "color" | "image" | "pattern"
- `background_value?: string` — CSS color, image URL, or pattern name
- `marquee_enabled?: boolean`
- `marquee_text?: string`
- `marquee_color?: string`
- `marquee_speed?: string` — "slow" | "medium" | "fast"

---

## P0: Custom Backgrounds

### Problem

Users can pick from 6 preset themes but can't set their own background color, image, or pattern. The `ProfileSettings` type already has `background_type` and `background_value` fields — they just need UI and rendering.

### Requirements

Add a **Background** section to the theme picker dropdown (or as a separate panel in edit mode):

1. **Solid color**: Color picker input → stores hex value in `background_value`
2. **Image URL**: Text input for image URL → renders as `background-image: url(...); background-size: cover`
3. **Pattern**: Selectable from a set of CSS-generated retro patterns (scanlines, checkerboard, dots, stars, diamonds)
4. **Default**: Resets to the active theme's default gradient

### UI Design

In edit mode, when the theme picker is open, add a second section below the theme swatches:

```
┌─ Theme ────────────────────────┐
│ [Default] [Midnight] [Sunset]  │
│ [Forest]  [Ocean]  [Retrowave] │
├─ Background ───────────────────┤
│ ○ Default  ○ Color  ○ Image    │
│ ○ Pattern                      │
│                                │
│ [color picker / URL input /    │
│  pattern selector based on     │
│  selected radio]               │
└────────────────────────────────┘
```

### Rendering

Apply background to `.profile-geocities`:
- If `background_type === 'color'`: `background: {background_value}`
- If `background_type === 'image'`: `background: url({background_value}) center/cover no-repeat fixed`
- If `background_type === 'pattern'`: Apply the selected CSS pattern (each pattern is a CSS-only repeating-gradient or background-image combo)

### Pattern Presets

```typescript
const BACKGROUND_PATTERNS: Record<string, React.CSSProperties> = {
    scanlines: {
        background: 'repeating-linear-gradient(0deg, transparent, transparent 2px, rgba(0,0,0,0.15) 2px, rgba(0,0,0,0.15) 4px)',
    },
    checkerboard: {
        background: 'repeating-conic-gradient(rgba(255,255,255,0.05) 0% 25%, transparent 0% 50%) 0 0 / 20px 20px',
    },
    dots: {
        background: 'radial-gradient(circle, rgba(255,255,255,0.08) 1px, transparent 1px) 0 0 / 16px 16px',
    },
    stars: {
        background: 'radial-gradient(2px 2px at 10px 10px, rgba(255,255,255,0.3), transparent), radial-gradient(1px 1px at 30px 25px, rgba(255,255,255,0.2), transparent), radial-gradient(1px 1px at 50px 40px, rgba(255,255,255,0.15), transparent)',
        backgroundSize: '60px 60px',
    },
    diamonds: {
        background: 'linear-gradient(45deg, rgba(255,255,255,0.04) 25%, transparent 25%, transparent 75%, rgba(255,255,255,0.04) 75%), linear-gradient(45deg, rgba(255,255,255,0.04) 25%, transparent 25%, transparent 75%, rgba(255,255,255,0.04) 75%)',
        backgroundPosition: '0 0, 10px 10px',
        backgroundSize: '20px 20px',
    },
};
```

### Files to modify

| File | Changes |
|------|---------|
| `src/screens/ProfileScreen.tsx` | Add background picker UI in edit mode; apply background styles to .profile-geocities |
| `src/screens/ProfileScreen.css` | Background overlay styles, pattern classes |

---

## P1: Marquee Ticker (Opt-In)

### Problem

The old scrolling ticker was removed in v18 ("tacky"), but user feedback says to keep it as an **opt-in customization** with message, font, color, and speed controls.

### Requirements

1. Off by default (`marquee_enabled: false`)
2. In edit mode → show a "Marquee" toggle + configuration panel:
   - Message text input
   - Color picker (text color)
   - Speed selector (slow / medium / fast)
3. When enabled → render a `<marquee>`-style scrolling bar at the bottom of the hero area (or below the page nav)
4. Use CSS animation (not actual `<marquee>` element) for modern compatibility

### UI Design

In edit mode, add a collapsible "Marquee Ticker" section below the theme/background picker:

```
┌─ Marquee Ticker ──── [ON/OFF] ─┐
│ Message: [_________________]    │
│ Color:   [■ #ff00ff]           │
│ Speed:   ○ Slow  ● Medium  ○ Fast │
└─────────────────────────────────┘
```

### Rendering

When `profileSettings.marquee_enabled === true`:

```html
<div class="profile-marquee" style="color: {marquee_color}">
    <div class="profile-marquee-content" style="animation-duration: {speed}s">
        {marquee_text}
    </div>
</div>
```

CSS animation: `@keyframes marquee-scroll { from { transform: translateX(100%); } to { transform: translateX(-100%); } }`

Speed map: slow = 20s, medium = 12s, fast = 6s.

### Files to modify

| File | Changes |
|------|---------|
| `src/screens/ProfileScreen.tsx` | Marquee config UI in edit mode; marquee render below hero/nav |
| `src/screens/ProfileScreen.css` | Marquee animation, ticker bar styles |

---

## P2: Avatar Image

### Problem

Profile currently shows a letter avatar (first letter of display name). Users need to set an actual profile picture.

### Requirements

1. In edit mode → clicking the avatar opens an image URL input (or file picker if backend supports it)
2. Store as `avatar_url` in ProfileSettings
3. Render the image (with fallback to letter avatar on load error)
4. Circular avatar with the theme's accent border

### Implementation

Add `avatar_url?: string` to `ProfileSettings` in `types/index.ts`.

In ProfileScreen hero area, make the avatar clickable in edit mode:

```typescript
{editMode ? (
    <div className="profile-avatar-large profile-avatar-editable" onClick={() => setShowAvatarInput(true)}>
        {profileSettings.avatar_url ? (
            <img src={profileSettings.avatar_url} alt="avatar" onError={(e) => { e.currentTarget.style.display = 'none'; }} />
        ) : (
            <span>{displayName[0].toUpperCase()}</span>
        )}
        <div className="profile-avatar-edit-overlay">📷</div>
    </div>
) : (
    <div className="profile-avatar-large">
        {profileSettings.avatar_url ? (
            <img src={profileSettings.avatar_url} alt="avatar" />
        ) : (
            <span>{displayName[0].toUpperCase()}</span>
        )}
    </div>
)}
```

### Files to modify

| File | Changes |
|------|---------|
| `src/types/index.ts` | Add `avatar_url?: string` to `ProfileSettings` |
| `src/screens/ProfileScreen.tsx` | Avatar image rendering + edit mode URL input |
| `src/screens/ProfileScreen.css` | Avatar image styles, edit overlay |

---

## P3: Custom Color Picker

### Problem

The 6 preset themes are good starting points, but some users want their own exact colors. Need a custom color option that lets them pick hero gradient colors and accent color.

### Requirements

1. Add a "Custom" option to the theme picker (7th swatch)
2. When "Custom" is selected, show three color inputs:
   - **Gradient Start** (hero background gradient start)
   - **Gradient End** (hero background gradient end)
   - **Accent Color** (text highlights, borders)
3. Store in ProfileSettings as:
   - `custom_gradient_start?: string`
   - `custom_gradient_end?: string`
   - `custom_accent?: string`
4. Apply via CSS custom properties (same pattern as existing themes)

### Implementation

Add fields to `ProfileSettings` in `types/index.ts`.

When `theme === 'custom'`, generate the theme vars dynamically:

```typescript
const customThemeVars = profileSettings.theme === 'custom' ? {
    '--profile-hero-bg': `linear-gradient(135deg, ${profileSettings.custom_gradient_start || '#1a1a2e'} 0%, ${profileSettings.custom_gradient_end || '#16213e'} 100%)`,
    '--profile-hero-accent': profileSettings.custom_accent || '#e2e8f0',
    '--profile-card-bg': `rgba(0,0,0,0.3)`,
} : {};
```

### Files to modify

| File | Changes |
|------|---------|
| `src/types/index.ts` | Add `custom_gradient_start`, `custom_gradient_end`, `custom_accent` to `ProfileSettings` |
| `src/screens/ProfileScreen.tsx` | Custom theme swatch + color inputs + dynamic theme vars |
| `src/screens/ProfileScreen.css` | Custom theme picker layout |

---

## P4: GIF Section Type

### Problem

User wants "a section full of gif buttons as a custom section" — old-web style GIF collections as profile decoration.

### Requirements

1. Add a new section type: `'gif-collection'` to `ProfileBioSections`
2. In edit mode → user can add GIFs by URL (paste in a URL)
3. GIFs display in a responsive grid (2-3 columns, auto-height)
4. Clicking a GIF in view mode could do nothing (decoration) or open it full-size
5. Store GIF URLs as JSON array in `section.content`

### Implementation

Add to `ProfileBioSections.tsx`:

```typescript
// In addableTypes:
{ type: 'gif-collection', label: 'GIF Collection', emoji: '🖼️' },

// New renderer:
function renderGifCollection(section: ProfileSection) {
    let gifs: string[] = [];
    try { gifs = JSON.parse(section.content || '[]'); } catch { gifs = []; }
    
    if (gifs.length === 0) {
        return <p className="bio-section-placeholder">
            {editing ? 'Click ✏️ to add GIF URLs...' : 'No GIFs yet.'}
        </p>;
    }
    
    return (
        <div className="bio-gif-grid">
            {gifs.map((url, i) => (
                <img key={i} src={url} alt="GIF" className="bio-gif-item" loading="lazy" />
            ))}
        </div>
    );
}
```

GIF editor (in edit mode): list of URL inputs with add/remove, similar to the links editor.

### Column placement

GIF collections → `'full'` width (below the 3-column grid) for maximum visual impact.

### Files to modify

| File | Changes |
|------|---------|
| `src/components/ProfileBioSections.tsx` | Add 'gif-collection' section type, renderer, and editor |
| `src/components/ProfileBioSections.css` | GIF grid styles (responsive, auto-height) |

---

## P5: Connections Expansion

### Problem

Connections currently show 8 handles with a "+N more" badge. User wants searchable, filterable by source, and expandable to view all.

### Requirements

1. **Expand/collapse**: "Show all" button that reveals full list (scrollable, max-height)
2. **Search**: Filter input to search by handle name
3. **Source filter**: Tabs or buttons for "All / Bluesky / P2P / Mastodon"
4. **Better badges**: Show source icon with each connection (🦋 / 🤝 / 🐘)

### Implementation

Modify `renderConnections()` in `ProfileBioSections.tsx`:

```typescript
const [connectionsExpanded, setConnectionsExpanded] = useState(false);
const [connectionSearch, setConnectionSearch] = useState('');

// Load ALL follows (not just 8) when expanded
useEffect(() => {
    if (connectionsExpanded && did) {
        invoke<Array<...>>('list_follows', { actor: did })
            .then(follows => setAllFollows(follows));
    }
}, [connectionsExpanded, did]);
```

Add a search bar + source filter tabs at top of connections section. Filtered list renders in a scrollable grid area.

### Files to modify

| File | Changes |
|------|---------|
| `src/components/ProfileBioSections.tsx` | Expand connections with search, filter, and full list |
| `src/components/ProfileBioSections.css` | Expanded connections list styles, search bar |

---

## P6: Richer Page Editor

### Problem

Profile pages currently use a plain `<textarea>` for content. Users want richer editing closer to a Website builder.

### Requirements

1. **Markdown rendering**: Parse page content as Markdown and render it (use a lightweight parser — `marked` or manual regex for basic Markdown: headers, bold, italic, links, images, lists, code blocks)
2. **Live preview**: Side-by-side or toggle between edit and preview
3. **Embed support**: Accept iframe embed URLs (YouTube, Bandcamp, external sites) in page content — detect and render inline
4. **Gallery page type**: The `gallery` content type exists in `ProfilePage` but isn't implemented. Add a simple grid layout for image URLs.

### Markdown rendering approach

To avoid adding a heavy dependency, implement a lightweight Markdown-to-HTML renderer:
- `# heading` → `<h1>`
- `**bold**` → `<strong>`
- `*italic*` → `<em>`
- `[text](url)` → `<a>`
- `![alt](url)` → `<img>`
- `` `code` `` → `<code>`
- Lists (`-` / `1.`)
- Line breaks

Or, if `marked` is already in `package.json`, use it. Check first: `grep marked package.json`.

### Gallery page type

```typescript
if (page.content_type === 'gallery') {
    return renderGalleryPage();
}

function renderGalleryPage() {
    let images: string[] = [];
    try { images = JSON.parse(content || '[]'); } catch { images = []; }
    
    return (
        <Win98Box title={`🖼️ ${page.title}`}>
            {editMode && (/* URL input editor, similar to GIF editor */)}
            <div className="profile-gallery-grid">
                {images.map((url, i) => (
                    <img key={i} src={url} className="profile-gallery-image" loading="lazy" />
                ))}
            </div>
        </Win98Box>
    );
}
```

### Files to modify

| File | Changes |
|------|---------|
| `src/components/ProfilePageEditor.tsx` | Add Markdown rendering, live preview toggle, embed detection, gallery page type |
| `src/screens/ProfileScreen.css` | Gallery grid, preview panel, Markdown rendered content styles |

---

## Verification

Before committing:

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
