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

## Sprint Theme: "Polish What Exists"

The systems architect recommends stabilizing core infrastructure before new features. This sprint focuses on performance bottlenecks, cleanup, and reliability — all directly addressing user-reported issues and architectural debt.

---

## Sprint Priorities

### P1 — Batch Blob Resolution ⚡⚡⚡

**Systems Advisory HP-1. Single biggest feed render speed improvement.**

**Problem:** Each image in the feed triggers an individual Tauri `invoke()` call to resolve its blob hash to a URL. A feed page with 20 images = 20 sequential IPC round-trips. The user reports feed loading takes ~16 seconds.

**Fix:**
1. New command: `resolve_blob_urls_batch(hashes: Vec<String>) → HashMap<String, String>`
2. Check blob cache in a single pass, return all URLs at once
3. Register the command in `lib.rs`
4. Document the command interface in `progress.md` so the Frontend agent can update `useBlobUrl`

**Expected improvement:** Feed image load from ~20 IPC calls → 1 IPC call. Estimated 3-5× improvement on image-heavy feeds.

---

### P2 — Cargo Warnings Cleanup ⚡⚡

**Systems Advisory QW-4.** 27 compiler warnings obscure real issues during development.

**Fix:**
1. Run `cargo clippy --fix` for auto-fixable issues
2. Manually review remaining warnings: unused imports, dead code, unused variables
3. Only clean up warnings in YOUR files (not dependency warnings)
4. **Do not change any behavior** — only remove genuinely unused code and imports

**Target:** `cargo check 2>&1 | grep warning | wc -l` → 0

---

### P3 — Video Jitter Investigation ⚡⚡

**User:** *"Videos are mostly fixed. Still somewhat jittery, but better."*

**Current state:** FFmpeg remux uses `-c copy` with `aac_adtstoasc` and `genpts+discardcorrupt`. HLS segments can have PTS jumps that `-c copy` doesn't fix.

**Investigation approach:**
1. Examine a cached MP4 file with `ffprobe` to check for PTS discontinuities
2. If PTS gaps: try `-vsync cfr` or `-fps_mode cfr` to force constant frame rate
3. Last resort: lightweight re-encode (`-c:v libx264 -preset ultrafast -crf 23`) — only if remux fixes don't work
4. Keep the two-strategy system (native HLS first, manual fallback)

**Target:** Smooth playback. Audio already works.

---

### P4 — Export Vault Data — Verify & Document ⚡

The command `export_vault_data` exists (183 lines) and is registered. The Frontend agent will wire the button this sprint. Your job:
1. **Test it end-to-end** — verify it produces a valid ZIP with vault items, contacts, messages
2. If `include_media` is true, verify it bundles blob files from the media cache
3. Return a meaningful `ExportResult` (item count, file size) that the frontend can display
4. Document the exact command interface in `progress.md`

---

### P5 — Cross-Store Consistency Check ⚡

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
- [ ] `resolve_blob_urls_batch` command exists and is registered
- [ ] Cargo warnings reduced to 0 (or near-0)
- [ ] Video jitter investigated — fix applied or documented diagnosis
- [ ] `export_vault_data` tested end-to-end, interface documented
- [ ] Consistency check job runs on startup (background, non-blocking)
- [ ] `cargo check` — 0 errors
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `pnpm test -- --run` — all pass
- [ ] All terminals closed
- [ ] Committed to `main` with semantic messages
