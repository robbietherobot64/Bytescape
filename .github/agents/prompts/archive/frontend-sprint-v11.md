# Frontend Sprint v11 — Phase 4.5 Foundation (Track B)

**Sprint Goal:** Begin the client-side preparation for `bytescape.social` by implementing the DataProvider interface (B-S1) and adding the server account setup UI placeholder (B-S2). Also address the remaining scroll jitter from user feedback.

**Context:** Phase 4.5 introduces `bytescape.social` as a unified server hosting AT Protocol PDS + ActivityPub instance + Iroh relay. The frontend must be refactored so all data operations go through a `DataProvider` interface rather than calling `invoke()` directly. This enables the same React components to work with `TauriDataProvider` (desktop), `RestDataProvider` (web), and `WasmProvider` (visitor portal). Full architecture: `.github/agents/prompts/systems-dual-engine-architecture-2026-03-25.md`.

**HEAD:** `29f44fe` on `main` (clean tree). **Build:** 0 tsc errors, 267/267 tests pass.

---

## P1: Define DataProvider Interface (B-S1, part 1)

**Priority:** 🔥 Critical — foundational for multi-target builds.

**Task:** Create a TypeScript interface that abstracts ALL data operations currently done via `invoke()`.

1. **Audit all `invoke()` calls** across the codebase. Run: `grep -rn "invoke(" src/ --include="*.ts" --include="*.tsx" | grep -v node_modules | grep -v ".test."` to find all call sites.
2. **Group them by domain:**
   - Feed: `get_timeline_unified`, `get_unified_feed_oauth`, `get_author_feed`, etc.
   - Profile: `get_profile`, `update_profile`, etc.
   - Chat: `get_conversations`, `send_message`, `get_or_create_bsky_convo`, etc.
   - Auth: `check_auth_status`, `login_bluesky_oauth`, `mastodon_login`, etc.
   - Vault: `list_vault_files`, `export_vault_data`, `import_archive`, etc.
   - Posting: `publish_post_v2`, `syndicate_vault_item`, etc.
   - Identity: `get_owner_did`, `get_linked_accounts`, etc.
   - Browser: `browser_create`, `browser_navigate`, `browser_show`, `browser_hide`, etc.
   - Relay (new stubs): `relay_connect`, `relay_sync_status`, etc.
3. **Create `src/providers/DataProvider.ts`:**
   ```typescript
   export interface DataProvider {
     // Feed
     getTimelineUnified(params: { tab: string; limit: number; source_filter?: string }): Promise<FeedEntry[]>;
     getAuthorFeed(params: { limit: number }): Promise<FeedEntry[]>;
     
     // Profile
     getProfile(params: { did: string }): Promise<Profile>;
     
     // ... etc for all domains
   }
   ```
4. **Export the interface** and the types it uses from a barrel file (`src/providers/index.ts`).

**Key design decisions:**
- Method names should be camelCase TypeScript conventions (not snake_case Rust command names).
- Parameters should be typed objects (not positional args).
- Return types should use existing TypeScript interfaces from `types/index.ts`.
- The interface should be comprehensive but not exhaustive in this sprint — cover the most-used commands first (feed, profile, auth, posting). Less-used commands can be added incrementally.

**Files to create:**
- `src/providers/DataProvider.ts` — the interface
- `src/providers/index.ts` — barrel export

---

## P2: Implement TauriDataProvider (B-S1, part 2)

**Priority:** 🔥 Critical — must work identically to current direct `invoke()` calls.

**Task:** Implement the `DataProvider` interface using Tauri `invoke()` calls.

1. **Create `src/providers/TauriDataProvider.ts`:**
   ```typescript
   import { invoke } from '@tauri-apps/api/core';
   import { DataProvider } from './DataProvider';
   
   export class TauriDataProvider implements DataProvider {
     async getTimelineUnified(params) {
       return invoke('get_timeline_unified', params);
     }
     // ... etc
   }
   ```
2. **Create a provider context:** `src/providers/DataProviderContext.tsx` using React Context:
   ```typescript
   const DataProviderContext = createContext<DataProvider>(new TauriDataProvider());
   export const useDataProvider = () => useContext(DataProviderContext);
   export const DataProviderRoot = ({ provider, children }) => (
     <DataProviderContext.Provider value={provider}>{children}</DataProviderContext.Provider>
   );
   ```
3. **Wrap the app** in `DataProviderRoot` in `App.tsx` or `main.tsx` with `TauriDataProvider` as the default.

**⚠️ CRITICAL: This is a backwards-compatible refactor.** No behavior should change. Every screen must work identically. Do NOT refactor all invoke() calls in this sprint — just create the infrastructure. Screens will be migrated incrementally in future sprints.

**Verification:** `npx tsc --noEmit` (0 errors), `pnpm test -- --run` (267+ pass). Manually verify feed loads, auth works, posting works.

