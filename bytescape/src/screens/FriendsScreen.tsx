import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { AuthStatus, ProfileView, MyPeerInfo, PeerInfo, PeerConnectResult } from '../types';
import { usePeerStore, useNavStore } from '../store';
import { openUrl } from '@tauri-apps/plugin-opener';

interface FriendsScreenProps {
    auth: AuthStatus | null;
    onOpenChat?: (peerNodeId: string, peerName: string) => void;
    onViewProfile?: (peerNodeId: string, peerName?: string) => void;
}

export function FriendsScreen({ auth, onOpenChat, onViewProfile }: FriendsScreenProps) {
    const [subview, setSubview] = useState<'following' | 'followers' | 'peers'>('following');

    return (
        <div className="friends-screen">
            <div className="friends-sidebar">
                <h3>Social</h3>
                {auth?.source === 'bluesky' && (
                    <>
                        <button
                            className={`nav-item ${subview === 'following' ? 'active' : ''}`}
                            onClick={() => setSubview('following')}
                        >
                            👥 Following
                        </button>
                        <button
                            className={`nav-item ${subview === 'followers' ? 'active' : ''}`}
                            onClick={() => setSubview('followers')}
                        >
                            👤 Followers
                        </button>
                    </>
                )}
                <button
                    className={`nav-item ${subview === 'peers' ? 'active' : ''}`}
                    onClick={() => setSubview('peers')}
                >
                    🌐 P2P Peers
                </button>
            </div>

            <div className="friends-content">
                {subview === 'peers' && <PeersView onOpenChat={onOpenChat} onViewProfile={onViewProfile} />}
                {(subview === 'following' || subview === 'followers') && (
                    <BlueskyFollowsView auth={auth} subview={subview} />
                )}
            </div>
        </div>
    );
}

// ─── P2P Peers View ───

