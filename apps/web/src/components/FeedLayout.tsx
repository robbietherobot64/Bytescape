import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import FeedColumn, { loadSavedInterests, loadCachedInterests, saveInterests, isOnboarded } from './FeedColumn';
import FeedCatalog from './FeedCatalog';
import { SourceSearch, SourcesPanel } from '@bytescape/events-ui';
import OAuthCallback from './OAuthCallback';
import NotificationBell from './NotificationBell';
import { feedApi, storiesApi, subscriptionApi, rssApi, profileApi, feedgenApi, settingsApi, api as authedFetch, type FeedPost, type StoryGroup, type Subscription, type RssSubscription } from '../utils/api';
import StoriesRail from './stories/StoriesRail';
import StoryViewer from './stories/StoryViewer';
import StoryComposer from './stories/StoryComposer';
import StoriesExplorer from './stories/StoriesExplorer';

const API = window.location.hostname === 'localhost' ? 'http://localhost:8080' : '';
import {
  getPrefs, pinLens, toggleStar,
  addSourceToLens, removeSourceFromLens, removeLens,
  type SavedLens, type ChipSource,
} from '../utils/feedPrefs';
import {
  loadWindowPref, saveWindowPref, windowQuery, WINDOW_OPTIONS,
  type WindowPref,
} from '../utils/feedWindow';
import { maxCreatedAtMs, planFeedMerge } from '../utils/feedMerge';

type FeedPhase = 'loading' | 'callback' | 'feed';
type ViewMode = 'feed' | 'interests';
type SourceView =
  | { kind: 'home' }
  | { kind: 'ephemeral'; key: string; label: string }
  | { kind: 'lens'; id: string; name: string; sources: string[] };

interface NativeFollow {
  id: string;
  label?: string;
  actor?: string;
  avatar?: string;
}

const CATEGORY_TABS: { id: 'following' | 'groups' | 'feeds' | 'hashtags'; icon: string; label: string }[] = [
  { id: 'following', icon: '👥', label: 'Following' },
  { id: 'groups', icon: '🏘️', label: 'Groups' },
  { id: 'feeds', icon: '📡', label: 'Feeds' },
  { id: 'hashtags', icon: '#️⃣', label: 'Hashtags' },
];
const CATEGORY_KINDS: Record<string, string[]> = {
  following: ['following', 'follows'],
  groups: ['group', 'groups'],
  feeds: ['feed', 'feeds'],
  hashtags: ['hashtag', 'hashtags'],
};

interface Props {
  onNavigate?: (tab: string) => void;
  onLogout?: () => void;
  onInterests?: () => void;
  activeTab?: string;
  interestsRequest?: number;
}

