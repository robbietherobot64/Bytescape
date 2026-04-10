# Backend Sprint v19 — Profile Backend Support

**Sprint**: v19 (Profile Polish)  
**Track**: A (Backend — Rust)  
**Date**: 2026-04-09  
**Previous sprint**: v18a delivered backend profile commands (commit `97dda2c`)  
**HEAD at prompt time**: `8f8070b`  

---

## Sprint Context

The v18 frontend was reverted, but all v18a backend commands remain intact and functional:

| Command | Status | Purpose |
|---------|--------|---------|
| `save_profile_settings` | ✅ Shipped | JSON blob persistence |
| `get_profile_settings` | ✅ Shipped | Load profile settings |
| `save_profile_page` | ✅ Shipped | Page CRUD |
| `get_profile_pages` | ✅ Shipped | List pages |
| `delete_profile_page` | ✅ Shipped | Delete page |
| `delete_vault_item` | ✅ Shipped | Vault post deletion |

This is a **light sprint** — two incremental additions.

---

## P0: Avatar Storage Command

The frontend sprint adds avatar URL support. Currently `save_profile_settings` stores the full settings blob including any `avatar_url`. However, if the user uploads an actual image file (not just a URL), we need a command to store it as a blob and return a local path.

### New command: `save_avatar_image`

```rust
#[tauri::command]
async fn save_avatar_image(
    state: State<'_, AppState>,
    image_data: Vec<u8>,
    mime_type: String,
) -> Result<String, String> {
    // 1. Validate mime_type is image/* (png, jpg, gif, webp only)
    // 2. Validate image_data size < 5MB
    // 3. Write to app_data_dir/avatars/{hash}.{ext}
    // 4. Return the local file path as asset:// URL
}
```

Register in `lib.rs` invoke_handler.

### Files to modify

| File | Changes |
|------|---------|
| `src-tauri/src/lib.rs` | Add `save_avatar_image` command + registration |

---

## P1: Profile Data Export Composite

Prepare for v20 (go-live) by adding a composite export command that bundles the user's profile into a single JSON structure — this is what gets published to their domain.

### New command: `export_profile_data`

```rust
#[tauri::command]
async fn export_profile_data(
    state: State<'_, AppState>,
) -> Result<ProfileExport, String> {
    // 1. Load profile_settings (theme, background, widgets, marquee)
    // 2. Load profile_pages (all pages with content)
    // 3. Load widget_data for each widget
    // 4. Load avatar (if local, include as base64 data URL)
    // 5. Bundle into ProfileExport struct
}

#[derive(Serialize, Deserialize)]
struct ProfileExport {
    settings: serde_json::Value,
    pages: Vec<serde_json::Value>,
    widgets: Vec<serde_json::Value>,
    avatar: Option<String>,  // data URL or external URL
    exported_at: String,     // ISO 8601 timestamp
}
```

This gives the v20 publish flow a single data structure to serialize and push to the user's domain.

### Files to modify

| File | Changes |
|------|---------|
| `src-tauri/src/lib.rs` | Add `export_profile_data` command + `ProfileExport` struct + registration |

---

## Verification

```bash
cd bytescape
cargo check           # 0 errors
npx tsc --noEmit      # 0 errors (types still match)
pnpm test -- --run    # all tests pass
```

---

## Track Boundaries

- **Track A owns**: `src-tauri/src/` (Rust), `src-tauri/Cargo.toml`
- Both new commands are entirely within Track A scope
- No shared file modifications needed
