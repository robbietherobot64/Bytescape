/**
 * types/index.ts — Bytescape TypeScript Type Definitions
 *
 * This is the single source of truth for ALL TypeScript types used throughout
 * the frontend. Every interface here has a matching Rust struct in the backend
 * (see src-tauri/src/models/mod.rs). When the backend sends data to the frontend
 * via Tauri's IPC bridge (invoke()), the JSON is deserialized into these types.
 *
 * HOW IT WORKS:
 * 1. The Rust backend defines structs with #[derive(Serialize, Deserialize)]
 * 2. When the frontend calls invoke('some_command'), Tauri serializes the Rust
 *    struct to JSON and sends it across the IPC bridge
 * 3. The frontend receives the JSON and TypeScript treats it as these interfaces
 * 4. The two MUST stay in sync — if a Rust field changes, the TS interface must too
 *
 * ORGANIZATION:
 * - Authentication & Identity — login state, linked accounts, DID identity
 * - P2P Peer Types — information about peers in the network
 * - Import/Parse Types — types for importing social media archives
 * - Core Content Types — the universal data containers (BytescapeItem, FeedEntry)
 * - Vault Types — personal archive storage types
 * - Publishing & Syndication — cross-posting to multiple platforms
 * - Platform-Specific Types — Bluesky, Mastodon-specific data
 * - Profile Builder & Widgets — customizable profile page types
 * - Groups & Topics — community/forum types
 *
 * RELATED FILES:
 * - src-tauri/src/models/mod.rs — Rust counterparts of these types
 * - src-tauri/src/models/groups.rs — Rust group/topic models
 * - src/providers/DataProvider.ts — Uses these types in its interface
 * - src/utils/adapters.ts — Converts between BlueskyPost ↔ FeedEntry
 */

// ═══════════════════════════════════════════════════════════════════
// SECTION 1: AUTHENTICATION & IDENTITY
// Types related to user login, linked platform accounts, and
// decentralized identity (DID) management
// ═══════════════════════════════════════════════════════════════════

/**
 * A social media account the user has connected to Bytescape.
 * Users can link multiple accounts (Bluesky, Mastodon, etc.) and Bytescape
 * will pull data from all of them into the unified feed.
 */
export interface LinkedAccount {
    /** Which protocol this account uses: "bluesky", "mastodon", or "activitypub" */
    protocol: string;
    /** The canonical DID (Decentralized Identifier) — a universal ID like did:plc:abc123 */
    identifier: string;
    /** Platform-native ID format: acct:user@instance for Mastodon, did:plc:... for Bluesky */
    platform_identifier?: string;
    /** Human-readable handle: "@user.bsky.social" or "@user@mastodon.social" */
    display_handle: string;
    /** The user's display name on that platform (may differ from handle) */
    display_name?: string;
    /** URL to the user's profile picture on that platform */
    avatar_url?: string;
    /** When this account was linked to Bytescape (Unix timestamp in seconds) */
    connected_at: number;
    /** Whether this account is currently active and being used for data fetching */
    active: boolean;
}

/**
 * The current user's authentication state.
 * Returned by the backend after a successful login and used throughout the
 * frontend to know WHO is logged in, HOW they authenticated, and whether
 * their local Bytescape identity has been set up.
 */
