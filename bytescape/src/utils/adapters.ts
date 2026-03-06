import { BlueskyPost, BytescapeItem, FeedEntry } from '../types';
import { inferContentKind, sanitizeVaultContent } from './content';

// === Feed Deduplication ===
//
// Composite-key dedup that treats reposts as distinct feed events.
//
//   Original post X       → key "X"          (the post's AT URI)
//   Repost of X by user Y → key "repost:Y:X" (distinct from X)
//   Repost of X by user Z → key "repost:Z:X" (distinct from Y's repost)
//   Duplicate of X        → collapsed         (same key as first X)
//
// When two entries collide, the one that appears first wins.
// Callers should pre-sort if ordering matters.

export function deduplicateFeedEntries(entries: FeedEntry[]): FeedEntry[] {
    if (entries.length === 0) return entries;

    const seen = new Set<string>();
    return entries.filter(entry => {
        const uri = entry.original_url || entry.id;
        if (!uri) return true;

        const key = entry.reposted_by
            ? `repost:${entry.reposted_by}:${uri}`
            : uri;

        if (seen.has(key)) return false;
        seen.add(key);
        return true;
    });
}

// === BlueskyPost → FeedEntry ===
//
// Converts the Rust-serialized BlueskyImportedPost (received as BlueskyPost
// on the TypeScript side) into the universal FeedEntry interface.
//
// For reposts:
//   - author_* fields point to the ORIGINAL post's author
//   - reposted_by / reposted_by_handle identify the reposter
//   - timestamp is already the repost time (set by parse_feed_item in Rust)
//
// For quote posts:
//   - quoted_post is recursively converted

export function adaptBlueskyPost(post: BlueskyPost): FeedEntry {
    const mediaUrls = post.video_url ? [post.video_url] : (post.image_urls || []);
    const mediaKinds = post.video_url
        ? ['video' as const]
        : (post.image_urls || []).map(() => 'image' as const);

    return {
        id: post.uri || post.cid,
        cid: post.cid,
        timestamp: post.created_at,
        content: post.text,
        author_did: post.author_did,
        author_handle: post.author_handle,
        author_avatar: post.author_avatar,
        source: 'bluesky',

        media_urls: mediaUrls,
        media_thumbnails: post.video_thumbnail ? [post.video_thumbnail] : [],
        media_kinds: mediaKinds,

        link_url: post.external_url,
        link_title: post.external_title,
        link_description: post.external_description,
        link_thumbnail: post.external_thumb,

        like_count: post.like_count,
        reply_count: post.reply_count,
        repost_count: post.repost_count,

        reposted_by: post.reposted_by,
        reposted_by_handle: post.reposted_by_handle,
        reply_parent_handle: post.reply_parent_handle,
        quoted_post: post.quoted_post ? adaptBlueskyPost(post.quoted_post) : undefined,

        viewer_like: post.viewer_like,
        viewer_repost: post.viewer_repost,

        item_type: 'Status',
        audiences: ['Local', 'Bluesky'],
        share_history: post.uri ? [{
            platform: 'bluesky',
            uri: post.uri,
            timestamp: post.created_at || 0,
            active: true,
        }] : [],
        original_url: post.uri,
    };
}

// === Blob Reference Normalizer ===

function normalizeBlobRef(value: string): string {
    const v = (value || '').trim();
    if (!v) return v;
    if (v.startsWith('blob:') || v.startsWith('http://') || v.startsWith('https://')) return v;
    if (v.startsWith('/') || v.includes('/') || v.includes('\\')) return v;
    return `blob:${v}`;
}

// === BytescapeItem → FeedEntry ===

export function adaptBytescapeItem(item: BytescapeItem): FeedEntry {
    const { text, link } = sanitizeVaultContent(item.content || '');

    const media_urls = item.media_blobs && item.media_blobs.length > 0
        ? item.media_blobs.map(normalizeBlobRef)
        : item.media_paths || [];

    const media_thumbnails = item.thumbnail_blobs && item.thumbnail_blobs.length > 0
        ? item.thumbnail_blobs.map(normalizeBlobRef)
        : [];

    const media_hints = item.media_paths || [];
    const media_mimes = item.media_mimes || [];

    // Determine content kind: prefer MIME metadata (populated at import time),
    // then fall back to URL/path extension heuristics.
    const media_kinds = media_urls.map((src, i) => {
        // Primary: use stored MIME type if available
        const mime = media_mimes[i];
        if (mime) {
            if (mime.startsWith('video/')) return 'video' as const;
            if (mime.startsWith('audio/')) return 'audio' as const;
            if (mime === 'application/pdf') return 'pdf' as const;
            if (mime.startsWith('image/')) return 'image' as const;
        }

        // Fallback: heuristic from URL or path hint
        const fromUrl = inferContentKind(src);
        if (fromUrl !== 'unknown') return fromUrl;
        const hint = media_hints[i];
        if (hint) {
            const fromHint = inferContentKind(hint);
            if (fromHint !== 'unknown') return fromHint;
        }

        // Last resort: check if item type suggests video? 
        // Or if the blob hash is known to be a video in some other way?
        // For now, default to image if uncertain, or unknown.
        return 'unknown';
    });

    return {
        id: item.id,
        timestamp: item.original_timestamp,
        content: text,
        author_did: item.author_did,
        author_handle: item.author_handle,
        author_avatar: item.author_avatar,
        source: item.source,

        media_urls,
        media_thumbnails,
        media_kinds,
        media_mimes,

        link_url: item.link_url || link,
        link_title: item.title,
        link_description: item.description,
        link_thumbnail: item.preview_img,

        like_count: item.like_count,
        reply_count: item.reply_count,
        repost_count: item.repost_count,

        item_type: item.item_type,
        syndication_status: item.syndication_status,
        audiences: item.audiences || ['Local'],
        share_history: item.share_history || [],
        location: item.location,
        tagged_people: item.tagged_people,
        title: item.title,
        original_url: item.url,
        syndicated_uri: item.syndicated_uri,

        // Repost metadata — vault items with reposted_by_did are the user's own reposts
        viewer_repost: item.reposted_by_did ? 'repost' : undefined,
    };
}
