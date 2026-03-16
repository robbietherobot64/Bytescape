# Systems Advisory — Architecture Audit & Strategic Recommendations

> **From:** Systems Agent  
> **To:** PM Agent  
> **Date:** 2026-03-15  
> **Scope:** Full codebase audit — Rust backend, React frontend, data flows, P2P stack, dependency graph  
> **Full analysis:** [TECHNICAL_SPEC.md § Systems Analysis & Strategic Roadmap](../../TECHNICAL_SPEC.md)

---

## Executive Summary

Bytescape's core architecture is sound. The Engine & Lens separation, the `BytescapeItem` universal container, and the Iroh P2P stack are structurally solid. The Bluesky integration is production-grade. Identity and auth are the most mature subsystem.

However, the project has accumulated **structural debt** that will compound if not addressed before the next feature push. The three most impactful issues are: (1) no secondary database indexes creating a performance ceiling, (2) a 6,478-line monolithic CSS file causing constant merge conflicts, and (3) per-blob IPC calls creating a feed rendering bottleneck. All three have clear fix paths.

Below are prioritized recommendations organized for PM sprint planning.

---

## Part 1: Quick Wins — Assign Immediately

These are verified, low-risk, high-value items that can be dropped into any agent's current sprint with minimal context.

### QW-1. Window Maximized on Launch ⏱️ 5 min
**Track:** Frontend  
**Problem:** `"maximized": true` is missing from `tauri.conf.json`. Was added in `4989f10`, lost during 20-commit churn. User has flagged this in feedback as a reverted issue.  
**Fix:** Add `"maximized": true` to the window config in `tauri.conf.json`. One line.  
**Verification:** Launch app → window fills screen.

### QW-2. Wire Export Button ⏱️ 30 min
**Track:** Frontend  
**Problem:** Backend `export_vault_data` command exists and is registered (183 lines, `commands/export.rs`). Frontend button shows `alert('coming soon')`. User feedback explicitly flags this.  
**Fix:** Replace the alert with `invoke("export_vault_data", { include_media: false })` + Tauri save dialog for output path. The backend returns a ZIP file path.  
**Verification:** Settings → Export → saves a ZIP with vault JSON data.

### QW-3. Remove Dead Dependencies ⏱️ 5 min
**Track:** Frontend  
**Problem:** `wouter` (router library) is installed in `package.json` but never imported anywhere. Navigation is entirely Zustand-driven. Dead weight in `node_modules`.  
**Fix:** `pnpm remove wouter`.  
**Verification:** `pnpm test` still passes. App still launches.

### QW-4. Cargo Warning Cleanup ⏱️ 1 session
**Track:** Backend  
**Problem:** 27 compiler warnings (unused imports, dead code, unused variables). No errors. Warnings obscure real issues during development.  
**Fix:** `cargo clippy --fix` + manual review of remaining items.  
**Verification:** `cargo check 2>&1 | grep warning | wc -l` → 0.

---

## Part 2: High-Priority Architecture Fixes

These address structural bottlenecks that are already affecting user experience or developer velocity.

### HP-1. Batch Blob Resolution 🔧 Priority: P1
**Track:** Backend + Frontend  
**Impact:** Single biggest feed render speed improvement. Currently, each image in the feed triggers an individual Tauri `invoke()` call to resolve its blob hash to a URL. A feed page with 20 images = 20 sequential IPC round-trips.  
**Proposed fix:**
- **Backend:** New command `resolve_blob_urls_batch(hashes: Vec<String>) → HashMap<String, String>`. Checks blob cache in a single pass, returns all URLs at once.
- **Frontend:** Update `useBlobUrl` hook (or add `useBlobUrls`) to batch-resolve all media hashes for visible feed items in one call.
- **Expected improvement:** Feed image load from ~20 IPC calls → 1 IPC call. Estimated 3-5× improvement on image-heavy feeds.
- **Effort:** 1 session backend, 1 session frontend.

### HP-2. CSS Modularization 🔧 Priority: P1
**Track:** Frontend (or Content — whoever owns CSS)  
**Impact:** `App.css` is 6,478 lines containing 4 complete theme definitions + all component styles + all responsive breakpoints. This is the #1 contributor to merge conflicts across agents. Every sprint, multiple agents touch this file and cause integration headaches.  
**Proposed approach:**
1. Extract theme definitions into `src/themes/{geocities,win98,midnight,hacker}.css` (one file per theme)
2. Extract component-specific styles into CSS Modules (`.module.css` per component) for the largest components
3. Keep a thin `src/global.css` for resets, CSS custom properties, scrollbar styles, and shared layout
4. Update `main.tsx` to import the new files
- **Effort:** 2-3 sessions. Low risk — CSS changes don't affect logic.

