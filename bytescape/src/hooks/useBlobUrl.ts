import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { blobCacheUrl } from '../utils/mediaServer';

/**
 * In-memory cache of hash → asset URL to avoid re-invoking for already-resolved blobs.
 * The files on disk are also cached (the Rust side writes once to blob_cache/),
 * but this avoids the IPC round-trip entirely for repeat renders.
 */
const resolvedUrlCache = new Map<string, string>();

/**
 * Hook to resolve a media path that may be a blob hash.
 * Returns [url, isLoading, error]
 *
 * For blob:hash paths, asks the backend to export to a cache file,
 * then uses convertFileSrc() to get an asset:// URL. Zero bytes cross IPC.
 * For other paths, returns them directly.
 */
export function useBlobUrl(path: string | undefined): [string | null, boolean, string | null] {
    const [url, setUrl] = useState<string | null>(null);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        if (!path) {
            setUrl(null);
            return;
        }

        // Check if it's a blob reference
        if (path.startsWith('blob:')) {
            // Browser Object URLs look like blob:http://... or blob:tauri://...
            // These are already resolved — pass them through directly.
            const afterPrefix = path.slice(5);
            if (afterPrefix.startsWith('http://') || afterPrefix.startsWith('https://') || afterPrefix.startsWith('tauri://')) {
                setUrl(path);
                setIsLoading(false);
                setError(null);
                return;
            }

            let hash = afterPrefix.trim(); // Remove "blob:" prefix
            while (hash.startsWith('blob:')) {
                hash = hash.slice(5).trim();
            }

            if (!hash) {
                setError('Invalid blob hash');
                setUrl(null);
                setIsLoading(false);
                return;
            }

            // Check in-memory cache first
            const cached = resolvedUrlCache.get(hash);
            if (cached) {
                setUrl(cached);
                setIsLoading(false);
                setError(null);
                return;
            }

            setIsLoading(true);
            setError(null);

            let cancelled = false;

            // Backend writes blob to disk cache and returns the file path
            invoke<string>('get_media_blob', { hashStr: hash })
                .then(async (filePath) => {
                    if (cancelled) return;
                    // Use local HTTP server to bypass WebKitGTK asset:// bugs
                    const httpUrl = await blobCacheUrl(filePath);
                    resolvedUrlCache.set(hash, httpUrl);
                    setUrl(httpUrl);
                })
                .catch((e) => {
                    if (cancelled) return;
                    console.error('Failed to resolve blob:', hash, e);
                    setError(String(e));
                    setUrl(null);
                })
                .finally(() => {
                    if (!cancelled) setIsLoading(false);
                });

            return () => {
                cancelled = true;
            };
        } else {
            // Not a blob. Convert local filesystem paths so media can load in Tauri WebView.
            const lower = path.toLowerCase();
            const isWebUrl = lower.startsWith('http://') || lower.startsWith('https://');
            const isDataUrl = lower.startsWith('data:');
            const isAssetUrl = lower.startsWith('asset:') || lower.startsWith('tauri:');

            try {
                setUrl(isWebUrl || isDataUrl || isAssetUrl ? path : convertFileSrc(path));
                setError(null);
            } catch (e) {
                setUrl(path);
                setError(String(e));
            }

            setIsLoading(false);
        }
    }, [path]);

    return [url, isLoading, error];
}

/**
 * Hook that only activates useBlobUrl when an element is in the viewport.
 * Attaches an IntersectionObserver to the returned ref.
 * Returns [ref, url, isLoading, error]
 */
export function useLazyBlobUrl(path: string | undefined): [
    React.RefCallback<HTMLElement>,
    string | null,
    boolean,
    string | null
] {
    const [isVisible, setIsVisible] = useState(false);
    const observerRef = useRef<IntersectionObserver | null>(null);
    const elementRef = useRef<HTMLElement | null>(null);

    const setRef: React.RefCallback<HTMLElement> = useCallback((node: HTMLElement | null) => {
        // Disconnect previous observer
        if (observerRef.current) {
            observerRef.current.disconnect();
            observerRef.current = null;
        }

        if (node) {
            elementRef.current = node;
            observerRef.current = new IntersectionObserver(
                (entries) => {
                    if (entries[0]?.isIntersecting) {
                        setIsVisible(true);
                        // Once visible, stop observing — no need to refetch
                        observerRef.current?.disconnect();
                        observerRef.current = null;
                    }
                },
                { rootMargin: '200px' } // Start loading 200px before entering viewport
            );
            observerRef.current.observe(node);
        } else {
            elementRef.current = null;
        }
    }, []);

    // Cleanup observer on unmount
    useEffect(() => {
        return () => {
            observerRef.current?.disconnect();
        };
    }, []);

    // Only fetch blob when visible
    const activePath = isVisible ? path : undefined;
    const [url, isLoading, error] = useBlobUrl(activePath);

    return [setRef, url, isLoading, error];
}

export default useBlobUrl;
