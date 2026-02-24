# Low Hanging Fruit Roadmap (Execution Tracker)

Last updated: 2026-02-17 (live feeds + message import + deletion hardening pass)

This is the working execution roadmap for fast, high-impact improvements with clear deliverables.

## Focused low-hanging scope (current)

### 1) Finish content consistency across Feed, Profile, Vault
- Status: ✅ Completed
- Scope:
  - Video (`VideoPlayer`)
  - Link cards
  - GIFs
  - Audio files
  - PDF/document preview behavior
- Surfaces:
  - Feed: [bytescape/src/screens/FeedScreen.tsx](bytescape/src/screens/FeedScreen.tsx)
  - Profile: [bytescape/src/components/BytescapeItemCard.tsx](bytescape/src/components/BytescapeItemCard.tsx)
  - Vault: [bytescape/src/screens/VaultScreen.tsx](bytescape/src/screens/VaultScreen.tsx)
- Exit criteria:
  - Same media-type behavior and fallback states across all three screens. ✅

- Progress note:
  - Feed now uses `VideoPlayer` and safer external host rendering.
  - Profile card now supports typed media rendering for video/audio/pdf fallback.
  - Vault supports inline PDF/open-document preview fallback and now uses `VideoPlayer` for video preview consistency.
  - Shared content helpers added for hostname/media-kind/gif detection in [bytescape/src/utils/content.ts](bytescape/src/utils/content.ts).
  - Vault upgraded to fullscreen media-first viewer, with optional `File Data` panel and keyboard/arrow navigation.

### 2) Add posting abilities with explicit destination routing
- Status: ✅ Completed (ActivityPub remains placeholder by design)
- Required targets:
  - Local P2P only
  - Bluesky only
  - ActivityPub only
  - Multi-post (`all`) where enabled
- Exit criteria:
  - A single compose flow with explicit destination selector and per-destination confirmation.
  - Backend routes per target capability, with partial-failure reporting.

- Progress note:
  - Compose UI added to feed with destinations: Local P2P, Bluesky, ActivityPub placeholder, All.
  - Backend `publish_post` routes Local + Bluesky now and reports ActivityPub placeholder as explicit not-yet-implemented.

### 3) Unify feed model via adapter layer
- Status: ✅ Completed
- Meaning:
  - Keep source-specific models (`BlueskyPost`, imported item shapes) at boundaries.
  - Convert each source into one UI-facing model (e.g., `FeedEntry`) through adapters.
  - Render Feed/Profile with shared presentation logic against `FeedEntry`.
- Current evidence:
  - Separate models exist in `bytescape/src/types/index.ts` but are adapted via `bytescape/src/utils/adapters.ts`.
- Exit criteria:
  - Feed/Profile rendering consumes one normalized UI model. ✅
  - Source-specific conditionals are localized to adapter functions, not spread through screens. ✅

## Deferred (not low-hanging)

### 4) Focused Rust warning cleanup
- Status: 🟨 In progress

### 5) Two-device sync verification
- Status: ⬜ Not started

## Additional completed sprint items

### A) Live feed behavior for subscribed Bluesky feeds
- Status: ✅ Completed
- Delivered:
  - `Feeds` tab now resolves subscribed feed URIs robustly from Bluesky preferences.
  - `get_custom_feed` uses encoded feed URI/cursor and propagates hard errors when feed fetch fails.
  - `Feeds` and `All` support manual-first refresh with optional background auto-refresh toggle.
- Key files:
  - [bytescape/src-tauri/src/connectors/bluesky.rs](bytescape/src-tauri/src/connectors/bluesky.rs)
  - [bytescape/src/screens/FeedScreen.tsx](bytescape/src/screens/FeedScreen.tsx)

### B) Message import and chat isolation
- Status: ✅ Completed
- Delivered:
  - Message sync now runs during Bluesky import flow and on entering Chat screen.
  - `VaultMessage` is owner-scoped (`owner_did`) to prevent cross-account leakage.
  - Chat UI split into live vs archived conversation sections, with archived read-only composer state.
- Key files:
  - [bytescape/src/App.tsx](bytescape/src/App.tsx)
  - [bytescape/src-tauri/src/db.rs](bytescape/src-tauri/src/db.rs)
  - [bytescape/src-tauri/src/commands/import.rs](bytescape/src-tauri/src/commands/import.rs)
  - [bytescape/src/screens/ChatScreen.tsx](bytescape/src/screens/ChatScreen.tsx)

### C) Deletion pathways hardening
- Status: ✅ Completed
- Delivered:
  - `clear_vault` now clears full user-scoped local data + synced vault file metadata.
  - `logout` now performs full local+sync wipe and clears pending import buffer before session removal.
- Key files:
  - [bytescape/src-tauri/src/commands/vault.rs](bytescape/src-tauri/src/commands/vault.rs)
  - [bytescape/src-tauri/src/commands/auth.rs](bytescape/src-tauri/src/commands/auth.rs)
  - [bytescape/src-tauri/src/db.rs](bytescape/src-tauri/src/db.rs)

### D) Feed layout polish
- Status: ✅ Completed
- Delivered:
  - Fixed create-post/feed spillover so compose box stays above feed posts in one column.
- Key files:
  - [bytescape/src/screens/FeedScreen.tsx](bytescape/src/screens/FeedScreen.tsx)
  - [bytescape/src/App.css](bytescape/src/App.css)

## Recent completed quick wins (already done)
- Add feed pagination model for `Feeds`/`All` with non-disruptive incremental append
- Real timeline cursor wiring:
  - [bytescape/src-tauri/src/connectors/bluesky.rs](bytescape/src-tauri/src/connectors/bluesky.rs#L257-L350)
  - [bytescape/src-tauri/src/commands/import.rs](bytescape/src-tauri/src/commands/import.rs#L378-L387)
- Feed right-side clipping/layout cleanup:
  - [bytescape/src/App.css](bytescape/src/App.css#L1795-L1898)
  - [bytescape/src/App.tsx](bytescape/src/App.tsx#L384-L393)
  - [bytescape/src/screens/FeedScreen.tsx](bytescape/src/screens/FeedScreen.tsx#L165-L166)
- Frontend baseline quality checks enabled (`lint`, tests) in [bytescape/package.json](bytescape/package.json#L6-L14).

## Next execution order
1. Continue Rust warning cleanup and start two-device sync verification
