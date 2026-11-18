# Screen Spec: Import (v2)

**Status:** Draft
**Date:** 2026-08-12
**Route:** `/import` (standalone), also accessible from Feed `[+]` dropdown and Onboarding step 2
**Salvage:** MEDIUM — import concept kept, UI reworked for multi-platform + progress streaming
**Satisfies:** FR-1 (Facebook Archive Import), FR-2 (Instagram Archive Import), FR-3 (Threads Archive Import), FR-4 (Social Graph Reconstruction), FR-5 (Archive Vault — import creates vault content)

---

## 1. Overview

The import screen handles GDPR archive uploads from Facebook, Instagram, and Threads. It streams parsing progress via SSE, surfaces errors per-item, and transitions into friend discovery (FR-4) once the archive is processed. The import pipeline writes `com.bytescape.archive.post` records to the user's PDS (AD-01).

### 1.1 Data flow

```
User uploads ZIP → server streams to disk → parser extracts posts/media/contacts
  → posts stored as com.bytescape.archive.post on PDS (batched commits, AD-01)
  → media uploaded as PDS blobs (AD-12)
  → contacts extracted for social graph matching (FR-4)
  → progress streamed to client via SSE
  → import session tracked in app DB (import_sessions table)
```

### 1.2 Dual PDS note

| | Bytescape-hosted | Bluesky-hosted |
|---|---|---|
| Archive records | Direct PDS write (signed commit) | XRPC to bsky.social with OAuth token |
| Media blobs | S3 storage via local PDS | bsky.social blob store |
| Contact matching | App DB lookup | Same |

Frontend is identical — the backend handles the PDS split (AD-01b).

---

## 2. v1 Salvage Assessment

| Component | Verdict | Rationale |
|---|---|---|
| Import upload area | **KEEP** | Drag-and-drop + file picker, good UX |
| Platform selector | **REFINE** | Add Threads option; auto-detect platform from archive structure |
| Progress display | **REWORK** | v1 polled; v2 uses SSE streaming |
| Error display | **REFINE** | Per-item errors with retry; v1 had basic error list |
| Friend discovery post-import | **KEEP** | Transition to FR-4 matching |

---

## 3. Layout

### 3.1 Desktop — Upload + Progress

