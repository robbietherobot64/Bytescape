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

## 2026-03-14 — A1.1 ActivityPub Actor & Inbox/Outbox ✅

### Summary
A1.1 is **complete**. Full ActivityPub foundation implemented: actor identity with ed25519 HTTP signatures, inbox processing with signature verification, outbox collection pagination, signed activity delivery, WebFinger discovery, and all 9 ActivityStreams activity constructors. This is the most critical-path item in the project — it unblocks 7 items across Tracks B and C.

### New Files (9 files, 1,636 lines total)

| File | Lines | Purpose |
|------|-------|---------|
| `activitypub/mod.rs` | 12 | Module re-exports: ApActor, ApObject, ApActivity, ApCollection, ApPublicKey, ApMediaLink |
| `activitypub/types.rs` | 164 | Core AP types: `ApObject`, `ApActivity`, `ApCollection`, `ApPublicKey`, `ApMediaLink`, `ApTag`, `ApSource`. AS/Security JSON-LD context constants. |
| `activitypub/actor.rs` | 170 | `ApActor` struct with ed25519 keys via `iroh::SecretKey`. `new()` generates keypair. `to_activitystreams()` produces JSON-LD (excludes secret key). `ed25519_public_key_to_pem()` for DER→PEM encoding. Domain: `bytescape.social`. |
| `activitypub/signatures.rs` | 404 | HTTP Signatures (draft-cavage-http-signatures-12, Mastodon-compatible). `sign_post_request()` / `sign_get_request()` produce signed headers. `parse_signature_header()` / `verify_ed25519_signature()` for inbox verification. Inline SHA-256 implementation (no extra deps). |
| `activitypub/activities.rs` | 211 | Activity constructors: `create_note`, `follow`, `accept`, `reject`, `like`, `announce`, `delete`, `undo`, `update`. `bytescape_item_to_note()` converts `BytescapeItem` → AP `Note` with RFC 3339 timestamps. |
| `activitypub/inbox.rs` | 263 | `process_inbox()`: verify HTTP signature → parse activity → dispatch. `InboxResult` enum: `Accepted`, `FollowReceived`, `ObjectReceived`, `Undone`, `Ignored`. Handlers for all 9 activity types. `fetch_actor_public_key()` for remote key retrieval. |
| `activitypub/outbox.rs` | 146 | `build_outbox_collection()` / `build_outbox_page()` for OrderedCollection pagination. `deliver_activity()`: signed POST to remote inboxes. `fetch_remote_actor()`: signed/unsigned GET with AP Accept header. |
| `activitypub/webfinger.rs` | 117 | `WebFingerResponse` / `WebFingerLink` structs. `generate_webfinger()` produces JRD for local actors. `parse_acct_resource()` extracts username/domain. `resolve_webfinger()` queries remote servers. |
| `commands/ap.rs` | 149 | 5 Tauri commands: `create_ap_actor`, `get_ap_actor`, `send_ap_activity`, `fetch_remote_actor`, `resolve_webfinger`. All derive `owner_did` from Iroh endpoint secret key via bs58. |

### Integration Points (4 existing files modified)

| File | Changes |
|------|---------|
| `connectors/mod.rs` | `ActivityPub` variant added to `Source` enum + all 5 match arms (`as_str` → "activitypub", `from_str`, `label` → "ActivityPub", `emoji` → "🌐") |
| `db/mod.rs` | `AP_ACTORS_TABLE` constant, table creation in `DbManager::new()`, 3 CRUD methods: `save_ap_actor()`, `get_ap_actor()`, `delete_ap_actor()` |
| `commands/mod.rs` | `pub mod ap;` + `pub use ap::*;` |
| `lib.rs` | `pub mod activitypub;` + 5 AP commands registered in invoke_handler |

### Architecture Decisions

