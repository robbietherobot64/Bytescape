# Screen Spec: Chat / DMs (v2)

**Status:** Draft
**Date:** 2026-08-12
**Route:** `/chat`
**Satisfies:** FR-19 (Chat and DMs) — 1:1 DMs via Bluesky's `chat.bsky.conversation.*` APIs; contributes to FR-20 (Groups — group chat is a separate screen using `space.bytescape.*` lexicons, but the Chat tab in the bottom nav routes here first), FR-25 (unread message badge on notification area), SM-? (chat engagement metric TBD)

> **FR-19 / FR-20 boundary:** This spec covers **1:1 DMs only** — Bluesky's `chat.bsky.conversation.*` APIs. Group chat (`space.bytescape.*` lexicons, Roomy-style spaces/rooms/threads) is FR-20 and gets its own screen spec. The `/chat` route shows the DM inbox; a future `/chat/groups` or `/spaces` route handles groups. The bottom nav "Chat" tab routes to `/chat` (DMs); group entry points live inside the DM screen as a tab or link, not a separate nav item.

**Sources:**
- v1 implementation: No dedicated chat screen exists in v1. `SocialPanel.tsx` (203 lines) has a CHAT tab stub that renders a fake presence dot list; `MobileNav.tsx` (49 lines) routes "Chat" to an empty placeholder. Neither contains real conversation or messaging logic. Salvage is **LOW**.
- v2 constraints: `wiki/product/prd.md` (FR-19, FR-20), `wiki/architecture/decisions.md` (AD-01b dual PDS, AD-01d chat layers, AD-01c lexicons)

> **Decision 2026-08-12 — chat backend:** Bytescape does NOT host its own message store for 1:1 DMs. All DM traffic routes through Bluesky's chat service (`chat.bsky.convo.*` XRPC endpoints). Bytescape acts as a chat client, like Bluesky's own app. This means: (a) messages are stored by Bluesky's infrastructure, not in the Bytescape Postgres DB; (b) both Bytescape-hosted (`did:web`) and Bluesky-hosted (`did:plc`) users use the same backend — the chat service resolves DIDs regardless of hosting; (c) there are no `chat_messages` or `chat_conversations` tables in the Bytescape database. The AppView Postgres DB is not involved in chat at all.

> **Decision 2026-08-12 — E2EE status:** Bluesky's 1:1 DMs are NOT end-to-end encrypted as of August 2026. Messages are encrypted in transit (TLS) and at rest on Bluesky's servers, but Bluesky's chat service can read them. This is the same trust model as Twitter DMs pre-2023. E2EE is on Bluesky's roadmap but has no ship date. Bytescape inherits this limitation transparently — the spec includes an informational notice, not a workaround.

---

## 1. Overview

The Chat screen is Bytescape's 1:1 DM client — a Bluesky chat client skinned in the Bytescape aesthetic. It does not store, relay, or proxy messages; it talks directly to Bluesky's `chat.bsky.convo.*` XRPC endpoints using the authenticated user's credentials (OAuth token for Bluesky-hosted users, or a service-auth token for Bytescape-hosted users — see 1.1). The screen presents a conversation list and a message view in a split-panel layout on desktop, collapsing to a single-column drill-down on mobile.

The screen serves one audience — **authenticated users** — with no unauthenticated access (DMs are private). There is no owner/visitor split; every viewer of `/chat` sees their own inbox.

### 1.1 Dual PDS implications (AD-01b)

Chat is the one feature where the dual-PDS model **does not split** on the frontend. Bluesky's chat service identifies users by DID, not by PDS host. Both user classes talk to the same `chat.bsky.convo.*` endpoints:

| | Bytescape-hosted (`did:web`) | Bluesky-hosted (`did:plc`) |
|---|---|---|
| Auth for chat API | Service-auth token minted by Bytescape backend (proxy auth — the backend holds the `did:web` signing key and generates a DPoP-bound token for `chat.bsky.*` scopes) | OAuth token with `chat.bsky.*` scopes (already obtained during Bluesky OAuth login — FR-27) |
| XRPC endpoint | `chat.bsky.social` (same as Bluesky) | `chat.bsky.social` |
| Message storage | Bluesky's chat service | Bluesky's chat service |
| Frontend difference | None — the API response shape is identical | None |

The backend mints a service-auth token for Bytescape-hosted users on first chat access and caches it. The frontend receives a short-lived JWT containing the token; all chat API calls include it as a Bearer token. This is invisible to the React code — the `ChatService` abstraction handles token refresh transparently.

### 1.2 Data sources

