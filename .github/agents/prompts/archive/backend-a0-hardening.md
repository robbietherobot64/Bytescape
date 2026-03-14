# Backend Agent Kickoff: A0 OAuth Hardening + Merge

## Priority 0 — Merge `track-a/a0-bluesky-oauth` → `main`

The A0 branch has 6 commits (+951/-49, 8 files) that have NOT been merged yet. **Do this first.**

```bash
cd /home/korendian/Bytescape
git checkout main
git merge track-a/a0-bluesky-oauth
```

### Expected conflict: `ROADMAP.md`

Track A's branch forked before B2.5 Theme Engine was completed. You will see a conflict at the B2.5 entry:
- **Your branch** has `- [ ] **B2.5 Theme Engine**: Cozy Mode (default GeoCities aesthetic)...` (old, incomplete)
- **Main** has `- [x] **B2.5 Theme Engine**: Three built-in themes — **Cozy Mode** (GeoCities default)...` (completed)

**Resolution:**
1. Keep main's B2.5 `[x]` entry (the completed version with full description)
2. Accept your A0 section (the 6-line block inserted before A1)
3. `git add ROADMAP.md && git commit` (accept default merge message)
4. `git branch -d track-a/a0-bluesky-oauth`

### Verify after merge
```bash
cd bytescape && pnpm test && npx tsc --noEmit
```

---

## Priority 1 — A0 OAuth Hardening

After the merge, address these 7 gaps identified by the PM audit. They are documented in ROADMAP.md under "A0 OAuth — Known Gaps". Work them in order of severity.

### A0-GAP-4: Wire Write Operations for OAuth (CRITICAL)
**Why first:** OAuth users can read feeds but **cannot post**. This is the most user-visible gap.

**Files:** `commands/posting.rs`, `commands/syndicate.rs`

**Pattern to follow:** Look at `commands/import.rs` — it has 9 instances of `is_oauth_session()` branching. Apply the same pattern:
```rust
if state.is_oauth_session() {
    let pds_url = state.oauth_manager.get_pds_url().await?;
    // Use oauth_manager.authenticated_post() instead of raw reqwest with Bearer token
} else {
    // Existing Bearer token path
}
```

**Verify:** After this change, an OAuth-logged-in user should be able to create a post that syndicates to Bluesky.

### A0-GAP-1: Persistent OAuth Sessions (HIGH)
**Why:** App restarts force re-authentication. Bad UX.

**File:** `oauth.rs`

**Current:** `MemorySessionStore` — sessions live only in RAM.

**Fix:** Persist the OAuth session to the encrypted keychain (`keychain_encrypted.rs`) or redb. On app startup, check for a stored session and restore it into `OAuthManager`. The `atrium-oauth` crate's `OAuthSession` may need serialization — check if it implements `Serialize`/`Deserialize`. If not, store the raw tokens (access, refresh, DPoP keypair) and reconstruct the session.

**Fallback:** If `OAuthSession` can't be easily serialized, store enough state to call the refresh endpoint on next startup. The refresh token + DPoP key are sufficient to get a new access token.

### A0-GAP-2: Dynamic Callback Port (MEDIUM)
**File:** `oauth.rs` — `OAUTH_CALLBACK_PORT`

**Current:** Hardcoded to `28457`. Fails if port is in use.

**Fix:** Bind to `127.0.0.1:0` (OS assigns a free port), then read the actual port from the bound socket. Construct the redirect URI dynamically. Verify that `AtprotoLocalhostClientMetadata` accepts dynamic ports — check the atrium source code or docs.

### A0-GAP-3: Add OAuth Unit Tests (MEDIUM)
**File:** `src-tauri/tests/` or inline `#[cfg(test)]` in `oauth.rs`

**Minimum coverage:**
1. `OAuthManager::new()` constructs without error
2. Callback URL query string parsing (`OAuthCallbackQuery` from `code=X&state=Y&iss=Z`)
3. `is_oauth_session()` returns true when access_jwt is the sentinel value
4. `process_callback` returns error on missing/invalid parameters
5. Error handling when PDS URL resolution fails

### A0-GAP-7: Verify Transitive Dependencies (LOW)
**File:** `Cargo.toml`

Check whether `p256` and `urlencoding` are used directly in `oauth.rs`. If so, verify they come transitively from `atrium-oauth`. If not transitive, add explicit entries:
```toml
p256 = "0.13"       # if needed
urlencoding = "2"    # if needed
```

Run `cargo tree -p p256` and `cargo tree -p urlencoding` to check.

### A0-GAP-6: Fix Saved Feeds for OAuth (LOW)
**File:** `connectors/bluesky.rs`

Saved feeds and unified feed return empty for OAuth sessions. This is likely a routing issue — OAuth sessions should use the user's PDS URL (from `OAuthSession::base_uri()`) for authenticated reads, not `AUTH_XRPC` (`bsky.social/xrpc`). Apply the same fix pattern from commit `3b01559`.

### A0-GAP-5: DMs for OAuth Sessions (DEFERRED)
Document as future work — requires expanding OAuth scopes to include `atproto:chat`. Skip for now.

---

## Priority 2 — A1.1 AP Actor & Inbox/Outbox

Once A0 hardening is complete, begin A1.1. This is the critical path item that unblocks 6+ downstream items across all three tracks. See the existing ROADMAP.md A1.1 entry for requirements.

---

## Files You Will Modify
- `bytescape/src-tauri/src/oauth.rs` (GAP-1, GAP-2, GAP-3)
- `bytescape/src-tauri/src/commands/posting.rs` (GAP-4)
- `bytescape/src-tauri/src/commands/syndicate.rs` (GAP-4)
- `bytescape/src-tauri/src/connectors/bluesky.rs` (GAP-6)
- `bytescape/src-tauri/Cargo.toml` (GAP-7, if needed)
- `ROADMAP.md` (mark completed items)

## Files You Must NOT Touch
- `bytescape/src/components/ComposeBox.tsx` (Track C)
- `bytescape/src/components/ArticleEditor.tsx` (Track C)
- `bytescape/src/components/ThemePicker.tsx` (Track B)
- `bytescape/src/hooks/useTheme.ts` (Track B)