export default function FeedLayout({ onNavigate, onLogout, onInterests, activeTab, interestsRequest }: Props) {
  const [phase, setPhase] = useState<FeedPhase>('loading');
  const [posts, setPosts] = useState<FeedPost[]>([]);
  const [subscriptions, setSubscriptions] = useState<Subscription[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [feedMode, setFeedMode] = useState<'aggregated' | 'discover' | 'interests' | 'combined' | 'sources' | 'rss'>('aggregated');
  const [rssSubscriptions, setRssSubscriptions] = useState<RssSubscription[]>([]);
  const [expandedCats, setExpandedCats] = useState<Record<string, boolean>>({
    follows: false, feeds: false, hashtags: false, subscriptions: false, rss: false,
  });
  const [viewMode, setViewMode] = useState<ViewMode>('feed');
  // Opt-in FeedCatalog toggle — only ever set true by the "Discover feeds" button.
  const [showFeedCatalog, setShowFeedCatalog] = useState(false);
  const [activeFilters, setActiveFilters] = useState<Set<string>>(new Set());
  const [selectedSources, setSelectedSources] = useState<Set<string>>(new Set(['follows', 'feeds', 'hashtags', 'subscriptions', 'rss']));
  const [contentTypes, setContentTypes] = useState<string[]>(() => {
    try {
      return JSON.parse(localStorage.getItem('bytescape_feed_types') || '[]');
    } catch {
      return [];
    }
  });
  const [sortMode, setSortMode] = useState<string>(() => {
    const stored = localStorage.getItem('bytescape_feed_sort') || 'new';
    // D1 (feed-refocus): chronological is the default — trust-building Home.
    // Legacy migration: 'popular' was the old auto-default; the decision
    // reversed it, so migrate the stored default once (2026-10-11).
    if (stored === 'chronological' || stored === 'popular') {
      localStorage.setItem('bytescape_feed_sort', 'new');
      return 'new';
    }
    return stored;
  });
  // Saved source-mix preference (Settings → Feed & sources). Seeded 'all' as the
  // fast path; reconciled to the account-stored value on mount (below).
  const [sourceMix, setSourceMix] = useState<string>('all');

  // Platforms filter (mobile feed toolbar). Client-owned like sort/types: the
  // selected bucket persists in localStorage and is sent as `platform=` on the
  // combined feed query. Bucket keys mirror the backend's PLATFORM_BUCKETS.
  const [platformFilter, setPlatformFilter] = useState<string>(() => {
    const stored = localStorage.getItem('bytescape_feed_platform') || 'all';
    return ['bytescape', 'bluesky', 'mastodon', 'lemmy', 'pixelfed'].includes(stored) ? stored : 'all';
  });
  // Platform mix for the dropdown, delivered with the feed response. Counts are
  // scope-wide (independent of the selected bucket) so the mix stays visible
  // while a filter is active.
  const [platformCounts, setPlatformCounts] = useState<Record<string, number> | undefined>(undefined);

  // Time-period filter (Sort by popular needs a window). Persists like the
  // platform pref — preset plus custom from/to — and is sent on every feed call.
  const [windowPref, setWindowPref] = useState<WindowPref>(loadWindowPref);

  // Reconcile feed prefs to the account-stored values on mount: localStorage is
  // only the fast path — the server blob is the source of truth across devices.
  useEffect(() => {
    let alive = true;
    settingsApi.get().then((s) => {
      if (!alive) return;
      if (s.sort === 'new' || s.sort === 'popular') {
        localStorage.setItem('bytescape_feed_sort', s.sort);
        setSortMode(s.sort);
      }
      if (s.source_mix === 'all' || s.source_mix === 'bluesky' || s.source_mix === 'mastodon') {
        setSourceMix(s.source_mix);
      }
    }).catch(() => {});
    return () => { alive = false; };
  }, []);

  // ── Feed-refocus views: Home / saved lenses / ephemeral source tabs ──
  const [activeView, setActiveView] = useState<SourceView>({ kind: 'home' });
  const [categoryTab, setCategoryTab] = useState<'home' | 'following' | 'groups' | 'feeds' | 'hashtags'>('home');
  const [ephemeralTabs, setEphemeralTabs] = useState<Array<{ key: string; label: string }>>([]);
  const [viewPosts, setViewPosts] = useState<FeedPost[]>([]);
  const [viewLoading, setViewLoading] = useState(false);
  // Groups tab population (owner fix) — its own list, fetched from the
  // groups query; the merged home feed is never touched.
  const [groupsTabPosts, setGroupsTabPosts] = useState<FeedPost[]>([]);
  const [groupsTabLoading, setGroupsTabLoading] = useState(false);
  const [lenses, setLenses] = useState<SavedLens[]>(getPrefs().lenses);
  const [, setPrefsTick] = useState(0);

  const loadSourceView = async (keys: string[]) => {
    setViewLoading(true);
    try {
      // Source-scoped fetch: exactly this source — author keys use the author
      // filter (that person's posts), source keys scope the source pool.
      const authorKey = keys.find((k) => k.startsWith('author:'));
      const ids = keys.filter((k) => k !== 'follows:all' && !k.startsWith('author:'));
      const cats = keys.includes('follows:all')
        ? 'following'
        : keys.some((k) => k.startsWith('feed:'))
          ? 'feeds'
          : keys.some((k) => k.startsWith('hashtag:'))
            ? 'hashtags'
            : undefined;
      const wq = windowQuery(windowPref);
      const result = await feedApi.combined(
        ids,
        undefined,
        undefined,
        undefined,
        'new',
        authorKey ? authorKey.slice('author:'.length) : undefined,
        cats,
        undefined,
        platformFilter,
        wq.window,
        wq.from,
        wq.to,
      );
      setViewPosts(result.items || []);
      if (result.platform_counts) setPlatformCounts(result.platform_counts);
    } catch {
      setViewPosts([]);
    } finally {
      setViewLoading(false);
    }
  };

  const openEphemeral = (key: string, label: string) => {
    setEphemeralTabs((prev) => (prev.some((t) => t.key === key) ? prev : [...prev, { key, label }]));
    setActiveView({ kind: 'ephemeral', key, label });
    loadSourceView([key]);
  };

  // Chip taps open ephemeral source views
  useEffect(() => {
    const handler = (e: Event) => {
      const m = (e as CustomEvent).detail as ChipSource;
      const key = m.kind === 'hashtag' ? `hashtag:${m.label}` : m.kind === 'feed' ? `feed:${m.label}` : 'follows:all';
      const label = m.kind === 'following' ? 'Following' : m.kind === 'hashtag' ? `#${m.label}` : m.label;
      openEphemeral(key, label);
    };
    window.addEventListener('bytescape:openSourceView', handler);
    return () => window.removeEventListener('bytescape:openSourceView', handler);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Stories: rail + viewer/composer/explorer overlays ──
  const [railKey, setRailKey] = useState(0);
  const [storyViewer, setStoryViewer] = useState<{ groups: StoryGroup[]; index: number; item?: number } | null>(null);
  const [storyComposerOpen, setStoryComposerOpen] = useState(false);
  const [storyExplorerOpen, setStoryExplorerOpen] = useState(false);

  // Deep link from feed story-preview cards (window-event convention).
  useEffect(() => {
    const openStory = (e: Event) => {
      const uri = (e as CustomEvent<string>).detail;
      storiesApi.rail().then(async (r) => {
        for (let gi = 0; gi < r.groups.length; gi++) {
          const ii = r.groups[gi].items.findIndex((i) => i.uri === uri);
          if (ii >= 0) {
            setStoryViewer({ groups: r.groups, index: gi, item: ii });
            return;
          }
        }
        // Own stories (and other non-circle items) resolve directly.
        try {
          const item = await storiesApi.item(uri);
          setStoryViewer({
            groups: [{
              author: { did: item.author_did, handle: item.author_did, display_name: null, avatar: null },
              seen: false,
              items: [item],
            }],
            index: 0,
          });
        } catch {
          /* expired or gone — stay put */
        }
      }).catch(() => {});
    };
    window.addEventListener('bytescape:openStory', openStory);
    return () => window.removeEventListener('bytescape:openStory', openStory);
  }, []);

  const goHome = () => setActiveView({ kind: 'home' });

  const closeEphemeral = (key: string) => {
    setEphemeralTabs((prev) => prev.filter((t) => t.key !== key));
    if (activeView.kind === 'ephemeral' && activeView.key === key) setActiveView({ kind: 'home' });
  };

  // Pin persistence (Phase 2): the tab's source key resolves to its backing
  // subscription the same way the search dropdown's delete flow does — the
  // category item whose source_id matches the key carries the subscription id.
  // Non-fatal on failure: the local lens pin is the source of truth.
  const resolveSubscriptionIdForKey = (key: string): string | null => {
    // Only feeds/hashtags are subscription-backed rows; follows items carry
    // synthetic ids and pinned people don't map to savedFeedsPrefV2 anyway.
    for (const cat of categories) {
      if (cat.id !== 'feeds' && cat.id !== 'hashtags') continue;
      for (const item of cat.items) {
        if (item.source_id === key) return item.id;
      }
    }
    return null;
  };

  const persistPin = (key: string, pinned: boolean) => {
    const subId = resolveSubscriptionIdForKey(key);
    if (!subId) return;
    subscriptionApi.pin(subId, pinned).catch((e) => {
      console.warn(`[sources] failed to ${pinned ? 'pin' : 'unpin'} subscription ${subId}:`, e);
    });
  };

  // Pin an ephemeral tab: it becomes a saved lens (the ephemeral tab is
  // consumed — the lens tab takes its place) and the backing subscription is
  // marked pinned server-side.
  const pinEphemeral = (key: string, label: string) => {
    const lens = pinLens(label, [key]);
    if (lens) {
      setLenses(getPrefs().lenses);
      setPrefsTick((t) => t + 1);
    }
    const wasActive = activeView.kind === 'ephemeral' && activeView.key === key;
    closeEphemeral(key);
    if (wasActive && lens) {
      setActiveView({ kind: 'lens', id: lens.id, name: lens.name, sources: lens.sources });
    }
    persistPin(key, true);
  };

  const starActiveView = () => {
    if (activeView.kind === 'home') return;
    const key = activeView.kind === 'ephemeral' ? activeView.key : activeView.sources[0];
    toggleStar(key);
    setPrefsTick((t) => t + 1);
  };

  const openLens = (l: SavedLens) => {
    setActiveView({ kind: 'lens', id: l.id, name: l.name, sources: l.sources });
    loadSourceView(l.sources);
  };

  const addToActiveLens = (key: string) => {
    if (activeView.kind !== 'lens' || !key) return;
    addSourceToLens(activeView.id, key);
    const updated = getPrefs().lenses.find((l) => l.id === activeView.id);
    setLenses(getPrefs().lenses);
    setPrefsTick((t) => t + 1);
    if (updated) openLens(updated);
  };

  const removeFromActiveLens = (key: string) => {
    if (activeView.kind !== 'lens') return;
    removeSourceFromLens(activeView.id, key);
    const updated = getPrefs().lenses.find((l) => l.id === activeView.id);
    setLenses(getPrefs().lenses);
    setPrefsTick((t) => t + 1);
    if (updated && updated.sources.length > 0) openLens(updated);
    else if (updated) {
      removeLens(updated.id);
      setLenses(getPrefs().lenses);
      setActiveView({ kind: 'home' });
    }
  };

  // Source-category scoping for the active tab (Home = everything).
  const sourceCategories = useMemo(() => {
    if (categoryTab === 'home') return '';
    const backendMap: Record<string, string> = {
      following: 'following',
      feeds: 'feeds',
      hashtags: 'hashtags',
    };
    return backendMap[categoryTab] || '';
  }, [categoryTab]);

  const toggleSource = useCallback((categoryId: string) => {
    setSelectedSources(prev => {
      const next = new Set(prev);
      if (next.has(categoryId)) next.delete(categoryId); else next.add(categoryId);
      return next;
    });
  }, []);

  // Pagination state
  const [cursor, setCursor] = useState<string | undefined>(undefined);
  const [hasMore, setHasMore] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [prefetchedPage, setPrefetchedPage] = useState<FeedPost[] | null>(null);
  const sentinelRef = useRef<HTMLDivElement | null>(null);

  // Ref to always point at the latest loadMore (avoids stale event listener closure)
  const loadMoreRef = useRef<() => Promise<void>>(async () => {});

  // Keep a ref in sync with the latest cursor value so the pre-fetch inside loadMore
  // doesn't use a stale cursor from the useCallback closure.
  const cursorRef = useRef<string | undefined>(undefined);
  useEffect(() => { cursorRef.current = cursor; }, [cursor]);

  // Live mirror of the visible list for async batch classification: a batch
  // arriving after another merge must dedupe against the LATEST list, not the
  // one captured when its fetch started.
  const postsRef = useRef<FeedPost[]>(posts);
  useEffect(() => { postsRef.current = posts; }, [posts]);

  // Auto-load: reaching the bottom of the list loads the next page on its
  // own (the manual LOAD MORE button stays as a fallback).
  useEffect(() => {
    const el = sentinelRef.current;
    if (!el) return;
    const obs = new IntersectionObserver((entries) => {
      if (entries.some(e => e.isIntersecting)) {
        loadMoreRef.current();
      }
    }, { rootMargin: '300px' });
    obs.observe(el);
    return () => obs.disconnect();
  }, [posts.length, hasMore, loadingMore]);

  // Posts newer than the visible feed top, waiting for the user to return
  // to the top before being prepended (never shift content under a reader).
  const [pendingNew, setPendingNew] = useState<FeedPost[]>([]);
  // Popular sort: live arrivals are held out of the ranked list entirely and
  // surfaced through the "N new posts" affordance (click = refresh/re-sort).
  const [heldNew, setHeldNew] = useState<FeedPost[]>([]);
  useEffect(() => {
    if (pendingNew.length === 0) return;
    const onScroll = () => {
      if (window.scrollY < 50) {
        setPosts(prev => {
          const have = new Set(prev.map((p) => p.id));
          return [...pendingNew.filter((p) => !have.has(p.id)), ...prev];
        });
        setPendingNew([]);
      }
    };
    window.addEventListener('scroll', onScroll, { passive: true });
    return () => window.removeEventListener('scroll', onScroll);
  }, [pendingNew]);

  // Route one fetched batch into the list or the buffers per the active sort
  // and fetch kind. Live-refresh batches (the two-phase loader's "full"
  // fetch): arrivals insert at the top under New; under Popular the whole
  // unseen batch goes to the held "N new posts" counter — the ranked list is
  // never reshuffled in place. Cursor pages are pagination continuation and
  // append below under either sort. Id-dedup happens inside every state
  // updater so racing batches can never render a post twice. With
  // `deferAppend`, the continuation is NOT applied — the caller parks the
  // returned plan.append as the pre-fetched next page instead.
  const mergeBatch = useCallback((items: FeedPost[], opts: { mode: 'live' | 'page'; deferAppend?: boolean }) => {
    const plan = planFeedMerge(
      items,
      new Set(postsRef.current.map((p) => p.id)),
      maxCreatedAtMs(postsRef.current),
      sortMode,
      opts.mode,
    );
    if (plan.atTop.length > 0) {
      const arriving = plan.atTop;
      if (window.scrollY < 50) {
        setPosts((prev) => {
          const have = new Set(prev.map((p) => p.id));
          return [...arriving.filter((p) => !have.has(p.id)), ...prev];
        });
      } else {
        setPendingNew((prev) => {
          const have = new Set(prev.map((p) => p.id));
          return [...arriving.filter((p) => !have.has(p.id)), ...prev];
        });
      }
    }
    if (plan.append.length > 0 && !opts?.deferAppend) {
      const below = plan.append;
      setPosts((prev) => {
        const have = new Set(prev.map((p) => p.id));
        return [...prev, ...below.filter((p) => !have.has(p.id))];
      });
    }
    if (plan.held.length > 0) {
      const held = plan.held;
      setHeldNew((prev) => {
        const have = new Set(prev.map((p) => p.id));
        return [...prev, ...held.filter((p) => !have.has(p.id))];
      });
    }
    return plan;
  }, [sortMode]);

  const loadMore = useCallback(async () => {
    console.log('[loadMore] called —', { loadingMore, hasMore, cursor, prefetchedLength: prefetchedPage?.length ?? 0, activeFiltersSize: activeFilters.size });
    if (loadingMore || !hasMore) {
      console.log('[loadMore] guard blocked —', { loadingMore, hasMore });
      return;
    }
    setLoadingMore(true);
    try {
      // If we have a pre-fetched page from Phase 2, use it instantly
      if (prefetchedPage && prefetchedPage.length > 0) {
        console.log('[loadMore] using pre-fetched page (' + prefetchedPage.length + ' items)');
        // Sort-aware merge (parked pages are continuation by construction, so
        // this is a dedup + append — but classify anyway, never assume).
        const plan = mergeBatch(prefetchedPage, { mode: 'page' });
        console.log('[loadMore] after merge:', plan.append.length, 'appended out of', prefetchedPage.length);
        if (plan.append.length === 0) {
          // All pre-fetched posts are duplicates — stop pagination
          console.log('[loadMore] no new posts in pre-fetched page, setting hasMore=false');
          setHasMore(false);
        }
        setPrefetchedPage(null);  // consumed
        // Pre-fetch the next page in the background using the latest cursor.
        // Pass the cursor as both cursor and since — the backend uses `since` to
        // skip AppView posts already shown, and `cursor` for row-level pagination.
        const savedIds = await loadSavedInterests();
        const latestCursor = cursorRef.current;
        console.log('[loadMore] pre-fetching next page with cursor:', latestCursor);
        feedApi.combined(savedIds, latestCursor, undefined, latestCursor, sortMode, undefined, sourceCategories, contentTypes.join(',') || undefined, sourceMix, platformFilter, windowPref.preset, windowPref.from, windowPref.to).then(result => {
          // Live arrivals surface now (top / held counter); only true
          // continuation is parked as the next page.
          const next = mergeBatch(result.items, { mode: 'page', deferAppend: true });
          setPrefetchedPage(next.append.length > 0 ? next.append : null);
          setCursor(result.cursor || undefined);
          const cursorAdvanced = Boolean(result.cursor) && result.cursor !== latestCursor;
          setHasMore(result.has_more && (next.append.length > 0 || cursorAdvanced));
          console.log('[loadMore] pre-fetch done —', { newCursor: result.cursor, hasMore: result.has_more, items: result.items.length, parked: next.append.length });
        }).catch(e => {
          console.warn('[loadMore] pre-fetch failed:', e);
        });
      } else if (activeFilters.size > 0) {
        // Check for author/follows filters (collect ALL, not just first)
        const authorFilters = Array.from(activeFilters).filter(f => f.startsWith('author:') || f.startsWith('follows:'));
        if (authorFilters.length > 0) {
          const author = authorFilters.map(f =>
            f.startsWith('author:') ? f.slice(7) : f.slice(8)
          ).join(',');
          console.log('[loadMore] fetching author-filtered combined with cursor:', cursor, { author });
          const savedIds = await loadSavedInterests();
          const result = await feedApi.combined(savedIds, cursor, undefined, cursor, sortMode, author, sourceCategories, contentTypes.join(',') || undefined, sourceMix, platformFilter, windowPref.preset, windowPref.from, windowPref.to);
          const plan = mergeBatch(result.items, { mode: 'page' });
          setCursor(result.cursor || undefined);
          // Progress = new content OR a deeper server cursor. An all-dupe page
          // must not end pagination: popular's score-sorted pages overlap the
          // seed heavily, so dupes with a deeper cursor are still progress.
          const cursorAdvanced = Boolean(result.cursor) && result.cursor !== cursor;
          setHasMore(result.has_more && (plan.append.length > 0 || plan.atTop.length > 0 || plan.held.length > 0 || cursorAdvanced));
          console.log('[loadMore] author-filtered fetch done —', { newCursor: result.cursor, hasMore: result.has_more, items: result.items.length });
        } else {
          console.log('[loadMore] fetching filtered sources with cursor:', cursor);
          const sourcesStr = Array.from(activeFilters).join(',');
          const result = await feedApi.sources(sourcesStr, cursor);
          const plan = mergeBatch(result.items, { mode: 'page' });
          setCursor(result.cursor || undefined);
          // Progress = new content OR a deeper server cursor. An all-dupe page
          // must not end pagination: popular's score-sorted pages overlap the
          // seed heavily, so dupes with a deeper cursor are still progress.
          const cursorAdvanced = Boolean(result.cursor) && result.cursor !== cursor;
          setHasMore(result.has_more && (plan.append.length > 0 || plan.atTop.length > 0 || plan.held.length > 0 || cursorAdvanced));
          console.log('[loadMore] filtered fetch done —', { newCursor: result.cursor, hasMore: result.has_more, items: result.items.length });
        }
      } else {
        console.log('[loadMore] fetching normal combined with cursor:', cursor);
        const savedIds = await loadSavedInterests();
        // Pass cursor as both cursor and since to paginate past already-shown posts
        const result = await feedApi.combined(savedIds, cursor, undefined, cursor, sortMode, undefined, sourceCategories, contentTypes.join(',') || undefined, sourceMix, platformFilter, windowPref.preset, windowPref.from, windowPref.to);
        // Sort-aware merge: dedup + placement per the active sort (never raw-append)
        const plan = mergeBatch(result.items, { mode: 'page' });
        console.log('[loadMore] combined fetch done —', { newCursor: result.cursor, hasMore: result.has_more, items: result.items.length, appended: plan.append.length });
        setCursor(result.cursor || undefined);
        // Progress = new content OR a deeper server cursor. An all-dupe page
        // must not end pagination: popular's score-sorted pages overlap the
        // seed heavily, so dupes with a deeper cursor are still progress.
        const cursorAdvanced = Boolean(result.cursor) && result.cursor !== cursor;
        setHasMore(result.has_more && (plan.append.length > 0 || plan.atTop.length > 0 || plan.held.length > 0 || cursorAdvanced));
      }
    } catch (e) {
      console.warn('[loadMore] Failed to load more:', e);
    } finally {
      setLoadingMore(false);
    }
  }, [loadingMore, hasMore, cursor, activeFilters, prefetchedPage, sortMode, sourceCategories, contentTypes, sourceMix, platformFilter, windowPref, mergeBatch]);

  // Keep the ref pointing at the latest loadMore on every render
  loadMoreRef.current = loadMore;

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    if (params.get('provider') && params.get('success')) {
      setPhase('callback');
      return;
    }
  }, []);

  // Open the interests editor when requested. Prop signal. `interestsRequest`
  // is a monotonic counter that stays elevated after the first open, so on a
  // Feed remount (tab switch) the old value would re-open the picker every
  // time. Seed the ref with the current value so a remount never treats an
  // existing counter as "new"; only a fresh increment opens the picker.
  const lastInterestsRequest = useRef(interestsRequest);
  useEffect(() => {
    if (interestsRequest && interestsRequest !== lastInterestsRequest.current) {
      lastInterestsRequest.current = interestsRequest;
      setViewMode('interests');
    }
  }, [interestsRequest]);

  // The catalog is strictly opt-in: leaving the feeds view (or opening the
  // picker) resets the toggle so it can never re-open without a fresh click.
  useEffect(() => {
    setShowFeedCatalog(false);
  }, [categoryTab, viewMode, activeView.kind]);

  const [nativeFollows, setNativeFollows] = useState<NativeFollow[]>([])

  const loadSubscriptions = useCallback(async () => {
    // Speed pass (2026-11-17): the three fetches are independent — run them
    // concurrently instead of serially so the sidebar fills in one round-trip
    // window. Per-call error swallow preserved.
    const [subs, rssSubs, follows] = await Promise.all([
      subscriptionApi.list().catch(() => null),
      rssApi.list().catch(() => null),
      authedFetch<{ follows: NativeFollow[] }>('/api/feed/follows').catch(() => null),
    ]);
    if (subs) {
      // Dead-platform purge: Mastodon/Lemmy-era subscriptions carry no content
      // on this AT Protocol platform (AD-00) — hide them everywhere. EXEMPT
      // bootstrap_hashtag rows: they reuse the 'mastodon:{tag}' source_id format
      // for tag keys but are native interest hashtags, not Mastodon sources.
      setSubscriptions(subs.filter(s => {
        if (s.source === 'bootstrap_hashtag') return true;
        return !s.source_id.startsWith('mastodon:') && !s.source_id.startsWith('lemmy:');
      }));
    }
    if (rssSubs) setRssSubscriptions(rssSubs);
    if (follows) setNativeFollows(follows.follows || []);
  }, []);

  const loadFeed = useCallback(async () => {
    setLoading(true);
    setError('');
    setCursor(undefined);
    setPosts([]);
    // Stale batch buffers must never bleed into a fresh load.
    setPrefetchedPage(null);
    setPendingNew([]);
    setHeldNew([]);

    // If we have active filters, check for author/follows filters first
    if (activeFilters.size > 0) {
      const authorFilters = Array.from(activeFilters).filter(f => f.startsWith('author:') || f.startsWith('follows:'));
      if (authorFilters.length > 0) {
        // Extract author identifiers from ALL matching filters
        const author = authorFilters.map(f =>
          f.startsWith('author:') ? f.slice(7) : f.slice(8) // Remove prefix
        ).join(',');
        try {
          const savedIds = await loadSavedInterests();
          const seed = await feedApi.combined(savedIds, undefined, 'seed', undefined, sortMode, author, sourceCategories, contentTypes.join(',') || undefined, sourceMix, platformFilter, windowPref.preset, windowPref.from, windowPref.to);
          setPosts(seed.items);
          if (seed.platform_counts) setPlatformCounts(seed.platform_counts);
          setFeedMode('combined');
          setCursor(seed.cursor || undefined);
          setHasMore(seed.has_more);
          setLoading(false);
          // Phase 2 pre-fetch in background
          try {
            const full = await feedApi.combined(savedIds, undefined, 'full', seed.cursor || undefined, sortMode, author, sourceCategories, contentTypes.join(',') || undefined, sourceMix, platformFilter, windowPref.preset, windowPref.from, windowPref.to);
            // Sort-aware merge: arrivals surface now (top / held counter),
            // only true continuation is parked as the pre-fetched next page.
            const plan = mergeBatch(full.items, { mode: 'live', deferAppend: true });
            setPrefetchedPage(plan.append.length > 0 ? plan.append : null);
            setCursor(full.cursor || seed.cursor || undefined);
            setHasMore(full.has_more);
          } catch (e) {
            console.warn('Phase 2 pre-fetch failed:', e);
            setCursor(seed.cursor);
            setHasMore(true);
          }
        } catch (e: any) {
          setError(e.message || 'Failed to load feed');
          setPosts([]);
          setLoading(false);
        }
        return;
      }
      // Non-author filters — use sources endpoint
      try {
        const sourcesStr = Array.from(activeFilters).join(',');
        const result = await feedApi.sources(sourcesStr);
        setPosts(result.items);
        setFeedMode('sources');
        setCursor(result.cursor || undefined);
        setHasMore(result.has_more);
      } catch (e: any) {
        setError(e.message || 'Failed to load feed');
        setPosts([]);
      } finally {
        setLoading(false);
      }
      return;
    }

    // No filters — two-phase combined feed from interests
    const savedIds = await loadSavedInterests();
    const hasSidebarSources = subscriptions.length > 0 || rssSubscriptions.length > 0;

    if (savedIds.length === 0 && hasSidebarSources) {
      // Sources exist but no interests chosen yet. Never show unpersonalized
      // global/discover posts here — that leaked random content into the feed.
      // Show an empty state prompting the user to pick interests.
      setPosts([]);
      setFeedMode('combined');
      setHasMore(false);
      setLoading(false);
      return;
    }

    // PHASE 1 — seed (block UI, must be <500ms)
    let seedCursor: string | undefined;
    try {
      const seed = await feedApi.combined(savedIds, undefined, 'seed', undefined, sortMode, undefined, sourceCategories, contentTypes.join(',') || undefined, sourceMix, platformFilter, windowPref.preset, windowPref.from, windowPref.to);
      setPosts(seed.items);
      if (seed.platform_counts) setPlatformCounts(seed.platform_counts);
      seedCursor = seed.cursor || undefined;
      setCursor(seedCursor);
      setHasMore(seed.has_more);
      setFeedMode('combined');
      setLoading(false);  // first paint happens here
    } catch (e: any) {
      // Seed failed — surface the error, never fall back to unpersonalized
      // discover/global posts (that leaked random content into the feed).
      setError(e.message || 'Failed to load feed');
      setPosts([]);
      setLoading(false);
      return;
    }

    // PHASE 2 — full (pre-fetch next page in background, DON'T merge into visible feed)
    // The full results are stored as a pre-fetched "next page" that loads seamlessly
    // when the user scrolls to the bottom, instead of jarringly replacing the feed.
    // Detached: loadFeed resolves at first paint — the full pipeline must not
    // block callers (it kept the interests save on the "Setting up your feed"
    // screen for half a minute).
    (async () => {
      try {
        const full = await feedApi.combined(savedIds, undefined, 'full', seedCursor, sortMode, undefined, sourceCategories, contentTypes.join(',') || undefined, sourceMix, platformFilter, windowPref.preset, windowPref.from, windowPref.to);
        // Sort-aware merge: live arrivals go to the top under New and into the
        // held "N new posts" counter under Popular — never appended behind the
        // loaded list. Only true continuation is parked as the next page.
        const plan = mergeBatch(full.items, { mode: 'live', deferAppend: true });
        setPrefetchedPage(plan.append.length > 0 ? plan.append : null);
        setCursor(full.cursor || seedCursor);
        setHasMore(full.has_more);
      } catch (e) {
        console.warn('Phase 2 pre-fetch failed, continuing with seed only:', e);
        // Keep seed cursor for scroll-based pagination
        setCursor(seedCursor);
        setHasMore(true);
      }
    })();
  }, [activeFilters, sortMode, sourceCategories, contentTypes, sourceMix, platformFilter, windowPref, mergeBatch]);

  // After a generated feed / combo subscribe: refresh sidebar subs + the feed
  const refreshFeedgen = useCallback(async () => {
    await loadSubscriptions();
    loadFeed();
  }, [loadSubscriptions, loadFeed]);

  useEffect(() => {
    const handler = () => {
      loadSubscriptions();
      loadFeed();
    };
    const refreshHandler = () => loadFeed();
    // When profile is updated (e.g. interests changed), re-sync interests from backend and reload feed
    const profileUpdatedHandler = async () => {
      await loadSavedInterests();
      loadFeed();
    };
    // Use loadMoreRef so the handler always calls the latest loadMore (avoids stale closure)
    const loadMoreHandler = () => { console.log('[event] bytescape:loadMore received'); loadMoreRef.current(); };
    window.addEventListener('bytescape:accountLinked', handler);
    window.addEventListener('bytescape:refreshFeed', refreshHandler);
    window.addEventListener('bytescape:loadMore', loadMoreHandler);
    window.addEventListener('bytescape:profileUpdated', profileUpdatedHandler);
    return () => {
      window.removeEventListener('bytescape:accountLinked', handler);
      window.removeEventListener('bytescape:refreshFeed', refreshHandler);
      window.removeEventListener('bytescape:loadMore', loadMoreHandler);
      window.removeEventListener('bytescape:profileUpdated', profileUpdatedHandler);
    };
  }, [loadSubscriptions, loadFeed]);

  useEffect(() => {
    // Speed pass (2026-11-17): every startup fetch is independent — fire them
    // concurrently and run the gate + feed load the moment the profile answers
    // (isOnboarded/loadSavedInterests share one single-flight /api/profile/me).
    // The old chain waited for subscriptions → rss → follows → onboarded →
    // interests (5 serial round trips) before the seed fetch even started; the
    // sidebar now fills independently while the feed loads. loadSubscriptions
    // fetches + filters subscriptions (dead-platform purge) and also fills the
    // RSS state — do NOT add an unfiltered subscriptionApi.list() here: that
    // raced the filtered load and re-showed purged Mastodon-era rows.
    loadSubscriptions();
    // Onboarding gate: a not-onboarded account (server `users.onboarded`
    // false) sees the interests picker, not the global feed. This is checked
    // against the server, not localStorage, so a re-created account always
    // onboards. No interest auto-migration here — a fresh account must pick
    // its own interests rather than inherit a stale browser cache.
    isOnboarded().then((onboarded) => {
      if (!onboarded) {
        setViewMode('interests');
        setLoading(false);
        return;
      }
      loadFeed();
    });
  }, [loadFeed]);

  const handleOAuthDone = useCallback(() => {
    setPhase('feed');
    loadFeed();
  }, [loadFeed]);

  // The picker batch-wrote the selection already (PUT + bootstrap via
  // saveInterests) — just refresh subscriptions + feed and close the picker.
  const handleInterestsSelected = useCallback(async (_ids: string[]) => {
    // Close the picker even if a refresh fails — a thrown load must never
    // trap the user on the interests screen.
    try {
      await loadSubscriptions();
      await loadFeed();
    } finally {
      setViewMode('feed');
    }
  }, [loadSubscriptions, loadFeed]);

  if (phase === 'callback') {
    return <OAuthCallback onDone={handleOAuthDone} />;
  }

  const toggleExpand = (id: string) => {
    setExpandedCats(prev => ({ ...prev, [id]: !prev[id] }));
  };

  // Build source_id for each item
  const buildSourceId = (sub: Subscription): string => {
    if (sub.category === 'hashtags') {
      // Strip "mastodon:" prefix if present (DB stores "mastodon:tagname")
      const tag = sub.source_id.replace('#', '').replace(/^mastodon:/, '');
      return `hashtag:${tag}`;
    }
    if (sub.category === 'feeds') {
      if (sub.source_id.startsWith('at://')) return `feed:${sub.source_id}`;
      // Text-based feeds are search terms — use as hashtag (the interest's keyword)
      const tag = sub.source_id.replace('#', '').replace(/^mastodon:/, '');
      // Multi-word search terms: use the first word as the hashtag
      const firstWord = tag.split(/[\s,]+/)[0];
      return `hashtag:${firstWord}`;
    }
    if (sub.category === 'follows') return `follows:${sub.source_id}`;
    if (sub.category === 'subscriptions') return `follows:${sub.source_id}`;
    return sub.source_id;
  };

  const handleDeleteItem = async (categoryId: string, itemId: string) => {
    // Delete from subscriptions
    try {
      await subscriptionApi.delete(itemId);
      setSubscriptions(prev => prev.filter(s => s.id !== itemId));
    } catch {}
    // Also remove from active filters
    const sub = subscriptions.find(s => s.id === itemId);
    if (sub) {
      const sourceId = buildSourceId(sub);
      setActiveFilters(prev => {
        const next = new Set(prev);
        next.delete(sourceId);
        return next;
      });
    }
  };

  const openFromMenu = (categoryId: string, itemId: string) => {
    // Left menu = browse & build: opening a source spawns a top tab
    // (ephemeral, pinnable) — it is never a silent filter change.
    for (const cat of categories) {
      const item = cat.items.find((i) => i.id === itemId);
      if (item) {
        const key = item.source_id as string;
        if (key.startsWith('follows:')) {
          const actor = key.slice('follows:'.length);
          const handle = actor.split('/').pop() || actor;
          openEphemeral(`author:@${handle.replace(/^@/, '')}`, item.label as string);
        } else {
          openEphemeral(key, item.label as string);
        }
        return;
      }
    }
  };

  // Search handler — searches per category
  const handleSearch = async (categoryId: string, query: string): Promise<any[]> => {
    if (categoryId === 'follows') {
      // Search Bluesky actors
      try {
        const results = await feedApi.searchPeople(query);
        return results;
      } catch { return []; }
    } else if (categoryId === 'hashtags') {
      // Search Bluesky AppView hashtags
      try {
        const results = await feedApi.searchHashtags(query);
        return results;
      } catch { return []; }
    } else if (categoryId === 'feeds') {
      // Search Bluesky feed generators
      try {
        const results = await feedApi.searchFeeds(query);
        return results;
      } catch { return []; }
    } else if (categoryId === 'rss') {
      // RSS — just return the URL as a searchable result if it looks like a URL
      if (query.startsWith('http://') || query.startsWith('https://')) {
        return [{
          label: query,
          source_id: query,
          description: 'Click + to subscribe to this RSS feed',
        }];
      }
      return [];
    }
    return [];
  };

  // Global search — searches across all platforms concurrently
  const handleGlobalSearch = async (query: string, mode: string = 'all'): Promise<any[]> => {
    const results: any[] = [];
    if (mode === 'all' || mode === 'people') {
      try { results.push(...await feedApi.searchPeople(query)); } catch {}
    }
    if (mode === 'all' || mode === 'hashtags') {
      try { results.push(...await feedApi.searchHashtags(query)); } catch {}
    }
    if (mode === 'all' || mode === 'feeds') {
      try { results.push(...await feedApi.searchFeeds(query)); } catch {}
    }
    // Groups (Lemmy/PieFed communities over AP) — design D7.
    if (mode === 'all' || mode === 'groups') {
      try { results.push(...await feedApi.searchGroups(query)); } catch {}
    }
    // Deduplicate by source_id
    const seen = new Set();
    return results.filter(item => {
      const key = item.source_id || item.id || item.tag || item.uri || item.did;
      if (!key || seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  };

  // Add a source from search results
  const handleAddSource = async (categoryId: string, source: any) => {
    try {
      if (categoryId === 'rss') {
        // RSS — subscribe via rssApi
        await rssApi.create(source.source_id || source.label);
      } else {
        await subscriptionApi.create({
          category: categoryId,
          source_id: source.source_id || source.id || source.tag || source.uri || source.did,
          label: source.label || source.handle || source.name || source.tag || source.displayName,
          source: categoryId === 'follows' ? 'bluesky_follow' : categoryId === 'hashtags' ? 'hashtag' : 'bluesky_feed',
        });
      }
      await loadSubscriptions();

      // If it's a hashtag, also save as an interest so the feed picks it up
      if (categoryId === 'hashtags') {
        const sourceId = source.source_id || source.id || source.tag || source.uri || source.did;
        if (sourceId) {
          const raw = localStorage.getItem('bytescape_interests');
          const current: string[] = raw ? JSON.parse(raw) : [];
          if (!current.includes(sourceId)) {
            current.push(sourceId);
            await saveInterests(current);
          }
        }
      }

      window.dispatchEvent(new CustomEvent('bytescape:toast', { detail: { message: 'Added to your feed!', type: 'success' } }));
      // Clear search
      setSearchTexts_helper(categoryId);
    } catch (e) {
      console.warn('Failed to add source:', e);
      window.dispatchEvent(new CustomEvent('bytescape:toast', { detail: { message: 'Failed to add source', type: 'error' } }));
    }
  };

  // Global add — determines category from source type, then delegates to handleAddSource.
  // A group result deep-links to the CommunityScreen (which offers follow) —
  // the global bar is the ONLY search surface (owner round 3).
  const handleGlobalAdd = async (source: any) => {
    const groupActorId = (source.source_id || source.uri || '').replace(/^groups:/, '');
    if (source.source_type === 'community' || source.instance || groupActorId.startsWith('http')) {
      if (groupActorId.startsWith('http')) {
        window.dispatchEvent(new CustomEvent('bytescape:viewCommunity', { detail: { actorId: groupActorId } }));
        return;
      }
    }
    let categoryId = 'follows';
    const label = source.label || '';
    if (label.startsWith('#') || source.source_type === 'hashtag' || source.tag) categoryId = 'hashtags';
    else if (source.source_type === 'feed' || source.uri?.startsWith('at://')) categoryId = 'feeds';
    else if (source.source_type === 'community' || source.instance) categoryId = 'subscriptions';
    await handleAddSource(categoryId, source);
  };

  const [searchTexts_state, setSearchTexts_state] = useState<Record<string, string>>({});
  const setSearchTexts_helper = (catId: string) => {
    setSearchTexts_state(prev => ({ ...prev, [catId]: '' }));
  };

  // Build categories for the source-selection sidebar (AP-era categories —
  // Communities/Lemmy — removed with AD-00; platform select removed 2026-10-11).
  const categories = [
    {
      id: 'follows',
      label: 'Following',
      icon: '👤',
      searchPlaceholder: 'people/actors on Bluesky',
      expanded: !!expandedCats['follows'],
      items: [
        ...nativeFollows.map(f => ({
          id: f.id,
          label: f.label || f.actor?.split('/').pop() || 'Unknown',
          enabled: true,
          source_type: 'follows',
          source_id: `follows:${f.actor}`,
          avatar: f.avatar,
        })),
        ...subscriptions.filter(s => s.category === 'follows').map(s => ({
          id: s.id,
          label: s.label,
          enabled: true,
          source_type: 'follows',
          source_id: buildSourceId(s),
        })),
      ],
    },
    {
      id: 'feeds',
      label: 'Feeds',
      icon: '📡',
      searchPlaceholder: 'Bluesky feeds + generators',
      expanded: !!expandedCats['feeds'],
      items: subscriptions.filter(s => s.category === 'feeds').map(s => ({
        id: s.id,
        label: s.label,
        enabled: true,
        source_type: 'feed',
        source_id: buildSourceId(s),
      })),
    },
    {
      id: 'hashtags',
      label: 'Hashtags',
      icon: '#',
      searchPlaceholder: 'hashtags',
      expanded: !!expandedCats['hashtags'],
      items: subscriptions.filter(s => s.category === 'hashtags').map(s => ({
        id: s.id,
        label: s.label,
        enabled: true,
        source_type: 'hashtag',
        source_id: buildSourceId(s),
      })),
    },
    {
      id: 'groups',
      label: 'Groups',
      icon: '👥',
      searchPlaceholder: 'search groups (global bar)',
      expanded: !!expandedCats['groups'],
      items: subscriptions.filter(s => s.category === 'groups').map(s => ({
        id: s.id,
        label: s.label,
        enabled: true,
        source_type: 'community',
        source_id: `groups:${s.source_id}`,
      })),
    },
  ];

  const changeSort = (newSort: string) => {
    localStorage.setItem('bytescape_feed_sort', newSort);
    setLoading(true);
    setPosts([]);
    setHasMore(false);
    setPrefetchedPage(null);
    setPendingNew([]);
    setHeldNew([]);
    setCursor(undefined);
    setSortMode(newSort);
  };

  // Platforms filter: same reset contract as sort — drop the current page so
  // the refetch starts clean at the first page for the new bucket.
  const changePlatform = (next: string) => {
    localStorage.setItem('bytescape_feed_platform', next);
    setLoading(true);
    setPlatformCounts(undefined);
    setPosts([]);
    setHasMore(false);
    setPrefetchedPage(null);
    setPendingNew([]);
    setHeldNew([]);
    setCursor(undefined);
    setPlatformFilter(next);
  };

  // Time-period filter: filter first, then sort — same clean-page reset as
  // sort/platform so the refetch starts from the first page of the window.
  const changeWindow = (next: WindowPref) => {
    saveWindowPref(next);
    setLoading(true);
    setPosts([]);
    setHasMore(false);
    setPrefetchedPage(null);
    setPendingNew([]);
    setHeldNew([]);
    setCursor(undefined);
    setWindowPref(next);
  };

  // "N new posts" affordance. New sort: insert the deferred arrivals at the
  // top now. Popular sort: the ranked list is never reshuffled in place —
  // reload so every post re-sorts into its true rank.
  const showNewPosts = () => {
    if (sortMode === 'new') {
      const arriving = pendingNew;
      setPendingNew([]);
      setPosts((prev) => {
        const have = new Set(prev.map((p) => p.id));
        return [...arriving.filter((p) => !have.has(p.id)), ...prev];
      });
    } else {
      setHeldNew([]);
      setPendingNew([]);
      loadFeed();
    }
  };

  // Type filter (dropdown next to sort): one bucket or all content.
  const changeType = (v: string) => {
    const next = v === 'all' ? [] : [v];
    localStorage.setItem('bytescape_feed_types', JSON.stringify(next));
    setCursor(undefined);
    setContentTypes(next);
  };

  // Groups tab population: the groups surface's interest-first, never-blank
  // query (/api/feed/groups — the same population the Groups section uses),
  // fetched whenever the tab activates. NOT a client filter over the merged
  // feed — that pool carries no group posts unless group sources are
  // selected, which made this tab structurally empty (owner fix).
  useEffect(() => {
    if (categoryTab !== 'groups') return;
    let alive = true;
    setGroupsTabLoading(true);
    feedApi
      .groupScope('home', 50)
      .then((res) => { if (alive) setGroupsTabPosts(res.items || []); })
      .catch(() => { if (alive) setGroupsTabPosts([]); })
      .finally(() => { if (alive) setGroupsTabLoading(false); });
    return () => { alive = false; };
  }, [categoryTab]);

  // Posts for the current category tab — hoisted so the empty feeds tab can
  // swap in the generated feeds catalog.
  const tabPosts = categoryTab === 'home'
    ? (activeView.kind === 'home' ? posts : viewPosts)
    : categoryTab === 'groups' && activeView.kind === 'home'
      ? groupsTabPosts
      : (activeView.kind === 'home' ? posts : viewPosts).filter((p) =>
        (CATEGORY_KINDS[categoryTab] || []).some(
          (k) =>
            p.matched_sources?.some((m) => m.kind === k) ||
            (categoryTab === 'following' && p.is_following) ||
            (categoryTab === 'groups' && !!p.group)
        )
      );
  const tabLoading = activeView.kind === 'home'
    ? (categoryTab === 'groups' ? groupsTabLoading || loading : loading)
    : viewLoading;
  const inFeedsView = categoryTab === 'feeds' && activeView.kind === 'home' && viewMode === 'feed';
  const showDiscoverButton = inFeedsView && !tabLoading && !error && tabPosts.length === 0;

  return (
    <div className="feed-layout">
      <div className="feed-main" style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', minHeight: 0, overflow: 'hidden' }}>
        {/* Feed-refocus tab strip: Home (always first) / saved lenses / ephemeral tabs */}
        <div className="feed-tabstrip">
          <button
            className={`btn feed-tab${activeView.kind === 'home' && categoryTab === 'home' ? ' active' : ''}`}
            onClick={() => { setCategoryTab('home'); goHome(); }}
          >
            🏠 Home
          </button>
          {CATEGORY_TABS.map((ct) => (
            <button
              key={ct.id}
              className={`btn feed-tab${activeView.kind === 'home' && categoryTab === ct.id ? ' active' : ''}`}
              onClick={() => {
                if (activeView.kind !== 'home') goHome();
                setCategoryTab(categoryTab === ct.id ? 'home' : ct.id);
              }}
            >
              {ct.icon} {ct.label}
            </button>
          ))}
          {lenses.map((l) => (
            <span key={l.id} className="feed-tab-group">
              <button
                className={`btn feed-tab feed-tab-lens${activeView.kind === 'lens' && activeView.id === l.id ? ' active' : ''}`}
                onClick={() => openLens(l)}
                title={`Saved lens: ${l.sources.join(', ')}`}
              >
                🔍 {l.name}
              </button>
              <button
                className="feed-tab-close"
                title="Remove lens"
                onClick={() => {
                  // Unpin: the lens's backing subscriptions lose their pinned flag.
                  l.sources.forEach((k) => persistPin(k, false));
                  removeLens(l.id);
                  setLenses(getPrefs().lenses);
                  if (activeView.kind === 'lens' && activeView.id === l.id) setActiveView({ kind: 'home' });
                }}
              >
                ×
              </button>
            </span>
          ))}
          {ephemeralTabs.map((t) => (
            <span key={t.key} className="feed-tab-group">
              <button
                className={`btn feed-tab feed-tab-ephemeral${activeView.kind === 'ephemeral' && activeView.key === t.key ? ' active' : ''}`}
                onClick={() => openEphemeral(t.key, t.label)}
              >
                {t.label}
              </button>
              <button
                className="feed-tab-pin"
                title="Pin as saved lens"
                onClick={() => pinEphemeral(t.key, t.label)}
              >
                📌
              </button>
              <button
                className="feed-tab-close"
                title="Close"
                onClick={() => closeEphemeral(t.key)}
              >
                ×
              </button>
            </span>
          ))}
          {activeView.kind !== 'home' && (
            <span className="feed-tab-controls">
              <button className="btn" onClick={starActiveView} title="Star this source — its posts jump the queue"
                style={{ fontSize: 11, padding: '2px 10px' }}>
                ★ Star
              </button>
              {activeView.kind === 'lens' && (
                <>
                  <select
                    className="btn"
                    value=""
                    onChange={(e) => addToActiveLens(e.target.value)}
                    title="Add a source to this lens"
                    style={{ fontSize: 11, padding: '2px 6px' }}
                  >
                    <option value="">+ source</option>
                    {subscriptions.map((s) => {
                      const k = buildSourceId(s);
                      return !activeView.sources.includes(k) ? (
                        <option key={k} value={k}>{s.source_id}</option>
                      ) : null;
                    })}
                  </select>
                  {activeView.sources.map((s) => (
                    <button
                      key={s}
                      onClick={() => removeFromActiveLens(s)}
                      title="Remove from lens"
                      style={{ fontSize: 10, padding: '2px 8px', color: 'var(--text-muted)' }}
                    >
                      − {s.replace(/^(hashtag|feed|follows):/, '')}
                    </button>
                  ))}
                </>
              )}
            </span>
          )}
        </div>

        <StoriesRail
          key={railKey}
          onOpenGroup={(groups, index) => setStoryViewer({ groups, index })}
          onOpenComposer={() => setStoryComposerOpen(true)}
          onExplore={() => setStoryExplorerOpen(true)}
        />
        {storyViewer && (
          <StoryViewer
            groups={storyViewer.groups}
            startGroup={storyViewer.index}
            startItem={storyViewer.item ?? 0}
            onClose={() => setStoryViewer(null)}
          />
        )}
        {storyComposerOpen && (
          <StoryComposer
            onClose={() => setStoryComposerOpen(false)}
            onPublished={() => setRailKey((k) => k + 1)}
          />
        )}
        {storyExplorerOpen && (
          <StoriesExplorer
            onOpenGroup={(groups, index) => setStoryViewer({ groups, index })}
            onClose={() => setStoryExplorerOpen(false)}
          />
        )}

        {/* Action bar — spec 2026-10-14: sort dropdown (New default), compose
            pinned right. Interests gear lives on the "Your sources" header in
            the right column; search lives above the sources panel. */}
        <div
          className={`feed-actionbar${showDiscoverButton ? '' : ' feed-actionbar-compose-only'}`}
          style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '2px 0 8px' }}
        >
          {showDiscoverButton && (
            <button
              className="btn"
              onClick={() => setShowFeedCatalog(v => !v)}
              title={showFeedCatalog ? 'Back to your feed' : 'Browse generated topic feeds'}
            >
              {showFeedCatalog ? '< Back to feed' : 'Discover feeds'}
            </button>
          )}
          {/* Owner spec (2026-11-17): content type first, then sort, then the
              time period the sort/filter runs over. */}
          <select
            className="feed-sort-select"
            value={contentTypes[0] || 'all'}
            onChange={(e) => changeType(e.target.value)}
            title="Filter by content type"
          >
            <option value="all">✨ All content</option>
            <option value="text">📝 Text posts</option>
            <option value="blog">✏️ Blog posts</option>
            <option value="image">📷 Images</option>
            <option value="video">▶️ Videos</option>
            <option value="link">🔗 Links</option>
            <option value="event">📅 Events</option>
            <option value="market">🛒 Marketplace</option>
            <option value="live">📡 Live streams</option>
            <option value="audio">🎙️ Audio</option>
            <option value="poll">🗳️ Polls</option>
            <option value="list">📋 Lists</option>
            <option value="other">⚙️ Other</option>
          </select>
          <select
            className="feed-sort-select"
            value={sortMode || 'new'}
            onChange={(e) => changeSort(e.target.value)}
            title="Sort feed"
          >
            <option value="new">New</option>
            <option value="popular">Popular</option>
          </select>
          <select
            className="feed-sort-select feed-window-select"
            value={windowPref.preset}
            onChange={(e) => changeWindow({ ...windowPref, preset: e.target.value as WindowPref['preset'] })}
            title="Time period"
          >
            {WINDOW_OPTIONS.map((opt) => (
              <option key={opt.value} value={opt.value}>{opt.label}</option>
            ))}
          </select>
          {windowPref.preset === 'custom' && (
            <span className="feed-window-custom" title="Custom date range (UTC; empty end = open-ended)">
              <input
                type="date"
                aria-label="From date"
                value={windowPref.from}
                onChange={(e) => changeWindow({ ...windowPref, from: e.target.value })}
              />
              <span className="feed-window-custom-sep">–</span>
              <input
                type="date"
                aria-label="To date"
                value={windowPref.to}
                onChange={(e) => changeWindow({ ...windowPref, to: e.target.value })}
              />
            </span>
          )}
          <button
            className="btn feed-compose-btn"
            style={{
              marginLeft: 'auto',
              background: 'var(--accent)', color: '#000', fontWeight: 700,
              borderRadius: 999, padding: '8px 16px', border: 'none',
              boxShadow: '0 2px 12px rgba(0,0,0,0.4)', cursor: 'pointer',
            }}
            onClick={() => window.dispatchEvent(new Event('bytescape:openComposer'))}
            title="New post"
          >
            ✎ New post
          </button>
        </div>

      {(heldNew.length > 0 || pendingNew.length > 0) && (
        <button
          className="feed-new-posts-pill"
          onClick={showNewPosts}
          title={sortMode === 'new' ? 'Insert the new posts at the top' : 'Refresh and re-rank the feed'}
        >
          ⬆ {heldNew.length + pendingNew.length} new post{heldNew.length + pendingNew.length === 1 ? '' : 's'}
        </button>
      )}

      {showFeedCatalog && inFeedsView && !error && tabPosts.length === 0 ? (
        <FeedCatalog onSubscribed={refreshFeedgen} />
      ) : (
        <FeedColumn
          posts={tabPosts}
          loading={tabLoading}
          error={error}
          feedMode={feedMode}
          viewMode={viewMode}
          loadingMore={activeView.kind === 'home' ? loadingMore : false}
          hasMore={activeView.kind === 'home' ? hasMore : false}
          sentinelRef={sentinelRef}
          onRetry={loadFeed}
          onInterestsSelected={handleInterestsSelected}
          onCloseInterests={() => setViewMode('feed')}
          activeFilters={activeFilters}
          onClearFilters={() => setActiveFilters(new Set())}
          sortMode={sortMode}
          onSortChange={changeSort}
          typeFilter={contentTypes}
          platform={platformFilter}
          platformCounts={platformCounts}
          onPlatformChange={changePlatform}
          windowPref={windowPref}
          onWindowChange={changeWindow}
          onNotifications={() => onNavigate?.('Notifications')}
        />
      )}
      </div>
      {/* Right column (Phase 2): search + "Your sources" panel — replaces the
          old friends/chat SocialPanel. Search stays above the panel so add
          and browse live together. */}
      <div className="sources-column">
        <div className="sources-search">
          <SourceSearch
            categories={categories}
            activeCategory={categoryTab === 'home' ? undefined : categoryTab}
            onScope={(item, categoryId) => openFromMenu(categoryId, item.id)}
            onGlobalSearch={handleGlobalSearch}
            onGlobalAdd={handleGlobalAdd}
            onFeedgenSubscribed={refreshFeedgen}
            onDelete={handleDeleteItem}
            onLoadGenTopics={() => feedgenApi.topics().then((r) => r.topics)}
            onSubscribeGenTopic={async (t) => {
              const { at_uri } = await feedgenApi.publish(t.rkey);
              await subscriptionApi.create({
                source: 'bytescape_feed',
                source_id: at_uri,
                label: t.name,
                icon: '✨',
                category: 'feeds',
              });
            }}
          />
          {/* Owner QA 2026-11-17: the bell lives at the right end of the
              search row (top-right cluster over "Your sources"). */}
          <NotificationBell onOpenAll={() => onNavigate?.('Notifications')} />
        </div>
        <SourcesPanel
          categories={categories}
          onScope={(item, categoryId) => openFromMenu(categoryId, item.id)}
          onDelete={handleDeleteItem}
          onFeedgenSubscribed={refreshFeedgen}
          onInterests={onInterests}
        />
      </div>
    </div>
  );
}