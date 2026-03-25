# Backend Sprint v11 — Phase 4.5 Foundation (Track A)

**Sprint Goal:** Begin the server foundation work by completing identity migration (A-S1) and stubbing relay sync commands (A-S2). This sprint prepares the Tauri desktop client for the `bytescape.social` server deployment.

**Context:** Phase 4.5 introduces `bytescape.social` as a unified server hosting AT Protocol PDS + ActivityPub instance + Iroh relay. Before deploying the server, the client-side identity model must be corrected to support the 3-level DID hierarchy (`did:web` → `did:plc` → `did:key`). Full architecture: `.github/agents/prompts/systems-dual-engine-architecture-2026-03-25.md`.

**HEAD:** `29f44fe` on `main` (clean tree). **Build:** 0 tsc errors, 267/267 tests pass.

---

## P1: Identity Migration — Remove `generate_platform_plc()` (A-S1, part 1)

**Priority:** 🔥 Critical — blocks all Phase 4.5 identity work.

**Problem:** The current codebase fabricates `did:plc:` identifiers for non-Bluesky platforms via `generate_platform_plc()` in `keychain_encrypted.rs`. These are not real PLC DIDs registered at `plc.directory` — they are hash-based fabrications that look like DIDs but aren't resolvable. This breaks the identity model.

**Task:**
1. **Find and remove `generate_platform_plc()`** from `keychain_encrypted.rs`. Search for all call sites.
2. **Audit all `LinkedAccount.identifier` values** that were created by this function. These will have `did:plc:` format but are fabricated. Specifically:
   - Mastodon accounts: Should use `acct:user@instance` format
   - Generic AP accounts: Should use the actor URL (`https://instance/@username`)
3. **Write a startup migration function** (in `keychain_encrypted.rs` or a new `migrations.rs`) that:
   - Runs on app startup (before identity is used)
   - Scans all `LinkedAccount` entries
   - For entries with fabricated `did:plc:` identifiers and a Mastodon/AP `platform`:
     - If the account has `server_url` and `username` fields: reconstruct as `acct:{username}@{server_domain}`
     - If the account has an `actor_url` field: use it directly
     - Otherwise: log a warning and leave unchanged (manual fix needed)
   - Persists the migrated keychain
4. **Write tests** for the migration function (at least 3 cases: Mastodon acct migration, AP actor URL migration, already-correct entry left unchanged).

**Files to explore:**
- `src-tauri/src/keychain_encrypted.rs` — `generate_platform_plc()`, `LinkedAccount` struct
- `src-tauri/src/connectors/mastodon.rs` — 13 locations using fabricated DIDs (from identity audit)
- `src-tauri/src/commands/mastodon.rs` — line 267 area
- `src-tauri/src/commands/ap.rs` — line 23 area (DID derivation)

**Verification:** `cargo check` (0 errors), `cargo test` (all pass), `pnpm test -- --run` (267+ pass).

---

## P2: Simplify `get_owner_did()` (A-S1, part 2)

**Priority:** High — completes identity migration.

**Problem:** `get_owner_did()` currently has a 5-level fallback chain that includes fabricated `did:plc` for non-Bluesky platforms. The new hierarchy is:

```
1. did:web:bytescape.social:{username}     ← Set when username is chosen (Phase 4.5 server)
2. did:plc:...                              ← ONLY real Bluesky DID (from PLC directory)
3. did:key:z6Mk...                          ← P2P/offline only (from Iroh Ed25519 keypair)
```

