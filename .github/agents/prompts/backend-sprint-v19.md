# Backend Sprint v19 — Profile Backend Support Reactivation

**Sprint**: v19 (Profile Customization Support)  
**Track**: A (Backend — Rust)  
**Date**: 2026-04-10  
**HEAD at prompt time**: `1b59005`  
**Status**: Reissued because the profile sprint was skipped while prompt work drifted into hosted-identity planning  

---

## Sprint Context

Frontend is re-entering the profile customization sprint. Backend support should stay small and focused: enable the profile customization work and prepare cleanly for the later hosted-identity publish phase.

### Current backend baseline

These profile commands already exist and should be reused:

- `save_profile_settings`
- `get_profile_settings`
- `save_profile_page`
- `get_profile_pages`
- `delete_profile_page`
- `delete_vault_item`

The current codebase does **not** have the additional v19 helper commands the frontend sprint can benefit from:

- `save_avatar_image`
- `export_profile_data`

Do not jump ahead into hosted-identity server work in this sprint.

---

## P0: Avatar Storage Command

Add a focused command for saving avatar image data locally so the profile UI can support avatar customization safely.

### New command

```rust
#[tauri::command]
async fn save_avatar_image(
    state: State<'_, AppState>,
    image_data: Vec<u8>,
    mime_type: String,
) -> Result<String, String>
```

### Requirements

1. Accept only image formats the frontend can safely preview: png, jpeg, gif, webp.
2. Reject oversized payloads.
3. Store avatars under the app data directory in a stable local location.
4. Return a local path / asset URL the frontend can render.
5. Keep the implementation simple and local-first.

### Files likely involved

| File | Work |
|------|------|
| `src-tauri/src/commands/profile.rs` | Add `save_avatar_image` implementation |
| `src-tauri/src/lib.rs` | Register the command |

---

## P1: Export Profile Data For Later Publish Flow

Add a composite export path so the later hosted-identity sprint can publish profile data without reconstructing it ad hoc.

### New command

```rust
#[tauri::command]
async fn export_profile_data(
    state: State<'_, AppState>,
) -> Result<ProfileExport, String>
```

### Export shape

Bundle the current profile state into one serializable structure:

- profile settings
- profile pages
- saved section/widget data
- avatar reference or data
- export timestamp

The exact struct shape can be pragmatic, but it must be stable and usable by future publish code.

### Important constraint

The profile now inherits the app's active system theme. Do not introduce backend support for a separate profile-only theme system. Export only what the frontend genuinely persists.

### Files likely involved

| File | Work |
|------|------|
| `src-tauri/src/commands/profile.rs` | Add `export_profile_data` and any helper structs |
| `src-tauri/src/lib.rs` | Register the command |

---

## Guardrails

### Do

- Keep this sprint small and surgical.
- Reuse existing profile persistence tables and command patterns.
- Make the export path reflect the real saved frontend profile model.

### Do not

- Do not start the hosted-identity server work here.
- Do not re-scope this sprint into relay / PDS / DID publishing.
- Do not invent new profile-theme backend state if the frontend is inheriting the active app theme.

---

## Verification

Run all of these before handing back work:

```bash
cd bytescape/src-tauri && cargo check
cd bytescape && npx tsc --noEmit
cd bytescape && pnpm test -- --run
```

---

## Track Boundaries

- Track A owns `src-tauri/src/`.
- No React UI work in this sprint.
- Keep shared-contract changes minimal if frontend needs a small type update alongside these commands.