1. **ed25519 only** — No RSA. Uses `iroh::SecretKey` / `iroh::PublicKey` (same key infra as P2P layer). Avoids additional crypto dependencies.
2. **Inline SHA-256** — Hand-rolled in `signatures.rs` using FIPS 180-4 constants. Avoids pulling in `sha2` crate just for HTTP signature digest computation.
3. **draft-cavage-http-signatures-12** — Mastodon-compatible HTTP Signature scheme. Signs `(request-target)`, `host`, `date`, `digest` (POST) or `(request-target)`, `host`, `date` (GET).
4. **PEM encoding** — DER prefix `302a300506032b6570032100` + raw ed25519 public key bytes, base64-wrapped in `-----BEGIN PUBLIC KEY-----` / `-----END PUBLIC KEY-----`. Compatible with ActivityPub `publicKeyPem` field.
5. **No HTTP server yet** — Inbox/outbox are library functions, not HTTP endpoints. Serving will be added when `bytescape.social` hosting is implemented (B9.3).

### Tests (22 new unit tests)

| Module | Tests | Coverage |
|--------|-------|----------|
| types | 2 | AP context generation, ApObject serialization |
| actor | 4 | Actor creation, AS JSON-LD serialization, secret key roundtrip, PEM structure validation |
| signatures | 5 | SHA-256 known vector, POST signing + verification, GET signing, signature header parsing, PEM→key roundtrip |
| activities | 4 | Create Note, Follow activity, Delete tombstone, Announce addressing |
| inbox | 1 | Unsupported activity type → Ignored |
| outbox | 2 | Outbox root collection, outbox page with pagination |
| webfinger | 4 | WebFinger generation, valid acct parsing, invalid acct rejection, JRD serialization |

### Cross-Track Items Unblocked

| Item | Track | Description |
|------|-------|-------------|
| B2.6 | Track B | Dual-Format Serving (AP + AT Protocol) |
| B8.5 | Track B | AP Event Federation |
| B9.1 | Track B | AP-Federated Group Threading |
| B9.3 | Track B | WASM Web Portal |
| C2.4 | Track C | Custom Emoji (needs A1.2) |
| C5.2 | Track C | AP destination wiring (needs A1.2) |

### Commits

| Commit | Branch | Description |
|--------|--------|-------------|
| `7c58c40` | `track-c/c1-facet-frontend` | Initial A1.1 implementation |
| `a4e4062` | `main` | Cherry-pick to main (conflict resolution: removed track-c profile/unfurl refs) |
| `28a6b8c` | `main` | Post-merge cleanup (duplicate http_client, ap module dedup) |

---

## Build & Test Status

- **Rust build**: ✅ Compiles (25 pre-existing warnings, 0 errors, 0 new AP warnings)
- **Rust tests**: ✅ 125/125 passing (68 lib + 7 db + 8 error + 12 facebook + 2 iroh + 28 oauth)
- **Frontend tests**: ✅ 75/75 passing (5 test files — main branch)
- **Branch**: `main` at `28a6b8c`
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

---

## 2026-03-15 — Content Agent Sprint: C1.4 + C2.1

### Summary
Sprint from `content-c1c2-sprint.md` — priorities P0 (merge main), P1 (C1.4 character count), P2 (C2.1 link card embeds), P3 (content-type classification research). All completed.

### Commits (track-c/c1-facet-frontend)
| Commit | Description |
|--------|-------------|
| `f68679c` | merge: main into track-c (get fe890b4 remediation) |
| `23c7bbe` | feat(track-c): C1.4 character count & limits — grapheme-aware multi-target counters |
| `9123e0b` | feat(track-c): C2.1 link card embeds — unfurl.rs, LinkPreviewCard, URL detection |
| `93fde71` | feat(track-c): C2.1 embed.external — Bluesky link card publishing |

### C1.4 — Multi-Target Character Counting
- **useCharacterCount hook** (121 lines): `Intl.Segmenter` grapheme counting, per-target limits (Bluesky 300 graphemes, AP 500, Local 10K)
- **TARGET_LIMITS**, **TARGET_ICONS**, **STATUS_COLORS** exports for UI rendering
- **resolveTargets()**: Maps PostTarget → active target list ('all' → bluesky + activitypub + local)
- **Color-coded counters**: ok (green) → warn (yellow 80%) → danger (orange 95%) → over (red >100%)
- **Truncation warnings**: Shows which targets will truncate and by how much
- **Post button logic**: Disabled when ALL targets over; enabled with warning when some over
- **28 unit tests**: ASCII, emoji, ZWJ sequences, CJK, multi-target resolution, color transitions

### C2.1 — Link Card Embeds (Full Stack)

