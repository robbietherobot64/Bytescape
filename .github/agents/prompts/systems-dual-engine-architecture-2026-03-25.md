# Bytescape: Unified Server Architecture
### Systems Advisory — March 25, 2026
### For PM delegation to Backend (Track A), Frontend (Track B), and Infrastructure agents

---

## Executive Summary

Bytescape will extend from a local-first desktop application into a **hybrid platform** by deploying a unified server at `bytescape.social` that natively hosts both an **AT Protocol PDS** and an **ActivityPub instance**. This allows users to fully migrate their Bluesky and Mastodon identities onto Bytescape-controlled infrastructure, while the desktop/mobile Tauri clients remain the sovereign data authority.

This is **not a rewrite**. The existing Tauri app (23,000+ lines Rust, 12,000+ lines React/TS, 110+ commands, 13+ tables) remains Phase 1 — the working product. Server infrastructure builds outward from it.

### Key Principles

1. **No two-class privacy model.** Every user — web, desktop, or mobile — gets client-side key generation and encrypted storage. The server is always a blind relay for sovereign data. Web users who opt into server-managed convenience get clearly disclosed trade-offs, not silently degraded privacy.
2. **One server, dual protocol stacks.** A single deployment hosts both the AT Protocol PDS and the ActivityPub instance, sharing identity resolution, storage, and the sync bridge to Iroh.
3. **Desktop-first, server-second.** The Tauri app is the primary product. The server extends its reach (always-on federation, mobile async sync, web portal), not the other way around.
4. **Self-hostable from day one.** The server ships as a Docker Compose stack that runs on a home NAS or a commercial VPS with identical configuration. Initial deployment will be commercial-grade to handle user load, but the architecture never assumes managed cloud services.

---

## 1. Architecture Overview

```
                        bytescape.social (Single Server)
┌──────────────────────────────────────────────────────────────────────┐
│                                                                      │
│  ┌─────────────────┐  ┌──────────────────┐  ┌────────────────────┐  │
│  │  AT Proto PDS    │  │  AP Instance     │  │  Iroh Relay        │  │
│  │  (Tranquil PDS)  │  │  (custom axum    │  │  (blind encrypted  │  │
│  │                  │  │   from existing   │  │   store+forward)   │  │
│  │                  │  │   AP module)     │  │                    │  │
│  │  • XRPC server   │  │  • Inbox/Outbox  │  │  • Iroh blob cache │  │
│  │  • Repo storage   │  │  • WebFinger     │  │  • Push notify     │  │
│  │  • DID hosting    │  │  • Followers DB   │  │  • Mobile wakeup   │  │
│  │  • Firehose out   │  │  • Delivery queue │  │  • P2P relay       │  │
│  └────────┬─────────┘  └────────┬─────────┘  └─────────┬──────────┘  │
│           │                     │                       │            │
│  ┌────────┴─────────────────────┴───────────────────────┴──────────┐  │
│  │                     Sync Bridge Daemon                          │  │
│  │                                                                 │  │
│  │  • Indexes Iroh blobs → SQL for discovery/search                │  │
│  │  • Publishes PDS repo changes → Iroh for edge sync             │  │
│  │  • Routes AP inbox → Iroh blobs for offline edge users          │  │
│  │  • Maps did:web ↔ did:plc ↔ AP actor URLs (via IdentityVerifier)│  │
│  └─────────────────────────────┬───────────────────────────────────┘  │
│                                │                                     │
│  ┌─────────────────────────────┴───────────────────────────────────┐  │
│  │           Identity Resolver (/.well-known/*)                    │  │
│  │                                                                 │  │
│  │  • /.well-known/did.json         → did:web document (user-facing│  │
│  │  • /.well-known/did.jsonl        → did:webvh verifiable log     │  │
│  │  • /.well-known/webfinger        → acct: resolution             │  │
│  │  • /.well-known/atproto-did      → did:plc for handle           │  │
│  │  • HTTPS actor endpoint          → AP actor JSON                │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │           Web Portal (optional — WASM Iroh + React SPA)         │  │
│  │           Static assets, visitor read-only mode                 │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │           Reverse Proxy (Caddy)                                 │  │
│  │           Auto-TLS, wildcard subdomains, WebSocket support      │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
         │              │               │              │
    XRPC/HTTPS     AP S2S/HTTPS    Iroh QUIC      HTTPS/WSS
         │              │               │              │
    ┌────┘         ┌────┘          ┌────┘         ┌────┘
    ▼              ▼               ▼              ▼
 Bluesky       Mastodon/       Tauri          Web
 Network       Fediverse       Desktop        Browser
                               & Mobile
```

### Component Responsibilities

| Component | Role | Technology |
|-----------|------|-----------|
| **AT Proto PDS** | Hosts user repositories, serves XRPC, firehoses to Bluesky Relay | Tranquil PDS (Rust, Postgres) — single binary, native `did:web`, passkeys/2FA/SSO. Repo: `tangled.org/tranquil.farm/tranquil-pds`. Official PDS (Node.js, SQLite) is the documented fallback. |
| **AP Instance** | Federates with Mastodon/Lemmy/Threads, handles inbox/outbox/WebFinger | Custom `axum` server extending existing AP module (`src-tauri/src/activitypub/`) |
| **Iroh Relay** | Encrypted store-and-forward for edge clients, mobile push wakeup, P2P relay | Custom Rust binary using `iroh` crate — extends existing `sync.rs` code |
| **Sync Bridge** | Bidirectional translation: Iroh blobs ↔ SQL ↔ PDS repos ↔ AP objects | Custom Rust daemon, internal-only (not exposed to internet) |
| **Identity Resolver** | Serves DID documents, WebFinger, handle resolution | Thin HTTP handler (can be part of Caddy config or a small axum service) |
| **Web Portal** | Read-only public profiles via WASM Iroh client | Static SPA (same React components, `RestDataProvider` adapter) |
| **Caddy** | Reverse proxy, auto-TLS, wildcard `*.bytescape.social` subdomains | Caddy v2 with `tls` and `on_demand_tls` directives |

