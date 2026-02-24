# Bytescape AI Agent Instructions

## Architecture: Engine & Lens Pattern

Bytescape is a **local-first, P2P social vault** using an unbundled "Engine & Lens" architecture:

- **Engine** (`src-tauri/src/`): Rust backend managing data, P2P (Iroh), and protocol bridges
- **Lens** (`src/`): React/TypeScript UI consuming the engine via Tauri commands

All platform data (Bluesky, Facebook archives) is normalized into **BytescapeItem** - a universal container with three types: `Status`, `Gallery`, `Article` (see [db.rs](bytescape/src-tauri/src/db.rs#L57-L95)).

## Development Commands

```bash
cd bytescape
pnpm install          # Install frontend deps
pnpm tauri dev        # Run dev mode (compiles Rust + Vite HMR)
pnpm tauri build      # Production build
```

Frontend runs on port **1420** (Vite). Rust errors show in terminal, not browser.

## Frontend-Backend Communication

**Pattern**: React calls Rust via `invoke()` from `@tauri-apps/api/core`:
```typescript
// Frontend: invoke a Tauri command
const posts = await invoke<BlueskyPost[]>('get_timeline', { limit: 50 });
```

```rust
// Backend: define the command in lib.rs
#[tauri::command]
async fn get_timeline(state: State<'_, AppState>, limit: i32) -> Result<Vec<BlueskyPost>, String> { ... }
```

Commands are registered in [lib.rs#L1170-L1200](bytescape/src-tauri/src/lib.rs#L1170-L1200). Events use `listen()` (frontend) and `app.emit()` (backend) for real-time updates.

## Key Data Flows

1. **Bluesky Integration**: `auth.rs` handles OAuth → `connectors/bluesky.rs` fetches feeds/DMs → normalized to `BytescapeItem`
2. **Archive Import**: User uploads ZIP → `connectors/facebook.rs` parses → staged in `pending_import` → `commit_memories` persists to redb
3. **P2P Sync**: `sync.rs` uses Iroh Docs for CRDT-based sync, `iroh-blobs` for media, `iroh-gossip` for presence

## Code Organization

### Rust Backend (`src-tauri/src/`)
| File | Purpose |
|------|---------|
| `lib.rs` | Tauri commands, app state, P2P listener |
| `db.rs` | redb tables, `BytescapeItem`, `Message`, `Contact` schemas |
| `auth.rs` | Bluesky session management |
| `connectors/` | Platform-specific adapters (bluesky.rs, facebook.rs) |
| `sync.rs` | Iroh Docs/Blobs/Gossip integration |
| `keychain.rs` | Secure credential storage |

### Frontend (`src/`)
| Directory | Purpose |
|-----------|---------|
| `screens/` | Top-level views (FeedScreen, ChatScreen, ImportScreen) |
| `components/` | Reusable UI (Sidebar, VideoPlayer, SourceBadge) |
| `types/index.ts` | TypeScript interfaces mirroring Rust structs |

## Conventions

- **Types must match**: TypeScript interfaces in `types/index.ts` must mirror Rust `#[derive(Serialize, Deserialize)]` structs
- **Error handling**: Tauri commands return `Result<T, String>` - frontend catches errors in try/catch
- **Token refresh**: Bluesky commands retry with `refresh_session()` on 401/ExpiredToken
- **Source tracking**: All `BytescapeItem` records include `source: Source` enum (Bluesky, Facebook, etc.)

## Testing

```bash
cd bytescape
pnpm test             # Run Vitest (frontend unit tests)
```

Test setup in [setupTests.ts](bytescape/src/setupTests.ts). Use `@testing-library/react` for component tests.

## Adding a New Connector

1. Create `src-tauri/src/connectors/{platform}.rs`
2. Add variant to `Source` enum in [connectors/mod.rs](bytescape/src-tauri/src/connectors/mod.rs)
3. Normalize data to `BytescapeItem` schema
4. Register commands in `lib.rs` invoke_handler

## CSS Variables

UI uses a dark theme with CSS custom properties defined in [App.css](bytescape/src/App.css#L1-L24). Key tokens: `--bg-primary`, `--accent-purple`, `--text-primary`.

When you finish reading this document, review the skills that are available to you in `.github/skills.md`. 