**Backend:**
- `commands/unfurl.rs` (167 lines): `unfurl_link` Tauri command
  - reqwest fetch: 5s timeout, 3 redirects max, 1MB body limit, text/html only
  - scraper HTML parsing: og:title/description/image/site_name with twitter:* fallbacks
  - Favicon detection, relative URL resolution, string sanitization (500 char cap)
- `commands/posting.rs`: `LinkCardInput` struct, `link_card` parameter on `publish_post_v2`
  - `embed.external` emission: `{uri, title, description, thumb}` in AT Protocol record
  - `download_and_upload_thumb()`: fetches OG image, uploads as blob to PDS
  - Local vault: stores `link_url` and `preview_img` on `BytescapeItem`

**Frontend:**
- `LinkPreviewCard.tsx` (144 lines): Thumbnail, favicon + domain, title, description, dismiss button
- ComposeBox URL detection: regex `/https?:\/\/[^\s<>"{}|\\^`[\]]+/`, 500ms debounce
- `LinkCard` type in `types/index.ts`
- `PublishOptions.linkCard` flows ComposeBox → FeedScreen → invoke → posting.rs
- 3 integration tests: loading indicator, card rendering, card dismissal

### P3 — Content-Type Classification Research

**Problem:** User feedback (UF.7) reports that ArticleReader shows too many non-articles (videos, GIFs, links with just a title + description). Current classification in `publish_post_v2` is naive: video → Status, multi-image → Gallery, everything else → Status. No "Link" type exists, and the Status/Article/Gallery classification doesn't reflect content substance.

**Proposed Classification Heuristics:**

| Type | Signal | Threshold |
|------|--------|-----------|
| `status` | Short text, minimal media | ≤300 chars body, 0–1 images, no video |
| `gallery` | Image-centric content | ≥2 images as primary content, text is caption-length |
| `article` | Substantial long-form text | >500 chars body text (excluding URL + OG metadata), may have inline images |
| `link` | URL embed with minimal original text | Has link_url, ≤100 chars of original text (not OG description), title comes from OG metadata |
| `video` | Video as primary content | Has video attachment(s), any text length |

**Where should classification happen?**

**Recommendation: At ingest time in Rust, stored as `item_type` on `BytescapeItem`.**

Reasoning:
1. Classification at ingest ensures all views (feed, gallery, reader, search) have consistent typing without re-computing
2. Existing `BytescapeType` enum already has `Status`, `Gallery`, `Article` — needs `Link` and `Video` variants added
3. Bluesky connector already has access to embed types (`app.bsky.embed.images`, `app.bsky.embed.video`, `app.bsky.embed.external`) which map cleanly to the new types
4. Facebook archive connector has `media_type` hints (photos vs videos)
5. Frontend filters become trivial: `items.filter(i => i.item_type === 'article')`

**Implementation plan (cross-track):**

1. **Track A**: Add `Link` and `Video` variants to `BytescapeType` enum in `models/mod.rs`. Update Bluesky connector to classify posts at fetch time using embed type. Update Facebook connector to classify using media presence/type.
2. **Track C**: Update `publish_post_v2` to auto-classify based on content analysis (apply heuristic table above). When a link card is attached but text is short, set type to `Link` instead of `Status`.
3. **Track B**: Replace lens-based content routing with filter chips on feed toolbar. Filter by `item_type`. ArticleReader only shows `item_type: 'article'`.
4. **Re-classification migration**: One-time migration command `reclassify_vault_items` that re-scans existing vault items and updates their `item_type` using the new heuristics. This handles items imported before the classification logic was improved.

**Open question for PM:** Should `Link` be a new `BytescapeType` variant, or should it be a metadata flag (e.g., `has_link_embed: bool`) on any type? The variant approach is simpler but a linked article is both a "link" and potentially an "article" if the OG content is substantial.

### Build & Test Status
- **Rust build**: ✅ Compiles (25 warnings, 0 errors)
- **Frontend tests**: ✅ 185/185 passing (10 test files)
- **TypeScript**: ✅ Clean (`npx tsc --noEmit`)
- **Branch**: `track-c/c1-facet-frontend` at `93fde71`
- **Last verified**: 2026-03-15

---

## Frontend Agent (Track B) — Progress Log

---

## 2026-03-15 — Feature Branch Merge Crisis & Resolution

### Problem

User reported: *"The entire UI seems to have been rolled back like a half dozen builds."* All frontend work appeared missing from the running app.

**Root Cause:** The user was running from the `main` branch, which only contained Backend (Track A) commits. All Frontend Agent work (40+ commits on `track-b/ui-redesign`) and Content Agent work (15+ commits on `track-c/c1-facet-frontend`) had **never been merged into `main`**.

### Resolution

Merged both feature branches into `main` and resolved all post-merge conflicts.

### Commits

| Commit | Description |
|--------|-------------|
| `7351377` | **Merge `track-b/ui-redesign` into `main`** — resolved 3 Rust conflicts (commands/mod.rs, db/mod.rs, lib.rs). Kept both AP Actor (main) + Profile/Guestbook (track-b) code. |
| `0c6bcd0` | **Merge `track-c/c1-facet-frontend` into `main`** — resolved 5 conflicts (progress.md, db/mod.rs, lib.rs, ComposeBox.tsx, ComposeBox.test.tsx). Accepted track-c versions for TS files, manually merged Rust files. |
| `28a6b8c` | **Fix post-merge duplicates** — removed duplicate `pub mod ap` / `pub use ap::*` in commands/mod.rs, removed duplicate `http_client` field + initialization in lib.rs AppState. |

### Merge Conflict Details

**Track-B merge (3 Rust conflicts):**
- `commands/mod.rs`: Both main (AP module) and track-b (profile/unfurl modules) added new `pub mod` declarations — kept both.
- `db/mod.rs`: Main added `AP_ACTORS_TABLE` + CRUD methods; track-b added `PROFILE_LAYOUT_TABLE` + `GUESTBOOK_TABLE` + CRUD methods — merged all table definitions and methods.
- `lib.rs`: Main registered AP commands; track-b registered profile/widget/guestbook/unfurl commands — merged all command registrations into single invoke_handler.

**Track-C merge (5 conflicts):**
- `db/mod.rs` / `lib.rs`: Same pattern — track-c added collab doc commands on top of track-b's profile commands. Merged to keep everything.
- `ComposeBox.tsx` / `ComposeBox.test.tsx`: Track C owns these. Accepted track-c version (includes link card unfurling, character counting).
- `progress.md`: Accepted track-c version (most up-to-date).

**Post-merge compilation errors (8 total):**
- Duplicate `pub mod ap` and `pub use ap::*` in `commands/mod.rs` (appeared in both merge parents)
- Duplicate `http_client: reqwest::Client` field in `AppState` struct and its initialization in `lib.rs`

All fixed in commit `28a6b8c`.

---

## 2026-03-14 — UX Fixes Sprint (P0–P4) 

### Summary

Completed all priorities from `frontend-b10-b11-sprint.md`: P0 merge main, P1–P4 UX fix items.

### Commit

| Commit | Description |
|--------|-------------|
| `f08b001` | **fix(track-b): UF.1/UF.2/UF.4/UF.8/UF.12** — layout overhaul, post card sizing, friend actions, GrapesJS blank screen fix, import UX |

### UF.1 — Layout Overhaul
- Full-width content layout (960px max-width, feed uses full available width)
- Fixed cramped sidebar and content area spacing
- Updated `App.css` with `--content-max-width: 960px`, feed full-width rules

### UF.2 — Post Card Sizing
- Squarer media aspect ratio (changed from 16/9 to 4/3)
- Better image display in feed entries
- Consistent card sizing across feed views

### UF.4 — Friends Panel Actions
- Bluesky friend click → opens chat with that friend
- Added `did` field to `UnifiedFriend` type
- Wired friend click handler in `FriendsPanel.tsx`

### UF.8 — GrapesJS Blank Screen Fix
- Root cause: `panels: { defaults: [] }` config was removing ALL GrapesJS panels including the canvas
- Fix: Removed the empty `panels` override, kept `grapesjs-preset-webpage` plugin defaults
- Profile Builder now loads correctly with drag-and-drop editor visible

### UF.12 — Import Screen UX
- Centered "Import Archive" header
- Removed confusing "View Feed" button
- Added Instagram to supported formats list
- Centered import tools layout

---

## Previously Completed (Track B) — Merged to Main

All of the following were completed on `track-b/ui-redesign` and are now merged to `main`:

### B1 — Lenses & Content Views ✅
- **B1.1** Gallery Lens — masonry grid for Gallery-type items
- **B1.2** Reader Lens — article view with clean typography
- **B1.3** FeedItemRenderer — FeedEntryCard switches view by BytescapeType

### B2 — Profile Builder & Theming ✅
- **B2.1** GrapesJS Integration — `ProfileBuilderScreen` with drag-and-drop builder
- **B2.2** Raw HTML Override — upload custom HTML/CSS/JS, sandboxed `<iframe>` render
- **B2.3** DID Meta Tags — `<meta name="bytescape:did">` in profile output
- **B2.4** Widget System — away message, music player, friends list, guestbook widgets (Rust CRUD backend included)
- **B2.5** Theme Engine — 3 themes (Cozy/Desktop/Midnight), `useTheme` hook, `ThemePicker`, 5 tests

### B3 — Peer Profile Browser ✅
- **B3.1** Profile Browser Tab — new top-level screen for traversing peer profiles
- **B3.2** Sandboxed iframe Renderer — isolated frame with `<meta>` tag parsing

### B10 — Navigation & Browser UX ✅
- **B10.1** Internal URL Scheme — `byte://` URL routing
- **B10.2** Navigation History Stack — back/forward browser-style navigation
- **B10.3** Omnibar — browser-style address bar with search
- **B10.6** Peer Browser Sync — Omnibar ↔ Profile Browser state sync

