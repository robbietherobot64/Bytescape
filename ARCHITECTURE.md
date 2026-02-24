# Bytescape Architecture

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
