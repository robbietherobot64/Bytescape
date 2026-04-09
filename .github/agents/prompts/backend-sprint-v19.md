# Backend Sprint v19 — Profile Customization Support

**Sprint**: v19 (Profile Polish: Creative Expression)  
**Track**: A (Backend — Rust)  
**Date**: 2026-04-09  
**Previous sprint**: v18a (vault deletion, profile settings, pages CRUD — delivered)  
**HEAD at prompt time**: `b7fca4e`  
**Scope**: Light sprint — most v19 work is frontend. Backend provides avatar storage and ensures profile data completeness for future publish.

---

## Sprint Context

Frontend (Track B) is deepening profile customization: custom backgrounds, marquee ticker, avatar images, GIF sections, custom colors, expanded connections, and richer page editing. Most of these store data as new keys in the `ProfileSettings` JSON blob, which already works via `save_profile_settings`/`get_profile_settings` — no backend schema changes needed.

Backend scope is limited to:
1. **Avatar image storage** — save a local image file and return a path the frontend can render
2. **Profile data completeness** — ensure all profile data is available for the v20 publish API

### What already exists

| Component | File | Status |
|-----------|------|--------|
| Profile settings persistence | `commands/profile.rs` | ✅ Generic JSON blob — new fields just work |
| Profile pages CRUD | `commands/profile.rs` | ✅ Working |
| Vault item deletion | `commands/vault.rs` | ✅ Working |
| Widget data (bio sections) | `lib.rs` commands | ✅ `get_widget_data` / `save_widget_data` |
| Follows/contacts listing | `list_follows`, `list_contacts` commands | ✅ Working |
| Linked accounts | `list_linked_accounts` command | ✅ Working |
| Media path resolution | App data dir + `convertFileSrc` pattern | ✅ Working (used by feed) |

---

## P0: Avatar Image Storage

### Problem

Frontend needs to store a profile avatar image locally and render it. Currently the profile shows a letter avatar only. While the frontend can use URL-based avatars (external links), users should also be able to pick a local file.

### New command: `save_profile_avatar`

```rust
#[tauri::command]
pub async fn save_profile_avatar(
    state: State<'_, AppState>,
    image_path: String,
) -> Result<String, String>
```

**Behavior:**
1. Read the source image from `image_path` (an absolute path from a file picker dialog)
2. Copy it to `{app_data_dir}/profile/avatar.{ext}` (preserve extension: jpg, png, gif, webp)
3. Return the relative storage path (e.g., `profile/avatar.png`) — frontend resolves via `convertFileSrc`

### New command: `get_profile_avatar`

```rust
#[tauri::command]
pub async fn get_profile_avatar(
    state: State<'_, AppState>,
) -> Result<Option<String>, String>
```

Returns the relative path to the stored avatar if it exists, `None` otherwise.

### File operations

- Create `{app_data_dir}/profile/` directory if it doesn't exist
- Overwrite existing avatar on re-upload
- Validate file extension (only allow jpg, jpeg, png, gif, webp)
- Validate file size (reject > 5MB)

### Register in `lib.rs`

Add `save_profile_avatar` and `get_profile_avatar` to the invoke handler.

### Files to modify

| File | Changes |
|------|---------|
| `src-tauri/src/commands/profile.rs` | Add `save_profile_avatar` and `get_profile_avatar` commands |
| `src-tauri/src/lib.rs` | Register new commands |

---

## P1: Profile Data Export for Publish (Pre-work for v20)

### Problem

When v20 (go-live) ships, `publish_identity` will need to send a complete profile payload (display name, bio, avatar, links, theme, etc.) to the server. Ensure all this data is easily retrievable in a single call.

### New command: `get_full_profile_data`

```rust
#[tauri::command]
pub async fn get_full_profile_data(
    state: State<'_, AppState>,
) -> Result<FullProfileData, String>
```

Returns a composite struct:

```rust
#[derive(Serialize)]
pub struct FullProfileData {
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub web_did: Option<String>,
    pub avatar_path: Option<String>,
    pub profile_settings: Option<String>,  // JSON blob
    pub widget_data: Option<String>,       // Bio sections JSON
    pub pages: Vec<ProfilePageData>,
    pub linked_accounts: Vec<LinkedAccountData>,
}
```

This aggregates data from keychain, profile settings, widget data, pages, and linked accounts into one response. The v20 publish command will use this internally, and the frontend can use it for a "profile preview" before publishing.

### Files to modify

| File | Changes |
|------|---------|
| `src-tauri/src/commands/profile.rs` | Add `get_full_profile_data` command |
| `src-tauri/src/lib.rs` | Register new command |

---

## Verification

Before committing:

```bash
cd bytescape
cargo check                          # 0 errors
npx tsc --noEmit                     # 0 errors
pnpm test -- --run                   # all tests pass
```

Document all changes in `progress.md`.

---

## Track Boundaries

- **Track A owns**: `src-tauri/src/` (Rust commands)
- **Track B owns**: `src/` (React/TypeScript, CSS)
- **Shared files** (require PM coordination): `src/types/index.ts` if new TypeScript interfaces are needed for new commands
