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
 *
 * IMPORTANT: This was previously a closure-captured variable that suffered
 * from stale closures in useEffect cleanup. Now tracked via a simple global
 * that's incremented/decremented symmetrically using a per-instance ref flag.
 */
let activeVideoCount = 0;
const MAX_CONCURRENT_VIDEOS = 2;

/** Cache a resolved video URL with LRU eviction to prevent unbounded growth. */
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
        el.load(); // Triggers WebKitGTK to tear down the media pipeline
    } catch (_) { /* ignore errors during cleanup */ }
}

/**
 * Standard, native VideoPlayer component for Bytescape.
 * Uses a lightweight, dedicated `warp` file server to bypass WebKitGTK's strict drop-dead media restrictions.
 *
 * Tab-aware: reads TabActiveContext to know when its parent tab is hidden.
 * When the tab is inactive, video src is cleared through React state (not DOM
 * hacking) — this reliably tears down GStreamer pipelines because React drives
 * the DOM update.
 */
export function VideoPlayer({ src, poster, className, style, autoPlay = false }: VideoPlayerProps) {
    const [resolvedUrl, setResolvedUrl] = useState<string | null>(null);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const containerRef = useRef<HTMLDivElement>(null);
    const videoRef = useRef<HTMLVideoElement>(null);
    const [isVisible, setIsVisible] = useState(false);
    const isTabActive = useTabActive();

    // Track whether this instance has incremented activeVideoCount.
    // This ref is the single source of truth — no stale closure issues.
    const isCountedRef = useRef(false);

    function claimSlot() {
        if (!isCountedRef.current) {
            isCountedRef.current = true;
            activeVideoCount++;
        }
    }
    function releaseSlot() {
        if (isCountedRef.current) {
            isCountedRef.current = false;
            activeVideoCount = Math.max(0, activeVideoCount - 1);
        }
    }

    // Smoothly resolve the poster if it's a blob:hash
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

    // Cleanup on unmount — release decoder + slot
    useEffect(() => {
        return () => {
            if (videoRef.current) {
                releaseVideoResources(videoRef.current);
                VideoPlaybackManager.unregister(videoRef.current);
                videoRef.current = null;
            }
            releaseSlot();
        };
    }, []);

    const handlePlay = () => {
        if (videoRef.current) {
            VideoPlaybackManager.notifyPlay(videoRef.current);
        }
    };

    // Bidirectional intersection observer
    useEffect(() => {
        let debounceTimer: ReturnType<typeof setTimeout> | null = null;

        const observer = new IntersectionObserver(
            ([entry]) => {
                if (debounceTimer) clearTimeout(debounceTimer);

                if (!entry.isIntersecting) {
                    setIsVisible(false);
                } else {
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

    // The "should load" gate: must be visible AND tab must be active.
    // When tab becomes inactive, shouldLoad becomes false, clearing the URL
    // through React state — not DOM hacking. This is the key fix for the
    // GStreamer pipeline leak on tab switch.
    const shouldLoad = isVisible && isTabActive;

    useEffect(() => {
        if (!src || !shouldLoad) {
            // Clear resolved URL — React removes src from <video> on next render.
            // WebKitGTK tears down the GStreamer pipeline when src disappears.
            releaseSlot();
            setResolvedUrl(null);
            return;
        }

        let cancelled = false;
        let loadTimer: ReturnType<typeof setTimeout> | null = null;

        async function loadVideo() {
            // Backpressure: wait for pipeline slots
            if (activeVideoCount >= MAX_CONCURRENT_VIDEOS) {
                loadTimer = setTimeout(() => {
                    if (!cancelled) loadVideo();
                }, 200);
                return;
            }

            if (videoUrlCache.has(src)) {
                claimSlot();
                setResolvedUrl(videoUrlCache.get(src)!);
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
                            const assetUrl = `http://127.0.0.1:${port}/${filename}`;
                            cacheVideoUrl(src, assetUrl);
                            claimSlot();
                            setResolvedUrl(assetUrl);
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
                            const assetUrl = `http://127.0.0.1:${port}/${filename}`;
                            cacheVideoUrl(src, assetUrl);
                            claimSlot();
                            setResolvedUrl(assetUrl);
                        }
                    }
                }
                else {
                    claimSlot();
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
            if (!videoUrlCache.has(src) && src.includes('.m3u8')) {
                invoke('cancel_video_resolve', { url: src }).catch(() => {});
            }
            // Release slot and video resources on cleanup
            releaseSlot();
            if (videoRef.current) {
                releaseVideoResources(videoRef.current);
            }
        };
    }, [src, shouldLoad]);

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
