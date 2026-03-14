import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { usePeerStore } from '../store';
import { PeerInfo } from '../types';

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

/** Get status icon based on how long ago the peer was seen */
function getStatusIndicator(status: 'online' | 'offline', lastSeen: number | null): { dot: string; color: string } {
    if (status === 'online') return { dot: '●', color: '#50fa7b' };
    if (!lastSeen) return { dot: '●', color: '#666' };
    const secs = Math.floor(Date.now() / 1000) - lastSeen;
    if (secs < 300) return { dot: '●', color: '#ffb86c' };   // idle/just left
    if (secs < 3600) return { dot: '●', color: '#ff9944' };  // away
    if (secs < 86400) return { dot: '●', color: '#ff5555' }; // long away
    return { dot: '○', color: '#666' };                      // offline
}

export function FriendsPanel({ onOpenChat, onViewProfile }: FriendsPanelProps) {
    const globalStatuses = usePeerStore(s => s.peerStatuses);
    const [peers, setPeers] = useState<PeerInfo[]>([]);
    const [loading, setLoading] = useState(true);

    const loadPeers = useCallback(async () => {
        try {
            const peerList = await invoke<PeerInfo[]>('list_peers');
            setPeers(peerList);
        } catch {
            // P2P not ready yet
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => {
        loadPeers();
        const interval = setInterval(loadPeers, 60_000);
        return () => clearInterval(interval);
    }, [loadPeers]);

    const onlinePeers = peers.filter(p => globalStatuses[p.node_id] === 'online');
    const offlinePeers = peers.filter(p => globalStatuses[p.node_id] !== 'online');

    return (
        <div className="friends-panel">
            <div className="friends-panel-header">
                <span className="friends-panel-title">Friends</span>
                <span className="friends-panel-count">
                    {onlinePeers.length > 0 && (
                        <span className="friends-online-badge">{onlinePeers.length}</span>
                    )}
                    {peers.length}
                </span>
            </div>

            <div className="friends-panel-list">
                {loading && (
                    <div className="friends-panel-empty">Loading…</div>
                )}

                {!loading && peers.length === 0 && (
                    <div className="friends-panel-empty">
                        <div style={{ fontSize: '1.4rem', marginBottom: 6 } as React.CSSProperties}>👤</div>
                        <div style={{ fontWeight: 700 } as React.CSSProperties}>No peers yet</div>
                        <div style={{ fontSize: '0.65rem', marginTop: 4, opacity: 0.7 } as React.CSSProperties}>
                            Go to Social to add friends
                        </div>
                    </div>
                )}

                {/* Online peers */}
                {onlinePeers.length > 0 && (
                    <div className="friends-panel-group">
                        <div className="friends-panel-group-label">Online ({onlinePeers.length})</div>
                        {onlinePeers.map(peer => (
                            <FriendItem
                                key={peer.node_id}
                                peer={peer}
                                status="online"
                                onChat={onOpenChat}
                                onProfile={onViewProfile}
                            />
                        ))}
                    </div>
                )}

                {/* Offline / Away peers */}
                {offlinePeers.length > 0 && (
                    <div className="friends-panel-group">
                        <div className="friends-panel-group-label">Away ({offlinePeers.length})</div>
                        {offlinePeers.map(peer => (
                            <FriendItem
                                key={peer.node_id}
                                peer={peer}
                                status="offline"
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
    peer: PeerInfo;
    status: 'online' | 'offline';
    onChat?: (nodeId: string, name: string) => void;
    onProfile?: (nodeId: string, name?: string) => void;
}

function FriendItem({ peer, status, onChat, onProfile }: FriendItemProps) {
    const displayName = peer.display_name || `Peer ${peer.node_id.slice(0, 8)}`;
    const initial = displayName[0]?.toUpperCase() || '?';
    const indicator = getStatusIndicator(status, peer.last_seen);
    const awayMsg = status === 'offline' ? getAwayMessage(peer.last_seen) : 'Online';

    return (
        <div className="friend-item">
            <button
                className="friend-item-main"
                onClick={() => onProfile?.(peer.node_id, peer.display_name || undefined)}
                title={`View ${displayName}'s profile`}
            >
                <div className="friend-item-avatar">
                    <span className="friend-item-initial">{initial}</span>
                    <span
                        className={`friend-item-dot ${status}`}
                        style={{ background: indicator.color } as React.CSSProperties}
                    />
                </div>
                <div className="friend-item-info">
                    <div className="friend-item-name">{displayName}</div>
                    <div className={`friend-item-away ${status}`}>
                        {awayMsg}
                    </div>
                </div>
            </button>
            {onChat && (
                <button
                    className="friend-item-chat-btn"
                    onClick={() => onChat(peer.node_id, displayName)}
                    title={`Chat with ${displayName}`}
                >
                    💬
                </button>
            )}
        </div>
    );
}
