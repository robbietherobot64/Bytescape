# Lesson 1: How A UI Action Reaches Rust And Comes Back

## What You Will Learn

This lesson teaches the runtime path for one real action in this app: loading profile media previews.

By the end, you should be able to explain this full chain out loud:

component -> invoke command -> command registration -> command implementation -> storage layer -> serialized response -> UI render.

## Why This Lesson Comes First

When people get stuck in large codebases, it is usually because they cannot follow execution boundaries.
If you can trace one full path reliably, most bugfixes become process work instead of guessing.

## The Core Architecture In Plain Language

- Frontend: React screens, hooks, and state in bytescape/src/
- Bridge: Tauri IPC command layer (string command names)
- Backend: Rust command handlers and storage services in bytescape/src-tauri/src/

You can think of this as: "React asks Rust for data through named commands."

## Step 1: Boot Sequence (Where The App Wiring Starts)

Read these files in order:

1. bytescape/src/main.tsx
2. bytescape/src/providers/DataProviderContext.tsx
3. bytescape/src/App.tsx

What to notice:

- main.tsx wraps App in DataProviderRoot
- DataProviderRoot defaults to TauriDataProvider
- App.tsx owns top-level app orchestration and many direct invoke calls

Important design detail:

This codebase mixes two styles:

1. Provider style (use DataProvider interface)
2. Direct invoke style (screen calls invoke itself)

Both work, but the mix is a common source of confusion for new contributors.

## Step 2: Follow A Real Flow (Profile Media)

Open bytescape/src/screens/ProfileScreen.tsx and find loadMedia.

What loadMedia does:

1. Calls invoke<VaultFile[]>('vault_list_files')
2. Filters to visual files (Image or Video)
3. Builds thumbnail candidates from Image files with thumbnail_hash
4. Calls invoke<number[]>('vault_get_thumbnail', { fileId }) in batches
5. Converts bytes to Blob URLs and stores them in mediaThumbs

Why this matters for stability:

- It avoids full-image fallback in grid loading
- It revokes old Blob URLs to reduce memory growth
- It caps thumbnail batch size to reduce pressure

## Step 3: Find Command Registration

Open bytescape/src-tauri/src/lib.rs and locate tauri::generate_handler![...].

You should find both commands registered there:

- commands::vault::vault_list_files
- commands::vault::vault_get_thumbnail

Key point:

If a Rust command exists but is missing here, invoke will fail even if the function is implemented.

## Step 4: Find Command Implementations

Open bytescape/src-tauri/src/commands/vault.rs.

Look for:

- pub async fn vault_list_files(...)
- pub async fn vault_get_thumbnail(...)

What these do:

- vault_list_files delegates to SyncManager::vault_list_files()
- vault_get_thumbnail fetches metadata, generates thumbnail on demand for image files when needed, then returns thumbnail bytes or None

## Step 5: Follow Into Storage Layer

Open bytescape/src-tauri/src/sync.rs.

Look at:

- struct VaultFile
- async fn vault_list_files(&self)

Storage model summary:

- File metadata is stored as JSON entries in an Iroh doc namespace
- Raw bytes are stored as blobs, referenced by blob_hash
- Thumbnail bytes are separate blobs referenced by thumbnail_hash

This split explains why some files can exist without usable thumbnails.

## Step 6: Find The Frontend Type Contract

Open bytescape/src/types/index.ts and locate interface VaultFile.

Important fields for this flow:

- id
- kind
- thumbnail_hash
- blob_hash

If these drift from Rust definitions, the UI can render wrong assumptions even when commands return success.

## Worked Example Summary

Here is the exact profile media preview chain you just studied:

1. ProfileScreen loadMedia starts request
2. invoke('vault_list_files') returns VaultFile[]
3. UI filters and selects Image records with thumbnail_hash
4. invoke('vault_get_thumbnail', { fileId }) requests bytes
5. Rust command reads/generates thumbnail bytes
6. UI converts bytes to Blob URL and renders image preview

## Common Failure Points In This Flow

1. Missing or null thumbnail_hash on many imported images
2. vault_get_thumbnail returning None for specific files
3. Blob URL lifecycle issues (revoked too early)
4. Command registered but parameter shape mismatch

## Checkpoint (With Answer Key)

Questions:

1. Which command lists profile media files?
2. Which command fetches thumbnail bytes?
3. Where are these commands registered?
4. Which frontend type receives list results?

Answer key:

1. vault_list_files
2. vault_get_thumbnail
3. bytescape/src-tauri/src/lib.rs in generate_handler
4. VaultFile in bytescape/src/types/index.ts

## Application Task (Short, Not Open-Ended)

Write this exact 5-line summary in progress-tracker.md:

1. Entry point file and function
2. Two command names
3. Registration file
4. Implementation file
5. Type contract file

Do not move to Lesson 2 until this summary is complete.
