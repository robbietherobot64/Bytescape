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
*Cross-cutting audit findings addressed across all tracks. Technical Debt Audit (2026-04-02) surfaced additional HIGH-severity items marked with 🛡️.*

- [x] **CRIT-4 P2P Rate Limiting**: `PeerRateLimiter` on `BytescapeProtocol` — max 3 concurrent connections per peer, 60 req/min sliding window, 64KB payload enforcement, stale entry eviction.
- [x] **CRIT-5 CORS Restriction**: Warp media server restricted to `tauri://localhost`, `https://tauri.localhost`, `http://localhost:1420` (no more `allow_any_origin`).
- [x] **CRIT-6 BLAKE3 Escrow Integrity**: Replaced MD5 with BLAKE3 for identity escrow bundle integrity hashes.
- [x] **HIGH-3 XSS Sanitization**: DOMPurify-based `sanitizeHtml()` (safe tags only) and `stripHtml()` utilities. Applied in `FeedEntryCard`, `QuoteRepostModal`. Codebase audited for `dangerouslySetInnerHTML` — none found.
- [x] **HIGH-5 Blob Hash Centralization**: `blob_hash.rs` with `parse_blob_hash()` and `normalize_blob_ref()`. Replaced all inline hash parsing across 5 files. 19 unit tests.
- [x] **MED-4 Input Length Validation**: ComposeBox hardened — 10K char limit (8K warning), 20 attachment cap, 100MB file size limit (50MB warning), color-coded counters, Post button disabled when over limits.
- [x] **MED-5 MIME Detection Expansion**: `infer` crate (0.16) replacing hand-rolled `mime_from_magic_bytes()` — 100+ file types. Extended `mime_from_path_hint()` with AVIF, SVG, ICO, audio formats.
- [ ] 🛡️ **SEC-1 Master Key Plaintext File** 🔥 *(Audit 3.3)*: `.master_key` stored as base64 on disk — any user-level process can read it and decrypt the entire keychain. **Fix:** Integrate OS keyring (Linux `secret-service`, macOS Keychain, Windows DPAPI) or use file-system ACLs + memory protection. *(Backend — Track A, Sprint v13)*
- [ ] 🛡️ **SEC-2 Hardcoded Recovery Code** 🔥 *(Audit 3.2)*: Auto-initialized identities use `"bytescape-auto-init"` — anyone with the escrow bundle can decrypt the Iroh secret key. **Fix:** Generate random recovery code on identity creation. *(Backend — Track A, Sprint v13)*
- [ ] 🛡️ **SEC-3 Secrets Never Zeroized** *(Audit 3.4)*: `master_key`, JWT fields, and other secrets are plain `String` — key material persists in freed heap memory. **Fix:** Add `zeroize` crate, derive `ZeroizeOnDrop` on secret-holding structs. *(Backend — Track A, Sprint v13)*
- [ ] 🛡️ **SEC-4 SSRF via JWT `aud` Claim** 🔥 *(Audit 4.3)*: Extracted URL from JWT `aud` used to construct API endpoint — crafted JWT can redirect requests to internal network. **Fix:** Validate URL against allowlist of known PDS hosts. *(Backend — Track A, Sprint v13)*
- [ ] 🛡️ **SEC-5 HTTP Instance URLs Accepted** *(Audit 4.6)*: Mastodon tokens sent in plaintext over HTTP connections. **Fix:** Enforce HTTPS for all Mastodon instance URLs. *(Backend — Track A, Sprint v13)*
- [ ] 🛡️ **SEC-6 XSS in OAuth Error Page** *(Audit 3.11)*: `error_description` rendered unescaped in OAuth error HTML. **Fix:** HTML-escape all interpolated values. *(Backend — Track A, Sprint v13)*
- [ ] 🛡️ **SEC-7 Non-Atomic Keychain Writes** *(Audit 3.7)*: Direct `fs::write` — crash mid-write corrupts keychain, causing total credential loss. **Fix:** Write to temp file then atomic rename. *(Backend — Track A, Sprint v13)*
- [ ] 🛡️ **SEC-8 SSRF in AP Inbox** 🔥 *(Audit 6.2)*: `fetch_actor_public_key` fetches arbitrary URLs from untrusted `keyId` field. No private-IP check or timeout. **Fix:** Add private-IP blocklist + request timeout. *(Backend — Track A, Sprint v14)*
- [ ] 🛡️ **SEC-9 No AP Digest Verification** *(Audit 6.3)*: Inbox receives `digest_header` but never verifies it against body hash — body substitution attack possible. **Fix:** Verify SHA-256 digest matches body. *(Backend — Track A, Sprint v14)*
- [ ] 🛡️ **SEC-10 Hand-Rolled SHA-256** *(Audit 6.4)*: 120+ lines of custom SHA-256 instead of `sha2` crate. **Fix:** Replace with `sha2::Sha256`. *(Backend — Track A, Sprint v14)*
- [ ] 🛡️ **SEC-11 AP Private Key Leak Risk** *(Audit 6.1)*: `ApActor` derives `Serialize` with `secret_key_hex` field — accidental serialization leaks private key. **Fix:** Mark field `#[serde(skip)]` or split into separate non-serializable struct. *(Backend — Track A, Sprint v14)*
- [ ] 🛡️ **SEC-12 SSRF in Link Unfurl** *(Audit 7.39)*: `unfurl_link` fetches user-provided URLs with no private IP / localhost check. **Fix:** Add private-IP blocklist. *(Backend — Track A, Sprint v14)*
- [ ] 🛡️ **SEC-13 JWT Refresh Race Condition** *(Audit 3.1)*: No mutex around check→refresh→save cycle — concurrent commands can both refresh, second invalidates first's token. **Fix:** Add `Mutex` around refresh. *(Backend — Track A, Sprint v13)*

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
- [x] **A1.2 Basic Mastodon/Lemmy Support**: Feed reading, posting, DMs via custom AP C2S/S2S client. Follow/Accept/Reject activities. *(Sprint v7 — `05d6d4b`)* ⚠️ *Sprint v8 Post-Mortem: Mastodon connector uses fake `mastodon:instance:id` identifiers in `did` fields (13 locations in `connectors/mastodon.rs`). Identity linking is implicit only. See A1.2b for remediation.*
- [x] **A1.2b Identity Architecture Remediation** 🔥: *(Partially complete — Sprint v8, completed Sprint v11)* Sprint v8 added `LinkedAccount` data structure, `master_did` root identity, formal identity linking in keychain, Settings screen identity UI with disconnect buttons + Mastodon connect. Sprint v10 added platform selector and Bytescape identity branding. **Sprint v11 (commit `baf72a4`):** (1) ✅ Removed `generate_platform_plc()` + `base32_lower_encode()` — Mastodon accounts now use `acct:user@instance` as canonical identifier. Startup migration converts existing fabricated `did:plc` entries. 8 new Rust tests. (2) ✅ AP actor DID derivation already fixed in Sprint v8 — verified `did_key_from_ed25519_bytes()` used consistently. (3) ✅ Simplified `get_owner_did()` from 5-level to 3-level hierarchy (`did:web` → `did:plc` real Bluesky only → `did:key`).
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
- [ ] **A2.7 Groups (AP `Group` Actor)**: Federate local groups as AP `Group` actors (FEP-1b12 / Lemmy-compatible). Three visibility modes: Public (auto-join), Closed (mod approval), Private (invite-only). `Follow` → `Accept` for joining; posts sent as `Create/Page` to Group inbox; Group wraps in `Announce` and distributes to followers. Bidirectional federation with Lemmy/PieFed communities. See B4 for the full data model and topic hierarchy.
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
- [ ] ⚡ **UF.3 Friends List Shows Only Chatted Users**: The friends/contacts list in the Social tab only shows users you've chatted with, not all connected contacts. It should show **all** contacts (Bluesky follows, P2P peers, imported contacts) with a chat initiation button next to each. *(Frontend + Backend — Track B + A)* ⚠️ *Sprint v6 Update: Chat initiation now fully functional — `get_or_create_bsky_convo` backend command (`bc5fcf6`) + frontend wiring (`4a02200`, `748d181`) enables clicking any Bluesky contact to start a new DM. Remaining: archive-imported contacts (Facebook friends, Instagram follows) not yet merged into unified contact list.*
- [x] ⚡ **UF.4 Feed Friends Panel — Missing Chat + Profile Actions**: No chat button next to user names on the feed sidebar friends list. Clicking a profile does nothing. Fix: chat icon → opens DM, profile click → navigates to user's profile (Bluesky profile, AP actor page, or Bytescape peer profile). *(Commit: `f08b001`. Chat handler + DID wiring in FriendsPanel.)*
- [x] 🔄 **UF.5 Following/Subscribed Tab Dedup**: "My Contacts" section on the Following view is a worse version of the friends tab. "Subscribed" is a copy of "All" sidebar. Redesign: merge contacts into the Social tab (its home), collapse subscribed into All with subscription-source filters instead of a separate tab. *(Frontend — Track B)* ✅ *Sprint v4 Audit: Subscribed tab now has a DISTINCT purpose — fetches only custom feeds (`list_saved_feeds`) + followed hashtags, properly wired to `get_unified_feed`. 4-tab system: All (firehose), Following (social graph), Subscribed (curated feeds/hashtags), Saved (bookmarks). Commit: `277c3a0`.*
- [x] 🔄 **UF.6 Feed Content-Type Filters (Replaces Lenses)**: Replace the Gallery/Reader lens toggle with **content-type filter chips** on the feed toolbar. Filters: 📝 Text · 📸 Images · 🎬 Video · 📰 Articles · 🔗 Links. Each filter adjusts the feed query to only show items of that type and optimizes the card layout (image filter → larger thumbnails in a grid; article filter → headline + excerpt + reading time; video filter → video player cards). Add a 🌐 Source filter (Bluesky / ActivityPub / Local / Archives / All). Filters are combinable. This replaces B1.1, B1.2, and B1.3. *(Frontend — Track B, Backend content-type tagging — Track A)* ✅ *Sprint v4 Audit: Implemented as 6 media lens toggle buttons (All / Images / Video / Short / Long / Articles) on FeedScreen. Source filter also present as a platform dropdown. Not exactly "filter chips" visually, but functionally equivalent. Commits: `69aa92f`, `a031b09`.*
- [x] ⚡ **UF.7 Article Filter — Content Quality**: The article/reader view shows YouTube videos, GIFs, and links that aren't articles. The article filter must only show: blog posts, news articles, long-form text posts (>500 chars), and `BytescapeType::Article` items. Exclude: video embeds, GIF-only posts, short link shares, image-only posts. Backend needs a `content_classification` field or heuristic to tag items. *(Content — Track C, Backend — Track A)* ✅ *Sprint v4 Audit: `classify_feed_entry` restored in `feed_aggregation.rs` (line 85). Items now properly classified as Status/Gallery/Article/Link/Video based on content heuristics. Commit: `77702d9`.*
- [x] ⚡ **UF.8 Profile Builder Broken**: "Customize" on the profile shows a blank screen that says "Body" on hover. GrapesJS is not initializing properly. Fix: ensure GrapesJS editor loads with interactive toolbars, add-block buttons, drag-and-drop panels, and configuration dropdowns for size/shape/color. The builder must be usable by a non-technical user. *(Commit: `f08b001`. Removed `panels: { defaults: [] }` override, restored default toolbars + style manager.)*
- [ ] 🔄 **UF.9 Profile Page Rework**: Profile posts are too wide — make them narrow cards. Split profile feed into two tabs: **Wall** (posts by/to the user — pinnable, guestbook-like) and **Posts** (user's posts on the wider web). Both tabs have the same filter chips as the main feed (UF.6). Remove the stats bar (media count, vault files, storage). Move network/source display into a filter box on the Posts tab. Add verified account badges for linked external accounts. *(Frontend — Track B)* ⚠️ **User feedback (2026-03-28) — three-column profile layout:** Move "About Me", "Now Playing", and "Links" boxes to a **left column**. Put "Guestbook" in a **center column**. Put "Connections" in a **right column**. Remove the "Photos" section entirely (it doesn't make sense as a default profile section — can be replicated via the Posts tab). Image previews on profile page are broken (small box with X, no image loads).
- [ ] 🔄 **UF.10 Social Module Redesign**: The Social tab is the weakest module. Current "Browse Profiles" opens a redundant second browser bar below the main Omnibar — remove it. Redesign the Social tab to focus on: ① managing followers/following across all platforms, ② webring membership and discovery, ③ friend search and discovery, ④ cross-platform contact management. The sidebar icons (browse, search, add/remove) are closer to the right UX — expand on that pattern rather than the current profile browser approach. *(Frontend — Track B)*
- [ ] 🔄 **UF.11 Settings Consolidation**: Merge "Account" and "Connected Accounts" into one section — Bytescape identity as parent, linked accounts below with "Link New Account" button. Add sections: default search settings (search engine, scope), privacy settings (global search visibility, friend request policy), browser customization (ad blocker toggle, privacy level). Move "Bluesky Feeds" management out of Settings and into the Feeds screen directly. Evaluate whether "Device Sync" is still needed (P2P node pairing covers this) — if redundant, remove. *(Frontend — Track B)* ⚠️ *Sprint v4 Audit: PARTIAL — Account + Connected Accounts merged, green ✓ indicator for linked accounts, "Connect Account" button with green plus added. Search/privacy/browser settings NOT added. P2P Connections section still present (user questions its relevance).* ⚠️ *Sprint v8: Major identity UI overhaul planned — wire disconnect buttons, add Mastodon connect, render all `LinkedAccount`s from keychain, truncate DIDs. See `frontend-sprint-v8.md` P4.* ⚠️ **User feedback (2026-03-28):** Identity section should NOT enumerate individual platforms (Mastodon, Bluesky) — instead, show a **single green "Connect Account" button** that opens a popup with platform selection and linking workflow. Same popup used everywhere an account link is needed (settings, profile icon dropdown, etc.).
- [ ] ⚡ **UF.12 Import Screen UX Fix**: ① Content is crushed to upper-left — center it. ② Remove redundant "View Feed" button. ③ Change "Shred Surveillance & Save Memories" → "Import Archive". ④ Change "Exorcism Complete" → "Import Complete". ⑤ Classify and define discovered trackers with categories and descriptions rather than just a count. *(Frontend — Track B)* ✅ *Sprint v4 Audit: AuditScreen fully rewritten with 3-terminal layout (Import/Memories/Trackers), ~20 hardcoded colors → CSS variables, centered layout, calm verbiage ("📦 Import & Protect"). Commits: `d642229`, `4aec883`. Minor: Some old verbiage may persist in edge cases.* ⚠️ **User feedback (2026-03-28) — REOPENED with new items:** ⑥ Import screen should be more animated — scrolling progress bar or rotating loading screen, perhaps ASCII art terminal-style animation. ⑦ When import is complete, auto-redirect to profile posts to see imported content displayed as a feed. ⑧ Consider adding a review process for posts with uncertain classification.
- [x] ⚡ ~~**UF.13 Facebook Import Quality**~~: Facebook import glitches the import screen and shows random activity instead of actual posts and media. Major rework needed in `connectors/facebook.rs` — prioritize posts with text/photos/videos, filter out low-value activity logs (likes, reactions, login events, ad interactions). *(Backend — Track A)* ✅ *Sprint v6 Verified: Parser verified against workspace archive (`facebook-rabiezaater-2025-01-12-TwiR2QHB`). HTML format correctly detected, CSS selectors match perfectly, `posts/your_posts` path matching covers all post files, noise filter handles lifecycle events/friend requests/empty posts/ad interactions (50+ exclusions). 125 media files extracted. Quality: GOOD. Only operational issue: archive on disk must be a ZIP (not pre-extracted folder). Instagram connector (507 lines) still needs testing. See Open Issues for detail.*
- [x] 🔄 **UF.14 Theme Customization Enhancement**: Users should be able to click any color swatch in a theme and change it individually (custom color picker per CSS variable). Support importing images/GIFs as backgrounds. Provide default animated background options. Themes become fully user-customizable, not just preset switching. *(Content — Track C)* ✅ *Sprint v4 Audit: ThemeCustomizer.tsx (658 lines) — per-color picker for 21 CSS variables across 4 groups (Backgrounds, Accents, Text, Links). 7 background presets (Aurora, Sunset, Ocean, Warm Mesh, Matrix, Retro Grid). File upload (4MB limit) + custom URL input. Tile/cover mode toggle. 20 tests. Commits: `ce5e130`, `e0606a1`.*
- [ ] 🔄 **UF.15 Export Manager & Account Deletion**: Add "Export" option before "Clear Vault" — detailed export manager to export whole or partial vault + profile data as a ZIP file in JSON format. Rename "Clear Vault" to "Delete Account" — permanently wipes all data, unlinks all connected accounts, removes identity from the platform. *(Frontend + Backend — Track B + A)* ⚠️ *Sprint v5 Update: Export wired — frontend button invokes `export_vault_data` with save dialog (`aab39db`). Remaining: account deletion (wipe all data, unlink accounts, remove identity).*
- [x] ⚡ **UF.16 Feed Refresh Performance**: Feed takes tens of seconds to refresh, appearing stalled. Investigate: timeline fetch timeout, redundant API calls, missing pagination, heavy deserialization. Add visible loading indicators and progressive rendering (show cached items immediately, update when fresh data arrives). *(Backend — Track A)* ✅ *Sprint v5 (2026-03-19): Resolved. Root causes: (1) OAuth Mutex held across network calls → changed to `RwLock<Option<Arc<>>>` (commit `fd3ebd1`). (2) Serial custom feed fetches → `futures::future::join_all` for parallel. Feed load: 29s→~2s. Batch blob resolution (`623d3c6`) + parallel IPC in frontend. Progressive video poster display (`1e0386e`).*
- [ ] 🔄 **UF.17 Profile Icon → Account Switcher Dropdown** *(User feedback 2026-03-28)*: Move the profile icon from the left side of the URL/navigation bar to the **top right** of the browser window, above the hamburger navigation menu and on the same horizontal level as the tabs section. It should function as a dropdown that lets you: (a) select which profile to view/act as (Bytescape, Bluesky, Mastodon, etc.), (b) link new accounts (opens the universal "Connect Account" popup from UF.11). Also add "Profile" as a menu navigation item in addition to the icon. *(Frontend — Track B)*
- [ ] 🔄 **UF.18 Vowel-less Platform Abbreviations** *(User feedback 2026-03-28)*: Establish a convention for filtering friends lists by platform: spell the platform name **without vowels**. blsky (Bluesky), mstdn (Mastodon), lmmy (Lemmy), pfd (PieFed), btscp (Bytescape), etc. Use this convention everywhere a friends/contacts list is present (feed sidebar, chat, social tab) with a platform filter. Every friends list should support filtering by platform. *(Frontend — Track B, design convention)*
- [ ] 🔄 **UF.19 Auto-Login to Linked Platforms** *(User feedback 2026-03-28)*: When visiting a profile on a platform (e.g., Bluesky) and the user has a linked account for that platform, they should be **automatically logged in** and able to interact. If the user has NO linked account on that platform: show a Bytescape placeholder profile with Bytescape interaction options (DM via P2P, follow via Bytescape), plus a toggle in the upper-left to view the original platform profile (read-only, logged out). *(Frontend + Backend — Track B + A, depends on B10.7 External Web Browsing)*
- [ ] 🔄 **UF.20 Feed Stability Overhaul** *(User feedback 2026-03-28)*: The feed rendering is fundamentally unstable — images blink, posts resize unpredictably, gaps appear between items, virtualization causes visual jitter. This needs a ground-up re-architecture of the feed rendering pipeline: stable content measurement before display, image pre-loading with placeholder dimensions, CSS containment (`contain: content`), debounced scroll handlers, and possibly switching from dynamic-height virtualization to fixed-height chunked rendering. This is a high-effort overhaul that touches VirtualizedFeedList, FeedEntryCard, BlobImage, and the overall feed layout. *(Frontend — Track B, High effort)*
- [ ] 🔄 **UF.21 Desktop PDS Hosting & Federation** *(User feedback 2026-03-28)*: User wants to host data on a desktop server instance/PDS and federate with the larger fediverse. This is already scoped in Phase 4.5 (Server Foundation) — confirms user priority alignment with the server deployment roadmap. Key deliverables: Tranquil PDS deployment, custom AP server, Iroh relay, identity resolution. *(Cross-track — Phase 4.5)*

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

