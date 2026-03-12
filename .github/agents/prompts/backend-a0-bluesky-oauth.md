# Backend Agent Kickoff: A0 Bluesky OAuth

## Context

Bytescape currently uses **app-password** authentication for Bluesky (`auth.rs`, 74 lines). Users generate an app password from Bluesky settings and paste it into the login screen. This works but has UX problems: app passwords expire, users don't understand them, and it's not the native OAuth flow that first-party apps use.

A0 replaces this with proper **Bluesky OAuth (atproto OAuth)** so users click "Login with Bluesky," get redirected to Bluesky's authorization server, and return with proper OAuth tokens. This is the gateway to all of Track A — without proper auth, federation work can't proceed.

## Prior Research (Already Done)

API research was completed in a previous session. Key findings:

1. **Crate**: [`atrium-oauth`](https://crates.io/crates/atrium-oauth) v0.1.x (part of the `atrium` ecosystem by `sugyan`)
2. **Working example**: https://github.com/sugyan/atrium/tree/main/examples — contains a complete OAuth flow example
3. **Protocol**: AT Protocol OAuth uses:
   - **PKCE** (Proof Key for Code Exchange) — standard S256 challenge
   - **DPoP** (Demonstration of Proof-of-Possession) — token-binding via signed JWTs
   - **PAR** (Pushed Authorization Request) — server-side authorization request storage
   - **Client metadata document** — JSON served at a well-known URL describing the client
4. **Flow**: User enters handle → resolve PDS → discover authorization server → PAR → redirect to auth page → callback with code → token exchange with DPoP proof → access token + refresh token

## Challenge: Client Metadata Document

Bluesky OAuth requires the client to serve a **client metadata document** at a publicly-accessible URL. For a desktop app, this is tricky because there's no server.

**Options** (implement in this order of preference):
1. **Host on `bytescape.social`**: If the domain is live, serve `https://bytescape.social/.well-known/oauth-client-metadata` as a static JSON file. This is the long-term solution.
2. **Use `localhost` redirect**: For development and until hosting is live, use `http://127.0.0.1:{port}/callback` as the redirect URI. The atrium-oauth crate supports localhost clients.
3. **Use `did:web` client ID**: Some AT Protocol OAuth implementations accept `did:web:` as a client identifier without a hosted metadata document.

For now, implement option 2 (localhost) as the primary flow, with option 1 as a configuration toggle for when `bytescape.social` goes live.

## Your Task

### Step 1: Add dependencies

```toml
# In bytescape/src-tauri/Cargo.toml [dependencies]
atrium-api = "0.24"          # AT Protocol API types
atrium-xrpc-client = "0.5"   # XRPC HTTP client
atrium-oauth = { version = "0.1", features = ["default"] }  # OAuth flow
```

Check https://crates.io for current versions — the atrium ecosystem moves fast. Use the latest compatible versions.

### Step 2: Create `oauth.rs` module

Create `bytescape/src-tauri/src/oauth.rs`:

**Requirements:**
- `OAuthManager` struct that wraps `atrium_oauth::OAuthClient`
- `start_auth_flow(handle: String) -> Result<String, String>` — resolves PDS, discovers auth server, initiates PAR, returns authorization URL for the frontend to open
- `handle_callback(code: String, state: String) -> Result<Session, String>` — exchanges authorization code for tokens using DPoP proof
- `refresh_tokens() -> Result<Session, String>` — refresh expired access tokens
- Store tokens in the encrypted keychain (reuse `keychain_encrypted.rs`)
- Convert the OAuth session to the existing `Session` struct format so downstream code (feed fetching, posting, DMs) doesn't need changes

**Key design constraint:** The existing `Session` struct has `access_jwt` and `refresh_jwt` fields. The OAuth flow produces different tokens (DPoP-bound access tokens). You need to either:
- Extend `Session` with optional OAuth-specific fields (preferred — backward compatible)
- Or create an `AuthMethod` enum (`AppPassword(Session)` | `OAuth(OAuthSession)`) with a common interface

### Step 3: Local callback server

The OAuth flow needs a local HTTP server to receive the callback. Options:
- Reuse the existing `warp` dependency (already used for `media_server.rs`)
- Or use Tauri's deep link plugin (`tauri-plugin-deep-link`)

Recommended: Spin up a temporary `warp` server on `127.0.0.1:0` (OS-assigned port), listen for the callback, extract the `code` and `state` parameters, then shut down the server.

### Step 4: Add Tauri commands

```rust
#[tauri::command]
pub async fn start_bluesky_oauth(handle: String, state: State<'_, AppState>) -> Result<String, String>
// Returns authorization URL for frontend to open in system browser

#[tauri::command]
pub async fn complete_bluesky_oauth(code: String, state_param: String, app_state: State<'_, AppState>) -> Result<Session, String>
// Exchanges code for tokens, stores in keychain, returns Session
```

Register in `lib.rs` invoke_handler.

### Step 5: Update AppState

Add `OAuthManager` to `AppState` in `lib.rs`:
```rust
pub struct AppState {
    // ... existing fields ...
    pub oauth_manager: Option<OAuthManager>,
}
```

### Step 6: Update LoginScreen frontend

Modify `bytescape/src/screens/LoginScreen.tsx`:
- Add "Login with Bluesky" button (primary, prominent)
- Keep "Use App Password" as a secondary/fallback option
- "Login with Bluesky" calls `invoke('start_bluesky_oauth', { handle })` → opens returned URL in system browser via `@tauri-apps/plugin-shell` `open()`
- Listen for a Tauri event `oauth-callback-received` that fires when the local server gets the callback
- On callback: call `invoke('complete_bluesky_oauth', { code, stateParam })` → receive Session → proceed to feed

### Step 7: Maintain backward compatibility

**Critical:** Do NOT break app-password auth. Many users may still prefer it, and it's the fallback for environments where OAuth redirect doesn't work (e.g., some Linux WMs that don't open system browser properly).

The login flow should be:
1. User enters Bluesky handle
2. Primary action: "Login with Bluesky" → OAuth flow
3. Secondary action: "Use App Password" → existing `create_session` flow
4. Both produce a `Session` that works identically downstream

### Step 8: Token refresh

The OAuth access token has a short lifetime (typically 5 minutes). The refresh flow must:
- Detect 401/`ExpiredToken` responses (existing retry logic in `connectors/bluesky.rs`)
- Call the OAuth refresh endpoint with DPoP proof
- Update the stored session

Check if the existing `refresh_session` function in `auth.rs` can be extended, or if OAuth refresh needs its own path.

### Step 9: Verify

1. `pnpm tauri dev` — compiles without errors
2. `pnpm test` — frontend tests pass
3. Manual test: attempt OAuth login, verify redirect opens, callback returns tokens
4. Manual test: verify app-password login still works
5. Manual test: verify feed fetching works with OAuth tokens

### Step 10: Commit

```bash
git add -A
git commit -m "feat(track-a): A0 Bluesky OAuth login flow

- atrium-oauth integration with PKCE + DPoP
- Local warp callback server for OAuth redirect
- OAuthManager with start/complete/refresh flow
- LoginScreen updated with OAuth primary + app-password fallback
- Backward-compatible Session struct
- Tokens stored in encrypted keychain"
```

## Reference Implementation

Study the atrium example OAuth flow:
- https://github.com/sugyan/atrium — main repo
- Look at `examples/` directory for the OAuth client example
- The `atrium-oauth` crate README has usage documentation

Also reference:
- https://atproto.com/specs/oauth — AT Protocol OAuth spec
- https://docs.bsky.app/docs/advanced-guides/oauth-client — Bluesky OAuth guide

## Files You Will Create
- `bytescape/src-tauri/src/oauth.rs`

## Files You Will Modify
- `bytescape/src-tauri/Cargo.toml` (atrium deps)
- `bytescape/src-tauri/src/lib.rs` (AppState, command registration, module declaration)
- `bytescape/src-tauri/src/auth.rs` (extend Session or add AuthMethod enum)
- `bytescape/src/screens/LoginScreen.tsx` (OAuth UI)
- `ROADMAP.md` (mark A0 complete when done)

## Files You Must NOT Touch
- `bytescape/src-tauri/src/connectors/bluesky.rs` — this consumes Session, shouldn't need changes if Session interface is preserved
- `bytescape/src/components/ComposeBox.tsx` (Track C)
- `bytescape/src/components/lenses/` (Track B)

## What Unblocks After A0

Once OAuth works:
- **A1.1 AP Actor & Inbox/Outbox** can begin (this is the real critical path for federation)
- Users get a production-quality login experience
- Token refresh is handled properly (no more expiring app passwords)
