import { useState, useEffect, useCallback } from 'react';
import { ProfileView, GeneratorView } from '../types';
import { openProfile } from '../utils/openInApp';
import { FeedTab } from '../hooks/useUnifiedFeed';

/**
 * FeedRightPanel
 *
 * PURPOSE
 * - Render contextual right/left rail controls for feed navigation and filtering.
 * - Provide tab-specific controls for Following, Subscribed, You, and Discovery views.
 *
 * HOW IT WORKS
 * - Receives all state via props from the parent feed composition layer.
 * - Uses local UI-only expansion/enable toggles for feed/tag filter sections.
 * - Updates parent-managed filter arrays through callback setters.
 *
 * RELATED FILES
 * - ../hooks/useUnifiedFeed (tab model + feed query behavior)
 * - ../screens/FeedScreen.tsx (parent orchestration and prop wiring)
 * - ../utils/openInApp (external profile routing for follow rows)
 * - ../types/index (ProfileView/GeneratorView interfaces)
 *
 * CROSS-SYSTEM NOTES
 * - This component does not call backend commands directly; data comes from parent hooks.
 * - Parent-provided callbacks (`setSelectedFeedUris`, `setSelectedHashtagsFilter`,
 *   `onNavigateTab`) are the integration seam to store/provider/backend-driven state.
 */

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

/* ── (styles moved to FeedScreen.css — .sub-* classes) ─────────────── */


/* ═══════════════════════════════════════════════════════════════════════
    Main exported panel
    ═══════════════════════════════════════════════════════════════════════ */

