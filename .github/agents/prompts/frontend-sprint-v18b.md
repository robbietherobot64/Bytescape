# Frontend Sprint v18b — Profile Polish (Rich Media, Connections, Customization)

**Sprint**: v18b (Profile Polish — Part 2)  
**Track**: B (Frontend — React/TypeScript)  
**Date**: 2026-04-08  
**Depends on**: v18a (layout + landing page changes must land first)  
**HEAD at prompt time**: `3f35751`  
**Context**: After v18a fixes the structural issues, this sprint adds the rich media, smart defaults, and creative customization the profile needs.

---

## Sprint Context

This sprint is about making the profile sections **actually useful and delightful**: Now Playing with real embeds, Links that auto-populate, Connections that are navigable, and creative customization options that let users express themselves.

### Key files

| File | Lines | Purpose |
|------|-------|--------|
| `src/components/ProfileBioSections.tsx` | 769 | Wall sections — main editing target |
| `src/components/ProfileBioSections.css` | 422 | Section styles |
| `src/screens/ProfileScreen.tsx` | ~324 | Profile page (post-v18a changes) |
| `src/screens/ProfileScreen.css` | 229 | Profile page styles |
| `src/components/widgets/MusicPlayerWidget.tsx` | varies | GrapesJS music player widget |
| `src/types/index.ts` | ~700 | Types (AuthStatus has linked_accounts) |

---

## P0: Now Playing — Rich Media Embeds

### Problem
User feedback: "Now playing should allow for bandcamp/soundcloud embeds. Or some other way to have rich media embedded. This can autoplay if the user so chooses to enable it."

### Current state
The Now Playing section in `ProfileBioSections.tsx` renders a spinning 💿 emoji + plain text content. No embed support whatsoever.

### Required changes

#### 0a. Embed URL detection
When the user enters a URL in the Now Playing content, detect and render an embed:

| Platform | URL Pattern | Embed Method |
|----------|-------------|-------------|
| Bandcamp | `*.bandcamp.com/track/*` or `*.bandcamp.com/album/*` | `<iframe>` with Bandcamp player |
| SoundCloud | `soundcloud.com/*` | oEmbed API → `<iframe>` |
| Spotify | `open.spotify.com/track/*` or `/album/*` | Spotify embed `<iframe>` |
| YouTube | `youtube.com/watch?v=*` or `youtu.be/*` | YouTube embed `<iframe>` |

#### 0b. Embed rendering
```tsx
function NowPlayingEmbed({ url, autoplay }: { url: string; autoplay: boolean }) {
    // Bandcamp
    if (url.includes('bandcamp.com')) {
        // Extract album/track ID, render Bandcamp embed player
        // https://bandcamp.com/EmbeddedPlayer/album=XXXX/size=small/bgcol=333333/linkcol=ffffff
        return <iframe src={embedUrl} style={{ border: 0, width: '100%', height: 120 }} />;
    }
    // SoundCloud
    if (url.includes('soundcloud.com')) {
        // Use SoundCloud widget: https://w.soundcloud.com/player/?url=ENCODED_URL
        return <iframe src={`https://w.soundcloud.com/player/?url=${encodeURIComponent(url)}&auto_play=${autoplay}`} ... />;
    }
    // Spotify
    if (url.includes('open.spotify.com')) {
        const embedPath = url.replace('open.spotify.com', 'open.spotify.com/embed');
        return <iframe src={embedPath} style={{ borderRadius: 12, width: '100%', height: 152 }} allow="autoplay" />;
    }
    // YouTube (audio/video)
    if (url.includes('youtube.com') || url.includes('youtu.be')) {
        const videoId = extractYouTubeId(url);
        return <iframe src={`https://www.youtube.com/embed/${videoId}?autoplay=${autoplay ? 1 : 0}`} ... />;
    }
    // Fallback: just show the link
    return <a href={url} target="_blank">{url}</a>;
}
```

#### 0c. Autoplay toggle
Add an autoplay setting to the Now Playing section. In edit mode:
- Toggle switch: "Autoplay when visitors view your profile"
- Store as part of section content or as a separate field

#### 0d. UI in non-edit mode
```
┌──────────────────────────┐
│ 🎵 Now Playing           │
│ ┌──────────────────────┐ │
│ │ [Bandcamp embed       │ │
│ │  player widget]       │ │
│ └──────────────────────┘ │
│ Song Title — Artist      │
└──────────────────────────┘
```

#### 0e. Content model
The Now Playing section content should support either:
- Plain text (backward compat): `"Song Name — Artist"`
- Structured JSON: `{"url": "https://artist.bandcamp.com/track/song", "title": "Song Name", "artist": "Artist", "autoplay": false}`

Parse both formats. In edit mode, show a form with URL input + title + artist + autoplay toggle. If only text is entered (no URL), render the spinning 💿 + text as before.

### Files to modify
- `src/components/ProfileBioSections.tsx` — rewrite `renderNowPlaying()`, add embed component
- `src/components/ProfileBioSections.css` — embed iframe styles, autoplay toggle

---

## P1: Links Auto-Population from Linked Accounts

### Problem
User feedback: "Links should autopopulate with the accounts the user has linked."

### Current state
The Links section renders from a manually edited JSON array: `[{"label": "...", "url": "..."}]`. Users must type each link manually in edit mode.

### Required changes

#### 1a. Auto-populate from AuthStatus
`AuthStatus` has `linked_accounts?: LinkedAccount[]` where each `LinkedAccount` has:
```typescript
interface LinkedAccount {
    protocol: string;      // "bluesky", "facebook", "instagram"
    identifier: string;    // handle or ID
    display_handle?: string;
    active: boolean;
}
```

When the Links section has no user-defined links (empty `[]`), auto-populate with linked accounts:

| Protocol | Generated Link |
|----------|---------------|
| bluesky | `https://bsky.app/profile/{identifier}` → label: "Bluesky" |
| facebook | `https://facebook.com/{identifier}` → label: "Facebook" |
| instagram | `https://instagram.com/{identifier}` → label: "Instagram" |

