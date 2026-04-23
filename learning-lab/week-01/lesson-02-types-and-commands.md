# Lesson 2: Type Contracts And Command Boundaries

## What You Will Learn

This lesson teaches how to validate data contracts between Rust and TypeScript so you can detect bugs before runtime.

By the end, you should be able to compare one command path and confidently answer:

- Is the return shape aligned?
- Is nullability aligned?
- Is parameter naming aligned?

## Why This Is A High-Leverage Skill

In this app, many bugs are not logic failures. They are contract failures:

- backend returns field as optional
- frontend assumes field always exists
- UI silently renders placeholders, blanks, or crashes in edge cases

## Where Contracts Live

Use this lookup map:

1. TypeScript contracts: bytescape/src/types/index.ts
2. Provider method signatures: bytescape/src/providers/DataProvider.ts
3. Command string mapping: bytescape/src/providers/TauriDataProvider.ts
4. Command registration: bytescape/src-tauri/src/lib.rs
5. Rust command handlers: bytescape/src-tauri/src/commands/
6. Rust data structs: bytescape/src-tauri/src/sync.rs and bytescape/src-tauri/src/models/

## Worked Comparison: VaultFile

Frontend definition:

- bytescape/src/types/index.ts -> interface VaultFile

Backend definition:

- bytescape/src-tauri/src/sync.rs -> struct VaultFile

Field alignment highlights:

- id: string <-> String
- name: string <-> String
- mime_type: string <-> String
- kind: VaultFileKind <-> VaultFileKind enum
- size: number <-> u64
- blob_hash: string <-> String
- thumbnail_hash?: string | null <-> Option<String>
- item_id?: string | null <-> Option<String>
- created_at?: number | null <-> Option<i64>

This pair is mostly aligned and is a good baseline example.

## Command Boundary Example

Profile media flow commands:

1. vault_list_files
2. vault_get_thumbnail

Where each boundary is defined:

- direct frontend invoke in bytescape/src/screens/ProfileScreen.tsx
- registration in bytescape/src-tauri/src/lib.rs
- implementation in bytescape/src-tauri/src/commands/vault.rs

## Important Real-World Caveat In This Codebase

You will see both naming styles for input params:

- fileId in frontend invoke payloads
- file_id in Rust function parameters

Tauri command argument binding generally handles this path today, but inconsistent naming is a maintenance risk and should be treated carefully when adding or refactoring commands.

## Red Flags Checklist

Use this checklist before changing command contracts:

1. Backend field added but TS interface not updated
2. Rust Option<T> changed but TS still non-null
3. Enum variants changed without TS union update
4. Provider method returns any instead of typed contract
5. Direct invoke call bypasses provider contract and drifts

## Safe Change Procedure (Use Every Time)

When you touch a command or returned model:

1. Update Rust struct or return type
2. Update command handler if shape changed
3. Update registration only if command name changed
4. Update TypeScript interface
5. Update provider signature or direct invoke typing
6. Run:
	- (cd src-tauri && cargo check)
	- npx tsc --noEmit
	- pnpm test -- --run

## Checkpoint (With Answer Key)

Questions:

1. Which file defines the frontend VaultFile contract?
2. Which file defines the backend VaultFile struct used by vault commands?
3. Which file maps many camelCase provider methods to snake_case invoke command names?
4. Which file registers commands into Tauri invoke_handler?

Answer key:

1. bytescape/src/types/index.ts
2. bytescape/src-tauri/src/sync.rs
3. bytescape/src/providers/TauriDataProvider.ts
4. bytescape/src-tauri/src/lib.rs

## Application Task (Constrained)

Perform one contract check and log results in progress-tracker.md using this template:

- Command reviewed:
- Frontend type file:
- Backend type file:
- Nullability risk found (yes/no):
- Parameter naming risk found (yes/no):
- Final judgment (safe/risky):

Keep this short and factual.
