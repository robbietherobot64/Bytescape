# Frontend Agent — Sprint v6 (2026-03-19)

## ⚠️ OPERATIONAL DISCIPLINE — READ CAREFULLY

Read `.github/instructions/agent-guardrails.instructions.md` before doing anything.
Read `.github/agents/prompts/user-feedback` for the user's actual words.
Read `.github/agents/prompts/systems-advisory-2026-03-15.md` for the systems architect's recommendations.

### Organizational Context: Two-Agent Model

There are only **two agents**: you (Frontend/Track B) and the Backend agent (Track A — `src-tauri/src/`).

You own **all frontend code** including:
- `src/components/` (all React components, including ComposeBox, ThemeCustomizer, feed/ sub-components)
- `src/screens/` (all screen-level views)
- `src/hooks/` (all custom hooks)
- `src/store/` (Zustand stores)
- `src/types/` (TypeScript types)
- `src/App.css` and `src/themes/` (CSS)
- `tauri.conf.json` (UI config only)

### Performance Standards
- **Research before coding.** If a feature involves Tauri APIs or unfamiliar libraries, read the docs first. Use `fetch_webpage` on official documentation.
- **One feature per commit.** Each commit addresses one priority item.
- **Verify before committing:** `cargo check` + `npx tsc --noEmit` + `pnpm test -- --run` — all must pass.
- **Two-attempt cap:** If stuck after two genuine investigated attempts, document what you tried in `progress.md` and move on. Do not burn 5 commits on one feature.
- **Terminal discipline:** Do not spawn multiple `pnpm tauri dev` processes. Kill background processes when done. Do not use `sleep` loops. **Close every terminal you open when you are done with it** — do not leave zombie terminals.
- **Do not thrash files.** Check `git log --oneline -5 -- <filepath>` before editing. Check `git diff --stat` before every commit.

### Track Boundaries
Your scope is `src/` (all React/TypeScript), `src/App.css`, `src/themes/`, and `tauri.conf.json` (UI config only).
- **Do NOT create or modify files in `src-tauri/src/`** (Rust backend). That is Track A.
- If you need a Rust command that doesn't exist, document the need in `progress.md` and move on.

---

## 🚨 Codebase State (as of commit `66d2e1c`)

### What Already Works (DO NOT BREAK THESE)
- **4-tab feed** (All, Following, Subscribed, Saved) — `useUnifiedFeed.ts` handles all 4 properly
- **Multi-tab system** — NavStore in `store/index.ts`, TabBar component, instant switching
- **Batch blob resolution** — single IPC call for all feed images (`resolve_blob_urls_batch`)
- **Progressive video** — poster shown immediately, FFmpeg converts in background, cancel on scroll-away
- **Global video manager** — only one video plays at a time, double-play bug fixed
- **Settings screen** — working (localStorage quota bug fixed), export button wired to backend
- **FeedEntryCard decomposed** — `src/components/feed/` sub-components with `React.memo`
- **Theme CSS extracted** — `src/themes/{geocities,win98,midnight,hacker}.css`
- **ThemeCustomizer** — Per-color picker, 7 background presets, upload, tile/cover mode
- **ComposeBox** — 508 lines, media attachment, multi-platform posting
- **250 tests passing**, TypeScript clean, Rust clean (0 warnings)

### What Was Fixed in Sprint v5 (context)
- ✅ Window launches maximized
- ✅ Settings black screen fixed (localStorage quota guard)
- ✅ Feed load 29s→~2s (backend: OAuth lock + parallel fetches)
- ✅ Video stutter fixed (backend: 5 FFmpeg flag bugs)
- ✅ Video double-play fixed (global playback manager)
- ✅ Export button wired to backend with save dialog
- ✅ Dead deps removed (wouter, hls.js)
- ✅ Video centering in fullscreen gallery
- ✅ Theme CSS extracted, FeedEntryCard decomposed

---

## Sprint Theme: "Fix Sources & Build Profile"

