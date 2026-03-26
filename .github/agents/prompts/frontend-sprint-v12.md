# Frontend Sprint v12 — ForumScreen & Groups UI (Track B)

**Sprint Goal:** Build the forum-style groups UI — the hierarchical topic browser (ForumScreen), individual group view (GroupScreen), group creation flow, and a "Groups" feed tab. Also fix the scroll jitter by virtualizing the feed list (carried forward from original sprint v12 plan — user approved this).

**Context:** The user wants a forum-style community system with hierarchical topics and three group visibility types (Public, Closed, Private). The backend sprint v12 delivers the data model, redb tables, and ~20 Tauri commands for topics, groups, membership, and feeds. This sprint builds the frontend that consumes those commands.

**Design from user:**
- Landing page looks like an old-style forum with collapsible topic sections
- Topics can have sub-topics (tree hierarchy) — each with a [Subscribe] button
- Groups are nested under topics — each with a [Join] button and member count
- 🔒 badge on Closed groups (needs approval). Private groups don't appear in the listing.
- Users can create groups and request topic placement
- Subscribed topic/group posts populate a "Groups" feed tab on the home page
- Posts can be shared to another group, to the user's timeline, or via DM (share menu)

**HEAD:** `b782bef` on `main` (clean tree). **Build:** 0 tsc errors, 272/272 frontend tests, 207 Rust tests pass.

---

## P1: Feed Virtualization (🔥 Critical — Scroll Jitter Fix)

**Priority:** 🔥 Critical — directly addresses user feedback #2 ("Images and overall UI somewhat jittery when scrolling").

**Problem:** `FeedScreen.tsx` renders every feed entry via `filtered.map(entry => <FeedItemRenderer ... />)`. With 100+ entries containing `BlobImage` components with IntersectionObserver watchers, the browser must layout-calculate all elements on every scroll frame. This is the root cause of scroll jank.

**Task:**
1. **Install `@tanstack/react-virtual`**:
   ```bash
   pnpm add @tanstack/react-virtual
   ```
2. **Create `VirtualizedFeedList` component** (`src/components/feed/VirtualizedFeedList.tsx`):
   - Uses `useVirtualizer` from `@tanstack/react-virtual`
   - Takes `entries: FeedEntry[]`, `resolveMediaPath`, `mediaDir` as props
   - Renders only visible entries + configurable overscan (5 items)
   - Dynamic row height: `estimateSize` callback (300px default), `measureElement` ref for real DOM heights
   - The scroll container is the existing `.feed-scroll-container` div
3. **Replace the `.map()` call in FeedScreen** with `<VirtualizedFeedList>`.
4. **Preserve:**
   - Pull-to-refresh (overscroll detection on `scrollContainerRef`)
   - "Load Older Posts" button at bottom
   - Media filter applied before passing entries to virtualizer
5. **Test:** Existing feed tests pass. Add a test for `VirtualizedFeedList` rendering a subset.

**Expected impact:** DOM nodes drop from 500-2000+ to ~50-100. Smooth 60fps scroll.

**Key files:**
- `src/screens/FeedScreen.tsx` (lines 380-470) — feed rendering area
- `src/components/feed/FeedItemRenderer.tsx` — per-entry component
- `src/hooks/useUnifiedFeed.ts` — provides `feedEntries`

---

## P2: TypeScript Types for Groups (🔥 Critical)

**Priority:** 🔥 Critical — P3-P6 depend on these types.

**Task:** Add the TypeScript interfaces to `src/types/index.ts` that mirror the backend Rust structs from backend sprint v12 P1.

