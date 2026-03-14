# Backend Agent — Sprint: USER FEEDBACK FIXES (2026-03-15)

## 🚨 CRITICAL CONTEXT — THE USER IS UNHAPPY

The user gave 16 feedback items. After a full sprint, only 3 were completed. The user says "literally none of my feedback has been listened to." **This sprint is 100% user feedback. No roadmap items. No A1.2 research. Only fixing what the user asked for.**

All work is on **`main`**. Verify with `git branch --show-current` before every commit.

---

## Priority 1 — UF.16: Feed Performance ⚡⚡⚡

**User:** *"Feed takes a while to refresh...like a really really long while, tens of seconds that it seems like its stalled out."*

This is the single most complained-about backend issue. Your parallel fetch work was a start, but the feed is still painfully slow. Fix it for real:

### 1a. Cached-First Rendering
When `get_timeline` / `get_unified_feed` is called:
1. **Immediately return cached vault items** — items stored from previous fetches. This should take <200ms.
2. **Spawn a background tokio task** to fetch fresh data from Bluesky API
3. **On success**: merge new items into vault, emit `app.emit("feed-updated", new_items)` event
4. Frontend can then re-render with the updated feed

```rust
// Pseudo-flow:
// 1. Query vault for cached items → return immediately
// 2. Spawn tokio task: fetch from Bluesky API
// 3. On success: merge new items into vault, emit "feed-updated" event
```

### 1b. Loading Events
Emit `feed-loading-start` before API call and `feed-loading-end` after. The Frontend will use these for spinner display.

### 1c. Timing Instrumentation
Gated behind `#[cfg(debug_assertions)]` — log duration of each step:
- Token retrieval, API request, deserialization, vault merge/dedup, total wall time

**Target**: Feed shows content within 500ms (cached), fresh data within 3-5s.

---

## Priority 2 — UF.3: Unified Contacts ⚡

**User:** *"Friends list only has users you have chatted with, rather than all of your connected people."*

Your ContactManager currently only handles Bluesky follows/followers + P2P. Extend:

1. **Extract contacts from imported archives**: Facebook friends, Instagram followers, group members, conversation participants → add to unified contacts table
2. **P2P peers** discovered via Iroh should appear in contacts, not just as a capability flag
3. **Dedup across sources**: Same person as Bluesky follow + Facebook friend + P2P peer. Dedup by DID, then by display name + handle as fallback.
4. **`list_contacts` must return ALL contacts** — not just chat participants. Frontend calls this from the Friends/Social screen.

---

## Priority 3 — UF.15: Export Data Backend ⚡

**User:** *"Before 'Clear Vault' there should be an 'Export' option, that takes you to a detailed export manager, to export your vault and profile data as a zip file, in JSON format."*

### Implement:
```rust
#[tauri::command]
pub async fn export_vault_data(
    output_path: String,
    include_media: bool,
    state: State<'_, AppState>
) -> Result<ExportResult, String>
```

1. Export all `BytescapeItem` records as JSON
2. Export contacts, messages, profile data
3. If `include_media`, include media blobs
4. Package everything into a ZIP file at `output_path`
5. Return `ExportResult { path, item_count, media_count, size_bytes }`

Register in `lib.rs`. The Frontend will add a button that calls this before "Delete Account."

---

## Priority 4 — UF.13: Facebook Import Verification ⚡

**User:** *"The facebook import doesn't really work at all, it just shows random activity rather than actual posts or media."*

You added extensive filtering (50+ directory exclusions). Now verify it actually works:

1. **Test with the user's actual archive** at `/home/korendian/Downloads/facebook-rabiezaater-2025-01-12-TwiR2QHB/`
2. Run the parser and check: are the results actual posts with text/photos/videos, or still random activity?
3. If still broken, trace through `facebook.rs` and find what's still leaking through the filters
4. The import should produce `BytescapeItem` entries that look like real social media posts — not ad interaction logs

---

## Anti-Patterns
- Do NOT modify frontend components (`src/`) — Track B owns those
- Do NOT modify `ComposeBox.tsx` or content components — Track C owns those
- Do NOT commit to feature branches — all work is on `main`
- **Always run `git branch --show-current` before committing**
- Do NOT hallucinate crate imports — check Cargo.toml before using
- Do NOT start A1.2 research or any roadmap item — user feedback only

## Definition of Done
- [ ] Feed shows cached items in <500ms, fresh data arrives via `feed-updated` event
- [ ] `feed-loading-start` / `feed-loading-end` events emitted
- [ ] Debug timing logs for each pipeline step
- [ ] `list_contacts` returns unified list from ALL sources (Bluesky + archives + P2P)
- [ ] `export_vault_data` command registered and produces valid ZIP
- [ ] Facebook import tested with real archive, produces real posts
- [ ] `cargo check` passes (0 errors)
- [ ] `pnpm test` passes
- [ ] Committed to `main` with semantic messages