export interface AuthStatus {
    /** The user's handle on the platform they logged in with (e.g. "user.bsky.social") */
    handle: string;
    /** The user's DID (Decentralized Identifier) from the platform they logged in with */
    did: string;
    /** "bluesky" | "local" | "mastodon" — which platform was used for initial login */
    source: string;
    /** The user's chosen Bytescape-local username (set during identity setup) */
    local_username?: string | null;
    /** Whether the local identity setup step has been completed */
    identity_ready: boolean;
    /** Master identity DID (did:web:bytescape.social:{username}), set after identity setup */
    web_did?: string | null;
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 2: P2P PEER TYPES
// Types for Bytescape's peer-to-peer network. Each Bytescape user runs
// an Iroh P2P node, identified by a node_id. Peers can connect directly
// to share content, chat, and sync data without any central server.
// ═══════════════════════════════════════════════════════════════════

/**
 * Information about the CURRENT user's P2P node.
 * This is "me" in the network — my node ID and identity info.
 */
export interface MyPeerInfo {
    /** Iroh node ID — a unique identifier for this P2P node (hex-encoded public key) */
    node_id: string;
    /** The user's Bytescape username, if they've set one up */
    username: string | null;
    /** The user's DID (Decentralized Identifier), if identity is configured */
    did: string | null;
}

/**
 * Information about ANOTHER user's P2P node.
 * This is "them" — a peer the user has added to their network.
 */
export interface PeerInfo {
    /** Iroh node ID of the remote peer */
    node_id: string;
    /** How this peer wants to be displayed (their chosen name) */
    display_name: string | null;
    /** The peer's DID, if known */
    did: string | null;
    /** When this peer was added to our network (Unix timestamp) */
    added_at: number;
    /** When we last saw this peer online (Unix timestamp, null if never seen) */
    last_seen: number | null;
    /** Whether we are following this peer's content */
    is_following: boolean;
}

/**
 * A peer's full profile information, received directly from them via P2P.
 * More detailed than PeerInfo — includes their username and linked identities.
 */
export interface PeerProfile {
    /** Iroh node ID */
    node_id: string;
    /** The peer's chosen Bytescape username */
    username: string;
    /** The peer's primary DID */
    did: string;
    /** Other DIDs this peer claims to control (e.g., Bluesky DID + Mastodon DID) */
    also_known_as?: string[];
}

/**
 * Response from fetching a peer's feed over P2P.
 * Contains their profile info plus the actual content items they've shared.
 */
export interface PeerFeedResponse {
    /** The peer's profile metadata */
    profile: PeerProfile;
    /** The posts/content items from this peer's vault */
    items: BytescapeItem[];
    /** Total number of items available (may be more than returned if paginated) */
    total_count: number;
}

/**
 * Result of attempting to connect to a peer over P2P.
 * Used to check if a peer is online before trying to fetch their data.
 */
export interface PeerConnectResult {
    /** Whether the peer is currently reachable */
    online: boolean;
    /** The peer's profile, if they responded to our connection attempt */
    profile: PeerProfile | null;
}

/**
 * A direct message conversation with a P2P peer.
 * Unlike Bluesky DMs which go through a server, P2P conversations
 * are end-to-end between two Bytescape nodes.
 */
export interface P2pConversation {
    /** Unique conversation ID (derived from both peer node IDs) */
    convo_id: string;
    /** Display name for this conversation (usually the peer's name) */
    display_name: string;
    /** Iroh node ID of the other participant */
    node_id: string;
    /** The other participant's DID, if known */
    did: string | null;
    /** Whether the other participant is currently online */
    online: boolean;
    /** Total number of messages in this conversation */
    message_count: number;
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 3: IDENTITY & RECOVERY
// Types for Bytescape's local identity system. Each user generates
// a cryptographic key pair (DID) that proves they are who they claim
// to be, independent of any platform. The recovery system lets users
// restore their identity if they lose access to their device.
// ═══════════════════════════════════════════════════════════════════

/**
 * Result of creating a new Bytescape identity.
 * Contains the user's new DID, their public key, and an encrypted
 * backup bundle they should save somewhere safe for recovery.
 */
export interface IdentityInitResult {
    /** The newly created DID (e.g., did:web:bytescape.social:username) */
    did: string;
    /** The public half of the user's cryptographic key pair (base64-encoded) */
    public_key: string;
    /** Encrypted backup of the private key — user should save this externally */
    escrow_bundle: string;
}

/**
 * Current state of the user's identity recovery setup.
 * Used by the UI to show whether recovery is available and guide
 * the user through the recovery flow if needed.
 */
export interface IdentityRecoveryStatus {
    /** Whether a local identity (key pair) exists on this device */
    has_local_identity: boolean;
    /** Whether an escrow bundle has been saved for recovery */
    has_escrow: boolean;
    /** Whether a recovery attempt is currently in progress */
    recovery_pending: boolean;
    /** Whether recovery has been verified but not yet finalized */
    recovery_verified_pending_finalize: boolean;
    /** The DID associated with the recovery, if known */
    did?: string | null;
    /** The public key associated with the recovery, if known */
    public_key?: string | null;
    /** When the identity was last recovered (Unix timestamp) */
    last_recovered_at?: number | null;
}

/** Step 1 of recovery: Backend generates a challenge the user must answer */
export interface IdentityRecoveryStartResult {
    /** Unique ID for this recovery challenge */
    challenge_id: string;
    /** Hint about which DID is being recovered */
    did_hint: string;
}

/** Step 2 of recovery: User's answer has been verified */
export interface IdentityRecoveryVerifyResult {
    /** Whether the verification was successful */
    verified: boolean;
    /** The recovered DID */
    did: string;
    /** The recovered public key */
    public_key: string;
}

/** Step 3 of recovery: Identity has been fully restored on this device */
export interface IdentityRecoveryFinalizeResult {
    /** Whether the restoration completed successfully */
    restored: boolean;
    /** The restored DID (now usable on this device) */
    did: string;
    /** The restored public key */
    public_key: string;
}

/** The portable encrypted blob the user saves for identity recovery. */
export interface RecoveryEscrowBundle {
    version: number;
    created_at: number;
    did: string;
    public_key: string;
    salt_b64: string;
    nonce_b64: string;
    ciphertext_b64: string;
    integrity_md5: string;
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 4: IMPORT & ARCHIVE PARSING
// Types for importing social media data exports (Facebook, Instagram).
// When a user uploads a ZIP archive of their data, the backend parses
// it and reports progress/results using these types.
// ═══════════════════════════════════════════════════════════════════

/**
 * Real-time progress updates emitted during archive parsing.
 * The backend sends these as events so the UI can show a progress bar
 * while a large archive (potentially thousands of posts) is being parsed.
 */
export interface ParseProgress {
    /** Current parsing stage description (e.g., "Parsing posts...", "Extracting photos...") */
    stage: string;
    /** Number of posts found so far */
    posts_found: number;
    /** Number of photos found so far */
    photos_found: number;
    /** Number of videos found so far */
    videos_found: number;
    /** Number of ad trackers found in the export */
    trackers_found: number;
    /** Number of chat messages found so far */
    messages_found: number;
    /** The file currently being parsed (shown to user for transparency) */
    current_file: string;
}

/**
 * An advertising tracker found in a social media data export.
 * Facebook/Instagram exports include data about how advertisers tracked the user.
 * Bytescape surfaces this in the Audit screen for transparency.
 */
export interface TrackerEntry {
    /** Name of the advertiser or tracking company */
    name: string;
    /** How many tracking events were logged */
    event_count: number;
    /** Types of tracking events (e.g., "website_visit", "app_activity") */
    event_types: string[];
}

/**
 * Complete result of parsing a social media archive.
 * Returned after the backend finishes processing the uploaded ZIP file.
 * Contains everything found: posts, trackers, media counts, and messages.
 */
export interface ParseResult {
    /** All posts/status updates found in the archive */
    posts: ParsedPost[];
    /** Advertising trackers found (shown on Audit screen) */
    trackers: TrackerEntry[];
    /** Total number of photos found across all posts */
    total_photos: number;
    /** Total number of videos found across all posts */
    total_videos: number;
    /** Total number of individual tracking events across all trackers */
    total_tracker_events: number;
    /** Chat/DM messages found in the archive */
    messages: VaultMessage[];
    /** Which platform this archive came from ("facebook", "instagram") */
    source: string;
}

/**
 * A single post extracted from a social media archive.
 * This is the raw parsed data before it gets converted into a BytescapeItem.
 * Contains both the post content and references to any attached media files.
 */
export interface ParsedPost {
    /** When the post was originally created (Unix timestamp) */
    timestamp: number;
    /** The text content of the post */
    content: string;
    /** URIs of attached media (images/videos) — may be AT Protocol URIs or file paths */
    media_uris: string[];
    /** Local filesystem paths to media files extracted from the archive */
    media_local_paths: string[];
    /** Post title, if any (e.g., for shared articles or notes) */
    title: string | null;
    /** Who wrote this post (may differ from the archive owner for shared posts) */
    author_name?: string | null;
    /** What kind of action this represents (e.g., "posted", "shared", "commented") */
    action_type?: string | null;
    /** People tagged/mentioned in this post */
    tagged_people?: string[];
    /** Location/check-in data attached to the post */
    location?: PostLocation | null;
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 5: CORE CONTENT TYPES
// The fundamental data types that represent ALL content in Bytescape.
// Every post, photo, article, and video — regardless of which platform
// it came from — gets normalized into these types.
// ═══════════════════════════════════════════════════════════════════

/**
 * The five content types in Bytescape. Every piece of content is classified
 * as one of these. The classification is automatic based on the content:
 * - Status: Short text post (like a tweet) — the default type
 * - Gallery: Image-focused post with 1+ photos and short caption
 * - Article: Long-form text (500+ characters)
 * - Link: A shared URL with a preview card
 * - Video: Post with video attachment(s)
 */
export type BytescapeType = 'Status' | 'Gallery' | 'Article' | 'Link' | 'Video';

/**
 * Tracks whether a post has been shared to external platforms.
 * - LocalOnly: Only exists in the user's local vault
 * - PendingSyndication: Queued to be published but not yet sent
 * - Syndicated: Successfully published to at least one external platform
 */
export type SyndicationStatus = 'LocalOnly' | 'PendingSyndication' | 'Syndicated';

/**
 * Which networks/audiences a post is visible to.
 * A post can be shared to multiple audiences simultaneously.
 * - Local: Only in the user's vault (private)
 * - Bluesky: Published to Bluesky via AT Protocol
 * - P2p: Shared with connected Bytescape peers
 * - Activitypub: Published to the fediverse (Mastodon, Lemmy, etc.)
 */
export type Audience = 'Local' | 'Bluesky' | 'P2p' | 'Activitypub';

/**
 * A record of a post being shared to a specific platform.
 * These form an event log ("share history") so Bytescape can track
 * where a post has been published and whether it's still live there.
 */
export interface SyndicationEvent {
    /** Which platform ("bluesky", "mastodon", "p2p") */
    platform: string;
    /** The post's identifier on that platform (e.g., AT Protocol URI for Bluesky) */
    uri: string;
    /** When the share happened (Unix timestamp) */
    timestamp: number;
    /** Whether the post is still live on that platform (false = deleted remotely) */
    active: boolean;
}

/** A person tagged in a post — stored privately in the vault */
export interface TaggedPerson {
    source_name: string;
    source_platform: string;
    linked_did?: string | null;
    linked_handle?: string | null;
}

/** A location or check-in attached to a post */
export interface PostLocation {
    name: string;
    latitude?: number | null;
    longitude?: number | null;
}

/**
 * A single item in the unified social feed.
 *
 * This is the MAIN type the UI renders. Every post shown in the feed — whether
 * it came from Bluesky, Mastodon, the local vault, or a P2P peer — is a FeedEntry.
 * It's created by the `useUnifiedFeed` hook, which merges data from all sources
 * and converts each into this common format via `adapters.ts`.
 *
 * KEY DIFFERENCE from BytescapeItem:
 * - FeedEntry is for DISPLAY — optimized for what the UI needs to render
 * - BytescapeItem is for STORAGE — the canonical format saved in the database
 * - A FeedEntry may represent a live post from Bluesky that was never saved to vault
 */
export interface FeedEntry {
    /** Unique identifier (CID for Bluesky posts, UUID for vault items) */
    id: string;
    /** Content ID from the AT Protocol (Bluesky-specific) */
    cid?: string;
    /** When the post was created (Unix timestamp) */
    timestamp: number;
    /** The text content of the post */
    content: string;
    /** DID of the post author */
    author_did?: string;
    /** Display handle of the post author (e.g., "user.bsky.social") */
    author_handle?: string;
    /** URL to the author's avatar image */
    author_avatar?: string;
    /** Which platform this post came from ("bluesky", "vault", "mastodon", etc.) */
    source: string;

