/**
 * Phosphor skeleton cards for the feed content area while the first page is
 * in flight. Painted inside the normal feed chrome (header, toolbar, action
 * bar) so a load never shows a blank column; mirrors the PostCard silhouette
 * (avatar + handle + text lines + one media block) so the swap to real cards
 * is near-invisible. Pure presentational — no data, no loading flags.
 */
export default function FeedSkeleton({ count = 3 }: { count?: number }) {
  return (
    <div className="feed-skeleton" aria-hidden="true">
      {Array.from({ length: count }, (_, i) => (
        <div className="feed-skeleton-card" key={i}>
          <div className="feed-skeleton-head">
            <div className="feed-skeleton-avatar" />
            <div className="feed-skeleton-meta">
              <div className="feed-skeleton-line" style={{ width: '42%' }} />
              <div className="feed-skeleton-line short" />
            </div>
          </div>
          <div className="feed-skeleton-line" />
          <div className="feed-skeleton-line" style={{ width: '92%' }} />
          <div className="feed-skeleton-line" style={{ width: '74%' }} />
          {i === 0 && <div className="feed-skeleton-media" />}
        </div>
      ))}
    </div>
  );
}
