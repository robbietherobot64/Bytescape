/**
 * PURPOSE
 * - Render Bytescape video media with progressive UX and backend-assisted URL resolution.
 *
 * HOW IT WORKS
 * - **Click-to-play**: No GStreamer pipeline is created until the user explicitly taps
 *   the play overlay. This prevents passive scrolling from accumulating 100-500 MB
 *   decoder pipelines per video in WebKitGTK.
 * - **Visibility unloading**: When the video scrolls out of the viewport, its source
 *   is unloaded (`src='' + load()`) to force GStreamer to tear down the pipeline.
 *   This ensures at most 1 active pipeline at a time regardless of scroll speed.
 * - Resolves sources by strategy: HLS playlist -> backend native file, vault blob
 *   hash -> backend media path, or direct URL passthrough.
 * - Uses global playback coordination so only one video actively plays at a time.
 * - Caches resolved URLs in-memory to avoid repeated backend work for the same source.
 *
 * MEMORY MODEL (why click-to-play is critical):
 * Each `<video>` element with a src in WebKitGTK creates a GStreamer decoder pipeline
 * that holds 100-500 MB of native memory (decoded frames, demuxer buffers). Even with
 * `preload="metadata"`, GStreamer allocates a partial pipeline. In a virtualized feed
 * with overscan, 4-9 pipelines can exist simultaneously, consuming 1-4 GB. If pipeline
 * teardown is slow (common in WebKitGTK), this accumulates to 10-20+ GB during fast
 * scrolling, causing OOM. Click-to-play limits active pipelines to exactly 1.
 *
 * RELATED FILES
 * - `src/hooks/useBlobUrl.ts` (poster blob URL resolution)
 * - `src/utils/mediaServer.ts` (local media server port discovery)
 * - `src/utils/VideoPlaybackManager.ts` (global play/pause coordination)
 * - `src-tauri/src/lib.rs` command handlers for media resolution/cancellation
 */
import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useLazyBlobUrl } from '../hooks/useBlobUrl';
import { getMediaPort } from '../utils/mediaServer';

interface VideoPlayerProps {
    src: string;
    poster?: string;
    mime?: string;
    className?: string;
    style?: React.CSSProperties;
    autoPlay?: boolean;
}

const VIDEO_URL_CACHE_MAX = 100;
const videoUrlCache = new Map<string, string>();

function setVideoUrlCache(key: string, value: string): void {
    if (videoUrlCache.size >= VIDEO_URL_CACHE_MAX) {
        const firstKey = videoUrlCache.keys().next().value;
        if (firstKey !== undefined) videoUrlCache.delete(firstKey);
    }
    videoUrlCache.set(key, value);
}

/** Clear the entire video URL cache. Call on filter/view changes to free memory. */
export function clearVideoUrlCache(): void {
    videoUrlCache.clear();
}

/** Current number of cached video URLs (for diagnostics). */
export function videoUrlCacheSize(): number {
    return videoUrlCache.size;
}

// ═══════════════════════════════════════════════════════════════════════════════
// Module-level singleton: only one video pipeline active at a time.
// When a new video starts, the previous one is destroyed (not just paused).
// ═══════════════════════════════════════════════════════════════════════════════
let activeVideoElement: HTMLVideoElement | null = null;

function activateVideo(el: HTMLVideoElement): void {
    if (activeVideoElement && activeVideoElement !== el) {
        try {
            activeVideoElement.pause();
            activeVideoElement.removeAttribute('src');
            activeVideoElement.load(); // forces GStreamer pipeline release
        } catch (_) { /* ignore cleanup errors */ }
    }
    activeVideoElement = el;
}

function deactivateVideo(el: HTMLVideoElement): void {
    if (activeVideoElement === el) {
        activeVideoElement = null;
    }
}

