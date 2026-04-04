//! Shared security utilities for URL validation and SSRF prevention.
//!
//! Provides a centralized `validate_public_url()` function used by:
//! - `activitypub/inbox.rs` — SEC-8: AP key fetch SSRF protection
//! - `commands/unfurl.rs` — SEC-12: link unfurl SSRF protection
//! - `connectors/bluesky.rs` — PDS URL validation (existing)
//!
//! ## Policy
//!
//! Rejects URLs that resolve to:
//! - Loopback addresses (127.0.0.0/8, ::1)
//! - Private networks (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16)
//! - Link-local addresses (169.254.0.0/16, fe80::/10)
//! - Non-HTTPS schemes (optional, configurable)
//! - Cloud metadata endpoints (169.254.169.254)

/// Validate that a URL points to a public internet host, not a private/loopback address.
///
/// Returns `Ok(())` if the URL is safe to fetch, `Err(reason)` if it targets
/// a private network, loopback, or link-local address.
///
/// # Arguments
/// * `url` — The URL string to validate
/// * `require_https` — If true, rejects non-HTTPS URLs
pub fn validate_public_url(url: &str, require_https: bool) -> Result<(), String> {
    let parsed = url::Url::parse(url)
        .map_err(|e| format!("Invalid URL '{}': {}", url, e))?;

    if require_https && parsed.scheme() != "https" {
        return Err(format!("URL must use HTTPS, got scheme '{}': {}", parsed.scheme(), url));
    }

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(format!("URL must use HTTP(S), got scheme '{}': {}", parsed.scheme(), url));
    }

    let host = parsed.host_str()
        .ok_or_else(|| format!("URL has no host: {}", url))?;

    // Block localhost and loopback
    if host == "localhost" || host.starts_with("127.") || host == "[::1]" || host == "::1" {
        return Err(format!("URL points to loopback address: {}", url));
    }

    // Check IPv4 private ranges
    if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
        if ip.is_loopback() {
            return Err(format!("URL points to loopback: {}", url));
        }
        if ip.is_private() {
            return Err(format!("URL points to private network: {}", url));
        }
        if ip.is_link_local() {
            return Err(format!("URL points to link-local address: {}", url));
        }
        // Block cloud metadata endpoint
        if ip == std::net::Ipv4Addr::new(169, 254, 169, 254) {
            return Err(format!("URL points to cloud metadata endpoint: {}", url));
        }
        if ip.is_broadcast() || ip.is_unspecified() {
            return Err(format!("URL points to invalid address: {}", url));
        }
    }

    // Check IPv6 private ranges
    if let Ok(ip) = host.trim_matches(|c| c == '[' || c == ']').parse::<std::net::Ipv6Addr>() {
        if ip.is_loopback() || ip.is_unspecified() {
            return Err(format!("URL points to loopback/unspecified IPv6: {}", url));
        }
        // fe80::/10 link-local
        let segments = ip.segments();
        if segments[0] & 0xffc0 == 0xfe80 {
            return Err(format!("URL points to IPv6 link-local address: {}", url));
        }
        // fc00::/7 unique local (private)
        if segments[0] & 0xfe00 == 0xfc00 {
            return Err(format!("URL points to IPv6 private network: {}", url));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_url_accepted() {
        assert!(validate_public_url("https://mastodon.social/@user", false).is_ok());
        assert!(validate_public_url("https://example.com/inbox", true).is_ok());
    }

    #[test]
    fn test_loopback_rejected() {
        assert!(validate_public_url("http://127.0.0.1:8080/admin", false).is_err());
        assert!(validate_public_url("http://localhost/admin", false).is_err());
        assert!(validate_public_url("http://[::1]/admin", false).is_err());
    }

    #[test]
    fn test_private_ip_rejected() {
        assert!(validate_public_url("http://10.0.0.1/secret", false).is_err());
        assert!(validate_public_url("http://172.16.0.1/secret", false).is_err());
        assert!(validate_public_url("http://192.168.1.1/secret", false).is_err());
    }

    #[test]
    fn test_link_local_rejected() {
        assert!(validate_public_url("http://169.254.169.254/latest/meta-data", false).is_err());
        assert!(validate_public_url("http://169.254.1.1/test", false).is_err());
    }

    #[test]
    fn test_https_required() {
        assert!(validate_public_url("http://example.com/page", true).is_err());
        assert!(validate_public_url("https://example.com/page", true).is_ok());
    }

    #[test]
    fn test_non_http_scheme_rejected() {
        assert!(validate_public_url("ftp://example.com/file", false).is_err());
        assert!(validate_public_url("file:///etc/passwd", false).is_err());
    }
}
