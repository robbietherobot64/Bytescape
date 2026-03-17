# Backend Agent — Sprint v5 (2026-03-15)

## ⚠️ OPERATIONAL DISCIPLINE — READ CAREFULLY

Read `.github/instructions/agent-guardrails.instructions.md` before doing anything.
Read `.github/agents/prompts/user-feedback` for the user's actual words.
Read `.github/agents/prompts/systems-advisory-2026-03-15.md` for the systems architect's recommendations.

### Organizational Change: Two-Agent Model

The Content agent has been dissolved. There are now only **two agents**: you (Backend/Track A) and the Frontend agent (Track B — all of `src/`).

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

## 🚨 Codebase State (as of commit `ab8c00c`)

### What you delivered last sprint (verified, all working)
- `SHARED_CLIENT` LazyLock with connection pooling — ✅ in `bluesky.rs`
- `FeedCache` + `CachedResponse<T>` — ✅ in `lib.rs` (timeline 30s, saved feeds 5min)
- `header_cache` on DbManager — ✅ in `db/mod.rs`
- `classify_feed_entry` restored — ✅ in `feed_aggregation.rs`
- `export_vault_data` command — ✅ in `commands/export.rs` (183 lines, registered)
- FFmpeg `aac_adtstoasc` + `genpts+discardcorrupt` — ✅ in `commands/media.rs`
- Facebook connector filtering — ✅ (50+ directory exclusions, lifecycle filtering)
- Instagram connector — ✅ (507 lines, working)

**Build:** cargo ✅ (27 warnings, 0 errors), tsc ✅, 250/250 tests ✅

---

## Sprint Theme: "Bugfix & Stabilize"

User testing on 2026-03-17 revealed escalated issues: feed load time now 20s, video still stuttery, archive re-imports create duplicates, and Bluesky getFollowers returns 401. Phase 1 is critical bug fixes, Phase 2 is the originally planned infrastructure work.

---

## Phase 1 — Critical Bug Fixes (do these FIRST)

### P1 — Video Playback Fix ⚡⚡⚡ CRITICAL

**User:** *"Video stuttery/choppy when playing."* and *"I always see at least a few seconds of 'loading native media' before a video loads."*

This has been escalated from "mostly fixed" to a critical complaint. The user is seeing both stutter during playback AND a multi-second delay before video starts.

**Two problems to solve:**
1. **Stutter during playback:** FFmpeg remux uses `-c copy` with `aac_adtstoasc` and `genpts+discardcorrupt`. HLS segments can have PTS jumps that `-c copy` doesn't fix.
   - Examine a cached MP4 with `ffprobe` for PTS discontinuities
   - Try `-vsync cfr` or `-fps_mode cfr` for constant frame rate
   - Last resort: lightweight re-encode (`-c:v libx264 -preset ultrafast -crf 23`)
2. **"Loading native media" delay:** The HLS→MP4 conversion takes too long before playback starts.
   - Check if the FFmpeg process is waiting on full download before converting
   - Consider streaming the conversion (pipe segments as they arrive)
   - Add progress events so the frontend knows conversion status

**Target:** Video starts within 1-2s. Smooth playback without stutter.

---

### P2 — Bluesky getFollowers 401 Unauthorized ⚡⚡⚡ CRITICAL

**User:** *"Followers in the social module has a 'Bluesky API error: getFollowers failed with status:401 Unauthorized.'"*

The Bluesky session token is expired or not being refreshed before the `getFollowers` call.

**Investigation:**
1. Check the `getFollowers` command in the Bluesky connector — does it call `ensure_valid_session()` or `refresh_session()` before making the API request?
2. Other Bluesky commands (timeline, etc.) likely DO refresh — find the pattern they use and apply it to `getFollowers`
3. The auth retry pattern is in `auth.rs` — check if followers-related endpoints follow the same 401→refresh→retry flow

**Fix:** Ensure `getFollowers` (and `getFollows`) calls go through the same session-refresh middleware as timeline fetches.

---

### P3 — Archive Import Deduplication ⚡⚡

**User:** *"Posts/images on the facebook feed are duplicating. If someone tries to import a social media archive more than once, it should not duplicate. The import should be smart enough to differentiate."*