#### B4. Groups, Topics & Location

Three-layer community system: **Topics** (hierarchical categories, like PieFed/old-style forums) → **Groups** (communities nested under topics) → **Posts** (BytescapeItems tagged to groups). Designed for bidirectional federation with Lemmy/PieFed communities via AP `Group` actor (see A2.7). Topic hierarchy is local-only (PieFed model — not federated); groups federate as AP `Group` actors.

##### B4a. Topic Hierarchy (local-only organizational tree)
- [x] **B4.1 Topic Data Model**: `Topic` record in redb — `id`, `name`, `slug`, `parent_id` (nullable → root), `description`, `icon`, `sort_order`, `created_by`, `created_at`. Adjacency-list tree structure. ✅ *Sprint v12 — `models/groups.rs` (133 lines, 10 types), commit `c431990`.*
- [x] **B4.2 Topic Tree Commands**: `create_topic`, `update_topic`, `delete_topic` (only if no groups assigned), `get_topic_tree` (returns full `Vec<TopicNode>` with nested children + group summaries + subscription state). ✅ *Sprint v12 — 7 Tauri commands in `commands/groups.rs`, commit `c431990`.*
- [x] **B4.3 Topic Subscriptions**: `subscribe_topic` / `unsubscribe_topic`. Subscribing to a topic includes all its sub-topics and groups in the Groups feed. `TopicSubscription` table in redb. ✅ *Sprint v12 — commit `c431990`.*
- [x] **B4.4 Default Seed Topics**: On first run, seed the topic tree with: Fediverse > {Bytescape > {Announcements, Feature Requests, Bug Reports, Groups (meta-topic for suggesting new topics)}, Lemmy, Mastodon, PieFed}, Technology > {Programming, Linux & FOSS, Security}, Health & Wellness, Art & Creative, Gaming, Science, News & Politics. ✅ *Sprint v12 — 18 seeded topics, `seed_default_topics()`, commit `c431990`.*