```typescript
// === Groups & Topics ===

export type GroupVisibility = 'Public' | 'Closed' | 'Private';
export type MemberRole = 'Owner' | 'Moderator' | 'Member';
export type MemberStatus = 'Active' | 'Pending' | 'Invited' | 'Banned';

export interface Topic {
  id: string;
  name: string;
  slug: string;
  parent_id: string | null;
  description: string;
  icon: string | null;
  sort_order: number;
  created_by: string;
  created_at: number;
}

export interface TopicNode extends Topic {
  children: TopicNode[];
  groups: GroupSummary[];
  subscribed: boolean;
}

export interface LocalGroup {
  group_id: string;
  name: string;
  slug: string;
  description: string;
  visibility: GroupVisibility;
  topic_id: string;
  owner_did: string;
  icon_blob: string | null;
  banner_blob: string | null;
  rules: string | null;
  posting_restricted_to_mods: boolean;
  member_count: number;
  created_at: number;
  updated_at: number;
}

export interface GroupSummary {
  group_id: string;
  name: string;
  slug: string;
  visibility: GroupVisibility;
  member_count: number;
  icon_blob: string | null;
}

export interface GroupMember {
  group_id: string;
  member_did: string;
  role: MemberRole;
  status: MemberStatus;
  joined_at: number;
}

export interface GroupPost {
  group_id: string;
  item_id: string;
  pinned: boolean;
  shared_by: string | null;
  posted_at: number;
}

export interface TopicSubscription {
  topic_id: string;
  user_did: string;
  subscribed_at: number;
}
```

**Shared file warning:** Before editing `types/index.ts`, run `git log --oneline -5 -- src/types/index.ts`.

---

## P3: ForumScreen — Topic Hierarchy Browser (🔥 Critical)

**Priority:** 🔥 Critical — the main groups landing page.

**Task:** Create `src/screens/ForumScreen.tsx` — the old-style forum browser.

### 3.1 Layout

```
┌─────────────────────────────────────────────────┐
│  📋 Forums                              🔍 [+]  │
├─────────────────────────────────────────────────┤
│                                                 │
│  ▼ 🌐 Fediverse                    [Subscribe] │
│    ├─ ▼ Bytescape                   [Subscribe] │
│    │   ├─ Announcements             [Subscribe] │
│    │   ├─ Feature Requests          [Subscribe] │
│    │   ├─ Bug Reports               [Subscribe] │
│    │   └─ Groups                    [Subscribe] │
│    ├─ Lemmy                         [Subscribe] │
│    ├─ Mastodon                      [Subscribe] │
│    └─ PieFed                        [Subscribe] │
│                                                 │
│  ▼ 💻 Technology                    [Subscribe] │
│    ├─ Programming                   [Subscribe] │
│    │   └─ Rust Enthusiasts (23)           [Join]│
│    ├─ Linux & FOSS                  [Subscribe] │
│    │   └─ NixOS Tips (12)                 [Join]│
│    └─ Security                      [Subscribe] │
│                                                 │
│  ▶ 🏥 Health & Wellness            [Subscribe] │
│  ▶ 🎨 Art & Creative               [Subscribe] │
│  ▶ 🎮 Gaming                       [Subscribe] │
│  ▶ 🔬 Science                      [Subscribe] │
│  ▶ 📰 News & Politics              [Subscribe] │
│                                                 │
│  [+ Create Group]                               │
└─────────────────────────────────────────────────┘
```

### 3.2 Implementation

1. **Data fetching:** Call `invoke<TopicNode[]>('get_topic_tree')` on mount. Store in component state.
2. **`TopicTreeNode` component** — recursive component rendering a single topic:
   - Expand/collapse toggle (▶/▼) for topics with children or groups
   - Topic name + icon
   - [Subscribe] / [Subscribed ✓] toggle button — calls `invoke('subscribe_topic')` / `invoke('unsubscribe_topic')`
   - Indentation increases with depth (CSS `padding-left` based on depth prop)
3. **`GroupListItem` component** — rendered under each topic:
   - Group name + member count badge
   - 🔒 icon for Closed groups
   - [Join] / [Joined ✓] / [Pending ⏳] button based on membership status
   - Clicking the group name navigates to the GroupScreen (P4)
4. **Search bar** at the top — calls `invoke('search_groups')` and shows results inline
5. **[+ Create Group]** button at the bottom — opens the creation modal (P5)
6. **Collapsed by default:** Top-level topics start collapsed. User clicks to expand. Remember expansion state in `localStorage`.

### 3.3 Styling

- Use the existing CSS variable system (`--bg-primary`, `--bg-secondary`, `--accent-purple`, `--text-primary`)
- Forum rows alternate background colors slightly (`--bg-secondary` for odd rows)
- Subscribe/Join buttons: small, pill-shaped, accent color when active
- Indentation: 20px per level
- Create a `ForumScreen.css` for component-specific styles

### 3.4 Sidebar Integration

