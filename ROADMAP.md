# Bytescape Roadmap

## Mission
To build a local-first Social Vault—an "Engine and Lens" system that bridges the gap between Web2 convenience and Web3 sovereignty.

## ✅ Completed: Foundation ✅
- [x] **Core P2P Identity**: Generates Iroh Node IDs.
- [x] **Basic Bluesky Bridge**: `getTimeline`, `getAuthorFeed`, and OAuth login.
- [x] **Frontend Basics**: Chat & Feed UI foundation.
- [x] **Data Normalization**: `BytescapeItem` schema with 3 Universal Types (Status, Gallery, Article).
- [x] **Multi-Feed Selection**: Unified hashtags + custom feeds via `get_unified_feed`.

---

## Phase 1: The Universal Translator & Media Vault 📦🌐
**Goal:** Complete the local data layer—the core technology is normalization across sources, with a Profile Wall to display imported content.

### Completed
- [x] **Universal Schema**: `BytescapeItem` in `db.rs` supports Status, Gallery, Article types.
- [x] **Media Pipeline**: Wire `iroh-blobs` for media storage (scaffolding exists in `sync.rs`).
- [x] **WebP Thumbnails**: Add `image` crate for thumbnail generation on import.
- [x] **Expose P2P Commands**: `create_sync_doc`, `join_sync_doc`, `send_p2p_message`, `store_media_blob`, `get_media_blob`.
- [x] **BlobImage Component**: Frontend resolves `blob:hash` URLs via Tauri command + `useBlobUrl` hook.
- [x] **Connector: Bluesky**: `connectors/bluesky.rs` — feed, timeline, likes import, media→vault pipeline.
- [x] **Connector: Facebook**: `connectors/facebook.rs` — ZIP archive parsing (HTML + JSON formats), HTML scraping with structural fallback, media extraction with Iroh blob storage, off-Facebook tracker detection & audit.
- [x] **Connector: Instagram**: `connectors/instagram.rs` — ZIP archive parsing with post/media extraction and normalized `BytescapeItem` import.
- [x] **Facebook Parser Refinement**: Targeted sub-selector extraction (header `_a6-h`, timestamp `_a72d`, body `_a6-p`), expanded path/file/content noise filtering, am/pm timestamp fix, lifecycle event filtering. Posts-only feed achieved.
- [x] **Archive Import Reliability**: End-to-end archive ingest now persists media to vault and writes `media_blobs` for feed rendering across Facebook + Instagram imports.
- [x] **Archive Chat Import**: Imported archive conversations are now persisted to `VaultMessage` with owner scoping and source partitioning. Supports Instagram JSON exports and Facebook HTML thread exports (`message_*.html`).

### Profile Wall
- [x] **`syndication_status` field**: Added to `BytescapeItem` — `LocalOnly` (default), `PendingSyndication`, `Syndicated`.
- [x] **BytescapeItemCard**: Reusable component rendering any `BytescapeItem` with content, media (via BlobImage), timestamp, SourceBadge, syndication indicator.
- [x] **ProfileScreen Wall**: Expanded to show all imported posts with source/type filters, stats header, and scrollable feed.

### Vault File Manager
- [x] **VaultScreen**: Full file manager UI with grid/list view, thumbnails via `BlobImage`, drag-and-drop upload, file picker upload.
  - Source badges (Facebook, Bluesky, Upload)
  - Kind filters (Image, Video, Audio, Document, Archive)
  - Search, sort, detail modal with full-size preview
  - File deletion with Iroh doc tombstone approach
  - Stats bar (file counts by kind, total storage)
- [x] **Filtered DB Queries**: `list_items_by_source`, `list_items_by_type`, `list_items_by_date_range`.
- [x] **Archive Import E2E**: Facebook HTML archive → parsed → media stored in Iroh blobs → VaultFile entries created → visible in vault.
- [x] **AuditScreen**: Off-Facebook tracker visualization (capped at 100 entries, event count formatting, 3-strategy detection with deduplication).
- [x] **Video Support**: `VideoPlayer` handles blob-stored videos.
- [x] **Vault Viewer UX**: Fullscreen media-first viewer, left/right navigation, optional file-data panel.

