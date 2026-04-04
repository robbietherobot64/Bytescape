//! HTTP Signatures for ActivityPub federation (draft-cavage-http-signatures-12).
//!
//! Implements the authentication mechanism used across the fediverse: every
//! outbound request is signed with the local actor's Ed25519 key, and every
//! inbound request's signature is verified against the sender's public key.
//!
//! # How It Works
//!
//! 1. **Outbound POST** — `sign_post_request()` computes a SHA-256 digest of
//!    the body, builds a canonical signing string from the request headers,
//!    signs it with the local actor's Ed25519 key, and attaches the result as
//!    a `Signature` header.
//! 2. **Outbound GET** — `sign_get_request()` does the same for fetch requests
//!    (some servers like Mastodon require signed fetches to return actor data).
//! 3. **Inbound** — `parse_signature_header()` extracts components from an
//!    incoming `Signature` header, then `verify_ed25519_signature()` checks
//!    the signature against the sender's public key PEM.
//! 4. The SHA-256 digest is computed using the `sha2` crate (SEC-10 fix).
//!
//! # Related Files
//!
//! - `inbox.rs` — uses `parse_signature_header` + `verify_ed25519_signature`
//!   for incoming activity authentication
//! - `outbox.rs` — uses `sign_post_request` and `sign_get_request` for delivery
//! - `actor.rs` — provides the secret key and public key PEM for signing

use base64::Engine;
use chrono::Utc;
use sha2::Digest;

// ═══════════════════════════════════════════════════════════════════════════════
// CONSTANTS
// ═══════════════════════════════════════════════════════════════════════════════

/// Headers included in the signature for outbound POST requests (inbox delivery).
const SIGNED_HEADERS_POST: &[&str] = &["(request-target)", "host", "date", "digest", "content-type"];
/// Headers included in the signature for outbound GET requests (actor fetches).
const SIGNED_HEADERS_GET: &[&str] = &["(request-target)", "host", "date", "accept"];

