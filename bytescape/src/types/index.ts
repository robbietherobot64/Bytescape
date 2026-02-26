export interface AuthStatus {
    handle: string;
    did: string;
}

export interface IdentityInitResult {
    did: string;
    public_key: string;
    escrow_bundle: string;
}

export interface IdentityRecoveryStatus {
    has_local_identity: boolean;
    has_escrow: boolean;
    recovery_pending: boolean;
    recovery_verified_pending_finalize: boolean;
    did?: string | null;
    public_key?: string | null;
    last_recovered_at?: number | null;
}

export interface IdentityRecoveryStartResult {
    challenge_id: string;
    did_hint: string;
}

export interface IdentityRecoveryVerifyResult {
    verified: boolean;
    did: string;
    public_key: string;
}

export interface IdentityRecoveryFinalizeResult {
    restored: boolean;
    did: string;
    public_key: string;
}

export interface ParseProgress {
    stage: string;
    posts_found: number;
    photos_found: number;
    videos_found: number;
    trackers_found: number;
    current_file: string;
}

export interface TrackerEntry {
    name: string;
    event_count: number;
    event_types: string[];
}

export interface ParseResult {
    posts: ParsedPost[];
    trackers: TrackerEntry[];
    total_photos: number;
    total_videos: number;
    total_tracker_events: number;
    source: string;
}

export interface ParsedPost {
    timestamp: number;
    content: string;
    media_uris: string[];
    media_local_paths: string[];
    title: string | null;
    author_name?: string | null;
    action_type?: string | null;
    tagged_people?: string[];
    location?: PostLocation | null;
}

export type BytescapeType = 'Status' | 'Gallery' | 'Article';
export type SyndicationStatus = 'LocalOnly' | 'PendingSyndication' | 'Syndicated';
export type Audience = 'Local' | 'Bluesky' | 'P2p' | 'Activitypub';

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

export interface FeedEntry {
    id: string;
    timestamp: number;
    content: string;
    author_did?: string;
    author_handle?: string;
    author_avatar?: string;
    source: string;

    // Media
    media_urls: string[];
    media_thumbnails: string[];
    media_kinds?: Array<'video' | 'audio' | 'pdf' | 'image' | 'unknown'>;
    /** MIME types for each media entry, aligned with media_urls */
    media_mimes?: string[];

    // Link Preview
    link_url?: string | null;
    link_title?: string | null;
    link_description?: string | null;
    link_thumbnail?: string | null;

    // Stats
    like_count?: number;
    reply_count?: number;
    repost_count?: number;

    // Context
    reposted_by?: string;
    reposted_by_handle?: string;
    quoted_post?: FeedEntry;

    // Vault specific
    item_type?: BytescapeType;
    syndication_status?: SyndicationStatus;
    audiences: Audience[];
    syndicated_uris: Record<string, string>;
    location?: string;
    tagged_people?: string;
    title?: string | null;
    original_url?: string | null;
    syndicated_uri?: string | null;
}

export interface BytescapeItem {
    id: string;
    original_timestamp: number;
    content: string;
    media_paths: string[];
    source: string;
    path?: string;
    imported_at: number;
    title: string | null;
    original_id: string | null;
    author_did?: string;
    author_handle?: string;
    author_avatar?: string;
    /** JSON-serialized TaggedPerson[] — private vault data */
    tagged_people?: string;
    /** JSON-serialized PostLocation — place/check-in data */
    location?: string;
    is_liked?: boolean;
    like_count?: number;
    reply_count?: number;
    repost_count?: number;
    // New fields
    item_type?: BytescapeType;
    url?: string | null;
    content_html?: string | null;
    preview_img?: string | null;
    // Link Preview
    link_url?: string | null;
    description?: string | null;
    // Iroh Blob Storage
    media_blobs?: string[];
    thumbnail_blobs?: string[];
    /** MIME types for each media blob, aligned with media_blobs. e.g. ["video/mp4", "image/jpeg"] */
    media_mimes?: string[];
    // Syndication
    syndication_status?: SyndicationStatus;
    syndicated_uri?: string | null;
    audiences?: Audience[];
    syndicated_uris?: Record<string, string>;
}

export interface VaultStats {
    total: number;
    by_source: Record<string, number>;
    by_type: Record<string, number>;
    media_count: number;
}

// === Vault File Manager ===

export type VaultFileKind = 'Image' | 'Video' | 'Audio' | 'Document' | 'Archive' | 'Other';

export interface VaultFile {
    id: string;
    name: string;
    mime_type: string;
    kind: VaultFileKind;
    size: number;
    blob_hash: string;
    thumbnail_hash?: string | null;
    source: string;
    item_id?: string | null;
    added_at: number;
    created_at?: number | null;
    tags: string[];
}

export interface VaultFileStats {
    total_files: number;
    total_size: number;
    by_kind: Record<string, number>;
    by_source: Record<string, number>;
}

export interface ImportProgress {
    source: string;
    stage: string;
    posts_imported: number;
    photos_downloaded: number;
    current_item: string;
}

export interface VaultMessage {
    id: string;
    owner_did?: string | null;
    convo_id: string;
    sender_did: string;
    sender_handle: string;
    text: string;
    sent_at: string;
    source: string;
    imported_at: number;
    peer_did?: string;
    peer_handle?: string;
}

export interface PublishResult {
    local: boolean;
    bluesky: boolean;
    activitypub: boolean;
    bluesky_uri?: string | null;
    errors: string[];
}

export type Protocol = "atproto" | "activitypub" | "meta" | "x";

export interface ProtocolOption {
    id: Protocol;
    label: string;
    subtitle: string;
    emoji: string;
    hasApi: boolean;
    field1Placeholder: string;
    field2Placeholder: string;
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

export type Screen = "login" | "feed" | "import" | "audit" | "archive" | "profile" | "settings" | "chat" | "friends" | "vault";


export interface BlueskyPost {
    uri: string;
    cid: string;
    text: string;
    created_at: number;
    image_urls: string[];
    image_local_paths: string[];
    author_did: string;
    author_handle: string;
    author_avatar?: string;
    reply_count?: number;
    repost_count?: number;
    like_count?: number;
    video_url?: string;
    video_thumbnail?: string;
    external_url?: string;
    external_title?: string;
    external_description?: string;
    external_thumb?: string;
    // Repost info
    reposted_by?: string;
    reposted_by_handle?: string;
    // Quoted content
    quoted_post?: BlueskyPost;
}

export interface ProfileView {
    did: string;
    handle: string;
    displayName?: string;
    avatar?: string;
    description?: string;
}

export interface GeneratorView {
    uri: string;
    cid: string;
    did: string;
    displayName?: string;
    description?: string;
    avatar?: string;
}

// === P2P Sync ===

export interface DocTicketInfo {
    ticket: string;
    namespace_id: string;
}

export type SidebarItem = "feed" | "search" | "profile" | "chat" | "followers" | "import" | "vault" | "settings";
