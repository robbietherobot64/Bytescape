import React, { useMemo, useState } from 'react';
import { createPortal } from 'react-dom';
import { convertFileSrc } from '@tauri-apps/api/core';
import { useLazyBlobUrl } from '../../hooks/useBlobUrl';
import { VideoPlayer } from '../VideoPlayer';
import { ExpandIcon } from './icons';
import './FeedMedia.css';

/**
 * PURPOSE:
 * Unified feed media renderer — displays 1-4 images/videos in a grid and
 * provides a built-in fullscreen gallery overlay. Replaces the old MediaGrid
 * + GalleryOverlay pair that failed to render consistent widths in WebKitGTK.
 *
 * DESIGN PRINCIPLES:
 * 1. ALL layout properties are inline styles. CSS cannot override inline
 *    styles, so WebKitGTK containment bugs, content-visibility, and
 *    cascade specificity cannot interfere with sizing.
 * 2. Images render as <div> with background-image, NOT <img>. WebKitGTK's
 *    replaced-element sizing for <img> is fundamentally broken — it ignores
 *    CSS width/height and uses intrinsic image dimensions instead.
 * 3. The gallery overlay is an internal render mode, not a separate component.
 *    This keeps gallery state local and eliminates prop drilling.
 *
 * RELATED FILES:
 * - src/components/VideoPlayer.tsx (shared video player)
 * - src/hooks/useBlobUrl.ts (blob URL resolution with IntersectionObserver)
 * - src/components/FeedEntryCard.tsx (parent consumer)
 * - src/components/feed/FeedMedia.css (decorative-only styles)
 */

// ═════════════════════════════════════════════════════════════════════════════
// URL Resolution Hook (shared by FeedMediaImage)
// ═════════════════════════════════════════════════════════════════════════════

/**
 * Resolve a media source string to a browser-displayable URL.
 * Handles blob: hashes, filesystem paths, and HTTP URLs.
 * Uses IntersectionObserver to defer blob resolution until near-viewport.
 */