## Phase 1.5: Mobile App & UX Polish 📱
**Goal:** Ship a functional Android build with a responsive, mobile-first interface.

### Completed
- [x] **Android Build Pipeline**: Tauri v2 Android target compiling for aarch64, armv7, i686, x86_64. APK + AAB output.
- [x] **GeoCities "Old Web" Theme**: Win98 beveled edges, navy sidebar, beige content, Verdana typography, tiled star backgrounds.
- [x] **Mobile Top Bar Navigation**: Nav bar moved from bottom to top of screen (mobile only). All 8 nav icons behind hamburger dropdown menu. Feed tabs (All/Following/Archives) render inline in the top bar next to hamburger.
- [x] **Collapsible Context Drawer**: Mobile-only toggle bar below nav shows "Discovery Tools" / "My Contacts" / "Archive Explorer" per active tab. Expands to show FeedRightPanel content inline.
- [x] **Feed Tab State Lifting**: `activeTab` state lifted from FeedScreen to App.tsx for slot-based rendering in the mobile top bar.
- [x] **CSS Cascade Fixes**: Desktop override rules moved inside `@media (min-width: 769px)` to prevent global `display: none` from breaking mobile elements.
- [x] **TypeScript csstype Fixes**: Cast inline styles as `React.CSSProperties` to resolve strict type-checking errors in ChatLayout, ComposeBox, SettingsScreen.

## Phase 2: Willow Data Model Migration 🌳
**Goal:** Restructure vault storage to be Willow-compatible for future P2P sync. Do this now while only FB + Bluesky data exists—simpler migration.

- [x] **Add `willow_25` crate**: Import Willow types from `earthstar-project/willow-rs`.
- [x] **Add `path` field to BytescapeItem**: Hierarchical addressing (`/{source}/{type}/{year}/{month}/{id}`).
- [x] **Migration script**: Backfill paths for existing items based on source + timestamp (auto-run via startup migration command).
- [x] **Evaluate full Willow store**: Considered `willow_store_simple_sled` as redb replacement; selected hybrid staged migration path (see `bytescape/src-tauri/WILLOW_STORE_EVALUATION.md`).

## Phase 2.5: Hybrid Migration Bridge 🌉
**Goal:** Introduce Willow-native access patterns on top of current redb storage before full store replacement.

- [x] **Path-indexed read API (bridge start)**: Added paginated path-prefix query + command-layer routing/fallback for vault feed reads.
- [x] **Shadow write parity hooks**: Capture and compare canonical item writes across legacy and Willow-compatible paths (`save_item`/`upsert_item`) with `get_willow_write_parity_report` summary command.
- [x] **Parity verification tooling**: Added consistency report metrics for total items, path coverage ratio, and canonical path mismatches.
- [x] **Canary read rollout**: Added `BYTESCAPE_WILLOW_PATH_ONLY_READS` flag for source-filtered vault feed reads to run path-prefix-only without legacy fallback.

## Phase 3: Syndication & Connectors 📤🌐
**Goal:** Safe posting to external networks + remaining connectors. Connectors matter once sharing exists.

### Syndication
- [x] **Syndication Backend**: `syndicate_vault_item` command posts to Bluesky via `createRecord`.
- [x] **Syndication History & Rollback**: `share_history` event log with `active` flags, background `verify_syndications` rollback on upstream deletion.
- [x] **Syndication Vault Dedup**: Timeline importers skip re-importing already syndicated items to prevent ghost duplicates.
- [x] **Archive Sorting**: Archive feed uses `effective_timestamp` to bump recently syndicated items to the top of the feed.
- [ ] **Full Post Box UI**:
  - Text composition area.
  - Media upload support (Images, Video).
  - Pre-population logic for Vault items.
