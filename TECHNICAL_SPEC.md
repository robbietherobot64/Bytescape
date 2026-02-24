# Bytescape Technical Specification

This document aggregates the core technical specifications for Bytescape: Architecture, System Design, Storage Boundaries, and Security.



---

# ARCHITECTURE


Bytescape is a local-first, peer-to-peer (P2P) social data vault and unified communication client.

**Core Philosophy:** "Low Floor, High Ceiling."
- **Low Floor:** Zero-configuration, cloud-like convenience for average users.
- **High Ceiling:** Fully independent, self-hosted and/or off-grid p2p capabilities for power users.

## 1. System Architecture: Engine & Lens

The system follows an unbundled design, separating heavy data processing from the user interface.

### 1.1 Bytescape Bridge (The Engine)
A headless backend service (`bytescape-bridge`) running locally on the user's device.
- **Role:** Manages the local database, P2P networking (`iroh`), cryptographic keys, and protocol bridges.
- **Capabilities:**
    - **Universal Ingestion:** Scrubs and normalizes data from Bluesky, ActivityPub, RSS, and legacy archives (Facebook/Twitter JSON).
    - **P2P Transport:** Encrypted communication via Iroh.
    - **Storage:** Manages the "Vault" (universal content store) using Willow-compatible data structures.

### 1.2 Bytescape (The Frontends)
Lightweight UI clients that plug into the Bridge.
- **Role:** Query the local database and render data through specific "Lenses".
- **Lenses:** Specialized views for different content types (e.g., a "Chat Lens" for DMs, a "Gallery Lens" for photos, a "Reader Lens" for articles), unified under a multifunctional application shell initially, with single function spinoff apps down the line.
- **Tech Stack:** Tauri v2 (Desktop/Mobile), React, TypeScript.

## 2. Data Architecture: The Willow Vault

