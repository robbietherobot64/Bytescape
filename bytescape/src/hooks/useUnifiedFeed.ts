import { useState, useCallback, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { AuthStatus, BlueskyPost, BytescapeItem, FeedEntry, GeneratorView, ProfileView } from '../types';
import { adaptBlueskyPost, adaptBytescapeItem, deduplicateFeedEntries } from '../utils/adapters';
import { useFeedCacheStore } from '../store';
import { prefetchBlobUrls } from '../utils/prefetchBlobUrls';

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

export type FeedTab = 'all' | 'following' | 'subscribed' | 'you';

export const LIVE_TABS: FeedTab[] = ['all', 'following', 'subscribed'];

export const FEED_TAB_LABELS: Record<FeedTab, string> = {
    all: 'All',
    following: 'Following',
    subscribed: 'Subscribed',
    you: 'You',
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

    const { setCache, getCache, clearCache, setCachedSavedFeeds, setCachedFollowingList } = useFeedCacheStore();

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

    const fetchFollowingList = async (actorDid: string) => {
        try {
            const follows = await invoke<ProfileView[]>('list_follows', { actor: actorDid });
            setFollowingList(follows);
            setCachedFollowingList(follows);
        } catch (e) {
            console.error('Failed to fetch follows:', e);
            try {
                const contacts = await invoke<any[]>('list_contacts');
                setFollowingList(contacts.map(c => ({ did: c.did, handle: c.handle })));
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

        // Cache-first: show stale data immediately while refreshing in background
        if (!background) {
            const cached = getCache(cacheKey);
            if (cached) {
                // Instant render from cache — no loading spinner
                setFeedEntries(cached.entries);
                setCursor(cached.cursor);
                // Still fetch fresh data in background (stale-while-revalidate)
                background = true;
            } else {
                setIsLoading(true);
            }
            setFeedError(null);
            verifiedIdsRef.current.clear();
        }

        try {
            if (tab === 'following') {
                // ── Following tab: Bluesky home timeline ──
                const [posts, nextCursor] = await invoke<[BlueskyPost[], string | null]>('get_timeline', { limit: 50 });
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
                // Batch-prefetch blob URLs for images (P6)
                prefetchBlobUrls(deduped).catch(() => {});
                await fetchFollowingList(auth.did);

            } else if (tab === 'you') {
                // ── You tab: user's own posts (Bluesky author feed + local vault) ──
                const [bskyResult, vaultResult, sources] = await Promise.all([
                    invoke<[BlueskyPost[], string | null]>('get_author_feed', { limit: 50 })
                        .catch((): [BlueskyPost[], string | null] => [[], null]),
                    invoke<{ items: BytescapeItem[]; total: number }>('list_vault_posts_paged', {
                        limit: ARCHIVE_PAGE_SIZE,
                        offset: 0,
                        source: archiveSourceFilter || null,
                    }),
                    invoke<string[]>('list_vault_sources').catch((): string[] => []),
                ]);
                if (isStale()) return;
                setArchiveOffset(ARCHIVE_PAGE_SIZE);
                setArchiveTotal(vaultResult.total);
                setVaultSources(sources);

                const [bskyPosts, bskyCursor] = bskyResult;
                const bskyEntries = bskyPosts.map(adaptBlueskyPost);
                const vaultEntries = nestOwnerReplies(vaultResult.items.map(adaptBytescapeItem));

                // Merge both sources, newest first, then deduplicate
                const merged = [...bskyEntries, ...vaultEntries].sort(
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
                // Batch-prefetch blob URLs for images (P6)
                prefetchBlobUrls(dedupedYou).catch(() => {});

            } else if (tab === 'subscribed') {
                // ── Subscribed tab: show content from user's saved/subscribed feeds ──
                // Parallelize independent IPC calls (P7 — saves 200-500ms)
                const [feedsData, tags] = await Promise.all([
                    invoke<GeneratorView[]>('list_saved_feeds').catch((e): GeneratorView[] => {
                        console.error('list_saved_feeds failed:', e);
                        return [];
                    }),
                    invoke<string[]>('list_followed_hashtags').catch((): string[] => []),
                ]);
                // Use ALL saved feed URIs (including following) for subscribed tab
                const allFeedUris = feedsData.map(f => f.uri);
                setFollowedHashtags(tags);

                const activeFeedUris = selectedFeedUris.length > 0
                    ? selectedFeedUris.filter(uri => allFeedUris.includes(uri))
                    : allFeedUris;
                
                const activeTags = selectedHashtagsFilter.length > 0
                    ? selectedHashtagsFilter.filter(tag => tags.includes(tag))
                    : tags;

                if (activeFeedUris.length > 0 || activeTags.length > 0) {
                    try {
                        const [posts, nextCur] = await invoke<[BlueskyPost[], string | null]>('get_unified_feed', {
                            tags: activeTags,
                            feedUris: activeFeedUris,
                            limit: 50,
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
                        // Batch-prefetch blob URLs for images (P6)
                        prefetchBlobUrls(deduped).catch(() => {});
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
                    invoke<GeneratorView[]>('list_saved_feeds').catch((): GeneratorView[] => []),
                    invoke<string[]>('list_followed_hashtags').catch((): string[] => []),
                    invoke<{ items: FeedEntry[]; next_cursor: string | null }>('get_timeline_unified', {
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
                    // Batch-prefetch blob URLs for images (P6)
                    prefetchBlobUrls(deduped).catch(() => {});
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

    // Invalidate subscribed cache when feed URI changes — prevents stale-while-revalidate
    // from showing the old feed's content while the new one loads
    useEffect(() => {
        clearCache('subscribed');
    }, [selectedFeedUris, selectedHashtagsFilter, clearCache]);

    // Invalidate all tab caches when source filter changes to prevent stale data
    useEffect(() => {
        clearCache('all');
        clearCache('you');
        clearCache('following');
        clearCache('subscribed');
        // Also clear filter-specific keys
        if (archiveSourceFilter) {
            clearCache(`all:${archiveSourceFilter}`);
            clearCache(`you:${archiveSourceFilter}`);
            clearCache(`following:${archiveSourceFilter}`);
            clearCache(`subscribed:${archiveSourceFilter}`);
        }
    }, [archiveSourceFilter, clearCache]);

    // Trigger reload on tab/filter change
    useEffect(() => {
        if (!auth) return;
        setCursor(null);
        setArchiveOffset(0);
        fetchFeed(activeTab, false);
    }, [auth, activeTab, selectedFeedUris, selectedHashtagsFilter, archiveSourceFilter, fetchFeed]);

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

    // Background verification of active syndications
    useEffect(() => {
        if (!auth || feedEntries.length === 0) return;

        const isVaultId = (id: string) => !id.startsWith('at://');
        const unverified = feedEntries.filter(entry =>
            isVaultId(entry.id) &&
            entry.share_history?.some(e => e.active) &&
            !verifiedIdsRef.current.has(entry.id)
        );

        if (unverified.length === 0) return;

        unverified.forEach(async (entry) => {
            verifiedIdsRef.current.add(entry.id);
            try {
                const changed = await invoke<boolean>('verify_syndications', { itemId: entry.id });
                if (changed) {
                    fetchFeed(activeTab, true);
                }
            } catch (e) {
                console.error(`sync: verify failed for ${entry.id}`, e);
            }
        });
    }, [feedEntries, auth, activeTab, fetchFeed]);

    // === Load More ===

    const handleLoadMore = async () => {
        if (isFetchingMore || !auth) return;
        setIsFetchingMore(true);

        try {
            if (activeTab === 'following' && cursor) {
                const [posts, nextCursor] = await invoke<[BlueskyPost[], string | null]>('get_timeline', { limit: 30, cursor });
                let newEntries = posts.map(adaptBlueskyPost);
                if (archiveSourceFilter) {
                    newEntries = newEntries.filter(item => item.source.toLowerCase() === archiveSourceFilter.toLowerCase());
                }
                setFeedEntries(prev => deduplicateFeedEntries([...prev, ...newEntries]));
                setCursor(nextCursor);

            } else if (activeTab === 'you') {
                // Load more from both Bluesky author feed and vault
                const [bskyResult, vaultResult] = await Promise.all([
                    cursor
                        ? invoke<[BlueskyPost[], string | null]>('get_author_feed', { limit: 30, cursor })
                            .catch((): [BlueskyPost[], string | null] => [[], null])
                        : Promise.resolve([[], null] as [BlueskyPost[], string | null]),
                    invoke<{ items: BytescapeItem[]; total: number }>('list_vault_posts_paged', {
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
                setFeedEntries(prev => nestOwnerReplies(deduplicateFeedEntries([...prev, ...newEntries])));
                setArchiveOffset(prev => prev + ARCHIVE_PAGE_SIZE);
                setCursor(bskyCursor);

            } else if (activeTab === 'subscribed' && cursor) {
                const allFeedUris = savedFeeds.map(f => f.uri);
                const [posts, nextCur] = await invoke<[BlueskyPost[], string | null]>('get_unified_feed', {
                    tags: followedHashtags || [],
                    feedUris: allFeedUris,
                    limit: 50,
                    cursor,
                });

                if (posts.length > 0) {
                    let newEntries = posts.map(adaptBlueskyPost);
                    if (archiveSourceFilter) {
                        newEntries = newEntries.filter(item => item.source.toLowerCase() === archiveSourceFilter.toLowerCase());
                    }
                    setFeedEntries(prev => deduplicateFeedEntries([...prev, ...newEntries]));
                    setCursor(nextCur);
                }

            } else if (activeTab === 'all') {
                const response = await invoke<{ items: FeedEntry[]; next_cursor: string | null }>('get_timeline_unified', {
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
                    setFeedEntries(prev => deduplicateFeedEntries([...prev, ...newItems]));
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
