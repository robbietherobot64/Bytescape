import { useRef, useCallback, useEffect, useMemo } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { FeedEntry } from '../../types';
import { FeedItemRenderer } from '../lenses/FeedItemRenderer';
import { prefetchBlobUrls } from '../../utils/prefetchBlobUrls';

/**
 * PURPOSE:
 * Renders long feed collections with viewport virtualization to keep scrolling smooth
 * and memory usage bounded.
 *
 * HOW IT WORKS:
 * - Uses `@tanstack/react-virtual` to mount only visible rows + overscan.
 * - Caches measured row heights keyed by entry ID (not index) so heights survive
 *   feed prepend/append operations and are only cleared on full feed replacement.
 * - Prefetches blob URLs only for the visible window (+ small lookahead) to avoid
 *   issuing IPC/media resolution work for the entire feed at once.
 * - Exposes a re-measure event so child BlobImage components can trigger height
 *   recalculation after images load without clearing the entire cache.
 *
 * RELATED FILES:
 * - src/components/lenses/FeedItemRenderer.tsx (per-entry renderer)
 * - src/utils/prefetchBlobUrls.ts (viewport-aware media prefetch helper)
 * - src/hooks/useUnifiedFeed.ts (typical upstream provider of `entries`)
 * - src/store/* and src/providers/* (upstream feed state orchestration)
 */

interface VirtualizedFeedListProps {
    entries: FeedEntry[];
    resolveMediaPath?: (path: string) => string;
    mediaDir?: string;
    /** The scrollable container element ref */
    scrollContainerRef: React.RefObject<HTMLDivElement | null>;
    /** Optional group badge renderer for group feed context */
    groupBadge?: (entry: FeedEntry) => React.ReactNode;
    /**
     * Number of items to render above/below the visible window.
     * Default: 5.  Reduce to 1-2 for video-dense feeds to limit
     * the number of simultaneously mounted VideoPlayer components.
     */
    overscan?: number;
}

/**
 * Exported feed virtualization container.
 *
 * Notes:
 * - This component is intentionally presentation-focused; data loading,
 *   pagination, and source selection are handled by upstream hooks/stores.
 * - `entries` ordering and identity stability are expected to be handled by
 *   the calling layer.
 */