function useResolvedMediaUrl(src: string, mediaDir?: string): [
    React.RefCallback<HTMLElement>,
    string | null,
] {
    // Transform blob: → blob-display: for lower-res display variant
    const effectiveSrc = useMemo(() => {
        if (src?.startsWith('blob:')) {
            const afterPrefix = src.slice(5);
            if (
                !afterPrefix.startsWith('http://') &&
                !afterPrefix.startsWith('https://') &&
                !afterPrefix.startsWith('tauri://')
            ) {
                return `blob-display:${afterPrefix}`;
            }
        }
        return src;
    }, [src]);

    // Lazy blob resolution (IntersectionObserver-gated)
    const [containerRef, blobUrl, , error] = useLazyBlobUrl(
        effectiveSrc?.startsWith('blob:') || effectiveSrc?.startsWith('blob-display:')
            ? effectiveSrc
            : undefined
    );

    // Final URL — same resolution logic as BlobImage
    const resolvedUrl = useMemo(() => {
        if (!src) return null;
        if (effectiveSrc?.startsWith('blob:') || effectiveSrc?.startsWith('blob-display:')) {
            return blobUrl;
        }
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

    const finalUrl = resolvedUrl && !error ? resolvedUrl : null;
    return [containerRef, finalUrl];
}

// ═════════════════════════════════════════════════════════════════════════════
// FeedMediaImage — renders a single image as <div> with background-image
// ═════════════════════════════════════════════════════════════════════════════

function FeedMediaImage({ src, mediaDir }: { src: string; mediaDir?: string }) {
    const [containerRef, url] = useResolvedMediaUrl(src, mediaDir);

    return (
        <div
            ref={containerRef}
            role="img"
            style={{
                position: 'absolute' as const,
                top: 0,
                left: 0,
                width: '100%',
                height: '100%',
                backgroundImage: url ? `url("${url.replace(/"/g, '%22')}")` : 'none',
                backgroundSize: 'cover',
                backgroundPosition: 'center',
                backgroundRepeat: 'no-repeat',
            }}
        />
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// Grid Layout Computation (pure functions, no CSS dependency)
// ═════════════════════════════════════════════════════════════════════════════

function gridColumns(count: number): string {
    if (count <= 1) return '1fr';
    if (count === 3) return '2fr 1fr';
    return '1fr 1fr';
}

function gridRows(count: number): string | undefined {
    if (count >= 3) return '1fr 1fr';
    return undefined;
}

function cellAspectRatio(count: number, isVideo: boolean): string {
    if (count === 1 && !isVideo) return '4 / 3';
    return '16 / 9';
}

// ═════════════════════════════════════════════════════════════════════════════
// Gallery Overlay (internal, portal-rendered)
// ═════════════════════════════════════════════════════════════════════════════

interface GalleryProps {
    mediaSources: string[];
    thumbnailSources: string[];
    mediaKinds?: string[];
    index: number;
    setIndex: (i: number) => void;
    onClose: () => void;
}

function GalleryOverlay({ mediaSources, thumbnailSources, mediaKinds, index, setIndex, onClose }: GalleryProps) {
    const kind = mediaKinds?.[index] || 'image';

    return createPortal(
        <div
            className="fm-overlay"
            style={{
                position: 'fixed' as const,
                inset: 0,
                zIndex: 9999,
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
            }}
            onClick={onClose}
        >
            <div
                className="fm-overlay-content"
                style={{
                    position: 'relative' as const,
                    maxWidth: '90vw',
                    maxHeight: '90vh',
                    display: 'flex',
                    flexDirection: 'column' as const,
                    alignItems: 'center',
                    justifyContent: 'center',
                    padding: 4,
                }}
                onClick={(e) => e.stopPropagation()}
            >
                {kind === 'video' ? (
                    <div style={{ maxWidth: '90vw', maxHeight: '80vh', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                        <VideoPlayer
                            src={mediaSources[index]}
                            poster={thumbnailSources[index] || mediaSources[index]}
                            autoPlay={true}
                        />
                    </div>
                ) : (
                    <img
                        src={mediaSources[index]}
                        alt=""
                        style={{
                            maxWidth: '100%',
                            maxHeight: '80vh',
                            objectFit: 'contain' as const,
                        }}
                    />
                )}

                {/* Slide indicators */}
                {mediaSources.length > 1 && (
                    <div style={{ display: 'flex', gap: 4, marginTop: 8 }}>
                        {mediaSources.map((_, i) => (
                            <button
                                key={i}
                                className={`fm-dot${i === index ? ' fm-dot--active' : ''}`}
                                style={{ width: 10, height: 10 }}
                                onClick={() => setIndex(i)}
                            />
                        ))}
                    </div>
                )}

                {/* Close button */}
                <button
                    className="fm-overlay-close"
                    style={{
                        position: 'absolute' as const,
                        top: -28,
                        right: -4,
                        padding: '2px 8px',
                    }}
                    onClick={onClose}
                >
                    ✕
                </button>
            </div>
        </div>,
        document.body
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// FeedMedia — Exported main component
// ═════════════════════════════════════════════════════════════════════════════

interface FeedMediaProps {
    mediaSources: string[];
    thumbnailSources: string[];
    mediaKinds?: string[];
    mediaDir?: string;
}

export function FeedMedia({ mediaSources, thumbnailSources, mediaKinds, mediaDir }: FeedMediaProps) {
    const [galleryOpen, setGalleryOpen] = useState(false);
    const [galleryIndex, setGalleryIndex] = useState(0);

    if (mediaSources.length === 0) return null;

    const count = Math.min(mediaSources.length, 4);
    const isSingle = count === 1;

    const openGallery = (index: number) => {
        // Pause all playing videos before opening gallery
        document.querySelectorAll('video').forEach((v) => {
            try { (v as HTMLVideoElement).pause(); } catch (_) { /* ignore */ }
        });
        setGalleryIndex(index);
        setGalleryOpen(true);
    };

    // ── Grid container: ALL layout inline ──
    const gridStyle: React.CSSProperties = {
        display: 'grid',
        gridTemplateColumns: gridColumns(count),
        gridTemplateRows: gridRows(count),
        gap: 2,
        width: '100%',
        overflow: 'hidden',
        margin: 0,
        boxSizing: 'border-box',
    };

    return (
        <>
            <div className="fm-grid" style={gridStyle}>
                {mediaSources.slice(0, 4).map((src, i) => {
                    const kind = mediaKinds?.[i] || 'image';
                    const isVideo = kind === 'video';
                    const thumb = thumbnailSources[i] || (isVideo ? undefined : src);

                    // ── Cell: ALL layout inline ──
                    const cellStyle: React.CSSProperties = {
                        position: 'relative',
                        overflow: 'hidden',
                        width: '100%',
                        aspectRatio: cellAspectRatio(count, isVideo),
                        minHeight: isSingle && !isVideo ? 200 : undefined,
                        maxHeight: isSingle ? 600 : undefined,
                    };

                    // items-3: first cell spans two rows
                    if (count === 3 && i === 0) {
                        (cellStyle as Record<string, unknown>).gridRow = '1 / 3';
                    }

                    return (
                        <div
                            key={i}
                            className="fm-cell"
                            style={cellStyle}
                            onClick={(e) => { e.stopPropagation(); openGallery(i); }}
                        >
                            {isVideo ? (
                                <div
                                    className="fm-video-wrapper"
                                    style={{
                                        position: 'absolute' as const,
                                        top: 0,
                                        left: 0,
                                        width: '100%',
                                        height: '100%',
                                    }}
                                    onClick={(e) => e.stopPropagation()}
                                >
                                    <VideoPlayer
                                        src={src}
                                        poster={thumb}
                                        style={{
                                            position: 'absolute' as const,
                                            top: 0,
                                            left: 0,
                                            width: '100%',
                                            height: '100%',
                                        }}
                                    />
                                </div>
                            ) : (
                                <FeedMediaImage src={src} mediaDir={mediaDir} />
                            )}

                            {/* Expand button */}
                            <button
                                className="fm-expand"
                                style={{
                                    position: 'absolute' as const,
                                    bottom: 4,
                                    right: 4,
                                    zIndex: 2,
                                    width: 22,
                                    height: 22,
                                    display: 'flex',
                                    alignItems: 'center',
                                    justifyContent: 'center',
                                }}
                                onClick={(e) => { e.stopPropagation(); openGallery(i); }}
                                title="Open fullscreen"
                            >
                                <ExpandIcon />
                            </button>
                        </div>
                    );
                })}
            </div>

            {/* Gallery overlay — portal to document.body */}
            {galleryOpen && (
                <GalleryOverlay
                    mediaSources={mediaSources}
                    thumbnailSources={thumbnailSources}
                    mediaKinds={mediaKinds}
                    index={galleryIndex}
                    setIndex={setGalleryIndex}
                    onClose={() => setGalleryOpen(false)}
                />
            )}
        </>
    );
}
