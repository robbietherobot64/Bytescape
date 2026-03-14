# Bytescape Roadmap

## Mission
To build a local-first Social Vault—an "Engine and Lens" system that bridges the gap between Web2 convenience and Web3 sovereignty.

## ✅ Completed: Foundation ✅
- [x] **Core P2P Identity**: Generates Iroh Node IDs.
- [x] **Basic Bluesky Bridge**: `getTimeline`, `getAuthorFeed`, etc
- [x] **Frontend Basics**: Chat & Feed UI foundation.
- [x] **Data Normalization**: `BytescapeItem` schema with 3 Universal Types (Status, Gallery, Article).
- [x] **Multi-Feed Selection**: Unified hashtags + custom feeds via `get_unified_feed`.

---

## ✅ Phase 1: The Universal Translator & Media Vault 📦🌐 ✅
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

## ✅ Phase 1.5: Mobile App & UX Polish 📱 ✅
**Goal:** Ship a functional Android build with a responsive, mobile-first interface.

- [x] **Android Build Pipeline**: Tauri v2 Android target compiling for aarch64, armv7, i686, x86_64. APK + AAB output.
- [x] **GeoCities "Old Web" Theme**: Win98 beveled edges, navy sidebar, beige content, Verdana typography, tiled star backgrounds.
- [x] **Mobile Top Bar Navigation**: Nav bar moved from bottom to top of screen (mobile only). All 8 nav icons behind hamburger dropdown menu. Feed tabs (All/Following/Archives) render inline in the top bar next to hamburger.
- [x] **Collapsible Context Drawer**: Mobile-only toggle bar below nav shows "Discovery Tools" / "My Contacts" / "Archive Explorer" per active tab. Expands to show FeedRightPanel content inline.
- [x] **Feed Tab State Lifting**: `activeTab` state lifted from FeedScreen to App.tsx for slot-based rendering in the mobile top bar.
- [x] **CSS Cascade Fixes**: Desktop override rules moved inside `@media (min-width: 769px)` to prevent global `display: none` from breaking mobile elements.
- [x] **TypeScript csstype Fixes**: Cast inline styles as `React.CSSProperties` to resolve strict type-checking errors in ChatLayout, ComposeBox, SettingsScreen.

## ✅ Phase 2: Willow Data Model Migration 🌳 ✅
**Goal:** Restructure vault storage to be Willow-compatible for future P2P sync.

- [x] **Add `willow_25` crate**: Import Willow types from `earthstar-project/willow-rs`.
- [x] **Add `path` field to BytescapeItem**: Hierarchical addressing (`/{source}/{type}/{year}/{month}/{id}`).
- [x] **Migration script**: Backfill paths for existing items based on source + timestamp (auto-run via startup migration command).
- [x] **Evaluate full Willow store**: Considered `willow_store_simple_sled` as redb replacement; selected hybrid staged migration path (see `bytescape/src-tauri/WILLOW_STORE_EVALUATION.md`).
- [x] **Path-indexed read API (bridge start)**: Added paginated path-prefix query + command-layer routing/fallback for vault feed reads.
- [x] **Shadow write parity hooks**: Capture and compare canonical item writes across legacy and Willow-compatible paths (`save_item`/`upsert_item`) with `get_willow_write_parity_report` summary command.
- [x] **Parity verification tooling**: Added consistency report metrics for total items, path coverage ratio, and canonical path mismatches.
- [x] **Canary read rollout**: Added `BYTESCAPE_WILLOW_PATH_ONLY_READS` flag for source-filtered vault feed reads to run path-prefix-only without legacy fallback.

---

## ✅ Phase 3: P2P Sync & Identity 🔗 ✅
**Goal:** Prove the core differentiator—two devices syncing via Iroh. Sovereign identity without dependence on any external service.

### Infrastructure
- [x] **Expose SyncManager**: Tauri commands for `create_doc`, `join_doc`, `send_message`.
- [x] **Iroh Node Init**: `setup_iroh_node` initializes Iroh endpoint, SyncManager, secret key from keychain.
- [x] **P2P Doc Operations**: `create_sync_doc`, `join_sync_doc`, `send_p2p_message` commands registered.
- [x] **Gossip Presence**: `SyncManager::broadcast_presence` via `iroh-gossip`.
- [x] **Identity Recovery**: 5 identity commands for local DID lifecycle (`identity_init_full_recovery`, `identity_get_recovery_status`, `identity_start_recovery`, `identity_verify_recovery`, `identity_finalize_recovery`).

### P2P Connectivity & Protocol
- [x] **Iroh Router Architecture**: Replaced broken manual accept loop with `iroh::protocol::Router` dispatching 4 ALPN handlers: docs (`/iroh-sync/1`), gossip (`/iroh-gossip/1`), blobs (`/iroh-bytes/4`), and custom `bytescape/1.0`. Commits: `28bc93b`, `b957cb4`.
- [x] **Bidirectional Profile Exchange**: `PROFILE_EXCHANGE` protocol sends requester's profile to responder and vice versa, with `last_seen` fallback for asymmetric NAT. Desktop↔Mobile connectivity verified.
- [x] **Peer Management Commands**: `get_my_peer_info`, `add_peer`, `list_peers`, `remove_peer`, `check_peer_connectivity` — all registered and functional.
- [x] **P2P Chat Protocol**: `CHAT_MESSAGE` handler in `BytescapeProtocol` saves incoming messages to `VaultMessage` via `save_incoming_chat_message()`, emits `p2p-message` frontend event.
- [x] **P2P Chat Commands**: `send_p2p_chat_message` (QUIC connect → send → save locally) and `list_p2p_conversations` (peer list with message counts and online status).
- [x] **P2P Chat UI**: ChatScreen has P2P tab with send routing, `p2p-message` event listener, empty-state prompt. FriendsScreen peer names are clickable with 💬 icon → navigates to `p2p:{node_id}` conversation.
- [x] **Cross-Network P2P Verified**: P2P connectivity confirmed working across different WiFi networks via Pkarr DHT relay.
- [x] **Android APK**: Built with full P2P chat code (77MB, March 2026).

