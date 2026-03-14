# Backend Agent — Sprint: USER FEEDBACK v2 (2026-03-15)

## 🚨 READ FIRST — User Reverted Agent Work

The user manually reverted several backend changes:
- ❌ `headers_cache` removed from `DbManager` — do NOT re-add caching to the struct
- ❌ `item_type_filter` parameter removed from `get_timeline_unified` — do NOT add server-side type filtering
- ❌ `reclassify_vault_items` command removed from `lib.rs` — do NOT re-register it
- ❌ `commands/classify.rs` module removed from `commands/mod.rs` — the file still exists but is dead code

The `classify_feed_entry` and `classify_item` functions in `models/mod.rs` still exist and should be used for item classification on ingest — just don't expose them as standalone commands.

All work on **`main`**. Run `git branch --show-current` before every commit.

Read the user's latest feedback at `.github/agents/prompts/user-feedback` before starting.

---

## Priority 1 — Feed Performance ⚡⚡⚡

**User:** *"Feed takes a while to refresh...like a really really long while. Backend worked on this for a bit, but it's still 16 seconds to load the feed."*

The headers_cache approach was rejected. Try a different strategy:

### Fix:
1. **Profile the actual bottleneck**: Add `#[cfg(debug_assertions)]` timing logs to pinpoint whether the 16s is in token retrieval, Bluesky API request, response parsing, vault merge, or dedup
2. **Reduce API payload**: Use pagination properly — request only 30 items per call, not bulk fetches
3. **Parallel fetch**: If fetching timeline + custom feeds, use `tokio::join!` to run them concurrently (check if this is already implemented, it may be)
4. **Quick-return vault items**: Return vault items immediately as a first page, then fetch live data in the background and emit a `feed-updated` event (do NOT use struct-level caching — use a simple event-based approach)

**Target**: Feed shows content within 2-3 seconds. 16 seconds is unacceptable.

---

## Priority 2 — Chat Friends List ⚡

**User:** *"Chat friends list only has users you have chatted with, rather than all of your connected people you can chat with."*

### Fix:
1. `list_contacts` must return ALL contacts — not just chat participants
2. Include: Bluesky follows/followers, imported archive contacts (Facebook friends, Instagram follows), P2P peers
3. Dedup by DID, then by handle/display name
4. The frontend friends panel calls `list_contacts` — it should show everyone you could potentially chat with

---

## Priority 3 — Export Vault Data ⚡

**User:** *"Export button added, but no functionality."*

The frontend has an export button but it calls a command that doesn't exist or doesn't work yet.

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
4. Package into ZIP at `output_path`
5. Return `ExportResult { path, item_count, media_count, size_bytes }`
6. Register in `lib.rs`

---

## Priority 4 — Facebook Import Quality

The user's previous feedback about Facebook import "not really working" and "showing random activity" may have been partially addressed. Verify:

1. Test with archive at `/home/korendian/Downloads/facebook-rabiezaater-2025-01-12-TwiR2QHB/`
2. Run parser, check that results are actual posts with text/photos — not ad logs or activity noise
3. If still broken, trace through `facebook.rs` filtering logic

---

## Anti-Patterns
- Do NOT re-add `headers_cache` to DbManager
- Do NOT re-add `item_type_filter` to `get_timeline_unified`
- Do NOT re-register `reclassify_vault_items` command
- Do NOT modify frontend components — Frontend scope
- Do NOT modify ComposeBox — Content scope
- **Always run `git branch --show-current` before committing**

## Definition of Done
- [ ] Feed loads in <3 seconds with timing logs showing bottleneck
- [ ] `list_contacts` returns unified contacts from all sources
- [ ] `export_vault_data` command produces valid ZIP
- [ ] Facebook import produces real posts (verified with user's archive)
- [ ] `cargo check` passes
- [ ] `pnpm test` passes
- [ ] Committed to `main`
