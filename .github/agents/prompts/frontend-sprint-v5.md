# Frontend Agent — Sprint v5 (2026-03-15)

## ⚠️ OPERATIONAL DISCIPLINE — READ CAREFULLY

Read `.github/instructions/agent-guardrails.instructions.md` before doing anything.
Read `.github/agents/prompts/user-feedback` for the user's actual words.
Read `.github/agents/prompts/systems-advisory-2026-03-15.md` for the systems architect's recommendations.

### Organizational Change: Content Track Merged Into Frontend

The Content agent (Track C) has been **dissolved**. You now own **all frontend code** including:
- `src/components/ComposeBox*` (formerly Content scope)
- `src/components/ThemeCustomizer.tsx`, `src/components/ThemePicker.tsx`
- `src/components/lenses/*`
- All CSS (including `App.css` and any future CSS modules)
- Everything in `src/`

There are only **two agents** now: you (Frontend/Track B) and the Backend agent (Track A — `src-tauri/src/`).

### Performance Standards
- **Research before coding.** If a feature involves Tauri APIs or unfamiliar libraries, read the docs first. Use `fetch_webpage` on official documentation.
- **One feature per commit.** Each commit addresses one priority item.
- **Verify before committing:** `cargo check` + `npx tsc --noEmit` + `pnpm test -- --run` — all must pass.
- **Two-attempt cap:** If stuck after two genuine investigated attempts, document what you tried in `progress.md` and move on. Do not burn 5 commits on one feature.
- **Terminal discipline:** Do not spawn multiple `pnpm tauri dev` processes. Kill background processes when done. Do not use `sleep` loops. **Close every terminal you open when you are done with it** — do not leave zombie terminals. The PM will kill all remaining terminals during review.
- **Do not thrash files.** Check `git log --oneline -5 -- <filepath>` before editing. Check `git diff --stat` before every commit.

### Track Boundaries
Your scope is `src/` (all React/TypeScript), `src/App.css`, and `tauri.conf.json` (UI config only).
- **Do NOT create or modify files in `src-tauri/src/`** (Rust backend). That is Track A.
- If you need a Rust command that doesn't exist, document the need in `progress.md` and move on.

---

## 🚨 Codebase State (as of commit `ab8c00c`)

### What Already Works (DO NOT BREAK THESE)
- **4-tab feed** (All, Following, Subscribed, Saved) — `useUnifiedFeed.ts` handles all 4 properly
- **Multi-tab system** — NavStore in `store/index.ts`, TabBar component, instant switching
- **3-terminal AuditScreen** — Import/Memories/Trackers layout with themed styling
- **Connected accounts** — Green ✓ indicator + "Connect Account" button in SettingsScreen
- **Media lens toggles** — All/Images/Video/Short/Long/Articles on FeedScreen
- **ThemeCustomizer** — Per-color picker, 7 background presets, upload, tile/cover mode (658 lines, 20 tests)
- **4 themes** — geocities (default), win98, midnight, hacker — all in App.css with full variable sets
- **ComposeBox** — 508 lines, media attachment via file picker, multi-platform posting
- **250 tests passing**, TypeScript clean, Rust clean

---

## Sprint Theme: "Polish What Exists"

The systems architect recommends stabilizing and decomposing before new features. This sprint focuses on quick wins, CSS modularization, and component decomposition — all directly addressing user complaints and merge conflict debt.

---

## Sprint Priorities

### P1 — Fix Fullscreen Launch (5 minutes) ⚡⚡⚡

**User:** *"Not opening in full screen"* (REVERTED ISSUE — flagged twice)

This is a one-line fix that was added once (`4989f10`) then lost during commit churn.

**Fix:** Add `"maximized": true` to the window config in `tauri.conf.json`:
```json
{
  "title": "bytescape",
  "width": 1280,
  "height": 800,
  "maximizable": true,
  "maximized": true
}
```

**Verification:** Launch the app — it should open maximized.

*(Systems Advisory: QW-1)*

---

### P2 — Wire Export Button to Backend Command ⚡⚡

**User:** *"Export button added, but no functionality"*

The backend command `export_vault_data` exists in `commands/export.rs` (183 lines) and is registered. The frontend button in SettingsScreen just shows an alert.

**Fix:**
1. Replace the alert with `invoke('export_vault_data', { include_media: false })` — the backend returns a ZIP file path
2. Use `@tauri-apps/plugin-dialog` save dialog to let the user pick an export location
3. Show progress/completion feedback (item count, file size)
4. Handle errors gracefully

