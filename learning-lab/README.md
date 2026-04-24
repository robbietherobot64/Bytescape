# Bytescape Learning Lab

This is now a teach-first curriculum, not a loose task list.

The goal is to teach programming fundamentals first, then teach the codebase from top to bottom.

Learning order is now:
coding basics -> app startup -> app shell -> state and providers -> Tauri command bridge -> backend commands -> storage and sync -> debugging and bugfixing.

## What You Get In Every Lesson

Every lesson file includes four blocks:

1. Concept lecture (what the subsystem does and why it exists)
2. Guided code walkthrough (exact files and call paths)
3. Worked example (real flow traced in this repository)
4. Application checkpoint (small scoped action after the teaching)

You should never have to "wander the code" with no direction.

## Read Order

1. CURRICULUM.md
2. foundations/README.md
3. foundations lessons in order
4. foundations/EXERCISES.md
5. foundations exercises in order
6. CODEBASE_TOP_TO_BOTTOM.md
7. week-01/README.md
8. week-01 lessons in order

## Program Goal

Near-term target: become reliable at fixing stability bugs and adding simple features safely.

Success means you can:

- Understand core coding concepts without guessing
- Explain a frontend-to-backend call path from memory
- Locate command registration and implementation quickly
- Compare Rust and TypeScript contracts without guessing
- Reproduce, isolate, and validate one real bugfix end-to-end

## Time Budget

Planned cadence is 6-10 hours per week.

## Verification Commands

Run from /home/korendian/Bytescape/bytescape

```bash
cd /home/korendian/Bytescape/bytescape
(cd src-tauri && cargo check)
npx tsc --noEmit
pnpm test -- --run
pnpm tauri dev
```

## Current Lab Files

- CURRICULUM.md: full multi-week lesson plan
- foundations/: beginner coding fundamentals track
- foundations/EXERCISES.md and exercise files: mandatory practice track
- CODEBASE_TOP_TO_BOTTOM.md: architecture walkthrough in dependency order
- quiz-results.md: baseline assessment
- progress-tracker.md: session notes and competency gate tracking
- week-01/: detailed lessons and first guided stability project
- week-02/: feed and state lesson plan
- week-03/: storage and data model lesson plan

## Working Agreement

- Learn first, then apply.
- Keep changes minimal and testable.
- Document what changed, why, and how you verified it.
