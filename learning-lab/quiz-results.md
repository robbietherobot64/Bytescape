# Baseline Quiz Results (2026-04-22)

## Score
4/10

## Strengths
- Correct high-level understanding of backend vs frontend roles
- Correct stability-first product prioritization
- Realistic first bugfix selection aligned with roadmap priorities

## Gaps To Close First
- Command registration location in Rust backend
- Type contract location and relationship (Rust <-> TypeScript)
- End-to-end invoke command flow
- Provider abstraction vs concrete provider implementation
- Practical debugging hotspots for media performance and memory behavior

## Correct Reference Points
- Command registration root: bytescape/src-tauri/src/lib.rs
- Shared frontend type contracts: bytescape/src/types/index.ts
- App shell and routing orchestration: bytescape/src/App.tsx
- Provider abstraction: bytescape/src/providers/DataProvider.ts
- Tauri implementation: bytescape/src/providers/TauriDataProvider.ts

## Personalized Week 1 Outcome Target
By end of week 1, you should be able to:
1. Trace a UI action to backend command and return type without help.
2. Explain one real bug path (profile media previews) and identify likely root causes.
3. Propose and implement one small, validated fix.
