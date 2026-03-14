# Content Agent — Progress Report

> This file tracks cumulative progress from the Content Agent (Track C). Updated after each work session. Reviewed by PM.

---

## Session: 2026-03-13 — Bug Fixes & UI Unification

### Context
User-reported bugs related to OAuth chat integration and chat UI polish. These were blocking issues that needed resolution before continuing Track C feature work.

### Completed Work

#### 1. OAuth Chat Proxy Header Leak Fix ✅
**Commit:** `c29e549`
**Problem:** `authenticated_chat_get/post` called `session.configure_proxy_header()` which permanently mutated the main OAuth session's proxy header. All subsequent XRPC calls (timeline, author feed, etc.) were routed through the chat proxy endpoint → 404 `XRPCNotSupported`.
**Root Cause:** `configure_proxy_header()` is a persistent mutation on the atrium `OAuthSession`, not a per-request setting.
**Fix:** Used `session.clone_with_proxy()` to create a temporary session clone for chat calls, leaving the main session untouched.
**Files:** `src-tauri/src/oauth.rs`

#### 2. Parallel Chat Import ✅
**Commit:** `ad6af24`
**Problem:** Chat messages never imported. `import_vault_messages` was sequentially chained after `import_bluesky_posts` which processes ~99 posts with media downloads taking several minutes. Chat import never executed in practice.
**Root Cause:** Sequential `await` in `App.tsx` `importBluesky()` function.
**Fix:** Launch `import_vault_messages` as a concurrent Promise alongside `import_bluesky_posts`. Also added `transition:chat.bsky` OAuth scope that was missing from the OAuth client metadata.
**Files:** `src/App.tsx`, `src-tauri/src/oauth.rs`
**Result:** 5 conversations found, all messages fetched successfully.

#### 3. Blank Buddy List Handles Fix ✅
**Commit:** `bf56e20`
**Problem:** 3 blank entries in the chat buddy list — users showed up with no display name.
**Root Cause:** Bluesky `chat.bsky.convo.getMessages` API sender object only contains `did` (no `handle`). The `sender_handle` field was always `""`. The `derivedBuddies` dedup logic created entries from sender_did first (with blank handles), preventing the peer_handle entries (which had data from `listConvos` members) from overwriting them.
**Fix (Backend):** Build a `did→handle` lookup map from the conversation's `members` array (which has full profile data from `listConvos`). When message sender object lacks a handle, fall back to this map.
**Fix (Frontend):** Updated `derivedBuddies` dedup logic to allow entries with real handles to overwrite entries with blank/truncated-DID handles.
**Files:** `src-tauri/src/commands/import.rs`, `src/screens/ChatScreen.tsx`

#### 4. Unified Friends Panel — Feed & Chat ✅
**Commit:** `ae4bf4b`
**Problem:** Chat screen had a minimal "Buddy List" (AIM-style) that didn't match the FeedScreen's "Friends" panel. No action icons on either panel.
**Changes:**
- **ChatScreen:** Replaced the `Win98Box`-wrapped buddy list with a full `friends-panel`-style layout matching FeedScreen's right sidebar. Renamed "Buddy List" → "Friends". Uses same CSS classes (`friends-panel`, `friend-item`, `friend-item-avatar`, etc.), same header gradient, group labels, status dots, and two-line display (name + handle).
- **Both Panels:** Added two action icon buttons next to each friend:
  - 💬 **Message** (`BsChatDotsFill`) — ChatScreen: selects the conversation with that friend. FeedScreen: opens P2P chat.
  - 👤 **Profile** (`BsPersonFill`) — ChatScreen: opens Bluesky profile in browser. FeedScreen: opens peer profile view.
- **CSS:** New `.friend-item-actions` and `.friend-item-action-btn` styles. Removed old `.chat-buddy-*` styles. Updated responsive breakpoint from `.chat-buddy-list` to `.chat-friends-panel`.
**Files:** `src/screens/ChatScreen.tsx`, `src/components/FriendsPanel.tsx`, `src/App.css`

