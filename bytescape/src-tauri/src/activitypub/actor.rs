//! AP Actor — Local User's ActivityPub Identity
//!
//! This module defines the local user's ActivityPub identity — their "profile"
//! in federation terms. Every Bytescape user who enables federation gets an
//! [`ApActor`] document that remote servers fetch, cache, and use to:
//! - verify HTTP Signatures on outgoing activities,
//! - display the user's profile (name, bio, avatar) in remote UIs,
//! - address replies, follows, and mentions back to the correct inbox.
//!
//! # How It Works
//!
//! 1. When a user enables federation, [`ApActor::new()`] takes their Iroh
//!    Ed25519 secret key and derives a full actor profile — inbox, outbox,
//!    followers, and following URLs — all rooted at [`AP_DOMAIN`].
//! 2. The actor document is served to remote servers via the actor endpoint.
//!    They use the embedded `publicKey` to verify HTTP Signatures and the
//!    profile fields to display user info.
//! 3. [`ApActor::to_activitystreams()`] serializes the actor to JSON-LD
//!    (with the standard `@context`) while **carefully excluding** the
//!    `secret_key_hex` field to prevent private key leakage.
//! 4. [`ed25519_public_key_to_pem()`] wraps the raw 32-byte Ed25519 public
//!    key in ASN.1 DER (SubjectPublicKeyInfo) format and PEM-encodes it,
//!    providing interoperability with implementations expecting PKIX keys.
//!
//! # Related Files
//!
//! - `signatures.rs` — signs outgoing HTTP requests with this actor's secret key.
//! - `inbox.rs` — receives activities addressed to this actor's inbox URL.
//! - `outbox.rs` — delivers activities originating from this actor.
//! - `types.rs` — defines [`ApPublicKey`] embedded in the actor document.
//!
//! # Security Note
//!
//! `secret_key_hex` lives on a `#[derive(Serialize)]` struct. The
//! [`to_activitystreams()`](ApActor::to_activitystreams) method manually
//! constructs the JSON to exclude it, but **accidental serialization
//! elsewhere (e.g., `serde_json::to_value(&actor)`) would leak the private
//! key**. Treat any code path that serializes `ApActor` with extreme care.

use serde::{Deserialize, Serialize};
use super::types::{ApPublicKey, ApMediaLink, ap_context};

// ═══════════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════════

/// The domain used for all ActivityPub actor URLs.
///
/// All actor IDs, inboxes, outboxes, and key IDs are rooted under this domain
/// (e.g., `https://bytescape.social/@alice`). In production this would be
/// configurable per-instance; currently hardcoded for the flagship instance.
pub const AP_DOMAIN: &str = "bytescape.social";

// ═══════════════════════════════════════════════════════════════════════════════
// Actor Struct
// ═══════════════════════════════════════════════════════════════════════════════

/// A local user's ActivityPub actor profile.
///
/// Mirrors the ActivityPub `Person` type. Contains both the public-facing
/// fields (id, inbox, outbox, public key) and the sensitive `secret_key_hex`
/// needed for signing outgoing requests.
///
/// **Warning:** This struct derives `Serialize`. Never serialize it directly
/// to a response — use [`to_activitystreams()`](Self::to_activitystreams)
/// which excludes the secret key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApActor {
    pub id: String,
    pub inbox: String,
    pub outbox: String,
    pub followers: String,
    pub following: String,
    pub preferred_username: String,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub public_key: ApPublicKey,
    pub icon: Option<ApMediaLink>,
    pub image: Option<ApMediaLink>,
    pub manually_approves_followers: bool,
    pub owner_did: String,
    pub secret_key_hex: String,
    pub created_at: String,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Actor Implementation
// ═══════════════════════════════════════════════════════════════════════════════

