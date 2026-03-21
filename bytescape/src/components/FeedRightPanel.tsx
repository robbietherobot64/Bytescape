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
    /** Render position — affects border/shadow direction */
    position?: 'left' | 'right';
    /** Navigate to a different feed tab (e.g. clicking "+N more" → Following) */
    onNavigateTab?: (tab: FeedTab, opts?: { feedUri?: string }) => void;
}

import { useState } from 'react';

export function FeedRightPanel({
    activeTab,
    followingList,
    savedFeeds,
    selectedFeedUris,
    setSelectedFeedUris,
    FEED_FILTER_ALL,
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
                        {/* Full follows list — always expanded */}
                        <div className="panel-section">
                            <div className="panel-section-label">Active Follows</div>
                        </div>
                        {followingList.map(profile => (
                            <div
                                key={profile.did}
                                className="contact-mini-row"
                                style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '5px 0', cursor: 'pointer' } as React.CSSProperties}
                                onClick={() => {
                                    if (profile.handle) {
                                        openProfile(profile.handle);
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
                    </div>
                ) : activeTab === 'subscribed' ? (
                    <div>
                        {/* Subscribed tab — Custom feeds & tags with toggles */}
                        <div className="panel-section">
                            <div 
                                className="panel-section-label" 
                                onClick={() => setFeedsExpanded(!feedsExpanded)} 
                                style={{cursor: 'pointer', display: 'flex', justifyContent: 'space-between'}}
                            >
                                <span>Feed Sources</span>
                                <span>{feedsExpanded ? '▼' : '▶'}</span>
                            </div>
                            {feedsExpanded && (
                                <div style={{ marginTop: 8 }}>
                                    <div className="hashtag-input-row" style={{ marginBottom: 12 }}>
                                        <input
                                            className="input-field"
                                            value={feedInput}
                                            onChange={e => setFeedInput(e.target.value)}
                                            onKeyDown={e => e.key === 'Enter' && alert("Feed search by name coming soon!")}
                                            placeholder="Enter feed name..."
                                        />
                                        <button className="btn btn-primary hashtag-add-btn" onClick={() => alert("Feed search by name coming soon!")}>+</button>
                                    </div>
                                    <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                                        {savedFeeds.filter(f => !f.uri.includes('following')).length === 0 && (
                                            <div className="panel-hint">No custom feeds found.</div>
                                        )}
                                        {savedFeeds.filter(f => !f.uri.includes('following')).map(feed => {
                                            const isChecked = selectedFeedUris.length === 0 || selectedFeedUris.includes(feed.uri);
                                            return (
                                                <div key={feed.uri} style={{display: 'flex', alignItems: 'center', gap: 8}}>
                                                    <input 
                                                        type="checkbox" 
                                                        checked={isChecked}
                                                        onChange={(e) => {
                                                            if (e.target.checked) {
                                                                if (selectedFeedUris.length > 0) setSelectedFeedUris([...selectedFeedUris, feed.uri]);
                                                            } else {
                                                                if (selectedFeedUris.length === 0) {
                                                                    const all = savedFeeds.filter(fx => !fx.uri.includes('following')).map(fx => fx.uri);
                                                                    setSelectedFeedUris(all.filter(u => u !== feed.uri));
                                                                } else {
                                                                    setSelectedFeedUris(selectedFeedUris.filter(u => u !== feed.uri));
                                                                }
                                                            }
                                                        }}
                                                    />
                                                    <button 
                                                        onClick={() => {
                                                            if (selectedFeedUris.length === 1 && selectedFeedUris[0] === feed.uri) {
                                                                setSelectedFeedUris([]);
                                                            } else {
                                                                setSelectedFeedUris([feed.uri]);
                                                            }
                                                        }} 
                                                        style={{
                                                            background: 'none', 
                                                            border: 'none', 
                                                            color: isChecked ? 'var(--accent-purple)' : 'var(--text-primary)', 
                                                            cursor: 'pointer', 
                                                            textAlign: 'left', 
                                                            flex: 1, 
                                                            overflow: 'hidden', 
                                                            textOverflow: 'ellipsis', 
                                                            whiteSpace: 'nowrap'
                                                        }}
                                                        title="Click to view only this feed"
                                                    >
                                                        {feed.displayName}
                                                    </button>
                                                </div>
                                            );
                                        })}
                                    </div>
                                </div>
                            )}
                        </div>

                        <div className="panel-section">
                            <div 
                                className="panel-section-label" 
                                onClick={() => setTagsExpanded(!tagsExpanded)} 
                                style={{cursor: 'pointer', display: 'flex', justifyContent: 'space-between'}}
                            >
                                <span>Followed Hashtags</span>
                                <span>{tagsExpanded ? '▼' : '▶'}</span>
                            </div>
                            {tagsExpanded && (
                                <div style={{ marginTop: 8 }}>
                                    <div className="hashtag-input-row" style={{ marginBottom: 12 }}>
                                        <input
                                            className="input-field"
                                            value={hashtagInput}
                                            onChange={e => setHashtagInput(e.target.value)}
                                            onKeyDown={e => e.key === 'Enter' && handleAddHashtag()}
                                            placeholder="#topic"
                                        />
                                        <button className="btn btn-primary hashtag-add-btn" onClick={handleAddHashtag}>+</button>
                                    </div>
                                    <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                                        {followedHashtags.length === 0 && (
                                            <div className="panel-hint">No tags followed yet.</div>
                                        )}
                                        {followedHashtags.map(tag => {
                                            const isChecked = selectedHashtagsFilter.length === 0 || selectedHashtagsFilter.includes(tag);
                                            return (
                                                <div key={tag} style={{display: 'flex', alignItems: 'center', gap: 8}}>
                                                    <input 
                                                        type="checkbox" 
                                                        checked={isChecked}
                                                        onChange={(e) => {
                                                            if (e.target.checked) {
                                                                if (selectedHashtagsFilter.length > 0) setSelectedHashtagsFilter([...selectedHashtagsFilter, tag]);
                                                            } else {
                                                                if (selectedHashtagsFilter.length === 0) {
                                                                    setSelectedHashtagsFilter(followedHashtags.filter(t => t !== tag));
                                                                } else {
                                                                    setSelectedHashtagsFilter(selectedHashtagsFilter.filter(t => t !== tag));
                                                                }
                                                            }
                                                        }}
                                                    />
                                                    <button 
                                                        onClick={() => {
                                                            if (selectedHashtagsFilter.length === 1 && selectedHashtagsFilter[0] === tag) {
                                                                setSelectedHashtagsFilter([]);
                                                            } else {
                                                                setSelectedHashtagsFilter([tag]);
                                                            }
                                                        }} 
                                                        style={{
                                                            background: 'none', 
                                                            border: 'none', 
                                                            color: isChecked ? 'var(--accent-purple)' : 'var(--text-primary)', 
                                                            cursor: 'pointer', 
                                                            textAlign: 'left', 
                                                            flex: 1, 
                                                            fontWeight: 'bold'
                                                        }}
                                                        title="Click to view only this hashtag"
                                                    >
                                                        #{tag}
                                                    </button>
                                                    <button 
                                                        onClick={() => handleRemoveHashtag(tag)}
                                                        style={{
                                                            background: 'none', 
                                                            border: 'none', 
                                                            color: 'var(--text-muted)', 
                                                            cursor: 'pointer',
                                                            fontSize: 12
                                                        }}
                                                        title="Unfollow hashtag"
                                                    >
                                                        ✕
                                                    </button>
                                                </div>
                                            );
                                        })}
                                    </div>
                                </div>
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
                        {/* All tab — unified super-menu combining all context panels */}

                        {/* ── Section 1: Active Follows ── */}
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
                                        openProfile(profile.handle);
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
                            <div
                                className="panel-hint panel-hint--clickable"
                                style={{ marginTop: 8, cursor: 'pointer' } as React.CSSProperties}
                                onClick={() => onNavigateTab?.('following')}
                                title="View all follows"
                            >
                                +{followingList.length - 12} more →
                            </div>
                        )}

                        {/* ── Divider ── */}
                        <div className="panel-divider" />

                        {/* ── Section 2: Feeds & Hashtags ── */}
                        <div className="panel-section">
                            <div className="panel-section-label">Feeds & Hashtags</div>
                            <select
                                className="input-field"
                                value={selectedFeedUris[0] || FEED_FILTER_ALL}
                                onChange={e => {
                                    const uri = e.target.value;
                                    setSelectedFeedUris([uri]);
                                    // If user picks a specific feed, navigate to Subscribed tab
                                    if (uri !== FEED_FILTER_ALL && onNavigateTab) {
                                        onNavigateTab('subscribed', { feedUri: uri });
                                    }
                                }}
                            >
                                <option value={FEED_FILTER_ALL}>All Feeds</option>
                                {savedFeeds
                                    .filter(feed => !feed.uri.includes('app.bsky.feed.generator/following'))
                                    .map(feed => (
                                        <option key={feed.uri} value={feed.uri}>
                                            {feed.displayName || 'Custom Feed'}
                                        </option>
                                    ))}
                            </select>
                        </div>

                        <div className="panel-section">
                            <div className="hashtag-input-row">
                                <input
                                    className="input-field"
                                    value={hashtagInput}
                                    onChange={e => setHashtagInput(e.target.value)}
                                    onKeyDown={e => {
                                        if (e.key === 'Enter') {
                                            handleAddHashtag();
                                            // Navigate to Subscribed tab when adding a hashtag
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
