import { BlobImage } from '../BlobImage';
import { VideoPlayer } from '../VideoPlayer';
import { ExpandIcon } from './icons';

/**
 * PURPOSE:
 * Displays up to four media items for a feed entry in a compact grid layout.
 *
 * HOW IT WORKS:
 * - Images are rendered as simple block-level elements with intrinsic sizing
 *   (width: 100% + aspect-ratio + object-fit: cover on the <img> itself).
 *   NO absolute positioning, NO inner wrappers, NO ResizeObserver.
 *   This is the simplest possible model, immune to WebKitGTK containment bugs.
 * - Videos still use absolute positioning via .media-item-inner wrapper
 *   because VideoPlayer is a complex component that needs a sized container.
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
                            </>
                        )}
                    </div>
                );
            })}
        </div>
    );
}