#### 1b. Implementation
- Pass `auth` (or just `auth.linked_accounts`) as a prop to ProfileBioSections
- In the links renderer, merge auto-populated links with user-defined links
- Auto links shown with a platform emoji + "(linked)" badge
- Users can remove individual auto links (by adding them to a "hidden" list in content JSON)
- Users can still add custom links on top

#### 1c. Edit mode
In edit mode, show:
```
Linked Accounts (auto):
  🦋 Bluesky — rabie.bsky.social  [hide]
  📘 Facebook — Rabie Zaater      [hide]

Custom Links:
  🔗 My Website — https://...     [edit] [remove]
  🔗 GitHub — https://...         [edit] [remove]
  [+ Add Link]
```

### Files to modify
- `src/screens/ProfileScreen.tsx` — pass auth/linked_accounts to ProfileBioSections
- `src/components/ProfileBioSections.tsx` — accept linked_accounts prop, auto-populate links renderer, merge logic
- `src/components/ProfileBioSections.css` — linked account badge styling

---

## P2: Connections — Navigable, Searchable, Filterable

### Problem
User feedback: "Connections should be more navigable, searchable, filterable by source, expandable to view all connections."

### Current state
The Connections section renders a badge grid: Bluesky handle, follow count, P2P contact count, and first 6 follow handles. No search, no filtering, no expansion.

### Required changes

#### 2a. Expandable view
- Show first 8 connections in the sidebar card (compact mode)
- "View All Connections (N)" button that either:
  - Expands the section to show all connections (scrollable, max-height with overflow)
  - Or opens a modal/overlay with the full connections list

#### 2b. Source filtering
Add filter tabs/chips inside the connections section:
```
[All] [Bluesky (142)] [P2P (3)] [Facebook (87)]
```
Use `invoke('list_vault_sources')` to get available sources.

#### 2c. Search
Add a search input at the top of the expanded view:
```
🔍 Search connections...
```
Filter connections by handle/name matching the search query (client-side).

#### 2d. Connection cards
Each connection should show:
- Avatar (first letter or platform icon)
- Handle/name
- Source badge (emoji + platform name)
- Follow/follower relationship indicator

#### 2e. Data loading
Current commands:
- `list_follows` → returns `{ follows: number, handles: string[] }` (only first 8)
- `list_contacts` → returns `{ count: number }`

**Backend dependency**: We may need new/expanded commands to get ALL follows with metadata. For now, use what's available and paginate client-side. If `list_follows` only returns first 8, we need Track A to add a `list_all_follows` or expand the existing command.

Document this dependency clearly. If the full list isn't available, show what we have with a note: "Showing first 8 of N connections."

### Files to modify
- `src/components/ProfileBioSections.tsx` — rewrite `renderConnections()` with search, filter, expand
- `src/components/ProfileBioSections.css` — connection cards, search bar, filter chips, scrollable list

---

## P3: Profile Customization Options

### Problem
User feedback: "Overall increase of customization options... Custom backgrounds? Custom themes? What creative ideas can we come up with that allows users to be creative on their profile?"

### Required changes

#### 3a. Profile Theme Selector
Add a theme picker in edit mode (accessible from the "Edit Profile" button added in v18a). Themes affect the profile's visual style:

| Theme | Description |
|-------|-------------|
| `default` | Current dark purple/gold geocities style |
| `midnight` | Deep navy + cyan accents |
| `sunset` | Warm orange/pink gradient |
| `forest` | Dark green + amber |
| `retro` | Classic web 1.0 — gray background, Times New Roman, blue links |
| `minimal` | Clean white/light with minimal decoration |
| `vaporwave` | Pink/cyan gradients, glitch effects |
| `terminal` | Green on black, monospace everything |

