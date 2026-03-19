# Backend Agent — Sprint v6 (2026-03-19)

## ⚠️ OPERATIONAL DISCIPLINE — READ CAREFULLY

Read `.github/instructions/agent-guardrails.instructions.md` before doing anything.
Read `.github/agents/prompts/user-feedback` for the user's actual words.
Read `.github/agents/prompts/systems-advisory-2026-03-15.md` for the systems architect's recommendations.

### Organizational Context: Two-Agent Model

There are only **two agents**: you (Backend/Track A) and the Frontend agent (Track B — all of `src/`).

### Performance Standards
- **Research before coding.** If a feature involves an unfamiliar API or library, read the documentation first. Do not guess-and-check.
- **One feature per commit.** Each commit addresses one priority item.
- **Verify before committing:** `cargo check` + `npx tsc --noEmit` + `pnpm test -- --run` — all must pass.
- **Two-attempt cap:** If stuck after two genuine investigated attempts, document what you tried in `progress.md` and move on.
- **Terminal discipline:** Do not spawn multiple `pnpm tauri dev` processes. Kill background processes when done. Do not use `sleep` loops. **Close every terminal you open when you are done with it** — do not leave zombie terminals.

### Track Boundaries
Your scope is `src-tauri/src/` (Rust backend), `src-tauri/Cargo.toml`.
- **Do NOT modify files in `src/`** (React/TypeScript) — that is Frontend scope.
- If a frontend change is needed to complete your feature, document the requirement in `progress.md`.

---

## 🚨 Codebase State (as of commit `66d2e1c`)

### What you delivered in Sprint v5 (verified, all working)
- Tiered A→B→C FFmpeg strategy with 5 root cause fixes — ✅ smooth video playback
- OAuth `Mutex` → `RwLock<Option<Arc<>>>` + parallel feed fetches — ✅ feed 29s→~2s
- Archive import deduplication via stable content hash — ✅ no duplicates on re-import
- getFollowers 401 → public API relay migration — ✅
- Cargo warnings 27 → 0 — ✅
- Batch blob URL resolution command — ✅ (frontend wired it)
- Background blob consistency check on startup — ✅
- FeedCache extended to subscribed & custom feeds — ✅
- Cached MP4 validation + auto-reconversion — ✅

**Build:** cargo ✅ (0 warnings, 0 errors), tsc ✅, 250/250 tests ✅

---

## Sprint Theme: "Unblock Frontend & Source Fix"

Sprint v5 was highly productive — most backend critical bugs are resolved. This sprint addresses: a new source filter regression, the chat button backend dependency, WebView window label fix, and begins feature work for profile persistence.

---

## Phase 1 — Critical Fixes

### P1 — Fix Source Filter / Feed State Bug ⚡⚡⚡ CRITICAL NEW BUG

**User:** *"'Sources' dropdown on the feed does not properly filter Facebook, it shows up as Bluesky posts. Also, it seems to break the feed in general, when you return to All after filtering, it only shows your posts, not the posts of others."*

Two problems:
1. **Facebook items display with wrong source badge**: The `source` field on `FeedEntry` items imported from Facebook may not be correctly set, or the `FeedEntry` conversion loses the source info
2. **State corruption when clearing filter**: When the frontend sends a source filter then clears it, the backend may be returning a cached (filtered) result instead of the full feed

**Investigation steps:**
1. Check `commands/feed_aggregation.rs` — `get_timeline_unified` — how does source filtering work? Is there a `source_filter` parameter?
2. Check the `From<BytescapeItem> for FeedEntry` conversion — does it correctly map `source: Facebook` through to the `FeedEntry`?
3. Check if `FeedCache` caches filtered results under the same key as unfiltered results — if so, a filtered cache entry could be returned for an unfiltered request
4. Verify: load vault items with `source: Facebook` via redb and confirm the `source` field is correctly set in the DB

**Likely fixes:**
- Ensure the `FeedCache` key includes the source filter parameter (so filtered and unfiltered results are cached separately)
- Ensure `FeedEntry.source` correctly carries through the `BytescapeItem.source` value for archive-imported items
- Check if the `source` enum serialization matches what the frontend expects (e.g., "facebook" vs "Facebook" vs "bluesky")

---

### P2 — Expose `get_or_create_bsky_convo` Command ⚡⚡

**Blocked since Sprint v5.** The frontend needs a Tauri command to create or retrieve a Bluesky DM conversation.

`chat.rs:251` already has `get_or_create_convo()` which calls `chat.bsky.convo.getConvoForMembers`. It's just not exposed as a `#[tauri::command]`.

**Fix:**
1. Create a new Tauri command wrapping the existing function:
```rust
#[tauri::command]
pub async fn get_or_create_bsky_convo(
    state: State<'_, AppState>,
    member_did: String,
) -> Result<String, String>  // returns convo_id
```
2. Call the existing `get_or_create_convo()` logic in `chat.rs`
3. Handle both OAuth and app-password auth paths
4. Register in `lib.rs` invoke_handler
5. Document the command interface in `progress.md` for the Frontend agent

