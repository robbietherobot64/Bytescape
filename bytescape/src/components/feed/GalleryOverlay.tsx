import React from 'react';
import { createPortal } from 'react-dom';
import { VideoPlayer } from '../VideoPlayer';

interface GalleryOverlayProps {
    mediaSources: string[];
    thumbnailSources: string[];
    mediaKinds?: string[];
    galleryIndex: number;
    setGalleryIndex: (i: number) => void;
    onClose: () => void;
}

export function GalleryOverlay({ mediaSources, thumbnailSources, mediaKinds, galleryIndex, setGalleryIndex, onClose }: GalleryOverlayProps) {
    return createPortal(
        <div className="gallery-overlay" onClick={onClose}>
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
                <button className="gallery-close-btn" onClick={onClose}>✕</button>
            </div>
        </div>,
        document.body
    );
}
