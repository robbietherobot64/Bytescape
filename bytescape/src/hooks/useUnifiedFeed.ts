/**
 * useUnifiedFeed — Bounded-state feed data hook (v15 rebuild)
 *
 * Manages feed data retrieval, caching, pagination, and per-tab isolation
 * for the five feed tabs: All, Following, Subscribed, Groups, RSS.
 *
 * Design principles:
 * - Hard cap of 150 items per tab (prevents WebKitGTK RGBA bitmap accumulation)
 * - Cursor-based pagination (no client-side offset tracking)
 * - Stale-while-revalidate: cached data renders instantly, background refresh
 * - Per-tab isolation: inactive tabs retain state but don't fetch
 * - Source toggles on Following tab for multi-platform filtering
 *
 * Related files:
 * - src/types/index.ts — FeedPage, FeedTabState, FeedTab, LinkedFeedSource
 * - src/store/index.ts — FeedCacheStore (Zustand)
 * - src/utils/adapters.ts — adaptBlueskyPost, deduplicateFeedEntries
 * - src/screens/FeedScreen.tsx — primary consumer
 * - src/providers/DataProvider.ts — backend command bridge
 */

import { useState, useCallback, useEffect, useRef } from 'react';
import type {
  AuthStatus,
  FeedEntry,
  FeedPage,
  FeedTab,
  GeneratorView,
  LinkedFeedSource,
} from '../types';
import { LIVE_TABS } from '../types';
import { adaptBlueskyPost, deduplicateFeedEntries } from '../utils/adapters';
import { useFeedCacheStore } from '../store';
import { useDataProvider } from '../providers';

// ═══════════════════════════════════════════════════════════════════════════════
// Re-exports for backward compatibility (consumers import from this file)
// ═══════════════════════════════════════════════════════════════════════════════

export type { FeedTab } from '../types';
export { LIVE_TABS, FEED_TAB_LABELS } from '../types';

/** Special filter token meaning "do not narrow by source". */
export const FEED_FILTER_ALL = '__all__';

/**
 * Returns custom feed URIs, excluding Bluesky's built-in following feed URI.
 */
export function customFeedUris(feeds: GeneratorView[]): string[] {
  return feeds
    .map(f => f.uri)
    .filter(uri => !uri.includes('app.bsky.feed.generator/following'));
}

// ═══════════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════════

const MAX_ENTRIES = 150;        // Hard cap per tab — prevents WebKitGTK RGBA bitmap accumulation
const DEFAULT_PAGE_SIZE = 50;   // Configurable via settings — larger for richer initial load
const STALE_THRESHOLD_MS = 120_000; // 2 minutes — render cached, then background refresh
const AUTO_REFRESH_MS = 120_000;    // 2 minutes auto-refresh — slower to avoid disrupting video playback

// ═══════════════════════════════════════════════════════════════════════════════
// Utility: nest owner replies under parent posts
// ═══════════════════════════════════════════════════════════════════════════════

function nestOwnerReplies(entries: FeedEntry[]): FeedEntry[] {
  const uriToIndex = new Map<string, number>();
  for (let i = 0; i < entries.length; i++) {
    const e = entries[i];
    if (e.id) uriToIndex.set(e.id, i);
    if (e.original_url) uriToIndex.set(e.original_url, i);
    if (e.syndicated_uri) uriToIndex.set(e.syndicated_uri, i);
  }

  const replyIndices = new Set<number>();
  for (let i = 0; i < entries.length; i++) {
    const entry = entries[i];
    if (!entry.reply_parent_uri) continue;
    const parentIdx = uriToIndex.get(entry.reply_parent_uri);
    if (parentIdx === undefined) continue;
    const parent = entries[parentIdx];
    if (!parent.owner_replies) parent.owner_replies = [];
    parent.owner_replies.push(entry);
    replyIndices.add(i);
  }

  return entries.filter((_, i) => !replyIndices.has(i));
}

// ═══════════════════════════════════════════════════════════════════════════════
// Hook options & return types
// ═══════════════════════════════════════════════════════════════════════════════

interface UseUnifiedFeedOptions {
  auth: AuthStatus | null;
  activeTab: FeedTab;
  selectedFeedUris: string[];
  pageSize?: number;
  // Legacy compat — kept for FeedScreen but largely unused in new pipeline
  selectedHashtagsFilter?: string[];
  archiveSourceFilter?: string;
  autoRefreshFeeds?: boolean;
}

interface UseUnifiedFeedReturn {
  feedEntries: FeedEntry[];
  isLoading: boolean;
  feedError: string | null;
  hasMore: boolean;
  loadMore: () => Promise<void>;
  refresh: () => Promise<void>;
  isLoadingMore: boolean;
  // Source toggles (Following tab)
  linkedSources: LinkedFeedSource[];
  sourceToggles: Record<string, boolean>;
  setSourceToggle: (sourceId: string, enabled: boolean) => void;
  // Metadata
  savedFeeds: GeneratorView[];
  // Legacy compat — kept so FeedScreen destructuring doesn't break
  cursor: string | null;
  isFetchingMore: boolean;
  followingList: never[];
  followedHashtags: string[];
  setFollowedHashtags: (tags: string[]) => void;
  vaultSources: string[];
  archiveTotal: number;
  fetchFeed: () => Promise<void>;
  handleLoadMore: () => Promise<void>;
  setFeedEntries: React.Dispatch<React.SetStateAction<FeedEntry[]>>;
}

