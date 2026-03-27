# Frontend Sprint v13 — Groups Feed Tab, Moderation UI & Polish (Track B)

**Sprint Goal:** Wire up the Groups feed tab that was scaffolded but never completed in v12, add the moderation UI for group owners/mods, add the share-to-group flow, and polish the existing Forum/Group screens.

**Context:** Sprint v12 delivered ForumScreen, GroupScreen, CreateGroupModal, VirtualizedFeedList, and TypeScript types. However, the Groups feed tab on FeedScreen was only scaffolded (type/label/empty-state exist but the tab button isn't rendered and `get_groups_feed` isn't wired). The share-to-group UI and group badges on feed cards are also missing. Additionally, the backend agent built most of the frontend UI (track violation) — some of it uses `any[]` types and non-reactive mock state that should be cleaned up.

**HEAD:** `52b3d41` on `main`. **Build:** 0 tsc errors, 272 frontend tests, 216 Rust tests pass.

---

## P1: Wire Up Groups Feed Tab (🔥 Critical)

**Priority:** 🔥 Critical — the Groups feed tab exists in types/labels but the button never renders and data isn't connected.

**Task:** Complete the Groups feed tab that was partially scaffolded in sprint v12.

### 1.1 Render the Tab Button

In `FeedScreen.tsx`, the tab bar currently hardcodes which tabs to show. The `'groups'` tab value exists in `FeedTab` type and `FEED_TAB_LABELS` but is filtered out or not rendered.

**Fix:** Ensure the "Groups" tab button appears in the tab bar alongside the existing tabs. Use the same button styling and active-state logic.

### 1.2 Data Source