```
┌──────────────────────────────────────────────────────────────────────┐
│  IMPORT YOUR ARCHIVE                                                 │
│                                                                      │
│  ┌─ Step 1: Choose platform ─────────────────────────────────────┐  │
│  │  [📘 Facebook] [📸 Instagram] [🧵 Threads] [🔍 Auto-detect]  │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌─ Step 2: Upload archive ──────────────────────────────────────┐  │
│  │                                                                │  │
│  │          ┌──────────────────────────────────────┐              │  │
│  │          │                                      │              │  │
│  │          │   📦 Drop your GDPR export ZIP here  │              │  │
│  │          │                                      │              │  │
│  │          │   or [Choose file…]                  │              │  │
│  │          │                                      │              │  │
│  │          │   Max size: 2GB                      │              │  │
│  │          └──────────────────────────────────────┘              │  │
│  │                                                                │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌─ Step 3: Processing ──────────────────────────────────────────┐  │
│  │                                                                │  │
│  │  Platform: Facebook          Status: Processing...             │  │
│  │                                                                │  │
│  │  Posts:    ████████████░░░░░░░░  847 / 2,134                   │  │
│  │  Media:   ██████░░░░░░░░░░░░░░  312 / 1,856                   │  │
│  │  Contacts: ████████████████████  156 / 156 ✓                   │  │
│  │                                                                │  │
│  │  Errors: 3 items (view details ▼)                              │  │
│  │  ┌─────────────────────────────────────────────────────────┐   │  │
│  │  │ ⚠ post_2019_03_15: Missing media reference              │   │  │
│  │  │ ⚠ post_2020_07_22: Unparseable date format              │   │  │
│  │  │ ⚠ photo_album_42: Corrupted image data                  │   │  │
│  │  └─────────────────────────────────────────────────────────┘   │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌─ Step 4: Done! ──────────────────────────────────────────────┐   │
│  │                                                                │  │
│  │  ✅ Import complete!                                           │  │
│  │  2,131 posts · 1,842 photos · 156 contacts imported            │  │
│  │                                                                │  │
│  │  [> FIND FRIENDS]  [> VIEW VAULT]  [> GO TO FEED]             │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

### 3.2 Progress states

The import has four visual states, shown as stacked sections:

1. **Choose platform** — always visible, pre-selected if launched from a specific context
2. **Upload archive** — file picker / drag-and-drop zone
3. **Processing** — progress bars + error list (appears after upload starts)
4. **Done** — summary + next actions (appears when SSE signals completion)

### 3.3 Mobile (≤550px)

Same sections, full-width. Progress bars are wider on mobile. Error list is collapsible.

```
┌──────────────────────────┐
│  IMPORT YOUR ARCHIVE     │
│                          │
│  Platform:               │
│  [📘 FB] [📸 IG] [🧵 TH]│
│                          │
│  ┌──────────────────────┐│
│  │ 📦 Drop ZIP here    ││
│  │ or [Choose file…]   ││
│  └──────────────────────┘│
│                          │
│  Processing...           │
│  Posts: 847/2,134 ████░░ │
│  Media: 312/1,856 ██░░░░ │
│  Errors: 3 (▼)          │
│                          │
│  ✅ Done!                │
│  [Find Friends] [Vault]  │
└──────────────────────────┘
```

---

## 4. Component Breakdown

### 4.1 Import screen shell

**`ImportScreen` (NEW)**
- Purpose: orchestrate the import flow
- Route: `/import`
- State: `platform`, `uploadState: idle | uploading | processing | done | error`, `progress`, `errors`, `result`
- Mounts the four sections, managing transitions between them

### 4.2 Platform selector

**`PlatformSelector` (NEW)**
- Three buttons: Facebook (📘), Instagram (📸), Threads (🧵)
- Optional: Auto-detect (🔍) — server inspects ZIP structure
- Selected state: accent border + highlight
- Props: `selected`, `onSelect`, `disabled` (locked after upload starts)

### 4.3 Upload zone

**`ImportUploadZone` (NEW)**
- Drag-and-drop area with file picker fallback
- Accepts `.zip` files only
- Max size: 2GB (configurable; server uses `DefaultBodyLimit::disable()`)
- Props: `platform`, `onUploadStart`, `disabled`
- Drag state: accent border glow on dragover
- Upload: `POST /api/import/upload` with multipart form data

### 4.4 Progress display

**`ImportProgress` (NEW)**
- SSE connection to `GET /api/import/progress/{session_id}`
- Three progress bars: Posts, Media, Contacts
- Each bar: label, progress text (current/total), percentage fill
- Error section: collapsible list of per-item errors
- Props: `sessionId`, `onComplete`, `onError`
- Reconnect on SSE disconnect (exponential backoff, max 30s)

### 4.5 Error list

**`ImportErrorList` (NEW)**
- Collapsible list of per-item import errors
- Each error: ⚠ icon, item identifier, error description
- Props: `errors: ImportError[]`
- No retry per-item (import continues; failed items are logged)

### 4.6 Completion summary

**`ImportComplete` (NEW)**
- Summary stats: posts imported, media imported, contacts found
- Three CTAs: Find Friends (primary), View Vault, Go to Feed
- Props: `result: ImportResult`, `onFindFriends`, `onViewVault`, `onGoToFeed`

---

## 5. States

### 5.1 Loading

| Surface | Behavior |
|---|---|
| Upload in progress | Progress bar on the upload zone (not the import progress bars) |
| Processing | Real-time SSE progress updates; bars animate smoothly |
| Reconnecting SSE | "Reconnecting..." banner above progress bars |

### 5.2 Error

| Case | Behavior |
|---|---|
| Invalid ZIP | "This doesn't look like a valid {platform} export. Check that you downloaded the full archive." |
| ZIP too large | "Archive exceeds 2GB limit. Try splitting your export." |
| Network error during upload | "Upload failed. Please try again." + retry |
| SSE disconnect | Auto-reconnect with backoff; banner: "Connection lost. Reconnecting..." |
| All items failed | "Import failed — no items could be processed. [View errors]" |
| Server error (5xx) | "Something went wrong. Please try again later." |

### 5.3 Edge cases

- **Duplicate import** — server detects content hashes, skips duplicates. Progress shows "Skipped (duplicate)" count.
- **Partial import** — user closes tab during processing. Import continues server-side. On return, progress resumes from current state.
- **Multiple archives** — user can import from multiple platforms. Each is a separate import session. Previous imports are viewable in the Vault.
- **Corrupted media** — individual media failures don't stop the import. Failed media items are logged as errors.

---

## 6. Interactions

### 6.1 Platform selection
1. Click platform → highlight, update state
2. Auto-detect: server inspects ZIP structure, returns detected platform
3. Platform locked after upload starts (no switching mid-import)

### 6.2 Upload
1. Drag ZIP onto zone → visual feedback (border glow)
2. Drop → upload begins immediately
3. Or: click zone → file picker → select ZIP → upload begins
4. Upload progress shown on the zone itself (not the import progress bars)

### 6.3 SSE progress
1. Upload complete → server starts processing → SSE connection opens
2. Progress events update bars in real-time
3. Error events append to error list
4. Completion event → transition to done state

### 6.4 Post-import actions
1. "Find Friends" → `/onboarding` step 2 (friend discovery) or standalone `/friends/discover`
2. "View Vault" → `/vault` with the imported content
3. "Go to Feed" → `/` (feed)

---

## 7. Responsive Behavior

| Breakpoint | Layout |
|---|---|
| Desktop >900px | Centered content (max 720px), generous padding |
| Tablet 551–900px | Centered (max 600px) |
| Mobile ≤550px | Full-width, 16px padding, progress bars full-width |

---

## 8. Refinement List

- R-1: SSE streaming replaces polling (v1 polled every 2s)
- R-2: Threads platform option (FR-3)
- R-3: Auto-detect platform from ZIP structure
- R-4: Per-item error list with collapsible details
- R-5: Partial import recovery on tab close/reopen
- R-6: Duplicate detection by content hash

---

## 9. Open Questions

| OQ | Question | Recommendation |
|---|---|---|
| OQ-1 | **Threads export format** | Does Threads use the same GDPR format as FB/IG? PRD assumption A1 says yes. **Gated on verification.** |
| OQ-2 | **Import session history** | Should users see a list of past imports? **Rec: yes, accessible from Vault and Settings** |
| OQ-3 | **Re-import behavior** | What happens if a user re-imports the same archive? Skip duplicates silently? Show a diff? **Rec: skip duplicates, show skipped count** |
