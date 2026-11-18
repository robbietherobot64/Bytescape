//! AP actor keypairs — RSA key generation and at-rest encryption.
//!
//! ActivityPub HTTP-signatures use RSA (most of the fediverse verifies
//! RSA-SHA256), so the AP actor needs its **own** RSA keypair. This is distinct
//! from the AT identity's Ed25519 key (`users.public_key`) and must never be
//! swapped with it.
//!
//! The RSA private key is stored encrypted at rest using chacha20poly1305, keyed
//! by a value derived from `JWT_SECRET` (reusing existing config; no new
//! required env var). The public key is stored in the clear as PEM — it is, by
//! definition, public (served in the actor document).

use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, OsRng};
use chacha20poly1305::{AeadCore, ChaCha20Poly1305, Key, Nonce};
use rsa::pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey, EncodeRsaPrivateKey, EncodeRsaPublicKey};
use rsa::pkcs8::{DecodePublicKey, LineEnding};
use rsa::{RsaPrivateKey, RsaPublicKey};
use sha2::{Digest, Sha256};

const RSA_BITS: usize = 2048;
const NONCE_LEN: usize = 12;

/// A generated AP actor keypair, in the shapes we persist + serve.
pub struct ApKeypair {
    /// RSA private key, PEM (PKCS#1). Encrypted before persisting.
    pub private_pem: String,
    /// RSA public key, PEM (PKCS#1). Persisted in the clear; served in the actor doc.
    pub public_pem: String,
}

/// Generate a fresh 2048-bit RSA keypair for an AP actor.
pub fn generate_keypair() -> Result<ApKeypair, String> {
    let mut rng = OsRng;
    let private = RsaPrivateKey::new(&mut rng, RSA_BITS).map_err(|e| format!("rsa keygen: {e}"))?;
    let public = RsaPublicKey::from(&private);

    let private_pem = private
        .to_pkcs1_pem(LineEnding::LF)
        .map_err(|e| format!("encode privkey: {e}"))?
        .to_string();
    let public_pem = public
        .to_pkcs1_pem(LineEnding::LF)
        .map_err(|e| format!("encode pubkey: {e}"))?
        .to_string();

    Ok(ApKeypair { private_pem, public_pem })
}

/// Derive a 32-byte chacha20poly1305 key from the config secret.
fn aead_key(secret: &str) -> Key {
    let mut hasher = Sha256::new();
    hasher.update(b"bytescape.ap.actor.v1");
    hasher.update(secret.as_bytes());
    Key::from_slice(&hasher.finalize()).to_owned()
}

/// Encrypt the RSA private-key PEM for storage. Output is
/// `base64(nonce || ciphertext)`.
pub fn encrypt_private_key(private_pem: &str, secret: &str) -> Result<String, String> {
    let cipher = ChaCha20Poly1305::new(&aead_key(secret));
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ct = cipher
        .encrypt(&nonce, private_pem.as_bytes())
        .map_err(|_| "ap key encrypt failed".to_string())?;

    let mut out = nonce.to_vec();
    out.extend_from_slice(&ct);
    Ok(base64::engine::general_purpose::STANDARD.encode(out))
}

/// Decrypt a stored (encrypted) RSA private-key PEM back to plaintext PEM.
pub fn decrypt_private_key(stored: &str, secret: &str) -> Result<String, String> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(stored)
        .map_err(|e| format!("ap key b64: {e}"))?;
    if raw.len() < NONCE_LEN {
        return Err("ap key ciphertext too short".into());
    }
    let (nonce, ct) = raw.split_at(NONCE_LEN);
    let cipher = ChaCha20Poly1305::new(&aead_key(secret));
    let pt = cipher
        .decrypt(Nonce::from_slice(nonce), ct)
        .map_err(|_| "ap key decrypt failed".to_string())?;
    String::from_utf8(pt).map_err(|e| format!("ap key utf8: {e}"))
}

/// Load an AP private key from storage, handling both formats: plaintext PEM
/// (starts with `-----BEGIN`) and encrypted (base64 chacha20poly1305). This
/// unifies the read path — issue and fetch-signer both use it.
pub fn private_key_pem(stored: &str, secret: &str) -> Result<String, String> {
    if stored.trim_start().starts_with("-----BEGIN") {
        return Ok(stored.to_string());
    }
    decrypt_private_key(stored, secret)
}

/// Parse an RSA private key from PEM (PKCS#1). Used when signing outbound
/// activities (Phase C) — validated here so key handling is one code path.
pub fn parse_private_key(pem: &str) -> Result<RsaPrivateKey, String> {
    RsaPrivateKey::from_pkcs1_pem(pem).map_err(|e| format!("parse privkey: {e}"))
}