When the "Groups" tab is active:
1. Call `invoke<BytescapeItem[]>('get_groups_feed', { limit: 50 })` (or use the `useGroupsData` hook's `getGroupsFeed`).
2. Display the results using the same `VirtualizedFeedList` component.
3. If the hook is using mock data (`USE_MOCK = true`), set `USE_MOCK = false` since the backend commands now exist.

### 1.3 Empty State

The empty state already exists in the code. Verify it shows:
- "No group subscriptions yet."
- A clickable "Browse Forums →" link that navigates to ForumScreen (`setScreen('forum')`).

### 1.4 Fallback

If `get_groups_feed` fails or returns an error (e.g., no authenticated session), show a user-friendly error rather than crashing.

**Key files:**
- `src/screens/FeedScreen.tsx` — tab bar rendering, data fetching per tab
- `src/hooks/useGroupsData.ts` — switch `USE_MOCK` to `false`
- `src/store/index.ts` — `FeedTab` type, `FEED_TAB_LABELS`

---

## P2: Share-to-Group Menu (High)

**Priority:** High — users should be able to share existing posts to groups.

**Task:** Add "Share to Group" as an option in the post action menu.

### 2.1 Locate Existing Share UI

Check these files for the existing share/repost flow:
- `src/components/feed/FeedEntryCard.tsx` — post action buttons
- `src/components/QuoteRepostModal.tsx` — repost modal (if it exists)
- `src/components/feed/FeedActionBar.tsx` — action bar below posts

### 2.2 Add "Share to Group" Option

When user clicks a share/repost button on any feed card:
1. Show existing options (repost, quote post, etc.) PLUS a new "📋 Share to Group" option.
2. Clicking "Share to Group" opens a **group picker** — a small modal/dropdown listing the user's joined groups (from `invoke('get_user_group_memberships')`).
3. On selecting a group, call `invoke('share_to_group', { itemId, targetGroupId })`.
4. Show a success toast: "Shared to {group name}" (use existing toast/notification system if one exists, or a simple temporary banner).

### 2.3 Group Picker Component

Create `src/components/GroupPicker.tsx`:
- Fetches user's joined groups on mount
- Renders a searchable list
- Emits `onSelect(groupId, groupName)` callback
- Styled as a small modal overlay (max-height 300px, scrollable)

**Key files:**
- `src/components/feed/` — existing action buttons
- `src/components/GroupPicker.tsx` — new component
- `src/hooks/useGroupsData.ts` — `getUserGroupMemberships()`

---

## P3: Group Badge on Feed Cards (High)

**Priority:** High — when viewing the Groups feed tab, users should see which group each post came from.

**Task:** Add a group origin badge to feed cards when displayed in the Groups feed context.

### 3.1 Badge Component

Create a simple inline badge: `📋 GroupName` — small pill, clickable, navigates to GroupScreen.

### 3.2 Integration

The `VirtualizedFeedList` already accepts a `headerSlot` prop. When rendering the Groups feed tab:
- Pass group context data to each feed entry (the `GroupPost.group_id` associated with each item)
- Render the badge between the author info and the post content
- Clicking the badge navigates to the group: `setScreen('group')` + set `activeGroupId`

### 3.3 Shared Posts Badge

For cross-posted items (where `GroupPost.shared_by` is set), also show:
- "Shared by @handle" — small text below the group badge

**Key files:**
- `src/components/feed/VirtualizedFeedList.tsx` — `headerSlot` prop
- `src/components/feed/FeedEntryCard.tsx` or `FeedItemRenderer.tsx` — badge rendering
- `src/screens/FeedScreen.tsx` — pass group context when Groups tab is active

---

## P4: Moderation UI (High)

**Priority:** High — group owners/mods need a UI for managing members and content.

**Task:** Add moderation capabilities to GroupScreen.

### 4.1 Pending Members Queue

For group owners/mods viewing a Closed group's Members tab:
1. Show a "Pending" section at the top listing members with `status: Pending`.
2. Each pending member row shows: handle/DID, [✅ Approve] button, [❌ Reject] button.
3. Approve calls `invoke('approve_member', { groupId, memberDid })`.
4. Reject calls `invoke('reject_member', { groupId, memberDid })`.
5. After action, remove the member from the Pending list and refresh.

### 4.2 Member Actions

For group owners viewing an Active member:
1. Three-dot menu (⋮) on each member row
2. Options:
   - "Promote to Moderator" → `invoke('set_member_role', { groupId, memberDid, role: 'Moderator' })`
   - "Demote to Member" → (for mods) `invoke('set_member_role', { groupId, memberDid, role: 'Member' })`
   - "Ban" → `invoke('ban_member', { groupId, memberDid })` with confirmation dialog
3. Owner badge (👑) and Mod badge (🛡️) are already shown — just add the action menu.

### 4.3 Invite Member

For Private groups:
1. Add an "Invite Member" button at the top of the Members tab.
2. Opens a text input for entering a DID or handle.
3. Calls `invoke('invite_member', { groupId, memberDid })`.
4. Shows "Invitation sent" confirmation.

### 4.4 Post Moderation

For owners/mods on the Posts tab:
1. Add a ⋮ menu on each post card with:
   - "📌 Pin" / "Unpin" → `invoke('pin_group_post')` / `invoke('unpin_group_post')`
   - "🗑️ Remove" → `invoke('remove_group_post')` with confirmation
2. Pinned posts show a 📌 badge and appear at the top of the feed.

### 4.5 Conditional Rendering

All moderation UI is **only visible** to owners/mods:
- Check current user's role via `invoke('get_group_members_cmd')` or store membership state in `useGroupsData`.
- Regular members see the clean member list without action menus.
- Non-members see no Members tab content (or a limited view).

**Key files:**
- `src/screens/GroupScreen.tsx` — Members tab, Posts tab
- `src/hooks/useGroupsData.ts` — add membership role checking helpers

---

## P5: useGroupsData Cleanup (Medium)

**Priority:** Medium — the hook was created by the backend agent (track violation) with some issues.

**Task:** Fix issues in `src/hooks/useGroupsData.ts`:

### 5.1 Switch to Live Backend

- Set `USE_MOCK = false` (or remove the mock path entirely since backend commands exist).
- Verify all `invoke()` calls match the actual backend command signatures.

### 5.2 Fix Type Safety

- Replace `any[]` with proper types where used (e.g., in GroupScreen's posts list).
- Ensure all hook return types match the TypeScript interfaces in `types/index.ts`.

### 5.3 Reactive State

The mock state uses module-level `let` variables that aren't reactive:
- Remove the mock data path.
- Use React state or Zustand store for group data that needs to trigger re-renders.
- Consider a simple `useGroupsStore` Zustand store for cached group/membership state.

### 5.4 Error Handling

Ensure all `invoke()` calls have proper try/catch with user-friendly error messages rather than raw Rust error strings.

**Key files:**
- `src/hooks/useGroupsData.ts`

---

## P6: Forum/Group Screen Polish (Medium)

**Priority:** Medium — small fixes to improve the UX.

### 6.1 ForumScreen Search

Verify the search bar at the top of ForumScreen works:
- Calls `invoke('search_groups_cmd', { query })` on input.
- Shows matching groups inline (replacing the topic tree temporarily).
- Clearing search restores the topic tree view.

### 6.2 Navigation Consistency

- Verify all navigation paths work:
  - Sidebar → Forums → click group → GroupScreen → Back → ForumScreen
  - FeedScreen Groups tab → click group badge → GroupScreen → Back → FeedScreen
  - GroupScreen → New Post → ComposeBox → submit → post appears in group feed
- Ensure `useLocationStore` URL updates for forum/group screens.

### 6.3 Loading States

Add loading spinners/skeletons:
- ForumScreen: while fetching topic tree
- GroupScreen: while fetching group details and feed
- CreateGroupModal: while submitting

### 6.4 Accessibility

- Add `aria-label` attributes to ForumScreen subscribe/join buttons
- Add `aria-expanded` to collapsible topic nodes
- Modal overlay should trap focus and be dismissible with Escape
- Forum tree should be navigable with keyboard (arrow keys, Enter)

---

## Constraints

- **Track boundary:** Only modify files in `src/`. Do NOT touch `src-tauri/src/` (backend files).
- **Build verification:** Run `npx tsc --noEmit` + `pnpm test -- --run` before EVERY commit. Zero errors, zero test regressions.
- **Shared files:** Before editing `types/index.ts`, `useUnifiedFeed.ts`, `App.tsx`, `store/index.ts`, run `git log --oneline -5 -- <filepath>`.
- **Commit format:** `feat(track-b): description` or `fix(track-b): description`.
- **No TODOs in code.** Implement fully.
- **Backend dependency:** P1-P4 depend on backend sprint v13 moderation commands. If backend hasn't delivered yet:
  - P1 (Groups feed tab) works with existing v12 commands (`get_groups_feed` exists).
  - P2-P3 (share menu, badges) work with existing v12 commands (`share_to_group`, `get_user_group_memberships` exist).
  - P4 (moderation UI) requires v13 commands — implement the UI with `invoke()` calls that will work once backend lands. Show "Coming soon" or disable buttons if command returns error.
- **Update `progress.md`** with deliverables when done.

---

## What This Sprint Does NOT Include (Future)

- **Remote community browsing** (sprint v14+) — browsing Lemmy/PieFed instances from ForumScreen
- **Events & Calendar UI** (sprint v14+)
- **RSS subscription management** (sprint v14+)
- **Deep profile customization** (sprint v14+)
