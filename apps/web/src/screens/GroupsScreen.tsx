import { useCallback, useEffect, useState } from 'react';
import FeedColumn from '../components/FeedColumn';
import { SourcesPanel } from '@bytescape/events-ui';
import type { SourceCategory } from '@bytescape/events-ui';
import {
  feedApi,
  groupsApi,
  subscriptionApi,
  type FeedPost,
  type Subscription,
} from '../utils/api';

/**
 * Groups view — a specialized version of the main feed (owner model): same
 * layout as Feed/Events (standard tab row up top, reading column, standard
 * sources rail on the right), filtered to group-linked posts. No stories.
 *
 * Search is ONLY the global search bar (upper right) — it already searches
 * groups and deep-links a result to the CommunityScreen. No other search UI in
 * the groups section (owner round 3).
 *
 * Population is INTEREST-BASED (design D6): the tab scopes map to one
 * parameterized backend query (`/api/feed/groups?scope=…`).
 */
const SCOPE_TABS: { id: string; icon: string; label: string }[] = [
  { id: 'home', icon: '🏠', label: 'Home' },
  { id: 'following', icon: '👥', label: 'Following' },
  { id: 'feeds', icon: '📡', label: 'Feeds' },
  { id: 'hashtags', icon: '#️⃣', label: 'Hashtags' },
  { id: 'subscriptions', icon: '⭐', label: 'Groups' },
];

export default function GroupsScreen({ currentUserHandle }: { currentUserHandle?: string }) {
  const [scope, setScope] = useState('home');
  const [posts, setPosts] = useState<FeedPost[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [subs, setSubs] = useState<Subscription[]>([]);

  const load = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const res = await feedApi.groupScope(scope, 50);
      setPosts(res.items || []);
    } catch {
      setError('Could not load group posts.');
      setPosts([]);
    } finally {
      setLoading(false);
    }
  }, [scope]);

  useEffect(() => {
    load();
  }, [load]);

  // Sources rail: the user's subscribed groups (source='ap_group') rendered as
  // a standard source section — toggleable/removed like any other source (D4).
  const loadSubs = useCallback(() => {
    subscriptionApi.list().then(setSubs).catch(() => setSubs([]));
  }, []);
  useEffect(loadSubs, [loadSubs]);

  const categories: SourceCategory[] = [
    {
      id: 'groups',
      label: 'Groups',
      icon: '👥',
      items: subs
        .filter((s) => s.category === 'groups')
        .map((s) => ({
          id: s.id,
          label: s.label,
          source_id: s.source_id,
          source_type: 'community',
          icon: s.icon,
        })),
    },
  ];
  const onScope = (_item: SourceCategory['items'][number], _categoryId: string) => {
    setScope('subscriptions');
  };
  const onDelete = async (_categoryId: string, itemId: string) => {
    const sub = subs.find((s) => s.id === itemId);
    if (sub) {
      await subscriptionApi.delete(sub.id).catch(() => {});
      await groupsApi.unfollow(sub.source_id).catch(() => {});
      loadSubs();
    }
  };

  return (
    <div className="feed-layout">
      <div
        className="feed-main"
        style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', minHeight: 0, overflow: 'hidden' }}
      >
        {/* Standard tab row — identical shape to Feed/Events. */}
        <div className="feed-tabstrip">
          {SCOPE_TABS.map((t) => (
            <button
              key={t.id}
              className={`btn feed-tab${scope === t.id ? ' active' : ''}`}
              onClick={() => setScope(t.id)}
            >
              {t.icon} {t.label}
            </button>
          ))}
        </div>

        {/* Reading column — the standard FeedColumn (renders GroupPostCard for
            group posts, PostCard otherwise). */}
        <FeedColumn
          posts={posts}
          loading={loading}
          error={error}
          feedMode="combined"
          viewMode="feed"
          loadingMore={false}
          hasMore={false}
          onRetry={load}
          sortMode="new"
          onSortChange={() => {}}
        />
      </div>

      {/* Standard sources rail — groups as toggleable sources (D4). NO search
          box here: the ONLY search is the top-right global bar (owner round 4). */}
      <div className="sources-column">
        <SourcesPanel
          categories={categories}
          onScope={onScope}
          onDelete={onDelete}
        />
      </div>
    </div>
  );
}
