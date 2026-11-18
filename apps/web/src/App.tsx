import React, { useEffect, useState, Suspense } from 'react';
import LoginFlow from './screens/LoginFlow';
import LandingPage from './screens/LandingPage';
import FeedScreen from './screens/FeedScreen';
import { api, refreshAccessToken, settingsApi, applyAppTheme, normalizeAppTheme, applyThemeOverrides, loadThemeOverrides, userOverrideClassNames, subscribeSessionExpired, clearToken } from './utils/api';
import { configureEventsApi, eventsApi } from '@bytescape/events-ui';
import type { EventsViewer } from '@bytescape/events-ui';
import SidebarNav from './components/SidebarNav';
import ChatDock from './components/ChatDock';
import type { TabName } from './components/tabs';
import MobileNav from './components/MobileNav';

// Shared Events surface — the same package the standalone site mounts
// (spec: events/site — one component tree, both hosts).
const EventsLayout = React.lazy(() =>
  import('@bytescape/events-ui').then((m) => ({ default: m.EventsLayout })),
);
// Token seam: the web host attaches its localStorage token; the events host
// rides the family session cookie (design decision 8).
configureEventsApi({ getToken: () => localStorage.getItem('bytescape_token') });
const AccountScreen = React.lazy(() => import('./screens/AccountScreen'));
const SettingsScreen = React.lazy(() => import('./screens/SettingsScreen'));
const ProfileScreen = React.lazy(() => import('./screens/ProfileScreen'));
const NotificationsScreen = React.lazy(() => import('./screens/NotificationsScreen'));
const VaultScreen = React.lazy(() => import('./screens/VaultScreen'));
const GroupsScreen = React.lazy(() => import('./screens/GroupsScreen'));
const CommunityScreen = React.lazy(() => import('./screens/CommunityScreen'));
const ForumsScreen = React.lazy(() => import('./screens/ForumsScreen'));
const ExchangeScreen = React.lazy(() => import('./screens/ExchangeScreen'));
const ImportScreen = React.lazy(() => import('./screens/ImportScreen'));

function LoadingSpinner() {
  return (
    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 40 }}>
      <div style={{ color: 'var(--text-muted)', fontSize: 14 }}>Loading...</div>
    </div>
  );
}

type AppPhase = 'login' | 'app' | 'checking';

class ErrorBoundary extends React.Component<{ children: React.ReactNode }, { error: Error | null }> {
  constructor(props: any) { super(props); this.state = { error: null }; }
  static getDerivedStateFromError(error: Error) { return { error }; }
  render() {
    if (this.state.error) return (
      <div style={{ padding: 20, color: '#ff3333', background: '#111', height: '100%', fontFamily: 'monospace' }}>
        <h2>Component Crash</h2>
        <pre>{this.state.error.stack || this.state.error.message}</pre>
      </div>
    );
    return this.props.children;
  }
}

const LOCALHOST_MODE = import.meta.env.VITE_LOCALHOST_MODE === 'true';

// SSO hop return target (spec: events/session — one login covers the site
// family). Stored raw on arrival; validated at read time (below) so a
// same-origin-poisoned value can never redirect off-family.
const SSO_NEXT_KEY = 'bytescape_sso_next';

function getInitialPhase(): AppPhase {
  const token = localStorage.getItem('bytescape_token');
  if (token) return 'app';
  const refresh = localStorage.getItem('bytescape_refresh_token');
  if (refresh) return 'checking';
  return 'login';
}

// Unauthenticated landing gate (public landing spec): a BARE `/` visit shows
// the landing page instead of the bare login screen. Anything else — a
// different path, any query (`?join`, OAuth/SSO returns like `?oauth_complete`
// or `?next`), or a token-carrying fragment — goes to the auth flow exactly as
// before. The decision is frozen at mount (not read per render) so the `?next`
// stash below, which strips its query via replaceState, can never flip the
// screen mid-session; popstate re-evaluates for back/forward navigation.
// Gate predicate (identical at both evaluation sites):
//   pathname === '/' && no query && no hash

