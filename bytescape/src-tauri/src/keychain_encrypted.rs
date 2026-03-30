//! # keychain_encrypted.rs — Encrypted Credential & Identity Vault
//!
//! This module is Bytescape's secure storage for all sensitive data: session tokens,
//! cryptographic keys, DIDs (decentralized identifiers), linked accounts, and recovery
//! escrow bundles. Everything is encrypted on disk using ChaCha20-Poly1305 authenticated
//! encryption with Argon2id key derivation — the same level of security used by
//! password managers like Bitwarden.
//!
//! ## How It Works
//! 1. On first launch, `lib.rs` generates a random 32-byte master key (or reads one
//!    from the environment / a persisted file)
//! 2. An `EncryptedKeychain` is created with this master key and a file path
//! 3. When credentials need to be stored, the `AuthData` struct is:
//!    a. Serialized to JSON
//!    b. Encrypted with ChaCha20-Poly1305 (random salt + nonce per write)
//!    c. Base64-encoded and written to `keychain.json`
//! 4. On read, the process reverses: file → base64 decode → decrypt → parse JSON
//! 5. A Mutex-guarded in-memory cache avoids repeated disk I/O for frequent reads
//!
//! ## Identity System (5 DID Types)
//! Bytescape uses a layered identity system where users accumulate DIDs over time:
//!
//! | DID Type | Example | When Created | Purpose |
//! |----------|---------|-------------|---------|
//! | `did:key` | `did:key:z6Mk...` | First P2P setup | Offline/P2P identity (from Ed25519 key) |
//! | `did:plc` | `did:plc:abc123` | Bluesky login | Bluesky permanent identity |
//! | `did:web` | `did:web:bytescape.social:alice` | Username chosen | Master Bytescape identity |
//! | `acct:` URI | `acct:alice@mastodon.social` | Mastodon linked | Fediverse identity |
//! | Actor URL | `https://mastodon.social/@alice` | Mastodon linked | ActivityPub actor |
//!
//! `get_owner_did()` returns the highest-priority DID: web > plc > key.
//! `all_owner_dids()` returns ALL known identifiers for inclusive matching.
//!
//! ## Recovery Escrow
//! When a user sets up identity recovery, their Iroh secret key is encrypted with
//! a 6-digit code derived via Argon2id and stored as a `RecoveryEscrowBundle`. This
//! bundle can be transferred to a new device and unlocked with the code, restoring
//! the user's P2P identity without a central server.
//!
//! ## Security Features
//! - ChaCha20-Poly1305 authenticated encryption (256-bit key)
//! - Argon2id key derivation (memory-hard, resistant to GPU attacks)
//! - Random salt per encryption (no two writes produce the same ciphertext)
//! - Random nonce per encryption (12 bytes, unique per operation)
//! - File permissions restricted to owner-only on Unix (mode 0o600 for master key)
//!
//! ## Related Files
//! - `lib.rs` — Initializes the keychain in `run()` with the master password
//! - `oauth.rs` — Has its own `FileSessionStore` with similar encryption (for
//!   atrium-oauth's session persistence)
//! - `auth.rs` — Session tokens from app-password login are stored here
//! - `commands/auth.rs` — Tauri commands that call keychain methods

use argon2::Argon2;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::error::BytescapeError;

// ═══════════════════════════════════════════════════════════════════
// ENCRYPTION CONSTANTS
// Parameters for the ChaCha20-Poly1305 + Argon2id encryption scheme.
// These are fixed — changing them would break existing keychain files.
// ═══════════════════════════════════════════════════════════════════

/// Salt length for Argon2id key derivation (stored in plaintext alongside ciphertext).
const SALT_LEN: usize = 16;
/// Nonce length for ChaCha20-Poly1305 (unique per encryption operation).
const NONCE_LEN: usize = 12;
/// Version tag for recovery escrow bundle format (for future-proofing).
const RECOVERY_VERSION: u8 = 1;

// ═══════════════════════════════════════════════════════════════════
// DATA STRUCTURES
// The AuthData struct is the "vault" — it holds everything sensitive
// about the user's identity. LinkedAccount represents one connected
// platform. Recovery types support the escrow recovery flow.
// ═══════════════════════════════════════════════════════════════════

/// The complete set of decrypted authentication and identity data.
///
/// This struct is what gets encrypted and written to `keychain.json`. It contains
/// every piece of sensitive data Bytescape manages: AT Protocol tokens, Iroh keys,
/// DIDs, recovery escrow state, Mastodon sessions, and linked accounts.
///
/// All fields are `Option` because they accumulate over time — a fresh install
/// has nothing, then the user links Bluesky (adds DID + tokens), then sets a
/// username (adds web_did), then links Mastodon (adds mastodon_session_json), etc.
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct AuthData {
    /// The Iroh Ed25519 secret key (hex-encoded). This is the user's P2P identity.
    /// All P2P operations (document sync, blob transfer, gossip) use this key.
    pub iroh_secret_key: Option<String>,
    /// The Bluesky DID (e.g., "did:plc:abc123...") from the AT Protocol session.
    pub did: Option<String>,
    /// The Bluesky handle (e.g., "alice.bsky.social").
    pub handle: Option<String>,
    /// Short-lived Bluesky access JWT (~2 hour lifetime).
    /// For OAuth sessions, this holds the sentinel value "oauth_session".
    pub access_jwt: Option<String>,
    /// Long-lived Bluesky refresh JWT (~90 day lifetime).
    /// For OAuth sessions, this holds the sentinel value "oauth_session".
    pub refresh_jwt: Option<String>,
    /// Timestamp when tokens were last refreshed
    pub last_refresh: Option<u64>,
    /// Master DID (did:key:z6Mk...) derived from the Iroh Ed25519 keypair.
    /// Used for P2P/offline interactions, relay syncing, friend content access.
    pub master_did: Option<String>,
    /// Web DID (did:web:bytescape.social:{username}) — the user's master identity.
    /// Assigned when they set their Bytescape username. All platform accounts link to this.
    #[serde(default)]
    pub web_did: Option<String>,
    /// Local identity DID used for P2P identity/recovery flows.
    pub local_identity_did: Option<String>,
    /// Stable node public key for local identity.
    pub local_identity_public_key: Option<String>,
    /// User-chosen Bytescape local username (universal overarching ID).
    pub local_identity_username: Option<String>,
    /// Local escrow bundle copy (encrypted blob payload + metadata).
    pub recovery_escrow_bundle: Option<String>,
    /// Escrow creation timestamp.
    pub recovery_escrow_created_at: Option<u64>,
    /// Ongoing recovery challenge ID.
    pub recovery_pending_challenge: Option<String>,
    /// Recovery bundle currently under verification.
    pub recovery_pending_bundle: Option<String>,
    /// Temporarily verified DID before finalization.
    pub recovery_pending_verified_did: Option<String>,
    /// Temporarily verified public key before finalization.
    pub recovery_pending_verified_public_key: Option<String>,
    /// Temporarily verified secret before finalization.
    pub recovery_pending_verified_secret: Option<String>,
    /// Last successful recovery timestamp.
    pub recovery_last_recovered_at: Option<u64>,
    /// Serialized Mastodon session + app credentials (JSON blob).
    #[serde(default)]
    pub mastodon_session_json: Option<String>,
    /// Linked platform accounts (Bluesky, Mastodon, etc.) under this identity.
    #[serde(default)]
    pub linked_accounts: Vec<LinkedAccount>,
}

/// A platform account linked to the user's master identity.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LinkedAccount {
    /// Protocol name: "bluesky", "mastodon", "activitypub"
    pub protocol: String,
    /// Canonical identifier for this linked account:
    /// - Bluesky: `did:plc:...` (real PLC DID from plc.directory)
    /// - Mastodon: `acct:user@instance`
    /// - Generic AP: actor URL (`https://instance/@username`)
    pub identifier: String,
    /// Platform-native identifier (acct:user@instance for Mastodon, same as identifier for Bluesky)
    #[serde(default)]
    pub platform_identifier: Option<String>,
    /// Human-readable handle: "@user.bsky.social", "@user@mastodon.social"
    pub display_handle: String,
    /// Display name (optional)
    pub display_name: Option<String>,
    /// Avatar URL (optional)
    pub avatar_url: Option<String>,
    /// Unix timestamp when the account was linked
    pub connected_at: u64,
    /// Whether the account is currently active/connected
    pub active: bool,
}

