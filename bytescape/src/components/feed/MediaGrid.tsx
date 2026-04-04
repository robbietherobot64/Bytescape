import React from 'react';
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
 * Exported feed media grid renderer.
 *
 * Cross-system note:
 * Media URL hydration/resolution should be completed upstream (hook/store/provider)
 * before values are passed to this component.
 */
export function MediaGrid({ mediaUrls, mediaKinds, mediaSources, thumbnailSources, mediaDir, openGallery }: MediaGridProps) {
    if (mediaUrls.length === 0) return null;

    return (
        <div className={`post-media-grid items-${Math.min(mediaUrls.length, 4)}`}>
            {mediaUrls.slice(0, 4).map((_, i) => {
                // ── Per-cell derived display values ───────────────────────────
                const kind = (mediaKinds && mediaKinds[i]) || 'image';
                const thumb = thumbnailSources[i] || mediaSources[i];

                return (
                    <div key={i} className="media-item">
                        {/* ── Inline video cell ───────────────────────────────── */}
                        {kind === 'video' ? (
                            /* Inline video player — plays directly in the feed */
                            <div onClick={(e) => e.stopPropagation()} style={{ width: '100%', height: '100%' }}>
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
                            /* Image */
                            <div onClick={(e) => { e.stopPropagation(); openGallery(i); }}>
                                <BlobImage
                                    src={mediaSources[i]}
                                    alt=""
                                    mediaDir={mediaDir}
                                    displayVariant
                                    style={{ width: '100%', height: '100%', display: 'block', objectFit: 'contain' } as React.CSSProperties}
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
