import { useState, useEffect, useCallback, useRef } from 'react';
import {
  notificationsApi,
  apiErrorDisplay,
  type NotificationItem,
  type NotificationType,
} from '../utils/api';

/**
 * Notifications screen (spec: wiki/design/screens/notifications.md).
 *
 * A reverse-chronological feed of social events — likes, reposts, follows,
 * replies, mentions, profile comments — with a type filter bar, day grouping,
 * unread dots, mark-all-read, load-more, and inline approve/deny on
 * follow-request items. Delivery: polling (tightened 30s, paused when the tab
 * is hidden, refetch on focus) — see the openspec tasks note on SSE vs polling.
 */

const FILTERS: Array<{ id: 'all' | NotificationType; label: string; icon: string }> = [
  { id: 'all', label: 'All', icon: '☰' },
  { id: 'like', label: 'Likes', icon: '❤️' },
  { id: 'repost', label: 'Reposts', icon: '🔁' },
  { id: 'follow', label: 'Follows', icon: '👤' },
  { id: 'reply', label: 'Replies', icon: '💬' },
  { id: 'mention', label: 'Mentions', icon: '@' },
  { id: 'profile_comment', label: 'Comments', icon: '📝' },
];

const TYPE_ICON: Record<string, string> = {
  like: '❤️',
  repost: '🔁',
  follow: '👤',
  follow_request: '👤',
  reply: '💬',
  mention: '@',
  profile_comment: '📝',
};

const TYPE_ACTION: Record<string, string> = {
  like: 'liked your post',
  repost: 'reposted your post',
  follow: 'followed you',
  follow_request: 'wants to follow you',
  reply: 'replied to your post',
  mention: 'mentioned you',
  profile_comment: 'commented on your profile',
};

