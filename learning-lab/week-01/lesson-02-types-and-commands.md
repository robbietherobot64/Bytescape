# Lesson 2: Type Contracts and Command Boundaries

## Why This Matters
In this project, many subtle bugs come from contract drift between Rust models and TypeScript interfaces.

## Learning Objective
Understand where contracts live, how to compare them, and how to detect mismatch risk quickly.

## Files To Study
- bytescape/src/types/index.ts
- bytescape/src-tauri/src/models/
- bytescape/src-tauri/src/lib.rs
- bytescape/src/providers/TauriDataProvider.ts

## Core Concepts
- Rust structs are serialized and returned through Tauri commands.
- TypeScript interfaces must match field names, nullability, and shape.
- Optional in Rust (Option<T>) generally maps to nullable or optional in TypeScript.
- A command can succeed at transport level but still break UI if shape assumptions are wrong.

## Guided Exercise
1. Pick one command used by profile or vault screens.
2. Find the Rust return type.
3. Find the matching TypeScript interface.
4. Compare fields and note any mismatch risk.
5. Record one potential bug that could appear from mismatch.

## Practical Red Flags
- Field exists in Rust but not TypeScript
- TypeScript expects non-null but backend can return null
- Array element shape differs
- Enum or discriminated type not aligned

## Exit Ticket
Add one real command contract comparison to progress-tracker.md and write whether it looks safe or risky.
