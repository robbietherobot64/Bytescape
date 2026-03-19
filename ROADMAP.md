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

- [x] **A1.1 AP Actor & Inbox/Outbox**: Create Bytescape AP actor at `bytescape.social/@{username}`. Implement HTTP signature verification, inbox POST handler, outbox GET endpoint. Add `ActivityPub` variant to `Source` enum. *(8 files, 1,636 lines, 22 tests. Commits: `7c58c40`, `a4e4062`, `28a6b8c`)*
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

#### A5. Platform Integrity & Anti-Bot 🛡️
*Bytescape is 100% user-generated content. No AI bots, no spam accounts, no automated posting. These measures apply to Bytescape-native accounts and to federated content ingested from external protocols.*

> **Philosophy**: We can't control what happens on Bluesky or Mastodon, but we can control what Bytescape amplifies. Federated content from external sources passes through the same detection pipeline — flagged content is demoted or hidden, not displayed alongside human posts.

- [ ] **A5.1 Identity Verification Gate**: New Bytescape identities (local or hosted on `bytescape.social`) must pass a proof-of-humanity step during registration. Options (implement at least two, configurable):
  1. **Interactive CAPTCHA**: Locally-rendered challenge (no third-party CAPTCHA service — keep it privacy-respecting). E.g., image-based puzzle, drag-to-solve, or audio challenge. Rendered in the app UI, verified by the Rust backend.
  2. **Invite-only onboarding**: New accounts require an invite code from an existing user. Each user gets N invite codes (configurable, default 5). Invite tree is tracked for abuse tracing.
  3. **Email verification** *(for hosted accounts)*: When `bytescape.social` is live, require email confirmation. Rate-limit registrations per email domain. Block known disposable email providers.
  4. **Vouching system**: Existing users can vouch for new accounts. Unvouched accounts have limited posting visibility until vouched by ≥2 peers.
  *(Backend — Track A)*
- [ ] **A5.2 Rate Limiting & Behavioral Analysis**: Detect and throttle bot-like posting patterns on Bytescape-native accounts:
  1. **Posting rate limits**: Max posts per hour/day (configurable, default 30/hr, 200/day). Burst detection — flag accounts that post >5 times in 60 seconds.
  2. **Content similarity detection**: BLAKE3-hash the text content of each post. If an account posts >3 near-identical items within 24 hours, flag for review. Use Levenshtein distance or n-gram similarity for fuzzy matching.
  3. **Engagement ratio monitoring**: Accounts that post heavily but never interact (no likes, replies, or DMs) get flagged. Pure broadcast accounts are suspicious.
  4. **New account probation**: Fresh accounts (first 7 days) have reduced posting limits (10/hr, 50/day) and cannot DM non-contacts.
  *(Backend — Track A)*
- [ ] **A5.3 Federated Content Filtering**: Apply bot/spam detection to content ingested from Bluesky, AP, and other federated sources:
  1. **AT Protocol labeler integration**: Subscribe to community bot-detection labelers on Bluesky. Content labeled as bot-generated is hidden by default (user can override in settings).
  2. **AP actor heuristics**: Check inbound AP actors for bot signals — missing `summary`/bio, default avatar, account age <24h with >100 posts, `bot: true` flag in actor metadata (Mastodon sets this).
  3. **Content-level signals**: Detect AI slop patterns — repetitive phrasing, excessive hashtag stuffing (>10 per post), URL-only posts from unknown domains, identical text posted to multiple targets.
  4. **User-level block/report**: Users can flag accounts as bots. Aggregated reports (≥3 unique reporters) auto-hide the account's content platform-wide pending review.
  *(Backend — Track A, builds on A1.1 and A0)*
- [ ] **A5.4 AI Scraping Prevention (Hosted)** *(unlocked by A1.1 + hosting)*: Protect Bytescape user content from AI training data harvesting when served via `bytescape.social`:
  1. **`robots.txt`**: Block all known AI crawlers — GPTBot, CCBot, Google-Extended, anthropic-ai, Bytespider, etc. Maintain an updatable blocklist.
  2. **`X-Robots-Tag` headers**: Serve `noai, noimageai` on all user content pages.
  3. **`ai.txt`** *(emerging spec)*: Publish machine-readable AI usage policy at `bytescape.social/ai.txt`.
  4. **Rate limiting**: Aggressive rate limiting on `bytescape.social` — max 60 req/min per IP, CAPTCHA challenge on sustained high-frequency access patterns.
  5. **Hotlink protection**: Prevent direct embedding/scraping of user media. Serve images via signed, time-limited URLs when accessed from non-Bytescape referrers.
  *(Backend — Track A, hosted infrastructure)*
- [ ] **A5.5 Content Provenance & Attestation** *(stretch)*: Cryptographic proof that content was created by a human using the Bytescape app:
  1. **Signed posts**: Every `BytescapeItem` created locally is signed with the user's Iroh ed25519 key. The signature covers the content hash + timestamp. Federated recipients can verify the signature against the user's published `did:key`.
  2. **Device attestation** *(research)*: Investigate platform-specific attestation (Android SafetyNet/Play Integrity, Apple DeviceCheck) to prove content originated from a real device, not a script. Privacy implications need careful evaluation.
  3. **C2PA metadata** *(stretch)*: For photos/videos, embed C2PA (Content Credentials) provenance data proving the media was captured on a real camera and not AI-generated. Requires integration with the C2PA Rust SDK.
  *(Backend — Track A, research phase)*

