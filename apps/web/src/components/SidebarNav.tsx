import React from 'react';
import { TAB_ICONS, type TabName } from './tabs';
import { notificationsApi } from '../utils/api';

interface Props {
  activeTab: TabName;
  onNavigate: (tab: TabName) => void;
  /** Situational events notification count (RSVPs, event updates). */
  eventCount?: number;
  onLogout: () => void;
}

interface NavItem {
  tab: TabName;
  label: string;
  disabled?: boolean;
}

/** Global left-rail order. Disabled entries are not built yet (stub slots). */
const NAV_ITEMS: NavItem[] = [
  { tab: 'Feed', label: 'Feed' },
  { tab: 'Notifications', label: 'Notifications' },
  { tab: 'Groups', label: 'Groups' },
  { tab: 'Events', label: 'Events' },
  { tab: 'Exchange', label: 'Classifieds', disabled: true },
  { tab: 'Vault', label: 'Vault' },
  { tab: 'Import', label: 'Import' },
  { tab: 'Profile', label: 'Profile' },
  { tab: 'Account', label: 'Account' },
  { tab: 'Settings', label: 'Settings' },
];

function Badge({ count }: { count?: number }) {
  if (!count) return null;
  return <span className="nav-badge">{count > 99 ? '99+' : count}</span>;
}

/**
 * Global left navigation rail — the single top-level nav. Items highlight
 * based on the active page. Events/Classifieds/Groups are greyed-out stubs;
 * Events still carries a situational notification badge. The Messages launcher
 * is a dedicated button pinned just above Logout (not a nav item) so it stays
 * reachable on every screen (including Profile).
 */
export default function SidebarNav({ activeTab, onNavigate, eventCount, onLogout }: Props) {
  const [unread, setUnread] = React.useState(0);
  React.useEffect(() => {
    const onUnread = (e: Event) => setUnread((e as CustomEvent).detail?.count || 0);
    window.addEventListener('bytescape:chatUnread', onUnread as EventListener);
    return () => window.removeEventListener('bytescape:chatUnread', onUnread as EventListener);
  }, []);
  // Notifications unread badge — same nav-badge pattern as Events. The bell
  // broadcasts its count (bytescape:notificationsUnread) whenever it refreshes
  // and bytescape:notificationsChanged triggers a refetch; since the bell now
  // lives on the Feed screen only, this side keeps a 30s hidden-aware poll of
  // its own so the badge stays live on every screen.
  const [notifUnread, setNotifUnread] = React.useState(0);
  React.useEffect(() => {
    const onCount = (e: Event) => setNotifUnread((e as CustomEvent).detail?.count || 0);
    const refetch = () => {
      notificationsApi
        .unreadCount()
        .then((r) => setNotifUnread(r.count || 0))
        .catch(() => {});
    };
    const onVisibility = () => {
      if (!document.hidden) refetch();
    };
    refetch();
    const interval = setInterval(() => {
      if (!document.hidden) refetch();
    }, 30000);
    document.addEventListener('visibilitychange', onVisibility);
    window.addEventListener('bytescape:notificationsUnread', onCount as EventListener);
    window.addEventListener('bytescape:notificationsChanged', refetch);
    return () => {
      clearInterval(interval);
      document.removeEventListener('visibilitychange', onVisibility);
      window.removeEventListener('bytescape:notificationsUnread', onCount as EventListener);
      window.removeEventListener('bytescape:notificationsChanged', refetch);
    };
  }, []);
  return (
    <nav className="sidebar-nav">
      {NAV_ITEMS.map((item) => (
        <button
          key={item.tab}
          className={`sidebar-nav-item${activeTab === item.tab ? ' active' : ''}${item.disabled ? ' disabled' : ''}`}
          disabled={item.disabled}
          onClick={() => onNavigate(item.tab)}
        >
          <span className="sidebar-nav-icon">{TAB_ICONS[item.tab]}</span> {item.label}
          {item.tab === 'Events' && <Badge count={eventCount} />}
          {item.tab === 'Notifications' && <Badge count={notifUnread} />}
        </button>
      ))}
      <button
        className="chat-launcher sidebar-chat-launcher"
        onClick={() => window.dispatchEvent(new CustomEvent('bytescape:toggleChatPanel'))}
      >
        <span className="chat-launcher-label">💬 Messages</span>
        {unread > 0 && <span className="chat-launcher-badge">{unread > 99 ? '99+' : unread}</span>}
      </button>
      <button className="sidebar-nav-item sidebar-logout" onClick={onLogout}>
        <span className="sidebar-nav-icon">🚪</span> Logout
      </button>
    </nav>
  );
}