export default function App() {
  const [phase, setPhase] = React.useState<AppPhase>(getInitialPhase);
  // Landing gate (see shouldShowLanding): anonymous `/` shows the landing;
  // the Join/Sign-in doors push `?join=1` and force the auth flow on. The
  // query is what keeps a mid-flow reload on the auth flow (non-empty search
  // → gate off) while a bare `/` reload lands back on the landing.
  const [landingForLogin, setLandingForLogin] = React.useState(
    () => window.location.pathname === '/' && !window.location.search && !window.location.hash,
  );
  useEffect(() => {
    const onPop = () =>
      setLandingForLogin(window.location.pathname === '/' && !window.location.search && !window.location.hash);
    window.addEventListener('popstate', onPop);
    return () => window.removeEventListener('popstate', onPop);
  }, []);
  const enterLoginFlow = React.useCallback(() => {
    if (!new URLSearchParams(window.location.search).has('join')) {
      window.history.pushState({}, '', '/?join=1');
    }
    setLandingForLogin(false);
  }, []);
  // Terminal auth state (expired-token fix): the api layer announces an
  // unrecoverable session once — the app shows an honest interstitial and
  // every automatic fetch stays off (no silent refetch loop).
  const [sessionExpired, setSessionExpired] = useState(false);
  useEffect(() => subscribeSessionExpired(() => setSessionExpired(true)), []);
  const [openTabs, setOpenTabs] = React.useState<TabName[]>(['Feed']);
  const [activeTab, setActiveTab] = React.useState<TabName>('Feed');
  const [notification, setNotification] = useState<string | null>(null);
  const [toast, setToast] = useState<{ message: string; type: string } | null>(null);
  const [eventCount, setEventCount] = useState(0);
  const [viewedProfile, setViewedProfile] = useState<string | null>(null);
  const [viewedCommunity, setViewedCommunity] = useState<string | null>(null);
  const [appTheme, setAppTheme] = useState<string>('default');
  const [mobileMoreOpen, setMobileMoreOpen] = useState(false);
  const [eventsViewer, setEventsViewer] = useState<EventsViewer | null>(null);
  // User theme-override classes MUST be React state: the `.app` div's
  // className is React-controlled, so imperatively-added classes are wiped on
  // every re-render. Derive the flags from the stored overrides and render
  // them in the className so they survive.
  const [overrideClasses, setOverrideClasses] = useState<string>(() => userOverrideClassNames(loadThemeOverrides()));

  // App-chrome theme (app-themes spec): the stored theme loads once on boot
  // and drives the root `app-theme-<id>` class. The Settings screen posts a
  // `bytescape:appThemeChanged` event after a successful save so the chrome
  // restyles live without a reload. User customization overrides apply on
  // boot too (they persist across reload, not just while Settings is open).
  useEffect(() => {
    if (phase !== 'app') return;
    applyThemeOverrides(loadThemeOverrides());
    setOverrideClasses(userOverrideClassNames(loadThemeOverrides()));
    const apply = (raw: unknown) => {
      const id = normalizeAppTheme(raw);
      setAppTheme(id);
      applyAppTheme(id);
    };
    settingsApi.get().then((s) => apply(s.theme)).catch(() => apply('default'));
    const onChanged = (e: Event) => apply((e as CustomEvent).detail?.theme);
    const onOverrides = () => setOverrideClasses(userOverrideClassNames(loadThemeOverrides()));
    window.addEventListener('bytescape:appThemeChanged', onChanged as EventListener);
    window.addEventListener('bytescape:themeOverridesChanged', onOverrides as EventListener);
    return () => {
      window.removeEventListener('bytescape:appThemeChanged', onChanged as EventListener);
      window.removeEventListener('bytescape:themeOverridesChanged', onOverrides as EventListener);
    };
  }, [phase]);

  // Events tab identity: session fetch keyed on phase — runs on boot with a
  // live token and again after an in-place login (no remount happens then).
  useEffect(() => {
    if (phase !== 'app') return;
    if (!localStorage.getItem('bytescape_token')) {
      setEventsViewer(null);
      return;
    }
    let alive = true;
    eventsApi
      .session()
      .then((s) => {
        if (alive) setEventsViewer({ username: s.username, dids: s.dids });
      })
      .catch(() => {
        if (alive) setEventsViewer(null);
      });
    return () => {
      alive = false;
    };
  }, [phase]);

  // On mount: if no token but refresh token exists, try to auto-refresh
  useEffect(() => {
    if (phase === 'checking') {
      refreshAccessToken().then((newToken) => {
        if (newToken) {
          setPhase('app');
        } else {
          setPhase('login');
        }
      });
    }
  }, []);

  // Capture `next` before any login round trip. The Bluesky OAuth return
  // URL is built server-side (`/?oauth_complete=…`) and drops query params,
  // so sessionStorage is what carries the destination across the hop.
  // Declaration order matters: stash runs before the redirect effect below.
  useEffect(() => {
    const raw = new URLSearchParams(window.location.search).get('next');
    if (!raw) return;
    sessionStorage.setItem(SSO_NEXT_KEY, raw);
    window.history.replaceState({}, '', window.location.pathname);
  }, []);

  // Once a session exists — on arrival or after any completed login — hop
  // back to the stashed destination. Validated at read time: only
  // bytescape.social (+ subdomains) or loopback; external origins ignored
  // (spec: events/session), including values written same-origin into
  // sessionStorage after the stash ran.
  useEffect(() => {
    if (phase !== 'app') return;
    const next = sessionStorage.getItem(SSO_NEXT_KEY);
    if (!next) return;
    sessionStorage.removeItem(SSO_NEXT_KEY);
    try {
      const u = new URL(next, window.location.origin);
      const host = u.hostname;
      if (
        host === 'localhost' ||
        host === '127.0.0.1' ||
        host === 'bytescape.social' ||
        host.endsWith('.bytescape.social')
      ) {
        // Re-mint family cookies before leaving (spec: idle origin resumes):
        // the destination rides the cookie, and its own refresh may be idle.
        // The rotated pair is persisted here so this origin stays valid too.
        fetch('/api/auth/refresh', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            refresh_token: localStorage.getItem('bytescape_refresh_token') ?? '',
          }),
        })
          .then((r) => (r.ok ? r.json() : null))
          .then((j) => {
            if (j && j.access_token) {
              localStorage.setItem('bytescape_token', j.access_token);
              localStorage.setItem('bytescape_refresh_token', j.refresh_token);
            }
          })
          .catch(() => {})
          .finally(() => window.location.replace(u.toString()));
      }
    } catch {
      // Malformed next — ignored.
    }
  }, [phase]);

  // On mount: check for redirect flags set by Welcome/first-session screen
  useEffect(() => {
    if (phase === 'app') {
      const redirectTab = localStorage.getItem('bytescape_redirect_tab');
      if (redirectTab) {
        localStorage.removeItem('bytescape_redirect_tab');
        if (!openTabs.includes(redirectTab as TabName)) {
          setOpenTabs(prev => [...prev, redirectTab as TabName]);
        }
        setActiveTab(redirectTab as TabName);
      } else {
        // Onboarding entry: route to the Import screen for accounts that have
        // not completed onboarding. The AUTHORITATIVE signal is the server
        // `users.onboarded` flag (via /api/profile/me) — localStorage
        // 'onboarding_complete' is only a local hint and must not re-prompt a
        // returning user who has already onboarded.
        // Route to Import at most once per browser session — without this
        // guard every reload snapped the user back to Import forever.
        if (!sessionStorage.getItem('onboarding_route_done')) {
          import('./utils/api').then(async ({ profileApi }) => {
            let onboarded = false;
            try {
              const profile = await profileApi.me();
              onboarded = profile.onboarded === true;
            } catch {
              // Server unreachable — fall back to the local hint.
              onboarded = localStorage.getItem('onboarding_complete') === 'true';
            }
            // Checked once per session either way (avoids repeat profile fetches).
            sessionStorage.setItem('onboarding_route_done', '1');
            if (!onboarded) {
              if (!openTabs.includes('Import')) {
                setOpenTabs(prev => [...prev, 'Import']);
              }
              setActiveTab('Import');
            }
          });
        }
      }
    }
  }, [phase]);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const provider = params.get('provider');
    const success = params.get('success');
    const error = params.get('error');
    if (provider && success === 'true') {
      window.history.replaceState({}, '', '/app');
      const pName = provider.charAt(0).toUpperCase() + provider.slice(1);
      setNotification(`${pName} account linked successfully!`);
      if (!openTabs.includes('Account')) {
        setOpenTabs(prev => [...prev, 'Account']);
      }
      setActiveTab('Account');
      setTimeout(() => setNotification(null), 5000);
    } else if (provider && success === 'false' && error) {
      window.history.replaceState({}, '', '/app');
      const pName = provider.charAt(0).toUpperCase() + provider.slice(1);
      setNotification(`${pName} linking failed: ${error}`);
      if (!openTabs.includes('Account')) {
        setOpenTabs(prev => [...prev, 'Account']);
      }
      setActiveTab('Account');
      setTimeout(() => setNotification(null), 8000);
    }
  }, []);

  // Situational badge: events notifications (RSVPs, event updates) on the
  // Events nav item. Message unread counts badge the chat launcher itself.
  useEffect(() => {
    let alive = true;
    const poll = () => {
      api<Array<{ type?: string; read?: boolean }>>('/api/notifications')
        .then((list) => {
          if (!alive) return;
          const events = (Array.isArray(list) ? list : []).filter(
            (n) => !n.read && /event|rsvp|invite/i.test(n.type || ''),
          );
          setEventCount(events.length);
        })
        .catch(() => {});
    };
    poll();
    const interval = setInterval(poll, 60000);
    window.addEventListener('bytescape:notificationsChanged', poll);
    return () => {
      alive = false;
      clearInterval(interval);
      window.removeEventListener('bytescape:notificationsChanged', poll);
    };
  }, []);

  useEffect(() => {
    const handler = () => {
      const provider = localStorage.getItem('last_linked_provider') || 'Account';
      setNotification(`${provider} linked successfully!`);
      setTimeout(() => setNotification(null), 5000);
    };
    window.addEventListener('bytescape:accountLinked', handler);
    return () => window.removeEventListener('bytescape:accountLinked', handler);
  }, []);

  useEffect(() => {
    const handler = () => setViewedProfile(null);
    window.addEventListener('bytescape:viewOwnProfile', handler);
    return () => window.removeEventListener('bytescape:viewOwnProfile', handler);
  }, []);

  // Interaction-failure toasts must work on every tab — this listener lives
  // on the always-mounted App root (it was FeedLayout-only, which unmounts
  // away from the Feed tab and silently dropped the errors).
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      setToast(detail);
      setTimeout(() => setToast(null), 3000);
    };
    window.addEventListener('bytescape:toast', handler);
    return () => window.removeEventListener('bytescape:toast', handler);
  }, []);

  // Handle tour navigation to feed
  useEffect(() => {
    const handler = () => {
      if (!openTabs.includes('Feed')) {
        setOpenTabs(prev => [...prev, 'Feed']);
      }
      setActiveTab('Feed');
    };
    window.addEventListener('bytescape:navigateToFeed', handler);
    return () => window.removeEventListener('bytescape:navigateToFeed', handler);
  }, [openTabs]);

  const openTab = React.useCallback((tab: TabName) => {
    setOpenTabs(prev => prev.includes(tab) ? prev : [...prev, tab]);
    setActiveTab(tab);
    if (tab === 'Profile') setViewedProfile(null);
  }, []);

  const selectTab = React.useCallback((tab: TabName) => {
    setActiveTab(tab);
    if (!openTabs.includes(tab)) {
      setOpenTabs(prev => [...prev, tab]);
    }
  }, [openTabs]);

  const [interestsRequest, setInterestsRequest] = React.useState(0);
  const handleInterests = React.useCallback(() => {
    if (!openTabs.includes('Feed')) openTab('Feed');
    setActiveTab('Feed');
    // Prop signal, not a window event: FeedScreen unmounts away from the
    // Feed tab, so a synchronous event fired here could land before the
    // listener existed and silently open nothing (the "Edit interests
    // shows the feed" bug). A prop arrives with the first render.
    setInterestsRequest(n => n + 1);
  }, [openTabs, openTab]);

  const handleLogout = React.useCallback(async () => {
    const { disposeMatrixClient } = await import('./utils/matrix');
    disposeMatrixClient();
    localStorage.clear();
    window.location.reload();
  }, []);

  useEffect(() => {
    const handler = (e: Event) => {
      const { handle, source } = (e as CustomEvent).detail;
      const cleanHandle = (handle || '').replace(/^@+/, '').trim();
      if (!cleanHandle) return;
      const parts = cleanHandle.split('@').filter(Boolean);
      if (parts.length >= 2) {
        const username = parts[0];
        const domain = parts[parts.length - 1];
        window.open(`https://${domain}/@${username}`, '_blank');
      } else {
        setViewedProfile(cleanHandle);
        setOpenTabs(prev => prev.includes('Profile') ? prev : [...prev, 'Profile']);
        setActiveTab('Profile');
      }
    };
    window.addEventListener('bytescape:viewProfile', handler);
    return () => window.removeEventListener('bytescape:viewProfile', handler);
  }, []);

  // Community page: a group post card's community name opens the Bytescape-
  // native community page (design: interactive post).
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      const actorId = detail?.actorId;
      if (!actorId) return;
      setViewedCommunity(actorId);
      setOpenTabs(prev => (prev.includes('Groups') ? prev : [...prev, 'Groups']));
      setActiveTab('Groups');
    };
    window.addEventListener('bytescape:viewCommunity', handler);
    return () => window.removeEventListener('bytescape:viewCommunity', handler);
  }, []);

  if (phase === 'login') {
    if (landingForLogin) {
      return (
        <ErrorBoundary>
          <LandingPage onJoin={enterLoginFlow} />
        </ErrorBoundary>
      );
    }
    return (
      <ErrorBoundary>
        <LoginFlow onComplete={() => setPhase('app')} />
      </ErrorBoundary>
    );
  }

  if (sessionExpired) {
    // Honest interstitial (loading-state pattern): the session is gone and
    // nothing retries behind the user's back.
    return (
      <div className="fullscreen" style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', flexDirection: 'column' }}>
        <div className="terminal-title" style={{ marginBottom: 16 }}>SESSION EXPIRED</div>
        <div style={{ color: 'var(--text-muted)', fontSize: 14, marginBottom: 20, maxWidth: 420, textAlign: 'center' }}>
          Your session has expired — please sign in again.
        </div>
        <button
          className="btn"
          onClick={() => {
            clearToken();
            setSessionExpired(false);
            // The interstitial's "Sign in" must reach the auth flow itself —
            // not the public landing (which is for fresh visitors).
            enterLoginFlow();
            setPhase('login');
          }}
          style={{ padding: '10px 28px', fontWeight: 600 }}
        >
          Sign in
        </button>
      </div>
    );
  }

  if (phase === 'checking') {
    return (
      <div className="fullscreen" style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', flexDirection: 'column' }}>
        <div className="terminal-title" style={{ marginBottom: 16 }}>RESTORING SESSION</div>
        <div style={{ color: 'var(--text-muted)', fontSize: 14 }}>Checking credentials...</div>
      </div>
    );
  }

  return (
    <div className={`app app-theme-${appTheme}${overrideClasses ? ' ' + overrideClasses : ''}`}>
      {notification && (
        <div className="notification" style={{
          position: 'fixed', top: 60, left: '50%', transform: 'translateX(-50%)',
          background: '#2d5a2d', color: '#fff', padding: '8px 16px', borderRadius: 8,
          zIndex: 1000, fontSize: 14,
        }}>
          {notification}
        </div>
      )}
      {toast && (
        <div className={`toast toast-${toast.type}`}>
          {toast.message}
        </div>
      )}
      <SidebarNav
        activeTab={activeTab}
        onNavigate={selectTab}
        eventCount={eventCount}
        onLogout={handleLogout}
      />
      <ChatDock />
      <div className="tab-content">
        <Suspense fallback={<LoadingSpinner />}>
          {activeTab === 'Feed' && (
            <FeedScreen
              onNavigate={(t) => { if (!openTabs.includes(t as TabName)) openTab(t as TabName); selectTab(t as TabName); }}
              onLogout={handleLogout}
              onInterests={handleInterests}
              activeTab={activeTab}
              interestsRequest={interestsRequest}
            />
          )}
          {activeTab === 'Account' && <AccountScreen />}
          {activeTab === 'Settings' && <SettingsScreen />}
          {activeTab === 'Notifications' && <NotificationsScreen />}
          {activeTab === 'Profile' && (
            viewedProfile
              ? <ProfileScreen username={viewedProfile} onViewProfile={setViewedProfile} />
              : <ProfileScreen onViewProfile={setViewedProfile} />
          )}
          {activeTab === 'Forums' && <ForumsScreen />}
          {activeTab === 'Groups' && (
            viewedCommunity
              ? <CommunityScreen
                  actorId={viewedCommunity}
                  onBack={() => setViewedCommunity(null)}
                />
              : <GroupsScreen />
          )}
          {activeTab === 'Exchange' && <ExchangeScreen />}
          {activeTab === 'Events' && (
            <EventsLayout
              viewer={eventsViewer}
              onRequireSignIn={() => {
                // The web host is the login surface itself; a missing viewer
                // with a live token means the session fetch is stale.
                if (localStorage.getItem('bytescape_token')) {
                  eventsApi
                    .session()
                    .then((s) => setEventsViewer({ username: s.username, dids: s.dids }))
                    .catch(() => setEventsViewer(null));
                }
              }}
            />
          )}
          {activeTab === 'Vault' && <VaultScreen />}
          {activeTab === 'Import' && <ImportScreen onNavigate={(t) => { if (!openTabs.includes(t)) openTab(t); selectTab(t); }} />}
        </Suspense>
      </div>
      {/* Mobile bottom nav */}
      <MobileNav
        activeTab={activeTab}
        onSelectTab={(t) => { if (!openTabs.includes(t)) openTab(t); selectTab(t); }}
        onOpenMore={() => setMobileMoreOpen(true)}
      />
      {/* Mobile more sheet — slides in from right */}
      {mobileMoreOpen && (
        <>
          <div className="mobile-more-overlay" onClick={() => setMobileMoreOpen(false)} />
          <div className="mobile-more-sheet">
            <div className="mobile-more-header">
              <span>More</span>
              <button onClick={() => setMobileMoreOpen(false)}>✕</button>
            </div>
            <button className="mobile-more-item" onClick={() => { if (!openTabs.includes('Notifications')) openTab('Notifications'); selectTab('Notifications'); setMobileMoreOpen(false); }}>
              🔔 Notifications
            </button>
            <button className="mobile-more-item" onClick={() => { if (!openTabs.includes('Groups')) openTab('Groups'); selectTab('Groups'); setMobileMoreOpen(false); }}>
              👥 Groups
            </button>
            <button className="mobile-more-item" onClick={() => { if (!openTabs.includes('Forums')) openTab('Forums'); selectTab('Forums'); setMobileMoreOpen(false); }}>
              💬 Forums
            </button>
            <button className="mobile-more-item" onClick={() => { if (!openTabs.includes('Exchange')) openTab('Exchange'); selectTab('Exchange'); setMobileMoreOpen(false); }}>
              🔁 Exchange
            </button>
            <button className="mobile-more-item" onClick={() => { if (!openTabs.includes('Events')) openTab('Events'); selectTab('Events'); setMobileMoreOpen(false); }}>
              📅 Events
            </button>
            <button className="mobile-more-item" onClick={() => { if (!openTabs.includes('Vault')) openTab('Vault'); selectTab('Vault'); setMobileMoreOpen(false); }}>
              🖼️ Vault
            </button>
            <button className="mobile-more-item" onClick={() => { if (!openTabs.includes('Account')) openTab('Account'); selectTab('Account'); setMobileMoreOpen(false); }}>
              🔗 Account
            </button>
            <button className="mobile-more-item" onClick={() => { if (!openTabs.includes('Settings')) openTab('Settings'); selectTab('Settings'); setMobileMoreOpen(false); }}>
              ⚙️ Settings
            </button>
            <button className="mobile-more-item" onClick={() => { handleInterests?.(); setMobileMoreOpen(false); }}>
              ⭐ Interests
            </button>
            <button className="mobile-more-item logout" onClick={() => { handleLogout?.(); setMobileMoreOpen(false); }}>
              🚪 Logout
            </button>
          </div>
        </>
      )}
    </div>
  );
}