Add a "Forums" icon to the Sidebar (`src/components/Sidebar.tsx`) — use a list/forum icon. Clicking it navigates to `ForumScreen`. Place it between the existing Feed and Chat icons.

### 3.5 Routing

Add the `ForumScreen` to the screen routing in `App.tsx` (follow the same pattern as FeedScreen, ChatScreen, etc.).

**Verification:** The ForumScreen renders the seeded topic tree. Expanding topics shows sub-topics. Subscribe buttons toggle. `npx tsc --noEmit` + `pnpm test -- --run` pass.

---

## P4: GroupScreen — Individual Group View (High)

**Priority:** High — where users browse a group's content.

**Task:** Create `src/screens/GroupScreen.tsx`.

### 4.1 Layout

```
┌─────────────────────────────────────────────────┐
│  [← Back to Forums]                             │
│  ┌─────────────────────────────────────────────┐│
│  │ 🖼️ Banner Image                             ││
│  │           Mindfulness Daily                  ││
│  │           🔓 Public · 42 members             ││
│  │           [Joined ✓]  [Share]                ││
│  └─────────────────────────────────────────────┘│
│                                                 │
│  ┌── Posts ──┬── About ──┬── Members ──┐        │
│  │                                     │        │
│  │  [📝 New Post]                      │        │
│  │                                     │        │
│  │  ┌─ FeedEntryCard ──────────────┐   │ Rules  │
│  │  │ Post content...              │   │ -----  │
│  │  │ by @alice · 2h ago           │   │ Be kind│
│  │  └──────────────────────────────┘   │ ...    │
│  │                                     │        │
│  │  ┌─ FeedEntryCard ──────────────┐   │        │
│  │  │ Shared by @bob               │   │        │
│  │  │ Original post content...     │   │        │
│  │  └──────────────────────────────┘   │        │
│  │                                     │        │
│  └─────────────────────────────────────┘        │
└─────────────────────────────────────────────────┘
```

### 4.2 Implementation

1. **Header:** Group name, banner image, visibility badge (🔓 Public / 🔒 Closed / 🔐 Private), member count, Join/Leave button
2. **Three tabs:**
   - **Posts** — Feed of group posts via `invoke('get_group_feed', { groupId, limit: 50 })`. Reuse existing `FeedEntryCard` components. Show "Shared by @handle" badge on cross-posted items.
   - **About** — Group description (rendered as markdown) + rules + topic breadcrumb (e.g., "Technology > Programming")
   - **Members** — List of group members with role badges (👑 Owner, 🛡️ Mod, 👤 Member)
3. **[📝 New Post] button** — Opens ComposeBox. On submit, creates a `BytescapeItem` (via existing compose flow) AND calls `invoke('post_to_group', { groupId, itemId })`.
4. **Join/Leave button:**
   - Not a member → [Join] (calls `join_group`)
   - Public group → immediate join, button changes to [Joined ✓]
   - Closed group → [Pending ⏳] after click, waiting for approval
   - Member → [Leave] (calls `leave_group`)
5. **Rules sidebar** — Rendered from `group.rules` markdown, right side panel on wide screens, collapsed on narrow.

### 4.3 Navigation

- `GroupScreen` receives `groupId` as a prop (from ForumScreen click or deep link)
- Back button returns to ForumScreen

**Verification:** GroupScreen renders group info and posts. Join/Leave buttons work. `tsc + tests` pass.

---

## P5: Group Creation Modal (High)

**Priority:** High — users need to create groups.

**Task:** Create `src/components/CreateGroupModal.tsx`.

### 5.1 Form Fields

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| Name | text input | ✅ | "Mindfulness Daily" |
| Slug | auto-generated from name | ✅ | "mindfulness-daily", editable |
| Description | textarea | ✅ | Markdown supported |
| Visibility | radio group | ✅ | Public / Closed / Private |
| Topic | dropdown/tree selector | ✅ | Select from topic tree |
| Rules | textarea | ❌ | Markdown |
| Icon | image upload/URL | ❌ | Group avatar |
| Banner | image upload/URL | ❌ | Header image |

### 5.2 Implementation