##### B4b. Groups (communities under topics)
- [x] **B4.5 Group Data Model**: `LocalGroup` record in redb — `group_id`, `name`, `slug`, `description`, `visibility` (Public/Closed/Private), `topic_id` (FK to Topic), `owner_did`, `icon_blob`, `banner_blob`, `rules` (markdown), `posting_restricted_to_mods`, `member_count`, `created_at`, `updated_at`. Three visibility modes: ✅ *Sprint v12 — commit `c431990`.*
  - 🔓 **Public** — searchable globally, auto-join on request
  - 🔒 **Closed** — searchable globally, join requires moderator approval
  - 🔐 **Private** — invite-only, not listed in search results
- [x] **B4.6 Group CRUD Commands**: `create_group`, `update_group`, `delete_group`, `get_group`, `list_groups_by_topic`, `search_groups` (full-text on name/description, excludes Private groups from non-members). ✅ *Sprint v12 — 6 Tauri commands, commit `c431990`.*
- [x] **B4.7 Group Membership**: `GroupMember` table — `group_id`, `member_did`, `role` (Owner/Moderator/Member), `status` (Active/Pending/Invited/Banned), `joined_at`. Commands: `join_group` (auto-Active for Public, Pending for Closed), `leave_group`, `approve_member`, `reject_member`, `invite_member`, `ban_member`, `set_member_role`, `get_group_members`. ✅ *Sprint v12 — core ops done (join/leave/members/memberships), approve/invite/ban deferred to v13, commit `c431990`.*
- [x] **B4.8 Group Feed**: `GroupPost` table links `BytescapeItem` entries to groups — `group_id`, `item_id`, `pinned`, `shared_by` (nullable DID for cross-posts), `posted_at`. Commands: `post_to_group`, `get_group_feed`, `get_groups_feed` (aggregated feed from all subscribed topics + joined groups), `pin_group_post`. ✅ *Sprint v12 — core feed ops done (post/share/get_group_feed/get_groups_feed), pin deferred to v13, commit `c431990`.*
- [x] **B4.9 Cross-Posting / Sharing**: `share_to_group` command — share any `BytescapeItem` to another group (creates a `GroupPost` with `shared_by` set). Share to timeline or DM handled by existing syndication/chat mechanisms. ✅ *Sprint v12 — commit `c431990`.*
- [ ] **B4.10 Group Moderation**: Group owner can pin posts, remove content, and delegate moderator roles. Closed groups have a pending-member approval queue. Ban list prevents re-joining.

##### B4c. Forum UI
- [x] **B4.11 ForumScreen**: Old-style forum landing page with collapsible topic hierarchy. Each topic row shows expand/collapse toggle, icon, name, [Subscribe] button. Groups listed under their parent topic with member count and visibility badge. [+ Create Group] button at bottom. ✅ *Sprint v12 — `ForumScreen.tsx` (217 lines) + `ForumScreen.css` (293 lines) + `useGroupsData.ts` (417 lines), commit `c431990`.* ⚠️ **User-observed (2026-03-27)**: Forum tab is visible and navigable, but displays only hardcoded mock data from `useGroupsData.ts` (`USE_MOCK = true`). No `invoke()` calls to Rust backend — the 21 groups commands are unreachable from the UI. Sprint v13 frontend P1/P5 must wire real data.
- [x] **B4.12 GroupScreen**: Individual group view with banner, visibility badge, member count, Join/Leave button, three tabs (Posts, About, Members). Posts tab reuses FeedEntryCard components. "New Post" button for members. ✅ *Sprint v12 — `GroupScreen.tsx` (173 lines) + `GroupScreen.css` (291 lines), commit `c431990`.*
- [x] **B4.13 Group Creation Modal**: Form with name, slug (auto-generated), description, visibility radio, topic picker, rules, icon/banner upload. Markdown preview for description and rules. ✅ *Sprint v12 — `CreateGroupModal.tsx` (183 lines) + `CreateGroupModal.css` (217 lines), commit `c431990`.*
- [ ] **B4.14 Groups Feed Tab**: "Groups" tab on FeedScreen aggregating posts from subscribed topics + joined groups. Group badge on each card links to GroupScreen. ⚠️ *Sprint v12 — type/label/empty-state scaffolded but tab button not rendered in FeedScreen tab bar; `get_groups_feed` not wired; share-to-group UI and group badges on cards not built.* ⚠️ **User-observed (2026-03-27)**: Entire forums/groups UI is non-functional — mock data only, no content from backend. This is the top priority for sprint v13 frontend.

