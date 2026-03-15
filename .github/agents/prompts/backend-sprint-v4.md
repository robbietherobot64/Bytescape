# Backend Agent — Sprint v4 (2026-03-15)

## ⚠️ OPERATIONAL DISCIPLINE — READ CAREFULLY

Read `.github/instructions/agent-guardrails.instructions.md` before doing anything.
Read `.github/agents/prompts/user-feedback` for the user's actual words.

### Performance Standards
- **Research before coding.** If a feature involves an unfamiliar API or library, read the documentation first. Do not guess-and-check.
- **One feature per commit.** Each commit addresses one priority item.
- **Verify before committing:** `cargo check` + `npx tsc --noEmit` + `pnpm test -- --run` — all must pass.
- **If stuck after two genuine attempts:** Document what you tried and what failed in `progress.md`, then move on. Do not burn cycles.
- **Terminal discipline:** Do not spawn multiple `pnpm tauri dev` processes. Kill background processes when done. Do not use `sleep` loops. **Close every terminal you open when you are done with it** — do not leave zombie terminals.

### Track Boundaries
Your scope is `src-tauri/src/` (Rust backend), `src-tauri/Cargo.toml`.
- **Do NOT modify files in `src/`** (React/TypeScript) — that is Frontend/Content scope.
- **Do NOT modify `src/components/ComposeBox*`** — Content scope.
- If a frontend change is needed to complete your feature, document the requirement in `progress.md`.

### Current Codebase State (as of commit `09ab5a4`)

**What you delivered last sprint (verified, all working):**
- `SHARED_CLIENT` LazyLock with connection pooling — ✅ in `bluesky.rs`
- `FeedCache` + `CachedResponse<T>` — ✅ in `lib.rs` (timeline 30s, saved feeds 5min)
- `header_cache` on DbManager — ✅ in `db/mod.rs`
- `classify_feed_entry` restored — ✅ in `feed_aggregation.rs`
- `export_vault_data` command — ✅ in `commands/export.rs` (183 lines, registered)
- FFmpeg `aac_adtstoasc` + `genpts+discardcorrupt` — ✅ in `commands/media.rs`

**Build:** cargo ✅ (27 warnings, 0 errors), tsc ✅, 250/250 tests ✅

---

## Sprint Priorities

### P1 — Video Jitter Investigation ⚡⚡⚡

**User:** *"Videos are mostly fixed. Still somewhat jittery, but better."*

**Current state:** The FFmpeg remux uses `-c copy` (no re-encoding) with `aac_adtstoasc` and `genpts+discardcorrupt`. This fixes audio and handles most timestamp issues, but HLS segments can have presentation timestamp jumps that `-c copy` doesn't fix because it doesn't re-encode.

**Investigation approach:**
1. Examine a cached MP4 file with `ffprobe` to check for PTS discontinuities or frame drops
2. If PTS gaps are the issue, consider adding `-vsync cfr` or `-fps_mode cfr` to force constant frame rate
3. As a last resort, a lightweight re-encode pass (`-c:v libx264 -preset ultrafast -crf 23`) would eliminate jitter but be slower — only do this if remux-level fixes don't work
4. The two-strategy system (native HLS first, manual fallback) is good architecture — keep it, just improve the flags

**Target:** Smooth playback without jitter. Audio already works.

---

### P2 — Export Vault Data — Polish & Test ⚡⚡

**User:** *"Export button added, but no functionality"*

The command `export_vault_data` exists (183 lines) and is registered. But:
1. **Test it end-to-end** — invoke it via `pnpm tauri dev` and verify it produces a valid ZIP
2. Check: Does it include vault items, contacts, messages? Does the media inclusion work?
3. If `include_media` is true, does it actually bundle blob files from the media cache?
4. Return a meaningful `ExportResult` that the frontend can display (item count, file size, etc.)
5. Document the command's interface clearly in `progress.md` so the Frontend agent can wire it up

---

### P3 — Feed Cache Tuning ⚡

The caching system was implemented last sprint but hasn't been tuned with real-world usage.

1. Add `#[cfg(debug_assertions)]` timing logs to `get_timeline_unified` showing: cache hit/miss, API call duration, vault merge duration, total
2. If the user reports the feed is still slow, these logs will help diagnose whether it's cache misses, API latency, or vault scanning
3. Consider whether 30s TTL for timeline is appropriate, or if it should be shorter/longer

---

### P4 — Cargo Warnings Cleanup ⚡

There are 27 compiler warnings. Clean up the most impactful ones:
1. Unused imports
2. Dead code warnings
3. Unused variables
4. Keep it to warnings in YOUR files (not dependency warnings)

**Do not change any behavior** — only remove genuinely unused code and imports.

---

## Anti-Patterns (MUST AVOID)
- ❌ Do NOT modify files in `src/` — Frontend/Content scope
- ❌ Do NOT spawn multiple build processes or use `sleep` loops
- ❌ Do NOT make multi-feature commits
- ❌ Do NOT guess at solutions — read docs, read error messages, investigate
- ❌ Do NOT re-add caching layers without documenting why the approach differs from previous attempts

## Definition of Done
- [ ] Video jitter investigated — fix applied or documented diagnosis
- [ ] `export_vault_data` tested end-to-end, produces valid ZIP
- [ ] Feed cache timing logs added (debug builds only)
- [ ] Cargo warnings reduced
- [ ] `cargo check` — 0 errors
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `pnpm test -- --run` — all pass
- [ ] Committed to `main` with semantic messages
