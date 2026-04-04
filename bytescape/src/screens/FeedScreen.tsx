import "./FeedScreen.css";
import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { AuthStatus, PublishResult } from '../types';
import { ComposeBox, Attachment, PublishOptions, PostTarget } from '../components/ComposeBox';
import { platformAbbrev } from '../utils/platform';
import { FeedRightPanel } from '../components/FeedRightPanel';
import { FriendsPanel } from '../components/FriendsPanel';
import { VirtualizedFeedList } from '../components/feed/VirtualizedFeedList';
import { filterFacebookDuplicateImagePosts } from '../utils/feedAnalytics';
import { useUnifiedFeed, FeedTab, FEED_FILTER_ALL, FEED_TAB_LABELS } from '../hooks/useUnifiedFeed';
import { appDataDir } from '@tauri-apps/api/path';
import { BsGlobe } from 'react-icons/bs';

/**
 * PURPOSE
 * - Render the primary social feed surface: compose, filter, browse, and
 *   paginate content across local + protocol-backed sources.
 *
 * HOW IT WORKS
 * - Delegates feed retrieval/state machine behavior to `useUnifiedFeed`.
 * - Publishes posts via engine command (`publish_post_v2`) and refreshes feed.
 * - Resolves local media paths to app-safe URLs (`convertFileSrc`).
 * - Applies client-side media/content lens filtering plus dedupe pass.
 * - Supports overscroll pull-to-refresh and infinite/older pagination flows.
 *
 * RELATED FILES
 * - src/hooks/useUnifiedFeed.ts
 * - src/components/ComposeBox.tsx
 * - src/components/feed/VirtualizedFeedList.tsx
 * - src/components/FeedRightPanel.tsx
 * - src/components/FriendsPanel.tsx
 * - src-tauri/src/lib.rs (publish + hashtag commands)
 *
 * CROSS-SYSTEM NOTES
 * - `useUnifiedFeed` encapsulates cross-tab feed composition and cache policy.
 * - `invoke()` bridges publish and hashtag mutations to engine-side commands.
 * - Custom window events (`navigate-hashtag`) coordinate card-level interactions
 *   with screen-level tab/filter state.
 */

/**
 * UI-only secondary feed lens applied after unified feed fetch.
 */
export type MediaFilter = 'all' | 'images' | 'video' | 'short' | 'long' | 'articles';

const CONTEXT_LABELS: Record<string, string> = {
    all: 'Discovery Tools',
    following: 'My Contacts',
    subscribed: 'Subscriptions',
    you: 'Your Posts',
    groups: 'Group Subscriptions',
};

interface FeedScreenProps {
    auth: AuthStatus | null;
    activeTab: FeedTab;
    setActiveTab: (tab: FeedTab) => void;
    isImporting?: boolean;
    onOpenChat?: (peerNodeId: string, peerName: string) => void;
    onViewProfile?: (peerNodeId: string, peerName?: string) => void;
}

/**
 * Main feed composition and browsing screen.
 */