Sprint v5 was a successful bugfix session. Most critical issues are resolved. The remaining work is: one new regression (source filter), one persistent issue (blank tabs), and the user's #1 feature request (profile bio).

---

## Phase 1 — Critical Bug Fixes

### P1 — Fix Source Filter Regression ⚡⚡⚡ CRITICAL NEW BUG

**User:** *"'Sources' dropdown on the feed does not properly filter Facebook, it shows up as Bluesky posts. Also, it seems to break the feed in general, when you return to All after filtering, it only shows your posts, not the posts of others."*

Two problems:
1. **Facebook posts display with wrong source**: When filtering by Facebook, the posts that appear are actually Bluesky posts (wrong source field mapping)
2. **State corruption after filtering**: After selecting a source filter and returning to "All", only user's own posts appear instead of the full feed

**Investigation steps:**
1. Check the source filter dropdown in `FeedScreen.tsx` — what value does it pass when "Facebook" is selected?
2. Check `useUnifiedFeed.ts` — how does it apply the source filter? Does it pass it to the backend or filter client-side?
3. Check the backend's `get_timeline_unified` — does it have a `source_filter` parameter? What values does it expect?
4. Check if the source field on `FeedEntry` items correctly reflects their origin (Facebook vs Bluesky)
5. Check if resetting to "All" properly clears the filter state or if it leaves stale filter params

**The "All" state corruption** is likely a stale parameter issue — the source filter value is sent to the backend but not cleared when switching back to All.

---

### P2 — Fix Blank Tabs on Link/Profile Navigation ⚡⚡⚡ PERSISTENT

**User:** *"Links and profiles don't load in new tabs, just open a new tab and it says 'loading page' pretty much forever."*

Sprint v5 added a retry loop + error UI (`1890344`) so blank tabs aren't dead white screens anymore. But the underlying issue persists — URLs never actually load in the WebView.

**The real problem is the WebView never receives the URL.** The `commands/browser.rs` Rust commands exist but the window label lookup fails at runtime.

**Investigation steps:**
1. Read `WebViewScreen.tsx` — what does it do when it mounts? Does it invoke a backend command?
2. Read `commands/browser.rs` — what window label does `create_browser_view` use? Compare with `tauri.conf.json` window labels.
3. Check `tauri.conf.json` — what is the actual window label? Is it `"main"`, `"Bytescape"`, or something else?
4. If the label mismatch is confirmed, fix it in the Rust side by using the correct label — but **you cannot modify Rust files**. Document the exact fix needed in `progress.md` for Track A.

**If the Rust fix is out of scope**, consider an alternative approach:
- For **Bluesky profile** links (`bsky.app` URLs): parse the handle/DID from the URL and render a native profile view using existing Bluesky API commands (no WebView needed)
- For **external links**: use `@tauri-apps/plugin-opener` to open in the system browser as a fallback
- This gives users a functional experience while the WebView debugging continues in a future sprint

---

## Phase 2 — Profile Bio Feature

### P3 — Profile Default Bio Boxes ⚡⚡ FEATURE REQUEST

**User:** *"User profile needs default boxes added for 'About me:', 'Connections:', etc, that users can enter some basic info about themselves if they want, or delete those sections and add other custom widgets/boxes. Let's turn it into a robust social media bio by default that you can edit and customize in terms of size, location, color, background, adding gifs, animations, etc. Use spacehey.com or myspace as the [inspiration]."*

This is the user's #1 feature request. The profile screen currently has Wall/Posts tabs but lacks personal bio sections.

**Design — MySpace/SpaceHey-inspired profile:**
1. **Default sections** (editable, deletable, reorderable):
   - 📝 **About Me** — free-text bio area (rich text, supports basic formatting)
   - 👥 **Connections** — shows friends/followers count with avatars
   - 🎵 **Now Playing** — optional music/media embed (existing music player widget)
   - 📸 **Photos** — recent gallery items grid
   - 💬 **Guestbook** — existing guestbook widget
   - 🔗 **Links** — external links (website, socials, etc.)

