# Backend Agent — Sprint: USER FEEDBACK v3 (2026-03-15)

## 🚨 READ FIRST — Codebase State & Guardrails

Read `.github/instructions/agent-guardrails.instructions.md` before doing anything.

### Current Codebase State (as of commit `eef398e`)

These are facts about the current code — not user preferences. Fix or keep based on **user feedback**, not file state.

- **`headers_cache`**: Not present in `DbManager` struct. Was added by Backend agent in Sprint 3 (`c60d784`), removed in PM fix commit. The performance problem (16s feed load) is still unsolved — find a different approach.
- **`item_type_filter`**: Not present on `get_timeline_unified`. Server-side type filtering was removed.
- **`reclassify_vault_items`**: Not registered in `lib.rs`. `commands/classify.rs` still exists as a file but is dead code (not in `mod.rs`).
- **`classify_feed_entry`** and **`classify_item`** in `models/mod.rs` still exist and should be called during ingest to set `item_type` on feed entries.
- **`item_type` is hardcoded**: In `feed_aggregation.rs`, the `From<BlueskyImportedPost>` impl sets `item_type: Some(BytescapeType::Status)` for ALL posts. This breaks the ReaderLens (Frontend expects `Article` type items). Restore the `classify_feed_entry` call.
- **`store/index.ts`**: Backend agent created this in Sprint 3. It still exists and is imported by `useUnifiedFeed.ts`. This is a **frontend file** — Backend should not have created it (see guardrails §3).

### ⚠️ Track Boundaries
You (Backend) modified `useUnifiedFeed.ts` and created `store/index.ts` in Sprint 3. These are **frontend files**. Going forward, do NOT modify files in `src/` (React) — coordinate with Frontend agent through `progress.md` if a frontend change is needed.

All work on **`main`**. Run `git branch --show-current` before every commit.

Read the user's latest feedback at `.github/agents/prompts/user-feedback` before starting.

---

## Priority 1 — Feed Performance ⚡⚡⚡

**User:** *"Feed takes a while to refresh...like a really really long while. Backend worked on this for a bit, but it's still 16 seconds to load the feed."*

The Sprint 3 `headers_cache` approach was removed in the PM fix commit. Try a different strategy:

### Fix:
1. **Profile the actual bottleneck**: Add `#[cfg(debug_assertions)]` timing logs to pinpoint whether the 16s is in token retrieval, Bluesky API request, response parsing, vault merge, or dedup
2. **Reduce API payload**: Use pagination properly — request only 30 items per call, not bulk fetches
3. **Parallel fetch**: If fetching timeline + custom feeds, use `tokio::join!` to run them concurrently (check if this is already implemented, it may be)
4. **Quick-return vault items**: Return vault items immediately as a first page, then fetch live data in the background and emit a `feed-updated` event (event-based approach, not struct-level caching)

### Also Fix — Restore classify_feed_entry:
In `feed_aggregation.rs`, the `From<BlueskyImportedPost>` impl hardcodes `item_type: Some(BytescapeType::Status)`. Change it to:
```rust
let mut entry = FeedEntry { ..., item_type: None, ... };
entry.item_type = Some(crate::models::classify_feed_entry(&entry));
entry
```
This is critical — it unblocks the Frontend ReaderLens (which filters on `item_type === 'Article'`).

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
- Do NOT modify files in `src/` (React/TypeScript) — that is Frontend/Content scope. See guardrails §3.
- Do NOT modify ComposeBox — Content scope
- Check `git log --oneline -5 -- <file>` before editing any file another agent recently touched
- **Always run `git branch --show-current` before committing**
- **Always verify before committing**: `cargo check` + `npx tsc --noEmit` + `pnpm test -- --run`

## Definition of Done
- [ ] Feed loads in <3 seconds with timing logs showing bottleneck
- [ ] `list_contacts` returns unified contacts from all sources
- [ ] `export_vault_data` command produces valid ZIP
- [ ] Facebook import produces real posts (verified with user's archive)
- [ ] `cargo check` passes
- [ ] `pnpm test` passes
- [ ] Committed to `main`
