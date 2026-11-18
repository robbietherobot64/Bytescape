//! did:plc genesis-operation client — mints a per-user `did:plc` DID via the
//! public PLC directory (spec: https://web.plc.directory/spec/v0.1/did-plc).
//!
//! Replaces the old `did:web:{handle}.{domain}` minting: a `did:web` DID can
//! never resolve on the real AT Protocol network (bridges/relays require
//! `did:plc`, see bluesky-social/atproto discussion #4379), so every new
//! account now gets a self-certifying `did:plc` derived from a signed genesis
//! operation. Existing `did:web` rows in `pds_accounts` are re-identified to
//! `did:plc` by the 091 runner (`pds::reidentify`, `REIDENTIFY_DID_WEB`) and
//! the old identity is tombstoned (its well-known routes answer an honest 404)
//! — they are NOT kept loading/serving (the pre-091 comment here claimed they
//! "keep loading/serving untouched"; corrected 2026-11-14).

use base64::Engine as _;
use indexmap::IndexMap;
use secp256k1::{ecdsa, Message, PublicKey, Secp256k1};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// `did:key`-encode a secp256k1 public key as the full `did:key:z…` method-
/// prefixed identifier — the form PLC operations carry (verbatim fixture:
/// `"rotationKeys":["did:key:zQ3shd…"]`) and the form this module's own
/// [`decode_did_key_pubkey`] accepts. The multibase body is `z` + base58btc of
/// the multicodec-prefixed compressed key (`0xe7 0x01` + 33 bytes) — the same
/// body `crate::pds::account::encode_public_key_multibase` stores; kept local
/// so this module stays self-contained without an `account.rs` dependency.
fn did_key_from_pubkey(pubkey: &PublicKey) -> String {
    let serialized = pubkey.serialize();
    let mut multicodec = Vec::with_capacity(2 + serialized.len());
    multicodec.push(0xe7);
    multicodec.push(0x01);
    multicodec.extend_from_slice(&serialized);
    format!("did:key:z{}", bs58::encode(&multicodec).into_string())
}

/// Decode a `did:key` (multibase `z` + base58btc, secp256k1 multicodec prefix
/// `0xe7 0x01`) back to the raw 33-byte compressed public key. Used by the
/// fixture signature round-trip test and `derive_did`'s callers; not part of
/// the minting hot path.
fn decode_did_key_pubkey(did_key: &str) -> Result<PublicKey, String> {
    let hex_str = did_key
        .strip_prefix("did:key:")
        .ok_or_else(|| format!("not a did:key: {did_key}"))?;
    let z_str = hex_str
        .strip_prefix('z')
        .ok_or_else(|| format!("did:key must be multibase base58btc (z prefix): {did_key}"))?;
    let decoded = bs58::decode(z_str)
        .into_vec()
        .map_err(|e| format!("base58 decode failed: {e}"))?;
    if decoded.len() != 35 || decoded[0] != 0xe7 || decoded[1] != 0x01 {
        return Err(format!("unexpected multicodec prefix or length: {did_key}"));
    }
    PublicKey::from_slice(&decoded[2..]).map_err(|e| format!("invalid pubkey bytes: {e}"))
}

/// DAG-CBOR-encode a `serde_json::Value` with canonical map-key ordering
/// (RFC 7049 canonical: length-first, then bytewise). `serde_ipld_dagcbor`'s
/// `to_vec` already sorts top-level map entries, but its nested-map sorting is
/// known-buggy (see `vendor-rsky-min/rsky-common/src/sign.rs`); the IndexMap
/// round-trip below forces deterministic key order at every nesting level so
/// signing and DID derivation are byte-stable across runs.
fn dag_cbor_canonical(value: &Value) -> Result<Vec<u8>, String> {
    let json_str = serde_json::to_string(value).map_err(|e| format!("json serialize: {e}"))?;
    let ordered: IndexMap<String, Value> =
        serde_json::from_str(&json_str).map_err(|e| format!("indexmap roundtrip: {e}"))?;
    serde_ipld_dagcbor::to_vec(&ordered).map_err(|e| format!("dag-cbor encode: {e}"))
}

/// Signable form of a genesis/update op per the did:plc spec: the operation
/// object with the `sig` field **removed entirely** (not set to `null`), then
/// DAG-CBOR-encoded. Returns `(unsigned_json, unsigned_bytes)` — the caller
/// signs `unsigned_bytes` and re-attaches `sig` to a copy of the full object.
fn signable_form(op: &Value) -> Result<(Value, Vec<u8>), String> {
    let mut unsigned = op.clone();
    if let Some(obj) = unsigned.as_object_mut() {
        obj.remove("sig");
    } else {
        return Err("operation is not a JSON object".into());
    }
    let bytes = dag_cbor_canonical(&unsigned)?;
    Ok((unsigned, bytes))
}

