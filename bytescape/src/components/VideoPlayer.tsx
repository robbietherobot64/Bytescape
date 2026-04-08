/**
 * VideoPlayer — In-post video playback for Bytescape feeds.
 *
 * Resolves HLS / blob / URL sources to a local file served via warp,
 * then hands off to a plain <video> element. Visibility-gated so only
 * videos near the viewport resolve their URL (saves IPC + FFmpeg work).
 *
 * Key design decisions for smooth playback:
 * - NO destructive src teardown (removeAttribute + load) — that causes
 *   GStreamer pipeline re-creation and audible/visual glitches.
 * - Visibility changes are debounced (150ms) to prevent rapid play/pause
 *   thrashing during fast scrolling.
 * - Fullscreen playback is protected: while a video is fullscreen, the
 *   IntersectionObserver pause logic is bypassed.
 * - Only one video plays at a time (global singleton tracking).
 */
import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import useBlobUrl from '../hooks/useBlobUrl';
import { getMediaPort } from '../utils/mediaServer';

interface VideoPlayerProps {
    src: string;
    poster?: string;
    mime?: string;
    className?: string;
    style?: React.CSSProperties;
    autoPlay?: boolean;
    controls?: boolean;
    muted?: boolean;
}

const VIDEO_URL_CACHE_MAX = 120;
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

// One active playing element at a time.
let activeVideoElement: HTMLVideoElement | null = null;

/** Returns true if any video is currently playing globally. */
export function isAnyVideoPlaying(): boolean {
    if (!activeVideoElement) return false;
    return !activeVideoElement.paused && !activeVideoElement.ended;
}

function activateVideo(el: HTMLVideoElement): void {
    if (activeVideoElement && activeVideoElement !== el) {
        try {
            activeVideoElement.pause();
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
 * Stable feed video player:
 * - resolves source once per visible item
 * - auto-plays when visible (when `autoPlay` is true)
 * - pauses when not visible (debounced to avoid scroll thrash)
 * - protects fullscreen playback from visibility changes
 */
export function VideoPlayer({
    src,
    poster,
    className,
    style,
    autoPlay = false,
    controls = true,
    muted,
}: VideoPlayerProps) {
    const [resolvedUrl, setResolvedUrl] = useState<string | null>(null);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const containerRef = useRef<HTMLDivElement>(null);
    const videoRef = useRef<HTMLVideoElement>(null);
    const [isVisible, setIsVisible] = useState(false);
    const isFullscreenRef = useRef(false);

    // autoplay in feeds should be muted to satisfy platform autoplay policies.
    const effectiveMuted = muted ?? autoPlay;

    const [resolvedPoster] = useBlobUrl(poster);

    const videoRefCallback = useCallback((el: HTMLVideoElement | null) => {
        videoRef.current = el;
    }, []);

    // Track fullscreen state so we never pause a fullscreen video
    // due to IntersectionObserver firing isIntersecting=false.
    useEffect(() => {
        const onFsChange = () => {
            isFullscreenRef.current = !!(
                document.fullscreenElement ||
                (document as unknown as Record<string, unknown>).webkitFullscreenElement
            );
        };
        document.addEventListener('fullscreenchange', onFsChange);
        document.addEventListener('webkitfullscreenchange', onFsChange);
        return () => {
            document.removeEventListener('fullscreenchange', onFsChange);
            document.removeEventListener('webkitfullscreenchange', onFsChange);
        };
    }, []);

    useEffect(() => {
        return () => {
            if (videoRef.current) {
                try {
                    videoRef.current.pause();
                } catch (_) { /* ignore cleanup errors */ }
                deactivateVideo(videoRef.current);
            }
        };
    }, []);

    const handlePlay = () => {
        if (videoRef.current) {
            activateVideo(videoRef.current);
        }
    };

    // Debounced visibility tracking — prevents rapid play/pause thrashing
    // during fast scrolling. 150ms delay means the video must be continuously
    // out-of-viewport for 150ms before we consider it "gone".
    useEffect(() => {
        let debounceTimer: ReturnType<typeof setTimeout> | null = null;

        const observer = new IntersectionObserver(
            ([entry]) => {
                if (entry.isIntersecting) {
                    // Entering viewport: apply immediately (start resolution ASAP)
                    if (debounceTimer) { clearTimeout(debounceTimer); debounceTimer = null; }
                    setIsVisible(true);
                } else {
                    // Leaving viewport: debounce to avoid thrash during fast scroll
                    if (debounceTimer) clearTimeout(debounceTimer);
                    debounceTimer = setTimeout(() => setIsVisible(false), 150);
                }
            },
            { rootMargin: '220px' }
        );

        if (containerRef.current) {
            observer.observe(containerRef.current);
        }

        return () => {
            observer.disconnect();
            if (debounceTimer) clearTimeout(debounceTimer);
        };
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
                if (src.includes('.m3u8')) {
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
                } else if (src.startsWith('blob:') && !src.startsWith('blob:http') && !src.startsWith('blob:tauri')) {
                    const hash = src.replace(/^blob:/, '').trim();
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
                } else {
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
            if (!videoUrlCache.has(src) && src.includes('.m3u8')) {
                invoke('cancel_video_resolve', { playlistUrl: src }).catch(() => {});
            }
        };
    }, [src, isVisible]);

    // Visibility-driven autoplay in feed contexts.
    // Only one visible autoplay video should actively play at a time.
    // NEVER pause a video that's in native fullscreen (the element leaves
    // normal flow so IntersectionObserver reports it as non-intersecting).
    useEffect(() => {
        const el = videoRef.current;
        if (!el || !resolvedUrl || !autoPlay) return;

        if (!isVisible && !isFullscreenRef.current) {
            el.pause();
            if (activeVideoElement === el) {
                deactivateVideo(el);
            }
            return;
        }

        // Don't interrupt an already-playing video (including fullscreen)
        if (!el.paused) return;

        // If another video is currently active and still playing, do not steal.
        if (activeVideoElement && activeVideoElement !== el && !activeVideoElement.paused) {
            return;
        }

        const p = el.play();
        if (p && typeof p.then === 'function') {
            p.then(() => {
                activateVideo(el);
            }).catch(() => {});
        } else {
            activateVideo(el);
        }
    }, [autoPlay, isVisible, resolvedUrl]);

    // ═══════════════════════════════════════════════════════════════════════
    // Render
    // ═══════════════════════════════════════════════════════════════════════
    return (
        <div ref={containerRef} className={`${className?.includes('inline-video-container') ? '' : 'video-player-responsive '}${className || ''}`} style={{ ...style, position: 'relative', backgroundColor: '#000', overflow: 'hidden' }}>
            {error && (
                <div style={{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 20, textAlign: 'center', zIndex: 2 }}>
                    <span style={{ color: '#fca5a5', fontSize: '0.86rem' }}>⚠️ {error}</span>
                </div>
            )}

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
            <video
                ref={videoRefCallback}
                src={resolvedUrl || undefined}
                poster={resolvedPoster || undefined}
                controls={controls}
                autoPlay={autoPlay}
                muted={effectiveMuted}
                playsInline
                preload={autoPlay ? 'auto' : 'metadata'}
                style={{ width: '100%', height: '100%', display: 'block' }}
                onPlay={handlePlay}
                onEnded={() => {
                    if (videoRef.current) {
                        deactivateVideo(videoRef.current);
                    }
                }}
                onError={(e) => {
                    if (resolvedUrl) {
                        console.error("[VideoPlayer] Video tag intrinsic WebKit error:", e.currentTarget.error);
                    }
                }}
            />
        </div>
    );
}
