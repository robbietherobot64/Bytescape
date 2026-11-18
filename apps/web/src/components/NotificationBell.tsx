import { useState, useEffect, useRef } from 'react';
import { notificationsApi, type NotificationItem } from '../utils/api';

/**
 * NotificationBell — the TabBar bell. Unread badge + a dropdown preview of the
 * last 5 notifications. Clicking a notification navigates to its target; the
 * "See all" link opens the full /notifications screen. Polling (30s) pauses
 * when the tab is hidden; badge updates from the same stream. (Spec 4.5 —
 * refined from v1's NotificationBell.)
 */

interface Props {
  /** Open the full Notifications screen. */
  onOpenAll: () => void;
}

const TYPE_ICON: Record<string, string> = {
  like: '❤️',
  repost: '🔁',
  follow: '👤',
  follow_request: '👤',
  reply: '💬',
  mention: '@',
  profile_comment: '📝',
};

function actorHandle(n: NotificationItem): string {
  const data = n.data as Record<string, unknown> | null | undefined;
  const h = data?.actor_handle || n.actor_id;
  return typeof h === 'string' ? h.replace(/^@/, '') : 'someone';
}

export default function NotificationBell({ onOpenAll }: Props) {
  const [open, setOpen] = useState(false);
  const [items, setItems] = useState<NotificationItem[]>([]);
  const [unread, setUnread] = useState(0);
  const rootRef = useRef<HTMLDivElement>(null);

  const refresh = async () => {
    try {
      const res = await notificationsApi.list({ limit: 5 });
      const list = res.notifications || [];
      setItems(list);
      const count = await notificationsApi.unreadCount();
      setUnread(count.count || 0);
    } catch {
      // keep the last known state
    }
  };

  useEffect(() => {
    refresh();
    const onVisibility = () => {
      if (!document.hidden) refresh();
    };
    const interval = setInterval(() => {
      if (!document.hidden) refresh();
    }, 30000);
    document.addEventListener('visibilitychange', onVisibility);
    window.addEventListener('bytescape:notificationsChanged', refresh);
    return () => {
      clearInterval(interval);
      document.removeEventListener('visibilitychange', onVisibility);
      window.removeEventListener('bytescape:notificationsChanged', refresh);
    };
  }, []);

  // The bell owns the unread count; broadcast it so the sidebar nav badge
  // (and anything else) follows without a second polling loop.
  useEffect(() => {
    window.dispatchEvent(
      new CustomEvent('bytescape:notificationsUnread', { detail: { count: unread } }),
    );
  }, [unread]);

  // Close on outside click.
  useEffect(() => {
    if (!open) return;
    const onClick = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', onClick);
    return () => document.removeEventListener('mousedown', onClick);
  }, [open]);

  const markRead = async (n: NotificationItem) => {
    if (!n.read) {
      try {
        await notificationsApi.markRead(n.id);
        setUnread((u) => Math.max(0, u - 1));
      } catch {
        // ignore
      }
    }
    setOpen(false);
    if (n.notification_type === 'follow' || n.notification_type === 'follow_request') {
      window.dispatchEvent(
        new CustomEvent('bytescape:viewProfile', { detail: { handle: actorHandle(n) } }),
      );
    } else {
      window.dispatchEvent(new CustomEvent('bytescape:navigateToFeed'));
    }
  };

  const markAllRead = async () => {
    try {
      await notificationsApi.markAllRead();
      setItems((prev) => prev.map((x) => ({ ...x, read: true })));
      setUnread(0);
      window.dispatchEvent(new CustomEvent('bytescape:notificationsChanged'));
    } catch {
      // ignore
    }
  };

  return (
    <div className="notification-bell" ref={rootRef}>
      <button
        className="notification-bell-btn"
        aria-label={`Notifications${unread ? ` (${unread} unread)` : ''}`}
        aria-live="assertive"
        onClick={() => setOpen((o) => !o)}
      >
        <span className="notification-bell-icon">🔔</span>
        {unread > 0 && <span className="notification-bell-badge">{unread > 9 ? '9+' : unread}</span>}
      </button>

      {open && (
        <div className="notification-bell-dropdown">
          <div className="notification-bell-header">
            <span>Notifications</span>
            <button className="notification-bell-markall" onClick={markAllRead}>
              Mark all read
            </button>
          </div>
          <div className="notification-bell-list">
            {items.length === 0 && (
              <div className="notification-bell-empty">No notifications yet.</div>
            )}
            {items.map((n) => (
              <button
                key={n.id}
                className={`notification-bell-item${n.read ? '' : ' unread'}`}
                onClick={() => markRead(n)}
              >
                <span className="notification-bell-item-icon" aria-hidden="true">
                  {TYPE_ICON[n.notification_type] || '🔔'}
                </span>
                <span className="notification-bell-item-text">
                  <strong>@{actorHandle(n)}</strong>{' '}
                  {n.notification_type === 'follow_request' ? 'wants to follow you' : n.title}
                </span>
              </button>
            ))}
          </div>
          <button
            className="notification-bell-seeall"
            onClick={() => {
              setOpen(false);
              onOpenAll();
            }}
          >
            See all notifications →
          </button>
        </div>
      )}
    </div>
  );
}