### HP-3. FeedEntryCard Decomposition 🔧 Priority: P2
**Track:** Frontend  
**Impact:** `FeedEntryCard.tsx` is 684 lines handling media grids, video, link previews, quote posts, engagement bar, audience pills, syndication drawer, threads, and reactions. It's not wrapped in `React.memo`, so parent re-renders cascade to every card in the feed.  
**Proposed approach:**
1. Extract sub-components: `MediaGrid`, `EngagementBar`, `SyndicationDrawer`, `ThreadReplies`, `QuotePostEmbed`
2. Wrap `FeedEntryCard` in `React.memo` with a custom comparator (compare by item `id` + `is_liked` + `like_count`)
3. This also makes each sub-component independently testable
- **Effort:** 2 sessions.

---

## Part 3: Performance & Scale Concerns

These don't cause visible problems today but will become blockers as vault sizes grow.

### PS-1. Database Query Performance Ceiling 🔧 Priority: P2
**Track:** Backend  
**Problem:** All 13 redb tables use `TableDefinition<&str, &str>` — string key to JSON string value. Every non-key query requires a full table scan: iterate all rows → deserialize JSON → filter → sort. The `header_cache` (in-memory copy of all item headers) mitigates feed pagination but is invalidated on every write.  
**Scale concern:** At 1,000 items this is fast. At 50,000 items (realistic for a user with multi-year history + archives), feed loads will become multi-second.  
**Options (choose one):**
1. **Secondary index tables in redb** — e.g., `ITEMS_BY_SOURCE`, `ITEMS_BY_DATE` with composite keys. Lowest risk, stays within redb.
2. **SQLite for the index layer** — move structured queries to SQLite (proper indexes, joins, full-text search), keep redb for simple KV (keychain, preferences). More powerful but larger migration.
3. **Accelerate Willow migration** — the Willow store provides path-native querying. The migration contract (Willow paths + parity auditing) is already in place.
- **Recommendation:** Option 1 for near-term, with Option 3 as the long-term target. Don't jump to SQLite unless secondary indexes prove insufficient.
- **Effort:** 3-5 sessions for Option 1.

### PS-2. Cross-Store Consistency 🔧 Priority: P3
**Track:** Backend  
**Problem:** `BytescapeItem.media_blobs[]` references Iroh blob hashes, but there's no transactional guarantee between redb and Iroh. Three failure modes:
  - redb row deleted → Iroh blob survives → **orphan blob** (wasted storage)
  - Iroh blob lost → redb row survives → **broken reference** (missing image in feed)
  - Crash mid-import → partial state
**Proposed fix:** Background consistency-check job. On startup (after 10s delay), compare all blob hashes referenced in `ITEMS_TABLE` against Iroh blob inventory. Log orphans. Flag broken references. Optionally auto-heal by removing broken references from items.  
**Effort:** 1-2 sessions.

### PS-3. Archive Import Memory Pressure 🔧 Priority: P3
**Track:** Backend  
**Problem:** Large archive ZIPs (multi-GB Facebook exports) are processed entirely in memory before results are returned. No chunking, no streaming, no resume-on-crash.  
**Proposed fix:** Chunked import pipeline — process N items at a time, commit each chunk to redb, emit progress events per chunk. Use `tokio::task::spawn_blocking` for CPU-intensive thumbnail generation.  
**Effort:** 2-3 sessions.

---

## Part 4: Strategic Opportunities for Next Phase

These are feature-level recommendations based on impact analysis. They inform sprint planning priorities, not immediate assignments.

### SO-1. RSS/Atom Connector (New Data Source)
**Why now:** The `feed-rs` crate is mature and well-maintained. The ingestion pipeline (`BytescapeItem` + Willow path assignment + redb storage) is proven with 3 connectors. Adding RSS would be the 4th connector with minimal new infrastructure — it follows the exact same pattern as Bluesky timeline import.  
**Impact:** Opens Bytescape to blog/newsletter/podcast consumption. Many users who care about data sovereignty also care about RSS.  
**Effort estimate:** 2-3 sessions (Backend Track A).  
**Dependencies:** None. Can be done in parallel with everything else.

### SO-2. ActivityPub Live Connection (A1.2)
**Why now:** The scaffold exists — 7 modules, ~1,271 lines of AP types, HTTP signatures, inbox/outbox, WebFinger. But there's no live connection to any Mastodon/Lemmy instance. Completing A1.2 doubles the federated network reach.  
**Impact:** Unlocks Mastodon/Lemmy/Threads interop. Unlocks several Track B and Track C items that are blocked on AP.  
**Effort estimate:** 5-8 sessions (Backend Track A).  
**Recommendation:** Start with read-only Mastodon feed ingestion. Follow/Accept and posting can come after.

