import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { BsChatDotsFill, BsPersonFill } from 'react-icons/bs';
import { usePeerStore, useAuthStore } from '../store';
import { openUrl } from '@tauri-apps/plugin-opener';
import { PeerInfo, ProfileView } from '../types';

interface FriendsPanelProps {
    onOpenChat?: (peerNodeId: string, peerName: string) => void;
    onViewProfile?: (peerNodeId: string, peerName?: string) => void;
}

/** Status-aware away message based on last-seen timestamp */
function getAwayMessage(lastSeen: number | null): string {
    if (!lastSeen) return 'Away';
    const secs = Math.floor(Date.now() / 1000) - lastSeen;
    if (secs < 60) return 'Just left';
    if (secs < 300) return `Away ${Math.floor(secs / 60)}m`;
    if (secs < 3600) return `Away ${Math.floor(secs / 60)}m`;
    if (secs < 86400) return `Away ${Math.floor(secs / 3600)}h`;
    if (secs < 604800) return `Away ${Math.floor(secs / 86400)}d`;
    return 'Offline';
}

/** Get status indicator color based on how long ago the peer was seen */
function getStatusIndicator(status: 'online' | 'offline', lastSeen: number | null): { color: string } {
    if (status === 'online') return { color: '#50fa7b' };
    if (!lastSeen) return { color: '#666' };
    const secs = Math.floor(Date.now() / 1000) - lastSeen;
    if (secs < 300) return { color: '#ffb86c' };
    if (secs < 3600) return { color: '#ff9944' };
    if (secs < 86400) return { color: '#ff5555' };
    return { color: '#666' };
}

/** Unified friend entry — can be a P2P peer or a Bluesky follow */
interface UnifiedFriend {
    id: string;
    displayName: string;
    source: 'p2p' | 'bluesky';
    /** P2P: online/offline based on connectivity. Bluesky: always 'cloud'. */
    status: 'online' | 'offline' | 'cloud';
    avatar?: string;
    handle?: string;
    lastSeen?: number | null;
    nodeId?: string;
    did?: string;
}

