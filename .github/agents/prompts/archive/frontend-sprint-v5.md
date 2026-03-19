# Frontend Agent — Sprint v5 (2026-03-15)

## ⚠️ OPERATIONAL DISCIPLINE — READ CAREFULLY

Read `.github/instructions/agent-guardrails.instructions.md` before doing anything.
Read `.github/agents/prompts/user-feedback` for the user's actual words.
Read `.github/agents/prompts/systems-advisory-2026-03-15.md` for the systems architect's recommendations.
Read `.github/agents/prompts/systems-perf-analysis-2026-03-17.md` for the performance deep-dive (feed loading + video bottlenecks).

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

## Sprint Theme: "Bugfix & Stabilize"

User testing on 2026-03-17 revealed **critical regressions** (Settings screen missing, blank tabs). These must be fixed before any refactoring work. The sprint is now split into two phases: Phase 1 is critical bug fixes, Phase 2 is the originally planned polish/decomposition work.

---

## Phase 1 — Critical Bug Fixes (do these FIRST)

### P1 — Fix Fullscreen Launch (5 minutes) ⚡⚡⚡

**User:** *"Not opening in full screen"* (REVERTED ISSUE — flagged three times now)

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

### P2 — Fix Missing Settings Screen ⚡⚡⚡ CRITICAL REGRESSION

**User:** *"Settings page completely missing, no navigation bar, just totally missing screen."*

This is a **regression** — Settings was working previously. Something broke it.

**Investigation steps:**
1. Check if SettingsScreen is still imported and rendered in `App.tsx` or the router/NavStore
2. Check if a CSS change hid it (`display: none`, `visibility: hidden`, `z-index` issue)
3. Check if the sidebar navigation link to Settings still exists and triggers the correct tab
4. Check the browser console for runtime errors when clicking the Settings icon

**This must be fixed before P3 (export wiring) since export lives in Settings.**

---

### P3 — Fix Blank Tab on Link/Profile Navigation ⚡⚡⚡ CRITICAL

**User:** *"Links and profiles don't load in new tabs, just open a new tab and it's a blank screen."*

When a user clicks a link or profile that should open in a new tab, the tab opens but renders blank.

**Investigation steps:**
1. Check `NavStore.openTab()` — is the correct screen type being set for the new tab?
2. Check if the component for the new tab type is actually rendered (look at the tab content rendering logic in `App.tsx` or wherever tabs map to components)
3. Check if WebViewScreen or ProfileScreen handles the URL/data passed via the tab metadata
4. Test: click a profile in the feed → does the tab open? Does the console show errors?

---

### P4 — Fix Chat Button on FriendsScreen ⚡⚡

**User:** *"Chat button on friendslist contacts does not work. Works fine on the feed screen."*

The chat button handler works on the feed's friends panel but not on the dedicated FriendsScreen.

**Fix:**
1. Compare the chat button onClick handler in FeedScreen's friends panel vs FriendsScreen
2. Likely the FriendsScreen version is missing the `invoke` call or NavStore navigation to open a chat tab
3. Wire it to the same handler pattern that works in the feed

---

## Phase 2 — Performance Hotfixes (after Phase 1, before Polish)

> These come from the Systems architect's performance analysis (`systems-perf-analysis-2026-03-17.md`).
> They are the highest-impact changes for reducing the ~20s feed load and video stutter.

### P6 — Wire `resolve_blob_urls_batch` ⚡⚡⚡ HIGH IMPACT

**Systems Analysis §L3. Single biggest feed render optimization. Backend command already exists.**

**Problem:** Each image in the feed triggers an individual `invoke('get_media_blob')` call via `useBlobUrl`. A feed page with 20 images = 20 sequential IPC round-trips adding 500–2000ms to perceived load time.

**The backend already has `resolve_blob_urls_batch`** — it takes `{ sources: string[] }` and returns a `HashMap<String, String>` of input → resolved URL. It's registered in `lib.rs`. The frontend just never calls it.

