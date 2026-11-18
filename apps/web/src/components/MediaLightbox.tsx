import { useCallback, useEffect, useRef, useState } from 'react';
import PostCard, { HlsVideo } from './PostCard';
import type { FeedPost, GroupRef } from '../utils/api';

/**
 * Fullscreen media viewer (Instagram-style split view) — the ONE media lightbox
 * for PostCard image/video clicks everywhere (main feed, group cards, community
 * page). Reworks the old `CarouselLightbox` plumbing (index, Escape, swipe).
 *
 *   LEFT  — media fullscreen (image contain / video with controls).
 *   RIGHT — post author + community context (header), then the shared
 *           `<PostCard>` for body + interaction row + comments — the EXACT same
 *           controls as a regular post card (❤️ like + 👍/👎 votes-as-reactions
 *           on group posts + emoji reactions + the comment thread/composer),
 *           with the body shown in full (no clamping).
 *
 * Close via Escape / backdrop click / X. Keyboard: Escape closes, ←/→ change
 * media, focus moves into the dialog on open and back out on close. Mobile
 * (≤550px): media on top, text/interactions/comments stacked + scrollable below.
 */
export default function MediaLightbox({
  post,
  group,
  startIndex,
  onClose,
  currentUserHandle,
}: {
  post: FeedPost;
  group?: GroupRef | null;
  startIndex: number;
  onClose: () => void;
  currentUserHandle?: string;
}) {
  const [index, setIndex] = useState(startIndex);
  const touchStartX = useRef<number | null>(null);
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const restoreFocus = useRef<HTMLElement | null>(null);

  const media = post.media ?? [];
  const m = media[index];

  const goNext = useCallback(() => {
    setIndex((i) => Math.min(media.length - 1, i + 1));
  }, [media.length]);
  const goPrev = useCallback(() => {
    setIndex((i) => Math.max(0, i - 1));
  }, []);

  // Focus management: remember the opener, move focus into the dialog, restore
  // on close. Escape closes; ←/→ navigate media.
  useEffect(() => {
    restoreFocus.current = (document.activeElement as HTMLElement) ?? null;
    dialogRef.current?.focus();
    return () => {
      restoreFocus.current?.focus?.();
    };
  }, []);
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        onClose();
      } else if (e.key === 'ArrowRight') {
        goNext();
      } else if (e.key === 'ArrowLeft') {
        goPrev();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [onClose, goNext, goPrev]);

  const handleTouchStart = (e: React.TouchEvent) => {
    touchStartX.current = e.touches[0].clientX;
  };
  const handleTouchEnd = (e: React.TouchEvent) => {
    if (touchStartX.current === null) return;
    const diff = e.changedTouches[0].clientX - touchStartX.current;
    if (Math.abs(diff) > 50) {
      if (diff > 0) goPrev();
      else goNext();
    }
    touchStartX.current = null;
  };

  return (
    <div
      className="mlb-overlay"
      onClick={onClose}
      onTouchStart={handleTouchStart}
      onTouchEnd={handleTouchEnd}
    >
      <div
        ref={dialogRef}
        className="mlb-dialog"
        role="dialog"
        aria-modal="true"
        aria-label="Media viewer"
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
      >
        <button className="mlb-close" onClick={onClose} aria-label="Close">
          ×
        </button>

        {/* LEFT — media */}
        <div className="mlb-media">
          {m ? (
            m.mime_type?.startsWith('video') ? (
              // Shared HLS-capable player (hls.js for playlists, native for
              // direct mp4) — the plain <video> can't play bsky playlists.
              <HlsVideo src={m.url} poster={m.thumbnail_url ?? undefined} className="mlb-video" />
            ) : (
              <img src={m.url} alt={m.alt_text || ''} className="mlb-img" draggable={false} />
            )
          ) : (
            <div className="mlb-img-empty">No media</div>
          )}
          {media.length > 1 && (
            <div className="mlb-nav">
              <button onClick={goPrev} disabled={index === 0} aria-label="Previous">
                ‹
              </button>
              <span>
                {index + 1} / {media.length}
              </span>
              <button onClick={goNext} disabled={index === media.length - 1} aria-label="Next">
                ›
              </button>
            </div>
          )}
        </div>

        {/* RIGHT — context header + the shared PostCard (body + interactions + comments) */}
        <div className="mlb-side">
          <div className="mlb-header">
            <div className="mlb-author">
              {post.author.avatar ? (
                <img src={post.author.avatar} alt="" className="mlb-avatar" />
              ) : (
                <span className="mlb-avatar mlb-avatar-fallback">
                  {(post.author.handle || '?').replace(/^@/, '')[0]?.toUpperCase() || '?'}
                </span>
              )}
              <span className="mlb-author-name">
                {post.display_name || post.author.display_name || post.author.handle}
              </span>
            </div>
            {group && (
              <div className="mlb-community">
                c/{group.name}@{group.instance}
              </div>
            )}
          </div>

          <div className="mlb-scroll">
            {/* The EXACT post-card controls: body (full, no clamp), the
                interaction row (❤️ like + 👍/👎 votes on group posts + emoji
                reactions + counts), and the comments thread + composer. */}
            <PostCard
              post={post}
              hideHeader
              hideMedia
              forceExpanded
              currentUserHandle={currentUserHandle}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
