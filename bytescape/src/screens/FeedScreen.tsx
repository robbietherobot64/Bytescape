import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { AuthStatus, PublishResult, FeedEntry } from '../types';
import { FeedEntryCard } from '../components/FeedEntryCard';
import { ComposeBox, Attachment } from '../components/ComposeBox';
import { FeedRightPanel } from '../components/FeedRightPanel';
import { filterFacebookDuplicateImagePosts } from '../utils/feedAnalytics';
import { useUnifiedFeed, FeedTab, FEED_FILTER_ALL, customFeedUris } from '../hooks/useUnifiedFeed';

interface FeedScreenProps {
    auth: AuthStatus | null;
}

export function FeedScreen({ auth }: FeedScreenProps) {
    // === UI Control State ===
    const [activeTab, setActiveTab] = useState<FeedTab>('all');
    const [autoRefreshFeeds] = useState(false);
    const [selectedFeedUri, setSelectedFeedUri] = useState<string>(FEED_FILTER_ALL);
    const [hashtagInput, setHashtagInput] = useState('');
    const [archiveSourceFilter, setArchiveSourceFilter] = useState<string>('');
    const [isPosting, setIsPosting] = useState(false);
    const [postStatus, setPostStatus] = useState<string>('');
    const [syndicateTarget, setSyndicateTarget] = useState<FeedEntry | null>(null);

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
        const uris = customFeedUris(savedFeeds);
        if (selectedFeedUri !== FEED_FILTER_ALL && !uris.includes(selectedFeedUri)) {
            setSelectedFeedUri(FEED_FILTER_ALL);
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

    const handleSyndicateClick = useCallback((entry: FeedEntry) => {
        setSyndicateTarget(entry);
        setPostStatus('');
    }, []);

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
                if (syndicateTarget) setSyndicateTarget(null);
                fetchFeed(activeTab, false);
            }
        } catch (e: any) {
            setPostStatus(typeof e === 'string' ? e : 'Failed to publish post');
        } finally {
            setIsPosting(false);
        }
    };

    const resolveMediaPath = (src: string) => {
        if (src.startsWith('http')) return src;
        return src;
    };

    const mediaDir = '';

    return (
        <div className="feed-layout">
            <div className="feed-main">
                {/* Fixed Header with Tabs */}
                <div className="feed-header">
                    <div className="tabs">
                        {(['all', 'following', 'archives'] as FeedTab[]).map(tab => (
                            <button
                                key={tab}
                                className={`tab ${activeTab === tab ? 'active' : ''}`}
                                onClick={() => setActiveTab(tab)}
                            >
                                {tab.charAt(0).toUpperCase() + tab.slice(1)}
                            </button>
                        ))}
                    </div>
                    {isLoading && <div className="status-pill"><div className="dot online"></div> Syncing</div>}
                </div>

                {/* Scrollable Center Area */}
                <div className="feed-scroll-container">
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
                                key={entry.id}
                                entry={entry}
                                resolveMediaPath={resolveMediaPath}
                                mediaDir={mediaDir}
                                onSyndicate={handleSyndicateClick}
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

            {/* Syndication Modal */}
            {syndicateTarget && (
                <div className="modal-overlay" onClick={() => setSyndicateTarget(null)}>
                    <div className="modal" style={{ maxWidth: 650, padding: 0, overflow: 'hidden' }} onClick={e => e.stopPropagation()}>
                        <div style={{ padding: '20px 24px', borderBottom: '1px solid var(--border)', display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                            <h3 style={{ margin: 0 }}>Syndicate to Bluesky</h3>
                            <button className="btn btn-ghost btn-sm" onClick={() => setSyndicateTarget(null)} style={{ padding: '0 8px' }}>✕</button>
                        </div>
                        <div style={{ padding: 24 }}>
                            <ComposeBox
                                onPublish={handlePublishPost}
                                isPosting={isPosting}
                                postStatus={postStatus}
                                initialText={syndicateTarget.content}
                                initialAttachments={syndicateTarget.media_urls.map((url, i) => ({
                                    blob_hash: url.startsWith('blob:') ? url : undefined,
                                    path: !url.startsWith('blob:') ? url : undefined,
                                    preview: url,
                                    mime_type: syndicateTarget.media_mimes ? syndicateTarget.media_mimes[i] : undefined
                                }))}
                            />
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
}