    // ─── Media Attachments ───
    /** URLs to display for each attached image/video (may be blob: URLs or HTTP URLs) */
    media_urls: string[];
    /** Thumbnail URLs for videos (shown before video loads) */
    media_thumbnails: string[];
    /** What kind of media each attachment is: 'video', 'audio', 'pdf', 'image', or 'unknown' */
    media_kinds?: Array<'video' | 'audio' | 'pdf' | 'image' | 'unknown'>;
    /** MIME types for each media entry, aligned with media_urls */
    media_mimes?: string[];

    // ─── Link Preview (OpenGraph/unfurl data) ───
    /** URL of a shared link, if this post contains a link preview */
    link_url?: string | null;
    /** Title from the linked page's metadata */
    link_title?: string | null;
    /** Description snippet from the linked page */
    link_description?: string | null;
    /** Thumbnail image from the linked page */
    link_thumbnail?: string | null;

    // ─── Engagement Statistics ───
    /** Number of likes on the original platform */
    like_count?: number;
    /** Number of replies */
    reply_count?: number;
    /** Number of reposts/boosts */
    repost_count?: number;

    // ─── Repost & Reply Context ───
    /** If this is a repost, who reposted it (DID) */
    reposted_by?: string;
    /** If this is a repost, who reposted it (display handle) */
    reposted_by_handle?: string;
    /** If this is a reply, the handle of the post being replied to */
    reply_parent_handle?: string;
    /** If this is a reply, the URI of the parent post */
    reply_parent_uri?: string;
    /** If this post quotes another post, the quoted post's data (recursive) */
    quoted_post?: FeedEntry;
    /** Replies by the same author nested under this post (thread display) */
    owner_replies?: FeedEntry[];

