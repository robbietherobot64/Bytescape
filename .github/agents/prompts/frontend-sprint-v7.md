# Frontend Agent — Sprint v7 (2026-03-19)

## ⚠️ OPERATIONAL DISCIPLINE — READ CAREFULLY

Read `.github/instructions/agent-guardrails.instructions.md` before doing anything.
Read `.github/agents/prompts/user-feedback` — **NOTE: this file is stale/outdated.** Rely on this prompt for current priorities.
Read `.github/agents/prompts/systems-advisory-2026-03-15.md` for the systems architect's recommendations.

### Organizational Context: Two-Agent Model

There are only **two agents**: you (Frontend/Track B — all of `src/`) and the Backend agent (Track A — `src-tauri/src/`).

### Performance Standards
- **Research before coding.** Read the existing code before changing it.
- **One feature per commit.** Each commit addresses one priority item.
- **Verify before committing:** `npx tsc --noEmit` + `pnpm test -- --run` — all must pass.
- **Two-attempt cap:** If stuck after two genuine attempts, document what you tried in `progress.md` and move on.
- **Terminal discipline:** Kill background processes when done. Close every terminal you open.
- **Do NOT modify files in `src-tauri/`** — that is Backend scope.

---

## 🚨 Codebase State (as of commit `a1e5be7`)

### What was fixed by the PM before this sprint
1. **`USE_SYSTEM_BROWSER = true`** — External URLs now reliably open in the system browser. The in-app WebView (`add_child()` on WebKitGTK) has been broken for 3 sprints. Do NOT flip this back to `false`.
2. **FriendsScreen chat button restructured** — The `.friend-card` no longer has a competing `onClick`. Chat (💬) and Profile (👤) are now separate buttons. Loading state (⏳) shows while `get_or_create_bsky_convo` resolves.
3. **Source filter now passed to backend** — The `all` tab in `useUnifiedFeed.ts` sends `sourceFilter` to `get_timeline_unified`. Client-side filter retained as safety net.

### Sprint v6 results (your previous work)
- ✅ Source filter regression fixed (composite cache keys + client-side filter)
- ✅ Profile bio sections built (MySpace/SpaceHey-inspired, 6 default sections)
- ✅ Image scroll jitter mitigated (aspect-ratio placeholders + GPU compositing)
- ✅ ChatScreen/AuditScreen CSS extracted from App.css
- ⚠️ Chat button partially fixed (now fully fixed by PM — see above)
- ⚠️ WebView browser tabs broken (now working via system browser — see above)

**Build:** tsc ✅ (0 errors), 264/264 tests ✅, cargo ✅ (0 warnings)

---

## Sprint Theme: "Polish, Chat, & Native Profile View"

The feed is functional, source filters work, and external links open reliably in the system browser. This sprint focuses on polishing what exists and building the first native profile view.

---

## Phase 1 — Critical Fixes

### P1 — Verify Chat Button Works End-to-End ⚡⚡⚡ CRITICAL

The PM restructured the FriendsScreen chat button (commit `a1e5be7`), but it needs runtime verification:

1. Open the Friends tab → Following subview
2. Click the 💬 button on a contact → verify:
   - ⏳ spinner shows immediately
   - After 1-5 seconds, the Chat tab opens with the conversation selected
   - If it's a new convo, the "New conversation with @handle" placeholder shows
3. Click the 👤 button → verify: system browser opens with the Bluesky profile
4. **If the chat button still doesn't work**, investigate and fix. Add `console.log` breadcrumbs in the `handleChat` function and the `onOpenChat` callback in App.tsx to trace exactly where the failure occurs.

### P2 — Chat Button on ChatScreen Buddy Panel ⚡⚡

The ChatScreen right-panel buddy list also has a chat button. Verify it works:
- ChatScreen.tsx line ~598: The buddy panel button calls `invoke('get_or_create_bsky_convo', { memberDid: buddy.did })`
- It also needs a loading state (currently has none — user sees no feedback during the async call)
- Add similar loading state as FriendsScreen: `chattingWith` state, ⏳ during invoke, disable button while loading

