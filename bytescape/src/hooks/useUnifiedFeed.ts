import { useState, useCallback, useEffect, useRef } from 'react';
import { AuthStatus, BlueskyPost, FeedEntry, GeneratorView, LinkedAccount, ProfileView } from '../types';
import { adaptBlueskyPost, adaptBytescapeItem, deduplicateFeedEntries } from '../utils/adapters';
import { useFeedCacheStore } from '../store';
import { useDataProvider } from '../providers';

/**
 * Maximum feed entries held in React state. Prevents unbounded growth
 * during repeated "Load More" operations. Aligned with the Zustand
 * cache cap (500) but more conservative since React state drives renders.
 * 300 entries × ~5 KB avg ≈ 1.5 MB max in component state.
 */
const MAX_FEED_ENTRIES = 300;

/** Cap an entry array to MAX_FEED_ENTRIES, keeping newest (end of array). */
function capEntries(entries: FeedEntry[]): FeedEntry[] {
    return entries.length > MAX_FEED_ENTRIES
        ? entries.slice(entries.length - MAX_FEED_ENTRIES)
        : entries;
}

/**
 * Nest owner replies under their parent posts.
 * If a FeedEntry has a reply_parent_uri that matches another entry's id or original_url,
 * move it into the parent's owner_replies array instead of showing it standalone.
 */
function nestOwnerReplies(entries: FeedEntry[]): FeedEntry[] {
    // Build a lookup: URI → parent entry index
    const uriToIndex = new Map<string, number>();
    for (let i = 0; i < entries.length; i++) {
        const e = entries[i];
        // Map by id (CID), original_url (AT-URI), and syndicated_uri
        if (e.id) uriToIndex.set(e.id, i);
        if (e.original_url) uriToIndex.set(e.original_url, i);
        if (e.syndicated_uri) uriToIndex.set(e.syndicated_uri, i);
    }

    const parentIndices = new Set<number>(); // entries that receive replies
    const replyIndices = new Set<number>();   // entries that ARE replies (to be removed from top-level)

    for (let i = 0; i < entries.length; i++) {
        const entry = entries[i];
        if (!entry.reply_parent_uri) continue;
        const parentIdx = uriToIndex.get(entry.reply_parent_uri);
        if (parentIdx === undefined) continue;

        // Found a parent in the same page — nest this reply under it
        const parent = entries[parentIdx];
        if (!parent.owner_replies) parent.owner_replies = [];
        parent.owner_replies.push(entry);
        parentIndices.add(parentIdx);
        replyIndices.add(i);
    }

    // Return entries without the nested replies (they're now inside their parent)
    return entries.filter((_, i) => !replyIndices.has(i));
}

export type FeedTab = 'all' | 'following' | 'subscribed' | 'you' | 'groups';

export const LIVE_TABS: FeedTab[] = ['all', 'following', 'subscribed', 'groups'];

export const FEED_TAB_LABELS: Record<FeedTab, string> = {
    all: 'All',
    following: 'Following',
    subscribed: 'Subscribed',
    you: 'You',
    groups: 'Groups',
};
export const FEED_FILTER_ALL = '__all__';
const ARCHIVE_PAGE_SIZE = 200;

export function customFeedUris(feeds: GeneratorView[]): string[] {
    return feeds
        .map(f => f.uri)
        .filter(uri => !uri.includes('app.bsky.feed.generator/following'));
}

interface UseUnifiedFeedOptions {
    auth: AuthStatus | null;
    activeTab: FeedTab;
    selectedFeedUris: string[];
    selectedHashtagsFilter: string[];
    archiveSourceFilter: string;
    autoRefreshFeeds: boolean;
}

