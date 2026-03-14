# Frontend Agent Kickoff: B2.1 GrapesJS Profile Builder

## Context

Bytescape profiles are currently a static `ProfileScreen` that displays imported posts in a wall layout. The goal of B2.1 is to give users a **drag-and-drop visual profile builder** so they can design their own profile pages — think MySpace/GeoCities customization but with a modern builder UX.

[GrapesJS](https://grapesjs.com/) is an open-source web builder framework that outputs clean HTML/CSS. It provides a visual drag-and-drop canvas, a component/block system, and a style manager — exactly what we need.

## Your Task

### Step 1: Install GrapesJS

```bash
cd bytescape && pnpm add grapesjs grapesjs-preset-webpage
```

Check for React wrapper packages (`grapesjs-react`) — if a maintained one exists, prefer it. Otherwise, mount GrapesJS imperatively in a `useEffect`.

### Step 2: Create `ProfileBuilderScreen.tsx`

**New file:** `src/screens/ProfileBuilderScreen.tsx`

This is a new top-level screen accessible from the sidebar/settings. It should:

1. **Mount a GrapesJS editor** in a full-width canvas area
2. **Provide Bytescape-specific blocks** (pre-built drag-and-drop components):
   - **About Me** — text block with editable bio
   - **Post Feed** — embedded feed widget showing recent posts
   - **Media Grid** — gallery of vault images
   - **Friends List** — peer list widget
   - **Music Player** — placeholder widget (wired later in B2.4)
   - **Guestbook** — placeholder widget (wired later in B2.4)
   - **Away Message** — status text block
3. **Save the output** — GrapesJS produces HTML + CSS. Store it locally via a Tauri command. For now, store in redb as a `profile_layout` key-value pair. Create a simple Tauri command:
   ```rust
   #[tauri::command]
   pub async fn save_profile_layout(html: String, css: String, state: State<'_, AppState>) -> Result<(), String>
   
   #[tauri::command]
   pub async fn get_profile_layout(state: State<'_, AppState>) -> Result<Option<ProfileLayout>, String>
   ```
4. **Load saved layouts** — on mount, if a saved layout exists, load it into the GrapesJS editor
5. **Preview button** — toggle between edit mode and preview mode

### Step 3: Integrate with ProfileScreen

Modify `src/screens/ProfileScreen.tsx` to:
1. Check if user has a custom profile layout (`get_profile_layout`)
2. If yes, render the saved HTML/CSS in a **sandboxed container** (use `<div>` with `dangerouslySetInnerHTML` + DOMPurify sanitization, or a sandboxed `<iframe>` for full isolation — `<iframe>` is preferred per B2.2)
3. If no, show the existing default profile wall
4. Add an "Edit Profile" button that navigates to `ProfileBuilderScreen`

### Step 4: Add Navigation

Add the ProfileBuilder to the app's navigation:
- Add a route in `App.tsx` or the router configuration
- Add a sidebar/settings entry to access it
- Icon suggestion: paint palette or layout grid icon from `react-icons`

### Step 5: Theme Integration

The profile builder should respect the current theme (Cozy/Desktop/Midnight from B2.5):
- GrapesJS canvas background should match `--bg-primary`
- The builder's panel/sidebar should use the Win98Box styling
- Pre-built blocks should use the current theme's CSS variables so the profile looks consistent

### Step 6: Verify

1. `pnpm tauri dev` compiles without errors
2. `pnpm test` passes (no regressions)
3. Can open ProfileBuilderScreen, drag blocks onto canvas, save, and see result on ProfileScreen

---

## Design Constraints

- **Security:** The profile HTML output will eventually be shown to peers (B3.2). It MUST be sanitized before rendering. Use DOMPurify's `sanitizeHtml()` (already available in the codebase via `src/utils/sanitize.ts`). Better yet, render in a sandboxed `<iframe>` with `sandbox="allow-same-origin"` (no scripts).
- **GeoCities Aesthetic:** The builder UI should maintain the Win98/GeoCities visual language. Use `Win98Box` for panels. The blocks should have the beveled-edge style.
- **No External Hosting Required:** Everything stores locally. No server needed for B2.1 — that comes later with B2.6.

## GrapesJS Quick Reference

```typescript
import grapesjs from 'grapesjs';
import 'grapesjs/dist/css/grapes.min.css';

// In useEffect:
const editor = grapesjs.init({
  container: '#gjs',
  fromElement: true,
  width: 'auto',
  height: '100%',
  storageManager: false, // We handle storage ourselves via Tauri
  plugins: ['grapesjs-preset-webpage'],
  blockManager: {
    blocks: [
      { id: 'about-me', label: 'About Me', content: '<div class="bs-about">...</div>' },
      // ... more blocks
    ]
  }
});

// Get output:
const html = editor.getHtml();
const css = editor.getCss();

// Load existing:
editor.setComponents(savedHtml);
editor.setStyle(savedCss);
```

---

## Files You Will Create
- `src/screens/ProfileBuilderScreen.tsx`
- `src/screens/ProfileBuilderScreen.test.tsx` (basic render test)

## Files You Will Modify
- `src/screens/ProfileScreen.tsx` (custom layout rendering + "Edit Profile" button)
- `src/App.tsx` (add route for ProfileBuilderScreen)
- `src/components/Sidebar.tsx` (add navigation entry)
- `bytescape/src-tauri/src/commands/` (new profile layout commands — small Rust addition)
- `bytescape/src-tauri/src/lib.rs` (register new commands)
- `bytescape/src-tauri/src/db/mod.rs` (profile_layout table, if needed)
- `bytescape/package.json` (grapesjs dependency — via pnpm add)
- `ROADMAP.md` (mark B2.1 complete when done)

## Files You Must NOT Touch
- `src/components/ComposeBox.tsx` (Track C)
- `src/components/ArticleEditor.tsx` (Track C)
- `bytescape/src-tauri/src/oauth.rs` (Backend agent)
- `bytescape/src-tauri/src/connectors/bluesky.rs` (Backend agent)
