import { useEffect, useState, useRef, VideoHTMLAttributes } from 'react';
import { invoke } from '@tauri-apps/api/core';

// Global cache for resolved blob URLs so we don't re-fetch/re-create across rerenders
const videoBlobCache = new Map<string, string>();


/**
 * Returns true when the src looks like a vault/Iroh blob hash
 * (e.g. "blob:B2WF6..." or "blob:blob:B2WF6..."),
 * but NOT a browser-created Object URL ("blob:http://...", "blob:tauri://...").
 */
function isBlobHashSrc(src: string): boolean {
    if (!src) return false;
    if (!src.startsWith('blob:')) return false;
    const rest = src.slice(5);
    return !rest.startsWith('http://') && !rest.startsWith('https://') && !rest.startsWith('tauri://');
}

/**
 * Extract the raw hash from a blob src, stripping nested "blob:" prefixes.
 */
function extractHash(src: string): string {
    let hash = src.slice(5).trim();
    while (hash.startsWith('blob:')) hash = hash.slice(5).trim();
    // Remove query params if any
    return hash.split('?')[0];
}

/** Detect video MIME type from magic bytes */
function detectVideoMime(bytes: Uint8Array): string {
    if (bytes.length < 12) return 'video/mp4';
    
    // MP4/MOV: "ftyp" at offset 4
    if (bytes[4] === 0x66 && bytes[5] === 0x74 && bytes[6] === 0x79 && bytes[7] === 0x70) {
        return 'video/mp4';
    }
    
    // WebM: starts with 0x1A45DFA3
    if (bytes[0] === 0x1A && bytes[1] === 0x45 && bytes[2] === 0xDF && bytes[3] === 0xA3) {
        return 'video/webm';
    }
    
    // AVI: "RIFF" ... "AVI "
    if (bytes[0] === 0x52 && bytes[1] === 0x49 && bytes[2] === 0x46 && bytes[3] === 0x46) {
        return 'video/avi';
    }
    
    // Ogg container
    if (bytes[0] === 0x4F && bytes[1] === 0x67 && bytes[2] === 0x67 && bytes[3] === 0x53) {
        return 'video/ogg';
    }
    
    return 'video/mp4'; // Default fallback
}

/**
 * Custom hook to resolve Iroh blob hashes into playable ObjectURLs.
 * Handles caching and cleanup.
 */
function useVideoSrc(src: string, mime?: string): { url: string; isLoading: boolean; error: string | null } {
    const [state, setState] = useState({
        url: isBlobHashSrc(src) ? '' : src,
        isLoading: isBlobHashSrc(src),
        error: null as string | null
    });
    
    const isMounted = useRef(true);

    useEffect(() => {
        isMounted.current = true;
        
        // Reset state if src changes
        if (!isBlobHashSrc(src)) {
            setState({ url: src, isLoading: false, error: null });
            return;
        }

        const hash = extractHash(src);
        if (!hash) {
            setState({ url: '', isLoading: false, error: 'Invalid hash' });
            return;
        }
        
        // 1. Check Cache
        if (videoBlobCache.has(hash)) {
            setState({ url: videoBlobCache.get(hash)!, isLoading: false, error: null });
            return;
        }

        setState(prev => ({ ...prev, isLoading: true, error: null }));

        // 2. Fetch Data
        let cancelled = false;
        
        // Use camelCase for Tauri argument mapping (hash_str -> hashStr)
        invoke<number[]>('get_media_blob_bytes', { hashStr: hash })
            .then((data) => {
                if (cancelled || !isMounted.current) return;
                
                try {
                    // Convert JSON array (number[]) to Uint8Array
                    let arr: Uint8Array;
                    if (data instanceof Uint8Array) {
                        arr = data;
                    } else if (Array.isArray(data)) {
                        arr = new Uint8Array(data);
                    } else {
                        throw new Error(`Unexpected data type: ${typeof data}`);
                    }
                    
                    if (arr.length === 0) {
                        throw new Error("Received 0 bytes");
                    }

                    // Resolve MIME
                    let resolvedMime = mime;
                    if (!resolvedMime || 
                        resolvedMime === 'application/octet-stream' || 
                        resolvedMime === 'binary/octet-stream' ||
                        // Sometimes the backend guesses wrong for unknown exts
                        resolvedMime === 'application/x-binary') {
                        resolvedMime = detectVideoMime(arr);
                    }

                    // Create Blob URL
                    const blob = new Blob([arr], { type: resolvedMime });
                    const url = URL.createObjectURL(blob);
                    
                    // Cache it
                    videoBlobCache.set(hash, url);
                    
                    setState({ url, isLoading: false, error: null });
                    console.log(`[VideoPlayer] Resolved ${hash} (${arr.length} bytes) as ${resolvedMime}`);
                } catch (err: any) {
                    console.error('[VideoPlayer] Error processing bytes:', err);
                    setState({ url: '', isLoading: false, error: err.message });
                }
            })
            .catch((err) => {
                if (cancelled || !isMounted.current) return;
                console.error('[VideoPlayer] IPV error:', err);
                setState({ url: '', isLoading: false, error: String(err) });
            });

        return () => {
            cancelled = true;
            isMounted.current = false;
            // Note: We don't revoke URL here to keep the cache valid.
            // A more sophisticated cache would handle LRU eviction and revocation.
        };
    }, [src, mime]);

    return state;
}

interface VideoPlayerProps extends VideoHTMLAttributes<HTMLVideoElement> {
    src: string;
    mime?: string;
}

export function VideoPlayer({ src, mime, className, style, poster, ...props }: VideoPlayerProps) {
    const { url, isLoading, error } = useVideoSrc(src, mime);
    
    // If it's loading, show a placeholder or loader
    if (isLoading) {
        return (
            <div 
                className={`video-player-loading ${className || ''}`} 
                style={{ 
                    ...style, 
                    display: 'flex', 
                    alignItems: 'center', 
                    justifyContent: 'center',
                    background: '#000',
                    color: '#666',
                    aspectRatio: '16/9',
                    minHeight: '200px'
                }}
            >
                <span>Loading video...</span>
            </div>
        );
    }

    if (error) {
         return (
            <div 
                className={`video-player-error ${className || ''}`} 
                style={{ 
                    ...style, 
                    display: 'flex', 
                    alignItems: 'center', 
                    justifyContent: 'center',
                    background: '#220000',
                    color: '#ff6b6b'
                }}
            >
                <span>Failed to load video</span>
            </div>
        );       
    }

    return (
        <video
            controls
            playsInline
            src={url}
            poster={poster}
            className={className}
            style={style}
            preload="metadata"
            {...props}
        />
    );
}
