# Backend Agent — Sprint v5 (2026-03-15)

## ⚠️ OPERATIONAL DISCIPLINE — READ CAREFULLY

Read `.github/instructions/agent-guardrails.instructions.md` before doing anything.
Read `.github/agents/prompts/user-feedback` for the user's actual words.
Read `.github/agents/prompts/systems-advisory-2026-03-15.md` for the systems architect's recommendations.
Read `.github/agents/prompts/systems-perf-analysis-2026-03-17.md` for the performance deep-dive (feed loading + video bottlenecks).

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

This has been escalated from "mostly fixed" to a critical complaint. The user is seeing both stutter during playback AND a **5–20 second** delay before video starts.

**⚠️ SYSTEMS ANALYSIS (2026-03-17) — CORRECTED UNDERSTANDING:**

The previous sprint description was incorrect about the current state. Here is the truth:

- **CURRENT STATE:** Both `ffmpeg_hls_to_mp4()` (line ~808) and `remux_ts_to_mp4()` (line ~855) already use **full H.264 re-encode** (`-c:v libx264 -preset ultrafast -crf 23 -c:a aac -b:a 128k`). This is NOT a remux — it's a full CPU re-encode of every video frame AND audio stream.
- **The function `remux_ts_to_mp4` is misleadingly named** — it does a full re-encode, not a remux.
- **The comment says "Audio is copied (AAC passthrough)"** but the actual flags are `-c:a aac -b:a 128k` (re-encode, not `-c:a copy`).
- **The "Loading native media" delay IS the re-encode time.** The `<video>` tag in VideoPlayer.tsx is not rendered until FFmpeg completes. Users wait 5–20s per video.
- **The `ultrafast` preset inflates output 2–3×** vs source because it skips motion estimation.

**THE FIX — Tiered Strategy (go the OPPOSITE direction from current):**

The current code went to the "nuclear option" (full re-encode) to fix PTS discontinuities. The fix is to back off to cheaper strategies and only use re-encode as a validated last resort:

```
Strategy A (try first): Stream-copy with PTS fixup
  ffmpeg -y -fflags +genpts+discardcorrupt
         -avoid_negative_ts make_zero
         -i <input>
         -c:v copy                    ← NO re-encode
         -c:a copy                    ← NO re-encode
         -fps_mode cfr
         -movflags +faststart
         -f mp4 <output>
  Expected time: <1 second (I/O bound, not CPU bound)

Strategy B (fallback): Copy video, re-encode audio only
  ffmpeg ... -c:v copy -c:a aac -b:a 128k -bsf:a aac_adtstoasc ...
  Fixes AAC-in-TS framing issues while keeping video stream-copy speed.
  Expected time: 1-2 seconds

Strategy C (last resort): Current full re-encode
  Only if A and B produce files that fail ffprobe PTS validation.
  Keep existing libx264 ultrafast crf 23 code as-is for this path.
```

**Implementation steps:**
1. Rename `remux_ts_to_mp4` → `reencode_ts_to_mp4` (accuracy)
2. Create a new `try_stream_copy_to_mp4()` function with Strategy A flags
3. Create a new `try_audio_reencode_to_mp4()` function with Strategy B flags
4. In `resolve_hls_video()`, try A → validate with ffprobe → if bad, try B → validate → if bad, fall back to C
5. Cache which strategy succeeded per URL hash so repeat plays are instant
6. Fix the misleading comments to match actual behavior

**Also add FFmpeg cancellation support:**
- Track active FFmpeg `Child` process handles in a `HashMap<String, std::process::Child>` on `AppState`
- Expose `cancel_video_resolve(url: String)` Tauri command that kills the FFmpeg process
- Clean up partial output files on cancel
- Document the new command in `progress.md` so Frontend can wire it to scroll-away cleanup

**Target:** Video starts within 1–2s (Strategy A), smooth playback. Full re-encode only for genuinely broken streams.

**Reference:** See `systems-perf-analysis-2026-03-17.md` for full trace.

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

### P4 — Batch Blob Resolution — Verify & Document ⚡⚡

**Systems Advisory HP-1. Single biggest feed render speed improvement.**

**⚠️ SYSTEMS ANALYSIS (2026-03-17): This command ALREADY EXISTS.**

`resolve_blob_urls_batch` is already implemented at `commands/media.rs` line ~1021 (takes `Vec<String>`, returns `HashMap<String, String>`). It is already registered in `lib.rs`. **The Frontend agent just needs to wire it up — the backend work is done.**

**Your job this sprint:**
1. **Verify it works end-to-end** — invoke it manually with test blob hashes, confirm it returns correct URLs
2. **Document the exact interface in `progress.md`** so the Frontend agent can call it:
   - Command name: `resolve_blob_urls_batch`
   - Input: `{ sources: string[] }` — array of media source strings (blob hashes, URLs, paths)
   - Output: `HashMap<String, String>` — input → resolved `http://127.0.0.1:{port}/{filename}` URL
   - Behavior: cache-first for blobs, passthrough for http/https/asset URLs, Iroh export fallback for uncached blobs
3. **Performance test** — batch 20 blob hashes, measure IPC round-trip time vs 20 individual `get_media_blob` calls

**Expected improvement:** Feed image load from ~20 IPC calls → 1 IPC call. Estimated **10–20× speedup** on image-heavy feeds.

**Reference:** See `systems-perf-analysis-2026-03-17.md` §L3.

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

### P8 — Extend FeedCache to Subscribed & Custom Feeds ⚡⚡

**Systems Analysis (2026-03-17): §L4**

**Problem:** `FeedCache` only caches the "timeline" sub-query (30s TTL) inside `get_timeline_unified`. The Subscribed tab and custom feed fetches in `get_unified_feed` always hit the Bluesky network — no caching at all. With 45-second auto-refresh on the frontend, this means every refresh cycle re-fetches all custom feeds from the network.

**Fix:**
1. In `get_unified_feed` (called by Subscribed tab), add cache keys:
   - `feed:{feed_uri}:{cursor}` — cache each custom feed response (30s TTL, same as timeline)
   - `hashtag:{tag}:{cursor}` — cache hashtag feed responses
2. Only cache first-page requests (`cursor == None`), same pattern as timeline cache
3. Use the existing `FeedCache` infrastructure — this should be straightforward

**Expected improvement:** Subscribed tab re-loads drop from 1–3s network → <200ms cache hit.

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
- [ ] `resolve_blob_urls_batch` tested end-to-end, interface documented in `progress.md`
- [ ] `FeedCache` extended to cover subscribed/custom feeds (30s TTL)
- [ ] `cancel_video_resolve` command exposed for FFmpeg cancellation
- [ ] Cargo warnings reduced to 0 (or near-0)
- [ ] `export_vault_data` tested end-to-end, interface documented
- [ ] Consistency check job runs on startup (background, non-blocking)
### Build verification
- [ ] `cargo check` — 0 errors
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `pnpm test -- --run` — all pass
- [ ] All terminals closed
- [ ] Committed to `main` with semantic messages
