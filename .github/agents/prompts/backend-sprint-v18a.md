# Backend Sprint v18a — Profile Backend Support

**Sprint**: v18a (Profile Polish — Backend)  
**Track**: A (Backend — Rust/Tauri)  
**Date**: 2026-04-08  
**Previous sprint**: v17 (cross-platform repost, feed density — delivered)  
**HEAD at prompt time**: `3f35751`  
**Context**: Frontend is overhauling the profile. Backend must supply missing commands for post deletion, expanded connections, and profile theme persistence.

---

## Sprint Context

The frontend (Track B) is redesigning the profile across two sprints (v18a layout, v18b rich media). This backend sprint provides the Tauri commands those frontend changes depend on.

### Key files

| File | Lines | Purpose |
|------|-------|--------|
| `src-tauri/src/commands/profile.rs` | 333 | Profile layout/sections/guestbook commands |
| `src-tauri/src/commands/vault.rs` | varies | Vault post commands |
| `src-tauri/src/db/mod.rs` | varies | redb storage tables |
| `src-tauri/src/lib.rs` | ~1200 | Command registration |

---

## P0: Vault Post Deletion Command

### Problem
Frontend needs to delete posts from the profile. Check if `delete_vault_post` or equivalent exists.

### Required
Search for existing delete commands:
```bash
grep -rn "delete.*vault\|remove.*post\|delete.*post" src-tauri/src/
```

If no command exists, implement:

```rust
#[tauri::command]
async fn delete_vault_post(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.delete_item(&id).map_err(|e| e.to_string())?;
    Ok(())
}
```

### Implementation notes
- The `BytescapeItem` table in redb uses `id` as key — implement `delete_item(&self, id: &str)` on `DbManager` if it doesn't exist
- Must work for ALL sources (Bluesky, Facebook, Instagram, local) — deletion is local-only (removes from vault, not from source platform)
- Register in `lib.rs` invoke_handler

### Files to modify
- `src-tauri/src/db/mod.rs` — add `delete_item()` if missing
- `src-tauri/src/commands/vault.rs` (or new command) — add `delete_vault_post`
- `src-tauri/src/lib.rs` — register command

---

## P1: Expanded Connections List

### Problem
Frontend currently calls `list_follows` which returns `{ follows: number, handles: string[] }` with only the first 8 handles. The profile redesign needs ALL connections with metadata for search/filter/pagination.

### Required
Add or expand a command that returns full connection data:

```rust
#[derive(Serialize)]
struct ConnectionEntry {
    handle: String,
    display_name: Option<String>,
    source: String,          // "bluesky", "facebook", "instagram", "p2p"
    relationship: String,    // "following", "follower", "mutual", "contact"
    avatar_url: Option<String>,
}

#[derive(Serialize)]
struct ConnectionsResult {
    connections: Vec<ConnectionEntry>,
    total: usize,
}

#[tauri::command]
async fn list_all_connections(
    state: State<'_, AppState>,
    source: Option<String>,     // filter by source
    search: Option<String>,     // filter by handle/name substring
    limit: Option<usize>,       // pagination
    offset: Option<usize>,      // pagination
) -> Result<ConnectionsResult, String> {
    // Aggregate from:
    // 1. Bluesky follows (from session/cache)
    // 2. P2P contacts (from contacts table)
    // 3. Facebook friends (from vault/import data if available)
    // 4. Instagram followers (from vault/import data if available)
    // Apply source + search filters, paginate
}
```

### Data sources
- **Bluesky follows**: Check how `list_follows` currently fetches them. It likely uses the Bluesky API or cached data.
- **P2P contacts**: `list_contacts` command exists, returns count. Need to expand to return contact entries.
- **Facebook friends**: If imported from archive, they may be in the vault as connections. Check `connectors/facebook.rs` for friend data.
- **Instagram followers**: If imported, check `connectors/instagram.rs`.