/// An encrypted bundle containing the user's Iroh secret key for recovery.
///
/// When a user initializes identity recovery, their Iroh secret key is encrypted
/// with a key derived from a 6-digit recovery code. The bundle can be saved
/// externally (e.g., printed, stored in a password manager) and used to restore
/// identity on a new device by entering the code.
///
/// Wire format: version + metadata + encrypted payload (ChaCha20-Poly1305).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RecoveryEscrowBundle {
    /// Format version (currently 1) for forward-compatible deserialization.
    pub version: u8,
    /// Unix timestamp when the bundle was created.
    pub created_at: u64,
    /// The DID this bundle recovers (did:key:z6Mk...).
    pub did: String,
    /// The Ed25519 public key (hex) corresponding to the escrowed secret.
    pub public_key: String,
    /// Base64-encoded Argon2id salt used to derive the encryption key from the recovery code.
    pub salt_b64: String,
    /// Base64-encoded ChaCha20-Poly1305 nonce.
    pub nonce_b64: String,
    /// Base64-encoded ciphertext (encrypted `RecoveryEscrowPayload`).
    pub ciphertext_b64: String,
    /// MD5 integrity hash for quick validation before attempting decryption.
    pub integrity_md5: String,
}

/// The plaintext payload inside a `RecoveryEscrowBundle` (after decryption).
#[derive(Serialize, Deserialize, Debug, Clone)]
struct RecoveryEscrowPayload {
    version: u8,
    did: String,
    public_key: String,
    /// The Iroh Ed25519 secret key in hex — the actual secret being protected.
    iroh_secret_hex: String,
}

/// Result of initializing a fresh identity with recovery escrow.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IdentityInitResult {
    /// The newly generated DID (did:key:z6Mk...).
    pub did: String,
    /// The Ed25519 public key (hex).
    pub public_key: String,
    /// The serialized recovery escrow bundle (JSON string).
    pub escrow_bundle: String,
}

/// Status snapshot of the user's identity and recovery state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IdentityRecoveryStatus {
    pub has_local_identity: bool,
    pub has_escrow: bool,
    pub recovery_pending: bool,
    pub recovery_verified_pending_finalize: bool,
    pub did: Option<String>,
    pub public_key: Option<String>,
    pub last_recovered_at: Option<u64>,
}

/// Result of starting a recovery process from an escrow bundle.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IdentityRecoveryStartResult {
    /// Challenge ID for the recovery session (opaque identifier).
    pub challenge_id: String,
    /// The DID embedded in the bundle (hint for the user to verify).
    pub did_hint: String,
}

/// Result of verifying the recovery code against the escrow bundle.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IdentityRecoveryVerifyResult {
    /// Whether the code successfully decrypted the bundle.
    pub verified: bool,
    /// The DID that was recovered.
    pub did: String,
    /// The Ed25519 public key that was recovered.
    pub public_key: String,
}

/// Result of finalizing recovery (committing the restored identity).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IdentityRecoveryFinalizeResult {
    /// Whether the identity was successfully restored.
    pub restored: bool,
    /// The DID now active on this device.
    pub did: String,
    /// The Ed25519 public key now active on this device.
    pub public_key: String,
}

// ═══════════════════════════════════════════════════════════════════
// ENCRYPTED KEYCHAIN
// The main struct that ties everything together — file I/O, encryption,
// caching, and all the identity/session management operations.
// ═══════════════════════════════════════════════════════════════════

/// Encrypted keychain for secure credential storage.
///
/// All sensitive data (tokens, keys, DIDs) passes through this struct.
/// It maintains a Mutex-guarded in-memory cache to avoid repeated disk
/// reads, and flushes to the encrypted file on every mutation.
///
/// ## Usage Pattern
/// ```text
/// Read:  cache.lock() → if Some → return clone → else load_encrypted() → update cache → return
/// Write: cache.lock() → mutate → save_encrypted() → update cache
/// ```
pub struct EncryptedKeychain {
    /// Path to the encrypted keychain file (typically `~/.local/share/.../keychain.json`).
    file_path: PathBuf,
    /// In-memory cache of the decrypted `AuthData`. Avoids disk I/O on every
    /// `get_access_jwt()` or `get_owner_did()` call.
    cache: Arc<Mutex<Option<AuthData>>>,
    /// Master password (or machine-generated key) used for Argon2id key derivation.
    master_password: String,
}

impl EncryptedKeychain {
    /// Create a new encrypted keychain pointed at the given file path.
    ///
    /// Does NOT read from disk immediately — the cache starts empty and is
    /// populated lazily on the first read operation (e.g., `load_session()`).
    ///
    /// # Arguments
    /// * `path` — Path to the keychain file (e.g., `app_data_dir/keychain.json`)
    /// * `master_password` — Password (or machine-generated key) for encryption
    pub fn new(path: PathBuf, master_password: String) -> Self {
        println!("EncryptedKeychain: Using encrypted storage at {:?}", path);
        
        Self {
            file_path: path,
            cache: Arc::new(Mutex::new(None)),
            master_password,
        }
    }

    // ─── Core Cryptographic Operations ─────────────────────────────────

    /// Derive a 256-bit encryption key from the master password + salt using Argon2id.
    ///
    /// Argon2id is a memory-hard KDF that resists GPU/ASIC brute-force attacks.
    /// Each call produces a unique key because the salt is random per encryption.
    fn derive_key(&self, salt: &[u8]) -> Result<[u8; 32], BytescapeError> {
        // Use Argon2id to derive a 256-bit key
        let argon2 = Argon2::default();
        let mut key_material = [0u8; 32];
        
        argon2.hash_password_into(
            self.master_password.as_bytes(),
            salt,
            &mut key_material,
        ).map_err(|e| BytescapeError::Keychain(format!("Key derivation failed: {}", e)))?;
        
        Ok(key_material)
    }

    /// Generate a cryptographically random salt (16 bytes).
    fn generate_salt() -> Vec<u8> {
        use rand::RngCore;
        let mut salt = vec![0u8; SALT_LEN];
        rand::rng().fill_bytes(&mut salt);
        salt
    }

    /// Encrypt the `AuthData` struct and write it to the keychain file.
    ///
    /// Each write uses a fresh random salt and nonce, so the ciphertext is
    /// different every time even if the data hasn't changed. The wire format
    /// is: `base64(salt || nonce || ciphertext)` wrapped in a JSON object.
    ///
    /// # Side Effects
    /// * Writes to the keychain file on disk (atomic overwrite)
    fn save_encrypted(&self, data: &AuthData) -> Result<(), BytescapeError> {
        use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
        use chacha20poly1305::aead::Aead;
        
        // Generate new salt and nonce for this encryption
        let salt = Self::generate_salt();
        let nonce_bytes = Self::generate_nonce_bytes();
        
        // Derive key
        let key_bytes = self.derive_key(&salt)?;
        let cipher = ChaCha20Poly1305::new_from_slice(&key_bytes)
            .map_err(|e| BytescapeError::KeychainSave(format!("Invalid key: {}", e)))?;
        
        // Serialize data to JSON
        let json = serde_json::to_string(data)
            .map_err(|e| BytescapeError::KeychainSave(format!("Serialization failed: {}", e)))?;
        
        // Encrypt
        let nonce = Nonce::from(nonce_bytes);
        let ciphertext = cipher.encrypt(&nonce, json.as_bytes())
            .map_err(|e| BytescapeError::KeychainSave(format!("Encryption failed: {}", e)))?;
        
        // Combine salt + nonce + ciphertext and encode as base64
        let mut encrypted_data = salt;
        encrypted_data.extend_from_slice(&nonce_bytes);
        encrypted_data.extend_from_slice(&ciphertext);
        let encoded = STANDARD.encode(&encrypted_data);
        
        // Write to file
        let content = serde_json::to_string_pretty(&serde_json::json!({
            "encrypted_data": encoded
        }))
            .map_err(|e| BytescapeError::KeychainSave(format!("Serialization failed: {}", e)))?;
        fs::write(&self.file_path, content)
            .map_err(|e| BytescapeError::KeychainSave(format!("File write failed: {}", e)))?;
        
        Ok(())
    }

