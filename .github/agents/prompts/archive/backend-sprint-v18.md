# Backend Sprint v18 — Identity Resolver, DID Document Service, Publish API

**Sprint**: v18 (Go-Live: Hosted Identity)  
**Track**: A (Backend — Rust)  
**Date**: 2026-04-08  
**Previous sprint**: v17 (cross-platform repost + server-side filtering — delivered)  
**HEAD at prompt time**: `a12201a`  
**Decision**: Single-user launch first (fast go-live, harden later)

---

## Sprint Context

This sprint transitions Bytescape from a local-only desktop app to a **publicly resolvable identity**. When complete, `did:web:bytescape.social:{username}` will resolve to a real DID document, `acct:{username}@bytescape.social` will resolve via WebFinger, and `https://bytescape.social/@{username}` will serve a public profile.

### What already exists (reuse, don't rewrite)

| Component | File | Status |
|-----------|------|--------|
| Ed25519 keypair gen + storage | `keychain_encrypted.rs:56-72` | ✅ Working |
| `did:key` derivation (multicodec) | `keychain_encrypted.rs:356-370` | ✅ Working |
| `did:web` string formatting | `keychain_encrypted.rs:412`, `auth.rs:423` | ✅ String only — no document |
| WebFinger JRD generation | `activitypub/webfinger.rs:55-75` | ✅ Working (`generate_webfinger()`) |
| WebFinger resolution (remote) | `activitypub/webfinger.rs:83-118` | ✅ Working |
| AP Actor creation | `activitypub/actor.rs:109-148` | ✅ Working (`ApActor::new()`) |
| AP Actor → JSON-LD | `activitypub/actor.rs:152-190` | ✅ Working (`to_activitystreams()`) |
| AP HTTP Signatures | `activitypub/signatures.rs` | ✅ Working |
| `AP_DOMAIN` constant | `activitypub/actor.rs:52` | ✅ `"bytescape.social"` |
| Provisioning stubs | `commands/relay.rs:33-165` | ⚠️ All return errors — must replace |
| Tauri command registration | `lib.rs:500-510` | ✅ Stubs registered |

### What doesn't exist (must build)

- DID Document (`did.json`) generation from keychain material
- Server-side identity resolver (axum service serving `.well-known/*` endpoints)
- Publish API (POST endpoints for the Tauri app to push DID + profile to server)
- AT Protocol handle resolution (`/.well-known/atproto-did`)

---

## P0: New Server Crate — `bytescape-server`

### Architecture Decision

Create a **new Rust crate** at `bytescape/server/` (sibling to `src-tauri/`). This is a standalone `axum` HTTP server, NOT embedded in the Tauri app. Rationale:

1. The Tauri app runs on the user's desktop — it can't serve public HTTP
2. The server runs on `bytescape.social` — always-on, publicly reachable
3. They share identity data via the Publish API (app → server)
4. They share code via the existing `activitypub/` module (extract to shared lib later, but for v18 just copy the types)

### Crate Structure

```
bytescape/server/
├── Cargo.toml
├── src/
│   ├── main.rs              # axum server entrypoint
│   ├── config.rs             # env-based config (domain, port, data dir, auth secret)
│   ├── identity.rs           # DID document generation + storage
│   ├── webfinger.rs          # /.well-known/webfinger handler
│   ├── atproto_did.rs        # /.well-known/atproto-did handler
│   ├── profile.rs            # /@{username} profile page handler
│   ├── publish.rs            # POST /api/publish/* handlers (auth required)
│   ├── auth.rs               # Bearer token validation for publish API
│   └── storage.rs            # File-based storage for DID docs + profiles
```

### Dependencies

```toml
[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tower-http = { version = "0.6", features = ["cors", "trace"] }
tracing = "0.1"
tracing-subscriber = "0.3"
```

---

## P1: Identity Resolver Endpoints

### `GET /.well-known/webfinger?resource=acct:{username}@bytescape.social`

Return a JRD response. Reuse the same structure as `activitypub/webfinger.rs:WebFingerResponse`:

```json
{
  "subject": "acct:alice@bytescape.social",
  "aliases": ["https://bytescape.social/@alice"],
  "links": [
    {
      "rel": "self",
      "type": "application/activity+json",
      "href": "https://bytescape.social/@alice"
    },
    {
      "rel": "http://webfinger.net/rel/profile-page",
      "type": "text/html",
      "href": "https://bytescape.social/@alice"
    }
  ]
}
```

