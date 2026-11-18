//! Sealed storage for `linked_accounts` OAuth secrets (home-instance access /
//! refresh tokens and the DPoP private key) — chacha20poly1305 at rest, the
//! same AEAD `ap::keys` uses for the (less privileged) AP actor keys.
//!
//! Stored format: `enc:v1:<base64(nonce || ciphertext)>` with a fresh random
//! 12-byte nonce per encryption. The key is
//! `Sha256("bytescape.linked_accounts.v1" || JWT_SECRET)` — a fresh domain
//! string, deliberately NOT the AP-keys one (`bytescape.ap.actor.v1`), so the
//! two key families are independent. Reuses existing config: no new env var.
//!
//! [`open`] is tolerant: values without the [`ENC_PREFIX`] marker are legacy
//! plaintext (rows written before the encryption rollout) and are returned
//! as-is, so nothing breaks before the startup backfill
//! (`db::accounts::encrypt_plaintext_tokens`) seals them.

use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, OsRng};
use chacha20poly1305::{AeadCore, ChaCha20Poly1305, Key, Nonce};
use sha2::{Digest, Sha256};

/// Marker of a sealed stored value: `enc:v1:<base64(nonce || ciphertext)>`.
pub const ENC_PREFIX: &str = "enc:v1:";

const NONCE_LEN: usize = 12;

/// Derive the 32-byte chacha20poly1305 key from the config secret.
fn aead_key(secret: &str) -> Key {
    let mut hasher = Sha256::new();
    hasher.update(b"bytescape.linked_accounts.v1");
    hasher.update(secret.as_bytes());
    Key::from_slice(&hasher.finalize()).to_owned()
}

/// Seal `plain` for storage: `enc:v1:<base64(nonce || ciphertext)>`, fresh
/// random nonce per call (sealing the same value twice yields different
/// stored forms).
pub fn seal(plain: &str, secret: &str) -> String {
    let cipher = ChaCha20Poly1305::new(&aead_key(secret));
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ct = cipher
        .encrypt(&nonce, plain.as_bytes())
        .expect("chacha20poly1305 seal: encrypt cannot fail with a valid key/nonce");
    let mut out = nonce.to_vec();
    out.extend_from_slice(&ct);
    format!(
        "{ENC_PREFIX}{}",
        base64::engine::general_purpose::STANDARD.encode(out)
    )
}

/// Open a stored value back to plaintext. Tolerant: values without the
/// [`ENC_PREFIX`] marker are legacy plaintext and come back unchanged. A
/// prefixed value that fails to decode or decrypt is an error — never
/// silently served as a credential.
pub fn open(stored: &str, secret: &str) -> Result<String, String> {
    let Some(body) = stored.strip_prefix(ENC_PREFIX) else {
        return Ok(stored.to_string());
    };
    let raw = base64::engine::general_purpose::STANDARD
        .decode(body)
        .map_err(|e| format!("secret b64: {e}"))?;
    if raw.len() < NONCE_LEN {
        return Err("secret ciphertext too short".into());
    }
    let (nonce, ct) = raw.split_at(NONCE_LEN);
    let cipher = ChaCha20Poly1305::new(&aead_key(secret));
    let pt = cipher
        .decrypt(Nonce::from_slice(nonce), ct)
        .map_err(|_| "secret decrypt failed".to_string())?;
    String::from_utf8(pt).map_err(|e| format!("secret utf8: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrips_with_fresh_nonces() {
        let plain = "home-instance-token";
        let sealed = seal(plain, "s3cret");
        assert!(sealed.starts_with(ENC_PREFIX));
        assert_ne!(sealed, plain);
        assert_eq!(open(&sealed, "s3cret").expect("open"), plain);
        // Fresh nonce per seal: same plaintext seals to a different stored form.
        assert_ne!(seal(plain, "s3cret"), sealed);
        // Wrong secret must fail; tampered ciphertext must fail.
        assert!(open(&sealed, "other").is_err());
        let tampered = format!("{ENC_PREFIX}AAAA{}", &sealed[ENC_PREFIX.len() + 4..]);
        assert!(open(&tampered, "s3cret").is_err());
    }

    #[test]
    fn open_passes_legacy_plaintext_through() {
        assert_eq!(
            open("plain-legacy-token", "s3cret").expect("passthrough"),
            "plain-legacy-token"
        );
        assert_eq!(open("", "s3cret").expect("empty"), "");
    }
}
