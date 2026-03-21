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

/* ── Style tokens ─────────────────────────────────────────────────────── */
const sectionHeaderStyle: React.CSSProperties = {
    display: 'flex', alignItems: 'center', gap: 6,
    padding: '6px 4px', cursor: 'pointer', userSelect: 'none',
    borderBottom: '1px solid var(--border-color)',
    marginBottom: 6,
};
const caretStyle: React.CSSProperties = {
    marginLeft: 'auto', fontSize: 10, opacity: 0.5, flexShrink: 0,
};
const chipContainerStyle: React.CSSProperties = {
    display: 'flex', flexWrap: 'wrap', gap: 5, padding: '4px 0 8px',
};
const chipBase: React.CSSProperties = {
    padding: '4px 10px', borderRadius: 4, border: '1px solid var(--border-color)',
    cursor: 'pointer', fontSize: '0.82rem', fontWeight: 500,
    transition: 'background 0.12s, color 0.12s, border-color 0.12s',
    whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis',
    maxWidth: '100%',
};
const chipActive: React.CSSProperties = {
    ...chipBase,
    background: 'var(--accent-purple)', color: '#fff',
    borderColor: 'var(--accent-purple)',
};
const chipInactive: React.CSSProperties = {
    ...chipBase,
    background: 'var(--bg-tertiary, #1a1a2e)', color: 'var(--text-muted)',
    borderColor: 'var(--border-color)',
};
const addRowStyle: React.CSSProperties = {
    display: 'flex', gap: 4, padding: '4px 0 2px',
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
    /* ── Subscribed tab state ────────────────────────────────────────── */
    const [feedsExpanded, setFeedsExpanded] = useState(true);
    const [tagsExpanded, setTagsExpanded] = useState(true);
    const [feedsEnabled, setFeedsEnabled] = useState(true);
    const [tagsEnabled, setTagsEnabled] = useState(true);

    const visibleFeeds = savedFeeds.filter(f => !f.uri.includes('/following'));

    /* When the master checkbox is toggled, update the filter arrays.
       Enabled + nothing selected = "show all".  Disabled = show none. */
    useEffect(() => {
        if (!feedsEnabled) {
            setSelectedFeedUris(['__disabled__']);
        } else if (selectedFeedUris.length === 1 && selectedFeedUris[0] === '__disabled__') {
            setSelectedFeedUris([]);
        }
    // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [feedsEnabled]);

    useEffect(() => {
        if (!tagsEnabled) {
            setSelectedHashtagsFilter(['__disabled__']);
        } else if (selectedHashtagsFilter.length === 1 && selectedHashtagsFilter[0] === '__disabled__') {
            setSelectedHashtagsFilter([]);
        }
    // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [tagsEnabled]);

    /* ── Feed chip toggle ────────────────────────────────────────────── */
    const isFeedActive = useCallback(
        (uri: string) => selectedFeedUris.length === 0 || selectedFeedUris.includes(uri),
        [selectedFeedUris],
    );
    const toggleFeedChip = (uri: string) => {
        if (selectedFeedUris.length === 1 && selectedFeedUris[0] === uri) {
            setSelectedFeedUris([]);          // solo → back to all
        } else {
            setSelectedFeedUris([uri]);       // solo this feed
        }
    };

    /* ── Hashtag chip toggle ─────────────────────────────────────────── */
    const isTagActive = useCallback(
        (tag: string) => selectedHashtagsFilter.length === 0 || selectedHashtagsFilter.includes(tag),
        [selectedHashtagsFilter],
    );
    const toggleTagChip = (tag: string) => {
        if (selectedHashtagsFilter.length === 1 && selectedHashtagsFilter[0] === tag) {
            setSelectedHashtagsFilter([]);   // solo → back to all
        } else {
            setSelectedHashtagsFilter([tag]); // solo this tag
        }
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

                {/* ═══ FOLLOWING ══════════════════════════════════════════ */}
                {activeTab === 'following' && (
                    <FollowingSection followingList={followingList} />
                )}

                {/* ═══ SUBSCRIBED ═════════════════════════════════════════ */}
                {activeTab === 'subscribed' && (
                    <div>
                        {/* Feeds header */}
                        <div style={sectionHeaderStyle} onClick={() => setFeedsExpanded(v => !v)}>
                            <input
                                type="checkbox"
                                checked={feedsEnabled}
                                onChange={e => { e.stopPropagation(); setFeedsEnabled(e.target.checked); }}
                                onClick={e => e.stopPropagation()}
                                title="Include feeds in your subscribed timeline"
                            />
                            <span style={{ fontWeight: 600, fontSize: '0.85rem' }}>Feeds</span>
                            <span style={caretStyle}>{feedsExpanded ? '▾' : '▸'}</span>
                        </div>

                        {feedsExpanded && (
                            <div>
                                {visibleFeeds.length === 0 && (
                                    <div className="panel-hint" style={{ padding: '4px 8px' } as React.CSSProperties}>No custom feeds found.</div>
                                )}
                                <div style={chipContainerStyle}>
                                    {visibleFeeds.map(feed => (
                                        <button
                                            key={feed.uri}
                                            style={isFeedActive(feed.uri) && feedsEnabled ? chipActive : chipInactive}
                                            onClick={() => { if (feedsEnabled) toggleFeedChip(feed.uri); }}
                                            title={isFeedActive(feed.uri) ? 'Click to filter to only this feed' : 'Click to show only this feed'}
                                            disabled={!feedsEnabled}
                                        >
                                            {feed.displayName || 'Custom Feed'}
                                        </button>
                                    ))}
                                </div>
                            </div>
                        )}

                        {/* Hashtags header */}
                        <div style={sectionHeaderStyle} onClick={() => setTagsExpanded(v => !v)}>
                            <input
                                type="checkbox"
                                checked={tagsEnabled}
                                onChange={e => { e.stopPropagation(); setTagsEnabled(e.target.checked); }}
                                onClick={e => e.stopPropagation()}
                                title="Include hashtag posts in your subscribed timeline"
                            />
                            <span style={{ fontWeight: 600, fontSize: '0.85rem' }}>Hashtags</span>
                            <span style={caretStyle}>{tagsExpanded ? '▾' : '▸'}</span>
                        </div>

                        {tagsExpanded && (
                            <div>
                                {followedHashtags.length === 0 && (
                                    <div className="panel-hint" style={{ padding: '4px 8px' } as React.CSSProperties}>No tags followed yet.</div>
                                )}
                                <div style={chipContainerStyle}>
                                    {followedHashtags.map(tag => (
                                        <button
                                            key={tag}
                                            style={isTagActive(tag) && tagsEnabled ? chipActive : chipInactive}
                                            onClick={() => { if (tagsEnabled) toggleTagChip(tag); }}
                                            title={isTagActive(tag) ? 'Click to filter to only this hashtag' : 'Click to show only this hashtag'}
                                            disabled={!tagsEnabled}
                                        >
                                            #{tag}
                                        </button>
                                    ))}
                                </div>

                                {/* Compact add row */}
                                <div style={addRowStyle}>
                                    <input
                                        className="input-field"
                                        value={hashtagInput}
                                        onChange={e => setHashtagInput(e.target.value)}
                                        onKeyDown={e => { if (e.key === 'Enter') handleAddHashtag(); }}
                                        placeholder="#topic"
                                        style={{ flex: 1, fontSize: '0.8rem', padding: '3px 6px' } as React.CSSProperties}
                                    />
                                    <button className="btn btn-primary hashtag-add-btn" onClick={handleAddHashtag} style={{ padding: '3px 8px', fontSize: '0.8rem' } as React.CSSProperties}>+</button>
                                </div>
                            </div>
                        )}
                    </div>
                )}

                {/* ═══ YOU ════════════════════════════════════════════════ */}
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

                {/* ═══ ALL ════════════════════════════════════════════════ */}
                {activeTab !== 'following' && activeTab !== 'subscribed' && activeTab !== 'you' && (
                    <div>
                        <FollowingSection followingList={followingList} max={12} />
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
                            {/* Feed chips (compact, max 6) */}
                            <div style={{ ...chipContainerStyle, marginTop: 4 } as React.CSSProperties}>
                                {visibleFeeds.slice(0, 6).map(feed => (
                                    <button
                                        key={feed.uri}
                                        style={selectedFeedUris.includes(feed.uri) ? chipActive : chipInactive}
                                        onClick={() => {
                                            toggleFeedChip(feed.uri);
                                            onNavigateTab?.('subscribed', { feedUri: feed.uri });
                                        }}
                                        title={feed.displayName}
                                    >
                                        {feed.displayName || 'Feed'}
                                    </button>
                                ))}
                            </div>

                            {/* Hashtag add + pills */}
                            <div className="hashtag-input-row" style={{ marginTop: 6 } as React.CSSProperties}>
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
                            <div style={{ ...chipContainerStyle, marginTop: 6 } as React.CSSProperties}>
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

/* ─── Following list sub-component (reused in Following + All tabs) ──── */
function FollowingSection({ followingList, max }: { followingList: ProfileView[]; max?: number }) {
    const list = max ? followingList.slice(0, max) : followingList;
    return (
        <div>
            <div className="panel-section">
                <div className="panel-section-label">Active Follows</div>
            </div>
            {list.map(profile => (
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
    );
}