/// Derive the `did:plc:{base32(sha256(full_signed_op_dagcbor))[:24]}` DID from
/// a fully-signed genesis operation (including its `sig` field). Pure and
/// deterministic — matches the spec's "DID Creation" derivation exactly
/// (raw DAG-CBOR bytes hashed directly with SHA-256, no multihash wrapper,
/// lowercase base32, no padding, first 24 chars).
pub fn derive_did(op: &Value) -> Result<String, String> {
    let bytes = dag_cbor_canonical(op)?;
    let digest = Sha256::digest(&bytes);
    let b32 = base32_nopad_lower(&digest);
    if b32.len() < 24 {
        return Err("base32 digest shorter than 24 chars".into());
    }
    Ok(format!("did:plc:{}", &b32[..24]))
}

/// Lowercase base32 (RFC 4648 alphabet) without padding.
fn base32_nopad_lower(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::with_capacity((bytes.len() * 8 + 4) / 5);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for &b in bytes {
        acc = (acc << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((acc >> bits) & 0x1f) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((acc << (5 - bits)) & 0x1f) as usize] as char);
    }
    out
}

/// Build a signed `plc_operation` genesis object for `handle` (the RESOLVABLE
/// AT handle, `at://`-stripped `{user}.{domain}` — a bare username is not a
/// handle, and a wrong `alsoKnownAs` burns an irreversible public artifact on
/// a dead identity), served by the PDS at `pds_endpoint`, using `signing` as
/// the account's own secp256k1 key
/// (also the sole `rotationKeys` entry, matching the briefed outline). The
/// returned `Value` is complete (`type`/`prev`/`rotationKeys`/
/// `verificationMethods`/`alsoKnownAs`/`services`/`sig`) and JSON-ready for
/// POST submission to the PLC directory.
pub fn build_genesis_op(
    signing: &secp256k1::Keypair,
    handle: &str,
    pds_endpoint: &str,
) -> Result<Value, String> {
    let did_key = did_key_from_pubkey(&signing.public_key());
    let unsigned = json!({
        "type": "plc_operation",
        "rotationKeys": [did_key.clone()],
        "verificationMethods": { "atproto": did_key.clone() },
        "alsoKnownAs": [format!("at://{handle}")],
        "services": {
            "atproto_pds": {
                "type": "AtprotoPersonalDataServer",
                "endpoint": pds_endpoint,
            }
        },
        "prev": Value::Null,
    });

    let (_unsigned, signable_bytes) = signable_form(&unsigned)?;
    let digest = Sha256::digest(&signable_bytes);
    let message = Message::from_digest_slice(&digest)
        .map_err(|e| format!("message from digest: {e}"))?;
    let mut sig: ecdsa::Signature = signing.secret_key().sign_ecdsa(message);
    sig.normalize_s();
    let compact = sig.serialize_compact();
    let sig_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(compact);

    let mut signed = unsigned.clone();
    signed
        .as_object_mut()
        .ok_or_else(|| "unsigned op is not an object".to_string())?
        .insert("sig".to_string(), Value::String(sig_b64));
    Ok(signed)
}