**Task:**
1. **Find `get_owner_did()`** — likely in `keychain_encrypted.rs` or `commands/identity.rs`.
2. **Refactor to 3-level fallback:**
   - Check for `did:web:bytescape.social:{username}` in keychain (will be `None` until Phase 4.5 server is live — this is fine, it's forward-compatible)
   - Check for real `did:plc:` from Bluesky OAuth session
   - Fall back to `did:key:z6Mk...` from Iroh Ed25519 keypair
3. **Remove any fabricated DID generation** from the fallback chain.
4. **Update callers** — search for all usages of `get_owner_did()` and verify they handle the new return values correctly.
5. **Test:** Verify the app still works with Bluesky OAuth (returns `did:plc`), local-only login (returns `did:key`), and that no fabricated DIDs are generated.

**Verification:** `cargo check`, `cargo test`, `pnpm test -- --run`.

---

## P3: Fix AP Actor DID Derivation (A-S1, part 3)

**Priority:** High — the current derivation produces different DIDs from the same keypair.

**Problem:** `commands/ap.rs` (around line 23) derives a DID from the ed25519 keypair using raw `bs58` encoding without the multicodec prefix. This produces a different DID string than `did_key_from_ed25519_bytes()` in the keychain module, even for the same keypair.

**Task:**
1. **Find the DID derivation in `commands/ap.rs`** — look for `bs58::encode` or similar.
2. **Replace with the canonical `did_key_from_ed25519_bytes()` function** from the keychain module.
3. **Verify** that all DID derivation paths in the codebase produce the same DID for the same keypair.
4. **Test:** Unit test comparing the output of both derivation methods for a known test keypair.

---

## P4: Relay Sync Command Stubs (A-S2, stubs only)

**Priority:** Medium — forward-looking scaffolding for Phase 4.5 server.

**Task:** Create stub Tauri commands for relay synchronization. These commands will be implemented fully when the Iroh relay server (I-5) is deployed. For now, they should:
1. Exist as registered Tauri commands (in `lib.rs` invoke_handler)
2. Accept the correct parameter types
3. Return `Err("Relay not configured — Phase 4.5 infrastructure required".to_string())`
4. Be documented with `///` doc comments explaining their future purpose

**Commands to stub:**
```rust
#[tauri::command]
async fn relay_connect(state: State<'_, AppState>, relay_url: String) -> Result<String, String>

#[tauri::command]
async fn relay_push_blob(state: State<'_, AppState>, blob_hash: String) -> Result<(), String>

#[tauri::command]
async fn relay_pull_pending(state: State<'_, AppState>) -> Result<Vec<String>, String>

#[tauri::command]
async fn relay_register_push_token(state: State<'_, AppState>, token: String, platform: String) -> Result<(), String>

#[tauri::command]
async fn relay_sync_status(state: State<'_, AppState>) -> Result<String, String>
```

**File:** Create `src-tauri/src/commands/relay.rs`. Register in `lib.rs`.

**Verification:** `cargo check`, `pnpm test -- --run`.

---

## P5: PDS Account Provisioning Stub (A-S3, stub only)

**Priority:** Low — scaffolding for when PDS is deployed.

**Task:** Create a stub Tauri command for PDS account provisioning:

```rust
#[tauri::command]
async fn provision_pds_account(
    state: State<'_, AppState>,
    username: String,
    invite_code: Option<String>,
) -> Result<String, String>
```

This will eventually: create an account on the Bytescape PDS via XRPC, store returned credentials in keychain, update `did:plc` if needed. For now: return an error explaining that the PDS is not yet deployed.

**File:** Add to `src-tauri/src/commands/relay.rs` (or a new `commands/server.rs`). Register in `lib.rs`.

---

## P6: AP Actor Provisioning Stub (A-S4, stub only)

**Priority:** Low — scaffolding for when AP server is deployed.

**Task:** Create a stub Tauri command for AP actor provisioning:

```rust
#[tauri::command]
async fn provision_ap_actor(
    state: State<'_, AppState>,
    username: String,
    invite_code: Option<String>,
) -> Result<String, String>
```

This will eventually: create an account on the custom axum AP server, link to `did:web`, store credentials. For now: return an error explaining the AP server is not yet deployed.

---

## Constraints

- **Track boundary:** Only modify files in `src-tauri/src/`. Do NOT touch `src/` (frontend files).
- **Build verification:** Run `cargo check` + `pnpm test -- --run` before EVERY commit. Zero errors, zero test regressions.
- **Shared files:** Before editing `lib.rs`, `db.rs`, or `keychain_encrypted.rs`, run `git log --oneline -5 -- <filepath>` to check for recent cross-agent modifications.
- **Commit format:** `feat(track-a): description` or `fix(track-a): description`. One logical change per commit.
- **No TODOs:** Implement stubs fully (with proper error returns), don't leave `todo!()` or `unimplemented!()` macros.