Currently, re-importing the same archive creates duplicate `BytescapeItem` entries.

**Fix:**
1. Before inserting a new item during import, check if an item with the same `source` + `source_id` (or content hash if no source_id) already exists
2. If it exists, skip it (or update if the new version has more data)
3. For Facebook/Instagram archives: use the combination of `source: Facebook/Instagram` + timestamp + content hash as a dedup key
4. Multiple DIFFERENT archives should still merge correctly — only skip true duplicates

**This must work for both Facebook and Instagram connectors.**

---

## Phase 2 — Infrastructure & Performance

### P4 — Batch Blob Resolution ⚡⚡

**Systems Advisory HP-1. Single biggest feed render speed improvement.**

**Problem:** Each image in the feed triggers an individual Tauri `invoke()` call to resolve its blob hash to a URL. A feed page with 20 images = 20 sequential IPC round-trips. The user reports feed loading takes **~20 seconds**.

**Fix:**
1. New command: `resolve_blob_urls_batch(hashes: Vec<String>) → HashMap<String, String>`
2. Check blob cache in a single pass, return all URLs at once
3. Register the command in `lib.rs`
4. Document the command interface in `progress.md` so the Frontend agent can update `useBlobUrl`

**Expected improvement:** Feed image load from ~20 IPC calls → 1 IPC call. Estimated 3-5× improvement on image-heavy feeds.

---

### P5 — Cargo Warnings Cleanup ⚡

**Systems Advisory QW-4.** 27 compiler warnings obscure real issues during development.

**Fix:**
1. Run `cargo clippy --fix` for auto-fixable issues
2. Manually review remaining warnings: unused imports, dead code, unused variables
3. Only clean up warnings in YOUR files (not dependency warnings)
4. **Do not change any behavior** — only remove genuinely unused code and imports

**Target:** `cargo check 2>&1 | grep warning | wc -l` → 0

---

### P6 — Export Vault Data — Verify & Document ⚡

The command `export_vault_data` exists (183 lines) and is registered. The Frontend agent will wire the button this sprint. Your job:
1. **Test it end-to-end** — verify it produces a valid ZIP with vault items, contacts, messages
2. If `include_media` is true, verify it bundles blob files from the media cache
3. Return a meaningful `ExportResult` (item count, file size) that the frontend can display
4. Document the exact command interface in `progress.md`

---

### P7 — Cross-Store Consistency Check ⚡

**Systems Advisory PS-2.**

**Problem:** `BytescapeItem.media_blobs[]` references Iroh blob hashes, but there's no consistency guarantee. Failure modes: orphan blobs (wasted storage), broken references (missing images in feed), partial state from crash mid-import.

**Fix:** Background consistency-check job:
1. On startup (after 10s delay), compare all blob hashes referenced in `ITEMS_TABLE` against Iroh blob inventory
2. Log orphans and flag broken references
3. Optionally auto-heal by removing broken blob references from items
4. Use `tokio::spawn` with a delay so it doesn't block app startup

---

## Anti-Patterns (MUST AVOID)
- ❌ Do NOT modify files in `src/` — Frontend scope
- ❌ Do NOT spawn multiple build processes or use `sleep` loops
- ❌ Do NOT make multi-feature commits
- ❌ Do NOT guess at solutions — read docs, read error messages, investigate
- ❌ Do NOT leave terminals open when done

## Definition of Done
### Phase 1 (Critical — must complete)
- [ ] Video playback smooth, load delay reduced
- [ ] Bluesky `getFollowers` no longer returns 401 (session refresh applied)
- [ ] Archive re-import does not create duplicate items
### Phase 2 (Infrastructure — complete as time allows)
- [ ] `resolve_blob_urls_batch` command exists and is registered
- [ ] Cargo warnings reduced to 0 (or near-0)
- [ ] `export_vault_data` tested end-to-end, interface documented
- [ ] Consistency check job runs on startup (background, non-blocking)
### Build verification
- [ ] `cargo check` — 0 errors
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `pnpm test -- --run` — all pass
- [ ] All terminals closed
- [ ] Committed to `main` with semantic messages