**Validation**: Only respond for usernames that have published a DID document. Return 404 for unknown users.

### `GET /.well-known/atproto-did`

Return the user's AT Protocol DID (plain text). This is handle-to-DID resolution for the AT Protocol.

For single-user launch, return the sole registered user's `did:web`. For multi-user, this endpoint needs to be handle-aware (via `Host:` header subdomain or query param).

```
did:web:bytescape.social:alice
```

### `GET /{username}/did.json`

**This is the critical endpoint.** The `did:web` spec requires the DID document to be served at this exact path. For `did:web:bytescape.social:alice`, resolvers fetch `https://bytescape.social/alice/did.json`.

Generate the DID document from published keychain material:

```json
{
  "@context": [
    "https://www.w3.org/ns/did/v1",
    "https://w3id.org/security/multikey/v1"
  ],
  "id": "did:web:bytescape.social:alice",
  "alsoKnownAs": [
    "at://alice.bytescape.social",
    "acct:alice@bytescape.social"
  ],
  "verificationMethod": [
    {
      "id": "did:web:bytescape.social:alice#iroh",
      "type": "Multikey",
      "controller": "did:web:bytescape.social:alice",
      "publicKeyMultibase": "z6Mk..."
    }
  ],
  "service": [
    {
      "id": "#activitypub",
      "type": "ActivityPubActor",
      "serviceEndpoint": "https://bytescape.social/@alice"
    }
  ]
}
```

The `publicKeyMultibase` is the same `z6Mk...` format already produced by `did_key_from_ed25519_bytes()` in `keychain_encrypted.rs:356`. Strip the `did:key:` prefix to get the multibase value.

### `GET /@{username}`

Serve a public profile page. For v18, this can be a minimal HTML page:

```html
<!DOCTYPE html>
<html>
<head>
  <title>{display_name} (@{username}@bytescape.social)</title>
  <meta property="og:title" content="{display_name}" />
  <meta property="og:description" content="{bio}" />
  <link rel="alternate" type="application/activity+json" href="https://bytescape.social/@{username}" />
</head>
<body>
  <h1>{display_name}</h1>
  <p>@{username}@bytescape.social</p>
  <p>{bio}</p>
  <p><a href="https://bytescape.social/{username}/did.json">DID Document</a></p>
</body>
</html>
```

**Content negotiation**: If `Accept: application/activity+json` or `application/ld+json`, return the AP actor JSON-LD instead of HTML. This makes `/@{username}` work as both a profile page AND an AP actor endpoint.

---

## P2: Publish API

These endpoints are called by the Tauri desktop app to push identity data to the server.

### Auth Model

Bearer token authentication. For single-user launch, use a pre-shared secret configured as an environment variable on the server (`PUBLISH_SECRET`). The Tauri app stores this in the keychain.

```
Authorization: Bearer {PUBLISH_SECRET}
```

### `POST /api/publish/did`

Push the user's DID document data to the server.

**Request body:**
```json
{
  "username": "alice",
  "public_key_multibase": "z6Mk...",
  "display_name": "Alice",
  "bio": "Building the future",
  "also_known_as": ["at://alice.bytescape.social"],
  "services": [
    { "id": "#activitypub", "type": "ActivityPubActor", "endpoint": "https://bytescape.social/@alice" }
  ]
}
```

**Server behavior:**
1. Validate bearer token
2. Write DID document JSON to `{data_dir}/{username}/did.json`
3. Update WebFinger registry (add username to known users)
4. Return `{ "status": "published", "did": "did:web:bytescape.social:alice" }`

### `POST /api/publish/profile`

Push profile HTML/metadata for the `/@{username}` page.

**Request body:**
```json
{
  "username": "alice",
  "display_name": "Alice",
  "bio": "Building the future",
  "avatar_url": "https://...",
  "links": [
    { "label": "Website", "url": "https://alice.com" }
  ]
}
```

### `GET /api/publish/status`

Check the current publish status for the authenticated user.

**Response:**
```json
{
  "username": "alice",
  "did_published": true,
  "did_url": "https://bytescape.social/alice/did.json",
  "profile_published": true,
  "profile_url": "https://bytescape.social/@alice",
  "webfinger_active": true,
  "last_published_at": "2026-04-08T12:00:00Z"
}
```

---

## P3: Tauri App — Replace Stubs with Real Publish Calls

### Replace `provision_pds_account` and `provision_ap_actor`

