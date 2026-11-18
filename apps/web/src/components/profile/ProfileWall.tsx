import { useState } from 'react';
import type { WallPost } from '../../utils/api';

interface Props {
  posts: WallPost[];
  hasMore: boolean;
  loading: boolean;
  onLoadMore: () => void;
  onPost: (content: string) => void;
  canPost: boolean;
}

function formatTime(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleDateString() + ' ' + d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

export default function ProfileWall({ posts = [], hasMore, loading, onLoadMore, onPost, canPost }: Props) {
  const [text, setText] = useState('');
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = () => {
    if (!text.trim()) return;
    setError(null);
    try {
      onPost(text.trim());
      setText('');
    } catch (e: any) {
      setError(e.message || 'Failed to post');
    }
  };

  return (
    <div className="profile-wall">
      <div className="profile-wall-header">
        <span className="profile-wall-title">📖 Sign My Guestbook!</span>
        <span className="profile-wall-counter">001,247 visitors</span>
      </div>

      {canPost && (
        <div className="profile-wall-input-area">
          <textarea
            className="profile-wall-input"
            placeholder="Write something nice..."
            value={text}
            onChange={e => setText(e.target.value)}
            rows={3}
          />
          {error && <div className="profile-wall-error">{error}</div>}
          <button className="profile-wall-submit" onClick={handleSubmit}>Submit Entry</button>
        </div>
      )}

      <div className="profile-wall-entries">
        {posts.length === 0 && (
          <div className="profile-wall-empty">No entries yet. Be the first to sign!</div>
        )}
        {posts && posts.map((post, idx) => (
          <div
            key={post.id}
            className={`profile-wall-entry ${idx % 2 === 0 ? 'even' : 'odd'}`}
          >
            <div className="profile-wall-entry-header">
              {post.author.avatar ? (
                <img src={post.author.avatar} alt="" className="profile-wall-avatar" />
              ) : (
                <div className="profile-wall-avatar-fallback">{post.author.handle[0]?.toUpperCase() || '?'}</div>
              )}
              <div className="profile-wall-entry-meta">
                <span className="profile-wall-author">{post.author.display_name || post.author.handle}</span>
                <span className="profile-wall-date">{formatTime(post.created_at)}</span>
              </div>
            </div>
            <div className="profile-wall-content">{post.content}</div>
          </div>
        ))}
      </div>

      {hasMore && (
        <div className="profile-wall-load-more">
          <button className="profile-wall-submit" onClick={onLoadMore} disabled={loading}>
            {loading ? 'Loading...' : 'Load more'}
          </button>
        </div>
      )}
    </div>
  );
}
