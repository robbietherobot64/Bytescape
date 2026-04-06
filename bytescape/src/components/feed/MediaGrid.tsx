import { useRef, useEffect } from 'react';
import { BlobImage } from '../BlobImage';
import { VideoPlayer } from '../VideoPlayer';
import { ExpandIcon } from './icons';

/**
 * PURPOSE:
 * Displays up to four media items for a feed entry in a compact grid layout.
 *
 * HOW IT WORKS:
 * - Renders inline video players for `video` media kinds.
 * - Renders blob-aware images for non-video media.
 * - Delegates fullscreen/gallery state to parent via `openGallery(index)`.
 * - Fires no custom events — the virtualizer's built-in ResizeObserver on
 *   measured row elements automatically detects height changes when images load.
 * - Uses a ResizeObserver on the nearest `.feed-entry-card` ancestor to
 *   measure its clientWidth and set an explicit pixel width on the grid.
 *   This is a belt-and-suspenders fix for WebKitGTK, which has bugs with
 *   percentage width resolution inside CSS containment contexts.
 *
 * RELATED FILES:
 * - src/components/feed/GalleryOverlay.tsx
 * - src/components/VideoPlayer.tsx
 * - src/components/BlobImage.tsx
 * - src/components/FeedEntryCard.tsx (parent composer)
 */

interface MediaGridProps {
    mediaUrls: string[];
    mediaKinds?: string[];
    mediaSources: string[];
    thumbnailSources: string[];
    mediaDir?: string;
    openGallery: (index: number) => void;
}

/**
 * Hook: measure the nearest `.feed-entry-card` ancestor's clientWidth and
 * set it as an explicit pixel width on the grid element.  This bypasses
 * WebKitGTK bugs where `width: 100%` on a grid child resolves incorrectly
 * when ancestor elements have CSS containment (`contain: layout` or
 * `content-visibility: auto`).
 */
function useCardWidth(gridRef: React.RefObject<HTMLDivElement | null>) {
    useEffect(() => {
        const el = gridRef.current;
        if (!el) return;
        const card = el.closest('.feed-entry-card') as HTMLElement | null;
        if (!card) return;

        const sync = () => {
            const w = card.clientWidth;
            if (w > 0) el.style.width = `${w}px`;
        };

        // Measure immediately on mount
        sync();

        // Re-measure when the card resizes (e.g. window resize)
        const ro = new ResizeObserver(sync);
        ro.observe(card);
        return () => ro.disconnect();
    }, [gridRef]);
}

/**
 * Exported feed media grid renderer.
 *
 * Cross-system note:
 * Media URL hydration/resolution should be completed upstream (hook/store/provider)
 * before values are passed to this component.
 */
export function MediaGrid({ mediaUrls, mediaKinds, mediaSources, thumbnailSources, mediaDir, openGallery }: MediaGridProps) {
    const gridRef = useRef<HTMLDivElement>(null);
    useCardWidth(gridRef);

    if (mediaUrls.length === 0) return null;

    return (
        <div ref={gridRef} className={`post-media-grid items-${Math.min(mediaUrls.length, 4)}`}>
            {mediaUrls.slice(0, 4).map((_, i) => {
                // ── Per-cell derived display values ───────────────────────────
                const kind = (mediaKinds && mediaKinds[i]) || 'image';
                // For videos without a dedicated thumbnail, use the video source itself.
                // VideoPlayer will resolve blob:hash → local HTTP URL, and the browser
                // can extract a poster frame from the video with preload="metadata".
                const thumb = thumbnailSources[i] || (kind === 'video' ? undefined : mediaSources[i]);

                return (
                    <div key={i} className="media-item">
                        {/* ── Inline video cell ───────────────────────────────── */}
                        {kind === 'video' ? (
                            /* Inline video player — plays directly in the feed */
                            <div className="media-item-inner" onClick={(e) => e.stopPropagation()}>
                                <VideoPlayer
                                    src={mediaSources[i]}
                                    poster={thumb}
                                    className="inline-video-container"
                                    style={{ width: '100%', height: '100%' }}
                                />
                                <button
                                    className="media-expand-btn"
                                    onClick={(e) => { e.stopPropagation(); openGallery(i); }}
                                    title="Open fullscreen"
                                >
                                    <ExpandIcon />
                                </button>
                            </div>
                        ) : (
                            /* ── Image cell ───────────────────────────────────── */
                            /* Image — .media-item-inner fills .media-item via absolute positioning */
                            <div className="media-item-inner" onClick={(e) => { e.stopPropagation(); openGallery(i); }}>
                                <BlobImage
                                    src={mediaSources[i]}
                                    alt=""
                                    mediaDir={mediaDir}
                                    displayVariant
                                />
                                <button
                                    className="media-expand-btn"
                                    onClick={(e) => { e.stopPropagation(); openGallery(i); }}
                                    title="Open fullscreen"
                                >
                                    <ExpandIcon />
                                </button>
                            </div>
                        )}
                    </div>
                );
            })}
        </div>
    );
}