---

### Track B — Profile, Lenses & Communities 🎨
*Agent focus: Frontend React components, UI screens, CSS, map integration. No AP dependency for most items — build against local data and P2P peers.*

#### UF. User Feedback — Critical UX Overhaul (2026-03-14) 🔥
*Direct user feedback. These take priority over new feature development. Items marked ⚡ are bugs in "completed" features that need immediate fixing. Items marked 🔄 are design philosophy changes that require rethinking existing implementations.*

> **Philosophy shift — Filters, not Lenses:** The Gallery/Reader "lens" concept is being replaced with **feed content-type filters**. Instead of switching to entirely different views, users filter the main feed by content type (images, videos, articles, short text, etc.) and the feed adapts its card layout to optimize for that type. The feed stays the feed — it just shows different slices.

- [ ] ⚡ **UF.1 Layout & Spacing Overhaul**: Default to fullscreen on launch. Sidebars, content area, and panels should fill available width. Feed tabs and filters align with the edges of the content section. All panels and content adapt fluidly to window resize. Currently the app wastes horizontal space — fix CSS grid/flex to be full-width responsive. *(Frontend — Track B)* ⚠️ *Sprint v5 Update (2026-03-19): Window maximized on launch FIXED (`5213f01`). Remaining: full-width CSS grid/flex responsive layout for sidebars, panels, content area.*
- [x] ⚡ **UF.2 Feed Post Card Sizing**: Posts are too narrow in height. Post cards should be more symmetrical (closer to square), and images/video previews should show the full media rather than cropping aggressively. Adjust `FeedEntryCard` aspect ratios and image container styling. *(Commit: `f08b001`. Aspect ratio changed to 4/3.)*
- [ ] ⚡ **UF.3 Friends List Shows Only Chatted Users**: The friends/contacts list in the Social tab only shows users you've chatted with, not all connected contacts. It should show **all** contacts (Bluesky follows, P2P peers, imported contacts) with a chat initiation button next to each. *(Frontend + Backend — Track B + A)* ⚠️ *Sprint v5 Update: Chat button added to Bluesky follows list (`36a8df6`). getFollowers 401 fixed via public API relay (`e9a36ca`). Remaining: archive-imported contacts (Facebook friends, Instagram follows) not yet merged into unified contact list.*
- [x] ⚡ **UF.4 Feed Friends Panel — Missing Chat + Profile Actions**: No chat button next to user names on the feed sidebar friends list. Clicking a profile does nothing. Fix: chat icon → opens DM, profile click → navigates to user's profile (Bluesky profile, AP actor page, or Bytescape peer profile). *(Commit: `f08b001`. Chat handler + DID wiring in FriendsPanel.)*
- [x] 🔄 **UF.5 Following/Subscribed Tab Dedup**: "My Contacts" section on the Following view is a worse version of the friends tab. "Subscribed" is a copy of "All" sidebar. Redesign: merge contacts into the Social tab (its home), collapse subscribed into All with subscription-source filters instead of a separate tab. *(Frontend — Track B)* ✅ *Sprint v4 Audit: Subscribed tab now has a DISTINCT purpose — fetches only custom feeds (`list_saved_feeds`) + followed hashtags, properly wired to `get_unified_feed`. 4-tab system: All (firehose), Following (social graph), Subscribed (curated feeds/hashtags), Saved (bookmarks). Commit: `277c3a0`.*
- [x] 🔄 **UF.6 Feed Content-Type Filters (Replaces Lenses)**: Replace the Gallery/Reader lens toggle with **content-type filter chips** on the feed toolbar. Filters: 📝 Text · 📸 Images · 🎬 Video · 📰 Articles · 🔗 Links. Each filter adjusts the feed query to only show items of that type and optimizes the card layout (image filter → larger thumbnails in a grid; article filter → headline + excerpt + reading time; video filter → video player cards). Add a 🌐 Source filter (Bluesky / ActivityPub / Local / Archives / All). Filters are combinable. This replaces B1.1, B1.2, and B1.3. *(Frontend — Track B, Backend content-type tagging — Track A)* ✅ *Sprint v4 Audit: Implemented as 6 media lens toggle buttons (All / Images / Video / Short / Long / Articles) on FeedScreen. Source filter also present as a platform dropdown. Not exactly "filter chips" visually, but functionally equivalent. Commits: `69aa92f`, `a031b09`.*
- [x] ⚡ **UF.7 Article Filter — Content Quality**: The article/reader view shows YouTube videos, GIFs, and links that aren't articles. The article filter must only show: blog posts, news articles, long-form text posts (>500 chars), and `BytescapeType::Article` items. Exclude: video embeds, GIF-only posts, short link shares, image-only posts. Backend needs a `content_classification` field or heuristic to tag items. *(Content — Track C, Backend — Track A)* ✅ *Sprint v4 Audit: `classify_feed_entry` restored in `feed_aggregation.rs` (line 85). Items now properly classified as Status/Gallery/Article/Link/Video based on content heuristics. Commit: `77702d9`.*
- [x] ⚡ **UF.8 Profile Builder Broken**: "Customize" on the profile shows a blank screen that says "Body" on hover. GrapesJS is not initializing properly. Fix: ensure GrapesJS editor loads with interactive toolbars, add-block buttons, drag-and-drop panels, and configuration dropdowns for size/shape/color. The builder must be usable by a non-technical user. *(Commit: `f08b001`. Removed `panels: { defaults: [] }` override, restored default toolbars + style manager.)*
- [ ] 🔄 **UF.9 Profile Page Rework**: Profile posts are too wide — make them narrow cards. Split profile feed into two tabs: **Wall** (posts by/to the user — pinnable, guestbook-like) and **Posts** (user's posts on the wider web). Both tabs have the same filter chips as the main feed (UF.6). Remove the stats bar (media count, vault files, storage). Move network/source display into a filter box on the Posts tab. Add verified account badges for linked external accounts. *(Frontend — Track B)*
- [ ] 🔄 **UF.10 Social Module Redesign**: The Social tab is the weakest module. Current "Browse Profiles" opens a redundant second browser bar below the main Omnibar — remove it. Redesign the Social tab to focus on: ① managing followers/following across all platforms, ② webring membership and discovery, ③ friend search and discovery, ④ cross-platform contact management. The sidebar icons (browse, search, add/remove) are closer to the right UX — expand on that pattern rather than the current profile browser approach. *(Frontend — Track B)*
- [ ] 🔄 **UF.11 Settings Consolidation**: Merge "Account" and "Connected Accounts" into one section — Bytescape identity as parent, linked accounts below with "Link New Account" button. Add sections: default search settings (search engine, scope), privacy settings (global search visibility, friend request policy), browser customization (ad blocker toggle, privacy level). Move "Bluesky Feeds" management out of Settings and into the Feeds screen directly. Evaluate whether "Device Sync" is still needed (P2P node pairing covers this) — if redundant, remove. *(Frontend — Track B)* ⚠️ *Sprint v4 Audit: PARTIAL — Account + Connected Accounts merged, green ✓ indicator for linked accounts, "Connect Account" button with green plus added. Search/privacy/browser settings NOT added. P2P Connections section still present (user questions its relevance).*
- [x] ⚡ **UF.12 Import Screen UX Fix**: ① Content is crushed to upper-left — center it. ② Remove redundant "View Feed" button. ③ Change "Shred Surveillance & Save Memories" → "Import Archive". ④ Change "Exorcism Complete" → "Import Complete". ⑤ Classify and define discovered trackers with categories and descriptions rather than just a count. *(Frontend — Track B)* ✅ *Sprint v4 Audit: AuditScreen fully rewritten with 3-terminal layout (Import/Memories/Trackers), ~20 hardcoded colors → CSS variables, centered layout, calm verbiage ("📦 Import & Protect"). Commits: `d642229`, `4aec883`. Minor: Some old verbiage may persist in edge cases.*
- [ ] ⚡ **UF.13 Facebook Import Quality**: Facebook import glitches the import screen and shows random activity instead of actual posts and media. Major rework needed in `connectors/facebook.rs` — prioritize posts with text/photos/videos, filter out low-value activity logs (likes, reactions, login events, ad interactions). *(Backend — Track A)* ⚠️ *Sprint v4 Audit: Backend agent added extensive filtering (50+ directory exclusions, lifecycle/activity log filtering) in facebook.rs (2,620 lines). Needs user verification — does the Facebook import now show real posts? Instagram connector also exists (507 lines) and needs testing against workspace archive.*
- [x] 🔄 **UF.14 Theme Customization Enhancement**: Users should be able to click any color swatch in a theme and change it individually (custom color picker per CSS variable). Support importing images/GIFs as backgrounds. Provide default animated background options. Themes become fully user-customizable, not just preset switching. *(Content — Track C)* ✅ *Sprint v4 Audit: ThemeCustomizer.tsx (658 lines) — per-color picker for 21 CSS variables across 4 groups (Backgrounds, Accents, Text, Links). 7 background presets (Aurora, Sunset, Ocean, Warm Mesh, Matrix, Retro Grid). File upload (4MB limit) + custom URL input. Tile/cover mode toggle. 20 tests. Commits: `ce5e130`, `e0606a1`.*
- [ ] 🔄 **UF.15 Export Manager & Account Deletion**: Add "Export" option before "Clear Vault" — detailed export manager to export whole or partial vault + profile data as a ZIP file in JSON format. Rename "Clear Vault" to "Delete Account" — permanently wipes all data, unlinks all connected accounts, removes identity from the platform. *(Frontend + Backend — Track B + A)* ⚠️ *Sprint v5 Update: Export wired — frontend button invokes `export_vault_data` with save dialog (`aab39db`). Remaining: account deletion (wipe all data, unlink accounts, remove identity).*
- [x] ⚡ **UF.16 Feed Refresh Performance**: Feed takes tens of seconds to refresh, appearing stalled. Investigate: timeline fetch timeout, redundant API calls, missing pagination, heavy deserialization. Add visible loading indicators and progressive rendering (show cached items immediately, update when fresh data arrives). *(Backend — Track A)* ✅ *Sprint v5 (2026-03-19): Resolved. Root causes: (1) OAuth Mutex held across network calls → changed to `RwLock<Option<Arc<>>>` (commit `fd3ebd1`). (2) Serial custom feed fetches → `futures::future::join_all` for parallel. Feed load: 29s→~2s. Batch blob resolution (`623d3c6`) + parallel IPC in frontend. Progressive video poster display (`1e0386e`).*

#### B1. ~~Lenses & Content Views~~ → Feed Content Filters *(reworked per UF.6)*
- [x] ~~**B1.1 Lens: Gallery**~~: *(Implemented but user feedback: "worst possible version of Instagram." Replaced by UF.6 Image content filter with optimized grid layout.)*
- [x] ~~**B1.2 Lens: Reader**~~: *(Implemented but user feedback: "totally busted — shows YouTube videos, GIFs, links." Replaced by UF.6 Article content filter with UF.7 quality filtering.)*
- [x] ~~**B1.3 Item-Type Rendering**~~: *(Implemented but user feedback: lens paradigm is wrong — should be feed filters, not separate views. Replaced by UF.6.)*

#### B2. Profile Builder & Theming
- [x] **B2.1 GrapesJS Integration**: Embed GrapesJS drag-and-drop module builder in `ProfileBuilderScreen`. Outputs clean HTML/CSS stored locally in user's Willow store. Requires `grapesjs` npm package. ⚠️ *User feedback: shows blank screen — see UF.8 for required fixes.*
- [x] **B2.2 Raw HTML Override**: Upload custom HTML/CSS/JS directly to Willow store. Rendered in a sandboxed `<iframe>` to prevent XSS.
- [x] **B2.3 DID Meta Tags**: Emit `<meta name="bytescape:did">` and cross-platform subscription link tags for client-side DID resolution.
- [x] **B2.4 Widget System**: Away message, music player, friends list, guestbook, visitor counter, post feed, media grid — live React widget hydration from GrapesJS blocks, with persistent configuration via Tauri commands and redb storage.
- [x] **B2.5 Theme Engine**: Three built-in themes — **Cozy Mode** (GeoCities default), **Desktop Mode** (Win98 silver/teal, sharper bevels, system font, no stars), **Midnight Mode** (deep blues, soft purples, rounded corners). `ThemeId` type, `useThemeStore` (Zustand + localStorage persistence), `useTheme()` hook applies `data-theme` attribute on `<html>`. `ThemePicker` component in SettingsScreen Appearance card with swatch previews and instant switching. Ghost CSS variables (`--accent-purple`, `--bg-primary`, etc.) defined with defaults. 5 ThemePicker tests. ⚠️ *User feedback: needs per-color customization + animated backgrounds — see UF.14.*
- [x] **B2.5b Hacker Theme**: `[data-theme="hacker"]` — off-black (#0a0a0a), bright green (#00ff41), Courier New monospace, no border radius, full CSS variable set. Selectable via ThemePicker. *(Sprint v3 delivery)*
- [x] **B2.5c ThemeCustomizer**: Per-color picker for 21 CSS variables (4 groups: Backgrounds, Accents, Text, Links), 7 background gradient presets, image/GIF upload (4MB limit) + custom URL, tile/cover mode toggle, reset controls. 20 tests. *(Content agent, commits: `ce5e130`, `e0606a1`)*
- [ ] **B2.6 Dual-Format Serving** *(unlocked by Track A — A1.1)*: Profile URL serves HTML to browsers and Atom/Lexicon to aggregators simultaneously via `<link rel="alternate">`.
- [ ] **B2.7 "Memory" Post Type** *(tentative, low priority)*: Vault item resurfacing with "On This Day" semantics and optional syndication as a nostalgia post. Details TBD when we get to it.

#### B3. ~~Peer Profile Browser~~ → Social Module *(reworked per UF.10)*
- [x] ~~**B3.1 Profile Browser Tab**~~: *(Implemented but user feedback: opens redundant second browser bar, Social module is weakest — needs redesign. See UF.10.)*
- [x] ~~**B3.2 Sandboxed iframe Renderer**~~: *(Implemented. Renderer code may be reused in the redesigned Social module.)*
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

- [x] **B10.1 Internal URL Scheme**: Define and register internal URL addressing for all Bytescape screens. Mapping: `/feed`, `/feed/following`, `/feed/feeds/{feedUri}`, `/feed/hashtags/{tag}`, `/chat/bsky/{convoId}`, `/chat/p2p/{nodeId}`, `/vault`, `/vault/{blobHash}`, `/profile`, `/profile/@{handle}`, `/profile-builder`, `/import`, `/settings`, `/article/{itemId}`, `/map`. Create a `useLocation()` hook (Zustand) that exposes `currentUrl` and `navigate(url)`. All screen transitions go through this hook so the URL always reflects the visible page. *(useLocation.ts — resolveUrl/buildUrl + useLocationStore with syncFromNavStore bridge, 45 tests)*
- [x] **B10.2 Navigation History Stack**: Browser-style forward/backward navigation. `useNavigationHistory` Zustand store maintaining a stack of visited URLs with a cursor index. `goBack()`, `goForward()`, `canGoBack`, `canGoForward` exposed to the UI. Max depth ~50 entries. Each `navigate()` call pushes onto the stack. *(Integrated into useLocationStore — history array + cursor, back/forward with trim-on-branch, max 50 depth)*
- [x] **B10.3 Omnibar UI**: Evolve the existing `TopBar` search bar into a browser-style omnibar. Shows the current page URL (from B10.1 scheme) in a styled address bar. Editable — user can type a URL to navigate directly, or type text to search. Auto-completes known routes and recent URLs. Flanked by ◀ Back / ▶ Forward buttons. Shows a 🏠 local / 🌐 relay / 🔗 peer icon indicating content origin. The bar has the classic beveled-inset browser field look consistent with the GeoCities aesthetic. *(Omnibar.tsx \u2014 replaces TopBar search bar, wired to useLocationStore)*
- [ ] **B10.4 Reload / Refresh**: Reload button next to the omnibar triggers a full re-fetch of the current screen's data sources. Each screen exports a `refresh()` callback (or listens for a `byte://reload` event). Feed screens re-fetch timeline, vault re-queries blob list, profile re-fetches actor data, chat re-polls conversations. *(Frontend agent — Track B)*
- [ ] **B10.5 URL Deep Linking** *(unlocked by Track A — A1.1 + hosting)*: When `bytescape.social` is live, the internal URL scheme maps to public HTTP URLs. `byte://profile/@alice` → `https://alice.bytescape.social`. The omnibar shows the public HTTPS URL when the user's relay mirror is active. Clicking "Copy Link" copies the public URL. Incoming links from external browsers resolve via the WASM portal (B9.3) or redirect to the app via `byte://` protocol handler (registered via Tauri deep-link plugin). *(Cross-track — A + B)*
- [x] **B10.6 URL Bar for Peer Browsing**: When visiting a peer's profile (B3.1), the omnibar shows `byte://peer/{nodeId}/profile`. Users can type a peer's node ID or `did:key` into the omnibar to navigate directly to their profile. Integrates with the Peer Profile Browser — omnibar becomes the primary navigation surface for traversing the peer graph. *(ProfileBrowserScreen navigateTo/goHome sync to useLocationStore, Omnibar detects hex node IDs)*
- [x] **B10.6b Multi-Tab System**: Browser-style tab management with `NavStore` (Zustand) — `openTab`, `closeTab`, `switchTab`, `navigateInTab`, `updateTabLabel`. `TabBar` component renders horizontal tab strip with close buttons, active tab highlighting. All tabs render simultaneously with `display: none` on inactive tabs (preserves React state for instant switching, no re-fetch). Supports 13 screen types including WebView tabs with URL metadata. *(Commits: `b9a007a`, `6e4f4d1`, `ca8846c`)*
- [x] **B10.6c Feed Performance Infrastructure**: Multi-layer caching: `CachedResponse<T>` generic + `FeedCache` on AppState (timeline 30s TTL, saved feeds 5min TTL), `SHARED_CLIENT` LazyLock with connection pooling (replaced 12 `Client::new()` calls), vault `header_cache` on DbManager with write-invalidation. Frontend `useUnifiedFeed` hook has stale-while-revalidate + 45s auto-refresh for live tabs. *(Backend commit: `822223d`)*
- [x] **B10.6d HLS Video Fix**: Tiered A→B→C FFmpeg strategy: (A) stream copy with `+genpts+igndts`, (B) audio re-encode, (C) full re-encode as last resort. All strategies include `-analyzeduration 10M -probesize 10M`. Strict ffprobe validation (stderr-clean, video stream present, duration > 0.5s). Cache hit validation auto-reconverts old broken MP4s. *(Backend commits: `18783bb`, `f7f3dda`, `a24e11c`, `b017091`, `6be698b`)*
- [x] **B10.6e Export Vault Command**: `export_vault_data` Tauri command (183 lines, `commands/export.rs`). Exports vault items, contacts, messages as JSON in ZIP. Media inclusion optional. Registered in `lib.rs`. Frontend wired via save dialog. *(Backend commit: `77702d9`, Frontend commit: `aab39db`)*
- [ ] **B10.7 External Web Browsing (Desktop Only)**: On desktop, the content area uses a **secondary Tauri `Webview`** (created via `WebviewBuilder::new()` with **no IPC access**) that can load arbitrary `http://`/`https://` URLs. The React shell (omnibar, sidebar) stays in the primary webview and controls the content webview via Tauri commands (`webview.navigate(url)`, `webview.go_back()`, `webview.go_forward()`). Content webview navigation events bubble up to update the omnibar URL display and history stack (B10.2). When viewing a `byte://` internal route, the content webview is hidden and the React screen renders directly; when navigating to an external URL, the React screen is hidden and the content webview is shown. **iframe fallback**: For niche cases like embedded profile widgets or whitelisted embeds, sandboxed `<iframe>` elements remain available within React components (e.g., embedding a Bandcamp player or a peer's profile widget). **Mobile**: External links open via the platform's in-app browser (SFSafariViewController / Chrome Custom Tabs via `tauri-plugin-opener`) — mobile stays focused on the social platform experience, not general browsing. *(Frontend + Backend — Track B)* ⚠️ *Sprint v4 Audit: PARTIALLY IMPLEMENTED. `commands/browser.rs` exists with 4 Rust commands (`create_browser_view`, `destroy_browser_view`, `resize_browser_view`, `hide_browser_view`). `WebViewScreen.tsx` invokes them. Compiles clean, 250 tests pass. Runtime failure: URLs don't load. Likely `get_window("main")` returns `None` due to window label mismatch. 5 approaches attempted — Rust WebviewBuilder is the correct one. Needs debugging, not rewriting.*
- [ ] **B10.8 Unified Search (Omnibar)**: The omnibar doubles as a federated search surface. When the user types text that isn't a URL or `byte://` route, it triggers a **tiered search** across multiple sources, prioritized in this order:
  1. **Bytescape Network** (default, highest priority): Search local vault items, contacts, P2P peers, and (when hosting is live) the Bytescape directory. Results include profiles, pages, articles, vault items.
  2. **Fediverse** (AT Protocol + ActivityPub): Search public content across all subscribed protocols — Bluesky actor search (`app.bsky.actor.searchActors`), AP WebFinger lookups, hashtag federation. Returns profiles, posts, communities.
  3. **Web** (lowest priority): Falls back to a configurable search engine (default: DuckDuckGo) for general web results. Results open in the content webview (B10.7 on desktop) or in-app browser (mobile).
  Results render in a unified dropdown below the omnibar, grouped by source with clear section headers (🏠 Bytescape · 🌐 Fediverse · 🔍 Web). **Filters**: User can prefix queries to constrain scope — `@linkinpark` (people only), `#rockmusic` (hashtags only), `site:bandcamp.com linkin park` (specific site via web search), or toggle filter chips: "Bytescape only", "Fediverse only", "Web only". **Example**: Searching "Linkin Park" → ① Linkin Park's Bytescape profile → ② Linkin Park fan page on Bytescape → ③ Linkin Park Bluesky/AP profiles → ④ linkinpark.com (web result). *(Cross-track — A + B + C)*
- [ ] **B10.9 Built-in Ad Blocker (Desktop)**: Bytescape is ad-free by philosophy — this extends to external web content viewed in the content webview (B10.7). Implement content filtering on the Rust side by intercepting the webview's resource requests (WebKitGTK `WebView::resource-load-started` / WebView2 `add_WebResourceRequested`) and blocking requests matching known ad/tracker domains. Ship a bundled filter list (EasyList subset or similar — curated for size). Block ad iframes, tracking pixels, and video pre-rolls where possible. **Scope**: Desktop only (mobile uses platform in-app browsers which have their own ad handling). **Research items**: Evaluate WebKitGTK vs WebView2 interception APIs for feasibility, determine filter list update mechanism (bundled vs. periodic fetch), assess performance impact of request-level filtering. *(Backend — Track B, research phase)*
- [ ] **B10.10 Privacy Shield (Desktop)**: Comprehensive anti-tracking for the content webview, layered on top of the ad blocker (B10.9). Bytescape protects user privacy even when browsing external sites:
  1. **Third-party cookie blocking**: Configure the webview cookie policy to reject all third-party cookies (WebKitGTK `WEBKIT_COOKIE_POLICY_ACCEPT_NO_THIRD_PARTY` / WebView2 equivalent). First-party cookies preserved so logins work.
  2. **Tracker filter list**: Extend B10.9 filter lists with EasyPrivacy or Disconnect.me tracker list — blocks known analytics endpoints (Google Analytics, Meta Pixel, etc.), tracking pixels (1×1 images with query-string IDs), and beacon requests.
  3. **Referrer stripping**: Set `Referrer-Policy: no-referrer` on the content webview so external sites can't see where the user came from.
  4. **URL hygiene**: Before navigating, strip known tracking parameters from URLs — `utm_source`, `utm_medium`, `utm_campaign`, `utm_content`, `utm_term`, `fbclid`, `gclid`, `mc_eid`, `igshid`, `twclid`. Implement as a URL-cleaning pass in the Rust navigate handler.
  5. **Fingerprint resistance** *(research)*: Investigate spoofing or randomizing canvas fingerprint, WebGL renderer strings, `AudioContext` output, and `navigator.hardwareConcurrency` in the content webview. This is cat-and-mouse territory — start with canvas noise injection via JS injection on page load. Evaluate impact on site functionality.
  6. **DNS-over-HTTPS** *(stretch)*: Route content webview DNS through a DoH resolver (Cloudflare `1.1.1.1` or NextDNS) to prevent ISP-level tracking. Requires system-level or webview-level DNS override — feasibility varies by platform.
  **Scope**: Desktop content webview only. Internal `byte://` screens don't load external resources, so they're inherently tracking-free. *(Backend — Track B, builds on B10.9)*

#### B11. Network & Interaction Polish
- [ ] **B11.1 Network State Feed**: `NetworkStateEntry` event type (profile updated, webring joined, import completed, group created). Feed injection with distinct visual treatment.
- [ ] **B11.2 Friend Zoom**: Per-connection closeness slider that filters feed by relationship strength.
- [ ] **B11.3 Haptics & Audio**: "Mechanical" (crisp clicks) and "Organic" (wood taps, paper) sound profiles via `tauri-plugin-haptics`.

---

### Track C — Rich Sharing Engine 📝
*Agent focus: ComposeBox enhancement, new content types, rich text processing, media pipeline. The goal is a full-featured posting system that rivals mainstream platforms for expressiveness while syndication targets remain Bluesky, AP, and local vault.*

#### C1. Rich Text & Facets
- [x] **C1.1 Rich Text Facet Parsing — Backend**: `richtext.rs` — `detect_facets()`, `parse_facets_for_bluesky()`, `parse_facets_local()` with UTF-8 byte offset calculation. 23 unit tests. Wired into `posting.rs` (publish_post_v2) and `syndicate.rs` (syndicate_vault_item) — facets auto-generated on Bluesky publish.
- [x] **C1.1b Rich Text Facet Rendering — Frontend**: `RichText` component renders facets as clickable @mentions, #hashtags, and hyperlinks in `FeedEntryCard` and thread replies. Facet TypeScript types added to `types/index.ts`. *(Commit: `38cf4c2`)*
- [x] **C1.2 Mention Autocomplete — Backend**: `search_mentions` Tauri command registered — searches local contacts by handle prefix, returns `MentionSuggestion { handle, did, avatar_url }`.
- [x] **C1.2b Mention Autocomplete — Frontend**: `AutocompleteDropdown` UI in ComposeBox triggered by `@` — calls `search_mentions`, inserts selected handle. *(Commit: `38cf4c2`)*
- [x] **C1.3 Hashtag Autocomplete — Backend**: `search_hashtags_autocomplete` Tauri command registered — searches followed hashtags by prefix.
- [x] **C1.3b Hashtag Autocomplete — Frontend**: Dropdown UI in ComposeBox triggered by `#` — calls `search_hashtags_autocomplete`, inserts selected tag. *(Commit: `38cf4c2`)*
- [x] **C1.4 Character Count & Limits**: Per-target live counter in ComposeBox. Bluesky: 300 graphemes (via `Intl.Segmenter`). AP: configurable per-server (default 500). Show which targets will truncate. *(Commit: `23c7bbe`. 121-line hook, 28 tests.)*

#### C2. Link & Embed System
- [x] **C2.1 Link Card Embeds**: URL detection in compose text → fetch Open Graph / Twitter Card metadata → render preview card. Emit `app.bsky.embed.external` for Bluesky, `attachment` with `Link` type for AP. *(Commits: `9123e0b`, `93fde71`. unfurl.rs 166 lines, LinkPreviewCard 144 lines.)*
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

- [x] **C6.1 CRDT Storage Layer** *(prerequisite: C3.2 ships with Tiptap)*: `yrs` crate in backend, `collab.rs` with Yjs doc CRUD + state vector management + update encoding. `COLLAB_DOCS` redb table. `collab_doc_id: Option<String>` on `BytescapeItem`. Tauri commands: `create_collab_doc`, `apply_collab_update`, `get_collab_state`, `snapshot_collab_to_item` (accepts `content: &str` — frontend renders via Tiptap, Rust stores CRDT state + snapshots). *(Backend: GAP-4 commit `9c60417` + Content: `16a9c69`)*
- [x] **C6.2 Frontend Yjs Integration**: `useCollabDoc` hook (216 lines) manages Y.Doc lifecycle, debounced persistence (300ms batched via `Y.mergeUpdates()`), and snapshot function. `ArticleEditor` enhanced with `collaborative`/`collabDocId`/`itemId` props + `@tiptap/extension-collaboration`. 18 tests. Delivered within C6.1 scope. *(Commit: `16a9c69`)*
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
- [x] ~~**Feed Refresh Performance** *(UF.16)*~~: Multi-layer caching (FeedCache + SHARED_CLIENT + header_cache) *(Commit: `822223d`)*. Root cause of 29s load identified and fixed: (1) `OAuthManager.session` was `Mutex` held across entire network call — globally serialized ALL OAuth XRPC requests. Changed to `RwLock<Option<Arc<>>>`, clone Arc + drop lock before network. (2) `get_unified_feed_oauth` fetched N custom feeds in serial loop (10 × 2.5s = 25s) — replaced with `futures::future::join_all` for parallel fetches. *(Commit: `fd3ebd1`)*
- [ ] **Facebook Import Quality** *(UF.13)*: Backend filtering improved (50+ exclusions, facebook.rs 2,620 lines), but user hasn't verified results. Instagram connector (507 lines) also needs testing against workspace archive.
- [x] ~~**Content-Type Classification** *(UF.7)*~~: `classify_feed_entry` restored in `feed_aggregation.rs`. Items now classified as Status/Gallery/Article/Link/Video. *(Commit: `77702d9`)*
- [ ] **In-App Browser Tabs — Runtime Failure**: `commands/browser.rs` (4 Rust commands) + `WebViewScreen.tsx` compile clean and 250 tests pass, but URLs don't load at runtime. Likely cause: `get_window("main")` returns `None` (window label mismatch in `tauri.conf.json`). 5 approaches attempted (iframe, domain blocklist, OS window, JS WebviewWindow, Rust WebviewBuilder) — only the last is architecturally viable. Needs focused debugging, not another rewrite. ⚠️ *Sprint v5: WebViewScreen now has retry loop + error UI (`1890344`) so blank tabs show an error message + retry option instead of dead white screen. User still reports "loading page" hangs — underlying WebView URL load issue persists.*
- [ ] **App.css Bloat**: ~~6,477 lines~~ Reduced by ~250 lines in Sprint v5: theme CSS extracted to `src/themes/{geocities,win98,midnight,hacker}.css` (`573ec8e`), FeedEntryCard decomposed into `src/components/feed/` sub-components with React.memo (`106c2dc`). Still large — further component CSS module extraction needed.
- [x] ~~**Export Button Not Wired**~~: Backend `export_vault_data` command wired to frontend via save dialog. *(Backend: `77702d9`, Frontend: `aab39db`)*
- [x] ~~**Video Playback Jitter**~~: 5 root causes identified and fixed: (1) `-fps_mode cfr` incompatible with `-c:v copy` — removed from stream-copy strategies, (2) `+discardcorrupt` dropped valid frames at HLS segment boundaries — replaced with `+igndts`, (3) `-bsf:a aac_adtstoasc` wrong for re-encoded audio — removed from audio re-encode path, (4) `validate_mp4_with_ffprobe` too permissive — now rejects if ANY stderr at `-v error` + validates video stream presence + duration > 0.5s, (5) missing `-analyzeduration 10M -probesize 10M` — added to all strategies. Also fixed video filter bug: `save_posts()` now stores `video_url` in `media_paths` + adds `video/mp4` MIME. Cache invalidation: old broken MP4s auto-reconverted on access. *(Commits: `b017091`, `6be698b`)*
- [x] ~~**Settings Screen Black Screen**~~: Exponential localStorage growth from duplicate `addUserBackground` calls across 3 `useTheme()` consumers → `QuotaExceededError` crashed React 19 root. Fixed: idempotent `addUserBackground`, one-time restore flag, try/catch on all localStorage writes. *(Commit: `6ba6c10`)*
- [x] ~~**Video Not Centered in Fullscreen Gallery**~~: `.gallery-content` had fixed `width: 90vw` forcing widescreen container for all videos. Vertical/portrait videos stretched into horizontal aspect ratio with large black bars. Fixed: removed forced width, added `max-width`/`max-height` constraints on `.gallery-content > div` and `.gallery-content video` with `width: auto; height: auto` for natural aspect ratio adaptation. *(Commits: `e14d99c`, `6abfb71`)*
- [ ] **Chat Button on ChatScreen Friends List — Blocked on Backend**: Clicking a buddy's chat button on ChatScreen's right-panel friends list does nothing when no existing conversation exists. Root cause: frontend has no way to obtain a valid Bluesky `convo_id` for a new conversation. `chat.rs:251` has `get_or_create_convo()` which calls `chat.bsky.convo.getConvoForMembers`, but it's not exposed as a `#[tauri::command]`. **Requires Track A** to wrap this as a Tauri command (e.g., `get_or_create_bsky_convo(member_did: String) -> Result<String, String>`). Once available, frontend fix is trivial: click → invoke command → `setSelectedConvoId(result)`. ⚠️ *Sprint v5: Two frontend-only workarounds attempted and reverted (`705e4e2`→`c5c51f4`, `b835a86`→`e067671`) — confirms backend dependency is real. Do not retry frontend-only approaches.*
- [x] ~~**Window Not Maximized on Launch**~~: Fixed. `"maximized": true` added to `tauri.conf.json`. *(Commit: `5213f01`)*
- [x] ~~**Cargo Warnings**~~: 27 → 0 warnings. `#[allow(dead_code)]` annotations, `pub(crate)`, `_` prefixes for unused variables. *(Commit: `315cb8f`)*
- [x] ~~**Archive Import Deduplication**~~: Re-importing same archive no longer creates duplicates. Stable `original_id` computed from `hash(timestamp|content_200chars|media_uris)` via `DefaultHasher`. DB key becomes `source:stable_id` — identical across re-imports. *(Commit: `d1a662f`)*
- [x] ~~**Bluesky getFollowers 401**~~: `PUBLIC_XRPC` pointed to `bsky.social/xrpc` (now requires auth). Changed to `public.api.bsky.app/xrpc`. Affects `getFollowers`, `getFollows`, `resolveHandle`. *(Commit: `e9a36ca`)*
- [x] ~~**Blob Consistency Check**~~: Background task on startup (10s delay) scans all items with `media_blobs`, verifies each hash in Iroh. Auto-heals broken references. *(Commit: `d1d1855`)*
- [ ] **Source Filter Bug (NEW — Sprint v5)**: "Sources" dropdown on the feed does not properly filter Facebook posts (shows as Bluesky). Returning to "All" after filtering only shows user's own posts instead of full feed. Likely a source field mapping issue in the filter logic or a state reset bug.
- [ ] **Image Scroll Jitter (NEW — Sprint v5)**: Images somewhat jittery when scrolling through the feed. May be related to lazy blob resolution or layout shifts during image load.

### A0 OAuth — Known Gaps (post-merge follow-ups)
*Identified by PM audit of Track A delivery. These are not bugs — they are documented scope gaps in the initial OAuth implementation that need follow-up work.*

- [x] **A0-GAP-1 Ephemeral OAuth Sessions**: Persistent `FileSessionStore` with ChaCha20-Poly1305 encryption + Argon2id KDF. OAuth sessions survive app restarts. *(Commit: `45c6d04`)*
- [x] **A0-GAP-2 Fixed Callback Port**: Dynamic fallback to OS-assigned port (`127.0.0.1:0`) when `28457` is in use. *(Commit: `f2f279f`)*
- [x] **A0-GAP-3 OAuth Rust Tests**: 30 test functions covering FileSessionStore CRUD, OAuthManager construction, callback query-string parsing (full/error/empty/URL-encoded), XRPC URL parsing, session-absent error paths. *(Commit: `fe890b4`)*
- [x] **A0-GAP-4 Write Operations Not Wired for OAuth**: `publish_post_v2` and `syndicate_vault_item` now use `is_oauth_session()` branching. *(Commit: `9c60417`)*
- [x] **A0-GAP-5 DMs for OAuth Sessions**: Added `transition:chat.bsky` OAuth scope, `clone_with_proxy()` for isolated chat proxy headers, parallel chat import. *(Commits: `c29e549`, `ad6af24`)*
- [x] **A0-GAP-6 Saved Feeds / Unified Feed Empty for OAuth**: Fixed PDS URL vs AppView routing. *(Commit: `c684d84`)*
- [x] **A0-GAP-7 Transitive Crate Dependencies Verified**: `p256` confirmed transitive via `atrium-oauth` (zero direct imports — correct). `urlencoding` confirmed in `Cargo.toml`. *(Verified in remediation commit `fe890b4`)*

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