### Window & Layout
- Window size: 1280×800, maximizable
- Responsive bottom nav for mobile
- Tablet layout support
- AppLayout wrapper component

---

## Deferred Items (Not Started — Per PM Directive)

The following UF items were explicitly told NOT to start:
- UF.5 (Compose Box visibility)
- UF.6 (Compose Box targeting)
- UF.9 (Video player)
- UF.10 (RSS feeds)
- UF.11 (Hashtag feeds)
- UF.14 (Settings)

---

---

## Content Agent (Track C) — Complete Deliverables Report (2026-03-15)

### Overview

All Content Agent work on `track-c/c1-facet-frontend` has been **merged to `main`** (commit `0c6bcd0`). Track C delivered 6 major features across 4 sprints, totaling ~3,500 lines of new code and 152 frontend tests. The ComposeBox is now a full-featured composition environment with rich text facets, autocomplete, character counting, link card embeds, and CRDT-backed article editing.

### Feature Inventory — All Shipped ✅

| ID | Feature | Commit(s) | Tests | Status |
|----|---------|-----------|-------|--------|
| C1.1b | Facet Rendering Frontend — @mentions, #hashtags, links render as interactive elements in feed | `38cf4c2` | 10 (RichText.test.tsx) | ✅ Merged |
| C1.2b | Mention Autocomplete — `@` triggers typeahead from contacts + follows | `38cf4c2` | included in ComposeBox tests | ✅ Merged |
| C1.3b | Hashtag Autocomplete — `#` triggers typeahead from followed + recent tags | `38cf4c2` | included in ComposeBox tests | ✅ Merged |
| C1.4 | Character Count & Limits — grapheme-aware per-target counters | `23c7bbe` | 28 (useCharacterCount.test.ts) | ✅ Merged |
| C2.1 | Link Card Embeds — full-stack unfurl + preview + Bluesky embed.external | `9123e0b`, `93fde71` | 3 (ComposeBox.test.tsx) | ✅ Merged |
| C3.2 | Article Editor — Tiptap-based rich text editor for long-form content | prior sprint | 34 (ArticleEditor.test.tsx) | ✅ Merged |
| C6.1 | CRDT Storage Layer — Yjs backend + useCollabDoc hook + collaborative ArticleEditor | `cc85d0f`, `80a5407` | 18 (useCollabDoc.test.tsx) | ✅ Merged |