| Data | Source | Notes |
|---|---|---|
| Conversation list | `chat.bsky.convo.listConvos` XRPC | Returns convos with last message, members, unread count |
| Messages in a conversation | `chat.bsky.convo.getMessages` XRPC | Cursor-based pagination, newest first |
| Send a message | `chat.bsky.convo.sendMessage` XRPC | Returns the sent message object |
| Create a new conversation | `chat.bsky.convo.getConvoForMembers` XRPC | Finds or creates a 1:1 convo for a given DID pair |
| Mark as read | `chat.bsky.convo.updateRead` XRPC | Opts: `chat.bsky.convo.updateAllRead` for mark-all-read |
| Mute a conversation | `chat.bsky.convo.muteConvo` / `unmuteConvo` XRPC | Per-conversation mute |
| Leave a conversation | `chat.bsky.convo.leaveConvo` XRPC | For 1:1 DMs this effectively blocks the thread |
| Member profiles | `appview_profiles` (local AppView) or `app.bsky.actor.getProfile` XRPC | Display names, avatars for the conversation list and message headers |

### 1.3 What changed vs. v1 at a glance

1. **Everything is new.** v1 had no chat implementation — `SocialPanel.tsx` rendered a hardcoded list of fake "online" users with no conversation or messaging logic. The entire screen is built from scratch against Bluesky's chat APIs.
2. **No local message store.** Unlike feed posts (which are indexed in `appview_posts`), messages live entirely on Bluesky's chat service. Bytescape fetches them on demand and may cache a recent window in memory/localStorage for perceived speed — but there is no Postgres table for messages.
3. **Bottom nav integration.** The Chat tab in the bottom nav (MobileNav, feed spec 4.6) routes to `/chat`. v1's Chat tab was a dead link.
4. **Group chat is out of scope.** FR-20 (`space.bytescape.*`) gets its own screen. This spec covers the DM inbox only.

---

## 2. v1 Salvage Assessment

Verdicts: **KEEP** (reuse as-is or with trivial touch-ups), **REFINE** (keep concept and most code, improve), **REWORK** (keep concept, rebuild internals), **DROP** (obsolete; mine for parts), **DEFER** (concept preserved intact, design work postponed).

Summary: **0 KEEP · 1 REFINE · 0 REWORK · 2 DROP · 0 DEFER** — consistent with the LOW salvage verdict. v1 had no chat screen; the only chat-adjacent code is the SocialPanel's CHAT tab, which is fake.

| # | File (lines) | Verdict | One-line rationale |
|---|---|---|---|
| 1 | `SocialPanel.tsx` (203) — CHAT tab | **REFINE** | The FRIENDS/CHAT tab toggle and the "click friend → open DM" concept survive; the CHAT tab body (fake presence dots, hardcoded `online/idle` cycling) is replaced entirely by the real conversation list. The FRIENDS tab stays in the feed spec's social panel; the CHAT tab becomes a link to `/chat` instead of an inline panel |
| 2 | `SocialPanel.tsx` — presence indicators | **DROP** | `statusIndicator()` cycles hardcoded `online/idle` for every user. Same family as the profile's fake visitor counter. No real presence data exists in the AT Protocol chat model |
| 3 | `MobileNav.tsx` (49) — Chat route | **DROP** | Routes "Chat" to a placeholder. Replaced by the real `/chat` route mounting `ChatScreen` |

### 2.1 v1 defects fixed by this spec

1. **Chat tab is non-functional.** The CHAT tab in `SocialPanel.tsx` renders a list of users with fake presence dots but no actual conversation list, no message view, and no compose. Fixed: the CHAT tab becomes a "View all chats" link to `/chat`, or an embedded mini-conversation-list (see 4.2).
2. **No DM route.** `MobileNav`'s "Chat" button goes nowhere. Fixed: `/chat` is a real route mounting `ChatScreen`.
3. **Fake presence indicators.** The `statusIndicator()` function returns `online` or `idle` based on a modulo of the user index. Removed entirely — Bluesky's chat API does not expose presence data. No replacement until the protocol adds it.

---

## 3. Layout

### 3.1 Desktop (>900px) — split panel

