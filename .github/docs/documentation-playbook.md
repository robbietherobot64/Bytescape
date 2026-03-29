# Documentation Pass — Execution Playbook

> Step-by-step instructions for working through the 14-batch documentation pass.
> Each batch can be completed in one agent session.

---

## General Workflow Per Batch

```
1. Read the batch file list from documentation-tracker.md
2. For each file in the batch:
   a. Read the full file
   b. Understand what it does (grep for imports, exported functions, key patterns)
   c. Add a file-level header comment following commenting-standards.md
   d. Add doc comments to all exported functions/types/components
   e. Add inline "why" comments on non-obvious logic
   f. Add section dividers if the file is 200+ lines
3. After all files in the batch are commented:
   a. Run `npx tsc --noEmit` (for TypeScript files)
   b. Run `cargo check` (for Rust files)
   c. Run `pnpm test -- --run`
4. Commit: `docs(batch-N): document [layer name]`
5. Update documentation-tracker.md — change ⬜ → ✅ for completed files
```

---

## Batch-Specific Guidance

### Batch 1 — Core Types & Models (6 files · ~1,900 lines)
**Estimated effort**: 1 session (light — mostly adding field-level comments to type defs)

**Key files**:
- `src/types/index.ts` — Already has *some* inline comments. Needs: file header, "RELATED FILES" section, and doc comments on every interface explaining what it represents and where it's used.
- `src-tauri/src/models/mod.rs` — Has zero documentation. Needs: module doc comment explaining this is the data model layer, plus doc comments on every struct/enum.
- `src-tauri/src/models/groups.rs` — Group-specific models.
- `src-tauri/src/connectors/mod.rs` — The `Source` enum and connector traits.
- `src-tauri/src/error.rs` — Error types.
- `src-tauri/src/activitypub/types.rs` — ActivityPub protocol types.

**Watch out for**: The TypeScript types in `index.ts` must mirror the Rust types in `models/mod.rs`. Document this relationship explicitly.

---

### Batch 2 — Database Layer (6 files · ~2,300 lines)
**Estimated effort**: 1 session

**Key files**:
- `src-tauri/src/db/mod.rs` — Table constant definitions + `DbManager` struct. Explain what each table stores and why it's a `TableDefinition<&str, &str>` (key-value with JSON-serialized values).
- `src-tauri/src/db/groups.rs` — Largest DB file (940 lines). Has group CRUD, membership management, invitations. Will need section dividers.
- `src-tauri/src/db/vault.rs` — Vault operations. Explain the header cache optimization.

**Watch out for**: redb's transactional model (begin_write → write_txn → commit). Explain it once in `db/mod.rs` and reference back in other files.

---

### Batch 3 — Backend Core (6 files · ~3,700 lines)
**Estimated effort**: 1–2 sessions (oauth.rs and keychain_encrypted.rs are dense)

**Key files**:
- `src-tauri/src/lib.rs` — The app entry point. Has `AppState`, command registration, P2P listener. Critical "map of the backend" file.
- `src-tauri/src/oauth.rs` — OAuth PKCE + DPoP flow. Complex crypto — needs careful "here's what's happening in plain English" comments.
- `src-tauri/src/keychain_encrypted.rs` — Credential encryption. Same — explain the crypto concepts simply.

**Watch out for**: `lib.rs` has the command registration block (`.invoke_handler(tauri::generate_handler![...])`) — document what each registered command does.

---

### Batch 4 — Platform Connectors (4 files · ~6,600 lines)
**Estimated effort**: 2 sessions (these are large files with lots of parsing logic)

**Key files**:
- `src-tauri/src/connectors/facebook.rs` — 2,676 lines of HTML parsing. Needs section dividers badly. Explain the Facebook export format and how each section is parsed.
- `src-tauri/src/connectors/bluesky.rs` — 2,093 lines. AT Protocol API client. Explain the API patterns, token refresh, and data normalization.

**Watch out for**: These files have many helper functions that parse specific format quirks. Group them into sections (e.g., "Post parsing", "Photo extraction", "Date parsing", "Error handling").

---

