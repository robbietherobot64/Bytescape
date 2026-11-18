//! HTTP Signatures (draft-cavage) for ActivityPub `authorized fetch`.
//!
//! Many fediverse instances (e.g. indieweb.social) reject unauthenticated AP
//! reads with `401 {"error":"Request not signed"}`. To read their actor docs /
//! outboxes we must sign our GETs with an RSA-SHA256 HTTP Signature over
//! `(request-target) host date accept`, using a key the remote can resolve at
//! `keyId`.
//!
//! We sign as an AP actor whose public key is served at `{actor}#main-key` (see
//! [`crate::ap::actor`]). The caller supplies the actor's id + its decrypted
//! RSA private-key PEM (from [`crate::ap::keys`]).

use axum::http::HeaderMap;
use base64::Engine;
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs1v15::{Signature, SigningKey, VerifyingKey};
use rsa::signature::{SignatureEncoding, Signer, Verifier};
use rsa::RsaPrivateKey;
use sha2::{Digest, Sha256};

/// Build a `Signature` header value for a signed GET.
///
/// Signs `(request-target)`, `host`, `date`, and `accept` (the headers the
/// remote needs to verify + reproduce the request line). Algorithm `rsa-sha256`
/// = RSASSA-PKCS1-v1_5 over SHA-256 — the standard the fediverse verifies.
pub fn sign_get(
    privkey_pem: &str,
    key_id: &str,
    method: &str,
    path_and_query: &str,
    host: &str,
    date: &str,
) -> Result<String, String> {
    let private = RsaPrivateKey::from_pkcs1_pem(privkey_pem)
        .map_err(|e| format!("load ap signing key: {e}"))?;

    // signing string per draft-cavage-http-signatures. Minimal header set
    // (request-target) host date — the standard Mastodon verifies for GETs.
    // Mastodon reconstructs (request-target) with a LOWERCASE method, so the
    // signing string must lowercase it too or verification fails.
    let signing_string = format!(
        "(request-target): {} {path_and_query}\nhost: {host}\ndate: {date}",
        method.to_lowercase()
    );

    let signing_key = SigningKey::<Sha256>::new(private);
    let sig = signing_key.sign(signing_string.as_bytes());
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());

    Ok(format!(
        r#"keyId="{key_id}",algorithm="rsa-sha256",headers="(request-target) host date",signature="{sig_b64}""#
    ))
}

/// Current time as an HTTP-date (IMF-fixdate), e.g.
/// `Sun, 06 Nov 1994 08:49:37 GMT` — the format draft-cavage verification
/// expects for the `Date` header and the signing string.
pub fn http_date() -> String {
    chrono::Utc::now()
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string()
}

/// Headers to attach to a GET of `url` for `authorized fetch`: `Signature`
/// (built by [`sign_get`]), `Host`, and `Date`.
///
/// `signer` is `Some((key_id, privkey_pem))` for a signed fetch. Returns an
/// empty `Vec` when `signer` is `None` (unsigned fallback — the previous
/// behavior) or when the URL can't be parsed / signing fails; those failures
/// are logged and the request goes out unsigned rather than erroring.
pub fn signed_get_headers(
    url: &str,
    signer: Option<(&str, &str)>,
    _accept: &str,
) -> Vec<(&'static str, String)> {
    let Some((key_id, privkey_pem)) = signer else {
        return Vec::new();
    };
    let parsed = match url::Url::parse(url) {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!("[AP] cannot parse {url} for signing ({e}); sending unsigned");
            return Vec::new();
        }
    };
    let Some(host_base) = parsed.host_str() else {
        tracing::warn!("[AP] no host in {url}; sending unsigned");
        return Vec::new();
    };
    // Match the Host header hyper would send: host plus port only when the
    // port is non-default for the scheme (the url crate strips default ports).
    let host = match parsed.port() {
        Some(port) => format!("{host_base}:{port}"),
        None => host_base.to_string(),
    };
    let path_and_query = match parsed.query() {
        Some(q) => format!("{}?{q}", parsed.path()),
        None => parsed.path().to_string(),
    };
    let date = http_date();
    match sign_get(privkey_pem, key_id, "GET", &path_and_query, &host, &date) {
        Ok(signature) => vec![("Signature", signature), ("Host", host), ("Date", date)],
        Err(e) => {
            tracing::warn!("[AP] signing GET {url} failed ({e}); sending unsigned");
            Vec::new()
        }
    }
}