export function FeedScreen({ auth, activeTab, setActiveTab, isImporting, onOpenChat, onViewProfile }: FeedScreenProps) {
    // ═══ UI Control State ═══
    // === UI Control State ===
    const [contextOpen, setContextOpen] = useState(false);
    const [mediaFilter, setMediaFilter] = useState<MediaFilter>('all');
    const autoRefreshFeeds = true;
    const [selectedFeedUris, setSelectedFeedUris] = useState<string[]>([]);
    const [selectedHashtagsFilter, setSelectedHashtagsFilter] = useState<string[]>([]);
    const [hashtagInput, setHashtagInput] = useState('');
    const [archiveSourceFilter, setArchiveSourceFilter] = useState<string>('');
    const [isPosting, setIsPosting] = useState(false);
    const [postStatus, setPostStatus] = useState<string>('');
    const [mediaDir, setMediaDir] = useState<string>('');

    // === Pull-to-Refresh State ===
    const [pullDistance, setPullDistance] = useState(0);
    const [isRefreshing, setIsRefreshing] = useState(false);
    const scrollContainerRef = useRef<HTMLDivElement>(null);
    const activeTabRef = useRef(activeTab);
    activeTabRef.current = activeTab;
    const PULL_THRESHOLD = 80;
    const triggerRefreshRef = useRef<() => Promise<void>>(() => Promise.resolve());

    // ═══ Feed Data Integration (`useUnifiedFeed`) ═══
    // === Data Hook ===
    const {
        feedEntries,
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
        handleLoadMore
    } = useUnifiedFeed({
        auth,
        activeTab,
        selectedFeedUris,
        selectedHashtagsFilter,
        archiveSourceFilter,
        autoRefreshFeeds
    });

    // ═══ Lifecycle Wiring (media dir, hashtag navigation, import refresh) ═══
    useEffect(() => {
        appDataDir().then(dir => setMediaDir(dir)).catch(console.error);
    }, []);

    // Listen for hashtag chip clicks from FeedEntryCard (navigate-hashtag custom event)
    useEffect(() => {
        const handler = (e: Event) => {
            const tag = (e as CustomEvent<string>).detail;
            if (!tag) return;
            // Switch to subscribed tab and set the hashtag filter
            setActiveTab('subscribed');
            setSelectedHashtagsFilter([tag]);
            setSelectedFeedUris([]);
        };
        window.addEventListener('navigate-hashtag', handler);
        return () => window.removeEventListener('navigate-hashtag', handler);
    }, [setActiveTab]);

    // Refresh feed after boot import finishes (isImporting: true → false)
    const wasImportingRef = useRef(false);
    useEffect(() => {
        if (wasImportingRef.current && !isImporting) {
            // Import just finished — refresh the feed to pick up new vault data (including reposts)
            fetchFeed(activeTabRef.current, true);
        }
        wasImportingRef.current = !!isImporting;
    }, [isImporting, fetchFeed]);

    useEffect(() => {
        // Only run reset if we actually have feeds loaded and current selection is invalid
        if (savedFeeds.length > 0 && selectedFeedUris.length > 0) {
            const uris = savedFeeds.map(f => f.uri);
            const validUris = selectedFeedUris.filter(uri => uris.includes(uri));
            if (validUris.length !== selectedFeedUris.length) {
                setSelectedFeedUris(validUris);
            }
        }
    }, [savedFeeds, selectedFeedUris]);

    // ═══ Action Handlers: Hashtags + Publish + Media Resolve ═══
    const handleAddHashtag = async () => {
        const tag = hashtagInput.trim().replace(/^#/, '');
        if (!tag) return;
        try {
            const current = followedHashtags || [];
            if (!current.includes(tag)) {
                const refreshed = await invoke<string[]>('add_followed_hashtag', { hashtag: tag });
                setFollowedHashtags(refreshed);
                // Also follow on Mastodon if a session is active (silent — no-ops if not connected)
                invoke('mastodon_follow_hashtag', { hashtag: tag }).catch(() => {});
            }
            setHashtagInput('');
        } catch (e) {
            console.error('Failed to add hashtag:', e);
        }
    };

    const handleRemoveHashtag = async (tag: string) => {
        try {
            const refreshed = await invoke<string[]>('remove_followed_hashtag', { hashtag: tag });
            setFollowedHashtags(refreshed);
            // Also unfollow on Mastodon if connected (silent)
            invoke('mastodon_unfollow_hashtag', { hashtag: tag }).catch(() => {});
        } catch (e) {
            console.error('Failed to remove hashtag:', e);
        }
    };

    // ═══ Derived Feed Projection (dedupe + media lens filtering) ═══
    const handlePublishPost = async (text: string, target: PostTarget, attachments: Attachment[], options?: PublishOptions) => {
        setIsPosting(true);
        setPostStatus('Publishing...');
        try {
            const result = await invoke<PublishResult>('publish_post_v2', {
                text,
                attachments: attachments.map(a => ({
                    path: a.path,
                    blob_hash: a.blob_hash,
                    mime_type: a.mime_type
                })),
                destination: target,
                itemType: options?.itemType || 'Status',
                title: options?.title || null,
                linkCard: options?.linkCard ? {
                    url: options.linkCard.url,
                    title: options.linkCard.title ?? null,
                    description: options.linkCard.description ?? null,
                    image_url: options.linkCard.image_url ?? null,
                } : null,
            });

            const okTargets = [
                result.local ? 'Local' : null,
                result.bluesky ? 'Bluesky' : null,
                result.mastodon ? 'Mastodon' : null,
                result.activitypub ? 'ActivityPub' : null,
            ].filter(Boolean).join(', ');

            if (result.errors.length > 0) {
                setPostStatus(`${okTargets ? `Posted to ${okTargets}. ` : ''}${result.errors[0]}`);
            } else {
                setPostStatus(`✅ Posted to ${okTargets || 'selected destination'}`);
                fetchFeed(activeTab, false);
            }
        } catch (e: any) {
            setPostStatus(typeof e === 'string' ? e : 'Failed to publish post');
        } finally {
            setIsPosting(false);
        }
    };

    const resolveMediaPath = (src: string) => {
        if (!src) return '';
        if (src.startsWith('http://') || src.startsWith('https://') || src.startsWith('asset://') || src.startsWith('tauri://') || src.startsWith('blob:')) return src;

        if (mediaDir) {
            if (src.startsWith('media/')) {
                const filename = src.replace(/^media\//, '');
                return convertFileSrc(`${mediaDir}/${filename}`);
            }
            if (src.startsWith('/') || src.match(/^[A-Za-z]:\\/)) {
                return convertFileSrc(src);
            }
            return convertFileSrc(`${mediaDir}/${src}`);
        }
        return src;
    };

    // === Filtered + Deduped entries (memoized for virtualizer stability) ===
    const filteredEntries = useMemo(() => {
        const deduped = filterFacebookDuplicateImagePosts(feedEntries);
        if (mediaFilter === 'all') return deduped;
        return deduped.filter(entry => {
            const hasImages = entry.media_urls.length > 0 && (!entry.media_kinds || entry.media_kinds.some(k => k === 'image'));
            const hasVideo = entry.media_kinds?.some(k => k === 'video') || entry.media_urls.some(u => /\.(mp4|webm|mov|m3u8)/i.test(u));
            const textLen = (entry.content || '').length;
            const isArticle = entry.item_type === 'Article' || entry.item_type === 'Link' || (entry.link_url && entry.link_title);
            switch (mediaFilter) {
                case 'images': return hasImages && !hasVideo;
                case 'video': return hasVideo;
                case 'short': return textLen > 0 && textLen <= 300 && !hasImages && !hasVideo && !isArticle;
                case 'long': return textLen > 300 && !isArticle;
                case 'articles': return !!isArticle;
                default: return true;
            }
        });
    }, [feedEntries, mediaFilter]);

    // ═══ Pull-to-Refresh Interaction State Machine ═══
    // === Pull-to-Refresh via Scroll Wheel Overscroll ===
    const overscrollAccum = useRef(0);
    const decayTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

    const handleWheel = useCallback((e: React.WheelEvent) => {
        const container = scrollContainerRef.current;
        if (!container || isRefreshing) return;

        // Only activate when scrolled to the very top and scrolling up
        if (container.scrollTop <= 0 && e.deltaY < 0) {
            // Accumulate upward scroll with rubber-band dampening
            overscrollAccum.current = Math.min(
                overscrollAccum.current + Math.abs(e.deltaY) * 0.35,
                140
            );
            setPullDistance(overscrollAccum.current);

            // Reset decay timer — snap back if user stops scrolling
            if (decayTimer.current) clearTimeout(decayTimer.current);
            decayTimer.current = setTimeout(() => {
                if (!isRefreshing) {
                    // Check if we passed threshold before decaying
                    if (overscrollAccum.current >= PULL_THRESHOLD) {
                        triggerRefreshRef.current();
                    } else {
                        overscrollAccum.current = 0;
                        setPullDistance(0);
                    }
                }
            }, 300);

            e.preventDefault();
        } else if (overscrollAccum.current > 0 && !isRefreshing) {
            // Scrolling down or not at top — reset overscroll
            overscrollAccum.current = 0;
            setPullDistance(0);
            if (decayTimer.current) clearTimeout(decayTimer.current);
        }
    }, [isRefreshing]);

    const triggerRefresh = useCallback(async () => {
        setIsRefreshing(true);
        setPullDistance(PULL_THRESHOLD);
        try {
            await fetchFeed(activeTabRef.current, false);
        } finally {
            overscrollAccum.current = 0;
            setIsRefreshing(false);
            setPullDistance(0);
        }
    }, [fetchFeed]);
    triggerRefreshRef.current = triggerRefresh;

    // Cleanup decay timer on unmount
    useEffect(() => {
        return () => {
            if (decayTimer.current) clearTimeout(decayTimer.current);
        };
    }, []);

    const handleNavigateTab = useCallback((tab: FeedTab, opts?: { feedUri?: string }) => {
        setActiveTab(tab);
        if (opts?.feedUri) {
            opts.feedUri ? setSelectedFeedUris([opts.feedUri]) : setSelectedFeedUris([]);
        }
    }, [setActiveTab]);

    // ═══ Render: Context Panel + Feed Body + Friends Panel ═══
    return (
        <div className="feed-layout">
            {/* Feed context panel — left side */}
            <FeedRightPanel
                activeTab={activeTab}
                followingList={followingList}
                savedFeeds={savedFeeds}
                selectedFeedUris={selectedFeedUris}
                                setSelectedFeedUris={setSelectedFeedUris}
                                selectedHashtagsFilter={selectedHashtagsFilter}
                                setSelectedHashtagsFilter={setSelectedHashtagsFilter}
                FEED_FILTER_ALL={FEED_FILTER_ALL}
                hashtagInput={hashtagInput}
                setHashtagInput={setHashtagInput}
                handleAddHashtag={handleAddHashtag}
                followedHashtags={followedHashtags || []}
                handleRemoveHashtag={handleRemoveHashtag}
                archiveSourceFilter={archiveSourceFilter}
                setArchiveSourceFilter={setArchiveSourceFilter}
                vaultSources={vaultSources}
                archiveTotal={archiveTotal}
                position="left"
                onNavigateTab={handleNavigateTab}
            />
            <div className="feed-main">
                {/* Fixed Header with Tabs */}
                <div className="feed-header">
                    <div className="tabs">
                        {(['all', 'following', 'subscribed', 'you', 'groups'] as FeedTab[]).map(tab => (
                            <button
                                key={tab}
                                className={`tab ${activeTab === tab ? 'active' : ''}`}
                                onClick={() => setActiveTab(tab)}
                            >
                                {FEED_TAB_LABELS[tab]}
                            </button>
                        ))}
                    </div>

                    {/* Platform filter — centered in header (no functionality yet, positioning only) */}
                    <div className="feed-platform-filter">
                        <BsGlobe size={13} style={{ opacity: 0.6 }} />
                        <select
                            className="platform-filter-select"
                            value={archiveSourceFilter}
                            onChange={e => setArchiveSourceFilter(e.target.value)}
                        >
                            <option value="">All Platforms</option>
                            {vaultSources.map(s => (
                                <option key={s} value={s}>{platformAbbrev(s)}</option>
                            ))}
                        </select>
                    </div>

                    <div className="feed-lens-toggle">
                        {([
                            ['all', '⊞', 'All'],
                            ['images', '🖼️', 'Images'],
                            ['video', '▶', 'Video'],
                            ['short', '💬', 'Short'],
                            ['long', '📝', 'Long'],
                            ['articles', '📰', 'Articles'],
                        ] as [MediaFilter, string, string][]).map(([key, icon, label]) => (
                            <button
                                key={key}
                                className={`lens-btn ${mediaFilter === key ? 'active' : ''}`}
                                onClick={() => setMediaFilter(key)}
                                title={label}
                            >
                                {icon}
                            </button>
                        ))}
                    </div>
                    {isLoading && <div className="status-pill"><div className="dot online"></div> Syncing</div>}
                </div>

                {/* Mobile collapsible context drawer */}
                <div className="mobile-context-drawer">
                    <button
                        className={`mobile-context-toggle ${contextOpen ? 'open' : ''}`}
                        onClick={() => setContextOpen(v => !v)}
                    >
                        <span className="mobile-context-arrow">{contextOpen ? '▲' : '▼'}</span>
                        <span className="mobile-context-label">{CONTEXT_LABELS[activeTab] || 'Tools'}</span>
                    </button>
                    {contextOpen && (
                        <div className="mobile-context-content">
                            <FeedRightPanel
                                activeTab={activeTab}
                                followingList={followingList}
                                savedFeeds={savedFeeds}
                                selectedFeedUris={selectedFeedUris}
                                setSelectedFeedUris={setSelectedFeedUris}
                                selectedHashtagsFilter={selectedHashtagsFilter}
                                setSelectedHashtagsFilter={setSelectedHashtagsFilter}
                                FEED_FILTER_ALL={FEED_FILTER_ALL}
                                hashtagInput={hashtagInput}
                                setHashtagInput={setHashtagInput}
                                handleAddHashtag={handleAddHashtag}
                                followedHashtags={followedHashtags || []}
                                handleRemoveHashtag={handleRemoveHashtag}
                                archiveSourceFilter={archiveSourceFilter}
                                setArchiveSourceFilter={setArchiveSourceFilter}
                                vaultSources={vaultSources}
                                archiveTotal={archiveTotal}
                                onNavigateTab={handleNavigateTab}
                            />
                        </div>
                    )}
                </div>

                {/* Scrollable Center Area */}
                <div
                    className="feed-scroll-container"
                    ref={scrollContainerRef}
                    onWheel={handleWheel}
                >
                    {/* Pull-to-Refresh Indicator */}
                    <div
                        className={`pull-to-refresh-indicator ${isRefreshing ? 'refreshing' : ''} ${pullDistance > 0 ? 'visible' : ''}`}
                        style={{
                            height: pullDistance,
                            opacity: Math.min(pullDistance / PULL_THRESHOLD, 1),
                        }}
                    >
                        <div className={`ptr-spinner ${pullDistance >= PULL_THRESHOLD || isRefreshing ? 'active' : ''}`}>
                            <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round">
                                <path d="M21 12a9 9 0 1 1-6.22-8.56" />
                            </svg>
                        </div>
                        <span className="ptr-label">
                            {isRefreshing ? 'Refreshing…' : pullDistance >= PULL_THRESHOLD ? 'Release to refresh' : 'Pull to refresh'}
                        </span>
                    </div>

                    {/* Composition Area */}
                    <div className="feed-compose">
                        <ComposeBox
                            onPublish={handlePublishPost}
                            isPosting={isPosting}
                            postStatus={postStatus}
                        />
                    </div>

                    {/* Feed Content */}
                    <div className="feed-list">
                        {feedError && (
                            <div className="card error" style={{ margin: '16px 0', padding: 16 }}>
                                <div style={{ fontWeight: 600, marginBottom: 8 }}>Failed to load feed</div>
                                <div style={{ fontSize: '0.9rem' }}>{feedError}</div>
                                <button className="btn btn-sm" style={{ marginTop: 12 }} onClick={() => fetchFeed(activeTab, true)}>Retry</button>
                            </div>
                        )}

                        {isLoading && feedEntries.length === 0 && (
                            <div className="empty-state">Loading latest posts...</div>
                        )}

                        {!isLoading && feedEntries.length === 0 && !feedError && activeTab !== 'groups' && (
                            <div className="empty-state">
                                <div className="empty-icon">🕸️</div>
                                <div className="empty-title">Nothing here yet</div>
                                <div className="empty-hint">Try switching tags or following more channels.</div>
                            </div>
                        )}

                        {!isLoading && feedEntries.length === 0 && !feedError && activeTab === 'groups' && (
                            <div className="empty-state">
                                <div className="empty-icon">📋</div>
                                <div className="empty-title">No group subscriptions yet</div>
                                <div className="empty-hint">
                                    Join groups and subscribe to topics in the Forums screen to see posts here.
                                </div>
                            </div>
                        )}

                        {filteredEntries.length > 0 && (
                            <VirtualizedFeedList
                                entries={filteredEntries}
                                resolveMediaPath={resolveMediaPath}
                                mediaDir={mediaDir}
                                scrollContainerRef={scrollContainerRef}
                            />
                        )}

                        {isFetchingMore && (
                            <div style={{ textAlign: 'center', padding: 20, opacity: 0.6 }}>Loading more archives...</div>
                        )}

                        {!isLoading && cursor && (
                            <div style={{ textAlign: 'center', padding: '20px 0' }}>
                                <button className="btn btn-ghost" onClick={handleLoadMore}>Load Older Posts</button>
                            </div>
                        )}
                    </div>
                </div>
            </div>

            {/* Friends panel — right side, hugs the feed */}
            <FriendsPanel
                onOpenChat={onOpenChat}
                onViewProfile={onViewProfile}
            />
        </div>
    );
}
