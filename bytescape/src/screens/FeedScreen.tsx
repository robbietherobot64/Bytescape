import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { AuthStatus, BlueskyPost, BytescapeItem, GeneratorView, ProfileView, PublishResult, FeedEntry } from '../types';
import { FeedEntryCard } from '../components/FeedEntryCard';
import { ComposeBox } from '../components/ComposeBox';
import { FeedRightPanel } from '../components/FeedRightPanel';
import { adaptBlueskyPost, adaptBytescapeItem } from '../utils/adapters';

interface FeedScreenProps {
    auth: AuthStatus | null;
}

type FeedTab = 'all' | 'following' | 'feeds' | 'hashtags' | 'rss' | 'archives';

const LIVE_TABS: FeedTab[] = ['all', 'following', 'feeds', 'hashtags'];
const FEED_FILTER_ALL = '__all__';
const ARCHIVE_PAGE_SIZE = 200;

function customFeedUris(feeds: GeneratorView[]): string[] {
    return feeds
        .map(f => f.uri)
        .filter(uri => !uri.includes('app.bsky.feed.generator/following'));
}

export function FeedScreen({ auth }: FeedScreenProps) {
    // === Tabs ===
    const [activeTab, setActiveTab] = useState<FeedTab>('all');

    // === Data State ===
    const [feedEntries, setFeedEntries] = useState<FeedEntry[]>([]);
    const [isLoading, setIsLoading] = useState(false);
    const [feedError, setFeedError] = useState<string | null>(null);
    const [cursor, setCursor] = useState<string | null>(null);
    const [isFetchingMore, setIsFetchingMore] = useState(false);
    const [autoRefreshFeeds, setAutoRefreshFeeds] = useState(false);

    // === Context Data (Right Panel) ===
    const [followingList, setFollowingList] = useState<ProfileView[]>([]);
    const [savedFeeds, setSavedFeeds] = useState<GeneratorView[]>([]);
    const [selectedFeedUri, setSelectedFeedUri] = useState<string>(FEED_FILTER_ALL);
    const [followedHashtags, setFollowedHashtags] = useState<string[]>([]);
    const [hashtagInput, setHashtagInput] = useState('');
    const [archiveSourceFilter, setArchiveSourceFilter] = useState<string>('');
    const [archiveOffset, setArchiveOffset] = useState(0);
    const [archiveTotal, setArchiveTotal] = useState(0);
    const [vaultSources, setVaultSources] = useState<string[]>([]);
    const [composeText, setComposeText] = useState('');
    const [postTarget] = useState<'local' | 'bluesky' | 'activitypub' | 'all'>('local');
    const [isPosting, setIsPosting] = useState(false);
    const [postStatus, setPostStatus] = useState<string>('');

    // Track latest fetch to avoid stale updates when switching tabs mid-request
    const fetchTokenRef = useRef(0);


    // === Setup ===
    // Initial fetch
    const fetchFeed = useCallback(async (tab: FeedTab, background = false) => {
        if (!auth) return;

        const requestId = ++fetchTokenRef.current;
        const isStale = () => fetchTokenRef.current !== requestId;

        if (!background) {
            setIsLoading(true);
            setFeedError(null);
        }

        try {
            console.log(`Fetching feed for tab: ${tab}${background ? ' (background)' : ''}`);

            if (tab === 'following') {
                const [posts, nextCursor] = await invoke<[BlueskyPost[], string | null]>('get_timeline', { limit: 50 });
                if (isStale()) return;
                setFeedEntries(posts.map(adaptBlueskyPost));
                setCursor(nextCursor);
                await fetchFollowingList(auth.did);

            } else if (tab === 'feeds') {
                const feeds = await invoke<GeneratorView[]>("list_saved_feeds");
                if (isStale()) return;
                setSavedFeeds(feeds);

                const allCustomUris = customFeedUris(feeds);
                const targetFeedUris = selectedFeedUri === FEED_FILTER_ALL
                    ? allCustomUris
                    : allCustomUris.filter(uri => uri === selectedFeedUri);

                const [unified, nextCursor] = await invoke<[BlueskyPost[], string | null]>('get_unified_feed', {
                    tags: [],
                    feedUris: targetFeedUris,
                    limit: 50,
                    cursor: null
                });
                if (isStale()) return;
                setFeedEntries(unified.map(adaptBlueskyPost));
                setCursor(nextCursor);

            } else if (tab === 'hashtags') {
                const tags = await invoke<string[]>('list_followed_hashtags');
                if (isStale()) return;
                setFollowedHashtags(tags);
                if (tags.length > 0) {
                    const [posts, nextCursor] = await invoke<[BlueskyPost[], string | null]>('get_unified_feed', {
                        tags,
                        feedUris: [],
                        limit: 50,
                        cursor: null
                    });
                    if (isStale()) return;
                    setFeedEntries(posts.map(adaptBlueskyPost));
                    setCursor(nextCursor);
                } else {
                    setFeedEntries([]);
                    setCursor(null);
                }
                if (isStale()) return;

            } else if (tab === 'rss') {
                // RSS connector not yet implemented
                if (isStale()) return;
                setFeedEntries([]);
                setCursor(null);

            } else if (tab === 'archives') {
                const pageSize = ARCHIVE_PAGE_SIZE;
                const [result, sources] = await Promise.all([
                    invoke<{ items: BytescapeItem[]; total: number }>('list_vault_posts_paged', {
                        limit: pageSize,
                        offset: 0,
                        source: archiveSourceFilter || null,
                    }),
                    invoke<string[]>('list_vault_sources').catch((): string[] => []),
                ]);
                if (isStale()) return;
                setArchiveOffset(pageSize);
                setArchiveTotal(result.total);
                setVaultSources(sources);
                setFeedEntries(result.items.map(adaptBytescapeItem));
                setCursor(null);

            } else if (tab === 'all') {
                // Merge live timeline + custom feeds + vault items, sorted by timestamp
                const [timelineRes, vaultRes, feedsData] = await Promise.all([
                    invoke<[BlueskyPost[], string | null]>('get_timeline', { limit: 30 })
                        .catch((): [BlueskyPost[], string | null] => [[], null]),
                    invoke<{ items: BytescapeItem[]; total: number }>('list_vault_posts_paged', { limit: ARCHIVE_PAGE_SIZE, offset: 0, source: null })
                        .catch(() => ({ items: [] as BytescapeItem[], total: 0 })),
                    invoke<GeneratorView[]>("list_saved_feeds").catch((): GeneratorView[] => []),
                ]);

                const [timeline, nextCursor] = timelineRes;
                if (isStale()) return;
                setSavedFeeds(feedsData);
                setCursor(nextCursor);

                const feedUris = customFeedUris(feedsData);
                let feedPosts: BlueskyPost[] = [];
                if (feedUris.length > 0) {
                    const [posts] = await invoke<[BlueskyPost[], string | null]>('get_unified_feed', {
                        tags: [],
                        feedUris,
                        limit: 30,
                        cursor: null
                    }).catch((): [BlueskyPost[], string | null] => [[], null]);
                    feedPosts = posts;
                }

                const seen = new Set<string>();
                const livePosts = [...timeline, ...feedPosts]
                    .sort((a, b) => b.created_at - a.created_at)
                    .filter(p => {
                        const id = p.uri || p.cid;
                        if (seen.has(id)) return false;
                        seen.add(id);
                        return true;
                    });

                const merged = [...livePosts.map(adaptBlueskyPost), ...vaultRes.items.map(adaptBytescapeItem)]
                    .sort((a, b) => b.timestamp - a.timestamp);
                if (isStale()) return;
                setFeedEntries(merged);
                await fetchFollowingList(auth.did);
            }

            if (!background && !isStale()) {
                setFeedError(null);
            }
        } catch (e) {
            console.error("Failed to fetch feed:", e);
            if (!background && !isStale()) {
                setFeedError(typeof e === 'string' ? e : 'Failed to fetch feed data');
            }
        } finally {
            if (!background && !isStale()) {
                setIsLoading(false);
            }
        }
    }, [auth, selectedFeedUri, archiveSourceFilter]);

    useEffect(() => {
        if (!auth) return;
        setCursor(null);
        setArchiveOffset(0);
        fetchFeed(activeTab, false);
    }, [auth, activeTab, selectedFeedUri, fetchFeed]);

    useEffect(() => {
        if (!auth || !autoRefreshFeeds) return;
        if (!LIVE_TABS.includes(activeTab)) return;

        const interval = setInterval(() => {
            if (typeof document !== 'undefined' && document.hidden) return;
            fetchFeed(activeTab, true);
        }, 45000);

        return () => clearInterval(interval);
    }, [auth, activeTab, autoRefreshFeeds, fetchFeed]);

    useEffect(() => {
        const uris = customFeedUris(savedFeeds);
        if (selectedFeedUri !== FEED_FILTER_ALL && !uris.includes(selectedFeedUri)) {
            setSelectedFeedUri(FEED_FILTER_ALL);
        }
    }, [savedFeeds, selectedFeedUri]);

    const fetchFollowingList = async (actorDid: string) => {
        try {
            const follows = await invoke<ProfileView[]>('list_follows', { actor: actorDid });
            setFollowingList(follows);
        } catch (e) {
            console.error('Failed to fetch follows, falling back to local contacts:', e);
            try {
                const contacts = await invoke<any[]>('list_contacts');
                const fallback: ProfileView[] = contacts.map(c => ({
                    did: c.did,
                    handle: c.handle,
                }));
                setFollowingList(fallback);
            } catch (e2) {
                console.error('Failed to fetch local contacts fallback:', e2);
                setFollowingList([]);
            }
        }
    };

    const handleLoadMore = async () => {
        if (isFetchingMore || !auth) return;
        setIsFetchingMore(true);
        try {
            if (activeTab === 'following' && cursor) {
                const [newPosts, nextCursor] = await invoke<[BlueskyPost[], string | null]>('get_timeline', { limit: 30, cursor });
                setFeedEntries(prev => [...prev, ...newPosts.map(adaptBlueskyPost)]);
                setCursor(nextCursor);
            } else if ((activeTab === 'feeds' || activeTab === 'hashtags') && cursor) {
                const tags = activeTab === 'hashtags' ? followedHashtags : [];
                const uris = activeTab === 'feeds' ? (selectedFeedUri === FEED_FILTER_ALL ? customFeedUris(savedFeeds) : [selectedFeedUri]) : [];
                const [newPosts, nextCursor] = await invoke<[BlueskyPost[], string | null]>('get_unified_feed', {
                    tags,
                    feedUris: uris,
                    limit: 30,
                    cursor
                });
                setFeedEntries(prev => [...prev, ...newPosts.map(adaptBlueskyPost)]);
                setCursor(nextCursor);
            } else if (activeTab === 'archives') {
                const pageSize = ARCHIVE_PAGE_SIZE;
                const result = await invoke<{ items: BytescapeItem[]; total: number }>('list_vault_posts_paged', {
                    limit: pageSize,
                    offset: archiveOffset,
                    source: archiveSourceFilter || null,
                });
                setFeedEntries(prev => [...prev, ...result.items.map(adaptBytescapeItem)]);
                setArchiveOffset(prev => prev + pageSize);
            }
        } catch (err) {
            console.error("Load more failed:", err);
        } finally {
            setIsFetchingMore(false);
        }
    };

    const handleAddHashtag = async () => {
        const tag = hashtagInput.trim().replace(/^#/, '');
        if (!tag) return;
        try {
            await invoke('save_hashtag', { tag });
            setHashtagInput('');
            const updated = await invoke<string[]>('list_followed_hashtags');
            setFollowedHashtags(updated);
            if (activeTab === 'hashtags') fetchFeed('hashtags', false);
        } catch (e) {
            console.error('Failed to save hashtag:', e);
        }
    };

    const handleRemoveHashtag = async (tag: string) => {
        try {
            await invoke('delete_hashtag', { tag });
            const updated = await invoke<string[]>('list_followed_hashtags');
            setFollowedHashtags(updated);
            if (activeTab === 'hashtags') fetchFeed('hashtags', false);
        } catch (e) {
            console.error('Failed to remove hashtag:', e);
        }
    };

    const handlePublishPost = async () => {
        const text = composeText.trim();
        if (!text) return;

        setIsPosting(true);
        setPostStatus('Publishing...');
        try {
            const result = await invoke<PublishResult>('publish_post', {
                text,
                destination: postTarget,
            });

            const okTargets = [
                result.local ? 'Local' : null,
                result.bluesky ? 'Bluesky' : null,
                result.activitypub ? 'ActivityPub' : null,
            ].filter(Boolean).join(', ');

            if (result.errors.length > 0) {
                setPostStatus(`${okTargets ? `Posted to ${okTargets}. ` : ''}${result.errors[0]}`);
            } else {
                setPostStatus(`✅ Posted to ${okTargets || 'selected destination'}`);
                setComposeText('');
                fetchFeed(activeTab, false);
            }
        } catch (e: any) {
            setPostStatus(typeof e === 'string' ? e : 'Failed to publish post');
        } finally {
            setIsPosting(false);
        }
    };

    // === Media Path Resolution ===
    const [mediaDir, setMediaDir] = useState<string>("");
    useEffect(() => {
        invoke('get_media_dir').then((dir) => setMediaDir(dir as string));
    }, []);

    const resolveMediaPath = useCallback((path: string) => {
        if (!path) return '';
        if (path.startsWith('http') || path.startsWith('blob:') || path.startsWith('data:')) return path;

        // Handle relative paths (media/filename)
        // Similar logic to old feed, but simplified if backend is consistent now
        if (mediaDir && !path.startsWith('/')) {
            // Basic join logic
            const filename = path.replace(/^media\//, ""); // remove prefix if present to avoid doubling
            return convertFileSrc(`${mediaDir}/${filename}`);
        }
        return convertFileSrc(path);
    }, [mediaDir]);


    // === Render ===
    return (
        <div className="feed-layout">

            {/* Main Content Area */}
            <div className="feed-main">

                {/* Header / Tabs */}
                <div className="feed-header">
                    <div style={{ display: 'flex', gap: 16, alignItems: 'center', flexWrap: 'wrap', minWidth: 0 }}>
                        <h1 style={{ margin: 0, fontSize: '1.25rem', fontWeight: 700, whiteSpace: 'nowrap' }}>Home</h1>

                        {/* Tabs */}
                        <div className="tabs">
                            {([
                                ['all', 'All'],
                                ['following', 'Following'],
                                ['feeds', 'Feeds'],
                                ['hashtags', '#Hashtags'],
                                ['rss', 'RSS'],
                                ['archives', 'Archives'],
                            ] as [FeedTab, string][]).map(([tab, label]) => (
                                <button
                                    key={tab}
                                    className={`tab ${activeTab === tab ? 'active' : ''}`}
                                    onClick={() => setActiveTab(tab)}
                                >
                                    {label}
                                </button>
                            ))}
                        </div>
                    </div>

                    <button className="btn btn-ghost btn-sm" onClick={() => fetchFeed(activeTab, false)}>
                        Refresh
                    </button>
                </div>

                {(LIVE_TABS.includes(activeTab) || activeTab === 'archives') && (
                    <div style={{ padding: '0 16px 10px', fontSize: '0.78rem', color: 'var(--text-muted)', display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
                        <span>
                            {activeTab === 'archives' ? 'Your imported content — read-only.' : 'Live view from connected networks.'}
                        </span>
                        {LIVE_TABS.includes(activeTab) && (
                            <label style={{ display: 'inline-flex', alignItems: 'center', gap: 6, cursor: 'pointer' }}>
                                <input
                                    type="checkbox"
                                    checked={autoRefreshFeeds}
                                    onChange={e => setAutoRefreshFeeds(e.target.checked)}
                                />
                                Auto-refresh
                            </label>
                        )}
                    </div>
                )}

                {/* Feed List */}
                <div className="feed-scroll-container">

                    <ComposeBox 
                        onPublish={handlePublishPost} 
                        isPosting={isPosting} 
                        postStatus={postStatus} 
                    />

                    {isLoading && <div style={{ textAlign: 'center', padding: 20 }}>Loading...</div>}

                    {feedError && (
                        <div className="card" style={{ margin: '12px 0', borderColor: 'rgba(239,68,68,0.4)' }}>
                            <div style={{ color: '#fca5a5', fontSize: '0.86rem' }}>⚠️ {feedError}</div>
                        </div>
                    )}

                    {activeTab === 'rss' && !isLoading && (
                        <div className="card" style={{ textAlign: 'center', padding: 32, color: 'var(--text-secondary)' }}>
                            <div style={{ fontSize: '2rem', marginBottom: 12 }}>📡</div>
                            <div style={{ fontWeight: 600, marginBottom: 8 }}>RSS / Atom — coming soon</div>
                            <div style={{ fontSize: '0.85rem' }}>Subscribe to any blog, newsletter, or Atom feed. Your subscriptions live in your Willow vault alongside everything else.</div>
                        </div>
                    )}

                    {activeTab !== 'rss' && !isLoading && <FeedList entries={feedEntries} resolveMediaPath={resolveMediaPath} mediaDir={mediaDir} />}

                    {!isLoading && feedEntries.length > 0 &&
                     (activeTab === 'following' ? !!cursor : activeTab === 'archives' ? feedEntries.length < archiveTotal : false) && (
                        <div style={{ textAlign: 'center', padding: '20px 0' }}>
                            <button
                                className="btn btn-ghost"
                                onClick={handleLoadMore}
                                disabled={isFetchingMore}
                            >
                                {isFetchingMore ? 'Loading...' : 'Load More'}
                            </button>
                        </div>
                    )}
                </div>
            </div>

            {/* Right Context Panel (Desktop) */}
            <FeedRightPanel
                activeTab={activeTab}
                followingList={followingList}
                savedFeeds={savedFeeds}
                selectedFeedUri={selectedFeedUri}
                setSelectedFeedUri={setSelectedFeedUri}
                FEED_FILTER_ALL={FEED_FILTER_ALL}
                hashtagInput={hashtagInput}
                setHashtagInput={setHashtagInput}
                handleAddHashtag={handleAddHashtag}
                followedHashtags={followedHashtags}
                handleRemoveHashtag={handleRemoveHashtag}
                archiveSourceFilter={archiveSourceFilter}
                setArchiveSourceFilter={setArchiveSourceFilter}
                vaultSources={vaultSources}
                archiveTotal={archiveTotal}
            />
        </div>
    );
}


function FeedList({ entries, resolveMediaPath, mediaDir }: { entries: FeedEntry[], resolveMediaPath: (p: string) => string, mediaDir: string }) {
    const visibleEntries = filterArchiveArtifacts(filterFacebookDuplicateImagePosts(entries));

    if (visibleEntries.length === 0) {
        return <div className="empty-state">No posts found.</div>;
    }

    return (
        <div className="feed-list">
            {visibleEntries.map(entry =>
                <FeedEntryCard key={entry.id} entry={entry} resolveMediaPath={resolveMediaPath} mediaDir={mediaDir} />
            )}
        </div>
    );
}

function isLiveSource(source?: string): boolean {
    const s = (source || '').toLowerCase();
    return s === 'bluesky' || s === 'mastodon';
}

function looksLikeGifUrl(url?: string): boolean {
    if (!url) return false;
    const clean = url.toLowerCase().split('?')[0];
    return clean.endsWith('.gif') || clean.includes('.gif#') || clean.includes('/gif/');
}

function isGroupSharePost(entry: FeedEntry): boolean {
    const haystack = `${entry.title || ''} ${entry.content || ''}`.toLowerCase();
    // e.g. "Rob shared a link in Ska Musicians group"
    return /shared\s+(a\s+)?link\s+in\s+.+\s+group/.test(haystack);
}

function filterArchiveArtifacts(entries: FeedEntry[]): FeedEntry[] {
    return entries.filter(entry => {
        // Keep live network behavior untouched (including GIFs)
        if (isLiveSource(entry.source)) return true;

        const source = (entry.source || '').toLowerCase();

        // Hide archived GIF posts/media artifacts in archive-derived sources.
        if (source === 'facebook' || source === 'instagram') {
            const hasGifMedia = (entry.media_urls || []).some(looksLikeGifUrl)
                || (entry.media_thumbnails || []).some(looksLikeGifUrl)
                || looksLikeGifUrl(entry.link_url || undefined);
            if (hasGifMedia) return false;

            // Hide group-share artifacts from public/main feed.
            if (source === 'facebook' && isGroupSharePost(entry)) {
                return false;
            }
        }

        return true;
    });
}

function normalizeMediaKey(url: string): string {
    if (!url) return '';
    return url.trim().toLowerCase().split('?')[0];
}

function facebookContextScore(entry: FeedEntry): number {
    const text = `${entry.title || ''} ${entry.content || ''}`
        .replace(/timeline photos/gi, ' ')
        .replace(/\s{2,}/g, ' ')
        .trim();

    let score = Math.min(text.length, 240);
    if (/cover photos?|profile photos?|album|added\s+\d+\s+new\s+photos?/i.test(text)) {
        score += 60;
    }
    if ((entry.media_urls?.length || 0) > 1) {
        score += Math.min(entry.media_urls.length * 3, 24);
    }
    return score;
}

function filterFacebookDuplicateImagePosts(entries: FeedEntry[]): FeedEntry[] {
    if (entries.length === 0) return entries;

    const byId = new Map<string, FeedEntry>();
    for (const e of entries) byId.set(e.id, e);

    const mediaToBest = new Map<string, string>();

    const facebookWithMedia = entries.filter(e =>
        (e.source || '').toLowerCase() === 'facebook' &&
        Array.isArray(e.media_urls) &&
        e.media_urls.length > 0
    );

    for (const entry of facebookWithMedia) {
        const keys = entry.media_urls.map(normalizeMediaKey).filter(Boolean);
        const entryScore = facebookContextScore(entry);
        for (const key of keys) {
            const existingId = mediaToBest.get(key);
            if (!existingId) {
                mediaToBest.set(key, entry.id);
                continue;
            }
            const existing = byId.get(existingId);
            if (!existing) {
                mediaToBest.set(key, entry.id);
                continue;
            }

            const existingScore = facebookContextScore(existing);
            const shouldReplace =
                entryScore > existingScore ||
                (entryScore === existingScore && (entry.media_urls.length || 0) > (existing.media_urls.length || 0)) ||
                (entryScore === existingScore && (entry.media_urls.length || 0) === (existing.media_urls.length || 0) && entry.timestamp > existing.timestamp);

            if (shouldReplace) {
                mediaToBest.set(key, entry.id);
            }
        }
    }

    return entries.filter(entry => {
        if ((entry.source || '').toLowerCase() !== 'facebook') return true;
        if (!entry.media_urls || entry.media_urls.length === 0) return true;

        const keys = entry.media_urls.map(normalizeMediaKey).filter(Boolean);
        if (keys.length === 0) return true;

        const winners = keys
            .map(k => mediaToBest.get(k))
            .filter((id): id is string => !!id)
            .map(id => byId.get(id))
            .filter((e): e is FeedEntry => !!e);

        if (winners.length === 0) return true;
        if (winners.some(w => w.id === entry.id)) return true;

        // Hide only older duplicate uploads once a richer winner exists.
        const hasRicherOrEqualNewerWinner = winners.every(w => {
            const winnerScore = facebookContextScore(w);
            const entryScore = facebookContextScore(entry);
            return winnerScore >= entryScore && w.timestamp >= entry.timestamp;
        });

        return !hasRicherOrEqualNewerWinner;
    });
}