All data is normalized into a hierarchical, path-based structure inspired by the [Willow Protocol](https://willowprotocol.org/). This enables file-manager-style browsing and P2P sync.

### 2.1 Willow Data Model

Data is organized in a 3-dimensional coordinate system:

| Dimension | Purpose | Bytescape Usage |
|-----------|---------|-----------------|
| **Namespace** | Top-level vault isolation | One per user vault |
| **Subspace** | Per-author data separation | User's own content vs imported sources |
| **Path** | Hierarchical addressing | `/{source}/{type}/{year}/{month}/{id}` |
| **Timestamp** | Version ordering | Original post timestamp (microseconds) |

**Path Schema:**
```
/{source}/{item_type}/{year}/{month}/{content_hash}

Examples:
/bluesky/status/2024/06/abc123...
/facebook/gallery/2023/12/vacation/def456...
/import/article/2022/03/ghi789...
```

**Rust crate:** `willow_25` from [earthstar-project/willow-rs](https://github.com/earthstar-project/willow-rs) provides ready-to-use types (Entry, Path, Area) with ed25519 signatures and BLAKE3 hashing.

### 2.2 The BytescapeItem Schema
The Vault stores three primary data types, each mapped to Willow entries:

1.  **Short Text (Status):** Microblogging posts (Bluesky, Mastodon).
2.  **Media Content (Gallery):** Visual posts (Instagram, Facebook Albums).
3.  **Link/Metadata (Article):** Long-form content (RSS, Substack).

Each item includes:
- `path`: Hierarchical location (`/{source}/{type}/{date}/{id}`)
- `subspace_id`: Author identity (user's key or source identifier)
- `timestamp`: Original creation time (microseconds)
- `payload_digest`: Content hash (for media: Iroh blob hash)
- `syndication_status`: `LocalOnly` | `PendingSyndication` | `Syndicated { at_uri }`

### 2.3 The Data Vault UI

The Vault is presented as a **file-manager interface**:
- **Browse by Source:** Folders for Facebook, Bluesky, Instagram, etc.
- **Browse by Type:** Filters for Status, Gallery, Article
- **Browse by Date:** Year → Month → Day hierarchy
- **Rich Media Preview:** Thumbnails, video playback, article rendering
- **Export:** Any item or folder can be exported as standard formats

### 2.4 Ingestion Pipeline
- **Live Bridge:** Real-time fetching from connected networks (Bluesky, ActivityPub).
- **Archive Importer:** Offline extraction for GDPR exports (Facebook JSON, etc.). 
  - Chunks media into Iroh blobs
  - Generates WebP thumbnails
  - Assigns Willow paths based on source and date
  - Default syndication: `LocalOnly`

### 2.5 Spatial Profile Discovery
Bytescape replaces algorithmic discovery with **spatial, graph-based exploration** — navigating the decentralized web the way hyperlinks worked on the old web.

- **Profile Graph Traversal:** Each user profile contains a manually curated list of links to other profiles, webrings, and communities. Users discover new people by following these links, not by algorithmic injection.
- **No Trending, No Explore:** The system has no discovery feed. Finding new people requires actively visiting profiles, subscribing to a hashtag, or following a webring link.
- **Webring Navigation:** Users publish a `com.platform.webring.member` Lexicon record to their local PDS pointing to a shared AT-URI. AppView indexers (see §7.3) stitch these independent records into a navigable loop or unified forum feed.
- **Network State Events:** The unified feed also surfaces **network state changes** — a contact updating their profile theme, joining a new webring, or publishing a new archive import — creating ambient awareness of the social graph without algorithmic injection.

## 3. Syndication & Audience Control

### 3.1 Syndication Model

Items have explicit syndication status to prevent accidental spam:
- **LocalOnly:** Stored in vault, never posted externally (default for imports)
- **PendingSyndication:** Queued for posting
- **Syndicated:** Posted to external network (stores AT URI or ActivityPub ID)

**Import Behavior:**
- Archives import as `LocalOnly` by default
- User can opt-in to "Post to Wall on Import" which queues items as `PendingSyndication`
- Syndication happens via background job, respecting rate limits
- Consider a "Memory" post type that signals "this is from my archive"

**⚠️ Bluesky Note:** Backdating `createdAt` doesn't prevent posts from appearing in followers' timelines *now*. True backfill without notification may require `com.atproto.repo.applyWrites` batch operations or explicit "Memory/Archive" post semantics.

### 3.2 Audience Circles

Posts can be scoped to specific audiences:
- **Public:** Syndicated to all connected networks
- **P2P Only:** Shared only with direct P2P contacts
- **Circles:** User-defined groups (e.g., "Close Friends", "Family", "Work")

### 3.3 Contextual Syndication (Smart Publisher)

Outbound posts are routed based on content type:
- **Short Text:** Broadcasts natively to Bluesky/Mastodon
- **High-Res Media:** Stored locally via P2P, "Keyhole" preview (low-res thumbnail + secure link) syndicated externally
- **Long-Form:** Posted as Article with preview card

## 3.4 Profile Generation (The Builder)

Every Bytescape user owns a **sovereign static profile** — a customizable mini-website served from their Willow store.

- **Visual Builder (GrapesJS):** A drag-and-drop module builder generates clean HTML/CSS for non-technical users. Widgets include away message, music player, friends list, and guestbook.
- **Raw Override:** Advanced users upload custom HTML/CSS/JS directly to their local Willow directory. Custom code runs in a sandboxed `<iframe>` to prevent XSS.
- **Dual-Format Serving:** The profile URL serves HTML to human visitors and structured data (Atom feed / AT Protocol Lexicons) simultaneously to feed aggregators. Backend consumers autodiscover the machine-readable endpoint via `<link rel="alternate">` in the HTML `<head>`.
- **DID Discovery via `<meta>` tags:** The profile embeds `<meta name="bytescape:did" content="did:plc:...">` so that the Bytescape client can resolve the owner's identity and populate cross-platform subscription links (ActivityPub, AT Protocol, RSS).
- **Aesthetic:** Profiles are visually inspired by MySpace/iGoogle — warmth, personality, tactile Aero-Skeuomorphic design language.

## 3.5 Peer Profile Browser

The Bytescape client acts as a **specialized browser** for traversing the P2P profile graph.

- **Browser Shell:** Manages identity resolution, Willow/Iroh sync, and the unified toolbar.
- **Sandbox `<iframe>`:** When visiting a peer's profile, the custom HTML/CSS/JS renders in an isolated frame to prevent malicious code affecting the host application.
- **`<meta>` Tag Extraction:** On each profile visit, the shell reads the embedded meta tags to surface the owner's DID, their followed hashtags, and available subscription endpoints.
- **Swarm Caching:** High-traffic profiles distribute load organically — visitors temporarily cache and re-serve the cryptographically signed profile data they view. Optional dedicated **Pub nodes** ensure 24/7 availability for users who want guaranteed uptime without self-hosting.
- **NAT Traversal:** Iroh handles direct QUIC connections with automatic NAT traversal and relay fallback.

## 4. Networking: "The People's Cloud"

Bytescape replaces centralized servers with a location-aware P2P mesh.

### 4.1 Topology
- **Sovereign Nodes (Users):** The primary source of truth. Data lives on your device.
- **Community Stations:** Encrypted "dumb stores" and Iroh relays.
    - **Role:** Provide 24/7 presence and bandwidth.
    - **Privacy:** Stations store *encrypted* shards; they cannot read user data.
    - **Auto-Docking:** Geohash-based discovery of nearest high-uptime Station.

### 4.2 Stealth Mode
Users can toggle "Stealth Mode" to sever all Station connections, relying purely on direct device-to-device Iroh connections or pairing with a self-hosted `bytescape-station` binary.

### 4.3 Resilience
- **Neighborhood Erasure Coding:** Data is sharded across the home station and regional neighbors. If a node fails, data can be rebuilt from parity shards.

## 5. Identity & Access

### 5.1 The "Two Doors" Flow
1.  **Door 1 (Verified Sovereign):**
    - Create a local DID.
    - "Blind Escrow" recovery: Encrypted key shard stored on a server, unlockable only with Master Password + 2FA.
2.  **Door 2 (OAuth Bridge):**
    - Log in with existing Bluesky/ActivityPub identity.
    - Instantly import social graph and establish sync.

### 5.2 Meadowcap Capabilities
Access control uses capability-based permissions (inspired by Willow's Meadowcap):
- Read/write permissions per namespace, subspace, or path prefix
- Delegatable capabilities for sharing specific folders

## 6. User Experience: The Library Model

Control shifts from the platform to the user.

### 6.1 Anti-Patterns (What We Don't Do)
- **No Algorithmic Discovery:** Content only appears if explicitly subscribed
- **No Infinite Scroll:** Feeds have a definitive end; UI signals completion
- **No "Starter Packs":** Onboarding avoids mass-following strangers
- **Strict Chronology:** Timelines sort by timestamp, not engagement

### 6.2 The Feed: Six Tabs
The timeline is segmented to protect high-signal relationships from high-volume topics:

1.  **All (The River):** Merged view of every source — live timeline, custom feeds, vault items — sorted strictly by timestamp. Friend posts are visually elevated above topic posts.
2.  **Following (The Inner Circle):** Strict chronological feed of posts from followed DIDs. No algorithms, no boosts, no injected content.
3.  **Feeds:** Subscribed Bluesky custom feed generators, selectable individually or merged.
4.  **#Hashtags (The Window):** Posts matching subscribed hashtags across all bridged networks. Each tag has a "Volume Knob" (Quiet/Normal/Loud) to throttle high-traffic tags.
5.  **RSS:** Atom/RSS feed subscriptions from blogs, newsletters, and podcasts. Stored and served from the Willow vault.
6.  **Archives:** Read-only view of locally imported content (Facebook, Instagram, etc.). Filterable by source.

### 6.3 Profile Customization

User profiles are **customizable** in the spirit of MySpace/iGoogle:
- **Widgets:** Away message, music player, friends list, guestbook, etc.
- **Custom Layouts:** Drag-and-drop widget arrangement
- **Custom Themes:** HTML/CSS (sandboxed in iframes for security)
- **Public Portal:** Static blurred preview; full content requires P2P decryption via WASM Iroh node

### 6.4 Aesthetic: Tactile Aero-Skeuomorphism
- **Frutiger Aero:** Reviving the warmth and depth of mid-2000s design
- **Skeuomorphism:** Digital textures (paper, glass) for intuitive affordance
- **Glass & Depth:** Translucent panels with background blur (Windows Aero style)

### 6.5 Theme Engine
- **Cozy Mode (Default):** Soft rounded corners, warm palettes, serif typography
- **Desktop Mode (Win98):** High-density data views, monospace fonts, sharp bevels
- **Custom Themes:** User-created via CSS-like syntax or visual editor

### 6.6 Haptics & Audio
- **Mechanical Profile:** Crisp clicks for definitive actions (Save, Publish)
- **Organic Profile:** Soft natural sounds (wood taps, paper) for passive scrolling
- **Silent Profile:** Pure haptic vibration patterns

## 7. Federation & Interoperability

### 7.1 Protocol Adapters
- **AT Protocol:** `bsky-sdk` for Bluesky identity and data
- **ActivityPub:** `megalodon-rs` for Mastodon/Lemmy
- **RSS/Atom:** `feed-rs` + `readability` for blogs and newsletters

### 7.2 WASM Web Portal
Public HTTP profiles serve static, blurred previews. Full-fidelity content requires loading a WebAssembly Iroh node in the browser to decrypt the P2P stream.

### 7.3 Webrings & Decentralized Forums
Groups and affiliations operate as cryptographic records rather than hosted spaces.

- **User-Initiated Membership:** A user publishes a `com.platform.webring.member` Lexicon record to their local PDS, pointing to a shared AT-URI that represents the webring or forum.
- **AppView Indexing:** Dedicated indexer components (part of `bytescape-bridge`, mirroring the primary AT Protocol parser architecture) monitor the AT Protocol firehose for these specific record types and stitch independent member nodes together into a navigable loop or unified forum feed.
- **No Central Host Required:** The webring exists as a set of distributed records. Any member can run an indexer to reconstruct the full loop from the firehose. If all indexers go offline, the records still exist on each member's PDS and can be re-indexed at any time.


---

# SYSTEM DESIGN & DATA FLOWS


This document defines the end-to-end data flow and storage architecture for:

1. Bluesky/Facebook post cloning and timeline display
2. Media extraction and Vault storage/display
3. Messaging import + real-time chat + archived chat partitions
4. Hard deletion pathways (Clear Vault + account switch)

## 1) Data Sources

## Social post sources
- Bluesky API
  - Following timeline (`get_timeline`)
  - Saved custom feeds (`list_saved_feeds` + feed fetch)
- Facebook archive ZIP
  - Parsed posts + media references (`import_facebook_zip` → `commit_memories`)

## Messaging sources
- Bluesky DM API (live import/send)
  - `import_vault_messages`
  - `send_bsky_dm`
- Walled-garden archives (Facebook) — archived/read-only lane in UI model

## Media sources
- Remote media URLs discovered from Bluesky/Facebook connectors
- Local filesystem media extracted from archive ZIPs
- Blob-addressed media (`blob:<hash>`) in Iroh

## 2) Display Interfaces

## Feed screen tabs
- Following: people/profile timeline only (Bluesky following timeline)
- Feeds: all followed custom feeds combined, with dropdown to select one feed or all
- All: merged + deduplicated union of Following + Feeds

## Vault screen
- File-first media browser from Iroh Docs metadata (`vault/file/*`)
- Fullscreen media viewer with navigation and optional metadata panel

## Chat screen
- Live lane: Bluesky/Mastodon sources (interactive composer)
- Archived lane: walled-garden sources (read-only)

## 3) Storage Configuration

## Local index store (redb)
- `ITEMS_TABLE`: normalized posts (`BytescapeItem`)
- `VAULT_MESSAGES_TABLE`: imported/sent messages (`VaultMessage`) with `owner_did`
- `CONTACTS_TABLE`, `CONVERSATIONS_TABLE`, `FOLLOWED_HASHTAGS_TABLE`, etc.

## Sync/media store (Iroh)
- Iroh Blobs: binary media bytes (content-addressed)
- Iroh Docs: Vault file catalog entries (`vault/file/{id}` -> `VaultFile`)

## Ownership and isolation
- Session owner DID is the tenant boundary.
- Messages are saved with `owner_did` and queried by owner.
- Logout clears local tables and synced vault file metadata.

## 4) End-to-End Flow

## A. Post cloning + feed rendering
1. Fetch from Bluesky timeline and saved feeds, parse Facebook archive posts.
2. Normalize to `BytescapeItem` and save to `ITEMS_TABLE`.
3. Feed UI composes views:
   - Following = timeline
   - Feeds = `get_unified_feed` over selected/all followed feeds
   - All = merge(Following, Feeds), sort by timestamp, dedupe by URI/CID

## B. Media extraction to Vault
1. Import connectors detect media paths/URLs.
2. Media bytes are stored in Iroh Blobs (or resolved from `blob:` hash).
3. A `VaultFile` metadata entry is written to Iroh Docs.
4. Vault UI lists from `vault_list_files` and streams bytes via `vault_get_file_data`.

## C. Messages and chat partitioning
1. Bluesky DM import stores messages as `VaultMessage { owner_did, source, ... }`.
2. Chat UI groups conversations by source:
   - live: `bluesky`, `mastodon`
   - archived: everything else
3. Send composer is enabled only for live conversations.

## D. Deletion pathways
- Clear Vault button:
  - clears all local user-scoped redb tables
  - clears all Iroh vault file metadata entries
- Logout/account switch:
  - clears all local user-scoped redb tables
  - clears all Iroh vault file metadata entries
  - clears pending import buffer
  - deletes session tokens

## 5) Required Functions / Contracts

## Import and feed contracts
- `import_bluesky_posts`
- `import_facebook_zip`, `commit_memories`
- `get_timeline`
- `list_saved_feeds`
- `get_unified_feed(tags, feed_uris, limit)`
- `list_follows(actor)`

## Vault/media contracts
- `vault_add_file`, `vault_upload_file`, `vault_upload_files_from_paths`
- `vault_list_files`, `vault_get_file_data`, `vault_get_thumbnail`
- `vault_delete_file`, `vault_deduplicate_files`, `vault_clear_all_files`

## Chat contracts
- `import_vault_messages`
- `send_bsky_dm`
- `list_vault_messages` (owner-scoped)

## Deletion/auth contracts
- `clear_vault` (full wipe of local + sync data)
- `logout` (full wipe + session removal)

## 6) Current Guarantees

- Cross-account leakage prevention for chats via `owner_did` scoping + full wipe on logout.
- Feed semantics match requested model (Following / Feeds with selector / All merged).
- Vault ingestion from imports writes media into vault catalog for discoverability.
- Deletion pathways remove both index records and synced vault media catalog entries.

This document defines the storage boundaries between **redb** (local database) and **Iroh** (P2P sync) to prevent data inconsistency and clarify ownership.

## Overview

Bytescape uses a **dual-storage architecture**:

| Storage | Purpose | Sync | Location |
|---------|---------|------|----------|
| **redb** | Local indexes, cache, user preferences | ❌ No sync | `$APPDATA/db.redb` |
| **Iroh Blobs** | Media files (images, videos) | ✅ P2P synced | `$APPDATA/sync/blobs/` |
| **Iroh Docs** | Vault file metadata, P2P documents | ✅ P2P synced | `$APPDATA/sync/docs.db` |

---

## Storage Decision Matrix

### What Goes Where

| Data Type | Storage | Rationale |
|-----------|---------|-----------|
| **User sessions** (JWT, DID) | redb (keychain file) | Local-only, security-sensitive |
| **Iroh node keys** | redb (keychain file) | Local-only, security-sensitive |
| **Bluesky posts** (imported) | redb `ITEMS_TABLE` | Local archive, fast queries |
| **Facebook posts** (imported) | redb `ITEMS_TABLE` | Local archive, fast queries |
| **P2P messages** | redb `MESSAGES_TABLE` | Local cache of received messages |
| **Contacts** | redb `CONTACTS_TABLE` | Local address book |
| **Conversations** | redb `CONVERSATIONS_TABLE` | Local conversation metadata |
| **Vault files** (metadata) | Iroh Docs `vault_namespace` | P2P synced file catalog |
| **Vault files** (content) | Iroh Blobs | P2P synced binary data |
| **Sync documents** | Iroh Docs | P2P CRDT state |
| **Followed hashtags** | redb `FOLLOWED_HASHTAGS_TABLE` | Local preferences |

---

## Data Flow Diagrams

### Import Flow (Bluesky/Facebook)

```
┌──────────────┐     ┌─────────────┐     ┌──────────────┐
│  External    │────▶│  Parser/    │────▶│  redb        │
│  API/ZIP     │     │  Connector  │     │  ITEMS_TABLE │
└──────────────┘     └─────────────┘     └──────────────┘
                            │
                            ▼
                     ┌─────────────┐
                     │  Iroh       │
                     │  Blobs      │
                     │  (media)    │
                     └─────────────┘
```

**Process:**
1. Connector fetches/parses external data
2. Media downloaded → stored in Iroh Blobs (returns hash)
3. Metadata + blob hashes → stored in redb `ITEMS_TABLE`
4. Item is queryable locally, media is P2P-synced

---

### Vault File Upload Flow

```
┌──────────────┐     ┌─────────────┐     ┌──────────────┐
│  User        │────▶│  Iroh       │────▶│  Iroh        │
│  File        │     │  Blobs      │     │  Docs        │
└──────────────┘     │  (content)  │     │  (metadata)  │
                     └─────────────┘     └──────────────┘
```

**Process:**
1. File bytes → stored in Iroh Blobs (returns hash)
2. Thumbnail generated → stored in Iroh Blobs (returns hash)
3. `VaultFile` metadata (name, size, hashes, tags) → stored in Iroh Docs
4. File is immediately P2P-synced to connected peers

---

### P2P Message Flow

```
┌──────────────┐     ┌─────────────┐     ┌──────────────┐
│  Remote      │────▶│  Iroh       │────▶│  redb        │
│  Peer        │     │  Connection │     │  MESSAGES_   │
│              │     │             │     │  TABLE       │
└──────────────┘     └─────────────┘     └──────────────┘
                            │
                            ▼
                     ┌─────────────┐
                     │  Frontend   │
                     │  Event      │
                     └─────────────┘
```

**Process:**
1. Incoming P2P connection accepted
2. Message received via bidirectional stream
3. Message persisted to redb `MESSAGES_TABLE`
4. Frontend notified via Tauri event
5. Message queryable from local DB

---

## Table Schemas

### redb Tables

```rust
// Local posts (Bluesky, Facebook imports)
ITEMS_TABLE: TableDefinition<&str, &str>  // Key: source:original_id, Value: JSON<BytescapeItem>

// P2P and Bluesky messages
MESSAGES_TABLE: TableDefinition<&str, &str>  // Key: message_id, Value: JSON<Message>

// Known contacts
CONTACTS_TABLE: TableDefinition<&str, &str>  // Key: did, Value: JSON<Contact>

// Conversation metadata
CONVERSATIONS_TABLE: TableDefinition<&str, &str>  // Key: conversation_id, Value: JSON<Conversation>

// User preferences
FOLLOWED_HASHTAGS_TABLE: TableDefinition<&str, &str>  // Key: tag, Value: timestamp
```

### Iroh Docs Entries

```
// Vault file metadata
vault/file/{file_id} → JSON<VaultFile>

// Sync documents (created per sharing session)
doc/{namespace_id} → CRDT state
```

### Iroh Blobs

```
// Media content addressed by hash
{hash} → bytes (image, video, etc.)
{hash}:thumb → bytes (WebP thumbnail)
```

---

## Ownership Rules

### Rule 1: redb is Local Cache
- redb data is **NOT** synced to peers
- redb can be rebuilt from Iroh + re-import if lost
- redb stores denormalized data for fast queries

### Rule 2: Iroh Docs is Source of Truth for Vault
- Vault file metadata lives in Iroh Docs
- Multiple peers can edit (CRDT resolution)
- redb may cache Iroh Docs data for performance

### Rule 3: Iroh Blobs is Content-Addressed Storage
- Content addressed by BLAKE3 hash
- Deduplicated automatically
- Thumbnails stored as separate blobs

### Rule 4: Security-Sensitive Data Stays Local
- JWT tokens → keychain file only
- Iroh secret keys → keychain file only
- Never sync credentials via P2P

---

## Migration Strategy

### Current State (Technical Debt)
- ❌ `BytescapeItem` stored in redb only (not P2P synced)
- ❌ `VaultFile` stored in Iroh Docs (separate from items)
- ❌ No clear migration path between systems

### Target State
1. **Unified Item Model**: All content items use Iroh Docs for P2P sync
2. **redb as Index**: redb stores materialized views for fast queries
3. **Sync on Import**: Imported items automatically synced to Iroh

### Migration Steps
1. Add `sync_id: Option<NamespaceId>` to `BytescapeItem`
2. On import, create Iroh Doc entry + store sync_id
3. Background job syncs existing redb items to Iroh
4. Queries check Iroh first, fallback to redb

---

## Performance Considerations

### Query Patterns

| Query | Storage | Notes |
|-------|---------|-------|
| List items by date | redb | Indexed, fast |
| List items by source | redb | Filter on source field |
| Get vault stats | redb | Pre-computed counts |
| Get file content | Iroh Blobs | Stream from blob store |
| Search content | redb | Future: full-text index |
| Sync status | Iroh Docs | Check CRDT state |

### Caching Strategy

```
┌─────────────────────────────────────┐
│  Frontend Request                   │
└──────────────┬──────────────────────┘
               ▼
┌─────────────────────────────────────┐
│  redb (fast path)                   │
│  - Items, Messages, Contacts        │
│  - Materialized views               │
└──────────────┬──────────────────────┘
               │ (cache miss)
               ▼
┌─────────────────────────────────────┐
│  Iroh Docs/Blobs (slow path)        │
│  - P2P fetch                        │
│  - Update redb cache                │
└─────────────────────────────────────┘
```

---

## Error Handling

| Error | Storage | Recovery |
|-------|---------|----------|
| redb corruption | redb | Delete + re-import from source |
| Iroh blob missing | Iroh Blobs | Fetch from P2P peers |
| Iroh doc conflict | Iroh Docs | CRDT auto-resolution |
| Keychain lost | File | User must re-login |

---

## Future Considerations

1. **Willow Protocol**: Consider migrating to Willow-compatible storage for true P2P interoperability
2. **Erasure Coding**: Add redundancy for blob storage across peers
3. **Selective Sync**: Allow users to choose which namespaces to sync
4. **Offline Mode**: Queue writes locally, sync when connected


---

# WILLOW MIGRATION STRATEGY


Date: 2026-02-22

## Scope
Evaluate replacing the current redb-backed `BytescapeItem` storage with a Willow-native store (`willow_store_simple_sled`), considering the current media-import pipeline and recently added Willow-style item paths.

## Current State

- Structured/social items (`BytescapeItem`, `VaultMessage`) are stored in `redb` via `DbManager`.
- Vault file metadata and blob references already live in Iroh Docs/Blobs (`SyncManager`), i.e. partially Willow-aligned behavior already exists for media.
- `BytescapeItem.path` has been added and backfilled using hierarchical format:
  - `/{source}/{type}/{year}/{month}/{id}`
- New imports now assign Willow-style paths at write time.

## Evaluation Summary

### Option A — Full Immediate Replacement (redb -> willow_store_simple_sled)

Pros:
- Single data model aligned with Willow from end to end.
- Path-native querying semantics.

Cons / Risks:
- Large blast radius: all read/write paths, dedupe keys, pagination, and filtering APIs must be rewritten.
- Potential feature regressions in import/feed/chat while replacing mature redb code.
- Current dependency ecosystem showed compile friction when enabling Willow crates in this codebase (resolved by feature-gating `willow_25`).

Assessment: **High risk right now**.

### Option B — Hybrid Staged Migration (recommended)

Pros:
- Keeps imports/feed/chat stable while continuing Willow conformance work.
- Uses `path` as migration contract now, enabling transparent backend swap later.
- Lets us migrate by domain (items first, then messages/contacts) with measurable checkpoints.

Cons:
- Temporary dual-store complexity.

Assessment: **Best balance of velocity + safety**.

## Decision

**Choose Option B (Hybrid Staged Migration).**

- Keep redb as active source of truth for structured item/message query paths in current release.
- Continue writing and maintaining canonical Willow paths on every `BytescapeItem`.
- Gate direct Willow crate usage behind Cargo feature flags until transitive dependency graph is stabilized.

## Proposed Execution Plan

1. **Stabilize path-indexed APIs on current store**
   - Add/expand path-prefix query interfaces on `DbManager` as migration-compatible API surface.
2. **Add shadow-write adapter (optional feature)**
   - On `save_item`, write to redb + Willow store in parallel under feature flag.
3. **Consistency verification tooling**
   - Compare counts/checksums/path coverage between stores.
4. **Read switch by endpoint**
   - Move archive/feed endpoints to Willow-backed reads once parity is proven.
5. **Remove redb item path for migrated entities**
   - Keep fallback read for one release cycle.

## Acceptance Criteria for Full Swap

- Import parity: Facebook, Instagram, Bluesky imports identical counts and media linkage.
- Feed parity: same item ordering and dedupe behavior across tabs.
- Migration safety: no data loss in backfilled paths and no missing records during restart.
- Performance: pagination and filtered reads remain within current UX latency.

## Notes on Dependencies

- `willow_25` has been integrated in feature-gated form (`feature = "willow"`).
- This allows immediate path-parameter conformance without destabilizing baseline builds.
- Full Willow storage activation should be coupled with dependency unification work before default enablement.


---

# SECURITY


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