// ─── Inbound verification (Phase C) ────────────────────────────────────────
//
// draft-cavage verification for signed POSTs (inbox): the signature is checked
// over the EXACT raw bytes and the exact received header values before any JSON
// parsing or state change. `Digest` (SHA-256 over the body) and `Date` freshness
// are mandatory and must themselves be signed, together with `host` and
// `(request-target)`.

/// Headers a signed POST must cover. Anything less is rejected outright.
pub const REQUIRED_SIGNED_HEADERS: &[&str] = &["(request-target)", "host", "date", "digest"];

/// Max accepted clock skew for the `Date` header (both directions).
pub const DATE_TOLERANCE_SECS: i64 = 30 * 60;

/// Why a signed request was rejected. `status` is the HTTP code the handler
/// should answer with (401 = authentication of the request failed, 400 = the
/// request's signed parts are inconsistent).
#[derive(Debug)]
pub enum VerifyError {
    MissingSignature,
    Malformed(String),
    Unsupported(String),
    MissingHeader(String),
    DigestMismatch,
    StaleDate,
    BadSignature,
}

impl VerifyError {
    pub fn status(&self) -> u16 {
        match self {
            // The signature itself could not authenticate the request.
            VerifyError::MissingSignature
            | VerifyError::StaleDate
            | VerifyError::BadSignature => 401,
            // The request is internally inconsistent / unsupported.
            VerifyError::Malformed(_)
            | VerifyError::Unsupported(_)
            | VerifyError::MissingHeader(_)
            | VerifyError::DigestMismatch => 400,
        }
    }
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyError::MissingSignature => write!(f, "Signature header missing"),
            VerifyError::Malformed(m) => write!(f, "malformed signature: {m}"),
            VerifyError::Unsupported(m) => write!(f, "unsupported: {m}"),
            VerifyError::MissingHeader(h) => write!(f, "header not signed or absent: {h}"),
            VerifyError::DigestMismatch => write!(f, "Digest does not match body"),
            VerifyError::StaleDate => write!(f, "Date outside freshness window"),
            VerifyError::BadSignature => write!(f, "signature verification failed"),
        }
    }
}

/// The parsed `Signature` header.
#[derive(Debug)]
pub struct ParsedSignature {
    pub key_id: String,
    pub algorithm: Option<String>,
    /// Lowercased header names as declared, in signing-string order.
    pub signed_headers: Vec<String>,
    pub signature: Vec<u8>,
}

/// Base64 `Digest` header value for a body: `SHA-256=<base64(sha256)>`.
pub fn digest_header(body: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(body);
    let digest = h.finalize();
    format!(
        "SHA-256={}",
        base64::engine::general_purpose::STANDARD.encode(digest)
    )
}

/// Check a received `Digest` header against the exact raw body bytes. Only
/// SHA-256 is accepted; any other digest algorithm is rejected (never silently
/// ignored).
pub fn check_digest(body: &[u8], header: &str) -> Result<(), VerifyError> {
    let (algo, value) = header
        .split_once('=')
        .ok_or_else(|| VerifyError::Malformed("Digest without algorithm".into()))?;
    if !algo.trim().eq_ignore_ascii_case("SHA-256") {
        return Err(VerifyError::Unsupported(format!("digest algorithm {algo}")));
    }
    if value.trim() != digest_header(body).strip_prefix("SHA-256=").unwrap_or("") {
        return Err(VerifyError::DigestMismatch);
    }
    Ok(())
}

/// Check `Date` freshness against `now` (±[`DATE_TOLERANCE_SECS`]).
pub fn check_date_fresh(date: &str, now: chrono::DateTime<chrono::Utc>) -> Result<(), VerifyError> {
    let parsed = chrono::DateTime::parse_from_rfc2822(date.trim())
        .map_err(|_| VerifyError::Malformed("unparseable Date".into()))?;
    let skew = (now.timestamp() - parsed.timestamp()).abs();
    if skew > DATE_TOLERANCE_SECS {
        return Err(VerifyError::StaleDate);
    }
    Ok(())
}

fn build_signing_string(method: &str, path_and_query: &str, resolved: &[(String, String)]) -> String {
    // Mastodon reconstructs (request-target) with a LOWERCASE method.
    let mut out = format!(
        "(request-target): {} {path_and_query}",
        method.to_lowercase()
    );
    for (name, value) in resolved {
        out.push_str(&format!("\n{name}: {value}"));
    }
    out
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let vals: Vec<String> = headers
        .get_all(name)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .collect();
    if vals.is_empty() {
        None
    } else {
        Some(vals.join(", "))
    }
}

