import { useState, useEffect } from 'react';
import type { WallPost } from '../../../utils/api';
import { profileApi } from '../../../utils/api';
import ProfileWall from '../ProfileWall';

interface Props {
  username: string;
  isOwn: boolean;
  isFollowing: boolean;
}

export default function WidgetWall({ username, isOwn, isFollowing }: Props) {
  const [posts, setPosts] = useState<WallPost[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [offset, setOffset] = useState(0);

  const loadWall = async (reset = false) => {
    const off = reset ? 0 : offset;
    setLoading(true);
    try {
      const data = await profileApi.wall(username, 50, off);
      if (reset) {
        setPosts(data.items);
        setOffset(data.items.length);
      } else {
        setPosts(prev => [...prev, ...data.items]);
        setOffset(prev => prev + data.items.length);
      }
      setHasMore(data.has_more);
    } catch {
      setHasMore(false);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadWall(true);
  }, [username]);

  const handlePost = async (content: string) => {
    const post = await profileApi.postWall(username, content);
    setPosts(prev => [post, ...prev]);
  };

  return (
    <ProfileWall
      posts={posts}
      hasMore={hasMore}
      loading={loading}
      onLoadMore={() => loadWall(false)}
      onPost={handlePost}
      canPost={isOwn || isFollowing}
    />
  );
}