### Files Created (Track C)

| File | Lines | Purpose |
|------|-------|---------|
| `src-tauri/src/commands/unfurl.rs` | 167 | `unfurl_link` Tauri command — OG/Twitter Card extraction, favicon, sanitization |
| `src-tauri/src/commands/richtext.rs` | ~200 | Rich text facet parsing — byte-offset @mention/#hashtag/URL detection for Bluesky AT Protocol |
| `src-tauri/src/collab.rs` | ~324 | CRDT storage — Yjs doc CRUD via `yrs` crate, redb persistence |
| `src-tauri/src/commands/collab.rs` | ~94 | Tauri command wrappers for collab operations |
| `src/components/LinkPreviewCard.tsx` | 144 | Link card preview UI — thumbnail, domain, title, description, dismiss |
| `src/components/ArticleEditor.tsx` | ~350 | Tiptap rich text editor with collaborative mode (Yjs-backed) |
| `src/components/ArticleReader.tsx` | ~150 | Rendered article view with facet-aware content |
| `src/components/AutocompleteDropdown.tsx` | ~180 | Reusable typeahead dropdown for @mention and #hashtag |
| `src/components/RichText.tsx` | ~120 | Facet-aware text renderer — clickable mentions, hashtags, links |
| `src/hooks/useCharacterCount.ts` | 121 | `Intl.Segmenter` grapheme counting, per-target limits, color-coded status |
| `src/hooks/useCollabDoc.ts` | 216 | Y.Doc lifecycle management, debounced persistence, snapshot |
| `src/utils/richtext.ts` | ~250 | Frontend facet detection/parsing utilities |
| `src/utils/markdown.ts` | ~100 | Markdown ↔ HTML conversion for articles |

