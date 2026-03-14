# Backend Agent (Track A) — Progress Log

> This document is a running log of work completed by the Backend Agent. Updated after each commit. PM reviews this for status tracking.

---

## 2026-03-14 — A0 OAuth Hardening Complete

### Summary
All A0 OAuth gaps (GAP-1 through GAP-7) are now **closed**. The critical bug causing empty feeds and broken DMs for OAuth users has been fixed. A0 is fully hardened.

### Commits (chronological)

| Commit | Description |
|--------|-------------|
| `c4e8123` | Merge `track-a/a0-bluesky-oauth` into `main` — initial OAuth implementation |
| `9c60417` | **GAP-4**: Wire write operations (`publish_post_v2`, `syndicate_vault_item`) for OAuth via `is_oauth_session()` branching |
| `45c6d04` | **GAP-1**: Persistent OAuth sessions via encrypted `FileSessionStore` (AES-256-GCM). Sessions survive app restarts. |
| `f2f279f` | **GAP-2**: Dynamic OAuth callback port fallback — binds `127.0.0.1:0` when port 28457 is in use |
| `c684d84` | **GAP-6**: Saved feeds and unified feed work for OAuth sessions. `xrpc_get`/`xrpc_post_json` helpers on `AppState` route through OAuth or app-password path |
| `bac702e` | **GAP-3**: `FileSessionStore` unit tests (249 lines, Rust `#[cfg(test)]` module) |
| `22a0435` | **Critical bug fix**: OAuth feeds empty + DMs broken — 3 root causes (see below) |
| `a870548` | Fix "You" feed tab empty for OAuth — added `get_author_feed` command |
| `c29e549` | Fix OAuth chat proxy header poisoning feed XRPC calls — `clone_with_proxy()` isolation |
| `97c6faa` | Remove debug println from OAuth `xrpc_get` |
| `ad6af24` | Run chat import in parallel with feed import |
| `bf56e20` | Fix blank buddy handles from convo members array |
| `ae4bf4b` | Unify chat buddy list with friends panel |

### Critical Bug Fix Details (commit `22a0435`)

**User-reported symptoms:**
1. "The current feed only imports the posts from the user, not anyone else"
2. "The messages don't load at all"

**Root causes identified:**

| Bug | Description | Impact |
|-----|-------------|--------|
| **Missing DPoP Auth Header** | `authenticated_get`/`authenticated_post` in `oauth.rs` called `send_http()` without adding `Authorization: DPoP {token}` header. The atrium `DpopClient::send_http` reads this header to compute the DPoP proof's `ath` (access token hash) claim. Without it, requests were effectively unauthenticated. | PDS returns 401 → all feed APIs return empty |
| **Wrong PDS URL** | `get_pds_base()` returned raw PDS host for OAuth (e.g. `https://bsky.network`) but `{host}/xrpc` for app-password. XRPC helpers append `/{endpoint}`, producing `https://bsky.network/app.bsky.feed.getTimeline` instead of `.../xrpc/app.bsky.feed.getTimeline`. | All XRPC calls hit 404 |
| **No Chat Proxy Path** | `import_vault_messages` returned zeros for OAuth; `send_bsky_dm` returned error. `ChatClient` only supports Bearer auth — no DPoP + `atproto-proxy` path existed. | DMs completely non-functional for OAuth |

**Fixes applied across 5 files:**

- **`oauth.rs`**: Added `Authorization: DPoP {token}` to all `send_http` calls. Shared `FileSessionStore` via `Arc` so `OAuthManager` reads tokens the `OAuthClient` refreshes. Added `current_access_token()`, `authenticated_chat_get()`, `authenticated_chat_post()`.
- **`lib.rs`**: Fixed `get_pds_base()` to append `/xrpc` for OAuth. Added `xrpc_chat_get`/`xrpc_chat_post` helpers with `atproto-proxy` header.
- **`import.rs`**: Wired `import_vault_messages` OAuth path (list convos → get messages → save). Wired `send_bsky_dm` OAuth path.
- **`auth.rs`**: Eagerly restore OAuth session in `check_auth_status` so API calls work immediately after frontend receives auth status.
- **`feed_aggregation.rs`**: Added error logging for timeline/custom feed fetches (was silently swallowing errors).

### A0 Gap Status — All Closed ✅

| Gap | Status | Description |
|-----|--------|-------------|
| GAP-1 | ✅ Done | Persistent OAuth sessions (encrypted FileSessionStore) |
| GAP-2 | ✅ Done | Dynamic callback port fallback |
| GAP-3 | ✅ Done | OAuth unit tests |
| GAP-4 | ✅ Done | Write operations wired for OAuth |
| GAP-5 | ✅ Done | DMs wired for OAuth (`transition:chat.bsky` scope, chat proxy) |
| GAP-6 | ✅ Done | Saved feeds / unified feed for OAuth |
| GAP-7 | ✅ Done | Verified transitive deps (`p256`, `urlencoding`) |

