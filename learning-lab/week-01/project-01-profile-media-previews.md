# Project 1: Profile Media Image Preview Investigation

## Project Goal
Investigate and fix image preview behavior in the profile media section while preserving memory stability.

## Why This Project
- It is directly relevant to active priorities.
- It touches a practical path: media loading, thumbnails, and UI rendering.
- It is scoped enough for a first guided bugfix.

## Target Files (Start Here)
- bytescape/src/screens/ProfileScreen.tsx
- bytescape/src/screens/VaultScreen.tsx
- bytescape/src/components/BlobImage.tsx

## Step-by-Step Plan
1. Reproduce
- Open profile media section with archive-imported images and uploaded images.
- Record exactly which items do not show previews.

2. Trace Data Path
- Identify which command fetches file list.
- Identify which command fetches thumbnail bytes.
- Confirm when fallback behavior is expected or intentionally disabled.

3. Form Hypotheses
- Missing thumbnail_hash for some imported files
- Thumbnail fetch command failing for specific source types
- Blob URL lifecycle revokes too early
- UI path only supports one source shape

4. Add Temporary Instrumentation
- Add focused logs around thumbnail fetch success/failure and file id.
- Avoid broad noisy logs.

5. Validate Hypotheses
- Confirm which hypothesis matches observed behavior.
- Remove temporary logs after learning.

6. Implement Minimal Fix
- Keep memory-safe behavior intact.
- Prefer narrow changes in one file if possible.

7. Verify
Run from /home/korendian/Bytescape/bytescape

```bash
(cd src-tauri && cargo check)
npx tsc --noEmit
pnpm test -- --run
```

8. Document
- What was broken
- Root cause
- What changed
- What remains risky

## Success Criteria
- Profile media previews render for affected images.
- No regression in validation checks.
- No reintroduction of heavy full-image fallback behavior without explicit reason.