### Bug Fixes Resolved in ROADMAP.md
- ✅ "Bluesky Buddy Names Missing in Chat Sidebar" — removed from Open Issues
- ✅ OAuth GAP-5 (DMs Skipped for OAuth Sessions) — marked resolved
- ✅ OAuth Chat Proxy Header Leak — added to Resolved Issues
- ✅ "You" Feed Tab Empty — added to Resolved Issues
- ✅ Chat Import Never Completes — added to Resolved Issues

### Also Marked Resolved (from prior main branch work)
- ✅ A0-GAP-1 (Persistent OAuth Sessions) — `FileSessionStore` with AES-256-GCM
- ✅ A0-GAP-2 (Fixed Callback Port) — dynamic fallback
- ✅ A0-GAP-3 (No OAuth Tests) — `FileSessionStore` unit tests
- ✅ A0-GAP-4 (Write Operations Not Wired) — `is_oauth_session()` branching
- ✅ A0-GAP-6 (Saved Feeds Empty) — PDS URL routing fix

### Test Status
- **75/75 frontend tests pass** (Vitest)
- TypeScript compiles clean (`npx tsc --noEmit`)
- Rust compiles clean (`cargo check` — only pre-existing warnings)

### Notes for PM
- **Profile viewing for other users**: Currently, clicking the profile icon on a Bluesky friend in the chat panel opens their profile on `bsky.app` in the browser. Once we build B3 (Peer Profile Browser) with sandboxed iframe rendering, we'll have in-app profile viewing for all users — both P2P peers and Bluesky follows.
- **`import_bluesky_follows_as_contacts` is defined but never called.** The contacts table stays empty; buddy lists always use the `derivedBuddies` fallback. Consider wiring this into the boot flow to populate proper contact records. This would give the friends panel richer data (display names, avatars from profile metadata).
- **Friends panel data sources differ by screen**: FeedScreen's `FriendsPanel` loads from `list_peers` + `list_follows` (live API). ChatScreen's friends panel derives from imported message senders. These could be unified in a future pass.

---

## Cumulative Track C Status

### Completed ✅
| ID | Item | Commit/Notes |
|----|------|--------------|
| C1.1 | Rich Text Facet Parsing (Backend) | `richtext.rs` — 23 tests |
| C1.2 | Mention Autocomplete (Backend) | `search_mentions` command |
| C1.3 | Hashtag Autocomplete (Backend) | `search_hashtags_autocomplete` command |
| C3.2 | Long-Form Articles (Tiptap) | `ArticleEditor` + `ArticleReader` — 34 tests |
| Security | CRIT-4/5/6, HIGH-3/5, MED-4/5 | All resolved |

### In Progress 🔄
| ID | Item | Status |
|----|------|--------|
| C1.1b | Facet Rendering (Frontend) | Next priority |
| C1.2b | Mention Autocomplete (Frontend) | Next priority |
| C1.3b | Hashtag Autocomplete (Frontend) | Next priority |

### Not Started ⬜
| ID | Item |
|----|------|
| C1.4 | Character Count & Limits |
| C2.1–C2.4 | Link Cards, GIF Picker, Emoji Picker, Custom Emoji |
| C3.1 | Polls |
| C3.3 | Event Post Type |
| C4.1–C4.5 | Media Pipeline (drag-drop, video, multi-image, external, audio) |
| C5.1–C5.5 | Posting Flows (syndicate UX, multi-dest, background job, scheduled, safety) |
| C6.1–C6.6 | Collaborative Editing (CRDT, Yjs, P2P sync, offline merge, relay, ACL) |

### Bug Fixes (This Session)
| Commit | Description |
|--------|-------------|
| `c29e549` | OAuth chat proxy header leak fix |
| `ad6af24` | Parallel chat import + OAuth chat scope |
| `bf56e20` | Blank buddy handles from convo members |
| `ae4bf4b` | Unified friends panel + action icons |
