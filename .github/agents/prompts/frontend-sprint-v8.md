# Frontend Agent — Sprint v8 (2026-03-19) — Identity UI + Persistent Bug Fixes

## ⚠️ OPERATIONAL DISCIPLINE — READ CAREFULLY

Read `.github/instructions/agent-guardrails.instructions.md` before doing anything.
Read `.github/agents/prompts/identity-architecture-audit.md` — **this is the architectural specification for this sprint.**
Read `.github/agents/prompts/user-feedback` — current user feedback.

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

## 🚨 ABSOLUTE RULES — VIOLATIONS WILL BREAK THE APP

### Rule 1: NEVER set `USE_SYSTEM_BROWSER = false`

**File:** `src/utils/openInApp.ts` line 21

The PM has fixed this **TWICE** (commits `a1e5be7` and prior). You set it back to `false` in your sprint v7 commit `80bdadc`. This directly causes the user's #1 complaint: "Links and profiles don't load in new tabs, just open a new tab and it says loading page pretty much forever."

**The in-app WebView (Tauri `add_child()` on WebKitGTK) has been broken for 4+ sprints.** It will NOT start working by flipping a boolean. The system browser is the reliable path.

**Your first commit this sprint MUST set `USE_SYSTEM_BROWSER = true` and you must NOT change it again.**

If you built a `ProfileViewScreen` that uses in-app webview tabs, that screen should use `invoke()` commands to fetch profile data (not load a web URL in a broken WebView). For external URLs (link posts, articles, etc.), they MUST go to the system browser.

### Rule 2: Do NOT add unnecessary patch scripts

Your sprint v7 left `patch.js`, `patch_app.cjs`, and `patch_openProfile.cjs` in the project root. These are workaround scripts that should not exist. Do not create these. If a file needs changing, change it directly.

---

## 🚨 Codebase State

### User Feedback (Critical — read ALL of this)
1. **"It says mastodon:instance_name:hashnumber. What the hell is that?"** — The Identity section shows raw fake Mastodon DIDs. Backend is fixing the data model. You need to fix the display.
2. **"Disconnect button doesn't work"** — SettingsScreen line 192: `onClick={() => alert('Disconnect & remove Bluesky data — coming soon.')}`. Backend has `mastodon_disconnect` and `logout_bluesky` commands. Wire them.
3. **"Links and profiles don't load in new tabs"** — `USE_SYSTEM_BROWSER = false`. Fix it (Rule 1 above).
4. **"Chat button on friendslist contacts still does not work"** — Works on feed, broken on friends list.
5. **"Some Mastodon data not fully loading"** — friends list, profile posts, "You" feed. Partially backend, but verify frontend is calling the right commands.

---

## Sprint Theme: "Identity UI Overhaul + Bug Fixes"

---

## Phase 1 — Fix What's Broken (CRITICAL)

### P1 — Fix `USE_SYSTEM_BROWSER` ⚡⚡⚡ CRITICAL

Open `src/utils/openInApp.ts`. Set line 21:
```typescript
const USE_SYSTEM_BROWSER = true;
```

**Also review the `openProfile` function:** For Bluesky and Mastodon profiles that don't have a native `ProfileViewScreen` implementation, they should fall through to the system browser — not to `openInApp()` which tries to create a broken WebView tab.

**Test:** After this change, clicking a link in a post should open it in Firefox/Chrome (system browser), not create a blank "Loading page..." tab.

### P2 — Fix Chat Button on Friends List ⚡⚡⚡ CRITICAL

The user says the chat button works on the feed screen but NOT on the friends list. Investigate:

1. **Read `FriendsScreen.tsx` `handleChat` function** (line ~579) — it calls `onOpenChat(person.did, ...)` 
2. **Read `App.tsx` `onOpenChat` callback** — what does it do? Does it call `get_or_create_bsky_convo`?
3. **Compare with how feed screen triggers chat** — what's different?
4. **Check if the issue is Mastodon contacts:** The chat button tries to call `get_or_create_bsky_convo` with a `mastodon:instance:id` fake DID — which would fail because Bluesky doesn't know that DID. The fix: detect protocol from the DID format and route accordingly.