2. **Editing mode:**
   - "✏️ Edit Profile" button toggles into edit mode
   - Each section becomes an editable card with: drag handle (reorder), resize handle, delete button, edit button
   - Click a section to edit its content inline
   - "Add Section" button to add new custom sections (free-text blocks)
   - Save/Cancel buttons to persist or discard changes

3. **Storage:**
   - Profile sections stored via existing `save_profile_layout` / `get_profile_layout` backend commands (already exist from B2.1 GrapesJS integration)
   - Each section is a JSON object: `{ id, type, title, content, order, visible }`
   - If `save_profile_layout` doesn't accept this format, store in localStorage as a first pass and document the backend change needed in `progress.md`

4. **Styling:**
   - Each section has a card background matching the current theme
   - MySpace/SpaceHey vibe: bordered sections, customizable section titles, personality-driven layout
   - Responsive: single column on narrow widths, 2-column grid on wider screens

**Do NOT use GrapesJS for this.** Build it with React components + CSS Grid. GrapesJS is too heavy for a structured profile editor — it's a full page builder. The profile needs predefined section types that users can configure, not freeform HTML editing.

---

## Phase 3 — Polish (as time allows)

### P4 — Fix Image Scroll Jitter ⚡

**User:** *"Images somewhat jittery when scrolling."*

Likely causes:
1. Layout shifts from lazy-loaded images that don't have placeholder dimensions
2. Blob URL resolution causing re-renders during scroll
3. Missing `will-change: transform` or GPU compositing hints on scrollable containers

**Investigation:**
1. Check if images have explicit `width`/`height` attributes or CSS `aspect-ratio` before load
2. Check if `useBlobUrl` triggers re-renders on already-visible items
3. Test with the React DevTools profiler to identify unnecessary re-renders during scroll
4. Consider adding `content-visibility: auto` on off-screen feed items for rendering optimization

---

### P5 — Continue CSS Modularization ⚡

Sprint v5 extracted theme CSS to `src/themes/` and decomposed FeedEntryCard. Continue the work:

1. Extract the largest remaining component style blocks from `App.css` into CSS Modules (`.module.css`) colocated with their components
2. Start with: `SettingsScreen`, `ChatScreen`, `ProfileScreen`, `AuditScreen`
3. Keep `App.css` for: resets, `:root` CSS variables, shared layout (`.app-grid`, sidebar), scrollbar styles
4. This is a **pure visual refactor** — output must be identical before and after

---

## Anti-Patterns (MUST AVOID)
- ❌ Do NOT try 5 approaches to the same problem without understanding why each failed
- ❌ Do NOT modify `src-tauri/src/` files — Backend scope
- ❌ Do NOT spawn multiple `pnpm tauri dev` processes or use `sleep` loops
- ❌ Do NOT make massive multi-feature commits — one feature per commit
- ❌ Do NOT leave terminals open when done

## Definition of Done
### Phase 1 (Critical Bugs — must complete)
- [ ] Source filter correctly shows Facebook posts as Facebook (not Bluesky)
- [ ] Returning to "All" source shows full feed (not just user's posts)
- [ ] Links/profiles either load in tabs OR gracefully fallback (system browser for external, native view for Bluesky profiles)
### Phase 2 (Profile Bio — high priority)
- [ ] Profile has default editable sections (About Me, Connections, Photos, Links)
- [ ] Edit mode toggles sections into editable cards with save/cancel
- [ ] MySpace/SpaceHey aesthetic with theme-aware section cards
### Phase 3 (Polish — as time allows)
- [ ] Image scroll jitter investigated and mitigated
- [ ] At least 2 more component style blocks extracted from App.css
### Build verification
- [ ] `cargo check` — 0 errors
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `pnpm test -- --run` — all pass
- [ ] All terminals closed
- [ ] Committed to `main` with semantic messages
