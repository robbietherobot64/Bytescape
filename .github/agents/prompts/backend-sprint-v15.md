# Backend Sprint v15 — Feed Pipeline Rebuild: Backend Support (Track A)

**Sprint Goal:** Provide the backend infrastructure the frontend needs for the §6.2 feed rebuild. This means: (1) remaining memory leak cleanup from v14 medium-risk sites, (2) standardized cursor-based pagination across all feed commands, (3) configurable page size, and (4) source-toggle support for the multi-platform Following tab.

**HEAD:** `75f6c01` on `main`. **Build:** 0 tsc errors, 272 frontend tests, cargo check clean.

**Reference:** TECHNICAL_SPEC.md §6.2 "Ground-Up Rebuild Specification (v2, 2026-04-04)"

---

## Context: What the Frontend Needs

The §6.2 spec requires:
- **30 items per page** (configurable 15/30/50), with cursor-based pagination on every tab
- **150-item hard cap** in frontend state — the backend must support cursor-based re-fetching (no offset-only APIs)
- **Source toggles** on Following tab — filter by platform (Bluesky, Mastodon, Pixelfed, etc.)
- **Direct CDN URLs** — backend feed responses must include `thumb` URLs for images (not fullsize)
- **Zero full-blob reads** for any feed-path operation

Currently, feed commands use inconsistent pagination (some cursor, some offset, some hybrid) and inconsistent page sizes. This sprint standardizes them.

---

## P1: Remaining Memory Leak Cleanup (Medium-Risk `sync.get_media()` Sites)

**Priority:** P1 — must be done before the rebuild goes live.

The v14 sprint fixed the primary leak paths (P0.1-P0.7). These medium-risk sites remain:

| File | Line | Context | Fix |
|------|------|---------|-----|
| `commands/syndicate.rs:128` | Post syndication | Replace with header check + export. Syndication sends the blob to a remote server — stream from cache file instead of heap. |
| `commands/vault.rs:249` | Single vault export | Low risk but easy win — export to cache, then read file. |
| `commands/vault.rs:274` | Batch vault export | Medium risk — loop over blobs. Add semaphore (reuse `BLOB_EXPORT_SEMAPHORE`). Export each to cache, then read. |
| `commands/interactions.rs:327` | Attachment fetch | Typically small, but add a size guard: if blob >10 MB, warn and reject. |

**Implementation pattern** (same as v14 P0.1):
```rust
// Instead of: let bytes = sync.get_media(hash).await?;
// Do:
let header = sync.get_media_header(hash, 32).await?;
let ext = detect_extension_from_bytes(&header);
let cache_path = cache_dir.join(format!("{}.{}", hash_str, ext));
if !cache_path.exists() {
    sync.export_media_to_file(hash, &cache_path).await?;
}
let bytes = std::fs::read(&cache_path)?;
```

**Memory test:** After all P1 fixes, `grep -rn "sync.get_media(" src-tauri/src/` should return ZERO results in any feed-path code. Only `import.rs:1219` (one-time import) and `posting.rs:313` (user-initiated, single small blob) may remain.

---

## P2: Standardize Cursor-Based Pagination

**Goal:** Every feed command returns the same shape: `{ items: Vec<FeedEntry>, cursor: Option<String>, has_more: bool }`. The frontend should never need to track offsets.

### P2.1: Create `FeedPage` response type

**File:** `src-tauri/src/commands/media.rs` or a new `src-tauri/src/commands/feed.rs`

```rust
#[derive(Serialize, Clone)]
pub struct FeedPage {
    pub items: Vec<serde_json::Value>,  // Serialized FeedEntry-compatible objects
    pub cursor: Option<String>,
    pub has_more: bool,
}
```

### P2.2: Update `get_timeline` 

**Current:** Returns `(Vec<BlueskyPost>, Option<String>)` — a tuple.
**New:** Returns `FeedPage`. The `cursor` field encodes the Bluesky cursor. `has_more = cursor.is_some()`.

