# Frontend Agent — Sprint v7 (2026-03-19) — Protocol-Aware Chat & AP UI

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

## Sprint Theme: "Protocol-Aware Chat & ActivityPub UI"

### Strategic Context

The user has decided to pivot toward ActivityPub (Mastodon/fediverse) integration rather than polishing Bluesky-only features. The Backend agent is building the Mastodon OAuth client, AP DM transport, and a unified `send_message` command this sprint. Your job is to make the frontend **protocol-agnostic** so that when the backend delivers AP messages and Mastodon feeds, the UI is ready.

### What Exists in ChatScreen Today (READ CAREFULLY)

The current ChatScreen (`src/screens/ChatScreen.tsx`) has several Bluesky-hardcoded patterns you need to abstract:

1. **`type ChatTab = 'bluesky' | 'p2p' | 'archives'`** (line 17) — no ActivityPub tab
2. **`const BLUESKY_SOURCES = new Set(['bluesky'])`** (line 20) — used to classify which convos are "live" vs archive
3. **`activeIsLive = activeIsP2p || BLUESKY_SOURCES.has(activeSource) || isNewBlueskyConvo`** (line 168) — determines if the send box is enabled
4. **`invoke('send_bsky_dm', ...)`** (line 275) — directly calls Bluesky-specific send command
5. **Tab badges** (line 461) — hardcoded Bluesky blue badge

The **good news**: The data model is already protocol-agnostic. `VaultMessage` has a generic `source: string` field, and `convo_id`/`sender_did`/`peer_did` work for any protocol. The UI just needs to stop assuming Bluesky is the only live protocol.

---

## Phase 1 — Make Chat Protocol-Agnostic (CRITICAL)

### P1 — Abstract ChatScreen Protocol Constants ⚡⚡⚡ CRITICAL

Refactor the hardcoded protocol assumptions in ChatScreen:

1. **Expand `ChatTab`:**
   ```typescript
   type ChatTab = 'live' | 'p2p' | 'archives';
   // 'live' replaces 'bluesky' — encompasses all live protocol conversations
   ```
   
2. **Replace `BLUESKY_SOURCES` with `LIVE_SOURCES`:**
   ```typescript
   const LIVE_SOURCES = new Set(['bluesky', 'activitypub', 'mastodon']);
   ```
   
3. **Update `activeIsLive`:**
   ```typescript
   const activeIsLive = activeIsP2p || LIVE_SOURCES.has(activeSource) || isNewConvo;
   ```
   Where `isNewConvo` replaces `isNewBlueskyConvo` and supports creating convos for any protocol.

4. **Update tab labels and icons:**
   - 'live' tab → label "Messages" (not "Bluesky"), icon 💬
   - Tab badges should show the protocol source (Bluesky blue badge for Bluesky convos, 🌐 green badge for ActivityPub convos) — but on a per-conversation basis, not per-tab
   
5. **Update the convo list classification:**
   - Line ~139: `return BLUESKY_SOURCES.has(src)` → `return LIVE_SOURCES.has(src)`
   - Conversations from `activitypub` or `mastodon` source should appear in the 'live' tab alongside Bluesky convos

### P2 — Use Unified `send_message` Command ⚡⚡⚡ CRITICAL

The Backend agent is building a unified `send_message(protocol, convo_id, text)` command. Wire the ChatScreen to use it:

1. **Replace the direct `send_bsky_dm` call** (line ~275) with:
   ```typescript
   await invoke('send_message', {
     protocol: activeSource,   // 'bluesky', 'activitypub', 'p2p'
     convoId: activeConvo,
     text: messageText,
   });
   ```

2. **Fallback:** If `send_message` doesn't exist yet (backend agent may not have committed it yet), keep `send_bsky_dm` as a fallback:
   ```typescript
   try {
     await invoke('send_message', { protocol: activeSource, convoId: activeConvo, text: messageText });
   } catch {
     // Fallback for Bluesky until unified command is deployed
     if (activeSource === 'bluesky') {
       await invoke('send_bsky_dm', { convoId: activeConvo, text: messageText });
     }
   }
   ```