### Post-GAP Bug Fixes

| Issue | Fix | Commit |
|-------|-----|--------|
| "You" tab empty for OAuth | Added `get_author_feed` merging Bluesky + vault posts | `a870548` |
| Chat proxy header leaked into feed calls | `clone_with_proxy()` creates isolated session | `c29e549` |
| Chat import blocked feed loading | Parallel `import_vault_messages` alongside `import_bluesky_posts` | `ad6af24` |
| Buddy handles blank in chat sidebar | Resolve handles from convo members array | `bf56e20` |
| FriendsPanel + buddy list redundant | Unified into single component | `ae4bf4b` |

---

## Next Up: A1.1 — ActivityPub Actor & Inbox/Outbox

**Status:** Not started. Blocked on A0 completion (now unblocked).

**Plan:**
1. Create `src-tauri/src/activitypub/` module
2. Define AP Actor type at `bytescape.social/@{username}`
3. Implement HTTP signature verification for inbox POST
4. Add outbox GET endpoint
5. Add `ActivityPub` variant to `Source` enum
6. This unlocks Track B items: B2.6, B9.1, B9.3

---

## Build & Test Status

- **Rust build**: ✅ Compiles (warnings only, no errors)
- **Frontend tests**: ✅ 151/151 passing
- **Branch**: `track-c/c1-facet-frontend` (merged `main` + `track-b/ui-redesign`)
- **Last verified**: 2026-03-14

---

## Content Agent (Track C) — C6.1 CRDT Storage Layer

### Summary
C6.1 CRDT Storage Layer is **complete**. Local-only Yjs-backed collaborative editing for Articles is fully wired end-to-end: Rust backend (yrs crate) → Tauri commands → React hook (useCollabDoc) → Tiptap Collaboration extension in ArticleEditor.

### Implementation Details

**Backend (already existed from GAP-4, modified for Tiptap compatibility):**
- `collab.rs`: 6 methods on DbManager — `create_collab_doc`, `apply_collab_update`, `get_collab_state`, `snapshot_collab_to_item`, `delete_collab_doc`, `list_collab_docs`
- `snapshot_collab_to_item` changed to accept `content: &str` parameter from frontend (Tiptap uses Y.XmlFragment, not Y.Text — cannot extract plain text in Rust)
- Removed unused `GetString` import after the snapshot change
- `commands/collab.rs`: 6 Tauri command wrappers, `snapshot_collab_to_item` updated with `content: String` param
- All commands registered in `lib.rs` invoke_handler

**Frontend (new):**
- `src/hooks/useCollabDoc.ts` (216 lines) — React hook managing Y.Doc lifecycle:
  - Creates Y.Doc, loads/creates collab doc via Tauri invoke
  - Subscribes to Yjs update events with 300ms debounced batching
  - Persists updates to backend via `apply_collab_update`
  - Provides `snapshot(content)` function for saving to BytescapeItem
  - Cleanup: Y.Doc.destroy() on unmount
- `src/components/ArticleEditor.tsx` — Enhanced with collaborative mode:
  - New props: `collaborative?: boolean`, `collabDocId?: string`, `itemId?: string`
  - Collaborative mode: `StarterKit.configure({ undoRedo: false })` + `Collaboration.configure({ document: ydoc, field: 'content' })`
  - Loading/error states for collab initialization
  - `collabDocId` included in `ArticleEditorData` onChange output
  - Non-collaborative mode: zero behavior change

**Tests (18 new, 151 total):**
- `useCollabDoc` hook: creation, loading, error handling, update persistence, snapshot
- Yjs operations: XmlFragment, state encoding, merging, update events
- ArticleEditor: collaborative loading/error states, normal mode

### Key Design Decisions
1. **Y.XmlFragment vs Y.Text**: Tiptap's Collaboration extension creates Y.XmlFragment (ProseMirror XML nodes), not Y.Text. Backend's `snapshot_collab_to_item` was modified to accept rendered content from frontend instead of extracting from Yjs doc.
2. **Debounced persistence**: Updates are batched at 300ms intervals using `Y.mergeUpdates()` before sending to backend.
3. **No P2P sync**: C6.1 is local-only. P2P real-time sync is C6.3 (requires iroh-gossip integration).

### Commits
| Commit | Description |
|--------|-------------|
| `cc85d0f` | Merge main into track-c (resolve lib.rs conflicts) |
| `80a5407` | Merge track-b/ui-redesign into track-c (bring in C6.1 + Profile Builder work) |