    // ─── Viewer State (current user's interaction with this post) ───
    /** If the current user liked this post, the URI of their like record (used for un-liking) */
    viewer_like?: string;
    /** If the current user reposted this post, the URI of their repost record */
    viewer_repost?: string;

    // ─── Vault-Specific Fields (only set for posts stored locally) ───
    /** Content classification: Status, Gallery, Article, Link, or Video */
    item_type?: BytescapeType;
    /** Whether this post has been shared externally */
    syndication_status?: SyndicationStatus;
    /** Which networks this post has been shared to */
    audiences: Audience[];
    /** Full history of when/where this post was shared */
    share_history: SyndicationEvent[];
    /** Location/check-in data, if attached */
    location?: PostLocation | null;
    /** People tagged in the post */
    tagged_people?: TaggedPerson[];
    /** Post title (for articles or titled posts) */
    title?: string | null;
    /** Original URL on the source platform (used for deduplication) */
    original_url?: string | null;
    /** URI on the platform where this post was syndicated to */
    syndicated_uri?: string | null;
}

/**
 * The universal content container — the single most important type in Bytescape.
 *
 * Every piece of content, regardless of its source platform, gets normalized
 * into a BytescapeItem before being stored in the local vault (redb database).
 * This is what makes Bytescape "universal" — a Bluesky post, a Facebook memory,
 * an Instagram photo, and a locally-composed article all share this shape.
 *
 * KEY DIFFERENCE from FeedEntry:
 * - BytescapeItem is for STORAGE and backend operations
 * - FeedEntry is for DISPLAY in the UI
 * - BytescapeItem → FeedEntry conversion happens in adapters.ts
 *
 * Rust counterpart: src-tauri/src/models/mod.rs::BytescapeItem
 */
export interface BytescapeItem {
    /** Unique identifier — usually a UUID for local items, or AT-URI for Bluesky items */
    id: string;
    /** When the content was originally created on the source platform (Unix timestamp) */
    original_timestamp: number;
    /** The text content of the post/article */
    content: string;
    /** Local filesystem paths to media files (legacy — being replaced by media_blobs) */
    media_paths: string[];
    /** Which platform this came from: "bluesky", "facebook", "instagram", "mastodon", "bytescape" */
    source: string;
    /** Hierarchical storage path in the vault (e.g., "/facebook/status/2024/03/abc123") */
    path?: string;
    /** When this item was imported/saved into Bytescape (Unix timestamp) */
    imported_at: number;
    /** Post title (for articles and titled posts) */
    title: string | null;
    /** Original ID from the source platform */
    original_id: string | null;
    /** DID of the content author */
    author_did?: string;
    /** Display handle of the content author */
    author_handle?: string;
    /** Avatar URL of the content author */
    author_avatar?: string;
    /** People tagged in this post — stored as structured data */
    tagged_people?: TaggedPerson[];
    /** Location/check-in data — stored as structured data */
    location?: PostLocation | null;
    /** Whether the current user has liked this post */
    is_liked?: boolean;
    /** Engagement counts from the source platform */
    like_count?: number;
    reply_count?: number;
    repost_count?: number;
    // ─── Content Classification ───
    /** Auto-classified content type: Status, Gallery, Article, Link, or Video */
    item_type?: BytescapeType;
    /** Original URL on the source platform */
    url?: string | null;
    /** HTML-formatted version of the content (for rich rendering) */
    content_html?: string | null;
    /** Preview/thumbnail image URL */
    preview_img?: string | null;

