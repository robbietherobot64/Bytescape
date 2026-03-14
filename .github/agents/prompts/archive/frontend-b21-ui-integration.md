# Frontend Agent Kickoff: B2.1 Review + UI Redesign Integration

## Context — What's Changed Since Your Last Session

### Your B2.1 work was delivered ✅
Your commit `aabfa9e` (B2.1 GrapesJS Profile Builder) is on `track-b/b2-profile-builder`. The PM has not yet verified this branch in detail, but the commit message and file stats look complete (13 files, +1,356/-6 lines, 5 new tests). **Do not re-do this work.**

### ⚠️ Major: PM-Authorized UI Redesign on `track-b/ui-redesign`

The Backend agent was PM-authorized to build frontend UI components. They created a full UI redesign on `track-b/ui-redesign` with 7 commits (~1,300 lines of new React/CSS). This is **your domain** — you need to review it, decide what to keep, refine, or rework.

**What was built:**

| Commit | Component | Description |
|--------|-----------|-------------|
| `737105a` | `TopBar.tsx` | Profile icon, search bar, hamburger menu — replaces sidebar navigation |
| `737105a` | `ContextPanel.tsx` | Left panel with screen-aware navigation (feed tabs, lens picker) |
| `737105a` | `FriendsPanel.tsx` | Right panel — AIM buddy list style with online/offline status |
| `737105a` | `AppLayout.tsx` | Rewritten to 3-column: TopBar + [ContextPanel \| main \| FriendsPanel] |
| `65d70e5` | FriendsPanel v2 | Status-aware away messages, color-coded dots (green/orange/red/grey) |
| `aed605e` | FriendsPanel v3 | Unified Bluesky follows + P2P peers, source filter tabs, avatar images |
| `ae4bf4b` | Chat unification | Buddy list merged with FriendsPanel, message/profile action icons |

**Files on that branch you need to review:**
- `src/components/TopBar.tsx` (116 lines — new)
- `src/components/ContextPanel.tsx` (103 lines — new)
- `src/components/FriendsPanel.tsx` (326 lines — new)
- `src/components/AppLayout.tsx` (58 lines — modified)
- `src/App.css` (+975/-238 lines — heavy modifications)
- `src/App.tsx` (+67 lines — routing/state changes)
- `src/screens/FeedScreen.tsx` (+38 lines — layout integration)
- `src/screens/ChatScreen.tsx` (+122 lines — buddy list integration)
- `src/hooks/useUnifiedFeed.ts` (+50 lines — feed hook changes)
- `src/components/FeedRightPanel.tsx` (+5 lines — minor)

### Branch State

Your `track-b/b2-profile-builder` branch forked from `c4e8123` (the A0 merge). `main` has moved to `bac702e` (6 more commits). The UI redesign is on a separate branch `track-b/ui-redesign` which also forked from `main` at `bac702e`.

**You need to:**
1. Merge `main` into your branch first
2. Then decide how to integrate the UI redesign

---

## Priority 0 — Merge Main + Review UI Redesign

### Step 1: Update Your Branch

```bash
git checkout track-b/b2-profile-builder
git merge main
# Resolve conflicts in lib.rs, oauth.rs, import.rs, Cargo.toml if any
```

### Step 2: Review the UI Redesign

```bash
# View the UI redesign changes
git diff main..track-b/ui-redesign -- 'bytescape/src/components/' 'bytescape/src/App.css' 'bytescape/src/App.tsx' 'bytescape/src/screens/'
```

Assess each component:
- **TopBar.tsx** — Does the navigation UX work? Is the search bar functional or placeholder?
- **ContextPanel.tsx** — Is the screen-aware navigation useful? Does it duplicate sidebar logic?
- **FriendsPanel.tsx** — This is the most complex piece (326 lines). Check: Does the Bluesky follows integration work? Is the status dot system clean? Is the 5-min refresh appropriate?
- **AppLayout.tsx** — The 3-column layout is a significant architectural change. Does it handle mobile correctly?
- **App.css** — 975 lines changed. Look for: naming consistency, variable usage, responsive breakpoints, overlap with your Profile Builder CSS additions.

### Step 3: Integration Decision

You have three options:

**Option A: Merge `track-b/ui-redesign` into your branch** — Accept the UI redesign as-is, fix any conflicts with your ProfileBuilder work, then continue.

**Option B: Cherry-pick selectively** — Take the components you like (e.g., FriendsPanel) and rework or discard others (e.g., ContextPanel if it duplicates functionality).

**Option C: Rework from scratch** — If the UI redesign doesn't meet quality standards, build your own version using theirs as reference. Keep the concepts (3-column layout, buddy list) but improve the implementation.

Whichever you choose, the end result should be a single `track-b/*` branch with both B2.1 and the UI redesign integrated cleanly.

---

## Priority 1 — B2.1 Verification & Polish

Before moving on, verify your ProfileBuilder work:

1. Run `pnpm test` — confirm all 80 claimed tests pass
2. Run `pnpm tauri dev` — open ProfileBuilderScreen, drag blocks, save, verify on ProfileScreen
3. Check: Does the sandboxed iframe rendering work correctly?
4. Check: Is DOMPurify sanitization applied to the saved HTML?
5. Fix any issues found

---

## Priority 2 — Next Track B Item

After B2.1 and UI redesign are integrated, pick the next item from ROADMAP.md Track B. Good candidates that don't have cross-track blockers:

- **B2.4 Theme Widgets** — Music player, guestbook (placeholder blocks already exist in your ProfileBuilder)
- **B3.1 Mobile-First Responsive** — Ensure the new 3-column layout works well on mobile
- **B4.1 Archive Import Wizard** — Drag-and-drop ZIP import screen (existing `import.rs` backend handles parsing)

Items that ARE blocked on Backend agent (A1.1):
- B2.6 Dual-Format Serving — needs AP Actor
- B9.1 AP-Federated Groups — needs AP support
- B9.3 WASM Web Portal — needs AP hosting

---

## ⚠️ Merge Conflict Warning

Multiple branches touch the same files:
- `App.css` — ALL three agent branches modified this
- `lib.rs` — ALL three branches
- `types/index.ts` — Content + your branch
- `App.tsx` — Backend UI redesign + your branch

When merging, take care with these files. The CSS conflicts will be the messiest — different agents added different sections. Use git's merge tool and verify visually after resolving.

---

## Files You Will Review (from track-b/ui-redesign)
- `src/components/TopBar.tsx`
- `src/components/ContextPanel.tsx`
- `src/components/FriendsPanel.tsx`
- `src/components/AppLayout.tsx`
- `src/App.css`
- `src/App.tsx`
- `src/screens/FeedScreen.tsx`
- `src/screens/ChatScreen.tsx`

## Files You Own
- `src/screens/ProfileBuilderScreen.tsx`
- `src/screens/ProfileScreen.tsx`
- `src/components/TopBar.tsx` (after adoption)
- `src/components/FriendsPanel.tsx` (after adoption)
- `src/components/AppLayout.tsx`
- `src/App.css` (shared — coordinate on structure)
- `src/store/index.ts`

## Files You Must NOT Touch
- `src/components/ComposeBox.tsx` (Track C)
- `src/components/ArticleEditor.tsx` (Track C)
- `src/components/RichText.tsx` (Track C)
- `src/components/AutocompleteDropdown.tsx` (Track C)
- `bytescape/src-tauri/src/oauth.rs` (Backend agent)
- `bytescape/src-tauri/src/connectors/` (Backend agent)
