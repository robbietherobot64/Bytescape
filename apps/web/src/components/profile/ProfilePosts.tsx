import PostCard from '../PostCard';
import type { FeedPost } from '../../utils/api';
import { currentUserHandle } from '../../utils/api';

interface Props {
  posts: FeedPost[];
  hasMore: boolean;
  loading: boolean;
  onLoadMore: () => void;
}

export default function ProfilePosts({ posts = [], hasMore, loading, onLoadMore }: Props) {
  const me = currentUserHandle();
  return (
    <div className="profile-posts">
      {loading && (!posts || posts.length === 0) ? (
        <div className="profile-posts-loading">Loading posts...</div>
      ) : !posts || posts.length === 0 ? (
        <div className="profile-posts-empty">
          <div className="profile-posts-empty-icon">📭</div>
          <div className="profile-posts-empty-title">No posts yet</div>
          <div className="profile-posts-empty-desc">
            Posts you create or that are federated to your account will appear here.
          </div>
        </div>
      ) : (
        <>
          <div className="post-list">
            {posts.map(p => <PostCard key={p.id} post={p} currentUserHandle={me || undefined} />)}
          </div>
          {hasMore && (
            <div className="profile-posts-load-more">
              <button className="profile-wall-submit" onClick={onLoadMore} disabled={loading}>
                {loading ? 'Loading...' : 'Load more'}
              </button>
            </div>
          )}
        </>
      )}
    </div>
  );
}