### SO-3. Background Syndication Queue
**Why now:** `PendingSyndication` status exists in the `BytescapeItem` schema. `share_history` event sourcing is built. But there's no queue processor — syndication is currently one-at-a-time, user-triggered. If a user wants to post a batch of vault items to Bluesky, they must click "Syndicate" on each one individually.  
**Impact:** Enables "bulk publish" workflows. Required for future multi-network posting (Bluesky + AP simultaneously).  
**Effort estimate:** 2-3 sessions (Backend Track A).

### SO-4. AI-Enhanced Local Vault Search (Research)
**Why this is special:** Because all data is private and local, an on-device embedding model (ONNX Runtime or Hugging Face `candle`) could power semantic search ("find that photo from the trip to Maine") without any data ever leaving the device. This aligns perfectly with Bytescape's zero-knowledge philosophy and would be a genuine differentiator.  
**Status:** Research phase. No code yet.  
**Next step:** Spike session to evaluate `candle` crate for on-device embeddings. Test with ~1,000 `BytescapeItem.content` fields. Measure indexing time and query latency.

---

## Part 5: User Feedback Cross-Reference

Mapping current user feedback items to the findings in this audit:

| User Feedback Item | Systems Finding | Recommended Action |
|--------------------|----------------|-------------------|
| "Not opening in full screen" | `tauri.conf.json` missing `"maximized": true` (lost in commit churn) | **QW-1** — one-line fix |
| "Feed takes tens of seconds to refresh" | Multi-layer caching added (Sprint v3) but blob resolution is still N sequential IPC calls | **HP-1** — batch blob resolution |
| "Customize is janky and nonsensical" | GrapesJS integration works but UX flow is disconnected from profile rendering | Needs UX redesign — outside Systems scope, but architecture supports the desired flow |
| "Social module is the weakest" | Current implementation is a profile browser with redundant address bar | Needs PM-driven redesign sprint. Backend has the data (`list_follows`, `list_peers`, contacts) |
| "Export button added but not functionality" | Backend command exists (183 lines), frontend button shows `alert()` | **QW-2** — wire the invoke call |
| "P2P Connections under Settings — is this relevant?" | It's the Iroh doc sync UI. User already paired via node ID, so this is redundant for P2P chat | Consider removing or collapsing into a "Device Sync" section |

---

## Part 6: Guardrail Notes for PM

### Observed Debt from Previous Sprints
1. **Terminal discipline:** Sprint v3 audit noted ~170+ zombie terminals causing a crash. Agents must close terminals when done.
2. **Commit hygiene:** Frontend agent had 20 commits in Sprint v3, many bundling unrelated changes. Enforce "one logical change per commit."
3. **Track boundary violations:** Frontend agent created `commands/browser.rs` (Track A file). Backend agent re-added `header_cache` without PM coordination. Guardrails need reinforcement in sprint prompts.
4. **CSS contention:** `App.css` is the most-contended file across all agents. **HP-2 (CSS modularization)** would eliminate this conflict vector permanently.

### Codebase Health Snapshot
- **Rust:** 0 errors, 27 warnings, ~22,925 lines across 46 files
- **TypeScript:** 0 errors, ~12,000 lines
- **Tests:** 250/250 passing (frontend). Rust tests pass. Coverage gaps on major screens (FeedScreen, ChatScreen, VaultScreen untested).
- **Git state:** Clean working tree, HEAD at `09ab5a4` on `main`
- **Dead dependencies:** `wouter` (npm), possibly `hls.js` (npm — HLS handled in Rust)

---

## Part 7: Suggested Sprint v5 Structure

Based on this audit, I'd recommend the PM structure Sprint v5 around two themes:

### Theme A: "Polish What Exists" (2-3 sessions per agent)
- **Frontend:** QW-1 (maximized), QW-2 (export wiring), QW-3 (remove wouter), HP-2 (CSS split), HP-3 (FeedEntryCard decomposition)
- **Backend:** QW-4 (cargo warnings), HP-1 (batch blob resolution), PS-2 (consistency check job)
- **Content:** Help with HP-2 (CSS modularization — Content owns ThemeCustomizer, the heaviest CSS contributor)

### Theme B: "Expand Reach" (5+ sessions per agent)
- **Backend:** SO-1 (RSS connector) or SO-2 (AP live connection) — pick one, not both
- **Frontend:** Social module redesign (user's #1 complaint about weak functionality)
- **Content:** Profile builder UX rework (user wants in-place editing, not a separate builder screen)

**My recommendation:** Do Theme A first. The polish items directly address 4 of the 6 active user feedback items. Ship those, get user confirmation, then start Theme B with a stable foundation.

---

*End of advisory. The full technical analysis with diagrams, data flow traces, and dependency risk tables is in [TECHNICAL_SPEC.md](../../TECHNICAL_SPEC.md) § Systems Analysis & Strategic Roadmap.*