**Fix:**
1. After feed data arrives in `useUnifiedFeed.ts`, collect all `blob:hash` references from post embeds (images, thumbnails, posters)
2. Make a single `invoke('resolve_blob_urls_batch', { sources: [...allBlobRefs] })` call
3. Store the result in a Map/object that `useBlobUrl` can read from (check its `resolvedUrlCache`)
4. When `useBlobUrl` finds a hash in this pre-resolved map, return immediately without IPC

**Implementation approach:**
- Add a `prefetchBlobUrls(sources: string[])` utility that calls the batch command and populates `resolvedUrlCache` in `useBlobUrl.ts`
- Call it from `useUnifiedFeed.ts` after `setFeedEntries()` with all blob references extracted from the feed data
- `useBlobUrl`'s existing `resolvedUrlCache` Map is already checked before IPC — just pre-populate it

**Expected improvement:** 20 blob resolutions go from 20 sequential IPC calls (~500–2000ms) → 1 IPC call (~50–100ms). **10–20× speedup on image loading.**

---

### P7 — Parallelize IPC Calls in `useUnifiedFeed.ts` ⚡⚡

**Systems Analysis §L1, §L2. Eliminates 200–500ms of serial wait.**

**Problem:** The hook makes sequential `await invoke()` calls where parallelism is possible.

**Fix for All tab (line ~240):**
```typescript
// CURRENT (serial, redundant):
const feedsData = await invoke('list_saved_feeds');  // IPC #1 — waits 150-300ms
const response = await invoke('get_timeline_unified', { feedUris });  // IPC #2

// OPTION A — Parallel:
const [feedsData, response] = await Promise.all([
  invoke('list_saved_feeds'),
  invoke('get_timeline_unified', { feedUris: [], ... }),
]);
// Note: get_timeline_unified already internally calls list_saved_feeds for its
// own logic, so the feedUris param can be empty — it handles it.

// OPTION B — Eliminate redundant call:
// Just call get_timeline_unified directly. It already fetches saved feeds internally.
// Only call list_saved_feeds separately if you need the GeneratorView[] data for UI.
```

**Fix for Subscribed tab (line ~192):**
```typescript
// CURRENT (3 serial calls):
const feedsData = await invoke('list_saved_feeds');     // IPC #1
const tags = await invoke('list_followed_hashtags');     // IPC #2 — independent!
const [posts, cur] = await invoke('get_unified_feed', { tags, feedUris }); // IPC #3

// FIX (parallelize independent calls):
const [feedsData, tags] = await Promise.all([
  invoke('list_saved_feeds'),
  invoke('list_followed_hashtags'),
]);
// IPC #3 still sequential (needs results from above)
const [posts, cur] = await invoke('get_unified_feed', { tags, feedUris: feedsData.map(f => f.uri) });
```

This reduces the Subscribed tab from a 3-step serial chain to a 2-step chain.

**Expected improvement:** Shaves 200–500ms off initial feed load.

---

### P8 — Progressive Video Playback ⚡⚡

**Systems Analysis §V2. Eliminates "Loading native media" perceived wait.**

**Problem:** `VideoPlayer.tsx` doesn't render the `<video>` element until FFmpeg completes AND returns the file path. Users stare at "⏳ Loading native media..." for 5–20s (or 1–2s after backend fixes stream-copy).

**Fix:**
1. Render the `<video>` element immediately with the `poster` image (already resolved via `useLazyBlobUrl`)
2. Show a small loading indicator overlaid on the poster (not replacing it)
3. When `resolve_hls_video` returns, update `src` — the video becomes playable
4. User sees: poster image → instant swap to playable video (perceived near-zero wait)

**Also: Cancel FFmpeg on scroll-away:**
- In `VideoPlayer`'s `useEffect` cleanup, call `invoke('cancel_video_resolve', { url: src })` if the video hasn't loaded yet
- This prevents multiple concurrent FFmpeg processes from thrashing the CPU
- The backend is adding this command (see backend sprint P1)
- If the command doesn't exist yet, skip cancellation gracefully (try/catch)

