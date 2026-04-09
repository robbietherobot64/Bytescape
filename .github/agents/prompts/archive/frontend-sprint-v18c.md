# Frontend Sprint v18c — Profile as Personal Website (Pages & Directory)

**Sprint**: v18c (Profile Polish — Part 3)  
**Track**: B (Frontend — React/TypeScript)  
**Date**: 2026-04-08  
**Depends on**: v18a (layout), v18b (customization)  
**HEAD at prompt time**: `3f35751`  
**Context**: User wants the profile to function as a personal website/blog with navigable pages. Think Neocities/Geocities + modern CMS.

---

## Sprint Context

User feedback: "How can we implement a directory system on the profile that allows users to have a navigable site of pages that they customize? Let's make this into a rich website editor, and supporting of all features like sites like neocities. So if I want to be able to natively link to and display a wordpress site on my profile, I can."

This is the most ambitious profile feature. It turns the profile from a single-page view into a **multi-page personal website** with navigation.

### Architecture decision
The profile already has two editing modes:
1. **ProfileBioSections** — structured sections (About, Links, etc.) — simple, everyone uses it
2. **ProfileBuilderScreen** (GrapesJS) — drag-and-drop layout builder — power users
3. **RawHtmlEditor** — raw HTML/CSS/JS — advanced users

The **Pages system** builds on top of these. Each page is a separate layout that users can create, name, and link to. The profile becomes a site with navigation.

---

## P0: Page Data Model

### New types

Add to `src/types/index.ts`:

```typescript
export interface ProfilePage {
    id: string;
    slug: string;           // URL-friendly name: "about", "blog", "music"
    title: string;          // Display name: "About Me", "My Blog", "Music"
    content_type: 'sections' | 'builder' | 'html' | 'embed';
    content: string;        // JSON (sections), HTML (builder/html), URL (embed)
    css?: string;           // Custom CSS for builder/html pages
    js?: string;            // Custom JS for html pages
    order: number;          // Navigation order
    visible: boolean;       // Show in nav
    icon?: string;          // Emoji or icon for nav
    created_at: string;
    updated_at: string;
}
```

Content types explained:
| Type | Editor | Use Case |
|------|--------|----------|
| `sections` | ProfileBioSections-style structured editor | Default profile, simple pages |
| `builder` | GrapesJS drag-and-drop | Visual layout pages |
| `html` | RawHtmlEditor | Full creative control (Neocities-style) |
| `embed` | URL input | WordPress, external site embedding |

### Default pages
Every profile starts with one page:
```typescript
const DEFAULT_PAGES: ProfilePage[] = [
    {
        id: 'home',
        slug: 'home',
        title: 'Home',
        content_type: 'sections',
        content: '', // uses existing ProfileBioSections data
        order: 0,
        visible: true,
        icon: '🏠',
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
    }
];
```

### Files to modify
- `src/types/index.ts` — add `ProfilePage` interface

---

## P1: Page Navigation Bar

### Design
Add a tabbed navigation bar below the hero banner, above the content area:

```
┌─────────────────────────────────────────────────┐
│ Hero Banner (username, avatar, DID)              │
├─────────────────────────────────────────────────┤
│ 🏠 Home | 📝 Blog | 🎵 Music | 🔗 Links | ⚙️ +│
├─────────────────────────┬───────────────────────┤
│ [Active page content]   │ [Sidebar widgets]     │
└─────────────────────────┴───────────────────────┘
```

### Implementation
- Navigation renders from the user's pages list, ordered by `page.order`
- Active page is tracked in component state: `const [activePage, setActivePage] = useState('home')`
- The "+" button (only in edit mode) opens a "New Page" dialog
- Pages are styled as tabs matching the app's existing tab aesthetic (`.wall-filter-btn` style)

### Page routing
Since this is a local app (not URL-routed), page switching is purely state-driven:
```tsx
const [activePage, setActivePage] = useState<string>('home');
const pages = useProfilePages(); // loads from backend
```

When we go public (go-live sprint), pages will map to URL paths: `/@username/blog`, `/@username/music`, etc.

### Files to modify
- `src/screens/ProfileScreen.tsx` — add page navigation bar, page state management
- `src/screens/ProfileScreen.css` — page nav tab styles

---

## P2: Page Editor

### New Page creation flow
When user clicks "+" in the navigation:

```
┌──────────────────────────────────┐
│ Create New Page                   │
│                                   │
│ Page Title: [_______________]     │
│ URL Slug:   [_______________]     │
│ Icon:       [🎵] (emoji picker)  │
│                                   │
│ Page Type:                        │
│ ○ Sections (structured editor)    │
│ ○ Visual Builder (drag & drop)    │
│ ○ HTML/CSS/JS (full control)      │
│ ○ Embed (external website)        │
│                                   │
│ [Create Page]  [Cancel]           │
└──────────────────────────────────┘
```

### Page editing
Based on `content_type`, render the appropriate editor:

| Type | Editor Component | Notes |
|------|-----------------|-------|
| `sections` | `<ProfileBioSections>` | Reuse existing component, scoped to page content |
| `builder` | `<ProfileBuilderScreen>` (embedded, not as separate screen) | GrapesJS with page-specific content |
| `html` | `<RawHtmlEditor>` | Full HTML/CSS/JS editing |
| `embed` | URL input + `<iframe>` preview | Sanitized iframe embedding |

