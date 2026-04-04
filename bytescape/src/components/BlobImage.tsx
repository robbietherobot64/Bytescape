import { useMemo, useCallback } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { useLazyBlobUrl } from '../hooks/useBlobUrl';

/**
 * Props for `BlobImage`.
 *
 * Cross-system note:
 * - `blob:` identifiers are resolved through frontend hooks that call Tauri
 *   commands backed by Rust media/blob storage.
 * - Filesystem paths are converted with `convertFileSrc` for webview-safe URL
 *   loading in the Tauri runtime.
 */
interface BlobImageProps {
    src: string | undefined;
    alt?: string;
    mediaDir?: string;
    className?: string;
    style?: React.CSSProperties;
    loading?: 'lazy' | 'eager';
    onError?: () => void;
    /** Called when the image finishes loading — used by virtualizer to re-measure row heights */
    onLoad?: () => void;
    /** When true, blob: URLs are resolved through get_media_blob_display
     *  (1200px max-dimension WebP) instead of the full-resolution original.
     *  Reduces WebKitGTK decoded bitmap memory from ~48 MB to ~5 MB per image.
     *  Use for feed cards and grids; omit for gallery/lightbox full-size views. */
    displayVariant?: boolean;
}

/**
 * Image component that handles both blob:hash URLs and filesystem paths.
 * Uses IntersectionObserver to only fetch blob data when near the viewport,
 * preventing memory blowout from loading hundreds of images at once.
 *
 * URL resolution is synchronous (useMemo) so cached blob URLs render on
 * the very first frame — no dark placeholder flash.
 *
 * Exported symbol used by feed cards, galleries, and article/media surfaces.
 */
export function BlobImage({ src, alt = '', mediaDir, className, style, loading = 'lazy', onError, onLoad, displayVariant }: BlobImageProps) {
     // ===== Section: source normalization =====
    // When displayVariant is requested and src is a blob: hash, transform to
    // blob-display: prefix so useBlobUrl invokes the display-size command.
    const effectiveSrc = useMemo(() => {
        if (displayVariant && src?.startsWith('blob:')) {
            // Only transform if it's a real blob hash (not a browser Object URL)
            const afterPrefix = src.slice(5);
            if (!afterPrefix.startsWith('http://') && !afterPrefix.startsWith('https://') && !afterPrefix.startsWith('tauri://')) {
                return `blob-display:${afterPrefix}`;
            }
        }
        return src;
    }, [src, displayVariant]);

    // ===== Section: lazy blob URL resolution =====
    const [containerRef, blobUrl, , error] = useLazyBlobUrl(effectiveSrc?.startsWith('blob:') || effectiveSrc?.startsWith('blob-display:') ? effectiveSrc : undefined);

    // ===== Section: final URL resolution =====
    // Synchronous URL resolution — eliminates the useState+useEffect gap
    // that caused a 1-2 frame dark placeholder flash on every mount.
    const resolvedUrl = useMemo(() => {
        if (!src) return null;

        // Blob URLs (both regular and display variant) are resolved by the hook
        if (effectiveSrc?.startsWith('blob:') || effectiveSrc?.startsWith('blob-display:')) return blobUrl;

        // HTTP URLs pass through directly
        if (src.startsWith('http://') || src.startsWith('https://')) return src;

        // Filesystem paths need conversion
        if (mediaDir) {
            if (src.startsWith('media/')) {
                const filename = src.replace(/^media\//, '');
                return convertFileSrc(`${mediaDir}/${filename}`);
            } else if (src.startsWith('/') || src.match(/^[A-Za-z]:\\/)) {
                return convertFileSrc(src);
            } else {
                return convertFileSrc(`${mediaDir}/${src}`);
            }
        }
        return convertFileSrc(src);
    }, [src, effectiveSrc, blobUrl, mediaDir]);

    // ===== Section: image load handler =====
    // When the image loads, notify parent (and the virtualizer) that dimensions
    // may have changed — this lets the virtualizer re-measure without clearing
    // the entire height cache.
    const handleLoad = useCallback(() => {
        onLoad?.();
    }, [onLoad]);

    // ===== Section: stable image element strategy =====
    // Always render <img> to avoid DOM element type switching (img↔div) that
    // causes WebKitGTK repaints and visible flicker on virtualizer remount.
    // Use a transparent 1x1 GIF as placeholder when loading/errored.
    const TRANSPARENT_PIXEL = 'data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7';
    const imgSrc = (resolvedUrl && !error) ? resolvedUrl : TRANSPARENT_PIXEL;

    return (
        <img
            ref={containerRef as React.RefCallback<HTMLImageElement>}
            src={imgSrc}
            alt={error ? '⚠️ Failed to load' : alt}
            className={className}
            style={style}
            loading={loading || 'lazy'}
            decoding="async"
            onLoad={handleLoad}
            onError={() => {
                if (resolvedUrl) {
                    console.error('Image failed to load:', src);
                    onError?.();
                }
            }}
        />
    );
}

export default BlobImage;
