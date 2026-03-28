import { useRef, useCallback, useEffect } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { FeedEntry } from '../../types';
import { FeedItemRenderer } from '../lenses/FeedItemRenderer';
import { prefetchBlobUrls } from '../../utils/prefetchBlobUrls';

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
 * Virtualized feed list using @tanstack/react-virtual.
 * Only renders visible entries + overscan, reducing DOM nodes from 500-2000+
 * to ~50-100 for smooth 60fps scrolling.
 */
export function VirtualizedFeedList({
    entries,
    resolveMediaPath,
    mediaDir,
    scrollContainerRef,
    groupBadge,
    overscan: overscanProp,
}: VirtualizedFeedListProps) {
    const measurementCache = useRef<Map<number, number>>(new Map());

    // Clear stale measurements when feed content changes (e.g., tab switch,
    // new posts loaded). Without this, old heights persist and cause gaps.
    useEffect(() => {
        measurementCache.current.clear();
    }, [entries]);

    const virtualizer = useVirtualizer({
        count: entries.length,
        getScrollElement: () => scrollContainerRef.current,
        estimateSize: useCallback((index: number) => {
            // Use cached measurement if available, otherwise estimate based on content
            const cached = measurementCache.current.get(index);
            if (cached) return cached;
            const entry = entries[index];
            if (!entry) return 300;
            // Rough estimate: base height + media + text
            const hasMedia = entry.media_urls.length > 0;
            const textLen = (entry.content || '').length;
            let estimate = 120; // base header + engagement
            if (hasMedia) estimate += 300; // media grid
            if (textLen > 0) estimate += Math.min(textLen * 0.4, 200); // text content
            if (entry.link_url) estimate += 100; // link preview
            if (entry.owner_replies?.length) estimate += entry.owner_replies.length * 80; // thread replies
            return estimate;
        }, [entries]),
        overscan: overscanProp ?? 5,
        measureElement: (el: Element) => {
            const htmlEl = el as HTMLElement;
            const index = Number(htmlEl.dataset.index);
            const height = htmlEl.getBoundingClientRect().height;
            if (!isNaN(index) && height > 0) {
                measurementCache.current.set(index, height);
            }
            return height;
        },
    });

    const virtualItems = virtualizer.getVirtualItems();

    // ── Viewport-aware blob prefetch ──
    // Instead of resolving ALL blob URLs for ALL feed entries on load
    // (which hammered the backend with hundreds of IPC calls), only prefetch
    // blobs for entries currently visible + a small lookahead buffer.
    // This turns O(N_total_entries) into O(N_visible + lookahead) per render.
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
                            contain: 'layout style',
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