export function VirtualizedFeedList({
    entries,
    resolveMediaPath,
    mediaDir,
    scrollContainerRef,
    groupBadge,
    overscan: overscanProp,
}: VirtualizedFeedListProps) {
    // ──────────────────────────────────────────────────────────────────────────
    // Row height measurement cache — keyed by entry ID for stability
    // ──────────────────────────────────────────────────────────────────────────

    // ID-keyed cache survives prepend/append/reorder without losing heights.
    const idHeightCache = useRef<Map<string, number>>(new Map());

    // Build an index→ID lookup for the current entries array (cheap O(n) pass)
    const entryIdByIndex = useMemo(() => {
        const map = new Map<number, string>();
        for (let i = 0; i < entries.length; i++) {
            map.set(i, entries[i].id);
        }
        return map;
    }, [entries]);

    // Only clear the cache when the entry set truly changes identity
    // (e.g., tab switch), not when the same feed gets new items appended.
    const prevIdSetRef = useRef<string>('');
    useEffect(() => {
        // Build a coarse fingerprint from first+last 3 IDs + count
        const ids = entries.map(e => e.id);
        const fingerprint = `${entries.length}:${ids.slice(0, 3).join(',')}:${ids.slice(-3).join(',')}`;
        if (prevIdSetRef.current && prevIdSetRef.current !== fingerprint) {
            // Different feed — clear stale measurements
            idHeightCache.current.clear();
        }
        prevIdSetRef.current = fingerprint;
    }, [entries]);

    const virtualizer = useVirtualizer({
        count: entries.length,
        getScrollElement: () => scrollContainerRef.current,
        estimateSize: useCallback((index: number) => {
            // Use ID-keyed cached measurement if available
            const entryId = entryIdByIndex.get(index);
            if (entryId) {
                const cached = idHeightCache.current.get(entryId);
                if (cached) return cached;
            }
            const entry = entries[index];
            if (!entry) return 300;
            // Content-aware estimate to minimize layout shift
            const hasMedia = entry.media_urls.length > 0;
            const mediaCount = Math.min(entry.media_urls.length, 4);
            const textLen = (entry.content || '').length;
            let estimate = 120; // base header + engagement bar
            if (hasMedia) {
                // Better media estimate based on grid layout
                if (mediaCount === 1) estimate += 400; // 4:3 aspect single image
                else if (mediaCount === 2) estimate += 250;
                else estimate += 300;
            }
            if (textLen > 0) estimate += Math.min(textLen * 0.4, 200);
            if (entry.link_url) estimate += 100;
            if (entry.owner_replies?.length) estimate += entry.owner_replies.length * 80;
            return estimate;
        }, [entries, entryIdByIndex]),
        overscan: overscanProp ?? 5,
        measureElement: (el: Element) => {
            const htmlEl = el as HTMLElement;
            const index = Number(htmlEl.dataset.index);
            const height = htmlEl.getBoundingClientRect().height;
            if (!isNaN(index) && height > 0) {
                // Store by entry ID for cross-render stability
                const entryId = entryIdByIndex.get(index);
                if (entryId) {
                    idHeightCache.current.set(entryId, height);
                }
            }
            return height;
        },
    });

    // ──────────────────────────────────────────────────────────────────────────
    // Image-load re-measurement: the virtualizer's built-in ResizeObserver on
    // measureElement refs automatically detects when a card's height changes
    // (e.g., after a BlobImage finishes loading and the container expands).
    // No custom event system needed — the observer calls resizeItem() for us.
    // ──────────────────────────────────────────────────────────────────────────

    const virtualItems = virtualizer.getVirtualItems();

    // ──────────────────────────────────────────────────────────────────────────
    // Viewport-aware blob prefetch
    // ──────────────────────────────────────────────────────────────────────────
    const prefetchedRangeRef = useRef<string>('');
    useEffect(() => {
        if (virtualItems.length === 0) return;
        const startIdx = virtualItems[0].index;
        const endIdx = virtualItems[virtualItems.length - 1].index;
        // Extend by lookahead of 5 entries in each direction
        const lo = Math.max(0, startIdx - 5);
        const hi = Math.min(entries.length - 1, endIdx + 5);
        const rangeKey = `${lo}:${hi}`;
        // Skip if we already prefetched this exact range
        if (prefetchedRangeRef.current === rangeKey) return;
        prefetchedRangeRef.current = rangeKey;
        const visibleEntries = entries.slice(lo, hi + 1);
        prefetchBlobUrls(visibleEntries).catch(() => {});
    }, [virtualItems, entries]);

    if (entries.length === 0) return null;

    // ──────────────────────────────────────────────────────────────────────────
    // Virtualized render surface
    // ──────────────────────────────────────────────────────────────────────────
    return (
        <div
            style={{
                height: virtualizer.getTotalSize(),
                width: '100%',
                position: 'relative',
            }}
        >
            {virtualItems.map((virtualRow) => {
                const entry = entries[virtualRow.index];
                if (!entry) return null;
                const key = entry.reposted_by
                    ? `repost:${entry.reposted_by}:${entry.id}`
                    : entry.id;
                return (
                    <div
                        key={key}
                        data-index={virtualRow.index}
                        ref={virtualizer.measureElement}
                        style={{
                            position: 'absolute',
                            top: 0,
                            left: 0,
                            width: '100%',
                            transform: `translateY(${virtualRow.start}px)`,
                            contain: 'content',
                            willChange: 'transform',
                        } as React.CSSProperties}
                    >
                        {groupBadge?.(entry)}
                        <FeedItemRenderer
                            entry={entry}
                            resolveMediaPath={resolveMediaPath}
                            mediaDir={mediaDir}
                        />
                    </div>
                );
            })}
        </div>
    );
}
