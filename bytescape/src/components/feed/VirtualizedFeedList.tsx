import React, { memo } from 'react';
import { FeedEntry } from '../../types';
import { FeedItemRenderer } from '../lenses/FeedItemRenderer';

/**
 * FeedList — Bounded flat-DOM feed renderer (v15 rebuild)
 *
 * Renders all ≤150 entries directly in the DOM, delegating off-screen
 * paint/layout elision to CSS `content-visibility: auto`. The browser
 * natively skips rendering for cards outside the viewport, achieving
 * virtualization-equivalent performance without JS-side height estimation,
 * absolute positioning, or @tanstack/react-virtual overhead.
 *
 * Why content-visibility instead of virtualization:
 * - With MAX_ENTRIES=150, full DOM mount is < 150 nodes (well within budget)
 * - Browser-native skip is smoother than JS-measured absolute positioning
 * - Eliminates height estimation errors that cause scroll jumps
 * - Simpler code, no height cache, no resize observers
 *
 * RELATED FILES:
 * - src/components/lenses/FeedItemRenderer.tsx (per-entry renderer)
 * - src/hooks/useUnifiedFeed.ts (upstream provider, MAX_ENTRIES=150)
 * - src/screens/FeedScreen.tsx (primary consumer)
 */

interface FeedListProps {
    entries: FeedEntry[];
    resolveMediaPath?: (path: string) => string;
    mediaDir?: string;
    /** Optional group badge renderer for group feed context */
    groupBadge?: (entry: FeedEntry) => React.ReactNode;
    /** Slot for source toggle bar (Following tab) */
    sourceToggleBar?: React.ReactNode;
    /** Ref to the scrollable container (used by pull-to-refresh) */
    scrollContainerRef?: React.RefObject<HTMLDivElement | null>;
}

/**
 * Memoized wrapper for a single feed card.
 * content-visibility:auto on the outer div lets the browser skip
 * layout+paint for off-screen cards. contain-intrinsic-size gives
 * a height estimate to prevent scrollbar jumps.
 */
const FeedCardWrapper = memo(function FeedCardWrapper({
    entry,
    resolveMediaPath,
    mediaDir,
    groupBadge,
}: {
    entry: FeedEntry;
    resolveMediaPath?: (path: string) => string;
    mediaDir?: string;
    groupBadge?: (entry: FeedEntry) => React.ReactNode;
}) {
    return (
        <div
            className="feed-card-wrapper"
            style={{
                contentVisibility: 'auto',
                containIntrinsicBlockSize: 'auto 400px',
                width: '100%',
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
});

/**
 * Exported feed list component — drop-in replacement for the old
 * VirtualizedFeedList. Same name export for backward compatibility.
 */
export function VirtualizedFeedList({
    entries,
    resolveMediaPath,
    mediaDir,
    groupBadge,
    sourceToggleBar,
}: FeedListProps) {
    if (entries.length === 0) return null;

    return (
        <div className="feed-list-container">
            {sourceToggleBar}
            {entries.map((entry) => {
                const key = entry.reposted_by
                    ? `repost:${entry.reposted_by}:${entry.id}`
                    : entry.id;
                return (
                    <FeedCardWrapper
                        key={key}
                        entry={entry}
                        resolveMediaPath={resolveMediaPath}
                        mediaDir={mediaDir}
                        groupBadge={groupBadge}
                    />
                );
            })}
        </div>
    );
}
