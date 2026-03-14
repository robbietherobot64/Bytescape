# Content Agent Kickoff: C6.1 CRDT Storage Layer

## Context — What's Changed Since Your Last Session

### Your C1 work was delivered ✅
Your commit `38cf4c2` (C1.1b/C1.2b/C1.3b — Facet rendering + autocomplete frontend) is on `track-c/c1-facet-frontend`. The PM has not yet verified this branch in detail, but the commit message and file stats look complete. **Do not re-do this work.**

### C6.1 Backend Already Partially Exists ⚠️
The Backend agent's GAP-4 commit (`9c60417`, now on `main`) **created collab.rs and commands/collab.rs** as part of fixing build errors. This means:
- `src-tauri/src/collab.rs` already exists (~324 lines)
- `src-tauri/src/commands/collab.rs` already exists (~94 lines)
- `COLLAB_DOCS_TABLE` is registered in `lib.rs`
- `collab_doc_id: Option<String>` is already on `BytescapeItem` in `models/mod.rs`
- The `yrs` crate may or may not be in `Cargo.toml` — **check first**

**Do NOT recreate these files from scratch.** Read them, assess their quality, and build on top of them. If they're incomplete or wrong, fix them in place.

### Branch State
Your branch `track-c/c1-facet-frontend` forked from `c4e8123` (the A0 merge). `main` has since moved to `bac702e` (6 more GAP commits). You need to merge `main` into your branch before starting C6.1 work:

```bash
git checkout track-c/c1-facet-frontend
git merge main
# Resolve any conflicts in lib.rs, oauth.rs, import.rs
# These files were modified by both you (GAP-4 cherry-pick) and main (GAP-1/2/3/6/7)
```

### Cross-Track UI Redesign
The Backend agent (PM-authorized) created a UI redesign on `track-b/ui-redesign` — new `TopBar`, `ContextPanel`, `FriendsPanel`, 3-column layout. This is on a separate branch and **does not affect your work**. You don't need to worry about it.

---

## Priority 1 — C6.1: CRDT Storage Layer

### Step 1: Assess Existing Backend Code

Read these files that the Backend agent created:
- `src-tauri/src/collab.rs` — check if it has `yrs` integration, `create_collab_doc`, `apply_collab_update`, `get_collab_state`, `snapshot_collab_to_item`
- `src-tauri/src/commands/collab.rs` — check the Tauri command wrappers
- `src-tauri/Cargo.toml` — check if `yrs` crate is listed

If the backend code is:
- **Complete and correct** → move straight to frontend wiring
- **Skeleton/stub only** → fill in the implementation
- **Broken or wrong** → fix it

### Step 2: Backend — Ensure Full `collab.rs` Implementation

The collab backend must provide:

```rust
// Create a new Yjs document, store initial state in redb
pub fn create_collab_doc(doc_id: &str, db: &Database) -> Result<Vec<u8>, String>

// Apply a Yjs update (from frontend or peer), merge into stored state
pub fn apply_collab_update(doc_id: &str, update: &[u8], db: &Database) -> Result<Vec<u8>, String>

// Get current Yjs state vector for sync
pub fn get_collab_state(doc_id: &str, db: &Database) -> Result<Vec<u8>, String>

// Snapshot the Yjs doc's current text content into a BytescapeItem
pub fn snapshot_collab_to_item(doc_id: &str, item_id: &str, db: &Database) -> Result<(), String>
```

The `yrs` crate usage should look like:
```rust
use yrs::{Doc, Text, Transact, ReadTxn, StateVector, Update};
```

Ensure `COLLAB_DOCS_TABLE` stores binary Yjs state (key: doc_id string, value: `&[u8]`).

### Step 3: Frontend — Wire Tiptap Collaboration

**File:** `src/components/ArticleEditor.tsx`

The Tiptap collaboration extensions are **already installed** (`@tiptap/extension-collaboration`, `@tiptap/extension-collaboration-cursor`). Wire them:

1. Install `yjs` npm package if not already present:
   ```bash
   cd bytescape && pnpm add yjs
   ```

2. When `ArticleEditor` opens in collaborative mode:
   ```typescript
   import * as Y from 'yjs';
   import Collaboration from '@tiptap/extension-collaboration';

   const ydoc = new Y.Doc();

   // Load existing state from backend
   const state = await invoke<number[]>('get_collab_state', { docId });
   if (state.length > 0) {
     Y.applyUpdate(ydoc, new Uint8Array(state));
   }

   // Wire to Tiptap
   const editor = useEditor({
     extensions: [
       StarterKit.configure({ history: false }), // Disable built-in history — Yjs handles undo
       Collaboration.configure({ document: ydoc }),
     ],
   });

   // On every Yjs update, persist to backend
   ydoc.on('update', async (update: Uint8Array) => {
     await invoke('apply_collab_update', {
       docId,
       update: Array.from(update),
     });
   });
   ```

3. Add a mode toggle or prop: `collaborative?: boolean`. When false, use the existing non-CRDT editor. When true, use the Yjs-backed editor.

4. **Test the round-trip:** Edit → Yjs update → Tauri command → `yrs` merge → redb persist → reload editor → content preserved.

### Step 4: Snapshot Integration

When the user publishes or saves an article from the collaborative editor:
1. Call `snapshot_collab_to_item` to extract the current text from the Yjs doc
2. Store it as a `BytescapeItem` with `BytescapeType::Article`
3. Link it via `collab_doc_id`

**DO NOT wire P2P sync yet** — that's C6.3. This is single-user CRDT-backed editing only.

---

## Priority 2 — C1 Branch Merge Prep (if time permits)

Your C1 branch will need to merge to main. Prepare by:
1. Running `pnpm test` on your branch to verify all 128 claimed tests pass
2. Running `pnpm tauri dev` to verify build
3. Rebasing on latest `main` to minimize merge conflicts
4. Checking for any conflicts in `App.css`, `lib.rs`, `types/index.ts`

---

## Files You Will Modify
- `src-tauri/src/collab.rs` (assess + complete)
- `src-tauri/src/commands/collab.rs` (assess + complete)
- `src-tauri/src/lib.rs` (register commands if not already done)
- `src-tauri/Cargo.toml` (add `yrs` if missing)
- `src/components/ArticleEditor.tsx` (Yjs wiring)
- `bytescape/package.json` (add `yjs` if missing)
- `ROADMAP.md` (mark C6.1 progress)

## Files You Must NOT Touch
- `src/screens/LoginScreen.tsx` (Backend agent — OAuth UI)
- `src/hooks/useTheme.ts` (Frontend agent)
- `src/components/ThemePicker.tsx` (Frontend agent)
- `bytescape/src-tauri/src/oauth.rs` (Backend agent)
- `src/screens/ProfileBuilderScreen.tsx` (Frontend agent)
- `src/components/TopBar.tsx` (Track B UI redesign — separate branch)
- `src/components/FriendsPanel.tsx` (Track B UI redesign — separate branch)
- `src/components/ContextPanel.tsx` (Track B UI redesign — separate branch)