- [ ] **Video Upload Support**: Implement video blob uploading to Bluesky PDS.
- [ ] **"Post to Wall" UX**: 
  - In vault: "Syndicate" button triggers Post Box.
- [ ] **Background Syndication Job**: Rate-limited processing of `PendingSyndication` items.
- [ ] **"Memory" Post Type**: Research AT Protocol lexicon for archive/throwback semantics.

### Connectors
- [x] **Connector: Instagram**: `connectors/instagram.rs` — Separate parser for Instagram data export ZIP.
- [ ] **Connector: Threads**: `connectors/threads.rs` — Separate parser for Threads data export ZIP.
- [ ] **Connector: ActivityPub**: Add `megalodon` crate, create `connectors/activitypub.rs` for Mastodon/Lemmy.
- [ ] **Connector: RSS/Atom**: Add `feed-rs` crate, create `connectors/rss.rs` for blog/newsletter subscriptions.
- [ ] **Connector: Article Extraction**: Add `readability` crate for sanitized HTML from arbitrary URLs.
- [ ] **Add `Instagram`, `Threads`, `Rss` variants to Source enum**: In `connectors/mod.rs`.

## Phase 4: P2P Sync & Identity 🔗
**Goal:** Prove the core differentiator—two devices syncing via Iroh. This is the next active phase.

### Current Infrastructure (already built)
- [x] **Expose SyncManager**: Tauri commands for `create_doc`, `join_doc`, `send_message`.
- [x] **Iroh Node Init**: `setup_iroh_node` initializes Iroh endpoint, SyncManager, secret key from keychain.
- [x] **P2P Doc Operations**: `create_sync_doc`, `join_sync_doc`, `send_p2p_message` commands registered.
- [x] **Gossip Presence**: `SyncManager::broadcast_presence` via `iroh-gossip`.
- [x] **Identity Recovery**: 5 identity commands for local DID lifecycle (`identity_init_full_recovery`, `identity_get_recovery_status`, `identity_start_recovery`, `identity_verify_recovery`, `identity_finalize_recovery`).

### Completed: P2P Connectivity & Protocol
- [x] **Iroh Router Architecture**: Replaced broken manual accept loop with `iroh::protocol::Router` dispatching 4 ALPN handlers: docs (`/iroh-sync/1`), gossip (`/iroh-gossip/1`), blobs (`/iroh-bytes/4`), and custom `bytescape/1.0`. Commits: `28bc93b`, `b957cb4`.
- [x] **Bidirectional Profile Exchange**: `PROFILE_EXCHANGE` protocol sends requester's profile to responder and vice versa, with `last_seen` fallback for asymmetric NAT. Desktop↔Mobile connectivity verified.
- [x] **Peer Management Commands**: `get_my_peer_info`, `add_peer`, `list_peers`, `remove_peer`, `check_peer_connectivity` — all registered and functional.
- [x] **P2P Chat Protocol**: `CHAT_MESSAGE` handler in `BytescapeProtocol` saves incoming messages to `VaultMessage` via `save_incoming_chat_message()`, emits `p2p-message` frontend event.
- [x] **P2P Chat Commands**: `send_p2p_chat_message` (QUIC connect → send → save locally) and `list_p2p_conversations` (peer list with message counts and online status).
- [x] **P2P Chat UI**: ChatScreen has P2P section in sidebar (purple dot), P2P send routing, `p2p-message` event listener, empty-state prompt. FriendsScreen peer names are clickable with 💬 icon → navigates to `p2p:{node_id}` conversation.
- [x] **Android APK**: Built with full P2P chat code (77MB, March 2026).

### Next Steps: P2P Chat Debugging & Local-Only Login (Active)
**Current blocker:** P2P chat UI opens correctly but messages do not appear to send/receive between devices. The protocol plumbing is in place but needs debugging — likely an issue with message serialization, delivery confirmation, or the frontend not loading/displaying `VaultMessage` entries for P2P conversations.

