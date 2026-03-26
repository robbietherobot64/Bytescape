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

    // Always render a stable container div for the IntersectionObserver ref.
    // The <img> mounts inside it when the URL is available. This avoids DOM
    // element type switching (div→img→div) which causes WebKitGTK repaints.
    if (resolvedUrl && !error) {
        return (
            <img 
                ref={containerRef as React.RefCallback<HTMLImageElement>}
                src={resolvedUrl} 
                alt={alt} 
                className={className} 
                style={style} 
                loading={loading || 'lazy'}
                decoding="auto"
                onError={() => {
                    console.error('Image failed to load:', src);
                    onError?.();
                }}
            />
        );
    }

    // Loading or error state — render a placeholder div with the observer ref.
    // Use transparent background instead of dark to prevent "black flash".
    return (
        <div 
            ref={containerRef}
            className={className} 
            style={{ 
                ...style, 
                display: 'flex', 
                alignItems: 'center', 
                justifyContent: 'center',
                width: '100%',
                height: '100%',
            }}
        >
            {error && (
                <span style={{ color: 'var(--text-muted, #666)' }}>
                    ⚠️ Failed to load
                </span>
            )}
        </div>
    );
}

export default BlobImage;