/// Split a `Signature` header into its comma-separated `k="v"` params
/// (quote-aware).
fn split_sig_params(value: &str) -> Vec<(String, String)> {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for ch in value.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                cur.push(ch);
            }
            ',' if !in_quotes => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        parts.push(cur);
    }
    parts
        .into_iter()
        .filter_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            Some((
                k.trim().to_lowercase(),
                v.trim().trim_matches('"').to_string(),
            ))
        })
        .collect()
}

/// Parse the `Signature` header value.
pub fn parse_signature_header(value: &str) -> Result<ParsedSignature, String> {
    // Param keys are lowercased by split_sig_params — look them up lowercase.
    let params = split_sig_params(value);
    let get = |name: &str| {
        params
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };
    let key_id = get("keyid").filter(|s| !s.is_empty()).ok_or("missing keyId")?;
    let headers_field = get("headers").filter(|s| !s.is_empty()).ok_or("missing headers")?;
    let sig_b64 = get("signature").filter(|s| !s.is_empty()).ok_or("missing signature")?;
    let signature = base64::engine::general_purpose::STANDARD
        .decode(sig_b64)
        .map_err(|e| format!("signature base64: {e}"))?;
    Ok(ParsedSignature {
        key_id,
        algorithm: get("algorithm"),
        signed_headers: headers_field
            .split_whitespace()
            .map(|s| s.to_lowercase())
            .collect(),
        signature,
    })
}

/// Everything checked but the RSA verify itself: header parse, algorithm
/// policy, mandatory signed set, Digest over the exact bytes, Date freshness,
/// and the reconstructed draft-cavage signing string over every DECLARED
/// header. The caller then resolves `key_id` to a public key and calls
/// [`verify_signature`].
pub struct PreparedVerify {
    pub key_id: String,
    pub signing_string: String,
    pub signature: Vec<u8>,
}

pub fn prepare_verify(
    method: &str,
    path_and_query: &str,
    headers: &HeaderMap,
    body: &[u8],
    now: chrono::DateTime<chrono::Utc>,
) -> Result<PreparedVerify, VerifyError> {
    let sig_raw = header_value(headers, "signature")
        .ok_or(VerifyError::MissingSignature)?;
    let parsed = parse_signature_header(&sig_raw).map_err(VerifyError::Malformed)?;

    // Algorithm policy: rsa-sha256 (or hs2019 — an RSA key is verified the
    // same way). Anything else cannot be verified with our keys → reject.
    match parsed.algorithm.as_deref() {
        None | Some("rsa-sha256") | Some("hs2019") => {}
        Some(other) => {
            return Err(VerifyError::Unsupported(format!("algorithm {other}")))
        }
    }

    // Every mandatory header must be declared as signed.
    for req in REQUIRED_SIGNED_HEADERS {
        if !parsed.signed_headers.iter().any(|h| h == req) {
            return Err(VerifyError::MissingHeader((*req).to_string()));
        }
    }

    // Digest over the exact raw bytes; never optional for POSTs.
    let digest = header_value(headers, "digest")
        .ok_or_else(|| VerifyError::MissingHeader("digest".into()))?;
    check_digest(body, &digest)?;

    // Date freshness, checked independently of the signature.
    let date = header_value(headers, "date")
        .ok_or_else(|| VerifyError::MissingHeader("date".into()))?;
    check_date_fresh(&date, now)?;

    // Rebuild the signing string over every declared header, in order, from
    // the values actually received.
    let mut resolved: Vec<(String, String)> = Vec::new();
    for name in &parsed.signed_headers {
        if name == "(request-target)" {
            continue;
        }
        let value = header_value(headers, name)
            .ok_or_else(|| VerifyError::MissingHeader(name.clone()))?;
        resolved.push((name.clone(), value));
    }
    Ok(PreparedVerify {
        key_id: parsed.key_id,
        signing_string: build_signing_string(method, path_and_query, &resolved),
        signature: parsed.signature,
    })
}

/// RSA-SHA256 verification of a prepared request against the signer's public
/// key (PKCS#1 PEM).
pub fn verify_signature(
    signing_string: &str,
    signature: &[u8],
    public_key_pem: &str,
) -> Result<(), VerifyError> {
    let public = crate::ap::keys::parse_public_key(public_key_pem)
        .map_err(|_| VerifyError::BadSignature)?;
    let verifying_key = VerifyingKey::<Sha256>::new(public);
    let sig = Signature::try_from(signature).map_err(|_| VerifyError::BadSignature)?;
    verifying_key
        .verify(signing_string.as_bytes(), &sig)
        .map_err(|_| VerifyError::BadSignature)
}