### P2.3: Update `get_unified_feed` (Subscribed tab)

**Current:** Returns `(Vec<BlueskyPost>, Option<String>)`.
**New:** Returns `FeedPage`. Same cursor semantics.

### P2.4: Update `get_timeline_unified` (All tab)

**Current:** Returns `(Vec<BlueskyPost>, Option<String>)` — but internally manages both Bluesky cursor AND vault offset.
**New:** Returns `FeedPage`. The `cursor` field is a **composite cursor**: `bsky:{bluesky_cursor}|vault:{vault_offset}`. The frontend passes this opaque string back; the backend parses it to resume both pagination streams.

Example cursor: `bsky:1712345678000::abc123|vault:200`
On first request (no cursor): fetches first page from both sources.
On subsequent: parses cursor to resume each source independently.

### P2.5: Update `list_vault_posts_paged` 

**Current:** Returns `Vec<BytescapeItem>` with offset-based pagination.
**New:** Returns `FeedPage`. The cursor encodes the offset: `vault:{offset}`. `has_more = items.len() == limit`.

### P2.6: Update `get_groups_feed`

**Current:** Returns `(Vec<serde_json::Value>, Option<String>)`.
**New:** Returns `FeedPage`.

### P2.7: Add `limit` parameter to all commands

All feed commands must accept a `limit: Option<u32>` parameter (default: 30, max: 100, min: 1). Clamp to bounds:
```rust
let limit = limit.unwrap_or(30).clamp(1, 100) as i32;
```

### P2.8: Update `get_author_feed`

**Current:** Returns `(Vec<BlueskyPost>, Option<String>)`.
**New:** Returns `FeedPage`. (Used by Profile, not the feed tabs, but consistency matters.)

**Memory test:** After P2, call each feed command with `limit=30` and verify response contains ≤30 items. Call with cursor from previous response and verify next page is returned. Verify no full-blob reads occur during pagination.

---

## P3: Source Toggle Support for Following Tab

**Goal:** The Following tab merges content from all linked platforms. The frontend needs to filter by source. Two approaches (pick one):

### Option A: Server-side filtering (preferred)
Add a `source_filter: Option<Vec<String>>` parameter to `get_timeline` and `get_timeline_unified`. When provided, only fetch from the specified sources.

```rust
#[tauri::command]
async fn get_timeline(
    state: State<'_, AppState>,
    limit: Option<u32>,
    cursor: Option<String>,
    source_filter: Option<Vec<String>>,  // ["bluesky", "mastodon", "pixelfed"]
) -> Result<FeedPage, String> {
    let limit = limit.unwrap_or(30).clamp(1, 100) as i32;
    
    let mut all_posts = Vec::new();
    let mut next_cursor = None;
    
    let sources = source_filter.unwrap_or_else(|| vec![
        "bluesky".into(), "mastodon".into()
    ]);
    
    if sources.contains(&"bluesky".to_string()) {
        // Fetch from Bluesky timeline...
    }
    if sources.contains(&"mastodon".to_string()) {
        // Fetch from Mastodon home timeline...
    }
    // Future: pixelfed, peertube
    
    // Merge, sort, truncate to limit
    // ...
}
```

### Option B: Client-side filtering
Add a `source` field to every feed response item (already exists in `FeedEntry`). Frontend filters in-memory. No backend changes needed.

