import React from 'react';
import { createPortal } from 'react-dom';
import { VideoPlayer } from '../VideoPlayer';

/**
 * PURPOSE:
 * Fullscreen/lightbox overlay for feed media browsing.
 *
 * HOW IT WORKS:
 * - Renders through a React portal into `document.body`.
 * - Switches image/video rendering by `mediaKinds[galleryIndex]`.
 * - Delegates index updates and close behavior to parent callbacks.
 *
 * RELATED FILES:
 * - src/components/feed/MediaGrid.tsx
 * - src/components/VideoPlayer.tsx
 * - src/components/FeedEntryCard.tsx (gallery state ownership)
 */

interface GalleryOverlayProps {
    mediaSources: string[];
    thumbnailSources: string[];
    mediaKinds?: string[];
    galleryIndex: number;
    setGalleryIndex: (i: number) => void;
    onClose: () => void;
}

/**
 * Exported fullscreen gallery overlay.
 *
 * Cross-system note:
 * This component intentionally avoids data fetching/state ownership; hook/store
 * layers should provide stable media source arrays and selection state.
 */
export function GalleryOverlay({ mediaSources, thumbnailSources, mediaKinds, galleryIndex, setGalleryIndex, onClose }: GalleryOverlayProps) {
    return createPortal(
        // ── Backdrop ──────────────────────────────────────────────────────────
        <div className="gallery-overlay" onClick={onClose}>
            {/* ── Modal content ──────────────────────────────────────────────── */}
            <div className="gallery-content" onClick={e => e.stopPropagation()}>
                {(mediaKinds && mediaKinds[galleryIndex]) === 'video' ? (
                    <VideoPlayer
                        src={mediaSources[galleryIndex]}
                        poster={thumbnailSources[galleryIndex] || mediaSources[galleryIndex]}
                        autoPlay={true}
                    />
                ) : (
                    <img
                        src={mediaSources[galleryIndex]}
                        alt=""
                        style={{ maxWidth: '100%', maxHeight: '90vh', objectFit: 'contain' } as React.CSSProperties}
                    />
                )}
                {/* ── Slide indicators ───────────────────────────────────────── */}
                {mediaSources.length > 1 && (
                    <div className="gallery-dots">
                        {mediaSources.map((_, i) => (
                            <button
                                key={i}
                                className={`gallery-dot ${i === galleryIndex ? 'active' : ''}`}
                                onClick={() => setGalleryIndex(i)}
                            />
                        ))}
                    </div>
                )}
                {/* ── Close action ───────────────────────────────────────────── */}
                <button className="gallery-close-btn" onClick={onClose}>✕</button>
            </div>
        </div>,
        document.body
    );
}
