# Contributing

How work happens in this project — the rules, the loop, and where to plug in.

## Ground rules

- **One precedence chain:** `VISION.md` > `wiki/product/mvp-scope.md` (ship
  gates) > `wiki/product/prd.md` (archived reference). Before building
  anything, check the ship gates — nothing ships before its shelf gate.
- **Design decisions come from the user** — never inferred. Open questions get
  asked, not assumed.
- **Small fixes and investigation skip the spec flow.** Everything
  non-trivial goes through it.

## The change loop

1. **Shape the idea** — brainstorm first if it needs dialogue.
2. **Propose** — an OpenSpec change folder in `openspec/changes/<name>/` with
   a proposal (why/what/non-goals), spec deltas (behavior contracts), design
   notes, and a task list.
3. **Implement** — tasks are checked off with their verify criteria; review
   before showing results.
4. **Verify with exercised evidence** — builds green is necessary, not
   sufficient: the changed surface is demonstrated (real browser runs for UI,
   live queries for data paths). Wording/implementation tests that re-pin
   incidental behavior get deleted, not kept.
5. **Land it** — update `STATUS.md` (living truth) and add a dated line to
   `DECISIONS.md` for any decision; then sync the change's specs into
   `openspec/specs/` and archive the change.

## Quality conventions

- **Explain-first** for code changes: a plain-language brief *before* code
  (what/why/where/risks), a diff walkthrough after, and an explain-back to
  confirm the change is understood — green tests alone are not delivery.
- **Tests must catch consumer-visible bugs** — behavior, boundaries,
  invariants, transitions. Never tautologies, wiring echoes, or
  "doesn't throw".
- **Deletes are real** — records leave the repository, not just the UI.
- **One decision, one dated line** in `DECISIONS.md`; current phase and next
  actions live in `STATUS.md`.

## The toolchain

| Tool | Role |
|---|---|
| **omp** | The agent harness this project develops in |
| **OpenSpec** | Planning layer — durable behavior contracts per change |
| **Compound Engineering** | Implementation layer — work, review, compounding learnings |
| **CodeDebrief** | Optional: code-flow explanations, release-time model refresh |
| **Obsidian** | The repo root is the vault; markdown is the whole doc surface |

## Where to plug in

Work is coordinated in the project's GitLab repository. To see where things
stand: read [STATUS.md](../STATUS.md) for the current phase, then the active
change folders in `openspec/changes/` — each has a proposal and task list
showing exactly where a change stands. Sprint picture:
[Current work](current-work.md).
