# Week 3 Plan: Storage And Data Model

## Objective
Learn the data model and persistence boundaries so you can reason about backend changes safely.

## Lessons
1. Data shape and normalization
   - File focus: bytescape/src/types/index.ts and bytescape/src-tauri/src/models/
   - Outcome: map key app entities across frontend and backend

2. redb table patterns
   - File focus: bytescape/src-tauri/src/db/mod.rs
   - Outcome: explain where records are stored and queried

3. Vault metadata and blob architecture
   - File focus: bytescape/src-tauri/src/sync.rs and bytescape/src-tauri/src/commands/vault.rs
   - Outcome: explain metadata vs binary blob responsibilities

## Application Lab
Implement one small read-focused backend improvement with typed frontend wiring.

## Definition Of Done
- You can trace one backend query end-to-end.
- You can compare TS and Rust contracts without guesswork.
- You complete one compile-safe and type-safe backend-linked change.
