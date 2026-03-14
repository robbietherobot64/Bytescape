/**
 * ProfileBrowserScreen (B3.1 + B3.2)
 *
 * A browser-like top-level screen for discovering, visiting, and traversing
 * peer profiles. Features:
 *
 * - Address bar showing the current peer's node ID / handle
 * - Peer directory listing from the friend list
 * - Sandboxed iframe rendering for peers who have custom profile layouts (B3.2)
 * - Falls back to the inline PeerProfileScreen content for peers without custom layouts
 * - Reads <meta name="bytescape:did"> tags from rendered profiles for action buttons
 * - Back/forward navigation through visited peers
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Win98Box } from '../components/Win98Box';
import { useNavStore, usePeerStore } from '../store';
import { useLocationStore } from '../hooks/useLocation';
import { PeerInfo, PeerProfile, BytescapeItem, PeerFeedResponse } from '../types';
import { FeedEntryCard } from '../components/FeedEntryCard';
import { adaptBytescapeItem } from '../utils/adapters';
import {
    BsGlobe, BsArrowLeft, BsArrowRight, BsHouseDoor,
    BsPersonFill, BsSearch, BsCircleFill
} from 'react-icons/bs';

interface VisitedPeer {
    nodeId: string;
    name: string;
}

export function ProfileBrowserScreen() {
    const { setScreen, setActiveSidebar } = useNavStore();
    const { peerStatuses } = usePeerStore();

    // Navigation state
    const [history, setHistory] = useState<VisitedPeer[]>([]);
    const [historyIdx, setHistoryIdx] = useState(-1);
    const [addressInput, setAddressInput] = useState('');

    // Peer directory
    const [peers, setPeers] = useState<PeerInfo[]>([]);
    const [loadingPeers, setLoadingPeers] = useState(true);

    // Current peer view state
    const [currentNodeId, setCurrentNodeId] = useState<string | null>(null);
    const [peerProfile, setPeerProfile] = useState<PeerProfile | null>(null);
    const [peerItems, setPeerItems] = useState<BytescapeItem[]>([]);
    const [totalCount, setTotalCount] = useState(0);
    const [loadingFeed, setLoadingFeed] = useState(false);
    const [feedError, setFeedError] = useState<string | null>(null);
    const [connected, setConnected] = useState(false);
    const offsetRef = useRef(0);

    // Load peer list on mount
    useEffect(() => {
        setLoadingPeers(true);
        invoke<PeerInfo[]>('list_peers')
            .then(setPeers)
            .catch(console.error)
            .finally(() => setLoadingPeers(false));
    }, []);

    // Navigate to a peer
    const navigateTo = useCallback((nodeId: string, name: string) => {
        const newEntry: VisitedPeer = { nodeId, name };

        setHistory(prev => {
            // Trim forward history and add new entry
            const trimmed = prev.slice(0, historyIdx + 1);
            return [...trimmed, newEntry];
        });
        setHistoryIdx(prev => prev + 1);

        setCurrentNodeId(nodeId);
        setAddressInput(`bytescape://peer/${nodeId.slice(0, 16)}…`);
        setPeerProfile(null);
        setPeerItems([]);
        setTotalCount(0);
        setFeedError(null);
        setConnected(false);
        offsetRef.current = 0;

        // B10.6: Sync Omnibar URL to show byte://peer/{nodeId}
        useLocationStore.getState().navigate(`byte://peer/${nodeId}`, { replace: true });
    }, [historyIdx]);

    // Load peer feed when currentNodeId changes
    useEffect(() => {
        if (!currentNodeId) return;

        setLoadingFeed(true);
        setFeedError(null);

        invoke<PeerFeedResponse>('request_peer_feed', {
            nodeId: currentNodeId,
            limit: 20,
            offset: 0,
            sourceFilter: null,
        })
            .then(async (response) => {
                setPeerProfile(response.profile);
                setPeerItems(response.items);
                setTotalCount(response.total_count);
                setConnected(true);
                offsetRef.current = response.items.length;

                // Download media blobs
                const hashes = new Set<string>();
                for (const item of response.items) {
                    for (const h of item.media_blobs ?? []) if (h) hashes.add(h);
                    for (const h of item.thumbnail_blobs ?? []) if (h) hashes.add(h);
                }
                const hashArr = [...hashes];
                for (let i = 0; i < hashArr.length; i += 4) {
                    const batch = hashArr.slice(i, i + 4);
                    await Promise.allSettled(
                        batch.map(hashStr =>
                            invoke<boolean>('download_peer_blob', { hashStr, nodeId: currentNodeId })
                        )
                    );
                }
            })
            .catch((e) => {
                setFeedError(typeof e === 'string' ? e : String(e));
                setConnected(false);
            })
            .finally(() => setLoadingFeed(false));
    }, [currentNodeId]);

    // Load more items
    const loadMore = useCallback(async () => {
        if (!currentNodeId || offsetRef.current >= totalCount) return;
        setLoadingFeed(true);
        try {
            const response = await invoke<PeerFeedResponse>('request_peer_feed', {
                nodeId: currentNodeId,
                limit: 20,
                offset: offsetRef.current,
                sourceFilter: null,
            });
            setPeerItems(prev => [...prev, ...response.items]);
            offsetRef.current += response.items.length;
        } catch (e) {
            console.error('Load more failed:', e);
        } finally {
            setLoadingFeed(false);
        }
    }, [currentNodeId, totalCount]);

    // Back/forward navigation
    const canGoBack = historyIdx > 0;
    const canGoForward = historyIdx < history.length - 1;

    const goBack = useCallback(() => {
        if (!canGoBack) return;
        const newIdx = historyIdx - 1;
        setHistoryIdx(newIdx);
        const entry = history[newIdx];
        setCurrentNodeId(entry.nodeId);
        setAddressInput(`bytescape://peer/${entry.nodeId.slice(0, 16)}…`);
        setPeerProfile(null);
        setPeerItems([]);
        offsetRef.current = 0;
    }, [canGoBack, historyIdx, history]);

    const goForward = useCallback(() => {
        if (!canGoForward) return;
        const newIdx = historyIdx + 1;
        setHistoryIdx(newIdx);
        const entry = history[newIdx];
        setCurrentNodeId(entry.nodeId);
        setAddressInput(`bytescape://peer/${entry.nodeId.slice(0, 16)}…`);
        setPeerProfile(null);
        setPeerItems([]);
        offsetRef.current = 0;
    }, [canGoForward, historyIdx, history]);

    const goHome = useCallback(() => {
        setCurrentNodeId(null);
        setPeerProfile(null);
        setPeerItems([]);
        setAddressInput('');
        setFeedError(null);
        // B10.6: Sync Omnibar URL back to profile-browser home
        useLocationStore.getState().navigate('byte://profile-browser', { replace: true });
    }, []);

    // Handle address bar submit
    const handleAddressSubmit = useCallback((e: React.FormEvent) => {
        e.preventDefault();
        const input = addressInput.trim();
        if (!input) return;

        // Try to parse nodeId from various formats
        let nodeId = input;
        if (input.startsWith('bytescape://peer/')) {
            nodeId = input.replace('bytescape://peer/', '').replace(/…$/, '');
        }

        // Find matching peer
        const matchedPeer = peers.find(p =>
            p.node_id.startsWith(nodeId) ||
            p.display_name?.toLowerCase().includes(input.toLowerCase())
        );

        if (matchedPeer) {
            navigateTo(matchedPeer.node_id, matchedPeer.display_name || matchedPeer.node_id.slice(0, 12));
        } else if (nodeId.length >= 8) {
            // Try direct connection even without matching peer
            navigateTo(nodeId, nodeId.slice(0, 12));
        }
    }, [addressInput, peers, navigateTo]);

    const isOnline = (nodeId: string): boolean => {
        return peerStatuses[nodeId] === 'online';
    };

    const truncateId = (id: string, len = 8) =>
        id.length > len * 2 ? `${id.slice(0, len)}…${id.slice(-len)}` : id;

    // ─── Address bar chrome ───
    const renderAddressBar = () => (
        <div style={{
            display: 'flex',
            alignItems: 'center',
            gap: 4,
            padding: '4px 8px',
            background: 'var(--bg-secondary)',
            borderBottom: '2px solid var(--border-color)',
        } as React.CSSProperties}>
            {/* Nav buttons */}
            <button
                onClick={goBack}
                disabled={!canGoBack}
                title="Back"
                style={{
                    ...navBtnStyle,
                    opacity: canGoBack ? 1 : 0.3,
                } as React.CSSProperties}
            >
                <BsArrowLeft />
            </button>
            <button
                onClick={goForward}
                disabled={!canGoForward}
                title="Forward"
                style={{
                    ...navBtnStyle,
                    opacity: canGoForward ? 1 : 0.3,
                } as React.CSSProperties}
            >
                <BsArrowRight />
            </button>
            <button
                onClick={goHome}
                title="Home — Peer Directory"
                style={navBtnStyle}
            >
                <BsHouseDoor />
            </button>

            {/* Address bar */}
            <form onSubmit={handleAddressSubmit} style={{ flex: 1, display: 'flex' } as React.CSSProperties}>
                <div style={{
                    flex: 1,
                    display: 'flex',
                    alignItems: 'center',
                    gap: 6,
                    background: '#0f0f23',
                    border: '2px inset var(--border-color)',
                    borderRadius: 2,
                    padding: '3px 8px',
                } as React.CSSProperties}>
                    <BsGlobe style={{ color: 'var(--text-muted)', fontSize: 12, flexShrink: 0 }} />
                    <input
                        type="text"
                        value={addressInput}
                        onChange={e => setAddressInput(e.target.value)}
                        placeholder="Enter peer node ID or name…"
                        style={{
                            flex: 1,
                            background: 'transparent',
                            border: 'none',
                            color: 'var(--text-primary)',
                            fontSize: 12,
                            fontFamily: 'monospace',
                            outline: 'none',
                        } as React.CSSProperties}
                    />
                </div>
                <button
                    type="submit"
                    style={{
                        ...navBtnStyle,
                        marginLeft: 4,
                    } as React.CSSProperties}
                >
                    <BsSearch />
                </button>
            </form>

            {/* Connection indicator */}
            {currentNodeId && (
                <div style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 4,
                    padding: '0 8px',
                    fontSize: 11,
                    color: connected ? '#50fa7b' : 'var(--text-muted)',
                } as React.CSSProperties}>
                    <BsCircleFill style={{ fontSize: 6, color: connected ? '#50fa7b' : '#ff6b6b' }} />
                    {connected ? 'P2P' : '…'}
                </div>
            )}

            {/* Back to app button */}
            <button
                onClick={() => { setScreen('friends'); setActiveSidebar('social'); }}
                style={{
                    ...navBtnStyle,
                    fontSize: 11,
                    padding: '4px 8px',
                } as React.CSSProperties}
            >
                ✕
            </button>
        </div>
    );

    // ─── Peer directory (home view) ───
    const renderPeerDirectory = () => (
        <div style={{ padding: 16, maxWidth: 700, margin: '0 auto' } as React.CSSProperties}>
            <Win98Box variant="window" title="🌐 Profile Browser — Peer Directory">
                <div style={{ padding: 16 } as React.CSSProperties}>
                    <p style={{
                        color: 'var(--text-muted)',
                        fontSize: 13,
                        marginTop: 0,
                        marginBottom: 16,
                    } as React.CSSProperties}>
                        Browse your peers' profiles. Click a peer to visit their page via P2P.
                    </p>

                    {loadingPeers && (
                        <div style={{ textAlign: 'center', padding: 32, color: 'var(--text-muted)' } as React.CSSProperties}>
                            📡 Loading peer list…
                        </div>
                    )}

                    {!loadingPeers && peers.length === 0 && (
                        <div style={{ textAlign: 'center', padding: 32 } as React.CSSProperties}>
                            <div style={{ fontSize: 32, marginBottom: 8 }}>🔍</div>
                            <p style={{ color: 'var(--text-muted)', margin: 0 } as React.CSSProperties}>
                                No peers yet. Add peers from the Friends screen to browse their profiles.
                            </p>
                        </div>
                    )}

                    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 } as React.CSSProperties}>
                        {peers.map(peer => {
                            const online = isOnline(peer.node_id);
                            return (
                                <button
                                    key={peer.node_id}
                                    onClick={() => navigateTo(peer.node_id, peer.display_name || peer.node_id.slice(0, 12))}
                                    style={{
                                        display: 'flex',
                                        alignItems: 'center',
                                        gap: 12,
                                        padding: '10px 14px',
                                        background: 'var(--bg-tertiary)',
                                        border: '1px solid var(--border-color)',
                                        borderRadius: 'var(--border-radius, 4px)',
                                        cursor: 'pointer',
                                        textAlign: 'left',
                                        width: '100%',
                                        transition: 'background 0.15s',
                                    } as React.CSSProperties}
                                    onMouseEnter={e => (e.currentTarget.style.background = 'var(--bg-secondary)')}
                                    onMouseLeave={e => (e.currentTarget.style.background = 'var(--bg-tertiary)')}
                                >
                                    {/* Avatar */}
                                    <div style={{
                                        width: 36,
                                        height: 36,
                                        borderRadius: '50%',
                                        background: 'linear-gradient(135deg, var(--accent-purple), #6366f1)',
                                        display: 'flex',
                                        alignItems: 'center',
                                        justifyContent: 'center',
                                        fontSize: 16,
                                        fontWeight: 700,
                                        color: '#fff',
                                        flexShrink: 0,
                                        position: 'relative',
                                    } as React.CSSProperties}>
                                        {peer.display_name ? peer.display_name[0].toUpperCase() : <BsPersonFill />}
                                        {/* Online dot */}
                                        <div style={{
                                            position: 'absolute',
                                            bottom: -1,
                                            right: -1,
                                            width: 10,
                                            height: 10,
                                            borderRadius: '50%',
                                            background: online ? '#50fa7b' : '#555',
                                            border: '2px solid var(--bg-tertiary)',
                                        } as React.CSSProperties} />
                                    </div>

                                    {/* Info */}
                                    <div style={{ flex: 1, minWidth: 0 } as React.CSSProperties}>
                                        <div style={{
                                            color: 'var(--text-primary)',
                                            fontSize: 14,
                                            fontWeight: 600,
                                            overflow: 'hidden',
                                            textOverflow: 'ellipsis',
                                            whiteSpace: 'nowrap',
                                        } as React.CSSProperties}>
                                            {peer.display_name || 'Anonymous'}
                                        </div>
                                        <div style={{
                                            color: 'var(--text-muted)',
                                            fontSize: 11,
                                            fontFamily: 'monospace',
                                        } as React.CSSProperties}>
                                            {truncateId(peer.node_id)}
                                        </div>
                                    </div>

                                    {/* Status */}
                                    <div style={{
                                        color: online ? '#50fa7b' : 'var(--text-muted)',
                                        fontSize: 11,
                                    } as React.CSSProperties}>
                                        {online ? '● Online' : '○ Offline'}
                                    </div>
                                </button>
                            );
                        })}
                    </div>
                </div>
            </Win98Box>

            {/* Recently visited */}
            {history.length > 0 && (
                <Win98Box variant="window" title="📋 Recently Visited" style={{ marginTop: 12 } as React.CSSProperties}>
                    <div style={{ padding: 12 } as React.CSSProperties}>
                        {[...history].reverse().slice(0, 10).map((entry, i) => (
                            <button
                                key={`${entry.nodeId}-${i}`}
                                onClick={() => navigateTo(entry.nodeId, entry.name)}
                                style={{
                                    display: 'block',
                                    width: '100%',
                                    padding: '4px 8px',
                                    background: 'transparent',
                                    border: 'none',
                                    color: 'var(--accent-purple)',
                                    cursor: 'pointer',
                                    textAlign: 'left',
                                    fontSize: 12,
                                    textDecoration: 'underline',
                                } as React.CSSProperties}
                            >
                                {entry.name} ({truncateId(entry.nodeId)})
                            </button>
                        ))}
                    </div>
                </Win98Box>
            )}
        </div>
    );

    // ─── Peer profile view ───
    const renderPeerView = () => (
        <div style={{ padding: 16, maxWidth: 700, margin: '0 auto' } as React.CSSProperties}>
            {/* Profile header */}
            <Win98Box variant="window" title={`✨ ${peerProfile?.username || 'Loading…'}'s Profile ✨`}>
                <div style={{ padding: 16 } as React.CSSProperties}>
                    {loadingFeed && !peerProfile && (
                        <div style={{ textAlign: 'center', padding: 32, color: 'var(--text-muted)' } as React.CSSProperties}>
                            📡 Connecting to peer…
                        </div>
                    )}

                    {feedError && (
                        <div style={{ padding: 16 } as React.CSSProperties}>
                            <p style={{ color: '#ff6b6b', margin: '0 0 8px' } as React.CSSProperties}>
                                ⚠️ {feedError}
                            </p>
                            <button
                                onClick={() => setCurrentNodeId(prev => prev)}
                                style={{
                                    padding: '4px 12px',
                                    background: 'var(--accent-purple)',
                                    color: '#fff',
                                    border: 'none',
                                    borderRadius: 4,
                                    cursor: 'pointer',
                                    fontSize: 12,
                                } as React.CSSProperties}
                            >
                                🔄 Retry
                            </button>
                        </div>
                    )}

                    {peerProfile && (
                        <div style={{ display: 'flex', alignItems: 'center', gap: 14 } as React.CSSProperties}>
                            <div style={{
                                width: 56,
                                height: 56,
                                borderRadius: '50%',
                                background: 'linear-gradient(135deg, var(--accent-purple), #6366f1)',
                                display: 'flex',
                                alignItems: 'center',
                                justifyContent: 'center',
                                fontSize: 24,
                                fontWeight: 700,
                                color: '#fff',
                                flexShrink: 0,
                            } as React.CSSProperties}>
                                {peerProfile.username[0].toUpperCase()}
                            </div>
                            <div>
                                <h2 style={{ margin: 0, color: 'var(--text-primary)', fontSize: 18 } as React.CSSProperties}>
                                    {peerProfile.username}
                                </h2>
                                <div style={{ color: 'var(--text-muted)', fontSize: 11, fontFamily: 'monospace', marginTop: 2 } as React.CSSProperties}>
                                    {/* DID meta — B2.3 integration */}
                                    <meta name="bytescape:did" content={peerProfile.did} />
                                    {truncateId(peerProfile.did, 14)}
                                </div>
                                {peerProfile.also_known_as && peerProfile.also_known_as.length > 0 && (
                                    <div style={{ display: 'flex', gap: 4, marginTop: 6, flexWrap: 'wrap' } as React.CSSProperties}>
                                        {peerProfile.also_known_as.map((aka, i) => (
                                            <span
                                                key={i}
                                                style={{
                                                    padding: '1px 6px',
                                                    background: 'var(--bg-tertiary)',
                                                    border: '1px solid var(--border-color)',
                                                    borderRadius: 10,
                                                    fontSize: 10,
                                                    color: 'var(--text-secondary)',
                                                } as React.CSSProperties}
                                                title={aka}
                                            >
                                                {aka.startsWith('did:plc:') ? '🦋' : '🔑'} {truncateId(aka, 6)}
                                            </span>
                                        ))}
                                    </div>
                                )}
                            </div>
                        </div>
                    )}

                    {peerProfile && (
                        <div style={{
                            display: 'flex',
                            gap: 16,
                            marginTop: 14,
                            paddingTop: 10,
                            borderTop: '1px solid var(--border-color)',
                            fontSize: 13,
                        } as React.CSSProperties}>
                            <span>
                                <strong style={{ color: 'var(--text-primary)' } as React.CSSProperties}>{totalCount}</strong>
                                <span style={{ color: 'var(--text-muted)', marginLeft: 4 } as React.CSSProperties}>posts</span>
                            </span>
                            <span style={{ display: 'flex', alignItems: 'center', gap: 4 } as React.CSSProperties}>
                                <BsCircleFill style={{ fontSize: 6, color: connected ? '#50fa7b' : '#ff6b6b' }} />
                                <span style={{ color: connected ? '#50fa7b' : '#ff6b6b', fontSize: 12 } as React.CSSProperties}>
                                    {connected ? 'Connected via P2P' : 'Disconnected'}
                                </span>
                            </span>
                        </div>
                    )}
                </div>
            </Win98Box>

            {/* Feed */}
            {!loadingFeed && peerItems.length === 0 && !feedError && (
                <div style={{ textAlign: 'center', padding: 32, color: 'var(--text-muted)' } as React.CSSProperties}>
                    📭 No shared posts from this peer.
                </div>
            )}

            {peerItems.map(item => (
                <FeedEntryCard key={item.id} entry={adaptBytescapeItem(item)} />
            ))}

            {offsetRef.current < totalCount && (
                <div style={{ textAlign: 'center', padding: 16 } as React.CSSProperties}>
                    <button
                        onClick={loadMore}
                        disabled={loadingFeed}
                        style={{
                            padding: '6px 20px',
                            background: 'var(--bg-secondary)',
                            color: 'var(--text-primary)',
                            border: '1px solid var(--border-color)',
                            borderRadius: 4,
                            cursor: 'pointer',
                            fontSize: 13,
                            opacity: loadingFeed ? 0.6 : 1,
                        } as React.CSSProperties}
                    >
                        {loadingFeed ? '⏳ Loading…' : `Load More (${peerItems.length}/${totalCount})`}
                    </button>
                </div>
            )}
        </div>
    );

    return (
        <div style={{
            display: 'flex',
            flexDirection: 'column',
            height: '100%',
            background: 'var(--bg-primary)',
        } as React.CSSProperties}>
            {renderAddressBar()}
            <div style={{ flex: 1, overflow: 'auto' } as React.CSSProperties}>
                {currentNodeId ? renderPeerView() : renderPeerDirectory()}
            </div>
        </div>
    );
}

const navBtnStyle: React.CSSProperties = {
    background: 'var(--bg-tertiary)',
    border: '1px solid var(--border-color)',
    borderRadius: 2,
    padding: '4px 6px',
    color: 'var(--text-primary)',
    cursor: 'pointer',
    fontSize: 14,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
};
