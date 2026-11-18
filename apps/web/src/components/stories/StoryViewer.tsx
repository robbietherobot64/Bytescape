import { useCallback, useEffect, useRef, useState } from 'react';
import type { CSSProperties, MutableRefObject } from 'react';
import { storiesApi, storyMediaUrl } from '../../utils/api';
import type { StoryComment, StoryGroup, StoryItem, StoryMediaItem } from '../../utils/api';

const REACTIONS = ['❤️', '🔥', '👍', '😂', '😮', '😢'];

function ageOf(iso: string): string {
  const mins = Math.max(0, Math.floor((Date.now() - new Date(iso).getTime()) / 60000));
  if (mins < 60) return `${mins}m`;
  const h = Math.floor(mins / 60);
  if (h < 24) return `${h}h`;
  return `${Math.floor(h / 24)}d`;
}

// Display/hold duration for a story item, in seconds.
function displaySecs(it: StoryItem): number {
  return it.media?.[0]?.duration || 5;
}

function StoryMedia({ authorDid, media, onEnded, videoRef, onTimeUpdate }: { authorDid: string; media: StoryMediaItem; onEnded?: () => void; videoRef: MutableRefObject<HTMLVideoElement | null>; onTimeUpdate?: (t: number, d: number) => void }) {
  const [muted, setMuted] = useState(true);
  const url = storyMediaUrl(authorDid, media);

  useEffect(() => {
    const el = videoRef.current;
    if (!el) return;
    // Kick playback explicitly: for large relayed videos Chrome defers the
    // autoplay attribute well past canplay, leaving the story frozen on the
    // first frame ("never loads"). play() starts the moment data allows —
    // the element is still muted here, so autoplay policy permits it.
    const kick = () => el.play().catch(() => {});
    if (url.endsWith('.m3u8')) {
      import('hls.js').then(({ default: Hls }) => {
        if (Hls.isSupported()) {
          const hls = new Hls();
          hls.loadSource(url);
          hls.attachMedia(el);
          el.addEventListener('loadeddata', kick, { once: true });
          return () => hls.destroy();
        }
        el.src = url;
        kick();
      });
    } else {
      el.src = url;
      kick();
    }
  }, [url]);

  if (media.type === 'video') {
    return (
      <>
        <video
          ref={videoRef}
          className="story-media"
          autoPlay
          preload="auto"
          muted={muted}
          playsInline
          onEnded={onEnded}
          onTimeUpdate={(e) => onTimeUpdate?.(e.currentTarget.currentTime, e.currentTarget.duration || 0)}
          onPlay={() => {
            // The viewer opens on a tap, so unmuted playback is allowed in
            // the gesture context. If the browser refuses, the toggle below
            // still gives manual control.
            const el = videoRef.current;
            if (el && el.muted) {
              el.muted = false;
              setMuted(false);
            }
          }}
        />
        <button
          className="story-sound-toggle"
          title={muted ? 'Unmute' : 'Mute'}
          onClick={(e) => {
            e.stopPropagation();
            const el = videoRef.current;
            if (!el) return;
            el.muted = !el.muted;
            setMuted(el.muted);
          }}
        >
          {muted ? '\u{1F507}' : '\u{1F50A}'}
        </button>
      </>
    );
  }
  return <img className="story-media" src={url} alt={media.alt || ''} />;
}

function StatusCardBody({ item }: { item: StoryItem }) {
  const bg = item.style?.bg;
  const style: CSSProperties = bg?.image
    ? { backgroundImage: `url(${bg.image})`, backgroundSize: 'cover', backgroundPosition: 'center' }
    : { background: bg?.color || 'linear-gradient(135deg, #0a2a0a, #003311)' };
  return (
    <div className="story-status-card" style={style}>
      {item.is_live ? (
        <div className="story-status-text">🔴 Live · started {ageOf(item.created_at)} ago</div>
      ) : (
        <div className="story-status-text">{item.text || '…'}</div>
      )}
      {item.is_live && item.text && item.text !== 'Live now' && (
        <div className="story-status-title">{item.text}</div>
      )}
      {item.music_url && (item.music_url.includes('soundcloud.com') || item.music_url.includes('bandcamp.com')) && (
        item.music_url.includes('soundcloud.com') ? (
          <iframe
            className="story-music-player"
            title="music"
            width="100%"
            height="80"
            scrolling="no"
            frameBorder="no"
            src={`https://w.soundcloud.com/player/?url=${encodeURIComponent(item.music_url)}&color=%2300ff41`}
            onClick={(e) => e.stopPropagation()}
          />
        ) : (
          <a
            className="story-music-link"
            href={item.music_url}
            target="_blank"
            rel="noreferrer"
            onClick={(e) => e.stopPropagation()}
          >
            ♪ Listen on Bandcamp
          </a>
        )
      )}
      {item.music_url
        && !item.music_url.includes('soundcloud.com')
        && !item.music_url.includes('bandcamp.com') && (
        <a
          className="story-music-link"
          href={item.music_url}
          target="_blank"
          rel="noreferrer"
          onClick={(e) => e.stopPropagation()}
        >
          ▶ Watch on {hostOf(item.music_url)}
        </a>
      )}
    </div>
  );
}

