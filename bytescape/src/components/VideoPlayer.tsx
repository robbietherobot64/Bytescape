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

const VIDEO_URL_CACHE_MAX = 50;
const videoUrlCache = new Map<string, string>();

/**
 * Debounce delay (ms) before a visibility change triggers video loading.
 * Prevents GStreamer pipeline churn when rapidly toggling filters or scrolling.
 * WebKitGTK needs ~150-300ms to fully tear down a GStreamer pipeline —
 * this delay ensures the old pipeline is released before we create a new one.
 */
const VISIBILITY_DEBOUNCE_MS = 200;

/**
 * Global count of currently active (src-loaded) video elements.
 * Used for backpressure: when too many videos are simultaneously loading,
 * new ones wait for existing pipelines to settle.
 */
let activeVideoCount = 0;
const MAX_CONCURRENT_VIDEOS = 6;

/** Cache a resolved video URL with LRU eviction to prevent unbounded growth. */
function cacheVideoUrl(src: string, url: string) {
    if (videoUrlCache.size >= VIDEO_URL_CACHE_MAX) {
        // Maps iterate in insertion order — evict the oldest entry
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
        el.load(); // Triggers WebKitGTK to tear down the media pipeline
    } catch (_) { /* ignore errors during cleanup */ }
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
    const videoRef = useRef<HTMLVideoElement>(null);
    const [isVisible, setIsVisible] = useState(false);

    // Smoothly resolve the poster if it's a blob:hash
    const [, resolvedPoster] = useLazyBlobUrl(poster);

    // Register/unregister with global playback manager
    const videoRefCallback = useCallback((el: HTMLVideoElement | null) => {
        // Release previous element's decoder buffers before losing the reference
        if (videoRef.current && videoRef.current !== el) {
            releaseVideoResources(videoRef.current);
            VideoPlaybackManager.unregister(videoRef.current);
        }
        videoRef.current = el;
        if (el) {
            VideoPlaybackManager.register(el);
        }
    }, []);

    // Cleanup on unmount — force WebKitGTK to release decoder memory
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

    // Bidirectional intersection observer — tracks both entering AND leaving
    // the viewport. Debounced to prevent rapid GStreamer pipeline churn when
    // filter toggles cause mass mount/unmount of VideoPlayer components.
    // The debounce ensures WebKitGTK has time to tear down old pipelines
    // before new ones are created.
    useEffect(() => {
        let debounceTimer: ReturnType<typeof setTimeout> | null = null;

        const observer = new IntersectionObserver(
            ([entry]) => {
                if (debounceTimer) clearTimeout(debounceTimer);

                if (!entry.isIntersecting) {
                    // Leaving viewport — apply immediately to release resources fast
                    setIsVisible(false);
                } else {
                    // Entering viewport — debounce to prevent pipeline churn
                    debounceTimer = setTimeout(() => {
                        setIsVisible(true);
                    }, VISIBILITY_DEBOUNCE_MS);
                }
            },
            { rootMargin: '200px' }
        );

        if (containerRef.current) {
            observer.observe(containerRef.current);
        }

        return () => {
            if (debounceTimer) clearTimeout(debounceTimer);
            observer.disconnect();
        };
    }, []);

    useEffect(() => {
        if (!src || !isVisible) {
            // Clear resolved URL when not visible — React will remove src from
            // the <video> DOM element on the next render. This is more reliable
            // than manual removeAttribute('src') because React drives the DOM.
            // WebKitGTK tears down its GStreamer pipeline when src is removed.
            if (resolvedUrl) {
                activeVideoCount = Math.max(0, activeVideoCount - 1);
            }
            setResolvedUrl(null);
            return;
        }

        let cancelled = false;
        let loadTimer: ReturnType<typeof setTimeout> | null = null;

        async function loadVideo() {
            // Backpressure: wait for pipeline slots if too many videos are active.
            // This prevents overwhelming WebKitGTK's GStreamer with 15+ simultaneous
            // decode pipelines (which is the root cause of the memory leak on video filter).
            if (activeVideoCount >= MAX_CONCURRENT_VIDEOS) {
                // Wait and retry — another video will free a slot
                loadTimer = setTimeout(() => {
                    if (!cancelled) loadVideo();
                }, 100);
                return;
            }

            if (videoUrlCache.has(src)) {
                activeVideoCount++;
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
                            cacheVideoUrl(src, assetUrl);
                            activeVideoCount++;
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
                            cacheVideoUrl(src, assetUrl);
                            activeVideoCount++;
                            setResolvedUrl(assetUrl);
                        }
                    }
                }
                // 3. Handle direct ready-to-play URLs (HTTP, Object URL, standard Tauri Asset URL)
                else {
                    activeVideoCount++;
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
            if (loadTimer) clearTimeout(loadTimer);
            // Cancel in-progress video resolve when scrolling away (P8)
            // Prevents CPU thrashing from multiple concurrent FFmpeg processes
            if (!videoUrlCache.has(src) && src.includes('.m3u8')) {
                invoke('cancel_video_resolve', { url: src }).catch(() => {});
            }
            // Release video decoder resources to prevent WebKitGTK memory leak
            if (videoRef.current) {
                releaseVideoResources(videoRef.current);
            }
            if (resolvedUrl) {
                activeVideoCount = Math.max(0, activeVideoCount - 1);
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
                playsInline
                preload="none"
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
