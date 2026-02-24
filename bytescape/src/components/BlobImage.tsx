import { useState, useEffect } from 'react';
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
 */
export function BlobImage({ src, alt = '', mediaDir, className, style, loading = 'lazy', onError }: BlobImageProps) {
    const [resolvedUrl, setResolvedUrl] = useState<string | null>(null);
    const [containerRef, blobUrl, isLoading, error] = useLazyBlobUrl(src?.startsWith('blob:') ? src : undefined);

    useEffect(() => {
        if (!src) {
            setResolvedUrl(null);
            return;
        }

        // Blob URLs are handled by the hook
        if (src.startsWith('blob:')) {
            setResolvedUrl(blobUrl);
            return;
        }

        // HTTP URLs pass through directly
        if (src.startsWith('http://') || src.startsWith('https://')) {
            setResolvedUrl(src);
            return;
        }

        // Filesystem paths need conversion
        if (mediaDir) {
            // Handle relative paths like "media/filename.jpg"
            if (src.startsWith('media/')) {
                const filename = src.replace(/^media\//, '');
                setResolvedUrl(convertFileSrc(`${mediaDir}/${filename}`));
            } else if (src.startsWith('/') || src.match(/^[A-Za-z]:\\/)) {
                // Absolute paths
                setResolvedUrl(convertFileSrc(src));
            } else {
                // Default: assume relative to mediaDir
                setResolvedUrl(convertFileSrc(`${mediaDir}/${src}`));
            }
        } else {
            // No mediaDir - try to convert directly
            setResolvedUrl(convertFileSrc(src));
        }
    }, [src, blobUrl, mediaDir]);

    if (isLoading) {
        return (
            <div 
                ref={containerRef}
                className={className} 
                style={{ 
                    ...style, 
                    display: 'flex', 
                    alignItems: 'center', 
                    justifyContent: 'center',
                    backgroundColor: 'var(--bg-secondary, #1a1a1a)',
                    minHeight: 100 
                }}
            >
                <span style={{ color: 'var(--text-muted, #666)' }}>Loading...</span>
            </div>
        );
    }

    if (error || !resolvedUrl) {
        return (
            <div 
                ref={containerRef}
                className={className} 
                style={{ 
                    ...style, 
                    display: 'flex', 
                    alignItems: 'center', 
                    justifyContent: 'center',
                    backgroundColor: 'var(--bg-secondary, #1a1a1a)',
                    minHeight: 100 
                }}
            >
                <span style={{ color: 'var(--text-muted, #666)' }}>
                    {error ? '⚠️ Failed to load' : ''}
                </span>
            </div>
        );
    }

    return (
        <img 
            ref={containerRef as React.RefCallback<HTMLImageElement>}
            src={resolvedUrl} 
            alt={alt} 
            className={className} 
            style={style} 
            loading={loading}
            onError={() => {
                console.error('Image failed to load:', src);
                onError?.();
            }}
        />
    );
}

export default BlobImage;
