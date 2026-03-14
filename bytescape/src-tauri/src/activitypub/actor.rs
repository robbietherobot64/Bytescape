//! AP Actor — local user's ActivityPub identity.

use serde::{Deserialize, Serialize};
use super::types::{ApPublicKey, ApMediaLink, ap_context};

pub const AP_DOMAIN: &str = "bytescape.social";

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

impl ApActor {
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

    pub fn to_activitystreams(&self) -> serde_json::Value {
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

pub fn ed25519_public_key_to_pem(public_key_bytes: &[u8; 32]) -> String {
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