```
┌────────────────────────────────────────────────────────────────────────────┐
│ TAB BAR  [📰 Feed] [👤 @you] [＋] [💬 Chat] [☰ More]            [🔔 3]   │
├──────────────────────┬─────────────────────────────────────────────────────┤
│ CONVERSATION LIST    │  CHAT VIEW                                         │
│ 320px, fixed         │  1fr                                               │
│ ┌──────────────────┐ │  ┌───────────────────────────────────────────────┐ │
│ │ 💬 Chats     [⋯] │ │  │ [av] Mika @mika.bsky.social         [⋯] [✕] │ │
│ ├──────────────────┤ │  ├───────────────────────────────────────────────┤ │
│ │ > Search chats.. │ │  │                                               │ │
│ ├──────────────────┤ │  │  ─── Today ───                                │ │
│ │ [av] Mika        │ │  │                                               │ │
│ │  hey, did you    │ │  │  ┌─ yours ──────────────┐                    │ │
│ │  see the new...  │ │  │  │ hey, did you see the │                    │ │
│ │  2m ago       2● │ │  │  │ new terminal theme?  │                    │ │
│ │ ──────────────── │ │  │  │ 2:14 PM              │                    │ │
│ │ [av] @jon        │ │  │  └──────────────────────┘                    │ │
│ │  sounds good!    │ │  │                                               │ │
│ │  1h ago          │ │  │  ┌─ theirs ──────────────┐                   │ │
│ │ ──────────────── │ │  │  │ yeah it's so cool!    │                    │ │
│ │ [av] @skeeter    │ │  │  │ love the green        │                    │ │
│ │  📷 Photo        │ │  │  │ 2:15 PM               │                    │ │
│ │  3h ago          │ │  │  └───────────────────────┘                    │ │
│ │ ──────────────── │ │  │                                               │ │
│ │ [av] @alice      │ │  │  ─── Yesterday ───                            │ │
│ │  see you at the  │ │  │                                               │ │
│ │  event?          │ │  │  ┌─ yours ──────────────┐                    │ │
│ │  1d ago          │ │  │  │ see you at the event │                    │ │
│ └──────────────────┘ │  │  │ tomorrow? 🎉         │                    │ │
│                      │  │  │ Yesterday 4:30 PM    │                    │ │
│                      │  │  └──────────────────────┘                    │ │
│                      │  │                                               │ │
│                      │  ├───────────────────────────────────────────────┤ │
│                      │  │ ┌─────────────────────────────────────────┐   │ │
│                      │  │ │ > Type a message...              [Send] │   │ │
│                      │  │ └─────────────────────────────────────────┘   │ │
│                      │  └───────────────────────────────────────────────┘ │
├──────────────────────┴─────────────────────────────────────────────────────┤
│  grid: 320px · 1fr — each panel scrolls independently                     │
│  conversation list: 1px right border (--border-dim)                       │
└────────────────────────────────────────────────────────────────────────────┘
```

