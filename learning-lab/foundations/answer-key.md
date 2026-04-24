# Foundations Exercise Answer Key

Open only after attempting each exercise.

## Exercise 01

Part A
1. string, no reassignment
2. number, reassignment allowed
3. boolean[], no reassignment of variable binding

Part B
1. string value, null, missing field/undefined
2. because thumbnails are not guaranteed for every file and strict string would force invalid assumptions

Part C
1. visual
2. non-visual

Part D
1. files.filter(f => f.kind === 'Image' || f.kind === 'Video')
2. files.map(f => f.id)
3. files.find(f => f.size > 150)

## Exercise 02

Part A
1. function returns a Promise
2. pauses that function until Promise settles
3. it rejects upward as unhandled unless a caller catches it

Part B
1. reset to safe fallback data
2. guarantee loading reset on success and failure
3. loading UI can get stuck forever

Part C
1. independent requests can run concurrently for speed
2. memory and resource pressure from too many requests
3. keeps concurrency controlled while gaining speed
4. run async work for each item in batch and wait until all complete

## Exercise 03

Part A
1. expose command to frontend invoke
2. public visibility
3. asynchronous function
4. success or error; success contains optional byte vector

Part B
1. Err(String)
2. Ok(None)
3. Ok(Some(Vec<u8>))

Part C
1. convert missing Option to Result error lazily
2. return early with error
3. concise guard clause for required dependencies

## Exercise 04

Part A
1. ProfileScreen.tsx, loadMedia
2. vault_list_files
3. src-tauri/src/lib.rs
4. src-tauri/src/commands/vault.rs, vault_list_files

Part B
1. VaultFile
2. src/types/index.ts
3. src-tauri/src/sync.rs

Part C
1. command error: backend operation failed
2. success with no thumbnail: valid call but no thumbnail available
3. success with bytes: thumbnail data returned and renderable