### Embed page type
For "Embed" pages (WordPress, external sites):
```tsx
<div className="profile-embed-page">
    <iframe
        src={page.content}
        style={{ width: '100%', height: '80vh', border: 'none' }}
        sandbox="allow-scripts allow-same-origin allow-popups"
        title={page.title}
    />
</div>
```

In edit mode, show a URL input:
```
External URL: [https://mywordpress.com_________]
[Preview] [Save]
```

### Page management
In edit mode, each page tab gets a context menu (right-click or "⋮" button):
- **Rename** — change title/icon
- **Reorder** — move left/right (change order)
- **Change Type** — switch content type (with content migration warning)
- **Hide** — toggle visibility (page still exists but not in nav)
- **Delete** — remove page (with confirmation, "Home" cannot be deleted)

### Files to modify
- `src/screens/ProfileScreen.tsx` — page editor routing, new page dialog
- `src/components/ProfileBioSections.tsx` — accept `pageId` prop to scope sections per page
- New file: `src/components/PageEditor.tsx` — page creation/management modal
- New file: `src/components/PageEditor.css` — styling

---

## P3: Backend — Page Persistence

### Required Tauri commands (Track A dependency)

```rust
#[tauri::command]
async fn save_profile_page(state: State<'_, AppState>, page_json: String) -> Result<(), String>;

#[tauri::command]
async fn get_profile_pages(state: State<'_, AppState>) -> Result<Vec<String>, String>;
// Returns list of page JSONs

#[tauri::command]
async fn delete_profile_page(state: State<'_, AppState>, page_id: String) -> Result<(), String>;

#[tauri::command]
async fn reorder_profile_pages(state: State<'_, AppState>, page_ids: Vec<String>) -> Result<(), String>;
```

### Frontend stubs
Until Track A delivers these commands, stub them:
```typescript
async function saveProfilePage(page: ProfilePage): Promise<void> {
    try {
        await invoke('save_profile_page', { pageJson: JSON.stringify(page) });
    } catch {
        // Stub: save to localStorage as fallback
        const pages = JSON.parse(localStorage.getItem('profile_pages') || '[]');
        const idx = pages.findIndex((p: ProfilePage) => p.id === page.id);
        if (idx >= 0) pages[idx] = page; else pages.push(page);
        localStorage.setItem('profile_pages', JSON.stringify(pages));
    }
}
```

### Files to modify
- New file: `src/hooks/useProfilePages.ts` — hook for page CRUD operations
- `src/types/index.ts` — `ProfilePage` type (from P0)

---

## P4: Blog Page Template

### Problem
User wants the profile to work as a website/blog. Provide a "Blog" page template that users can add.

### Blog template
When creating a new page, offer templates:
```
Templates:
  📝 Blog — Post-style page with entries, dates, and tags
  🎵 Music — Showcase tracks and playlists
  📸 Gallery — Photo/media grid
  📄 Blank — Start from scratch
```

### Blog page implementation
The Blog template creates a `sections`-type page with:
- A new section type `'blog-entry'` that renders as a blog post card
- Title, date, body (markdown-rendered), optional images
- Entries sorted by date, newest first
- "New Post" button in edit mode

Content model:
```json
{
    "entries": [
        {
            "id": "entry-123",
            "title": "My First Post",
            "date": "2026-04-08",
            "body": "This is my first blog post...",
            "tags": ["personal", "intro"],
            "images": []
        }
    ]
}
```

### Files to modify
- `src/components/ProfileBioSections.tsx` — add `blog-entry` section type with renderer
- New file: `src/components/PageTemplates.tsx` — template definitions and selection UI

---

## Verification

Before committing:

```bash
cd bytescape
npx tsc --noEmit        # 0 errors
pnpm test -- --run      # all tests pass
```

### Manual verification
1. Profile shows page navigation tabs below hero
2. "Home" page renders existing profile content
3. "+" button creates a new page with type selection
4. New sections-type page works with ProfileBioSections
5. New HTML-type page works with RawHtmlEditor
6. New embed-type page renders iframe with external URL
7. Pages can be renamed, reordered, hidden, deleted
8. Blog template creates a page with blog entries
9. Navigation works — switching pages shows different content

---

## Sprint Sequencing Note

This is the **third** profile sprint. The full sequence:

1. **v18a** — Layout & Landing Page (username, buttons, 2-column layout, post delete, marquee)
2. **v18b** — Rich Media & Customization (embeds, links, connections, themes, GIFs)
3. **v18c** — Pages & Directory (multi-page profile, navigation, blog, embeds)

After these three, reintroduce the **go-live sprints** (archived as `archive/backend-sprint-v18.md`, `archive/frontend-sprint-v18.md`, `archive/systems-infra-sprint-v18.md`). Renumber them as v19.

---

## Track Boundaries
- **Track B owns**: `src/` (all React/TypeScript, CSS)
- **Track A dependency**: Page persistence commands (P3) — document in progress.md
- **Shared file**: `src/types/index.ts` — check `git log --oneline -5 -- src/types/index.ts` before editing