### Master Identity & P2P Content
- [x] **`did:key` Master Identity**: Generate a `did:key:z6Mk...` from each user's Iroh Ed25519 keypair as the universal umbrella identity. Store as `master_did` in keychain. Link Bluesky `did:plc:...` via `alsoKnownAs` at app layer.
- [x] **P2P Profile/Post Viewing**: `PROFILE_REQUEST` and `FEED_REQUEST` protocol commands allowing peers to browse each other's profiles and posts over the `bytescape/1.0` ALPN.
- [x] **Local-Only Login Flow**: "Local Vault" login option generates a local identity (Iroh keypair + `did:key`) without requiring any remote API. Backend: `create_local_identity` Tauri command. Frontend: "Create Local Vault" button on LoginScreen.
- [x] **Two-Device Sync Test**: Verified end-to-end: Device A sends P2P chat message → Device B receives and displays it.
- [x] **Contact P2P Discovery**: Exchange Iroh node IDs via QR code, manual ticket paste, or Bluesky profile metadata.
- [x] **Chat Tab Cleanup**: Tabbed chat UI separating Bluesky, P2P, and Archives into distinct tabs.

### Remaining
- [x] **Identity "Door 1"**: Local DID generation with blind escrow recovery. RecoverySetupModal prompts user for real recovery code after identity creation, generates encrypted escrow bundle. LoginScreen has 4th "Recover Identity" option with 3-step restore flow (paste bundle → enter code → finalize).

---

## Phase 4: Federation, Rich Sharing & Profile Platform 📤🌐🎨
**Goal:** Three parallel tracks building the full social platform simultaneously — federation with the fediverse, a rich media sharing engine, and the profile/lens/community system. Designed for independent agents working each track in parallel.

> **Track interdependencies:** Track A (Federation) unlocks several items in B and C marked with *(unlocked by Track A)*. Those items should be stubbed/prepared but not completed until their Track A prerequisite is merged. All other items within each track can proceed in listed order without cross-track blocking.

> **Unsupported content syndication policy:** Some content types (polls, long-form articles, events, etc.) won't be supported on every target network. Until `bytescape.social` is live with public profile links, unsupported content is simply **not syndicated** to that network. Once hosting is live, unsupported content syndicates as a simple HTTP link back to the Bytescape profile where it can be viewed natively.

### Already Complete (from earlier phases)
- [x] **Syndication Backend**: `syndicate_vault_item` command posts to Bluesky via `createRecord`.
- [x] **Syndication History & Rollback**: `share_history` event log with `active` flags, background `verify_syndications` rollback on upstream deletion.
- [x] **Syndication Vault Dedup**: Timeline importers skip re-importing already syndicated items to prevent ghost duplicates.
- [x] **Archive Sorting**: Archive feed uses `effective_timestamp` to bump recently syndicated items to the top of the feed.
- [x] **Connector: Instagram**: `connectors/instagram.rs` — Separate parser for Instagram data export ZIP.

### Security Hardening 🔒
*Cross-cutting audit findings addressed across all tracks.*

- [x] **CRIT-4 P2P Rate Limiting**: `PeerRateLimiter` on `BytescapeProtocol` — max 3 concurrent connections per peer, 60 req/min sliding window, 64KB payload enforcement, stale entry eviction.
- [x] **CRIT-5 CORS Restriction**: Warp media server restricted to `tauri://localhost`, `https://tauri.localhost`, `http://localhost:1420` (no more `allow_any_origin`).
- [x] **CRIT-6 BLAKE3 Escrow Integrity**: Replaced MD5 with BLAKE3 for identity escrow bundle integrity hashes.
- [x] **HIGH-3 XSS Sanitization**: DOMPurify-based `sanitizeHtml()` (safe tags only) and `stripHtml()` utilities. Applied in `FeedEntryCard`, `QuoteRepostModal`. Codebase audited for `dangerouslySetInnerHTML` — none found.
- [x] **HIGH-5 Blob Hash Centralization**: `blob_hash.rs` with `parse_blob_hash()` and `normalize_blob_ref()`. Replaced all inline hash parsing across 5 files. 19 unit tests.
- [x] **MED-4 Input Length Validation**: ComposeBox hardened — 10K char limit (8K warning), 20 attachment cap, 100MB file size limit (50MB warning), color-coded counters, Post button disabled when over limits.
- [x] **MED-5 MIME Detection Expansion**: `infer` crate (0.16) replacing hand-rolled `mime_from_magic_bytes()` — 100+ file types. Extended `mime_from_path_hint()` with AVIF, SVG, ICO, audio formats.

---

### Track A — Federation & Connectors 🌐
*Agent focus: Rust backend, protocol implementation, data import/export. AP is the critical path — it unblocks items across all three tracks.*

#### A0. Bluesky OAuth
- [x] **A0.1 OAuth Login Flow (PKCE + DPoP + PAR)**: Full Bluesky OAuth implementation replacing app-password auth. Pushed Authorization Requests, DPoP proof generation with `p256` ECDSA, PKCE `S256` code challenge, localhost callback server with auto-shutdown. `atrium-oauth` v0.1 integration with `OAuthSession` managing token lifecycle. *(Commits: `538034a`, `abc92fa`)*
- [x] **A0.2 DPoP-Authenticated Timeline**: OAuth sessions use user's PDS URL (resolved via `OAuthSession::base_uri()`) for authenticated API calls instead of `bsky.social/xrpc`. *(Commit: `3b01559`)*
- [x] **A0.3 Identity Hijack Prevention**: Fixed auto-generated local identity overwriting OAuth login identity during `check_auth_status`. Local identity creation now requires explicit `local_username` flag. *(Commit: `a45a8d5`)*
- [x] **A0.4 Public API Migration**: Migrated unauthenticated reads from `bsky.social/xrpc` (now requires auth for all endpoints) to `public.api.bsky.app/xrpc`. Split into `PUBLIC_XRPC` (unauthenticated) and `AUTH_XRPC` (Bearer-auth) constants. *(Commit: `2d970df`)*

