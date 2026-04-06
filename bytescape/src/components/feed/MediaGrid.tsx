import { useMemo } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { useLazyBlobUrl } from '../../hooks/useBlobUrl';
import { VideoPlayer } from '../VideoPlayer';
import { ExpandIcon } from './icons';

/**
 * PURPOSE:
 * Displays up to four media items for a feed entry in a compact grid layout.
 *
 * HOW IT WORKS:
 * - Images render as <div> elements with background-image (NOT <img> tags).
 *   A <div> is NOT a replaced element, so position:absolute + width:100% +
 *   height:100% is guaranteed to work in WebKitGTK. <img> elements have
 *   replaced-element sizing quirks that cause inconsistent widths.
 * - background-size:cover + background-position:center gives identical
 *   visual results to object-fit:cover + object-position:center.
 * - Videos still use absolute positioning via .media-item-inner wrapper
 *   because VideoPlayer is a complex component that needs a sized container.
 *
 * RELATED FILES:
 * - src/components/feed/GalleryOverlay.tsx
 * - src/components/VideoPlayer.tsx
 * - src/components/BlobImage.tsx (used for gallery overlay, NOT here)
 * - src/components/FeedEntryCard.tsx (parent composer)
 */

/**
 * Internal component: renders a <div> with background-image instead of <img>.
 * Reuses the same URL resolution pipeline as BlobImage (useLazyBlobUrl,
 * convertFileSrc, blob-display: prefix) but avoids all replaced-element
 * sizing issues by being a plain <div>.
 */
function FeedMediaImage({ src, mediaDir }: { src: string; mediaDir?: string }) {
    // Transform blob: → blob-display: for lower-resolution display variant
    const effectiveSrc = useMemo(() => {
        if (src?.startsWith('blob:')) {
            const afterPrefix = src.slice(5);
            if (!afterPrefix.startsWith('http://') && !afterPrefix.startsWith('https://') && !afterPrefix.startsWith('tauri://')) {
                return `blob-display:${afterPrefix}`;
            }
        }
        return src;
    }, [src]);

    // Lazy blob URL resolution (IntersectionObserver-gated)
    const [containerRef, blobUrl, , error] = useLazyBlobUrl(
        effectiveSrc?.startsWith('blob:') || effectiveSrc?.startsWith('blob-display:') ? effectiveSrc : undefined
    );

    // Final URL resolution — same logic as BlobImage
    const resolvedUrl = useMemo(() => {
        if (!src) return null;
        if (effectiveSrc?.startsWith('blob:') || effectiveSrc?.startsWith('blob-display:')) return blobUrl;
        if (src.startsWith('http://') || src.startsWith('https://')) return src;
        if (mediaDir) {
            if (src.startsWith('media/')) {
                return convertFileSrc(`${mediaDir}/${src.replace(/^media\//, '')}`);
            } else if (src.startsWith('/') || src.match(/^[A-Za-z]:\\/)) {
                return convertFileSrc(src);
            } else {
                return convertFileSrc(`${mediaDir}/${src}`);
            }
        }
        return convertFileSrc(src);
    }, [src, effectiveSrc, blobUrl, mediaDir]);

    const bgUrl = resolvedUrl && !error ? resolvedUrl : null;

    // ALL sizing as inline styles — no CSS class dependency for layout
    return (
        <div
            ref={containerRef}
            className="feed-media-bg"
            role="img"
            style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                height: '100%',
                backgroundImage: bgUrl ? `url("${bgUrl.replace(/"/g, '%22')}")` : 'none',
                backgroundSize: 'cover',
                backgroundPosition: 'center',
                backgroundRepeat: 'no-repeat',
            } as React.CSSProperties}
        />
    );
}

interface MediaGridProps {
    mediaUrls: string[];
    mediaKinds?: string[];
    mediaSources: string[];
    thumbnailSources: string[];
    mediaDir?: string;
    openGallery: (index: number) => void;
}

/**
 * Exported feed media grid renderer.
 */
/** Compute grid-template-columns for N items */
function gridCols(n: number): string {
    if (n <= 1) return '1fr';
    if (n === 3) return '2fr 1fr';
    return '1fr 1fr'; // 2 or 4+
}

/** Compute grid-template-rows for N items */
function gridRows(n: number): string | undefined {
    if (n === 3 || n >= 4) return '1fr 1fr';
    return undefined;
}

export function MediaGrid({ mediaUrls, mediaKinds, mediaSources, thumbnailSources, mediaDir, openGallery }: MediaGridProps) {
    if (mediaUrls.length === 0) return null;

    const count = Math.min(mediaUrls.length, 4);
    const isSingle = count === 1;

    // ALL grid layout as inline styles — CSS cannot interfere
    const gridStyle: React.CSSProperties = {
        display: 'grid',
        gridTemplateColumns: gridCols(count),
        gridTemplateRows: gridRows(count),
        gap: '2px',
        width: '100%',
        overflow: 'hidden',
        margin: 0,
        borderTop: '2px solid #999',
        borderBottom: '2px solid #999',
    };

    return (
        <div className={`post-media-grid items-${count}`} style={gridStyle}>
            {mediaUrls.slice(0, 4).map((_, i) => {
                const kind = (mediaKinds && mediaKinds[i]) || 'image';
                const thumb = thumbnailSources[i] || (kind === 'video' ? undefined : mediaSources[i]);
                const isVideo = kind === 'video';

                // Inline styles for the cell — position:relative + aspect-ratio
                const cellStyle: React.CSSProperties = {
                    position: 'relative',
                    overflow: 'hidden',
                    cursor: 'pointer',
                    background: '#000',
                    width: '100%',
                    aspectRatio: isVideo ? '16 / 9' : (isSingle ? '4 / 3' : '16 / 9'),
                    minHeight: isSingle && !isVideo ? 200 : undefined,
                    maxHeight: isSingle ? 600 : undefined,
                };

                // For items-3, first child spans two rows
                if (count === 3 && i === 0) {
                    (cellStyle as Record<string, unknown>).gridRow = '1 / 3';
                }

                return (
                    <div
                        key={i}
                        className={`media-item${isVideo ? ' media-item-video' : ''}`}
                        style={cellStyle}
                        onClick={(e) => { e.stopPropagation(); openGallery(i); }}
                    >
                        {isVideo ? (
                            <div
                                className="media-item-inner"
                                style={{ position: 'absolute', top: 0, left: 0, width: '100%', height: '100%' } as React.CSSProperties}
                                onClick={(e) => e.stopPropagation()}
                            >
                                <VideoPlayer
                                    src={mediaSources[i]}
                                    poster={thumb}
                                    className="inline-video-container"
                                    style={{ width: '100%', height: '100%' } as React.CSSProperties}
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
                            <>
                                <FeedMediaImage
                                    src={mediaSources[i]}
                                    mediaDir={mediaDir}
                                />
                                <button
                                    className="media-expand-btn"
                                    onClick={(e) => { e.stopPropagation(); openGallery(i); }}
                                    title="Open fullscreen"
                                >
                                    <ExpandIcon />
                                </button>
                            </>
                        )}
                    </div>
                );
            })}
        </div>
    );
}