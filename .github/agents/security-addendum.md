# Security Addendum — Infrastructure Audit Findings

> This file documents security issues found during the infrastructure hardening audit.
> Each agent should address the items tagged for their track when working on related code.

## Context: What's Already Fixed (Main Agent)

The following were fixed on the `infra/hardening-audit-fixes` branch:

- ✅ **CRIT-3**: Asset protocol scope restricted to `$APPDATA/**` (was `**`)
- ✅ **ARCH-2**: `Mutex<Database>` → `RwLock<Database>` for concurrent readers
- ✅ **PERF**: O(n²) path prefix query fixed in `list_my_items_by_path_prefix_paged`
- ✅ **PERF**: Background startup tasks no longer hold DB locks for entire duration
- ✅ **DEAD CODE**: Plaintext `keychain.rs` removed (was unused)
- ✅ **TOCTOU**: Media server port binding cleaned up

---

## Track A (Backend Agent) — P2P & Protocol Security

### CRIT-4: No P2P Rate Limiting

**File**: `protocol.rs` → `BytescapeProtocol::accept()`

**Problem**: Any peer can open unlimited connections and flood with requests. There's no rate limiting, connection throttling, or request validation on the `bytescape/1.0` ALPN handler.

**Required Fix**:
1. Add a per-peer connection rate limiter (e.g., `governor` crate or simple `HashMap<NodeId, (count, last_reset)>`)
2. Limit concurrent connections per peer (suggest: 3)
3. Limit request rate per peer per minute (suggest: 60 requests/min)
4. Reject oversized request payloads (max 64KB for protocol messages)
5. Add backpressure: if a peer exceeds limits, delay responses instead of hard-dropping

**Implementation hint**: Add a `RateLimiter` field to `BytescapeProtocol`:
```rust
use std::collections::HashMap;
use tokio::sync::Mutex;

struct PeerRateState {
    request_count: u32,
    window_start: std::time::Instant,
    active_connections: u32,
}

// In BytescapeProtocol:
rate_limits: Arc<Mutex<HashMap<iroh::PublicKey, PeerRateState>>>,
```

### CRIT-5: Unauthenticated Media Server CORS

**File**: `media_server.rs`

**Problem**: The Warp media server uses `allow_any_origin()` CORS. While it's localhost-only, any browser tab/extension can read cached media via `http://127.0.0.1:{port}/{hash}`.

**Required Fix**:
1. Replace `allow_any_origin()` with `allow_origin("tauri://localhost")` (desktop) and the dev origin `http://localhost:1420`
2. For Android, the origin is `https://tauri.localhost` — include that too
3. Consider adding a random token query parameter that the frontend knows but external scripts don't

### CRIT-6: MD5 for Escrow Integrity

**File**: `keychain_encrypted.rs` → `RecoveryEscrowBundle`

**Problem**: The recovery escrow bundle uses MD5 for integrity checking. MD5 is cryptographically broken — collisions are trivially constructable. An attacker who intercepts the escrow bundle could modify it while preserving the MD5 hash.

**Required Fix**:
1. Replace `md5::compute()` with `blake3::hash()` for the `integrity_hash` field
2. Add `blake3` to `Cargo.toml` dependencies (it's already a transitive dep via iroh)
3. Migrate: check for both MD5-length (16 bytes hex = 32 chars) and BLAKE3-length (32 bytes hex = 64 chars) hashes when verifying, to handle existing escrow bundles

---

## Track B (Frontend Agent) — Input Sanitization

### HIGH-3: XSS in Rich Content Rendering

**Files**: Any component that renders `item.content` or user-provided HTML

**Problem**: `BytescapeItem.content` may contain raw HTML from imported Facebook/Instagram archives. If rendered via `dangerouslySetInnerHTML` without sanitization, this is a stored XSS vector.

**Required Fix**:
1. Never use `dangerouslySetInnerHTML` on user content
2. If HTML rendering is needed (e.g., for rich text), use `dompurify` to sanitize first:
   ```typescript
   import DOMPurify from 'dompurify';
   const clean = DOMPurify.sanitize(content, { ALLOWED_TAGS: ['b', 'i', 'a', 'br', 'p'] });
   ```
3. Audit all components that display `content`, `text`, or imported data for unsanitized rendering
4. Add `dompurify` to `package.json` if not already present

### MED-4: No Input Length Validation on Frontend

**Files**: `ComposeBox.tsx`, any form inputs

**Problem**: While Bluesky enforces a 300-grapheme limit, local/P2P posts have no maximum length on the frontend side. A user (or bug) could create a post with megabytes of text, bloating the redb database.

**Required Fix**:
1. Add a reasonable max length for local posts (suggest: 10,000 characters)
2. Validate attachment count (suggest: max 20 per post)
3. Validate attachment size on the frontend before sending to Rust (suggest: warn at 50MB, block at 100MB)

---

## Track C (Content Agent) — Media Pipeline Security

### HIGH-5: Blob Hash Parsing is Fragile

**Files**: `commands/media.rs` → `parse_hash_safe()`, `normalize_blob_hash_input()`

**Problem**: Multiple functions parse blob hashes from strings with different normalization logic. A malformed hash string could cause panics (via `unwrap()` on decode) or return incorrect hashes.

**Required Fix**:
1. Centralize all hash parsing into a single `parse_blob_hash(input: &str) -> Result<Hash, String>` function
2. Use this function everywhere instead of inline parsing
3. Never `unwrap()` on hash decode — always return `Err`
4. Add unit tests for edge cases: empty string, "blob:" prefix, "blob:blob:" double prefix, hash with `:thumbhash` suffix, invalid base32, wrong length

### MED-5: MIME Detection from Magic Bytes is Incomplete

**Files**: `commands/media.rs` → `mime_from_magic_bytes()`

**Problem**: The MIME detection only handles 7 types (jpeg, png, gif, webp, mp4, webm, ogg). Importing archives may contain TIFF, BMP, HEIC, or other formats that default to `application/octet-stream`, breaking thumbnails and media type detection.

**Required Fix**:
1. Add detection for: TIFF (`0x49 0x49` or `0x4D 0x4D`), BMP (`BM`), HEIC/HEIF (ftyp + heic/mif1), AVI (RIFF + AVI), FLAC (fLaC), WAV (RIFF + WAVE)
2. Consider using the `infer` crate (already lightweight, 0 deps) instead of manual magic byte matching
3. Ensure `mime_from_path_hint()` is used as fallback when magic bytes don't match

---

## Cross-Track: Keychain Hardening (Deferred)

These are tracked but **deferred** pending a broader UX redesign:

- **CRIT-1**: Hardcoded master password fallback in `keychain_encrypted.rs` — needs OS keyring integration (platform-specific: `keyring` crate on desktop, Android Keystore on mobile)
- **CRIT-2**: Hardcoded recovery codes — needs proper random generation with `rand::rngs::OsRng`

The user's direction: "Keep things as simple and straightforward for the user as possible — no scary cryptographic key management, common auth methods, advanced options for power users." This will be addressed as a dedicated auth UX story.
