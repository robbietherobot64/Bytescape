import { useMemo } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { useLazyBlobUrl } from '../hooks/useBlobUrl';

interface BlobImageProps {
    src: string | undefined;
    alt?: string;
    mediaDir?: string;
    className?: string;
    style?: React.CSSProperties;
    loading?: 'lazy' | 'eager';
    onError?: () => void;
}

/**
 * Image component that handles both blob:hash URLs and filesystem paths.
 * Uses IntersectionObserver to only fetch blob data when near the viewport,
 * preventing memory blowout from loading hundreds of images at once.
 *
 * URL resolution is synchronous (useMemo) so cached blob URLs render on
 * the very first frame — no dark placeholder flash.
 */
export function BlobImage({ src, alt = '', mediaDir, className, style, loading = 'lazy', onError }: BlobImageProps) {
    const [containerRef, blobUrl, , error] = useLazyBlobUrl(src?.startsWith('blob:') ? src : undefined);

    // Synchronous URL resolution — eliminates the useState+useEffect gap
    // that caused a 1-2 frame dark placeholder flash on every mount.
    const resolvedUrl = useMemo(() => {
        if (!src) return null;

        // Blob URLs are resolved by the hook
        if (src.startsWith('blob:')) return blobUrl;

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
    }, [src, blobUrl, mediaDir]);

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
            decoding="auto"
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