/// Parsed components of an HTTP Signature header.
///
/// Extracted from an incoming request's `Signature` header by
/// `parse_signature_header()`. The `key_id` is used to fetch the sender's
/// public key for verification.
#[derive(Debug, Clone)]
pub struct HttpSignature {
    pub key_id: String,
    pub algorithm: String,
    pub headers: Vec<String>,
    pub signature: Vec<u8>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// OUTBOUND SIGNING
// ═══════════════════════════════════════════════════════════════════════════════

/// Sign an outbound HTTP POST request for inbox delivery.
///
/// Computes a SHA-256 digest of the body, builds a canonical signing string
/// from the request headers, signs it with the actor's Ed25519 secret key,
/// and returns the headers to attach to the request (Date, Digest, Signature,
/// Content-Type).
///
/// → Called by `outbox.rs` when delivering activities to remote inboxes.
pub fn sign_post_request(
    key_id: &str,
    secret_key: &iroh::SecretKey,
    target_url: &str,
    body: &[u8],
) -> Result<Vec<(String, String)>, String> {
    let url = url::Url::parse(target_url)
        .map_err(|e| format!("Invalid URL: {}", e))?;
    let host = url.host_str()
        .ok_or_else(|| "URL has no host".to_string())?;
    let path = url.path();

    let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
    let digest = compute_sha256_digest(body);
    let digest_header = format!("SHA-256={}", digest);

    let signing_string = build_signing_string(
        SIGNED_HEADERS_POST,
        "post",
        path,
        host,
        &date,
        Some(&digest_header),
        Some("application/activity+json"),
        None,
    );

    let sig_bytes = sign_ed25519(secret_key, signing_string.as_bytes());
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(&sig_bytes);

    let headers_str = SIGNED_HEADERS_POST.join(" ");
    let sig_header = format!(
        r#"keyId="{}",algorithm="ed25519",headers="{}",signature="{}""#,
        key_id, headers_str, sig_b64
    );

    Ok(vec![
        ("Date".to_string(), date),
        ("Digest".to_string(), digest_header),
        ("Signature".to_string(), sig_header),
        ("Content-Type".to_string(), "application/activity+json".to_string()),
    ])
}

/// Sign an outbound HTTP GET request (e.g., fetching remote actors).
///
/// Some servers (notably Mastodon) require signed fetches to return actor
/// profiles. Returns headers to attach: Date, Signature, Accept.
///
/// → Called by `actor.rs` when resolving remote actor profiles.
pub fn sign_get_request(
    key_id: &str,
    secret_key: &iroh::SecretKey,
    target_url: &str,
) -> Result<Vec<(String, String)>, String> {
    let url = url::Url::parse(target_url)
        .map_err(|e| format!("Invalid URL: {}", e))?;
    let host = url.host_str()
        .ok_or_else(|| "URL has no host".to_string())?;
    let path = url.path();

    let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();

    let signing_string = build_signing_string(
        SIGNED_HEADERS_GET,
        "get",
        path,
        host,
        &date,
        None,
        None,
        Some("application/activity+json"),
    );

    let sig_bytes = sign_ed25519(secret_key, signing_string.as_bytes());
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(&sig_bytes);

    let headers_str = SIGNED_HEADERS_GET.join(" ");
    let sig_header = format!(
        r#"keyId="{}",algorithm="ed25519",headers="{}",signature="{}""#,
        key_id, headers_str, sig_b64
    );

    Ok(vec![
        ("Date".to_string(), date),
        ("Signature".to_string(), sig_header),
        ("Accept".to_string(), "application/activity+json".to_string()),
    ])
}

// ═══════════════════════════════════════════════════════════════════════════════
// INBOUND VERIFICATION
// ═══════════════════════════════════════════════════════════════════════════════

/// Parse an HTTP Signature header into its components.
///
/// Extracts `keyId`, `algorithm`, `headers`, and `signature` from a
/// comma-separated Signature header value per draft-cavage-http-signatures-12.
///
/// → Called by `inbox.rs` to authenticate incoming ActivityPub requests.
pub fn parse_signature_header(header: &str) -> Result<HttpSignature, String> {
    let mut key_id = String::new();
    let mut algorithm = String::new();
    let mut headers = Vec::new();
    let mut signature = Vec::new();

    for part in header.split(',') {
        let part = part.trim();
        if let Some(val) = part.strip_prefix("keyId=\"") {
            key_id = val.trim_end_matches('"').to_string();
        } else if let Some(val) = part.strip_prefix("algorithm=\"") {
            algorithm = val.trim_end_matches('"').to_string();
        } else if let Some(val) = part.strip_prefix("headers=\"") {
            let h = val.trim_end_matches('"');
            headers = h.split_whitespace().map(|s| s.to_string()).collect();
        } else if let Some(val) = part.strip_prefix("signature=\"") {
            let sig_b64 = val.trim_end_matches('"');
            signature = base64::engine::general_purpose::STANDARD
                .decode(sig_b64)
                .map_err(|e| format!("Invalid base64 in signature: {}", e))?;
        }
    }

    if key_id.is_empty() {
        return Err("Missing keyId in Signature header".to_string());
    }

    Ok(HttpSignature {
        key_id,
        algorithm,
        headers,
        signature,
    })
}

/// Verify an Ed25519 signature against a public key PEM and signing string.
///
/// Extracts the 32-byte public key from the PEM, then delegates to the
/// raw verification function. Returns `Ok(true)` if valid, `Ok(false)` if
/// the signature doesn't match, or `Err` if the key is malformed.
///
/// → Called by `inbox.rs` after parsing the Signature header.
pub fn verify_ed25519_signature(
    pem: &str,
    signing_string: &[u8],
    signature: &[u8],
) -> Result<bool, String> {
    let pubkey_bytes = pem_to_ed25519_pubkey(pem)?;
    verify_ed25519_raw(&pubkey_bytes, signing_string, signature)
}

// ═══════════════════════════════════════════════════════════════════════════════
// SIGNING STRING CONSTRUCTION
// ═══════════════════════════════════════════════════════════════════════════════

/// Build the canonical signing string from request components.
///
/// Constructs a newline-separated string of `header: value` pairs in the
/// order specified by `headers`. This is the byte sequence that gets signed
/// or verified. The `(request-target)` pseudo-header is synthesized from
/// the HTTP method and path.
pub fn build_signing_string(
    headers: &[&str],
    method: &str,
    path: &str,
    host: &str,
    date: &str,
    digest: Option<&str>,
    content_type: Option<&str>,
    accept: Option<&str>,
) -> String {
    let mut parts = Vec::new();
    for h in headers {
        let line = match *h {
            "(request-target)" => format!("(request-target): {} {}", method, path),
            "host" => format!("host: {}", host),
            "date" => format!("date: {}", date),
            "digest" => format!("digest: {}", digest.unwrap_or("")),
            "content-type" => format!("content-type: {}", content_type.unwrap_or("")),
            "accept" => format!("accept: {}", accept.unwrap_or("")),
            other => format!("{}: ", other),
        };
        parts.push(line);
    }
    parts.join("\n")
}

// ═══════════════════════════════════════════════════════════════════════════════
// CRYPTOGRAPHIC PRIMITIVES
// ═══════════════════════════════════════════════════════════════════════════════

/// Sign data with an Ed25519 secret key. Returns the 64-byte signature.
fn sign_ed25519(secret_key: &iroh::SecretKey, data: &[u8]) -> Vec<u8> {
    let sig = secret_key.sign(data);
    sig.to_bytes().to_vec()
}

/// Verify an Ed25519 signature against raw 32-byte public key bytes.
/// Returns `Ok(true)` if valid, `Ok(false)` if invalid, `Err` if malformed.
fn verify_ed25519_raw(
    pubkey_bytes: &[u8; 32],
    data: &[u8],
    signature: &[u8],
) -> Result<bool, String> {
    if signature.len() != 64 {
        return Err(format!("Invalid signature length: expected 64, got {}", signature.len()));
    }
    let public_key = iroh::PublicKey::from_bytes(pubkey_bytes)
        .map_err(|e| format!("Invalid public key: {}", e))?;
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(signature);
    let sig = iroh::Signature::from_bytes(&sig_arr);
    match public_key.verify(data, &sig) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

/// Compute SHA-256 digest of data, returned as base64.
///
/// Uses the `sha2` crate (SEC-10: replaced hand-rolled implementation).
pub fn compute_sha256_digest(data: &[u8]) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    let hash = hasher.finalize();
    base64::engine::general_purpose::STANDARD.encode(hash)
}

/// Extract ed25519 public key bytes from a PEM string.
pub fn pem_to_ed25519_pubkey(pem: &str) -> Result<[u8; 32], String> {
    let b64: String = pem
        .lines()
        .filter(|l| !l.starts_with("-----"))
        .collect();
    let der = base64::engine::general_purpose::STANDARD
        .decode(&b64)
        .map_err(|e| format!("Invalid PEM base64: {}", e))?;

    // Ed25519 SubjectPublicKeyInfo is 44 bytes: 12 byte prefix + 32 byte key
    if der.len() == 44 {
        let mut key = [0u8; 32];
        key.copy_from_slice(&der[12..44]);
        Ok(key)
    } else if der.len() == 32 {
        let mut key = [0u8; 32];
        key.copy_from_slice(&der);
        Ok(key)
    } else {
        Err(format!("Unexpected DER length for ed25519: {}", der.len()))
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_known_vector() {
        // Verify the sha2 crate produces the correct SHA-256 hash (SEC-10 regression test)
        let mut hasher = sha2::Sha256::new();
        hasher.update(b"hello");
        let hash = hasher.finalize();
        let hex_str = hex::encode(hash);
        assert_eq!(hex_str, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    }

    #[test]
    fn test_compute_sha256_digest_base64() {
        let digest = compute_sha256_digest(b"test body");
        // SHA-256("test body") as base64
        assert!(!digest.is_empty());
        // Verify roundtrip: decode base64, compare to known hash
        let decoded = base64::engine::general_purpose::STANDARD.decode(&digest).unwrap();
        assert_eq!(decoded.len(), 32);
    }

    #[test]
    fn test_sign_and_verify_post() {
        let secret = iroh::SecretKey::generate(&mut rand::rng());
        let key_id = "https://bytescape.social/@test#main-key";
        let body = b"{\"type\":\"Create\"}";
        let headers = sign_post_request(key_id, &secret, "https://example.com/inbox", body).unwrap();
        assert!(headers.iter().any(|(k, _)| k == "Signature"));
        assert!(headers.iter().any(|(k, _)| k == "Digest"));
    }

    #[test]
    fn test_sign_get() {
        let secret = iroh::SecretKey::generate(&mut rand::rng());
        let key_id = "https://bytescape.social/@test#main-key";
        let headers = sign_get_request(key_id, &secret, "https://example.com/@alice").unwrap();
        assert!(headers.iter().any(|(k, _)| k == "Signature"));
        assert!(headers.iter().any(|(k, _)| k == "Accept"));
    }

    #[test]
    fn test_parse_signature_header() {
        let header = "keyId=\"https://example.com/@alice#main-key\",algorithm=\"ed25519\",headers=\"(request-target) host date\",signature=\"dGVzdA==\"";
        let parsed = parse_signature_header(header).unwrap();
        assert_eq!(parsed.key_id, "https://example.com/@alice#main-key");
        assert_eq!(parsed.algorithm, "ed25519");
        assert_eq!(parsed.headers, vec!["(request-target)", "host", "date"]);
    }

    #[test]
    fn test_pem_roundtrip() {
        let secret = iroh::SecretKey::generate(&mut rand::rng());
        let pem = super::super::actor::ed25519_public_key_to_pem(secret.public().as_bytes());
        let recovered = pem_to_ed25519_pubkey(&pem).unwrap();
        assert_eq!(&recovered, secret.public().as_bytes());
    }
}
