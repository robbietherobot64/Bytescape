import { useState, useEffect } from 'react';
import type { FeedPost } from '../../../utils/api';
import { profileApi } from '../../../utils/api';
import ProfilePosts from '../ProfilePosts';

interface Props {
  username: string;
}

export default function WidgetPosts({ username }: Props) {
  const [posts, setPosts] = useState<FeedPost[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [offset, setOffset] = useState(0);

  const loadPosts = async (reset = false) => {
    const off = reset ? 0 : offset;
    setLoading(true);
    try {
      const data = await profileApi.feed(username, 20, off);
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
    loadPosts(true);
  }, [username]);

  return (
    <ProfilePosts
      posts={posts}
      hasMore={hasMore}
      loading={loading}
      onLoadMore={() => loadPosts(false)}
    />
  );
}
