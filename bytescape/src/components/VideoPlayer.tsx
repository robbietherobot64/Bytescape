import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useLazyBlobUrl } from '../hooks/useBlobUrl';

interface VideoPlayerProps {
    src: string;
    poster?: string;
    mime?: string;
    className?: string;
    style?: React.CSSProperties;
    autoPlay?: boolean;
}

const videoUrlCache = new Map<string, string>();
let globalMediaPort: number | null = null;

async function getMediaPort(): Promise<number> {
    if (globalMediaPort) return globalMediaPort;
    globalMediaPort = await invoke<number>('get_media_server_port');
    return globalMediaPort;
}

/**
 * Standard, native VideoPlayer component for Bytescape.
 * Uses a lightweight, dedicated `warp` file server to bypass WebKitGTK's strict drop-dead media restrictions.
 */
export function VideoPlayer({ src, poster, className, style, autoPlay = false }: VideoPlayerProps) {
    const [resolvedUrl, setResolvedUrl] = useState<string | null>(null);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const containerRef = useRef<HTMLDivElement>(null);
    const [isVisible, setIsVisible] = useState(false);

    // Smoothly resolve the poster if it's a blob:hash
    const [, resolvedPoster] = useLazyBlobUrl(poster);

    // Intersection observer for lazy loading metadata bounds
    useEffect(() => {
        const observer = new IntersectionObserver(
            ([entry]) => {
                if (entry.isIntersecting) {
                    setIsVisible(true);
                    observer.disconnect();
                }
            },
            { rootMargin: '200px' }
        );

        if (containerRef.current) {
            observer.observe(containerRef.current);
        }

        return () => observer.disconnect();
    }, []);

    useEffect(() => {
        if (!src || !isVisible) return;

        let cancelled = false;

        async function loadVideo() {
            if (videoUrlCache.has(src)) {
                setResolvedUrl(videoUrlCache.get(src)!);
                return;
            }

            setIsLoading(true);
            setError(null);

            try {
                // 1. Handle HLS Bluesky Videos (which are m3u8 playlists)
                if (src.includes('.m3u8')) {
                    console.log(`[VideoPlayer] Resolving native file for HLS: ${src}`);
                    // Backend efficiently downloads the segments and creates a single local .ts file
                    const filePath = await invoke<string>('resolve_hls_video', { playlistUrl: src });
                    if (!cancelled && filePath) {
                        const filename = filePath.split(/[/\\]/).pop();
                        if (filename) {
                            const port = await getMediaPort();
                            const assetUrl = `http://127.0.0.1:${port}/${filename}`;
                            videoUrlCache.set(src, assetUrl);
                            setResolvedUrl(assetUrl);
                        }
                    }
                }
                // 2. Handle Vault Encrypted Blobs
                else if (src.startsWith('blob:') && !src.startsWith('blob:http') && !src.startsWith('blob:tauri')) {
                    const hash = src.replace(/^blob:/, '').trim();
                    console.log(`[VideoPlayer] Decoding Blob to native file path: ${hash}`);
                    // Backend streams out the decrypted file to a cache and returns the path
                    const filePath = await invoke<string>('get_media_blob', { hashStr: hash });
                    if (!cancelled && filePath) {
                        const filename = filePath.split(/[/\\]/).pop();
                        if (filename) {
                            const port = await getMediaPort();
                            const assetUrl = `http://127.0.0.1:${port}/${filename}`;
                            videoUrlCache.set(src, assetUrl);
                            setResolvedUrl(assetUrl);
                        }
                    }
                }
                // 3. Handle direct ready-to-play URLs (HTTP, Object URL, standard Tauri Asset URL)
                else {
                    setResolvedUrl(src);
                }
            } catch (err) {
                console.error("[VideoPlayer] native architecture load failed:", err);
                if (!cancelled) {
                    setError('Failed to load video');
                }
            } finally {
                if (!cancelled) {
                    setIsLoading(false);
                }
            }
        }

        loadVideo();

        return () => { cancelled = true; };
    }, [src, isVisible]);

    return (
        <div ref={containerRef} className={className} style={{ ...style, position: 'relative', backgroundColor: '#000', overflow: 'hidden', minHeight: 120 }}>
            {isLoading && !resolvedUrl && (
                <div style={{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                    <span style={{ color: '#aaa', fontSize: '0.86rem' }}>⏳ Loading native media...</span>
                </div>
            )}

            {error && (
                <div style={{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 20, textAlign: 'center' }}>
                    <span style={{ color: '#fca5a5', fontSize: '0.86rem' }}>⚠️ {error}</span>
                </div>
            )}

            {/* 
               Only render the actual video tag once we have a fully resolved, local asset URI.
               This prevents WebKitGTK from aggressively locking up on unready streams.
            */}
            {resolvedUrl && (
                <video
                    src={resolvedUrl}
                    poster={resolvedPoster || undefined}
                    controls
                    autoPlay={autoPlay}
                    loop
                    playsInline
                    preload="metadata"
                    style={{ width: '100%', height: '100%', objectFit: 'contain', display: 'block' }}
                    // Handle edge-case WebKit decoding errors gracefully
                    onError={(e) => {
                        console.error("[VideoPlayer] Video tag intrinsic WebKit error:", e.currentTarget.error);
                    }}
                />
            )}
        </div>
    );
}
