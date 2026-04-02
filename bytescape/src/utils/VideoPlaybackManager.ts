/**
 * PURPOSE:
 * Process-wide playback coordinator for HTML video elements in the UI layer.
 * Ensures only one video plays at a time and provides emergency global pause/
 * release paths for heavy feed transitions.
 *
 * HOW IT WORKS:
 * - Components register/unregister their `<video>` elements.
 * - The active player notifies the manager on play.
 * - Manager pauses competing players and can force release all pipelines.
 *
 * RELATED FILES:
 * - src/components/** (video-bearing cards/players)
 * - src/screens/FeedScreen.tsx (view transitions that may trigger cleanup)
 * - src/hooks/useUnifiedFeed.ts (filter/view changes that affect media volume)
 *
 * CAVEAT (singleton lifecycle):
 * This singleton is module-scoped and survives React remounts. If unregister
 * is skipped during unmount, stale element references may persist until prune/
 * release paths run.
 */

/**
 * VideoPlaybackManager — singleton that coordinates video playback
 * across the app. Ensures only one video plays at a time.
 *
 * When any video starts playing, all other registered videos are paused.
 * Gallery overlays call `pauseAll()` to silence background videos.
 *
 * Includes defensive pruning to prevent stale HTMLVideoElement references
 * from accumulating (which hold GStreamer pipeline memory in WebKitGTK).
 */
class VideoPlaybackManagerClass {
    private videos = new Set<HTMLVideoElement>();

    // -------------------------------------------------------------------------
    // Registration Lifecycle
    // -------------------------------------------------------------------------

    /** Maximum registered elements before forced pruning.
     * Lowered from 30 → 6 → 3 to aggressively reclaim GStreamer pipeline memory.
     * WebKitGTK pipelines consume 100–500 MB each of native (non-JS-heap)
     * memory. At 3 pipelines max, worst-case video memory is ~1.5 GB instead
     * of 3 GB. The virtualizer only shows ~3-4 videos on screen at once anyway.
     */
    private static readonly MAX_REGISTERED = 3;

    /** Register a video element for global coordination. */
    register(el: HTMLVideoElement): void {
        this.videos.add(el);
        // Defensive: if the set grows beyond what the virtualizer should
        // produce (~10-15 visible + 5 overscan), prune disconnected elements.
        if (this.videos.size > VideoPlaybackManagerClass.MAX_REGISTERED) {
            this.pruneDisconnected();
        }
    }

    /** Unregister a video element (call on unmount). */
    unregister(el: HTMLVideoElement): void {
        this.videos.delete(el);
    }

    /** Remove elements that are no longer in the DOM (stale refs). */
    private pruneDisconnected(): void {
        for (const el of this.videos) {
            if (!el.isConnected) {
                this.videos.delete(el);
            }
        }
    }

    // -------------------------------------------------------------------------
    // Playback Coordination
    // -------------------------------------------------------------------------

    /** Notify that a video started playing — pause all others. */
    notifyPlay(active: HTMLVideoElement): void {
        for (const el of this.videos) {
            if (el !== active && !el.paused) {
                el.pause();
            }
        }
    }

    /** Pause every registered video (e.g. when a gallery overlay opens). */
    pauseAll(): void {
        for (const el of this.videos) {
            if (!el.paused) {
                el.pause();
            }
        }
    }

    // -------------------------------------------------------------------------
    // Diagnostics + Hard Cleanup
    // -------------------------------------------------------------------------

    /** Number of currently registered video elements (for debugging). */
    get count(): number {
        return this.videos.size;
    }

    /**
     * Force-release all registered video elements and clear the set.
     * Call when switching away from a video-heavy view (e.g. filter toggle).
     */
    releaseAll(): void {
        for (const el of this.videos) {
            try {
                el.pause();
                el.removeAttribute('src');
                el.load();
            } catch (_) { /* ignore cleanup errors */ }
        }
        this.videos.clear();
    }
}

/**
 * Exported singleton instance used across UI components/hooks.
 *
 * Cross-system note:
 * - No IPC/store writes occur here; this module coordinates browser media
 *   elements only. Hook/store layers decide when lifecycle methods are called.
 */
export const VideoPlaybackManager = new VideoPlaybackManagerClass();
