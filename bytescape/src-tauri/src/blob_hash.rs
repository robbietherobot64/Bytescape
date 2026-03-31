//! Centralized Blob Hash Parsing
//!
//! **HIGH-5 Security Fix**: All blob hash string parsing is centralized here
//! to eliminate inconsistent inline parsing, prevent panics from malformed
//! inputs, and guarantee correct normalization everywhere.
//!
//! Hash strings in the codebase appear in many formats:
//! - Raw 64-char hex: `"afd9368b615f01ef..."`
//! - `blob:` prefixed: `"blob:HASH"` or `"blob:blob:HASH"`
//! - With thumbhash suffix: `"HASH:thumbhash"`
//! - Base32 encoded (uppercase or lowercase, with optional CIDv1 prefix)
//! - Whitespace-padded: `"  HASH  "`
//!
//! ## Related Files
//! - `sync.rs` — Uses blob hashes for content-addressed media storage (store_media, get_media)
//! - `connectors/bluesky.rs` — Receives blob hashes from Bluesky CDN URLs
//! - `db/vault.rs` — Stores blob hash references in the database
//! - `lib.rs` — Calls `parse_blob_hash` when resolving blob references from the frontend

use data_encoding::BASE32_NOPAD;
use crate::sync::Hash;

// ─── Normalization ───

/// Normalize a blob hash string by stripping all known prefixes and suffixes.
///
/// Returns the clean hash string (not yet decoded).
///
/// Handles:
/// - Leading/trailing whitespace
/// - One or more `blob:` prefixes
/// - Optional `:thumbhash` or other colon-delimited suffixes
pub fn normalize_blob_ref(input: &str) -> String {
    let mut s = input.trim().to_string();
    // Strip any number of blob: prefixes (handles "blob:blob:HASH")
    while let Some(rest) = s.strip_prefix("blob:") {
        s = rest.trim().to_string();
    }
    // Strip optional ":thumbhash" or other colon suffix
    if let Some(pos) = s.find(':') {
        s = s[..pos].to_string();
    }
    s
}

// ─── Canonical Parsing ───

/// Parse a blob hash from any format used in the codebase.
///
/// This is the single canonical parsing function. All call sites should
/// use this instead of inline normalization + decode logic.
///
/// # Supported Formats
/// - Raw 64-char hex: `"afd9368b615f01ef..."`
/// - `blob:` prefixed: `"blob:HASH"` or `"blob:blob:HASH"` (double prefix)
/// - With thumbhash suffix: `"HASH:thumbhash"`
/// - Base32 encoded (uppercase or lowercase)
/// - CIDv1/multihash payloads (≥32 bytes, takes last 32)
///
/// # Errors
/// Returns `Err` for empty strings, invalid encoding, or wrong-length digests.
pub fn parse_blob_hash(input: &str) -> Result<Hash, String> {
    let normalized = normalize_blob_ref(input);
    if normalized.is_empty() {
        return Err("Empty blob hash string".to_string());
    }

    // Path 1: 64-char hex string → direct decode
    if normalized.len() == 64 && normalized.chars().all(|c| c.is_ascii_hexdigit()) {
        let bytes = hex::decode(&normalized)
            .map_err(|e| format!("Invalid hex hash: {}", e))?;
        if bytes.len() != 32 {
            return Err(format!("Hex decode produced {} bytes, expected 32", bytes.len()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        return Ok(Hash::from_bytes(arr));
    }

    // Path 2: Base32 decode (try stripped 'b' prefix first for CIDv1 multibase,
    // then fall back to the full normalized string)
    let candidates = if let Some(stripped) = normalized.strip_prefix('b') {
        vec![stripped, normalized.as_str()]
    } else {
        vec![normalized.as_str()]
    };

    for candidate in candidates {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            continue;
        }

        let uppercase = trimmed.to_ascii_uppercase();
        let decoded = match BASE32_NOPAD.decode(uppercase.as_bytes()) {
            Ok(d) => d,
            Err(_) => continue,
        };

        if decoded.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&decoded);
            return Ok(Hash::from_bytes(arr));
        }

        // CIDv1/multihash-style payloads are commonly 36 bytes for blake3-256
        if decoded.len() > 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&decoded[decoded.len() - 32..]);
            return Ok(Hash::from_bytes(arr));
        }
    }

    Err(format!(
        "Invalid blob hash format: '{}' (normalized from '{}')",
        normalized,
        input.chars().take(80).collect::<String>()
    ))
}

// ─── Tests ───

#[cfg(test)]
mod tests {
    use super::*;

