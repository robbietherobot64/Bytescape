---
applyTo: '**'
---

# Agent Guardrails — Operational Rules for All Agents

These rules are **mandatory** for every agent (PM, Frontend, Backend, Content). Violations of these rules caused the Sprint 3 anomalous build (commit `eef398e`). Do not repeat them.

---

## 1. The User Never Edits Code

The user communicates **exclusively** through:
- The chat prompt box
- The `user-feedback` file at `.github/agents/prompts/user-feedback`

**The user does not:**
- Edit source code files (`.ts`, `.tsx`, `.rs`, `.css`, `.json`, etc.)
- Create or delete source files
- Run terminal commands
- Make git commits

If you see uncommitted changes in the working tree, **do not attribute them to the user.** Investigate their source:
1. Check `git log` — did another agent commit them?
2. Check `git stash list` — are they stashed changes from a crashed session?
3. Check if another agent modified the same files in a recent commit (cross-contamination)
4. If you cannot determine the source, treat them as **anomalous** — do not build plans around them.

**NEVER** write documentation that says "the user reverted X" or "the user prefers Y over Z" based on file state. The user's preferences come from the `user-feedback` file and chat messages — nowhere else.

---

## 2. Source of Truth for User Intent

| Source | Authority Level | Example |
|--------|----------------|---------|
| Chat prompt (current session) | **Highest** — direct instruction | "Make the profile tabs align left" |
| `user-feedback` file | **High** — persistent feedback | "Posts are too narrow in height" |
| File state on disk | **None** — never treat as user intent | Code changes are always from agents |

When user feedback conflicts with current code state, the **user feedback wins**. The code may be wrong because an agent implemented something the user didn't ask for, or implemented it incorrectly.

---

## 3. Cross-Agent File Conflicts

**Problem:** In Sprint 3, the Frontend agent and Backend agent both modified `useUnifiedFeed.ts`. The Backend agent committed on top of Frontend agent's changes without knowing the Frontend agent had already changed the file. Result: inconsistent state.

**Rules:**
- Before editing a file, run `git log --oneline -5 -- <filepath>` to check if another agent recently modified it.
- If another agent modified the file in the **current sprint**, coordinate through the PM or leave a comment in `progress.md` explaining your changes.
- **Never modify files outside your track** unless your sprint prompt explicitly authorizes it. Track boundaries:
  - **Frontend (Track B)**: `src/` (React/TypeScript), `src/App.css`, `tauri.conf.json` (UI config only)
  - **Backend (Track A)**: `src-tauri/src/` (Rust), `src-tauri/Cargo.toml`
  - **Content (Track C)**: `src/components/ComposeBox*`, `src/components/lenses/*`, `src-tauri/src/models/`, content-related CSS
  - **Shared files** (require PM coordination): `src/types/index.ts`, `src/hooks/useUnifiedFeed.ts`, `src-tauri/src/lib.rs` (command registration)

---

## 4. Commit Hygiene

- **One logical change per commit.** Don't bundle unrelated changes.
- **Commit message format**: `type(track): description` — e.g., `feat(track-b): align profile tabs left`
- **Always verify before committing:**
  1. `cargo check` — 0 errors
  2. `npx tsc --noEmit` — 0 errors
  3. `pnpm test -- --run` — all tests pass
- **Review your diff before committing:** Run `git diff --stat` and `git diff` to verify you're only committing files you intended to change.
- **Do not leave uncommitted changes.** If your session ends, either commit (with passing build) or `git stash` with a descriptive message.

---

## 5. Terminal Discipline

- **Do not spawn unnecessary terminals.** Reuse existing ones.
- **Kill background processes when done.** If you start `pnpm tauri dev` for testing, kill it before your session ends.
- **Avoid `sleep` loops.** If you need to wait for a build, use the build command directly (not background + sleep + grep pattern).
- **Close terminals when you are done with them.** Every terminal you open must be closed before your session ends. Do not leave zombie terminals. If you opened a terminal for a one-off command, close it after the command completes. The PM will kill all remaining terminals during code review — if terminals are still open, that means you left a mess.

---

## 6. PM Responsibilities

The PM agent has additional obligations:

- **Never assume.** When you see uncommitted changes, investigate before acting. Run `git log`, `git reflog`, check for agent commits.
- **Verify agent work against user feedback.** If an agent implemented something the user didn't ask for, or contradicts user feedback, flag it — don't preserve it.
- **Write sprint prompts from user feedback, not file state.** The `user-feedback` file and chat history are the specification. The code on disk is just the current (possibly incorrect) implementation.
- **Document the CURRENT state objectively.** Instead of "User reverted X," write "Current state: X is [description]. User feedback says: [quote]."
- **Pre-sprint checklist:**
  1. Read `user-feedback` file completely
  2. Run build verification (cargo check + tsc + tests)
  3. Check `git log` for recent agent commits
  4. Check for uncommitted changes and investigate their source
  5. Write sprint prompts based on user feedback, referencing current code state objectively

---

## 7. Sprint Workflow

```
PM reads user-feedback + chat
  → PM writes sprint prompts (based on user intent, not file state)
  → Agent reads prompt + explores codebase
  → Agent plans changes (documents in progress.md)
  → Agent implements (atomic commits, build-verified)
  → Agent documents results (progress.md)
  → PM audits results against user feedback
  → PM reports to user via chat
  → User provides feedback in chat / user-feedback file
  → Cycle repeats
```

---

## 8. What Went Wrong in Sprint 3 (Post-Mortem)

For reference, here is the chain of failures so agents understand why these rules exist:

1. **Content agent** (`d6108a8`) added `classification.test.ts` — tested types (`ReclassifyResult`, `Link`/`Video`) that don't exist in the TypeScript type system
2. **Frontend agent** (`4989f10`) made 13 file changes: removed subscribed tab, replaced lenses with filter chips, set full-width layout, added maximized window — **without explicit user request for most of these**
3. **Backend agent** (`c60d784`) added `headers_cache` to `DbManager`, modified `useUnifiedFeed.ts` — a **frontend file** that Frontend agent had already changed (cross-contamination of track boundaries)
4. **PM** saw 12 dirty files in the working tree, **assumed the user edited them**, and wrote sprint v2 prompts around "user design preferences" that never existed
5. **PM** committed "fixes" that created new inconsistencies (e.g., 'subscribed' tab in FeedScreen.tsx but not in FeedTab type)

Every rule above directly prevents one of these failures.