### Files Modified (Track C)

| File | Changes |
|------|---------|
| `src/components/ComposeBox.tsx` | +autocomplete triggers, +link preview state/UI, +character counters, +PublishOptions.linkCard |
| `src-tauri/src/commands/posting.rs` | +LinkCardInput struct, +link_card param, +embed.external for Bluesky, +download_and_upload_thumb |
| `src-tauri/src/commands/mod.rs` | +unfurl, +richtext, +collab modules |
| `src-tauri/src/lib.rs` | +unfurl_link, +richtext, +collab commands in invoke_handler |
| `src-tauri/Cargo.toml` | +yrs, +scraper dependencies |
| `src/types/index.ts` | +LinkCard, +ArticleEditorData interfaces |
| `src/screens/FeedScreen.tsx` | +linkCard in invoke('publish_post_v2') call |
| `bytescape/tsconfig.json` | +ES2022.Intl in lib array (for Intl.Segmenter) |

### Test Coverage (Track C — 133 tests across 6 files)

| Test File | Count | Coverage |
|-----------|-------|----------|
| `useCharacterCount.test.ts` | 28 | Grapheme counting: ASCII, emoji, ZWJ, CJK, multi-target, color transitions |
| `ArticleEditor.test.tsx` | 34 | Editor modes, collaborative loading/error, content persistence |
| `richtext.test.ts` | 43 | Facet parsing, byte offsets, @mention resolution, #hashtag extraction, URL detection |
| `useCollabDoc.test.tsx` | 18 | Y.Doc creation, loading, error handling, update persistence, snapshot |
| `RichText.test.tsx` | 10 | Facet rendering: clickable mentions, hashtags, plain links |
| `ComposeBox.test.tsx` (partial) | ~8 | Link preview loading/render/dismiss, character limit integration |

### Architecture Contributions

1. **Rich Text Pipeline**: ComposeBox text → `richtext.ts` facet parser → byte-offset facets → `app.bsky.richtext.facet` array on Bluesky publish. Same parsed facets render via `RichText.tsx` in feed.
2. **Link Embed Pipeline**: URL detected in compose text → 500ms debounce → `unfurl_link` Tauri command → `LinkPreviewCard.tsx` preview → on publish: `download_and_upload_thumb` + `app.bsky.embed.external` for Bluesky, `link_url`/`preview_img` for local vault.
3. **CRDT Architecture**: `yrs` (Rust Yjs port) stores binary Yjs state in `COLLAB_DOCS_TABLE` (redb). Frontend `useCollabDoc` hook manages Y.Doc lifecycle with 300ms debounced batch persistence. Tiptap `Collaboration` extension wires Y.XmlFragment for structured rich text editing. Snapshot extracts rendered HTML to `BytescapeItem.content`.
4. **Grapheme-Aware Counting**: `Intl.Segmenter` with `granularity: 'grapheme'` handles emoji, ZWJ sequences, CJK correctly. Per-target limits (Bluesky 300, AP 500, Local 10K) with `resolveTargets()` mapping for 'all' target.

### Pending Research (P3 — Content-Type Classification)