---

### P9 — Fix Video Double-Play Bug ⚡⚡

**User:** *"If a video is played in small screen view, the video starts a new video in the frame view, while the small screen view video continues to play."*

*(Renumbered from original P4 — same content)*

The video player creates a second instance when switching from inline to expanded view instead of transferring the existing playback.

**Fix:**
1. When expanding a video to full/frame view, pause or destroy the inline player
2. The expanded player should resume from the same timestamp (`currentTime`)
3. Global rule: only one video plays at a time — when a new video starts, pause all others
4. Look at `VideoPlayer` component and how it's instantiated in `FeedEntryCard` vs expanded view

---

## Phase 3 — Polish & Decomposition (after Phase 2)

### P10 — Wire Export Button to Backend Command ⚡⚡

**User:** *"Export button added, but no functionality"*

The backend command `export_vault_data` exists in `commands/export.rs` (183 lines) and is registered. The frontend button in SettingsScreen just shows an alert.

**Fix:**
1. Replace the alert with `invoke('export_vault_data', { include_media: false })` — the backend returns a ZIP file path
2. Use `@tauri-apps/plugin-dialog` save dialog to let the user pick an export location
3. Show progress/completion feedback (item count, file size)
4. Handle errors gracefully

**Verification:** Settings → Export → produces a valid ZIP with vault JSON data.

*(Systems Advisory: QW-2. Requires P2 Settings fix first.)*

---

### P11 — Remove Dead Dependencies ⚡

**Systems Advisory QW-3:** `wouter` (router library) is installed in `package.json` but never imported anywhere. Navigation is entirely Zustand-driven.

**Fix:** `pnpm remove wouter`

Check for any other unused dependencies. Verify: `pnpm test` still passes, app still launches.

---

### P12 — CSS Modularization ⚡⚡

**Systems Advisory HP-2. Important for developer velocity but deferred behind critical bugs.**

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

---

### P13 — FeedEntryCard Decomposition ⚡⚡

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

## Anti-Patterns (MUST AVOID)
- ❌ Do NOT try 5 approaches to the same problem without understanding why each failed
- ❌ Do NOT modify `src-tauri/src/` files — Backend scope
- ❌ Do NOT spawn multiple `pnpm tauri dev` processes or use `sleep` loops
- ❌ Do NOT make massive multi-feature commits — one feature per commit
- ❌ Do NOT change CSS visual output during modularization — it must be an identical refactor
- ❌ Do NOT leave terminals open when done

## Definition of Done
### Phase 1 (Critical Bugs — must complete)
- [ ] App opens maximized (`tauri.conf.json` has `"maximized": true`)
- [ ] Settings screen visible and navigable
- [ ] Links/profiles open in new tabs without blank screen
- [ ] Chat button works on FriendsScreen
### Phase 2 (Performance Hotfixes — high priority, do after Phase 1)
- [ ] `resolve_blob_urls_batch` wired: feed images load via single batch IPC call
- [ ] IPC calls parallelized in `useUnifiedFeed.ts` (Subscribed: 3→2 step chain, All: deduplicated)
- [ ] VideoPlayer shows poster immediately, swaps to video when ready (progressive playback)
- [ ] Video plays only one instance at a time, expands from where it left off
### Phase 3 (Polish — complete as time allows)
- [ ] Export button calls `export_vault_data` with file dialog
- [ ] Dead dependencies removed (`wouter` + any others)
- [ ] CSS modularized: themes in `src/themes/`, component styles in `.module.css`, thin `global.css`
- [ ] FeedEntryCard decomposed into sub-components with `React.memo`
### Build verification
- [ ] `cargo check` — 0 errors
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `pnpm test -- --run` — all pass
- [ ] All terminals closed
- [ ] Committed to `main` with semantic messages
