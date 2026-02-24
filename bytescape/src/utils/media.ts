import { invoke } from '@tauri-apps/api/core';
import { convertFileSrc } from '@tauri-apps/api/core';

/**
 * Resolve a media path to a displayable URL.
 * Handles both filesystem paths and Iroh blob hashes.
 * 
 * @param path - Either a filesystem path (e.g., "media/image.jpg") or blob hash
 * @param appDataDir - The app data directory for filesystem paths
 * @returns Promise<string> - A URL that can be used in <img src>
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

/**
 * Resolve multiple media paths in parallel.
 * Useful for Gallery items with multiple images.
 */
export async function resolveMediaPaths(paths: string[], appDataDir: string): Promise<string[]> {
    return Promise.all(paths.map(p => resolveMediaPath(p, appDataDir)));
}


