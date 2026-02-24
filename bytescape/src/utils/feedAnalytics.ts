import { FeedEntry } from '../types';

export function normalizeMediaKey(url: string): string {
    if (!url) return '';
    return url.trim().toLowerCase().split('?')[0];
}

export function facebookContextScore(entry: FeedEntry): number {
    const text = `${entry.title || ''} ${entry.content || ''}`
        .replace(/timeline photos/gi, ' ')
        .replace(/\s{2,}/g, ' ')
        .trim();

    let score = Math.min(text.length, 240);
    if (/cover photos?|profile photos?|album|added\s+\d+\s+new\s+photos?/i.test(text)) {
        score += 60;
    }
    if ((entry.media_urls?.length || 0) > 1) {
        score += Math.min(entry.media_urls.length * 3, 24);
    }
    return score;
}

export function filterFacebookDuplicateImagePosts(entries: FeedEntry[]): FeedEntry[] {
    if (entries.length === 0) return entries;

    const byId = new Map<string, FeedEntry>();
    for (const e of entries) byId.set(e.id, e);

    const mediaToBest = new Map<string, string>();

    const facebookWithMedia = entries.filter(e =>
        (e.source || '').toLowerCase() === 'facebook' &&
        Array.isArray(e.media_urls) &&
        e.media_urls.length > 0
    );

    for (const entry of facebookWithMedia) {
        const keys = entry.media_urls.map(normalizeMediaKey).filter(Boolean);
        const entryScore = facebookContextScore(entry);
        for (const key of keys) {
            const existingId = mediaToBest.get(key);
            if (!existingId) {
                mediaToBest.set(key, entry.id);
                continue;
            }
            const existing = byId.get(existingId);
            if (!existing) {
                mediaToBest.set(key, entry.id);
                continue;
            }

            const existingScore = facebookContextScore(existing);
            const shouldReplace =
                entryScore > existingScore ||
                (entryScore === existingScore && (entry.media_urls.length || 0) > (existing.media_urls.length || 0)) ||
                (entryScore === existingScore && (entry.media_urls.length || 0) === (existing.media_urls.length || 0) && entry.timestamp > existing.timestamp);

            if (shouldReplace) {
                mediaToBest.set(key, entry.id);
            }
        }
    }

    return entries.filter(entry => {
        if ((entry.source || '').toLowerCase() !== 'facebook') return true;
        if (!entry.media_urls || entry.media_urls.length === 0) return true;

        const keys = entry.media_urls.map(normalizeMediaKey).filter(Boolean);
        if (keys.length === 0) return true;

        const winners = keys
            .map(k => mediaToBest.get(k))
            .filter((id): id is string => !!id)
            .map(id => byId.get(id))
            .filter((e): e is FeedEntry => !!e);

        if (winners.length === 0) return true;
        if (winners.some(w => w.id === entry.id)) return true;

        // Hide only older duplicate uploads once a richer winner exists.
        const hasRicherOrEqualNewerWinner = winners.every(w => {
            const winnerScore = facebookContextScore(w);
            const entryScore = facebookContextScore(entry);
            return winnerScore >= entryScore && w.timestamp >= entry.timestamp;
        });

        return !hasRicherOrEqualNewerWinner;
    });
}
