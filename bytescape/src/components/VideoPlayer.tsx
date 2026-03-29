import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useLazyBlobUrl } from '../hooks/useBlobUrl';
import { getMediaPort } from '../utils/mediaServer';
import { VideoPlaybackManager } from '../utils/VideoPlaybackManager';
import { useTabActive } from '../contexts/TabActiveContext';

interface VideoPlayerProps {
    src: string;
    poster?: string;
    mime?: string;
    className?: string;
    style?: React.CSSProperties;
    autoPlay?: boolean;
}

const VIDEO_URL_CACHE_MAX = 50;
const videoUrlCache = new Map<string, string>();

/** Cache a resolved video URL with LRU eviction. */
function cacheVideoUrl(src: string, url: string) {
    if (videoUrlCache.size >= VIDEO_URL_CACHE_MAX) {
        const firstKey = videoUrlCache.keys().next().value;
        if (firstKey !== undefined) videoUrlCache.delete(firstKey);
    }
    videoUrlCache.set(src, url);
}

/** Force WebKitGTK to release a video element's decoder buffers. */
function releaseVideoResources(el: HTMLVideoElement) {
    try {
        el.pause();
        el.removeAttribute('src');
        el.load();
    } catch (_) { /* ignore */ }
}

/**
 * Click-to-play VideoPlayer for Bytescape.
 *
 * ARCHITECTURE: Zero GStreamer pipelines until the user explicitly clicks play.
 * - Default state: poster thumbnail + play button overlay. No <video> element.
 * - User clicks play: resolve blob URL → mount <video> → set src → pipeline starts.
 * - Scrolls out of view: tear down pipeline → back to poster.
 * - Tab switches: tear down pipeline → back to poster.
 *
 * This means a feed with 100 video posts uses ZERO native memory for video
 * until the user interacts. Only the lightweight poster <img> tags exist.
 */
