# Backend Agent — Sprint: UF Completion & Feed Perf (2026-03-15)

## 🚨 PM AUDIT FINDINGS — READ THIS FIRST

### ⚠️ Process Violations

1. **You committed A1.1 AP Actor code to `track-c/c1-facet-frontend` (the Content agent's branch) instead of `main`.** Commit `7c58c40` is an A1.1 AP Actor commit on Track C's branch. You then also committed/cherry-picked it as `a4e4062` on `main`, creating duplicates that required merge fixup (`28a6b8c`). **Always verify your current branch with `git branch --show-current` before committing.**

### ✅ Verified Completions
- **A1.1 AP Actor** ✅ — 8 files, 1,636 lines, 22 tests, all verified. Excellent work.
- **UF.13 Facebook Import** Partial — Extensive filtering added (50+ directory exclusions, lifecycle/activity filtering). 2,620 lines in facebook.rs. Needs user verification.
- **UF.3 Unified Contacts** Partial — ContactManager syncs Bluesky follows/followers + P2P capability detection. Does NOT include imported contacts from archives.
- **UF.16 Feed Performance** Partial — Parallel feed fetches implemented, dedup improved. But NO caching, NO progressive loading, NO timing instrumentation. Feed still returns full merged payload in one shot.

### Current State
All branches merged to **`main`** at `0f9f0a6`. All work happens on `main` going forward.

---

## Priority 1 — Complete UF.16: Feed Performance (Caching + Progressive Loading) ⚡

Your parallel fetch work was a good start, but the user still experiences multi-second stalls. The missing pieces:

1. **Cached-first rendering**: When `get_timeline` / `get_unified_feed` is called, IMMEDIATELY return locally-cached vault items (items already stored from previous fetches). Then kick off the API fetch in the background and emit a `feed-updated` event when fresh data arrives. The frontend can then re-render with the updated feed.
   ```rust
   // Pseudo-flow:
   // 1. Query vault for cached items → return immediately
   // 2. Spawn tokio task: fetch from Bluesky API
   // 3. On success: merge new items into vault, emit app.emit("feed-updated", new_items)
   ```

2. **Timing instrumentation** (gated behind `#[cfg(debug_assertions)]`): Wrap each step in `std::time::Instant` measurements and log them:
   - Token retrieval duration
   - API request duration
   - Deserialization duration
   - Vault merge/dedup duration
   - Total wall time

3. **Loading indicator event**: Emit `feed-loading-start` before API call, `feed-loading-end` after. Frontend can show a spinner.

**Target**: Feed should show content within 500ms (cached), with fresh data arriving within 3-5s.

---

## Priority 2 — Complete UF.3: True Unified Contacts

Your ContactManager currently only handles Bluesky follows/followers + P2P capability. Extend it:

1. **Import contacts from archives**: When a Facebook/Instagram archive is imported, extract any contact-like entities (friends list, group members, conversation participants) and add them to the unified contacts table.
2. **P2P peer contacts**: Peers discovered via Iroh should also appear in the contacts list, not just as a P2P capability flag.
3. **Dedup across sources**: Same person may appear as a Bluesky follow AND a Facebook friend AND a P2P peer. Dedup by DID where available, by display name + handle as fallback.

---

## Priority 3 — A1.2 Research Notes

Now that A1.1 is complete, document what A1.2 (Basic Mastodon/Lemmy Support) will require. Don't implement — research and note:
- What Mastodon API endpoints are needed for feed reading, posting, DMs
- How Mastodon auth works (OAuth 2.0 with app registration)
- Whether our ed25519 HTTP signatures will interop (Mastodon uses RSA by default)
- What activity types Lemmy uses that differ from Mastodon

Write findings in your progress.md update.

---

## Anti-Patterns
- Do NOT modify frontend components (`src/`) — Track B owns those
- Do NOT modify `ComposeBox.tsx` or content components — Track C owns those
- Do NOT commit to feature branches — all work is on `main` now
- **Always run `git branch --show-current` before committing**
- Do NOT hallucinate crate imports — check Cargo.toml before using
- Do NOT serve HTTP endpoints yet — `bytescape.social` is not live. Build AP logic as local Tauri commands

---

## Definition of Done (This Sprint)
- [ ] `get_timeline` / `get_unified_feed` returns cached items in <500ms, fresh data arrives via event
- [ ] Debug timing instrumentation logs each pipeline step duration
- [ ] `list_contacts` returns unified list: Bluesky follows + P2P peers + imported contacts
- [ ] Dedup logic handles same person across multiple sources
- [ ] A1.2 research notes added to progress.md
- [ ] `cargo check` passes (0 errors)
- [ ] `pnpm test` passes
- [ ] Committed to `main` with semantic message
