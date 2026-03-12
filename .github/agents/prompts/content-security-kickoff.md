# Content Agent — Security Hardening Kickoff

## Priority: Address the 2 security findings from the infrastructure audit

Read `.github/agents/security-addendum.md` for full context. You have 2 assigned items:

---

### Task 1: HIGH-5 — Centralize Blob Hash Parsing

**Problem**: Multiple functions across the codebase parse blob hashes from strings with inconsistent normalization logic. A malformed hash string could cause panics (via `unwrap()` on decode) or silently return incorrect hashes.

**Scattered parsing locations** (search for these patterns):
- `commands/media.rs` → `normalize_blob_hash_input()`, `decode_base32_hash_like()`, `parse_hash_safe()`
- `commands/posting.rs` → inline hash normalization in `publish_post_v2()`
- `commands/syndicate.rs` → inline hash normalization in `syndicate_vault_item()`
- `commands/import.rs` → `parse_blob_hash_path()`
- `lib.rs` → backfill task inline hash parsing

**What to implement**:

1. **Create `src-tauri/src/blob_hash.rs`** with a single canonical parsing function:
   ```rust
   use crate::sync::Hash;
   
   /// Parse a blob hash from any format used in the codebase.
   /// 
   /// Handles:
   /// - Raw 64-char hex: "afd9368b615f01ef..."
   /// - `blob:` prefixed: "blob:HASH" or "blob:blob:HASH" (double prefix)
   /// - With thumbhash suffix: "HASH:thumbhash"
   /// - Base32 encoded (uppercase or lowercase)
   /// - Already-parsed Hash (passthrough if wrapped in string)
   ///
   /// Returns Err for empty strings, invalid encoding, or wrong-length digests.
   pub fn parse_blob_hash(input: &str) -> Result<Hash, String> { ... }
   
   /// Normalize a blob hash string by stripping prefixes and suffixes.
   /// Returns the clean hash string (not yet decoded).
   pub fn normalize_blob_ref(input: &str) -> String { ... }
   ```

2. **Register the module** in `lib.rs`: `pub mod blob_hash;`

3. **Replace ALL inline hash parsing** with calls to `blob_hash::parse_blob_hash()`:
   - `commands/media.rs`: Replace `normalize_blob_hash_input()` + `decode_base32_hash_like()` + `parse_hash_safe()` — keep them as thin wrappers calling `blob_hash::parse_blob_hash()` for backward compat, or replace all call sites
   - `commands/posting.rs`: Replace the inline normalization block in `publish_post_v2()`
   - `commands/syndicate.rs`: Replace the inline normalization block
   - `commands/import.rs`: Replace `parse_blob_hash_path()` internals
   - `lib.rs` backfill task: Replace the inline hash parsing loop (the one with `strip_prefix("blob:")` etc.)

4. **Add comprehensive tests** in `blob_hash.rs`:
   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;
       
       #[test] fn test_empty_string() { assert!(parse_blob_hash("").is_err()); }
       #[test] fn test_hex_64_chars() { /* valid 64-char hex */ }
       #[test] fn test_blob_prefix() { /* "blob:HASH" */ }
       #[test] fn test_double_blob_prefix() { /* "blob:blob:HASH" */ }
       #[test] fn test_thumbhash_suffix() { /* "HASH:thumbhash" */ }
       #[test] fn test_base32_uppercase() { /* valid base32 */ }
       #[test] fn test_base32_lowercase() { /* auto-uppercase */ }
       #[test] fn test_invalid_encoding() { assert!(parse_blob_hash("not-a-hash").is_err()); }
       #[test] fn test_wrong_length() { assert!(parse_blob_hash("abcd1234").is_err()); }
       #[test] fn test_whitespace_trimmed() { /* "  HASH  " */ }
   }
   ```

5. **Ensure NO `unwrap()` calls** remain on hash decode paths — all must return `Result`

---

### Task 2: MED-5 — Expand MIME Magic Byte Detection

**Problem**: `commands/media.rs` → `mime_from_magic_bytes()` only detects 7 MIME types. Imported archives contain formats that fall through to `application/octet-stream`, breaking thumbnails and media type routing.

**What to implement**:

1. **Option A (preferred)**: Add the `infer` crate to `Cargo.toml` and use it:
   ```rust
   pub fn mime_from_magic_bytes(data: &[u8]) -> String {
       // Try the `infer` crate first — it handles 100+ file types
       if let Some(kind) = infer::get(data) {
           return kind.mime_type().to_string();
       }
       // Fallback for anything infer doesn't catch
       "application/octet-stream".to_string()
   }
   ```
   The `infer` crate is ~50KB, zero dependencies, and handles JPEG, PNG, GIF, WebP, TIFF, BMP, HEIC, AVIF, MP4, WebM, AVI, MKV, FLAC, WAV, OGG, and many more.

2. **Option B (if you prefer no new deps)**: Add manual detection for these additional types:
   - TIFF: starts with `0x49 0x49 0x2A 0x00` (little-endian) or `0x4D 0x4D 0x00 0x2A` (big-endian)
   - BMP: starts with `BM`
   - HEIC/HEIF: ftyp box with `heic`, `heix`, `mif1`, or `msf1`
   - AVI: starts with `RIFF` + 4 bytes + `AVI `
   - FLAC: starts with `fLaC`
   - WAV: starts with `RIFF` + 4 bytes + `WAVE`
   - AVIF: ftyp box with `avif` or `avis`

3. **Update `mime_from_path_hint()`** to cover the same formats as a fallback

4. **Add tests** for each new format (use the first 16 bytes of each format as test data)

---

## Workflow

1. Create branch `security/content-hardening` from current HEAD
2. Create `blob_hash.rs` with centralized parsing + tests
3. Replace all inline hash parsing call sites
4. Expand MIME detection (add `infer` crate or manual detection)
5. Run `cargo test` to verify blob_hash tests pass
6. Run `cargo check` to verify no compile errors
7. Run `pnpm tauri dev` from `bytescape/` to verify full build
8. Run `pnpm test` from `bytescape/`
9. Commit with: `fix(security): centralize blob hash parsing, expand MIME detection`