impl ApActor {
    /// Creates a new ActivityPub actor from an Iroh secret key.
    ///
    /// Derives the Ed25519 public key, converts it to PEM format, and builds
    /// the full actor profile with standard AP endpoint URLs.
    ///
    /// # Parameters
    /// - `username` — the local handle (e.g., `"alice"`); used in the actor URL.
    /// - `display_name` — optional human-readable name shown in remote UIs.
    /// - `bio` — optional profile summary / bio text.
    /// - `secret_key` — the Iroh Ed25519 secret key; its public half becomes
    ///   the actor's `publicKey` and the private bytes are stored as hex for
    ///   later signing operations.
    ///
    /// # Returns
    /// A fully-populated `ApActor` ready to be persisted and served.
    pub fn new(
        username: &str,
        display_name: Option<String>,
        bio: Option<String>,
        secret_key: &iroh::SecretKey,
    ) -> Self {
        let public_key = secret_key.public();
        let actor_id = format!("https://{}/@{}", AP_DOMAIN, username);
        let key_id = format!("{}#main-key", actor_id);
        let public_key_pem = ed25519_public_key_to_pem(public_key.as_bytes());

        ApActor {
            id: actor_id.clone(),
            inbox: format!("{}/inbox", actor_id),
            outbox: format!("{}/outbox", actor_id),
            followers: format!("{}/followers", actor_id),
            following: format!("{}/following", actor_id),
            preferred_username: username.to_string(),
            name: display_name,
            summary: bio,
            public_key: ApPublicKey {
                id: key_id,
                owner: actor_id,
                public_key_pem,
            },
            icon: None,
            image: None,
            manually_approves_followers: false,
            owner_did: String::new(),
            secret_key_hex: hex::encode(secret_key.to_bytes()),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Serializes this actor to an ActivityStreams JSON-LD document.
    ///
    /// Produces the `Person` object that remote servers fetch from the actor
    /// endpoint. Includes `@context`, profile fields, and the embedded
    /// `publicKey` — but **intentionally omits** `secret_key_hex`,
    /// `owner_did`, and `created_at` which are internal-only fields.
    ///
    /// Optional fields (`name`, `summary`, `icon`, `image`) are only included
    /// when present, keeping the document compact.
    pub fn to_activitystreams(&self) -> serde_json::Value {
        // Manually construct JSON rather than deriving Serialize to ensure
        // the secret key is never accidentally included in the output.
        let mut doc = serde_json::json!({
            "@context": ap_context(),
            "id": self.id,
            "type": "Person",
            "preferredUsername": self.preferred_username,
            "inbox": self.inbox,
            "outbox": self.outbox,
            "followers": self.followers,
            "following": self.following,
            "manuallyApprovesFollowers": self.manually_approves_followers,
            "publicKey": {
                "id": self.public_key.id,
                "owner": self.public_key.owner,
                "publicKeyPem": self.public_key.public_key_pem,
            },
        });
        if let Some(name) = &self.name {
            doc["name"] = serde_json::Value::String(name.clone());
        }
        if let Some(summary) = &self.summary {
            doc["summary"] = serde_json::Value::String(summary.clone());
        }
        if let Some(icon) = &self.icon {
            doc["icon"] = serde_json::to_value(icon).unwrap_or_default();
        }
        if let Some(image) = &self.image {
            doc["image"] = serde_json::to_value(image).unwrap_or_default();
        }
        doc
    }

    /// Recovers the Iroh `SecretKey` from the stored hex string.
    ///
    /// Used by `signatures.rs` when signing outgoing HTTP requests on behalf
    /// of this actor.
    ///
    /// # Errors
    /// Returns `Err` if the hex is invalid or not exactly 32 bytes.
    pub fn secret_key(&self) -> Result<iroh::SecretKey, String> {
        let bytes = hex::decode(&self.secret_key_hex)
            .map_err(|e| format!("Invalid secret key hex: {}", e))?;
        if bytes.len() != 32 {
            return Err(format!("Secret key must be 32 bytes, got {}", bytes.len()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(iroh::SecretKey::from_bytes(&arr))
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// PEM Encoding
// ═══════════════════════════════════════════════════════════════════════════════

/// Converts a raw 32-byte Ed25519 public key to PEM format.
///
/// Wraps the key in an ASN.1 DER `SubjectPublicKeyInfo` structure (OID
/// 1.3.101.112 = id-EdDSA / Ed25519), then base64-encodes with PEM headers.
/// This is the format expected by the `publicKeyPem` field in ActivityPub
/// actor documents and by most HTTP Signature verification libraries.
///
/// # Parameters
/// - `public_key_bytes` — the raw 32-byte Ed25519 public key.
///
/// # Returns
/// A PEM-encoded string including `-----BEGIN PUBLIC KEY-----` headers.
pub fn ed25519_public_key_to_pem(public_key_bytes: &[u8; 32]) -> String {
    // ASN.1 DER prefix for Ed25519 SubjectPublicKeyInfo:
    // SEQUENCE (42 bytes) → SEQUENCE (5 bytes) → OID 1.3.101.112 → BIT STRING (33 bytes)
    let der_prefix: [u8; 12] = [
        0x30, 0x2a,
        0x30, 0x05,
        0x06, 0x03, 0x2b, 0x65, 0x70,
        0x03, 0x21, 0x00,
    ];
    let mut der = Vec::with_capacity(44);
    der.extend_from_slice(&der_prefix);
    der.extend_from_slice(public_key_bytes);
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &der);
    let mut pem = String::from("-----BEGIN PUBLIC KEY-----\n");
    for (i, ch) in b64.chars().enumerate() {
        pem.push(ch);
        if (i + 1) % 64 == 0 {
            pem.push('\n');
        }
    }
    if !pem.ends_with('\n') {
        pem.push('\n');
    }
    pem.push_str("-----END PUBLIC KEY-----\n");
    pem
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_actor_creation() {
        let secret = iroh::SecretKey::generate(&mut rand::rng());
        let actor = ApActor::new("alice", Some("Alice".to_string()), None, &secret);
        assert_eq!(actor.preferred_username, "alice");
        assert!(actor.id.starts_with("https://bytescape.social/@alice"));
        assert!(actor.public_key.public_key_pem.contains("BEGIN PUBLIC KEY"));
    }

    #[test]
    fn test_activitystreams_serialization() {
        let secret = iroh::SecretKey::generate(&mut rand::rng());
        let mut actor = ApActor::new("bob", Some("Bob".to_string()), Some("Hello!".to_string()), &secret);
        actor.owner_did = "did:key:test123".to_string();
        let doc = actor.to_activitystreams();
        assert_eq!(doc["type"], "Person");
        assert_eq!(doc["preferredUsername"], "bob");
        assert!(doc.get("secretKeyHex").is_none());
    }

    #[test]
    fn test_key_roundtrip() {
        let secret = iroh::SecretKey::generate(&mut rand::rng());
        let actor = ApActor::new("carol", None, None, &secret);
        let recovered = actor.secret_key().unwrap();
        assert_eq!(secret.public(), recovered.public());
    }

    #[test]
    fn test_pem_structure() {
        let secret = iroh::SecretKey::generate(&mut rand::rng());
        let pem = ed25519_public_key_to_pem(secret.public().as_bytes());
        assert!(pem.starts_with("-----BEGIN PUBLIC KEY-----"));
        assert!(pem.ends_with("-----END PUBLIC KEY-----\n"));
    }
}