**Recommendation:** Option A for Following tab (avoids wasting bandwidth fetching posts we'll discard), Option B for other tabs (simpler, and other tabs already merge everything).

### P3.1: Return linked accounts list

Add a command `get_linked_feed_sources` that returns the list of platforms with active connections. The frontend uses this to render source toggle pills.

```rust
#[tauri::command]
async fn get_linked_feed_sources(
    state: State<'_, AppState>,
) -> Result<Vec<LinkedFeedSource>, String> {
    // Check which platforms have valid sessions
    // Return: [{ id: "bluesky", label: "Bluesky", icon: "bsky", connected: true }, ...]
}

#[derive(Serialize)]
struct LinkedFeedSource {
    id: String,       // "bluesky", "mastodon", "pixelfed", "peertube"
    label: String,    // "Bluesky", "Mastodon", etc.
    handle: String,   // "@user.bsky.social" or "@user@mastodon.social"
    connected: bool,  // Has valid session
}
```

**Memory test:** Fetch Following with `source_filter=["bluesky"]` only — verify zero Mastodon API calls. Fetch with `source_filter=["mastodon"]` only — verify zero Bluesky API calls.

---

## P4: Thumb URL Enforcement

**Goal:** Feed responses must never include fullsize image URLs where a thumbnail exists.

### P4.1: Verify Bluesky thumb preference

**File:** `src-tauri/src/connectors/bluesky.rs`

Commit `f69db7d` already switched to `thumb` URLs. **Verify this is still in place** — check that `images[].thumb` is used (not `images[].fullsize`) in the adapter that produces `BlueskyPost` objects for timeline/feed responses. If fullsize URLs are still leaking through anywhere, fix them.

### P4.2: Verify Mastodon small variant

**File:** `src-tauri/src/connectors/mastodon.rs`

Check the Mastodon connector's post adaptation. Mastodon API returns `media_attachments[].preview_url` (small) and `media_attachments[].url` (full). The feed adapter should use `preview_url` for `media_thumbnails[]` and `url` for `media_urls[]`. The frontend feed will use `media_thumbnails[]` exclusively; `media_urls[]` is only used in the gallery overlay.

### P4.3: Add `media_thumbnails` to FeedPage items

Ensure every item in `FeedPage.items` includes both `media_urls` (fullsize, for gallery) and `media_thumbnails` (small, for feed cards). If a source doesn't provide thumbnails, `media_thumbnails` should equal `media_urls` (the frontend will still apply `max-width: 600px` CSS).

**Memory test:** Fetch a page of 30 items. For every Bluesky image URL in the response, verify it contains `feed_thumbnail` (not `feed_fullsize`). For every Mastodon image, verify `media_thumbnails` contains `preview_url`.

---

## Verification Checklist

### Memory Tests (run after each P-level commit):

| Test | Command | Pass Criteria |
|------|---------|---------------|
| No full-blob reads in feed path | `grep -rn "sync.get_media(" src-tauri/src/commands/` | Only in import.rs and posting.rs |
| Page size enforcement | Call `get_timeline` with limit=30 | Response has ≤30 items |
| Cursor round-trip | Call with cursor from previous response | Returns next page, not duplicate |
| Source filtering | Call `get_timeline` with `source_filter=["bluesky"]` | No Mastodon API calls in logs |
| Thumb URLs | Inspect response items | All Bluesky images use `feed_thumbnail` |
| Memory under load | Fetch 5 pages of 30 items sequentially | RSS stays under 500 MB |

### Build Verification:
```bash
cargo check    # 0 errors
cargo test     # all pass
```

---

## Constraints

- **Track boundary:** Only modify files in `src-tauri/src/`. Do NOT touch `src/` (frontend files).
- **Build verification:** Run `cargo check` before EVERY commit. Zero errors.
- **Commit format:** `fix(track-a): description` for memory fixes, `feat(track-a): description` for new features.
- **No TODOs in code.** Implement fully.
- **Shared files:** Before editing `lib.rs` (command registration), run `git log --oneline -5 -- src-tauri/src/lib.rs` to check for recent agent modifications.
- **FeedPage type:** The frontend will depend on this exact response shape. Do not change field names after the first commit without coordinating via progress.md.
- **Backward compatibility:** Keep old command signatures working during the transition. The frontend sprint will migrate to the new signatures. Once migration is confirmed, old signatures can be removed in a follow-up sprint.
- **Update `progress.md`** with deliverables when done.
