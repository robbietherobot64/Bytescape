import { useState, useRef, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { invoke } from '@tauri-apps/api/core';
import { openInApp } from '../../utils/openInApp';

export interface ConnectionEntry {
    did: string;
    handle: string;
    source: string;
    avatar_url?: string;
    display_name?: string;
    profile_url?: string;
    relation: 'following' | 'follower' | 'mutual';
}

interface ConnectionsModuleProps {
    allConnections: ConnectionEntry[];
    setAllConnections: React.Dispatch<React.SetStateAction<ConnectionEntry[]>>;
    followingCount: number;
    setFollowingCount: React.Dispatch<React.SetStateAction<number>>;
    followerCount: number;
    authDid?: string;
}

export function ConnectionsModule({ allConnections, setAllConnections, followingCount, setFollowingCount, followerCount, authDid }: ConnectionsModuleProps) {
    const [showAll, setShowAll] = useState(false);
    const [search, setSearch] = useState('');
    const [sourceFilter, setSourceFilter] = useState('all');
    const [relationFilter, setRelationFilter] = useState<'all' | 'following' | 'follower'>('all');
    const [hoverCardIndex, setHoverCardIndex] = useState<number | null>(null);
    const [hoverCardStyle, setHoverCardStyle] = useState<React.CSSProperties>({});
    const [followLoading, setFollowLoading] = useState(false);
    const hoverCardRef = useRef<HTMLDivElement>(null);
    const hoverDismissTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
    const [hoverDropdownOpen, setHoverDropdownOpen] = useState(false);
    const [blockConfirm, setBlockConfirm] = useState<{ connection: ConnectionEntry; blockAll: boolean } | null>(null);
    const [blockLoading, setBlockLoading] = useState(false);

    useEffect(() => {
        if (hoverCardIndex === null) return;
        const dismiss = () => setHoverCardIndex(null);
        const handleClick = (e: MouseEvent) => {
            if (hoverCardRef.current && !hoverCardRef.current.contains(e.target as Node)) dismiss();
        };
        document.addEventListener('mousedown', handleClick);
        document.addEventListener('scroll', dismiss, true);
        return () => {
            document.removeEventListener('mousedown', handleClick);
            document.removeEventListener('scroll', dismiss, true);
        };
    }, [hoverCardIndex]);

    function showHoverCard(cardEl: HTMLElement, listIndex: number) {
        if (hoverDismissTimer.current) { clearTimeout(hoverDismissTimer.current); hoverDismissTimer.current = null; }
        const rect = cardEl.getBoundingClientRect();
        const cardW = 200;
        let left = rect.left + rect.width / 2 - cardW / 2;
        if (left < 8) left = 8;
        if (left + cardW > window.innerWidth - 8) left = window.innerWidth - 8 - cardW;
        let top = rect.bottom + 4;
        if (top + 140 > window.innerHeight) top = rect.top - 144;
        setHoverCardStyle({ position: 'fixed', left, top, width: cardW } as React.CSSProperties);
        setHoverCardIndex(listIndex);
        setHoverDropdownOpen(false);
    }

    function startHoverDismiss() {
        hoverDismissTimer.current = setTimeout(() => setHoverCardIndex(null), 200);
    }

    function cancelHoverDismiss() {
        if (hoverDismissTimer.current) { clearTimeout(hoverDismissTimer.current); hoverDismissTimer.current = null; }
    }

    async function handleFollow(c: ConnectionEntry) {
        setFollowLoading(true);
        try {
            if (c.source === 'Bluesky') {
                await invoke('bluesky_follow', { targetDid: c.did });
            } else if (c.source === 'Mastodon') {
                await invoke('mastodon_follow', { accountId: c.did });
            }
            setAllConnections(prev => prev.map(conn =>
                conn.did === c.did ? { ...conn, relation: conn.relation === 'follower' ? 'mutual' : 'following' } : conn
            ));
            setFollowingCount(prev => prev + 1);
        } catch (e) { console.error('Follow failed:', e); }
        setFollowLoading(false);
    }

    async function handleUnfollow(c: ConnectionEntry) {
        setFollowLoading(true);
        try {
            if (c.source === 'Bluesky') {
                await invoke('bluesky_unfollow', { followUri: `at://${authDid}/app.bsky.graph.follow/${c.did}` });
            } else if (c.source === 'Mastodon') {
                await invoke('mastodon_unfollow', { accountId: c.did });
            }
            setAllConnections(prev => prev.map(conn =>
                conn.did === c.did ? { ...conn, relation: conn.relation === 'mutual' ? 'follower' : 'follower' } : conn
            ));
            setFollowingCount(prev => Math.max(0, prev - 1));
        } catch (e) { console.error('Unfollow failed:', e); }
        setFollowLoading(false);
    }

    const sourceEmoji: Record<string, string> = { Bluesky: '🦋', P2P: '🔗', Mastodon: '🐘', ActivityPub: '📡' };
    const relationEmoji: Record<string, string> = { following: '→', follower: '←', mutual: '⇄' };
    const uniqueSources = [...new Set(allConnections.map(c => c.source))];
    const filtered = allConnections
        .filter(c => sourceFilter === 'all' || c.source === sourceFilter)
        .filter(c => relationFilter === 'all' || c.relation === relationFilter || (relationFilter === 'following' && c.relation === 'mutual') || (relationFilter === 'follower' && c.relation === 'mutual'))
        .filter(c => !search || c.handle.toLowerCase().includes(search.toLowerCase()));
    const displayList = showAll ? filtered : filtered.slice(0, 12);
    const totalConns = allConnections.length;

    return (
        <div>
            <div className="profile-connections-filters">
                <div className="profile-connections-filters-left">
                    <button
                        className={`profile-connections-filter-btn ${sourceFilter === 'all' && relationFilter === 'all' ? 'active' : ''}`}
                        onClick={() => { setSourceFilter('all'); setRelationFilter('all'); }}
                    >All ({totalConns})</button>
                    {uniqueSources.map(s => (
                        <button
                            key={s}
                            className={`profile-connections-filter-btn ${sourceFilter === s ? 'active' : ''}`}
                            onClick={() => { setSourceFilter(s); setRelationFilter('all'); }}
                        >{sourceEmoji[s] || '🌐'} {s}</button>
                    ))}
                </div>
                <div className="profile-connections-filters-right">
                    {followingCount > 0 && (
                        <button
                            className={`profile-connections-filter-btn ${relationFilter === 'following' ? 'active' : ''}`}
                            onClick={() => { setRelationFilter(relationFilter === 'following' ? 'all' : 'following'); setSourceFilter('all'); }}
                        >→ {followingCount} following</button>
                    )}
                    {followerCount > 0 && (
                        <button
                            className={`profile-connections-filter-btn ${relationFilter === 'follower' ? 'active' : ''}`}
                            onClick={() => { setRelationFilter(relationFilter === 'follower' ? 'all' : 'follower'); setSourceFilter('all'); }}
                        >← {followerCount} followers</button>
                    )}
                </div>
            </div>

            {showAll && (
                <input type="text" className="profile-connections-search" placeholder="Search connections..." value={search} onChange={(e) => setSearch(e.target.value)} />
            )}

            <div className="profile-connections-grid">
                {displayList.map((c, i) => (
                    <div
                        key={`${c.source}-${c.did}-${i}`}
                        className="profile-connection-wrapper"
                        onMouseEnter={(e) => showHoverCard(e.currentTarget as HTMLElement, i)}
                        onMouseLeave={() => startHoverDismiss()}
                    >
                        <div className={`profile-connection-card ${c.profile_url ? 'profile-connection-card--clickable' : ''}`}>
                            <div
                                className="profile-connection-avatar"
                                onClick={(e) => { if (c.profile_url) { e.stopPropagation(); openInApp(c.profile_url!, c.display_name || c.handle); } }}
                                style={c.profile_url ? { cursor: 'pointer' } as React.CSSProperties : undefined}
                            >
                                {c.avatar_url ? (
                                    <img src={c.avatar_url} alt="" className="profile-connection-avatar-img" onError={(e) => { (e.target as HTMLImageElement).style.display = 'none'; (e.target as HTMLImageElement).parentElement!.textContent = (c.display_name || c.handle)[0]?.toUpperCase() || '?'; }} />
                                ) : (
                                    c.handle[0]?.toUpperCase() || '?'
                                )}
                            </div>
                            <div className="profile-connection-name">{c.display_name || c.handle}</div>
                            <div className="profile-connection-meta">
                                <span className="profile-connection-platform-icon">{sourceEmoji[c.source] || '🌐'}</span>
                                <span className="profile-connection-relation">{relationEmoji[c.relation] || ''}</span>
                            </div>
                        </div>
                    </div>
                ))}
            </div>

            {hoverCardIndex !== null && displayList[hoverCardIndex] && createPortal(
                <div
                    className="profile-connection-hover-card"
                    style={hoverCardStyle}
                    ref={hoverCardRef}
                    onMouseEnter={() => cancelHoverDismiss()}
                    onMouseLeave={() => { if (!hoverDropdownOpen) startHoverDismiss(); }}
                    onClick={(e) => e.stopPropagation()}
                >
                    <div className="profile-connection-hover-name">{displayList[hoverCardIndex].display_name || displayList[hoverCardIndex].handle}</div>
                    <div className="profile-connection-hover-handle">@{displayList[hoverCardIndex].handle}</div>
                    <div className="profile-connection-hover-actions">
                        <div className="profile-connection-hover-split">
                            {(displayList[hoverCardIndex].relation === 'following' || displayList[hoverCardIndex].relation === 'mutual') ? (
                                <button
                                    className="profile-connection-hover-btn profile-connection-hover-btn--unfollow profile-connection-hover-split-main"
                                    onClick={() => handleUnfollow(displayList[hoverCardIndex!])}
                                    disabled={followLoading}
                                >
                                    {followLoading ? '...' : 'Unfollow'}
                                </button>
                            ) : (
                                <button
                                    className="profile-connection-hover-btn profile-connection-hover-btn--follow profile-connection-hover-split-main"
                                    onClick={() => handleFollow(displayList[hoverCardIndex!])}
                                    disabled={followLoading}
                                >
                                    {followLoading ? '...' : 'Follow'}
                                </button>
                            )}
                            <button
                                className="profile-connection-hover-btn profile-connection-hover-split-arrow"
                                onClick={() => setHoverDropdownOpen(!hoverDropdownOpen)}
                            >
                                ▾
                            </button>
                        </div>
                        {hoverDropdownOpen && (
                            <div className="profile-connection-hover-dropdown">
                                <button className="profile-connection-hover-dropdown-item" onClick={() => { setHoverDropdownOpen(false); }}>
                                    ⚙ Manage
                                </button>
                                <button className="profile-connection-hover-dropdown-item profile-connection-hover-dropdown-item--danger" onClick={() => {
                                    setHoverDropdownOpen(false);
                                    setBlockConfirm({ connection: displayList[hoverCardIndex!], blockAll: false });
                                }}>
                                    🚫 Block
                                </button>
                            </div>
                        )}
                    </div>
                </div>,
                document.body
            )}

            {blockConfirm && createPortal(
                <div className="profile-block-overlay" onClick={() => setBlockConfirm(null)}>
                    <div className="profile-block-modal" onClick={(e) => e.stopPropagation()}>
                        <div className="profile-block-modal-title">Block {blockConfirm.connection.display_name || blockConfirm.connection.handle}?</div>
                        <p className="profile-block-modal-text">
                            They won't be able to see your posts or contact you on {blockConfirm.connection.source}.
                        </p>
                        <label className="profile-block-modal-checkbox">
                            <input type="checkbox" checked={blockConfirm.blockAll} onChange={(e) => setBlockConfirm({ ...blockConfirm, blockAll: e.target.checked })} />
                            Block on all platforms
                        </label>
                        <div className="profile-block-modal-actions">
                            <button className="profile-block-modal-btn" onClick={() => setBlockConfirm(null)}>Cancel</button>
                            <button
                                className="profile-block-modal-btn profile-block-modal-btn--confirm"
                                disabled={blockLoading}
                                onClick={async () => {
                                    setBlockLoading(true);
                                    try {
                                        const c = blockConfirm.connection;
                                        if (c.relation === 'following' || c.relation === 'mutual') {
                                            try { await handleUnfollow(c); } catch { /* best effort */ }
                                        }
                                        if (c.source === 'Bluesky') {
                                            try { await invoke('bluesky_block', { targetDid: c.did }); } catch { /* not yet implemented */ }
                                        }
                                        setAllConnections(prev => prev.filter(conn => conn.did !== c.did));
                                    } catch (e) { console.error('Block failed:', e); }
                                    setBlockLoading(false);
                                    setBlockConfirm(null);
                                    setHoverCardIndex(null);
                                }}
                            >
                                {blockLoading ? '...' : 'Block'}
                            </button>
                        </div>
                    </div>
                </div>,
                document.body
            )}

            {totalConns > 12 && (
                <button
                    className="profile-connections-toggle"
                    onClick={() => { setShowAll(!showAll); if (showAll) setSearch(''); }}
                >
                    {showAll ? '▲ Show less' : `▼ Show all (${totalConns})`}
                </button>
            )}

            {totalConns === 0 && followingCount === 0 && followerCount === 0 && (
                <div className="profile-empty"><div className="profile-empty-icon">👥</div><p className="profile-empty-text">No connections yet. Follow people on Bluesky or Mastodon to see them here.</p></div>
            )}
        </div>
    );
}