Notes:
- **Left panel** scrolls independently; the active conversation has a 2px left accent border + accent-tinted background (`rgba(0,255,65,0.08)`).
- **Right panel** shows the selected conversation. Messages are grouped by date with date separator headers. Own messages align right with accent-tinted background; their messages align left with `--bg-secondary`.
- **Empty state** (no conversation selected): the right panel shows `ChatEmptyState` — a centered message prompting the user to select a conversation or start a new one.
- **Compose bar** is fixed at the bottom of the right panel. `Enter` sends; `Shift+Enter` inserts a newline (not supported by Bluesky's API — see OQ-1; the composer strips newlines on send).

### 3.2 Tablet (551–900px) — narrower split

Same layout, narrower left panel.

```
┌──────────────────────────────────────────────────────────────┐
│ TAB BAR  [📰 Feed] [👤 @you] [💬 Chat] [☰ More]      [🔔]  │
├────────────────────────┬─────────────────────────────────────┤
│ CONVERSATION LIST      │  CHAT VIEW                          │
│ 260px                  │  1fr                                │
│ ┌────────────────────┐ │  ┌─────────────────────────────────┐│
│ │ 💬 Chats       [⋯] │ │  │ [av] Mika @mika      [⋯] [✕]  ││
│ ├────────────────────┤ │  ├─────────────────────────────────┤│
│ │ > Search..         │ │  │  ─── Today ───                  ││
│ │ [av] Mika      2●  │ │  │  ┌─ yours ──────────────────┐  ││
│ │ [av] @jon          │ │  │  │ hey, did you see the...  │  ││
│ │ [av] @skeeter      │ │  │  └──────────────────────────┘  ││
│ └────────────────────┘ │  │  ┌─ theirs ──────────────────┐  ││
│                        │  │  │ yeah it's so cool!        │  ││
│                        │  │  └───────────────────────────┘  ││
│                        │  ├─────────────────────────────────┤│
│                        │  │ > Type a message...      [Send] ││
│                        │  └─────────────────────────────────┘│
└────────────────────────┴─────────────────────────────────────┘
```

### 3.3 Mobile (≤550px) — fullscreen drill-down

Two views: conversation list (default) and active chat. Tapping a conversation slides to the chat view; the back button returns to the list.

```
┌──────────────────────────┐     ┌──────────────────────────┐
│ TAB BAR  [💬 Chat] [🔔]  │     │ ← Back    Mika    [⋯]   │
├──────────────────────────┤     ├──────────────────────────┤
│ 💬 Chats             [⋯] │     │                          │
├──────────────────────────┤     │  ─── Today ───           │
│ > Search chats...        │     │                          │
├──────────────────────────┤     │  ┌─ yours ─────────────┐ │
│ [av] Mika                │     │  │ hey, did you see    │ │
│  hey, did you see the... │     │  │ the new terminal... │ │
│  2m ago               2● │     │  │ 2:14 PM             │ │
├──────────────────────────┤     │  └─────────────────────┘ │
│ [av] @jon                │     │                          │
│  sounds good!            │     │  ┌─ theirs ─────────────┐ │
│  1h ago                  │     │  │ yeah it's so cool!   │ │
├──────────────────────────┤     │  │ love the green       │ │
│ [av] @skeeter            │     │  │ 2:15 PM              │ │
│  📷 Photo                │     │  └─────────────────────┘ │
│  3h ago                  │     │                          │
├──────────────────────────┤     ├──────────────────────────┤
│ [av] @alice              │     │ > Type a message.. [Send]│
│  see you at the event?   │     └──────────────────────────┘
│  1d ago                  │       chat view
├──────────────────────────┤
│ 📰  👤  ＋  💬  ☰        │     ┌──────────────────────────┐
│ Feed Prof    Chat More   │     │ ← Back    Mika    [⋯]    │
└──────────────────────────┘     ├──────────────────────────┤
  conversation list               │ [keyboard area —         │
                                  │  compose stays visible   │
                                  │  above soft keyboard]    │
                                  └──────────────────────────┘
                                    chat view (keyboard open)
```

Notes:
- **Conversation list** is the default view at `/chat`. Bottom nav stays visible.
- **Chat view** hides the bottom nav and shows a `< Back` button in the header. The compose bar stays anchored above the soft keyboard (CSS `position: sticky; bottom: 0` + `env(safe-area-inset-bottom)`).
- **Back** returns to the conversation list (history push, not a new mount — preserves scroll position in the list).
- The **⋯ menu** on the chat header opens a popover with: View Profile, Mute Conversation, Leave Conversation, Block User.
- **New chat** is initiated from a `+` button in the conversation list header, which opens a user-search modal (see 6.5).

---

## 4. Component Breakdown

### 4.1 Screen shell

**`ChatScreen`**
- Purpose: mount the split-panel layout, manage conversation selection, handle route-level concerns (auth guard, service-auth token refresh).
- Props: none (route component — reads conversation ID from URL query param `?convo=...` on desktop, or from route state on mobile).
- State: `conversations: Convo[]`, `selectedConvoId: string | null`, `loadingConversations: boolean`, `conversationsError: string | null`.
- Behavior:
  - On mount: fetch conversation list via `chat.bsky.convo.listConvos`. Cache in `localStorage.bytescape_chat_convos` for instant first paint (stale-while-revalidate — show cached list, refetch in background).
  - `selectedConvoId` is set by clicking a `ConversationItem` or by URL param. On desktop/tablet, this populates the right panel. On mobile, this triggers the slide-to-chat-view transition.
  - Auth guard: if the user is not logged in, redirect to `/login?redirect=/chat`. If the user lacks chat permissions (no OAuth token with chat scopes and no service-auth), show a one-time "Connect Bluesky to chat" upsell (links to account linking in settings — FR-27).
  - Polling: refetch conversation list every 30 seconds to pick up new messages (same pattern as `NotificationBell`). Pause when the tab is hidden (`document.visibilitychange`). The active conversation's messages are not polled — new messages arrive via a WebSocket or SSE push from the backend (see OQ-2).
- Stack: React 19 + Vite + TypeScript. `useState`/`useEffect`. No new state library. Same patterns as the feed spec.

### 4.2 Left panel — conversation list

**`ConversationList`**
- Purpose: display all 1:1 DM conversations, sorted by most recent message. Search, start new conversations.
- Props: `conversations: Convo[]`, `selectedId: string | null`, `onSelect(id)`, `onNewChat()`, `loading: boolean`, `error: string | null`.
- State (local): `searchQuery: string`, `filteredList: Convo[]`.
- Subcomponents:
  - **Header:** "💬 Chats" title (VT323, uppercase), `[⋯]` overflow menu (Mark all read, Mute all, Settings), `[+]` new-chat button.
  - **SearchBar:** `> Search chats...` placeholder, filters the conversation list client-side by member handle or display name. 300ms debounce. The search is local (over the already-fetched conversation list) — there is no server-side conversation search in Bluesky's chat API.
  - **ConversationItem** (see 4.3).
  - **Empty state** (no conversations): centered message — "No conversations yet. Start a chat with someone you follow." — with a `> START A CHAT` button that opens the new-chat modal.
  - **Loading state:** 4 skeleton rows (avatar circle + two text lines + timestamp).
  - **Error state:** "Could not load conversations" + `> RETRY` button.

**`ConversationItem`**
- Purpose: one row in the conversation list.
- Props: `convo: Convo`, `isActive: boolean`, `onClick()`.
- Displays: member avatar (from `appview_profiles` or the convo member data), member display name + handle (truncated), last message preview (truncated to one line, text-only — media messages show "📷 Photo" or "🎬 Video"), relative timestamp ("2m ago", "1h ago", "Yesterday"), unread badge (count, max 9+).
- Layout: flex row, avatar 40px left, text column center, timestamp + badge right. Active state: 2px left border `--accent`, background `rgba(0,255,65,0.08)`.
- Accessibility: `role="option"`, `aria-selected={isActive}`, `aria-label="{name}: {last message}, {unread} unread"`.

### 4.3 Right panel — chat view

**`ChatView`**
- Purpose: display messages in the selected conversation, handle sending new messages.
- Props: `convoId: string`, `onBack()` (mobile only).
- State: `messages: Message[]`, `cursor: string | null`, `hasMore: boolean`, `loadingMessages: boolean`, `sendingMessage: string`, `sendingState: 'idle' | 'sending' | 'error'`, `typingIndicator: boolean`.
- Behavior:
  - On mount (or `convoId` change): fetch messages via `chat.bsky.convo.getMessages`. Display newest first (API default), then load older on scroll-up.
  - Scroll management: auto-scroll to bottom on mount and when a new own message is sent. When loading older messages (scroll-up), preserve scroll position so the user doesn't lose their place. Show a "Jump to latest" floating button when the user has scrolled up and new messages arrive.
  - Typing indicator: Bluesky's chat API does not expose typing indicators (see OQ-3). The `typingIndicator` state is reserved for future use — the component renders nothing for it today.
- Subcomponents:
  - **ChatHeader:** member avatar, display name + handle, `⋯` overflow menu (View Profile, Mute Conversation, Leave Conversation, Block User). On mobile, includes `< Back` button.
  - **DateSeparator:** horizontal rule with date label ("Today", "Yesterday", "Aug 10") — rendered between message groups on date boundaries.
  - **MessageBubble** (see 4.4).
  - **ChatComposer** (see 4.5).
  - **Loading older:** spinner at the top of the message list when fetching older messages. Sentinel element for intersection observer (load-more on scroll-up).
  - **Empty conversation:** "No messages yet. Say hello!" centered in the message area.

**`MessageBubble`**
- Purpose: render a single message.
- Props: `message: Message`, `isOwn: boolean`, `showAvatar: boolean` (false for consecutive messages from the same sender within 2 minutes).
- Displays: message text, timestamp (short: "2:14 PM"), delivery status icon for own messages (✓ sent, ✓✓ delivered — if the API provides this; see OQ-4).
- Layout:
  - Own messages: right-aligned, accent-tinted background (`rgba(0,255,65,0.12)`), green-tinted border, no avatar.
  - Their messages: left-aligned, `--bg-secondary` background, show avatar on the first message in a consecutive group (within 2 minutes).
  - Max width: 70% of the chat view width (prevents messages from stretching full-width on desktop).
  - Text wrapping: `word-break: break-word`, no horizontal scroll.
- Media: Bluesky's chat API supports embedding images in messages (`app.bsky.embed.images`). If a message contains an image embed, render it as a thumbnail (max 240px) with click-to-expand lightbox. The feed spec's lightbox component is reused here.
- Accessibility: messages have `role="log"` on the container (the `ChatView` message list), with `aria-live="polite"` so screen readers announce new messages as they arrive.

**`ChatComposer`**
- Purpose: text input and send button for composing messages.
- Props: `onSend(text: string)`, `disabled: boolean`, `sendingState: 'idle' | 'sending' | 'error'`.
- State (local): `text: string`, `inputRef`.
- Layout: flex row — text input (expandable textarea, max 4 lines, then scroll) + Send button. `> Type a message...` placeholder. Send button is accent-colored, disabled when `text` is empty or `sendingState === 'sending'`.
- Behavior:
  - `Enter` sends the message (calls `onSend(text)`, clears input). `Shift+Enter` inserts a newline — but Bluesky's chat API does not support multiline messages (see OQ-1), so the composer strips trailing newlines before sending.
  - Sending state: button shows a spinner while sending; on error, shows a toast "Failed to send. Retry?" with the message text preserved in the input.
  - Input focus: the composer auto-focuses when a conversation is selected (on desktop) or when the chat view opens (on mobile).
- Accessibility: `aria-label="Message input"`, `aria-label="Send message"` on the button. The textarea has `role="textbox"` and `aria-multiline="false"` (since newlines are stripped).

**`ChatEmptyState`**
- Purpose: shown in the right panel when no conversation is selected (desktop/tablet only).
- Displays: Bytescape logo or terminal art, "Select a conversation to start chatting", and a `> START A CHAT` button.
- Layout: centered vertically and horizontally in the right panel.

### 4.4 New-chat modal

**`NewChatModal`**
- Purpose: search for a user and start a new 1:1 conversation.
- Triggered by: the `[+]` button in the conversation list header, or the `> START A CHAT` button in the empty states.
- Behavior:
  - Search input: `> Search by handle or name...`. Calls `app.bsky.actor.searchActors` XRPC (or the local AppView search). Results show avatar, display name, handle.
  - Selecting a result calls `chat.bsky.convo.getConvoForMembers` with the selected DID — this either returns an existing conversation or creates a new one. The response includes the `convoId`.
  - The modal closes and the conversation list updates to show the (possibly new) conversation, selected.
  - If the target user has DMs disabled or is not on Bluesky, show an error: "This user can't receive messages."
- Accessibility: focus trap inside the modal, `Escape` closes, `role="dialog"`, `aria-label="Start a new conversation"`.

---

## 5. States

### 5.1 Loading states

| State | Trigger | UI |
|---|---|---|
| Conversation list loading | First mount, no cached data | 4 skeleton rows (shimmer animation: avatar circle + two text bars + timestamp bar) |
| Conversation list loading (cached) | First mount, cached data exists | Show cached list immediately, subtle spinner in the header while refetching. Stale data has a faint opacity (0.7) that resolves to 1.0 when fresh data arrives |
| Message history loading | Conversation selected, first fetch | Skeleton: 3 message bubbles (alternating left/right, shimmer) |
| Loading older messages | Scroll to top of message list | Small spinner at the top, above the oldest loaded message. Scroll position preserved |
| Sending message | User hits Enter/Send | Own message appears immediately with a clock icon (optimistic). On success: icon changes to ✓. On failure: icon changes to ⚠ with a tap-to-retry affordance |

### 5.2 Empty states

| State | Condition | UI |
|---|---|---|
| No conversations | `conversations.length === 0` | Centered: "No conversations yet" + "Start a chat with someone you follow." + `> START A CHAT` button |
| No conversation selected | Desktop/tablet, no `selectedConvoId` | `ChatEmptyState` in right panel (4.4) |
| Empty conversation | Selected convo has no messages | "No messages yet. Say hello!" centered in the message area |
| No search results | Conversation search yields nothing | "No conversations match '{query}'" |

### 5.3 Error states

| State | Trigger | UI |
|---|---|---|
| Conversation list fetch failed | Network error or API error | "Could not load conversations" + error detail (if available) + `> RETRY` button |
| Message fetch failed | Network error loading messages | "Could not load messages" + `> RETRY` button. The conversation list remains visible and functional |
| Send failed | Network error or API rejection on send | Toast: "Failed to send message" + the message text stays in the input. Optimistic bubble shows ⚠ icon |
| Service-auth failure | Bytescape-hosted user's chat token expired or invalid | "Chat connection lost. Reconnecting..." banner at the top of the screen. Automatic retry with exponential backoff (1s, 2s, 4s, max 30s). If it fails 5 times: "Could not connect to chat. Please try again later." + `> RETRY` |
| User not found (new chat) | Search returns no results or selected user can't receive DMs | "This user can't receive messages" or "No users found" |

### 5.4 Edge cases

| Case | Behavior |
|---|---|
| Very long messages | Text wraps with `word-break: break-word`. No horizontal scroll. Max-width 70% of chat view |
| Rapid message sending | Messages queue locally and send sequentially. Optimistic UI shows all immediately. If one fails, only that one shows the ⚠ icon |
| Conversation with self | Bluesky's API may allow self-DMs. The UI renders them normally — no special case |
| Blocked user | If the conversation partner blocks the viewer (or vice versa), `getMessages` returns an error. Show "You can no longer message this user" in the chat area |
| Muted conversation | Muted conversations appear in the list but without notification badges. A small 🔇 icon appears next to the name |
| Very old conversations | Cursor-based pagination handles this. Load-more on scroll-up. No maximum message count |
| Network offline | Show a red banner: "You're offline. Messages will send when you're back online." Queue unsent messages locally (localStorage). Retry on `online` event |
| Concurrent sessions | If the user has Bytescape open in two tabs, both tabs poll the conversation list. Messages sent from one tab appear in the other on the next poll cycle (30s). No real-time sync between tabs — this is acceptable for v2.0 |

---

## 6. Interactions

### 6.1 Open a conversation

1. User clicks/taps a `ConversationItem` in the conversation list.
2. `selectedConvoId` updates. On desktop/tablet, the right panel mounts `ChatView` for that conversation. On mobile, the view slides to the chat view.
3. Messages are fetched via `chat.bsky.convo.getMessages`. The list auto-scrolls to the bottom (newest messages).
4. The conversation's unread count is cleared client-side immediately (optimistic). The backend call `chat.bsky.convo.updateRead` fires in the background.
5. The `ConversationItem`'s unread badge disappears.

### 6.2 Send a message

1. User types in the `ChatComposer` input.
2. User presses `Enter` or clicks `Send`.
3. The message text is trimmed. If empty after trim, the send is a no-op.
4. An optimistic `MessageBubble` appears at the bottom of the message list with a clock icon.
5. `chat.bsky.convo.sendMessage` is called with the text.
6. On success: the optimistic bubble updates with the server-returned message data (CID, timestamp). Icon changes to ✓. The conversation's last-message preview in the list updates.
7. On failure: the optimistic bubble shows ⚠. A toast appears: "Failed to send message." The message text remains in the input for retry.

### 6.3 Keyboard navigation

| Key | Context | Action |
|---|---|---|
| `↑` / `↓` | Conversation list focused | Navigate between conversations |
| `Enter` | Conversation focused | Open conversation |
| `Escape` | Chat view open (mobile) | Return to conversation list |
| `Escape` | Conversation list focused | Clear search input (if search is active) |
| `Enter` | Composer focused | Send message |
| `Shift+Enter` | Composer focused | Insert newline (stripped on send — see OQ-1) |
| `Tab` | Composer focused | Move focus to Send button |
| `Ctrl/Cmd+K` | Anywhere on the screen | Open new-chat modal (shortcut) |

### 6.4 Scroll behavior

- **Auto-scroll to bottom:** on mount, on own-message send, and when a new message arrives and the user is already at (or near) the bottom (within 100px of the scroll container's end). "Near the bottom" prevents auto-scroll from hijacking the user when they're reading older messages.
- **"Jump to latest" button:** appears when the user has scrolled up and new messages arrive. Floating button at the bottom-right of the message area: "↓ New messages". Click scrolls to bottom and dismisses the button.
- **Load older on scroll-up:** intersection observer on a sentinel element at the top of the message list. When visible, fetch the previous page of messages and prepend them. Preserve scroll position by measuring the container's scroll height before and after the prepend, then adjusting `scrollTop`.

### 6.5 Start a new conversation

1. User clicks `[+]` in the conversation list header (or the empty-state button, or presses `Ctrl/Cmd+K`).
2. `NewChatModal` opens with a search input focused.
3. User types a handle or name. Results appear from `app.bsky.actor.searchActors`.
4. User clicks a result.
5. `chat.bsky.convo.getConvoForMembers` is called with the selected DID.
6. The modal closes. The conversation list refreshes. The new (or existing) conversation is selected and the chat view opens.

### 6.6 Conversation overflow menu

The `[⋯]` menu on a `ConversationItem` (long-press on mobile, hover+click on desktop) and the `[⋯]` on the `ChatHeader` offer:

| Action | Effect |
|---|---|
| View Profile | Navigate to `/@{handle}` |
| Mute Conversation | `chat.bsky.convo.muteConvo` — suppresses notifications for this convo. Item shows 🔇 icon |
| Unmute Conversation | `chat.bsky.convo.unmuteConvo` — restores notifications |
| Leave Conversation | `chat.bsky.convo.leaveConvo` — confirmation dialog: "Leave this conversation? You won't receive new messages." For 1:1 DMs, this effectively ends the thread |
| Block User | Confirmation dialog: "Block @{handle}? They won't be able to message you." Calls `app.bsky.graph.block` on the PDS |

### 6.7 Accessibility

- **Message announcements:** the message list container has `aria-live="polite"` so screen readers announce new messages as they arrive.
- **Conversation list:** `role="listbox"`, each `ConversationItem` is `role="option"` with `aria-selected`.
- **Focus management:** when a conversation is opened (desktop), focus moves to the composer. When the back button is pressed (mobile), focus returns to the corresponding `ConversationItem`.
- **prefers-reduced-motion:** the mobile slide transition between conversation list and chat view is disabled (instant swap). Skeleton shimmer animations are replaced with a static dimmed state.

---

## 7. Responsive Behavior

### 7.1 Breakpoints

| Breakpoint | Layout | Conversation list | Chat view |
|---|---|---|---|
| Desktop (>900px) | Split panel: 320px left + 1fr right | Always visible, independent scroll | Always visible (empty state if no selection) |
| Tablet (551–900px) | Split panel: 260px left + 1fr right | Always visible, narrower | Always visible |
| Mobile (≤550px) | Single column, drill-down | Fullscreen, bottom nav visible | Fullscreen, bottom nav hidden, `< Back` button |

### 7.2 Layout transitions

- **Desktop → Tablet:** conversation list narrows from 320px to 260px. No structural change. Message bubbles' max-width adjusts proportionally.
- **Tablet → Mobile:** the split panel collapses to a single column. If a conversation was selected, the mobile view opens directly to the chat view (not the list) — preserving context across the breakpoint change. The `< Back` button appears.
- **Mobile → Tablet:** if the user was viewing a conversation, both panels become visible with that conversation selected. If the user was on the conversation list, the list occupies the left panel and the right panel shows `ChatEmptyState`.

### 7.3 Mobile-specific behaviors

- **Soft keyboard:** the compose bar uses `position: sticky; bottom: 0` with `env(safe-area-inset-bottom)` padding. On iOS, the viewport is adjusted to keep the input visible above the keyboard (standard `visualViewport` resize handling).
- **Pull-to-refresh:** pulling down at the top of the conversation list triggers a refetch (standard mobile pattern). Pulling down at the top of the message list triggers loading older messages.
- **Swipe:** no swipe-to-delete or swipe-to-reply in v2.0 — the `[⋯]` menu covers all actions. Swipe gestures are deferred to avoid conflict with the browser's back/forward navigation.

---

## 8. Open Questions

| # | Question | Impact | Resolution target |
|---|---|---|---|
| OQ-1 | **Does Bluesky's chat API support multiline messages?** The `chat.bsky.convo.sendMessage` schema accepts a `text: string` field with no documented multiline support. If it does, the composer should allow `Shift+Enter` to insert real newlines. If it doesn't, the composer must strip them. | Composer behavior | Before implementation — test against the live API |
| OQ-2 | **Real-time message delivery.** The spec uses 30s polling for the conversation list. For the active conversation's messages, polling is not specified — new messages should appear without a full refetch. Does Bluesky provide a WebSocket or SSE endpoint for chat events? If not, the active conversation must also poll (e.g., every 5s). If yes, the backend can proxy the event stream to the frontend. | Architecture — affects `ChatView` message arrival mechanism | Before implementation — investigate `chat.bsky.convo` WebSocket support |
| OQ-3 | **Typing indicators.** Bluesky's chat API does not expose typing indicators as of August 2026. If they add them, the `ChatView` already has the `typingIndicator` state slot. The spec renders nothing for it today. | Feature completeness | Monitor Bluesky chat API changelog |
| OQ-4 | **Delivery/read receipts.** Does `chat.bsky.convo.getMessages` return delivery status (sent/delivered/read)? If yes, render ✓/✓✓ icons on own messages. If not, show only ✓ (sent). | MessageBubble rendering | Before implementation — test against the live API |
| OQ-5 | **Service-auth token lifecycle for Bytescape-hosted users.** The backend mints a DPoP-bound token for `chat.bsky.*` scopes. What is the token lifetime? How is refresh handled? Does the frontend need to handle 401s from the chat API and request a new token? | Auth flow, error handling | Before implementation — design the token refresh flow |
| OQ-6 | **Message caching strategy.** The spec mentions localStorage caching for the conversation list. Should messages also be cached (e.g., the last 50 messages per conversation)? This improves perceived speed but increases storage usage and introduces staleness. | Performance, storage | Implementation phase — start without message caching, add if latency is noticeable |
| OQ-7 | **Group chat integration point.** FR-20 group chat (`space.bytescape.*`) is a separate screen. Where does the user access it? Options: (a) a "Groups" tab alongside "Chats" in the conversation list header; (b) a separate `/chat/groups` route; (c) a toggle in the bottom nav (replace "Chat" with "Chat ▾" dropdown). This affects the conversation list component's scope. | Navigation, screen scope | Before FR-20 spec is written |
| OQ-8 | **Image/media sending.** Bluesky's chat API supports image embeds in messages. Does the composer need an image-attach button? If yes, what is the upload flow (upload blob to PDS, then attach to message)? | Composer feature set, ChatScreen scope | v2.0 — start with text-only; image sending is a fast-follow if the API is straightforward |

**Resolved by this spec (not questions):** 1:1 DMs only (group chat is FR-20); Bytescape acts as a chat client (no local message store); both PDS user classes use the same chat backend; E2EE is not available (Bluesky's limitation, inherited transparently); conversation list sorted by most recent message; mobile drill-down pattern with back button; optimistic message sending with retry.