3. **P2P messages:** The existing P2P send path should continue to work as-is. Just make sure the protocol detection includes it.

### P3 — Chat Button on FriendsScreen: Protocol-Aware ⚡⚡

The PM already restructured the FriendsScreen chat button to work with Bluesky. Extend it for ActivityPub:

1. **Detect protocol from contact data:** If the contact has a `did:plc:` or `did:web:` → Bluesky path (existing `get_or_create_bsky_convo`). If the contact has an `@user@instance.social` handle → AP path.
2. **For AP contacts:** Call the backend's `send_ap_dm` or `send_message` to initiate a conversation (no need for `get_or_create_convo` — AP conversations are implicit, keyed by the remote actor URL).
3. **Loading state:** The ⏳ loading state already exists. Just make sure it works for both paths.
4. **If AP commands don't exist yet:** Add a TODO comment and show a toast/alert "ActivityPub chat coming soon" — don't break the Bluesky path trying to support AP before the backend is ready.

### P4 — Verify Chat Button Works End-to-End ⚡⚡

The PM restructured the FriendsScreen chat button (commit `a1e5be7`), but it needs runtime verification:

1. Open the Friends tab → Following subview
2. Click the 💬 button on a contact → verify:
   - ⏳ spinner shows immediately
   - After 1-5 seconds, the Chat tab opens with the conversation selected
   - If it's a new convo, the "New conversation with @handle" placeholder shows
3. Click the 👤 button → verify: system browser opens with the Bluesky profile
4. **If the chat button still doesn't work**, investigate and fix. Add `console.log` breadcrumbs in the `handleChat` function and the `onOpenChat` callback in App.tsx to trace exactly where the failure occurs.

---

## Phase 2 — ActivityPub UI Integration

### P5 — Mastodon Login Screen ⚡⚡ HIGH VALUE

Create a login flow for Mastodon instances, complementing the existing Bluesky login.

**Implementation:**
1. Create `src/screens/MastodonLoginScreen.tsx` (or add a "Mastodon" tab to the existing login screen)
2. Flow:
   - User enters their Mastodon instance URL (e.g., `mastodon.social`, `fosstodon.org`)
   - Call `invoke('mastodon_register_app', { instance })` to register Bytescape and get the OAuth URL
   - Open the OAuth URL in the system browser (using `openInSystemBrowser()` from `openInApp.ts`)
   - User authorizes in browser, copies the auth code back to the app
   - User pastes auth code → call `invoke('mastodon_complete_auth', { instance, authCode })`
   - On success, navigate to the feed
3. **If the backend commands don't exist yet:** Build the UI shell with placeholder invoke calls and a TODO comment. The backend agent is building these commands this sprint — the UI should be ready to wire up.

**Integration:**
- Add a "Connect Mastodon" button on the Settings screen or alongside the Bluesky login
- Store the connected state in the app's React state (check `invoke('mastodon_get_session')` on startup)

### P6 — Native Profile Screen (Protocol-Aware) ⚡⚡ HIGH VALUE

Instead of loading external profile pages in a broken WebView, build a **native React profile screen** that works for both Bluesky and ActivityPub profiles.

