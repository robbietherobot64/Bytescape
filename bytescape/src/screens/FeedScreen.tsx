import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { AuthStatus, PublishResult } from '../types';
import { FeedEntryCard } from '../components/FeedEntryCard';
import { ComposeBox, Attachment } from '../components/ComposeBox';
import { FeedRightPanel } from '../components/FeedRightPanel';
import { filterFacebookDuplicateImagePosts } from '../utils/feedAnalytics';
import { useUnifiedFeed, FeedTab, FEED_FILTER_ALL, FEED_TAB_LABELS, customFeedUris } from '../hooks/useUnifiedFeed';
import { appDataDir } from '@tauri-apps/api/path';

const CONTEXT_LABELS: Record<string, string> = {
    all: 'Discovery Tools',
    following: 'My Contacts',
    subscribed: 'Subscriptions',
    you: 'Your Posts',
};

interface FeedScreenProps {
    auth: AuthStatus | null;
    activeTab: FeedTab;
    setActiveTab: (tab: FeedTab) => void;
}

export function FeedScreen({ auth, activeTab, setActiveTab }: FeedScreenProps) {
    // === UI Control State ===
    const [contextOpen, setContextOpen] = useState(false);
    const autoRefreshFeeds = true;
    const [selectedFeedUri, setSelectedFeedUri] = useState<string>(FEED_FILTER_ALL);
    const [hashtagInput, setHashtagInput] = useState('');
    const [archiveSourceFilter, setArchiveSourceFilter] = useState<string>('');
    const [isPosting, setIsPosting] = useState(false);
    const [postStatus, setPostStatus] = useState<string>('');
    const [mediaDir, setMediaDir] = useState<string>('');

    // === Pull-to-Refresh State ===
    const [pullDistance, setPullDistance] = useState(0);
    const [isPulling, setIsPulling] = useState(false);
    const [isRefreshing, setIsRefreshing] = useState(false);
    const pullStartY = useRef(0);
    const scrollContainerRef = useRef<HTMLDivElement>(null);
    const PULL_THRESHOLD = 80;
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
        selectedFeedUri,
        archiveSourceFilter,
        autoRefreshFeeds
    });

    useEffect(() => {
        appDataDir().then(dir => setMediaDir(dir)).catch(console.error);
    }, []);

    useEffect(() => {
        // Only run reset if we actually have feeds loaded and current selection is invalid
        if (savedFeeds.length > 0 && selectedFeedUri !== FEED_FILTER_ALL) {
            const uris = customFeedUris(savedFeeds);
            if (!uris.includes(selectedFeedUri)) {
                console.log('FeedScreen: Selected feed no longer available, resetting to Global Timeline');
                setSelectedFeedUri(FEED_FILTER_ALL);
            }
        }
    }, [savedFeeds, selectedFeedUri]);

    const handleAddHashtag = async () => {
        const tag = hashtagInput.trim().replace(/^#/, '');
        if (!tag) return;
        try {
            const current = followedHashtags || [];
            if (!current.includes(tag)) {
                const refreshed = await invoke<string[]>('add_followed_hashtag', { hashtag: tag });
                setFollowedHashtags(refreshed);
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
        } catch (e) {
            console.error('Failed to remove hashtag:', e);
        }
    };



    const handlePublishPost = async (text: string, target: 'local' | 'bluesky' | 'activitypub' | 'all', attachments: Attachment[]) => {
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

    // === Pull-to-Refresh Handlers ===
    const handlePointerDown = useCallback((e: React.PointerEvent) => {
        const container = scrollContainerRef.current;
        if (!container || container.scrollTop > 0 || isRefreshing) return;
        pullStartY.current = e.clientY;
        setIsPulling(true);
    }, [isRefreshing]);

    const handlePointerMove = useCallback((e: React.PointerEvent) => {
        if (!isPulling || isRefreshing) return;
        const container = scrollContainerRef.current;
        if (!container || container.scrollTop > 0) {
            setIsPulling(false);
            setPullDistance(0);
            return;
        }
        const delta = e.clientY - pullStartY.current;
        if (delta > 0) {
            // Rubber-band: diminishing returns as you pull further
            const dampened = Math.min(delta * 0.45, 140);
            setPullDistance(dampened);
            e.preventDefault();
        }
    }, [isPulling, isRefreshing]);

    const handlePointerUp = useCallback(async () => {
        if (!isPulling) return;
        setIsPulling(false);
        if (pullDistance >= PULL_THRESHOLD && !isRefreshing) {
            setIsRefreshing(true);
            setPullDistance(PULL_THRESHOLD); // Hold at threshold while refreshing
            try {
                await fetchFeed(activeTab, false);
            } finally {
                setIsRefreshing(false);
                setPullDistance(0);
            }
        } else {
            setPullDistance(0);
        }
    }, [isPulling, pullDistance, isRefreshing, fetchFeed, activeTab]);

    return (
        <div className="feed-layout">
            <div className="feed-main">
                {/* Fixed Header with Tabs */}
                <div className="feed-header">
                    <div className="tabs">
                        {(['all', 'following', 'subscribed', 'you'] as FeedTab[]).map(tab => (
                            <button
                                key={tab}
                                className={`tab ${activeTab === tab ? 'active' : ''}`}
                                onClick={() => setActiveTab(tab)}
                            >
                                {FEED_TAB_LABELS[tab]}
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
                                selectedFeedUri={selectedFeedUri}
                                setSelectedFeedUri={setSelectedFeedUri}
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
                            />
                        </div>
                    )}
                </div>

                {/* Scrollable Center Area */}
                <div
                    className="feed-scroll-container"
                    ref={scrollContainerRef}
                    onPointerDown={handlePointerDown}
                    onPointerMove={handlePointerMove}
                    onPointerUp={handlePointerUp}
                    onPointerLeave={handlePointerUp}
                    style={{ touchAction: pullDistance > 0 ? 'none' : undefined }}
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

                        {!isLoading && feedEntries.length === 0 && !feedError && (
                            <div className="empty-state">
                                <div className="empty-icon">🕸️</div>
                                <div className="empty-title">Nothing here yet</div>
                                <div className="empty-hint">Try switching tags or following more channels.</div>
                            </div>
                        )}

                        {filterFacebookDuplicateImagePosts(feedEntries).map(entry => (
                            <FeedEntryCard
                                key={entry.reposted_by ? `repost:${entry.reposted_by}:${entry.id}` : entry.id}
                                entry={entry}
                                resolveMediaPath={resolveMediaPath}
                                mediaDir={mediaDir}
                            />
                        ))}

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

            {/* Right Panel stays fixed on the right */}
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
                followedHashtags={followedHashtags || []}
                handleRemoveHashtag={handleRemoveHashtag}
                archiveSourceFilter={archiveSourceFilter}
                setArchiveSourceFilter={setArchiveSourceFilter}
                vaultSources={vaultSources}
                archiveTotal={archiveTotal}
            />

        </div>
    );
}
