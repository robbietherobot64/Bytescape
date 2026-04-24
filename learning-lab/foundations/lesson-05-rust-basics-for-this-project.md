# Lesson 05: Rust Basics For This Project

This is not full Rust training. It is the minimum Rust you need to read command handlers confidently.

## Part A: Function Signature Anatomy

Example shape used in this app:

```rust
#[tauri::command]
pub async fn vault_list_files(state: State<'_, AppState>) -> Result<Vec<VaultFile>, String> {
    // ...
}
```

Meaning:

1. #[tauri::command] exposes function to frontend invoke calls.
2. pub means function is publicly accessible to command registration.
3. async fn means it can await async operations.
4. state: State<'_, AppState> injects app state from Tauri runtime.
5. Result<Vec<VaultFile>, String> means success or error string.

## Part B: Option and Result

Two core Rust enums you will constantly read:

1. Option<T> = Some(value) or None
2. Result<T, E> = Ok(value) or Err(error)

Why this matters in Bytescape:

- thumbnail retrieval can be Ok(None) when no thumbnail exists.
- command call can be Err("...") when backend operation fails.

## Part C: Common Methods In This Codebase

1. .await for async operations
2. .map_err(|e| e.to_string()) to convert errors
3. .ok_or_else(...) to convert missing Option into Result error

Example pattern:

```rust
let sync = sync_guard.as_ref()
    .ok_or_else(|| BytescapeError::SyncNotInitialized)?;
```

Meaning:

- if sync_guard has no value, return an error immediately.

## Part D: Matching Branches

```rust
match file.thumbnail_hash {
    Some(thumb_str) => { /* use thumbnail */ }
    None => Ok(None),
}
```

This is Rust branch logic similar to if/else but for enum variants.

## Part E: Practical Reading Strategy

When reading a Rust command function:

1. Read the return type first.
2. Identify failure exits (Err, ?, ok_or_else).
3. Identify success path data shape.
4. Confirm how that shape maps to TypeScript interface.

## Quick Check

1. What is the difference between Option and Result?
2. Why does map_err appear so often in command handlers?
3. What does the ? operator do when an error occurs?