/**
 * Render the feed side panel with tab-aware controls and compact navigation affordances.
 */
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
    archiveSourceFilter: _archiveSourceFilter,
    setArchiveSourceFilter: _setArchiveSourceFilter,
    vaultSources: _vaultSources,
    archiveTotal: _archiveTotal,
    position = 'right',
    onNavigateTab,
}: FeedRightPanelProps) {
    /* ═══ Local UI state (presentation only) ═══════════════════════════ */
    /* ── Subscribed tab state ────────────────────────────────────────── */
    const [feedsExpanded, setFeedsExpanded] = useState(true);
    const [tagsExpanded, setTagsExpanded] = useState(true);
    const [feedsEnabled, setFeedsEnabled] = useState(true);
    const [tagsEnabled, setTagsEnabled] = useState(true);

    const visibleFeeds = savedFeeds.filter(f => !f.uri.includes('/following'));

    /* ═══ Parent filter synchronization ════════════════════════════════ */
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

    /* ═══ Chip toggle helpers ══════════════════════════════════════════ */
    /* ── Feed chip toggle (multi-select) ─────────────────────────────── */
    const isFeedActive = useCallback(
        (uri: string) => selectedFeedUris.length === 0 || selectedFeedUris.includes(uri),
        [selectedFeedUris],
    );
    const isFeedExplicitlySelected = useCallback(
        (uri: string) => selectedFeedUris.includes(uri),
        [selectedFeedUris],
    );
    const toggleFeedChip = (uri: string) => {
        if (selectedFeedUris.includes(uri)) {
            // Remove from selection
            const next = selectedFeedUris.filter(u => u !== uri);
            setSelectedFeedUris(next);
        } else {
            // Add to selection
            setSelectedFeedUris([...selectedFeedUris, uri]);
        }
    };

    /* ── Hashtag chip toggle (multi-select) ──────────────────────────── */
    const isTagActive = useCallback(
        (tag: string) => selectedHashtagsFilter.length === 0 || selectedHashtagsFilter.includes(tag),
        [selectedHashtagsFilter],
    );
    const isTagExplicitlySelected = useCallback(
        (tag: string) => selectedHashtagsFilter.includes(tag),
        [selectedHashtagsFilter],
    );
    const toggleTagChip = (tag: string) => {
        if (selectedHashtagsFilter.includes(tag)) {
            // Remove from selection
            const next = selectedHashtagsFilter.filter(t => t !== tag);
            setSelectedHashtagsFilter(next);
        } else {
            // Add to selection
            setSelectedHashtagsFilter([...selectedHashtagsFilter, tag]);
        }
    };

    /* ── X button handler: deselect if selected, confirm-remove if not ── */
    const handleFeedX = (uri: string, displayName: string) => {
        if (isFeedExplicitlySelected(uri)) {
            // Just deselect
            setSelectedFeedUris(selectedFeedUris.filter(u => u !== uri));
        } else {
            // Already deselected → confirm unsubscribe
            if (window.confirm(`Are you sure you want to remove the feed "${displayName}"?`)) {
                // Remove from saved feeds via parent callback (if available)
                // For now, deselect and mark as disabled
                setSelectedFeedUris(selectedFeedUris.filter(u => u !== uri));
            }
        }
    };
    const handleTagX = (tag: string) => {
        if (isTagExplicitlySelected(tag)) {
            // Just deselect
            setSelectedHashtagsFilter(selectedHashtagsFilter.filter(t => t !== tag));
        } else {
            // Already deselected → confirm unsubscribe
            if (window.confirm(`Are you sure you want to unsubscribe from #${tag}?`)) {
                handleRemoveHashtag(tag);
            }
        }
    };

    const panelTitle =
        activeTab === 'following' ? 'Following' :
        activeTab === 'subscribed' ? 'Subscriptions' :
        activeTab === 'groups' ? 'Groups' :
        activeTab === 'rss' ? 'RSS Feeds' :
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
                    <div className="sub-panel">
                        {/* Feeds header */}
                        <div className="sub-section-header" onClick={() => setFeedsExpanded(v => !v)}>
                            <span className="sub-section-label">Feeds</span>
                            <span className="sub-section-controls">
                                <input
                                    type="checkbox"
                                    checked={feedsEnabled}
                                    onChange={e => { e.stopPropagation(); setFeedsEnabled(e.target.checked); }}
                                    onClick={e => e.stopPropagation()}
                                    title="Include feeds in your subscribed timeline"
                                />
                                <span className="sub-section-caret">{feedsExpanded ? '▾' : '▸'}</span>
                            </span>
                        </div>

                        {feedsExpanded && (
                            <div className="sub-section-body">
                                {visibleFeeds.length === 0 && (
                                    <div className="panel-hint">No custom feeds found.</div>
                                )}
                                <div className="sub-chip-list">
                                    {visibleFeeds.map(feed => (
                                        <div key={feed.uri} className={`sub-chip ${isFeedExplicitlySelected(feed.uri) ? 'sub-chip--selected' : isFeedActive(feed.uri) && feedsEnabled ? 'sub-chip--active' : 'sub-chip--inactive'}`}>
                                            <span
                                                className="sub-chip-label"
                                                onClick={() => { if (feedsEnabled) toggleFeedChip(feed.uri); }}
                                                title={feed.displayName || 'Custom Feed'}
                                            >
                                                {feed.displayName || 'Custom Feed'}
                                            </span>
                                            {feedsEnabled && (
                                                <button
                                                    className="sub-chip-x"
                                                    onClick={(e) => { e.stopPropagation(); handleFeedX(feed.uri, feed.displayName || 'Custom Feed'); }}
                                                    title={isFeedExplicitlySelected(feed.uri) ? 'Remove from filter' : 'Unsubscribe from feed'}
                                                >✕</button>
                                            )}
                                        </div>
                                    ))}
                                </div>
                            </div>
                        )}

                        {/* Hashtags header */}
                        <div className="sub-section-header" onClick={() => setTagsExpanded(v => !v)}>
                            <span className="sub-section-label">Hashtags</span>
                            <span className="sub-section-controls">
                                <input
                                    type="checkbox"
                                    checked={tagsEnabled}
                                    onChange={e => { e.stopPropagation(); setTagsEnabled(e.target.checked); }}
                                    onClick={e => e.stopPropagation()}
                                    title="Include hashtag posts in your subscribed timeline"
                                />
                                <span className="sub-section-caret">{tagsExpanded ? '▾' : '▸'}</span>
                            </span>
                        </div>

                        {tagsExpanded && (
                            <div className="sub-section-body">
                                {followedHashtags.length === 0 && (
                                    <div className="panel-hint">No tags followed yet.</div>
                                )}
                                <div className="sub-chip-list">
                                    {followedHashtags.map(tag => (
                                        <div key={tag} className={`sub-chip ${isTagExplicitlySelected(tag) ? 'sub-chip--selected' : isTagActive(tag) && tagsEnabled ? 'sub-chip--active' : 'sub-chip--inactive'}`}>
                                            <span
                                                className="sub-chip-label"
                                                onClick={() => { if (tagsEnabled) toggleTagChip(tag); }}
                                                title={`#${tag}`}
                                            >
                                                #{tag}
                                            </span>
                                            {tagsEnabled && (
                                                <button
                                                    className="sub-chip-x"
                                                    onClick={(e) => { e.stopPropagation(); handleTagX(tag); }}
                                                    title={isTagExplicitlySelected(tag) ? 'Remove from filter' : 'Unfollow tag'}
                                                >✕</button>
                                            )}
                                        </div>
                                    ))}
                                </div>

                                {/* Compact add row */}
                                <div className="sub-add-row">
                                    <input
                                        className="input-field sub-add-input"
                                        value={hashtagInput}
                                        onChange={e => setHashtagInput(e.target.value)}
                                        onKeyDown={e => { if (e.key === 'Enter') handleAddHashtag(); }}
                                        placeholder="Add #topic"
                                    />
                                    <button className="btn btn-primary sub-add-btn" onClick={handleAddHashtag}>+</button>
                                </div>
                            </div>
                        )}
                    </div>
                )}

                {/* ═══ ALL / GROUPS / RSS ═══════════════════════════════ */}
                {activeTab !== 'following' && activeTab !== 'subscribed' && (
                    <div>
                        <FollowingSection followingList={followingList} max={12} />
                        {followingList.length > 12 && (
                            <div
                                className="panel-hint panel-hint--clickable"
                                onClick={() => onNavigateTab?.('following')}
                            >
                                +{followingList.length - 12} more →
                            </div>
                        )}

                        <div className="panel-divider" />

                        {/* ── Full subscription panel (same UI as subscribed tab) ── */}
                        <div className="sub-panel">
                            {/* Feeds header */}
                            <div className="sub-section-header" onClick={() => setFeedsExpanded(v => !v)}>
                                <span className="sub-section-label">Feeds</span>
                                <span className="sub-section-controls">
                                    <input
                                        type="checkbox"
                                        checked={feedsEnabled}
                                        onChange={e => { e.stopPropagation(); setFeedsEnabled(e.target.checked); }}
                                        onClick={e => e.stopPropagation()}
                                        title="Include feeds in results"
                                    />
                                    <span className="sub-section-caret">{feedsExpanded ? '▾' : '▸'}</span>
                                </span>
                            </div>

                            {feedsExpanded && (
                                <div className="sub-section-body">
                                    {visibleFeeds.length === 0 && (
                                        <div className="panel-hint">No custom feeds found.</div>
                                    )}
                                    <div className="sub-chip-list">
                                        {visibleFeeds.map(feed => (
                                            <div key={feed.uri} className={`sub-chip ${isFeedExplicitlySelected(feed.uri) ? 'sub-chip--selected' : isFeedActive(feed.uri) && feedsEnabled ? 'sub-chip--active' : 'sub-chip--inactive'}`}>
                                                <span
                                                    className="sub-chip-label"
                                                    onClick={() => { if (feedsEnabled) toggleFeedChip(feed.uri); }}
                                                    title={feed.displayName || 'Custom Feed'}
                                                >
                                                    {feed.displayName || 'Custom Feed'}
                                                </span>
                                                {feedsEnabled && (
                                                    <button
                                                        className="sub-chip-x"
                                                        onClick={(e) => { e.stopPropagation(); handleFeedX(feed.uri, feed.displayName || 'Custom Feed'); }}
                                                        title={isFeedExplicitlySelected(feed.uri) ? 'Remove from filter' : 'Unsubscribe from feed'}
                                                    >✕</button>
                                                )}
                                            </div>
                                        ))}
                                    </div>
                                </div>
                            )}

                            {/* Hashtags header */}
                            <div className="sub-section-header" onClick={() => setTagsExpanded(v => !v)}>
                                <span className="sub-section-label">Hashtags</span>
                                <span className="sub-section-controls">
                                    <input
                                        type="checkbox"
                                        checked={tagsEnabled}
                                        onChange={e => { e.stopPropagation(); setTagsEnabled(e.target.checked); }}
                                        onClick={e => e.stopPropagation()}
                                        title="Include hashtag posts in results"
                                    />
                                    <span className="sub-section-caret">{tagsExpanded ? '▾' : '▸'}</span>
                                </span>
                            </div>

                            {tagsExpanded && (
                                <div className="sub-section-body">
                                    {followedHashtags.length === 0 && (
                                        <div className="panel-hint">No tags followed yet.</div>
                                    )}
                                    <div className="sub-chip-list">
                                        {followedHashtags.map(tag => (
                                            <div key={tag} className={`sub-chip ${isTagExplicitlySelected(tag) ? 'sub-chip--selected' : isTagActive(tag) && tagsEnabled ? 'sub-chip--active' : 'sub-chip--inactive'}`}>
                                                <span
                                                    className="sub-chip-label"
                                                    onClick={() => { if (tagsEnabled) toggleTagChip(tag); }}
                                                    title={`#${tag}`}
                                                >
                                                    #{tag}
                                                </span>
                                                {tagsEnabled && (
                                                    <button
                                                        className="sub-chip-x"
                                                        onClick={(e) => { e.stopPropagation(); handleTagX(tag); }}
                                                        title={isTagExplicitlySelected(tag) ? 'Remove from filter' : 'Unfollow tag'}
                                                    >✕</button>
                                                )}
                                            </div>
                                        ))}
                                    </div>

                                    {/* Compact add row */}
                                    <div className="sub-add-row">
                                        <input
                                            className="input-field sub-add-input"
                                            value={hashtagInput}
                                            onChange={e => setHashtagInput(e.target.value)}
                                            onKeyDown={e => { if (e.key === 'Enter') handleAddHashtag(); }}
                                            placeholder="Add #topic"
                                        />
                                        <button className="btn btn-primary sub-add-btn" onClick={handleAddHashtag}>+</button>
                                    </div>
                                </div>
                            )}
                        </div>
                    </div>
                )}

            </div>
        </div>
    );
}

/* ─── Following list sub-component (reused in Following + All tabs) ──── */
/**
 * Compact follow-row list used in multiple panel contexts.
 *
 * Cross-system behavior:
 * - Uses `openProfile()` to route to in-app profile views for selected follows.
 */
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
