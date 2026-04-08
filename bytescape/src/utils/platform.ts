/**
 * Platform abbreviation convention (UF.18).
 *
 * Rule: spell platform names without vowels.
 *   bluesky → blsky, mastodon → mstdn, facebook → fcbk, etc.
 *
 * Used in friend list filters, feed source filters, and any context
 * where platform names serve as filter/tab labels.
 */

const ABBREV_MAP: Record<string, string> = {
    bluesky: 'blsky',
    mastodon: 'mstdn',
    facebook: 'fcbk',
    instagram: 'insta',
    lemmy: 'lmmy',
    piefed: 'pfd',
    bytescape: 'bytscpe',
    activitypub: 'ctvtypb',
    local: 'lcl',
    p2p: 'p2p',
    rss: 'rss',
};

/**
 * Returns the vowel-less abbreviation for a platform name.
 * Falls back to the raw lowercase key if not in the map.
 */
export function platformAbbrev(source: string): string {
    const key = source.toLowerCase();
    return ABBREV_MAP[key] || key;
}

/**
 * Returns the emoji for a platform source key.
 */
export function platformEmoji(source: string): string {
    switch (source.toLowerCase()) {
        case 'bluesky': return '🦋';
        case 'facebook': return '📘';
        case 'instagram': return '📸';
        case 'mastodon': return '🐘';
        case 'activitypub': return '🐘';
        case 'bytescape': return '✨';
        case 'p2p': return '🔗';
        case 'rss': return '📡';
        case 'local': return '💾';
        case 'lemmy': return '🐀';
        case 'piefed': return '🥧';
        default: return '📝';
    }
}