#### A1. ActivityPub Core
Federation with the wider fediverse. Protocol implementation informed by [Streams](https://codeberg.org/streams/streams) / [Forte](https://codeberg.org/fortified/forte).

- [ ] **A1.1 AP Actor & Inbox/Outbox**: Create Bytescape AP actor at `bytescape.social/@{username}`. Implement HTTP signature verification, inbox POST handler, outbox GET endpoint. Add `ActivityPub` variant to `Source` enum.
- [ ] **A1.2 Basic Mastodon/Lemmy Support**: Feed reading, posting, DMs via custom AP C2S/S2S client. Follow/Accept/Reject activities.
- [ ] **A1.3 Data Integrity Proofs (FEP-8b32)**: Sign all outbound AP activities with ed25519 (`eddsa-jcs-2022`) using Iroh node keys + `json-canonicalization` crate.
- [ ] **A1.4 Actor Metadata (FEP-fb2a)**: Map Bytescape profile fields to/from `attachment` array on AP actor records.
- [ ] **A1.5 Site Actor Discovery (FEP-d556)**: Fetch domain root as ActivityStreams `Service` actor for server-level metadata.
- [ ] **A1.6 Multibox Delivery (FEP-0499)**: Batch deliveries to multiple inboxes on the same server.

#### A2. AP Rich Features
- [ ] **A2.1 Direct Message Routing (`nomad:directMessage`)**: Route inbound `directMessage: true` notes to ChatScreen, others to feed.
- [ ] **A2.2 Conversation Containers (FEP-171b / FEP-400e / FEP-7888)**: Thread creator owns a `Collection` object; replies `Add`ed by owner after moderation.
- [ ] **A2.3 Comment Permissions (`canReply` + `endDate`)**: Emit `canReply` and optional `endDate` on outbound posts.
- [ ] **A2.4 Expiring Content (`expires`)**: Add `expires_at` field to `BytescapeItem`. Background task auto-deletes expired items and emits federated `Delete`.
- [ ] **A2.5 Quote Posts (FEP-e232)**: Structured quote-post support via `tag` with `type: Link`. Add `quote_uri` field to `BytescapeItem`.
- [ ] **A2.6 Federated Search (`canSearch`)**: Discover `searchContent`/`searchTags` endpoints from AP actor records.
- [ ] **A2.7 Groups (AP `Group` Actor)**: Public/private groups via `Group` actor type with `Follow/Join/Leave` support.
- [ ] **A2.8 Private Media (Bearcaps/OCAP)**: Generate capability URIs for private media syndication.

#### A3. AP Identity & Auth
- [ ] **A3.1 Nomadic Identity Federation (FEP-ef61 / FEP-c390)**: Emit `copiedTo` in AP actor record listing all device endpoints, handle `Sync` activity to mirror profile changes. Identity Proofs (FEP-c390) link `did:key` to AP actor.
- [ ] **A3.2 OpenWebAuth (FEP-61cf)**: Federated SSO — authenticate to remote fediverse servers via home endpoint to access permission-gated content.

#### A4. Remaining Connectors
- [ ] **A4.1 Connector: Threads**: `connectors/threads.rs` — Threads data export ZIP parser. Threads uses AP internally, so live feed reading may fall out of A1.2 with Threads-specific profile resolution. Add `Threads` variant to `Source` enum.
- [ ] **A4.2 Connector: RSS/Atom**: Add `feed-rs` crate, create `connectors/rss.rs` for blog/newsletter subscriptions. Add `Rss` variant to `Source` enum.
- [ ] **A4.3 Connector: Article Extraction**: Add `readability` crate for sanitized HTML from arbitrary URLs → stored as `BytescapeType::Article`.

---

### Track B — Profile, Lenses & Communities 🎨
*Agent focus: Frontend React components, UI screens, CSS, map integration. No AP dependency for most items — build against local data and P2P peers.*

#### B1. Lenses & Content Views
- [x] **B1.1 Lens: Gallery**: Masonry grid with column count by viewport. `GalleryLens` component with source filter dropdown, `Lightbox` overlay with prev/next navigation, keyboard controls (arrows, Escape), full metadata display. Source filter persisted across lightbox open/close.
- [x] **B1.2 Lens: Reader**: `ReaderLens` with clean serif typography, reading-time estimate, scroll progress bar, source/date header. Full article content rendered in sanitized HTML with `sanitizeHtml()` from DOMPurify. Empty-state messaging.
- [x] **B1.3 Item-Type Rendering**: `FeedItemRenderer` dispatches to `GalleryLens`, `ReaderLens`, or default `FeedEntryCard` based on `BytescapeType`. `FeedScreen` has lens toggle button (grid/list/book icons) cycling through Feed → Gallery → Reader views.

#### B2. Profile Builder & Theming
- [x] **B2.1 GrapesJS Integration**: Embed GrapesJS drag-and-drop module builder in `ProfileBuilderScreen`. Outputs clean HTML/CSS stored locally in user's Willow store. Requires `grapesjs` npm package.
- [x] **B2.2 Raw HTML Override**: Upload custom HTML/CSS/JS directly to Willow store. Rendered in a sandboxed `<iframe>` to prevent XSS.
- [x] **B2.3 DID Meta Tags**: Emit `<meta name="bytescape:did">` and cross-platform subscription link tags for client-side DID resolution.
- [x] **B2.4 Widget System**: Away message, music player, friends list, guestbook, visitor counter, post feed, media grid — live React widget hydration from GrapesJS blocks, with persistent configuration via Tauri commands and redb storage.
- [x] **B2.5 Theme Engine**: Three built-in themes — **Cozy Mode** (GeoCities default), **Desktop Mode** (Win98 silver/teal, sharper bevels, system font, no stars), **Midnight Mode** (deep blues, soft purples, rounded corners). `ThemeId` type, `useThemeStore` (Zustand + localStorage persistence), `useTheme()` hook applies `data-theme` attribute on `<html>`. `ThemePicker` component in SettingsScreen Appearance card with swatch previews and instant switching. Ghost CSS variables (`--accent-purple`, `--bg-primary`, etc.) defined with defaults. 5 ThemePicker tests.
- [ ] **B2.6 Dual-Format Serving** *(unlocked by Track A — A1.1)*: Profile URL serves HTML to browsers and Atom/Lexicon to aggregators simultaneously via `<link rel="alternate">`.
- [ ] **B2.7 "Memory" Post Type** *(tentative, low priority)*: Vault item resurfacing with "On This Day" semantics and optional syndication as a nostalgia post. Details TBD when we get to it.

#### B3. Peer Profile Browser
- [ ] **B3.1 Profile Browser Tab**: New top-level screen for visiting and traversing peer profiles by following profile links, webrings, and connection graphs.
- [ ] **B3.2 Sandboxed iframe Renderer**: Load peer profile HTML in an isolated frame. Shell reads `<meta>` tags and populates Follow/Subscribe action buttons.
- [ ] **B3.3 Swarm Caching**: Temporarily cache visited profile data locally (Iroh blobs, signed by owner's key) to distribute load from high-traffic profiles.
- [ ] **B3.4 Pub Node Support**: Allow users to configure a dedicated Pub pinning node for 24/7 profile availability without full self-hosting.

#### B4. Location, Maps & Groups
Inspired by [Habitat](https://github.com/carlnewton/habitat) — hyperlocal social posts linked to map coordinates. Requires `leaflet` or `maplibre-gl` npm package.

- [ ] **B4.1 Geolocation Field on `BytescapeItem`**: Add optional `geo: Option<GeoPoint>` (lat/lng + approximate geohash) to the universal item schema.
- [ ] **B4.2 Location Picker UI**: Map-based coordinate picker in the compose flow. Privacy-first: coarsen to ~1 km geohash by default.
- [ ] **B4.3 "Pin to Map" Action**: Existing vault items can be retroactively geotagged via a context-menu action.
- [ ] **B4.4 Group Data Model**: `LocalGroup` record in redb — `group_id`, `name`, `description`, `geohash_prefix`, `category`, `owner_did`, `created_at`. Backed by an Iroh doc namespace for P2P replication.
- [ ] **B4.5 Group Creation Flow**: "Create Local Group" UI with name, description, category selector, and map-drawn boundary.
- [ ] **B4.6 Group Feed**: Dedicated feed view showing `BytescapeItem` entries tagged to a group, ordered chronologically.
- [ ] **B4.7 Group Discovery (Nearby)**: Automatically surface groups whose geohash prefix overlaps the user's current approximate location.
- [ ] **B4.8 Group Membership**: Join/leave groups. Members' posts replicate via Iroh doc sync. Non-members see a read-only preview.
- [ ] **B4.9 Group Categories**: Admins define per-group categories (Landmarks, Events, Buy/Sell, Discussion, etc.).
- [ ] **B4.10 Group Moderation**: Group owner can pin posts, remove content, and delegate moderator roles to other DIDs.

#### B5. Spatial Map View
- [ ] **B5.1 MapScreen**: New top-level screen rendering a Leaflet/Maplibre map with clustered `BytescapeItem` markers.
- [ ] **B5.2 Heatmap Layer**: Toggle overlay showing content density by geohash tile.
- [ ] **B5.3 "Around Me" Mode**: Uses device location (with permission) to center map and auto-load nearby groups + posts. Radius slider (500 m → 50 km).
- [ ] **B5.4 Map Embed in Group Feed**: Inline mini-map at the top of each group feed.

#### B6. Local Marketplace
- [ ] **B6.1 Marketplace Category**: Special group category type `ForSale` / `Wanted` with structured fields: price, condition, pickup location.
- [ ] **B6.2 Marketplace Browse View**: Grid layout with thumbnail, price badge, distance indicator.
- [ ] **B6.3 Listing Detail**: Full item view with media carousel, seller profile link, "Message Seller" button (routes to P2P chat).
- [ ] **B6.4 Sold/Expired State**: Seller marks listing as sold; item grays out in feed but remains in vault history.

#### B7. P2P Group Sync
- [ ] **B7.1 Gossip-Based Group Discovery**: Leverage `iroh-gossip` to propagate group metadata across nearby peers.
- [ ] **B7.2 Geohash Routing**: Community Stations (Phase 5) index groups by geohash prefix for spatial queries without a central server.
- [ ] **B7.3 Cross-Group "Nearby" Feed**: Aggregate recent posts from all groups within a geohash radius into a single "Nearby" feed tab.

#### B8. Events & Calendar
- [ ] **B8.1 `Event` BytescapeType**: Add `Event` as a fourth variant to `BytescapeType` in `db.rs` with structured fields: `start_time`, `end_time`, `timezone`, `location`, `rsvp_status`, `rsvp_collection`.
- [ ] **B8.2 Event Creation UI**: Date/time picker, location picker, description, optional recurrence.
- [ ] **B8.3 Calendar View**: New lens/view rendering `Event`-type `BytescapeItem` entries in a calendar layout.
- [ ] **B8.4 Map Integration**: Events with `location` coordinates appear as pins on MapScreen.
- [ ] **B8.5 AP Event Federation** *(unlocked by Track A — A1.2)*: Emits `Invite(Event)` when syndicated via AP. RSVP Flow: Accept/Reject/Tentative. `Update(Event)` and `Delete(Event)` propagate via federation.

#### B9. Federated Features (unlocked by Track A)
- [ ] **B9.1 AP-Federated Group Threading** *(unlocked by Track A — A2.2)*: `ConversationContainer` record type with owner moderation (`Add`/`Remove` replies).
- [ ] **B9.2 Webrings & Forums**: `com.platform.webring.member` AT Protocol Lexicon, `publish_webring_membership` command, AppView Indexer subscribing to firehose, webring UI with prev/next navigation.
- [ ] **B9.3 WASM Web Portal** *(unlocked by Track A — A1.1 + hosting)*: Public profiles serve blurred preview to scrapers; full content via WebAssembly Iroh node in browser.

#### B10. Browser Chrome & Navigation 🌐
*Every Bytescape page is addressable by URL — whether served locally on desktop or mirrored on a relay. The app shell leans into a browser-within-a-browser aesthetic: omnibar, back/forward, reload.*

> **URL Scheme Design:** Locally, pages use path-based addresses like `byte://feed/following` or `byte://profile/@alice.bsky.social`. When a user mirrors content via a relay (Phase 5 Station or `bytescape.social`), the same paths resolve at `https://lovingisliving.bytescape.social/feed/following` (subdomain per user). The omnibar shows whichever form is canonical — `byte://` by default, the public HTTPS URL when the user's relay mirror is active. Peer profiles visited over P2P use `byte://peer/{nodeId}/profile`.

- [ ] **B10.1 Internal URL Scheme**: Define and register internal URL addressing for all Bytescape screens. Mapping: `/feed`, `/feed/following`, `/feed/feeds/{feedUri}`, `/feed/hashtags/{tag}`, `/chat/bsky/{convoId}`, `/chat/p2p/{nodeId}`, `/vault`, `/vault/{blobHash}`, `/profile`, `/profile/@{handle}`, `/profile-builder`, `/import`, `/settings`, `/article/{itemId}`, `/map`. Create a `useLocation()` hook (Zustand) that exposes `currentUrl` and `navigate(url)`. All screen transitions go through this hook so the URL always reflects the visible page. *(Frontend agent — Track B)*
- [ ] **B10.2 Navigation History Stack**: Browser-style forward/backward navigation. `useNavigationHistory` Zustand store maintaining a stack of visited URLs with a cursor index. `goBack()`, `goForward()`, `canGoBack`, `canGoForward` exposed to the UI. Max depth ~50 entries. Each `navigate()` call pushes onto the stack. *(Frontend agent — Track B)*
- [ ] **B10.3 Omnibar UI**: Evolve the existing `TopBar` search bar into a browser-style omnibar. Shows the current page URL (from B10.1 scheme) in a styled address bar. Editable — user can type a URL to navigate directly, or type text to search. Auto-completes known routes and recent URLs. Flanked by ◀ Back / ▶ Forward / 🔄 Reload buttons. Shows a 🏠 local / 🌐 relay / 🔗 peer icon indicating content origin. The bar should have the classic beveled-inset browser field look consistent with the GeoCities aesthetic. *(Frontend agent — Track B)*
- [ ] **B10.4 Reload / Refresh**: Reload button next to the omnibar triggers a full re-fetch of the current screen's data sources. Each screen exports a `refresh()` callback (or listens for a `byte://reload` event). Feed screens re-fetch timeline, vault re-queries blob list, profile re-fetches actor data, chat re-polls conversations. *(Frontend agent — Track B)*
- [ ] **B10.5 URL Deep Linking** *(unlocked by Track A — A1.1 + hosting)*: When `bytescape.social` is live, the internal URL scheme maps to public HTTP URLs. `byte://profile/@alice` → `https://alice.bytescape.social`. The omnibar shows the public HTTPS URL when the user's relay mirror is active. Clicking "Copy Link" copies the public URL. Incoming links from external browsers resolve via the WASM portal (B9.3) or redirect to the app via `byte://` protocol handler (registered via Tauri deep-link plugin). *(Cross-track — A + B)*
- [ ] **B10.6 URL Bar for Peer Browsing**: When visiting a peer's profile (B3.1), the omnibar shows `byte://peer/{nodeId}/profile`. Users can type a peer's node ID or `did:key` into the omnibar to navigate directly to their profile. Integrates with the Peer Profile Browser — omnibar becomes the primary navigation surface for traversing the peer graph. *(Frontend agent — Track B, builds on B3.1)*
- [ ] **B10.7 External Web Browsing (Desktop Only)**: On desktop, the content area uses a **secondary Tauri `Webview`** (created via `WebviewBuilder::new()` with **no IPC access**) that can load arbitrary `http://`/`https://` URLs. The React shell (omnibar, sidebar) stays in the primary webview and controls the content webview via Tauri commands (`webview.navigate(url)`, `webview.go_back()`, `webview.go_forward()`). Content webview navigation events bubble up to update the omnibar URL display and history stack (B10.2). When viewing a `byte://` internal route, the content webview is hidden and the React screen renders directly; when navigating to an external URL, the React screen is hidden and the content webview is shown. **iframe fallback**: For niche cases like embedded profile widgets or whitelisted embeds, sandboxed `<iframe>` elements remain available within React components (e.g., embedding a Bandcamp player or a peer's profile widget). **Mobile**: External links open via the platform's in-app browser (SFSafariViewController / Chrome Custom Tabs via `tauri-plugin-opener`) — mobile stays focused on the social platform experience, not general browsing. *(Frontend + Backend — Track B)*
- [ ] **B10.8 Unified Search (Omnibar)**: The omnibar doubles as a federated search surface. When the user types text that isn't a URL or `byte://` route, it triggers a **tiered search** across multiple sources, prioritized in this order:
  1. **Bytescape Network** (default, highest priority): Search local vault items, contacts, P2P peers, and (when hosting is live) the Bytescape directory. Results include profiles, pages, articles, vault items.
  2. **Fediverse** (AT Protocol + ActivityPub): Search public content across all subscribed protocols — Bluesky actor search (`app.bsky.actor.searchActors`), AP WebFinger lookups, hashtag federation. Returns profiles, posts, communities.
  3. **Web** (lowest priority): Falls back to a configurable search engine (default: DuckDuckGo) for general web results. Results open in the content webview (B10.7 on desktop) or in-app browser (mobile).
  Results render in a unified dropdown below the omnibar, grouped by source with clear section headers (🏠 Bytescape · 🌐 Fediverse · 🔍 Web). **Filters**: User can prefix queries to constrain scope — `@linkinpark` (people only), `#rockmusic` (hashtags only), `site:bandcamp.com linkin park` (specific site via web search), or toggle filter chips: "Bytescape only", "Fediverse only", "Web only". **Example**: Searching "Linkin Park" → ① Linkin Park's Bytescape profile → ② Linkin Park fan page on Bytescape → ③ Linkin Park Bluesky/AP profiles → ④ linkinpark.com (web result). *(Cross-track — A + B + C)*
- [ ] **B10.9 Built-in Ad Blocker (Desktop)**: Bytescape is ad-free by philosophy — this extends to external web content viewed in the content webview (B10.7). Implement content filtering on the Rust side by intercepting the webview's resource requests (WebKitGTK `WebView::resource-load-started` / WebView2 `add_WebResourceRequested`) and blocking requests matching known ad/tracker domains. Ship a bundled filter list (EasyList subset or similar — curated for size). Block ad iframes, tracking pixels, and video pre-rolls where possible. **Scope**: Desktop only (mobile uses platform in-app browsers which have their own ad handling). **Research items**: Evaluate WebKitGTK vs WebView2 interception APIs for feasibility, determine filter list update mechanism (bundled vs. periodic fetch), assess performance impact of request-level filtering. *(Backend — Track B, research phase)*

#### B11. Network & Interaction Polish
- [ ] **B11.1 Network State Feed**: `NetworkStateEntry` event type (profile updated, webring joined, import completed, group created). Feed injection with distinct visual treatment.
- [ ] **B11.2 Friend Zoom**: Per-connection closeness slider that filters feed by relationship strength.
- [ ] **B11.3 Haptics & Audio**: "Mechanical" (crisp clicks) and "Organic" (wood taps, paper) sound profiles via `tauri-plugin-haptics`.

---

### Track C — Rich Sharing Engine 📝
*Agent focus: ComposeBox enhancement, new content types, rich text processing, media pipeline. The goal is a full-featured posting system that rivals mainstream platforms for expressiveness while syndication targets remain Bluesky, AP, and local vault.*

#### C1. Rich Text & Facets
- [x] **C1.1 Rich Text Facet Parsing — Backend**: `richtext.rs` — `detect_facets()`, `parse_facets_for_bluesky()`, `parse_facets_local()` with UTF-8 byte offset calculation. 23 unit tests. Wired into `posting.rs` (publish_post_v2) and `syndicate.rs` (syndicate_vault_item) — facets auto-generated on Bluesky publish.
- [ ] **C1.1b Rich Text Facet Rendering — Frontend**: Render facets as clickable @mentions, #hashtags, and hyperlinks in `FeedEntryCard` and thread replies. Add facet TypeScript types to `types/index.ts`.
- [x] **C1.2 Mention Autocomplete — Backend**: `search_mentions` Tauri command registered — searches local contacts by handle prefix, returns `MentionSuggestion { handle, did, avatar_url }`.
- [ ] **C1.2b Mention Autocomplete — Frontend**: Typeahead dropdown UI in ComposeBox triggered by `@` — calls `search_mentions`, inserts selected handle.
- [x] **C1.3 Hashtag Autocomplete — Backend**: `search_hashtags_autocomplete` Tauri command registered — searches followed hashtags by prefix.
- [ ] **C1.3b Hashtag Autocomplete — Frontend**: Dropdown UI in ComposeBox triggered by `#` — calls `search_hashtags_autocomplete`, inserts selected tag.
- [ ] **C1.4 Character Count & Limits**: Per-target live counter in ComposeBox. Bluesky: 300 graphemes (via `Intl.Segmenter`). AP: configurable per-server (default 500). Show which targets will truncate. *(Basic local-only 10K char counter exists from MED-4.)*

#### C2. Link & Embed System
- [ ] **C2.1 Link Card Embeds**: URL detection in compose text → fetch Open Graph / Twitter Card metadata → render preview card. Emit `app.bsky.embed.external` for Bluesky, `attachment` with `Link` type for AP.
- [ ] **C2.2 GIF Picker**: Integrate Tenor or Giphy API for searchable GIF selection. Embed as image attachment with `image/gif` MIME type.
- [ ] **C2.3 Emoji Picker**: Full Unicode emoji grid with search, skin tone selector, recently-used section. Insert into compose text as Unicode characters.
- [ ] **C2.4 Custom Emoji** *(unlocked by Track A — A1.2)*: AP servers expose custom emoji via actor metadata. Render `:shortcode:` syntax with remote images. Local custom emoji uploaded to vault.

#### C3. Rich Content Types
- [ ] **C3.1 Polls**: Poll creation UI in ComposeBox — multiple choice, single/multi-select, expiration timer. Use AP `Question` type for fediverse syndication (compatible with Threads/Mastodon poll UX). Bluesky: not syndicated until platform supports polls natively or hosting is live (then syndicate as link).
- [x] **C3.2 Long-Form Articles**: `ArticleEditor` component using **Tiptap** StarterKit with Placeholder. Full toolbar (Bold, Italic, H1–H3, Lists, Blockquote, Code Block, HR). Title field, word count, reading time estimate. Markdown import/export via `utils/markdown.ts`. `ArticleReader` component with scroll progress bar, sanitized HTML rendering. Article mode toggle in ComposeBox — switches between Status/Article `item_type`. Backend `publish_post_v2` accepts `item_type` and `title` overrides. Tiptap collaboration extensions installed for C6. 34 tests.
- [ ] **C3.3 `Event` Post Type**: Quick-create an event from ComposeBox (date/time/location fields). Creates a `BytescapeType::Event` item. *(Complements B8 — B8 builds the calendar view and full CRUD; C3.3 adds the compose shortcut.)*

#### C4. Media Pipeline
- [ ] **C4.1 Drag-and-Drop Media**: Drop images/videos directly onto ComposeBox. Currently only file picker dialog is supported.
- [ ] **C4.2 Video Upload**: Implement video blob uploading to Bluesky PDS (`app.bsky.embed.video`), AP servers (as `attachment`), and local vault. Thumbnail generation for preview.
- [ ] **C4.3 Multi-Image Layout**: Support 1–4 image grid layouts in feed cards. Aspect ratio detection for optimal tiling.
- [ ] **C4.4 External Source Upload**: Pull media from Google Drive, Dropbox, or personal cloud storage into ComposeBox attachments.
- [ ] **C4.5 Audio Attachments**: Record or upload audio clips. Stored as vault blobs, syndicated with `audio/*` MIME type. Inline playback in feed.

#### C5. Posting Flows
- [ ] **C5.1 "Post to Wall" UX**: Vault items get a "Syndicate" button → opens ComposeBox pre-populated with item content + media. Posting to wall does not duplicate items in the vault.
- [ ] **C5.2 Multi-Destination Publishing**: ComposeBox target selector (local / Bluesky / AP / all) already exists in UI. Wire up AP destination once Track A delivers A1.2. Unsupported content types gracefully skip networks that can't render them (no syndication until hosting provides link fallback).
- [ ] **C5.3 Background Syndication Job**: Rate-limited processing of `PendingSyndication` items. Retry logic with exponential backoff. Status dashboard in SettingsScreen.
- [ ] **C5.4 Scheduled Posts**: Compose now, publish later. `scheduled_at` field on `BytescapeItem`, background job checks and publishes at target time.
- [ ] **C5.5 Syndication Safety Controls**: Explicit rate limits and confirmation prompts to prevent spamming external networks with old/bulk posts.

#### C6. Collaborative Editing 🤝
*Real-time Google Docs-level co-editing for Articles. Built as an additive layer — zero changes to existing `BytescapeItem` pipeline. Uses [Yjs](https://yjs.dev/) CRDTs (per-character conflict-free merging) with Iroh as the P2P transport.*

> **Architecture:** Yjs Doc ←→ `y-tiptap` (frontend) ←→ `iroh-gossip` (broadcast) ←→ `iroh-blobs` (persist snapshots) ←→ redb `COLLAB_DOCS` table (local state). The `BytescapeItem.content` field is periodically snapshot-updated from the Yjs doc so non-collaborative views work with zero changes.

> **Why not Willow?** Willow uses last-writer-wins per Entry — concurrent edits silently overwrite each other. Yjs merges at the character level with causal ordering, which is what "real-time collaborative editing" actually requires.

- [ ] **C6.1 CRDT Storage Layer** *(prerequisite: C3.2 ships with Tiptap)*: Add `yrs` (Rust Yjs port) crate to backend. New file `collab.rs` — Yjs doc CRUD, state vector management, update encoding/decoding. New redb table `COLLAB_DOCS: TableDefinition<&str, &[u8]>` storing binary Yjs state keyed by doc ID. Add optional `collab_doc_id: Option<String>` field to `BytescapeItem`. Tauri commands: `create_collab_doc`, `apply_collab_update`, `get_collab_state`, `snapshot_collab_to_item`. **Disruption: one optional field on BytescapeItem, everything else is new files/tables.**
- [ ] **C6.2 Frontend Yjs Integration**: Wire `yjs` + `y-tiptap` into the Article editor from C3.2. Local-only mode first — single user editing with CRDT doc backing. Verify round-trip: edits → Yjs update → Tauri command → `yrs` merge → redb persist → reload. Packages: `yjs`, `y-prosemirror` (or `@tiptap/extension-collaboration` already added in C3.2).
- [ ] **C6.3 P2P Real-Time Sync via Gossip** *(prerequisite: working iroh-gossip from existing P2P chat)*: New gossip topic type `CollabUpdate` — Yjs update vectors broadcast to all peers with the doc open. Implement awareness protocol over gossip for cursor positions, selection highlights, and user presence indicators. Frontend shows colored cursors with peer names. **Disruption: adds a gossip topic variant, no breaking changes to existing chat gossip.**
- [ ] **C6.4 Offline & Async Merge**: When a peer reconnects after editing offline, exchange Yjs state vectors and compute minimal sync updates. Yjs handles conflict resolution automatically (no user-facing merge conflicts). Persist pending updates in redb for crash recovery.
- [ ] **C6.5 Station Relay for Always-On Collaboration** *(unlocked by Phase 5 — Station Protocol)*: Community Station acts as Yjs awareness relay + document update cache. Peer A edits → gossip update → Station buffers → Peer B comes online → Station forwards. Enables async collaboration without simultaneous presence. **This is what delivers true Google Docs parity.**
- [ ] **C6.6 Access Control & Permissions**: Per-document permission model — Owner (full control), Editor (read/write), Viewer (read-only). Permissions stored in Iroh doc metadata, enforced in `collab.rs` before applying updates. Invite flow via DID-based sharing links.

---

### Phase 4 Convergence — Identity & Hosting
*These items require all three tracks to be substantially complete. They sit at the end of Phase 4.*

#### Identity Consolidation & Recovery
*Deferred so all connected identity sources (Bluesky, AP, etc.) are in place before building unified recovery.*

- [ ] **2FA / Email Recovery**: Replace the paste-a-JSON-bundle flow with a user-friendly recovery method — email-based one-time code, TOTP authenticator app, or SMS fallback. The escrow bundle is stored server-side keyed by email; user proves ownership to retrieve it.
- [ ] **Multi-Device Identity Sync**: Same identity across phone + desktop via Iroh doc replication of the identity keychain. Requires secure key exchange protocol (QR pairing or one-time code).

> **🖥️ Hosting Milestone — `bytescape.social`**
>
> This is the first point in the project that requires server infrastructure. The 2FA/email recovery flow needs a lightweight relay service to hold encrypted escrow bundles and send verification emails. The AP connector (Track A) also needs a publicly-reachable inbox endpoint.
>
> **Options:**
> - **Under-desk NAS**: Sufficient for AP inbox + escrow relay. Run behind Cloudflare Tunnel or Tailscale Funnel pointed at `bytescape.social`.
> - **Cloud VPS**: Small Hetzner/Fly.io instance for higher availability. Caddy reverse proxy + the relay service.
>
> Either way, `bytescape.social` goes live here — initially serving the escrow relay API and the AP actor endpoint. Once live, content types unsupported on external networks (polls, long-form articles, events) can be syndicated as simple HTTP links back to the user's Bytescape profile.

---

## Phase 5: The Sovereign Cloud ☁️
**Goal:** Scale the `bytescape.social` infrastructure from the Phase 4 relay into a full P2P mesh. Research existing Iroh public relays first.

- [ ] **Station Protocol**: "Blind Relay" logic for encrypted store-and-forward (extends the escrow relay from Phase 4).
- [ ] **Collab Relay Extension**: Extend Station Protocol with Yjs document awareness relay and update buffering (enables C6.5 — always-on collaborative editing without simultaneous peer presence).
- [ ] **Auto-Docking**: Geohash-based discovery of nearest Community Station on account creation.
- [ ] **Erasure Coding**: Shard data across home station and adjacent regional nodes for redundancy.
- [ ] **Gossip Presence**: Real-time online status via `iroh-gossip`.
- [ ] **Stealth Mode**: Toggle to sever all Station connections for pure device-to-device Iroh operation.
- [ ] **Paired Streams/Forte Server**: Optionally pair Bytescape with a self-hosted [Streams](https://codeberg.org/streams/streams) instance as an always-on ActivityPub relay. Bridges P2P identity with fediverse presence.

---

## 🐛 Open Issues
- [ ] **Type Duplication**: `BlueskyPost` vs `BytescapeItem` in frontend—normalize into shared `FeedEntry` adapter (currently handled per-tab in `FeedScreen`).

### A0 OAuth — Known Gaps (post-merge follow-ups)
*Identified by PM audit of Track A delivery. These are not bugs — they are documented scope gaps in the initial OAuth implementation that need follow-up work.*

- [x] **A0-GAP-1 Ephemeral OAuth Sessions**: Persistent `FileSessionStore` with ChaCha20-Poly1305 encryption + Argon2id KDF. OAuth sessions survive app restarts. *(Commit: `45c6d04`)*
- [x] **A0-GAP-2 Fixed Callback Port**: Dynamic fallback to OS-assigned port (`127.0.0.1:0`) when `28457` is in use. *(Commit: `f2f279f`)*
- [ ] **A0-GAP-3 No OAuth Rust Tests**: `FileSessionStore` unit tests added (commit `bac702e`), but required test areas (OAuthManager construction, callback parsing, process_callback errors, is_oauth_session, PDS URL failure) are **not covered**. Needs proper OAuth flow tests.
- [x] **A0-GAP-4 Write Operations Not Wired for OAuth**: `publish_post_v2` and `syndicate_vault_item` now use `is_oauth_session()` branching. *(Commit: `9c60417`)*
- [x] **A0-GAP-5 DMs for OAuth Sessions**: Added `transition:chat.bsky` OAuth scope, `clone_with_proxy()` for isolated chat proxy headers, parallel chat import. *(Commits: `c29e549`, `ad6af24`)*
- [x] **A0-GAP-6 Saved Feeds / Unified Feed Empty for OAuth**: Fixed PDS URL vs AppView routing. *(Commit: `c684d84`)*
- [ ] **A0-GAP-7 Verify Transitive Crate Dependencies**: `p256` is NOT in `Cargo.toml` (has zero direct imports — correct). `urlencoding` IS in `Cargo.toml`. Needs verification on clean machine builds.

<details>
<summary>✅ Resolved Issues (click to expand)</summary>

- [x] Syndication Safety: Addressed as C5.5 — explicit rate limits and confirmation prompts.
- [x] Rename Project: Migrated from `dipper` to `bytescape`.
- [x] Stabilize Bridge: Token refresh and timezone fixes.
- [x] Refactor Schemas: Aligned `redb` tables with `BytescapeItem` spec.
- [x] SyncManager Underutilized: Commands now exposed to frontend.
- [x] VaultScreen: Full file manager implemented.
- [x] Facebook HTML Import: HTML format detection, scraper-based parsing, media extraction, tracker audit.
- [x] Media Pipeline Fixes: Tokio runtime handle, path resolution, data:/https URI filtering.
- [x] Feed Tab Restructure: Tabs aligned as All / Following / Feeds / #Hashtags / RSS / Archives.
- [x] Bluesky Buddy Names Missing in Chat Sidebar: Resolved sender handles from convo members array; unified buddy list with friends panel. *(Commits: `bf56e20`, `ae4bf4b`)*
- [x] OAuth Chat Proxy Header Leak: `clone_with_proxy()` prevents chat proxy from poisoning feed XRPC calls. *(Commit: `c29e549`)*
- [x] "You" Feed Tab Empty: Added `get_author_feed` command merging Bluesky author feed with local vault posts. *(Commit: `a870548`)*
- [x] Chat Import Never Completes: Parallel import so `import_vault_messages` runs alongside `import_bluesky_posts`. *(Commit: `ad6af24`)*
- [x] OAuth Feeds Empty / DMs Broken: Three root causes — missing `Authorization: DPoP` header on `send_http`, wrong PDS URL construction (`/xrpc` suffix missing), no chat proxy path for OAuth. All fixed with shared `FileSessionStore` (Arc), `xrpc_chat_get/post` helpers, and DM wiring. *(Commit: `22a0435`)*
- [x] Cross-Account Message Leakage: Fixed by owner-scoping vault messages.
- [x] Deletion Path Coverage: `clear_vault` + `logout` wipe local user-scoped tables.
- [x] Live Feed Semantics: Feed tabs aligned with live Bluesky subscribed-feed fetching.
- [x] Feed Subscription Fetch Reliability: Saved-feed URI parsing hardened.
- [x] Message Import Triggering: Bluesky message import runs during import flow.
- [x] Chat Source Partitioning: Live and Archived sections with archived read-only composer.
- [x] Archive Chat Coverage: Instagram JSON + Facebook HTML thread formats.
- [x] Feed Compose Layout Spillover: Fixed create-post/feed column stacking.
- [x] Facebook: Metadata Leaking into Post Content: Selective exclusion of footer/tag/timestamp elements.
- [x] Facebook: Duplicate Photos: Within-post + cross-file dedup.
- [x] Syndication Rollback Bug: `verifiedIdsRef` cache clear on manual refresh.
- [x] Syndication Image Extraction Duplication Bug: `known_uris` passed to `import_bluesky_feed`.
- [x] Mobile CSS Cascade Bug: Desktop hide rules scoped inside `@media (min-width: 769px)`.
- [x] Android Build TypeScript Errors: `as React.CSSProperties` casts.
- [x] Repost Dedup Pipeline: Composite keys (`repost:<did>:<uri>`).
- [x] Repost Timestamp Sorting: Use repost's `indexed_at` instead of original `created_at`.
- [x] Repost Display & Reply Nesting: Full end-to-end repost rendering + `reply_parent_uri` field.
- [x] P2P Chat Messages Not Delivering: 4-bug fix in commit `11de2b7`.
- [x] Mobile Chat Input Hidden Behind Nav Bar: `--safe-bottom` CSS fix.

</details>
