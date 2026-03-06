import { useState, useCallback, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { AuthStatus, BlueskyPost, BytescapeItem, FeedEntry, GeneratorView, ProfileView } from '../types';
import { adaptBlueskyPost, adaptBytescapeItem, deduplicateFeedEntries } from '../utils/adapters';

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
    selectedFeedUri: string;
    archiveSourceFilter: string;
    autoRefreshFeeds: boolean;
}

export function useUnifiedFeed({
    auth,
    activeTab,
    selectedFeedUri,
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

    const fetchFollowingList = async (actorDid: string) => {
        try {
            const follows = await invoke<ProfileView[]>('list_follows', { actor: actorDid });
            setFollowingList(follows);
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
    //   'following'  → get_timeline (Bluesky getTimeline API)
    //   'subscribed' → get_unified_feed (merged custom feeds + hashtag search)
    //   'you'        → list_vault_posts_paged (local vault items)
    //   'all'        → get_timeline_unified (Rust merges timeline + feeds + vault)
    //
    // All paths end with deduplicateFeedEntries to collapse duplicates
    // while preserving reposts as distinct events.

    const fetchFeed = useCallback(async (tab: FeedTab, background = false) => {
        if (!auth) return;

        const requestId = ++fetchTokenRef.current;
        const isStale = () => fetchTokenRef.current !== requestId;

        if (!background) {
            setIsLoading(true);
            setFeedError(null);
            verifiedIdsRef.current.clear();
        }

        try {
            if (tab === 'following') {
                // ── Following tab: Bluesky home timeline ──
                const [posts, nextCursor] = await invoke<[BlueskyPost[], string | null]>('get_timeline', { limit: 50 });
                if (isStale()) return;

                const adapted = posts.map(adaptBlueskyPost);
                const deduped = deduplicateFeedEntries(adapted);

                setFeedEntries(deduped);
                setCursor(nextCursor);
                await fetchFollowingList(auth.did);

            } else if (tab === 'subscribed') {
                // ── Subscribed tab: custom feeds + followed hashtags ──
                const [feeds, tags] = await Promise.all([
                    invoke<GeneratorView[]>('list_saved_feeds').catch((): GeneratorView[] => []),
                    invoke<string[]>('list_followed_hashtags').catch((): string[] => []),
                ]);
                if (isStale()) return;
                setSavedFeeds(feeds);
                setFollowedHashtags(tags);

                const allUris = customFeedUris(feeds);
                const targetUris = selectedFeedUri === FEED_FILTER_ALL
                    ? allUris
                    : allUris.filter(uri => uri === selectedFeedUri);

                if (targetUris.length > 0 || tags.length > 0) {
                    const [posts, nextCursor] = await invoke<[BlueskyPost[], string | null]>('get_unified_feed', {
                        tags,
                        feedUris: targetUris,
                        limit: 50,
                        cursor: null,
                    });
                    if (isStale()) return;
                    setFeedEntries(deduplicateFeedEntries(posts.map(adaptBlueskyPost)));
                    setCursor(nextCursor);
                } else {
                    setFeedEntries([]);
                    setCursor(null);
                }

            } else if (tab === 'you') {
                // ── You tab: local vault items ──
                const [result, sources] = await Promise.all([
                    invoke<{ items: BytescapeItem[]; total: number }>('list_vault_posts_paged', {
                        limit: ARCHIVE_PAGE_SIZE,
                        offset: 0,
                        source: archiveSourceFilter || null,
                    }),
                    invoke<string[]>('list_vault_sources').catch((): string[] => []),
                ]);
                if (isStale()) return;
                setArchiveOffset(ARCHIVE_PAGE_SIZE);
                setArchiveTotal(result.total);
                setVaultSources(sources);
                setFeedEntries(result.items.map(adaptBytescapeItem));
                setCursor(null);

            } else if (tab === 'all') {
                // ── All tab: unified feed (Rust merges timeline + feeds + vault) ──
                const feedsData = await invoke<GeneratorView[]>('list_saved_feeds').catch((): GeneratorView[] => []);
                const feedUris = customFeedUris(feedsData);

                const response = await invoke<{ items: FeedEntry[]; next_cursor: string | null }>('get_timeline_unified', {
                    tags: [],
                    feedUris,
                    limit: 30,
                    cursor: null,
                    vaultOffset: 0,
                }).catch(() => null);

                if (isStale()) return;
                setSavedFeeds(feedsData);

                if (response) {
                    const deduped = deduplicateFeedEntries(response.items);

                    setFeedEntries(deduped);
                    setCursor(response.next_cursor);
                    setArchiveOffset(200);
                } else {
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
    }, [auth, selectedFeedUri, archiveSourceFilter]);

    // Trigger reload on tab/filter change
    useEffect(() => {
        if (!auth) return;
        setCursor(null);
        setArchiveOffset(0);
        fetchFeed(activeTab, false);
    }, [auth, activeTab, selectedFeedUri, archiveSourceFilter, fetchFeed]);

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
                setFeedEntries(prev => deduplicateFeedEntries([...prev, ...posts.map(adaptBlueskyPost)]));
                setCursor(nextCursor);

            } else if (activeTab === 'subscribed' && cursor) {
                const uris = selectedFeedUri === FEED_FILTER_ALL
                    ? customFeedUris(savedFeeds)
                    : [selectedFeedUri];
                const [posts, nextCursor] = await invoke<[BlueskyPost[], string | null]>('get_unified_feed', {
                    tags: followedHashtags,
                    feedUris: uris,
                    limit: 30,
                    cursor,
                });
                setFeedEntries(prev => deduplicateFeedEntries([...prev, ...posts.map(adaptBlueskyPost)]));
                setCursor(nextCursor);

            } else if (activeTab === 'you') {
                const result = await invoke<{ items: BytescapeItem[]; total: number }>('list_vault_posts_paged', {
                    limit: ARCHIVE_PAGE_SIZE,
                    offset: archiveOffset,
                    source: archiveSourceFilter || null,
                });
                setFeedEntries(prev => deduplicateFeedEntries([...prev, ...result.items.map(adaptBytescapeItem)]));
                setArchiveOffset(prev => prev + ARCHIVE_PAGE_SIZE);

            } else if (activeTab === 'all') {
                const response = await invoke<{ items: FeedEntry[]; next_cursor: string | null }>('get_timeline_unified', {
                    tags: [],
                    feedUris: customFeedUris(savedFeeds),
                    limit: 30,
                    cursor,
                    vaultOffset: archiveOffset,
                }).catch(() => null);

                if (response) {
                    setFeedEntries(prev => deduplicateFeedEntries([...prev, ...response.items]));
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
