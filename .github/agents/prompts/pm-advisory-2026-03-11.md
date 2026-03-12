# PM Advisory — 2026-03-11: Post-Synthesis Findings

> This advisory captures issues identified by the PM agent after synthesizing all three agent status reports (Backend A0, Frontend B1/B2.5, Content C3.2). All agents should read this file when starting a new session.

---

## 🔴 MERGE ALERT: `track-a/a0-bluesky-oauth` → `main`

The Track A branch (`track-a/a0-bluesky-oauth`, 6 commits, +951/-49 across 8 files) has **not yet been merged to main**. When merging:

### Expected Conflict: `ROADMAP.md` at B2.5 Theme Engine

Track A's branch forked from `main` **before** B2.5 was marked complete. Its copy of ROADMAP.md has:
```
- [ ] **B2.5 Theme Engine**: Cozy Mode (default GeoCities aesthetic), Desktop Mode (Win98 classic)...
```

But `main` has the **completed** version:
```
- [x] **B2.5 Theme Engine**: Three built-in themes — **Cozy Mode** (GeoCities default)...
```

**Resolution**: When `git merge track-a/a0-bluesky-oauth` produces a conflict in ROADMAP.md:
1. **Keep `main`'s B2.5 entry** (the `[x]` completed version with full description)
2. **Accept Track A's A0 section** (the new 6-line block inserted after line 134)
3. The A0 insertion and B2.5 update are in different parts of the file, so git may auto-resolve the A0 addition — verify both are present after merge.

### Merge Command Sequence
```bash
git checkout main
git merge track-a/a0-bluesky-oauth
# If conflict in ROADMAP.md:
#   - Edit ROADMAP.md: keep main's B2.5 [x] entry, accept A0 section
#   - git add ROADMAP.md
#   - git commit (accept default merge message)
# Then:
git branch -d track-a/a0-bluesky-oauth
```

---

## ⚠️ Cross-Track Integration Gaps

### OAuth → Write Operations (Backend Agent)
`publish_post_v2` and `syndicate_vault_item` do **not** have `is_oauth_session()` branching yet. OAuth users can read feeds but **cannot post**. The pattern to follow exists in `commands/import.rs` (9 instances of `is_oauth_session()` checks). Apply the same pattern to:
- `commands/posting.rs` — `publish_post_v2`
- `commands/syndicate.rs` — `syndicate_vault_item`

### C3.2 → C1.1b (Content Agent)
`ArticleReader` renders content as sanitized HTML. Once C1.1b (facet rendering frontend) ships, articles containing `@mentions` and `#hashtags` should render them as interactive links. The Content agent should ensure `ArticleReader` uses the same facet-rendering pipeline as `FeedEntryCard`.

### C3.2 → C6.1 (Content Agent)
Tiptap collaboration extensions (`@tiptap/extension-collaboration`, `@tiptap/extension-collaboration-cursor`) are installed but **not wired**. C6.1 (CRDT Storage Layer) is now unblocked. The `yrs` Rust crate needs to be added to Cargo.toml and `collab.rs` created.

---

## 📋 Recommended Next Wave Priority

Based on dependency analysis and unblocking potential:

| Priority | Task | Agent | Rationale |
|----------|------|-------|-----------|
| 1 | **C1.1b + C1.2b + C1.3b** — Facet rendering + autocomplete frontend | Content | Backend done, pure frontend work, high UX impact |
| 2 | **A0-GAP-1 through A0-GAP-7** — OAuth hardening | Backend | Gaps documented in ROADMAP.md Open Issues section |
| 3 | **C6.1** — CRDT storage layer (`yrs` + `collab.rs`) | Content | C3.2 unblocked it, critical path to collaborative editing |
| 4 | **A1.1** — AP Actor + Inbox/Outbox | Backend | Unblocks 6+ downstream items across all tracks |
| 5 | **B2.1** — GrapesJS profile builder | Frontend | Independent, no cross-track blockers |

---

## 📊 Current State Summary (as of 2026-03-11)

- **Branch**: `main` + `track-a/a0-bluesky-oauth` (pending merge)
- **Tests**: 75/75 passing (5 test files: sanitize 22, SourceBadge 2, ComposeBox 12, ThemePicker 5, ArticleEditor 34)
- **TypeScript**: Compiles clean (`npx tsc --noEmit` — zero errors)
- **All security hardening**: Merged and verified on main
- **All stashes**: Dropped (verified superseded)
- **All old feature branches**: Deleted