    // A known-good 32-byte hash as hex (64 chars)
    const VALID_HEX: &str = "afd9368b615f01ef9782b775160629b475ff5968973b0ffd0144fbdd5cc5931a";

    fn hash_from_hex(hex_str: &str) -> Hash {
        let bytes = hex::decode(hex_str).unwrap();
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Hash::from_bytes(arr)
    }

    #[test]
    fn test_empty_string() {
        assert!(parse_blob_hash("").is_err());
    }

    #[test]
    fn test_whitespace_only() {
        assert!(parse_blob_hash("   ").is_err());
    }

    #[test]
    fn test_hex_64_chars() {
        let result = parse_blob_hash(VALID_HEX);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), hash_from_hex(VALID_HEX));
    }

    #[test]
    fn test_blob_prefix() {
        let input = format!("blob:{}", VALID_HEX);
        let result = parse_blob_hash(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), hash_from_hex(VALID_HEX));
    }

    #[test]
    fn test_double_blob_prefix() {
        let input = format!("blob:blob:{}", VALID_HEX);
        let result = parse_blob_hash(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), hash_from_hex(VALID_HEX));
    }

    #[test]
    fn test_thumbhash_suffix() {
        let input = format!("{}:thumbhash", VALID_HEX);
        let result = parse_blob_hash(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), hash_from_hex(VALID_HEX));
    }

    #[test]
    fn test_blob_prefix_and_thumbhash() {
        let input = format!("blob:{}:thumbhash", VALID_HEX);
        let result = parse_blob_hash(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), hash_from_hex(VALID_HEX));
    }

    #[test]
    fn test_whitespace_trimmed() {
        let input = format!("  {}  ", VALID_HEX);
        let result = parse_blob_hash(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), hash_from_hex(VALID_HEX));
    }

    #[test]
    fn test_invalid_encoding() {
        assert!(parse_blob_hash("not-a-hash-at-all!!!").is_err());
    }

    #[test]
    fn test_wrong_length_hex() {
        assert!(parse_blob_hash("abcd1234").is_err());
    }

    #[test]
    fn test_normalize_blob_ref_strips_prefix() {
        assert_eq!(normalize_blob_ref("blob:abc123"), "abc123");
    }

    #[test]
    fn test_normalize_blob_ref_strips_double_prefix() {
        assert_eq!(normalize_blob_ref("blob:blob:abc123"), "abc123");
    }

    #[test]
    fn test_normalize_blob_ref_strips_suffix() {
        assert_eq!(normalize_blob_ref("abc123:thumbhash"), "abc123");
    }

    #[test]
    fn test_normalize_blob_ref_combined() {
        assert_eq!(normalize_blob_ref("  blob:abc123:thumbhash  "), "abc123");
    }

    #[test]
    fn test_base32_uppercase() {
        // Encode a known 32-byte value in base32
        let bytes = hex::decode(VALID_HEX).unwrap();
        let encoded = BASE32_NOPAD.encode(&bytes);
        let result = parse_blob_hash(&encoded);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), hash_from_hex(VALID_HEX));
    }

    #[test]
    fn test_base32_lowercase() {
        let bytes = hex::decode(VALID_HEX).unwrap();
        let encoded = BASE32_NOPAD.encode(&bytes).to_lowercase();
        let result = parse_blob_hash(&encoded);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), hash_from_hex(VALID_HEX));
    }

    #[test]
    fn test_base32_with_blob_prefix() {
        let bytes = hex::decode(VALID_HEX).unwrap();
        let encoded = BASE32_NOPAD.encode(&bytes);
        let input = format!("blob:{}", encoded);
        let result = parse_blob_hash(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), hash_from_hex(VALID_HEX));
    }

    #[test]
    fn test_cidv1_multihash_style() {
        // CIDv1 payloads have extra prefix bytes before the 32-byte digest
        let bytes = hex::decode(VALID_HEX).unwrap();
        let mut cid_payload = vec![0x01, 0x55, 0x1e, 0x20]; // example CID prefix
        cid_payload.extend_from_slice(&bytes);
        let encoded = BASE32_NOPAD.encode(&cid_payload);
        let result = parse_blob_hash(&encoded);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), hash_from_hex(VALID_HEX));
    }

    #[test]
    fn test_base32_with_b_prefix() {
        // Some formats have a leading 'b' before the base32 content
        let bytes = hex::decode(VALID_HEX).unwrap();
        let encoded = BASE32_NOPAD.encode(&bytes);
        let input = format!("b{}", encoded);
        let result = parse_blob_hash(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), hash_from_hex(VALID_HEX));
    }
}
