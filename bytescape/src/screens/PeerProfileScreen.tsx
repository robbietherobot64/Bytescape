import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { BytescapeItem, PeerFeedResponse, FeedEntry } from '../types';
import { VirtualizedFeedList } from '../components/feed/VirtualizedFeedList';
import { adaptBytescapeItem } from '../utils/adapters';
import { VideoPlaybackManager } from '../utils/VideoPlaybackManager';
import { useNavStore } from '../store';

/**
 * PURPOSE
 * Renders a peer's profile and shared feed over P2P, including profile metadata,
 * source filtering, and incremental paging.
 *
 * HOW IT WORKS
 * 1) Requests peer profile/feed slices from Rust via `request_peer_feed`.
 * 2) Pre-downloads referenced media blobs via `download_peer_blob` so existing
 *    blob rendering pipelines can resolve media locally.
 * 3) Adapts `BytescapeItem` records into `FeedEntry` for `VirtualizedFeedList`.
 * 4) Uses local UI state + refs for pagination, source filter, and connectivity.
 *
 * RELATED FILES
 * - src-tauri/src/lib.rs (Tauri command handlers for peer feed/blob download)
 * - src/components/feed/VirtualizedFeedList.tsx (virtualized feed rendering)
 * - src/utils/adapters.ts (BytescapeItem -> FeedEntry adaptation)
 * - src/utils/VideoPlaybackManager.ts (global video pipeline cleanup)
 * - src/store/* (navigation store for screen/sidebar transitions)
 */

const PAGE_SIZE = 20;
/** Hard cap on accumulated peer profile entries to prevent unbounded memory growth. */
const MAX_PEER_ENTRIES = 200;

/**
 * Download all media blobs from a peer's feed items via iroh-blobs.
 *
 * Fires download_peer_blob for every unique blob hash found in media_blobs
 * and thumbnail_blobs. Downloads run in parallel with a concurrency limit.
 * Failures are logged but non-fatal — the BlobImage component shows a
 * placeholder for any blob that couldn't be fetched.
 */
async function downloadPeerBlobs(items: BytescapeItem[], peerNodeId: string): Promise<void> {
    const hashes = new Set<string>();
    for (const item of items) {
        for (const h of item.media_blobs ?? []) {
            if (h) hashes.add(h);
        }
        for (const h of item.thumbnail_blobs ?? []) {
            if (h) hashes.add(h);
        }
    }
    if (hashes.size === 0) return;

    console.log(`[P2P-MEDIA] Downloading ${hashes.size} blobs from peer ${peerNodeId.slice(0, 12)}…`);

    // Download in batches of 4 to avoid overloading the QUIC connection
    const hashArr = [...hashes];
    const BATCH = 4;
    for (let i = 0; i < hashArr.length; i += BATCH) {
        const batch = hashArr.slice(i, i + BATCH);
        await Promise.allSettled(
            batch.map(hashStr =>
                invoke<boolean>('download_peer_blob', { hashStr, nodeId: peerNodeId })
                    .then(() => console.log(`[P2P-MEDIA] ✓ blob ${hashStr.slice(0, 12)}…`))
                    .catch(e => console.warn(`[P2P-MEDIA] ✗ blob ${hashStr.slice(0, 12)}…:`, e))
            )
        );
    }

    console.log(`[P2P-MEDIA] Blob download round complete`);
}

interface PeerProfileScreenProps {
    nodeId: string;
    initialPeerName?: string;
}

/**
 * Peer profile screen for remote identities discovered in the social graph.
 *
 * Cross-system notes:
 * - Invokes Rust commands (`request_peer_feed`, `download_peer_blob`) for data.
 * - Uses `useNavStore` to move between app-level screens/sidebar contexts.
 * - Coordinates with blob/media utilities rather than rendering binary payloads directly.
 */