**Key insight:** The `person.did` for Mastodon contacts is `mastodon:fosstodon.org:109312` — this gets passed to `get_or_create_bsky_convo` which sends it to the Bluesky API, where it inevitably fails. This is why chat "lags out" — it's timing out on an impossible API call.

**Fix approach:**
```typescript
const handleChat = async (person: ProfileView) => {
    if (!onOpenChat || !person.did || chattingWith) return;
    
    // Don't try to create Bluesky convos for non-Bluesky contacts
    if (person.did.startsWith('mastodon:') || person.did.startsWith('acct:')) {
        // TODO: Wire to send_message with protocol 'mastodon' once AP DM is ready
        alert('Mastodon DMs coming soon!');
        return;
    }
    
    setChattingWith(person.did);
    try {
        await onOpenChat(person.did, person.displayName || person.handle);
    } finally {
        setChattingWith(null);
    }
};
```

### P3 — Fix ChatScreen Lag ⚡⚡

The user says the chat screen "lags out like a mother." Investigate:

1. **Is `import_vault_messages` being called on every tab switch?** If it fetches Bluesky DMs on every render, that's the lag.
2. **Is there an infinite re-render loop?** Check `useEffect` dependencies in ChatScreen.tsx.
3. **Sprint v7 commit `413f09b` says "resolve chat loading loop"** — verify that fix actually works. Read the commit diff.

---

## Phase 2 — Identity UI Overhaul

### P4 — Overhaul Settings Identity Section ⚡⚡⚡ CRITICAL

The Settings screen (`screens/SettingsScreen.tsx`) Identity section needs a complete rebuild.

**Current problems:**
- Shows raw DID string (`mastodon:fosstodon.org:109312`) to users
- Source label only knows `'local'` and `'bluesky'` — no Mastodon case
- Linked Accounts only renders for `source === 'bluesky'`
- Disconnect button is an `alert()` placeholder
- Connect Account only offers Bluesky App Password

**New design:**

```
🪪 Identity
  Signed in as **username**
  Master Identity: did:key:z6Mk... (truncated, with copy button)
  
  Linked Accounts:
  ┌─────────────────────────────────────────┐
  │ 🦋 @user.bsky.social          ✓ Linked │ [Disconnect]
  │    Bluesky · Connected                   │
  ├─────────────────────────────────────────┤
  │ 🐘 @user@fosstodon.org        ✓ Linked │ [Disconnect]
  │    Mastodon · Connected                  │
  ├─────────────────────────────────────────┤
  │ ＋ Connect Account                       │
  │    [🦋 Bluesky] [🐘 Mastodon]           │
  └─────────────────────────────────────────┘
```

**Implementation:**
1. Call `invoke('list_linked_accounts')` on mount to get all linked accounts
2. Render each linked account with protocol icon, display handle, and status
3. Wire "Disconnect" buttons:
   - For Bluesky: `invoke('unlink_account', { protocol: 'bluesky', identifier: account.identifier })`
   - For Mastodon: `invoke('unlink_account', { protocol: 'mastodon', identifier: account.identifier })`
4. "Connect Account" should show protocol options:
   - Bluesky → OAuth flow (existing `handleBlueskyLogin` pattern)
   - Mastodon → invoke `mastodon_register_app` → open auth URL → paste code flow
5. Display the `master_did` (truncated) as the root identity, not platform-specific DIDs

**If `list_linked_accounts` doesn't exist yet** (backend may not have committed it), fall back to:
- Check `invoke('mastodon_get_session')` for Mastodon
- Check `auth.source === 'bluesky'` for Bluesky
- Display what you can with a TODO for the backend command

### P5 — Add `LinkedAccount` TypeScript Interface ⚡⚡

