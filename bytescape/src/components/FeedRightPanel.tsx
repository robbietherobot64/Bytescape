import { ProfileView, GeneratorView } from '../types';
import { Win98Box } from './Win98Box';
import { openUrl } from '@tauri-apps/plugin-opener';

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
        activeTab === 'following' ? 'Following' :
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
                        {/* Recently active follows — compact summary */}
                        <div className="panel-section">
                            <div className="panel-section-label">Active Follows</div>
                        </div>
                        {followingList.slice(0, 12).map(profile => (
                            <div
                                key={profile.did}
                                className="contact-mini-row"
                                style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '5px 0', cursor: 'pointer' } as React.CSSProperties}
                                onClick={() => {
                                    if (profile.handle) {
                                        openUrl(`https://bsky.app/profile/${profile.handle}`).catch(console.error);
                                    }
                                }}
                                title={`@${profile.handle}`}
                            >
                                <div style={{ width: 24, height: 24, borderRadius: '50%', background: 'var(--bg-toolbar)', border: '1px solid var(--border-color)', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 11, fontWeight: 700, flexShrink: 0, overflow: 'hidden' } as React.CSSProperties}>
                                    {profile.avatar ? (
                                        <img src={profile.avatar} alt="" style={{ width: '100%', height: '100%', objectFit: 'cover' } as React.CSSProperties} />
                                    ) : (
                                        profile.handle?.[0]?.toUpperCase()
                                    )}
                                </div>
                                <span style={{ fontSize: '0.8rem', color: 'var(--link-blue)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' } as React.CSSProperties}>
                                    {profile.displayName || profile.handle}
                                </span>
                            </div>
                        ))}
                        {followingList.length === 0 && (
                            <div className="empty-state panel-empty">No follows found.</div>
                        )}
                        {followingList.length > 12 && (
                            <div className="panel-hint" style={{ marginTop: 8 } as React.CSSProperties}>
                                +{followingList.length - 12} more
                            </div>
                        )}
                    </div>
                ) : activeTab === 'subscribed' ? (
                    <div>
                        {/* Subscribed tab — curated feed sources + hashtags */}
                        <div className="panel-section">
                            <div className="panel-section-label">Feed Sources</div>
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
                            {savedFeeds.filter(f => !f.uri.includes('app.bsky.feed.generator/following')).length === 0 && (
                                <div className="panel-hint" style={{ marginTop: 8 } as React.CSSProperties}>
                                    No custom feeds found. Subscribe to feeds on Bluesky to see them here.
                                </div>
                            )}
                        </div>

                        {/* Hashtag management */}
                        <div className="panel-section">
                            <div className="panel-section-label">Followed Tags</div>
                            <div className="hashtag-input-row">
                                <input
                                    className="input-field"
                                    value={hashtagInput}
                                    onChange={e => setHashtagInput(e.target.value)}
                                    onKeyDown={e => e.key === 'Enter' && handleAddHashtag()}
                                    placeholder="#topic"
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
