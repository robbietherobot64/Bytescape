import { useCallback, useEffect, useRef, useState } from 'react';
import MediaLightbox from '../MediaLightbox';
import { profileApi, storiesApi } from '../../utils/api';
import type { FeedPost, StoryGroup } from '../../utils/api';

export default function StoriesRail({
  onOpenGroup,
  onOpenComposer,
  onExplore,
}: {
  onOpenGroup: (groups: StoryGroup[], index: number) => void;
  onOpenComposer: () => void;
  onExplore: () => void;
}) {
  const [groups, setGroups] = useState<StoryGroup[] | null>(null);
  const [videos, setVideos] = useState<FeedPost[]>([]);
  const [videoIdx, setVideoIdx] = useState<number | null>(null);
  const [ownGroup, setOwnGroup] = useState<StoryGroup | null>(null);
  const [failed, setFailed] = useState(false);
  const scrollerRef = useRef<HTMLDivElement>(null);

  const load = useCallback(() => {
    storiesApi
      .rail()
      .then((r) => {
        setGroups(r.groups);
        // Federated video moments (Loops/Pixelfed/Mastodon video posts in the
        // viewer's interest/hashtag scope) — the ring's population source.
        // Narrowed at the wire boundary (utils/api.ts's typed boundary is
        // another lane's file this pass).
        const raw: unknown = r;
        const vids: FeedPost[] =
          raw && typeof raw === 'object' && 'videos' in raw && Array.isArray(raw.videos)
            ? raw.videos
            : [];
        setVideos(vids);
      })
      .catch(() => setFailed(true));
    Promise.all([storiesApi.mine(), profileApi.me().catch(() => null)])
      .then(([r, prof]) => {
        const items = r.items.filter((i) => i.kind === 'story' || i.state !== 'archived');
        if (items.length > 0) {
          const me = items[0].author_did;
          setOwnGroup({
            author: {
              did: me,
              handle: prof?.username || 'You',
              display_name: prof?.display_name || 'Your story',
              avatar: prof?.avatar_url || null,
            },
            seen: true,
            items,
          });
        } else {
          setOwnGroup(null);
        }
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  // Remount via key refreshes after publishing; expose reload for parents.
  useEffect(() => {
    const refresh = () => load();
    window.addEventListener('bytescape:refreshStories', refresh);
    return () => window.removeEventListener('bytescape:refreshStories', refresh);
  }, [load]);

  const scrollBy = (dx: number) => scrollerRef.current?.scrollBy({ left: dx, behavior: 'smooth' });

  const all: StoryGroup[] = ownGroup ? [ownGroup, ...(groups || [])] : groups || [];
  const empty = !failed && all.length === 0 && videos.length === 0;

  // Scrolling activates only once real content overflows the available space.
  const [scrollable, setScrollable] = useState(false);
  useEffect(() => {
    const el = scrollerRef.current;
    if (!el || empty) return;
    const update = () => {
      setScrollable(el.scrollWidth > el.clientWidth + 2);
    };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, [all.length, videos.length, ownGroup, empty]);

  return (
    <div className={`stories-rail${empty ? ' stories-rail-empty' : ''}`}>
      {empty && (
        <>
          <button className="btn stories-share-btn" onClick={onOpenComposer}>
            + Share a moment
          </button>
          <span className="stories-empty-hint">
            No stories or video moments in your interests yet — follow hashtags or people who
            post video, and this rail fills up.
          </span>
        </>
      )}
      {!empty && (
        <>
          {scrollable && (
            <button className="stories-arrow stories-arrow-left" onClick={() => scrollBy(-320)} aria-label="Scroll stories left">
              ‹
            </button>
          )}
          <div className="stories-rail-scroller" ref={scrollerRef}>
            <button
              className="story-ring story-ring-explore"
              onClick={onExplore}
              title="Explore stories"
            >
              <span className="story-ring-avatar story-ring-avatar-fallback">✦</span>
              <span className="story-ring-name">Explore</span>
            </button>
            <div className="story-ring story-ring-own">
              <button
                className="story-ring-btn"
                onClick={() => (ownGroup ? onOpenGroup(all, 0) : onOpenComposer())}
                title={ownGroup ? 'View your story' : 'Share a moment'}
              >
                {ownGroup ? (
                  ownGroup.author.avatar ? (
                    <img src={ownGroup.author.avatar} alt="" className="story-ring-avatar" />
                  ) : (
                    <span className="story-ring-avatar story-ring-avatar-fallback">Y</span>
                  )
                ) : (
                  <span className="story-ring-avatar story-ring-add">+</span>
                )}
                <span className="story-ring-name">Your story</span>
              </button>
              <button className="story-ring-compose" onClick={onOpenComposer} title="Add to your story">
                +
              </button>
            </div>
            {(ownGroup ? all.slice(1) : all).map((g, idx) => {
              const name = g.author.display_name || g.author.handle;
              const gi = ownGroup ? idx + 1 : idx;
              return (
                <button
                  key={g.author.did}
                  className={`story-ring${g.seen ? ' story-ring-seen' : ''}`}
                  onClick={() => onOpenGroup(all, gi)}
                  title={name}
                >
                  {g.author.avatar ? (
                    <img src={g.author.avatar} alt="" className="story-ring-avatar" />
                  ) : (
                    <span className="story-ring-avatar story-ring-avatar-fallback">
                      {name[1]?.toUpperCase() || '?'}
                    </span>
                  )}
                  <span className="story-ring-name">{name}</span>
                </button>
              );
            })}
            {videos.map((p, vi) => {
              // Loops-style ring: circular video preview (muted loop), tap to
              // fullscreen through the shared MediaLightbox video path.
              const name = p.author.display_name || p.author.handle;
              const vm = p.media.find((m) => (m.mime_type || '').startsWith('video'));
              const direct = !!vm && /\.(mp4|webm|mov|m4v)(\?|$)/i.test(vm.url);
              return (
                <button
                  key={p.id}
                  className="story-ring story-ring-video"
                  onClick={() => setVideoIdx(vi)}
                  title={name}
                >
                  {vm && direct ? (
                    <video
                      className="story-ring-avatar"
                      src={vm.url}
                      poster={vm.thumbnail_url || undefined}
                      autoPlay
                      muted
                      loop
                      playsInline
                      preload="metadata"
                      style={{ objectFit: 'cover' }}
                    />
                  ) : vm?.thumbnail_url ? (
                    <img
                      src={vm.thumbnail_url}
                      alt=""
                      className="story-ring-avatar"
                      style={{ objectFit: 'cover' }}
                    />
                  ) : p.author.avatar ? (
                    <img src={p.author.avatar} alt="" className="story-ring-avatar" />
                  ) : (
                    <span className="story-ring-avatar story-ring-avatar-fallback">
                      {name[1]?.toUpperCase() || '?'}
                    </span>
                  )}
                  <span className="story-ring-name">{name}</span>
                </button>
              );
            })}
          </div>
          {scrollable && (
            <button className="stories-arrow stories-arrow-right" onClick={() => scrollBy(320)} aria-label="Scroll stories right">
              ›
            </button>
          )}
        </>
      )}
      {videoIdx !== null && videos[videoIdx] && (
        <MediaLightbox
          post={videos[videoIdx]}
          startIndex={0}
          onClose={() => setVideoIdx(null)}
        />
      )}
    </div>
  );
}
