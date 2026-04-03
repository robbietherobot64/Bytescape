import { useState, useEffect, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { BlobImage } from '../BlobImage';
import { VideoPlayer } from '../VideoPlayer';
import { SourceBadge } from '../SourceBadge';
import { FeedEntry } from '../../types';

/**
 * PURPOSE:
 * Fullscreen media inspector for gallery-style navigation across feed entries.
 *
 * HOW IT WORKS:
 * - Receives item-grouped media and flattens it into a single ordered slide list.
 * - Tracks focused slide index and supports keyboard navigation.
 * - Renders in a portal so overlay layering is decoupled from parent layout stacking.
 *
 * RELATED FILES:
 * - src/components/lenses/GalleryLens.tsx
 * - src/components/VideoPlayer.tsx
 * - src/components/BlobImage.tsx
 * - src/types/index.ts (`FeedEntry`)
 *
 * CROSS-SYSTEM NOTES:
 * `FeedEntry` values originate from Rust-side normalization and connector ingestion.
 * This component does not resolve/transform data; it only projects normalized media metadata.
 */

interface LightboxProps {
    items: FeedEntry[];
    /** Index of the initially focused item's media */
    initialIndex: number;
    /** Which media index within that item to show first */
    initialMediaIndex?: number;
    onClose: () => void;
    mediaDir?: string;
}

/**
 * Lightbox — fullscreen media overlay with keyboard/swipe navigation.
 *
 * Flattens all media from the provided items into a single navigable list.
 * Shows item metadata (source, date, caption) in a footer.
 *
 * Exported symbol docs:
 * - `Lightbox` is controlled externally via `onClose` and initial positioning props.
 * - It is intentionally stateless regarding parent filters/sorting and only manages
 *   local viewer cursor state.
 */
export function Lightbox({ items, initialIndex, initialMediaIndex = 0, onClose, mediaDir }: LightboxProps) {
    // ========================================================================
    // Slide list derivation
    // ========================================================================

    // Flatten all media across items into a navigable list
    interface MediaSlide {
        url: string;
        thumbnail?: string;
        kind: 'video' | 'audio' | 'pdf' | 'image' | 'unknown';
        entry: FeedEntry;
    }

    const slides: MediaSlide[] = [];
    let startSlideIndex = 0;

    items.forEach((entry, entryIdx) => {
        entry.media_urls.forEach((url, mediaIdx) => {
            if (entryIdx === initialIndex && mediaIdx === initialMediaIndex) {
                startSlideIndex = slides.length;
            }
            slides.push({
                url,
                thumbnail: entry.media_thumbnails[mediaIdx],
                kind: (entry.media_kinds && entry.media_kinds[mediaIdx]) || 'image',
                entry,
            });
        });
    });

    const [current, setCurrent] = useState(startSlideIndex);
    const slide = slides[current];

    // ========================================================================
    // Navigation actions
    // ========================================================================

    const goNext = useCallback(() => {
        setCurrent(prev => Math.min(prev + 1, slides.length - 1));
    }, [slides.length]);

    const goPrev = useCallback(() => {
        setCurrent(prev => Math.max(prev - 1, 0));
    }, []);

    // ========================================================================
    // Keyboard + page interaction lifecycle
    // ========================================================================

    // Keyboard navigation
    useEffect(() => {
        const handleKey = (e: KeyboardEvent) => {
            switch (e.key) {
                case 'Escape':
                    onClose();
                    break;
                case 'ArrowRight':
                case 'ArrowDown':
                    goNext();
                    break;
                case 'ArrowLeft':
                case 'ArrowUp':
                    goPrev();
                    break;
            }
        };
        window.addEventListener('keydown', handleKey);
        return () => window.removeEventListener('keydown', handleKey);
    }, [onClose, goNext, goPrev]);

    // Prevent body scroll while lightbox is open
    useEffect(() => {
        document.body.style.overflow = 'hidden';
        return () => { document.body.style.overflow = ''; };
    }, []);

    if (!slide) {
        onClose();
        return null;
    }

    const formatDate = (ts: number) => {
        const d = new Date(ts * 1000);
        return d.toLocaleDateString(undefined, {
            year: 'numeric', month: 'long', day: 'numeric',
            hour: '2-digit', minute: '2-digit',
        });
    };

    return createPortal(
        <div className="lightbox-overlay" onClick={onClose}>
            <div className="lightbox-content" onClick={e => e.stopPropagation()}>
                {/* ==================== Header controls ==================== */}
                {/* Close button */}
                <button className="lightbox-close" onClick={onClose} title="Close (Esc)">✕</button>

                {/* Navigation arrows */}
                {current > 0 && (
                    <button className="lightbox-nav lightbox-prev" onClick={goPrev} title="Previous (←)">
                        ‹
                    </button>
                )}
                {current < slides.length - 1 && (
                    <button className="lightbox-nav lightbox-next" onClick={goNext} title="Next (→)">
                        ›
                    </button>
                )}

                {/* ==================== Media stage ==================== */}
                {/* Media */}
                <div className="lightbox-media">
                    {slide.kind === 'video' ? (
                        <VideoPlayer
                            src={slide.url}
                            poster={slide.thumbnail || slide.url}
                            autoPlay={true}
                            style={{ maxWidth: '100%', maxHeight: '85vh' } as React.CSSProperties}
                        />
                    ) : (
                        <BlobImage
                            src={slide.url}
                            alt={slide.entry.content?.slice(0, 80) || ''}
                            mediaDir={mediaDir}
                            style={{ maxWidth: '100%', maxHeight: '85vh', objectFit: 'contain' }}
                        />
                    )}
                </div>

                {/* ==================== Footer metadata ==================== */}
                {/* Footer metadata */}
                <div className="lightbox-footer">
                    <div className="lightbox-footer-left">
                        <SourceBadge source={slide.entry.source} />
                        <span className="lightbox-date">{formatDate(slide.entry.timestamp)}</span>
                    </div>
                    {slide.entry.content && (
                        <div className="lightbox-caption">
                            {slide.entry.content.length > 200
                                ? slide.entry.content.slice(0, 200) + '…'
                                : slide.entry.content}
                        </div>
                    )}
                    <div className="lightbox-counter">
                        {current + 1} / {slides.length}
                    </div>
                </div>
            </div>
        </div>,
        document.body
    );
}