/// Mint a new `did:plc`: build+sign a genesis operation, derive its DID, and
/// POST the op to `{plc_base}/{did}` — the directory's create route
/// (`POST /:did`, "update or create"; the bare base 404s "Cannot POST /"). The
/// directory re-derives the DID from the op and rejects a mismatch, so our
/// derivation is cross-checked live. Returns the derived DID string on a 2xx
/// response; any non-2xx (or transport error) is an `Err` with status+body —
/// never a silent fallback, since new mints must not fall back to `did:web`.
pub async fn create_did(
    http: &reqwest::Client,
    plc_base: &str,
    signing: &secp256k1::Keypair,
    handle: &str,
    pds_endpoint: &str,
) -> Result<String, String> {
    let op = build_genesis_op(signing, handle, pds_endpoint)?;
    let did = derive_did(&op)?;

    let url = format!("{}/{did}", plc_base.trim_end_matches('/'));
    let resp = http
        .post(&url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::USER_AGENT, "Bytescape/1.0")
        .body(op.to_string())
        .send()
        .await
        .map_err(|e| format!("PLC directory request failed: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!(
            "PLC directory rejected operation: HTTP {status} body: {body}"
        ));
    }
    Ok(did)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hard-coded real genesis operation from ap.brid.gy's DID, fetched from
    /// `https://plc.directory/did:plc:xbifsywyv5pka5jlknhv5yv3/log/audit`
    /// (entry[0].operation, verbatim JSON incl. real `sig`). Never rewritten —
    /// this is the byte-exact proof that encoding+hash+truncation match the
    /// real public did:plc directory.
    const FIXTURE_OP: &str = r#"{"sig":"kXUCiWoVTeH3D_r2Nhj-R4W8lX8slSNlAgbzN1545PI4WeZAgka-SCWgxatbESvKfh3Tcoq9LvsszacRpigunA","prev":null,"type":"plc_operation","services":{"atproto_pds":{"type":"AtprotoPersonalDataServer","endpoint":"https://atproto.brid.gy/"}},"alsoKnownAs":["at://ap.brid.gy"],"rotationKeys":["did:key:zQ3shdKQXsbiqDxWVdUDJ4ShjHVo1LpMeJhq5mQx6pEY4azJ1"],"verificationMethods":{"atproto":"did:key:zQ3shdkj5yRHCUESnzAnk7brbjTb33HvQaaLgeBq6rNegdkAZ"}}"#;
    const FIXTURE_DID: &str = "did:plc:xbifsywyv5pka5jlknhv5yv3";

    /// The key used to build the golden `build_genesis_op` output below. Kept
    /// fixed so the pinned DID is stable across test runs.
    fn fixed_test_keypair() -> secp256k1::Keypair {
        let secp = Secp256k1::new();
        let secret_bytes = [0x11u8; 32];
        let secret_key = secp256k1::SecretKey::from_slice(&secret_bytes)
            .expect("fixed test secret key");
        secp256k1::Keypair::from_secret_key(&secp, &secret_key)
    }

    #[test]
    fn derive_did_reproduces_real_public_fixture() {
        let op: Value = serde_json::from_str(FIXTURE_OP).expect("fixture parses");
        let did = derive_did(&op).expect("derive_did");
        assert_eq!(did, FIXTURE_DID);
    }

    #[test]
    fn build_genesis_op_shape_and_pinned_did() {
        let keypair = fixed_test_keypair();
        let op = build_genesis_op(&keypair, "testuser", "https://pds.example.com")
            .expect("build_genesis_op");

        assert_eq!(op["type"], json!("plc_operation"));
        assert_eq!(op["prev"], Value::Null);

        let expected_did_key = did_key_from_pubkey(&keypair.public_key());
        // The full method-prefixed `did:key:z…` form — the bare multibase is
        // not a did:key (the directory rejects it) — and it round-trips through
        // this module's own decoder (build → decode self-consistency).
        assert!(expected_did_key.starts_with("did:key:z"), "{expected_did_key}");
        decode_did_key_pubkey(&expected_did_key).expect("build output decodes");
        assert_eq!(op["rotationKeys"], json!([expected_did_key]));
        assert_eq!(op["verificationMethods"]["atproto"], json!(expected_did_key));
        assert_eq!(op["alsoKnownAs"], json!(["at://testuser"]));
        assert_eq!(
            op["services"]["atproto_pds"],
            json!({"type": "AtprotoPersonalDataServer", "endpoint": "https://pds.example.com"})
        );
        assert!(op["sig"].is_string(), "op must carry a base64url signature");

        let did = derive_did(&op).expect("derive_did");
        // Golden value — computed once by running this exact path with the
        // fixed key above, then pinned. Changing it means the signing or
        // encoding path changed and needs re-verification, not silent update.
        // Re-pinned 2026-11-14 with the did:key method-prefix fix (the old pin
        // derived from wrong bare-multibase key content). Re-verified, not
        // silently updated: `derive_did_reproduces_real_public_fixture` still
        // reproduces a REAL directory DID byte-exact (derivation path unchanged
        // and grounded on real bytes) and `fixture_signature_verifies…` pins the
        // encoder to the directory's own `did:key` string byte-for-byte — only
        // the key string inside the built op changed, so this DID necessarily
        // changed with it.
        assert_eq!(did, "did:plc:2r6omgho6wlo2ioss4pwrquj");
    }

    #[test]
    fn fixture_signature_verifies_over_spec_signable_form() {
        let op: Value = serde_json::from_str(FIXTURE_OP).expect("fixture parses");
        let rotation_key = op["rotationKeys"][0]
            .as_str()
            .expect("rotationKeys[0] is a string");
        let pubkey = decode_did_key_pubkey(rotation_key).expect("decode rotation key");

        let sig_str = op["sig"].as_str().expect("sig is a string");
        let sig_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(sig_str)
            .expect("base64url sig decodes");
        assert_eq!(sig_bytes.len(), 64, "compact sig is 64 bytes");
        let sig = ecdsa::Signature::from_compact(&sig_bytes).expect("valid compact sig");

        // Encode/decode round-trip against the directory's OWN bytes: our
        // encoder must reproduce a real `did:key` string byte-for-byte — the
        // decisive grounding for the method-prefix form.
        assert_eq!(
            did_key_from_pubkey(&pubkey),
            rotation_key,
            "encoder must reproduce the real directory did:key form"
        );

        let (_unsigned, signable_bytes) = signable_form(&op).expect("signable form");
        let digest = Sha256::digest(&signable_bytes);
        let message = Message::from_digest_slice(&digest).expect("message from digest");

        Secp256k1::verification_only()
            .verify_ecdsa(&message, &sig, &pubkey)
            .expect("fixture sig must verify over the spec's signable form");
    }
}