1. **Topic selector:** Render the topic tree as a nested dropdown/select. User picks which topic their group belongs under. If unsure, they can pick "Fediverse > Bytescape > Groups" and request placement in a post.
2. **Slug auto-generation:** Convert name to lowercase, replace spaces with hyphens, strip special chars. User can override.
3. **Visibility explanation:** Show a brief description next to each radio option:
   - 🔓 Public — "Anyone can find and join this group"
   - 🔒 Closed — "Anyone can find this group, but joining requires approval"
   - 🔐 Private — "Only invited members can see and join this group"
4. **Submit:** Calls `invoke('create_group', { name, slug, description, visibility, topicId, rules, iconBlob, bannerBlob })`.
5. **On success:** Close modal, navigate to the new GroupScreen.

### 5.3 Styling

- Modal overlay with centered card (max-width 600px)
- Form uses the existing theme CSS variables
- Submit button uses `--accent-purple`
- Create `CreateGroupModal.css` or use inline styles consistent with existing modals

**Verification:** Modal opens from ForumScreen's [+ Create Group] button. Form validates. Submitting creates a group. `tsc + tests` pass.

---

## P6: "Groups" Feed Tab + Share Menu (Medium)

**Priority:** Medium — integrates groups into the main user experience.

### 6.1 Groups Feed Tab

**Task:** Add a "Groups" tab to FeedScreen's tab bar.

1. **Tab bar update:** Add "Groups" tab alongside existing tabs (Following, Subscribed, etc.) in `FeedScreen.tsx`.
2. **Data source:** When "Groups" tab is active, call `invoke('get_groups_feed', { limit: 50 })` instead of the regular feed hooks.
3. **Empty state:** If user hasn't subscribed to any topics or joined any groups, show: "No group subscriptions yet. [Browse Forums →]" with a link to ForumScreen.
4. **Rendering:** Use the same `FeedEntryCard` components (or the new `VirtualizedFeedList` from P1 if ready). Add a small group badge on each card showing which group the post came from (group name as a clickable link to GroupScreen).

### 6.2 Share Menu Enhancement

**Task:** Add group sharing to the existing share/repost flow.

1. **Locate the existing share/repost UI** — likely in `QuoteRepostModal.tsx` or a context menu on feed cards.
2. **Add "Share to Group" option** that:
   - Opens a group picker (list of user's joined groups from `get_user_group_memberships`)
   - On selection, calls `invoke('share_to_group', { itemId, targetGroupId })`
   - Shows success toast: "Shared to {group name}"
3. **Keep existing options:** Share to timeline, quote repost, etc.

### 6.3 Group Badge on Feed Cards

**Task:** When a feed item came from a group (i.e., it appeared via the Groups feed), show a small badge:
- `📋 Mindfulness Daily` — clickable, links to GroupScreen
- Positioned below the author info, above the content
- Styled as a subtle pill badge using `--bg-secondary` background

**Verification:** Groups tab shows aggregated group posts. Share menu includes "Share to Group". `tsc + tests` pass.

---

## Constraints

- **Track boundary:** Only modify files in `src/`. Do NOT touch `src-tauri/src/` (backend files).
- **New dependency:** `@tanstack/react-virtual` — install via `pnpm add`.
- **Build verification:** Run `npx tsc --noEmit` + `pnpm test -- --run` before EVERY commit. Zero errors, zero test regressions.
- **Shared files:** Before editing `types/index.ts`, `useUnifiedFeed.ts`, `App.tsx`, or `Sidebar.tsx`, run `git log --oneline -5 -- <filepath>` to check for recent cross-agent modifications.
- **Commit format:** `feat(track-b): description` or `fix(track-b): description`. One logical change per commit.
- **No TODOs:** Implement features fully, don't leave placeholder comments.
- **Update `progress.md`** with your deliverables when done.
- **Backend dependency:** P2-P6 depend on backend sprint v12 commands existing. If backend hasn't delivered yet, implement the UI with mock data and wire up `invoke()` calls that will work once the backend lands. Use a `useGroupsData` hook that abstracts the data source — easy to swap from mock to real.

---

## What This Sprint Does NOT Include (Future Sprints)

- **Federated group browsing** (sprint v13) — browsing/subscribing to remote Lemmy/PieFed communities
- **Group moderation UI** (sprint v13) — approval queue, ban interface, role management
- **Events & Calendar UI** (sprint v14+)
- **RSS subscription management** (sprint v14+)
- **Deep profile customization** (sprint v14+)
