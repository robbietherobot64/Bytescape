# Backend Agent Kickoff: A0 Remediation + A1.1 AP Actor

## Context — PM Audit Findings (2026-03-14)

Read [pm-audit-2026-03-14.md](pm-audit-2026-03-14.md) for the full audit. Key findings affecting you:

### What was accepted ✅
- GAP-1 (persistent sessions via FileSessionStore) — ✅ Solid
- GAP-2 (dynamic callback port) — ✅ Clean
- GAP-4 (write ops wired for OAuth) — ✅ Working
- GAP-5 (DMs via `transition:chat.bsky`) — ✅ Good initiative (PM retroactively approved)
- GAP-6 (saved feeds / unified feed) — ✅ Fixed
- GAP-7 (transitive deps verified) — ✅ Correctly resolved
- Critical bug fix (3 root causes) — ✅ Architecturally sound fix using `send_xrpc()`
- Track B UI work (TopBar, FriendsPanel, ContextPanel) — ✅ PM-authorized, no fault

### What needs fixing ❌
- **GAP-3 is NOT done.** You wrote FileSessionStore tests (which are GAP-1 tests). The required tests are:
  1. `OAuthManager::new()` constructs without error
  2. Callback URL query string parsing (`OAuthCallbackQuery` from `code=X&state=Y&iss=Z`)
  3. `is_oauth_session()` returns true when `access_jwt` is sentinel
  4. `process_callback` returns error on missing/invalid parameters
  5. Error handling when PDS URL resolution fails
- **Hardcoded fallback master password** in `oauth.rs` — `"default_dev_password_CHANGE_IN_PRODUCTION"`. Must be replaced with a startup-time check or OS keyring derivation.
- **No DM pagination** — `import.rs` only fetches first 100 messages per convo and first page of convos. Add cursor-based pagination loops.
- **Empty JWT for OAuth feed import** — `import_bluesky_posts` uses `String::new()` as JWT for OAuth users, meaning only public posts are imported. Use the authenticated OAuth path instead.
- **Debug println statements** — `FileSessionStore` methods print DIDs to stdout. Remove or gate behind a `#[cfg(debug_assertions)]`.
- **Dual session restore race** — Both background task (500ms) and `check_auth_status` call `restore_session`. Add a guard (`AtomicBool` or `Once`) to prevent redundant restores.

### Documentation inaccuracies to correct
Your progress.md has factual errors. For future progress logs:
- The encryption is **ChaCha20-Poly1305**, not AES-256-GCM
- `current_access_token()` does not exist — `send_xrpc()` handles tokens internally
- The DPoP fix uses `send_xrpc()`, not manual `Authorization` header addition
- GAP-7: `p256` is NOT in Cargo.toml (correctly absent — zero direct imports). ROADMAP says "explicit entries added" — fix that text.

---

## Priority 0 — Branch Cleanup

Your work is currently split across `main` and `track-b/ui-redesign`. The A0 GAP commits (`c4e8123` through `bac702e`) are correctly on `main`. The post-GAP bug fixes and UI redesign are on `track-b/ui-redesign`.

**Task:** Cherry-pick the post-GAP **backend-only** bug fixes from `track-b/ui-redesign` onto `main`:

```bash
git checkout main

# Cherry-pick backend fixes only (NOT UI redesign commits):
git cherry-pick 22a0435   # Critical bug fix (DPoP + PDS URL + chat proxy)
git cherry-pick a870548   # "You" feed tab fix
git cherry-pick c29e549   # Chat proxy header isolation
git cherry-pick 97c6faa   # Remove debug println
git cherry-pick ad6af24   # Parallel chat import
git cherry-pick bf56e20   # Blank buddy handles fix

# Do NOT cherry-pick: 737105a, 72d0461, 65d70e5, aed605e, 507ce75, 7766512, ae4bf4b, 82ffeb6
# Those are Track B UI commits — they stay on track-b/ui-redesign
```

If cherry-picks conflict, resolve them. The Track B UI commits should remain on `track-b/ui-redesign` for the Frontend agent to review.

---

## Priority 1 — A0 Remediation

Work on `main` after cherry-picks land.

### 1a. Fix GAP-3 — Write Real OAuth Tests

File: `src-tauri/tests/oauth_test.rs` or `#[cfg(test)]` in `oauth.rs`