/**
 * Standard, native VideoPlayer component for Bytescape.
 * Uses click-to-play to prevent GStreamer pipeline accumulation in WebKitGTK.
 * Uses a lightweight, dedicated `warp` file server to bypass WebKitGTK's strict
 * drop-dead media restrictions.
 */
export function VideoPlayer({ src, poster, className, style, autoPlay = false }: VideoPlayerProps) {
    // ═══════════════════════════════════════════════════════════════════════
    // Local state and element refs
    // ═══════════════════════════════════════════════════════════════════════
    const [resolvedUrl, setResolvedUrl] = useState<string | null>(null);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const containerRef = useRef<HTMLDivElement>(null);
    const videoRef = useRef<HTMLVideoElement>(null);
    const [isVisible, setIsVisible] = useState(false);

    // Click-to-play: video src is only set after user interaction.
    // This prevents GStreamer pipeline creation during passive scrolling.
    const [userActivated, setUserActivated] = useState(false);

    // Cross-system note: poster rendering uses hook-level blob decoding; this component only consumes resolved output.
    // Smoothly resolve the poster if it's a blob:hash
    const [, resolvedPoster] = useLazyBlobUrl(poster);

    // ═══════════════════════════════════════════════════════════════════════
    // Global playback coordination
    // ═══════════════════════════════════════════════════════════════════════
    // Register/unregister with global playback manager
    const videoRefCallback = useCallback((el: HTMLVideoElement | null) => {
        videoRef.current = el;
    }, []);

    // Cleanup on unmount — unregister AND release the GStreamer pipeline.
    // Without removeAttribute('src') + load(), the WebKitGTK media backend
    // keeps the decoder pipeline (100-500 MB) alive even after the element is
    // removed from the DOM.
    useEffect(() => {
        return () => {
            if (videoRef.current) {
                try {
                    videoRef.current.pause();
                    videoRef.current.removeAttribute('src');
                    videoRef.current.load(); // forces GStreamer pipeline release
                } catch (_) { /* ignore cleanup errors */ }
                deactivateVideo(videoRef.current);
            }
        };
    }, []);

    const handlePlay = () => {
        // Destroy previous video's GStreamer pipeline, activate this one
        if (videoRef.current) {
            activateVideo(videoRef.current);
        }
    };

    // ═══════════════════════════════════════════════════════════════════════
    // Visibility gating — dual IntersectionObserver
    // ═══════════════════════════════════════════════════════════════════════
    // Observer 1: Trigger lazy URL resolution when near viewport (one-shot for initial entry).
    // Observer 2: Continuous monitoring — unload video src when scrolled far out of view
    //             to force GStreamer pipeline teardown. This is the primary memory defense.
    useEffect(() => {
        const entryObserver = new IntersectionObserver(
            ([entry]) => {
                if (entry.isIntersecting) {
                    setIsVisible(true);
                    entryObserver.disconnect();
                }
            },
            { rootMargin: '200px' }
        );

        if (containerRef.current) {
            entryObserver.observe(containerRef.current);
        }

        return () => entryObserver.disconnect();
    }, []);

    // Observer 2: Continuous visibility tracking — unload pipeline when far from viewport.
    // When the video leaves the viewport (+ 100px margin), we force-unload its src to
    // destroy the GStreamer pipeline. This ensures pipeline memory is reclaimed promptly
    // even if React's virtualizer hasn't unmounted the component yet.
    useEffect(() => {
        if (!containerRef.current) return;

        const visibilityObserver = new IntersectionObserver(
            ([entry]) => {
                if (!entry.isIntersecting && userActivated) {
                    // Video has scrolled out of view — tear down the pipeline
                    if (videoRef.current) {
                        try {
                            videoRef.current.pause();
                            videoRef.current.removeAttribute('src');
                            videoRef.current.load();
                        } catch (_) { /* ignore */ }
                        deactivateVideo(videoRef.current);
                    }
                    setUserActivated(false);
                }
            },
            { rootMargin: '100px' }
        );

        visibilityObserver.observe(containerRef.current);
        return () => visibilityObserver.disconnect();
    }, [userActivated]);

    // ═══════════════════════════════════════════════════════════════════════
    // Source resolution pipeline (includes Tauri invoke bridge)
    // ═══════════════════════════════════════════════════════════════════════
    // MEMORY-CRITICAL: URL resolution only starts when userActivated is true.
    // Before that, no src is set on the <video> element, so no GStreamer
    // pipeline is created. This is the core of the click-to-play strategy.
    useEffect(() => {
        if (!src || !isVisible || !userActivated) return;

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
                            setVideoUrlCache(src, assetUrl);
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
                            setVideoUrlCache(src, assetUrl);
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
    }, [src, isVisible, userActivated]);

    // ═══════════════════════════════════════════════════════════════════════
    // Click-to-play handler
    // ═══════════════════════════════════════════════════════════════════════
    const handleActivate = useCallback(() => {
        setUserActivated(true);
    }, []);

    // Determine whether the video element should have a src.
    // autoPlay prop (used in GalleryOverlay) bypasses click-to-play.
    const videoSrc = (userActivated || autoPlay) ? (resolvedUrl || undefined) : undefined;
    const showPlayOverlay = !userActivated && !autoPlay && !error;

    // Auto-activate resolution for autoPlay mode (gallery overlay)
    useEffect(() => {
        if (autoPlay && isVisible) {
            setUserActivated(true);
        }
    }, [autoPlay, isVisible]);

    // ═══════════════════════════════════════════════════════════════════════
    // Render
    // ═══════════════════════════════════════════════════════════════════════
    return (
        <div ref={containerRef} className={`video-player-responsive ${className || ''}`} style={{ ...style, position: 'relative', backgroundColor: '#000', overflow: 'hidden', minHeight: 120 }}>
            {error && (
                <div style={{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 20, textAlign: 'center', zIndex: 2 }}>
                    <span style={{ color: '#fca5a5', fontSize: '0.86rem' }}>⚠️ {error}</span>
                </div>
            )}

            {/* Loading indicator — overlaid on poster, not replacing it */}
            {isLoading && !resolvedUrl && userActivated && (
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

            {/* Click-to-play overlay — shown when video is not yet activated.
                Prevents GStreamer pipeline creation during passive scrolling. */}
            {showPlayOverlay && (
                <div
                    onClick={handleActivate}
                    style={{
                        position: 'absolute',
                        inset: 0,
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'center',
                        zIndex: 3,
                        cursor: 'pointer',
                        background: 'rgba(0,0,0,0.15)',
                    }}
                >
                    <div style={{
                        width: 56,
                        height: 56,
                        borderRadius: '50%',
                        background: 'rgba(0,0,0,0.55)',
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'center',
                        backdropFilter: 'blur(4px)',
                    }}>
                        <svg width="24" height="28" viewBox="0 0 24 28" fill="none">
                            <path d="M22 14L2 26V2L22 14Z" fill="white" fillOpacity="0.9"/>
                        </svg>
                    </div>
                </div>
            )}

            {/*
               Click-to-play video element:
               - src is undefined until the user taps the play overlay (or autoPlay is set)
               - preload="none" prevents GStreamer from creating even a metadata pipeline
               - When src is set, autoPlay starts playback immediately
            */}
            <video
                ref={videoRefCallback}
                src={videoSrc}
                poster={resolvedPoster || undefined}
                controls={!!videoSrc}
                autoPlay={!!videoSrc}
                loop
                playsInline
                preload={videoSrc ? 'metadata' : 'none'}
                style={{ width: '100%', height: '100%', objectFit: 'contain', display: 'block' }}
                onPlay={handlePlay}
                onError={(e) => {
                    if (videoSrc) {
                        console.error("[VideoPlayer] Video tag intrinsic WebKit error:", e.currentTarget.error);
                    }
                }}
            />
        </div>
    );
}