##### B4d. Location Features (geo-tagging, separate from group hierarchy)
- [ ] **B4.15 Geolocation Field on `BytescapeItem`**: Add optional `geo: Option<GeoPoint>` (lat/lng + approximate geohash) to the universal item schema. Inspired by [Habitat](https://github.com/carlnewton/habitat).
- [ ] **B4.16 Location Picker UI**: Map-based coordinate picker in the compose flow. Privacy-first: coarsen to ~1 km geohash by default. Requires `leaflet` or `maplibre-gl` npm package.
- [ ] **B4.17 "Pin to Map" Action**: Existing vault items can be retroactively geotagged via a context-menu action.

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
- [ ] **B7.1 Gossip-Based Group Discovery**: Leverage `iroh-gossip` to propagate group metadata across peers in the same topic/group namespace.
- [ ] **B7.2 Iroh Doc Per Group**: Each group backed by an Iroh doc namespace for CRDT-based P2P replication of group posts among members.
- [ ] **B7.3 Federated Group Relay**: When AP federation is active (A2.7), bridge local Iroh group sync with federated `Announce` distribution to remote Lemmy/PieFed instances.
- [ ] **B7.4 Geohash Routing** *(optional, depends on B4d)*: Community Stations (Phase 5) index geo-tagged groups by geohash prefix for spatial queries.

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
- [ ] **B10.7 External Web Browsing (Desktop Only)**: On desktop, the content area uses a **secondary Tauri `Webview`** (created via `WebviewBuilder::new()` with **no IPC access**) that can load arbitrary `http://`/`https://` URLs. The React shell (omnibar, sidebar) stays in the primary webview and controls the content webview via Tauri commands (`webview.navigate(url)`, `webview.go_back()`, `webview.go_forward()`). Content webview navigation events bubble up to update the omnibar URL display and history stack (B10.2). When viewing a `byte://` internal route, the content webview is hidden and the React screen renders directly; when navigating to an external URL, the React screen is hidden and the content webview is shown. **iframe fallback**: For niche cases like embedded profile widgets or whitelisted embeds, sandboxed `<iframe>` elements remain available within React components (e.g., embedding a Bandcamp player or a peer's profile widget). **Mobile**: External links open via the platform's in-app browser (SFSafariViewController / Chrome Custom Tabs via `tauri-plugin-opener`) — mobile stays focused on the social platform experience, not general browsing. *(Frontend + Backend — Track B)* ⚠️ *Sprint v6 Update: Backend added explicit `"label": "main"` to `tauri.conf.json` + diagnostic logging in `browser_create` (`7b3acf8`). Frontend enabled in-app browser (`748d181`). Needs runtime verification — if URLs still don't load, terminal diagnostic output will pinpoint the failure.*
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
*These items require all three tracks to be substantially complete. They sit at the end of Phase 4. Server hosting is now fully scoped in Phase 4.5.*

#### Identity Consolidation & Recovery
*Identity linking is the critical foundation. Must be solid before building recovery flows on top.*

- [x] **Umbrella DID Model** *(Sprint v8)*: `master_did` (did:key:z6Mk... from Iroh Ed25519 keypair) is the universal root identity. Platform accounts (Bluesky `did:plc:`, Mastodon `acct:`, AP actor URLs) are formally linked children stored as `Vec<LinkedAccount>` in the encrypted keychain. Content ownership uses `master_did` for local content, platform-native identifiers for external content. See `identity-architecture-audit.md`.
- [ ] **2FA / Email Recovery**: Replace the paste-a-JSON-bundle flow with a user-friendly recovery method — email-based one-time code, TOTP authenticator app, or SMS fallback. The escrow bundle is stored server-side keyed by email; user proves ownership to retrieve it. *(Requires Phase 4.5 server infrastructure.)*
- [ ] **Multi-Device Identity Sync**: Same identity across phone + desktop via Iroh doc replication of the identity keychain. Requires secure key exchange protocol (QR pairing or one-time code). *(Requires Phase 4.5 Iroh relay.)*

> **🖥️ Hosting → Phase 4.5 (Server Foundation)**
>
> Server hosting has been expanded from the original "lightweight relay" concept into a full unified server deployment at `bytescape.social`. See **Phase 4.5** below for the complete server foundation scope: AT Protocol PDS, ActivityPub instance, Iroh relay, Sync Bridge, identity resolver, and Docker Compose packaging. The full architecture is documented in `.github/agents/prompts/systems-dual-engine-architecture-2026-03-25.md`.

---

## Phase 4.5: Server Foundation 🖥️🌐 (NEW)
**Goal:** Deploy `bytescape.social` as a unified server hosting AT Protocol PDS + ActivityPub instance + Iroh relay. Single Docker Compose stack. Serves identity resolution endpoints. Enough for the developer to migrate their own accounts as proof-of-concept.

**Duration:** 4-6 weeks with 2 agents parallel. **Invite-only launch.**

> **Architecture:** One server, dual protocol stacks. AT Proto PDS (Tranquil PDS — Rust, Postgres) and a custom `axum` AP server (extending existing 8-file, 1,636-line AP module) share identity resolution, storage, and the Sync Bridge to Iroh. Self-hosted initially on developer hardware behind Cloudflare Tunnel or Tailscale Funnel, with the same Docker Compose stack scaling to a commercial VPS when needed. Full architecture doc: `.github/agents/prompts/systems-dual-engine-architecture-2026-03-25.md`. Setup guide: `docs/SELF-HOSTING-GUIDE.md`.

> **Key Decisions:**
> - **PDS:** Tranquil PDS (Rust, Postgres, native `did:web` support, single binary). Official PDS (Node.js, SQLite) is the documented fallback. Repo: `tangled.org/tranquil.farm/tranquil-pds`.
> - **AP Instance:** Custom `axum` server extending the existing AP module. GoToSocial is the documented fallback if custom route proves too slow.
> - **Initial deployment:** Self-hosted on developer hardware. Docker Compose only — no managed cloud dependencies.
> - **Launch policy:** Invite-only (`PDS_INVITE_REQUIRED=true`).

### Identity Architecture

Every Bytescape user gets a `did:web:bytescape.social:{username}` as their universal umbrella identity. The DID Document links all protocol-specific identities:

| Protocol | Native Identifier | Resolution |
|----------|-------------------|------------|
| **Umbrella** | `did:web:bytescape.social:{username}` | `/.well-known/did.json` — ties everything together |
| **AT Protocol** | `did:plc:abc...` | Real PLC registered at `plc.directory`, service endpoint → `pds.bytescape.social` |
| **ActivityPub** | `https://bytescape.social/@username` | Standard AP actor. WebFinger: `acct:username@bytescape.social`. No DID fabrication. |
| **P2P / Iroh** | `did:key:z6Mk...` | Self-certifying from Ed25519 keypair. Used for P2P sync, device pairing, vault signing. |

**DID Hierarchy (replaces current 5-level `get_owner_did()` fallback):** ✅ *Implemented — Sprint v11 (`baf72a4`). `get_owner_did()` simplified to 3-level fallback. Fabricated `did:plc` removed. Startup migration converts existing keychains.*
```
1. did:web:bytescape.social:{username}     ← Set when username is chosen. Permanent.
2. did:plc:...                              ← ONLY real Bluesky DID (from PLC directory)
3. did:key:z6Mk...                          ← P2P/offline only (from Iroh Ed25519 keypair)
```

**Custom Domain Handles:** Users can bring their own domain as both an AT Protocol handle (`@alice.com` via `/.well-known/atproto-did`) and a fediverse handle (`@alice@alice.com` via WebFinger delegation). This makes personal websites the user's fediverse identity — a core differentiator.

### Infrastructure Tasks (Track I — new)

| ID | Task | Description | Effort |
|----|------|-------------|--------|
| **I-1** | Docker Compose scaffold | `caddy` + `postgres` + empty slots for PDS/AP/relay services. Caddy config with wildcard TLS for `*.bytescape.social`. | 2-3 days |
| **I-2** | Deploy Tranquil PDS | Configure and deploy Tranquil PDS pointed at Postgres. Verify XRPC health endpoint, account creation via `goat` CLI, WebSocket firehose. | 3-5 days |
| **I-3** | Deploy custom AP server | Build `axum` server binary extending existing AP module. Multi-user actor registry, persistent follower/following storage (Postgres), inbox POST handler, outbox GET, WebFinger resolution. Verify basic federation with external Mastodon instance. | 2-3 weeks |
| **I-4** | Identity resolver | Serve `/.well-known/did.json` generating `did:web` documents dynamically from user records. Serve `/.well-known/webfinger` routing to PDS (AT handle resolution) or AP server (acct resolution). | 3-5 days |
| **I-5** | Iroh relay daemon | Rust binary using `iroh` crate. Accepts encrypted blob uploads from edge clients, stores in content-addressed filesystem, serves on request. No decryption — blind relay only. | 1-2 weeks |
| **I-6** | Sync Bridge MVP | Internal daemon: (a) watches PDS firehose → packages as Iroh blobs for edge sync; (b) accepts Iroh blobs from edge clients → creates PDS repo records; (c) translates AP inbox messages → Iroh blobs for offline edge users. | 2-3 weeks |
| **I-7** | DNS + domain setup | Configure `bytescape.social` A records, wildcard subdomain, MX (for future email), Caddy auto-TLS. | 1 day |

### Backend Tasks (Track A)

| ID | Task | Description | Effort |
|----|------|-------------|--------|
| **A-S1** | Identity migration (keychain) | Remove `generate_platform_plc()`. Migrate `LinkedAccount.identifier` to platform-native IDs (`acct:user@instance` for Mastodon, actor URL for AP). Simplify `get_owner_did()` to 3-level hierarchy. Startup migration function + tests. *(Subsumes Open Issue: "Identity Architecture — 3 Critical Inconsistencies")* | 3-5 days | ✅ *(Sprint v11 — `baf72a4`)* |
| **A-S2** | Tauri ↔ Relay sync commands | New commands: `relay_connect`, `relay_push_blob`, `relay_pull_pending`, `relay_register_push_token`, `relay_sync_status`. Edge client pushes/pulls encrypted blobs to/from relay via existing `bytescape/1.0` ALPN. | 1-2 weeks | 🔶 Stubs registered (`baf72a4`), impl pending relay deployment |
| **A-S3** | PDS account provisioning | Tauri command `provision_pds_account` — creates account on Bytescape PDS via XRPC, stores credentials in keychain, updates `did:plc` if needed. | 3-5 days | 🔶 Stub registered (`baf72a4`), impl pending PDS deployment |
| **A-S4** | AP actor provisioning | Tauri command `provision_ap_actor` — creates account on custom AP server via API, links to `did:web`, stores credentials. | 3-5 days | 🔶 Stub registered (`baf72a4`), impl pending AP server deployment |

### Frontend Tasks (Track B)

| ID | Task | Description | Effort |
|----|------|-------------|--------|
| **B-S1** | DataProvider interface | Define `DataProvider` TypeScript interface (`getFeed()`, `getProfile()`, `sendPost()`, etc.). Implement `TauriDataProvider` wrapping existing `invoke()` calls. Backwards-compatible refactor — no behavior changes. | 1 week | ✅ *(Sprint v11 — `3b83ae8`)* |
| **B-S2** | Server account setup UI | Settings screen section: "Bytescape Social" — provision PDS + AP accounts, show migration status, link/unlink, custom domain setup wizard. | 3-5 days | ✅ *(Sprint v11 — `ba05daa`)* |
| **B-S3** | Relay status indicator | UI indicator showing relay connection status (connected/syncing/offline). Unread count from relay for items received while app was closed. | 2-3 days | ✅ *(Sprint v11 — `88b2d38`)* |

### Client Architecture: Adapter Pattern

The React frontend compiles into three targets from the same component tree via the `DataProvider` interface:

| Provider | Target | Data Source | Auth |
|----------|--------|-------------|------|
| `TauriDataProvider` | Desktop + Mobile (Tauri) | Local Rust backend via `invoke()` | Keychain (local) |
| `RestDataProvider` | Web app (bytescape.social) | Server REST API via `fetch()` | JWT/OAuth (server) |
| `WasmProvider` | Visitor portal (read-only) | Iroh WASM node fetching public blobs | None (public only) |

```bash
pnpm build:tauri      → TauriDataProvider injected, outputs Tauri bundle
pnpm build:web        → RestDataProvider injected, outputs static SPA
pnpm build:portal     → WasmProvider injected, outputs visitor portal
```

### Account Migration Flows

**Bluesky → Bytescape PDS:**
1. Deploy PDS at `pds.bytescape.social` → registers with Bluesky Relay (`bsky.network`)
2. User initiates migration via `com.atproto.server.requestAccountMove` on source PDS
3. Bytescape PDS imports full repo via `com.atproto.server.importRepo`
4. User signs PLC rotation operation → `plc.directory` updates service endpoint
5. Bluesky network routes all traffic through `pds.bytescape.social`

**Mastodon → Bytescape AP:**
1. User creates account on `bytescape.social` AP instance
2. AP actor document gets `alsoKnownAs: ["acct:user@old-instance"]`
3. User triggers `Move` from old instance → followers auto-transferred
4. (Optional) Export old archive → import into vault via existing pipeline

---

## Phase 5: Scale, Mobile & Web Portal 📱🌐 (MODIFIED)
**Goal:** Mobile native clients, account migration wizards, web portal, self-host packaging. Station Protocol for P2P mesh relay.

**Duration:** 8-12 weeks

### Mobile (Track M — new)

| ID | Task | Description | Effort |
|----|------|-------------|--------|
| **M-1** | Tauri Mobile lifecycle hooks | iOS `applicationWillResignActive` / Android `onTrimMemory` — trigger pre-suspend sync flush to relay within 25-second window. | 1 week |
| **M-2** | Push notification integration | APNs (iOS) + FCM (Android) registration with relay. Silent push to wake app for background sync. | 2 weeks |
| **M-3** | Responsive touch UI | CSS breakpoints, swipe gestures, pull-to-refresh, long-press context menus. Same React components. | 2-3 weeks |
| **M-4** | Mobile-optimized sync | Incremental sync with bandwidth awareness. WiFi: full sync. Cellular: headers-only, fetch bodies on demand. | 1 week |

### Account Migration (Track A)

| ID | Task | Description | Effort |
|----|------|-------------|--------|
| **A-M1** | Bluesky migration wizard | UI flow: connect existing account → export repo from source PDS → import to Bytescape PDS → sign PLC rotation → verify on network. | 2-3 weeks |
| **A-M2** | Mastodon migration wizard | UI flow: create alias on Bytescape AP instance → trigger Move from source → verify follower transfer. | 1-2 weeks |
| **A-M3** | Import existing archive | After migration, run existing archive import pipeline to pull full history into local vault. | 3-5 days |

### Web Portal (Track B)

| ID | Task | Description | Effort |
|----|------|-------------|--------|
| **B-W1** | `RestDataProvider` | HTTP `fetch()` implementation of DataProvider interface, talking to server REST API. | 1-2 weeks |
| **B-W2** | Server REST API | `axum` HTTP API serving same data as Tauri commands, for web clients. Auth via JWT. | 2-3 weeks |
| **B-W3** | Web SPA build | `pnpm build:web` — same React app with `RestDataProvider`, deployed as static assets behind Caddy. | 3-5 days |
| **B-W4** | WASM visitor portal | Read-only public profile viewer using Iroh WASM client to fetch public blobs. Blurred preview for non-authenticated visitors. | 2-3 weeks |

### Self-Host Packaging (Track I)

| ID | Task | Description | Effort |
|----|------|-------------|--------|
| **I-SH1** | One-command installer | `curl \| bash` installer — sets up Docker Compose, configures DNS prompts, generates secrets, starts stack. | 1 week |
| **I-SH2** | Admin CLI | `bytescape-admin` CLI for account management, invite codes, relay status, backup/restore. | 1-2 weeks |
| **I-SH3** | Backup/restore | Automated backup of Postgres, Iroh blob store, and config to local path or S3. Restore from backup. | 1 week |

### P2P Mesh & Station Protocol (retained from original Phase 5)

- [ ] **Station Protocol**: "Blind Relay" logic for encrypted store-and-forward. Extends the Iroh relay from Phase 4.5.
- [ ] **Collab Relay Extension**: Extend Station Protocol with Yjs document awareness relay and update buffering (enables C6.5 — always-on collaborative editing without simultaneous peer presence).
- [ ] **Auto-Docking**: Geohash-based discovery of nearest Community Station on account creation.
- [ ] **Erasure Coding**: Shard data across home station and adjacent regional nodes for redundancy.
- [ ] **Gossip Presence**: Real-time online status via `iroh-gossip` (implemented via Iroh relay).
- [ ] **Stealth Mode**: Toggle to sever all Station connections for pure device-to-device Iroh operation.

---

## Phase 6: Email Engine 📧
**Goal:** Integrate email as a first-class communication channel within Bytescape, starting with linked external accounts (IMAP/SMTP client) and progressing toward native `@username@bytescape.social` email addresses.

> **Philosophy:** Email is the oldest federated protocol. Adding it to Bytescape's multi-protocol vault unifies a user's entire communication history — social posts, DMs, and email — in one sovereign, local-first interface. The same Engine & Lens pattern applies: email data is normalized into the vault, displayed through a dedicated lens.

> **Phasing:** Mode A (linked accounts) ships first with zero server dependency. Mode B (AP messaging) leverages existing AP DM infrastructure. Mode C (hosted email) requires `bytescape.social` MX infrastructure from Phase 4/5.

### Mode A — Linked Email (IMAP/SMTP Client) 📬
*User links an existing Gmail/Outlook/ProtonMail/Fastmail account. Bytescape acts as a local email client — reads via IMAP, sends via SMTP. No server infrastructure needed.*

#### Backend (Track A)
- [ ] **E-A1 Email Connector**: `connectors/email.rs` — IMAP client using `async-imap` crate. Connect, authenticate (password/OAuth2/app-password), list folders, fetch headers (envelope + flags), fetch full message bodies on demand, IDLE for push notifications, IMAP SEARCH for server-side search. Add `Email` variant to `Source` enum.
- [ ] **E-A2 SMTP Sender**: SMTP client using `lettre` crate. Send new messages, reply (with `In-Reply-To` + `References` headers), forward with attachments. Support HTML + plaintext multipart bodies via `mail-builder`.
- [ ] **E-A3 Email Storage Tables**: Three new redb tables — `EMAIL_HEADERS_TABLE` (envelope data, threading references, flags, snippet), `EMAIL_BODIES_TABLE` (text/html bodies, fetched lazily on demand), `EMAIL_ACCOUNTS_TABLE` (linked account config, encrypted credentials in keychain). Pagination + folder filtering + search.
- [ ] **E-A4 Email OAuth2 Providers**: Gmail and Outlook OAuth2 flows for IMAP/SMTP access. Reuse localhost callback server pattern from Mastodon connector (`connectors/mastodon.rs`). Gmail scope: `https://mail.google.com/`. Outlook scope: `https://outlook.office365.com/IMAP.AccessAsUser.All`.
- [ ] **E-A5 Email Credential Storage**: Extend `LinkedAccount` in keychain with `protocol: "email"`. IMAP/SMTP credentials (password or OAuth2 tokens) stored via existing ChaCha20-Poly1305 encrypted keychain. Token refresh handling for OAuth2 providers.
- [ ] **E-A6 MIME Parser Integration**: `mail-parser` crate for RFC 5322 / MIME parsing — extract headers, text/html bodies, inline images, attachments. Thread reconstruction using `In-Reply-To` + `References` header chains (JWZ threading algorithm).
- [ ] **E-A7 IMAP Sync Engine**: Background sync with IMAP IDLE (push when app focused) + polling fallback (60s interval when minimized). Incremental fetch using UIDVALIDITY + HIGHESTMODSEQ. Emit Tauri events (`email-new-message`, `email-sync-complete`) for frontend reactivity. Configurable sync depth (last 30/90/365 days).
- [ ] **E-A8 Vault Bridge**: "Archive to Vault" action — email → `BytescapeItem` mapping (`Article` type for email bodies, `Gallery` for attachments). Attachments stored in Iroh blob cache (content-addressed, deduplicated). Archived emails appear in the main vault alongside social posts.
- [ ] **E-A9 Tauri Commands**: Register ~15 commands: `email_add_account`, `email_remove_account`, `email_list_accounts`, `email_list_folders`, `email_fetch_headers`, `email_fetch_body`, `email_send`, `email_reply`, `email_forward`, `email_search`, `email_mark_read`, `email_mark_flagged`, `email_move_to_folder`, `email_delete`, `email_sync_status`, `email_archive_to_vault`.

#### Frontend (Track B)
- [ ] **E-B1 EmailScreen**: Three-panel layout — folder sidebar (INBOX, Sent, Drafts, Spam, Trash, custom labels), thread list (sender, subject, snippet, date, unread indicator), message view (full email with thread history). Add `"email"` to `Screen` type and `SidebarItem` type. Singleton module tab.
- [ ] **E-B2 EmailComposer**: Modal/panel composer with To/CC/BCC fields (with contact autocomplete from unified contact graph), Subject line, rich text body (reuse Tiptap from ArticleEditor C3.2), attachment picker (reuse ComposeBox media attachment pattern). Support new message, reply, reply-all, and forward flows.
- [ ] **E-B3 EmailThreadView**: Threaded conversation display with collapsible messages. HTML email rendering in sandboxed `<iframe srcdoc>` with DOMPurify sanitization (reuse existing `sanitizeHtml` utility from HIGH-3). Remote image blocking by default (privacy) — load on explicit user click. Plaintext fallback toggle.
- [ ] **E-B4 Email Hooks**: `useEmailFolders()`, `useEmailThreads(folder, page)`, `useEmailMessage(uid)`, `useEmailCompose()` — standard Tauri `invoke()` pattern matching existing feed/chat hooks.
- [ ] **E-B5 Email Zustand Store**: `emailStore.ts` — folders, selectedFolder, threads[], selectedThread, draftState, linkedAccounts[], unreadCounts, syncStatus.
- [ ] **E-B6 Navigation Integration**: Add `"email"` to `Screen` and sidebar. Unread count badge on sidebar icon. Email tab opens as singleton module tab (same pattern as Feed, Chat).
- [ ] **E-B7 Email Account Settings**: Account linking UI in SettingsScreen — add/remove/configure email accounts. Provider presets (Gmail, Outlook, Yahoo, ProtonMail Bridge, Fastmail, custom IMAP/SMTP). OAuth2 flow for Gmail/Outlook, manual credentials for others. Per-account sync depth and notification preferences.
- [ ] **E-B8 HTML Email Sanitizer**: DOMPurify-based sanitization with strict allowlist. Strip `<script>`, `<iframe>`, `onclick`, tracking pixels, remote fonts. Block external image loading by default. CSP-enforced sandboxed rendering. Plaintext extraction fallback.
- [ ] **E-B9 Email Theme Styles**: CSS for all 4 themes (geocities, win98, midnight, hacker). Three-panel responsive layout. Thread list density options (compact/comfortable). Inline attachment previews.

#### Infrastructure
- [ ] **E-I1 Gmail OAuth2 App Registration**: Google Cloud Console → OAuth consent screen → client ID/secret. Gmail API restricted scopes. Note: Google verification process can take weeks.
- [ ] **E-I2 Outlook OAuth2 App Registration**: Azure AD → App registration for Microsoft Graph mail scopes.
- [ ] **E-I3 Provider Integration Testing**: End-to-end testing against Gmail, Outlook, ProtonMail Bridge, Fastmail, Yahoo. Verify IMAP IDLE, OAuth2 token refresh, folder hierarchy, search, flag sync.

### Mode B — AP-Address Messaging 💬 *(unlocked by A1.1 + hosting)*
*`@username@bytescape.social` addresses become usable for private ActivityPub messages. Not real SMTP email — but provides the "email-like" address that works across the fediverse.*

- [ ] **E-M1 AP DM as Email**: Route inbound AP direct messages (already handled by `inbox.rs`) to the Email tab in addition to Chat. User can choose which lens displays AP DMs.
- [ ] **E-M2 Email-Style AP Compose**: Allow composing AP DMs from the EmailComposer using `@user@instance` addressing. WebFinger resolution (already implemented) discovers the target inbox.
- [ ] **E-M3 Unified Inbox View**: Option to merge AP DMs and IMAP email into a single inbox view, filterable by source.

### Mode C — Hosted Email *(unlocked by Phase 5 — bytescape.social infrastructure)*
*Full `user@bytescape.social` email addresses with SMTP send/receive capability.*

- [ ] **E-H1 MX Infrastructure**: MX records on `bytescape.social` domain. Mail Transfer Agent (Postfix/Stalwart) receiving inbound SMTP. DKIM/SPF/DMARC for deliverability.
- [ ] **E-H2 Mailbox Storage**: Server-side mailbox storage for `@bytescape.social` accounts. IMAP access so the Bytescape client (Mode A connector) can read hosted mailboxes with no code changes.
- [ ] **E-H3 Anti-Spam**: Inbound spam filtering (rspamd or similar). Outbound rate limiting to protect domain reputation.
- [ ] **E-H4 Encryption at Rest**: All stored email encrypted with user's public key. Server cannot read mailbox contents (zero-knowledge where possible).

### Email Data Model

```
EmailAccount {
    id: UUID,
    owner_did: String,              // Links to master identity
    display_name: String,           // "Work Gmail", "Personal"
    email_address: String,          // user@gmail.com
    imap_host/port, smtp_host/port, // Server config
    auth_method: Password | OAuth2 | AppPassword,
    last_sync: u64,
    active: bool,
}

EmailHeader {
    uid: u64,                       // IMAP UID (unique per folder)
    account_id: String,
    message_id: String,             // RFC 2822 Message-ID
    in_reply_to: Option<String>,    // Threading
    references: Vec<String>,        // Full threading chain
    from, to: Vec<String>, cc, bcc,
    subject: String,
    date: u64,
    folder: String,                 // INBOX, Sent, etc.
    flags: Vec<String>,             // \Seen, \Flagged, etc.
    has_attachments: bool,
    snippet: String,                // First ~200 chars plaintext
    thread_id: String,              // Computed from References chain
}

EmailBody {                         // Fetched lazily from IMAP on demand
    uid: u64,
    account_id: String,
    text_plain: Option<String>,
    text_html: Option<String>,
    attachments: Vec<EmailAttachment>,
}

EmailAttachment {
    filename: String,
    mime_type: String,
    size: usize,
    blob_hash: Option<String>,      // Iroh blob if saved to vault
    inline: bool,                   // Content-Disposition: inline
}
```

### Rust Crate Stack
| Crate | Purpose | Maturity |
|---|---|---|
| `async-imap` | Async IMAP client (tokio) | Stable, 344K downloads |
| `lettre` | SMTP email sending | Very mature, 7.7M downloads |
| `mail-parser` | RFC 5322 / MIME parsing | Stalwart Labs, 1.3M downloads |
| `mail-builder` | RFC 5322 message construction | Stalwart Labs, pairs with parser |

### Integration with Existing Systems
| Existing System | Email Reuse |
|---|---|
| `LinkedAccount` in keychain | Add `protocol: "email"` with IMAP/SMTP credentials |
| `VaultMessage` with `source` field | Emails stored as `source: "email"`, threaded by conversation |
| `Source` enum in connectors | Add `Email` variant |
| `Screen` / `SidebarItem` types | Add `"email"` — singleton module tab |
| `BytescapeItem` | Emails archived to vault as `Article` type |
| Iroh blob store | Attachments stored content-addressed, deduplicated |
| Unified contact graph | Email addresses merged with social contacts |
| ChaCha20-Poly1305 keychain | IMAP/SMTP credentials encrypted at rest |
| DOMPurify sanitizer (HIGH-3) | Reused for HTML email rendering |
| Tiptap editor (C3.2) | Reused for rich text email composition |
| OAuth2 infrastructure | Reused for Gmail/Outlook authentication |

### Effort Estimates
| Scope | Effort | Dependencies |
|---|---|---|
| **Mode A (Linked Email)** — full local client | ~3–4 weeks (2 agents parallel) | None — zero server dependency |
| **Mode B (AP Messaging)** — AP DMs in email UI | ~3–5 days | A1.1 (AP inbox), bytescape.social hosting |
| **Mode C (Hosted Email)** — @bytescape.social | ~2–3 weeks + ongoing ops | Phase 5 server infrastructure |

---

## 🐛 Open Issues
- [x] ~~**Identity Architecture — 3 Critical Inconsistencies (Sprint v8)**~~ 🔥: Sprint v8 added `LinkedAccount` data structure, `master_did` root identity, and formal identity linking in keychain. Remaining fabricated `did:plc` cleanup (Mastodon `acct:` migration, removal of `generate_platform_plc()`) is **subsumed by Phase 4.5 task A-S1** (identity migration). Full audit: `.github/agents/prompts/identity-architecture-audit.md`. Architecture remediation: `.github/agents/prompts/systems-dual-engine-architecture-2026-03-25.md` §2.
- [x] ~~**Settings Screen Identity Section — Fully Resolved**~~: *(Sprint v8)* Disconnect buttons wired, Mastodon connect added, all `LinkedAccount`s rendered from keychain, DIDs truncated. *(Sprint v10)* Platform selector dropdown, Bytescape identity branding, Mastodon handle display fix. *(Sprint v11)* "Bytescape Social" section added with PDS/AP account provisioning status, migration status display, coming-soon badges. *(Phase 4.5 B-S2 — `ba05daa`)*
- [x] ~~**`USE_SYSTEM_BROWSER` Flipped Back to `false`**~~ ⚠️: Resolved in Sprint v9. WebView lifecycle properly managed via `browser_show`/`browser_hide`/`browser_show_at` Tauri commands. `USE_SYSTEM_BROWSER` constant removed — WebView tabs now render correctly via GtkOverlay reparenting.
- [ ] **Type Duplication**: `BlueskyPost` vs `BytescapeItem` in frontend—normalize into shared `FeedEntry` adapter (currently handled per-tab in `FeedScreen`).
- [ ] **Memory Leak on Video Filter** 🔥 *(User feedback 2026-03-28)*: Filtering feed by video causes 32+ GB memory usage. Only Instagram videos appear (not from linked platforms). Videos never fully load while memory skyrockets. Likely cause: video elements created but never destroyed during filter/scroll, or Iroh blob fetches accumulating without cleanup. Possibly related to HLS conversion pipeline holding large buffers. *(Backend + Frontend — Track A + B, Critical)*
- [ ] **CPU Spike on App Launch** 🔥 *(User feedback 2026-03-28)*: CPU spikes to ~100% when the app first launches, causing lag. Likely causes: blob consistency check (10s delay but still heavy), seed_default_topics running on every launch, Bluesky timeline pre-fetch, or Iroh node initialization. Need profiling to identify. *(Backend — Track A, Critical)*
- [ ] **Toggle/Checkbox Text Misalignment** *(User feedback 2026-03-28)*: Toggle buttons and checkboxes have their label text extremely misaligned to the right. The toggle itself is centered on screen when it should be left-aligned like all other text. Affects Settings and ForumScreen subscribe buttons. *(Frontend — Track B, CSS fix)*
- [ ] **Feed Gap & Image Sizing Inconsistency** *(User feedback 2026-03-28)*: Random gaps appearing between feed posts (not missing posts — just extra whitespace). Images inconsistently formatted — sometimes correct size, other times cut off in preview with post shrunk to 1/3 expected height. User says: "a fundamental rethink of the feed functionality is required." Likely related to VirtualizedFeedList dynamic height estimation, missing/broken image aspect ratio calculation, or inconsistent FeedEntryCard sizing. *(Frontend — Track B)*
- [ ] **Profile Page Image Previews Broken** *(User feedback 2026-03-28)*: Image previews on the profile page show only a small box with an X — no image loads. BlobImage resolution may be failing in profile context. *(Frontend — Track B)*
- [ ] **Instagram Posts Slow Initial Load** *(User feedback 2026-03-28)*: Instagram posts take a while to load on the feed on first view. After loading a couple times, they load quickly (cache warming). May need pre-caching strategy for imported archive media. *(Backend — Track A, performance)*
- [ ] **Video Playback Still Stuttery** *(User feedback 2026-03-28)*: Videos in general are "slightly janky/stuttery/laggy." Previous HLS fixes (B10.6d) resolved the worst cases but residual stutter remains. May need investigation of video element lifecycle, HLS segment buffering, or React re-render interference. *(Backend + Frontend — Track A + B)*
- [x] ~~**Feed Refresh Performance** *(UF.16)*~~: Multi-layer caching (FeedCache + SHARED_CLIENT + header_cache) *(Commit: `822223d`)*. Root cause of 29s load identified and fixed: (1) `OAuthManager.session` was `Mutex` held across entire network call — globally serialized ALL OAuth XRPC requests. Changed to `RwLock<Option<Arc<>>>`, clone Arc + drop lock before network. (2) `get_unified_feed_oauth` fetched N custom feeds in serial loop (10 × 2.5s = 25s) — replaced with `futures::future::join_all` for parallel fetches. *(Commit: `fd3ebd1`)*
- [ ] **Facebook Import Quality** *(UF.13)*: Backend filtering verified against workspace archive (`facebook-rabiezaater-2025-01-12-TwiR2QHB`). HTML format correctly detected, CSS selectors match perfectly, path matching covers all post files, noise filter handles lifecycle events/friend requests/empty posts. Parser quality is GOOD. Only operational issue: archive on disk is pre-extracted (not ZIP) — user must re-compress before import. Instagram connector (507 lines) still needs testing against workspace archive.
- [x] ~~**Content-Type Classification** *(UF.7)*~~: `classify_feed_entry` restored in `feed_aggregation.rs`. Items now classified as Status/Gallery/Article/Link/Video. *(Commit: `77702d9`)*
- [x] ~~**In-App Browser Tabs — Runtime Failure**~~: Resolved in Sprint v9 (backend) + Sprint v9.5 (frontend). Root cause: WebView lifecycle — child webviews created but not composited into the GtkOverlay correctly. Fix: `browser_show_at` command for precise positioning, `browser_show`/`browser_hide` for tab switching, GtkOverlay reparenting in Rust. Frontend: `WebViewScreen` refactored with show/hide/position synchronization, StrictMode double-mount guard. WebView tabs now render and load URLs correctly.
- [ ] **App.css Bloat**: ~~6,477 lines~~ Reduced by ~250 lines in Sprint v5: theme CSS extracted to `src/themes/{geocities,win98,midnight,hacker}.css` (`573ec8e`), FeedEntryCard decomposed into `src/components/feed/` sub-components with React.memo (`106c2dc`). Still large — further component CSS module extraction needed.
- [x] ~~**Export Button Not Wired**~~: Backend `export_vault_data` command wired to frontend via save dialog. *(Backend: `77702d9`, Frontend: `aab39db`)*
- [x] ~~**Video Playback Jitter**~~: 5 root causes identified and fixed: (1) `-fps_mode cfr` incompatible with `-c:v copy` — removed from stream-copy strategies, (2) `+discardcorrupt` dropped valid frames at HLS segment boundaries — replaced with `+igndts`, (3) `-bsf:a aac_adtstoasc` wrong for re-encoded audio — removed from audio re-encode path, (4) `validate_mp4_with_ffprobe` too permissive — now rejects if ANY stderr at `-v error` + validates video stream presence + duration > 0.5s, (5) missing `-analyzeduration 10M -probesize 10M` — added to all strategies. Also fixed video filter bug: `save_posts()` now stores `video_url` in `media_paths` + adds `video/mp4` MIME. Cache invalidation: old broken MP4s auto-reconverted on access. *(Commits: `b017091`, `6be698b`)*
- [x] ~~**Settings Screen Black Screen**~~: Exponential localStorage growth from duplicate `addUserBackground` calls across 3 `useTheme()` consumers → `QuotaExceededError` crashed React 19 root. Fixed: idempotent `addUserBackground`, one-time restore flag, try/catch on all localStorage writes. *(Commit: `6ba6c10`)*
- [x] ~~**Video Not Centered in Fullscreen Gallery**~~: `.gallery-content` had fixed `width: 90vw` forcing widescreen container for all videos. Vertical/portrait videos stretched into horizontal aspect ratio with large black bars. Fixed: removed forced width, added `max-width`/`max-height` constraints on `.gallery-content > div` and `.gallery-content video` with `width: auto; height: auto` for natural aspect ratio adaptation. *(Commits: `e14d99c`, `6abfb71`)*
- [ ] **Technical Debt — 432 Issues Identified** 📋 *(Systems Agent Audit 2026-04-02)*: Comprehensive audit of 162 source files (~53,780 lines) found 86 HIGH / 207 MEDIUM / 139 LOW issues. Full register: `.github/docs/technical-debt-audit.md`. Top systemic concerns: (1) full table scans in `<&str, &str>` DB schema, (2) security gaps in auth/keychain (SEC-1 through SEC-13 in Security Hardening), (3) duplicated logic (post parsing 4×, MIME detection 4×, identity building 5×), (4) missing pagination in connectors, (5) memory pressure from large media, (6) `import.rs` god file at 2,900 lines. Sprint integration: SEC-1 through SEC-7 + SEC-13 added to sprint v13 backend. DB perf + connector fixes in sprint v14. Frontend structural refactoring in sprint v15. *(Cross-track)*
- [ ] **Facebook Encoding Drops Non-Latin Unicode** 🔥 *(Audit 4.4)*: `fix_facebook_encoding` silently drops chars ≥ U+0100 — emoji, CJK, Cyrillic lost from imported posts. *(Backend — Track A)*
- [ ] **Facebook Timezone-Unaware Timestamps** *(Audit 4.5)*: Local dates treated as UTC — every imported timestamp off by user's UTC offset (up to ±12 hours). *(Backend — Track A)*
- [ ] **Bluesky Post Parsing Duplicated 4×** *(Audit 4.1)*: `parse_api_post`, `get_feed_with_media`, `get_liked_posts`, `get_bookmarks` each have their own extraction — bug fixes only apply to one path. *(Backend — Track A, refactor)*
- [ ] **Bluesky Follows N+1 API Bomb** *(Audit 4.2)*: `get_all_follows_with_posts` makes 2000+ sequential API calls with zero rate limiting — triggers Bluesky rate limits. *(Backend — Track A)*
- [ ] **Database Full Table Scans (15+ methods)** *(Audit Priority 2)*: `list_messages_for_convo` O(all_messages), `vault_list_files` O(n) per upload, `get_item` fallback full scan, `search_groups` full scan, `list_user_groups` full scan. Universal `<&str, &str>` schema forces JSON deserialization on every non-key query. *(Backend — Track A, performance)*
- [ ] **Stale Header Cache After Clear** *(Audit 2.1)*: `HeadersCache` not invalidated after `clear_all_data`/`remove_syndicated_duplicates` — `list_items_paged` returns phantom items. *(Backend — Track A)*
- [ ] **`clear_all_data` Misses 8 Tables** *(Audit 2.4)*: Groups, guestbook, AP actors, collab docs not cleared — "Clear all data" privacy feature leaves behind sensitive data. *(Backend — Track A)*
- [ ] **AP Media Attachments Dropped on Federate** 🔥 *(Audit 6.6)*: `bytescape_item_to_note()` drops all media — images/videos silently lost when federating to Mastodon/Lemmy. *(Backend — Track A)*
- [ ] **Mastodon Publish Drops Attachments** *(Audit 7.10)*: `publish_post_v2` Mastodon path publishes text only — attachment upload code exists for Bluesky but not Mastodon. *(Backend — Track A)*
- [ ] **`import.rs` God File (2,900 lines)** *(Audit 7.1)*: Mixes 8+ concerns (archive import, timeline fetch, DM import, feed aggregation, publishing, social graph). Should be split into 5-6 submodules. *(Backend — Track A, refactor sprint v14)*
- [ ] **Frontend Monolith Components** *(Audit Batches 8-14)*: App.tsx duplicated callbacks, `DataProvider` 100+ method flat interface, `store/index.ts` 6 stores in one file, `useUnifiedFeed.ts` 640-line orchestration hook, `ChatScreen`/`SettingsScreen`/`ComposeBox` god-components. *(Frontend — Track B, refactor sprint v15)*
- [ ] **No AP Delivery Retry/Queue** *(Audit 6.11)*: `deliver_activity()` single attempt, fail permanently — transient network errors cause permanent delivery failure. *(Backend — Track A, Sprint v14)*
- [ ] **No AP Clock Skew Check** *(Audit 6.5)*: Inbound signature verification doesn't validate Date header — replay attacks possible indefinitely. *(Backend — Track A, Sprint v14)*
- [ ] **Unbounded `HeadersCache` Memory** *(Audit 3.9)*: Cache grows without limit over long sessions — memory leak for users with diverse feeds. *(Backend — Track A)*
- [ ] **Chat Button on Friends List — Partially Fixed, Still Unreliable**: Backend command `get_or_create_bsky_convo` now exists (Track A: `bc5fcf6`). Frontend wired: App.tsx `onOpenChat` callbacks detect Bluesky DIDs and invoke the command; ChatScreen buddy panel has create-convo fallback; `pendingConvoMeta` in chat store enables new convos to show as live with input field (`4a02200`, `748d181`). CSS fix: `pointer-events: none` on `.friend-card::after` sparkle that was stealing ~80% of clicks (`c10a41a`). ⚠️ *Sprint v6 result: User reports "chat button works now, but only sometimes, and only after clicking like 5 times in a row. Most other times, it is still just completely unresponsive." Possible remaining causes: (1) async `invoke` to `get_or_create_bsky_convo` takes time with no loading indicator — user clicks again before it resolves, (2) additional click-stealing CSS elements not yet identified, (3) event propagation issue on the FriendsScreen button within the `.friend-card` onClick parent. **Needs interactive debugging** — add visible loading state to button while async call is in flight, add `console.log` breadcrumbs to trace which handler fires on each click.*
- [x] ~~**Window Not Maximized on Launch**~~: Fixed. `"maximized": true` added to `tauri.conf.json`. *(Commit: `5213f01`)*
- [x] ~~**Cargo Warnings**~~: 27 → 0 warnings. `#[allow(dead_code)]` annotations, `pub(crate)`, `_` prefixes for unused variables. *(Commit: `315cb8f`)*
- [x] ~~**Archive Import Deduplication**~~: Re-importing same archive no longer creates duplicates. Stable `original_id` computed from `hash(timestamp|content_200chars|media_uris)` via `DefaultHasher`. DB key becomes `source:stable_id` — identical across re-imports. *(Commit: `d1a662f`)*
- [x] ~~**Bluesky getFollowers 401**~~: `PUBLIC_XRPC` pointed to `bsky.social/xrpc` (now requires auth). Changed to `public.api.bsky.app/xrpc`. Affects `getFollowers`, `getFollows`, `resolveHandle`. *(Commit: `e9a36ca`)*
- [x] ~~**Blob Consistency Check**~~: Background task on startup (10s delay) scans all items with `media_blobs`, verifies each hash in Iroh. Auto-heals broken references. *(Commit: `d1d1855`)*
- [x] ~~**Source Filter Bug (Sprint v5)**~~: Fixed in Sprint v6. Backend: `source_filter` param added to `get_timeline_unified` with composite cache keys, vault query passthrough, skip-Bluesky optimization (Track A: `7bc515b`). Frontend: client-side filter applied to all 4 tabs not just "all" tab; cache invalidation clears all tab keys on filter change (Track B: `eddb52b`, `0906559`). User confirmed filter works correctly on all tabs.
- [ ] **Image / UI Blink on Scroll — STILL PRESENT** 🔥: ~~Previously marked resolved (Sprint v5→v11, commits `f3da093`, `96808fb`).~~ **User feedback (2026-03-28):** Images and overall UI still "blink" when scrolling — go black for a split second when hovered. Not a stable scrolling experience. One part of the UI seems affected by interactivity of another part. User says: "needs a fundamental ground up redesign of how the feed UI functions, so it's not so twitchy and is more stable overall." Prior fixes (synchronous cache init, removed hover effects) were insufficient — root cause is deeper, likely related to VirtualizedFeedList re-rendering, React reconciliation, or CSS reflow during scroll. *(Frontend — Track B, Critical)*
- [x] ~~**Mastodon Profile URL Parsing**~~: Clicking Mastodon author names caused DNS error "Error resolving 'username': Temporary failure in name resolution." Root cause: Mastodon connector formats `author_handle` as `"Display Name (@user@domain)"` but `openProfile()` naively split on `@`, putting the username in the domain slot. Fix: regex-based extraction `/@?([\w.-]+)@([\w.-]+\.\w+)/` handles all handle formats. *(Commit: `2fbcb7a`)*

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
