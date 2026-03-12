# Backend Agent — Security Hardening Kickoff

## Priority: Address the 3 critical security findings from the infrastructure audit

Read `.github/agents/security-addendum.md` for full context. You have 3 assigned items:

---

### Task 1: CRIT-4 — P2P Rate Limiting (`protocol.rs`)

**Problem**: `BytescapeProtocol::accept()` has no rate limiting. Any peer can flood with unlimited connections and requests.

**What to implement**:
1. Add a `PeerRateLimiter` struct (or field on `BytescapeProtocol`) that tracks per-peer connection state:
   - `HashMap<iroh::PublicKey, PeerRateState>` wrapped in `Arc<tokio::sync::Mutex<...>>`
   - `PeerRateState` tracks: `request_count: u32`, `window_start: Instant`, `active_connections: u32`
2. In `accept()`, before processing any request:
   - Check if peer has exceeded 3 concurrent connections → reject with a log
   - Check if peer has exceeded 60 requests in the current 60-second window → reject
   - Increment counters on accept, decrement `active_connections` when the handler returns
3. Add a size limit on incoming request payloads: reject any request > 64KB
4. Clean up stale entries periodically (peers not seen in 5 minutes)

**Constraints**:
- Don't block legitimate peers — log rate-limit events for debugging
- The rate limiter must be `Clone` + `Send` + `Sync` since `BytescapeProtocol` is cloned per connection
- Test: `cargo check` must pass, then `pnpm tauri dev` from `bytescape/`

---

### Task 2: CRIT-5 — Media Server CORS (`media_server.rs`)

**Problem**: `warp::cors().allow_any_origin()` lets any browser tab read cached media.

**What to implement**:
1. Replace `allow_any_origin()` with explicit allowed origins:
   ```rust
   .allow_origin("tauri://localhost")      // Desktop (Linux/macOS/Windows)
   .allow_origin("https://tauri.localhost") // Android
   .allow_origin("http://localhost:1420")   // Dev mode (Vite)
   ```
2. Use `warp::cors().allow_origins(vec![...])` to specify all three

**Constraints**:
- Must not break dev mode (Vite runs on port 1420)
- Must not break Android (uses `https://tauri.localhost`)
- Test by running `pnpm tauri dev` and confirming media still loads in the UI

---

### Task 3: CRIT-6 — MD5 → BLAKE3 for Escrow Integrity (`keychain_encrypted.rs`)

**Problem**: `RecoveryEscrowBundle` uses `md5::compute()` for `integrity_hash`. MD5 is cryptographically broken.

**What to implement**:
1. Add `blake3` to `Cargo.toml` dependencies (it's already a transitive dep via iroh, but add it explicitly)
2. In `keychain_encrypted.rs`, find the escrow bundle creation where `md5::compute()` is called
3. Replace with `blake3::hash(data).to_hex().to_string()`
4. In the verification path, support BOTH formats for backward compat:
   - If `integrity_hash.len() == 32` → legacy MD5 format, verify with md5
   - If `integrity_hash.len() == 64` → new BLAKE3 format, verify with blake3
5. New escrow bundles must always use BLAKE3

**Constraints**:
- Existing escrow bundles must still verify (don't break users who already have recovery data)
- Don't remove the `md5` crate yet — keep it for legacy verification only

---

## Workflow

1. Create branch `security/backend-hardening` from current HEAD
2. Implement all 3 fixes
3. Run `cargo check` after each fix
4. Run `pnpm tauri dev` from `bytescape/` to verify full build
5. Run `pnpm test` from `bytescape/`
6. Commit with: `fix(security): P2P rate limiting, CORS restriction, BLAKE3 escrow integrity`
