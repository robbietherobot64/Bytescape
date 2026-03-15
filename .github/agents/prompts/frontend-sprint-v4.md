# Frontend Agent — Sprint v4 (2026-03-15)

## ⚠️ PERFORMANCE EXPECTATIONS — READ CAREFULLY

You are starting with a fresh context. The previous Frontend agent session produced **20 commits touching 74 files** but failed to deliver several key items, burned 5 attempts on browser tabs without researching first, and lost a simple config change (`maximized: true`) in the churn. That wastes the user's time.

**You will not repeat that pattern. Here are your rules:**

### 1. Research Before You Code
Before implementing ANY feature that involves Tauri APIs, native platform behavior, or unfamiliar libraries:
- **Read the documentation first.** Use `fetch_webpage` on the official docs.
- **Read the existing code** to understand what's already been tried.
- **Write your approach in a comment** in `progress.md` BEFORE coding.
- If your approach fails, **stop and investigate the error message** — do not try a completely different approach without understanding WHY it failed.

### 2. One Feature Per Commit, Verified
- Each commit addresses **one** item from the priority list.
- Before committing: `cargo check` + `npx tsc --noEmit` + `pnpm test -- --run` — all must pass.
- If you cannot get a feature working after **two genuine attempts** (not two random guesses — two investigated, understood attempts), document what you tried and why it failed in `progress.md`, then **move on to the next priority**. Do not burn 5 commits on one feature.

### 3. Do Not Thrash Files
- `App.css` is 6,477 lines. **Do not rewrite large sections.** Make targeted edits only.
- If you touch a file, check `git log --oneline -5 -- <filepath>` first.
- Check `git diff --stat` before every commit to verify you're only changing what you intend.

### 4. Terminal Discipline
- **Do not spawn `pnpm tauri dev` more than once.** Kill it before starting a new one.
- **Do not use `sleep` loops.** Run build commands directly.
- **Do not spawn background processes and forget them.**
- Reuse existing terminals. The previous session left **170+ zombie terminals** and crashed the environment.

### 5. Track Boundaries
Your scope is `src/` (React/TypeScript), `src/App.css`, and `tauri.conf.json` (UI config only).
- **Do NOT create or modify files in `src-tauri/src/`** (Rust backend). That is Track A.
- **Do NOT modify `src/components/ComposeBox*`** — that is Track C.
- If you need a Rust command that doesn't exist, document the need in `progress.md` and move on.

---

## 🚨 READ FIRST — Codebase State (as of commit `09ab5a4`)

Read `.github/instructions/agent-guardrails.instructions.md` before doing anything.
Read `.github/agents/prompts/user-feedback` for the user's actual words.

### What Already Works (DO NOT BREAK THESE)
- **4-tab feed** (All, Following, Subscribed, Saved) — `useUnifiedFeed.ts` handles all 4 properly
- **Multi-tab system** — NavStore in `store/index.ts`, TabBar component, instant switching
- **3-terminal AuditScreen** — Import/Memories/Trackers layout with themed styling
- **Connected accounts** — Green ✓ indicator + "Connect Account" button in SettingsScreen
- **Media lens toggles** — All/Images/Video/Short/Long/Articles on FeedScreen
- **ThemeCustomizer** — Per-color picker, background presets, upload, tile/cover (Content agent built this)
- **4 themes** — geocities (default), win98, midnight, hacker — all in App.css with full variable sets
- **250 tests passing**, TypeScript clean, Rust clean

### What's Broken or Missing (These are your priorities)
- **`maximized: true`** is NOT in `tauri.conf.json` — user explicitly wants fullscreen on launch
- **In-app browser tabs** — `commands/browser.rs` exists (4 Rust commands) + `WebViewScreen.tsx` exists, but URLs don't load at runtime. Likely cause: `get_window("main")` returns `None` because the window label in `tauri.conf.json` doesn't match
- **Profile section in feed** — no profile card embedded in the feed layout
- **Social module** — weakest module per user, needs "managing/discovering friends" focus
- **Export button** — calls `alert('coming soon')` instead of the actual `export_vault_data` backend command (which now exists)

---