export function VideoPlayer({ src, poster, className, style }: VideoPlayerProps) {
    // activated = user has clicked the play button on THIS video
    const [activated, setActivated] = useState(false);
    const [resolvedUrl, setResolvedUrl] = useState<string | null>(null);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const containerRef = useRef<HTMLDivElement>(null);
    const videoRef = useRef<HTMLVideoElement>(null);
    const [isVisible, setIsVisible] = useState(false);
    const isTabActive = useTabActive();

    // Resolve poster blob:hash → displayable URL (lightweight, no video pipeline)
    const [, resolvedPoster] = useLazyBlobUrl(poster);

    // Register/unregister with global playback manager
    const videoRefCallback = useCallback((el: HTMLVideoElement | null) => {
        if (videoRef.current && videoRef.current !== el) {
            releaseVideoResources(videoRef.current);
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
                releaseVideoResources(videoRef.current);
                VideoPlaybackManager.unregister(videoRef.current);
                videoRef.current = null;
            }
        };
    }, []);

    const handlePlay = () => {
        if (videoRef.current) {
            VideoPlaybackManager.notifyPlay(videoRef.current);
        }
    };

    // IntersectionObserver — only used for CLEANUP (tearing down pipeline
    // when user scrolls away from an activated video). Does NOT trigger loading.
    useEffect(() => {
        const observer = new IntersectionObserver(
            ([entry]) => {
                setIsVisible(entry.isIntersecting);
            },
            { rootMargin: '100px' }
        );
        if (containerRef.current) observer.observe(containerRef.current);
        return () => observer.disconnect();
    }, []);

    // Deactivate (tear down pipeline) when scrolled away or tab switches.
    // User must click play again to restart — this is intentional.
    // Each GStreamer pipeline costs 100-500MB of native memory.
    useEffect(() => {
        if (activated && (!isVisible || !isTabActive)) {
            setActivated(false);
            setResolvedUrl(null);
            if (videoRef.current) {
                releaseVideoResources(videoRef.current);
            }
        }
    }, [activated, isVisible, isTabActive]);

    // Reset when src changes (different video)
    useEffect(() => {
        setActivated(false);
        setResolvedUrl(null);
        setError(null);
    }, [src]);

    // Load the actual video pipeline — only runs when user clicks play
    useEffect(() => {
        if (!activated || !src) return;

        let cancelled = false;

        async function loadVideo() {
            if (videoUrlCache.has(src)) {
                if (!cancelled) setResolvedUrl(videoUrlCache.get(src)!);
                return;
            }

            setIsLoading(true);
            setError(null);

            try {
                if (src.includes('.m3u8')) {
                    const filePath = await invoke<string>('resolve_hls_video', { playlistUrl: src });
                    if (!cancelled && filePath) {
                        const filename = filePath.split(/[/\\]/).pop();
                        if (filename) {
                            const port = await getMediaPort();
                            const url = `http://127.0.0.1:${port}/${filename}`;
                            cacheVideoUrl(src, url);
                            setResolvedUrl(url);
                        }
                    }
                }
                else if (src.startsWith('blob:') && !src.startsWith('blob:http') && !src.startsWith('blob:tauri')) {
                    const hash = src.replace(/^blob:/, '').trim();
                    const filePath = await invoke<string>('get_media_blob', { hashStr: hash });
                    if (!cancelled && filePath) {
                        const filename = filePath.split(/[/\\]/).pop();
                        if (filename) {
                            const port = await getMediaPort();
                            const url = `http://127.0.0.1:${port}/${filename}`;
                            cacheVideoUrl(src, url);
                            setResolvedUrl(url);
                        }
                    }
                }
                else {
                    if (!cancelled) setResolvedUrl(src);
                }
            } catch (err) {
                console.error("[VideoPlayer] load failed:", err);
                if (!cancelled) setError('Failed to load video');
            } finally {
                if (!cancelled) setIsLoading(false);
            }
        }

        loadVideo();

        return () => {
            cancelled = true;
            if (!videoUrlCache.has(src) && src.includes('.m3u8')) {
                invoke('cancel_video_resolve', { url: src }).catch(() => {});
            }
        };
    }, [activated, src]);

    // User clicks the play button overlay → activate this video
    const handleActivate = () => {
        if (!activated && !isLoading) {
            setActivated(true);
        }
    };

    return (
        <div
            ref={containerRef}
            className={className}
            style={{ ...style, position: 'relative', backgroundColor: '#000', overflow: 'hidden', minHeight: 120 }}
        >
            {error && (
                <div style={{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 20, textAlign: 'center', zIndex: 2 }}>
                    <span style={{ color: '#fca5a5', fontSize: '0.86rem' }}>⚠️ {error}</span>
                </div>
            )}

            {isLoading && (
                <div style={{
                    position: 'absolute', bottom: 8, right: 8,
                    background: 'rgba(0,0,0,0.6)', borderRadius: 6,
                    padding: '4px 10px', zIndex: 2,
                }}>
                    <span style={{ color: '#aaa', fontSize: '0.75rem' }}>Loading…</span>
                </div>
            )}

            {/*
               Two states:
               1. ACTIVATED + resolvedUrl → mount <video> with src (GStreamer pipeline runs)
               2. Everything else → poster image + play button (zero native memory)
            */}
            {activated && resolvedUrl ? (
                <video
                    ref={videoRefCallback}
                    src={resolvedUrl}
                    poster={resolvedPoster || undefined}
                    controls
                    autoPlay
                    playsInline
                    preload="none"
                    style={{ width: '100%', height: '100%', objectFit: 'contain', display: 'block' }}
                    onPlay={handlePlay}
                    onError={(e) => {
                        console.error("[VideoPlayer] WebKit error:", e.currentTarget.error);
                    }}
                />
            ) : (
                /* Poster + play button — zero GStreamer cost */
                <div
                    onClick={handleActivate}
                    style={{
                        width: '100%', height: '100%', cursor: 'pointer',
                        display: 'flex', alignItems: 'center', justifyContent: 'center',
                        position: 'relative', minHeight: 120,
                    }}
                >
                    {resolvedPoster ? (
                        <img
                            src={resolvedPoster}
                            alt=""
                            style={{ width: '100%', height: '100%', objectFit: 'contain', display: 'block' }}
                        />
                    ) : (
                        <div style={{ width: '100%', height: '100%', minHeight: 120, background: '#111' }} />
                    )}
                    {/* Play button overlay */}
                    {!isLoading && (
                        <div style={{
                            position: 'absolute', inset: 0,
                            display: 'flex', alignItems: 'center', justifyContent: 'center',
                            background: 'rgba(0,0,0,0.3)',
                        }}>
                            <div style={{
                                width: 56, height: 56, borderRadius: '50%',
                                background: 'rgba(0,0,0,0.6)', display: 'flex',
                                alignItems: 'center', justifyContent: 'center',
                                backdropFilter: 'blur(4px)',
                            }}>
                                <span style={{ color: '#fff', fontSize: '1.5rem', marginLeft: 3 }}>▶</span>
                            </div>
                        </div>
                    )}
                </div>
            )}
        </div>
    );
}