**Frontend agent's planned usage:**
```typescript
const convoId = await invoke<string>('get_or_create_bsky_convo', { memberDid: buddy.did });
setSelectedConvoId(convoId);
```

⚠️ **Two frontend workarounds were already attempted and reverted** — this confirms the backend dependency is real. Do not add frontend-side hacks.

---

### P3 — Fix WebView Window Label Mismatch ⚡⚡

**Persistent since Sprint v4.** The in-app browser (`commands/browser.rs`) creates a secondary WebView for external URLs, but `get_window("main")` returns `None` at runtime.

**Investigation:**
1. Check `tauri.conf.json` — what is the actual window label? Look at `"windows"` array → `"label"` field
2. Check `commands/browser.rs` — what label does `create_browser_view()` pass to `get_window()`?
3. If they don't match, fix the label in `browser.rs` to use the correct value from `tauri.conf.json`
4. Test: after fix, invoke `create_browser_view` with a URL and verify the WebView appears

**This has been a persistent issue across 2 sprints.** Do NOT try a different architecture. The `WebviewBuilder` approach is correct — it just needs the right window label.

---

## Phase 2 — Feature Work

### P4 — Profile Layout Persistence Command ⚡⚡

The Frontend agent is building a MySpace/SpaceHey-inspired profile with editable sections (About Me, Connections, Photos, Links, etc.). They need a backend command to save and load profile section layouts.

**Check first:** `save_profile_layout` and `get_profile_layout` may already exist from the B2.1 GrapesJS integration. If they do:
1. Verify they work with a JSON array of section objects
2. Confirm the data format supports: `{ id, type, title, content, order, visible }`
3. If the existing format doesn't support this, add a new `save_profile_sections` / `get_profile_sections` command pair

**If no profile persistence exists:**
1. Add `PROFILE_SECTIONS_TABLE` to redb (key: `owner_did`, value: serialized JSON array)
2. `save_profile_sections(sections: Vec<ProfileSection>) -> Result<(), String>`
3. `get_profile_sections() -> Result<Vec<ProfileSection>, String>`
4. Register both in `lib.rs`
5. Document in `progress.md`

### P5 — Facebook Import Verification ⚡

**User feedback** *(UF.13)*: "Facebook import glitches the import screen and shows random activity instead of actual posts and media."

Sprint v4 added extensive filtering (50+ directory exclusions, lifecycle/activity log filtering) to `facebook.rs`. This was never verified by the user.

**Action:**
1. Run a test import using the Facebook archive in the workspace: `/home/korendian/Downloads/facebook-rabiezaater-2025-01-12-TwiR2QHB/`
2. Check the imported items: are they actual posts with text/photos/videos? Or random activity logs?
3. If low-quality items are still getting through, tighten the filters
4. Document results in `progress.md`

---

## Phase 3 — Technical Debt (as time allows)

### P6 — Write Rust Tests for Sprint v5 Fixes ⚡

Sprint v5 shipped significant fixes without tests. Add targeted test coverage for:

1. **Tiered FFmpeg strategy**: Test that `tiered_ts_to_mp4()` tries strategies in order and falls back correctly
2. **Archive dedup**: Test that `commit_memories` with the same content twice produces only one DB entry
3. **FeedCache**: Test that filtered and unfiltered results are cached under different keys
4. **`get_or_create_bsky_convo`**: Integration test for the new command (P2)

Target: at least 10 new `#[cfg(test)]` functions.

---

## Anti-Patterns (MUST AVOID)
- ❌ Do NOT modify files in `src/` — Frontend scope
- ❌ Do NOT spawn multiple build processes or use `sleep` loops
- ❌ Do NOT make multi-feature commits
- ❌ Do NOT guess at solutions — read docs, read error messages, investigate
- ❌ Do NOT leave terminals open when done

## Definition of Done
### Phase 1 (Critical — must complete)
- [ ] Source filter returns correctly-sourced items (Facebook as Facebook, not Bluesky)
- [ ] Clearing source filter restores full feed
- [ ] `get_or_create_bsky_convo` command exists and is registered
- [ ] WebView window label matches `tauri.conf.json` — external URLs load in secondary WebView
### Phase 2 (Features — high priority)
- [ ] Profile sections persistence command(s) exist and are documented
- [ ] Facebook import verified against workspace archive — results documented
### Phase 3 (Debt — as time allows)
- [ ] ≥10 new Rust unit tests for sprint v5 fixes
### Build verification
- [ ] `cargo check` — 0 errors, 0 warnings
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `pnpm test -- --run` — all pass
- [ ] All terminals closed
- [ ] Committed to `main` with semantic messages