export function FriendsPanel({ onOpenChat, onViewProfile }: FriendsPanelProps) {
    const globalStatuses = usePeerStore(s => s.peerStatuses);
    const auth = useAuthStore(s => s.auth);
    const [peers, setPeers] = useState<PeerInfo[]>([]);
    const [blueskyFollows, setBlueskyFollows] = useState<ProfileView[]>([]);
    const [loading, setLoading] = useState(true);
    const [activeSection, setActiveSection] = useState<'all' | 'p2p' | 'bluesky'>('all');

    // Load P2P peers
    const loadPeers = useCallback(async () => {
        try {
            const peerList = await invoke<PeerInfo[]>('list_peers');
            setPeers(peerList);
        } catch {
            // P2P not ready yet
        }
    }, []);

    // Load Bluesky follows
    const loadBlueskyFollows = useCallback(async () => {
        if (!auth?.did) return;
        try {
            const follows = await invoke<ProfileView[]>('list_follows', { actor: auth.did });
            setBlueskyFollows(follows);
        } catch {
            // Bluesky not connected or API error
            try {
                // Fallback: use cached contacts table
                const contacts = await invoke<{ did: string; handle: string }[]>('list_contacts');
                setBlueskyFollows(contacts.map(c => ({ did: c.did, handle: c.handle })));
            } catch {
                // No contacts either
            }
        }
    }, [auth?.did]);

    useEffect(() => {
        Promise.all([loadPeers(), loadBlueskyFollows()]).finally(() => setLoading(false));
        const interval = setInterval(() => {
            loadPeers();
            // Refresh bluesky follows less frequently (every 5 min)
        }, 60_000);
        const bskyInterval = setInterval(loadBlueskyFollows, 300_000);
        return () => { clearInterval(interval); clearInterval(bskyInterval); };
    }, [loadPeers, loadBlueskyFollows]);

    // Build unified friend list
    const p2pFriends: UnifiedFriend[] = peers.map(peer => ({
        id: `p2p:${peer.node_id}`,
        displayName: peer.display_name || `Peer ${peer.node_id.slice(0, 8)}`,
        source: 'p2p' as const,
        status: globalStatuses[peer.node_id] === 'online' ? 'online' as const : 'offline' as const,
        lastSeen: peer.last_seen,
        nodeId: peer.node_id,
    }));

    // Filter out Bluesky follows that are also P2P peers (by DID match)
    const p2pDids = new Set(peers.map(p => p.did).filter(Boolean));
    const bskyFriends: UnifiedFriend[] = blueskyFollows
        .filter(f => !p2pDids.has(f.did)) // Don't duplicate if already a P2P peer
        .map(f => ({
            id: `bsky:${f.did}`,
            displayName: f.displayName || f.handle,
            source: 'bluesky' as const,
            status: 'cloud' as const,
            avatar: f.avatar,
            handle: f.handle,
            did: f.did,
        }));

    const onlineP2p = p2pFriends.filter(f => f.status === 'online');
    const offlineP2p = p2pFriends.filter(f => f.status !== 'online');
    const totalCount = p2pFriends.length + bskyFriends.length;

    return (
        <div className="friends-panel">
            <div className="friends-panel-header">
                <span className="friends-panel-title">Friends</span>
                <span className="friends-panel-count">
                    {onlineP2p.length > 0 && (
                        <span className="friends-online-badge">{onlineP2p.length}</span>
                    )}
                    {totalCount}
                </span>
            </div>

            {/* Source filter tabs */}
            {(p2pFriends.length > 0 || bskyFriends.length > 0) && (
                <div className="friends-source-tabs">
                    <button
                        className={`friends-source-tab ${activeSection === 'all' ? 'active' : ''}`}
                        onClick={() => setActiveSection('all')}
                    >
                        All
                    </button>
                    {p2pFriends.length > 0 && (
                        <button
                            className={`friends-source-tab ${activeSection === 'p2p' ? 'active' : ''}`}
                            onClick={() => setActiveSection('p2p')}
                        >
                            🔗 P2P
                        </button>
                    )}
                    {bskyFriends.length > 0 && (
                        <button
                            className={`friends-source-tab ${activeSection === 'bluesky' ? 'active' : ''}`}
                            onClick={() => setActiveSection('bluesky')}
                        >
                            🦋 Bsky
                        </button>
                    )}
                </div>
            )}

            <div className="friends-panel-list">
                {loading && (
                    <div className="friends-panel-empty">Loading…</div>
                )}

                {!loading && totalCount === 0 && (
                    <div className="friends-panel-empty">
                        <div style={{ fontSize: '1.4rem', marginBottom: 6 } as React.CSSProperties}>👤</div>
                        <div style={{ fontWeight: 700 } as React.CSSProperties}>No friends yet</div>
                        <div style={{ fontSize: '0.65rem', marginTop: 4, opacity: 0.7 } as React.CSSProperties}>
                            Follow people on Bluesky or add P2P peers
                        </div>
                    </div>
                )}

                {/* P2P Online peers */}
                {(activeSection === 'all' || activeSection === 'p2p') && onlineP2p.length > 0 && (
                    <div className="friends-panel-group">
                        <div className="friends-panel-group-label">
                            <span className="friends-group-source-icon">🔗</span>
                            Online ({onlineP2p.length})
                        </div>
                        {onlineP2p.map(friend => (
                            <FriendItem
                                key={friend.id}
                                friend={friend}
                                onChat={onOpenChat}
                                onProfile={onViewProfile}
                            />
                        ))}
                    </div>
                )}

                {/* P2P Offline/Away peers */}
                {(activeSection === 'all' || activeSection === 'p2p') && offlineP2p.length > 0 && (
                    <div className="friends-panel-group">
                        <div className="friends-panel-group-label">
                            <span className="friends-group-source-icon">🔗</span>
                            Away ({offlineP2p.length})
                        </div>
                        {offlineP2p.map(friend => (
                            <FriendItem
                                key={friend.id}
                                friend={friend}
                                onChat={onOpenChat}
                                onProfile={onViewProfile}
                            />
                        ))}
                    </div>
                )}

                {/* Bluesky follows */}
                {(activeSection === 'all' || activeSection === 'bluesky') && bskyFriends.length > 0 && (
                    <div className="friends-panel-group">
                        <div className="friends-panel-group-label">
                            <span className="friends-group-source-icon">🦋</span>
                            Bluesky ({bskyFriends.length})
                        </div>
                        {bskyFriends.map(friend => (
                            <FriendItem
                                key={friend.id}
                                friend={friend}
                                onChat={onOpenChat}
                                onProfile={onViewProfile}
                            />
                        ))}
                    </div>
                )}
            </div>
        </div>
    );
}