### P3 — Source Filter Visual Feedback ⚡⚡

The source filter dropdown was broken in v5 and patched in v6. Verify:
1. Open Feed → All tab → Sources dropdown → select "Facebook" → only Facebook items should appear
2. Switch back to "All" → full feed should restore (not just your own posts)
3. If items are still mislabeled (Facebook showing as Bluesky), the issue is in the backend source field — document in `progress.md` for Track A

---

## Phase 2 — Feature Work

### P4 — Native Bluesky Profile Screen ⚡⚡ HIGH VALUE

Instead of trying to load `bsky.app/profile/{handle}` in a broken WebView, build a **native React profile screen** that fetches data via the Bluesky API.

**Implementation:**
1. Create `src/screens/BlueskyProfileScreen.tsx`
2. Accept props: `{ handle: string }`
3. Fetch profile via `invoke('resolve_profile', { handle })` — this command should already exist (check `lib.rs`)
4. Display: avatar, display name, handle, bio, follower/following counts
5. Fetch their recent posts via `invoke('get_author_feed', { actor: did, limit: 20 })`
6. Display posts using existing `FeedEntryCard` component
7. Add "Chat" button if the user follows them

**Integration:**
- Update `openBlueskyProfile()` in `openInApp.ts` to open a `bsky-profile` tab instead of system browser
- Add `'bsky-profile'` to the `Screen` type in `store/index.ts`
- Register in `App.tsx` `renderTabScreen()`

**Design:** Use the existing profile layout patterns from `ProfileScreen.tsx` — hero banner, avatar, bio section. Keep it simple.

### P5 — Continue CSS Modularization ⚡

App.css is still ~5,700 lines. Extract more screen-specific CSS:
- `FeedScreen.css` (feed entries, filters, tabs)
- `FriendsScreen.css` (friend cards, friend list grid)
- `ProfileScreen.css` (hero banner, bio sections)

Target: reduce App.css by ~500 lines minimum.

---

## Phase 3 — Polish (as time allows)

### P6 — Profile Bio Sections Polish ⚡

Sprint v6 built the profile bio sections. Polish:
1. Verify all 6 default sections render correctly
2. Test edit mode (drag, resize, delete, color picker)
3. Verify persistence via `save_profile_sections` / `get_profile_sections` commands
4. Fix any visual glitches

### P7 — Loading States Audit ⚡

Many async operations have no visual feedback. Audit and add loading states:
- Feed tab switches (show skeleton/spinner while fetching)
- Profile loading
- Chat message sending (already has `isSending` state — verify it works)

---

## Anti-Patterns (MUST AVOID)
- ❌ Do NOT set `USE_SYSTEM_BROWSER = false` — the in-app WebView is broken on Linux
- ❌ Do NOT modify files in `src-tauri/` — Backend scope
- ❌ Do NOT spawn multiple build processes or leave zombie terminals
- ❌ Do NOT make multi-feature commits

## Definition of Done
### Phase 1 (Critical — must complete)
- [ ] Chat button on FriendsScreen works reliably (100% click rate with loading state)
- [ ] Chat button on ChatScreen buddy panel has loading state
- [ ] Source filter verified working end-to-end
### Phase 2 (Features — high priority)
- [ ] Native BlueskyProfileScreen exists and renders profile + posts
- [ ] `openBlueskyProfile()` opens native profile tab instead of system browser
- [ ] App.css reduced by 500+ lines via CSS extraction
### Phase 3 (Polish — as time allows)
- [ ] Profile bio sections verified and polished
- [ ] Loading states added to key async operations
### Build verification
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `pnpm test -- --run` — all pass
- [ ] All terminals closed
- [ ] Committed to `main` with semantic messages
