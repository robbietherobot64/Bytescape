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
            <div style={{ padding: 16, borderBottom: '1px solid var(--border)' }}>
                <h3 style={{ margin: 0, fontSize: '0.9rem', color: 'var(--text-secondary)', textTransform: 'uppercase', letterSpacing: '0.5px' }}>
                    {activeTab === 'following' ? 'Following' :
                     activeTab === 'feeds' ? 'Feeds' :
                     activeTab === 'hashtags' ? 'Hashtags' :
                     activeTab === 'rss' ? 'RSS Feeds' :
                     activeTab === 'archives' ? 'Archive' : 'All Sources'}
                </h3>
            </div>

            <div style={{ flex: 1, overflowY: 'auto', padding: 16 }}>
                {activeTab === 'following' ? (
                    <div className="sidebar-list">
                        {followingList.map(profile => (
                            <div key={profile.did} className="contact-item" style={{
                                padding: '8px 12px',
                                borderRadius: 8,
                                display: 'flex',
                                alignItems: 'center',
                                gap: 12,
                                marginBottom: 4,
                                background: 'var(--bg-elevated)'
                            }}>
                                <div style={{ width: 32, height: 32, borderRadius: 16, background: 'var(--accent-purple)', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: '0.8rem', fontWeight: 700 }}>
                                    {profile.handle?.[0]?.toUpperCase()}
                                </div>
                                <div style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                                    <div style={{ fontSize: '0.85rem', fontWeight: 600 }}>{profile.displayName || profile.handle}</div>
                                    <div style={{ fontSize: '0.75rem', color: 'var(--text-muted)' }}>{profile.handle}</div>
                                </div>
                            </div>
                        ))}
                        {followingList.length === 0 && (
                            <div style={{ color: 'var(--text-secondary)', fontSize: '0.85rem' }}>No followed profiles found.</div>
                        )}
                    </div>
                ) : activeTab === 'feeds' ? (
                    <div className="sidebar-list">
                        <div style={{ marginBottom: 16 }}>
                            <div style={{ fontSize: '0.75rem', fontWeight: 700, opacity: 0.5, marginBottom: 8, textTransform: 'uppercase' }}>Feed source</div>
                            <select
                                value={selectedFeedUri}
                                onChange={e => setSelectedFeedUri(e.target.value)}
                                style={{ width: '100%', height: 34, borderRadius: 8, background: 'var(--bg-elevated)', color: 'var(--text-primary)', border: '1px solid var(--border)', padding: '0 8px' }}
                            >
                                <option value={FEED_FILTER_ALL}>All followed feeds</option>
                                {savedFeeds
                                    .filter(feed => !feed.uri.includes('app.bsky.feed.generator/following'))
                                    .map(feed => (
                                        <option key={feed.uri} value={feed.uri}>
                                            {feed.displayName || feed.uri}
                                        </option>
                                    ))}
                            </select>
                        </div>
                        <div style={{ fontSize: '0.75rem', fontWeight: 700, opacity: 0.5, margin: '16px 0 8px', textTransform: 'uppercase' }}>Available followed feeds</div>
                        {savedFeeds
                            .filter(feed => !feed.uri.includes('app.bsky.feed.generator/following'))
                            .map(feed => (
                            <div key={feed.uri} style={{ padding: '6px 8px', borderRadius: 6, background: selectedFeedUri === feed.uri ? 'rgba(168,85,247,0.2)' : 'var(--bg-elevated)', marginBottom: 6, fontSize: '0.85rem' }}>
                                {feed.displayName || feed.uri}
                            </div>
                        ))}
                        {savedFeeds.filter(feed => !feed.uri.includes('app.bsky.feed.generator/following')).length === 0 && (
                            <div style={{ color: 'var(--text-secondary)', fontSize: '0.85rem' }}>No custom followed feeds found.</div>
                        )}
                    </div>
                ) : activeTab === 'hashtags' ? (
                    <div className="sidebar-list">
                        <div style={{ fontSize: '0.75rem', fontWeight: 700, opacity: 0.5, marginBottom: 8, textTransform: 'uppercase' }}>Subscribe to hashtag</div>
                        <div style={{ display: 'flex', gap: 6, marginBottom: 16 }}>
                            <input
                                value={hashtagInput}
                                onChange={e => setHashtagInput(e.target.value)}
                                onKeyDown={e => e.key === 'Enter' && handleAddHashtag()}
                                placeholder="#cats"
                                style={{ flex: 1, background: 'var(--bg-elevated)', border: '1px solid var(--border)', borderRadius: 6, padding: '4px 8px', color: 'var(--text-primary)', fontSize: '0.85rem' }}
                            />
                            <button className="btn btn-primary btn-sm" onClick={handleAddHashtag}>+</button>
                        </div>
                        <div style={{ fontSize: '0.75rem', fontWeight: 700, opacity: 0.5, marginBottom: 8, textTransform: 'uppercase' }}>Followed tags</div>
                        {followedHashtags.length === 0 && (
                            <div style={{ color: 'var(--text-secondary)', fontSize: '0.85rem' }}>No hashtags followed yet.</div>
                        )}
                        {followedHashtags.map(tag => (
                            <div key={tag} style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '5px 8px', borderRadius: 6, background: 'var(--bg-elevated)', marginBottom: 4, fontSize: '0.85rem' }}>
                                <span>#{tag}</span>
                                <button className="btn btn-ghost btn-sm" style={{ padding: '0 6px', lineHeight: 1.4 }} onClick={() => handleRemoveHashtag(tag)}>✕</button>
                            </div>
                        ))}
                    </div>
                ) : activeTab === 'rss' ? (
                    <div style={{ color: 'var(--text-secondary)', fontSize: '0.85rem', lineHeight: 1.6 }}>
                        <div style={{ fontWeight: 600, marginBottom: 8 }}>RSS / Atom Feeds</div>
                        <div>Add any blog, newsletter, or podcast feed URL and it will appear here alongside your social content.</div>
                        <div style={{ marginTop: 12, padding: '8px 10px', borderRadius: 8, background: 'var(--bg-elevated)', fontSize: '0.8rem', opacity: 0.7 }}>Coming soon — tracked in Phase 4 roadmap.</div>
                    </div>
                ) : activeTab === 'archives' ? (
                    <div className="sidebar-list">
                        <div style={{ fontSize: '0.75rem', fontWeight: 700, opacity: 0.5, marginBottom: 8, textTransform: 'uppercase' }}>Filter by source</div>
                        <select
                            value={archiveSourceFilter}
                            onChange={e => setArchiveSourceFilter(e.target.value)}
                            style={{ width: '100%', height: 34, borderRadius: 8, background: 'var(--bg-elevated)', color: 'var(--text-primary)', border: '1px solid var(--border)', padding: '0 8px', marginBottom: 16 }}
                        >
                            <option value="">All sources</option>
                            {vaultSources.map(s => (
                                <option key={s} value={s}>{s}</option>
                            ))}
                        </select>
                        <div style={{ fontSize: '0.75rem', fontWeight: 700, opacity: 0.5, marginBottom: 8, textTransform: 'uppercase' }}>Stats</div>
                        <div style={{ fontSize: '0.85rem', color: 'var(--text-secondary)' }}>
                            {archiveTotal} items in vault
                        </div>
                    </div>
                ) : (
                    <div style={{ color: 'var(--text-secondary)', fontSize: '0.85rem', lineHeight: 1.6 }}>
                        <div style={{ fontWeight: 600, marginBottom: 8 }}>Unified Timeline</div>
                        <div>This view merges your live Bluesky timeline, custom feeds, and local vault archives into a single chronological stream.</div>
                    </div>
                )}
            </div>
        </div>
    );
}