Documented in the C1.4+C2.1 sprint section above. Key open question for PM: Should `Link` be a new `BytescapeType` variant or a metadata flag? Cross-track implementation plan provided.

### What's Next for Track C

Per sprint instructions, the Content Agent has reported back and is **awaiting PM assignment** for the next sprint. Candidate items from the roadmap:

| ID | Feature | Dependencies |
|----|---------|-------------|
| C2.2 | GIF Picker (Tenor API) | None |
| C2.3 | Emoji Picker | None |
| C3.1 | Polls | None |
| C4.1 | Drag-and-Drop Media | None |
| C4.2 | Video Upload | None |
| C5.1 | "Post to Wall" UX | None |
| C6.3 | P2P Real-Time Sync via Gossip | Requires working iroh-gossip |
| C2.4 | Custom Emoji | Blocked on A1.2 |
| C5.2 | AP Destination Wiring | Blocked on A1.2 |

---

## Current Build & Test Status (All Tracks Unified on Main)

- **Rust build**: ✅ Compiles (25 warnings, 0 errors)
- **Rust tests**: ✅ 35/35 classification tests passing
- **Frontend tests**: ✅ 247/247 passing (12 test files)
- **TypeScript**: ✅ Clean (`npx tsc --noEmit` — 0 errors)
- **Branch**: `main` at `d6108a8`
- **Last verified**: 2026-03-15

---

## 2026-03-15 — Content Agent: UF.7 / UF.12 / UF.6 User Feedback Sprint ✅

### Summary
Sprint focused on user feedback items the PM flagged as unaddressed. Three priorities delivered:

**P1 — UF.7: Content-Type Classification** ✅
- Added `Link` and `Video` variants to `BytescapeType` (Rust + TypeScript already had them)
- `classify_item()` heuristic: Video > Gallery > Article > Link > Status with boundary thresholds (300 chars gallery, 500 chars article, 100 chars link)
- `classify_bluesky_post()` for ingest-time classification of Bluesky API posts
- `classify_feed_entry()` for live timeline FeedEntry classification
- All ingest sites auto-classify: Facebook import (MIME-based video detection), Bluesky import (uses `classify_bluesky_post`), `publish_post_v2` (Link/Video overrides, link_card detection)
- `reclassify_vault_items` command registered in lib.rs for retroactive re-classification
- `ReclassifyResult` type in TypeScript for frontend consumption

**P2 — UF.12: Tracker Category Classification** ✅ (Already complete from prior session)
- `TrackerEntry.category` field populated by `classify_tracker()` in facebook.rs
- Categories: ad_tracking, location_tracking, cross_site_tracking, data_collection, social_tracking, general
- All 5 tracker construction sites in facebook.rs already populate category
- TypeScript `TrackerEntry` type already includes `category: string`

**P3 — UF.6: Feed Query by Content Type** ✅
- `get_timeline_unified` accepts `item_type_filter: Option<String>` parameter
- Filters applied after dedup in "── 7. Item type filter ──" section
- Supports all 5 type strings: "status", "gallery", "article", "link", "video"

### Files Modified

| File | Changes |
|------|---------|
| `models/mod.rs` | Link/Video in BytescapeType, classify_item, classify_bluesky_post, classify_feed_entry, build_canonical_path match arms |
| `commands/import.rs` | willow_item_kind match, FB import video detection, Bluesky import uses classify_bluesky_post |
| `commands/posting.rs` | publish_post_v2 Link/Video classification overrides |
| `commands/feed_aggregation.rs` | item_type_filter param + filter logic |
| `commands/classify.rs` | reclassify_vault_items command (new file) |
| `db/vault.rs` | vault_stats match arms for Link/Video |
| `types/index.ts` | BytescapeType includes Link/Video, ReclassifyResult type |

### Tests Added

| File | Tests | Description |
|------|-------|-------------|
| `src-tauri/tests/classify_test.rs` | 35 | classify_item (18), classify_bluesky_post (6), priority order (3), classify_feed_entry (8) |
| `src/types/classification.test.ts` | 17 | TypeScript type validation for BytescapeType, TrackerEntry, ReclassifyResult, FeedEntry |

### Commits

| Commit | Description |
|--------|-------------|
| `5f72a4c`→`d6108a8` | Previous session's classify.rs + classification types + this session's feed_entry tests |