    // ─── Link Preview ───
    /** URL of a shared link */
    link_url?: string | null;
    /** Description from the linked page's metadata */
    description?: string | null;

    // ─── Iroh Blob Storage (content-addressed media) ───
    /** Iroh blob hashes for attached media files (replaces media_paths) */
    media_blobs?: string[];
    /** Iroh blob hashes for thumbnail images */
    thumbnail_blobs?: string[];
    /** MIME types for each media blob, aligned with media_blobs (e.g., ["video/mp4", "image/jpeg"]) */
    media_mimes?: string[];

    // ─── Syndication (cross-platform publishing) ───
    /** Whether this post has been shared externally */
    syndication_status?: SyndicationStatus;
    /** URI on the first platform this was syndicated to (legacy — use share_history) */
    syndicated_uri?: string | null;
    /** Which audiences this post has been shared with */
    audiences?: Audience[];
    /** Full log of when/where this post was published */
    share_history?: SyndicationEvent[];

    // ─── Repost & Reply Metadata ───
    /** If this is a repost, the DID of the person who reposted it */
    reposted_by_did?: string;
    /** If this is a repost, the display handle of the reposter */
    reposted_by_handle?: string;
    /** If this is a reply, the URI of the post being replied to */
    reply_parent_uri?: string;
    /** If this is a reply, the handle of the person being replied to */
    reply_parent_handle?: string;

