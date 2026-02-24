import { useState, useEffect, useCallback } from 'react';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { AuthStatus, PublishResult, FeedEntry } from '../types';
import { FeedEntryCard } from '../components/FeedEntryCard';
import { ComposeBox } from '../components/ComposeBox';
import { FeedRightPanel } from '../components/FeedRightPanel';
import { filterFacebookDuplicateImagePosts } from '../utils/feedAnalytics';

interface FeedScreenProps {
    auth: AuthStatus | null;
}

import { useUnifiedFeed, FeedTab, LIVE_TABS, FEED_FILTER_ALL, customFeedUris } from '../hooks/useUnifiedFeed';

export function FeedScreen({ auth }: FeedScreenProps) {
    // === UI Control State ===
    const [activeTab, setActiveTab] = useState<FeedTab>('all');
    const [autoRefreshFeeds, setAutoRefreshFeeds] = useState(false);
    const [selectedFeedUri, setSelectedFeedUri] = useState<string>(FEED_FILTER_ALL);
    const [hashtagInput, setHashtagInput] = useState('');
    const [archiveSourceFilter, setArchiveSourceFilter] = useState<string>('');
    const [composeText, setComposeText] = useState('');
    const [postTarget] = useState<'local' | 'bluesky' | 'activitypub' | 'all'>('local');
    const [isPosting, setIsPosting] = useState(false);
    const [postStatus, setPostStatus] = useState<string>('');

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


