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

    return (
        <div
            ref={containerRef}
            className="feed-media-bg"
            role="img"
            style={{
                backgroundImage: bgUrl ? `url("${bgUrl.replace(/"/g, '%22')}")` : 'none',
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
export function MediaGrid({ mediaUrls, mediaKinds, mediaSources, thumbnailSources, mediaDir, openGallery }: MediaGridProps) {
    if (mediaUrls.length === 0) return null;

    return (
        <div className={`post-media-grid items-${Math.min(mediaUrls.length, 4)}`}>
            {mediaUrls.slice(0, 4).map((_, i) => {
                const kind = (mediaKinds && mediaKinds[i]) || 'image';
                const thumb = thumbnailSources[i] || (kind === 'video' ? undefined : mediaSources[i]);

                return (
                    <div key={i} className={`media-item${kind === 'video' ? ' media-item-video' : ''}`} onClick={(e) => { e.stopPropagation(); openGallery(i); }}>
                        {kind === 'video' ? (
                            <div className="media-item-inner" onClick={(e) => e.stopPropagation()}>
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