function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, '');
  } catch {
    return 'stream';
  }
}

export default function StoryViewer({
  groups,
  startGroup,
  startItem = 0,
  onClose,
}: {
  groups: StoryGroup[];
  startGroup: number;
  startItem?: number;
  onClose: () => void;
}) {
  const [gi, setGi] = useState(startGroup);
  const [ii, setIi] = useState(startItem);
  const [showComments, setShowComments] = useState(false);
  const [comments, setComments] = useState<StoryComment[] | null>(null);
  const [commentText, setCommentText] = useState('');
  const [localCounts, setLocalCounts] = useState<Record<string, { like: number; repost: number; reply: number }>>({});
  const [ownReactions, setOwnReactions] = useState<Record<string, string>>({});
  const timerRef = useRef<number | null>(null);
  const mediaRef = useRef<HTMLVideoElement | null>(null);
  const [mt, setMt] = useState(0);
  const [md, setMd] = useState(0);

  const handleTime = useCallback((t: number, d: number) => {
    setMt(t);
    setMd(d);
  }, []);

  // Clicking the story (media or letterbox) pauses/resumes.
  const togglePlayPause = () => {
    const el = mediaRef.current;
    if (!el) return;
    if (el.paused) el.play().catch(() => {});
    else el.pause();
  };

  // Clicking the tracker seeks within the current video item; clicking
  // another item's segment jumps to that item.
  const scrubTo = (e: React.MouseEvent<HTMLDivElement>, idx: number) => {
    if (idx !== ii) {
      setIi(idx);
      return;
    }
    const el = mediaRef.current;
    if (!el || !Number.isFinite(el.duration) || el.duration <= 0) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const frac = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
    el.currentTime = frac * el.duration;
  };

  const group = groups[gi];
  const item: StoryItem | undefined = group?.items[ii];

  const countsFor = (uri: string) => {
    const l = localCounts[uri];
    return {
      like: (item && item.uri === uri ? item.like_count : 0) + (l?.like ?? 0),
      repost: (item && item.uri === uri ? item.repost_count : 0) + (l?.repost ?? 0),
      reply: (item && item.uri === uri ? item.reply_count : 0) + (l?.reply ?? 0),
    };
  };

  const next = useCallback(() => {
    setShowComments(false);
    if (!group) return;
    if (ii < group.items.length - 1) setIi(ii + 1);
    else if (gi < groups.length - 1) {
      setGi(gi + 1);
      setIi(0);
    } else onClose();
  }, [gi, ii, group, groups.length, onClose]);

  const prev = useCallback(() => {
    setShowComments(false);
    if (ii > 0) setIi(ii - 1);
    else if (gi > 0) {
      const p = gi - 1;
      setGi(p);
      setIi(Math.max(0, groups[p].items.length - 1));
    }
  }, [gi, ii, groups]);

  // Auto-advance: per-item display duration (ms/1000) or default 5s; pauses
  // while comments are open. Status cards never auto-advance: they carry
  // reading/link content and the timer would close the viewer mid-click.
  useEffect(() => {
    if (!item || showComments) return;
    if (item.kind === 'status') return;
    // Videos advance when playback finishes (StoryMedia onEnded); images
    // and text hold for their duration or 5s.
    if (item.media?.[0]?.type === 'video') return;
    const secs = displaySecs(item);
    timerRef.current = window.setTimeout(next, secs * 1000);
    return () => {
      if (timerRef.current) window.clearTimeout(timerRef.current);
    };
  }, [item, showComments, next]);

  // Mark seen as items play; reset the tracker per item.
  useEffect(() => {
    setMt(0);
    setMd(0);
    if (item) storiesApi.seen(item.uri).catch(() => {});
  }, [item]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
      if (e.key === 'ArrowRight') next();
      if (e.key === 'ArrowLeft') prev();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [next, prev, onClose]);

  if (!group || !item) return null;
  const counts = countsFor(item.uri);
  const authorName = group.author.display_name || group.author.handle;

  const react = async (emoji: string) => {
    if (!item.can_react) return;
    try {
      const res = await storiesApi.react(item.uri, emoji);
      const total = Object.values(res.reaction_counts || {}).reduce((a, b) => a + b, 0);
      setLocalCounts((m) => ({ ...m, [item.uri]: { like: total - item.like_count, repost: 0, reply: 0 } }));
      setOwnReactions((m) => ({ ...m, [item.uri]: res.user_reaction || '' }));
    } catch {
      /* keep viewing */
    }
  };

  const openComments = async () => {
    if (!item.can_reply) return;
    setShowComments(true);
    if (comments === null) {
      try {
        setComments(await storiesApi.comments(item.uri));
      } catch {
        setComments([]);
      }
    }
  };

  const sendComment = async () => {
    const text = commentText.trim();
    if (!text || !item) return;
    setCommentText('');
    try {
      await storiesApi.comment(item.uri, text);
      setComments(await storiesApi.comments(item.uri));
      setLocalCounts((m) => ({
        ...m,
        [item.uri]: {
          like: m[item.uri]?.like ?? 0,
          repost: m[item.uri]?.repost ?? 0,
          reply: (m[item.uri]?.reply ?? 0) + 1,
        },
      }));
    } catch {
      /* keep viewing */
    }
  };

  const reshare = async () => {
    try {
      await storiesApi.reshare(item.uri);
      setLocalCounts((m) => ({
        ...m,
        [item.uri]: {
          like: m[item.uri]?.like ?? 0,
          repost: (m[item.uri]?.repost ?? 0) + 1,
          reply: m[item.uri]?.reply ?? 0,
        },
      }));
    } catch {
      /* keep viewing */
    }
  };

  return (
    <div
      className="story-viewer-overlay"
      role="dialog"
      aria-label="Stories"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="story-viewer">
        <div className="story-progress-row">
          {group.items.map((it, idx) => {
            const isVideo = it.media?.[0]?.type === 'video';
            const fill =
              idx < ii
                ? { width: '100%' }
                : idx === ii
                  ? isVideo
                    ? { width: `${md > 0 ? Math.min(100, (mt / md) * 100) : 0}%`, animation: 'none' }
                    : { animationDuration: `${displaySecs(it)}s` }
                  : undefined;
            return (
              <div
                key={it.uri}
                className={`story-progress-seg${idx < ii ? ' done' : idx === ii ? ' active' : ''}`}
                onClick={(e) => scrubTo(e, idx)}
              >
                <div className="story-progress-fill" style={fill} />
              </div>
            );
          })}
        </div>

        <div className="story-viewer-header">
          <div
            className="story-viewer-author"
            role="button"
            style={{ cursor: 'pointer' }}
            onClick={(e) => {
              e.stopPropagation();
              window.dispatchEvent(new CustomEvent('bytescape:viewProfile', {
                detail: { handle: group.author.handle, source: 'bytescape' },
              }));
            }}
          >
            {group.author.avatar ? (
              <img src={group.author.avatar} alt="" className="story-viewer-avatar" />
            ) : (
              <span className="story-viewer-avatar story-viewer-avatar-fallback">
                {authorName[1]?.toUpperCase() || '?'}
              </span>
            )}
            <span className="story-viewer-name">{authorName}</span>
            <span className="story-viewer-age">{ageOf(item.created_at)}</span>
            {item.pinned && <span className="story-pin-badge" title="Pinned">📌</span>}
          </div>
          <button className="btn story-viewer-close" onClick={onClose} title="Close (Esc)">×</button>
        </div>

        <div className="story-viewer-body" onClick={togglePlayPause}>
          {item.kind === 'status' ? (
            <StatusCardBody item={item} />
          ) : (
            item.media?.[0] && <StoryMedia authorDid={item.author_did} media={item.media[0]} onEnded={next} videoRef={mediaRef} onTimeUpdate={handleTime} />
          )}
          {item.text && item.kind === 'story' && <div className="story-caption">{item.text}</div>}
        </div>

        <div className="story-viewer-footer">
          <div className="story-counts">
            {counts.like > 0 && <span>❤️ {counts.like}</span>}
            {counts.repost > 0 && <span>🔁 {counts.repost}</span>}
            {counts.reply > 0 && <span>💬 {counts.reply}</span>}
          </div>
          <div className="story-actions">
            <div className="story-reaction-row">
              {REACTIONS.map((e) => (
                <button
                  key={e}
                  className={`btn story-react-btn${ownReactions[item.uri] === e ? ' active' : ''}`}
                  onClick={() => react(e)}
                  disabled={!item.can_react}
                  title="React"
                >
                  {e}
                </button>
              ))}
            </div>
            <button className="btn" onClick={openComments} disabled={!item.can_reply} title="Comments">
              💬
            </button>
            <button className="btn" onClick={reshare} title="Reshare">
              🔁
            </button>
          </div>
        </div>

        {showComments && (
          <div className="story-comments-panel" onClick={(e) => e.stopPropagation()}>
            <div className="story-comments-list">
              {(comments || []).length === 0 && <div className="story-comments-empty">No comments yet.</div>}
              {(comments || []).map((c, idx) => (
                <div key={idx} className="story-comment">
                  <span className="story-comment-author">{c.author_handle}</span> {c.content}
                </div>
              ))}
            </div>
            <div className="story-comment-compose">
              <input
                className="story-comment-input"
                value={commentText}
                placeholder="Add a comment…"
                onChange={(e) => setCommentText(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') sendComment();
                  e.stopPropagation();
                }}
              />
              <button className="btn" onClick={sendComment}>Send</button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
