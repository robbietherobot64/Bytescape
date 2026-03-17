/**
 * VideoPlaybackManager — singleton that coordinates video playback
 * across the app. Ensures only one video plays at a time.
 *
 * When any video starts playing, all other registered videos are paused.
 * Gallery overlays call `pauseAll()` to silence background videos.
 */
class VideoPlaybackManagerClass {
    private videos = new Set<HTMLVideoElement>();

    /** Register a video element for global coordination. */
    register(el: HTMLVideoElement): void {
        this.videos.add(el);
    }

    /** Unregister a video element (call on unmount). */
    unregister(el: HTMLVideoElement): void {
        this.videos.delete(el);
    }

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
}

export const VideoPlaybackManager = new VideoPlaybackManagerClass();
