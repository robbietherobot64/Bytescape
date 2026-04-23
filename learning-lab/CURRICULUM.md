# Bytescape Curriculum (Teach-First)

This is the structured learning path for becoming an effective contributor.

The program is organized by capability, not by random tasks.

## Program Objectives

By the end of this curriculum, you should be able to:

1. Read and trace runtime flows across frontend and backend.
2. Diagnose common contract and command-boundary bugs.
3. Implement small-to-medium fixes safely with verification.
4. Add simple features without destabilizing core paths.

## Learning Principles

1. Explanation before implementation
2. Worked examples before independent work
3. Small diffs before broad refactors
4. Verification before merge

## Phase Structure

## Phase 1 (Week 1): Runtime Foundations

Focus: understand how the app starts, routes data, and serves media to UI.

Lessons:

1. Frontend-to-backend runtime flow
2. Type contracts and command boundaries
3. Media debugging playbook
4. Guided project: profile media previews

Outcome:

- You can trace command flow and execute one stability fix process end-to-end.

## Phase 2 (Week 2): Feed and State Mechanics

Focus: understand feed data lifecycle and client state architecture.

Core files to study:

- bytescape/src/hooks/useUnifiedFeed.ts
- bytescape/src/store/index.ts
- bytescape/src/screens/FeedScreen.tsx

Lessons:

1. Feed fetch modes and pagination model
2. Cache behavior and stale-while-revalidate
3. UI state boundaries (tab and screen persistence)

Application lab:

- Debug one feed behavior issue (stale data, ordering, source filter, or pagination).

Outcome:

- You can safely inspect and modify feed behavior without breaking cache invariants.

## Phase 3 (Week 3): Storage and Data Model Internals

Focus: understand how data is represented, persisted, and retrieved.

Core files:

- bytescape/src/types/index.ts
- bytescape/src-tauri/src/models/
- bytescape/src-tauri/src/db/mod.rs
- bytescape/src-tauri/src/sync.rs

Lessons:

1. Universal item shape and platform normalization
2. redb table patterns and item retrieval flow
3. Vault metadata versus blob content storage

Application lab:

- Add one safe read-only command or query enhancement.

Outcome:

- You can reason about data shape changes and persistence side effects.

## Phase 4 (Week 4): Command Development Workflow

Focus: create and wire one small command from Rust to UI.

Lessons:

1. Add command function in Rust command module
2. Register command in lib.rs
3. Add TypeScript contract and frontend integration
4. Verify compile, types, tests, and manual behavior

Application lab:

- Build one small utility command and display it in a screen.

Outcome:

- You can deliver a small feature from backend to UI with full verification.

## Phase 5 (Week 5): Stability and Regression Control

Focus: become reliable at fixing high-priority bugs without collateral damage.

Lessons:

1. Reproduction discipline and scope framing
2. Hypothesis-driven debugging
3. Minimal-fix strategy
4. Regression-proof verification checklist

Application lab:

- Pick one open stability issue and run full investigation-to-validation loop.

Outcome:

- You can complete real bugfixes with confidence and clear validation artifacts.

## Phase 6 (Week 6+): Contributor Readiness and Collaboration

Focus: move from guided learning to independent contribution in FOSS workflows.

Lessons:

1. Branch strategy and atomic commits
2. Review-ready PR notes
3. Test hygiene and documentation updates
4. Handling feedback and follow-up patches

Outcome:

- You can contribute in small, reviewable increments aligned with roadmap priorities.

## Weekly Schedule Template (6-10 hours)

1. Session A (90 min): concept lesson
2. Session B (90 min): guided code walkthrough
3. Session C (2-3 hr): application lab
4. Session D (60 min): reflection and review notes

## Verification Standard For Any Code Change

Run from /home/korendian/Bytescape/bytescape:

```bash
(cd src-tauri && cargo check)
npx tsc --noEmit
pnpm test -- --run
```

## Progress Measurement

Use progress-tracker.md to log:

1. What subsystem was learned
2. What flow was traced
3. What bug was reproduced
4. What was changed
5. What was verified
6. What remains uncertain
