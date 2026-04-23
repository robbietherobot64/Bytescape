# Lesson 3: Media Debug Playbook (Teach-First)

## What You Will Learn

This lesson teaches a reliable debugging protocol for media preview issues.

By the end, you should be able to:

1. Reproduce missing preview behavior consistently.
2. Identify whether failure is data, command, or render lifecycle.
3. Choose the smallest safe fix path.

## Why This Lesson Exists

Media bugs are easy to make worse if you patch blindly.

This app already has active memory-stability constraints, so you need a method that avoids reintroducing heavy fallback behavior.

## The Three-Layer Debug Model

When a preview is missing, test these layers in order.

Layer 1: Data readiness

- Is the file an image?
- Does it have thumbnail_hash?

Layer 2: Command response

- Does vault_get_thumbnail return bytes, null, or error?

Layer 3: UI lifecycle

- Is Blob URL created?
- Is Blob URL revoked before render/use?

If you skip this order, you risk patching the wrong layer.

## Known Relevant Files

- bytescape/src/screens/ProfileScreen.tsx
- bytescape/src/screens/VaultScreen.tsx
- bytescape/src-tauri/src/commands/vault.rs
- bytescape/src-tauri/src/sync.rs

## Practical Walkthrough

## Step 1: Record Ground Truth

Capture:

- number of media files loaded
- number of image files
- number of image files with thumbnail_hash

Interpretation:

- If thumbnail_hash is missing for most broken images, inspect generation/backfill path first.

## Step 2: Inspect Thumbnail Command Behavior

For a broken file id:

1. Check command call payload in frontend.
2. Follow to vault_get_thumbnail in Rust.
3. Observe return path (Some bytes vs None).

Interpretation:

- None is valid but indicates no usable thumbnail available for that file state.

## Step 3: Inspect Blob URL Lifecycle

In frontend:

- verify URL.createObjectURL is reached
- verify URL.revokeObjectURL is not called too early

Interpretation:

- Early revoke can create intermittent missing previews.

## Step 4: Choose Fix Strategy Using This Matrix

| Observation | Likely Cause | Safe Fix Direction |
|---|---|---|
| image has no thumbnail_hash | metadata lacks thumbnail reference | improve on-demand thumbnail generation reachability |
| thumbnail_hash exists, bytes are null | blob not found or failed export/read | harden backend thumbnail retrieval path |
| bytes exist, preview absent | UI condition or URL lifecycle | patch frontend render/URL lifecycle minimally |
| only first batch displays | batch cap or selection logic | adjust batch or candidate logic cautiously |

## What Not To Do

1. Do not immediately restore full-image fallback in grid flows.
2. Do not add broad logging everywhere.
3. Do not modify unrelated media paths until root cause is confirmed.

## Verification Discipline

After patching:

1. Re-run exact reproduction case.
2. Run compile and tests.
3. Confirm no new obvious memory-pressure behavior.

Commands:

```bash
cd /home/korendian/Bytescape/bytescape
(cd src-tauri && cargo check)
npx tsc --noEmit
pnpm test -- --run
```

## Self-Check (With Answers)

Questions:

1. Why test data layer before UI layer?
2. Why is full-image fallback risky in this context?
3. What does vault_get_thumbnail returning None usually mean?

Answers:

1. Because UI can only render what data and command layers actually provide.
2. It can reintroduce heavy memory and IPC pressure for large image sets.
3. No available thumbnail data for that file state (missing, generation failed, or inaccessible blob).
