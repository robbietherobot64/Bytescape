import { useState, useEffect, useCallback } from 'react';
import { ProfileView, GeneratorView } from '../types';
import { Win98Box } from './Win98Box';
import { openProfile } from '../utils/openInApp';
import { FeedTab } from '../hooks/useUnifiedFeed';

interface FeedRightPanelProps {
    activeTab: string;
    followingList: ProfileView[];
    savedFeeds: GeneratorView[];
    selectedFeedUris: string[];
    setSelectedFeedUris: (uris: string[]) => void;
    selectedHashtagsFilter: string[];
    setSelectedHashtagsFilter: (tags: string[]) => void;
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
    position?: 'left' | 'right';
    onNavigateTab?: (tab: FeedTab, opts?: { feedUri?: string }) => void;
}

const HIDDEN_FEEDS_KEY = 'bytescape:hidden_feed_uris';

function loadHiddenFeeds(): Set<string> {
    try {
        const raw = localStorage.getItem(HIDDEN_FEEDS_KEY);
        return new Set<string>(raw ? (JSON.parse(raw) as string[]) : []);
    } catch {
        return new Set<string>();
    }
}

const rowStyle: React.CSSProperties = {
    display: 'flex', alignItems: 'center', gap: 6, padding: '3px 0',
};
const itemBtnBase: React.CSSProperties = {
    background: 'none', border: 'none', cursor: 'pointer',
    textAlign: 'left', flex: 1, overflow: 'hidden',
    textOverflow: 'ellipsis', whiteSpace: 'nowrap',
    padding: '2px 0', fontSize: '0.82rem',
};
const removeBtnStyle: React.CSSProperties = {
    background: 'none', border: 'none', cursor: 'pointer',
    color: 'var(--text-muted)', fontSize: 11, padding: '0 2px', lineHeight: 1,
    flexShrink: 0,
};
const toggleHeaderStyle: React.CSSProperties = {
    cursor: 'pointer', display: 'flex', justifyContent: 'space-between',
    alignItems: 'center', userSelect: 'none',
};

