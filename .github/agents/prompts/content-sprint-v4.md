# Content Agent — Sprint v4 (2026-03-15)

## ⚠️ OPERATIONAL DISCIPLINE — READ CAREFULLY

Read `.github/instructions/agent-guardrails.instructions.md` before doing anything.
Read `.github/agents/prompts/user-feedback` for the user's actual words.

### Performance Standards
- **Research before coding.** If a feature involves an unfamiliar API or library, read the documentation first.
- **One feature per commit.** Each commit addresses one priority item.
- **Verify before committing:** `cargo check` + `npx tsc --noEmit` + `pnpm test -- --run` — all must pass.
- **If stuck after two genuine attempts:** Document what you tried and what failed in `progress.md`, then move on.
- **Terminal discipline:** Do not spawn multiple `pnpm tauri dev` processes. Kill background processes when done. Do not use `sleep` loops.

### Track Boundaries
Your scope: `src/components/ComposeBox*`, `src/components/lenses/*`, `src/components/ThemeCustomizer.tsx`, `src/components/ThemePicker.tsx`, content-related CSS, and content-related models in `src-tauri/src/models/`.
- **Do NOT modify** FeedScreen, ProfileScreen, SettingsScreen, AuditScreen, FriendsScreen — Frontend scope.
- **Do NOT modify** Rust connectors, DB layer, or lib.rs command registration — Backend scope.
- If you need a change outside your scope, document it in `progress.md`.

### Current Codebase State (as of commit `09ab5a4`)

**What you delivered last sprint (verified, all working):**
- `ThemeCustomizer.tsx` — 658 lines, per-color picker for 21 CSS variables across 4 groups ✅
- 7 background presets (Aurora, Sunset, Ocean, Warm Mesh, Matrix, Retro Grid, None) ✅
- File upload with 4MB limit + custom URL input ✅
- Tile/Cover mode toggle ✅
- 20 tests passing (ThemeCustomizer.test.tsx) ✅
- Integration: SettingsScreen → ThemePicker → ThemeCustomizer ✅

**Themes in App.css:** `geocities` (default), `win98`, `midnight`, `hacker` — 4 total.

**Build:** cargo ✅ (27 warnings, 0 errors), tsc ✅, 250/250 tests ✅

---

## Sprint Priorities

### P1 — Drag-and-Drop Media onto ComposeBox ⚡⚡⚡

**User context:** The ComposeBox (508 lines) currently supports media attachment via a file picker dialog (`@tauri-apps/plugin-dialog`). Users should also be able to drag files directly onto the compose area.

**Implementation:**
1. Add `onDragOver`, `onDragEnter`, `onDragLeave`, `onDrop` handlers to the ComposeBox container
2. On drop: read the dropped files, validate they're images/video (`png, jpg, jpeg, webp, gif, mp4, mov`)
3. Process them through the same attachment pipeline as the existing file picker
4. Show a visual drop zone indicator (border highlight, "Drop media here" overlay) during drag
5. Handle multiple files in a single drop

**Tests to add:**
- Renders drop zone indicator on drag enter
- Processes dropped image files
- Rejects non-media file types
- Handles multiple files

---

### P2 — Emoji Picker ⚡⚡

**Implementation:**
1. Add an emoji button (😀) to the ComposeBox toolbar
2. On click: show a searchable emoji picker panel
3. Use a lightweight emoji dataset — no external API needed. Consider `emoji-mart` or a simple built-in grid of common emojis organized by category
4. On emoji select: insert at cursor position in the compose text area
5. Check `package.json` first for any existing emoji dependencies before adding new ones

**Keep it simple:** A well-organized grid of common emojis with search is sufficient. No need for a massive emoji library.

**Tests to add:**
- Emoji button renders in toolbar
- Picker opens on click
- Emoji inserts into text area

---

### P3 — GIF Picker (Tenor API) ⚡⚡

**Implementation:**
1. Add a GIF button to the ComposeBox toolbar
2. On click: show a GIF search panel
3. Use Tenor API (free tier, no auth required for basic search): `https://tenor.googleapis.com/v2/search?q={query}&key={API_KEY}&limit=20`
4. Show GIF thumbnails in a grid
5. On select: attach the GIF URL as media to the post
6. You'll need a Tenor API key — check if one exists in the codebase, if not, use the free/anonymous endpoint or document that the user needs to configure one

**Important:** Check if Tenor's free tier actually requires an API key or allows anonymous access. Read their docs first.

**Tests to add:**
- GIF button renders in toolbar
- Search input works
- GIF selection attaches to post

---

### P4 — Theme Preset Gaps ⚡

**PM audit finding:** The previous sprint report mentioned "sunset orange" and "rose gold" theme presets, but these **do not exist** in App.css. Only `geocities`, `win98`, `midnight`, and `hacker` data-theme selectors exist.

**Options (pick one):**
1. Add `sunset` and `rose-gold` as new theme presets in App.css with appropriate variable sets, and add them as selectable cards in ThemePicker
2. OR — if the report was conflating background presets with themes, just make sure the ThemePicker accurately represents what's available (no phantom options)

**Either way:** Ensure every theme shown in ThemePicker has a corresponding `[data-theme="..."]` block in App.css.

---

### P5 — ComposeBox Regression Check ⚡

Quick verification pass on existing ComposeBox functionality:
1. Link preview unfurling works (paste a URL, see a card)
2. Rich text formatting works (if implemented)
3. Multi-platform posting targets work (`publish_post_v2`)
4. Image/video attachment works via file picker
5. Run existing ComposeBox tests and ensure they pass

---

## Anti-Patterns (MUST AVOID)
- ❌ Do NOT modify FeedScreen, ProfileScreen, SettingsScreen, AuditScreen — Frontend scope
- ❌ Do NOT modify Rust connectors or DB layer — Backend scope
- ❌ Do NOT create test files for TypeScript types that don't exist in the codebase
- ❌ Do NOT spawn multiple build processes or use `sleep` loops
- ❌ Do NOT make multi-feature commits
- ❌ Do NOT add npm dependencies without checking `package.json` for existing alternatives first

## Definition of Done
- [ ] Drag-and-drop media works on ComposeBox
- [ ] Emoji picker available in compose toolbar
- [ ] GIF picker researched and implemented (or documented if API key is required)
- [ ] Theme presets in ThemePicker match actual App.css themes
- [ ] ComposeBox regression check passed
- [ ] `cargo check` — 0 errors
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `pnpm test -- --run` — all pass
- [ ] Committed to `main` with semantic messages
