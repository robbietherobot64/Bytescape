import { invoke } from '@tauri-apps/api/core';
import { convertFileSrc } from '@tauri-apps/api/core';

/**
 * PURPOSE:
 * Resolve mixed media identifiers (blob refs, local paths, external URLs) into
 * browser-displayable `src` values.
 *
 * HOW IT WORKS:
 * - Blob refs (`blob:<hash>`) are resolved via backend IPC then converted to a
 *   Tauri-safe file URL.
 * - Local/absolute paths are normalized through `convertFileSrc`.
 * - HTTP(S) URLs are passed through unchanged.
 *
 * RELATED FILES:
 * - src/hooks/useBlobUrl.ts (hook-level blob resolution + cache behavior)
 * - src/utils/prefetchBlobUrls.ts (batch pre-resolution path)
 * - src-tauri/src/lib.rs (Tauri command registration, including `get_media_blob`)
 *
 * CAVEAT (duplicated media-resolution behavior):
 * Some blob/path resolution responsibilities overlap with hook-based flows
 * (`useBlobUrl` + prefetch). Keep behavior aligned to avoid drift.
 */

// -----------------------------------------------------------------------------
// Single Path Resolution
// -----------------------------------------------------------------------------

/**
 * Resolve a media path to a displayable URL.
 * Handles both filesystem paths and Iroh blob hashes.
 * 
 * @param path - Either a filesystem path (e.g., "media/image.jpg") or blob hash
 * @param appDataDir - The app data directory for filesystem paths
 * @returns Promise<string> - A URL that can be used in <img src>
 *
 * Cross-system note:
 * - IPC command: `get_media_blob` (Rust backend), used for blob hash materialization.
 * - Hook/store interaction: this function is utility-level and does not write
 *   shared UI cache/store state directly.
 */
export async function resolveMediaPath(path: string, appDataDir: string): Promise<string> {
    // Check if it's a blob reference
    if (path.startsWith('blob:')) {
        const hash = path.slice(5); // Remove "blob:" prefix
        try {
            // Backend writes blob to disk cache and returns the file path
            const filePath = await invoke<string>('get_media_blob', { hashStr: hash });
            return convertFileSrc(filePath);
        } catch (e) {
            console.error('Failed to fetch blob:', hash, e);
            return ''; // Return empty on error
        }
    }
    
    // Otherwise, it's a filesystem path
    // Handle relative paths (e.g., "media/filename.jpg")
    if (path.startsWith('media/')) {
        const fullPath = `${appDataDir}/${path}`;
        return convertFileSrc(fullPath);
    }
    
    // Absolute paths
    if (path.startsWith('/') || path.match(/^[A-Za-z]:\\/)) {
        return convertFileSrc(path);
    }
    
    // URLs (video URLs, external images)
    if (path.startsWith('http://') || path.startsWith('https://')) {
        return path;
    }
    
    // Default: assume it's a relative path
    return convertFileSrc(`${appDataDir}/${path}`);
}

// -----------------------------------------------------------------------------
// Batch Path Resolution
// -----------------------------------------------------------------------------

/**
 * Exported batch helper that resolves multiple media paths in parallel.
 *
 * Useful for gallery-like surfaces with multiple media sources.
 */
export async function resolveMediaPaths(paths: string[], appDataDir: string): Promise<string[]> {
    return Promise.all(paths.map(p => resolveMediaPath(p, appDataDir)));
}


