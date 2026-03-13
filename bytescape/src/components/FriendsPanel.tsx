import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { usePeerStore } from '../store';
import { PeerInfo } from '../types';

interface FriendsPanelProps {
    onOpenChat?: (peerNodeId: string, peerName: string) => void;
    onViewProfile?: (peerNodeId: string, peerName?: string) => void;
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
        // Refresh peer list every 60 seconds
        const interval = setInterval(loadPeers, 60_000);
        return () => clearInterval(interval);
    }, [loadPeers]);

    const formatLastSeen = (ts: number | null) => {
        if (!ts) return '';
        const secs = Math.floor(Date.now() / 1000) - ts;
        if (secs < 60) return 'just now';
        if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
        if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
        return `${Math.floor(secs / 86400)}d ago`;
    };

    const onlinePeers = peers.filter(p => globalStatuses[p.node_id] === 'online');
    const offlinePeers = peers.filter(p => globalStatuses[p.node_id] !== 'online');

    return (
        <div className="friends-panel">
            <div className="friends-panel-header">
                <span className="friends-panel-title">Friends</span>
                <span className="friends-panel-count">{onlinePeers.length}/{peers.length}</span>
            </div>

            <div className="friends-panel-list">
                {loading && (
                    <div className="friends-panel-empty">Loading…</div>
                )}

                {!loading && peers.length === 0 && (
                    <div className="friends-panel-empty">
                        <div style={{ fontSize: '1.2rem', marginBottom: 4 }}>👤</div>
                        <div>No peers added yet</div>
                        <div style={{ fontSize: '0.7rem', marginTop: 4, opacity: 0.7 }}>
                            Go to Social to add friends
                        </div>
                    </div>
                )}

                {/* Online peers first */}
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

                {/* Offline peers */}
                {offlinePeers.length > 0 && (
                    <div className="friends-panel-group">
                        <div className="friends-panel-group-label">Offline ({offlinePeers.length})</div>
                        {offlinePeers.map(peer => (
                            <FriendItem
                                key={peer.node_id}
                                peer={peer}
                                status="offline"
                                lastSeen={formatLastSeen(peer.last_seen)}
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
    lastSeen?: string;
    awayMessage?: string;
    onChat?: (nodeId: string, name: string) => void;
    onProfile?: (nodeId: string, name?: string) => void;
}

function FriendItem({ peer, status, lastSeen, onChat, onProfile }: FriendItemProps) {
    const displayName = peer.display_name || `Peer ${peer.node_id.slice(0, 8)}`;
    const initial = displayName[0]?.toUpperCase() || '?';

    return (
        <div className="friend-item">
            <button
                className="friend-item-main"
                onClick={() => onProfile?.(peer.node_id, peer.display_name || undefined)}
                title={`View ${displayName}'s profile`}
            >
                <div className="friend-item-avatar">
                    <span className="friend-item-initial">{initial}</span>
                    <span className={`friend-item-dot ${status}`} />
                </div>
                <div className="friend-item-info">
                    <div className="friend-item-name">{displayName}</div>
                    {status === 'offline' && lastSeen && (
                        <div className="friend-item-away">
                            Last seen {lastSeen}
                        </div>
                    )}
                    {status === 'online' && (
                        <div className="friend-item-status-text">Online</div>
                    )}
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
