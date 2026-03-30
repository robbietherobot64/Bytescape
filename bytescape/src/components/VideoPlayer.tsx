import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useLazyBlobUrl } from '../hooks/useBlobUrl';
import { getMediaPort } from '../utils/mediaServer';
import { VideoPlaybackManager } from '../utils/VideoPlaybackManager';

interface VideoPlayerProps {
    src: string;
    poster?: string;
    mime?: string;
    className?: string;
    style?: React.CSSProperties;
    autoPlay?: boolean;
}

const videoUrlCache = new Map<string, string>();

/**
 * Standard, native VideoPlayer component for Bytescape.
 * Uses a lightweight, dedicated `warp` file server to bypass WebKitGTK's strict drop-dead media restrictions.
 */
export function VideoPlayer({ src, poster, className, style, autoPlay = false }: VideoPlayerProps) {
    const [resolvedUrl, setResolvedUrl] = useState<string | null>(null);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const containerRef = useRef<HTMLDivElement>(null);
    const videoRef = useRef<HTMLVideoElement>(null);
    const [isVisible, setIsVisible] = useState(false);

    // Smoothly resolve the poster if it's a blob:hash
    const [, resolvedPoster] = useLazyBlobUrl(poster);

    // Register/unregister with global playback manager
    const videoRefCallback = useCallback((el: HTMLVideoElement | null) => {
        // Unregister previous element
        if (videoRef.current) {
            VideoPlaybackManager.unregister(videoRef.current);
        }
        videoRef.current = el;
        if (el) {
            VideoPlaybackManager.register(el);
        }
    }, []);

    // Cleanup on unmount
    useEffect(() => {
        return () => {
            if (videoRef.current) {
                VideoPlaybackManager.unregister(videoRef.current);
            }
        };
    }, []);

    const handlePlay = () => {
        if (videoRef.current) {
            VideoPlaybackManager.notifyPlay(videoRef.current);
        }
    };

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

        return () => {
            cancelled = true;
            // Cancel in-progress video resolve when scrolling away (P8)
            // Prevents CPU thrashing from multiple concurrent FFmpeg processes
            if (!videoUrlCache.has(src) && src.includes('.m3u8')) {
                invoke('cancel_video_resolve', { url: src }).catch(() => {});
            }
        };
    }, [src, isVisible]);

    return (
        <div ref={containerRef} className={className} style={{ ...style, position: 'relative', backgroundColor: '#000', overflow: 'hidden', minHeight: 120 }}>
            {error && (
                <div style={{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 20, textAlign: 'center', zIndex: 2 }}>
                    <span style={{ color: '#fca5a5', fontSize: '0.86rem' }}>⚠️ {error}</span>
                </div>
            )}

            {/* Loading indicator — overlaid on poster, not replacing it */}
            {isLoading && !resolvedUrl && (
                <div style={{
                    position: 'absolute',
                    bottom: 8,
                    right: 8,
                    background: 'rgba(0,0,0,0.6)',
                    borderRadius: 6,
                    padding: '4px 10px',
                    zIndex: 2,
                    display: 'flex',
                    alignItems: 'center',
                    gap: 6,
                }}>
                    <span style={{ color: '#aaa', fontSize: '0.75rem' }}>Loading…</span>
                </div>
            )}

            {/*
               Progressive playback: render the video element immediately
               with the poster so the user sees a frame right away.
               Once resolvedUrl is ready, it becomes the src and playback begins.
            */}
            <video
                ref={videoRefCallback}
                src={resolvedUrl || undefined}
                poster={resolvedPoster || undefined}
                controls={!!resolvedUrl}
                autoPlay={autoPlay && !!resolvedUrl}
                loop
                playsInline
                preload="metadata"
                style={{ width: '100%', height: '100%', objectFit: 'contain', display: 'block' }}
                onPlay={handlePlay}
                onError={(e) => {
                    if (resolvedUrl) {
                        console.error("[VideoPlayer] Video tag intrinsic WebKit error:", e.currentTarget.error);
                    }
                }}
            />
        </div>
    );
}
