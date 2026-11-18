import { useCallback, useEffect, useState } from 'react';
import GroupPostCard from '../components/GroupPostCard';
import {
  groupsApi,
  type FeedPost,
  type GroupView,
  type GroupRef,
} from '../utils/api';

/**
 * Community page — a Bytescape-native home for one group (Lemmy/PieFed
 * community over ActivityPub). Header (name, title, description, instance,
 * kind, nsfw) + Follow control + stats + the group's posts. Not a mirror of
 * the remote community — a Bytescape page over the group registry.
 *
 * Opened from the group post card's community name (`bytescape:viewCommunity`).
 */
export default function CommunityScreen({
  actorId,
  currentUserHandle,
  onBack,
}: {
  actorId: string;
  currentUserHandle?: string;
  onBack: () => void;
}) {
  const [group, setGroup] = useState<GroupView | null>(null);
  const [posts, setPosts] = useState<FeedPost[]>([]);
  const [loading, setLoading] = useState(false);
  const [following, setFollowing] = useState(false);
  const [followBusy, setFollowBusy] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [g, p] = await Promise.all([
        groupsApi.get(actorId),
        groupsApi.posts(actorId, 50),
      ]);
      setGroup(g);
      setFollowing(g.following);
      setPosts(p.items);
    } catch {
      setGroup(null);
      setPosts([]);
    } finally {
      setLoading(false);
    }
  }, [actorId]);

  useEffect(() => {
    load();
  }, [load]);

  const toggleFollow = async () => {
    if (followBusy || !group) return;
    setFollowBusy(true);
    try {
      const resp = following
        ? await groupsApi.unfollow(group.actor_id)
        : await groupsApi.follow(group.actor_id, group.title || group.name);
      setFollowing(resp.following);
    } catch {
      /* best-effort */
    } finally {
      setFollowBusy(false);
    }
  };

  if (!group) {
    return (
      <div className="feed-column">
        <div className="feed-column-header">
          <button className="btn" onClick={onBack}>← Back</button>
          <h1>COMMUNITY</h1>
          <p>{loading ? 'Loading…' : 'Community not found.'}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="feed-layout">
      <div className="feed-main" style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', minHeight: 0, overflow: 'hidden' }}>
        <div className="feed-tabstrip">
          <button className="btn feed-tab" onClick={onBack}>← Back</button>
          <button className="btn feed-tab active">🏘️ {group.title || group.name}</button>
        </div>

        {/* Community header — identity + stats + Follow. */}
        <div className="community-header">
          <div className="community-header-main">
            <h2 className="community-title">
              {group.title || group.name}
              {group.nsfw && <span className="group-post-nsfw" title="NSFW community">NSFW</span>}
            </h2>
            <div className="community-sub">c/{group.name}@{group.instance} · {group.kind}</div>
            {group.description && <p className="community-desc">{group.description}</p>}
            <div className="community-stats">
              <span className="community-stat">{group.post_count} posts</span>
            </div>
          </div>
          <button
            className={`btn group-follow-btn${following ? ' following' : ''}`}
            onClick={toggleFollow}
            disabled={followBusy}
          >
            {following ? '✓ Following' : '+ Follow'}
          </button>
        </div>

        {/* The community's posts — GroupPostCard via the shared reading column. */}
        <div className="feed-column" style={{ flex: 1, overflowY: 'auto', padding: '8px 0' }}>
          {loading && <div className="feed-loading">Loading posts…</div>}
          {!loading && posts.length === 0 && (
            <div className="empty-state" style={{ padding: 24, color: 'var(--text-muted)' }}>
              No posts indexed for this community yet.
            </div>
          )}
          {posts.map((p) => {
            const g = (p.group ?? group) as GroupRef;
            return (
              <GroupPostCard
                key={p.id}
                post={p}
                group={g}
                currentUserHandle={currentUserHandle}
                onFollowChange={setFollowing}
              />
            );
          })}
        </div>
      </div>
      {/* Right rail kept empty to mirror the feed's three-column shell. */}
      <div className="sources-column" />
    </div>
  );
}
