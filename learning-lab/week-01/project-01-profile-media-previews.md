# Project 1: Guided Lab - Fix Profile Media Previews Safely

## Lab Goal

Restore missing image previews in profile media while preserving current memory-safety constraints.

This lab is intentionally guided. Follow it in order.

## What You Are Practicing

1. Reproduction discipline
2. Flow tracing discipline
3. Hypothesis testing discipline
4. Minimal-fix implementation discipline
5. Verification discipline

## Files You Will Touch First

1. bytescape/src/screens/ProfileScreen.tsx
2. bytescape/src/screens/VaultScreen.tsx
3. bytescape/src-tauri/src/commands/vault.rs
4. bytescape/src-tauri/src/sync.rs

You may not need to edit all four; they define the full path.

## Phase A: Reproduce Precisely

1. Launch app with pnpm tauri dev
2. Open profile media section
3. Compare behavior for:
	- uploaded images
	- archive-imported images
4. Record counts in this template:

- total visual items:
- previews shown:
- previews missing:
- missing item ids (at least 5):

Do not code yet.

## Phase B: Observe Data Reality

Inspect the flow in ProfileScreen loadMedia and answer these exact checks:

1. Is the item filtered out before thumbnail fetch?
2. Does it have kind === 'Image'?
3. Does it have thumbnail_hash?
4. Does vault_get_thumbnail return bytes or null?

Use narrow temporary logging only around those checks.

## Phase C: Decision Tree (Do Not Skip)

Choose one branch based on your observations.

Branch 1: Many broken items have no thumbnail_hash

- Likely issue: metadata lacks generated thumbnail references.
- Investigate thumbnail generation behavior in backend path.
- Candidate fix: ensure on-demand thumbnail generation path is consistently reached.

Branch 2: thumbnail_hash exists but command returns null/error

- Likely issue: thumbnail blob missing, parse issue, or export/read path failure.
- Investigate vault_get_thumbnail implementation and cache export behavior.

Branch 3: bytes returned but preview still missing

- Likely issue: frontend URL lifecycle or render condition bug.
- Inspect Blob URL creation and revocation timing.

Branch 4: only first N items show previews

- Likely issue: batch cap behavior or selection logic.
- Inspect MEDIA_THUMB_BATCH and filters around thumbnail targets.

## Phase D: Implement A Minimal Fix

Rules:

1. Keep fix small and local.
2. Do not reintroduce full-image fallback in grid path unless explicitly justified.
3. Preserve Blob URL cleanup behavior.
4. Add one short comment only if logic is non-obvious.

## Phase E: Validate

Run from /home/korendian/Bytescape/bytescape:

```bash
(cd src-tauri && cargo check)
npx tsc --noEmit
pnpm test -- --run
```

Manual validation checklist:

1. Missing previews now render for affected set
2. UI remains responsive when opening media tab
3. No obvious increase in memory pressure from fallback behavior

## Phase F: Write A Short Lab Report

Add this to progress-tracker.md:

- Reproduction summary
- Branch selected from decision tree
- Root cause statement (1-2 lines)
- Files changed
- Verification command results
- Residual risk

## Completion Standard

This lab is complete only when:

1. You can explain why previews were missing.
2. You can explain why your fix is safe for memory.
3. Validation commands pass.
