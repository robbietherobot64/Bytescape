# Architecture Decisions

These decisions were made before writing any code. They guide all implementation choices.

> **Living record:** decisions superseded by later calls keep their original
> text with a dated banner — history is not rewritten. Product-level decisions
> live in `VISION.md` / `DECISIONS.md`.

> **Note (2026-10-10):** this is the original decision record. Where implementation has
> diverged from a decision (PDS-first storage for imports/community is partial; AD-02's
> module tree and AD-05/AD-06's tooling lists drifted), the current truth is summarized
> in [`README.md`](./README.md) and the code itself. Specs elsewhere citing **AD-07,
> AD-12, AD-13** refer to decisions that were never recorded here — treat as TBD.

---

## AD-00: AT Protocol Only — No Mastodon/ActivityPub

> **Superseded (2026-11-02):** Bytescape is now a **dual-protocol platform** —
> native ActivityPub alongside AT Protocol (the Wafrn model). See
> [`ap-pivot-plan.md`](ap-pivot-plan.md); the active build is
> `openspec/changes/add-activitypub-integration`. Kept below as the original
> rationale for the AT-first era.

**Status:** Accepted
**Date:** 2026-08-03

### Decision

Bytescape is an **AT Protocol-native application**. We do not implement Mastodon or ActivityPub integration. If users want Mastodon bridging, they use external tools like Bridgy Fed.

### Why

- Focus on being the best AT Protocol app, not a mediocre multi-protocol app
- Mastodon/AP integration was a source of significant complexity in the old codebase
- AT Protocol's federation model (PDS → relay → AppView) is the right architecture for what we're building
- Bridgy Fed and similar tools handle cross-protocol bridging better than we can

---

## AD-01: PDS-First Write Model

**Status:** Accepted
**Date:** 2026-08-03

### Decision

All content — both standard AT Protocol records and Bytescape-specific records — writes to the user's PDS repo first via a signed MST commit. Postgres is a **derived index** populated by the firehose consumer.

### How it works

```
User creates a post
  → API handler determines user's PDS location (Bytescape-hosted or bsky.social)
  → Writes app.bsky.feed.post record to the PDS (signed commit or XRPC call)
  → Commit emits firehose event
  → Firehose consumer processes event, upserts into pds_indexed_records
  → Handler also does a direct index insert (fast-path for read-after-write)
  → Returns to client with AT URI + CID
```

### What about imported content?

Imported content (Facebook/Instagram/Threads archives) IS stored in the user's PDS as **custom lexicon records** (e.g., `com.bytescape.archive.post`). This means:
- The data is in the user's PDS repo (portable, verifiable)
- It federates via the firehose (other AT Protocol apps could display it)
- Bluesky ignores it (it doesn't understand `com.bytescape.*` lexicons)
- Bytescape's AppView indexes and displays it

### Read-after-write consistency

After a PDS write, the handler does a **direct insert into `pds_indexed_records`** as a fast-path. This ensures the client can read the record immediately without waiting for the firehose consumer (which has a ~100ms poll interval). The firehose consumer will eventually reconcile the index with the PDS state.

---

## AD-01b: Dual PDS Model — Bytescape-Hosted and Bluesky-Hosted

**Status:** Accepted
**Date:** 2026-08-03

### Decision

Bytescape supports two types of users:

**Type 1: Bytescape-hosted user** — Signs up directly on Bytescape. We create a `did:web` DID, host their PDS repo, manage their signing key. We can read and write to their repo directly.

**Type 2: Bluesky-hosted user** — Signs in via Bluesky OAuth. Their DID is from bsky.social (e.g., `did:plc:abc123`). Their repo lives on bsky.social's PDS. We read via the firehose, write via XRPC calls using their OAuth token.

### How writes work for each type

| Action | Bytescape-hosted | Bluesky-hosted |
|--------|-----------------|----------------|
| Create post | Direct PDS write (signed commit) | XRPC call to bsky.social with user's token |
| Like a post | Direct PDS write | XRPC call to bsky.social |
| Follow someone | Direct PDS write | XRPC call to bsky.social |
| Import archive | Direct PDS write (custom lexicon) | XRPC call to bsky.social (custom lexicon) |
| Edit profile | Direct PDS write | XRPC call to bsky.social |

### OAuth scopes needed

For Bluesky-hosted users, we request these scopes:
- `atproto transition:generic` — basic access
- `repo:app.bsky.feed.post?action=create` — create posts
- `repo:app.bsky.feed.like?action=create` — create likes
- `repo:app.bsky.graph.follow?action=create` — create follows
- `repo:com.bytescape.*?action=create` — create Bytescape-specific records

### Why both?

- **Bytescape-hosted:** Users who want a fresh start, don't have a Bluesky account, or want Bytescape to be their primary PDS
- **Bluesky-hosted:** Existing Bluesky users who want to use Bytescape without creating a new identity. Their content appears on both platforms.

---

## AD-01c: Lexicons — Use Existing Standards Where They Exist

**Status:** Accepted
**Date:** 2026-08-03

### Decision

Bytescape uses existing community/standard lexicons when they exist, and defines `com.bytescape.*` lexicons only for data types with no existing standard.

### Lexicon strategy

| Data Type | Lexicon | Source | Notes |
|-----------|---------|--------|-------|
| Posts | `app.bsky.feed.post` | Standard | Visible on Bluesky + Bytescape |
| Likes | `app.bsky.feed.like` | Standard | Visible on Bluesky + Bytescape |
| Follows | `app.bsky.graph.follow` | Standard | Visible on Bluesky + Bytescape |
| Profile | `app.bsky.actor.profile` | Standard | Visible on Bluesky + Bytescape |
| Events | `community.lexicon.calendar.event` | Community (atmo.rsvp) | Interoperable with atmo.rsvp, Smoke Signal |
| Event RSVPs | `community.lexicon.calendar.rsvp` | Community (atmo.rsvp) | Interoperable with atmo.rsvp |
| Imported archives | `com.bytescape.archive.post` | Custom | Bytescape-specific (no existing standard) |
| Marketplace | `com.bytescape.exchange.listing` | Custom | Bytescape-specific (no existing standard) |
| Forums | **Hold** | — | Wait for `chat.bsky.group` (lexicons exist in Bluesky repo, no public API yet) |
| Profile widgets | `com.bytescape.widget.*` | Custom | Bytescape-specific (no existing standard) |
| Group chat | `space.bytescape.*` | Custom (replicating Roomy structure) | Spaces, rooms, threads, messages |

### Why use community lexicons for events?

- **atmo.rsvp** (`github.com/flo-bit/atmo-events`) is an open source event app using `community.lexicon.calendar.event` and `community.lexicon.calendar.rsvp`
- **Smoke Signal** (`smokesignal.events`) indexes 14,120+ events from the AT Protocol network
- Using the same lexicons means Bytescape events are discoverable by these apps (and vice versa)
- Users aren't locked into Bytescape for their event data

### Why hold on forums?

The `chat.bsky.group` lexicons already exist in the official Bluesky repo (`lexicons/chat/bsky/group/`):
- `createGroup`, `editGroup`, `addMembers`, `removeMembers`
- `requestJoin`, `approveJoinRequest`, `rejectJoinRequest`
- `createJoinLink`, `editJoinLink`, `enableJoinLink`, `disableJoinLink`
- `listJoinRequests`, `listMutualGroups`, `withdrawJoinRequest`

These are under `chat.bsky.group` — part of the chat system, not the main social graph. There's no public API yet (a June 2026 discussion asking about release date got no reply). Building our own `com.bytescape.forum.*` now risks being incompatible with what Bluesky ships.

**Recommendation:** Wait for `chat.bsky.group` public API. Build Bytescape forums on top of it when it ships.

### Why no Mobilizon?

> **Update (2026-11-02):** Mobilizon (and Gancio) are now relevant as *sources*
> — events federation rides the ActivityPub pivot; see
> [`ap-pivot-plan.md`](ap-pivot-plan.md).

Mobilizon is an ActivityPub platform. We are AT Protocol only (AD-00). No ActivityPub integration.

### How custom lexicons work

1. Define the lexicon schema (JSON-LD)
2. Store records using `com.atproto.repo.createRecord` with the appropriate collection
3. bsky.social's PDS stores the records (it doesn't validate against unknown lexicons)
4. Bytescape's AppView indexes them (it understands the lexicon)
5. Bluesky ignores records it doesn't understand

### Why store in PDS instead of Postgres?

- **Data sovereignty:** Users own their imported data in their PDS repo
- **Portability:** If a user migrates to another PDS provider, their imported archives go with them
- **Federation:** Other AT Protocol apps could build on `com.bytescape.*` lexicons
- **Consistency:** Everything is a PDS record — no dual data model

---

## AD-01d: Chat — Bluesky DMs First, Roomy-Style Groups Second

**Status:** Accepted
**Date:** 2026-08-03

### Decision

Bytescape implements chat in three layers:

**Layer 1: Bluesky DMs (first priority)**
- Use the existing `chat.bsky.convo.*` API
- Bytescape acts as a chat client (like Bluesky's app, but ours)
- Messages stored by Bluesky's chat service (server-to-server)
- NOT end-to-end encrypted (yet — Bluesky is working on this)
- Already works — Bluesky users can DM each other today

**Layer 2: Roomy-style group chat (second priority)**
- Rooms with channels and threads
- Built on AT Protocol using `space.bytescape.*` lexicons (replicating Roomy's `space.roomy.*` structure)
- Reference: [Roomy](https://a.roomy.space/) (`github.com/muni-town/roomy`) — open source AT Protocol group chat, 467 stars
- Roomy's data model: Space → Room → Thread → Message
- Roomy's lexicons: `space.roomy.space.*`, `space.roomy.room.*`, `space.roomy.message.*`, `space.roomy.user.*`, `space.roomy.sync.*`
- Our lexicons: `space.bytescape.*` (same structure, our own namespace)
- Using our own namespace so we can evolve independently without depending on Roomy's implementation
- More structured than simple group chat — channels, threads, knowledge artifacts

**Layer 3: Bluesky groups (when available)**
- Use `chat.bsky.group.*` when the public API ships
- Lexicons already exist in the official repo (`lexicons/chat/bsky/group/`)
- No public API yet (as of August 2026)
- Likely interoperable with Bluesky's native group feature

### Why this order?

- **Bluesky DMs first:** Works today, integrates with existing Bluesky users, lowest implementation effort
- **Roomy-style groups second:** Differentiates Bytescape from Bluesky's native chat, supports community building
- **Bluesky groups later:** Wait for the public API, build on top of it rather than competing with it

### Reference implementations

- **Bluesky DMs:** `chat.bsky.convo.*` lexicons in `bluesky-social/atproto`
- **Roomy:** `github.com/muni-town/roomy` — open source AT Protocol group chat with rooms, channels, threads
- **Bluesky groups:** `chat.bsky.group.*` lexicons in `bluesky-social/atproto` (no public API yet)

---

## AD-02: Single Binary, Clear Module Boundaries

**Status:** Accepted

### Decision

One Axum binary with clear internal module separation:

```
bytescape-server/src/
├── main.rs              # Bootstrap, route mounting
├── config.rs            # AppConfig from env vars
├── error.rs             # AppError enum
├── state.rs             # AppState (shared across handlers)
├── pds/                 # Bytescape-hosted PDS (for our users)
│   ├── mod.rs           # XRPC endpoints
│   ├── account.rs       # Account creation, key management
│   ├── storage.rs       # SqlxPostgresStorage (RepoStorage impl)
│   ├── did_doc.rs       # DID document hosting
│   ├── firehose_emit.rs # Commit → firehose event
│   └── firehose_consumer.rs # Firehose → Postgres index
├── appview/             # Derived index + query APIs
│   ├── consumer.rs      # Relay firehose consumer (processes ALL events)
│   ├── endpoints.rs     # AppView XRPC endpoints
│   └── handle_resolver.rs # DID → handle cache
├── api/                 # REST API for frontend
│   ├── mod.rs           # Route registration
│   ├── auth.rs          # Signup, login, refresh, email verify, Bluesky OAuth
│   ├── profile.rs       # Profile CRUD, widgets, wall
│   ├── feed/            # Feed aggregation (multiple files)
│   ├── import.rs        # Archive import → PDS records
│   ├── posts.rs         # Post create/delete (routes to PDS or Bluesky API)
│   ├── events.rs        # Events (custom lexicon records)
│   ├── forums.rs        # Forums (custom lexicon records)
│   ├── exchange.rs      # Marketplace (custom lexicon records)
│   └── ...              # Other features
├── auth/                # Auth primitives
│   ├── jwt.rs           # JWT create/validate
│   ├── password.rs      # Argon2 hash/verify
│   ├── refresh_token.rs # Token family rotation
│   └── email_verification.rs
├── db/                  # Database access layer
│   ├── mod.rs           # Pool, migrations
│   ├── users.rs         # UserRow
│   └── ...              # One file per table
├── bluesky/             # Bluesky API client (for Bluesky-hosted users)
│   ├── client.rs        # XRPC client with DPoP
│   ├── oauth.rs         # Bluesky OAuth flow
│   └── write.rs         # Write records to bsky.social PDS
└── lexicons/            # Custom lexicon definitions
    └── mod.rs           # com.bytescape.* schema definitions
```

### Dependency rules

- `api/` can call `pds/`, `db/`, `auth/`, `oauth/`
- `pds/` can call `db/`
- `appview/` can call `db/`
- `auth/` is standalone (no dependencies on other modules)
- `db/` is standalone (only depends on sqlx)
- No circular dependencies.

---

## AD-03: did:web Only (for now)

> **Clarified:** this covers *Bytescape-hosted* accounts. Bluesky-linked
> accounts use `did:plc` — see AD-01b.

**Status:** Accepted

### Decision

Use `did:web:{handle}.{domain}` exclusively. No `did:plc` support.

### Why

- `did:web` maps directly to HTTP: `did:web:alice.bytescape.social` resolves at `https://alice.bytescape.social/.well-known/did.json`
- No external dependency (PLC directory)
- Simple to implement and debug
- `did:plc` deferred to post-1.0 (requires PLC directory registration and update signing)

---

## AD-04: Error Handling

**Status:** Accepted

### Decision

Single `AppError` enum with `thiserror`. All handlers return `Result<Json<T>, AppError>`.

### Variants

| Variant | HTTP Status | When to use |
|---------|-------------|-------------|
| `Auth(String)` | 401 | Invalid credentials, expired token |
| `NotFound(String)` | 404 | Resource doesn't exist |
| `BadRequest(String)` | 400 | Validation failure, bad input |
| `Conflict(String)` | 409 | Duplicate resource |
| `Forbidden(String)` | 403 | Insufficient permissions |
| `Database(sqlx::Error)` | 500 | DB errors (logged, generic message returned) |
| `OAuth(String)` | 400/401/502 | OAuth-specific errors |
| `Internal(String)` | 500 | Unexpected errors (logged) |

---

## AD-05: Testing Strategy

**Status:** Accepted

### Decision

- **Unit tests** in each module (`#[cfg(test)]` blocks) for pure logic
- **Integration tests** in `tests/` directory requiring a running Postgres
- **Target:** Every public function in `db/`, `auth/`, `pds/` has at least one test
- **Frontend:** Vitest + React Testing Library for component tests

### Test database

Integration tests use a dedicated test database (`bytescape_test`). Tests create and tear down their own data. No shared state between tests.

---

## AD-06: Dependency Choices

**Status:** Accepted

### Keep from old codebase

| Crate | Purpose |
|-------|---------|
| axum 0.8 | Web framework |
| tokio | Async runtime |
| sqlx 0.8 | Database (Postgres) |
| serde/serde_json | Serialization |
| jsonwebtoken | JWT |
| argon2 | Password hashing |
| secp256k1 | PDS repo signing |
| ed25519-dalek | DPoP proof signing |
| reqwest | HTTP client |
| chrono | Time handling |
| uuid | ID generation |
| tracing | Logging |
| zip | Archive extraction |
| scraper | HTML parsing (imports) |
| rss/atom_syndication | RSS feed parsing |
| lettre | Email sending |
| aws-sdk-s3 | S3 storage |
| regex | Pattern matching |
| image | Image processing |
| base64/multibase | Encoding |

### Drop

| Crate | Reason |
|-------|--------|
| sea-orm | Never used — all DB access via sqlx |
| atrium-rs | Never imported — used direct reqwest calls instead |
| atom_syndication | No RSS/Mastodon integration |

### Add

| Crate | Purpose |
|-------|--------|
| tower-http | CORS, compression, tracing middleware |
| testcontainers | Integration test database setup |