**Verification:** Settings → Export → produces a valid ZIP with vault JSON data.

*(Systems Advisory: QW-2)*

---

### P3 — Remove Dead Dependencies ⚡

**Systems Advisory QW-3:** `wouter` (router library) is installed in `package.json` but never imported anywhere. Navigation is entirely Zustand-driven.

**Fix:** `pnpm remove wouter`

Check for any other unused dependencies. Verify: `pnpm test` still passes, app still launches.

---

### P4 — CSS Modularization ⚡⚡⚡

**Systems Advisory HP-2. This is the #1 priority for developer velocity.**

`App.css` is **6,478 lines** containing 4 complete theme definitions + all component styles + all responsive breakpoints. This is the primary cause of merge conflicts across agents.

**Approach:**
1. Extract theme definitions into `src/themes/{geocities,win98,midnight,hacker}.css` — one file per theme, containing only the `[data-theme="..."]` block and its CSS variable definitions
2. Extract the largest component style blocks into CSS Modules (`.module.css`) colocated with their components. Start with the biggest: `FeedScreen`, `SettingsScreen`, `ProfileScreen`, `ChatScreen`
3. Keep a thin `src/global.css` for resets, base CSS custom properties (`:root` defaults), scrollbar styles, and shared layout (`.app-grid`, sidebar)
4. Update `main.tsx` to import `global.css` + all theme files. Component files import their own `.module.css`

**Rules:**
- This is a **pure refactor** — visual output must be identical before and after
- Do NOT change any CSS values, selectors, or visual behavior
- Do it in phases: themes first (safest), then component extraction
- Commit after each phase with a verified build

*(This also resolves the "Theme Preset Gaps" issue — if ThemePicker shows themes that don't exist in CSS, you'll find that during extraction)*

---

### P5 — FeedEntryCard Decomposition ⚡⚡

**Systems Advisory HP-3.**

`FeedEntryCard.tsx` is 684 lines handling: media grids, video, link previews, quote posts, engagement bar, audience pills, syndication drawer, threads, and reactions. It's not wrapped in `React.memo`.

**Approach:**
1. Extract sub-components: `MediaGrid`, `EngagementBar`, `SyndicationDrawer`, `ThreadReplies`, `QuotePostEmbed`
2. Wrap `FeedEntryCard` in `React.memo` with a custom comparator (compare by item `id` + `is_liked` + `like_count`)
3. Each sub-component gets its own file in `src/components/feed/`
4. This makes each sub-component independently testable

**Rules:**
- Pure refactor — visual output identical
- No new features in this step
- Commit after extraction with verified build

---

### P6 — Social Module Redesign ⚡

**User:** *"The social module is the weakest and makes the least sense. We need it to be more focused on managing and discovering friends/content. This should be about followers on different sites, webrings, etc."*

**Fix (first pass — keep it scoped):**
1. Redesign FriendsScreen to show a unified cross-platform view:
   - Section: "Your Connections" — followers/following from all linked platforms, grouped by platform with badge icons
   - Section: "Discover" — placeholder for future content/people discovery
2. Each connection card shows: avatar, display name, handle, platform badge, last activity
3. Click → opens their profile (system browser for now)
4. Search/filter bar at the top

**Also:** Evaluate the P2P Connections section in Settings. User asks *"Is P2P Connections under the Settings tab even relevant still?"* — if redundant, simplify or remove.

---

## Anti-Patterns (MUST AVOID)
- ❌ Do NOT try 5 approaches to the same problem without understanding why each failed
- ❌ Do NOT modify `src-tauri/src/` files — Backend scope
- ❌ Do NOT spawn multiple `pnpm tauri dev` processes or use `sleep` loops
- ❌ Do NOT make massive multi-feature commits — one feature per commit
- ❌ Do NOT change CSS visual output during modularization — it must be an identical refactor
- ❌ Do NOT leave terminals open when done

## Definition of Done
- [ ] App opens maximized (`tauri.conf.json` has `"maximized": true`)
- [ ] Export button calls `export_vault_data` with file dialog
- [ ] Dead dependencies removed (`wouter` + any others)
- [ ] CSS modularized: themes in `src/themes/`, component styles in `.module.css`, thin `global.css`
- [ ] FeedEntryCard decomposed into sub-components with `React.memo`
- [ ] Social/FriendsScreen shows unified cross-platform connections
- [ ] P2P Connections section evaluated and simplified
- [ ] `cargo check` — 0 errors
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `pnpm test -- --run` — all pass
- [ ] All terminals closed
- [ ] Committed to `main` with semantic messages