/// Parse an RSA public key from PEM — PKCS#1 (`BEGIN RSA PUBLIC KEY`) or
/// SPKI (`BEGIN PUBLIC KEY`), both common on the fediverse.
pub fn parse_public_key(pem: &str) -> Result<RsaPublicKey, String> {
    // Remote actors publish SPKI PEM (`BEGIN PUBLIC KEY` — Mastodon and
    // virtually every AP implementation); our own minted keys are PKCS#1
    // (`BEGIN RSA PUBLIC KEY`). Accept BOTH: a single-format parse made
    // every real remote delivery fail as the generic "signature
    // verification failed" (parse failure is surfaced as BadSignature)
    // while self-keyed tests passed. Owner verdict 2026-11-18.
    match rsa::RsaPublicKey::from_pkcs1_pem(pem) {
        Ok(k) => Ok(k),
        Err(_) => rsa::RsaPublicKey::from_public_key_pem(pem)
            .map_err(|e| format!("parse pubkey: {e}")),
    }
}

/// Load the fetch-signing identity for `user_id`: `(key_id, privkey_pem)`.
///
/// `key_id` is the issued actor's id plus the `#main-key` fragment — the same
/// id [`crate::ap::actor`] serves as `publicKey.id`, so the remote can resolve
/// it back to this user's public key. The private key is decrypted at rest via
/// [`decrypt_private_key`] (`JWT_SECRET`-derived).
///
/// Returns `None` — and callers fall back to an unsigned fetch — when the user
/// has no issued AP actor (`ap_actor_id` / `ap_private_key_pem` unset), the row
/// can't be read, or decryption fails; each case is logged, never fatal.
pub async fn load_fetch_signer(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    jwt_secret: &str,
) -> Option<(String, String)> {
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT ap_actor_id, ap_private_key_pem FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| tracing::warn!("[AP] fetch signer load failed for {user_id}: {e}"))
    .ok()?;

    let (actor_id, enc_pem) = match row {
        Some((Some(actor_id), Some(enc_pem))) => (actor_id, enc_pem),
        _ => {
            tracing::warn!(
                "[AP] user {user_id} has no issued AP actor; AP fetches will be unsigned"
            );
            return None;
        }
    };
    // The stored key may be plaintext PEM or encrypted; private_key_pem handles both.
    match private_key_pem(&enc_pem, jwt_secret) {
        Ok(privkey_pem) => Some((format!("{actor_id}#main-key"), privkey_pem)),
        Err(e) => {
            tracing::warn!("[AP] fetch signer key load failed for {user_id}: {e}");
            None
        }
    }
}

/// The fetch signer for a LOCAL actor id: `(actor#main-key, private PEM)` read
/// from the issuing `users` row. Used wherever the acting identity is known by
/// actor id (the bridge's dereference, `store::audience_inboxes` fetch-on-miss)
/// so every actor-document fetch on the delivery/dereference path can sign —
/// authorized-fetch instances (e.g. vivaldi.net) answer unsigned GETs with 401.
/// Best-effort: `None` (no actor/key, unreadable secret, decrypt failure) means
/// callers fall back to an unsigned fetch; each gap is logged, never fatal.
pub async fn actor_fetch_signer(
    pool: &sqlx::PgPool,
    actor_id: &str,
) -> Option<(String, String)> {
    let secret = std::env::var("JWT_SECRET").ok()?;
    let row: (String,) =
        sqlx::query_as("SELECT ap_private_key_pem FROM users WHERE ap_actor_id = $1")
            .bind(actor_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| tracing::warn!("[AP] fetch signer load failed for {actor_id}: {e}"))
            .ok()
            .flatten()?;
    let pem = private_key_pem(&row.0, &secret).map_err(|e| {
        tracing::warn!("[AP] fetch signer key load failed for {actor_id}: {e}");
    });
    Some((format!("{actor_id}#main-key"), pem.ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn actor_fetch_signer_loads_the_actors_key() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else {
            return;
        };
        // `connect_pool` loads .env (JWT_SECRET) — the same secret
        // `set_ap_actor` encrypts at rest with, so the key round-trips.
        let la = crate::ap::testutil::make_local_actor(&pool, "keys.test").await;
        let signer = actor_fetch_signer(&pool, &la.actor_id)
            .await
            .expect("local actor must yield a fetch signer");
        assert_eq!(signer.0, format!("{}#main-key", la.actor_id));
        assert!(signer.1.contains("BEGIN RSA PRIVATE KEY"));
        // Unknown actor → no signer (unsigned fallback, never fatal).
        assert!(actor_fetch_signer(&pool, "https://keys.test/@nobody").await.is_none());
    }

    #[test]
    fn keypair_roundtrips_through_encryption() {
        let kp = generate_keypair().expect("keygen");
        assert!(kp.private_pem.contains("BEGIN RSA PRIVATE KEY"));
        assert!(kp.public_pem.contains("BEGIN RSA PUBLIC KEY"));

        let enc = encrypt_private_key(&kp.private_pem, "secret").expect("encrypt");
        let dec = decrypt_private_key(&enc, "secret").expect("decrypt");
        assert_eq!(dec, kp.private_pem);

        // Wrong secret must fail.
        assert!(decrypt_private_key(&enc, "other").is_err());
    }

    #[test]
    fn generated_keypair_parses() {
        let kp = generate_keypair().expect("keygen");
        assert!(parse_private_key(&kp.private_pem).is_ok());
        assert!(parse_public_key(&kp.public_pem).is_ok());
    }
}
