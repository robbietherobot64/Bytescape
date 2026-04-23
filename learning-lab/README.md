# Bytescape Learning Lab (Temporary)

This folder is your dedicated training workspace.

## Goal
Become capable of fixing real bugs and adding simple features in Bytescape quickly, then grow into deeper systems and architecture work.

## Current Focus
- Stability-first development
- Understand frontend-backend command flow
- Understand Rust and TypeScript type contracts
- Ship one guided bugfix: profile media image previews

## Weekly Cadence (6-10 hours)
- 2 lessons
- 1 guided mini-project
- 1 review and reflection

## Mentor Workflow
1. Read the lesson.
2. Complete the exercise in the lesson.
3. Run and verify with real project commands.
4. Log what you learned in progress-tracker.md.
5. Move to the next lesson only after checkpoints pass.

## Lab Structure
- quiz-results.md: your baseline assessment and skill gaps
- progress-tracker.md: weekly log and competency checklist
- week-01/: first week lessons and project

## Project Verification Commands
Run from /home/korendian/Bytescape/bytescape

```bash
cd /home/korendian/Bytescape/bytescape
(cd src-tauri && cargo check)
npx tsc --noEmit
pnpm test -- --run
pnpm tauri dev
```

## Working Agreement
- Prioritize understanding over speed.
- Keep changes small and verifiable.
- Every coding step should have a reason and a test/check.
