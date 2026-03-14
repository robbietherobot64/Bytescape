import { ProfileView, GeneratorView } from '../types';
import { Win98Box } from './Win98Box';

interface FeedRightPanelProps {
    activeTab: string;
    followingList: ProfileView[];
    savedFeeds: GeneratorView[];
    selectedFeedUri: string;
    setSelectedFeedUri: (uri: string) => void;
    FEED_FILTER_ALL: string;
    hashtagInput: string;
    setHashtagInput: (input: string) => void;
    handleAddHashtag: () => void;
    followedHashtags: string[];
    handleRemoveHashtag: (tag: string) => void;
    archiveSourceFilter: string;
    setArchiveSourceFilter: (source: string) => void;
    vaultSources: string[];
    archiveTotal: number;
    /** Render position — affects border/shadow direction */
    position?: 'left' | 'right';
}

export function FeedRightPanel({
    activeTab,
    followingList,
    savedFeeds,
    selectedFeedUri,
    setSelectedFeedUri,
    FEED_FILTER_ALL,
    hashtagInput,
    setHashtagInput,
    handleAddHashtag,
    followedHashtags,
    handleRemoveHashtag,
    archiveSourceFilter,
    setArchiveSourceFilter,
    vaultSources,
    archiveTotal,
    position = 'right',
}: FeedRightPanelProps) {

    const panelTitle =
        activeTab === 'following' ? 'My Contacts' :
        activeTab === 'subscribed' ? 'Subscriptions' :
        activeTab === 'you' ? 'Your Posts' :
        'Discovery Tools';

    return (
        <div className={`right-panel ${position === 'left' ? 'right-panel--left' : ''}`}>
            <div className="right-panel-header">
                <h3 className="right-panel-title">{panelTitle}</h3>
            </div>

            <div className="right-panel-body sidebar-list">
                {activeTab === 'following' ? (
                    <div>
                        {followingList.map(profile => (
                            <Win98Box key={profile.did} variant="raised" className="contact-item-card">
                                <div className="contact-avatar">
                                    {profile.handle?.[0]?.toUpperCase()}
                                </div>
                                <div className="contact-info">
                                    <div className="contact-name">{profile.displayName || profile.handle}</div>
                                    <div className="contact-handle">{profile.handle}</div>
                                </div>
                            </Win98Box>
                        ))}
                        {followingList.length === 0 && (
                            <div className="empty-state panel-empty">No contacts found.</div>
                        )}
                    </div>
                ) : activeTab === 'subscribed' ? (
                    <div>
                        {/* Feed Source Selector */}
                        <div className="panel-section">
                            <div className="panel-section-label">Curated Feed Source</div>
                            <select
                                className="input-field"
                                value={selectedFeedUri}
                                onChange={e => setSelectedFeedUri(e.target.value)}
                            >
                                <option value={FEED_FILTER_ALL}>All Subscribed Feeds</option>
                                {savedFeeds
                                    .filter(feed => !feed.uri.includes('app.bsky.feed.generator/following'))
                                    .map(feed => (
                                        <option key={feed.uri} value={feed.uri}>
                                            {feed.displayName || 'Custom Feed'}
                                        </option>
                                    ))}
                            </select>
                        </div>

                        {/* Hashtag Management */}
                        <div className="panel-section">
                            <div className="panel-section-label">Subscribe to Tags</div>
                            <div className="hashtag-input-row">
                                <input
                                    className="input-field"
                                    value={hashtagInput}
                                    onChange={e => setHashtagInput(e.target.value)}
                                    onKeyDown={e => e.key === 'Enter' && handleAddHashtag()}
                                    placeholder="#discovery"
                                />
                                <button className="btn btn-primary hashtag-add-btn" onClick={handleAddHashtag}>+</button>
                            </div>

                            <div className="hashtag-pills">
                                {followedHashtags.map(tag => (
                                    <div key={tag} className="hashtag-pill">
                                        <span>#{tag}</span>
                                        <button
                                            className="hashtag-remove-btn"
                                            onClick={() => handleRemoveHashtag(tag)}
                                        >
                                            ✕
                                        </button>
                                    </div>
                                ))}
                            </div>
                            {followedHashtags.length === 0 && (
                                <div className="panel-hint">No tags followed yet.</div>
                            )}
                        </div>

                        {/* RSS placeholder */}
                        <div className="panel-section">
                            <div className="panel-section-label">RSS Feeds</div>
                            <div className="panel-hint">Coming soon — add RSS feed URLs to subscribe.</div>
                        </div>
                    </div>
                ) : activeTab === 'you' ? (
                    <div>
                        {/* Source filter for your own posts */}
                        <div className="panel-section">
                            <div className="panel-section-label">Filter by source</div>
                            <select
                                className="input-field"
                                value={archiveSourceFilter}
                                onChange={e => setArchiveSourceFilter(e.target.value)}
                            >
                                <option value="">All sources</option>
                                {vaultSources.map(s => (
                                    <option key={s} value={s}>{s}</option>
                                ))}
                            </select>
                        </div>

                        <Win98Box variant="sunken" className="panel-stat-box">
                            <div className="panel-section-label">Your Posts</div>
                            <div className="panel-stat-value">{archiveTotal} <span className="panel-stat-unit">items</span></div>
                        </Win98Box>
                    </div>
                ) : (
                    <div>
                        {/* All tab — combined discovery tools */}
                        <div className="panel-section">
                            <div className="panel-section-label">Curated Feed Source</div>
                            <select
                                className="input-field"
                                value={selectedFeedUri}
                                onChange={e => setSelectedFeedUri(e.target.value)}
                            >
                                <option value={FEED_FILTER_ALL}>Global Timeline</option>
                                {savedFeeds
                                    .filter(feed => !feed.uri.includes('app.bsky.feed.generator/following'))
                                    .map(feed => (
                                        <option key={feed.uri} value={feed.uri}>
                                            {feed.displayName || 'Custom Feed'}
                                        </option>
                                    ))}
                            </select>
                        </div>

                        {/* Hashtag Management */}
                        <div className="panel-section">
                            <div className="panel-section-label">Subscribe to Tags</div>
                            <div className="hashtag-input-row">
                                <input
                                    className="input-field"
                                    value={hashtagInput}
                                    onChange={e => setHashtagInput(e.target.value)}
                                    onKeyDown={e => e.key === 'Enter' && handleAddHashtag()}
                                    placeholder="#discovery"
                                />
                                <button className="btn btn-primary hashtag-add-btn" onClick={handleAddHashtag}>+</button>
                            </div>

                            <div className="hashtag-pills">
                                {followedHashtags.map(tag => (
                                    <div key={tag} className="hashtag-pill">
                                        <span>#{tag}</span>
                                        <button
                                            className="hashtag-remove-btn"
                                            onClick={() => handleRemoveHashtag(tag)}
                                        >
                                            ✕
                                        </button>
                                    </div>
                                ))}
                            </div>
                            {followedHashtags.length === 0 && (
                                <div className="panel-hint">No tags followed yet.</div>
                            )}
                        </div>
                    </div>
                )}
            </div>
        </div>
    );
}