function PeersView({ onOpenChat, onViewProfile }: { onOpenChat?: (peerNodeId: string, peerName: string) => void; onViewProfile?: (peerNodeId: string, peerName?: string) => void }) {
    const globalStatuses = usePeerStore(s => s.peerStatuses);
    const [myInfo, setMyInfo] = useState<MyPeerInfo | null>(null);
    const [peers, setPeers] = useState<PeerInfo[]>([]);
    const [localStatuses, setLocalStatuses] = useState<Record<string, 'checking' | 'online' | 'offline'>>({});
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    // Add peer form
    const [addNodeId, setAddNodeId] = useState('');
    const [addDisplayName, setAddDisplayName] = useState('');
    const [addError, setAddError] = useState<string | null>(null);
    const [adding, setAdding] = useState(false);
    const [copied, setCopied] = useState(false);

    const loadData = useCallback(async () => {
        setLoading(true);
        setError(null);
        try {
            const [info, peerList] = await Promise.all([
                invoke<MyPeerInfo>('get_my_peer_info'),
                invoke<PeerInfo[]>('list_peers'),
            ]);
            setMyInfo(info);
            setPeers(peerList);
        } catch (e: any) {
            console.error('Failed to load peer info:', e);
            setError(typeof e === 'string' ? e : 'Failed to load P2P info');
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => {
        loadData();
    }, [loadData]);

    // Listen for inbound peer connections (e.g. mobile connecting to desktop).
    // When the remote peer connects TO us, the backend emits "peer-connected"
    // with their node_id and profile. We update the status map immediately
    // and refresh the peer list to pick up their display_name/DID.
    useEffect(() => {
        const unlisten = listen<{ node_id: string; username?: string; online?: boolean }>(
            'peer-connected',
            (event) => {
                const { node_id } = event.payload;
                console.log('[P2P] Inbound connection from peer:', node_id, event.payload);
                setLocalStatuses(prev => ({ ...prev, [node_id]: 'online' }));
                usePeerStore.getState().setPeerStatus(node_id, 'online');
                // Refresh peer list to pick up updated display_name/DID/last_seen
                loadData();
            }
        );
        return () => { unlisten.then(fn => fn()); };
    }, [loadData]);

    const handleCopyNodeId = async () => {
        if (!myInfo) return;
        try {
            await navigator.clipboard.writeText(myInfo.node_id);
            setCopied(true);
            setTimeout(() => setCopied(false), 2000);
        } catch {
            // Fallback for environments where clipboard API isn't available
            const el = document.createElement('textarea');
            el.value = myInfo.node_id;
            document.body.appendChild(el);
            el.select();
            document.execCommand('copy');
            document.body.removeChild(el);
            setCopied(true);
            setTimeout(() => setCopied(false), 2000);
        }
    };

    const handleAddPeer = async () => {
        const trimmed = addNodeId.trim();
        if (!trimmed) {
            setAddError('Please enter a Node ID');
            return;
        }
        setAdding(true);
        setAddError(null);
        try {
            await invoke<PeerInfo>('add_peer', {
                nodeId: trimmed,
                displayName: addDisplayName.trim() || null,
            });
            setAddNodeId('');
            setAddDisplayName('');
            await loadData();
        } catch (e: any) {
            setAddError(typeof e === 'string' ? e : 'Failed to add peer');
        } finally {
            setAdding(false);
        }
    };

    const handleRemovePeer = async (nodeId: string) => {
        try {
            await invoke('remove_peer', { nodeId });
            setPeers(prev => prev.filter(p => p.node_id !== nodeId));
            setLocalStatuses(prev => {
                const next = { ...prev };
                delete next[nodeId];
                return next;
            });
        } catch (e: any) {
            console.error('Failed to remove peer:', e);
        }
    };

    const handleCheckConnectivity = async (nodeId: string) => {
        setLocalStatuses(prev => ({ ...prev, [nodeId]: 'checking' }));
        try {
            const result = await invoke<PeerConnectResult>('check_peer_connectivity', { nodeId });
            const s = result.online ? 'online' as const : 'offline' as const;
            setLocalStatuses(prev => ({ ...prev, [nodeId]: s }));
            usePeerStore.getState().setPeerStatus(nodeId, s);
            if (result.online) {
                // Refresh data to pick up updated display_name, DID, and last_seen
                await loadData();
            }
        } catch {
            setLocalStatuses(prev => ({ ...prev, [nodeId]: 'offline' }));
            usePeerStore.getState().setPeerStatus(nodeId, 'offline');
        }
    };

    const handleCheckAll = async () => {
        for (const peer of peers) {
            await handleCheckConnectivity(peer.node_id);
        }
    };

    // Merge global background statuses with local interactive statuses.
    // Local statuses take priority (they include 'checking' state).
    const peerStatuses: Record<string, 'checking' | 'online' | 'offline'> = { ...globalStatuses, ...localStatuses };

    const truncateId = (id: string, len = 12) =>
        id.length > len * 2 ? `${id.slice(0, len)}…${id.slice(-len)}` : id;

    const formatAge = (ts: number) => {
        const secs = Math.floor(Date.now() / 1000) - ts;
        if (secs < 60) return 'just now';
        if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
        if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
        return `${Math.floor(secs / 86400)}d ago`;
    };

    if (loading) {
        return <div className="loading-state">Initializing P2P network…</div>;
    }

    if (error) {
        return (
            <div className="error-state">
                <p>{error}</p>
                <button onClick={loadData}>Retry</button>
            </div>
        );
    }

    return (
        <div style={{ padding: '16px', maxWidth: 700, margin: '0 auto' }}>
            {/* ── Your Identity ── */}
            <div className="card" style={{ marginBottom: 16 }}>
                <h3 style={{ margin: '0 0 8px', fontSize: '1rem', color: 'var(--accent-purple)' }}>
                    📡 Your Identity
                </h3>
                {myInfo && (
                    <div style={{ fontSize: '0.9rem' }}>
                        <div style={{ marginBottom: 6 }}>
                            <strong>Username:</strong>{' '}
                            <span style={{ color: 'var(--text-primary)' }}>
                                {myInfo.username || '(not set)'}
                            </span>
                        </div>
                        <div style={{ marginBottom: 8 }}>
                            <strong>Node ID:</strong>{' '}
                            <code style={{
                                fontSize: '0.8rem',
                                background: 'var(--bg-tertiary, #1a1a2e)',
                                padding: '2px 6px',
                                borderRadius: 4,
                                wordBreak: 'break-all',
                                display: 'inline',
                            }}>
                                {truncateId(myInfo.node_id, 16)}
                            </code>
                            <button
                                onClick={handleCopyNodeId}
                                style={{
                                    marginLeft: 8,
                                    padding: '2px 10px',
                                    fontSize: '0.8rem',
                                    cursor: 'pointer',
                                    background: copied ? 'var(--accent-green, #50fa7b)' : 'var(--bg-secondary)',
                                    color: copied ? '#000' : 'var(--text-primary)',
                                    border: '1px solid var(--border-color, #333)',
                                    borderRadius: 4,
                                    transition: 'all 0.2s',
                                }}
                            >
                                {copied ? '✓ Copied!' : '📋 Copy'}
                            </button>
                        </div>
                        <p style={{
                            margin: 0,
                            fontSize: '0.8rem',
                            color: 'var(--text-muted)',
                            fontStyle: 'italic',
                        }}>
                            Share your Node ID with a peer so they can connect to you.
                        </p>
                    </div>
                )}
            </div>

            {/* ── Browse Profiles ── */}
            <div className="card" style={{ marginBottom: 16 }}>
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                    <h3 style={{ margin: 0, fontSize: '1rem', color: 'var(--accent-purple)' }}>
                        🌐 Profile Browser
                    </h3>
                    <button
                        onClick={() => {
                            const { setScreen } = useNavStore.getState();
                            setScreen('profile-browser');
                        }}
                        style={{
                            padding: '6px 16px',
                            fontSize: '0.85rem',
                            cursor: 'pointer',
                            background: 'var(--accent-purple)',
                            color: '#fff',
                            border: 'none',
                            borderRadius: 4,
                        }}
                    >
                        Browse Profiles
                    </button>
                </div>
                <p style={{
                    margin: '8px 0 0',
                    fontSize: '0.8rem',
                    color: 'var(--text-muted)',
                    fontStyle: 'italic',
                }}>
                    Visit peer profiles with a browser-like interface. Navigate between peers and discover connections.
                </p>
            </div>

            {/* ── Add Peer ── */}
            <div className="card" style={{ marginBottom: 16 }}>
                <h3 style={{ margin: '0 0 8px', fontSize: '1rem', color: 'var(--accent-purple)' }}>
                    🔗 Add Peer
                </h3>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                    <input
                        type="text"
                        placeholder="Paste peer's Node ID"
                        value={addNodeId}
                        onChange={e => { setAddNodeId(e.target.value); setAddError(null); }}
                        onKeyDown={e => e.key === 'Enter' && handleAddPeer()}
                        style={{
                            padding: '8px 12px',
                            fontSize: '0.85rem',
                            background: 'var(--bg-primary)',
                            color: 'var(--text-primary)',
                            border: '1px solid var(--border-color, #333)',
                            borderRadius: 4,
                            fontFamily: 'monospace',
                        }}
                    />
                    <input
                        type="text"
                        placeholder="Display name (optional)"
                        value={addDisplayName}
                        onChange={e => setAddDisplayName(e.target.value)}
                        onKeyDown={e => e.key === 'Enter' && handleAddPeer()}
                        style={{
                            padding: '8px 12px',
                            fontSize: '0.85rem',
                            background: 'var(--bg-primary)',
                            color: 'var(--text-primary)',
                            border: '1px solid var(--border-color, #333)',
                            borderRadius: 4,
                        }}
                    />
                    <button
                        onClick={handleAddPeer}
                        disabled={adding || !addNodeId.trim()}
                        style={{
                            padding: '8px 16px',
                            fontSize: '0.9rem',
                            background: 'var(--accent-purple)',
                            color: '#fff',
                            border: 'none',
                            borderRadius: 4,
                            cursor: adding ? 'wait' : 'pointer',
                            opacity: adding || !addNodeId.trim() ? 0.5 : 1,
                        }}
                    >
                        {adding ? 'Adding…' : '➕ Follow Peer'}
                    </button>
                    {addError && (
                        <div style={{ color: '#ff6b6b', fontSize: '0.8rem' }}>{addError}</div>
                    )}
                </div>
            </div>

            {/* ── Peer List ── */}
            <div style={{ marginBottom: 8, display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                <h3 style={{ margin: 0, fontSize: '1rem', color: 'var(--accent-purple)' }}>
                    👥 Peers ({peers.length})
                </h3>
                {peers.length > 0 && (
                    <button
                        onClick={handleCheckAll}
                        style={{
                            padding: '4px 12px',
                            fontSize: '0.8rem',
                            background: 'var(--bg-secondary)',
                            color: 'var(--text-primary)',
                            border: '1px solid var(--border-color, #333)',
                            borderRadius: 4,
                            cursor: 'pointer',
                        }}
                    >
                        🔄 Check All
                    </button>
                )}
            </div>

            {peers.length === 0 ? (
                <div className="card" style={{ textAlign: 'center', padding: 32, color: 'var(--text-muted)' }}>
                    <div style={{ fontSize: '2rem', marginBottom: 8 }}>🔍</div>
                    <p>No peers yet. Add someone by pasting their Node ID above.</p>
                    <p style={{ fontSize: '0.8rem', marginTop: 4 }}>
                        Share your Node ID with a friend and have them share theirs with you.
                    </p>
                </div>
            ) : (
                <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                    {peers.map(peer => {
                        const status = peerStatuses[peer.node_id];
                        return (
                            <div
                                key={peer.node_id}
                                className="card"
                                style={{
                                    padding: '12px 16px',
                                    display: 'flex',
                                    alignItems: 'center',
                                    gap: 12,
                                }}
                            >
                                {/* Status indicator */}
                                <div style={{
                                    width: 10,
                                    height: 10,
                                    borderRadius: '50%',
                                    background: status === 'online' ? '#50fa7b'
                                        : status === 'offline' ? '#ff5555'
                                        : status === 'checking' ? '#f1fa8c'
                                        : '#555',
                                    boxShadow: status === 'online' ? '0 0 6px #50fa7b' : 'none',
                                    flexShrink: 0,
                                    animation: status === 'checking' ? 'pulse 1s infinite' : 'none',
                                }} />

                                {/* Peer info */}
                                <div style={{ flex: 1, minWidth: 0 }}>
                                    <div style={{ fontWeight: 600, fontSize: '0.95rem', display: 'flex', alignItems: 'center', gap: 6 }}>
                                        <span
                                            onClick={() => onOpenChat?.(peer.node_id, peer.display_name || truncateId(peer.node_id))}
                                            title="Open chat"
                                            style={{
                                                cursor: onOpenChat ? 'pointer' : 'default',
                                                color: 'var(--text-primary)',
                                                textDecoration: 'none',
                                                borderBottom: onOpenChat ? '1px dashed var(--accent-purple)' : 'none',
                                                transition: 'color 0.15s',
                                            }}
                                            onMouseEnter={e => { if (onOpenChat) (e.target as HTMLElement).style.color = 'var(--accent-purple)'; }}
                                            onMouseLeave={e => { if (onOpenChat) (e.target as HTMLElement).style.color = 'var(--text-primary)'; }}
                                        >
                                            {peer.display_name || truncateId(peer.node_id)}
                                        </span>
                                        {onOpenChat && (
                                            <span
                                                onClick={() => onOpenChat(peer.node_id, peer.display_name || truncateId(peer.node_id))}
                                                title="Open chat"
                                                style={{ cursor: 'pointer', fontSize: '0.8rem', opacity: 0.7 }}
                                            >💬</span>
                                        )}
                                        {status === 'online' && (
                                            <span style={{ fontSize: '0.75rem', color: '#50fa7b', fontWeight: 400 }}>
                                                ● online
                                            </span>
                                        )}
                                    </div>
                                    <div style={{
                                        fontSize: '0.75rem',
                                        color: 'var(--text-muted)',
                                        fontFamily: 'monospace',
                                        overflow: 'hidden',
                                        textOverflow: 'ellipsis',
                                        whiteSpace: 'nowrap',
                                    }}>
                                        {truncateId(peer.node_id, 10)}
                                    </div>
                                    {peer.did && peer.did !== 'local' && (
                                        <div style={{ fontSize: '0.7rem', color: 'var(--accent-purple)', marginTop: 1, opacity: 0.8 }}>
                                            DID: {truncateId(peer.did, 10)}
                                        </div>
                                    )}
                                    <div style={{ fontSize: '0.75rem', color: 'var(--text-muted)', marginTop: 2 }}>
                                        Added {formatAge(peer.added_at)}
                                        {peer.last_seen && ` · Last seen ${formatAge(peer.last_seen)}`}
                                    </div>
                                </div>

                                {/* Actions */}
                                <div style={{ display: 'flex', gap: 6, flexShrink: 0 }}>
                                    {onViewProfile && status === 'online' && (
                                        <button
                                            onClick={() => onViewProfile(peer.node_id, peer.display_name || undefined)}
                                            title="View profile & posts"
                                            style={{
                                                padding: '4px 8px',
                                                fontSize: '0.8rem',
                                                background: 'var(--accent-purple)',
                                                color: '#fff',
                                                border: 'none',
                                                borderRadius: 4,
                                                cursor: 'pointer',
                                            }}
                                        >
                                            👤 Profile
                                        </button>
                                    )}
                                    <button
                                        onClick={() => handleCheckConnectivity(peer.node_id)}
                                        disabled={status === 'checking'}
                                        title="Check connectivity"
                                        style={{
                                            padding: '4px 8px',
                                            fontSize: '0.8rem',
                                            background: 'var(--bg-secondary)',
                                            color: 'var(--text-primary)',
                                            border: '1px solid var(--border-color, #333)',
                                            borderRadius: 4,
                                            cursor: status === 'checking' ? 'wait' : 'pointer',
                                        }}
                                    >
                                        {status === 'checking' ? '⏳' : '📡'}
                                    </button>
                                    <button
                                        onClick={() => handleRemovePeer(peer.node_id)}
                                        title="Remove peer"
                                        style={{
                                            padding: '4px 8px',
                                            fontSize: '0.8rem',
                                            background: 'var(--bg-secondary)',
                                            color: '#ff5555',
                                            border: '1px solid var(--border-color, #333)',
                                            borderRadius: 4,
                                            cursor: 'pointer',
                                        }}
                                    >
                                        ✕
                                    </button>
                                </div>
                            </div>
                        );
                    })}
                </div>
            )}

            {/* Pulse animation for checking status */}
            <style>{`
                @keyframes pulse {
                    0%, 100% { opacity: 1; }
                    50% { opacity: 0.3; }
                }
            `}</style>
        </div>
    );
}

// ─── Bluesky Follows View (existing functionality) ───

function BlueskyFollowsView({ auth, subview }: { auth: AuthStatus | null; subview: 'following' | 'followers' }) {
    const [people, setPeople] = useState<ProfileView[]>([]);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        if (auth) fetchPeople();
    }, [auth, subview]);

    const fetchPeople = async () => {
        if (!auth) return;
        setLoading(true);
        setError(null);
        try {
            const result = subview === 'following'
                ? await invoke<ProfileView[]>('list_follows', { actor: auth.did })
                : await invoke<ProfileView[]>('list_followers', { actor: auth.did });
            setPeople(result);
        } catch (e: any) {
            console.error("Failed to fetch friends:", e);
            setError(typeof e === 'string' ? e : "Failed to load list");
        } finally {
            setLoading(false);
        }
    };

    return (
        <>
            <div className="friends-header">
                <h2>{subview === 'following' ? 'Following' : 'Followers'}</h2>
                <span className="count-badge">{people.length} people</span>
            </div>

            {loading && <div className="loading-state">Loading your network...</div>}
            {error && <div className="error-state">{error}</div>}

            <div className="friends-list">
                {people.map(person => (
                    <div
                        key={person.did}
                        className="friend-card"
                        onClick={() => {
                            if (person.handle) {
                                openUrl(`https://bsky.app/profile/${person.handle}`).catch(console.error);
                            }
                        }}
                        title={`View @${person.handle}'s profile`}
                    >
                        <div className="friend-avatar">
                            {person.avatar ? (
                                <img src={person.avatar} alt={person.handle} />
                            ) : (
                                <div className="avatar-placeholder">
                                    {person.handle.substring(0, 1).toUpperCase()}
                                </div>
                            )}
                        </div>
                        <div className="friend-info">
                            <div className="friend-name">
                                {person.displayName || person.handle}
                            </div>
                            <div className="friend-handle">@{person.handle}</div>
                            {person.description && (
                                <div className="friend-bio">{person.description}</div>
                            )}
                        </div>
                    </div>
                ))}

                {!loading && people.length === 0 && (
                    <div className="empty-state">
                        No one found in this list yet.
                    </div>
                )}
            </div>
        </>
    );
}