    // ─── Collaborative Editing ───
    /** Links to a Yjs CRDT document for collaborative article editing */
    collab_doc_id?: string | null;
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 6: VAULT & FILE STORAGE
// Types for the user's personal vault — the local database that stores
// all imported and composed content. Also includes the file manager
// which tracks individual media files (images, videos, documents).
// ═══════════════════════════════════════════════════════════════════

/** Summary statistics for the user's vault — shown on dashboard and settings screen */
export interface VaultStats {
    /** Total number of items (posts, articles, etc.) in the vault */
    total: number;
    /** Breakdown by source platform (e.g., { "bluesky": 150, "facebook": 300 }) */
    by_source: Record<string, number>;
    /** Breakdown by content type (e.g., { "Status": 200, "Gallery": 100 }) */
    by_type: Record<string, number>;
    /** Total number of media files (images + videos) */
    media_count: number;
}

// === Vault File Manager ===

/** The kind of file stored in the vault — determined by MIME type */
export type VaultFileKind = 'Image' | 'Video' | 'Audio' | 'Document' | 'Archive' | 'Other';

/**
 * Metadata for an individual file stored in the vault.
 * Each media file (photo, video, document) gets its own VaultFile record
 * with an Iroh blob hash for content-addressed storage.
 */
export interface VaultFile {
    /** Unique file identifier */
    id: string;
    /** Original filename */
    name: string;
    /** MIME type (e.g., "image/jpeg", "video/mp4") */
    mime_type: string;
    /** Classified file type for UI grouping */
    kind: VaultFileKind;
    /** File size in bytes */
    size: number;
    /** Iroh blob hash — content-addressed identifier for retrieving the file */
    blob_hash: string;
    /** Iroh blob hash for a generated thumbnail (images/videos only) */
    thumbnail_hash?: string | null;
    /** Which platform/import this file came from */
    source: string;
    /** ID of the BytescapeItem this file is attached to, if any */
    item_id?: string | null;
    /** When this file was added to the vault (Unix timestamp) */
    added_at: number;
    /** Original creation date of the file, if known from metadata */
    created_at?: number | null;
    /** User-applied tags for organization */
    tags: string[];
}

/** Summary statistics for vault files — used in the Vault screen header */
export interface VaultFileStats {
    /** Total number of files in the vault */
    total_files: number;
    /** Total size of all files in bytes */
    total_size: number;
    /** Count by file kind (e.g., { "Image": 500, "Video": 50 }) */
    by_kind: Record<string, number>;
    /** Count by source platform */
    by_source: Record<string, number>;
}

/**
 * Real-time progress updates during the import (commit) phase.
 * Distinct from ParseProgress — this tracks the phase AFTER parsing,
 * when items are being saved to the vault and media is being downloaded.
 */
export interface ImportProgress {
    /** Which platform's data is being imported */
    source: string;
    /** Current import stage description */
    stage: string;
    /** Number of posts saved to vault so far */
    posts_imported: number;
    /** Number of photos downloaded and stored so far */
    photos_downloaded: number;
    /** Description of the item currently being processed */
    current_item: string;
}

/**
 * A chat message stored in the vault.
 * These can come from imported archives (Facebook Messenger, Instagram DMs)
 * or from live P2P conversations between Bytescape peers.
 */
export interface VaultMessage {
    /** Unique message identifier */
    id: string;
    /** DID of the vault owner (the current user) */
    owner_did?: string | null;
    /** ID of the conversation this message belongs to */
    convo_id: string;
    /** DID of the person who sent this message */
    sender_did: string;
    /** Display handle of the sender */
    sender_handle: string;
    /** Message text content */
    text: string;
    /** When the message was sent (ISO 8601 string) */
    sent_at: string;
    /** Which platform this message came from ("facebook", "instagram", "p2p") */
    source: string;
    /** When this message was imported into Bytescape (Unix timestamp) */
    imported_at: number;
    /** DID of the other person in the conversation */
    peer_did?: string;
    /** Display handle of the other person */
    peer_handle?: string;
    /** Which protocol this message was sent over ("iroh", "bluesky") */
    protocol?: string;
    /** DID of the intended recipient */
    recipient_did?: string;
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 7: PUBLISHING & SYNDICATION
// Types for creating new posts and cross-posting to multiple platforms.
// ═══════════════════════════════════════════════════════════════════

/**
 * Result of publishing a post, potentially to multiple platforms at once.
 * Each boolean indicates success/failure per platform.
 */
export interface PublishResult {
    /** Whether the post was saved to the local vault */
    local: boolean;
    /** Whether the post was successfully published to Bluesky */
    bluesky: boolean;
    /** Whether the post was successfully published to Mastodon */
    mastodon: boolean;
    /** Whether the post was successfully published via ActivityPub */
    activitypub: boolean;
    /** URI of the Bluesky post, if published there */
    bluesky_uri?: string | null;
    /** Error messages from any platforms that failed */
    errors: string[];
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 8: PROTOCOL CONFIGURATION
// Defines the supported social media protocols and their UI configuration
// for the login/connect screen. Each protocol has different capabilities:
// some support live API access, others only archive import.
// ═══════════════════════════════════════════════════════════════════

/** Supported protocol identifiers */
export type Protocol = "atproto" | "activitypub" | "meta" | "x";

/**
 * Configuration for a protocol option on the login screen.
 * Defines how each platform is presented and what fields the user needs to fill in.
 */
export interface ProtocolOption {
    /** Protocol identifier */
    id: Protocol;
    /** Display name (e.g., "AT Protocol") */
    label: string;
    /** Platform name shown below the label (e.g., "Bluesky") */
    subtitle: string;
    /** Emoji icon for the protocol */
    emoji: string;
    /** Whether this protocol supports live API access (vs. archive-only import) */
    hasApi: boolean;
    /** Placeholder text for the first input field (usually handle/username) */
    field1Placeholder: string;
    /** Placeholder text for the second input field (usually password/token) */
    field2Placeholder: string;
    /** Whether to mask the second field as a password input */
    showPassword: boolean;
}

export const PROTOCOLS: ProtocolOption[] = [
    {
        id: "atproto",
        label: "AT Protocol",
        subtitle: "Bluesky",
        emoji: "🦋",
        hasApi: true,
        field1Placeholder: "Handle (e.g. user.bsky.social)",
        field2Placeholder: "App Password",
        showPassword: true,
    },
    {
        id: "activitypub",
        label: "ActivityPub",
        subtitle: "Mastodon, Lemmy",
        emoji: "🐘",
        hasApi: true,
        field1Placeholder: "Handle (e.g. user@mastodon.social)",
        field2Placeholder: "Coming soon — OAuth",
        showPassword: false,
    },
    {
        id: "meta",
        label: "Meta",
        subtitle: "Facebook, Instagram",
        emoji: "📘",
        hasApi: false,
        field1Placeholder: "Username or email",
        field2Placeholder: "",
        showPassword: false,
    },
    {
        id: "x",
        label: "X",
        subtitle: "Twitter",
        emoji: "𝕏",
        hasApi: false,
        field1Placeholder: "Handle (e.g. @user)",
        field2Placeholder: "",
        showPassword: false,
    },
];

// ═══════════════════════════════════════════════════════════════════
// SECTION 9: BLUESKY / AT PROTOCOL TYPES
// Types specific to the Bluesky social network and its AT Protocol.
// These represent the raw data format from Bluesky's API before it's
// converted into Bytescape's universal FeedEntry/BytescapeItem format.
// ═══════════════════════════════════════════════════════════════════

/** @deprecated - use Screen from store/index.ts */
export type Screen = "login" | "feed" | "import" | "audit" | "archive" | "profile" | "profile-builder" | "profile-browser" | "settings" | "chat" | "friends" | "vault" | "peer-profile";

/**
 * A post from Bluesky's AT Protocol API.
 * This is the RAW format from Bluesky before conversion to FeedEntry.
 * Contains Bluesky-specific fields like AT-URIs and CIDs.
 *
 * Converted to FeedEntry by adaptBlueskyPost() in utils/adapters.ts.
 */
export interface BlueskyPost {
    /** AT Protocol URI (e.g., at://did:plc:abc.../app.bsky.feed.post/xyz) */
    uri: string;
    /** Content Identifier — a hash that uniquely identifies this version of the post */
    cid: string;
    /** Post text content */
    text: string;
    /** When the post was created (Unix timestamp) */
    created_at: number;
    /** URLs to images attached to this post */
    image_urls: string[];
    /** Local file paths for cached images */
    image_local_paths: string[];
    /** DID of the post author */
    author_did: string;
    /** Handle of the post author (e.g., "user.bsky.social") */
    author_handle: string;
    /** URL to the author's avatar */
    author_avatar?: string;
    /** Engagement counts */
    reply_count?: number;
    repost_count?: number;
    like_count?: number;
    /** Video attachment URL (Bluesky supports one video per post) */
    video_url?: string;
    /** Thumbnail image for the video */
    video_thumbnail?: string;
    /** External link embed: URL */
    external_url?: string;
    /** External link embed: page title */
    external_title?: string;
    /** External link embed: page description */
    external_description?: string;
    /** External link embed: preview thumbnail */
    external_thumb?: string;
    /** If this is a repost, the DID of the reposter */
    reposted_by?: string;
    /** If this is a repost, the handle of the reposter */
    reposted_by_handle?: string;
    /** If this is a reply, the handle of the parent post's author */
    reply_parent_handle?: string;
    /** The current user's like record URI (null if not liked) */
    viewer_like?: string;
    /** The current user's repost record URI (null if not reposted) */
    viewer_repost?: string;
    /** If this post quotes another post, the quoted post's data */
    quoted_post?: BlueskyPost;
}

/** A Bluesky user's profile information (compact view for display in feeds) */
export interface ProfileView {
    /** The user's DID */
    did: string;
    /** The user's handle (e.g., "user.bsky.social") */
    handle: string;
    /** Display name (can be different from handle) */
    displayName?: string;
    /** URL to profile avatar */
    avatar?: string;
    /** Bio/description text */
    description?: string;
}

/**
 * A Bluesky feed generator (custom algorithm feed).
 * Bluesky lets users create custom feeds with their own algorithms.
 * Each generator produces a curated timeline the user can subscribe to.
 */
export interface GeneratorView {
    /** AT Protocol URI of the feed generator */
    uri: string;
    /** Content ID of the generator record */
    cid: string;
    /** DID of the feed generator's creator */
    did: string;
    /** Display name of the feed */
    displayName?: string;
    /** Description of what this feed shows */
    description?: string;
    /** Avatar/icon for the feed */
    avatar?: string;
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 10: P2P SYNC & NAVIGATION
// Types for Iroh document sync and UI navigation state.
// ═══════════════════════════════════════════════════════════════════

/**
 * Information needed to join an Iroh document sync session.
 * A "doc ticket" is like an invite link — it contains the cryptographic
 * credentials needed to connect to a shared document.
 */
export interface DocTicketInfo {
    /** The full Iroh doc ticket string (contains connection info + auth) */
    ticket: string;
    /** The namespace ID of the Iroh document (identifies which document to sync) */
    namespace_id: string;
}

/** Which section of the sidebar is currently selected */
export type SidebarItem = "feed" | "search" | "profile" | "chat" | "social" | "forums" | "import" | "vault" | "settings";

// ═══════════════════════════════════════════════════════════════════
// SECTION 11: THEME & PROFILE BUILDER
// Types for the customizable profile page system. Users can build their
// own profile pages using a drag-and-drop editor (GrapesJS) with widgets
// like music players, guestbooks, and away messages — inspired by MySpace.
// ═══════════════════════════════════════════════════════════════════

/** Available visual themes for the application UI */
export type ThemeId = 'geocities' | 'win98' | 'midnight' | 'hacker';

/**
 * The saved state of a user's profile page.
 * Contains the HTML/CSS output from the profile builder editor (GrapesJS),
 * plus the editor's internal state so it can be reloaded for further editing.
 * Can also be a raw HTML page for advanced users (render_mode = "raw").
 */
export interface ProfileLayout {
    /** The rendered HTML of the profile page */
    html: string;
    /** The CSS styles for the profile page */
    css: string;
    /** GrapesJS internal JSON for re-loading the drag-and-drop editor state */
    components_json?: string | null;
    /** GrapesJS internal JSON for re-loading style rules */
    styles_json?: string | null;
    /** Widget configuration data (away message, music playlist, etc.) stored as JSON */
    widget_data?: string | null;
    /** How to render this profile: null/"grapesjs" = widget hydration, "raw" = sandboxed iframe */
    render_mode?: string | null;
    /** Custom JavaScript for raw HTML mode (only runs in sandboxed iframe for security) */
    js?: string | null;
    /** When the profile was last modified (Unix timestamp) */
    updated_at: number;
}

/** Aggregated widget configuration saved alongside the profile layout */
export interface WidgetData {
    away_message?: AwayMessageConfig;
    music_playlist?: MusicTrack[];
    visitor_count?: number;
    /** Additional widget-specific config can go here */
    [key: string]: unknown;
}

/** Away-message widget configuration */
export interface AwayMessageConfig {
    status: 'online' | 'away' | 'busy' | 'offline';
    text: string;
    enabled: boolean;
}

/** A single track in the music-player widget */
export interface MusicTrack {
    id: string;
    title: string;
    artist?: string;
    /** Direct URL to the audio file */
    url?: string;
    /** Blob hash for vault-stored audio */
    blob_hash?: string;
    /** Fallback src field */
    src?: string;
}

/** A guestbook entry */
export interface GuestbookEntry {
    id: string;
    author: string;
    message: string;
    timestamp: number;
    node_id?: string | null;
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 12: INTERACTION RESULTS
// Return types from like, repost, and reply operations.
// These contain the URI and CID needed to undo the action later.
// ═══════════════════════════════════════════════════════════════════

/** Result of liking a post — the URI is needed to un-like later */
export interface LikeResult {
    /** AT Protocol URI of the like record */
    uri: string;
    /** Content ID of the like record */
    cid: string;
}

/** Result of reposting — the URI is needed to un-repost later */
export interface RepostResult {
    /** AT Protocol URI of the repost record */
    uri: string;
    /** Content ID of the repost record */
    cid: string;
}

/** Result of replying to a post */
export interface ReplyResult {
    /** AT Protocol URI of the reply post */
    uri: string;
    /** Content ID of the reply post */
    cid: string;
}

/**
 * A reply in a post's thread view.
 * When the user expands a post to see its replies, each reply is a ThreadReply.
 * Similar to BlueskyPost but simplified for the thread context.
 */
export interface ThreadReply {
    /** AT Protocol URI of this reply */
    uri: string;
    /** Content ID */
    cid: string;
    /** Reply text content */
    text: string;
    /** DID of the reply author */
    author_did: string;
    /** Handle of the reply author */
    author_handle: string;
    /** Avatar URL of the reply author */
    author_avatar?: string;
    /** When the reply was posted (Unix timestamp) */
    created_at: number;
    /** Engagement counts */
    like_count?: number;
    reply_count?: number;
    repost_count?: number;
    /** Current user's like/repost state (for undo) */
    viewer_like?: string;
    viewer_repost?: string;
    /** Image attachments on the reply */
    image_urls: string[];
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 13: RICH TEXT & MENTIONS
// Types for AT Protocol rich text (facets) and @-mention autocomplete.
// ═══════════════════════════════════════════════════════════════════

/** A user suggestion returned when typing @-mentions in the compose box */
export interface MentionSuggestion {
    handle: string;
    did: string;
    avatar_url?: string | null;
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 14: COLLABORATIVE EDITING
// Types for the Yjs CRDT-based collaborative editing system.
// Multiple users can edit the same article simultaneously via P2P sync.
// ═══════════════════════════════════════════════════════════════════

/** Metadata for a collaborative document */
export interface CollabDocMeta {
    id: string;
    created_at: number;
    updated_at: number;
    item_id?: string | null;
    title?: string | null;
}

/**
 * Full state of a collaborative document, returned by the backend.
 * Contains the Yjs CRDT state encoded as base64 for transfer over IPC.
 */
export interface CollabStateResponse {
    /** The full Yjs document state, base64-encoded */
    state_b64: string;
    /** The Yjs state vector (for incremental sync), base64-encoded */
    state_vector_b64: string;
    /** Document metadata (title, timestamps, linked item) */
    meta: CollabDocMeta;
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 15: LINK PREVIEW
// Type for URL unfurling — when a user shares a link, Bytescape fetches
// the page's metadata (title, description, image) to show a preview card.
// ═══════════════════════════════════════════════════════════════════

/** Link preview metadata fetched by the unfurl_link backend command */
export interface LinkCard {
    url: string;
    title?: string | null;
    description?: string | null;
    image_url?: string | null;
    site_name?: string | null;
    favicon_url?: string | null;
}

// ═══════════════════════════════════════════════════════════════════
// SECTION 16: GROUPS & TOPICS (Community/Forum System)
// Types for Bytescape's hierarchical forum system. Topics form a tree
// (e.g., Health → Mental Health → Anxiety). Groups are user-created
// communities filed under topics with three visibility modes.
// Rust counterparts: src-tauri/src/models/groups.rs
// ═══════════════════════════════════════════════════════════════════

/** Group visibility: Public (auto-join), Closed (needs approval), Private (invite-only) */
export type GroupVisibility = 'Public' | 'Closed' | 'Private';
/** Role within a group: Owner > Moderator > Member */
export type MemberRole = 'Owner' | 'Moderator' | 'Member';
/** Membership state: Active, Pending approval, Invited but not accepted, or Banned */
export type MemberStatus = 'Active' | 'Pending' | 'Invited' | 'Banned';

/**
 * A topic in the hierarchical topic tree.
 * Topics organize groups into categories. They can be nested
 * (parent → child) to form a browsable hierarchy.
 */
export interface Topic {
    id: string;
    name: string;
    slug: string;
    parent_id: string | null;
    description: string;
    icon: string | null;
    sort_order: number;
    created_by: string;
    created_at: number;
}

/**
 * A topic with its children and associated groups — used for rendering
 * the topic tree in the UI. Extends Topic with recursive children.
 */
export interface TopicNode extends Topic {
    children: TopicNode[];
    groups: GroupSummary[];
    subscribed: boolean;
}

/**
 * Full details of a group/community.
 * Contains everything needed to display the group page and manage membership.
 */
export interface LocalGroup {
    group_id: string;
    name: string;
    slug: string;
    description: string;
    visibility: GroupVisibility;
    topic_id: string;
    owner_did: string;
    icon_blob: string | null;
    banner_blob: string | null;
    rules: string | null;
    posting_restricted_to_mods: boolean;
    member_count: number;
    created_at: number;
    updated_at: number;
}

/** Compact group info for listing in the topic tree sidebar */
export interface GroupSummary {
    group_id: string;
    name: string;
    slug: string;
    visibility: GroupVisibility;
    member_count: number;
    icon_blob: string | null;
}

/** A user's membership in a group, including their role and status */
export interface GroupMember {
    group_id: string;
    member_did: string;
    role: MemberRole;
    status: MemberStatus;
    joined_at: number;
}

/**
 * Links a BytescapeItem to a group. The actual post content lives in the
 * vault (ITEMS_TABLE) — this is just a tag/reference, not a copy.
 */
export interface GroupPost {
    group_id: string;
    item_id: string;
    pinned: boolean;
    shared_by: string | null;
    posted_at: number;
}

/** Records that a user has subscribed to a topic (to see its groups in their feed) */
export interface TopicSubscription {
    topic_id: string;
    user_did: string;
    subscribed_at: number;
}