export function PeerProfileScreen({ nodeId, initialPeerName }: PeerProfileScreenProps) {
    const { setScreen, setActiveSidebar } = useNavStore();

    /* ════════════════════ Profile + feed state ════════════════════ */
    // Profile state
    const [peerUsername, setPeerUsername] = useState<string>(initialPeerName || '');
    const [peerDid, setPeerDid] = useState<string>('');
    const [alsoKnownAs, setAlsoKnownAs] = useState<string[]>([]);
    const [feedConnected, setFeedConnected] = useState(false);

    // Feed state
    const [items, setItems] = useState<BytescapeItem[]>([]);
    const [totalCount, setTotalCount] = useState(0);
    const [loading, setLoading] = useState(true);
    const [loadingMore, setLoadingMore] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const offsetRef = useRef(0);

    // Source filter
    const [sourceFilter, setSourceFilter] = useState<string | null>(null);
    const [sources, setSources] = useState<string[]>([]);
    const scrollContainerRef = useRef<HTMLDivElement>(null);

    // Release all video pipelines when source filter changes — prevents
    // GStreamer pipeline pile-up during filter toggle (each pipeline ~100-500 MB).
    const prevSourceFilterRef = useRef(sourceFilter);
    useEffect(() => {
        if (prevSourceFilterRef.current !== sourceFilter) {
            VideoPlaybackManager.releaseAll();
            prevSourceFilterRef.current = sourceFilter;
        }
    }, [sourceFilter]);

    // Convert BytescapeItem[] to FeedEntry[] for VirtualizedFeedList
    const feedEntries: FeedEntry[] = useMemo(
        () => items.map(adaptBytescapeItem),
        [items]
    );

    /* ════════════════════ Data loading/invoke boundary ════════════════════ */
    const loadFeed = useCallback(async (offset: number, append: boolean) => {
        if (!append) setLoading(true);
        else setLoadingMore(true);
        setError(null);

        try {
            const response = await invoke<PeerFeedResponse & { _error?: string }>('request_peer_feed', {
                nodeId,
                limit: PAGE_SIZE,
                offset,
                sourceFilter,
            });

            // The backend may return cached peer info with an _error field
            // when the P2P connection failed but we have cached peer data
            if (response._error) {
                // We still got profile info from cache
                setPeerUsername(response.profile.username);
                setPeerDid(response.profile.did);
                setAlsoKnownAs(response.profile.also_known_as || []);
                setFeedConnected(false);
                setError(response._error);
            } else {
                setPeerUsername(response.profile.username);
                setPeerDid(response.profile.did);
                setAlsoKnownAs(response.profile.also_known_as || []);
                setTotalCount(response.total_count);
                setFeedConnected(true);

                // Download media blobs from the peer BEFORE rendering items.
                // This uses the iroh-blobs protocol — BlobsProtocol on the
                // peer's Router serves the data; our Downloader fetches it
                // into the local blob store. Once present, the existing
                // BlobImage → useLazyBlobUrl → get_media_blob pipeline works.
                if (response.items.length > 0) {
                    await downloadPeerBlobs(response.items, nodeId);
                }

                if (append) {
                    setItems(prev => {
                        const merged = [...prev, ...response.items];
                        return merged.length > MAX_PEER_ENTRIES ? merged.slice(0, MAX_PEER_ENTRIES) : merged;
                    });
                } else {
                    setItems(response.items);
                    // Derive unique sources from items for the filter
                    const uniqueSources = [...new Set(response.items.map(i => i.source))].filter(Boolean);
                    setSources(prev => {
                        const merged = new Set([...prev, ...uniqueSources]);
                        return [...merged];
                    });
                }

                offsetRef.current = offset + response.items.length;
            }
        } catch (e: any) {
            const msg = typeof e === 'string' ? e : e?.message || 'Failed to load peer feed';
            setError(msg);
            setFeedConnected(false);
        } finally {
            setLoading(false);
            setLoadingMore(false);
        }
    }, [nodeId, sourceFilter]);

    useEffect(() => {
        offsetRef.current = 0;
        setItems([]);
        loadFeed(0, false);
    }, [loadFeed]);

    /* ════════════════════ UI handlers/navigation ════════════════════ */
    const handleLoadMore = () => {
        if (!loadingMore && offsetRef.current < totalCount) {
            loadFeed(offsetRef.current, true);
        }
    };

    const handleSourceFilter = (s: string | null) => {
        if (s === sourceFilter) return;
        setSourceFilter(s);
        setItems([]);
        setTotalCount(0);
        offsetRef.current = 0;
    };

    const handleBack = () => {
        setScreen('friends');
        setActiveSidebar('social');
    };

    const truncateId = (id: string, len = 12) =>
        id.length > len * 2 ? `${id.slice(0, len)}…${id.slice(-len)}` : id;

    const sourceEmoji = (s: string): string => {
        switch (s.toLowerCase()) {
            case 'bluesky': return '🦋';
            case 'facebook': return '📘';
            case 'instagram': return '📷';
            case 'mastodon': return '🐘';
            default: return '📝';
        }
    };

    // Parse alsoKnownAs into readable labels
    const linkedIdentities = alsoKnownAs.map(aka => {
        if (aka.startsWith('did:plc:')) return { label: 'Bluesky', value: aka, emoji: '🦋' };
        if (aka.startsWith('at://')) return { label: 'Handle', value: aka.replace('at://', '@'), emoji: '🔗' };
        if (aka.startsWith('did:key:')) return { label: 'did:key', value: aka, emoji: '🔑' };
        return { label: 'Identity', value: aka, emoji: '🆔' };
    });

    const hasMore = offsetRef.current < totalCount && items.length < MAX_PEER_ENTRIES;

    /* ════════════════════ Render ════════════════════ */
    return (
        <div ref={scrollContainerRef} style={{ height: '100%', overflow: 'auto', background: 'var(--bg-primary)' }}>
            <div style={{ maxWidth: 700, margin: '0 auto', padding: '16px' }}>
                {/* Back button */}
                <button
                    onClick={handleBack}
                    style={{
                        padding: '6px 14px',
                        fontSize: '0.85rem',
                        background: 'var(--bg-secondary)',
                        color: 'var(--text-primary)',
                        border: '1px solid var(--border-color, #333)',
                        borderRadius: 6,
                        cursor: 'pointer',
                        marginBottom: 16,
                        display: 'flex',
                        alignItems: 'center',
                        gap: 6,
                    }}
                >
                    ← Back to Peers
                </button>

                {/* Profile Header */}
                <div className="card" style={{ marginBottom: 16, padding: '20px 24px' }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 16 }}>
                        {/* Avatar */}
                        <div style={{
                            width: 56,
                            height: 56,
                            borderRadius: '50%',
                            background: 'linear-gradient(135deg, var(--accent-purple), #6366f1)',
                            display: 'flex',
                            alignItems: 'center',
                            justifyContent: 'center',
                            fontSize: '1.5rem',
                            fontWeight: 700,
                            color: '#fff',
                            flexShrink: 0,
                        }}>
                            {peerUsername ? peerUsername[0].toUpperCase() : '?'}
                        </div>

                        {/* Name & identities */}
                        <div style={{ flex: 1, minWidth: 0 }}>
                            <h2 style={{
                                margin: 0,
                                fontSize: '1.3rem',
                                color: 'var(--text-primary)',
                                lineHeight: 1.2,
                            }}>
                                {loading ? 'Loading…' : peerUsername || 'Anonymous'}
                            </h2>
                            <div style={{
                                fontSize: '0.75rem',
                                color: 'var(--text-muted)',
                                fontFamily: 'monospace',
                                marginTop: 4,
                                wordBreak: 'break-all',
                            }}>
                                {peerDid && peerDid !== 'local' ? truncateId(peerDid, 14) : ''}
                            </div>

                            {/* Linked identities */}
                            {linkedIdentities.length > 0 && (
                                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginTop: 8 }}>
                                    {linkedIdentities.map((id, i) => (
                                        <span
                                            key={i}
                                            title={id.value}
                                            style={{
                                                display: 'inline-flex',
                                                alignItems: 'center',
                                                gap: 4,
                                                padding: '2px 8px',
                                                fontSize: '0.75rem',
                                                background: 'var(--bg-tertiary, #1a1a2e)',
                                                border: '1px solid var(--border-color, #333)',
                                                borderRadius: 12,
                                                color: 'var(--text-secondary, #aaa)',
                                            }}
                                        >
                                            <span>{id.emoji}</span>
                                            <span>{id.label === 'Handle' ? id.value : truncateId(id.value, 8)}</span>
                                        </span>
                                    ))}
                                </div>
                            )}
                        </div>
                    </div>

                    {/* Stats bar */}
                    {!loading && (
                        <div style={{
                            display: 'flex',
                            gap: 20,
                            marginTop: 16,
                            paddingTop: 12,
                            borderTop: '1px solid var(--border-color, #333)',
                            fontSize: '0.85rem',
                        }}>
                            <div>
                                <span style={{ fontWeight: 600, color: 'var(--text-primary)' }}>{totalCount}</span>
                                <span style={{ color: 'var(--text-muted)', marginLeft: 4 }}>posts</span>
                            </div>
                            {sources.length > 0 && (
                                <div>
                                    <span style={{ fontWeight: 600, color: 'var(--text-primary)' }}>{sources.length}</span>
                                    <span style={{ color: 'var(--text-muted)', marginLeft: 4 }}>
                                        {sources.length === 1 ? 'network' : 'networks'}
                                    </span>
                                </div>
                            )}
                            <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                                <span style={{
                                    width: 8,
                                    height: 8,
                                    borderRadius: '50%',
                                    background: feedConnected ? '#50fa7b' : '#ff6b6b',
                                    display: 'inline-block',
                                    boxShadow: feedConnected ? '0 0 4px #50fa7b' : '0 0 4px #ff6b6b',
                                }} />
                                <span style={{ color: feedConnected ? '#50fa7b' : '#ff6b6b', fontSize: '0.8rem' }}>
                                    {feedConnected ? 'P2P Connected' : 'Disconnected'}
                                </span>
                            </div>
                        </div>
                    )}
                </div>

                {/* Source filter pills */}
                {sources.length > 1 && (
                    <div style={{ display: 'flex', gap: 6, marginBottom: 12, flexWrap: 'wrap' }}>
                        <button
                            className={`wall-filter-btn ${sourceFilter === null ? 'active' : ''}`}
                            onClick={() => handleSourceFilter(null)}
                            style={{
                                padding: '4px 12px',
                                fontSize: '0.8rem',
                                background: sourceFilter === null ? 'var(--accent-purple)' : 'var(--bg-secondary)',
                                color: sourceFilter === null ? '#fff' : 'var(--text-primary)',
                                border: '1px solid var(--border-color, #333)',
                                borderRadius: 12,
                                cursor: 'pointer',
                            }}
                        >
                            All
                        </button>
                        {sources.map(s => (
                            <button
                                key={s}
                                className={`wall-filter-btn ${sourceFilter === s ? 'active' : ''}`}
                                onClick={() => handleSourceFilter(s)}
                                style={{
                                    padding: '4px 12px',
                                    fontSize: '0.8rem',
                                    background: sourceFilter === s ? 'var(--accent-purple)' : 'var(--bg-secondary)',
                                    color: sourceFilter === s ? '#fff' : 'var(--text-primary)',
                                    border: '1px solid var(--border-color, #333)',
                                    borderRadius: 12,
                                    cursor: 'pointer',
                                }}
                            >
                                {sourceEmoji(s)} {s}
                            </button>
                        ))}
                    </div>
                )}

                {/* Loading state */}
                {loading && (
                    <div className="card" style={{ textAlign: 'center', padding: 40 }}>
                        <div style={{ fontSize: '2rem', marginBottom: 8 }}>📡</div>
                        <p style={{ color: 'var(--text-muted)', margin: 0 }}>
                            Connecting to peer…
                        </p>
                        <p style={{ color: 'var(--text-muted)', fontSize: '0.8rem', marginTop: 4 }}>
                            Fetching their shared posts over P2P
                        </p>
                    </div>
                )}

                {/* Error state */}
                {error && !loading && (
                    <div className="card" style={{ padding: 24 }}>
                        <div style={{ color: '#ff6b6b', marginBottom: 8 }}>
                            <strong>Connection Error</strong>
                        </div>
                        <p style={{ color: 'var(--text-muted)', fontSize: '0.9rem', margin: '0 0 12px' }}>
                            {error}
                        </p>
                        <button
                            onClick={() => { setError(null); loadFeed(0, false); }}
                            style={{
                                padding: '6px 16px',
                                fontSize: '0.85rem',
                                background: 'var(--accent-purple)',
                                color: '#fff',
                                border: 'none',
                                borderRadius: 6,
                                cursor: 'pointer',
                            }}
                        >
                            🔄 Retry
                        </button>
                    </div>
                )}

                {/* Empty state */}
                {!loading && !error && items.length === 0 && (
                    <div className="card" style={{ textAlign: 'center', padding: 40 }}>
                        <div style={{ fontSize: '2rem', marginBottom: 8 }}>📭</div>
                        <p style={{ color: 'var(--text-muted)', margin: 0 }}>
                            This peer hasn't shared any posts yet.
                        </p>
                    </div>
                )}

                {/* Feed items — virtualized to prevent unbounded VideoPlayer mounts */}
                {!loading && feedEntries.length > 0 && (
                    <VirtualizedFeedList
                        entries={feedEntries}
                        scrollContainerRef={scrollContainerRef}
                    />
                )}

                {/* Load more */}
                {!loading && hasMore && (
                    <div style={{ textAlign: 'center', padding: '16px 0' }}>
                        <button
                            onClick={handleLoadMore}
                            disabled={loadingMore}
                            style={{
                                padding: '8px 24px',
                                fontSize: '0.9rem',
                                background: 'var(--bg-secondary)',
                                color: 'var(--text-primary)',
                                border: '1px solid var(--border-color, #333)',
                                borderRadius: 6,
                                cursor: loadingMore ? 'wait' : 'pointer',
                                opacity: loadingMore ? 0.6 : 1,
                            }}
                        >
                            {loadingMore
                                ? '⏳ Loading…'
                                : `Load More (${items.length} / ${totalCount})`}
                        </button>
                    </div>
                )}
            </div>
        </div>
    );
}