Write tests for:
1. **`OAuthManager::new()`** — verify construction doesn't panic with valid/invalid config
2. **Callback URL parsing** — `OAuthCallbackQuery` parses `code=X&state=Y&iss=Z` correctly, returns error on missing fields
3. **`is_oauth_session()`** — returns `true` when `access_jwt` matches the OAuth sentinel value, `false` otherwise
4. **`process_callback` error paths** — returns meaningful errors for: missing code, missing state, invalid issuer, expired state
5. **PDS URL resolution** — `get_pds_base()` returns correct format for both OAuth and app-password paths

The existing FileSessionStore tests are fine — keep them. These are **additional** tests.

### 1b. Fix Hardcoded Master Password

File: `oauth.rs` and wherever `BYTESCAPE_MASTER_PASSWORD` env var is read.

**Option A (preferred):** Derive the master password from the OS keyring at startup. If no keyring entry exists, generate a random 32-byte key, store it in the OS keyring, and use that.

**Option B (acceptable):** At startup, check if the env var is set. If not, log a WARN and generate a random ephemeral key. Store it in memory only — sessions will be lost on restart if no persistent key is configured, but at least they won't be encrypted with a known password.

**DO NOT:** Ship with `"default_dev_password_CHANGE_IN_PRODUCTION"` as a fallback.

### 1c. Add DM Pagination

File: `commands/import.rs`

In the OAuth DM import path:
1. **`listConvos`** — add cursor-based pagination loop. Fetch pages until `cursor` is empty/null.
2. **`getMessages`** per convo — same cursor-based loop. Fetch until all messages are retrieved.

### 1d. Fix OAuth Feed Import JWT

File: `commands/import.rs`

Currently for OAuth users: `let access_jwt = String::new()`. This means only public posts are imported.

Fix: Use the OAuth authenticated path (`state.xrpc_get(...)`) to fetch the user's timeline, not the unauthenticated path with an empty JWT.

### 1e. Clean Up Debug Output

Files: `oauth.rs`

- Remove or `#[cfg(debug_assertions)]` gate all `println!` calls in `FileSessionStore` methods
- Remove the `eprintln!` with DID values (or redact to `did:***`)

### 1f. Add Session Restore Guard

Files: `lib.rs`, `commands/auth.rs`

Add an `AtomicBool` (or `tokio::sync::OnceCell`) to `AppState` to ensure `restore_session` runs at most once. Both the background task and `check_auth_status` should check this flag before attempting restore.

---

## Priority 2 — A1.1 AP Actor & Inbox/Outbox

Once A0 remediation is committed and verified, begin A1.1. This is the critical path item that unblocks 6+ downstream items across all three tracks.

See ROADMAP.md for full A1.1 requirements. High-level:
1. Create `src-tauri/src/activitypub/` module
2. Define AP Actor type at `bytescape.social/@{username}`
3. Implement HTTP signature verification for inbox POST
4. Add outbox GET endpoint
5. Add `ActivityPub` variant to `Source` enum
6. This unlocks Track B items: B2.6, B9.1, B9.3

---

## ⚠️ Merge Conflict Warning

Three branches have diverged from `main`:
- `track-c/c1-facet-frontend` — modifies `lib.rs`, `oauth.rs`, `import.rs`, `App.css`
- `track-b/b2-profile-builder` — modifies `lib.rs`, `oauth.rs`, `import.rs`, `App.css`, `types/index.ts`
- `track-b/ui-redesign` — modifies `lib.rs`, `oauth.rs`, `App.css`, `App.tsx`

When you cherry-pick or work on `main`, be aware these files will conflict when branches merge. Keep your changes surgical and well-commented so merge resolution is straightforward.

---

## Files You Will Modify
- `bytescape/src-tauri/src/oauth.rs` (1b master password, 1e debug cleanup)
- `bytescape/src-tauri/src/commands/import.rs` (1c pagination, 1d JWT fix)
- `bytescape/src-tauri/src/commands/auth.rs` (1f restore guard)
- `bytescape/src-tauri/src/lib.rs` (1f restore guard, cherry-pick conflicts)
- `bytescape/src-tauri/tests/oauth_test.rs` (1a real OAuth tests)
- `ROADMAP.md` (correct GAP-3 status, fix GAP-7 text)

## Files You Must NOT Touch
- `src/components/ComposeBox.tsx` (Track C)
- `src/components/ArticleEditor.tsx` (Track C)
- `src/components/ThemePicker.tsx` (Track B)
- `src/screens/ProfileBuilderScreen.tsx` (Track B)
- Any files under `src/components/` that are new Track C or B deliverables