interface FriendItemProps {
    friend: UnifiedFriend;
    onChat?: (nodeId: string, name: string) => void;
    onProfile?: (nodeId: string, name?: string) => void;
}

function FriendItem({ friend, onChat, onProfile }: FriendItemProps) {
    const initial = friend.displayName[0]?.toUpperCase() || '?';
    const isP2p = friend.source === 'p2p';
    const isBsky = friend.source === 'bluesky';

    // Status display
    let statusText: string;
    let statusClass: string;
    let dotColor: string;

    if (friend.status === 'online') {
        statusText = 'Online';
        statusClass = 'online';
        dotColor = '#50fa7b';
    } else if (friend.status === 'cloud') {
        statusText = friend.handle ? `@${friend.handle}` : 'Bluesky';
        statusClass = 'cloud';
        dotColor = '#1185fe'; // Bluesky blue
    } else {
        const indicator = getStatusIndicator('offline', friend.lastSeen ?? null);
        statusText = getAwayMessage(friend.lastSeen ?? null);
        statusClass = 'offline';
        dotColor = indicator.color;
    }

    const handleClick = () => {
        if (isP2p && friend.nodeId && onProfile) {
            onProfile(friend.nodeId, friend.displayName);
        } else if (isBsky && friend.handle) {
            // Open Bluesky profile externally
            openUrl(`https://bsky.app/profile/${friend.handle}`).catch(console.error);
        }
    };

    const handleChat = () => {
        if (isP2p && friend.nodeId && onChat) {
            onChat(friend.nodeId, friend.displayName);
        } else if (isBsky && friend.did && onChat) {
            // Open Bluesky DM via the chat screen with DID identifier
            onChat(friend.did, friend.displayName);
        }
    };

    return (
        <div className={`friend-item friend-item--${friend.source}`}>
            <button
                className="friend-item-main"
                onClick={handleClick}
                title={isP2p ? `View ${friend.displayName}'s profile` : isBsky ? `View @${friend.handle || friend.displayName} on Bluesky` : friend.displayName}
                style={{ cursor: 'pointer' } as React.CSSProperties}
            >
                <div className="friend-item-avatar">
                    {friend.avatar ? (
                        <img
                            src={friend.avatar}
                            alt=""
                            className="friend-item-avatar-img"
                        />
                    ) : (
                        <span className="friend-item-initial">{initial}</span>
                    )}
                    <span
                        className={`friend-item-dot ${statusClass}`}
                        style={{ background: dotColor } as React.CSSProperties}
                    />
                </div>
                <div className="friend-item-info">
                    <div className="friend-item-name">
                        {friend.displayName}
                    </div>
                    <div className={`friend-item-away ${statusClass}`}>
                        {statusText}
                    </div>
                </div>
            </button>
            <div className="friend-item-actions">
                {onChat && (friend.nodeId || friend.did) && (
                    <button
                        className="friend-item-action-btn"
                        onClick={handleChat}
                        title={`Message ${friend.displayName}`}
                    >
                        <BsChatDotsFill size={11} />
                    </button>
                )}
                <button
                    className="friend-item-action-btn"
                    onClick={handleClick}
                    title={`${friend.displayName}'s profile`}
                >
                    <BsPersonFill size={11} />
                </button>
            </div>
        </div>
    );
}