---

## 2. Identity Architecture

### Root Identity: `did:web:bytescape.social:{username}`

Every Bytescape user gets a `did:web` as their universal umbrella identity. The DID Document it resolves to links all protocol-specific identities together:

```json
{
  "@context": ["https://www.w3.org/ns/did/v1", "https://w3id.org/security/multikey/v1"],
  "id": "did:web:bytescape.social:alice",
  "alsoKnownAs": [
    "at://alice.bytescape.social",
    "acct:alice@bytescape.social"
  ],
  "verificationMethod": [{
    "id": "did:web:bytescape.social:alice#atproto",
    "type": "Multikey",
    "controller": "did:web:bytescape.social:alice",
    "publicKeyMultibase": "zDnae..."
  }, {
    "id": "did:web:bytescape.social:alice#iroh",
    "type": "Multikey",
    "controller": "did:web:bytescape.social:alice",
    "publicKeyMultibase": "z6Mk..."
  }],
  "service": [{
    "id": "#atproto_pds",
    "type": "AtprotoPersonalDataServer",
    "serviceEndpoint": "https://pds.bytescape.social"
  }, {
    "id": "#activitypub",
    "type": "ActivityPubActor",
    "serviceEndpoint": "https://bytescape.social/@alice"
  }, {
    "id": "#iroh_relay",
    "type": "IrohRelay",
    "serviceEndpoint": "iroh://relay.bytescape.social"
  }]
}
```

### Protocol-Native Identifiers (No Fabrication)

| Protocol | Native Identifier | How It Works |
|----------|-------------------|--------------|
| **AT Protocol (Bluesky)** | `did:plc:abc...` | Real PLC registered at `plc.directory`. PDS hosting means the service endpoint points to `pds.bytescape.social`. Users migrating from `bsky.social` sign a PLC rotation operation to update their service endpoint. |
| **ActivityPub** | `https://bytescape.social/@alice` (actor URL) | Standard AP actor. Discovery via WebFinger: `acct:alice@bytescape.social`. **No `did:plc` fabrication.** AP does not use DIDs. |
| **P2P / Iroh** | `did:key:z6Mk...` (from Ed25519 keypair) | Self-certifying, offline-capable. Used for P2P sync, device pairing, vault signing. |
| **Umbrella** | `did:web:bytescape.social:alice` | Ties everything together. The `alsoKnownAs` array links to the AT handle and AP acct. Bidirectional: the AP actor and PDS profile both reference the `did:web`. |

### DID Hierarchy (Corrected from Current Codebase)

The current `get_owner_did()` has a 5-level fallback chain including fabricated `did:plc` for non-Bluesky platforms. This is replaced with:

```
1. did:web:bytescape.social:{username}     ← Set when username is chosen. Permanent.
2. did:plc:...                              ← ONLY real Bluesky DID (from PLC directory)
3. did:key:z6Mk...                          ← P2P/offline only (from Iroh Ed25519 keypair)
```

**Migration:** Existing `LinkedAccount` entries with fabricated `did:plc` identifiers (generated by `generate_platform_plc()`) must be migrated to platform-native identifiers:
- Mastodon: `acct:user@instance` 
- Generic AP: actor URL
- The `generate_platform_plc()` function is removed.

### DID Verifiability: Phased Approach (did:webvh → did:webs)

#### The Problem with Plain `did:web`

Plain `did:web` resolves to a DID Document hosted at a URL — but there's no verifiable history. If the server is compromised, an attacker can silently replace the DID Document (and therefore the keys). There's no audit trail, no way to detect tampering, and no key rotation history.

#### UX Principle: Users Only See `did:web`

**Users never see `did:webvh`, `did:webs`, or any internal DID method.** The user-facing identity is always the clean, human-readable form:

```
alice.bytescape.social          ← handle (AT Proto & AP)
did:web:bytescape.social:alice   ← shown in advanced/developer UI only
```

The verifiable DID methods (`did:webvh`, later `did:webs`) are **infrastructure internals** — they secure the identity system behind the scenes, but the user never needs to know they exist. This is analogous to how HTTPS certificates secure the web without users ever seeing the X.509 chain.

This UX principle also **eliminates rework on method transitions**. Since no user-facing surface displays `did:webvh` or `did:webs`, swapping the underlying verifiability layer requires zero UI changes.

#### Phase 1: `did:webvh` (Deploy with Server Foundation — Phase 4.5)