### Batch 5 — P2P, Sync & Real-time (8 files · ~3,200 lines)
**Estimated effort**: 1–2 sessions

**Key concepts to explain**:
- Iroh Docs = CRDT-based document sync (like Google Docs but peer-to-peer)
- Iroh Blobs = content-addressed file sharing (each file has a hash-based ID)
- Iroh Gossip = real-time presence/messaging between peers
- The protocol message format and how peers negotiate connections

---

### Batch 6 — ActivityPub Federation (9 files · ~2,900 lines)
**Estimated effort**: 1 session

**Key concepts to explain**:
- ActivityPub = the standard that Mastodon, Lemmy, Pixelfed all speak
- Inbox = messages coming IN from other servers (someone followed you, liked your post)
- Outbox = messages going OUT to other servers (you published a post, followed someone)
- HTTP Signatures = how servers prove messages are authentic (not forged)
- WebFinger = how you look up `@user@server.com` to find their profile

---

### Batch 7 — Tauri Commands (20 files · ~10,000 lines)
**Estimated effort**: 3 sessions (largest batch by file count and line count)

**Strategy**: Split into sub-batches:
- 7a: auth, import, media (3 files, ~5,100 lines) — largest commands
- 7b: groups, mastodon, peers, vault, posting, browser (6 files, ~4,600 lines) — mid-size
- 7c: remaining 11 smaller command files (~2,400 lines)

**Key pattern**: Every `#[tauri::command]` function is callable from the frontend. Document:
1. What the frontend calls it for
2. What it does internally
3. What it returns
4. What errors can occur

---

### Batch 8 — Frontend Foundation (8 files · ~2,600 lines)
**Estimated effort**: 1 session

**Key files**:
- `src/App.tsx` — The entire app shell. Tab management, screen routing, sidebar. Long file — needs section dividers.
- `src/store/index.ts` — Zustand global state. Document every store slice.
- `src/providers/DataProvider.ts` & `TauriDataProvider.ts` — The abstraction layer. Explain why it exists (testability, future alternative backends).

---

### Batch 9 — Frontend Hooks (7 files · ~2,300 lines)
**Estimated effort**: 1 session

`useUnifiedFeed.ts` is the most important — it's the heart of the feed pipeline. Document the full data flow: which sources are queried, how results are merged, how pagination works, how the cache is used.

---

### Batch 10 — Frontend Utils (13 files · ~1,400 lines)
**Estimated effort**: 1 session (these are mostly small utility files)

Quick pass — many of these are self-explanatory. Focus on `adapters.ts` (format conversion), `richtext.ts` (facet rendering), and `VideoPlaybackManager.ts` (video lifecycle).

---

### Batch 11 — Frontend Screens (16 files · ~7,600 lines)
**Estimated effort**: 2 sessions

Each screen file follows a similar pattern:
1. State declarations
2. Data fetching effects
3. Event handlers
4. Render JSX

Use consistent section dividers across all screens so a reader can navigate them identically.

---

### Batches 12–14 — Components, Widgets, Lenses (59 files · ~10,900 lines)
**Estimated effort**: 3 sessions

These are leaf-level UI components. Many are straightforward. Focus documentation effort on:
- `ComposeBox.tsx` (complex Tiptap editor integration)
- `FeedEntryCard.tsx` (the most-rendered component in the app)
- `VideoPlayer.tsx` (already well-commented from recent rewrites)
- `VirtualizedFeedList.tsx` (virtual scrolling logic)
- Widget files (explain the profile builder system)

---

## Estimated Total Effort

| Phase | Batches | Sessions | Lines |
|-------|---------|----------|-------|
| Backend Foundation | 1–3 | 3–4 | ~7,900 |
| Backend Features | 4–7 | 6–8 | ~22,700 |
| Frontend | 8–14 | 8–9 | ~27,500 |
| **Total** | **1–14** | **~17–21** | **~58,100** |

---

## Verification After Each Batch

```bash
cd /home/korendian/Bytescape/bytescape

# TypeScript files
npx tsc --noEmit

# Rust files  
cd src-tauri && cargo check && cd ..

# Tests
pnpm test -- --run
```

Comments should never break compilation or tests, but always verify.