export function useUnifiedFeed({
    auth,
    activeTab,
    selectedFeedUris,
    selectedHashtagsFilter,
    archiveSourceFilter,
    autoRefreshFeeds
}: UseUnifiedFeedOptions) {
    // === DataProvider ===
    const provider = useDataProvider();

    // === Data State ===
    const [feedEntries, setFeedEntries] = useState<FeedEntry[]>([]);
    const [isLoading, setIsLoading] = useState(false);
    const [feedError, setFeedError] = useState<string | null>(null);
    const [cursor, setCursor] = useState<string | null>(null);
    const [isFetchingMore, setIsFetchingMore] = useState(false);

    // === Context Data ===
    const [followingList, setFollowingList] = useState<ProfileView[]>([]);
    const [savedFeeds, setSavedFeeds] = useState<GeneratorView[]>([]);
    const [followedHashtags, setFollowedHashtags] = useState<string[]>([]);
    const [archiveOffset, setArchiveOffset] = useState(0);
    const [archiveTotal, setArchiveTotal] = useState(0);
    const [vaultSources, setVaultSources] = useState<string[]>([]);

    const fetchTokenRef = useRef(0);
    const verifiedIdsRef = useRef<Set<string>>(new Set());
    const prevAuthDidRef = useRef<string | null>(null);

    const { setCache, getCache, clearCache, clearCachePrefix, setCachedSavedFeeds, setCachedFollowingList } = useFeedCacheStore();

    // When the auth identity changes (e.g. account disconnected), nuke all
    // local state so stale data from the old account doesn't flash.
    useEffect(() => {
        const currentDid = auth?.did ?? null;
        if (prevAuthDidRef.current !== null && prevAuthDidRef.current !== currentDid) {
            setFeedEntries([]);
            setFollowingList([]);
            setSavedFeeds([]);
            setCursor(null);
            setArchiveOffset(0);
            clearCache('all');
            clearCache('following');
            clearCache('subscribed');
            clearCache('you');
        }
        prevAuthDidRef.current = currentDid;
    }, [auth?.did, clearCache]);

    // Load available platform sources for the filter dropdown (independent of active tab)
    useEffect(() => {
        if (!auth) return;
        Promise.all([
            provider.listVaultSources().catch((): string[] => []),
            provider.listLinkedAccounts().catch((): LinkedAccount[] => []),
        ]).then(([vaultSrcs, accounts]) => {
            const platformSrcs = accounts
                .filter((a: LinkedAccount) => a.active)
                .map((a: LinkedAccount) => a.protocol);
            const merged = [...new Set([...vaultSrcs, ...platformSrcs])].sort();
            setVaultSources(merged);
        });
    }, [auth?.did]);

    const fetchFollowingList = async (actorDid: string) => {
        try {
            const follows = await provider.listFollows({ actor: actorDid });
            setFollowingList(follows);
            setCachedFollowingList(follows);
        } catch (e) {
            console.error('Failed to fetch follows:', e);
            try {
                const contacts = await provider.listContacts();
                setFollowingList(contacts.map((c: ProfileView) => ({ did: c.did, handle: c.handle })));
            } catch {
                setFollowingList([]);
            }
        }
    };

    // === Fetch Feed ===
    //
    // Each tab has a distinct data source:
    //   'following'  → get_timeline (Bluesky getTimeline API)\n    //   'you'        → list_vault_posts_paged (local vault items)\n    //   'all'        → get_timeline_unified (Rust merges timeline + feeds + vault)
    //
    // All paths end with deduplicateFeedEntries to collapse duplicates
    // while preserving reposts as distinct events.

    const fetchFeed = useCallback(async (tab: FeedTab, background = false) => {
        if (!auth) return;

        const requestId = ++fetchTokenRef.current;
        const isStale = () => fetchTokenRef.current !== requestId;

        // Cache key includes source filter to prevent cross-filter cache corruption
        const cacheKey = `${tab}:${archiveSourceFilter}:${selectedFeedUris.join(',')}:${selectedHashtagsFilter.join(',')}`;

        // Cache-first: if fresh cache exists, use it and skip the network fetch.
        // The 45-second auto-refresh handles eventual freshness — no need for
        // stale-while-revalidate which doubled every pipeline run (2× backend,
        // 2× IPC, 2× state updates, 2× blob resolution).
        if (!background) {
            const cached = getCache(cacheKey);
            if (cached) {
                // Instant render from cache — no loading spinner, no background fetch
                setFeedEntries(cached.entries);
                setCursor(cached.cursor);
                setFeedError(null);
                return;
            }
            setIsLoading(true);
            setFeedError(null);
            verifiedIdsRef.current.clear();
        }

        try {
            if (tab === 'following') {
                // ── Following tab: Bluesky home timeline ──
                const [posts, nextCursor] = await provider.getTimeline({ limit: 50 });
                if (isStale()) return;

                const adapted = posts.map(adaptBlueskyPost);
                // Apply client-side source filter
                const filtered = archiveSourceFilter
                    ? adapted.filter(item => item.source.toLowerCase() === archiveSourceFilter.toLowerCase())
                    : adapted;
                const deduped = deduplicateFeedEntries(filtered);

                setFeedEntries(deduped);
                setCursor(nextCursor);
                setCache(cacheKey, deduped, nextCursor);
                await fetchFollowingList(auth.did);

            } else if (tab === 'you') {
                // ── You tab: user's own posts (Bluesky author feed + local vault) ──
                const [bskyResult, vaultResult, sources] = await Promise.all([
                    provider.getAuthorFeed({ limit: 50 })
                        .catch((): [BlueskyPost[], string | null] => [[], null]),
                    provider.listVaultPostsPaged({
                        limit: ARCHIVE_PAGE_SIZE,
                        offset: 0,
                        source: archiveSourceFilter || null,
                    }),
                    provider.listVaultSources().catch((): string[] => []),
                ]);
                if (isStale()) return;
                setArchiveOffset(ARCHIVE_PAGE_SIZE);
                setArchiveTotal(vaultResult.total);
                setVaultSources(sources);

                const [bskyPosts, bskyCursor] = bskyResult;
                const bskyEntries = bskyPosts.map(adaptBlueskyPost);
                const vaultEntries = nestOwnerReplies(vaultResult.items.map(adaptBytescapeItem));

                // Merge both sources, newest first, then deduplicate.
                // Vault entries go first so that cross-platform posts keep the
                // richer vault version (with share_history) during dedup.
                const merged = [...vaultEntries, ...bskyEntries].sort(
                    (a, b) => (b.timestamp || 0) - (a.timestamp || 0)
                );
                // Apply client-side source filter to merged results
                const filtered = archiveSourceFilter
                    ? merged.filter(item => item.source.toLowerCase() === archiveSourceFilter.toLowerCase())
                    : merged;
                const dedupedYou = deduplicateFeedEntries(filtered);
                setFeedEntries(dedupedYou);
                setCursor(bskyCursor);
                setCache(cacheKey, dedupedYou, bskyCursor);

            } else if (tab === 'subscribed') {
                // ── Subscribed tab: show content from user's saved/subscribed feeds ──
                // Parallelize independent IPC calls (P7 — saves 200-500ms)
                const [feedsData, tags] = await Promise.all([
                    provider.listSavedFeeds().catch((e: unknown): GeneratorView[] => {
                        console.error('list_saved_feeds failed:', e);
                        return [];
                    }),
                    provider.listFollowedHashtags().catch((): string[] => []),
                ]);
                // Use ALL saved feed URIs (including following) for subscribed tab
                const allFeedUris = feedsData.map(f => f.uri);
                setFollowedHashtags(tags);

                // Solo logic: when a specific tag is soloed, don't include all feeds
                // (and vice-versa). This prevents e.g. #cats returning political feed posts.
                const feedsDisabled = selectedFeedUris.includes('__disabled__');
                const tagsDisabled = selectedHashtagsFilter.includes('__disabled__');
                const specificFeedSoloed = selectedFeedUris.length > 0 && !feedsDisabled;
                const specificTagSoloed = selectedHashtagsFilter.length > 0 && !tagsDisabled;

                let activeFeedUris: string[];
                if (feedsDisabled) {
                    activeFeedUris = [];                    // feeds checkbox off
                } else if (specificFeedSoloed) {
                    activeFeedUris = selectedFeedUris.filter(uri => allFeedUris.includes(uri));
                } else if (specificTagSoloed) {
                    activeFeedUris = [];                    // tag soloed → skip feeds
                } else {
                    activeFeedUris = allFeedUris;           // nothing soloed → all
                }

                let activeTags: string[];
                if (tagsDisabled) {
                    activeTags = [];                        // tags checkbox off
                } else if (specificTagSoloed) {
                    // Pass selected tags directly — don't filter against followedHashtags.
                    // When a user clicks a hashtag chip, that tag might not be in their
                    // followed list, but we still want to search for it.
                    activeTags = selectedHashtagsFilter;
                } else if (specificFeedSoloed) {
                    activeTags = [];                        // feed soloed → skip tags
                } else {
                    activeTags = tags;                      // nothing soloed → all
                }

                if (activeFeedUris.length > 0 || activeTags.length > 0) {
                    try {
                        const [posts, nextCur] = await provider.getUnifiedFeed({
                            tags: activeTags,
                            feedUris: activeFeedUris,
                            limit: 200,
                            cursor: null,
                        });

                        if (isStale()) return;
                        setSavedFeeds(feedsData);
                        setCachedSavedFeeds(feedsData);

                        const adapted = posts.map(adaptBlueskyPost);
                        // Apply client-side source filter
                        const filtered = archiveSourceFilter
                            ? adapted.filter(item => item.source.toLowerCase() === archiveSourceFilter.toLowerCase())
                            : adapted;
                        const deduped = deduplicateFeedEntries(filtered);
                        setFeedEntries(deduped);
                        setCursor(nextCur);
                        setCache(cacheKey, deduped, nextCur);
                    } catch (e: any) {
                        console.error('get_unified_feed failed for subscribed tab:', e);
                        if (!isStale()) {
                            let errorMsg = typeof e === 'string' ? e : 'Failed to load subscribed feeds';
                            if (errorMsg.includes('no access token available') || errorMsg.includes('not logged in')) {
                                errorMsg = 'Connect Bluesky to use subscribed feeds';
                            }
                            setFeedError(errorMsg);
                        }
                    }
                } else {
                    if (isStale()) return;
                    setSavedFeeds(feedsData);
                    setFeedEntries([]);
                    setCursor(null);
                }

            } else if (tab === 'all') {
                // ── All tab: unified feed (Rust merges timeline + feeds + vault) ──
                // Parallelize list_saved_feeds with get_timeline_unified (P7)
                // get_timeline_unified internally calls list_saved_feeds for its own logic,
                // but we still need the GeneratorView[] data for the UI feed selector.
                const [feedsData, tags, response] = await Promise.all([
                    provider.listSavedFeeds().catch((): GeneratorView[] => []),
                    provider.listFollowedHashtags().catch((): string[] => []),
                    provider.getTimelineUnified({
                        tags: [],
                        feedUris: [],
                        limit: 50,
                        cursor: null,
                        vaultOffset: 0,
                        sourceFilter: archiveSourceFilter || null,
                    }),
                ]);

                try {

                    if (isStale()) return;
                    setSavedFeeds(feedsData);
                    setCachedSavedFeeds(feedsData);
                    setFollowedHashtags(tags);

                    let items = response.items;
                    // Safety: client-side filter in case backend missed anything
                    if (archiveSourceFilter) {
                        items = items.filter(item =>
                            item.source.toLowerCase() === archiveSourceFilter.toLowerCase()
                        );
                    }
                    const deduped = deduplicateFeedEntries(items);
                    setFeedEntries(deduped);
                    setCursor(response.next_cursor);
                    setCache(cacheKey, deduped, response.next_cursor);
                    setArchiveOffset(200);
                } catch (e) {
                    console.error('get_timeline_unified/list_saved_feeds failed:', e);
                    if (isStale()) return;
                    setSavedFeeds([]);
                    setCachedSavedFeeds([]);
                    setFeedError(typeof e === 'string' ? e : 'Failed to load unified feed');
                    setFeedEntries([]);
                    setCursor(null);
                }

                await fetchFollowingList(auth.did);
            } else if (tab === 'groups') {
                // ── Groups tab: posts from subscribed groups & topics ──
                try {
                    const items = await provider.getGroupsFeed({ limit: 100 });
                    if (isStale()) return;

                    const adapted = items.map(adaptBytescapeItem);
                    // Apply client-side source filter
                    const filtered = archiveSourceFilter
                        ? adapted.filter(item => item.source.toLowerCase() === archiveSourceFilter.toLowerCase())
                        : adapted;
                    const deduped = deduplicateFeedEntries(filtered);
                    setFeedEntries(deduped);
                    setCursor(null);
                    setCache(cacheKey, deduped, null);
                } catch (e) {
                    console.error('get_groups_feed failed:', e);
                    if (!isStale()) {
                        setFeedError(typeof e === 'string' ? e : 'Failed to load groups feed');
                        setFeedEntries([]);
                    }
                }
            }
        } catch (e) {
            console.error('Failed to fetch feed:', e);
            if (!background && !isStale()) {
                setFeedError(typeof e === 'string' ? e : 'Failed to fetch feed data');
            }
        } finally {
            if (!background && !isStale()) {
                setIsLoading(false);
            }
        }
    }, [auth, selectedFeedUris, selectedHashtagsFilter, archiveSourceFilter]);

    // Unified effect: clear stale cache AND trigger reload on any filter/tab change.
    // Previously these were separate effects with the same deps, which could race
    // (fetch might hit stale cache before the clear effect ran).
    const prevFiltersRef = useRef({ selectedFeedUris, selectedHashtagsFilter, archiveSourceFilter });
    useEffect(() => {
        if (!auth) return;

        const prev = prevFiltersRef.current;
        const feedUrisChanged = prev.selectedFeedUris !== selectedFeedUris;
        const hashtagsChanged = prev.selectedHashtagsFilter !== selectedHashtagsFilter;
        const sourceChanged = prev.archiveSourceFilter !== archiveSourceFilter;
        prevFiltersRef.current = { selectedFeedUris, selectedHashtagsFilter, archiveSourceFilter };

        // Clear relevant caches BEFORE fetching
        if (feedUrisChanged || hashtagsChanged) {
            clearCachePrefix('subscribed');
        }
        if (sourceChanged) {
            clearCachePrefix('all');
            clearCachePrefix('you');
            clearCachePrefix('following');
            clearCachePrefix('subscribed');
        }

        setCursor(null);
        setArchiveOffset(0);
        fetchFeed(activeTab, false);
    }, [auth, activeTab, selectedFeedUris, selectedHashtagsFilter, archiveSourceFilter, fetchFeed, clearCachePrefix]);

    // Auto-refresh for live tabs
    useEffect(() => {
        if (!auth || !autoRefreshFeeds) return;
        if (!LIVE_TABS.includes(activeTab)) return;

        const interval = setInterval(() => {
            if (typeof document !== 'undefined' && document.hidden) return;
            fetchFeed(activeTab, true);
        }, 45000);

        return () => clearInterval(interval);
    }, [auth, activeTab, autoRefreshFeeds, fetchFeed]);

    // Background verification of active syndications.
    // Batched: mark all IDs as verified upfront, run checks, then do at most
    // ONE background refresh if any syndication state changed.
    // Previous pattern fired N individual IPC calls and each "changed" response
    // triggered a full fetchFeed — cascading into N full pipeline replays.
    useEffect(() => {
        if (!auth || feedEntries.length === 0) return;

        const isVaultId = (id: string) =>
            !id.startsWith('at://') &&
            !id.startsWith('http://') &&
            !id.startsWith('https://');
        const unverified = feedEntries.filter(entry =>
            isVaultId(entry.id) &&
            entry.share_history?.some(e => e.active) &&
            !verifiedIdsRef.current.has(entry.id)
        );

        if (unverified.length === 0) return;

        // Mark all as verified upfront to prevent re-triggering on re-render
        for (const entry of unverified) {
            verifiedIdsRef.current.add(entry.id);
        }

        // Run all verifications, then do at most one refresh
        let cancelled = false;
        (async () => {
            let anyChanged = false;
            for (const entry of unverified) {
                if (cancelled) return;
                try {
                    const changed = await provider.verifySyndications({ itemId: entry.id });
                    if (changed) anyChanged = true;
                } catch (e) {
                    console.error(`sync: verify failed for ${entry.id}`, e);
                }
            }
            // Single background refresh if anything changed (not N refreshes)
            if (anyChanged && !cancelled) {
                fetchFeed(activeTab, true);
            }
        })();

        return () => { cancelled = true; };
    }, [feedEntries, auth, activeTab, fetchFeed]);

    // === Load More ===

    const handleLoadMore = async () => {
        if (isFetchingMore || !auth) return;
        setIsFetchingMore(true);

        try {
            if (activeTab === 'following' && cursor) {
                const [posts, nextCursor] = await provider.getTimeline({ limit: 30, cursor });
                let newEntries = posts.map(adaptBlueskyPost);
                if (archiveSourceFilter) {
                    newEntries = newEntries.filter(item => item.source.toLowerCase() === archiveSourceFilter.toLowerCase());
                }
                setFeedEntries(prev => capEntries(deduplicateFeedEntries([...prev, ...newEntries])));
                setCursor(nextCursor);

            } else if (activeTab === 'you') {
                // Load more from both Bluesky author feed and vault
                const [bskyResult, vaultResult] = await Promise.all([
                    cursor
                        ? provider.getAuthorFeed({ limit: 30, cursor })
                            .catch((): [BlueskyPost[], string | null] => [[], null])
                        : Promise.resolve([[], null] as [BlueskyPost[], string | null]),
                    provider.listVaultPostsPaged({
                        limit: ARCHIVE_PAGE_SIZE,
                        offset: archiveOffset,
                        source: archiveSourceFilter || null,
                    }),
                ]);

                const [bskyPosts, bskyCursor] = bskyResult;
                const bskyEntries = bskyPosts.map(adaptBlueskyPost);
                const vaultEntries = vaultResult.items.map(adaptBytescapeItem);
                let newEntries: FeedEntry[] = [...bskyEntries, ...vaultEntries].sort(
                    (a, b) => (b.timestamp || 0) - (a.timestamp || 0)
                );
                if (archiveSourceFilter) {
                    newEntries = newEntries.filter(item => item.source.toLowerCase() === archiveSourceFilter.toLowerCase());
                }
                setFeedEntries(prev => capEntries(nestOwnerReplies(deduplicateFeedEntries([...prev, ...newEntries]))));
                setArchiveOffset(prev => prev + ARCHIVE_PAGE_SIZE);
                setCursor(bskyCursor);

            } else if (activeTab === 'subscribed' && cursor) {
                // Respect the same solo filter logic as the initial fetch
                const allFeedUris = savedFeeds.map(f => f.uri);
                const feedsOff = selectedFeedUris.includes('__disabled__');
                const tagsOff = selectedHashtagsFilter.includes('__disabled__');
                const feedSoloed = selectedFeedUris.length > 0 && !feedsOff;
                const tagSoloed = selectedHashtagsFilter.length > 0 && !tagsOff;

                let moreFeedUris: string[];
                if (feedsOff) moreFeedUris = [];
                else if (feedSoloed) moreFeedUris = selectedFeedUris.filter(u => allFeedUris.includes(u));
                else if (tagSoloed) moreFeedUris = [];
                else moreFeedUris = allFeedUris;

                let moreTags: string[];
                if (tagsOff) moreTags = [];
                else if (tagSoloed) moreTags = selectedHashtagsFilter;
                else if (feedSoloed) moreTags = [];
                else moreTags = followedHashtags || [];

                const [posts, nextCur] = await provider.getUnifiedFeed({
                    tags: moreTags,
                    feedUris: moreFeedUris,
                    limit: 200,
                    cursor,
                });

                if (posts.length > 0) {
                    let newEntries = posts.map(adaptBlueskyPost);
                    if (archiveSourceFilter) {
                        newEntries = newEntries.filter(item => item.source.toLowerCase() === archiveSourceFilter.toLowerCase());
                    }
                    setFeedEntries(prev => capEntries(deduplicateFeedEntries([...prev, ...newEntries])));
                    setCursor(nextCur);
                }

            } else if (activeTab === 'all') {
                const response = await provider.getTimelineUnified({
                    tags: [],
                    feedUris: customFeedUris(savedFeeds),
                    limit: 50,
                    cursor,
                    vaultOffset: archiveOffset,
                });

                if (response) {
                    let newItems = response.items;
                    if (archiveSourceFilter) {
                        newItems = newItems.filter(item => item.source.toLowerCase() === archiveSourceFilter.toLowerCase());
                    }
                    setFeedEntries(prev => capEntries(deduplicateFeedEntries([...prev, ...newItems])));
                    setCursor(response.next_cursor);
                    setArchiveOffset(prev => prev + 200);
                }
            }
        } catch (err) {
            console.error('Load more failed:', err);
        } finally {
            setIsFetchingMore(false);
        }
    };

    return {
        feedEntries,
        setFeedEntries,
        isLoading,
        feedError,
        cursor,
        isFetchingMore,
        followingList,
        savedFeeds,
        followedHashtags,
        setFollowedHashtags,
        vaultSources,
        archiveTotal,
        fetchFeed,
        handleLoadMore,
    };
}