Add to `types/index.ts`:
```typescript
export interface LinkedAccount {
    protocol: string;        // "bluesky" | "mastodon" | "activitypub"
    identifier: string;      // "did:plc:abc" | "acct:user@fosstodon.org"
    display_handle: string;  // "@user.bsky.social" | "@user@fosstodon.org"
    display_name?: string;
    avatar_url?: string;
    connected_at: number;    // Unix timestamp
    active: boolean;
}
```

---

## Phase 3 — Polish & Verification

### P6 — Verify Mastodon Data Loading ⚡

Check that Mastodon data shows up:
1. Friends list → Does `list_follows` include Mastodon following? If the backend returns them, does the frontend render them?
2. Profile wall → Does `get_author_feed` include Mastodon posts?
3. "You" feed → Does `get_timeline_unified` with "you" tab include Mastodon?
4. If data is missing, check if the frontend is filtering by source incorrectly

### P7 — Hashtags: Surface Error Properly ⚡

User reports: "Hashtags don't seem to work, subscribed feed gives 'failed to load feed' error due to 'no access token available'."

1. Check the subscribed feed loading code — is it calling a command that requires auth when no Bluesky session exists?
2. If the user is logged in via Mastodon only (no Bluesky), the subscribed feed should gracefully handle missing Bluesky auth instead of erroring
3. Surface a better error message: "Connect Bluesky to use subscribed feeds" instead of a cryptic token error

### P8 — Clean Up Patch Scripts ⚡

Remove from the project root:
- `patch.js`
- `patch_app.cjs`
- `patch_openProfile.cjs`

These were workaround scripts from sprint v7 that should not exist. Delete them.

---

## Anti-Patterns (MUST AVOID)
- ❌ Do NOT set `USE_SYSTEM_BROWSER = false` — **EVER** — the in-app WebView is broken
- ❌ Do NOT modify files in `src-tauri/` — Backend scope
- ❌ Do NOT create patch scripts or workaround files
- ❌ Do NOT display raw DIDs to users — truncate and add copy buttons
- ❌ Do NOT leave zombie terminals open
- ❌ Do NOT try to send Bluesky API calls with Mastodon identifiers

## Coordination with Backend Agent

The Backend agent is fixing the identity data model this sprint. Some commands may not exist when you start:

| Command | What it does | Your action if missing |
|---------|-------------|----------------------|
| `list_linked_accounts()` | Returns `Vec<LinkedAccount>` | Fall back to checking `mastodon_get_session` + `auth.source` |
| `unlink_account(protocol, identifier)` | Removes linked account + clears session | Show "coming soon" toast instead of `alert()` placeholder |
| `mastodon_disconnect()` | Already exists | Wire it to the Disconnect button for Mastodon |
| `logout_bluesky()` | Already exists | Wire it to the Disconnect button for Bluesky |

## Definition of Done
### Phase 1 (Critical — must complete)
- [ ] `USE_SYSTEM_BROWSER = true` — committed first, never changed
- [ ] Links open in system browser, not broken WebView tabs
- [ ] Chat button on FriendsScreen doesn't try to send Mastodon DIDs to Bluesky API
- [ ] ChatScreen doesn't lag/loop on tab switch
### Phase 2 (Identity UI — must complete)
- [ ] Settings Identity section shows all linked accounts (Bluesky + Mastodon)
- [ ] Disconnect buttons actually work (call backend commands)
- [ ] Connect Account offers both Bluesky and Mastodon options
- [ ] Raw DIDs are never shown to users without truncation
- [ ] `LinkedAccount` TypeScript interface added to types/index.ts
### Phase 3 (Polish — as time allows)
- [ ] Mastodon data verified loading in friends/profile/feed
- [ ] Hashtag error surfaces "connect Bluesky" message
- [ ] Patch scripts removed
### Build verification
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `pnpm test -- --run` — all pass
- [ ] All terminals closed
- [ ] Committed to `main` with semantic messages