    /// Decrypt and load auth data
    fn load_encrypted(&self) -> Result<Option<AuthData>, BytescapeError> {
        use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
        use chacha20poly1305::aead::Aead;
        
        // Read file
        let content = match fs::read_to_string(&self.file_path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(BytescapeError::KeychainLoad(format!("File read failed: {}", e))),
        };
        
        // Parse encrypted store
        let store: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| BytescapeError::KeychainLoad(format!("JSON parse failed: {}", e)))?;
        
        let encoded = store["encrypted_data"].as_str()
            .ok_or_else(|| BytescapeError::KeychainLoad("Missing encrypted data".to_string()))?;
        
        // Decode data
        let encrypted_data = STANDARD.decode(encoded)
            .map_err(|e| BytescapeError::KeychainLoad(format!("Data decode failed: {}", e)))?;
        
        // Split salt, nonce, and ciphertext
        if encrypted_data.len() <= SALT_LEN + NONCE_LEN {
            return Err(BytescapeError::KeychainLoad("Invalid encrypted data length".to_string()));
        }
        let salt = &encrypted_data[..SALT_LEN];
        let nonce_bytes = &encrypted_data[SALT_LEN..SALT_LEN + NONCE_LEN];
        let ciphertext = &encrypted_data[SALT_LEN + NONCE_LEN..];
        
        // Derive key
        let key_bytes = self.derive_key(salt)?;
        let cipher = ChaCha20Poly1305::new_from_slice(&key_bytes)
            .map_err(|e| BytescapeError::KeychainLoad(format!("Invalid key: {}", e)))?;
        
        // Decrypt
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = cipher.decrypt(nonce, ciphertext)
            .map_err(|_| BytescapeError::KeychainLoad("Decryption failed - wrong password or corrupted data".to_string()))?;
        
        // Parse JSON
        let data: AuthData = serde_json::from_slice(&plaintext)
            .map_err(|e| BytescapeError::KeychainLoad(format!("JSON parse failed: {}", e)))?;
        