Implementation:
- Store selected theme in profile sections data (or separate key in widget data)
- Each theme is a set of CSS custom property overrides applied to `.profile-geocities`
- Define themes as a record of CSS variable values
- In edit mode, show a grid of theme preview cards to pick from

```tsx
const PROFILE_THEMES: Record<string, Record<string, string>> = {
    default: {}, // no overrides — uses existing styles
    midnight: {
        '--profile-bg': '#0a0a2e',
        '--profile-accent': '#00d4ff',
        '--profile-text': '#e0e0ff',
        '--profile-hero-gradient': 'linear-gradient(135deg, #0a0a2e, #1a1a4e)',
    },
    // ...
};
```

#### 3b. Custom Background
In edit mode, allow users to set a custom background:
- **Color picker**: Solid background color
- **Image upload**: Background image (stored via `save_profile_layout` or separate command)
- **Pattern**: Select from built-in patterns (stars, dots, grid, scanlines)

Store the choice in widget data JSON. Apply as inline styles on `.profile-geocities`.

#### 3c. Customizable Marquee (from v18a P5)
When users add a marquee section, they can customize:
- **Text content**: Custom message
- **Font**: System fonts + web-safe decorative fonts
- **Color**: Text color + background color pickers
- **Speed**: Slow / Medium / Fast
- **Emojis**: Add emoji separators

#### 3d. GIF Sections
User feedback: "What sort of native gif embedding can I do? Can I have a section full of gif buttons as a custom section?"

Add a new section type `'gif-collection'`:
- Users paste GIF URLs (from Giphy, Tenor, etc.)
- Rendered as a grid of animated GIFs
- In edit mode: URL input + preview + remove button
- Consider adding a Giphy/Tenor search integration (API key required — defer API integration, start with paste-URL)

Content model:
```json
{
    "gifs": [
        {"url": "https://media.giphy.com/...", "alt": "dancing cat"},
        {"url": "https://media.tenor.com/...", "alt": "cool animation"}
    ],
    "columns": 3,
    "size": "medium"
}
```

### Files to modify
- `src/components/ProfileBioSections.tsx` — theme picker UI, GIF section type, marquee customization
- `src/components/ProfileBioSections.css` — theme variables, GIF grid styles
- `src/screens/ProfileScreen.tsx` — apply theme CSS variables to container
- `src/screens/ProfileScreen.css` — theme override support, custom background

---

## P4: Profile Section Type Updates

### Summary of all section type changes across this sprint

| Section Type | Change |
|-------------|--------|
| `about` | No changes |
| `now-playing` | P0: Rich embeds (Bandcamp, SoundCloud, Spotify, YouTube), autoplay toggle |
| `links` | P1: Auto-populate from linked accounts, merge with custom links |
| `guestbook` | No changes (structural changes in v18a) |
| `connections` | P2: Search, filter by source, expand to view all |
| `custom` | No changes |
| `marquee` | NEW (from v18a P5): Customizable text, font, color, speed |
| `gif-collection` | NEW (P3d): Grid of animated GIFs |

Update the `ProfileSection.type` union:
```typescript
type: 'about' | 'connections' | 'now-playing' | 'guestbook' | 'links' | 'custom' | 'marquee' | 'gif-collection';
```

Update the "Add Section" dropdown in edit mode to include new types:
```
📝 About Me
🎵 Now Playing
🔗 Links
📖 Guestbook
👥 Connections
⭐ Marquee Banner
🖼️ GIF Collection
📄 Custom Section
```

### Files to modify
- `src/components/ProfileBioSections.tsx` — update type union, add section dropdown entries, add renderers

---

## Verification

Before committing:

```bash
cd bytescape
npx tsc --noEmit        # 0 errors
pnpm test -- --run      # all tests pass
```

### Manual verification
1. Now Playing: paste a Bandcamp URL → embedded player renders
2. Now Playing: paste a SoundCloud URL → embedded player renders
3. Now Playing: autoplay toggle works in edit mode
4. Links: linked Bluesky account auto-appears in links section
5. Connections: search bar filters connections
6. Connections: source filter tabs work
7. Connections: "View All" expands to show full list
8. Themes: selecting a theme changes profile colors
9. GIF section: paste GIF URLs, grid renders
10. Marquee: customizable text/color/speed in edit mode

---

## Track Boundaries
- **Track B owns**: `src/` (all React/TypeScript, CSS)
- **Do NOT modify**: `src-tauri/src/` (Track A)
- **Backend dependencies**: Document in progress.md if `list_follows` needs expansion for P2 (connections). Track A can address in parallel.