---

## P3: Migrate FeedScreen to DataProvider (B-S1, part 3)

**Priority:** High — proves the pattern works on the most important screen.

**Task:** Migrate `FeedScreen` (or `useUnifiedFeed` hook) to use `useDataProvider()` instead of direct `invoke()` calls.

1. **In the feed hook** (likely `hooks/useUnifiedFeed.ts`), replace:
   ```typescript
   // Before
   const data = await invoke('get_timeline_unified', { tab, limit, source_filter });
   
   // After
   const provider = useDataProvider();
   const data = await provider.getTimelineUnified({ tab, limit, source_filter });
   ```
2. **Verify** the feed loads identically. All tabs (All, Following, Feeds, Hashtags, Archives) must work.
3. **Write a test** that mocks the DataProvider and verifies FeedScreen renders correctly.

**Files to modify:**
- `src/hooks/useUnifiedFeed.ts` (or wherever feed invoke calls live)
- Possibly `src/screens/FeedScreen.tsx`

---

## P4: Server Account Setup UI (B-S2)

**Priority:** Medium — placeholder UI for Phase 4.5 server integration.

**Task:** Add a "Bytescape Social" section to the Settings screen.

1. **Find the Settings screen** — likely `src/screens/SettingsScreen.tsx`.
2. **Add a new section** (after the Connected Accounts section) titled "Bytescape Social":
   - **Header:** "☁️ Bytescape Social" with subtitle "Host your data on bytescape.social"
   - **Status indicators:**
     - PDS Account: "Not provisioned" (grayed out, with "Coming soon" badge)
     - AP Actor: "Not provisioned" (grayed out, with "Coming soon" badge)
     - Relay: "Not connected" (grayed out, with "Coming soon" badge)
   - **Info text:** "Server hosting is being deployed. Once live, you'll be able to migrate your Bluesky and Mastodon accounts to bytescape.social for full data sovereignty."
   - **No functional buttons yet** — this is purely informational for now. When the backend stubs (A-S3, A-S4) are implemented, the UI will invoke them.
3. **Style** using existing CSS variables (`--bg-secondary`, `--text-secondary`, `--accent-purple`). Match the card style of existing Settings sections.

**Files to modify:**
- `src/screens/SettingsScreen.tsx` (add section)
- `src/App.css` or component CSS (if needed for new styles)

---

## P5: Relay Status Indicator (B-S3)

**Priority:** Medium — visible indicator for relay connectivity.

**Task:** Add a small status indicator in the sidebar or top bar showing relay connection status.

1. **Create `src/components/RelayStatusBadge.tsx`:**
   - Three states: 🟢 Connected, 🟡 Syncing, ⚫ Offline
   - Tooltip with details (e.g., "Relay: offline — server not configured")
   - Initially always shows "Offline" (until relay backend is deployed)
   - Calls `invoke('relay_sync_status')` on mount (the backend stub will return an error — catch it gracefully and show "Offline")
2. **Place the badge** in the sidebar footer or the Omnibar area — wherever it's visible but unobtrusive.
3. **Write tests** for the three visual states.

**Files to create:**
- `src/components/RelayStatusBadge.tsx`

**Files to modify:**
- Sidebar component or Omnibar (to include the badge)

---

## P6: Image Hover Shift Fix (User Feedback)

**Priority:** Medium — outstanding user feedback item.

**Problem:** User reported: "the 'jitter' I was seeing was what happens when you mouse over an image." 1px shift on hover. Images should be completely unresponsive to mouseover but fully functional otherwise.

**Task:**
1. **Search for `:hover` rules** on image/media elements: `grep -rn ":hover" src/ --include="*.css" --include="*.tsx"`.
2. **Find the rule** that applies `transform`, `border`, `box-shadow`, `outline`, or `margin`/`padding` change on image hover.
3. **Remove or neutralize** the hover effect on images. Images should not visually react to mouse hover at all.
4. **Test:** Verify images display correctly, click/tap still works, no layout shift on hover.

---

## Constraints

- **Track boundary:** Only modify files in `src/`. Do NOT touch `src-tauri/` (Rust backend files).
- **Build verification:** Run `npx tsc --noEmit` + `pnpm test -- --run` before EVERY commit. Zero errors, zero test regressions.
- **Shared files:** Before editing `types/index.ts` or `hooks/useUnifiedFeed.ts`, run `git log --oneline -5 -- <filepath>` to check for recent cross-agent modifications.
- **Commit format:** `feat(track-b): description` or `fix(track-b): description`. One logical change per commit.
- **Backwards compatibility:** The DataProvider refactor MUST NOT change any user-visible behavior. Every screen must work identically before and after.
- **No TODOs:** Implement fully. If a method isn't ready, have it throw a descriptive error, don't leave `// TODO` comments.
