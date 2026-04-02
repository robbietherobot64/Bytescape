import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { blobCacheUrl } from '../utils/mediaServer';

/**
 * useBlobUrl — Resolve media references (especially `blob:` hashes) into
 * browser-consumable URLs for rendering images/video in the Lens.
 *
 * PURPOSE
 * - Convert app-level media paths into displayable URLs.
 * - Resolve `blob:<hash>` entries through the backend cache pipeline.
 * - Reuse resolved URLs to avoid redundant IPC and reduce render jitter.
 *
 * HOW IT WORKS
 * - For non-blob paths, resolve synchronously (`convertFileSrc` fallback).
 * - For blob hashes, invoke the Tauri command `get_media_blob` to materialize
 *   a cached file path, then map it to an HTTP/media-server URL.
 * - Cache resolved hash -> URL mappings in memory for fast repeat access.
 * - `useLazyBlobUrl()` gates the expensive path behind viewport visibility.
 *
 * CROSS-SYSTEM NOTES
 * - Tauri invoke path: `invoke('get_media_blob', { hashStr })` from this hook
 *   crosses Lens -> Engine, where the command is registered in
 *   `src-tauri/src/lib.rs` and returns a filesystem path for the blob.
 * - HTTP URL conversion uses `blobCacheUrl()` to avoid known asset:// issues
 *   in some WebKitGTK/runtime combinations.
 *
 * TECH DEBT
 * - `resolvedUrlCache` is a process-global, unbounded `Map` with no eviction.
 *   Long sessions can accumulate entries indefinitely and increase memory use.
 *
 * RELATED FILES
 * - `src/utils/mediaServer.ts` (file-path -> local HTTP URL conversion)
 * - `src-tauri/src/lib.rs` (Tauri command registration)
 * - `src-tauri/src/db.rs` / media blob storage pipeline
 */

/**
 * In-memory cache of hash → asset URL to avoid re-invoking for already-resolved blobs.
 * The files on disk are also cached (the Rust side writes once to blob_cache/),
 * but this avoids the IPC round-trip entirely for repeat renders.
 *
 * Exported so that `prefetchBlobUrls()` can pre-populate it in bulk after feed loads.
 */
export const resolvedUrlCache = new Map<string, string>();

// ═══ Path Resolution Helpers ═══════════════════════════════════════════════

/**
 * Hook to resolve a media path that may be a blob hash.
 * Returns [url, isLoading, error]
 *
 * For blob:hash paths, asks the backend to export to a cache file,
 * then uses convertFileSrc() to get an asset:// URL. Zero bytes cross IPC.
 * For other paths, returns them directly.
 */
/**
 * Extract the blob hash from a "blob:..." path, handling nested prefixes.
 * Returns null if the path is a browser Object URL (blob:http://...) or invalid.
 */
function extractBlobHash(path: string): string | null {
    const afterPrefix = path.slice(5);
    if (afterPrefix.startsWith('http://') || afterPrefix.startsWith('https://') || afterPrefix.startsWith('tauri://')) {
        return null; // browser Object URL — not a hash
    }
    let hash = afterPrefix.trim();
    while (hash.startsWith('blob:')) {
        hash = hash.slice(5).trim();
    }
    return hash || null;
}

/**
 * Resolve a non-blob path synchronously (HTTP, data:, asset:, filesystem).
 */
function resolveNonBlobSync(path: string): string {
    const lower = path.toLowerCase();
    if (lower.startsWith('http://') || lower.startsWith('https://') || lower.startsWith('data:') || lower.startsWith('asset:') || lower.startsWith('tauri:')) {
        return path;
    }
    try {
        return convertFileSrc(path);
    } catch {
        return path;
    }
}

/**
 * Resolve a possibly-blob media path into a display URL.
 *
 * @param path Input media path (`blob:<hash>`, filesystem path, or URL).
 * @returns Tuple of `[resolvedUrl, isLoading, error]`.
 */
export function useBlobUrl(path: string | undefined): [string | null, boolean, string | null] {
    // Synchronous cache initialization — prevents the "null → effect → URL" render gap
    // that causes images to flash a dark placeholder for 1-2 frames on mount.
    const [url, setUrl] = useState<string | null>(() => {
        if (!path) return null;
        if (path.startsWith('blob:')) {
            const hash = extractBlobHash(path);
            if (hash === null) return path; // browser Object URL — pass through
            return resolvedUrlCache.get(hash) ?? null;
        }
        return resolveNonBlobSync(path);
    });
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        if (!path) {
            setUrl(null);
            return;
        }

        // Check if it's a blob reference
        if (path.startsWith('blob:')) {
            const hash = extractBlobHash(path);

            // Browser Object URLs pass through directly
            if (hash === null) {
                setUrl(path);
                setIsLoading(false);
                setError(null);
                return;
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
            // Not a blob — resolve synchronously
            try {
                setUrl(resolveNonBlobSync(path));
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

// ═══ Lazy Viewport-Aware Resolution ════════════════════════════════════════

/**
 * Lazily resolve media only when a target element is near/inside viewport.
 *
 * @param path Input media path to resolve once visible.
 * @returns `[refCallback, resolvedUrl, isLoading, error]`.
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

    // Bypass visibility gate for already-cached blob URLs.
    // This prevents the dark placeholder flash: if prefetchBlobUrls() has already
    // resolved this blob, we can return the URL synchronously without waiting for
    // the IntersectionObserver to fire.
    const isCached = path ? (() => {
        if (!path.startsWith('blob:')) return false;
        const hash = extractBlobHash(path);
        return hash !== null && resolvedUrlCache.has(hash);
    })() : false;

    const activePath = (isVisible || isCached) ? path : undefined;
    const [url, isLoading, error] = useBlobUrl(activePath);

    return [setRef, url, isLoading, error];
}

export default useBlobUrl;
