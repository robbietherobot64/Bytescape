import { ProfileView, GeneratorView } from '../types';

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
}: FeedRightPanelProps) {
    return (
        <div className="right-panel">
            <div style={{ padding: '20px 16px', borderBottom: '1px solid var(--border)' }}>
                <h3 style={{ margin: 0, fontSize: '0.75rem', color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '1px', fontWeight: 800 }}>
                    {activeTab === 'following' ? 'My Contacts' :
                        activeTab === 'archives' ? 'Archive Explorer' :
                            'Discovery Tools'}
                </h3>
            </div>

            <div style={{ flex: 1, overflowY: 'auto', padding: 16 }} className="sidebar-list">
                {activeTab === 'following' ? (
                    <div>
                        {followingList.map(profile => (
                            <div key={profile.did} className="contact-item" style={{
                                padding: '10px 12px',
                                borderRadius: 12,
                                display: 'flex',
                                alignItems: 'center',
                                gap: 12,
                                marginBottom: 6,
                                background: 'var(--bg-card)',
                                border: '1px solid var(--border)'
                            }}>
                                <div style={{ width: 36, height: 36, borderRadius: 18, background: 'var(--accent-blue)', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: '0.9rem', fontWeight: 700, color: 'white' }}>
                                    {profile.handle?.[0]?.toUpperCase()}
                                </div>
                                <div style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                                    <div style={{ fontSize: '0.9rem', fontWeight: 600 }}>{profile.displayName || profile.handle}</div>
                                    <div style={{ fontSize: '0.75rem', color: 'var(--text-muted)' }}>{profile.handle}</div>
                                </div>
                            </div>
                        ))}
                        {followingList.length === 0 && (
                            <div className="empty-state" style={{ padding: 20, fontSize: '0.85rem' }}>No contacts found.</div>
                        )}
                    </div>
                ) : activeTab === 'archives' ? (
                    <div>
                        <div style={{ marginBottom: 20 }}>
                            <div style={{ fontSize: '0.7rem', fontWeight: 800, color: 'var(--text-muted)', marginBottom: 8, textTransform: 'uppercase' }}>Filter by source</div>
                            <select
                                className="input-field"
                                value={archiveSourceFilter}
                                onChange={e => setArchiveSourceFilter(e.target.value)}
                                style={{ width: '100%', cursor: 'pointer' }}
                            >
                                <option value="">All vault sources</option>
                                {vaultSources.map(s => (
                                    <option key={s} value={s}>{s}</option>
                                ))}
                            </select>
                        </div>

                        <div style={{ padding: 16, borderRadius: 12, background: 'var(--bg-primary)', border: '1px solid var(--border)' }}>
                            <div style={{ fontSize: '0.7rem', fontWeight: 800, color: 'var(--text-muted)', marginBottom: 4, textTransform: 'uppercase' }}>Vault Inventory</div>
                            <div style={{ fontSize: '1.25rem', fontWeight: 800, color: 'var(--accent-purple)' }}>{archiveTotal} <span style={{ fontSize: '0.8rem', fontWeight: 500, color: 'var(--text-secondary)' }}>items</span></div>
                        </div>
                    </div>
                ) : (
                    <div>
                        {/* Feed Filter for 'all' tab */}
                        <div style={{ marginBottom: 24 }}>
                            <div style={{ fontSize: '0.7rem', fontWeight: 800, color: 'var(--text-muted)', marginBottom: 8, textTransform: 'uppercase' }}>Curated Feed Source</div>
                            <select
                                className="input-field"
                                value={selectedFeedUri}
                                onChange={e => setSelectedFeedUri(e.target.value)}
                                style={{ width: '100%', cursor: 'pointer' }}
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
                        <div>
                            <div style={{ fontSize: '0.7rem', fontWeight: 800, color: 'var(--text-muted)', marginBottom: 8, textTransform: 'uppercase' }}>Subscribe to Tags</div>
                            <div style={{ display: 'flex', gap: 6, marginBottom: 16 }}>
                                <input
                                    className="input-field"
                                    value={hashtagInput}
                                    onChange={e => setHashtagInput(e.target.value)}
                                    onKeyDown={e => e.key === 'Enter' && handleAddHashtag()}
                                    placeholder="#discovery"
                                />
                                <button className="btn btn-primary" style={{ padding: '0 12px' }} onClick={handleAddHashtag}>+</button>
                            </div>

                            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
                                {followedHashtags.map(tag => (
                                    <div key={tag} style={{
                                        display: 'flex',
                                        alignItems: 'center',
                                        gap: 6,
                                        padding: '4px 10px',
                                        borderRadius: 999,
                                        background: 'rgba(168,85,247,0.1)',
                                        border: '1px solid var(--accent-purple)',
                                        fontSize: '0.8rem',
                                        color: 'var(--accent-purple)',
                                        fontWeight: 600
                                    }}>
                                        <span>#{tag}</span>
                                        <button
                                            style={{ background: 'none', border: 'none', color: 'inherit', cursor: 'pointer', padding: 0, display: 'flex' }}
                                            onClick={() => handleRemoveHashtag(tag)}
                                        >
                                            ✕
                                        </button>
                                    </div>
                                ))}
                            </div>
                            {followedHashtags.length === 0 && (
                                <div style={{ color: 'var(--text-muted)', fontSize: '0.8rem', fontStyle: 'italic', marginTop: 4 }}>No tags followed yet.</div>
                            )}
                        </div>
                    </div>
                )}
            </div>
        </div>
    );
}