- [ ] **🐛 P2P Chat Messages Not Delivering**: Chat window opens for a peer but sent messages don't appear on the other device and received messages don't render. Debug the full pipeline: `send_p2p_chat_message` → QUIC connect → `CHAT_MESSAGE` handler → `save_incoming_chat_message` → `p2p-message` event → ChatScreen render. Check: Is the QUIC connection actually established? Is the message serialized/deserialized correctly? Are `VaultMessage` entries being written and read back? Is the `p2p-message` event reaching the frontend?
- [ ] **🐛 Mobile Chat Input Hidden Behind Nav**: The chat compose text box renders below the bottom navigation bar on mobile, making it impossible to type. Needs CSS fix to ensure the chat input stays above the mobile nav (similar to prior `Feed Compose Layout Spillover` fix).
- [ ] **Local-Only Login Flow**: Create a "Local Vault" login option that generates a local identity (Iroh keypair + DID) without requiring any remote API. User gets a handle like `local:<node-id-short>` and can access all vault features.
  - Backend: New `create_local_identity` Tauri command that generates keypair, stores in keychain, returns an `AuthStatus` with `source: "local"`.
  - Frontend: Add "Create Local Vault" button to `LoginScreen` as an alternative to Bluesky login.
  - Session persistence: `check_auth_status` must also detect and restore local-only sessions from keychain.
- [ ] **Two-Device Sync Test**: Verify end-to-end: Device A sends P2P chat message → Device B receives and displays it. Use `test-peer/` for automated verification.
- [ ] **Contact P2P Discovery**: Exchange Iroh node IDs via QR code, manual ticket paste, or Bluesky profile metadata for users who have both identities.

### Future
- [ ] **Identity "Door 1"**: Local DID generation with blind escrow recovery (backend commands exist, need frontend UX).
- [ ] **Multi-Device Identity Sync**: Same identity across phone + desktop via Iroh doc replication of the identity keychain.

## Phase 5: Profile Builder, Spatial Discovery & UI Polish 🗺️🎨
**Goal:** Build the sovereign profile system, spatial graph-based discovery, the Aero-Skeuomorphic visual language, and MySpace-style profile personalization. The profile is the product.

### Profile Builder
- [ ] **GrapesJS Integration**: Embed the GrapesJS drag-and-drop module builder in a `ProfileBuilderScreen`. Outputs clean HTML/CSS stored locally in the user's Willow store.
- [ ] **Raw HTML Override**: Upload custom HTML/CSS/JS directly to the Willow store (rendered in a sandboxed `<iframe>` to prevent XSS).
- [ ] **Dual-Format Serving**: Profile URL serves HTML to browsers and Atom/Lexicon to aggregators simultaneously via `<link rel="alternate">`.
- [ ] **DID Meta Tags**: Emit `<meta name="bytescape:did">` and cross-platform subscription link tags for client-side DID resolution.
- [ ] **Widget System**: Away message, music player, friends list, guestbook — configurable and drag-and-drop arrangeable.
- [ ] **WASM Web Portal**: Public profiles serve a static blurred preview to scrapers; full-fidelity content requires loading a WebAssembly Iroh node in the browser to decrypt the P2P stream.