export function FeedRightPanel({
    activeTab,
    followingList,
    savedFeeds,
    selectedFeedUris,
    setSelectedFeedUris,
    FEED_FILTER_ALL: _FEED_FILTER_ALL,
    hashtagInput,
    setHashtagInput,
    handleAddHashtag,
    followedHashtags,
    handleRemoveHashtag,
    selectedHashtagsFilter,
    setSelectedHashtagsFilter,
    archiveSourceFilter,
    setArchiveSourceFilter,
    vaultSources,
    archiveTotal,
    position = 'right',
    onNavigateTab,
}: FeedRightPanelProps) {
    const [feedsExpanded, setFeedsExpanded] = useState(true);
    const [tagsExpanded, setTagsExpanded] = useState(true);
    const [feedInput, setFeedInput] = useState('');
    const [hiddenFeedUris, setHiddenFeedUris] = useState<Set<string>>(loadHiddenFeeds);

    useEffect(() => {
        localStorage.setItem(HIDDEN_FEEDS_KEY, JSON.stringify([...hiddenFeedUris]));
        const cleaned = selectedFeedUris.filter(uri => !hiddenFeedUris.has(uri));
        if (cleaned.length !== selectedFeedUris.length) setSelectedFeedUris(cleaned);
    // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [hiddenFeedUris]);

    const visibleFeeds = savedFeeds.filter(
        f => !f.uri.includes('/following') && !hiddenFeedUris.has(f.uri)
    );

    const isFeedChecked = useCallback(
        (uri: string) => selectedFeedUris.length === 0 || selectedFeedUris.includes(uri),
        [selectedFeedUris]
    );

    const toggleFeed = (uri: string, on: boolean) => {
        if (on) {
            if (selectedFeedUris.length > 0) setSelectedFeedUris([...selectedFeedUris, uri]);
        } else {
            const base = selectedFeedUris.length === 0 ? visibleFeeds.map(f => f.uri) : selectedFeedUris;
            setSelectedFeedUris(base.filter(u => u !== uri));
        }
    };

    const soloFeed = (uri: string) => {
        setSelectedFeedUris(
            selectedFeedUris.length === 1 && selectedFeedUris[0] === uri ? [] : [uri]
        );
    };

    const hideFeed = (uri: string) => {
        const next = new Set(hiddenFeedUris);
        next.add(uri);
        setHiddenFeedUris(next);
    };

    const isTagChecked = useCallback(
        (tag: string) => selectedHashtagsFilter.length === 0 || selectedHashtagsFilter.includes(tag),
        [selectedHashtagsFilter]
    );

    const toggleTag = (tag: string, on: boolean) => {
        if (on) {
            if (selectedHashtagsFilter.length > 0) setSelectedHashtagsFilter([...selectedHashtagsFilter, tag]);
        } else {
            const base = selectedHashtagsFilter.length === 0 ? followedHashtags : selectedHashtagsFilter;
            setSelectedHashtagsFilter(base.filter(t => t !== tag));
        }
    };

    const soloTag = (tag: string) => {
        setSelectedHashtagsFilter(
            selectedHashtagsFilter.length === 1 && selectedHashtagsFilter[0] === tag ? [] : [tag]
        );
    };

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

                {activeTab === 'following' && (
                    <div>
                        <div className="panel-section">
                            <div className="panel-section-label">Active Follows</div>
                        </div>
                        {followingList.map(profile => (
                            <div
                                key={profile.did}
                                className="contact-mini-row"
                                style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '5px 0', cursor: 'pointer' } as React.CSSProperties}
                                onClick={() => { if (profile.handle) openProfile(profile.handle); }}
                                title={`@${profile.handle}`}
                            >
                                <div style={{ width: 24, height: 24, borderRadius: '50%', background: 'var(--bg-toolbar)', border: '1px solid var(--border-color)', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 11, fontWeight: 700, flexShrink: 0, overflow: 'hidden' } as React.CSSProperties}>
                                    {profile.avatar ? (
                                        <img src={profile.avatar} alt="" style={{ width: '100%', height: '100%', objectFit: 'cover' } as React.CSSProperties} />
                                    ) : profile.handle?.[0]?.toUpperCase()}
                                </div>
                                <span style={{ fontSize: '0.8rem', color: 'var(--link-blue)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' } as React.CSSProperties}>
                                    {profile.displayName || profile.handle}
                                </span>
                            </div>
                        ))}
                        {followingList.length === 0 && <div className="empty-state panel-empty">No follows found.</div>}
                    </div>
                )}

                {activeTab === 'subscribed' && (
                    <div>
                        {/* Feed Sources Section */}
                        <div className="panel-section">
                            <div className="panel-section-label" onClick={() => setFeedsExpanded(v => !v)} style={toggleHeaderStyle}>
                                <span>Feed Sources</span>
                                <span style={{ fontSize: 10, opacity: 0.55 } as React.CSSProperties}>{feedsExpanded ? '▼' : '▶'}</span>
                            </div>
                            {feedsExpanded && (
                                <div style={{ marginTop: 8 } as React.CSSProperties}>
                                    <div className="hashtag-input-row" style={{ marginBottom: 10 } as React.CSSProperties}>
                                        <input
                                            className="input-field"
                                            value={feedInput}
                                            onChange={e => setFeedInput(e.target.value)}
                                            onKeyDown={e => { if (e.key === 'Enter') setFeedInput(''); }}
                                            placeholder="Feed name (search coming soon)"
                                        />
                                        <button
                                            className="btn btn-primary hashtag-add-btn"
                                            onClick={() => setFeedInput('')}
                                            title="Feed search coming soon"
                                        >+</button>
                                    </div>
                                    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 } as React.CSSProperties}>
                                        {visibleFeeds.length === 0 && (
                                            <div className="panel-hint">No custom feeds found.</div>
                                        )}
                                        {visibleFeeds.map(feed => {
                                            const checked = isFeedChecked(feed.uri);
                                            return (
                                                <div key={feed.uri} style={rowStyle}>
                                                    <input
                                                        type="checkbox"
                                                        checked={checked}
                                                        onChange={e => toggleFeed(feed.uri, e.target.checked)}
                                                    />
                                                    <button
                                                        style={{ ...itemBtnBase, color: checked ? 'var(--accent-purple)' : 'var(--text-primary)', fontWeight: checked ? 600 : 400 } as React.CSSProperties}
                                                        onClick={() => soloFeed(feed.uri)}
                                                        title="Click to filter only this feed"
                                                    >
                                                        {feed.displayName || 'Custom Feed'}
                                                    </button>
                                                    <button
                                                        style={removeBtnStyle}
                                                        onClick={() => hideFeed(feed.uri)}
                                                        title="Hide feed from sidebar"
                                                    >✕</button>
                                                </div>
                                            );
                                        })}
                                    </div>
                                </div>
                            )}
                        </div>

                        {/* Hashtags Section */}
                        <div className="panel-section">
                            <div className="panel-section-label" onClick={() => setTagsExpanded(v => !v)} style={toggleHeaderStyle}>
                                <span>Hashtags</span>
                                <span style={{ fontSize: 10, opacity: 0.55 } as React.CSSProperties}>{tagsExpanded ? '▼' : '▶'}</span>
                            </div>
                            {tagsExpanded && (
                                <div style={{ marginTop: 8 } as React.CSSProperties}>
                                    <div className="hashtag-input-row" style={{ marginBottom: 10 } as React.CSSProperties}>
                                        <input
                                            className="input-field"
                                            value={hashtagInput}
                                            onChange={e => setHashtagInput(e.target.value)}
                                            onKeyDown={e => { if (e.key === 'Enter') handleAddHashtag(); }}
                                            placeholder="#topic"
                                        />
                                        <button className="btn btn-primary hashtag-add-btn" onClick={handleAddHashtag}>+</button>
                                    </div>
                                    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 } as React.CSSProperties}>
                                        {followedHashtags.length === 0 && (
                                            <div className="panel-hint">No tags followed yet.</div>
                                        )}
                                        {followedHashtags.map(tag => {
                                            const checked = isTagChecked(tag);
                                            return (
                                                <div key={tag} style={rowStyle}>
                                                    <input
                                                        type="checkbox"
                                                        checked={checked}
                                                        onChange={e => toggleTag(tag, e.target.checked)}
                                                    />
                                                    <button
                                                        style={{ ...itemBtnBase, color: checked ? 'var(--accent-purple)' : 'var(--text-primary)', fontWeight: 700 } as React.CSSProperties}
                                                        onClick={() => soloTag(tag)}
                                                        title="Click to filter only this hashtag"
                                                    >
                                                        #{tag}
                                                    </button>
                                                    <button
                                                        style={removeBtnStyle}
                                                        onClick={() => handleRemoveHashtag(tag)}
                                                        title="Unfollow hashtag"
                                                    >✕</button>
                                                </div>
                                            );
                                        })}
                                    </div>
                                </div>
                            )}
                        </div>
                    </div>
                )}

                {activeTab === 'you' && (
                    <div>
                        <div className="panel-section">
                            <div className="panel-section-label">Filter by source</div>
                            <select
                                className="input-field"
                                value={archiveSourceFilter}
                                onChange={e => setArchiveSourceFilter(e.target.value)}
                            >
                                <option value="">All sources</option>
                                {vaultSources.map(s => <option key={s} value={s}>{s}</option>)}
                            </select>
                        </div>
                        <Win98Box variant="sunken" className="panel-stat-box">
                            <div className="panel-section-label">Your Posts</div>
                            <div className="panel-stat-value">{archiveTotal} <span className="panel-stat-unit">items</span></div>
                        </Win98Box>
                    </div>
                )}

                {activeTab !== 'following' && activeTab !== 'subscribed' && activeTab !== 'you' && (
                    <div>
                        <div className="panel-section">
                            <div className="panel-section-label">Active Follows</div>
                        </div>
                        {followingList.slice(0, 12).map(profile => (
                            <div
                                key={profile.did}
                                className="contact-mini-row"
                                style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '5px 0', cursor: 'pointer' } as React.CSSProperties}
                                onClick={() => { if (profile.handle) openProfile(profile.handle); }}
                                title={`@${profile.handle}`}
                            >
                                <div style={{ width: 24, height: 24, borderRadius: '50%', background: 'var(--bg-toolbar)', border: '1px solid var(--border-color)', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 11, fontWeight: 700, flexShrink: 0, overflow: 'hidden' } as React.CSSProperties}>
                                    {profile.avatar ? (
                                        <img src={profile.avatar} alt="" style={{ width: '100%', height: '100%', objectFit: 'cover' } as React.CSSProperties} />
                                    ) : profile.handle?.[0]?.toUpperCase()}
                                </div>
                                <span style={{ fontSize: '0.8rem', color: 'var(--link-blue)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' } as React.CSSProperties}>
                                    {profile.displayName || profile.handle}
                                </span>
                            </div>
                        ))}
                        {followingList.length === 0 && <div className="empty-state panel-empty">No follows found.</div>}
                        {followingList.length > 12 && (
                            <div
                                className="panel-hint panel-hint--clickable"
                                style={{ marginTop: 8, cursor: 'pointer' } as React.CSSProperties}
                                onClick={() => onNavigateTab?.('following')}
                            >
                                +{followingList.length - 12} more →
                            </div>
                        )}

                        <div className="panel-divider" />

                        <div className="panel-section">
                            <div className="panel-section-label">Feeds & Hashtags</div>
                            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4, marginBottom: 8, marginTop: 4 } as React.CSSProperties}>
                                {visibleFeeds.slice(0, 6).map(feed => (
                                    <button
                                        key={feed.uri}
                                        className={`hashtag-pill ${selectedFeedUris.includes(feed.uri) ? 'hashtag-pill--active' : ''}`}
                                        onClick={() => {
                                            soloFeed(feed.uri);
                                            onNavigateTab?.('subscribed', { feedUri: feed.uri });
                                        }}
                                        title={feed.displayName}
                                    >
                                        {feed.displayName || 'Feed'}
                                    </button>
                                ))}
                            </div>
                            <div className="hashtag-input-row">
                                <input
                                    className="input-field"
                                    value={hashtagInput}
                                    onChange={e => setHashtagInput(e.target.value)}
                                    onKeyDown={e => {
                                        if (e.key === 'Enter') {
                                            handleAddHashtag();
                                            onNavigateTab?.('subscribed');
                                        }
                                    }}
                                    placeholder="#topic"
                                />
                                <button className="btn btn-primary hashtag-add-btn" onClick={() => {
                                    handleAddHashtag();
                                    onNavigateTab?.('subscribed');
                                }}>+</button>
                            </div>
                            <div className="hashtag-pills" style={{ marginTop: 6 } as React.CSSProperties}>
                                {followedHashtags.map(tag => (
                                    <div key={tag} className="hashtag-pill">
                                        <span>#{tag}</span>
                                        <button className="hashtag-remove-btn" onClick={() => handleRemoveHashtag(tag)}>✕</button>
                                    </div>
                                ))}
                            </div>
                            {followedHashtags.length === 0 && <div className="panel-hint">No tags followed yet.</div>}
                        </div>
                    </div>
                )}
            </div>
        </div>
    );
}
