import { TaggedPerson, FeedEntry } from '../../types';
import { stripHtml } from '../../utils/sanitize';

/**
 * PURPOSE:
 * Houses pure, reusable formatting/parsing helpers used by feed components.
 *
 * HOW IT WORKS:
 * - Keeps UI components lean by centralizing deterministic display logic.
 * - Provides compatibility parsing for legacy and current data shapes.
 * - Contains feed-specific sanitization and URI resolution helpers.
 *
 * RELATED FILES:
 * - src/components/feed/EngagementBar.tsx
 * - src/components/feed/ThreadReplies.tsx
 * - src/components/FeedEntryCard.tsx
 * - src/types/index.ts (shape contracts for `FeedEntry`, `TaggedPerson`)
 */

// ─────────────────────────────────────────────────────────────────────────────
// Tagged people helpers
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Exported parser for tagged-people metadata.
 * Supports both typed arrays and legacy JSON-string payloads.
 */
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

/**
 * Exported human-readable formatter for tagged-people labels.
 */
export function renderTaggedPeople(tags: TaggedPerson[]): string | null {
    if (tags.length === 0) return null;
    const names = tags.map(t => t.linked_handle ? `@${t.linked_handle}` : '👤');
    if (names.length === 1) return `with ${names[0]}`;
    if (names.length === 2) return `with ${names[0]} and ${names[1]}`;
    return `with ${names[0]} and ${names.length - 1} others`;
}

// ─────────────────────────────────────────────────────────────────────────────
// Display formatting helpers
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Exported relative timestamp formatter for feed UIs.
 */
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

/**
 * Exported compact number formatter (K/M) for engagement counters.
 */
export function formatCount(n?: number): string {
    if (!n || n === 0) return '';
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1000) return `${(n / 1000).toFixed(1)}K`;
    return n.toString();
}

/**
 * Exported display sanitizer for user/imported post text.
 *
 * Cross-system note:
 * This is a presentation-level guard; ingestion/store layers should still
 * validate and normalize content at write time.
 */
export function sanitizeDisplayContent(raw: string): string {
    // Strip all HTML to prevent XSS from imported archive content (HIGH-3)
    const cleaned = stripHtml(raw);
    return cleaned.replace(/https?:\/\/[^\s]+/g, (match) => {
        if (match.length > 50) return match.substring(0, 47) + '...';
        return match;
    });
}

/** Resolve the AT-URI for a FeedEntry. Only returns valid AT-URIs (at:// prefix).
 *  Returns empty string if no valid AT-URI exists (e.g. Mastodon-only posts). */
/**
 * Exported AT-URI resolver for mixed-origin feed entries.
 * Only returns URIs starting with 'at://' — never returns HTTP URLs.
 */
export function resolveAtUri(entry: FeedEntry): string {
    if (entry.id.startsWith('at://')) return entry.id;
    if (entry.syndicated_uri?.startsWith('at://')) return entry.syndicated_uri;
    // Check share_history for a bluesky cross-post AT-URI
    const bskyEvent = entry.share_history?.find(e => e.platform === 'bluesky' && e.active);
    if (bskyEvent?.uri?.startsWith('at://')) return bskyEvent.uri;
    // No valid AT-URI found
    return '';
}