[did:webvh](https://identity.foundation/didwebvh/) (v1.0, DIF) adds a **verifiable history** layer to `did:web` by publishing a hash-chained JSON Lines log alongside the standard `did.json`:

```
https://bytescape.social/alice/did.json      ← standard did:web document (unchanged)
https://bytescape.social/alice/did.jsonl     ← did:webvh verifiable log (new)
```

Key properties:
- **Parallel `did:web` publishing**: Every `did:webvh` DID is simultaneously a valid `did:web`. AT Protocol, ActivityPub, and all existing resolvers continue to work unchanged.
- **SCID (Self-Certifying Identifier)**: The DID contains a hash of the inception document, binding the identifier to its creation event. Enables domain portability — if `bytescape.social` moves to a new host, the SCID proves continuity.
- **Hash-chained log**: Every DID Document update is appended to `did.jsonl` with a hash of the previous entry. Tampering with history breaks the chain.
- **Pre-rotation** (optional): Commit to next rotation keys at creation time. Even if current keys are compromised, the attacker can't rotate.
- **Witnesses** (optional): Third-party services that countersign log entries, providing independent verification.
- **Rust library**: `didwebvh-rs` v0.4.0 on crates.io — production-ready.

**What we deploy:**
- Identity Resolver generates both `did.json` and `did.jsonl` for every user
- Rust backend uses `didwebvh-rs` crate for log generation and verification
- `did:web` continues to be the resolver entrypoint for all protocols
- `did:webvh` log is published alongside for verifiers that support it
- No witnesses initially (add in Phase 5 if needed)

#### Phase 2: `did:webs` (Upgrade Path — Phase 6+)

[did:webs](https://trustoverip.github.io/kswg-did-method-webs-specification/) (v0.9.18, ToIP/GLEIF) replaces the hash-chain with full **KERI infrastructure** — a cryptographic key event receipt system with pre-rotation, witnesses, watchers, multi-signature, delegation, and verifiable credentials (ACDCs) baked in.

```
https://bytescape.social/alice/did.json      ← same did:web document
https://bytescape.social/alice/keri.cesr     ← KERI event stream (replaces did.jsonl)
```

Key properties beyond did:webvh:
- **KERI pre-rotation**: Post-quantum-safe key commitment at inception
- **Native multi-signature**: ConditionalProof2022 threshold schemes
- **Delegation**: Hierarchical identity (e.g., family accounts, org structure)
- **ACDC integration**: Verifiable credentials anchored to the key event log
- **GLEIF backing**: The spec is driven by the Global Legal Entity Identifier Foundation

**Why wait:**
- Requires full KERI stack (KEL + CESR + ACDC + TEL) — significant implementation effort
- Reference implementation is Python only; no Rust `did:webs` resolver exists yet
- Overkill for personal social vault at current scale
- `did:webvh` covers all near-term security needs

**Why plan for it:**
- Enterprise features (multisig, delegation) become relevant as Bytescape grows
- GLEIF backing means institutional adoption pressure
- The same `did.json` file serves both methods — the upgrade is additive (publish `keri.cesr` alongside, retire `did.jsonl`)

#### Abstraction Layer: `IdentityVerifier` Trait

To minimize technical debt on the webvh → webs transition, all verifiable DID operations go through a Rust trait:

```rust
/// Backend identity verifier — all DID verifiability operations
/// go through this trait. Implementations are swappable.
pub trait IdentityVerifier: Send + Sync {
    /// Create a new verifiable DID for a user.
    /// Returns the did:web identifier (user-facing) and writes
    /// the verifiable artifacts (did.jsonl or keri.cesr) to storage.
    async fn create_did(&self, username: &str, signing_key: &Ed25519KeyPair) -> Result<String>;
    
    /// Rotate keys for an existing DID.
    async fn rotate_keys(&self, did: &str, new_key: &Ed25519KeyPair) -> Result<()>;
    
    /// Resolve and verify a DID, returning the verified DID Document.
    async fn resolve_and_verify(&self, did: &str) -> Result<DidDocument>;
    
    /// Generate the publishable artifacts for the web server.
    /// For webvh: did.json + did.jsonl
    /// For webs:  did.json + keri.cesr
    async fn export_artifacts(&self, did: &str) -> Result<DidArtifacts>;
}

/// Phase 1 implementation
pub struct WebvhVerifier { /* uses didwebvh-rs */ }

/// Phase 2 implementation (future)
pub struct WebsVerifier { /* uses keri-rs + cesr-rs */ }
```

The Identity Resolver HTTP handler calls `export_artifacts()` to serve files. The Sync Bridge calls `resolve_and_verify()` to validate incoming DIDs. Frontend never touches any of this — it only sees `did:web` strings.

**Transition plan:**
1. Deploy with `WebvhVerifier`
2. When ready for `did:webs`, implement `WebsVerifier`
3. Run both in parallel during transition (publish both `did.jsonl` and `keri.cesr`)
4. Retire `did.jsonl` once all verifiers support `did:webs`
5. Zero frontend changes required at any step

### Custom Domain Handles (Indieweb Identity)

Users can bring their own domain as their fediverse identity, making their personal website their handle:

```
User owns: alice.com
AT Protocol:   @alice.com  → alice.com/.well-known/atproto-did → did:plc:abc...
ActivityPub:   @alice@alice.com → alice.com/.well-known/webfinger → delegates to bytescape.social
DID:           did:web:alice.com → alice.com/.well-known/did.json → links all identities
```

**Implementation:**
- AT Proto handle: User adds DNS TXT record `_atproto.alice.com → did=did:plc:abc...` OR serves `/.well-known/atproto-did` returning their DID. Tranquil PDS verifies this during handle claim.
- AP handle: User's domain `/.well-known/webfinger?resource=acct:alice@alice.com` returns a response pointing to the `bytescape.social` AP actor. This can be a static JSON file on their web server, a redirect, or a reverse proxy rule.
- DID Document: User can optionally serve `did:web:alice.com` at `alice.com/.well-known/did.json`, linking to their AT Proto DID, AP actor, and Iroh node. This makes their personal domain the ultimate root of trust.
- **UI wizard:** Settings screen walks the user through DNS verification with copy-paste instructions for each step. Shows green checkmarks as each verification passes.

---

## 3. Account Migration Flows

### 3a. Mastodon → Bytescape (AP Account Migration)

The [Mastodon Account Migration](https://docs.joinmastodon.org/user/moving/) protocol is a standard AP feature:

```
Step 1: User creates account on bytescape.social AP instance
        → Gets @alice@bytescape.social identity
        
Step 2: On bytescape.social, set alias: "I am also @alice@mastodon.social"
        → AP actor document gets alsoKnownAs: ["acct:alice@mastodon.social"]
        
Step 3: On mastodon.social, trigger Move to @alice@bytescape.social
        → mastodon.social sends AP `Move` activity to all of Alice's followers
        
Step 4: Followers' instances automatically re-follow @alice@bytescape.social
        → mastodon.social marks Alice's old profile as "moved to..."
        
Step 5: (Optional) Alice exports her Mastodon archive (CSV/JSON)
        → Imports into Bytescape vault via existing archive import pipeline
```

**Server requirements:** The AP instance must support:
- `alsoKnownAs` on actor documents (standard AP extension, supported by GoToSocial)
- Inbound `Move` activity handling (accept follower transfers)
- Outbound `Move` activity (for users migrating away — good citizenship)
- Persistent follower/following collections

### 3b. Bluesky → Bytescape (AT Protocol PDS Migration)

AT Protocol has [built-in account migration](https://atproto.com/guides/account-migration):

```
Step 1: Deploy PDS at pds.bytescape.social
        → PDS registers with Bluesky Relay (bsky.network) for firehose
        
Step 2: User creates account on Bytescape PDS (or provisions via Tauri app)
        → PDS generates did:plc for new users, or accepts migrating did:plc
        
Step 3: User initiates migration from bsky.social PDS
        → com.atproto.server.requestAccountMove on source PDS
        → Source PDS exports full repo (MST + blobs)
        
Step 4: Bytescape PDS imports the repo
        → com.atproto.server.importRepo
        → All posts, likes, follows, blocks transferred
        
Step 5: User signs PLC rotation operation
        → Updates did:plc service endpoint from bsky.social → pds.bytescape.social
        → plc.directory reflects the change
        
Step 6: Bluesky network now routes all reads/writes for this did:plc 
        through pds.bytescape.social
```

**Server requirements:** The PDS must:
- Implement the full `com.atproto.sync` XRPC endpoints (repo export/import, firehose subscription)
- Implement `com.atproto.server` endpoints (account creation, session management)
- Support `did:plc` rotation key operations
- Firehose to the Bluesky Relay so content appears in the AppView
- Handle blob storage and serving via `com.atproto.sync.getBlob`

**This is why using an existing PDS implementation is critical** — implementing the AT Proto server spec from scratch is 3-6 months of work. Tranquil PDS (Rust, Postgres, `did:web` support, single binary) or the official PDS (Node.js, SQLite, battle-tested) provide this out of the box.

### 3c. New User (Native Bytescape Account)

```
Step 1: User creates account via Tauri app or web portal
        → Client generates Ed25519 keypair locally (even in browser via WebCrypto)
        → did:key derived from keypair (P2P identity)
        → Username chosen → did:web:bytescape.social:{username} assigned
        
Step 2: PDS account auto-provisioned
        → did:plc created and registered at plc.directory
        → Service endpoint: pds.bytescape.social
        → Handle: {username}.bytescape.social
        
Step 3: AP actor auto-provisioned
        → Actor URL: https://bytescape.social/@{username}
        → WebFinger: acct:{username}@bytescape.social
        → alsoKnownAs bidirectionally links did:web and AP acct
        
Step 4: Iroh relay registration
        → Encrypted blob sync channel established
        → did:key published in did:web document
```

User now has a single identity that works on Bluesky, the Fediverse, and Bytescape P2P — with one username.

---

## 4. Client Architecture: Adapter Pattern

The React frontend compiles into three targets from the same component tree:

```
┌─────────────── Shared React Components ───────────────┐
│                                                        │
│  FeedScreen, ChatScreen, ProfileScreen, EmailScreen,   │
│  ComposeBox, Sidebar, ThemeEngine, etc.                │
│                                                        │
│  All components call:  dataProvider.getFeed()           │
│                        dataProvider.sendPost()          │
│                        dataProvider.getProfile()        │
│                        etc.                             │
│                                                        │
└────────────┬──────────────┬──────────────┬─────────────┘
             │              │              │
     ┌───────┴───────┐ ┌───┴────┐  ┌──────┴──────┐
     │ TauriProvider  │ │ REST   │  │ WASM        │
     │               │ │Provider│  │Provider      │
     │ invoke() IPC  │ │ fetch()│  │ Iroh WASM +  │
     │ → Rust backend│ │ → HTTP │  │ fetch()      │
     │               │ │ → API  │  │ → read-only  │
     │ Desktop +     │ │ server │  │ blobs        │
     │ Mobile        │ │        │  │              │
     └───────────────┘ └────────┘  └──────────────┘
```

| Provider | Target | Data Source | Auth |
|----------|--------|-------------|------|
| `TauriProvider` | Desktop + Mobile (Tauri) | Local Rust backend via `invoke()` | Keychain (local) |
| `RestProvider` | Web app (bytescape.social) | Server REST API via `fetch()` | JWT/OAuth (server) |
| `WasmProvider` | Visitor portal (read-only) | Iroh WASM node fetching public blobs | None (public only) |

### Build Configuration

```
pnpm build:tauri      → TauriProvider injected, outputs Tauri bundle
pnpm build:web        → RestProvider injected, outputs static SPA
pnpm build:portal     → WasmProvider injected, outputs visitor portal
```

Environment variable or build-time flag selects the provider. All three share the same component tree, styles, and themes.

---

## 5. Mobile Architecture

### The Problem

Mobile OSes (iOS especially) aggressively suspend background processes:
- iOS gives ~30 seconds in `applicationWillResignActive` before full suspension
- All network connections (including QUIC/Iroh) are torn down on suspension
- Background fetch is unreliable and rate-limited by the OS
- The app cannot maintain a persistent P2P connection

### The Solution: Relay-Assisted Mobile Sync

```
┌─────────────┐          ┌──────────────┐         ┌────────────┐
│ Mobile App   │◄────────►│ Iroh Relay    │◄───────►│ Desktop    │
│ (Tauri)      │  QUIC    │ (bytescape   │  QUIC   │ App        │
│              │          │  .social)     │         │            │
└──────┬───────┘          └──────┬───────┘         └────────────┘
       │                         │
       │  On app suspend:        │  While app suspended:
       │  1. Flush pending       │  1. Relay buffers incoming
       │     writes to relay     │     encrypted blobs
       │  2. Register APNs/FCM   │  2. Relay sends push
       │     push token          │     notification via
       │  3. Close QUIC          │     APNs/FCM
       │                         │  3. Push wakes app for
       │  On app resume:         │     background fetch
       │  1. Reconnect QUIC      │
       │  2. Pull buffered blobs │
       │  3. Resume IDLE sync    │
```

### Mobile-Specific Implementation

| Task | Detail | Platform |
|------|--------|----------|
| **Lifecycle hooks** | Tauri mobile plugins for `applicationWillResignActive` (iOS) / `onTrimMemory` (Android) | Both |
| **Pre-suspend sync** | On lifecycle event, flush all pending vault writes to Iroh relay within 25-second window | Both |
| **Push registration** | Register APNs (iOS) / FCM (Android) token with relay on connect | Both |
| **Silent push** | Relay sends content-available push when new blobs arrive for suspended client | iOS primarily |
| **Background fetch** | iOS `BGAppRefreshTask` / Android `WorkManager` for periodic pull when push fails | Both |
| **Reconnect strategy** | Exponential backoff with jitter on QUIC reconnect after resume | Both |
| **Responsive layout** | Same React components, CSS responsive breakpoints for touch viewports | Both |
| **Gesture navigation** | Swipe-back, pull-to-refresh, long-press context menus via Tauri mobile plugins | Both |

### Tauri Mobile Build Targets

```bash
# iOS
rustup target add aarch64-apple-ios
pnpm tauri ios build

# Android (already proven — 77MB APK built March 2026)
rustup target add aarch64-linux-android armv7-linux-androideabi
pnpm tauri android build
```

---

## 6. Server Deployment

### Docker Compose Stack

The entire server ships as a single `docker-compose.yaml`:

```yaml
services:
  caddy:           # Reverse proxy, auto-TLS, wildcard subdomains
  pds:             # AT Protocol PDS (Tranquil PDS — Rust, Postgres)
  ap-server:       # ActivityPub instance (custom axum — extends existing AP module)
  iroh-relay:      # Encrypted Iroh blob relay + push notifications
  sync-bridge:     # Internal daemon: Iroh ↔ SQL ↔ PDS ↔ AP translation
  postgres:        # Shared database (Tranquil PDS + AP server + bridge)
```

### Hardware Requirements

| Scale | Hardware | Notes |
|-------|----------|-------|
| **Self-hosted testing (1-20 users)** | Any x86_64/arm64 with 2GB RAM, 50GB SSD | Developer's own hardware. Behind Cloudflare Tunnel or Tailscale Funnel for `bytescape.social`. **This is the initial deployment.** |
| **Small community (20-500)** | 4GB RAM, 2 CPU, 100GB SSD | Same Docker Compose stack, moved to commercial VPS (provider TBD based on testing). |
| **Commercial scale (500+)** | 8GB+ RAM, 4+ CPU, 250GB+ SSD, S3 for blobs | Dedicated server or cloud VPS. Blob storage offloaded to S3-compatible (Cloudflare R2, Wasabi). |

### DNS Configuration

```
bytescape.social          A     → server IP
*.bytescape.social        A     → server IP  (wildcard for user subdomains)
pds.bytescape.social      CNAME → bytescape.social  (AT Proto PDS)
relay.bytescape.social    CNAME → bytescape.social  (Iroh relay)
```

The wildcard allows:
- `alice.bytescape.social` → AT Protocol handle resolution
- `bytescape.social/@alice` → AP actor resolution
- `pds.bytescape.social` → XRPC endpoint for all PDS users

---

## 7. PDS Selection: Tranquil PDS vs Official PDS

Both are viable. Here's the trade-off:

| Factor | Tranquil PDS (Rust) | Official PDS (Node.js) |
|--------|--------------------|-----------------------|
| **Language** | Rust — same as Bytescape backend | TypeScript/Node.js — different stack |
| **Database** | PostgreSQL | SQLite |
| **`did:web`** | ✅ Native support (PDS-hosted subdomains or BYOD) | ❌ `did:plc` only |
| **Auth features** | Passkeys, 2FA (TOTP/FIDO2), SSO, app passwords with scopes | Basic password + app passwords |
| **Maturity** | Newer, actively developed (172 stars, commits daily) | Battle-tested by Bluesky team for 2+ years |
| **Binary** | Single binary, no runtime | Requires Node.js 18+ |
| **Scaling** | Postgres scales well | SQLite has ceiling (~500-1000 users) |
| **Maintenance** | Community-maintained (AGPL-3.0) | Bluesky-maintained (MIT/Apache-2.0) |
| **Repo** | `tangled.org/tranquil.farm/tranquil-pds` | `github.com/bluesky-social/pds` |

**Decision:** Use **Tranquil PDS** for alignment with the Rust stack, native `did:web` support, Postgres scalability, and advanced auth features (passkeys, 2FA, SSO). Fall back to the official PDS only if Tranquil proves too immature during self-hosted testing. Both expose the same XRPC API surface, so the Sync Bridge is unaffected by a swap.

---

## 8. AP Instance Selection: GoToSocial vs Custom

| Factor | GoToSocial | Custom (extend existing AP module) |
|--------|------------|-------------------------------------|
| **Migration support** | ✅ Full Mastodon-compatible migration (Move activity) | Must implement from scratch |
| **Follower management** | ✅ Persistent, battle-tested | Must implement from scratch |
| **Delivery queue** | ✅ Reliable with retries | Must implement from scratch |
| **Language** | Go | Rust (matches codebase) |
| **Customization** | Config-driven, limited | Full control |
| **Integration depth** | External process, communicate via AP S2S or internal API | Could be compiled into the same binary as the Sync Bridge |
| **License** | AGPL-3.0 | N/A (internal) |

**Decision:** Build a **custom axum AP server** extending the existing AP module (8 files, 1,636 lines in `src-tauri/src/activitypub/`). This maximizes flexibility, keeps the entire stack in Rust, and avoids an external Go dependency. The existing module already has actor management, inbox/outbox, HTTP signatures, and WebFinger — the gaps are persistent follower storage, reliable delivery queue, `Move` activity support, and multi-user actor registry. GoToSocial remains a documented fallback if the custom route proves too slow for the initial self-hosted deployment.

**Note on `megalodon-rs`:** The TECHNICAL_SPEC (§7.1) references `megalodon-rs` as an AP protocol adapter. This is a **Mastodon REST API client library** (like an SDK for talking to Mastodon instances), not an AP server framework. It's useful for the Tauri desktop app's client-side Mastodon interactions (fetching timelines, posting, DMs from within the app), but it has no role in the server-side AP instance. The custom axum server handles AP S2S federation directly via HTTP signatures and ActivityStreams JSON.

---

## 9. Development Phasing

### Corrected Phase Sequence

The existing roadmap phases 1-3 are complete. Phase 4 is in progress. This document modifies Phase 4 Convergence and Phase 5 to incorporate server hosting.

```
Phase 4 (CURRENT)     Phase 4.5 (NEW)        Phase 5 (MODIFIED)      Phase 6+
─────────────────     ───────────────         ──────────────────      ────────
Federation +          Server Foundation       Scale + Mobile +        Email
Rich Sharing +        ─────────────────       Web Portal              Engine
Profile Platform      • Deploy PDS            ────────────────        ────────
                      • Deploy AP instance    • Mobile native         • IMAP/
Already in progress   • Iroh relay            • Account migration     SMTP
with existing         • Sync Bridge MVP       • Web portal SPA        • Hosted
Phase 4 tracks        • Identity resolver     • Station Protocol      email
                      • DNS + TLS             • Push notifications    
                      • Docker Compose        • Self-host packaging   
```

### Phase 4.5: Server Foundation (NEW — insert between Phase 4 and Phase 5)

**Goal:** Deploy `bytescape.social` as a unified server hosting AT Proto PDS + AP instance + Iroh relay. Single Docker Compose stack. Serves the identity resolution endpoints. Enough for the developer (you) to migrate your own accounts as proof-of-concept.

**Duration:** 4-6 weeks with 2 agents parallel

#### Infrastructure Tasks (Track I — new)

| ID | Task | Description | Effort |
|----|------|-------------|--------|
| **I-1** | Docker Compose scaffold | `caddy` + `postgres` + empty slots for PDS/AP/relay services. Caddy config with wildcard TLS for `*.bytescape.social`. | 2-3 days |
| **I-2** | Deploy Official PDS | Deploy the official Bluesky PDS (`ghcr.io/bluesky-social/pds`) using its Docker Compose setup. Verify XRPC health endpoint (`/xrpc/_health`), account creation via `goat` CLI, WebSocket firehose (`subscribeRepos`). | 1-2 days |
| **I-3** | Deploy custom AP server | Build `axum` server binary extending existing AP module. Multi-user actor registry, persistent follower/following storage (Postgres), inbox POST handler, outbox GET, WebFinger resolution. Verify basic federation with external Mastodon instance. | 2-3 weeks |
| **I-4** | Identity resolver | Serve `/.well-known/did.json` generating `did:web` documents dynamically from user records. Serve `/.well-known/did.jsonl` with did:webvh verifiable log (generated via `IdentityVerifier` trait). Serve `/.well-known/webfinger` routing to either PDS (AT handle resolution) or AP server (acct resolution). Users only ever see `did:web` — the `did:webvh` log is infrastructure-only. | 4-6 days |
| **I-5** | Iroh relay daemon | Rust binary using `iroh` crate. Accepts encrypted blob uploads from edge clients, stores in content-addressed filesystem, serves on request. No decryption — blind relay only. | 1-2 weeks |
| **I-6** | Sync Bridge MVP | Internal daemon that: (a) watches PDS firehose for new records, packages as Iroh blobs for edge sync; (b) accepts Iroh blobs from edge clients, creates PDS repo records; (c) translates AP inbox messages into Iroh blobs for offline edge users. | 2-3 weeks |
| **I-7** | DNS + domain setup | Configure `bytescape.social` A records, wildcard subdomain, MX (for future email), Caddy auto-TLS. | 1 day |

#### Backend Tasks (Track A)

| ID | Task | Description | Effort |
|----|------|-------------|--------|
| **A-S1** | Identity migration (keychain) | Remove `generate_platform_plc()`. Migrate `LinkedAccount.identifier` to platform-native IDs. Simplify `get_owner_did()` to 3-level hierarchy. | 3-5 days |
| **A-S1b** | `IdentityVerifier` trait + `WebvhVerifier` | Implement the `IdentityVerifier` trait (see §2). First implementation: `WebvhVerifier` using `didwebvh-rs` crate. Generates `did.json` + `did.jsonl` artifacts. All DID creation/rotation/verification goes through this trait. Users only see `did:web` — the trait is the backend abstraction that makes webvh → webs swappable later. | 1-2 weeks |
| **A-S2** | Tauri ↔ Relay sync commands | New commands: `relay_connect`, `relay_push_blob`, `relay_pull_pending`, `relay_register_push_token`, `relay_sync_status`. Edge client pushes/pulls encrypted blobs to/from relay. | 1-2 weeks |
| **A-S3** | PDS account provisioning | Tauri command `provision_pds_account` — creates account on Bytescape PDS via XRPC, stores credentials in keychain, updates `did:plc` if needed. | 3-5 days |
| **A-S4** | AP actor provisioning | Tauri command `provision_ap_actor` — creates account on GoToSocial via API, links to `did:web`, stores credentials. | 3-5 days |

#### Frontend Tasks (Track B)

| ID | Task | Description | Effort |
|----|------|-------------|--------|
| **B-S1** | DataProvider interface | Define `DataProvider` interface (`getFeed()`, `getProfile()`, `sendPost()`, etc.). Implement `TauriDataProvider` wrapping existing `invoke()` calls. | 1 week |
| **B-S2** | Server account setup UI | Settings screen section: "Bytescape Social" — provision PDS + AP accounts, show migration status, link/unlink. | 3-5 days |
| **B-S3** | Relay status indicator | UI indicator showing relay connection status (connected/syncing/offline). Unread count from relay for items received while app was closed. | 2-3 days |

### Phase 5: Scale, Mobile & Web Portal (MODIFIED)

**Goal:** Mobile native clients, account migration wizards, web portal, self-host packaging.

**Duration:** 8-12 weeks

#### Mobile (Track M — new)

| ID | Task | Description | Effort |
|----|------|-------------|--------|
| **M-1** | Tauri Mobile lifecycle hooks | iOS `applicationWillResignActive` / Android `onTrimMemory` — trigger pre-suspend sync flush to relay. | 1 week |
| **M-2** | Push notification integration | APNs (iOS) + FCM (Android) registration with relay. Silent push to wake app for background sync. | 2 weeks |
| **M-3** | Responsive touch UI | CSS breakpoints, swipe gestures, pull-to-refresh, long-press context menus. Same React components. | 2-3 weeks |
| **M-4** | Mobile-optimized sync | Incremental sync with bandwidth awareness. WiFi: full sync. Cellular: headers-only, fetch bodies on demand. | 1 week |

#### Account Migration (Track A)

| ID | Task | Description | Effort |
|----|------|-------------|--------|
| **A-M1** | Bluesky migration wizard | UI flow: connect existing Bluesky account → export repo from source PDS → import to Bytescape PDS → sign PLC rotation → verify on network. | 2-3 weeks |
| **A-M2** | Mastodon migration wizard | UI flow: create alias on Bytescape AP instance → trigger Move from source instance → verify follower transfer. | 1-2 weeks |
| **A-M3** | Import existing archive | After migration, run existing archive import pipeline to pull full history into local vault. | 3-5 days (mostly wiring) |

#### Web Portal (Track B)

| ID | Task | Description | Effort |
|----|------|-------------|--------|
| **B-W1** | `RestDataProvider` | HTTP `fetch()` implementation of DataProvider interface, talking to server REST API. | 1-2 weeks |
| **B-W2** | Server REST API | Express/axum HTTP API serving same data as Tauri commands, for web clients. Auth via JWT. | 2-3 weeks |
| **B-W3** | Web SPA build | `pnpm build:web` — same React app compiled with `RestDataProvider`, deployed as static assets behind Caddy. | 3-5 days |
| **B-W4** | WASM visitor portal | Read-only public profile viewer using Iroh WASM client to fetch public blobs. Blurred preview for non-authenticated visitors. | 2-3 weeks |

#### Self-Host Packaging (Track I)

| ID | Task | Description | Effort |
|----|------|-------------|--------|
| **I-SH1** | One-command installer | `curl | bash` installer (similar to official PDS installer) — sets up Docker Compose, configures DNS prompts, generates secrets, starts stack. | 1 week |
| **I-SH2** | Admin CLI | `bytescape-admin` CLI for account management, invite codes, relay status, backup/restore. | 1-2 weeks |
| **I-SH3** | Backup/restore | Automated backup of Postgres, Iroh blob store, and config to local path or S3. Restore from backup. | 1 week |

#### DID Verifiability Upgrade (Phase 6+ — Track A)

| ID | Task | Description | Effort |
|----|------|-------------|--------|
| **A-DV1** | did:webvh witnesses | Add optional witness infrastructure — third-party services that countersign did:webvh log entries. Evaluate hosted witness services or deploy custom witness nodes. | 1-2 weeks |
| **A-DV2** | did:webvh pre-rotation | Enable pre-rotation key commitment in the did:webvh log. Adds post-quantum resistance to key rotation. | 3-5 days |
| **A-DV3** | `WebsVerifier` implementation | Implement `IdentityVerifier` trait using KERI stack (when Rust did:webs resolver matures). Generates `did.json` + `keri.cesr` artifacts. | 4-8 weeks |
| **A-DV4** | Parallel publishing | Run both `WebvhVerifier` and `WebsVerifier` simultaneously — publish `did.jsonl` AND `keri.cesr` during transition period. Zero user-facing changes. | 1 week |
| **A-DV5** | Retire did:webvh | Once all verifiers support did:webs, stop publishing `did.jsonl`. Remove `WebvhVerifier`. | 2-3 days |

---

## 10. What This Replaces in the Current Roadmap

| Current Roadmap Item | Status After This Document |
|----------------------|---------------------------|
| Phase 4 Convergence → Hosting Milestone | **Expanded** → Phase 4.5 (full server foundation, not just escrow relay) |
| Phase 5 → Station Protocol | **Retained** but moved to Phase 5 second half (after mobile/web portal) |
| Phase 5 → Paired Streams/Forte Server | **Replaced** by GoToSocial + custom Sync Bridge |
| Phase 5 → Gossip Presence | **Retained** — implemented via Iroh relay |
| Phase 5 → Stealth Mode | **Retained** — toggle relay ↔ direct P2P |
| Phase 6 → Email Engine | **Unchanged** — remains Phase 6, benefits from MX infrastructure in Phase 4.5 |
| Open Issues → Identity Architecture | **Subsumed** by A-S1 (identity migration task in Phase 4.5) |
| Open Issues → `generate_platform_plc` | **Removed** by A-S1 |

---

## 11. Agent Action Directives

### For Backend Agent (Track A)

**Phase 4.5 scope:**
1. **A-S1 (Identity migration):** Remove `generate_platform_plc()` from `keychain_encrypted.rs`. Migrate all `LinkedAccount.identifier` values from fabricated `did:plc` to platform-native identifiers (`acct:user@instance` for Mastodon, actor URL for AP). Simplify `get_owner_did()` to 3-level hierarchy. Add migration function that runs on startup, converts existing data. Write tests.
2. **A-S1b (IdentityVerifier + WebvhVerifier):** Implement the `IdentityVerifier` trait as defined in §2. First implementation uses `didwebvh-rs` crate. This is the **abstraction boundary** that prevents technical debt — all DID verifiability operations go through this trait, so swapping from did:webvh to did:webs later requires only a new trait implementation, no changes to callers. **Critical UX rule: no user-facing surface ever displays `did:webvh` or `did:webs` identifiers.** Users see only `did:web` (in advanced UI) or their handle (everywhere else).
3. **A-S2 (Relay sync commands):** Implement Tauri commands for pushing/pulling encrypted blobs to the Iroh relay. The relay protocol should use the existing `bytescape/1.0` ALPN with new message types for relay operations.
4. **A-S3/A-S4 (Account provisioning):** Implement XRPC client calls to provision accounts on the hosted PDS and API calls to provision actors on the custom AP server. Store returned credentials in keychain.

**Important constraints:**
- Do not modify frontend files (`src/`). Coordinate with Frontend via PM.
- Run `cargo check` + `pnpm test -- --run` before every commit.
- Check `git log --oneline -5 -- <filepath>` before editing any shared file.

### For Frontend Agent (Track B)

**Phase 4.5 scope:**
1. **B-S1 (DataProvider interface):** Define a TypeScript interface that abstracts all data operations currently done via `invoke()`. Implement `TauriDataProvider` as a thin wrapper around existing invoke calls. This is a refactor — no behavior changes, just indirection. Do NOT implement `RestDataProvider` yet (Phase 5).
2. **B-S2 (Server account setup UI):** Add "Bytescape Social" section to SettingsScreen — shows PDS/AP account status, provisioning buttons, migration status.
3. **B-S3 (Relay indicator):** Status badge in sidebar or top bar showing relay connection state.

**Important constraints:**
- Do not modify Rust files (`src-tauri/`). Coordinate with Backend via PM.
- The DataProvider refactor must be backwards-compatible — existing screens must work identically with `TauriDataProvider`.
- **DID display rule:** No UI surface ever shows `did:webvh:...` or `did:webs:...` identifiers. Users see their **handle** (`alice.bytescape.social`) in all normal UI. An optional "Advanced" or "Developer" section in Settings may show `did:web:bytescape.social:alice` for power users. The verifiable DID layer is invisible to end users.

### For Infrastructure Agent (NEW)

**Phase 4.5 scope:**
1. **I-1 through I-7:** Deploy the Docker Compose stack on the developer's self-hosted hardware. Configure DNS for `bytescape.social`, Cloudflare Tunnel or Tailscale Funnel for ingress, TLS via Caddy, Postgres, Tranquil PDS, custom axum AP server, Iroh relay, Sync Bridge, identity resolver.
2. **Test with developer account first** — migrate the developer's Bluesky and Mastodon accounts to `bytescape.social` before inviting others.
3. **Document the deployment** in a `server/README.md` with setup instructions that work identically for self-hosters and cloud VPS.

**Important constraints:**
- The server stack must be fully reproducible via `docker compose up`. No manual configuration steps that aren't scripted.
- All secrets (JWT keys, admin passwords, PLC rotation keys) generated at deploy time, never committed to git.
- Caddy handles TLS — no manual certificate management.

---

## 12. Design Decisions (Finalized)

1. **PDS choice: Tranquil PDS.** Rust-native, Postgres-backed, `did:web` support built in, single binary with no Node.js runtime. Aligns with the Rust stack. Falls back to official PDS only if Tranquil proves too immature during testing. Repo: `tangled.org/tranquil.farm/tranquil-pds`.
2. **AP instance choice: Custom (extend existing AP module).** The existing AP module (8 files, 1,636 lines) will be extended into a full server-side AP instance using `axum`. This provides maximum flexibility and keeps the entire stack in Rust. GoToSocial remains a documented fallback if the custom route proves too slow. Note: `megalodon-rs` (referenced in TECHNICAL_SPEC §7.1) is a **client library** for talking to Mastodon-API-compatible servers — it is useful for the Tauri app's client-side Mastodon interactions, but is not an AP server implementation and is not relevant to the server-side AP instance.
3. **Initial deployment: Self-hosted.** `bytescape.social` will be hosted on the developer's own hardware for initial testing with a small number of users. It federates identically to how the final self-hosted relay/instance will work. Once proven out, the same Docker Compose stack scales to a commercial cloud provider (TBD based on observed load). The architecture must be cloud-agnostic — no managed-service dependencies.
4. **Invite-only launch.** Both Tranquil PDS (`PDS_INVITE_REQUIRED=true`) and the custom AP instance will require invite codes for account creation. This allows controlled growth and infrastructure capacity planning before opening registration.
5. **Custom domain handles: Yes.** Users can bring their own domain as both an AT Protocol handle (`@alice.com` resolved via `/.well-known/atproto-did`) and a fediverse handle (`@alice@alice.com` via WebFinger delegation). **This is a core differentiator** — users' indieweb home pages become their fediverse identity. Implementation requires:
   - AT Proto: DNS TXT record or `/.well-known/atproto-did` on user's domain → PDS handle verification
   - ActivityPub: WebFinger delegation — user's domain `/.well-known/webfinger` redirects or proxies to `bytescape.social`'s WebFinger endpoint
   - DID Document: `did:web:{user-domain}` can be user-controlled, with `alsoKnownAs` linking to their `bytescape.social` identities
   - UI: Settings screen section for custom domain setup with DNS verification wizard
6. **DID verifiability: did:webvh first, did:webs later.** Users only ever see `did:web` identifiers (or their handle). Behind the scenes, `did:webvh` (DIF, v1.0, Rust library available) provides verifiable history, hash-chained logs, and SCID-based portability starting in Phase 4.5. A Rust `IdentityVerifier` trait abstracts all verifiable DID operations, enabling a future swap to `did:webs` (ToIP/GLEIF, KERI-based) when enterprise features (multisig, delegation, ACDC credentials) are needed and a Rust implementation matures. Both methods publish a standard `did:web` document in parallel, so AT Protocol, ActivityPub, and all existing resolvers work unchanged at every phase. The transition from webvh to webs requires only a new trait implementation — zero frontend changes, zero user-facing changes, zero protocol changes.

---

*This document supersedes the "Dual-Engine Architecture" proposal from the external agent analysis. It preserves the sound ideas (relay for mobile, adapter pattern, translation daemon) while correcting the phasing, privacy model, and identity architecture to build on the existing 35,000+ line codebase rather than starting over.*
