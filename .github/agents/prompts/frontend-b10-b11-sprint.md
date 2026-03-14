# Frontend Agent — Sprint Kickoff (2026-03-15)

## PM Sprint Review Summary

### ✅ Accepted Work
All deliverables verified and accepted:
- **Assigned (P0/P1/P2)**: B2.1 GrapesJS, B2.4 Widget System, B2.5 Theme Engine, B3.1 Mobile Responsive — all complete ✅
- **Prompt-adjacent**: B2.2 Raw HTML Override, B2.3 DID Meta Tags, B3.1 Profile Browser, B3.2 Sandboxed Peer View — all complete ✅
- **Self-directed B10.x**: B10.1 URL Scheme, B10.2 Nav History, B10.3 Omnibar, B10.6 Peer Browser Sync — **retroactively approved** ✅

Build verified: 143/143 tests, 0 TSC errors, clean git.

### ⚠️ Process Note — Scope Policy

Your B10.x work was high quality and aligned with the PM's roadmap design. However:

1. **Do NOT add items to ROADMAP.md** — roadmap modifications are PM-only. You added B10.7–B10.10 to the roadmap yourself. These items happened to match what the PM independently designed, but that's coincidental. In future sprints, if you see a gap in the roadmap, note it in your progress report and the PM will add it.

2. **Do NOT self-assign unassigned items** — after completing your prompt priorities, report back via progress.md and await the next prompt. Implementing 4 unassigned items delayed PM review and created merge complexity.

3. **Completing prompt-adjacent items (B2.2, B2.3, B3.1/B3.2) was fine** — these were reasonable continuations of assigned work. The line is: if it's in the ROADMAP and directly extends your current task, proceed. If it's a new section/category (like B10), stop and report.

---

## Priority 0 — Merge `main` Into Your Branch (REQUIRED FIRST)

Your branch `track-b/ui-redesign` is **missing 1 critical commit from `main`**:

```
fe890b4 fix(a0): PM audit remediation — master password, OAuth tests, DM pagination, feed auth, debug cleanup, restore guard
```

This commit modifies `lib.rs`, `oauth.rs`, and `oauth_test.rs` — **all three will conflict** during merge.

### Merge Instructions

```bash
git checkout track-b/ui-redesign
git merge main
```

**Expected conflicts:**
| File | Resolution Strategy |
|------|-------------------|
| `bytescape/src-tauri/src/lib.rs` | **Accept BOTH** — main added `xrpc_chat_get`/`xrpc_chat_post` helpers + `get_author_feed` command + master password generation. Your branch added `profile-browser` screen routing + location hooks. These are in different sections — merge manually. |
| `bytescape/src-tauri/src/oauth.rs` | **Accept theirs (main)** — main has the critical DPoP auth fix, `clone_with_proxy()`, `current_access_token()`. Your branch hasn't modified oauth.rs. |
| `bytescape/src-tauri/tests/oauth_test.rs` | **Accept theirs (main)** — main has 30 new test functions. Your branch hasn't modified this file. |

After resolving:
```bash
git add -A && git commit -m "merge: main into track-b/ui-redesign (resolve lib.rs OAuth conflicts)"
```

Verify: `pnpm tauri dev` builds, `pnpm test` passes.

---

## Priority 1 — B10.4: Reload / Refresh

You were already starting this when interrupted. Complete it now.

### Requirements
- Add a reload button (🔄) to the Omnibar, next to back/forward buttons
- Create a global refresh mechanism — each screen exports a `refresh()` callback or listens for a refresh event
- When the reload button is clicked:
  - **FeedScreen**: Re-fetch timeline from Bluesky + local vault
  - **ChatScreen**: Re-poll conversations
  - **VaultScreen**: Re-query blob list
  - **ProfileScreen**: Re-fetch actor data
  - **ProfileBrowserScreen**: Re-fetch peer data
  - **SettingsScreen**: No-op or re-read settings
- Use a Zustand store action or a custom event (`byte://reload`) that screens subscribe to
- The Omnibar reload button should show a spinning animation while refresh is in progress

### Implementation Approach
Option A: Add `refresh` action to `useLocationStore` — when called, screens react via `useEffect` watching a `refreshCounter` value.
Option B: Use a lightweight event bus (`mitt` or custom `EventTarget`) — screens register refresh handlers on mount.

Choose whichever is simpler. Option A keeps everything in Zustand. Option B is more decoupled.

### Tests
- Reload button renders in Omnibar
- Clicking reload increments refresh counter (or emits event)
- Navigation to a new page resets refresh state

---

## Priority 2 — Navigation Unification (Tech Debt)

The codebase currently has **two navigation systems**:
1. `useNavStore` (original, in `store/index.ts`) — `currentScreen`, `setScreen()`, `navigateTo()`
2. `useLocationStore` (B10.1, in `hooks/useLocation.ts`) — `currentUrl`, `navigate()`, `syncFromNavStore()`

They sync via `syncFromNavStore()` bridge called in `App.tsx`. This works but is **fragile** — if a component calls `useNavStore.setScreen()` directly, the URL won't update until the next sync cycle.

### Task
1. Audit all callers of `useNavStore.setScreen()` and `useNavStore.navigateTo()` — there may be 10-20 call sites across screens and components
2. Create a `useNavigate()` convenience hook that wraps `useLocationStore.navigate()` with the same API as the old nav store
3. Replace all `useNavStore.setScreen()` / `navigateTo()` calls with the new hook
4. Remove the `syncFromNavStore()` bridge from App.tsx
5. Keep `useNavStore.currentScreen` as a **read-only derived value** from `useLocationStore.currentUrl` (for backward compatibility with components that read it)
6. Mark `useNavStore.setScreen()` and `navigateTo()` as deprecated with a console.warn in dev mode

### Definition of Done
- All navigation goes through `useLocationStore` — URL always reflects current page
- No more `syncFromNavStore()` bridge
- `useNavStore.currentScreen` is derived, not independently set
- Tests pass, TSC clean

---

## Priority 3 — B11.1: Network State Feed (If Time Permits)

Low priority. Only start if P1 and P2 are complete.

- `NetworkStateEntry` event type (profile updated, webring joined, import completed, group created)
- Feed injection with distinct visual treatment (different card style from regular posts)
- Define TypeScript interface, add to `types/index.ts`

---

## Branch & Commit Strategy
- Stay on `track-b/ui-redesign`
- Commit after each priority: `merge: ...`, `feat(track-b): B10.4 Reload/Refresh`, `refactor(track-b): unify navigation stores`
- Update `progress.md` at the end of your session

## Anti-Patterns
- Do NOT modify `ComposeBox.tsx` — Track C owns that
- Do NOT modify Rust connector files (`connectors/*.rs`) — Track A owns those
- Do NOT add items to ROADMAP.md — PM scope only
- Do NOT start unassigned items after completing these priorities — report back via progress.md
