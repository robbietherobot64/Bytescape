# Codebase Guide: Top To Bottom

This guide walks through the real execution order of the app.

Use this when you want a complete mental map before editing.

## 1. Startup Path

Read in this order:

1. bytescape/index.html
2. bytescape/src/main.tsx
3. bytescape/src/providers/DataProviderContext.tsx
4. bytescape/src/App.tsx

What happens:

- index.html provides the root mount point.
- main.tsx mounts App and injects DataProviderRoot.
- DataProviderRoot defaults to TauriDataProvider.
- App.tsx initializes backend services and global listeners.

Why it matters:

- Most global side effects and app-level commands are started in App.tsx.

## 2. Navigation And Screen Shell

Primary file:

- bytescape/src/store/index.ts

What to learn:

- NavStore drives active screen and tab behavior.
- AppStore and AuthStore hold global status and identity context.
- FeedCacheStore supports stale-while-revalidate behavior.

How this connects:

- App.tsx reads and updates these stores, and screens consume slices.

## 3. Frontend Data Access Layer

Read files:

1. bytescape/src/providers/DataProvider.ts
2. bytescape/src/providers/TauriDataProvider.ts

What to learn:

- DataProvider defines typed method contracts.
- TauriDataProvider maps methods to invoke command strings.

Important caveat:

- Some screens call invoke directly instead of provider methods.
- This mixed style is functional but can create contract drift.

## 4. Tauri Command Registration Layer

Primary file:

- bytescape/src-tauri/src/lib.rs

What to learn:

- tauri::generate_handler! is the command allow-list.
- Missing registration means invoke calls fail even if function exists.

Related index file:

- bytescape/src-tauri/src/commands/mod.rs

What mod.rs does:

- groups command modules and re-exports them for registration.

## 5. Command Implementation Layer

Primary folder:

- bytescape/src-tauri/src/commands/

Examples:

- vault.rs: file and thumbnail operations
- import.rs: archive and Bluesky import operations
- auth.rs: identity and session operations

What to look for:

- command signatures
- state access (sync manager, db manager)
- return types and error mapping

## 6. Storage Layers

There are two major storage concerns:

1. Structured records in redb
2. Blob metadata and content in Iroh-backed sync layer

Key files:

- bytescape/src-tauri/src/db/mod.rs
- bytescape/src-tauri/src/sync.rs

Practical model:

- redb stores app records and queryable entities.
- sync layer stores vault file metadata and blob references.

## 7. Media Preview Path (Worked Map)

Frontend entry:

- bytescape/src/screens/ProfileScreen.tsx -> loadMedia

Commands:

1. vault_list_files
2. vault_get_thumbnail

Backend wiring:

- registration: bytescape/src-tauri/src/lib.rs
- implementation: bytescape/src-tauri/src/commands/vault.rs
- storage access: bytescape/src-tauri/src/sync.rs

Type contract:

- bytescape/src/types/index.ts -> interface VaultFile

## 8. Debugging Strategy For Any Path

Use this repeatable flow:

1. Reproduce and record exact symptoms.
2. Trace entry point in screen or hook.
3. Identify command names and payloads.
4. Confirm registration and implementation.
5. Validate returned type contract.
6. Patch minimally.
7. Run verification commands.

## 9. High-Value Beginner Paths

Start with these systems first:

1. ProfileScreen and VaultScreen media handling
2. useUnifiedFeed and FeedScreen state flow
3. DataProvider and command mapping integrity

Defer until later:

- deep ActivityPub protocol internals
- advanced Iroh sync edge cases
- large refactors across multiple subsystems

## 10. Quick Reference Table

| Task | Start File | Then Check |
|---|---|---|
| Missing media preview | bytescape/src/screens/ProfileScreen.tsx | commands/vault.rs, sync.rs |
| Command not found | bytescape/src/providers/TauriDataProvider.ts | lib.rs registration |
| Type mismatch crash | bytescape/src/types/index.ts | Rust struct in sync.rs/models |
| Feed odd behavior | hooks/useUnifiedFeed.ts | store/index.ts and FeedScreen.tsx |

Use this table when you are blocked and need a fast restart point.