### Peer Profile Browser
- [ ] **Profile Browser Tab**: New top-level screen for visiting and traversing peer profiles by following profile links, webrings, and connection graphs.
- [ ] **Sandboxed iframe Renderer**: Load peer profile HTML in an isolated frame. Shell reads `<meta>` tags and populates Follow/Subscribe action buttons.
- [ ] **Swarm Caching**: Temporarily cache visited profile data locally (Iroh blobs, signed by owner's key) to distribute load from high-traffic profiles.
- [ ] **Pub Node Support**: Allow users to configure a dedicated Pub pinning node for 24/7 profile availability without full self-hosting.

### Webrings & Forums
- [ ] **`com.platform.webring.member` Lexicon**: Define and implement the AT Protocol Lexicon record schema.
- [ ] **Webring Publish Command**: `publish_webring_membership` Tauri command writes the record to the user's local PDS.
- [ ] **AppView Indexer**: `bytescape-bridge` subscribes to the AT Protocol firehose, indexes `com.platform.webring.member` records, and stitches member nodes into navigable loops and forum feeds.
- [ ] **Webring UI**: Display webring membership on profile; prev/next navigation through the ring.

### Local Communities & Groups (inspired by [Habitat](https://github.com/carlnewton/habitat))
Habitat is an open-source hyperlocal social platform where posts are linked to map coordinates, giving communities a sense of *place*. We adapt its core ideas to Bytescape's P2P/local-first architecture.

#### Location-Tagged Content
- [ ] **Geolocation Field on `BytescapeItem`**: Add optional `geo: Option<GeoPoint>` (lat/lng + approximate geohash) to the universal item schema. Posts, vault files, and marketplace listings can be pinned to a location.
- [ ] **Location Picker UI**: Map-based coordinate picker (Leaflet or Maplibre GL) in the compose flow. Users tap a point on the map or enter an address; result stored as `GeoPoint`. Privacy-first: coarsen to ~1 km geohash by default.
- [ ] **"Pin to Map" Action**: Existing vault items can be retroactively geotagged via a context-menu action.

#### Local Groups
- [ ] **Group Data Model**: `LocalGroup` record in redb — `group_id`, `name`, `description`, `geohash_prefix` (defines the geographic boundary), `category`, `owner_did`, `created_at`. Backed by an Iroh doc namespace for P2P replication.
- [ ] **Group Creation Flow**: "Create Local Group" UI with name, description, category selector, and map-drawn boundary (circle or polygon → geohash prefix).
- [ ] **Group Feed**: Dedicated feed view showing `BytescapeItem` entries tagged to a group, ordered chronologically. Filter by category (e.g., Events, For Sale, Landmarks, Q&A, General).
- [ ] **Group Discovery (Nearby)**: Automatically surface groups whose geohash prefix overlaps the user's current approximate location. Sort by distance. Inspired by Habitat's "Nearby" feed concept — visitors see relevant local groups without subscribing.
- [ ] **Group Membership**: Join/leave groups. Members' posts to the group replicate via Iroh doc sync. Non-members see a read-only preview.
- [ ] **Group Categories**: Admins define per-group categories (Landmarks, Events, Buy/Sell, Discussion, etc.). Categories control visibility: some may be "Home only" (members) vs. "Nearby visible" (visitors), mirroring Habitat's category scoping.
- [ ] **Group Moderation**: Group owner can pin posts, remove content, and delegate moderator roles to other DIDs.

#### Spatial Map View
- [ ] **MapScreen**: New top-level screen rendering a Leaflet/Maplibre map. Clusters of geotagged `BytescapeItem` markers, colored by category or source. Tap a cluster → expand; tap a pin → preview card.
- [ ] **Heatmap Layer**: Toggle overlay showing content density by geohash tile — reveals where conversations are happening.
- [ ] **"Around Me" Mode**: Uses device location (with permission) to center the map and auto-load nearby groups + posts. Radius slider (500 m → 50 km).
- [ ] **Map Embed in Group Feed**: Inline mini-map at the top of each group feed showing the spatial distribution of recent posts.

#### Local Marketplace
- [ ] **Marketplace Category**: Special group category type `ForSale` / `Wanted` with structured fields: price, condition, pickup location (geohash).
- [ ] **Marketplace Browse View**: Grid layout with thumbnail, price badge, distance indicator. Filter by category, price range, distance.
- [ ] **Listing Detail**: Full item view with media carousel, seller profile link, "Message Seller" button (routes to P2P chat).
- [ ] **Sold/Expired State**: Seller marks listing as sold; item grays out in feed but remains in vault history.

#### P2P Group Sync (ties into Phase 4 & 6)
- [ ] **Gossip-Based Group Discovery**: Leverage `iroh-gossip` to propagate group metadata (name, geohash, member count) across nearby peers — similar to Habitat's gossip protocol for instance discovery.
- [ ] **Geohash Routing**: Community Stations (Phase 6) index groups by geohash prefix, enabling efficient spatial queries without a central server.
- [ ] **Cross-Group "Nearby" Feed**: Aggregate recent posts from all groups within a geohash radius into a single "Nearby" feed tab, ordered by distance then recency.

### Network State Feed
- [ ] **Network Event Schema**: Add `NetworkStateEntry` event type (profile updated, webring joined, archive import completed, group created/joined).
- [ ] **Feed Injection**: Bridge emits network state events for followed contacts; rendered inline in the feed with distinct visual treatment.

### Interface & Lenses
- [ ] **Aero UI System**: Glassmorphism, blurred panels, and skeuomorphic textures implementing the Frutiger Aero visual language.
- [ ] **Lens: Gallery**: Grid-based media browser for Gallery-type items.
- [ ] **Lens: Reader**: Article view with sanitized HTML rendering.
- [ ] **Item-Type Rendering**: UI switches view based on `BytescapeType`.
- [ ] **Theme Engine**: Cozy Mode (default), Desktop Mode (Win98), custom theme editor via CSS-like syntax.
- [ ] **Haptics & Audio**: "Mechanical" (crisp clicks) and "Organic" (wood taps, paper) sound profiles via `tauri-plugin-haptics`.

## Phase 6: The Sovereign Cloud ☁️
**Goal:** Enable robust P2P infrastructure for always-on sync. Research existing Iroh public relays first.

- [ ] **Station Protocol**: "Blind Relay" logic for encrypted store-and-forward.
- [ ] **Auto-Docking**: Geohash-based discovery of nearest Community Station on account creation.
- [ ] **Erasure Coding**: Shard data across home station and adjacent regional nodes for redundancy.
- [ ] **Gossip Presence**: Real-time online status via `iroh-gossip`.
- [ ] **Stealth Mode**: Toggle to sever all Station connections for pure device-to-device Iroh operation.

---

## 🐛 Current Issues & Tasks
- [x] **Rename Project**: Migrated from `dipper` to `bytescape`.
- [x] **Stabilize Bridge**: Token refresh and timezone fixes.
- [x] **Refactor Schemas**: Aligned `redb` tables with `BytescapeItem` spec.
- [x] **SyncManager Underutilized**: Commands now exposed to frontend.
- [x] **VaultScreen**: Full file manager implemented with upload, delete, search, filters.
- [x] **Facebook HTML Import**: HTML format detection, scraper-based parsing, media extraction, tracker audit.
- [x] **Media Pipeline Fixes**: Tokio runtime handle in spawn_blocking, path resolution for HTML src attributes, data:/https URI filtering.
- [x] **Feed Tab Restructure**: Tabs are now All / Following / Feeds / #Hashtags / RSS / Archives. "All" merges live Bluesky timeline + vault items chronologically. Archives shows imported vault content with source filter.
- [ ] **Type Duplication**: `BlueskyPost` vs `BytescapeItem` in frontend—normalize into shared `FeedEntry` adapter (currently handled per-tab in `FeedScreen`).
- [ ] **Syndication Safety**: Need explicit controls to prevent spamming networks with old posts.
- [x] **Cross-Account Message Leakage**: Fixed by owner-scoping vault messages and clearing user data on logout.
- [x] **Deletion Path Coverage**: `clear_vault` + `logout` now wipe local user-scoped tables and synced vault file entries.
- [x] **Live Feed Semantics**: Feed tabs now aligned as Following / Feeds / All with live Bluesky subscribed-feed fetching.
- [x] **Feed Subscription Fetch Reliability**: Saved-feed URI parsing hardened; custom feed URI requests are properly encoded.
- [x] **Message Import Triggering**: Bluesky message import now runs during import flow and when entering chat.
- [x] **Chat Source Partitioning**: Live (Bluesky/Mastodon) and Archived (walled-garden) sections with archived read-only composer.
- [x] **Archive Chat Coverage (Facebook + Instagram)**: Archive chat import now covers Instagram JSON and Facebook HTML thread formats; messages appear in Archived chat section.
- [x] **Feed Compose Layout Spillover**: Fixed create-post/feed column stacking.
- [x] **Facebook: Metadata Leaking into Post Content**: Fixed: selectively exclude footer/tag/timestamp elements from body text; strip GPS coordinates.
- [x] **Facebook: Duplicate Photos**: Fixed: within-post media dedup via HashSet, cross-file dedup by normalized filenames.
- [x] **Syndication Rollback Bug**: Fixed: `useUnifiedFeed` now explicitly clears its `verifiedIdsRef` cache on manual refresh, forcing `verify_syndications` to re-check stale posts accurately instead of locking them "active" forever.
- [x] **Syndication Image Extraction Duplication Bug**: Fixed: When actively sharing an image to Bluesky, solving an edge case where the background timeline importer would subsequently see the new AT Post, download its photos over the network as a new post, and stash duplicates in the `media` folder. `known_uris` is now passed to `import_bluesky_feed` to halt media processing for AT URIs already in the local vault.
- [x] **Mobile CSS Cascade Bug**: Global `display: none` rules for mobile elements overriding `@media` rules. Fixed by scoping all desktop hide rules inside `@media (min-width: 769px)`.
- [x] **Android Build TypeScript Errors**: `csstype` version mismatch rejecting valid CSS properties (`bottom`, `paddingBottom`). Fixed with `as React.CSSProperties` casts.
- [x] **Repost Dedup Pipeline**: Fixed 3-layer dedup (Rust `get_unified_feed`, Rust `get_timeline_unified`, TS `deduplicateFeedEntries`) to use composite keys (`repost:<did>:<uri>`) so reposts are distinct feed events that aren't collapsed with originals. Commits: 421efa9, 76a71ec.
- [x] **Repost Timestamp Sorting**: Reposts now use the repost's `indexed_at` (when the repost appeared on the feed) instead of the original post's `created_at`, so they sort chronologically to when the user actually saw them.
- [x] **Repost Display & Reply Nesting**: Fixed end-to-end repost rendering and owner reply merging. Reposts now display the correct original post content with a "X reposted" banner. `import_bluesky_feed()` no longer skips reposts due to `post.author.did != did` filtering. Owner replies to reposted posts are nested inline under the parent card as fully interactive `FeedEntryCard` components (with engagement bar, thread toggle, reply compose, reactions). Added `reply_parent_uri` field through the full Rust→TS pipeline (`FeedReplyPost` → `BlueskyImportedPost` → `BytescapeItem` → `FeedEntry`) and `nestOwnerReplies()` grouping in `useUnifiedFeed`. Also introduced `resolveAtUri()` helper in `FeedEntryCard` to correctly resolve AT-URIs for vault items (where `entry.id` is a CID), fixing thread loading, likes, reposts, and reply compose for imported posts.
- [ ] **P2P Chat Messages Not Delivering**: Chat window opens for peers but messages don't send/receive. Full pipeline needs debugging: `send_p2p_chat_message` → QUIC → `CHAT_MESSAGE` handler → `save_incoming_chat_message` → `p2p-message` event → ChatScreen.
- [ ] **Mobile Chat Input Hidden Behind Nav Bar**: Chat compose text box renders below the mobile bottom navigation, making it unusable. Needs CSS viewport/layout fix.
- [ ] **Bluesky Buddy Names Missing in Chat Sidebar**: Chat right-panel buddy list doesn't show Bluesky user display names; clicking buddies does nothing.
