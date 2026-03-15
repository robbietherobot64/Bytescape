# Content Agent — Sprint: USER FEEDBACK v3 (2026-03-15)

## 🚨 READ FIRST — Codebase State & Guardrails

Read `.github/instructions/agent-guardrails.instructions.md` before doing anything.

### Current Codebase State (as of commit `eef398e`)

These are facts about the current code. Fix or keep based on **user feedback**, not file state.

- **`commands/classify.rs`**: File exists on disk but is dead code — removed from `commands/mod.rs` and `lib.rs` in PM fix commit. The `classify_feed_entry` and `classify_item` functions in `models/mod.rs` still exist and should be used during ingest.
- **TypeScript types**: `BytescapeType = 'Status' | 'Gallery' | 'Article'` (3 variants). Rust has 5 (includes Link/Video). No `ReclassifyResult` type. No `category` on `TrackerEntry`.
- **Content-type filter chips**: Not present — FeedScreen uses lens toggle buttons (List/Gallery/Reader).
- **`classification.test.ts`**: Deleted in PM fix commit (tested types that don't exist in TS).

### ⚠️ Track Boundaries
Content agent created `classification.test.ts` in Sprint 3 — a test file for TypeScript types that didn't match the actual type definitions. Going forward, ensure any test files you create test **existing** types, not types you plan to add. See guardrails §3 for file ownership boundaries.

All work on **`main`**. Run `git branch --show-current` before every commit.

Read the user's latest feedback at `.github/agents/prompts/user-feedback` before starting.

---

## Priority 1 — Appearance Customization ⚡⚡

**User:** *"Let's make appearance more customizable. You should be able to click on any one of those selected colors in a given theme and change it. You should also be able to import images or gifs for animated and engaging backgrounds and themes."*

### Fix:
1. In the Appearance/Theme settings section, make each theme color swatch clickable
2. On click: show a color picker (native HTML `<input type="color">` is fine) that lets the user override that specific CSS variable
3. Save custom color overrides to localStorage or the vault
4. Add an option to import a background image/gif URL
5. Apply the custom background as `background-image` on the app root
6. Persist customization across sessions

---

## Priority 2 — Appearance: Default Background Options

**User:** *"If we could have some defaults provided, that would be cool."*

### Fix:
- Provide 4-6 default background presets: abstract patterns, gradient meshes, or subtle textures
- These can be CSS gradients (no external images needed) or embedded SVG patterns
- Add a "Background" row in theme settings with thumbnail previews

---

## Priority 3 — ComposeBox Enhancement Support

Review the ComposeBox for any outstanding issues. The user hasn't complained about it recently, so this is lower priority. Check:
1. Link preview unfurling still works
2. Rich text formatting works
3. Multi-platform posting targets work
4. No regressions from the user's manual edits

---

## Anti-Patterns
- Do NOT modify files outside Track C scope (see `agent-guardrails.instructions.md` §3)
- Do NOT modify FeedScreen, ProfileScreen, SettingsScreen, or AuditScreen — Frontend scope
- Do NOT modify Rust connectors or DB layer — Backend scope
- Do NOT create test files for types that don't exist yet
- Check `git log --oneline -5 -- <file>` before editing shared files
- **Always run `git branch --show-current` before committing**
- **Always verify before committing**: `cargo check` + `npx tsc --noEmit` + `pnpm test -- --run`

## Definition of Done
- [ ] Theme colors are individually customizable via color picker
- [ ] Custom colors persist across sessions
- [ ] Background image/gif support works
- [ ] Default background presets available
- [ ] ComposeBox still works correctly (no regressions)
- [ ] `pnpm tauri dev` compiles
- [ ] `pnpm test` passes
- [ ] Committed to `main`
