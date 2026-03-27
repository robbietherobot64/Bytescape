import { useRef, useCallback } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { FeedEntry } from '../../types';
import { FeedItemRenderer } from '../lenses/FeedItemRenderer';

interface VirtualizedFeedListProps {
    entries: FeedEntry[];
    resolveMediaPath?: (path: string) => string;
    mediaDir?: string;
    /** The scrollable container element ref */
    scrollContainerRef: React.RefObject<HTMLDivElement | null>;
    /** Optional group badge renderer for group feed context */
    groupBadge?: (entry: FeedEntry) => React.ReactNode;
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
}: VirtualizedFeedListProps) {
    const measurementCache = useRef<Map<number, number>>(new Map());

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
        overscan: 5,
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
                        }}
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