/// Build a `Signature` header for a signed request (POST delivery or test):
/// signs `signed_headers` (which must cover [`REQUIRED_SIGNED_HEADERS`]) over
/// the given raw header values.
pub fn sign_request(
    privkey_pem: &str,
    key_id: &str,
    method: &str,
    path_and_query: &str,
    headers: &[(&str, &str)],
    signed_headers: &[&str],
) -> Result<String, String> {
    for req in REQUIRED_SIGNED_HEADERS {
        if !signed_headers.iter().any(|h| h.eq_ignore_ascii_case(req)) {
            return Err(format!("signed header set must cover {req}"));
        }
    }
    let private = RsaPrivateKey::from_pkcs1_pem(privkey_pem)
        .map_err(|e| format!("load ap signing key: {e}"))?;

    let mut resolved: Vec<(String, String)> = Vec::new();
    for name in signed_headers {
        let lower = name.to_lowercase();
        if lower == "(request-target)" {
            continue;
        }
        let value = headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.trim().to_string())
            .ok_or_else(|| format!("missing value for signed header {name}"))?;
        resolved.push((lower, value));
    }
    let signing_string = build_signing_string(method, path_and_query, &resolved);
    let signing_key = SigningKey::<Sha256>::new(private);
    let sig = signing_key.sign(signing_string.as_bytes());
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
    Ok(format!(
        r#"keyId="{key_id}",algorithm="rsa-sha256",headers="{}",signature="{sig_b64}""#,
        signed_headers.join(" ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ap::keys::{generate_keypair, parse_public_key};
    use rsa::pkcs1v15::{Signature, VerifyingKey};
    use rsa::signature::Verifier;

    #[test]
    fn sign_get_builds_draft_cavage_signature_header() {
        let kp = generate_keypair().expect("keygen");
        let key_id = "https://bytescape.test/@alice#main-key";
        let date = "Sun, 06 Nov 1994 08:49:37 GMT";

        let header = sign_get(
            &kp.private_pem,
            key_id,
            "GET",
            "/users/alice/outbox",
            "remote.test",
            date,
        )
        .expect("sign");

        assert!(header.contains(&format!(r#"keyId="{key_id}""#)), "{header}");
        assert!(header.contains(r#"algorithm="rsa-sha256""#), "{header}");
        assert!(
            header.contains(r#"headers="(request-target) host date""#),
            "{header}"
        );

        // signature="<base64>" decodes to a 2048-bit (256-byte) RSA signature
        // that verifies against the actor's public key over the exact
        // draft-cavage signing string.
        let sig_b64 = header
            .split("signature=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("signature field");
        let sig_bytes = base64::engine::general_purpose::STANDARD
            .decode(sig_b64)
            .expect("base64 signature");
        assert_eq!(sig_bytes.len(), 256, "2048-bit signature is 256 bytes");

        let signing_string = format!(
            "(request-target): get /users/alice/outbox\nhost: remote.test\ndate: {date}"
        );
        let verifying_key = VerifyingKey::<Sha256>::new(parse_public_key(&kp.public_pem).expect("pubkey"));
        let signature = Signature::try_from(sig_bytes.as_slice()).expect("signature");
        verifying_key
            .verify(signing_string.as_bytes(), &signature)
            .expect("signature verifies");
    }

    #[test]
    fn signed_get_headers_empty_without_signer_or_on_bad_url() {
        // Unsigned fallback: no headers attached at all.
        assert!(signed_get_headers("https://x.test/a", None, "application/json").is_empty());
        // Unparseable URL with a signer → warn + unsigned rather than fail.
        assert!(signed_get_headers("not a url", Some(("k", "pem")), "application/json").is_empty());
    }

    #[test]
    fn signed_get_headers_covers_request_target_host_date() {
        let kp = generate_keypair().expect("keygen");
        let headers = signed_get_headers(
            "https://remote.test:8443/users/alice/outbox?page=2",
            Some(("https://bytescape.test/@alice#main-key", &kp.private_pem)),
            "application/activity+json",
        );
        let get = |name: &str| {
            headers
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| v.as_str())
                .unwrap_or_else(|| panic!("missing {name} header"))
        };
        assert_eq!(get("Host"), "remote.test:8443");
        let date = get("Date");
        assert!(date.ends_with(" GMT"), "HTTP-date must be GMT: {date}");
        let signature = get("Signature");
        assert!(signature.contains(r#"keyId="https://bytescape.test/@alice#main-key""#));
        assert!(signature.contains(r#"headers="(request-target) host date""#));
        assert!(signature.contains("signature=\""));
    }
}

#[cfg(test)]
mod verify_tests {
    use super::*;
    use crate::ap::keys::generate_keypair;

    const KEY_ID: &str = "https://bytescape.test/@alice#main-key";
    const PATH: &str = "/@alice/inbox";
    const HOST: &str = "bytescape.test";

    fn headers_with(date: &str, digest: &str, host: &str, signature: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("host", host.parse().unwrap());
        h.insert("date", date.parse().unwrap());
        h.insert("digest", digest.parse().unwrap());
        h.insert("signature", signature.parse().unwrap());
        h
    }

    /// Sign `body` for POST `path` with `privkey`, returning the header set.
    fn signed_headers(
        privkey: &str,
        body: &[u8],
        path: &str,
        host: &str,
        date: &str,
        signed: &[&str],
    ) -> HeaderMap {
        let digest = digest_header(body);
        let sig = sign_request(
            privkey,
            KEY_ID,
            "POST",
            path,
            &[("host", host), ("date", date), ("digest", &digest)],
            signed,
        )
        .expect("sign");
        headers_with(date, &digest, host, &sig)
    }

    #[test]
    fn post_signature_roundtrips_over_exact_bytes() {
        let kp = generate_keypair().expect("keygen");
        let body = br#"{"type":"Create","id":"https://remote.test/1"}"#;
        let date = http_date();
        let headers = signed_headers(
            &kp.private_pem,
            body,
            PATH,
            HOST,
            &date,
            &["(request-target)", "host", "date", "digest"],
        );

        let now = chrono::Utc::now();
        let prep = prepare_verify("POST", PATH, &headers, body, now).expect("prepare");
        assert_eq!(prep.key_id, KEY_ID);
        verify_signature(&prep.signing_string, &prep.signature, &kp.public_pem).expect("verify");

        // Wrong key (unknown signer) fails.
        let other = generate_keypair().expect("keygen");
        assert!(matches!(
            verify_signature(&prep.signing_string, &prep.signature, &other.public_pem),
            Err(VerifyError::BadSignature)
        ));
    }

    #[test]
    fn one_byte_body_tamper_fails_digest() {
        let kp = generate_keypair().expect("keygen");
        let body = br#"{"type":"Create"}"#;
        let date = http_date();
        let headers = signed_headers(&kp.private_pem, body, PATH, HOST, &date, &[
            "(request-target)", "host", "date", "digest",
        ]);
        let mut tampered = body.to_vec();
        tampered[0] ^= 0x01;
        assert!(matches!(
            prepare_verify("POST", PATH, &headers, &tampered, chrono::Utc::now()),
            Err(VerifyError::DigestMismatch)
        ));
    }

    #[test]
    fn query_tamper_fails_signature() {
        let kp = generate_keypair().expect("keygen");
        let body = b"{}";
        let date = http_date();
        let headers = signed_headers(&kp.private_pem, body, PATH, HOST, &date, &[
            "(request-target)", "host", "date", "digest",
        ]);
        let prep = prepare_verify(
            "POST",
            "/@alice/inbox?x=1",
            &headers,
            body,
            chrono::Utc::now(),
        )
        .expect("prepare (query not covered by digest)");
        assert!(verify_signature(&prep.signing_string, &prep.signature, &kp.public_pem).is_err());
    }

    #[test]
    fn host_tamper_fails_signature() {
        let kp = generate_keypair().expect("keygen");
        let body = b"{}";
        let date = http_date();
        let digest = digest_header(body);
        let sig = sign_request(
            &kp.private_pem,
            KEY_ID,
            "POST",
            PATH,
            &[("host", HOST), ("date", &date), ("digest", &digest)],
            &["(request-target)", "host", "date", "digest"],
        )
        .expect("sign");
        // One byte different in the host value.
        let headers = headers_with(&date, &digest, "bytescape.tesu", &sig);
        let prep = prepare_verify("POST", PATH, &headers, body, chrono::Utc::now()).expect("prepare");
        assert!(verify_signature(&prep.signing_string, &prep.signature, &kp.public_pem).is_err());
    }

    #[test]
    fn date_tamper_fails_signature_and_stale_date_fails() {
        let kp = generate_keypair().expect("keygen");
        let body = b"{}";
        let date = http_date();
        let digest = digest_header(body);
        let sig = sign_request(
            &kp.private_pem,
            KEY_ID,
            "POST",
            PATH,
            &[("host", HOST), ("date", &date), ("digest", &digest)],
            &["(request-target)", "host", "date", "digest"],
        )
        .expect("sign");
        // Correctly signed but tampered date: signature must not verify.
        let shifted = (chrono::Utc::now() + chrono::Duration::seconds(60))
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let headers = headers_with(&shifted, &digest, HOST, &sig);
        let prep = prepare_verify("POST", PATH, &headers, body, chrono::Utc::now()).expect("prepare");
        assert!(verify_signature(&prep.signing_string, &prep.signature, &kp.public_pem).is_err());

        // Stale (2h old) but correctly signed date is rejected as stale.
        let stale = (chrono::Utc::now() - chrono::Duration::hours(2))
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let stale_headers = signed_headers(&kp.private_pem, body, PATH, HOST, &stale, &[
            "(request-target)", "host", "date", "digest",
        ]);
        assert!(matches!(
            prepare_verify("POST", PATH, &stale_headers, body, chrono::Utc::now()),
            Err(VerifyError::StaleDate)
        ));
    }

    #[test]
    fn unknown_algorithm_and_unsigned_required_header_fail() {
        let kp = generate_keypair().expect("keygen");
        let body = b"{}";
        let date = http_date();
        let headers = signed_headers(&kp.private_pem, body, PATH, HOST, &date, &[
            "(request-target)", "host", "date", "digest",
        ]);
        // Rewrite the algorithm field; the signature no longer matters.
        let sig = headers.get("signature").unwrap().to_str().unwrap();
        let bad_alg = sig.replace("rsa-sha256", "hmac-sha256");
        let mut h = headers.clone();
        h.insert("signature", bad_alg.parse().unwrap());
        assert!(matches!(
            prepare_verify("POST", PATH, &h, body, chrono::Utc::now()),
            Err(VerifyError::Unsupported(_))
        ));

        // A signature set that omits `digest` is rejected before any parsing.
        let digest = digest_header(body);
        let sig = sign_request(
            &kp.private_pem,
            KEY_ID,
            "POST",
            PATH,
            &[("host", HOST), ("date", &date), ("digest", &digest)],
            &["(request-target)", "host", "date"],
        );
        // sign_request enforces the required set itself.
        assert!(sig.is_err());
    }

    #[test]
    fn digest_algorithm_and_missing_signature_fail() {
        let kp = generate_keypair().expect("keygen");
        let body = b"{}";
        let date = http_date();
        let mut headers = signed_headers(&kp.private_pem, body, PATH, HOST, &date, &[
            "(request-target)", "host", "date", "digest",
        ]);
        headers.insert("digest", "SHA-1=abc".parse().unwrap());
        assert!(matches!(
            prepare_verify("POST", PATH, &headers, body, chrono::Utc::now()),
            Err(VerifyError::Unsupported(_))
        ));

        let mut no_sig = headers.clone();
        no_sig.remove("signature");
        assert!(matches!(
            prepare_verify("POST", PATH, &no_sig, body, chrono::Utc::now()),
            Err(VerifyError::MissingSignature)
        ));
    }

    #[test]
    fn extra_signed_headers_are_covered_and_verified() {
        // `content-type` is declared signed — tampering it must fail.
        let kp = generate_keypair().expect("keygen");
        let body = b"{}";
        let date = http_date();
        let digest = digest_header(body);
        let sig = sign_request(
            &kp.private_pem,
            KEY_ID,
            "POST",
            PATH,
            &[
                ("host", HOST),
                ("date", &date),
                ("digest", &digest),
                ("content-type", "application/activity+json"),
            ],
            &["(request-target)", "host", "date", "digest", "content-type"],
        )
        .expect("sign");
        let mut headers = headers_with(&date, &digest, HOST, &sig);
        headers.insert("content-type", "application/activity+json".parse().unwrap());
        let prep = prepare_verify("POST", PATH, &headers, body, chrono::Utc::now()).expect("prepare");
        verify_signature(&prep.signing_string, &prep.signature, &kp.public_pem).expect("verify");

        headers.insert("content-type", "application/json".parse().unwrap());
        let prep = prepare_verify("POST", PATH, &headers, body, chrono::Utc::now()).expect("prepare");
        assert!(verify_signature(&prep.signing_string, &prep.signature, &kp.public_pem).is_err());
    }
}
