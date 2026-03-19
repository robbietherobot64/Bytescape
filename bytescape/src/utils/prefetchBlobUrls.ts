import { invoke } from '@tauri-apps/api/core';
import { resolvedUrlCache } from '../hooks/useBlobUrl';
import { FeedEntry } from '../types';

/**
 * Extract all blob references from a list of feed entries.
 * Returns de-duplicated sources that start with "blob:" but are NOT
 * browser Object URLs (blob:http://, blob:tauri://).
 */
function collectBlobRefs(entries: FeedEntry[]): string[] {
    const refs = new Set<string>();

    for (const entry of entries) {
        for (const url of entry.media_urls) {
            if (isBlobRef(url) && !resolvedUrlCache.has(normalizeBlobHash(url))) {
                refs.add(url);
            }
        }
        for (const url of entry.media_thumbnails) {
            if (isBlobRef(url) && !resolvedUrlCache.has(normalizeBlobHash(url))) {
                refs.add(url);
            }
        }
        if (entry.link_thumbnail && isBlobRef(entry.link_thumbnail) && !resolvedUrlCache.has(normalizeBlobHash(entry.link_thumbnail))) {
            refs.add(entry.link_thumbnail);
        }
        if (entry.author_avatar && isBlobRef(entry.author_avatar) && !resolvedUrlCache.has(normalizeBlobHash(entry.author_avatar))) {
            refs.add(entry.author_avatar);
        }
        // Check quoted post media too
        if (entry.quoted_post) {
            for (const url of entry.quoted_post.media_urls) {
                if (isBlobRef(url) && !resolvedUrlCache.has(normalizeBlobHash(url))) {
                    refs.add(url);
                }
            }
        }
    }

    return Array.from(refs);
}

/** True if this looks like a vault blob reference (not a browser Object URL). */
function isBlobRef(url: string): boolean {
    if (!url.startsWith('blob:')) return false;
    const after = url.slice(5);
    return !after.startsWith('http://') && !after.startsWith('https://') && !after.startsWith('tauri://');
}

/** Normalize a blob reference to the hash key used by resolvedUrlCache. */
function normalizeBlobHash(url: string): string {
    let hash = url;
    while (hash.startsWith('blob:')) {
        hash = hash.slice(5);
    }
    return hash.trim();
}

/**
 * Pre-resolve all blob references from the given feed entries in a single
 * batch IPC call. Populates the in-memory `resolvedUrlCache` used by
 * `useBlobUrl` so that individual hook instances return instantly without
 * making their own IPC round-trips.
 *
 * Safe to call multiple times — already-cached hashes are skipped.
 */
export async function prefetchBlobUrls(entries: FeedEntry[]): Promise<void> {
    const sources = collectBlobRefs(entries);
    if (sources.length === 0) return;

    try {
        const resolved = await invoke<Record<string, string>>('resolve_blob_urls_batch', { sources });

        for (const [src, url] of Object.entries(resolved)) {
            // Store under the normalized hash key that useBlobUrl uses
            const hash = normalizeBlobHash(src);
            if (hash && url) {
                resolvedUrlCache.set(hash, url);
            }
        }
    } catch (e) {
        // Non-fatal — individual useBlobUrl hooks will fall back to single-IPC resolution
        console.warn('[prefetchBlobUrls] Batch resolution failed, falling back to individual:', e);
    }
}
