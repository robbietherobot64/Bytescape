const fs = require('fs');

let panel = fs.readFileSync('backup_feedright.txt', 'utf-8');

// Replace props
panel = panel.replace(
    /selectedFeedUri:\s*string;[\s\S]*?setSelectedFeedUri:\s*\(uri:\s*string\)\s*=>\s*void;/g,
    `selectedFeedUris: string[];
    setSelectedFeedUris: (uris: string[]) => void;
    selectedHashtagsFilter: string[];
    setSelectedHashtagsFilter: (tags: string[]) => void;`
);

panel = panel.replace(
    /export function FeedRightPanel\(\{[^}]*\}\:\s*FeedRightPanelProps\)\s*\{/g,
    `import { useState } from 'react';\n\nexport function FeedRightPanel({
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
`
);

let oldSubscribedStr = `                ) : activeTab === 'subscribed' ? (
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
                    </div>`;

let newSubscribedStr = `                ) : activeTab === 'subscribed' ? (
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
                    </div>`;

panel = panel.replace(oldSubscribedStr, newSubscribedStr);

fs.writeFileSync('bytescape/src/components/FeedRightPanel.tsx', panel);
console.log("Patched FeedRightPanel");
