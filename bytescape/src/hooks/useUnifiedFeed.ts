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

import { useState, useCallback, useEffect, useRef, useMemo } from 'react';
import type {
  AuthStatus,
  FeedEntry,
  FeedPage,
  FeedTab,
  GeneratorView,
  LinkedFeedSource,
  ProfileView,
} from '../types';
import { LIVE_TABS } from '../types';
import { adaptBlueskyPost, deduplicateFeedEntries } from '../utils/adapters';
import { useFeedCacheStore } from '../store';
import { useDataProvider } from '../providers';
import { isAnyVideoPlaying } from '../components/VideoPlayer';

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
  /** When true, posts from followed users are excluded unless they match an active hashtag filter */
  followsDisabled?: boolean;
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
  followingList: ProfileView[];
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
  selectedFeedUris,
  pageSize = DEFAULT_PAGE_SIZE,
  selectedHashtagsFilter,
  archiveSourceFilter,
  autoRefreshFeeds = true,
  followsDisabled = false,
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
  const [vaultSources, setVaultSources] = useState<string[]>([]);
  const [followingList, setFollowingList] = useState<ProfileView[]>([]);
  const [archiveTotal, setArchiveTotal] = useState<number>(0);

  // Ref to prevent concurrent fetches
  const fetchingRef = useRef(false);
  const autoRefreshTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => { mountedRef.current = false; };
  }, []);

  // ── Resolve effective source filter ──
  const effectiveSourceFilter = archiveSourceFilter && archiveSourceFilter !== FEED_FILTER_ALL
    ? archiveSourceFilter : undefined;

  // ── Resolve effective tags from followed hashtags + active hashtag filters ──
  // When specific tag chips are selected, use ONLY those (narrow view).
  // When nothing is selected (empty array), use ALL followed hashtags (broad view).
  // When __disabled__, use none.
  const effectiveTags = useMemo(() => {
    if (selectedHashtagsFilter && selectedHashtagsFilter.length > 0) {
      if (selectedHashtagsFilter.length === 1 && selectedHashtagsFilter[0] === '__disabled__') {
        return []; // Tags disabled via master checkbox
      }
      return [...selectedHashtagsFilter]; // Specific chips selected — narrow to those
    }
    return [...followedHashtags]; // No selection — show all followed
  }, [followedHashtags, selectedHashtagsFilter]);

  // ── Resolve effective feed URIs (user-selected or all saved) ──
  const effectiveFeedUris = useMemo(() => {
    if (selectedFeedUris.length > 0) {
      if (selectedFeedUris.length === 1 && selectedFeedUris[0] === '__disabled__') {
        return []; // Feeds disabled via master checkbox
      }
      return selectedFeedUris.filter(u => u !== '__disabled__'); // Specific chips selected
    }
    return customFeedUris(savedFeeds); // No selection — use all saved feeds
  }, [selectedFeedUris, savedFeeds]);

  // ── Fetch a single page from the backend ──
  const fetchPage = useCallback(async (
    tab: FeedTab,
    cursor: string | null,
    limit: number,
  ): Promise<FeedPage> => {
    switch (tab) {
      case 'all': {
        // Backend returns FeedPage directly — pass tags + feed URIs
        // Source filter is applied client-side for instant reactivity
        return provider.getTimelineUnified({
          tags: effectiveTags,
          feedUris: effectiveFeedUris,
          limit,
          cursor,
          vaultOffset: 0,
        });
      }

      case 'following': {
        // Bluesky timeline + Mastodon home timeline, merged
        const results: FeedEntry[] = [];
        const enabledSources = Object.entries(sourceToggles)
          .filter(([, enabled]) => enabled)
          .map(([id]) => id);

        // If no toggles loaded yet, fetch all; otherwise respect toggles
        const fetchBluesky = enabledSources.length === 0 || enabledSources.includes('bluesky');
        const fetchMastodon = enabledSources.length === 0 || enabledSources.includes('mastodon');

        const promises: Promise<void>[] = [];

        if (fetchBluesky) {
          promises.push(
            provider.getTimeline({ limit, cursor: cursor ?? undefined })
              .then(([posts, _nextCursor]) => {
                results.push(...posts.map(adaptBlueskyPost));
              })
              .catch(() => {}) // non-critical — Bluesky may not be connected
          );
        }

        if (fetchMastodon) {
          promises.push(
            provider.mastodonGetTimeline({ limit })
              .then(entries => {
                results.push(...entries);
              })
              .catch(() => {}) // non-critical — Mastodon may not be connected
          );
        }

        await Promise.all(promises);

        // Sort merged results by timestamp descending
        results.sort((a, b) => (b.timestamp || 0) - (a.timestamp || 0));

        return { items: results.slice(0, limit), cursor: null, has_more: false };
      }

      case 'subscribed': {
        // Bluesky custom feeds + Mastodon hashtag timelines, merged
        const results: FeedEntry[] = [];
        const promises: Promise<void>[] = [];

        // Bluesky custom feeds (if any feed URIs exist)
        if (effectiveFeedUris.length > 0) {
          promises.push(
            provider.getUnifiedFeed({
              tags: effectiveTags,
              feedUris: effectiveFeedUris,
              limit,
              cursor,
            }).then(([posts]) => {
              results.push(...posts.map(adaptBlueskyPost));
            }).catch(() => {})
          );
        }

        // Mastodon hashtag timelines — fetch for each followed hashtag
        for (const tag of effectiveTags) {
          promises.push(
            provider.mastodonGetHashtagTimeline({ hashtag: tag, limit: Math.min(limit, 20) })
              .then(posts => {
                results.push(...posts.map(adaptBlueskyPost));
              })
              .catch(() => {}) // Mastodon may not be connected
          );
        }

        await Promise.all(promises);

        // Sort merged results by timestamp descending
        results.sort((a, b) => (b.timestamp || 0) - (a.timestamp || 0));
        const deduped = deduplicateFeedEntries(results);

        return { items: deduped.slice(0, limit), cursor: null, has_more: false };
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
  }, [provider, effectiveFeedUris, effectiveTags, sourceToggles]);

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

    // Load followed hashtags from local DB, then sync from Mastodon
    provider.listFollowedHashtags()
      .then(localTags => {
        if (mountedRef.current) setFollowedHashtags(localTags);
        // Sync followed tags from Mastodon instance → merge into local DB
        return provider.mastodonGetFollowedTags()
          .then(async remoteTags => {
            if (!mountedRef.current || remoteTags.length === 0) return;
            // Add any remote tags not already in local DB
            const localSet = new Set(localTags.map(t => t.toLowerCase()));
            const newTags = remoteTags.filter(t => !localSet.has(t.toLowerCase()));
            for (const tag of newTags) {
              await provider.addFollowedHashtag({ hashtag: tag }).catch(() => {});
            }
            // Re-read the full list after sync
            if (mountedRef.current && newTags.length > 0) {
              const refreshed = await provider.listFollowedHashtags().catch(() => localTags);
              if (mountedRef.current) setFollowedHashtags(refreshed);
            }
          })
          .catch(() => {}); // Mastodon may not be connected
      })
      .catch(() => {});

    // Load vault sources for platform filter dropdown
    provider.listVaultSources()
      .then(sources => { if (mountedRef.current) setVaultSources(sources); })
      .catch(() => {});

    // Load following list (contacts) for the side panel
    provider.listContacts()
      .then(contacts => { if (mountedRef.current) setFollowingList(contacts); })
      .catch(() => {});

    // Load archive total
    provider.countArchivePosts()
      .then(count => { if (mountedRef.current) setArchiveTotal(count); })
      .catch(() => {});
  }, [auth, provider]);

  // ── Re-fetch when savedFeeds resolves (fixes race where fetchFeed fires before feeds load) ──
  const savedFeedsLoadedRef = useRef(false);
  useEffect(() => {
    if (savedFeeds.length > 0 && !savedFeedsLoadedRef.current) {
      savedFeedsLoadedRef.current = true;
      // Saved feeds just loaded — re-fetch subscribed/all tabs if active
      if (activeTab === 'subscribed' || activeTab === 'all') {
        fetchFeed();
      }
    }
  }, [savedFeeds, activeTab, fetchFeed]);

  // ── Re-fetch when chip selection changes (feeds or hashtags) ──
  const filterKeyRef = useRef<string | null>(null);
  useEffect(() => {
    if (!auth) return;
    const key = JSON.stringify({ f: effectiveFeedUris, t: effectiveTags });
    if (filterKeyRef.current === null) {
      // Initial mount — just record the key, don't double-fetch
      filterKeyRef.current = key;
      return;
    }
    if (filterKeyRef.current !== key) {
      filterKeyRef.current = key;
      // Clear stale cache for this tab and re-fetch with new filters
      feedCache.setCache(activeTab, [], null);
      fetchFeed();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [auth, activeTab, effectiveFeedUris, effectiveTags]);

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
      // Skip auto-refresh if a video is actively playing to prevent
      // re-renders that cause choppy playback or pipeline teardown
      if (!fetchingRef.current && mountedRef.current && !isAnyVideoPlaying()) {
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
  const rawEntries = cached?.entries ?? [];
  const currentCursor = cached?.cursor ?? null;
  const hasMore = currentCursor != null;

  // ── Apply client-side filters for instant reactivity ──
  // The cache stores ALL fetched entries. Filtering happens here so
  // changing chips/dropdowns instantly narrows the visible feed without re-fetching.

  /** Extract hashtags from post text (matches FeedEntryCard's extractHashtags) */
  const extractPostTags = useCallback((text: string | undefined): Set<string> => {
    if (!text) return new Set();
    const matches = text.match(/#[\w\u00C0-\u024F\u1E00-\u1EFF]+/g);
    if (!matches) return new Set();
    return new Set(matches.map(t => t.replace(/^#/, '').toLowerCase()));
  }, []);

  // Determine filter intent from the chip state arrays
  const feedsDisabled = selectedFeedUris.length === 1 && selectedFeedUris[0] === '__disabled__';
  const tagsDisabled = selectedHashtagsFilter !== undefined
    && selectedHashtagsFilter.length === 1 && selectedHashtagsFilter[0] === '__disabled__';
  // Specific chips actively selected? (not "show all", not disabled)
  const hasSpecificTagChips = selectedHashtagsFilter !== undefined
    && selectedHashtagsFilter.length > 0
    && !tagsDisabled;

  const feedEntries = useMemo(() => {
    let entries = rawEntries;

    // Source filter (platform dropdown)
    if (effectiveSourceFilter) {
      entries = entries.filter(e =>
        e.source?.toLowerCase() === effectiveSourceFilter.toLowerCase()
      );
    }

    // Hashtag content filter — ONLY applies when the user has narrowed to hashtags only:
    // 1. Specific hashtag chips selected → filter to only posts with those exact #hashtags
    // 2. Both feeds AND follows disabled + tags enabled → filter by ALL followed hashtags
    // NEVER applies on the 'following' tab (that tab is purely about followed accounts).
    if (activeTab !== 'following') {
      if (hasSpecificTagChips) {
        // Scenario 1: narrow to specific selected tags
        const selectedLower = new Set(selectedHashtagsFilter!.map(t => t.toLowerCase()));
        entries = entries.filter(e => {
          const postTags = extractPostTags(e.content);
          for (const st of selectedLower) {
            if (postTags.has(st)) return true;
          }
          return false;
        });
      } else if (feedsDisabled && followsDisabled && !tagsDisabled && followedHashtags.length > 0) {
        // Scenario 2: BOTH feeds and follows off, tags on → show only hashtag-matching posts
        const allTagsLower = new Set(followedHashtags.map(t => t.toLowerCase()));
        entries = entries.filter(e => {
          const postTags = extractPostTags(e.content);
          for (const ft of allTagsLower) {
            if (postTags.has(ft)) return true;
          }
          return false;
        });
      }
    }

    return entries;
  }, [rawEntries, effectiveSourceFilter, hasSpecificTagChips, selectedHashtagsFilter, extractPostTags, feedsDisabled, tagsDisabled, followedHashtags, followsDisabled, activeTab]);

  // ── Merge vault sources with linked platform sources for filter dropdown ──
  const mergedSources = useMemo(() => {
    const sources = new Set(vaultSources);
    // Add connected platform IDs (lowercase) so filter values match FeedEntry.source
    for (const ls of linkedSources) {
      if (ls.connected) sources.add(ls.id);
    }
    return Array.from(sources).sort();
  }, [vaultSources, linkedSources]);

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
    followingList,
    followedHashtags,
    setFollowedHashtags,
    vaultSources: mergedSources,
    archiveTotal,
    fetchFeed,
    handleLoadMore: loadMore,
    setFeedEntries,
  };
}
