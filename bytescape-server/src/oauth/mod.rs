#![allow(dead_code)]

pub mod bluesky;
pub mod mastodon;

use base64::Engine;
use ed25519_dalek::Signer;
use rand::RngCore;
use serde_json::json;
use sha2::{Digest, Sha256};

/// Generate a DPoP proof JWT for an Ed25519 key pair.
/// Header: `{"typ":"dpop+jwt","alg":"EdDSA","jwk":{...}}`
/// Payload: `{"jti":"<uuid>","htm":"<method>","htu":"<url>","iat":<ts>,"ath":"<hash>","nonce":"<nonce>"}`
pub fn create_dpop_proof(
    signing_key: &ed25519_dalek::SigningKey,
    method: &str,
    url: &str,
    ath: Option<&str>,
    nonce: Option<&str>,
) -> Result<String, String> {
    let verifying_key = signing_key.verifying_key();
    let pub_bytes = verifying_key.to_bytes();

    // JWK for Ed25519
    let jwk = json!({
        "kty": "OKP",
        "crv": "Ed25519",
        "x": base64_url_encode(&pub_bytes),
    });

    let header = json!({
        "typ": "dpop+jwt",
        "alg": "EdDSA",
        "jwk": jwk,
    });

    let mut payload = serde_json::Map::new();
    let jti = uuid::Uuid::new_v4().to_string();
    let iat = chrono::Utc::now().timestamp();

    payload.insert("jti".into(), json!(jti));
    payload.insert("htm".into(), json!(method));
    payload.insert("htu".into(), json!(url));
    payload.insert("iat".into(), json!(iat));

    if let Some(ath_val) = ath {
        payload.insert("ath".into(), json!(ath_val));
    }

    if let Some(nonce_val) = nonce {
        payload.insert("nonce".into(), json!(nonce_val));
    }

    let header_b64 = base64_url_encode(&serde_json::to_vec(&header).map_err(|e| e.to_string())?);
    let payload_b64 =
        base64_url_encode(&serde_json::to_vec(&payload).map_err(|e| e.to_string())?);

    let message = format!("{}.{}", header_b64, payload_b64);
    let signature = signing_key.sign(message.as_bytes());
    let sig_b64 = base64_url_encode(&signature.to_bytes());

    Ok(format!("{}.{}.{}", header_b64, payload_b64, sig_b64))
}

/// Compute the `ath` (access token hash) for DPoP binding.
/// SHA-256 of the access token, base64url encoded.
pub fn dpop_ath(access_token: &str) -> String {
    let hash = Sha256::digest(access_token.as_bytes());
    base64_url_encode(&hash)
}

/// Generate a fresh Ed25519 key pair for DPoP.
pub fn generate_dpop_keypair() -> (ed25519_dalek::SigningKey, ed25519_dalek::VerifyingKey) {
    let mut seed = [0u8; 32];
    rand::rng().fill_bytes(&mut seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
    let verifying_key = signing_key.verifying_key();
    (signing_key, verifying_key)
}

/// Base64URL-encode (no padding).
pub fn base64_url_encode(data: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

/// Send an HTTP request with DPoP proof, automatically retrying on `use_dpop_nonce`.
/// If the server responds 400 with `error: "use_dpop_nonce"` and a `dpop-nonce` header,
/// rebuilds the DPoP proof including the nonce and retries once.
///
/// Note: the build closure may be called twice (retry on dpop-nonce), so
/// callers must ensure RequestBuilder parameters survive cloning (use body.clone()).
pub async fn send_dpop_request<F>(
    dpop_sk: &ed25519_dalek::SigningKey,
    method: &str,
    url: &str,
    ath: Option<&str>,
    build: F,
) -> Result<reqwest::Response, String>
where
    F: Fn(&str) -> reqwest::RequestBuilder + Send,
{
    let dpop = create_dpop_proof(dpop_sk, method, url, ath, None)?;
    let resp = build(&dpop)
        .send()
        .await
        .map_err(|e| format!("DPoP request failed: {}", e))?;

    let status = resp.status();
    let nonce = resp
        .headers()
        .get("dpop-nonce")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if status == reqwest::StatusCode::BAD_REQUEST && nonce.is_some() {
        let body = resp.text().await.map_err(|e| e.to_string())?;
        let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;

        if json["error"].as_str() == Some("use_dpop_nonce") {
            let Some(nonce_val) = nonce else { unreachable!("guarded by nonce.is_some() above") };
            let dpop = create_dpop_proof(dpop_sk, method, url, ath, Some(&nonce_val))?;
            return build(&dpop)
                .send()
                .await
                .map_err(|e| format!("DPoP request failed: {}", e));
        }

        return Err(format!("DPoP request failed ({}): {}", status, body));
    }

    Ok(resp)
}

/// Base64URL-decode.
pub fn base64_url_decode(data: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(data)
        .map_err(|e| e.to_string())
}
