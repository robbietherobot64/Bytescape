# Bytescape Security Guide

This document outlines the security measures implemented in Bytescape to protect user data and credentials.

## 🔐 Encryption

### Keychain Encryption

All sensitive credentials are encrypted at rest using:

| Component | Algorithm | Purpose |
|-----------|-----------|---------|
| **Encryption** | AES-256-GCM | Authenticated encryption |
| **Key Derivation** | Argon2id | Memory-hard password hashing |
| **Salt** | 128-bit random | Unique per encryption |
| **Nonce** | 96-bit random | Unique per operation |

### Encrypted Data

The following data is encrypted in the keychain:
- Iroh node secret key (P2P identity)
- Bluesky DID (decentralized identifier)
- Bluesky handle
- Access JWT (short-lived API token)
- Refresh JWT (long-lived refresh token)

### Key Derivation Parameters

```rust
Argon2id (default parameters)
- Memory: 19 MB
- Iterations: 2
- Parallelism: 1
```

## 🛡️ Content Security Policy (CSP)

Bytescape enforces a strict CSP to prevent XSS and code injection attacks:

```
default-src 'self'           # Only load resources from app origin
script-src 'self'            # No inline scripts, no eval
style-src 'self' 'unsafe-inline'  # Inline styles needed for dynamic theming
img-src 'self' asset: https: data: blob:  # Allow app images and blobs
media-src 'self' asset: blob:  # Media from app and blob URLs
connect-src 'self' https: wss:  # API calls and WebSocket connections
font-src 'self'              # Only local fonts
frame-src 'none'             # No iframes
object-src 'none'            # No plugins
base-uri 'self'              # Prevent base tag hijacking
form-action 'self'           # Forms only submit to app
```

### CSP Restrictions

| Directive | Policy | Rationale |
|-----------|--------|-----------|
| `script-src` | `'self'` | Prevents XSS via injected scripts |
| `frame-src` | `'none'` | Prevents clickjacking |
| `object-src` | `'none'` | Prevents plugin-based attacks |
| `base-uri` | `'self'` | Prevents base tag hijacking |
| `form-action` | `'self'` | Prevents form data exfiltration |

## 🔑 Token Management

### JWT Token Lifecycle

| Token | Lifetime | Storage | Refresh |
|-------|----------|---------|---------|
| Access JWT | ~1 hour | Encrypted keychain | On expiration |
| Refresh JWT | ~90 days | Encrypted keychain | On use |

### Token Refresh Strategy

```rust
// Check if tokens need refresh (older than 1 hour)
if keychain.session_needs_refresh()? {
    let new_session = auth_manager.refresh_session(old_refresh_jwt).await?;
    keychain.save_session(new_session)?;
}
```

### Security Best Practices

1. **Never log tokens** - Tokens are never written to console or logs
2. **Secure deletion** - Session deletion overwrites with new encrypted data
3. **Memory protection** - Tokens cleared from memory when no longer needed

## 📁 File System Security

### Asset Protocol Scope

```json
{
  "assetProtocol": {
    "enable": true,
    "scope": ["$APPDATA/**", "**"]
  }
}
```

Files are only accessible within:
- `$APPDATA` - Application data directory
- Explicit user-selected paths (via file picker)

### Media Blob Cache

Media blobs fetched from Iroh are cached in:
```
$APPDATA/blob_cache/{hash}.{ext}
```

Access is restricted to:
- Application process only
- OS file permissions (user-only access)

## 🔒 P2P Security

### Iroh Node Security

- **Secret Key**: Generated once, stored encrypted in keychain
- **ALPN**: Custom protocol `bytescape/1.0` prevents cross-protocol attacks
- **Connection Encryption**: TLS 1.3 via QUIC
- **Peer Verification**: Public key-based identity

### Sync Document Capabilities

| Capability | Access Level | Use Case |
|------------|--------------|----------|
| `Write` | Full read/write | Document owner |
| `Read` | Read-only | Shared with peers |

## 🚨 Security Incident Response

### If Credentials Are Compromised

1. **Revoke Bluesky session**
   - Go to bsky.app/settings
   - Revoke all app passwords
   - Create new app password

2. **Rotate Iroh identity**
   - Delete `$APPDATA/keychain.json`
   - Restart Bytescape (generates new identity)

3. **Clear P2P connections**
   - Disconnect from all peers
   - Create new sync documents

### Reporting Security Issues

Report security vulnerabilities to:
- GitHub Security Advisories (private)
- Do not disclose publicly until patched

## 🔍 Security Audit Checklist

### Development
- [ ] No secrets in source code
- [ ] No secrets in logs
- [ ] CSP headers enabled
- [ ] Dependencies up to date

### Deployment
- [ ] Code signing enabled
- [ ] Secure update mechanism
- [ ] Minimal permissions

### Runtime
- [ ] Encrypted keychain in use
- [ ] Token refresh working
- [ ] P2P connections encrypted

## 📚 References

- [OWASP Secure Coding Practices](https://owasp.org/www-project-secure-coding-practices-quick-reference-guide/)
- [Tauri Security Guidelines](https://v2.tauri.app/concept/security/)
- [AES-GCM Specification](https://csrc.nist.gov/publications/detail/sp/800-38d/final)
- [Argon2 Specification](https://datatracker.ietf.org/doc/html/rfc9106)