        Ok(Some(data))
    }

    /// Generate a cryptographically random 12-byte nonce for ChaCha20-Poly1305.
    fn generate_nonce_bytes() -> [u8; NONCE_LEN] {
        use rand::RngCore;
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::rng().fill_bytes(&mut nonce_bytes);
        nonce_bytes
    }

    /// Get the current Unix timestamp in seconds.
    fn now_unix() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    /// Derive a 256-bit key from a recovery code + salt using Argon2id.
    /// Used for encrypting/decrypting recovery escrow bundles.
    fn derive_recovery_key(recovery_code: &str, salt: &[u8]) -> Result<[u8; 32], BytescapeError> {
        let argon2 = Argon2::default();
        let mut key_material = [0u8; 32];
        argon2.hash_password_into(
            recovery_code.as_bytes(),
            salt,
            &mut key_material,
        ).map_err(|e| BytescapeError::Keychain(format!("Recovery key derivation failed: {}", e)))?;
        Ok(key_material)
    }

    // ─── DID Generation & Management ───────────────────────────────────

    /// Generate a `did:key:z6Mk...` from raw Ed25519 public key bytes (32 bytes).
    ///
    /// Format: multicodec(0xed01) + pubkey_bytes → base58btc → prepend 'z' → `did:key:z...`
    /// See: https://w3c-ccg.github.io/did-method-key/
    ///
    /// Used for P2P/offline identity (ephemeral interactions, relay syncing).
    pub fn did_key_from_ed25519_bytes(pubkey_bytes: &[u8; 32]) -> String {
        // Multicodec prefix for Ed25519 public key: 0xed, 0x01
        let mut multicodec = Vec::with_capacity(34);
        multicodec.push(0xed);
        multicodec.push(0x01);
        multicodec.extend_from_slice(pubkey_bytes);
        // base58btc encode with 'z' multibase prefix
        let encoded = bs58::encode(&multicodec).into_string();
        format!("did:key:z{}", encoded)
    }

    /// Generate a `did:key` from the stored Iroh secret key hex.
    /// Returns None if no Iroh key is stored.
    pub fn compute_master_did(&self) -> Result<Option<String>, BytescapeError> {
        let cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        let secret_hex = match cache.as_ref().and_then(|d| d.iroh_secret_key.clone()) {
            Some(h) => h,
            None => return Ok(None),
        };
        drop(cache);
        let bytes = hex::decode(&secret_hex)
            .map_err(|e| BytescapeError::Keychain(format!("Invalid iroh key hex: {}", e)))?;
        let secret = iroh::SecretKey::try_from(bytes.as_slice())
            .map_err(|e| BytescapeError::Keychain(format!("Invalid iroh secret: {}", e)))?;
        let pubkey = secret.public();
        Ok(Some(Self::did_key_from_ed25519_bytes(pubkey.as_bytes())))
    }

    /// Ensure the master_did (did:key) is computed and persisted in the keychain.
    /// Returns the P2P identity DID string (did:key:z6Mk...).
    pub fn ensure_master_did(&self) -> Result<Option<String>, BytescapeError> {
        // Check if already cached
        {
            let cache = self.cache.lock()
                .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
            if let Some(ref data) = *cache {
                if data.master_did.is_some() {
                    return Ok(data.master_did.clone());
                }
            }
        }
        // Compute from stored Iroh key
        let did = self.compute_master_did()?;
        if let Some(ref did_str) = did {
            let mut cache = self.cache.lock()
                .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
            let mut data = match cache.clone() {
                Some(d) => d,
                None => self.load_encrypted()?.unwrap_or_default(),
            };
            data.master_did = Some(did_str.clone());
            self.save_encrypted(&data)?;
            *cache = Some(data);
        }
        Ok(did)
    }

    /// Generate a `did:web:bytescape.social:{username}` master identity.
    ///
    /// This is the user's overarching identity that all platform accounts link to.
    /// Returns None if the user hasn't set a local username yet.
    pub fn ensure_web_did(&self) -> Result<Option<String>, BytescapeError> {
        // Check if already cached
        {
            let cache = self.cache.lock()
                .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
            if let Some(ref data) = *cache {
                if data.web_did.is_some() {
                    return Ok(data.web_did.clone());
                }
            }
        }
        // Compute from username
        let username = self.get_local_username()?;
        if let Some(ref name) = username {
            let web_did = format!("did:web:bytescape.social:{}", name);
            let mut cache = self.cache.lock()
                .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
            let mut data = match cache.clone() {
                Some(d) => d,
                None => self.load_encrypted()?.unwrap_or_default(),
            };
            data.web_did = Some(web_did.clone());
            self.save_encrypted(&data)?;
            *cache = Some(data);
            return Ok(Some(web_did));
        }
        Ok(None)
    }

    /// Get the web DID (did:web:bytescape.social:{username}) if already computed.
    pub fn get_web_did(&self) -> Result<Option<String>, BytescapeError> {
        let cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        if let Some(ref data) = *cache {
            return Ok(data.web_did.clone());
        }
        Ok(None)
    }

    /// Legacy helper: generate a local DID from a public key string.
    /// Now delegates to did:key format.
    fn local_did_for_public_key(public_key: &str) -> String {
        // Try to parse as an Iroh PublicKey and use did:key format
        if let Ok(pk) = public_key.parse::<iroh::PublicKey>() {
            return Self::did_key_from_ed25519_bytes(pk.as_bytes());
        }
        // Fallback for truly unparseable keys
        format!("did:key:z{}", public_key.to_lowercase())
    }

    // ─── Recovery Escrow System ────────────────────────────────────────
    // These methods implement a "paper key" recovery system:
    // 1. User initializes recovery → their Iroh secret key is encrypted
    //    with a code they choose and stored as an escrow bundle
    // 2. On a new device, they paste the bundle and enter the code
    // 3. The code decrypts the bundle, restoring their P2P identity

    /// Parse and validate a serialized recovery escrow bundle (JSON string).
    ///
    /// Checks the format version and runs an integrity check (BLAKE3 or legacy MD5)
    /// on the ciphertext to detect corruption before attempting decryption.
    fn parse_escrow_bundle(bundle_json: &str) -> Result<RecoveryEscrowBundle, BytescapeError> {
        let bundle: RecoveryEscrowBundle = serde_json::from_str(bundle_json)
            .map_err(|e| BytescapeError::InvalidInput(format!("Invalid escrow bundle JSON: {}", e)))?;

        if bundle.version != RECOVERY_VERSION {
            return Err(BytescapeError::InvalidInput(format!(
                "Unsupported recovery bundle version: {}",
                bundle.version
            )));
        }

        let ciphertext = STANDARD.decode(&bundle.ciphertext_b64)
            .map_err(|e| BytescapeError::InvalidInput(format!("Invalid escrow ciphertext encoding: {}", e)))?;

        // CRIT-6: Support both legacy MD5 (32 hex chars) and new BLAKE3 (64 hex chars)
        // integrity hashes for backward compatibility with existing escrow bundles.
        let integrity_ok = if bundle.integrity_md5.len() == 64 {
            // New BLAKE3 format
            let expected = blake3::hash(&ciphertext).to_hex().to_string();
            bundle.integrity_md5 == expected
        } else {
            // Legacy MD5 format (32 hex chars)
            let expected = format!("{:x}", md5::compute(&ciphertext));
            bundle.integrity_md5 == expected
        };
        if !integrity_ok {
            return Err(BytescapeError::InvalidInput("Escrow integrity check failed".to_string()));
        }

        Ok(bundle)
    }

    /// Decrypt a recovery escrow bundle using the recovery code.
    ///
    /// Derives an encryption key from the code + salt via Argon2id, then decrypts
    /// the ChaCha20-Poly1305 ciphertext to recover the `RecoveryEscrowPayload`.
    ///
    /// # Errors
    /// Returns `InvalidCredentials` if the code is wrong (decryption fails).
    fn decrypt_escrow_bundle(bundle: &RecoveryEscrowBundle, recovery_code: &str) -> Result<RecoveryEscrowPayload, BytescapeError> {
        use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
        use chacha20poly1305::aead::Aead;

        let salt = STANDARD.decode(&bundle.salt_b64)
            .map_err(|e| BytescapeError::InvalidInput(format!("Invalid escrow salt encoding: {}", e)))?;
        let nonce = STANDARD.decode(&bundle.nonce_b64)
            .map_err(|e| BytescapeError::InvalidInput(format!("Invalid escrow nonce encoding: {}", e)))?;
        let ciphertext = STANDARD.decode(&bundle.ciphertext_b64)
            .map_err(|e| BytescapeError::InvalidInput(format!("Invalid escrow ciphertext encoding: {}", e)))?;

        if nonce.len() != NONCE_LEN {
            return Err(BytescapeError::InvalidInput("Invalid escrow nonce length".to_string()));
        }

        let key_bytes = Self::derive_recovery_key(recovery_code, &salt)?;
        let cipher = ChaCha20Poly1305::new_from_slice(&key_bytes)
            .map_err(|e| BytescapeError::KeychainLoad(format!("Invalid recovery key: {}", e)))?;

        let plaintext = cipher.decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
            .map_err(|_| BytescapeError::InvalidCredentials)?;

        let payload: RecoveryEscrowPayload = serde_json::from_slice(&plaintext)
            .map_err(|e| BytescapeError::KeychainLoad(format!("Invalid escrow payload: {}", e)))?;

        if payload.version != RECOVERY_VERSION {
            return Err(BytescapeError::InvalidInput("Unsupported escrow payload version".to_string()));
        }

        Ok(payload)
    }

    /// Initialize local identity and create an encrypted recovery escrow bundle.
    pub fn identity_init_full_recovery(&self, recovery_code: &str) -> Result<IdentityInitResult, BytescapeError> {
        use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
        use chacha20poly1305::aead::Aead;

        if recovery_code.trim().len() < 8 {
            return Err(BytescapeError::InvalidInput("Recovery code must be at least 8 characters".to_string()));
        }

        let mut cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        let mut data = match cache.clone() {
            Some(v) => v,
            None => self.load_encrypted()?.unwrap_or_default(),
        };

        let (secret_hex, public_key) = match data.iroh_secret_key.clone() {
            Some(secret_hex) => {
                let bytes = hex::decode(&secret_hex)
                    .map_err(|e| BytescapeError::KeychainLoad(format!("Invalid stored iroh key: {}", e)))?;
                let secret = iroh::SecretKey::try_from(bytes.as_slice())
                    .map_err(|e| BytescapeError::KeychainLoad(format!("Invalid iroh secret bytes: {}", e)))?;
                (secret_hex, secret.public().to_string())
            }
            None => {
                let mut rng = rand::rng();
                let secret = iroh::SecretKey::generate(&mut rng);
                let secret_hex = hex::encode(secret.to_bytes());
                (secret_hex, secret.public().to_string())
            }
        };

        let did = data
            .local_identity_did
            .clone()
            .unwrap_or_else(|| Self::local_did_for_public_key(&public_key));

        let payload = RecoveryEscrowPayload {
            version: RECOVERY_VERSION,
            did: did.clone(),
            public_key: public_key.clone(),
            iroh_secret_hex: secret_hex.clone(),
        };

        let payload_bytes = serde_json::to_vec(&payload)
            .map_err(|e| BytescapeError::KeychainSave(format!("Failed to serialize escrow payload: {}", e)))?;
        let salt = Self::generate_salt();
        let nonce = Self::generate_nonce_bytes();
        let recovery_key = Self::derive_recovery_key(recovery_code.trim(), &salt)?;
        let cipher = ChaCha20Poly1305::new_from_slice(&recovery_key)
            .map_err(|e| BytescapeError::KeychainSave(format!("Invalid recovery key: {}", e)))?;
        let ciphertext = cipher.encrypt(&Nonce::from(nonce), payload_bytes.as_ref())
            .map_err(|e| BytescapeError::KeychainSave(format!("Failed to encrypt escrow payload: {}", e)))?;

        let bundle = RecoveryEscrowBundle {
            version: RECOVERY_VERSION,
            created_at: Self::now_unix(),
            did: did.clone(),
            public_key: public_key.clone(),
            salt_b64: STANDARD.encode(&salt),
            nonce_b64: STANDARD.encode(nonce),
            ciphertext_b64: STANDARD.encode(&ciphertext),
            // CRIT-6: Use BLAKE3 for new escrow bundles (field name kept for serde compat)
            integrity_md5: blake3::hash(&ciphertext).to_hex().to_string(),
        };

        let bundle_json = serde_json::to_string(&bundle)
            .map_err(|e| BytescapeError::KeychainSave(format!("Failed to serialize escrow bundle: {}", e)))?;

        data.iroh_secret_key = Some(secret_hex);
        data.local_identity_did = Some(did.clone());
        data.local_identity_public_key = Some(public_key.clone());
        data.recovery_escrow_bundle = Some(bundle_json.clone());
        data.recovery_escrow_created_at = Some(bundle.created_at);
        data.recovery_pending_challenge = None;
        data.recovery_pending_bundle = None;
        data.recovery_pending_verified_did = None;
        data.recovery_pending_verified_public_key = None;
        data.recovery_pending_verified_secret = None;

        self.save_encrypted(&data)?;
        *cache = Some(data);

        Ok(IdentityInitResult {
            did,
            public_key,
            escrow_bundle: bundle_json,
        })
    }

    /// Return current local identity/recovery state.
    pub fn get_identity_recovery_status(&self) -> Result<IdentityRecoveryStatus, BytescapeError> {
        let mut cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        let data = match cache.clone() {
            Some(v) => v,
            None => {
                let loaded = self.load_encrypted()?.unwrap_or_default();
                *cache = Some(loaded.clone());
                loaded
            }
        };

        Ok(IdentityRecoveryStatus {
            has_local_identity: data.local_identity_did.is_some() && data.local_identity_public_key.is_some(),
            has_escrow: data.recovery_escrow_bundle.is_some(),
            recovery_pending: data.recovery_pending_bundle.is_some(),
            recovery_verified_pending_finalize: data.recovery_pending_verified_secret.is_some(),
            did: data.local_identity_did,
            public_key: data.local_identity_public_key,
            last_recovered_at: data.recovery_last_recovered_at,
        })
    }

    /// Start recovery with an escrow bundle.
    pub fn start_recovery_from_bundle(&self, escrow_bundle: &str) -> Result<IdentityRecoveryStartResult, BytescapeError> {
        let bundle = Self::parse_escrow_bundle(escrow_bundle)?;

        let mut cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        let mut data = match cache.clone() {
            Some(v) => v,
            None => self.load_encrypted()?.unwrap_or_default(),
        };

        let challenge_id = uuid::Uuid::new_v4().to_string();
        data.recovery_pending_challenge = Some(challenge_id.clone());
        data.recovery_pending_bundle = Some(escrow_bundle.to_string());
        data.recovery_pending_verified_did = None;
        data.recovery_pending_verified_public_key = None;
        data.recovery_pending_verified_secret = None;

        self.save_encrypted(&data)?;
        *cache = Some(data);

        Ok(IdentityRecoveryStartResult {
            challenge_id,
            did_hint: bundle.did,
        })
    }

    /// Verify recovery code for the currently pending recovery bundle.
    pub fn verify_recovery_code(&self, recovery_code: &str) -> Result<IdentityRecoveryVerifyResult, BytescapeError> {
        if recovery_code.trim().is_empty() {
            return Err(BytescapeError::InvalidInput("Recovery code is required".to_string()));
        }

        let mut cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        let mut data = match cache.clone() {
            Some(v) => v,
            None => self.load_encrypted()?.unwrap_or_default(),
        };

        let bundle_json = data.recovery_pending_bundle.clone()
            .ok_or_else(|| BytescapeError::InvalidInput("No pending recovery request".to_string()))?;
        let bundle = Self::parse_escrow_bundle(&bundle_json)?;
        let payload = Self::decrypt_escrow_bundle(&bundle, recovery_code.trim())?;

        let key_bytes = hex::decode(&payload.iroh_secret_hex)
            .map_err(|e| BytescapeError::KeychainLoad(format!("Invalid recovered key hex: {}", e)))?;
        let secret = iroh::SecretKey::try_from(key_bytes.as_slice())
            .map_err(|e| BytescapeError::KeychainLoad(format!("Invalid recovered key bytes: {}", e)))?;
        let public_key = secret.public().to_string();

        if public_key != payload.public_key || payload.did != bundle.did {
            return Err(BytescapeError::Validation("Recovery bundle verification failed".to_string()));
        }

        data.recovery_pending_verified_secret = Some(payload.iroh_secret_hex.clone());
        data.recovery_pending_verified_did = Some(payload.did.clone());
        data.recovery_pending_verified_public_key = Some(payload.public_key.clone());

        self.save_encrypted(&data)?;
        *cache = Some(data);

        Ok(IdentityRecoveryVerifyResult {
            verified: true,
            did: payload.did,
            public_key: payload.public_key,
        })
    }

    /// Finalize recovery by promoting verified pending credentials to active identity.
    pub fn finalize_recovery(&self) -> Result<IdentityRecoveryFinalizeResult, BytescapeError> {
        let mut cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        let mut data = match cache.clone() {
            Some(v) => v,
            None => self.load_encrypted()?.unwrap_or_default(),
        };

        let recovered_secret = data.recovery_pending_verified_secret.clone()
            .ok_or_else(|| BytescapeError::InvalidInput("Recovery not verified yet".to_string()))?;
        let recovered_did = data.recovery_pending_verified_did.clone()
            .ok_or_else(|| BytescapeError::InvalidInput("Missing recovered DID".to_string()))?;
        let recovered_public_key = data.recovery_pending_verified_public_key.clone()
            .ok_or_else(|| BytescapeError::InvalidInput("Missing recovered public key".to_string()))?;

        data.iroh_secret_key = Some(recovered_secret);
        data.local_identity_did = Some(recovered_did.clone());
        data.local_identity_public_key = Some(recovered_public_key.clone());
        data.recovery_last_recovered_at = Some(Self::now_unix());
        data.recovery_pending_challenge = None;
        data.recovery_pending_bundle = None;
        data.recovery_pending_verified_did = None;
        data.recovery_pending_verified_public_key = None;
        data.recovery_pending_verified_secret = None;

        self.save_encrypted(&data)?;
        *cache = Some(data);

        Ok(IdentityRecoveryFinalizeResult {
            restored: true,
            did: recovered_did,
            public_key: recovered_public_key,
        })
    }

    // ─── Username & Local Identity ───────────────────────────────────

    /// Get the user's Bytescape-local username (e.g., "alice").
    ///
    /// This is the username chosen during onboarding, used to derive the
    /// `did:web:bytescape.social:{username}` master identity.
    pub fn get_local_username(&self) -> Result<Option<String>, BytescapeError> {
        let mut cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        if let Some(ref data) = *cache {
            return Ok(data.local_identity_username.clone());
        }
        let data = self.load_encrypted()?.unwrap_or_default();
        let username = data.local_identity_username.clone();
        *cache = Some(data);
        Ok(username)
    }

    /// Set the user's Bytescape-local username.
    /// Also generates/updates the web_did (did:web:bytescape.social:{username}).
    pub fn set_local_username(&self, username: &str) -> Result<(), BytescapeError> {
        let mut cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        let mut data = cache.clone().unwrap_or_default();
        data.local_identity_username = Some(username.to_string());
        data.web_did = Some(format!("did:web:bytescape.social:{}", username));
        self.save_encrypted(&data)?;
        *cache = Some(data);
        println!("EncryptedKeychain: Saved local username '{}', web_did = did:web:bytescape.social:{}", username, username);
        Ok(())
    }

    // ─── Iroh P2P Key Management ────────────────────────────────────

    /// Save the Iroh Ed25519 secret key to the encrypted keychain.
    ///
    /// The key is hex-encoded before storage. This key is the user's P2P identity —
    /// it's used for all Iroh operations (document sync, blob transfer, gossip).
    ///
    /// # Side Effects
    /// * Writes to the encrypted keychain file
    pub fn save_iroh_secret(&self, secret_key: &iroh::SecretKey) -> Result<(), BytescapeError> {
        let secret_hex = hex::encode(secret_key.to_bytes());
        
        let mut cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        
        let mut data = cache.clone().unwrap_or_default();
        data.iroh_secret_key = Some(secret_hex);
        
        self.save_encrypted(&data)?;
        *cache = Some(data);
        
        println!("EncryptedKeychain: Saved Iroh secret");
        Ok(())
    }

    /// Load the Iroh Ed25519 secret key from the encrypted keychain.
    ///
    /// Hex-decodes the stored string and constructs an `iroh::SecretKey`.
    /// Returns `None` if no key has been saved yet (P2P not set up).
    pub fn load_iroh_secret(&self) -> Result<Option<iroh::SecretKey>, BytescapeError> {
        let mut cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        
        // Check cache first
        if let Some(ref data) = *cache {
            if let Some(ref hex_str) = data.iroh_secret_key {
                let bytes = hex::decode(hex_str)
                    .map_err(|e| BytescapeError::KeychainLoad(e.to_string()))?;
                let key = iroh::SecretKey::try_from(bytes.as_slice())
                    .map_err(|e| BytescapeError::KeychainLoad(e.to_string()))?;
                return Ok(Some(key));
            }
        }
        
        // Load from disk
        if let Some(data) = self.load_encrypted()? {
            if let Some(ref hex_str) = data.iroh_secret_key {
                let bytes = hex::decode(hex_str)
                    .map_err(|e| BytescapeError::KeychainLoad(e.to_string()))?;
                let key = iroh::SecretKey::try_from(bytes.as_slice())
                    .map_err(|e| BytescapeError::KeychainLoad(e.to_string()))?;
                *cache = Some(data);
                println!("EncryptedKeychain: Loaded Iroh secret");
                return Ok(Some(key));
            }
            *cache = Some(data);
        }
        
        println!("EncryptedKeychain: No Iroh secret found");
        Ok(None)
    }

    // ─── Bluesky Session Token Management ──────────────────────────────
    // Store, retrieve, and delete AT Protocol session tokens.
    // For OAuth sessions, the JWT fields hold the sentinel "oauth_session"
    // and real API calls go through OAuthManager instead.

    /// Save Bluesky session tokens (DID, handle, access JWT, refresh JWT).
    ///
    /// For app-password auth, stores real JWT tokens. For OAuth, stores the
    /// sentinel value `"oauth_session"` — actual tokens are managed by
    /// `OAuthManager` in `oauth.rs`.
    ///
    /// Also auto-generates the `master_did` (did:key) and `web_did` if possible.
    pub fn save_session(
        &self,
        did: String,
        handle: String,
        access_jwt: String,
        refresh_jwt: String,
    ) -> Result<(), BytescapeError> {
        let mut cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        
        let mut data = cache.clone().unwrap_or_default();
        data.did = Some(did);
        data.handle = Some(handle);
        data.access_jwt = Some(access_jwt);
        data.refresh_jwt = Some(refresh_jwt);
        data.last_refresh = Some(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs());
        
        self.save_encrypted(&data)?;
        *cache = Some(data);
        
        println!("EncryptedKeychain: Saved session");
        Ok(())
    }

    /// Delete session (Bluesky tokens only). DEPRECATED — use delete_all_identity_data for full wipe.
    pub fn delete_session(&self) -> Result<(), BytescapeError> {
        let mut cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        
        // Keep Iroh secret, clear session data
        if let Some(ref mut data) = *cache {
            data.did = None;
            data.handle = None;
            data.access_jwt = None;
            data.refresh_jwt = None;
            data.last_refresh = None;
            self.save_encrypted(data)?;
        }
        
        println!("EncryptedKeychain: Deleted session");
        Ok(())
    }

    /// **Nuclear option**: Delete ALL identity and session data.
    ///
    /// Preserves only the `iroh_secret_key` (the underlying node keypair).
    /// Everything else — DIDs, usernames, linked accounts, sessions, recovery
    /// data — is wiped. This is what "Delete Account" should call.
    pub fn delete_all_identity_data(&self) -> Result<(), BytescapeError> {
        let mut cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;

        // Preserve iroh_secret_key so the node can still start,
        // but wipe literally everything else.
        let iroh_key = cache.as_ref().and_then(|d| d.iroh_secret_key.clone());

        let fresh = AuthData {
            iroh_secret_key: iroh_key,
            ..Default::default()
        };
        self.save_encrypted(&fresh)?;
        *cache = Some(fresh);

        println!("EncryptedKeychain: Deleted ALL identity data (full account wipe)");
        Ok(())
    }

    /// Load the current session as a (DID, handle) tuple.
    ///
    /// Returns `None` if no session has been saved (user not logged in).
    /// The DID and handle are always available regardless of auth mode.
    pub fn load_session(&self) -> Result<Option<(String, String)>, BytescapeError> {
        let mut cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        
        // Check cache first
        if let Some(ref data) = *cache {
            if let (Some(ref did), Some(ref handle)) = (&data.did, &data.handle) {
                return Ok(Some((did.clone(), handle.clone())));
            }
        }
        
        // Load from disk
        if let Some(data) = self.load_encrypted()? {
            let result = if let (Some(ref did), Some(ref handle)) = (&data.did, &data.handle) {
                Some((did.clone(), handle.clone()))
            } else {
                None
            };
            *cache = Some(data);
            return Ok(result);
        }
        
        Ok(None)
    }

    /// Get the current Bluesky access JWT.
    ///
    /// For app-password sessions, this is a real JWT used as a Bearer token.
    /// For OAuth sessions, this returns the sentinel `"oauth_session"` —
    /// callers should check `is_oauth_session()` before using this value.
    pub fn get_access_jwt(&self) -> Result<Option<String>, BytescapeError> {
        let cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        
        if let Some(ref data) = *cache {
            return Ok(data.access_jwt.clone());
        }
        
        Ok(None)
    }

    /// Get the current Bluesky refresh JWT.
    ///
    /// Used by `get_fresh_access_jwt()` in `lib.rs` to refresh expired access tokens.
    /// For OAuth sessions, this returns the sentinel value (unused).
    pub fn get_refresh_jwt(&self) -> Result<Option<String>, BytescapeError> {
        let cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        
        if let Some(ref data) = *cache {
            return Ok(data.refresh_jwt.clone());
        }
        
        Ok(None)
    }

    /// Get the Bluesky session DID (did:plc:... from AT Protocol login).
    ///
    /// This is specifically the DID from Bluesky authentication, not the
    /// master identity. For the best available DID across all identity types,
    /// use `get_owner_did()` instead.
    pub fn get_did(&self) -> Result<Option<String>, BytescapeError> {
        let cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        
        if let Some(ref data) = *cache {
            return Ok(data.did.clone());
        }
        
        Ok(None)
    }

    // ─── Identity Migration ──────────────────────────────────────────
    // Data format migrations that fix legacy identifier formats.
    // Run once at startup (see lib.rs setup closure).

    /// Migrate fabricated `did:plc:` identifiers in `LinkedAccount` entries
    /// to platform-native identifiers.
    ///
    /// Before Phase 4.5, `generate_platform_plc()` created fake `did:plc:`
    /// strings for non-Bluesky platforms. These are not real PLC DIDs and
    /// cannot be resolved at `plc.directory`. This migration replaces them:
    ///
    /// - **Mastodon**: `did:plc:{hash}` → `acct:user@instance` (from `platform_identifier`)
    /// - **Bluesky**: Left unchanged (real PLC DIDs)
    /// - **Other AP**: Uses `platform_identifier` if available
    ///
    /// Returns the number of accounts migrated. Safe to call multiple times
    /// (idempotent — already-correct entries are skipped).
    pub fn migrate_fabricated_plc_identifiers(&self) -> Result<usize, BytescapeError> {
        let mut cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;

        let mut data = match cache.clone() {
            Some(d) => d,
            None => {
                drop(cache);
                match self.load_encrypted()? {
                    Some(d) => {
                        cache = self.cache.lock()
                            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
                        d
                    }
                    None => return Ok(0),
                }
            }
        };

        let mut migrated = 0;
        for account in &mut data.linked_accounts {
            // Skip Bluesky accounts — their did:plc is real
            if account.protocol == "bluesky" {
                continue;
            }

            // Skip accounts that don't have fabricated did:plc identifiers
            if !account.identifier.starts_with("did:plc:") {
                continue;
            }

            // Migrate to platform-native identifier
            if let Some(ref native_id) = account.platform_identifier {
                eprintln!(
                    "[identity-migration] {} account: {} → {}",
                    account.protocol, account.identifier, native_id
                );
                account.identifier = native_id.clone();
                migrated += 1;
            } else {
                // No platform_identifier available — cannot auto-migrate
                eprintln!(
                    "[identity-migration] WARNING: {} account has fabricated did:plc '{}' \
                     but no platform_identifier — manual fix needed",
                    account.protocol, account.identifier
                );
            }
        }

        if migrated > 0 {
            self.save_encrypted(&data)?;
            *cache = Some(data);
            eprintln!("[identity-migration] migrated {} linked account(s)", migrated);
        }

        Ok(migrated)
    }

    // ─── DID Resolution & Identity Queries ─────────────────────────────
    // These methods answer "who am I?" in different contexts. The user may
    // have 5+ different identifiers, so choosing the right one matters.

    /// Get the owner DID — 3-level fallback hierarchy:
    ///
    ///   1. `did:web:bytescape.social:{username}` — master identity (set when username is chosen)
    ///   2. `did:plc:...` — real Bluesky DID from PLC directory (NOT fabricated)
    ///   3. `did:key:z6Mk...` — P2P/offline identity (from Iroh Ed25519 keypair)
    ///
    /// Non-DID identifiers (acct: URIs, actor URLs) are stored in `LinkedAccount`
    /// but are NOT returned by this function — they are not DIDs.
    pub fn get_owner_did(&self) -> Result<Option<String>, BytescapeError> {
        let mut cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;

        if let Some(ref data) = *cache {
            // Level 1: did:web (Bytescape username identity)
            if let Some(ref did) = data.web_did {
                return Ok(Some(did.clone()));
            }
            // Level 2: did:plc (real Bluesky DID only — from OAuth session)
            if let Some(ref did) = data.did {
                if did.starts_with("did:plc:") {
                    return Ok(Some(did.clone()));
                }
            }
            // Level 3: did:key (P2P / offline)
            if let Some(ref did) = data.master_did {
                return Ok(Some(did.clone()));
            }
            if let Some(ref did) = data.local_identity_did {
                return Ok(Some(did.clone()));
            }
            return Ok(None);
        }

        // Load from disk
        if let Some(data) = self.load_encrypted()? {
            let result = data.web_did.clone()
                .or_else(|| data.did.as_ref()
                    .filter(|d| d.starts_with("did:plc:"))
                    .cloned())
                .or_else(|| data.master_did.clone())
                .or_else(|| data.local_identity_did.clone());
            *cache = Some(data);
            return Ok(result);
        }

        Ok(None)
    }

    /// Collect ALL known DIDs for the current user.
    ///
    /// Messages/items may have been stamped with *any* of these DIDs at
    /// different points in the identity lifecycle (before username was set,
    /// before Mastodon was linked, etc.). Use this set for inclusive
    /// owner-matching instead of `get_owner_did()` which returns only the
    /// current highest-priority DID.
    pub fn all_owner_dids(&self) -> Result<Vec<String>, BytescapeError> {
        let cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;

        let data = match &*cache {
            Some(d) => d.clone(),
            None => {
                drop(cache);
                match self.load_encrypted()? {
                    Some(d) => {
                        let mut c = self.cache.lock()
                            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
                        *c = Some(d.clone());
                        d
                    }
                    None => return Ok(Vec::new()),
                }
            }
        };

        let mut dids: Vec<String> = Vec::new();
        if let Some(ref d) = data.web_did { dids.push(d.clone()); }
        if let Some(ref d) = data.did { dids.push(d.clone()); }
        for la in &data.linked_accounts {
            if !dids.contains(&la.identifier) {
                dids.push(la.identifier.clone());
            }
            // Also include the platform_identifier (acct: URI) for Mastodon DMs
            if let Some(ref pi) = la.platform_identifier {
                if !dids.contains(pi) {
                    dids.push(pi.clone());
                }
            }
        }
        if let Some(ref d) = data.master_did {
            if !dids.contains(d) { dids.push(d.clone()); }
        }
        if let Some(ref d) = data.local_identity_did {
            if !dids.contains(d) { dids.push(d.clone()); }
        }
        Ok(dids)
    }

    /// Get the Bluesky DID specifically (for AT Protocol API calls).
    /// This returns the did:plc:... used for Bluesky authentication.
    pub fn get_bluesky_did(&self) -> Result<Option<String>, BytescapeError> {
        let cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        if let Some(ref data) = *cache {
            return Ok(data.did.clone());
        }
        Ok(None)
    }

    /// Get the master DID (did:key:...) if already computed.
    pub fn get_master_did(&self) -> Result<Option<String>, BytescapeError> {
        let cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        if let Some(ref data) = *cache {
            return Ok(data.master_did.clone());
        }
        Ok(None)
    }

    /// Get the Bluesky handle (e.g., "alice.bsky.social") if available.
    pub fn get_handle(&self) -> Result<Option<String>, BytescapeError> {
        let cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        if let Some(ref data) = *cache {
            return Ok(data.handle.clone());
        }
        Ok(None)
    }

    /// Get the best display name for the user.
    /// Priority: local_username > Bluesky handle > "Anonymous"
    pub fn get_display_name(&self) -> String {
        if let Ok(Some(username)) = self.get_local_username() {
            return username;
        }
        if let Ok(Some(handle)) = self.get_handle() {
            return handle;
        }
        "Anonymous".to_string()
    }

    /// Check if session needs refresh (tokens older than 1 hour)
    pub fn session_needs_refresh(&self) -> Result<bool, BytescapeError> {
        let cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        
        if let Some(ref data) = *cache {
            if let Some(last_refresh) = data.last_refresh {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                // Refresh if older than 1 hour
                return Ok(now - last_refresh > 3600);
            }
        }
        
        Ok(true) // No refresh timestamp, needs refresh
    }

    // ─── Mastodon Session Management ────────────────────────────────
    // Mastodon auth state is stored as an opaque JSON blob (application
    // credentials + access token). Managed by connectors/mastodon.rs.

    /// Save a serialized Mastodon session JSON blob.
    ///
    /// The blob contains the instance URL, client ID/secret, and access token.
    /// It's treated as opaque here — `connectors/mastodon.rs` handles parsing.
    pub fn set_mastodon_session(&self, json: &str) -> Result<(), BytescapeError> {
        let mut cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        let mut data = cache.clone().unwrap_or_default();
        data.mastodon_session_json = Some(json.to_string());
        self.save_encrypted(&data)?;
        *cache = Some(data);
        Ok(())
    }

    /// Retrieve the stored Mastodon session JSON blob, if any.
    pub fn get_mastodon_session(&self) -> Result<Option<String>, BytescapeError> {
        let cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        if let Some(ref data) = *cache {
            return Ok(data.mastodon_session_json.clone());
        }
        // Attempt load from disk
        drop(cache);
        if let Some(data) = self.load_encrypted()? {
            let result = data.mastodon_session_json.clone();
            let mut cache = self.cache.lock()
                .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
            *cache = Some(data);
            return Ok(result);
        }
        Ok(None)
    }

    /// Clear the stored Mastodon session.
    pub fn clear_mastodon_session(&self) -> Result<(), BytescapeError> {
        let mut cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        if let Some(ref mut data) = *cache {
            data.mastodon_session_json = None;
            self.save_encrypted(data)?;
        }
        Ok(())
    }

    // ─── Linked Account Management ─────────────────────────────────
    // CRUD operations for platform accounts (Bluesky, Mastodon, etc.)
    // that are linked to the user's master identity.

    /// Add or update a linked account. If an account with the same protocol
    /// already exists, it is updated in place (upsert); otherwise appended.
    ///
    /// Only one account per protocol is supported (e.g., one Bluesky, one Mastodon).
    pub fn add_linked_account(&self, account: LinkedAccount) -> Result<(), BytescapeError> {
        let mut cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        let mut data = cache.clone().unwrap_or_default();
        // Upsert: replace existing entry for same protocol (one account per protocol).
        // Also matches legacy entries that used a different identifier format.
        if let Some(pos) = data.linked_accounts.iter().position(|a|
            a.protocol == account.protocol
        ) {
            data.linked_accounts[pos] = account;
        } else {
            data.linked_accounts.push(account);
        }
        self.save_encrypted(&data)?;
        *cache = Some(data);
        Ok(())
    }

    /// Remove a linked account by protocol and identifier.
    /// Also matches by platform_identifier for backward compatibility with old identifier formats.
    pub fn remove_linked_account(&self, protocol: &str, identifier: &str) -> Result<(), BytescapeError> {
        let mut cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        let mut data = cache.clone().unwrap_or_default();
        data.linked_accounts.retain(|a| {
            if a.protocol != protocol { return true; }
            // Match by canonical identifier OR platform_identifier
            let matches_id = a.identifier == identifier;
            let matches_platform = a.platform_identifier.as_deref() == Some(identifier);
            !(matches_id || matches_platform)
        });
        self.save_encrypted(&data)?;
        *cache = Some(data);
        Ok(())
    }

    /// List all linked accounts.
    pub fn list_linked_accounts(&self) -> Result<Vec<LinkedAccount>, BytescapeError> {
        let cache = self.cache.lock()
            .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
        if let Some(ref data) = *cache {
            return Ok(data.linked_accounts.clone());
        }
        drop(cache);
        if let Some(data) = self.load_encrypted()? {
            let accounts = data.linked_accounts.clone();
            let mut cache = self.cache.lock()
                .map_err(|e| BytescapeError::Keychain(format!("Lock poisoned: {}", e)))?;
            *cache = Some(data);
            return Ok(accounts);
        }
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_encrypted_keychain_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keychain.json");
        let keychain = EncryptedKeychain::new(path, "test_password".to_string());
        
        // Save data
        let mut data = AuthData::default();
        data.did = Some("did:test".to_string());
        data.handle = Some("test.bsky.social".to_string());
        data.access_jwt = Some("access_token_123".to_string());
        data.iroh_secret_key = Some("secret_key_456".to_string());
        
        keychain.save_encrypted(&data).unwrap();
        
        // Load data
        let loaded = keychain.load_encrypted().unwrap().unwrap();
        
        assert_eq!(loaded.did, Some("did:test".to_string()));
        assert_eq!(loaded.handle, Some("test.bsky.social".to_string()));
        assert_eq!(loaded.access_jwt, Some("access_token_123".to_string()));
        assert_eq!(loaded.iroh_secret_key, Some("secret_key_456".to_string()));
    }

    #[test]
    fn test_wrong_password_fails() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keychain.json");
        
        // Create with one password
        let keychain1 = EncryptedKeychain::new(path.clone(), "password1".to_string());
        let data = AuthData::default();
        keychain1.save_encrypted(&data).unwrap();
        
        // Try to load with different password
        let keychain2 = EncryptedKeychain::new(path, "password2".to_string());
        let result = keychain2.load_encrypted();
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Decryption failed"));
    }

    fn make_linked(protocol: &str, identifier: &str, platform_id: Option<&str>) -> LinkedAccount {
        LinkedAccount {
            protocol: protocol.to_string(),
            identifier: identifier.to_string(),
            platform_identifier: platform_id.map(|s| s.to_string()),
            display_handle: format!("@test_{}", protocol),
            display_name: None,
            avatar_url: None,
            connected_at: 0,
            active: true,
        }
    }

    #[test]
    fn test_migration_mastodon_fabricated_plc_to_acct() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keychain.json");
        let keychain = EncryptedKeychain::new(path, "password".to_string());

        // Simulate a keychain with a fabricated did:plc for Mastodon
        let mut data = AuthData::default();
        data.linked_accounts.push(make_linked(
            "mastodon",
            "did:plc:abc123fabricated",
            Some("acct:alice@mastodon.social"),
        ));
        keychain.save_encrypted(&data).unwrap();

        // Run migration
        let migrated = keychain.migrate_fabricated_plc_identifiers().unwrap();
        assert_eq!(migrated, 1);

        // Verify the identifier was updated
        let accounts = keychain.list_linked_accounts().unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].identifier, "acct:alice@mastodon.social");
        assert_eq!(accounts[0].platform_identifier.as_deref(), Some("acct:alice@mastodon.social"));
    }

    #[test]
    fn test_migration_bluesky_real_plc_unchanged() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keychain.json");
        let keychain = EncryptedKeychain::new(path, "password".to_string());

        // Simulate a keychain with a real Bluesky did:plc
        let mut data = AuthData::default();
        data.linked_accounts.push(make_linked(
            "bluesky",
            "did:plc:realblueskyplc",
            None,
        ));
        keychain.save_encrypted(&data).unwrap();

        // Run migration — should not touch Bluesky entries
        let migrated = keychain.migrate_fabricated_plc_identifiers().unwrap();
        assert_eq!(migrated, 0);

        let accounts = keychain.list_linked_accounts().unwrap();
        assert_eq!(accounts[0].identifier, "did:plc:realblueskyplc");
    }

    #[test]
    fn test_migration_already_correct_acct_unchanged() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keychain.json");
        let keychain = EncryptedKeychain::new(path, "password".to_string());

        // Simulate a keychain with an already-migrated Mastodon account
        let mut data = AuthData::default();
        data.linked_accounts.push(make_linked(
            "mastodon",
            "acct:bob@fosstodon.org",
            Some("acct:bob@fosstodon.org"),
        ));
        keychain.save_encrypted(&data).unwrap();

        // Run migration — already correct, should skip
        let migrated = keychain.migrate_fabricated_plc_identifiers().unwrap();
        assert_eq!(migrated, 0);

        let accounts = keychain.list_linked_accounts().unwrap();
        assert_eq!(accounts[0].identifier, "acct:bob@fosstodon.org");
    }

    #[test]
    fn test_migration_idempotent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keychain.json");
        let keychain = EncryptedKeychain::new(path, "password".to_string());

        let mut data = AuthData::default();
        data.linked_accounts.push(make_linked(
            "mastodon",
            "did:plc:xyz789fabricated",
            Some("acct:carol@hachyderm.io"),
        ));
        keychain.save_encrypted(&data).unwrap();

        // Run migration twice
        let migrated1 = keychain.migrate_fabricated_plc_identifiers().unwrap();
        assert_eq!(migrated1, 1);
        let migrated2 = keychain.migrate_fabricated_plc_identifiers().unwrap();
        assert_eq!(migrated2, 0); // second run is a no-op
    }

    #[test]
    fn test_migration_mixed_accounts() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keychain.json");
        let keychain = EncryptedKeychain::new(path, "password".to_string());

        let mut data = AuthData::default();
        // Bluesky with real did:plc — should NOT be migrated
        data.linked_accounts.push(make_linked(
            "bluesky",
            "did:plc:realbluesky123",
            None,
        ));
        // Mastodon with fabricated did:plc — should be migrated
        data.linked_accounts.push(make_linked(
            "mastodon",
            "did:plc:fakemasto456",
            Some("acct:dave@infosec.exchange"),
        ));
        keychain.save_encrypted(&data).unwrap();

        let migrated = keychain.migrate_fabricated_plc_identifiers().unwrap();
        assert_eq!(migrated, 1);

        let accounts = keychain.list_linked_accounts().unwrap();
        assert_eq!(accounts[0].identifier, "did:plc:realbluesky123"); // unchanged
        assert_eq!(accounts[1].identifier, "acct:dave@infosec.exchange"); // migrated
    }

    #[test]
    fn test_get_owner_did_hierarchy() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keychain.json");
        let keychain = EncryptedKeychain::new(path, "password".to_string());

        // Level 3: did:key only
        let mut data = AuthData::default();
        data.master_did = Some("did:key:z6MkTest".to_string());
        keychain.save_encrypted(&data).unwrap();
        assert_eq!(keychain.get_owner_did().unwrap(), Some("did:key:z6MkTest".to_string()));

        // Level 2: real Bluesky did:plc takes priority over did:key
        data.did = Some("did:plc:realbluesky".to_string());
        keychain.save_encrypted(&data).unwrap();
        // Clear cache so it reloads
        *keychain.cache.lock().unwrap() = None;
        assert_eq!(keychain.get_owner_did().unwrap(), Some("did:plc:realbluesky".to_string()));

        // Level 1: did:web takes top priority
        data.web_did = Some("did:web:bytescape.social:alice".to_string());
        keychain.save_encrypted(&data).unwrap();
        *keychain.cache.lock().unwrap() = None;
        assert_eq!(keychain.get_owner_did().unwrap(), Some("did:web:bytescape.social:alice".to_string()));
    }

    #[test]
    fn test_get_owner_did_ignores_non_plc_session_did() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("keychain.json");
        let keychain = EncryptedKeychain::new(path, "password".to_string());

        // If data.did is NOT a did:plc, it should be skipped
        let mut data = AuthData::default();
        data.did = Some("did:web:someone.example".to_string()); // not a plc
        data.master_did = Some("did:key:z6MkFallback".to_string());
        keychain.save_encrypted(&data).unwrap();

        assert_eq!(keychain.get_owner_did().unwrap(), Some("did:key:z6MkFallback".to_string()));
    }
}
