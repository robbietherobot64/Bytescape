# Frontend Sprint v12 — Feed Virtualization & UX Polish (Track B)

**Sprint Goal:** Fix the scroll jitter (user priority #2) by virtualizing the feed list, and address accumulated UX debt — unreliable chat button, type duplication, layout fill.

**Context:** Sprint v11 delivered the DataProvider interface (114 methods), TauriDataProvider, Settings "Bytescape Social" section, RelayStatusBadge, and fixed the image black flash. The biggest remaining performance issue is that `FeedScreen` renders ALL feed entries into the DOM simultaneously via `.map()` — no virtualization, no windowing. With 100+ posts containing images, this causes layout thrashing and scroll jank. Sprint v12 also continues DataProvider migration and prepares the frontend for real PDS connectivity (backend sprint v12 is deploying the PDS).

**HEAD:** `06cf97a` on `main` (clean tree). **Build:** 0 tsc errors, 272/272 frontend tests, 207 Rust tests pass.

---

## P1: Feed Virtualization (🔥 Highest Impact for Scroll Jitter)

**Priority:** 🔥 Critical — directly addresses user feedback #2 ("Images and overall UI somewhat jittery when scrolling").

**Problem:** `FeedScreen.tsx` (line ~454) renders every feed entry via `filtered.map(entry => <FeedItemRenderer ... />)`. With 100+ entries, each containing `BlobImage` components with IntersectionObserver watchers, the browser must layout-calculate all elements on every scroll frame. This is the root cause of scroll jank.

**Task:**
1. **Install `@tanstack/react-virtual`** (the standard React virtualization library):
   ```bash
   pnpm add @tanstack/react-virtual
   ```
2. **Create a `VirtualizedFeedList` component** (`src/components/feed/VirtualizedFeedList.tsx`):
   - Uses `useVirtualizer` from `@tanstack/react-virtual`
   - Takes `entries: FeedEntry[]`, `resolveMediaPath`, `mediaDir` as props
   - Renders only visible entries + configurable overscan (5 items recommended)
   - Dynamic row height estimation: use `estimateSize` callback. Feed cards vary in height (text-only: ~120px, with image: ~400px, with gallery: ~500px). Start with a reasonable estimate (300px) and let the virtualizer auto-measure.
   - The scroll container is the existing `.feed-scroll-container` div (line 385 of FeedScreen.tsx)
3. **Replace the `.map()` call in FeedScreen** (lines 438-462) with `<VirtualizedFeedList>`.
4. **Preserve existing behavior:**
   - Pull-to-refresh (overscroll detection on `scrollContainerRef`) must still work — the virtualizer's scroll container and the pull-to-refresh container need to be the same element.
   - "Load Older Posts" button at the bottom must still appear after the last virtualized item.
   - Media filter still applies before passing entries to the virtualizer.
   - Infinite scroll / "Load More" trigger at bottom should still work.
5. **Handle dynamic heights properly:**
   - Use `measureElement` ref callback on each rendered item for accurate height measurement.
   - After first render, the virtualizer measures real DOM heights and adjusts.
6. **Test:** Existing feed-related tests should still pass. Add a test for `VirtualizedFeedList` rendering only a subset of items.

**Key files:**
- `src/screens/FeedScreen.tsx` (lines 380-470) — the feed rendering area
- `src/components/feed/FeedItemRenderer.tsx` — the component rendered per entry
- `src/hooks/useUnifiedFeed.ts` — provides `feedEntries`
- `src/components/BlobImage.tsx` — already uses IntersectionObserver for lazy loading (now partly redundant with virtualization — but keep it for images that are off-screen within a visible card)

**Expected impact:** DOM node count drops from 500-2000+ (all entries) to ~50-100 (visible window). Scroll FPS should go from stuttery to smooth 60fps.

---

## P2: Chat Button Reliability Fix

**Priority:** High — user reported "works only sometimes, only after clicking 5 times."

**Problem:** From ROADMAP open issues: Chat initiation button on FriendsScreen is intermittently unresponsive. Possible causes:
1. Async `invoke('get_or_create_bsky_convo')` has no loading indicator — user clicks again before it resolves
2. CSS elements stealing click events (`.friend-card::after` sparkle was fixed but other elements may still block)
3. Event propagation — button is inside `.friend-card` which has its own `onClick`

**Task:**
1. **Add diagnostic logging** (temporary) to the chat button click handler — `console.log('Chat button clicked', { did, timestamp })` — to determine if clicks are reaching the handler at all.
2. **Add a loading spinner** to the chat initiation button:
   - When clicked, show a spinner and disable the button
   - On success, navigate to the chat
   - On error, show an error toast and re-enable
3. **Investigate click event propagation:**
   - Check if the `.friend-card` parent `onClick` is consuming the event before the chat button handler
   - Add `e.stopPropagation()` to the chat button handler if needed
   - Check for overlapping absolutely-positioned elements (use browser DevTools `pointer-events` inspection)
4. **Check for duplicate click handlers** — React 19 StrictMode can cause double-mount issues with event listeners
5. **Remove diagnostic logging** once the fix is verified, but keep the loading state.

**Key files:**
- `src/screens/FriendsScreen.tsx` — the friend card with chat button
- `src/App.tsx` — `onOpenChat` callback
- `src/screens/ChatScreen.tsx` — `pendingConvoMeta` handling
- `src/App.css` — check for any remaining `pointer-events` or overlay issues on friend cards

**Verification:** The chat button should respond on the first click every time, with visible loading feedback.

---

## P3: DataProvider Migration — ChatScreen + ProfileScreen

**Priority:** Medium — continues the B-S1 migration pattern from sprint v11.

**Task:** Migrate two more screens from direct `invoke()` calls to `useDataProvider()`.

1. **ChatScreen** — Replace all `invoke('get_conversations')`, `invoke('send_message')`, `invoke('get_messages')` etc. with `provider.getConversations()`, `provider.sendMessage()`, etc.
2. **ProfileScreen** — Replace all `invoke('get_profile')`, `invoke('get_author_feed')` etc. with `provider.getProfile()`, `provider.getAuthorFeed()`, etc.
3. **Pattern:** Same as the `useUnifiedFeed` migration in sprint v11:
   ```typescript
   // Before
   const result = await invoke<T>('command_name', { param1, param2 });
   // After
   const provider = useDataProvider();
   const result = await provider.commandName({ param1, param2 });
   ```
4. **Test:** All existing tests for these screens should pass without modification (the DataProvider wraps the same `invoke()` calls).

**Goal:** Get 3+ screens migrated to DataProvider (FeedScreen done in v11, plus ChatScreen + ProfileScreen in v12). This builds toward the web app target where `RestDataProvider` replaces `TauriDataProvider`.

---

## P4: Type Unification — `BlueskyPost` vs `FeedEntry`

**Priority:** Medium — reduces frontend complexity.

**Problem:** The frontend has two overlapping types for feed items: `BlueskyPost` (from Bluesky API) and `FeedEntry` (from unified feed). `FeedScreen` handles both through per-tab switching logic. The `useUnifiedFeed` hook already normalizes to `FeedEntry`, but some components still accept `BlueskyPost` directly.

**Task:**
1. **Audit all usages** of `BlueskyPost` in the frontend:
   ```bash
   grep -rn "BlueskyPost" src/ --include="*.ts" --include="*.tsx" | grep -v node_modules
   ```
2. **Identify components that accept `BlueskyPost`** directly — these should accept `FeedEntry` instead, since all feeds are now normalized through the unified pipeline.
3. **Create an adapter function** (if not already present) that converts `BlueskyPost` → `FeedEntry` for any legacy call sites.
4. **Update component props** to use `FeedEntry` as the canonical type.
5. **Remove `BlueskyPost` from `types/index.ts`** if all usages are migrated — or mark it as deprecated with a comment.

**Constraint:** Don't break any tests. The type change should be transparent at runtime since the data shape is compatible.

---

## P5: Settings Screen — Wire PDS Status Polling

**Priority:** Medium — connects to backend sprint v12's PDS deployment.

**Context:** The backend sprint v12 is deploying the Official PDS and advancing `provision_pds_account` toward real XRPC calls. The frontend Settings "Bytescape Social" section (added in sprint v11) currently shows static "coming soon" badges.

**Task:**
1. **Poll `relay_sync_status`** on the Settings screen when the "Bytescape Social" section is visible:
   - Currently returns `{ status: "not_configured" }` — display as "Not connected"
   - When PDS is live (future): will return actual status — display accordingly
2. **Wire the "Provision PDS Account" button** to call `provision_pds_account`:
   - Show a form: username + invite code
   - On submit, call the backend command
   - Show loading state during the async call
   - On success, refresh the status display
   - On error (current: "PDS not configured"), show the error message clearly
3. **Add a "Server URL" configuration field:**
   - Text input for PDS URL (e.g., `https://pds.bytescape.social`)
   - "Connect" button that calls `configure_server` (if backend exposes it) or stores in localStorage for now
   - This prepares for the self-hosted flow where users point to their own PDS

**Key files:**
- `src/screens/SettingsScreen.tsx` — the Bytescape Social section
- `src/providers/DataProvider.ts` — ensure relay/provisioning methods are in the interface
- `src/providers/TauriDataProvider.ts` — ensure they're wired to invoke()

---

## P6: UF.1 Layout Polish — Full-Width Responsive

**Priority:** Low — visual polish, only if P1-P5 are complete.

**Task:** The app is maximized on launch (fixed in sprint v10) but doesn't fill the available width efficiently. Sidebars and content area have hardcoded widths that waste horizontal space on wide monitors.

1. **Audit the main layout grid** in `App.css` / `App.tsx`:
   - Sidebar: should have a fixed width (e.g., 240px)
   - Content area: should `flex: 1` to fill remaining space
   - Right panel (if any): fixed or collapsible
2. **Feed cards:** Should fill the content area width, not be constrained to a narrow column
3. **Media grids:** Images in gallery posts should use CSS Grid with `auto-fill` / `minmax()` to adapt to available width
4. **Test at different window sizes:** 1280px, 1920px, 2560px widths

---

## Constraints

- **Track boundary:** Only modify files in `src/`. Do NOT touch `src-tauri/src/` (backend files) or `server/` (infrastructure).
- **New dependency:** `@tanstack/react-virtual` — install via `pnpm add`.
- **Build verification:** Run `npx tsc --noEmit` + `pnpm test -- --run` before EVERY commit. Zero errors, zero test regressions.
- **Shared files:** Before editing `types/index.ts`, `useUnifiedFeed.ts`, or `App.tsx`, run `git log --oneline -5 -- <filepath>` to check for recent cross-agent modifications.
- **Commit format:** `feat(track-b): description` or `fix(track-b): description`. One logical change per commit.
- **No TODOs:** Implement features fully, don't leave placeholder comments.
- **Update `progress.md`** with your deliverables when done.