// ═══════════════════════════════════════════════════════════════════════════════
// Main hook
// ═══════════════════════════════════════════════════════════════════════════════

export function useUnifiedFeed({
  auth,
  activeTab,
  selectedFeedUris: _selectedFeedUris,
  pageSize = DEFAULT_PAGE_SIZE,
  archiveSourceFilter,
  autoRefreshFeeds = true,
}: UseUnifiedFeedOptions): UseUnifiedFeedReturn {
  const provider = useDataProvider();
  const feedCache = useFeedCacheStore();

  // ── Local state ──
  const [isLoading, setIsLoading] = useState(false);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [feedError, setFeedError] = useState<string | null>(null);
  const [savedFeeds, setSavedFeeds] = useState<GeneratorView[]>([]);
  const [linkedSources, setLinkedSources] = useState<LinkedFeedSource[]>([]);
  const [sourceToggles, setSourceTogglesState] = useState<Record<string, boolean>>({});
  const [followedHashtags, setFollowedHashtags] = useState<string[]>([]);

  // Ref to prevent concurrent fetches
  const fetchingRef = useRef(false);
  const autoRefreshTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => { mountedRef.current = false; };
  }, []);

  // ── Fetch a single page from the backend ──
  const fetchPage = useCallback(async (
    tab: FeedTab,
    cursor: string | null,
    limit: number,
  ): Promise<FeedPage> => {
    switch (tab) {
      case 'all': {
        // Backend returns FeedPage directly
        const sourceFilter = archiveSourceFilter && archiveSourceFilter !== FEED_FILTER_ALL
          ? archiveSourceFilter : undefined;
        return provider.getTimelineUnified({
          tags: [],
          feedUris: customFeedUris(savedFeeds),
          limit,
          cursor,
          vaultOffset: 0,
          sourceFilter,
        });
      }

      case 'following': {
        // Backend returns tuple [BlueskyPost[], cursor] — normalize to FeedPage
        const [posts, nextCursor] = await provider.getTimeline({
          limit,
          cursor: cursor ?? undefined,
        });
        const items = posts.map(adaptBlueskyPost);
        return { items, cursor: nextCursor, has_more: nextCursor != null };
      }

      case 'subscribed': {
        // Backend returns tuple [BlueskyPost[], cursor] — normalize to FeedPage
        const uris = customFeedUris(savedFeeds);
        const [posts, nextCursor] = await provider.getUnifiedFeed({
          tags: [],
          feedUris: uris,
          limit,
          cursor,
        });
        const items = posts.map(adaptBlueskyPost);
        return { items, cursor: nextCursor, has_more: nextCursor != null };
      }

      case 'groups': {
        // Backend returns FeedPage directly
        return provider.getGroupsFeed({ limit, cursor });
      }

      case 'rss': {
        // Placeholder — no RSS backend yet
        return { items: [], cursor: null, has_more: false };
      }

      default:
        return { items: [], cursor: null, has_more: false };
    }
  }, [provider, savedFeeds, archiveSourceFilter]);

  // ── Get cached state for active tab ──
  const getCachedState = useCallback(() => {
    return feedCache.getCache(activeTab, STALE_THRESHOLD_MS);
  }, [feedCache, activeTab]);

  // ── Core fetch: load first page or refresh ──
  // On refresh, MERGES new items with existing cache so auto-refresh
  // doesn't wipe the feed while the user is reading/watching video.
  const fetchFeed = useCallback(async () => {
    if (fetchingRef.current || !mountedRef.current) return;
    fetchingRef.current = true;
    setIsLoading(true);
    setFeedError(null);

    try {
      const page = await fetchPage(activeTab, null, pageSize);
      if (!mountedRef.current) return;

      // Merge with existing cache — prepend new items, deduplicate
      const existing = feedCache.getCache(activeTab, Infinity)?.entries ?? [];
      const merged = [...page.items, ...existing];
      const deduped = deduplicateFeedEntries(merged);
      const nested = nestOwnerReplies(deduped);
      const capped = nested.slice(0, MAX_ENTRIES);

      feedCache.setCache(activeTab, capped, page.cursor ?? feedCache.getCache(activeTab, Infinity)?.cursor ?? null);
    } catch (err) {
      if (mountedRef.current) {
        setFeedError(err instanceof Error ? err.message : String(err));
      }
    } finally {
      if (mountedRef.current) setIsLoading(false);
      fetchingRef.current = false;
    }
  }, [activeTab, fetchPage, pageSize, feedCache]);

  // ── Load more (pagination) ──
  const loadMore = useCallback(async () => {
    const cached = feedCache.getCache(activeTab, Infinity);
    if (!cached?.cursor || fetchingRef.current || !mountedRef.current) return;

    fetchingRef.current = true;
    setIsLoadingMore(true);

    try {
      const page = await fetchPage(activeTab, cached.cursor, pageSize);
      if (!mountedRef.current) return;

      const merged = [...cached.entries, ...page.items];
      const deduped = deduplicateFeedEntries(merged);
      const nested = nestOwnerReplies(deduped);

      // Enforce hard cap: if over MAX_ENTRIES, evict oldest (first pageSize entries)
      let capped = nested;
      if (capped.length > MAX_ENTRIES) {
        capped = capped.slice(capped.length - MAX_ENTRIES);
      }

      feedCache.setCache(activeTab, capped, page.cursor);
    } catch (err) {
      if (mountedRef.current) {
        setFeedError(err instanceof Error ? err.message : String(err));
      }
    } finally {
      if (mountedRef.current) setIsLoadingMore(false);
      fetchingRef.current = false;
    }
  }, [activeTab, fetchPage, pageSize, feedCache]);

  // ── Source toggle management (Following tab) ──
  const setSourceToggle = useCallback((sourceId: string, enabled: boolean) => {
    setSourceTogglesState(prev => ({ ...prev, [sourceId]: enabled }));
  }, []);

  // ── Load saved feeds + linked sources on mount ──
  useEffect(() => {
    if (!auth) return;

    // Load saved feeds
    provider.listSavedFeeds()
      .then(feeds => { if (mountedRef.current) setSavedFeeds(feeds); })
      .catch(() => {}); // non-critical

    // Load linked sources for source toggles
    import('@tauri-apps/api/core').then(({ invoke }) => {
      invoke<LinkedFeedSource[]>('get_linked_feed_sources')
        .then(sources => {
          if (!mountedRef.current) return;
          setLinkedSources(sources);
          // Initialize toggles: all connected sources enabled by default
          const toggles: Record<string, boolean> = {};
          for (const s of sources) {
            toggles[s.id] = s.connected;
          }
          setSourceTogglesState(toggles);
        })
        .catch(() => {}); // non-critical
    });

    // Load followed hashtags
    provider.listFollowedHashtags()
      .then(tags => { if (mountedRef.current) setFollowedHashtags(tags); })
      .catch(() => {});
  }, [auth, provider]);

  // ── Fetch on tab switch (stale-while-revalidate) ──
  useEffect(() => {
    if (!auth) return;

    const cached = getCachedState();
    if (!cached) {
      // No cache or stale — foreground fetch
      fetchFeed();
    } else {
      // Cache exists and is fresh — background refresh
      fetchFeed();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeTab, auth]);

  // ── Auto-refresh for live tabs ──
  useEffect(() => {
    if (autoRefreshTimerRef.current) {
      clearInterval(autoRefreshTimerRef.current);
      autoRefreshTimerRef.current = null;
    }

    if (!auth || !autoRefreshFeeds || !LIVE_TABS.includes(activeTab)) return;

    autoRefreshTimerRef.current = setInterval(() => {
      if (!fetchingRef.current && mountedRef.current) {
        fetchFeed();
      }
    }, AUTO_REFRESH_MS);

    return () => {
      if (autoRefreshTimerRef.current) {
        clearInterval(autoRefreshTimerRef.current);
        autoRefreshTimerRef.current = null;
      }
    };
  }, [auth, activeTab, autoRefreshFeeds, fetchFeed]);

  // ── Derive current state from cache ──
  const cached = feedCache.getCache(activeTab, Infinity);
  const feedEntries = cached?.entries ?? [];
  const currentCursor = cached?.cursor ?? null;
  const hasMore = currentCursor != null;

  // ── Legacy compat setFeedEntries — writes directly to cache ──
  const setFeedEntries: React.Dispatch<React.SetStateAction<FeedEntry[]>> = useCallback((action) => {
    const current = feedCache.getCache(activeTab, Infinity)?.entries ?? [];
    const newEntries = typeof action === 'function' ? action(current) : action;
    feedCache.setCache(activeTab, newEntries.slice(0, MAX_ENTRIES), currentCursor);
  }, [feedCache, activeTab, currentCursor]);

  return {
    feedEntries,
    isLoading,
    feedError,
    hasMore,
    loadMore,
    refresh: fetchFeed,
    isLoadingMore,
    linkedSources,
    sourceToggles,
    setSourceToggle,
    savedFeeds,
    // Legacy compat
    cursor: currentCursor,
    isFetchingMore: isLoadingMore,
    followingList: [] as never[],
    followedHashtags,
    setFollowedHashtags,
    vaultSources: [],
    archiveTotal: 0,
    fetchFeed,
    handleLoadMore: loadMore,
    setFeedEntries,
  };
}