### Fallback
If aggregating all sources is complex, start with what's available:
- Return all Bluesky follows (full list, not just first 8)
- Return all P2P contacts
- Mark each entry with its `source`
- Leave Facebook/Instagram connections as a TODO if import doesn't store them as queryable connections

### Files to modify
- `src-tauri/src/commands/profile.rs` (or new `connections.rs`) — add `list_all_connections`
- `src-tauri/src/lib.rs` — register command
- `src/types/index.ts` — add `ConnectionEntry` and `ConnectionsResult` types (coordinate with Track B)

---

## P2: Profile Theme Persistence

### Problem
Frontend is adding theme selection for profiles (midnight, sunset, forest, retro, etc.). The selected theme needs to persist.

### Current state
Profile data is stored via `save_widget_data` / `get_widget_data` which uses the `PROFILE_TABLE` with key `"user_profile_layout"`. Bio sections use `save_profile_sections` / `get_profile_sections`.

### Required
No new command needed if the theme can be stored as part of the existing widget data or sections data. However, it's cleaner to have a dedicated key:

```rust
#[tauri::command]
async fn save_profile_settings(
    state: State<'_, AppState>,
    settings_json: String,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    // Store under key "user_profile_settings" in PROFILE_TABLE
    db.set_profile_data("user_profile_settings", &settings_json)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn get_profile_settings(
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_profile_data("user_profile_settings")
        .map_err(|e| e.to_string())
}
```

Settings JSON structure (defined by frontend, backend just stores opaque JSON):
```json
{
    "theme": "midnight",
    "background_type": "gradient",
    "background_value": null,
    "marquee_enabled": false
}
```

### Alternative
If adding new commands feels heavy, the frontend can store theme settings inside the existing `save_widget_data` payload by adding a `settings` key to the JSON. Check with Track B on their preference.

### Files to modify
- `src-tauri/src/commands/profile.rs` — add `save_profile_settings` / `get_profile_settings`
- `src-tauri/src/db/mod.rs` — ensure `set_profile_data` / `get_profile_data` can handle arbitrary keys (not just `"user_profile_layout"`)
- `src-tauri/src/lib.rs` — register new commands

---

## P3: Linked Accounts Exposure

### Problem
Frontend needs `linked_accounts` from `AuthStatus` to auto-populate the Links section. Verify this is already included in the `check_auth_status` response.

### Check
```bash
grep -n "linked_accounts" src-tauri/src/commands/auth.rs
```

`AuthStatus` should already include `linked_accounts: Vec<LinkedAccount>` per `types/index.ts`. Verify the Rust struct matches and is populated.

If `linked_accounts` is not populated in `check_auth_status`:
1. Collect from keychain — which platforms are linked
2. Return as part of `AuthStatus`

### Files to check/modify
- `src-tauri/src/commands/auth.rs` — verify `linked_accounts` in AuthStatus response
- `src-tauri/src/keychain_encrypted.rs` — verify linked account data is stored and retrievable

---

## Verification

Before committing:

```bash
cd bytescape
cargo check              # 0 errors
cargo test               # all tests pass
npx tsc --noEmit         # 0 errors (TypeScript types must match)
pnpm test -- --run       # frontend tests pass
```

### Test the new commands
```rust
// In src-tauri/tests/ or inline tests:
// 1. delete_vault_post: create item, delete it, verify it's gone
// 2. list_all_connections: verify returns ConnectionsResult with source filtering
// 3. save/get_profile_settings: roundtrip JSON persistence
```

---

## Track Boundaries
- **Track A owns**: `src-tauri/src/` (Rust), `src-tauri/Cargo.toml`
- **Shared files** (coordinate with Track B): `src/types/index.ts` — add `ConnectionEntry`, `ConnectionsResult` types
- Check `git log --oneline -5 -- src/types/index.ts` before editing
- Do NOT modify files in `src/` (Track B territory) except shared types