## Sprint Priorities

### P1 — Fix Fullscreen Launch (5 minutes) ⚡⚡⚡

**User:** *"Not opening in full screen"*

This is a one-line fix that was lost during the previous sprint's churn.

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

---

### P2 — Debug In-App Browser Tabs (Investigation, max 2 attempts) ⚡⚡

**User:** *"We are building a browser"*

The Rust backend (`commands/browser.rs`) has 4 commands: `create_browser_view`, `destroy_browser_view`, `resize_browser_view`, `hide_browser_view`. The frontend (`WebViewScreen.tsx`) invokes them. It compiles and tests pass, but URLs don't load at runtime.

**Investigation steps (in order):**
1. Check the window label: `tauri.conf.json` has `"title": "bytescape"` — but what is the **label**? Tauri v2 uses label, not title, for `get_window()`. If no label is specified, it defaults to `"main"`. Print the actual window labels at runtime.
2. In `browser.rs`, look at the `create_browser_view` function. Does it call `app.get_window("main")` or `app.get_webview_window("main")`? In Tauri v2, the correct API might be `get_webview_window`.
3. Add `eprintln!` logging to `create_browser_view` to trace exactly where it fails.
4. Check if the Tauri v2 `unstable` feature is enabled in `Cargo.toml` — multi-webview requires it.

**If you cannot solve this in 2 genuine investigated attempts:** Document exactly what you tried, what the error was, and what you think the fix is — then move on. Do not rewrite WebViewScreen from scratch again.

---

### P3 — Wire Export Button to Backend Command ⚡⚡

**User:** *"Export button added, but no functionality"*

The backend command `export_vault_data` exists in `commands/export.rs` and is registered. The frontend button in SettingsScreen just shows an alert.

**Fix:**
1. Replace the alert with an actual `invoke('export_vault_data', { ... })` call
2. Use `@tauri-apps/plugin-dialog` save dialog to let the user pick an export location
3. Show progress/completion feedback
4. Handle errors gracefully

---

### P4 — Social Module Redesign ⚡

**User:** *"The social module is the weakest and makes the least sense. We need it to be more focused on managing and discovering friends/content. This should be about followers on different sites, webrings, etc."*

**Current state:** FriendsScreen exists but is a basic list. The sidebar has Social icons.

**Fix (first pass — keep it scoped):**
1. Redesign FriendsScreen to show a unified cross-platform view:
   - Section: "Your Connections" — followers/following from all linked platforms, grouped by platform with badge icons
   - Section: "Discover" — placeholder for future content/people discovery
2. Each connection card shows: avatar, display name, handle, platform badge, last activity
3. Click → opens their profile (via in-app browser if working, or system browser fallback)
4. Search/filter bar at the top

---

### P5 — P2P Connections Cleanup ⚡

**User:** *"Is P2P Connections under the Settings tab even relevant still?"*

**Investigation:** Check what the P2P Connections section in Settings actually does. If it's redundant with the peer connection that already works via node ID, simplify or remove it. If it has unique functionality, clarify its purpose with better UX copy.

---

---

## Anti-Patterns (MUST AVOID)
- ❌ Do NOT try 5 different approaches to the same problem without understanding why each one failed
- ❌ Do NOT modify `src-tauri/src/` files — Backend scope
- ❌ Do NOT modify `src/components/ComposeBox*` — Content scope
- ❌ Do NOT spawn multiple `pnpm tauri dev` processes
- ❌ Do NOT use `sleep` loops for build waiting
- ❌ Do NOT make massive multi-feature commits — one feature per commit
- ❌ Do NOT rewrite App.css — make targeted edits only

## Definition of Done
- [ ] App opens maximized (`tauri.conf.json` has `"maximized": true`)
- [ ] Browser tabs investigated — either working or documented failure with diagnosis
- [ ] Export button calls `export_vault_data` with file dialog
- [ ] Social/FriendsScreen shows unified cross-platform connections
- [ ] P2P Connections section evaluated and simplified
- [ ] `cargo check` — 0 errors
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `pnpm test -- --run` — all pass
- [ ] Committed to `main` with semantic messages
