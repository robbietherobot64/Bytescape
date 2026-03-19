import { TaggedPerson, FeedEntry } from '../../types';
import { stripHtml } from '../../utils/sanitize';

export function parseTaggedPeople(input?: TaggedPerson[] | string): TaggedPerson[] {
    if (!input) return [];
    // New typed format: already an array
    if (Array.isArray(input)) return input;
    // Legacy string format: JSON-encoded
    try {
        return JSON.parse(input) as TaggedPerson[];
    } catch {
        return [];
    }
}

export function renderTaggedPeople(tags: TaggedPerson[]): string | null {
    if (tags.length === 0) return null;
    const names = tags.map(t => t.linked_handle ? `@${t.linked_handle}` : '👤');
    if (names.length === 1) return `with ${names[0]}`;
    if (names.length === 2) return `with ${names[0]} and ${names[1]}`;
    return `with ${names[0]} and ${names.length - 1} others`;
}

export function formatTimestamp(ts: number): string {
    const date = new Date(ts * 1000);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffSec = Math.floor(diffMs / 1000);
    const diffMin = Math.floor(diffSec / 60);
    const diffHrs = Math.floor(diffMin / 60);
    const diffDays = Math.floor(diffHrs / 24);

    if (diffSec < 60) return 'just now';
    if (diffMin < 60) return `${diffMin}m`;
    if (diffHrs < 24) return `${diffHrs}h`;
    if (diffDays === 1) return 'Yesterday';
    if (diffDays < 7) return date.toLocaleDateString(undefined, { weekday: 'short' });
    return date.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
}

export function formatCount(n?: number): string {
    if (!n || n === 0) return '';
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1000) return `${(n / 1000).toFixed(1)}K`;
    return n.toString();
}

export function sanitizeDisplayContent(raw: string): string {
    // Strip all HTML to prevent XSS from imported archive content (HIGH-3)
    const cleaned = stripHtml(raw);
    return cleaned.replace(/https?:\/\/[^\s]+/g, (match) => {
        if (match.length > 50) return match.substring(0, 47) + '...';
        return match;
    });
}

/** Resolve the AT-URI for a FeedEntry. Live posts use entry.id (already an AT-URI),
 *  but vault items store the CID as id and the AT-URI in original_url / syndicated_uri. */
export function resolveAtUri(entry: FeedEntry): string {
    // AT-URIs start with 'at://'
    if (entry.id.startsWith('at://')) return entry.id;
    if (entry.original_url) return entry.original_url;
    if (entry.syndicated_uri) return entry.syndicated_uri;
    // Fallback: check share_history for a bluesky URI
    const bskyEvent = entry.share_history?.find(e => e.platform === 'bluesky' && e.active);
    if (bskyEvent?.uri) return bskyEvent.uri;
    return entry.id;
}