function formatRelative(iso: string): string {
  const then = new Date(iso).getTime();
  const now = Date.now();
  const s = Math.max(0, Math.floor((now - then) / 1000));
  if (s < 60) return 'now';
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h`;
  const d = Math.floor(h / 24);
  if (d < 7) return `${d}d`;
  return `${Math.floor(d / 7)}w`;
}

function dayGroup(iso: string): string {
  const then = new Date(iso);
  const now = new Date();
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const t = then.getTime();
  if (t >= startOfToday) return 'Today';
  if (t >= startOfToday - 86400000) return 'Yesterday';
  if (t >= startOfToday - 7 * 86400000) return 'This week';
  return 'Older';
}

function actorHandle(n: NotificationItem): string {
  const data = n.data as Record<string, unknown> | null | undefined;
  const h = data?.actor_handle || n.actor_id;
  return typeof h === 'string' ? h.replace(/^@/, '') : 'someone';
}

function subjectPreview(n: NotificationItem): string | null {
  const data = n.data as Record<string, unknown> | null | undefined;
  const uri = data?.subject_uri;
  return typeof uri === 'string' && uri ? '[post]' : null;
}

export default function NotificationsScreen() {
  const [items, setItems] = useState<NotificationItem[]>([]);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
  const [activeFilter, setActiveFilter] = useState<'all' | NotificationType>('all');
  const [hasMore, setHasMore] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [resolving, setResolving] = useState<Record<string, boolean>>({});
  const filterRef = useRef<HTMLDivElement>(null);

  const load = useCallback(async (filter: 'all' | NotificationType, cursor?: string) => {
    if (cursor) setLoadingMore(true);
    else setStatus('loading');
    try {
      const res = await notificationsApi.list({
        type: filter === 'all' ? undefined : filter,
        cursor,
        limit: 20,
      });
      const list = res.notifications || [];
      setItems((prev) => {
        if (!cursor) return list;
        const seen = new Set(prev.map((n) => n.id));
        return [...prev, ...list.filter((n) => !seen.has(n.id))];
      });
      setHasMore(list.length === 20);
      setStatus('ready');
    } catch {
      if (!cursor) setStatus('error');
    } finally {
      setLoadingMore(false);
    }
  }, []);

  // Initial load + refetch when the filter changes.
  useEffect(() => {
    setItems([]);
    setHasMore(false);
    load(activeFilter);
  }, [activeFilter, load]);

  // Polling delivery: 30s, paused when the tab is hidden, refetch on focus.
  useEffect(() => {
    let alive = true;
    const refetch = () => {
      if (alive) load(activeFilter);
    };
    const onVisibility = () => {
      if (!document.hidden) refetch();
    };
    const interval = setInterval(() => {
      if (!document.hidden) refetch();
    }, 30000);
    document.addEventListener('visibilitychange', onVisibility);
    window.addEventListener('focus', onVisibility);
    return () => {
      alive = false;
      clearInterval(interval);
      document.removeEventListener('visibilitychange', onVisibility);
      window.removeEventListener('focus', onVisibility);
    };
  }, [activeFilter, load]);

  // Keyboard: focus the filter bar on mount (spec 6.7 focus management).
  useEffect(() => {
    filterRef.current?.focus();
  }, []);

  const markRead = async (n: NotificationItem) => {
    if (n.read) return;
    setItems((prev) => prev.map((x) => (x.id === n.id ? { ...x, read: true } : x)));
    try {
      await notificationsApi.markRead(n.id);
      // Keep the bell + sidebar nav badges in sync (the bell refreshes on this
      // event and rebroadcasts its count).
      window.dispatchEvent(new CustomEvent('bytescape:notificationsChanged'));
    } catch {
      setItems((prev) => prev.map((x) => (x.id === n.id ? { ...x, read: false } : x)));
    }
  };

  const markAllRead = async () => {
    setItems((prev) => prev.map((x) => ({ ...x, read: true })));
    try {
      await notificationsApi.markAllRead();
      window.dispatchEvent(new CustomEvent('bytescape:notificationsChanged'));
    } catch {
      load(activeFilter);
    }
  };

  const resolveRequest = async (n: NotificationItem, approve: boolean) => {
    const data = n.data as Record<string, unknown> | null | undefined;
    const requestId = data?.request_id;
    if (typeof requestId !== 'string') return;
    setResolving((r) => ({ ...r, [n.id]: true }));
    try {
      if (approve) await notificationsApi.approveRequest(requestId);
      else await notificationsApi.denyRequest(requestId);
      // Reflect the resolution in the list.
      setItems((prev) =>
        prev.map((x) =>
          x.id === n.id
            ? { ...x, data: { ...(x.data || {}), resolved: approve ? 'approved' : 'denied' } }
            : x,
        ),
      );
      window.dispatchEvent(new CustomEvent('bytescape:notificationsChanged'));
    } catch (e) {
      window.dispatchEvent(
        new CustomEvent('bytescape:toast', {
          detail: { message: apiErrorDisplay(e), type: 'error' },
        }),
      );
    } finally {
      setResolving((r) => ({ ...r, [n.id]: false }));
    }
  };

  const navigateFor = (n: NotificationItem) => {
    const data = n.data as Record<string, unknown> | null | undefined;
    const handle = actorHandle(n);
    if (n.notification_type === 'follow' || n.notification_type === 'follow_request') {
      window.dispatchEvent(new CustomEvent('bytescape:viewProfile', { detail: { handle } }));
      return;
    }
    const subject = data?.subject_uri;
    if (typeof subject === 'string' && subject) {
      window.dispatchEvent(new CustomEvent('bytescape:navigateToFeed'));
      return;
    }
    if (n.notification_type === 'profile_comment') {
      window.dispatchEvent(new CustomEvent('bytescape:viewOwnProfile'));
      return;
    }
    window.dispatchEvent(new CustomEvent('bytescape:navigateToFeed'));
  };

  const onRowClick = (n: NotificationItem) => {
    markRead(n);
    navigateFor(n);
  };

  // Group by day, preserving reverse-chronological order within each group.
  const groups: Array<{ label: string; items: NotificationItem[] }> = [];
  for (const n of items) {
    const label = dayGroup(n.created_at);
    const last = groups[groups.length - 1];
    if (last && last.label === label) last.items.push(n);
    else groups.push({ label, items: [n] });
  }

  const unreadCount = items.filter((n) => !n.read).length;

  return (
    <div className="screen notifications-screen">
      <div className="notifications-header">
        <h2 className="section-title notifications-title">NOTIFICATIONS</h2>
      </div>

      {/* Filter bar (sticky) */}
      <div
        className="notification-filter-bar"
        role="tablist"
        aria-label="Notification filters"
        tabIndex={-1}
        ref={filterRef as React.RefObject<HTMLDivElement>}
      >
        <div className="notification-filter-chips">
          {FILTERS.map((f) => (
            <button
              key={f.id}
              role="tab"
              aria-selected={activeFilter === f.id}
              className={`notification-filter-chip${activeFilter === f.id ? ' active' : ''}`}
              onClick={() => setActiveFilter(f.id)}
            >
              <span className="chip-icon">{f.icon}</span>
              <span className="chip-label">{f.label}</span>
            </button>
          ))}
        </div>
        <button
          className="btn notification-mark-all"
          onClick={markAllRead}
          aria-label="Mark all notifications as read"
          disabled={unreadCount === 0}
        >
          Mark all read
        </button>
      </div>

      {/* List */}
      {status === 'loading' && (
        <div className="notification-list" aria-busy="true">
          {[0, 1, 2].map((i) => (
            <div key={i} className="notification-skeleton">
              <div className="notification-skeleton-avatar" />
              <div className="notification-skeleton-lines">
                <div className="notification-skeleton-line" />
                <div className="notification-skeleton-line short" />
              </div>
            </div>
          ))}
        </div>
      )}

      {status === 'error' && (
        <div className="notification-empty">
          <div className="notification-empty-icon">⚠️</div>
          <div className="notification-empty-title">&gt; FAILED TO LOAD NOTIFICATIONS</div>
          <div className="notification-empty-sub">
            Something went wrong. Your connection may have dropped.
          </div>
          <button className="btn" onClick={() => load(activeFilter)}>
            &gt; RETRY
          </button>
        </div>
      )}

      {status === 'ready' && items.length === 0 && (
        <div className="notification-empty">
          <div className="notification-empty-icon">
            {activeFilter === 'all'
              ? '🔔'
              : FILTERS.find((f) => f.id === activeFilter)?.icon || '🔔'}
          </div>
          <div className="notification-empty-title">
            &gt; {activeFilter === 'all'
              ? 'NO NOTIFICATIONS YET'
              : `NO ${FILTERS.find((f) => f.id === activeFilter)?.label.toUpperCase() || ''} YET`}
          </div>
          <div className="notification-empty-sub">
            {activeFilter === 'all'
              ? "When someone interacts with your posts or profile, you'll see it here."
              : "When this happens, you'll see it here."}
          </div>
        </div>
      )}

      {status === 'ready' && items.length > 0 && (
        <div className="notification-list" aria-live="polite">
          {groups.map((group) => (
            <div key={group.label} className="notification-group">
              <div className="notification-group-header">{group.label}</div>
              {group.items.map((n) => {
                const data = n.data as Record<string, unknown> | null | undefined;
                const resolved = data?.resolved as string | undefined;
                const isRequest = n.notification_type === 'follow_request' && !resolved;
                return (
                  <div
                    key={n.id}
                    className={`notification-item${n.read ? '' : ' unread'}${isRequest ? ' request' : ''}`}
                    role="article"
                    tabIndex={0}
                    aria-label={`${actorHandle(n)} ${TYPE_ACTION[n.notification_type] || ''}`}
                    onClick={() => onRowClick(n)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        onRowClick(n);
                      }
                    }}
                  >
                    <span className="notification-dot" aria-hidden="true" />
                    <div className="notification-avatar" aria-hidden="true">
                      {actorHandle(n).slice(0, 1).toUpperCase()}
                    </div>
                    <div className="notification-body">
                      <div className="notification-text">
                        <span className="notification-type-icon" aria-hidden="true">
                          {TYPE_ICON[n.notification_type] || '🔔'}
                        </span>
                        <span className="notification-actor">@{actorHandle(n)}</span>{' '}
                        <span className="notification-action">
                          {TYPE_ACTION[n.notification_type] || n.title}
                        </span>
                        {subjectPreview(n) && (
                          <span className="notification-subject">{subjectPreview(n)}</span>
                        )}
                      </div>
                      <div className="notification-meta">
                        <span className="notification-network">{n.network}</span>
                        <span className="notification-time">{formatRelative(n.created_at)}</span>
                      </div>
                      {isRequest && (
                        <div className="notification-request-actions">
                          <button
                            className="btn notification-approve"
                            disabled={resolving[n.id]}
                            onClick={(e) => {
                              e.stopPropagation();
                              resolveRequest(n, true);
                            }}
                          >
                            {resolving[n.id] ? '...' : 'Approve'}
                          </button>
                          <button
                            className="btn notification-deny"
                            disabled={resolving[n.id]}
                            onClick={(e) => {
                              e.stopPropagation();
                              resolveRequest(n, false);
                            }}
                          >
                            Deny
                          </button>
                        </div>
                      )}
                      {resolved && (
                        <div className="notification-resolved">
                          {resolved === 'approved' ? 'Approved' : 'Denied'}
                        </div>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          ))}
          {hasMore && (
            <button
              className="btn notification-load-more"
              onClick={() => load(activeFilter, items[items.length - 1]?.created_at)}
              disabled={loadingMore}
            >
              {loadingMore ? 'Loading...' : '> LOAD MORE NOTIFICATIONS'}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
