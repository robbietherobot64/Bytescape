import { useState, useEffect, useCallback, useRef } from 'react';
import Hls from 'hls.js';
import type { EmbedInfo, FeedPost, MediaAttachment } from '../utils/api';
import { isStoryQuoteUri, api, feedApi, groupsApi, muteApi, interactionFailedToast } from '../utils/api';
import StoryPreviewCard from './stories/StoryPreviewCard';
import PostChips from './PostChips';
import SafeMarkdown from './SafeMarkdown';
import MediaLightbox from './MediaLightbox';
import AuthorHoverCard from './AuthorHoverCard';

const API = window.location.hostname === 'localhost' ? 'http://localhost:8080' : '';

export interface CommentItem {
  id: string;
  author_handle: string;
  author_avatar: string | null;
  content: string;
  created_at: string;
  parent_id: string | null;
  media?: MediaAttachment[];
  embeds?: EmbedInfo[];
  likes_count: number;
  author_url?: string | null;
  author_following?: boolean;
  author_resolved?: boolean;
}

function normalizeSource(source: string): string {
  return source.split('|')[0] || source;
}

/**
 * CR-8: when a reaction fails because the account's Mastodon grant is stale,
 * the server answers 403 with a "re-link" message naming the fix. Surface that
 * message verbatim (styled error toast) instead of the generic interaction
 * failure literal; returns null for any other failure.
 */
export function reactionErrorToastMessage(e: unknown): string | null {
  let msg = '';
  if (e && typeof e === 'object' && 'message' in e) {
    msg = String(e.message ?? '');
  }
  return /re-link/i.test(msg) ? msg : null;
}

/**
 * The instruction-focused re-link prompt (CR-8; identity-graph Phase 2 for
 * follows): the server names the fix ("re-link your Mastodon account…"), we
 * show it verbatim as a styled toast. Shared by reactions and follow actions.
 */
export function relinkToast(message: string) {
  const toast = document.createElement('div');
  toast.textContent = message;
  toast.style.cssText = 'position:fixed;bottom:20px;left:50%;transform:translateX(-50%);background:#c0392b;color:#fff;padding:10px 20px;border-radius:8px;z-index:9999;font-size:14px;';
  document.body.appendChild(toast);
  setTimeout(() => toast.remove(), 6000);
}

/** Server error envelope `{ error, message }` → the message to act on. */
async function serverErrorMessage(res: Response, fallback: string): Promise<string> {
  try {
    const j: unknown = await res.json();
    if (j && typeof j === 'object') {
      if ('message' in j && typeof j.message === 'string') return j.message;
      if ('error' in j && typeof j.error === 'string') return j.error;
    }
  } catch {}
  return fallback;
}

function CommentFollow({ handle, source, following }: { handle: string; source: string; following: boolean }) {
  const [isFollowing, setIsFollowing] = useState(following);
  const [busy, setBusy] = useState(false);
  const toggle = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (busy) return;
    setBusy(true);
    const prev = isFollowing;
    setIsFollowing(!prev);
    try {
      const token = localStorage.getItem('bytescape_token');
      const res = await fetch(`${API}/api/feed/${prev ? 'unfollow' : 'follow'}`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${token}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ handle, source: normalizeSource(source) }),
      });
      if (!res.ok) {
        throw new Error(await serverErrorMessage(res, 'follow failed'));
      }
    } catch (e) {
      setIsFollowing(prev);
      const relink = reactionErrorToastMessage(e);
      if (relink) relinkToast(relink);
    } finally {
      setBusy(false);
    }
  };
  return (
    <button
      className={`comment-follow${isFollowing ? ' following' : ''}`}
      onClick={toggle}
      disabled={busy}
    >
      {isFollowing ? '✓ Following' : '+ Follow'}
    </button>
  );
}

function FollowButton({ post }: { post: FeedPost }) {
  const [status, setStatus] = useState<'none' | 'pending' | 'following'>(
    post.follow_pending ? 'pending' : post.is_following ? 'following' : 'none'
  );
  const [loading, setLoading] = useState(false);

  const handleFollow = async () => {
    const prev = status;
    setStatus('pending');
    setLoading(true);
    try {
      const token = localStorage.getItem('bytescape_token');
      const res = await fetch(`${API}/api/feed/follow`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${token}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          handle: post.author.handle,
          source: normalizeSource(post.source),
        }),
      });
      if (!res.ok) {
        throw new Error(await serverErrorMessage(res, 'follow failed'));
      }
      const data = await res.json();
      setStatus(data.status === 'already_following' ? 'following' : data.status || 'pending');
    } catch (e) {
      setStatus(prev);
      const relink = reactionErrorToastMessage(e);
      if (relink) relinkToast(relink);
    } finally {
      setLoading(false);
    }
  };

  const handleUnfollow = async () => {
    const prev = status;
    setStatus('none');
    setLoading(true);
    try {
      const token = localStorage.getItem('bytescape_token');
      const res = await fetch(`${API}/api/feed/unfollow`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${token}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          handle: post.author.handle,
          source: normalizeSource(post.source),
        }),
      });
      if (!res.ok) {
        throw new Error(await serverErrorMessage(res, 'unfollow failed'));
      }
      const data = await res.json();
      setStatus(data.status || 'none');
    } catch (e) {
      setStatus(prev);
      const relink = reactionErrorToastMessage(e);
      if (relink) relinkToast(relink);
    } finally {
      setLoading(false);
    }
  };

  const baseBtnStyle = {
    borderRadius: 4,
    padding: '2px 8px',
    fontSize: 11,
    cursor: loading ? 'wait' as const : 'pointer' as const,
    whiteSpace: 'nowrap' as const,
    border: 'none',
  };

  if (status === 'following') {
    return (
      <button
        className="post-follow-btn"
        onClick={(e) => { e.stopPropagation(); handleUnfollow(); }}
        disabled={loading}
        style={{
          ...baseBtnStyle,
          background: 'var(--surface-hover, #333)',
          color: 'var(--text-secondary, #999)',
        }}
      >
        {loading ? '...' : 'Following'}
      </button>
    );
  }

  if (status === 'pending') {
    return (
      <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
        <span style={{ fontSize: 11, color: 'var(--text-secondary, #999)' }}>Pending...</span>
        <button
          className="post-follow-btn"
          onClick={(e) => { e.stopPropagation(); handleUnfollow(); }}
          disabled={loading}
          style={{
            ...baseBtnStyle,
            background: 'none',
            border: '1px solid var(--border-color, #444)',
            color: 'var(--text-secondary, #999)',
          }}
        >
          {loading ? '...' : 'Cancel Request'}
        </button>
      </span>
    );
  }

  return (
    <button
      className="post-follow-btn"
      onClick={(e) => { e.stopPropagation(); handleFollow(); }}
      disabled={loading}
      style={{
        ...baseBtnStyle,
        background: 'none',
        border: '1px solid var(--accent-color, #4caf50)',
        color: 'var(--accent-color, #4caf50)',
      }}
    >
      {loading ? '...' : '+ Follow'}
    </button>
  );
}