**Implementation:**
1. Create `src/screens/ProfileViewScreen.tsx`
2. Accept props: `{ handle: string, protocol?: 'bluesky' | 'activitypub' }`
3. Fetch profile via `invoke('resolve_profile', { handle, protocol })` — the backend is building this unified command
4. Display: avatar, display name, handle, bio, follower/following counts
5. Show a protocol badge (🦋 Bluesky / 🌐 Fediverse)
6. Fetch their recent posts via `invoke('get_author_feed', { actor: did, limit: 20 })` for Bluesky, or `invoke('mastodon_get_timeline', ...)` for Mastodon (or display "Posts loading..." placeholder if the command isn't ready)
7. Display posts using existing `FeedEntryCard` component
8. Add "Chat" button if the user follows them (uses protocol-aware chat initiation from P3)

**Integration:**
- Update `openBlueskyProfile()` in `openInApp.ts` to open a `profile-view` tab instead of system browser
- Rename the function to `openProfile()` — make it protocol-agnostic
- Add `'profile-view'` to the `Screen` type in `store/index.ts`
- Register in `App.tsx` `renderTabScreen()`

**If `resolve_profile` backend command doesn't exist yet:** Use `invoke('fetch_remote_actor', { actorUrl })` for AP profiles and the existing Bluesky profile data from `list_follows` as a stopgap.

---

## Phase 3 — Polish (as time allows)

### P7 — Source Filter Visual Feedback ⚡

The source filter dropdown was broken in v5 and patched in v6. Verify:
1. Open Feed → All tab → Sources dropdown → select "Facebook" → only Facebook items
2. Switch to "Mastodon" (if any Mastodon items exist after AP integration)
3. Switch back to "All" → full feed should restore

### P8 — Continue CSS Modularization ⚡

App.css is still ~5,700 lines. Extract more screen-specific CSS:
- `FeedScreen.css` (feed entries, filters, tabs)
- `FriendsScreen.css` (friend cards, friend list grid)
- `ProfileScreen.css` (hero banner, bio sections)

Target: reduce App.css by ~500 lines minimum.

### P9 — Chat Loading States Audit ⚡

Audit loading states in ChatScreen:
- Message sending (already has `isSending` — verify it works)
- Conversation loading (show skeleton while `list_vault_messages` fetches)
- Buddy panel chat button (add loading state like FriendsScreen)

---

## Anti-Patterns (MUST AVOID)
- ❌ Do NOT set `USE_SYSTEM_BROWSER = false` — the in-app WebView is broken on Linux
- ❌ Do NOT modify files in `src-tauri/` — Backend scope
- ❌ Do NOT spawn multiple build processes or leave zombie terminals
- ❌ Do NOT make multi-feature commits
- ❌ Do NOT break existing Bluesky chat while adding AP support — existing Bluesky DMs must continue working
- ❌ Do NOT hardcode new protocol names in ways that make adding future protocols hard — use `LIVE_SOURCES` set pattern

## Coordination with Backend Agent

The Backend agent is building these commands **this sprint**. Some may not be merged yet when you start:

| Command | What it does | Your action if missing |
|---------|-------------|----------------------|
| `send_message(protocol, convo_id, text)` | Unified send — dispatches to Bluesky/AP/P2P | Fall back to `send_bsky_dm` for Bluesky |
| `mastodon_register_app(instance)` | Registers Bytescape OAuth app | Build UI shell, leave TODO |
| `mastodon_complete_auth(instance, auth_code)` | Exchanges auth code for token | Build UI shell, leave TODO |
| `mastodon_get_session()` | Check if Mastodon session exists | Return null/skip if not available |
| `resolve_profile(handle, protocol)` | Unified profile lookup | Use existing Bluesky data or `fetch_remote_actor` |

**Strategy:** Build the UI shells and wire up what's available. Don't block on backend commands — use fallbacks and TODOs. The PM will coordinate in the next sprint to connect any remaining wires.

## Definition of Done
### Phase 1 (Critical — must complete)
- [ ] ChatScreen uses `LIVE_SOURCES` instead of `BLUESKY_SOURCES`
- [ ] ChatScreen tab is renamed from 'bluesky' to 'live' (or 'messages')
- [ ] Send message path uses `send_message` command with fallback to `send_bsky_dm`
- [ ] FriendsScreen chat button has protocol detection (Bluesky vs AP based on handle format)
- [ ] Chat button verified working end-to-end on FriendsScreen
### Phase 2 (Features — high priority)
- [ ] Mastodon login UI exists (even if backend commands are placeholder)
- [ ] Native ProfileViewScreen renders profiles with protocol badge
- [ ] `openBlueskyProfile()` renamed to `openProfile()` and routes to native screen
### Phase 3 (Polish — as time allows)
- [ ] Source filter verified working end-to-end
- [ ] App.css reduced by 500+ lines via CSS extraction
- [ ] Chat loading states audited and fixed
### Build verification
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `pnpm test -- --run` — all pass
- [ ] All terminals closed
- [ ] Committed to `main` with semantic messages