In `commands/relay.rs`, the stub functions at lines 117-165 currently return errors. Replace with real HTTP calls to the Publish API:

1. **`provision_ap_actor`** → Call `POST /api/publish/did` + `POST /api/publish/profile` on the bytescape.social server
2. **`provision_pds_account`** → Keep as stub for now (PDS is lower priority per user feedback), but update the error message to say "Coming after hosted identity is live"

### New command: `publish_identity`

```rust
#[tauri::command]
pub async fn publish_identity(
    state: State<'_, AppState>,
) -> Result<PublishResult, String>
```

This command:
1. Reads the user's keychain (username, public key, display name, bio)
2. Constructs the DID document payload
3. POSTs to `https://bytescape.social/api/publish/did`
4. POSTs to `https://bytescape.social/api/publish/profile`
5. Returns the publish status

### New command: `check_publish_status`

```rust
#[tauri::command]
pub async fn check_publish_status(
    state: State<'_, AppState>,
) -> Result<PublishStatus, String>
```

GETs `/api/publish/status` and returns the result.

### Store publish secret in keychain

Add `publish_secret: Option<String>` to `AuthData` struct in `keychain_encrypted.rs`. The frontend will provide this during the first-run publish flow.

### Register new commands in `lib.rs`

Add `publish_identity` and `check_publish_status` to the invoke handler.

### Files to modify

| File | Changes |
|------|---------|
| `commands/relay.rs` | Replace `provision_ap_actor` with real publish calls, add `publish_identity` + `check_publish_status` commands |
| `keychain_encrypted.rs` | Add `publish_secret` field to `AuthData` |
| `lib.rs` | Register new commands |

---

## P4: Endpoint Tests

Write integration tests that verify the server endpoints work correctly.

### Test structure

```
bytescape/server/tests/
├── identity_test.rs    # Test DID doc serving
├── webfinger_test.rs   # Test WebFinger resolution
├── publish_test.rs     # Test publish API
```

### Required test cases

1. **WebFinger**: `GET /.well-known/webfinger?resource=acct:testuser@bytescape.social` → 200 with valid JRD
2. **WebFinger unknown**: `GET /.well-known/webfinger?resource=acct:nonexistent@bytescape.social` → 404
3. **DID Document**: `GET /testuser/did.json` → 200 with valid DID document, correct `id`, `verificationMethod`, `service`
4. **AT Proto DID**: `GET /.well-known/atproto-did` → 200 with `did:web:bytescape.social:testuser`
5. **Profile page (HTML)**: `GET /@testuser` with `Accept: text/html` → 200 with HTML containing display name
6. **Profile page (AP)**: `GET /@testuser` with `Accept: application/activity+json` → 200 with AP actor JSON
7. **Publish DID**: `POST /api/publish/did` with valid auth → 200, then `GET /testuser/did.json` returns published data
8. **Publish unauthorized**: `POST /api/publish/did` without auth → 401

### Acceptance criteria (curl checks)

These exact curl commands must pass on the deployed server:

```bash
# WebFinger
curl -s https://bytescape.social/.well-known/webfinger?resource=acct:alice@bytescape.social | jq .subject
# Expected: "acct:alice@bytescape.social"

# DID Document
curl -s https://bytescape.social/alice/did.json | jq .id
# Expected: "did:web:bytescape.social:alice"

# AT Proto DID
curl -s https://bytescape.social/.well-known/atproto-did
# Expected: did:web:bytescape.social:alice

# Profile page
curl -s -H "Accept: text/html" https://bytescape.social/@alice | grep "<title>"
# Expected: contains display name

# AP actor
curl -s -H "Accept: application/activity+json" https://bytescape.social/@alice | jq .type
# Expected: "Person"
```

---

## Verification

Before committing:

```bash
cd bytescape
cargo check --manifest-path src-tauri/Cargo.toml   # 0 errors
cargo check --manifest-path server/Cargo.toml       # 0 errors (new crate)
npx tsc --noEmit                                    # 0 errors
pnpm test -- --run                                  # all tests pass
cd server && cargo test                             # server tests pass
```

---

## Track Boundaries

- **Track A owns**: `src-tauri/src/` (Rust app), `server/` (new server crate)
- **Track B owns**: `src/` (React/TypeScript)
- **Shared**: `src/types/index.ts` (add `PublishStatus` type), `src-tauri/src/lib.rs` (command registration)
- Do NOT modify frontend files — Track B will build the UI