export default function PostCard({ post, currentUserHandle, hideHeader, hideMedia, forceExpanded }: { post: FeedPost; currentUserHandle?: string; hideHeader?: boolean; hideMedia?: boolean; forceExpanded?: boolean }) {
  const [nsfwRevealed, setNsfwRevealed] = useState(false);
  const time = formatTime(post.created_at);
  const isLong = post.content_type === 'long';
  // Use backend is_own field if available, otherwise fall back to handle comparison
  const isOwnPost = post.is_own ?? (() => {
    const ownHandle = currentUserHandle ? currentUserHandle.replace(/^@+/, '').toLowerCase() : null;
    const postHandle = post.author.handle ? post.author.handle.replace(/^@+/, '').toLowerCase() : null;
    return ownHandle && postHandle && (
      ownHandle === postHandle ||
      postHandle.startsWith(ownHandle + '@') ||
      ownHandle.startsWith(postHandle + '@')
    );
  })();
  const hasYouTube = post.embeds.some(e => e.kind === 'youtube' || isYouTubeUrl(e.url))
    || isYouTubeUrl(post.content);
  const hasSoundCloud = post.embeds.some(e => e.kind === 'soundcloud' || isSoundCloudUrl(e.url))
    || isSoundCloudUrl(post.content);
  const hasBandcamp = post.embeds.some(e => e.kind === 'bandcamp' || isBandcampUrl(e.url))
    || isBandcampUrl(post.content);
  const hasLinkEmbeds = post.embeds.some(e => !['youtube', 'soundcloud', 'bandcamp', 'quote'].includes(e.kind));
  const [lightboxIndex, setLightboxIndex] = useState<number | null>(null);
  const [expanded, setExpanded] = useState(forceExpanded ?? false);
  // Expandable body (item 6): clamp long bodies to a few lines with a fade +
  // "More" control; expanded shows clean full text (no fade). Overflow is
  // measured so short posts never show the control.
  const [bodyExpanded, setBodyExpanded] = useState(forceExpanded ?? false);
  const [bodyClamped, setBodyClamped] = useState(false);
  const bodyInnerRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    const el = bodyInnerRef.current;
    if (!el) return;
    // Compare full scroll height against the collapsed clamp height (5 lines).
    const clampPx = 5 * 21; // line-height 1.5 * 14px ≈ 21px per line
    setBodyClamped(el.scrollHeight > clampPx + 4);
  }, [post.content]);
  const [comments, setComments] = useState<CommentItem[] | null>(null);
  const [loadingComments, setLoadingComments] = useState(false);
  const [reacting, setReacting] = useState(false);
  const [selectedEmoji, setSelectedEmoji] = useState(post.user_reaction || '');
  const [likeDelta, setLikeDelta] = useState(0);
  const [localCounts, setLocalCounts] = useState<Record<string, number>>(post.reaction_counts || {});
  const [pickerOpen, setPickerOpen] = useState(false);
  const [resharing, setResharing] = useState(false);
  const [shareOpen, setShareOpen] = useState(false);
  const [shareComment, setShareComment] = useState('');
  const [shareSuccess, setShareSuccess] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [replyContent, setReplyContent] = useState('');
  const [replySubmitting, setReplySubmitting] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [muting, setMuting] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const EMOJIS = ['❤️', '👍', '🔥', '😄', '😂', '😮', '😢', '😡', '👎'];

  const isLemmy = post.source.startsWith('lemmy');

  // Close menu on outside click
  useEffect(() => {
    if (!menuOpen) return;
    const handler = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [menuOpen]);

  // Extract instance base from the post's URL (always a valid absolute URL)
  let lemmyBase = '';
  if (isLemmy && post.url) {
    try {
      const u = new URL(post.url);
      lemmyBase = `${u.protocol}//${u.host}`;
    } catch {}
  }
  if (!lemmyBase) lemmyBase = post.source.split('|')[1] || '';
  if (lemmyBase && !lemmyBase.startsWith('http')) lemmyBase = `https://${lemmyBase}`;


  let lemmyTitle = '';
  let communityName = '';
  let communityActorId = '';
  let bodyContent = post.content;
  if (isLemmy) {
    const lines = post.content.split('\n');
    lemmyTitle = lines[0] || '';
    const metaLine = lines[1] || '';
    communityName = metaLine.match(/c\/(\S+)/)?.[1] || '';
    bodyContent = lines.slice(2).join('\n');
    // Parse actor_id from thread_id for proper cross-instance links
    if (post.thread_id) {
      const parts = post.thread_id.split('|');
      if (parts.length > 3) communityActorId = parts[3];
    }
  }
  const communityUrl = communityActorId || (communityName ? `${lemmyBase}/c/${communityName}` : '#');
  const cleanHandle = (post.author.handle || 'unknown').replace(/^@+/, '').trim();
  const userUrl = cleanHandle ? `${lemmyBase}/u/${cleanHandle}` : '#';

  // The last reaction state the SERVER confirmed for this post. Echoed back as
  // `seen_user_reaction` so a repeat click or stale client can never flip the
  // toggle the wrong way server-side, and rendered from the response so the UI
  // never shows state the server didn't confirm.
  const lastServerReaction = useRef(post.user_reaction || '');

  const handleReaction = async (emoji: string) => {
    if (reacting) return;
    setReacting(true);
    try {
      // Group posts: 👍/👎 are VOTES (owner model — votes ARE reactions). A
      // click dispatches the federated AP Like/Dislike via the proven
      // ui_like/ui_dislike path; a repeat click toggles it off (Undo). The
      // standard reaction chip row renders the counts (fed by
      // `reaction_counts`, which folds the federated upvote/downvote counts).
      if (post.group && (emoji === '👍' || emoji === '👎')) {
        const my = post.my_vote;
        const want = emoji === '👍' ? 'up' : 'down';
        const vote = my === want ? 'clear' : want;
        const resp = await groupsApi.vote(post.id, vote as 'up' | 'down' | 'clear');
        // Reflect the server's standing vote + counts in the chip row.
        const next = { ...(post.reaction_counts || {}) };
        if (resp.upvotes > 0) next['👍'] = resp.upvotes; else delete next['👍'];
        if (resp.downvotes > 0) next['👎'] = resp.downvotes; else delete next['👎'];
        setLocalCounts(next);
        post.my_vote = resp.my_vote;
        return;
      }
      const source = isLemmy ? 'lemmy' : post.source;
      // api() auto-refreshes an expired token, so a refreshable 401 recovers
      // instead of surfacing a misleading "re-link" error.
      const data = await api<{ reaction_counts?: Record<string, number>; user_reaction?: string | null; undo_queued?: boolean | null }>(`/api/feed/reaction`, {
        method: 'POST',
        body: JSON.stringify({ post_id: post.id, source, reaction: emoji, url: post.url, cid: post.cid, seen_user_reaction: lastServerReaction.current }),
      });
      // Server truth, never the optimistic flip: `user_reaction` is what now
      // actually stands (a repeat click / stale unlike resolves to a no-op).
      const wasHeart = lastServerReaction.current === '❤️';
      const now = data.user_reaction ?? '';
      lastServerReaction.current = now;
      setSelectedEmoji(now);
      const nowHeart = now === '❤️';
      setLikeDelta(d => d + (nowHeart ? 1 : 0) - (wasHeart ? 1 : 0));
      if (data.reaction_counts) setLocalCounts(data.reaction_counts);
      // An un-like that dispatched nothing federated (no original to resolve):
      // say so instead of silently pretending. Local-only emojis never
      // federate, so `undo_queued: false` is expected for them.
      if (data.undo_queued === false && !['😢', '😡', '👎'].includes(emoji)) {
        const toast = document.createElement('div');
        toast.textContent = 'Removed here, but nothing could be sent to the fediverse — the post may still show your like remotely.';
        toast.style.cssText = 'position:fixed;bottom:20px;left:50%;transform:translateX(-50%);background:#333;color:#fff;padding:10px 20px;border-radius:8px;z-index:9999;font-size:14px;';
        document.body.appendChild(toast);
        setTimeout(() => toast.remove(), 6000);
      }
    } catch (e) {
      const relink = reactionErrorToastMessage(e);
      if (relink) {
        relinkToast(relink);
      } else {
        interactionFailedToast();
      }
    }
    finally { setReacting(false); }
  };

  const fetchComments = async () => {
    if (loadingComments) return;
    setLoadingComments(true);
    try {
      const source = post.source.startsWith('lemmy') ? 'lemmy' : post.source.startsWith('bluesky') ? 'bluesky' : post.source.startsWith('mastodon') ? 'mastodon' : post.source;
      const postId = post.source.startsWith('lemmy') ? post.id.replace('lemmy:', '') : post.id;
      // api() auto-refreshes an expired token (the raw-fetch version silently
      // rendered "No comments yet." whenever the 15-minute access token had
      // expired — a thread that loaded fine on a fresh session "never loaded"
      // on a stale one).
      const items = await api<CommentItem[]>(`/api/feed/comments?post_id=${encodeURIComponent(postId)}&source=${source}`);
      setComments(items);
      // "Render fast with placeholder, then hydrate": if any author handle
      // is a did-derived placeholder (backend spawned an async real-handle
      // resolve), re-pull the thread shortly so the real handle replaces it.
      if (items.some(c => c.author_resolved === false)) {
        window.setTimeout(() => { fetchComments(); }, 2500);
      }
    } catch {
      setComments([]);
    } finally {
      setLoadingComments(false);
    }
  };

  const handleExpand = async () => {
    // The lightbox renders this card permanently expanded (owner bug: one
    // stray click on the body — text select, scroll drag, anything bubbling
    // to the article — collapsed the thread and the comments "vanished" with
    // no visible way back). forceExpanded = never toggle: the thread loads on
    // mount and stays.
    if (forceExpanded) return;
    if (expanded) { setExpanded(false); return; }
    setExpanded(true);
    if (comments !== null) return;
    await fetchComments();
  };

  // The media lightbox renders this card expanded (full body + thread). The
  // card's click-to-expand is intercepted in that context, so auto-load the
  // comment thread on mount (owner: "comments load and thread exactly like the
  // card").
  useEffect(() => {
    if (forceExpanded && comments === null) {
      fetchComments();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [forceExpanded]);

  const handleShare = async () => {
    if (resharing) return;
    setResharing(true);
    setShareSuccess(false);
    try {
      await api(`/api/posts/repost`, {
        method: 'POST',
        body: JSON.stringify({ post_id: post.id, source: post.source, url: post.url, comment: shareComment.trim() || undefined }),
      });
      setShareSuccess(true);
      setTimeout(() => { setShareOpen(false); setShareComment(''); setShareSuccess(false); }, 1200);
      window.dispatchEvent(new CustomEvent('bytescape:refreshFeed'));
    } catch {
      interactionFailedToast();
    }
    finally { setResharing(false); }
  };

  const handleDelete = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (deleting) return;
    if (!confirm('Delete this post?')) return;
    setDeleting(true);
    try {
      const token = localStorage.getItem('bytescape_token');
      const res = await fetch(`${API}/api/posts/${encodeURIComponent(post.id)}`, {
        method: 'DELETE',
        headers: { Authorization: `Bearer ${token}` },
      });
      const data = await res.json().catch(() => null);
      if (data?.deleted === true) {
        window.dispatchEvent(new CustomEvent('bytescape:refreshFeed'));
      } else if (data?.deleted === false) {
        const errMsg = Array.isArray(data.errors) ? data.errors.join('\n') : 'Remote deletion failed';
        alert(`Failed to delete:\n${errMsg}`);
      } else {
        alert('Failed to delete: unexpected server response');
      }
    } catch {
      alert('Failed to delete: network error');
    } finally { setDeleting(false); }
  };

  const handleReply = async () => {
    if (replySubmitting || !replyContent.trim()) return;
    setReplySubmitting(true);
    try {
      const source = isLemmy ? 'lemmy' : post.source;
      const postId = isLemmy ? post.id.replace('lemmy:', '') : post.id;
      await feedApi.reply(postId, source, replyContent.trim());
      setReplyContent('');
      await fetchComments();
    } catch {
      interactionFailedToast();
    }
    finally { setReplySubmitting(false); }
  };

  const handleMute = async () => {
    if (muting) return;
    setMuting(true);
    setMenuOpen(false);
    try {
      const did = post.author_external_id || null;
      const handle = post.author.handle?.replace(/^@+/, '') || null;
      await muteApi.mute(did, handle, 'mute');
      // Optimistic: remove post from feed
      window.dispatchEvent(new CustomEvent('bytescape:refreshFeed'));
      // Show toast
      const toast = document.createElement('div');
      toast.textContent = `Muted ${post.author.handle}`;
      toast.style.cssText = 'position:fixed;bottom:20px;left:50%;transform:translateX(-50%);background:#333;color:#fff;padding:10px 20px;border-radius:8px;z-index:9999;font-size:14px;';
      document.body.appendChild(toast);
      setTimeout(() => toast.remove(), 2500);
    } catch (e: any) {
      const toast = document.createElement('div');
      toast.textContent = `Failed to mute: ${e.message || 'unknown error'}`;
      toast.style.cssText = 'position:fixed;bottom:20px;left:50%;transform:translateX(-50%);background:#c0392b;color:#fff;padding:10px 20px;border-radius:8px;z-index:9999;font-size:14px;';
      document.body.appendChild(toast);
      setTimeout(() => toast.remove(), 3000);
    } finally { setMuting(false); }
  };

  return (
    <article className={`post-card ${expanded ? 'post-card-expanded' : ''}`} onClick={handleExpand} style={{ position: 'relative' }}>
      {post.nsfw && !nsfwRevealed && (
        <button
          className="nsfw-reveal"
          onClick={(e) => { e.stopPropagation(); setNsfwRevealed(true); }}
          style={{
            position: 'absolute', inset: 0, zIndex: 10, width: '100%',
            background: 'rgba(0,0,0,0.55)', color: '#fff', border: 'none',
            fontSize: 15, fontWeight: 700, cursor: 'pointer',
          }}
        >
          ⚠️ NSFW — click to reveal
        </button>
      )}
      <div style={{ filter: post.nsfw && !nsfwRevealed ? 'blur(22px)' : 'none' }}>
      {post.is_reshare && (
        <div className="repost-banner">
          🔁 Reshared from{' '}
          <AuthorHoverCard
            handle={post.original_post?.author?.handle || 'unknown'}
            source={post.source}
            displayName={post.original_post?.author?.display_name}
            avatarUrl={post.original_post?.author?.avatar}
            isFollowing={post.original_post?.is_following}
          >
            <span style={{ textDecoration: 'underline', textUnderlineOffset: 2 }}>
              {(() => { const h = post.original_post?.author?.handle || 'unknown'; return h.startsWith('@') ? h : `@${h}`; })()}
            </span>
          </AuthorHoverCard>
        </div>
      )}
      {post.reposted_by && (
        <div className="repost-banner">
          🔁 Reposted by{' '}
          <AuthorHoverCard handle={post.reposted_by} source={post.source}>
            <span style={{ textDecoration: 'underline', textUnderlineOffset: 2 }}>
              {post.reposted_by.startsWith('@') ? post.reposted_by : `@${post.reposted_by}`}
            </span>
          </AuthorHoverCard>
        </div>
      )}
      {post.boosted_by && (
        <div className="boost-banner">
          🔄 Boosted by{' '}
          <AuthorHoverCard handle={post.boosted_by} source={post.source}>
            <span style={{ textDecoration: 'underline', textUnderlineOffset: 2 }}>
              {post.boosted_by.startsWith('@') ? post.boosted_by : `@${post.boosted_by}`}
            </span>
          </AuthorHoverCard>
        </div>
      )}

      {/* Delete button for own posts */}

      {!hideHeader && (isLemmy ? (
        <div className="post-header-lemmy">
          <div className="lemmy-title">{lemmyTitle}</div>
          <div className="lemmy-meta">
            <a className="lemmy-user-chip"
               href={userUrl}
               target="_blank" rel="noopener noreferrer"
               onClick={e => e.stopPropagation()}>
              @{cleanHandle}
            </a>
            {communityName && (
              <a className="lemmy-community-chip"
                 href={communityUrl}
                 target="_blank" rel="noopener noreferrer"
                 onClick={e => e.stopPropagation()}>
                in c/{communityName}
              </a>
            )}
            <span className="post-time">{time}</span>
          </div>
        </div>
      ) : (
        <div className="post-header">
          <div className="post-avatar">
            {post.author.avatar ? (
              <img src={post.author.avatar} alt="" className="post-avatar-img" />
            ) : (
              <span className="post-avatar-fallback">{post.author.handle[1]?.toUpperCase() || '?'}</span>
            )}
          </div>
          <div className="post-author-info post-author-clickable"
               onClick={(e) => {
                 e.stopPropagation();
                 if (post.author_url) {
                   window.open(post.author_url, '_blank');
                 } else {
                   window.dispatchEvent(new CustomEvent('bytescape:viewProfile', {
                     detail: { handle: post.author.handle, source: post.source }
                   }));
                 }
               }}
               style={{ cursor: 'pointer' }}>
            <span className="post-author-name">
              {post.display_name || post.author.display_name || post.author.handle}
            </span>
            <span className="post-author-handle">{post.author.handle}</span>
            <PostChips
              sources={post.matched_sources || []}
              onOpenSource={(m) => {
                window.dispatchEvent(new CustomEvent('bytescape:openSourceView', { detail: m }));
              }}
            />
            {post.action_text && (() => {
              // Archive imports carry action_text "via {Source} archive" —
              // render those as a distinct archive badge so they don't read as
              // native Bytescape posts.
              const m = /^via (.+) archive$/i.exec(post.action_text);
              if (m) {
                return (
                  <span className="post-archive-chip" title={`Imported from a ${m[1]} data archive`}>
                    📦 {m[1]} archive
                  </span>
                );
              }
              return <span className="post-action-chip">{post.action_text}</span>;
            })()}
            {post.group_name && (
              <span className="post-group-chip">Posted in {post.group_name}</span>
            )}
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            {!isOwnPost && post.author.handle && (
              <FollowButton post={post} />
            )}
            {/* Three-dot menu */}
            {!isOwnPost && (
              <div ref={menuRef} style={{ position: 'relative' }} onClick={e => e.stopPropagation()}>
                <button
                  onClick={() => setMenuOpen(!menuOpen)}
                  style={{
                    background: 'none', border: 'none', cursor: 'pointer',
                    color: 'var(--text-secondary, #999)', fontSize: 18, padding: '2px 6px',
                    borderRadius: 4, lineHeight: 1,
                  }}
                  title="More options"
                >
                  ⋯
                </button>
                {menuOpen && (
                  <div style={{
                    position: 'absolute', right: 0, top: '100%', zIndex: 100,
                    background: 'var(--bg-primary, #1a1a1a)', border: '1px solid var(--border-color, #333)',
                    borderRadius: 8, minWidth: 160, boxShadow: '0 4px 12px rgba(0,0,0,0.4)',
                    overflow: 'hidden',
                  }}>
                    <button
                      onClick={handleMute}
                      disabled={muting}
                      style={{
                        display: 'block', width: '100%', padding: '10px 14px',
                        background: 'none', border: 'none', cursor: muting ? 'wait' : 'pointer',
                        color: 'var(--text-primary, #fff)', fontSize: 13, textAlign: 'left',
                      }}
                      onMouseEnter={(e) => (e.currentTarget.style.background = 'var(--surface-hover, #333)')}
                      onMouseLeave={(e) => (e.currentTarget.style.background = 'none')}
                    >
                      🔇 {muting ? 'Muting...' : `Mute ${post.author.handle}`}
                    </button>
                    <button
                      onClick={() => { setMenuOpen(false); alert('Report coming soon'); }}
                      style={{
                        display: 'block', width: '100%', padding: '10px 14px',
                        background: 'none', border: 'none', cursor: 'pointer',
                        color: 'var(--text-secondary, #999)', fontSize: 13, textAlign: 'left',
                        borderTop: '1px solid var(--border-color, #333)',
                      }}
                      onMouseEnter={(e) => (e.currentTarget.style.background = 'var(--surface-hover, #333)')}
                      onMouseLeave={(e) => (e.currentTarget.style.background = 'none')}
                    >
                      🚩 Report
                    </button>
                  </div>
                )}
              </div>
            )}
            <span className="post-time">{time}</span>
          </div>
        </div>
      ))}

      {!hideMedia && (
      <div className="post-media-block">
      {/* ═══ MEDIA + EMBEDS — single unified rendering pipeline ═══ */}
      {(() => {
        const videoItems = post.media.filter(m => m.mime_type?.startsWith('video'));
        const imageItems = post.media.filter(m => !m.mime_type?.startsWith('video'));

        // Collect all known URLs to avoid duplicates
        const knownUrls = new Set<string>();
        post.media.forEach(m => knownUrls.add(m.url));
        post.embeds.forEach(e => knownUrls.add(e.url));

        // Extract URLs from content ONLY if the post has no media and no embeds
        // (Bluesky/Mastodon posts already have proper embeds — don't double-render)
        const shouldExtractContentUrls = post.media.length === 0 && post.embeds.length === 0;
        const contentUrls = shouldExtractContentUrls ? extractUrls(post.content) : [];
        const contentImageUrls = contentUrls.filter(u => /\.(gif|png|jpg|jpeg|webp)(\?|$)/i.test(u));
        const contentYouTubeUrls = contentUrls.filter(u => isYouTubeUrl(u));
        const contentLinkUrls = contentUrls.filter(u =>
          !isYouTubeUrl(u) && !/\.(gif|png|jpg|jpeg|webp)(\?|$)/i.test(u)
        );

        // YouTube embeds from embeds[]
        const ytEmbeds = post.embeds.filter(e => e.kind === 'youtube' || isYouTubeUrl(e.url));
        // Link embeds from embeds[] (exclude youtube, soundcloud, bandcamp, quote)
        const linkEmbeds = post.embeds.filter(e => !['youtube', 'soundcloud', 'bandcamp', 'quote'].includes(e.kind));
        // Quote embeds
        const quoteEmbeds = post.embeds.filter(e => e.kind === 'quote');

        return (
          <>
          {/* 1. Videos from media[] — click opens the shared MediaLightbox */}
          {videoItems.map((m, i) => (
            <div
              key={`vid-${i}`}
              className="post-media"
              style={{ padding: '0 14px 10px', cursor: 'pointer' }}
              onClick={(e) => {
                e.stopPropagation();
                const fullIdx = post.media.findIndex((x) => x.url === m.url);
                setLightboxIndex(fullIdx >= 0 ? fullIdx : 0);
              }}
            >
              <HlsVideo src={m.url} poster={m.thumbnail_url} />
            </div>
          ))}

          {/* 2. Images from media[] — carousel style: single cover, arrows to browse */}
          {imageItems.length > 0 && (
            <ImageCarousel
              images={imageItems}
              onExpand={(idx) => {
                const fullIdx = post.media.findIndex((x) => x.url === imageItems[idx]?.url);
                setLightboxIndex(fullIdx >= 0 ? fullIdx : 0);
              }}
            />
          )}

          {/* 3. YouTube embeds */}
          {ytEmbeds.map((e, i) => (
            <div key={`yt-${i}`} className="post-embed">
              <YouTubeEmbed url={e.url} />
            </div>
          ))}
          {/* YouTube from content (only if no embeds) */}
          {contentYouTubeUrls.map((url, i) => (
            <div key={`ct-yt-${i}`} className="post-embed">
              <YouTubeEmbed url={url} />
            </div>
          ))}

          {/* 4. SoundCloud */}
          {hasSoundCloud && post.embeds.filter(e => e.kind === 'soundcloud' || isSoundCloudUrl(e.url)).map((e, i) => (
            <div key={`sc-${i}`} className="post-embed">
              <iframe src={`https://w.soundcloud.com/player/?url=${encodeURIComponent(e.url)}&color=%2300ff41&auto_play=false`}
                      className="soundcloud-embed" allow="autoplay" loading="lazy" />
            </div>
          ))}

          {/* 5. Bandcamp */}
          {hasBandcamp && post.embeds.filter(e => e.kind === 'bandcamp' || isBandcampUrl(e.url)).map((e, i) => (
            <div key={`bc-${i}`} className="post-embed">
              <BandcampEmbed url={e.url} />
            </div>
          ))}

          {/* Link cards from embeds[] — exclude YouTube (already rendered above) */}
          {linkEmbeds.filter(e => !isYouTubeUrl(e.url)).map((e, i) => (
            <LinkPreview key={`link-${i}`} url={e.url} title={e.title} description={e.description} thumbnail={e.thumbnail} />
          ))}

          {/* 7. Image URLs from content (only if no media/embeds) */}
          {contentImageUrls.map((url, i) => (
            <div key={`ct-img-${i}`} className="post-media" style={{ padding: '0 14px 10px' }}>
              <img src={url} alt="" className="post-media-img" loading="lazy" style={{ width: '100%', height: 'auto', borderRadius: '8px' }} />
            </div>
          ))}

          {/* 8. Link previews from content (only if no media/embeds) */}
          {contentLinkUrls.map((url, i) => (
            <LinkPreview key={`ct-link-${i}`} url={url} />
          ))}

          {/* 9. Quote posts (story reshares render as story-preview cards) */}
          {quoteEmbeds.map((e, i) => (
            isStoryQuoteUri(e.url) ? (
              <StoryPreviewCard
                key={`quote-${i}`}
                embeds={post.embeds}
                onOpenStory={(uri) => window.dispatchEvent(new CustomEvent('bytescape:openStory', { detail: uri }))}
              />
            ) : (
              <QuotePostEmbed key={`quote-${i}`} uri={e.url} />
            )
          ))}
          {post.quoted_post_uri && quoteEmbeds.length === 0 && (
            <QuotePostEmbed uri={post.quoted_post_uri} />
          )}
          </>
        );
      })()}
      </div>
      )}

      <div className="post-body">
        <div
          ref={bodyInnerRef}
          className={`post-body-inner${bodyClamped && !bodyExpanded && !forceExpanded ? ' post-body-clamped' : ''}`}
        >
          <SafeMarkdown text={bodyContent} />
        </div>
        {/* Fade overlay exists ONLY in the collapsed state (owner: no dim
            residue when expanded). */}
        {bodyClamped && !bodyExpanded && !forceExpanded && <div className="post-body-fade" />}
        {bodyClamped && !forceExpanded && (
          <button
            className="post-body-toggle"
            onClick={(e) => { e.stopPropagation(); setBodyExpanded((v) => !v); }}
          >
            {bodyExpanded ? 'Less' : 'More'}
          </button>
        )}
        {post.is_reshare && post.original_post && (
          <div className="repost-embed">
            <div className="repost-embed-header">
              <span className="repost-icon">🔁</span>
              <AuthorHoverCard
                handle={post.original_post.author?.handle || 'unknown'}
                source={post.source}
                displayName={post.original_post.author?.display_name}
                avatarUrl={post.original_post.author?.avatar}
                isFollowing={post.original_post.is_following}
              >
                <span className="repost-author">{(() => { const h = post.original_post.author?.handle || 'unknown'; return h.startsWith('@') ? h : `@${h}`; })()}</span>
              </AuthorHoverCard>
            </div>
            <div className="repost-embed-content">{post.original_post.content}</div>
            {post.original_post.media && post.original_post.media.length > 0 && (
              <div className="repost-embed-media">
                {post.original_post.media.slice(0, 4).map((m, i) => (
                  m.mime_type?.startsWith('video') ? (
                    <video key={i} src={m.url} style={{ width: 80, height: 80, objectFit: 'cover', borderRadius: 6 }} muted />
                  ) : (
                    <img key={i} src={m.url} style={{ width: 80, height: 80, objectFit: 'cover', borderRadius: 6 }} loading="lazy" />
                  )
                ))}
              </div>
            )}
          </div>
        )}
      </div>

      {lightboxIndex !== null && post.media[lightboxIndex] && (
        <MediaLightbox
          post={post}
          group={post.group ?? null}
          startIndex={lightboxIndex}
          onClose={() => setLightboxIndex(null)}
          currentUserHandle={currentUserHandle}
        />
      )}

      <div className="post-actions">
        <div className="reaction-picker" style={{ position: 'relative' }} onClick={e => e.stopPropagation()}>
          <button
            className={`action-btn reaction-main ${selectedEmoji === '❤️' ? 'has-reaction' : ''}`}
            onClick={() => handleReaction('❤️')}
            disabled={reacting}
            title={selectedEmoji === '❤️' ? 'Unlike' : 'Like'}
          >
            ❤️
            <span className="action-count">{post.likes_count + likeDelta}</span>
          </button>
          {/* Small affordance for the full reaction picker */}
          <button
            className="action-btn reaction-badge"
            onClick={() => setPickerOpen(!pickerOpen)}
            disabled={reacting}
            title="Add reaction"
            style={{ fontSize: 12 }}
          >
            😊+
          </button>
          {/* Show other reacted emojis inline next to the heart */}
          {Object.entries(localCounts)
            .filter(([emoji]) => emoji !== '❤️')
            .sort((a, b) => b[1] - a[1])
            .map(([emoji, count]) => (
              <button
                key={emoji}
                className="action-btn reaction-badge"
                onClick={() => handleReaction(emoji)}
                disabled={reacting}
                title={`${emoji} reactions`}
              >
                {emoji} <span className="action-count">{count}</span>
              </button>
            ))}
          {pickerOpen && (
            <div className="reaction-popover">
              {EMOJIS.map(emoji => {
                const count = localCounts[emoji] || 0;
                return (
                  <button
                    key={emoji}
                    className={`reaction-popover-btn ${selectedEmoji === emoji ? 'selected' : ''}`}
                    onClick={() => { setPickerOpen(false); handleReaction(emoji); }}
                    disabled={reacting}
                    title={emoji}
                  >
                    <span className="popover-emoji">{emoji}</span>
                    {count > 0 && <span className="popover-count">{count}</span>}
                  </button>
                );
              })}
            </div>
          )}
        </div>
        <button className="action-btn" onClick={e => { e.stopPropagation(); setShareOpen(true); }} title="Share / Reshare">
          ↗ <span className="action-count">{post.shares_count}</span>
        </button>
        <span className="action-btn" style={{ cursor: 'pointer' }} title="Click post to view comments">↩ <span className="action-count">{post.replies_count}</span></span>
        {isOwnPost && (
          <button
            className="action-btn"
            onClick={handleDelete}
            disabled={deleting}
            title="Delete post"
            style={{ color: 'var(--danger)', marginLeft: 'auto' }}
          >
            🗑
          </button>
        )}
      </div>

      {shareOpen && (
        <div className="share-dialog-overlay" onClick={e => { e.stopPropagation(); setShareOpen(false); }} style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.6)', zIndex: 200, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
          <div className="share-dialog" onClick={e => e.stopPropagation()} style={{ background: 'var(--bg-primary)', border: '1px solid var(--border-color)', borderRadius: 12, width: 520, maxWidth: '92vw', maxHeight: '90vh', overflow: 'auto', padding: 20 }}>
            <h3 style={{ margin: '0 0 12px', fontFamily: 'var(--font-retro)', fontSize: 16 }}>Share Post</h3>
            {shareSuccess ? (
              <div style={{ padding: '24px 0', textAlign: 'center', color: 'var(--accent)', fontFamily: 'var(--font-retro)' }}>Shared! ✓</div>
            ) : (
              <>
                <textarea
                  className="input-field"
                  placeholder="Add a comment (optional)..."
                  value={shareComment}
                  onChange={e => setShareComment(e.target.value)}
                  rows={3}
                  style={{ width: '100%', resize: 'vertical', marginBottom: 12, fontSize: 14 }}
                />
                <div style={{ border: '1px solid var(--border-color)', borderRadius: 8, padding: 12, background: 'var(--bg-secondary)', marginBottom: 16, opacity: 0.9 }}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
                    {post.author?.avatar && <img src={post.author.avatar} alt="" style={{ width: 28, height: 28, borderRadius: '50%' }} />}
                    <strong style={{ fontSize: 13 }}>{post.author?.display_name || post.display_name}</strong>
                    <span style={{ color: 'var(--text-muted)', fontSize: 12 }}>@{post.author?.handle}</span>
                  </div>
                  <p style={{ whiteSpace: 'pre-wrap', margin: 0, fontSize: 13, color: 'var(--text-secondary)' }}>{post.content}</p>
                  {post.media && post.media.length > 0 && (
                    <div style={{ display: 'flex', gap: 6, marginTop: 8, flexWrap: 'wrap' }}>
                      {post.media.slice(0, 4).map((m, i) => (
                        <div key={i} style={{ width: 80, height: 80, borderRadius: 6, overflow: 'hidden', background: '#000', flexShrink: 0 }}>
                          {m.mime_type?.startsWith('video') ? <video src={m.url} style={{ width: '100%', height: '100%', objectFit: 'cover' }} /> : <img src={m.url} style={{ width: '100%', height: '100%', objectFit: 'cover' }} />}
                        </div>
                      ))}
                    </div>
                  )}
                </div>
                <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
                  <button className="btn" onClick={() => { setShareOpen(false); setShareComment(''); }}>Cancel</button>
                  <button className="btn btn-primary" onClick={handleShare} disabled={resharing}>
                    {resharing ? 'Sharing...' : 'Share'}
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}

      {expanded && (
        <div className="post-comments" onClick={e => e.stopPropagation()}>
          <h4 className="comments-title">Comments ({post.replies_count})</h4>
          {loadingComments ? (
            <p className="comments-loading">Loading...</p>
          ) : comments && comments.length > 0 ? (
            comments.map(c => (
              <div key={c.id} className="comment-item">
                <div className="comment-header">
                  {c.author_url ? (
                    <a
                      className="comment-author-link"
                      href={c.author_url}
                      target="_blank"
                      rel="noopener noreferrer"
                      onClick={(e) => e.stopPropagation()}
                    >
                      <strong>{c.author_handle}</strong>
                    </a>
                  ) : (
                    <strong>{c.author_handle}</strong>
                  )}
                  <CommentFollow
                    handle={c.author_handle}
                    source={post.source}
                    following={!!c.author_following}
                  />
                  <span className="comment-time">{formatTime(c.created_at)}</span>
                  <span className="comment-likes">♡ {c.likes_count}</span>
                </div>
                <div className="comment-body">{c.content}</div>
                {(() => {
                  // Comment embeds (images, link cards, quotes) — the API
                  // previously dropped these, leaving comments text-only.
                  const cMedia = c.media ?? [];
                  const cEmbeds = c.embeds ?? [];
                  const videoItems = cMedia.filter((m) => m.mime_type?.startsWith('video'));
                  const imageItems = cMedia.filter((m) => !m.mime_type?.startsWith('video'));
                  const linkEmbeds = cEmbeds.filter((e) => e.kind === 'link');
                  const quoteEmbeds = cEmbeds.filter((e) => e.kind === 'quote');
                  return (
                    <>
                      {videoItems.map((m, i) => (
                        <div key={`cv-${i}`} style={{ marginTop: 8 }}>
                          <HlsVideo src={m.url} poster={m.thumbnail_url} />
                        </div>
                      ))}
                      {imageItems.length > 0 && (
                        <div style={{ display: 'flex', gap: 4, marginTop: 8, flexWrap: 'wrap' }}>
                          {imageItems.slice(0, 4).map((m, i) => (
                            <img
                              key={`ci-${i}`}
                              src={m.thumbnail_url || m.url}
                              alt={m.alt_text || ''}
                              loading="lazy"
                              style={{
                                width: imageItems.length === 1 ? '100%' : '48%',
                                height: 'auto',
                                borderRadius: 6,
                                cursor: 'pointer',
                                objectFit: 'cover',
                                maxHeight: 260,
                              }}
                              onClick={() => window.open(m.url, '_blank', 'noopener')}
                            />
                          ))}
                        </div>
                      )}
                      {linkEmbeds.map((e, i) => (
                        <div key={`cl-${i}`} style={{ marginTop: 8 }}>
                          <LinkPreview url={e.url} title={e.title} description={e.description} thumbnail={e.thumbnail} />
                        </div>
                      ))}
                      {quoteEmbeds.map((e, i) => (
                        <div key={`cq-${i}`} style={{ marginTop: 8 }}>
                          <QuotePostEmbed uri={e.url} />
                        </div>
                      ))}
                    </>
                  );
                })()}
              </div>
            ))
          ) : (
            <p className="comments-empty">No comments yet.</p>
          )}
          <div className="comment-composer">
            <textarea
              placeholder="Write a reply..."
              value={replyContent}
              onChange={e => setReplyContent(e.target.value)}
              disabled={replySubmitting}
            />
            <button onClick={handleReply} disabled={replySubmitting || !replyContent.trim()}>
              {replySubmitting ? '...' : 'Reply'}
            </button>
          </div>
        </div>
      )}
      </div>
    </article>
  );
}

// ── Carousel Lightbox ────────────────────────────────────────────────────
export function YouTubeEmbed({ url }: { url: string }) {
  const id = extractYouTubeId(url);
  if (!id) return <a href={url} target="_blank" rel="noopener noreferrer">{url}</a>;
  return (
    <div className="youtube-wrapper">
      <iframe src={`https://www.youtube-nocookie.com/embed/${id}?rel=0`} className="youtube-embed"
        allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
        allowFullScreen loading="lazy" />
    </div>
  );
}

function BandcampEmbed({ url }: { url: string }) {
  const isTrack = url.includes('/track/');
  const isAlbum = url.includes('/album/');
  if (!isTrack && !isAlbum) return <a href={url} target="_blank" rel="noopener noreferrer">{url}</a>;
  return (
    <iframe src={`https://bandcamp.com/EmbeddedPlayer/${isTrack ? 'track' : 'album'}/size=large/bgcol=0a0a0a/linkcol=00ff41/tracklist=false/artwork=small/transparent=true/`}
      className="bandcamp-embed" seamless loading="lazy" />
  );
}

function extractYouTubeId(url: string): string | null {
  const patterns = [
    /(?:youtube\.com\/watch\?v=|youtu\.be\/|youtube\.com\/embed\/|music\.youtube\.com\/watch\?v=)([a-zA-Z0-9_-]{11})/,
    /youtube\.com\/shorts\/([a-zA-Z0-9_-]{11})/,
  ];
  for (const p of patterns) {
    const m = url.match(p);
    if (m) return m[1];
  }
  return null;
}

function isYouTubeUrl(url: string): boolean {
  return /youtube\.com|youtu\.be|music\.youtube\.com/.test(url);
}

function isSoundCloudUrl(url: string): boolean {
  return /soundcloud\.com/.test(url);
}

function isBandcampUrl(url: string): boolean {
  return /bandcamp\.com/.test(url);
}

function extractUrls(text: string): string[] {
  const urlRe = /https?:\/\/(?:www\.)?[-a-zA-Z0-9@:%._\+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b(?:[-a-zA-Z0-9()@:%_\+.~#?&//=]*)/g;
  return [...text.matchAll(urlRe)].map(m => m[0]);
}

function extractHandleFromUri(uri: string): string {
  try {
    const parts = uri.replace('at://', '').split('/');
    return parts[0] || uri;
  } catch {
    return uri;
  }
}

export function formatTime(iso: string): string {
  if (!iso) return '';
  try {
    const d = new Date(iso);
    const now = Date.now();
    const postTime = d.getTime();
    // If post is in the future (spam/scam with fake timestamps), show as "now"
    // but cap it so it doesn't show future dates as "now" forever
    const diff = now - postTime;
    if (diff < 0) {
      // Future-dated post — treat as "now" but don't show future dates
      return 'now';
    }
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return 'now';
    if (mins < 60) return `${mins}m`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h`;
    const days = Math.floor(hours / 24);
    if (days < 7) return `${days}d`;
    return d.toLocaleDateString();
  } catch { return ''; }
}

export function LinkPreview({ url, title, description, thumbnail }: { url: string; title?: string; description?: string; thumbnail?: string }) {
  const [ogData, setOgData] = useState<{ title?: string; description?: string; image?: string; site_name?: string } | null>(null);
  const [loading, setLoading] = useState(!title && !thumbnail);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (title || thumbnail) return; // Already have data
    let cancelled = false;
    const token = localStorage.getItem('bytescape_token');
    fetch(`${API}/api/og?url=${encodeURIComponent(url)}`, {
      headers: { Authorization: `Bearer ${token}` },
    })
      .then(r => r.ok ? r.json() : Promise.reject())
      .then(d => { if (!cancelled) { setOgData(d); setLoading(false); } })
      .catch(() => { if (!cancelled) { setFailed(true); setLoading(false); } });
    return () => { cancelled = true; };
  }, [url]);

  const displayTitle = title || ogData?.title || ogData?.site_name;
  const displayDesc = description || ogData?.description;
  const displayImg = thumbnail || ogData?.image;
  const domain = (() => { try { return new URL(url).hostname.replace('www.', ''); } catch { return url; } })();

  if (loading) {
    return (
      <a href={url} target="_blank" rel="noopener noreferrer" className="post-link-card" onClick={e => e.stopPropagation()}>
        <div className="link-card-body" style={{ padding: 16, opacity: 0.5 }}>
          <strong className="link-card-title">{domain}</strong>
          <p className="link-card-desc">Loading preview...</p>
        </div>
      </a>
    );
  }

  if (failed || (!displayTitle && !displayDesc && !displayImg)) {
    return (
      <a href={url} target="_blank" rel="noopener noreferrer" className="post-link-card" onClick={e => e.stopPropagation()}>
        <div className="link-card-body" style={{ padding: 14 }}>
          <strong className="link-card-title" style={{ fontSize: 13, opacity: 0.7 }}>{domain}</strong>
          <p className="link-card-desc" style={{ fontSize: 12, opacity: 0.5, margin: '4px 0 0' }}>{url}</p>
        </div>
      </a>
    );
  }

  return (
    <a href={url} target="_blank" rel="noopener noreferrer" className="post-link-card" onClick={e => e.stopPropagation()}>
      {displayImg && <img src={displayImg} alt="" className="link-card-thumb" loading="lazy" onError={(e) => { (e.target as HTMLImageElement).style.display = 'none'; }} />}
      <div className="link-card-body">
        {displayTitle && <strong className="link-card-title">{displayTitle}</strong>}
        {displayDesc && <p className="link-card-desc">{displayDesc}</p>}
        <span style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 4, display: 'block' }}>{domain}</span>
      </div>
    </a>
  );
}

// ── HLS Video Player (for Bluesky videos) ──
export function HlsVideo({
  src,
  poster,
  className,
  style,
}: {
  src: string;
  poster?: string | null;
  /** Overrides the default `post-media-video` class (e.g. the lightbox's `mlb-video`). */
  className?: string;
  /** Overrides the default sizing inline style; omit to use the class's CSS. */
  style?: React.CSSProperties;
}) {
  const videoRef = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    if (src.endsWith('.m3u8')) {
      if (Hls.isSupported()) {
        const hls = new Hls({ enableWorker: true });
        hls.loadSource(src);
        hls.attachMedia(video);
        return () => { hls.destroy(); };
      } else if (video.canPlayType('application/vnd.apple.mpegurl')) {
        video.src = src;
      }
    } else {
      video.src = src;
    }
  }, [src]);

  return (
    <video
      ref={videoRef}
      className={className ?? 'post-media-video'}
      autoPlay
      muted
      controls
      playsInline
      loop
      poster={poster || undefined}
      style={style ?? { width: '100%', maxHeight: '600px', objectFit: 'contain', borderRadius: '8px' }}
    />
  );
}

// ── Image Carousel (single cover, arrows to browse, swipe on mobile) ──
function ImageCarousel({ images, onExpand }: { images: any[]; onExpand: (idx: number) => void }) {
  const [index, setIndex] = useState(0);
  const touchStartX = useRef<number | null>(null);

  const goPrev = (e: React.MouseEvent) => { e.stopPropagation(); setIndex(i => Math.max(0, i - 1)); };
  const goNext = (e: React.MouseEvent) => { e.stopPropagation(); setIndex(i => Math.min(images.length - 1, i + 1)); };

  const onTouchStart = (e: React.TouchEvent) => { touchStartX.current = e.touches[0].clientX; };
  const onTouchEnd = (e: React.TouchEvent) => {
    if (touchStartX.current === null) return;
    const diff = e.changedTouches[0].clientX - touchStartX.current;
    if (Math.abs(diff) > 50) {
      if (diff > 0) setIndex(i => Math.max(0, i - 1));
      else setIndex(i => Math.min(images.length - 1, i + 1));
    }
    touchStartX.current = null;
  };

  const img = images[index];

  return (
    <div className="post-media" style={{ padding: '0 14px 10px' }}>
      <div
        className="image-carousel"
        style={{ position: 'relative', borderRadius: '8px', overflow: 'hidden' }}
        onTouchStart={onTouchStart}
        onTouchEnd={onTouchEnd}
        onClick={(e) => { e.stopPropagation(); onExpand(index); }}
      >
        <img
          src={img.url}
          alt={img.alt_text || ''}
          className="post-media-img"
          loading="lazy"
          style={{ width: '100%', height: 'auto', display: 'block', cursor: 'pointer' }}
        />

        {/* Left arrow */}
        {images.length > 1 && index > 0 && (
          <button
            className="carousel-arrow carousel-prev"
            onClick={goPrev}
            style={{
              position: 'absolute', left: 8, top: '50%', transform: 'translateY(-50%)',
              background: 'rgba(0,0,0,0.6)', color: '#fff', border: 'none', borderRadius: '50%',
              width: 32, height: 32, fontSize: 18, cursor: 'pointer', display: 'flex',
              alignItems: 'center', justifyContent: 'center'
            }}
          >‹</button>
        )}

        {/* Right arrow */}
        {images.length > 1 && index < images.length - 1 && (
          <button
            className="carousel-arrow carousel-next"
            onClick={goNext}
            style={{
              position: 'absolute', right: 8, top: '50%', transform: 'translateY(-50%)',
              background: 'rgba(0,0,0,0.6)', color: '#fff', border: 'none', borderRadius: '50%',
              width: 32, height: 32, fontSize: 18, cursor: 'pointer', display: 'flex',
              alignItems: 'center', justifyContent: 'center'
            }}
          >›</button>
        )}

        {/* Counter */}
        {images.length > 1 && (
          <div style={{
            position: 'absolute', bottom: 8, right: 8,
            background: 'rgba(0,0,0,0.6)', color: '#fff',
            fontSize: 12, padding: '2px 8px', borderRadius: 12,
            fontFamily: 'var(--font-mono)'
          }}>
            {index + 1} / {images.length}
          </div>
        )}

        {/* Dots */}
        {images.length > 1 && images.length <= 8 && (
          <div style={{
            position: 'absolute', bottom: 8, left: '50%', transform: 'translateX(-50%)',
            display: 'flex', gap: 4
          }}>
            {images.map((_, i) => (
              <div key={i} style={{
                width: 6, height: 6, borderRadius: '50%',
                background: i === index ? '#fff' : 'rgba(255,255,255,0.4)'
              }} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

// ── Quote Post Embed ──
/** AppView view-shape of a fetched quoted post — only the fields we render. */
interface QuoteAuthor {
  did?: string;
  handle?: string;
  displayName?: string;
  avatar?: string;
}
interface QuoteImage {
  fullsize?: string;
  thumb?: string;
  image?: { ref?: { $link?: string } };
}
interface QuoteEmbed {
  $type?: string;
  images?: QuoteImage[];
  video?: { ref?: { $link?: string } };
  playlist?: string;
  external?: { uri?: string; title?: string; description?: string };
  /** recordWithMedia: the media half carries images / video / a link card. */
  media?: QuoteEmbed;
}

/**
 * The embed that actually holds the quoted post's media.
 *
 * A quoted post can itself be a quote-with-media (`recordWithMedia` — the
 * "10 year challenge" post that rendered text-only); its images live under
 * `embed.media.images`, not `embed.images`. Everything else is direct.
 */
function quotedMediaEmbed(embed: QuoteEmbed | undefined): QuoteEmbed | undefined {
  if (embed?.$type?.includes('recordWithMedia')) return embed.media;
  return embed;
}

interface QuotedPostView {
  author?: QuoteAuthor;
  record?: { text?: string; labels?: { values?: Array<{ val?: string }> } };
  embed?: QuoteEmbed;
  labels?: Array<{ val?: string } | string>;
}

/** Content labels that make a post sensitive (mirrors backend NSFW_LABELS). */
const QUOTE_NSFW_LABELS = [
  'nsfw', 'porn', 'sexual', 'nudity', 'graphic-media', 'adult',
  '!warn', '!hide', '!no-unspecified',
];

/** Labels on a proxied quoted post — applied labels plus record self-labels. */
function quotedPostLabels(post: QuotedPostView): string[] {
  const vals = [
    ...(post.labels ?? []).map((l) => (typeof l === 'string' ? l : l?.val)),
    ...((post.record?.labels?.values ?? []).map((l) => l?.val)),
  ];
  return vals
    .filter((v): v is string => typeof v === 'string' && v.length > 0)
    .map((v) => v.toLowerCase());
}

function isQuotedPostView(v: unknown): v is QuotedPostView {
  return typeof v === 'object' && v !== null;
}

export function QuotePostEmbed({ uri }: { uri: string }) {
  const [quotedPost, setQuotedPost] = useState<QuotedPostView | null>(null);
  const [loading, setLoading] = useState(true);
  const [failed, setFailed] = useState(false);
  const [enlarged, setEnlarged] = useState<string | null>(null);
  const [quoteRevealed, setQuoteRevealed] = useState(false);

  useEffect(() => {
    let cancelled = false;

    // Extract the DID and rkey from the at:// URI
    const match = uri.match(/^at:\/\/([^/]+)\/app\.bsky\.feed\.post\/(.+)$/);
    if (!match) { setFailed(true); setLoading(false); return; }

    const [, did, rkey] = match;

    // Fetch the quoted post via our server proxy (avoids CORS/rate-limit issues)
    const url = `${API}/api/bluesky/post?uri=${encodeURIComponent(uri)}`;

    fetch(url)
      .then((r) => (r.ok ? r.json() : Promise.reject()))
      .then((d: unknown) => {
        if (cancelled) return;
        const posts: unknown =
          d && typeof d === 'object' && 'posts' in d ? d.posts : undefined;
        const post: unknown = Array.isArray(posts) ? posts[0] : undefined;
        if (isQuotedPostView(post)) {
          setQuotedPost(post);
        } else {
          setFailed(true);
        }
        setLoading(false);
      })
      .catch(() => {
        if (!cancelled) { setFailed(true); setLoading(false); }
      });

    return () => { cancelled = true; };
  }, [uri]);

  if (loading) return <div className="quote-embed loading">Loading quoted post...</div>;
  if (failed || !quotedPost) {
    // Fallback: show a link to the post on bsky.app
    const bskyUrl = uri.replace(/^at:\/\//, 'https://bsky.app/profile/').replace(/app\.bsky\.feed\.post\//, 'post/');
    return (
      <a href={bskyUrl} target="_blank" rel="noopener noreferrer" className="quote-embed failed" onClick={e => e.stopPropagation()}>
        Quoted post unavailable — view on Bluesky
      </a>
    );
  }

  const author: QuoteAuthor = quotedPost.author ?? {};
  const record: { text?: string } = quotedPost.record ?? {};
  const embed: QuoteEmbed = quotedPost.embed ?? {};
  // A clean outer post can quote an explicitly labeled one (the 2026-11-15
  // leak: caption + quote of a `porn`-labeled video). The quoted post's own
  // labels gate its media here — never render it clear while flagged.
  const quoteFlagged = quotedPostLabels(quotedPost).some((v) => QUOTE_NSFW_LABELS.includes(v));

  // Extract all images from the quoted post. AppView returns view-form
  // embeds (direct fullsize/thumb URLs); record-form only carries blob refs.
  // A quoted quote-with-media keeps its media one level down (`embed.media`).
  const mediaEmbed = quotedMediaEmbed(embed);
  let quoteImages: string[] = [];
  if (mediaEmbed?.$type?.includes('images') && mediaEmbed.images) {
    quoteImages = mediaEmbed.images.map((img) => {
      if (img.fullsize) return img.fullsize;
      if (img.thumb) return img.thumb;
      const imgRef = img.image?.ref?.$link;
      return imgRef ? `https://cdn.bsky.app/img/feed_fullsize/plain/${author.did}/${imgRef}@jpeg` : null;
    }).filter((url): url is string => url !== null);
  }

  // Video: view-form carries the HLS playlist directly; record-form only the
  // blob ref from which the watch URL is built.
  let quoteVideoUrl: string | null = null;
  if (mediaEmbed?.$type?.includes('video')) {
    quoteVideoUrl =
      mediaEmbed.playlist ||
      (mediaEmbed.video?.ref?.$link
        ? `https://video.bsky.app/watch/${author.did}/${mediaEmbed.video.ref.$link}/playlist.m3u8`
        : null);
  }

  // External link card embedded in the quoted post
  const quoteExternal = mediaEmbed?.$type?.includes('external') ? mediaEmbed.external : null;

  const bskyProfileUrl = `https://bsky.app/profile/${author.handle || author.did}`;

  return (
    <>
    <div style={{ position: 'relative' }}>
    {quoteFlagged && !quoteRevealed && (
      <button
        className="nsfw-reveal"
        onClick={(e) => { e.stopPropagation(); setQuoteRevealed(true); }}
        style={{
          position: 'absolute', inset: 0, zIndex: 10, width: '100%',
          background: 'rgba(0,0,0,0.55)', color: '#fff', border: 'none',
          fontSize: 14, fontWeight: 700, cursor: 'pointer', borderRadius: 8,
        }}
      >
        ⚠️ NSFW — click to reveal
      </button>
    )}
    <div
      className="quote-embed"
      onClick={e => e.stopPropagation()}
      style={{ filter: quoteFlagged && !quoteRevealed ? 'blur(22px)' : 'none' }}
    >
      <a href={bskyProfileUrl} target="_blank" rel="noopener noreferrer" style={{ textDecoration: 'none', color: 'inherit' }}>
        <div className="quote-header">
          {author.avatar && <img src={author.avatar} className="quote-avatar" alt="" />}
          <span className="quote-author">{author.displayName || author.handle || 'Unknown'}</span>
          <span className="quote-handle">@{author.handle || 'unknown'}</span>
        </div>
      </a>
      <div className="quote-content">{record.text || ''}</div>
      {quoteExternal?.uri && (
        <a
          href={quoteExternal.uri}
          target="_blank"
          rel="noopener noreferrer"
          onClick={(e) => e.stopPropagation()}
          style={{ display: 'block', marginTop: 6, fontSize: 12, color: 'var(--accent)' }}
        >
          {quoteExternal.title || quoteExternal.uri}
        </a>
      )}
      {quoteImages.length > 0 && (
        <div className="quote-media">
          {quoteImages.length === 1 ? (
            <img
              src={quoteImages[0]}
              alt=""
              className="quote-media-img"
              loading="lazy"
              onClick={() => setEnlarged(quoteImages[0])}
            />
          ) : (
            <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>
              {quoteImages.map((url, i) => (
                <img
                  key={i}
                  src={url}
                  alt=""
                  style={{ width: 'calc(50% - 2px)', height: 'auto', borderRadius: 6, objectFit: 'cover', cursor: 'zoom-in' }}
                  loading="lazy"
                  onClick={() => setEnlarged(url)}
                />
              ))}
            </div>
          )}
        </div>
      )}
      {quoteVideoUrl && (
        <div className="quote-media">
          <HlsVideo src={quoteVideoUrl} />
        </div>
      )}
    </div>
    </div>
    {enlarged && (
      <div
        onClick={() => setEnlarged(null)}
        style={{
          position: 'fixed',
          inset: 0,
          background: 'rgba(0, 0, 0, 0.88)',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          zIndex: 1000,
          cursor: 'zoom-out',
        }}
      >
        <img src={enlarged} alt="" style={{ maxWidth: '92vw', maxHeight: '92vh', objectFit: 'contain' }} />
      </div>
    )}
    </>
  );
}
