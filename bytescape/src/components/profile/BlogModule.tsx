import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ProfileModule } from '../../types';
import { openInApp } from '../../utils/openInApp';
import { BsPlusLg } from 'react-icons/bs';

interface BlogEntry {
    id: string;
    title: string;
    body: string;
    timestamp: number;
    external_url?: string;
    author?: string;
    from_feed?: boolean;
}

interface BlogFeedConfig {
    feed_url: string;
    site_title?: string;
    last_fetched?: number;
}

interface BlogModuleData {
    entries: BlogEntry[];
    feed?: BlogFeedConfig;
}

interface FeedBlogEntry {
    id: string;
    title: string;
    body: string;
    timestamp: number;
    external_url: string | null;
    author: string | null;
}

interface BlogFeedResult {
    feed_url: string;
    site_title: string | null;
    entries: FeedBlogEntry[];
}

function generateId(): string {
    return `mod-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

interface BlogModuleProps {
    mod: ProfileModule;
    editMode: boolean;
    onSaveDirect: (modId: string, newContent: string) => void;
}

export function BlogModule({ mod, editMode, onSaveDirect }: BlogModuleProps) {
    const [editingBlogId, setEditingBlogId] = useState<string | null>(null);
    const [blogTitle, setBlogTitle] = useState('');
    const [blogBody, setBlogBody] = useState('');
    const [blogUrl, setBlogUrl] = useState('');
    const [expandedBlogId, setExpandedBlogId] = useState<string | null>(null);
    const [blogFeedLoading, setBlogFeedLoading] = useState(false);
    const [blogFeedError, setBlogFeedError] = useState('');
    const [blogSubscribeUrl, setBlogSubscribeUrl] = useState('');
    const [blogIframeView, setBlogIframeView] = useState<Record<string, boolean>>({});
    const [blogIframeLoaded, setBlogIframeLoaded] = useState<Record<string, 'loading' | 'loaded' | 'error'>>({});

    // Default: entries with external_url start in iframe view
    const isIframeView = (entryId: string, hasUrl: boolean): boolean => {
        if (!hasUrl) return false;
        // If user has explicitly toggled, respect that; otherwise default to true
        return blogIframeView[entryId] ?? true;
    };

    // Parse module content
    let data: BlogModuleData = { entries: [] };
    try {
        const parsed = JSON.parse(mod.content || '{"entries":[]}');
        if (Array.isArray(parsed)) {
            data = { entries: parsed };
        } else if (parsed && typeof parsed === 'object') {
            data = parsed as BlogModuleData;
            if (!Array.isArray(data.entries)) data.entries = [];
        }
    } catch { data = { entries: [] }; }

    const entries = data.entries;
    const feedConfig = data.feed;

    const saveBlogData = (newData: BlogModuleData) => {
        onSaveDirect(mod.id, JSON.stringify(newData));
    };

    const doFeedSubscribe = async () => {
        const url = blogSubscribeUrl.trim();
        if (!url) return;
        setBlogFeedLoading(true);
        setBlogFeedError('');
        try {
            const result = await invoke<BlogFeedResult>('fetch_blog_feed', { url });
            const existingUrls = new Set(entries.filter(e => e.external_url).map(e => e.external_url));
            const newEntries: BlogEntry[] = result.entries
                .filter(fe => !fe.external_url || !existingUrls.has(fe.external_url))
                .map(fe => ({
                    id: fe.id, title: fe.title, body: fe.body, timestamp: fe.timestamp,
                    external_url: fe.external_url || undefined, author: fe.author || undefined, from_feed: true,
                }));
            const merged = [...newEntries, ...entries].sort((a, b) => b.timestamp - a.timestamp);
            saveBlogData({
                entries: merged,
                feed: { feed_url: result.feed_url, site_title: result.site_title || undefined, last_fetched: Math.floor(Date.now() / 1000) },
            });
            setBlogSubscribeUrl('');
            if (merged.length > 0) setExpandedBlogId(merged[0].id);
        } catch (err) {
            setBlogFeedError(String(err));
        } finally {
            setBlogFeedLoading(false);
        }
    };

    const doFeedRefresh = async () => {
        if (!feedConfig?.feed_url) return;
        setBlogFeedLoading(true);
        setBlogFeedError('');
        try {
            const result = await invoke<BlogFeedResult>('fetch_blog_feed', { url: feedConfig.feed_url });
            const existingUrls = new Set(entries.filter(e => e.external_url).map(e => e.external_url));
            const newEntries: BlogEntry[] = result.entries
                .filter(fe => !fe.external_url || !existingUrls.has(fe.external_url))
                .map(fe => ({
                    id: fe.id, title: fe.title, body: fe.body, timestamp: fe.timestamp,
                    external_url: fe.external_url || undefined, author: fe.author || undefined, from_feed: true,
                }));
            if (newEntries.length > 0) {
                const merged = [...newEntries, ...entries].sort((a, b) => b.timestamp - a.timestamp);
                saveBlogData({ ...data, entries: merged, feed: { ...feedConfig, last_fetched: Math.floor(Date.now() / 1000) } });
            } else {
                saveBlogData({ ...data, feed: { ...feedConfig, last_fetched: Math.floor(Date.now() / 1000) } });
            }
        } catch (err) {
            setBlogFeedError(String(err));
        } finally {
            setBlogFeedLoading(false);
        }
    };

    const doUnsubscribe = () => {
        saveBlogData({ entries: entries.filter(e => !e.from_feed), feed: undefined });
    };

    const doBlogPublish = () => {
        let title = blogTitle.trim();
        const body = blogBody.trim();
        const url = blogUrl.trim();
        if (!title && url) {
            try { title = new URL(url).hostname.replace(/^www\./, ''); } catch { title = url; }
        }
        if (!title && body) {
            title = body.slice(0, 60) + (body.length > 60 ? '\u2026' : '');
        }
        if (!title) return;
        const newEntry: BlogEntry = {
            id: generateId(), title, body,
            timestamp: Math.floor(Date.now() / 1000),
            external_url: url || undefined,
        };
        saveBlogData({ ...data, entries: [newEntry, ...entries] });
        setExpandedBlogId(newEntry.id);
        setBlogTitle(''); setBlogBody(''); setBlogUrl(''); setEditingBlogId(null);
    };

    const doBlogEdit = () => {
        const updated = entries.map(e => e.id === editingBlogId ? { ...e, title: blogTitle.trim() || e.title, body: blogBody.trim(), external_url: blogUrl.trim() || undefined } : e);
        saveBlogData({ ...data, entries: updated });
        setBlogTitle(''); setBlogBody(''); setBlogUrl(''); setEditingBlogId(null);
    };

    if (editMode && editingBlogId === 'new') {
        return (
            <div className="profile-blog-editor">
                <input className="profile-blog-title-input" type="text" placeholder="Post title (auto-generated if left blank)" value={blogTitle} maxLength={200} onChange={(e) => setBlogTitle(e.target.value)} autoFocus />
                <textarea className="profile-blog-body-input" placeholder="Write your blog post&#x2026;" value={blogBody} maxLength={10000} onChange={(e) => setBlogBody(e.target.value)} />
                <input className="profile-blog-title-input" type="text" placeholder="Link to external blog post (optional)" value={blogUrl} onChange={(e) => setBlogUrl(e.target.value)} style={{ fontSize: 12, fontWeight: 400 } as React.CSSProperties} />
                <div className="profile-blog-editor-actions">
                    <button className="profile-blog-publish-btn" onClick={doBlogPublish}>Publish</button>
                    <button className="profile-blog-cancel-btn" onClick={() => { setEditingBlogId(null); setBlogTitle(''); setBlogBody(''); setBlogUrl(''); }}>Cancel</button>
                </div>
            </div>
        );
    }

    if (editMode && editingBlogId && editingBlogId !== 'new') {
        const entry = entries.find(e => e.id === editingBlogId);
        if (entry) {
            return (
                <div className="profile-blog-editor">
                    <input className="profile-blog-title-input" type="text" value={blogTitle} maxLength={200} onChange={(e) => setBlogTitle(e.target.value)} autoFocus />
                    <textarea className="profile-blog-body-input" value={blogBody} maxLength={10000} onChange={(e) => setBlogBody(e.target.value)} />
                    <input className="profile-blog-title-input" type="text" placeholder="External link (optional)" value={blogUrl} onChange={(e) => setBlogUrl(e.target.value)} style={{ fontSize: 12, fontWeight: 400 } as React.CSSProperties} />
                    <div className="profile-blog-editor-actions">
                        <button className="profile-blog-publish-btn" onClick={doBlogEdit}>Save</button>
                        <button className="profile-blog-cancel-btn" onClick={() => { setEditingBlogId(null); setBlogTitle(''); setBlogBody(''); setBlogUrl(''); }}>Cancel</button>
                    </div>
                </div>
            );
        }
    }

    return (
        <div>
            {editMode && !feedConfig && (
                <div className="profile-blog-subscribe" style={{ marginBottom: 10 } as React.CSSProperties}>
                    <div style={{ display: 'flex', gap: 6, alignItems: 'center' } as React.CSSProperties}>
                        <input className="profile-blog-title-input" type="text" placeholder="Paste blog URL to subscribe (WordPress, Ghost, any RSS/Atom)..." value={blogSubscribeUrl} onChange={(e) => { setBlogSubscribeUrl(e.target.value); setBlogFeedError(''); }} style={{ flex: 1, fontSize: 12 } as React.CSSProperties} onKeyDown={(e) => { if (e.key === 'Enter') doFeedSubscribe(); }} />
                        <button className="profile-blog-publish-btn" onClick={doFeedSubscribe} disabled={blogFeedLoading || !blogSubscribeUrl.trim()} style={{ whiteSpace: 'nowrap', fontSize: 11 } as React.CSSProperties}>
                            {blogFeedLoading ? '\u23F3 Fetching...' : '\uD83D\uDD17 Subscribe'}
                        </button>
                    </div>
                    {blogFeedError && <div style={{ color: 'var(--accent-red, #f44)', fontSize: 11, marginTop: 4 } as React.CSSProperties}>{blogFeedError}</div>}
                </div>
            )}

            {feedConfig && (
                <div className="profile-blog-feed-header" style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '6px 10px', background: 'var(--bg-tertiary)', borderRadius: 'var(--border-radius)', marginBottom: 10, fontSize: 12, color: 'var(--text-secondary)' } as React.CSSProperties}>
                    <span style={{ fontSize: 14 } as React.CSSProperties}>📡</span>
                    <span style={{ flex: 1 } as React.CSSProperties}>
                        Subscribed to <strong style={{ color: 'var(--text-primary)' } as React.CSSProperties}>{feedConfig.site_title || feedConfig.feed_url}</strong>
                        {feedConfig.last_fetched && <span style={{ marginLeft: 8, color: 'var(--text-muted)' } as React.CSSProperties}>· Last updated {new Date(feedConfig.last_fetched * 1000).toLocaleDateString()}</span>}
                    </span>
                    <button className="profile-blog-publish-btn" onClick={doFeedRefresh} disabled={blogFeedLoading} style={{ fontSize: 10, padding: '3px 8px' } as React.CSSProperties}>{blogFeedLoading ? '\u23F3' : '\uD83D\uDD04'} Refresh</button>
                    {editMode && <button className="profile-blog-cancel-btn" onClick={doUnsubscribe} style={{ fontSize: 10, padding: '3px 8px' } as React.CSSProperties}>Unsubscribe</button>}
                </div>
            )}

            {blogFeedError && feedConfig && <div style={{ color: 'var(--accent-red, #f44)', fontSize: 11, marginBottom: 8 } as React.CSSProperties}>{blogFeedError}</div>}

            {editMode && (
                <div style={{ display: 'flex', gap: 6, marginBottom: 8 } as React.CSSProperties}>
                    <button className="profile-blog-new-btn" onClick={() => { setBlogTitle(''); setBlogBody(''); setBlogUrl(''); setEditingBlogId('new'); }}>
                        <BsPlusLg size={10} /> New Post
                    </button>
                    {entries.length > 0 && (
                        <button className="profile-blog-cancel-btn" onClick={() => { saveBlogData({ entries: [] }); setBlogFeedError(''); }} style={{ fontSize: 11 } as React.CSSProperties}>
                            {'\uD83D\uDDD1'} Clear All Posts
                        </button>
                    )}
                </div>
            )}

            {entries.length === 0 && (
                <div className="profile-empty">
                    <div className="profile-empty-icon">{'\u270D\uFE0F'}</div>
                    <p className="profile-empty-text">{editMode ? 'Subscribe to a blog or click "New Post" to get started.' : 'No blog posts yet.'}</p>
                </div>
            )}

            <div className="profile-blog-list">
                {entries.map(entry => (
                    <div key={entry.id} className="profile-blog-entry">
                        <div className="profile-blog-entry-header" onClick={() => {
                            const expanding = expandedBlogId !== entry.id;
                            setExpandedBlogId(expanding ? entry.id : null);
                            // Set iframe loading state when expanding an entry that defaults to iframe view
                            if (expanding && entry.external_url && isIframeView(entry.id, true) && !blogIframeLoaded[entry.id]) {
                                setBlogIframeLoaded(prev => ({ ...prev, [entry.id]: 'loading' }));
                            }
                        }}>
                            <h4 className="profile-blog-entry-title">{entry.title}</h4>
                            <div style={{ display: 'flex', alignItems: 'center', gap: 6 } as React.CSSProperties}>
                                {entry.author && <span style={{ fontSize: 10, color: 'var(--text-muted)' } as React.CSSProperties}>by {entry.author}</span>}
                                {entry.from_feed && <span style={{ fontSize: 9, padding: '1px 5px', background: 'var(--accent-purple)', borderRadius: 3, color: '#fff' } as React.CSSProperties}>RSS</span>}
                                <span className="profile-blog-entry-date">{entry.timestamp > 0 ? new Date(entry.timestamp * 1000).toLocaleDateString() : ''}</span>
                                <span className="profile-blog-entry-expand">{expandedBlogId === entry.id ? '\u25B2' : '\u25BC'}</span>
                            </div>
                        </div>
                        {expandedBlogId === entry.id && (
                            <div className="profile-blog-entry-body">
                                {entry.external_url && (
                                    <div className="profile-blog-view-toggle">
                                        <button className={`profile-blog-view-btn${!isIframeView(entry.id, true) ? ' active' : ''}`} onClick={() => setBlogIframeView(prev => ({ ...prev, [entry.id]: false }))}>
                                            {'\uD83D\uDCDD'} Text
                                        </button>
                                        <button className={`profile-blog-view-btn${isIframeView(entry.id, true) ? ' active' : ''}`} onClick={() => { setBlogIframeView(prev => ({ ...prev, [entry.id]: true })); setBlogIframeLoaded(prev => ({ ...prev, [entry.id]: 'loading' })); }}>
                                            {'\uD83C\uDF10'} View Inline
                                        </button>
                                        <a href="#" className="profile-blog-external-link" onClick={(e) => { e.preventDefault(); openInApp(entry.external_url!, entry.title); }} style={{ marginLeft: 'auto' } as React.CSSProperties}>
                                            {'\uD83D\uDD17'} Open in browser
                                        </a>
                                    </div>
                                )}
                                {entry.external_url && isIframeView(entry.id, true) ? (
                                    <div className="profile-blog-iframe-container">
                                        {blogIframeLoaded[entry.id] === 'loading' && <div className="profile-blog-iframe-loading">Loading post...</div>}
                                        {blogIframeLoaded[entry.id] === 'error' && (
                                            <div className="profile-blog-iframe-error">
                                                <p>This site doesn't allow embedded viewing.</p>
                                                {entry.body ? entry.body.split('\n').map((line, i) => <p key={i}>{line || '\u00A0'}</p>) : null}
                                            </div>
                                        )}
                                        <iframe src={entry.external_url} className="profile-blog-iframe" sandbox="allow-scripts allow-same-origin allow-popups" referrerPolicy="no-referrer" style={{ display: blogIframeLoaded[entry.id] === 'error' ? 'none' : 'block' } as React.CSSProperties} onLoad={() => setBlogIframeLoaded(prev => ({ ...prev, [entry.id]: 'loaded' }))} onError={() => setBlogIframeLoaded(prev => ({ ...prev, [entry.id]: 'error' }))} title={entry.title} />
                                    </div>
                                ) : (
                                    entry.body ? entry.body.split('\n').map((line, i) => <p key={i}>{line || '\u00A0'}</p>) : (
                                        !entry.external_url ? <p style={{ color: 'var(--text-muted)', fontStyle: 'italic' } as React.CSSProperties}>No content preview available.</p> : null
                                    )
                                )}
                                {entry.external_url && !isIframeView(entry.id, true) && !entry.body && (
                                    <a href="#" className="profile-blog-external-link" onClick={(e) => { e.preventDefault(); openInApp(entry.external_url!, entry.title); }}>
                                        {'\uD83D\uDD17'} Read full post
                                    </a>
                                )}
                                {editMode && !entry.from_feed && (
                                    <div className="profile-blog-entry-actions">
                                        <button onClick={() => { setBlogTitle(entry.title); setBlogBody(entry.body); setBlogUrl(entry.external_url || ''); setEditingBlogId(entry.id); }}>{'\u270F\uFE0F'} Edit</button>
                                        <button className="profile-module-edit-btn--danger" onClick={() => { saveBlogData({ ...data, entries: entries.filter(e => e.id !== entry.id) }); }}>{'\uD83D\uDDD1'} Delete</button>
                                    </div>
                                )}
                                {editMode && entry.from_feed && (
                                    <div className="profile-blog-entry-actions">
                                        <button className="profile-module-edit-btn--danger" onClick={() => { saveBlogData({ ...data, entries: entries.filter(e => e.id !== entry.id) }); }}>{'\uD83D\uDDD1'} Remove</button>
                                    </div>
                                )}
                            </div>
                        )}
                    </div>
                ))}
            </div>
        </div>
    );
}
