import { useState } from 'react';
import PostCard from './PostCard';
import PostChips from './PostChips';
import { groupsApi, type FeedPost, type GroupRef } from '../utils/api';

/**
 * Group post card (Lemmy/PieFed community post over ActivityPub) — the group
 * chrome around the shared `<PostCard>`.
 *
 * Header (INSIDE the post, owner spec): the post title on top; directly under
 * it, right-aligned, the community handle rendered canonically as
 * `c/{name}@{instance}` (parsed from the actor id) — clickable to the
 * Bytescape-native community page — immediately next to a Follow/Subscribed
 * button (live state). Nothing community-related renders outside the card or
 * glued to other strings. Votes render as 👍/👎 reaction chips through the
 * standard PostCard reaction row; body/media/reactions/comments come from
 * `<PostCard>`.
 */

/** Canonical `c/{name}@{instance}` from a community actor id
 *  (`https://instance/c/name`). Falls back to the raw actor id path when the
 *  expected shape isn't present. */
function canonicalHandle(actorId: string): string {
  try {
    const u = new URL(actorId);
    const instance = u.host;
    const m = u.pathname.match(/\/[cm]\/([^/]+)/);
    const name = m ? m[1] : u.pathname.split('/').filter(Boolean).pop() || '';
    return `c/${name}@${instance}`;
  } catch {
    return actorId;
  }
}

export default function GroupPostCard({
  post,
  group,
  currentUserHandle,
  onFollowChange,
}: {
  post: FeedPost;
  group: GroupRef;
  currentUserHandle?: string;
  onFollowChange?: (following: boolean) => void;
}) {
  const [following, setFollowing] = useState(false);
  const [followBusy, setFollowBusy] = useState(false);

  // Post title = first content line (Lemmy/PieFed title+body join). Falls back
  // to the community title/name when the post has no title line.
  const title = post.content.split('\n')[0].trim() || group.title || group.name;

  const toggleFollow = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (followBusy) return;
    setFollowBusy(true);
    try {
      const resp = following
        ? await groupsApi.unfollow(group.actor_id)
        : await groupsApi.follow(group.actor_id, group.title || group.name);
      setFollowing(resp.following);
      onFollowChange?.(resp.following);
    } catch {
      /* best-effort follow */
    } finally {
      setFollowBusy(false);
    }
  };

  const openCommunity = (e: React.MouseEvent) => {
    e.stopPropagation();
    window.dispatchEvent(
      new CustomEvent('bytescape:viewCommunity', { detail: { actorId: group.actor_id } }),
    );
  };

  return (
    <div className="group-post-card">
      {/* Header inside the post: title on top; community handle + follow
          right-aligned directly under it. */}
      <div className="group-post-header">
        <a
          className="group-post-title group-link"
          href={post.url || '#'}
          target="_blank"
          rel="noopener noreferrer"
          onClick={(e) => e.stopPropagation()}
          title="Open the post on its home instance"
        >
          {title}
        </a>
        {/* Provenance chips — where this group post came from (following /
            feeds / hashtags / subscriptions), computed server-side per scope. */}
        <PostChips
          sources={post.matched_sources || []}
          onOpenSource={(m) => {
            window.dispatchEvent(new CustomEvent('bytescape:openSourceView', { detail: m }));
          }}
        />
        <div className="group-post-community-row">
          <button className="group-community-handle group-link" onClick={openCommunity} title="Open community">
            {canonicalHandle(group.actor_id)}
          </button>
          <button
            className={`group-follow-btn${following ? ' following' : ''}`}
            onClick={toggleFollow}
            disabled={followBusy}
            title={following ? 'Unfollow this group' : 'Follow this group'}
          >
            {following ? '✓ Subscribed' : '+ Follow'}
          </button>
        </div>
      </div>

      {/* Body + reactions (incl. 👍/👎 vote chips) + comments — the shared
          PostCard (its own header suppressed; the group header above replaces it). */}
      <PostCard post={post} hideHeader currentUserHandle={currentUserHandle} />
    </div>
  );
}
