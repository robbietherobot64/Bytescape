//! HTTP Signatures for ActivityPub federation (draft-cavage-http-signatures-12).

use base64::Engine;
use chrono::Utc;

const SIGNED_HEADERS_POST: &[&str] = &["(request-target)", "host", "date", "digest", "content-type"];
const SIGNED_HEADERS_GET: &[&str] = &["(request-target)", "host", "date", "accept"];

#[derive(Debug, Clone)]
pub struct HttpSignature {
    pub key_id: String,
    pub algorithm: String,
    pub headers: Vec<String>,
    pub signature: Vec<u8>,
}

/// Sign an outbound HTTP POST request for inbox delivery.
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

/// Parse an HTTP Signature header into its components.
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

/// Verify an ed25519 signature against a public key PEM and signing string.
pub fn verify_ed25519_signature(
    pem: &str,
    signing_string: &[u8],
    signature: &[u8],
) -> Result<bool, String> {
    let pubkey_bytes = pem_to_ed25519_pubkey(pem)?;
    verify_ed25519_raw(&pubkey_bytes, signing_string, signature)
}

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

fn sign_ed25519(secret_key: &iroh::SecretKey, data: &[u8]) -> Vec<u8> {
    let sig = secret_key.sign(data);
    sig.to_bytes().to_vec()
}

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
pub fn compute_sha256_digest(data: &[u8]) -> String {
    let hash = sha256(data);
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

// Minimal SHA-256 implementation to avoid adding a new dependency.
fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize()
}

struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffer_len: usize,
    total_len: u64,
}

const SHA256_K: [u32; 64] = [
    0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
    0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
    0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
    0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
    0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
    0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
    0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
    0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2,
];

impl Sha256 {
    fn new() -> Self {
        Sha256 {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
            ],
            buffer: [0u8; 64],
            buffer_len: 0,
            total_len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.total_len += data.len() as u64;
        let mut offset = 0;
        if self.buffer_len > 0 {
            let space = 64 - self.buffer_len;
            let copy_len = data.len().min(space);
            self.buffer[self.buffer_len..self.buffer_len + copy_len]
                .copy_from_slice(&data[..copy_len]);
            self.buffer_len += copy_len;
            offset = copy_len;
            if self.buffer_len == 64 {
                let block = self.buffer;
                self.process_block(&block);
                self.buffer_len = 0;
            }
        }
        while offset + 64 <= data.len() {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[offset..offset + 64]);
            self.process_block(&block);
            offset += 64;
        }
        if offset < data.len() {
            let remaining = data.len() - offset;
            self.buffer[..remaining].copy_from_slice(&data[offset..]);
            self.buffer_len = remaining;
        }
    }

    fn finalize(mut self) -> [u8; 32] {
        let bit_len = self.total_len * 8;
        let mut padding = vec![0x80u8];
        let pad_len = if self.buffer_len < 56 {
            55 - self.buffer_len
        } else {
            119 - self.buffer_len
        };
        padding.extend(vec![0u8; pad_len]);
        padding.extend_from_slice(&bit_len.to_be_bytes());
        self.update(&padding);

        let mut result = [0u8; 32];
        for (i, &word) in self.state.iter().enumerate() {
            result[i * 4..(i + 1) * 4].copy_from_slice(&word.to_be_bytes());
        }
        result
    }

    fn process_block(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i-15].rotate_right(7) ^ w[i-15].rotate_right(18) ^ (w[i-15] >> 3);
            let s1 = w[i-2].rotate_right(17) ^ w[i-2].rotate_right(19) ^ (w[i-2] >> 10);
            w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(SHA256_K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g; g = f; f = e;
            e = d.wrapping_add(temp1);
            d = c; c = b; b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_known_vector() {
        let hash = sha256(b"hello");
        let hex_str = hex::encode(hash);
        assert_eq!(hex_str, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
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
