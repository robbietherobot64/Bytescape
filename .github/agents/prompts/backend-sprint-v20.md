# Backend Sprint v20 — Identity Resolver Server & Publish API

**Sprint**: v20 (Go-Live: Hosted Identity)  
**Track**: A (Backend — Rust)  
**Date**: 2026-04-09  
**Previous sprint**: v19 (profile customization deepening — delivered)  
**HEAD at prompt time**: `b7fca4e`  
**Decision**: Single-user launch first (fast go-live, harden later)

---

## Sprint Context

This sprint transitions Bytescape from a local-only desktop app to a **publicly resolvable identity**. When complete:

- `did:web:bytescape.social:{username}` resolves to a real DID document
- `acct:{username}@bytescape.social` resolves via WebFinger
- `https://bytescape.social/@{username}` serves a public profile page
- `https://bytescape.social/{username}/did.json` serves the DID document
- `/.well-known/atproto-did` resolves for AT Protocol handle verification

The Tauri desktop app publishes identity data to the server via an authenticated Publish API. The server is a standalone axum HTTP service — NOT embedded in the Tauri app.

### User feedback driving this sprint

> "We are now moving into a hosted url based identity, such that users will be able to host a profile and did document for cross platform verification."  
> — `user-feedback`

### What already exists (reuse, don't rewrite)

| Component | File | Status |
|-----------|------|--------|
| Ed25519 keypair gen + storage | `keychain_encrypted.rs:56-72` | ✅ Working |
| `did:key` derivation (multicodec) | `keychain_encrypted.rs:356-370` | ✅ Working |
| `did:web` string formatting | `keychain_encrypted.rs:412`, `auth.rs:423` | ✅ String only — no document |
| WebFinger JRD generation | `activitypub/webfinger.rs:55-75` | ✅ Working (`generate_webfinger()`) |
| AP Actor → JSON-LD | `activitypub/actor.rs:152-190` | ✅ Working (`to_activitystreams()`) |
| AP HTTP Signatures | `activitypub/signatures.rs` | ✅ Working |
| `AP_DOMAIN` constant | `activitypub/actor.rs:52` | ✅ `"bytescape.social"` |
| Provisioning stubs | `commands/relay.rs:33-165` | ⚠️ All return errors — replace in P3 |
| Profile settings persistence | `commands/profile.rs` | ✅ Working (v18a) |
| Profile pages CRUD | `commands/profile.rs` | ✅ Working (v18a) |
| Vault item deletion | `commands/vault.rs` | ✅ Working (v18a) |

### What doesn't exist (must build)

- **`bytescape/server/`** — Standalone axum server crate (new)
- DID Document (`did.json`) generation from keychain material
- Server-side identity resolver endpoints
- Publish API (authenticated POST endpoints)
- AT Protocol handle resolution endpoint
- Tauri app commands to call the Publish API

---

## P0: New Server Crate — `bytescape-server`

### Architecture Decision

Create a **new Rust crate** at `bytescape/server/` (sibling to `src-tauri/`). This is a standalone `axum` HTTP server, NOT embedded in the Tauri app. Rationale:

1. The Tauri app runs on the user's desktop — it can't serve public HTTP
2. The server runs on `bytescape.social` — always-on, publicly reachable
3. They share identity data via the Publish API (app → server)
4. Types can be shared later via workspace crate extraction; for v19, copy minimal types

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
[package]
name = "bytescape-server"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tower-http = { version = "0.6", features = ["cors", "trace"] }
tracing = "0.1"
tracing-subscriber = "0.3"
```

### config.rs — Environment-based configuration

```rust
pub struct Config {
    pub bind_addr: String,           // default "0.0.0.0:3000"
    pub domain: String,              // "bytescape.social"
    pub data_dir: String,            // "/data" or "./data"
    pub publish_secret: String,      // Bearer token for Publish API auth
}
```

Read from environment variables. Fail fast with clear messages if `PUBLISH_SECRET` or `DOMAIN` are missing.

### storage.rs — File-based storage

For single-user launch, store identity data as JSON files:

```
{data_dir}/
├── {username}/
│   ├── did.json           # DID document
│   ├── profile.json       # Profile metadata (display name, bio, links, avatar)
│   └── webfinger.json     # Cached JRD for quick serving
```

Provide methods:
- `store_did_document(username, did_doc) -> Result<()>`
- `load_did_document(username) -> Result<Option<DIDDocument>>`
- `store_profile(username, profile) -> Result<()>`
- `load_profile(username) -> Result<Option<Profile>>`
- `list_users() -> Result<Vec<String>>` (list dirs in data_dir)

### main.rs — Server entrypoint

```rust
#[tokio::main]
async fn main() {
    tracing_subscriber::init();
    let config = Config::from_env();
    let state = AppState::new(config);
    
    let app = Router::new()
        // Identity resolution (public)
        .route("/.well-known/webfinger", get(webfinger::handler))
        .route("/.well-known/atproto-did", get(atproto_did::handler))
        .route("/{username}/did.json", get(identity::did_json_handler))
        .route("/@{username}", get(profile::handler))
        // Publish API (authenticated)
        .route("/api/publish/did", post(publish::publish_did))
        .route("/api/publish/profile", post(publish::publish_profile))
        .route("/api/publish/status", get(publish::status))
        // Health
        .route("/health", get(|| async { "ok" }))
        .with_state(state);
    
    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

---

## P1: Identity Resolver Endpoints

### `GET /.well-known/webfinger?resource=acct:{username}@bytescape.social`

Return a JRD response:

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

**Validation**: Only respond for usernames that have a published DID document on disk. Return 404 for unknown users. Parse the `resource` query param — reject requests without `acct:` prefix or wrong domain.

### `GET /.well-known/atproto-did`

Return the user's AT Protocol DID as plain text. For single-user launch, return the sole registered user's `did:web`. For multi-user, this needs host-based routing (future).

```
did:web:bytescape.social:alice
```

### `GET /{username}/did.json`

**Critical endpoint.** The `did:web` spec requires the DID document at this exact path. For `did:web:bytescape.social:alice`, resolvers fetch `https://bytescape.social/alice/did.json`.

Generate from published keychain material:

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

Serve a public profile page. **Content negotiation**:

- If `Accept: application/activity+json` or `application/ld+json` → return AP actor JSON-LD
- Otherwise → return HTML profile page

HTML page should include:
- OpenGraph meta tags (`og:title`, `og:description`)
- Link to DID document
- Basic profile info (display name, bio, username)
- `<link rel="alternate" type="application/activity+json">` header for AP discovery

For v19, use a minimal server-rendered HTML template. The profile data comes from what the user published via the Publish API.

---

## P2: Publish API

These endpoints are called by the Tauri desktop app to push identity data to the server.

### Auth Model

Bearer token authentication. For single-user launch, use a pre-shared secret configured as `PUBLISH_SECRET` env var on the server. The Tauri app stores this in the encrypted keychain.

```
Authorization: Bearer {PUBLISH_SECRET}
```

Validate the token in an axum middleware or extractor. Return 401 for missing/invalid tokens.

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
2. Validate username (alphanumeric + dashes, no path traversal)
3. Generate DID document JSON from the payload
4. Write to `{data_dir}/{username}/did.json`
5. Return `{ "status": "published", "did": "did:web:bytescape.social:alice" }`

### `POST /api/publish/profile`

Push profile data for the `/@{username}` page.

**Request body:**
```json
{
  "username": "alice",
  "display_name": "Alice",
  "bio": "Building the future",
  "avatar_url": null,
  "links": [
    { "label": "Website", "url": "https://alice.com" }
  ]
}
```

### `GET /api/publish/status`

Check the current publish status for the authenticated user. Requires bearer token.

**Response:**
```json
{
  "username": "alice",
  "did_published": true,
  "did_url": "https://bytescape.social/alice/did.json",
  "profile_published": true,
  "profile_url": "https://bytescape.social/@alice",
  "webfinger_active": true,
  "last_published_at": "2026-04-09T12:00:00Z"
}
```

---

## P3: Tauri App — Publish Commands

### New command: `publish_identity`

```rust
#[tauri::command]
pub async fn publish_identity(
    state: State<'_, AppState>,
) -> Result<PublishResult, String>
```

This command:
1. Reads the user's keychain (username, public key, display name, bio)
2. Reads the publish secret from the keychain
3. Constructs the DID document payload
4. POSTs to `https://bytescape.social/api/publish/did`
5. POSTs to `https://bytescape.social/api/publish/profile`
6. Returns the publish status

### New command: `check_publish_status`

```rust
#[tauri::command]
pub async fn check_publish_status(
    state: State<'_, AppState>,
) -> Result<PublishStatus, String>
```

GETs `/api/publish/status` with the bearer token and returns the result.

### New command: `set_publish_secret`

```rust
#[tauri::command]
pub async fn set_publish_secret(
    state: State<'_, AppState>,
    secret: String,
) -> Result<(), String>
```

Stores the publish secret in the encrypted keychain (`AuthData.publish_secret`).

### Replace relay stubs

In `commands/relay.rs`, the stub functions at lines 33-164 return errors. Replace:

- `provision_ap_actor` → call `publish_identity` internally (or mark as alias)
- `provision_pds_account` → keep as stub, update error message: "Coming after hosted identity is live"

### Register new commands in `lib.rs`

Add `publish_identity`, `check_publish_status`, and `set_publish_secret` to the invoke handler.

### Keychain changes

Add `publish_secret: Option<String>` to `AuthData` in `keychain_encrypted.rs`. Handle the new field in serialization (default to `None` for existing keychains).

---

## Files Summary

### New files (create)

| File | Purpose |
|------|---------|
| `server/Cargo.toml` | Server crate manifest |
| `server/src/main.rs` | axum server entrypoint |
| `server/src/config.rs` | Environment config |
| `server/src/identity.rs` | DID document generation |
| `server/src/webfinger.rs` | WebFinger handler |
| `server/src/atproto_did.rs` | AT Protocol handle resolution |
| `server/src/profile.rs` | Profile page handler (HTML + AP content negotiation) |
| `server/src/publish.rs` | Publish API handlers |
| `server/src/auth.rs` | Bearer token validation |
| `server/src/storage.rs` | File-based storage |

### Modified files (Track A owns)

| File | Changes |
|------|---------|
| `src-tauri/src/keychain_encrypted.rs` | Add `publish_secret` to `AuthData` |
| `src-tauri/src/commands/relay.rs` | Replace stubs with real publish calls |
| `src-tauri/src/lib.rs` | Register new commands |

### New Tauri commands

| Command | Purpose |
|---------|---------|
| `publish_identity` | Push DID + profile to server |
| `check_publish_status` | Query publish status from server |
| `set_publish_secret` | Store publish secret in keychain |

---

## Verification

Before committing:

```bash
cd bytescape
# Server crate
cd server && cargo check && cd ..
# Tauri app
cargo check
npx tsc --noEmit
pnpm test -- --run
```

Document all changes in `progress.md`.

---

## Track Boundaries

- **Track A owns**: `src-tauri/src/` (Rust app), `server/` (new server crate)
- **Track B owns**: `src/` (React/TypeScript, CSS)
- **Shared files** (require PM coordination): `src/types/index.ts` (Track B adds `PublishStatus`/`PublishResult`)
