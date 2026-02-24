import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { AuthStatus, BytescapeItem, VaultStats } from '../types';
import { FeedEntryCard } from '../components/FeedEntryCard';
import { adaptBytescapeItem } from '../utils/adapters';

const PAGE_SIZE = 20;

interface PagedResult {
    items: BytescapeItem[];
    total: number;
}

interface ProfileScreenProps {
    auth: AuthStatus | null;
    archiveCount: number;
}

type SourceFilter = 'all' | string;

export function ProfileScreen({ auth, archiveCount }: ProfileScreenProps) {
    const [items, setItems] = useState<BytescapeItem[]>([]);
    const [total, setTotal] = useState(0);
    const [stats, setStats] = useState<VaultStats | null>(null);
    const [sourceFilter, setSourceFilter] = useState<SourceFilter>('all');
    const [sources, setSources] = useState<string[]>([]);
    const [loading, setLoading] = useState(true);
    const [loadingMore, setLoadingMore] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const offsetRef = useRef(0);

    // Load first page + metadata
    const loadInitial = useCallback(async () => {
        if (!auth) return;
        setLoading(true);
        setError(null);
        offsetRef.current = 0;
        try {
            const [vaultStats, vaultSources] = await Promise.all([
                invoke<VaultStats>('get_vault_stats'),
                invoke<string[]>('list_vault_sources'),
            ]);
            setStats(vaultStats);
            setSources(vaultSources);

            const result = await invoke<PagedResult>('list_vault_posts_paged', {
                limit: PAGE_SIZE,
                offset: 0,
                source: sourceFilter === 'all' ? null : sourceFilter,
            });

            setItems(result.items);
            setTotal(result.total);
            offsetRef.current = result.items.length;
        } catch (e) {
            console.error('ProfileScreen load error:', e);
            setError(String(e));
        } finally {
            setLoading(false);
        }
    }, [auth, sourceFilter]);

    // Load next page
    const loadMore = useCallback(async () => {
        if (loadingMore || offsetRef.current >= total) return;
        setLoadingMore(true);
        try {
            const result = await invoke<PagedResult>('list_vault_posts_paged', {
                limit: PAGE_SIZE,
                offset: offsetRef.current,
                source: sourceFilter === 'all' ? null : sourceFilter,
            });
            setItems(prev => [...prev, ...result.items]);
            offsetRef.current += result.items.length;
        } catch (e) {
            console.error('ProfileScreen loadMore error:', e);
        } finally {
            setLoadingMore(false);
        }
    }, [loadingMore, total, sourceFilter]);

    useEffect(() => {
        loadInitial();
    }, [loadInitial]);

    // Reset when source filter changes
    const handleSourceFilter = (s: SourceFilter) => {
        if (s === sourceFilter) return;
        setSourceFilter(s);
        setItems([]);
        setTotal(0);
    };

    const sourceEmoji = (s: string): string => {
        switch (s.toLowerCase()) {
            case 'bluesky': return '🦋';
            case 'facebook': return '📘';
            case 'mastodon': return '🐘';
            default: return '📝';
        }
    };

    const hasMore = offsetRef.current < total;

    return (
        <>
            <div className="main-header">
                <h1>👤 Profile Wall</h1>
            </div>

            <div className="profile-wall">
                {/* Profile header card */}
                <div className="card profile-header-card">
                    {auth?.handle ? (
                        <div className="profile-avatar-placeholder-large">
                            {auth.handle[0].toUpperCase()}
                        </div>
                    ) : (
                        <div style={{ fontSize: '3rem', marginBottom: 12 }}>👤</div>
                    )}
                    <h2>{auth?.handle || 'Anonymous'}</h2>
                    <p className="text-muted text-sm">{auth?.did}</p>

                    {stats && (
                        <div className="profile-stats-row">
                            <div className="profile-stat">
                                <div className="profile-stat-num">{stats.total}</div>
                                <div className="profile-stat-label">Posts</div>
                            </div>
                            <div className="profile-stat">
                                <div className="profile-stat-num">{stats.media_count}</div>
                                <div className="profile-stat-label">With Media</div>
                            </div>
                            <div className="profile-stat">
                                <div className="profile-stat-num">{archiveCount}</div>
                                <div className="profile-stat-label">Vault Files</div>
                            </div>
                            <div className="profile-stat">
                                <div className="profile-stat-num">🛡️</div>
                                <div className="profile-stat-label">Local Only</div>
                            </div>
                        </div>
                    )}
                </div>

                {/* Source filter pills */}
                {sources.length > 1 && (
                    <div className="wall-filters">
                        <button
                            className={`wall-filter-btn ${sourceFilter === 'all' ? 'active' : ''}`}
                            onClick={() => handleSourceFilter('all')}
                        >
                            All
                        </button>
                        {sources.map(s => (
                            <button
                                key={s}
                                className={`wall-filter-btn ${sourceFilter === s ? 'active' : ''}`}
                                onClick={() => handleSourceFilter(s)}
                            >
                                {sourceEmoji(s)} {s}
                            </button>
                        ))}
                    </div>
                )}

                {/* Loading state */}
                {loading && (
                    <div className="wall-empty">
                        <div className="wall-empty-icon">⏳</div>
                        <p>Loading your wall...</p>
                    </div>
                )}

                {/* Error state */}
                {error && !loading && (
                    <div className="card" style={{ padding: 20, color: 'var(--accent-red)' }}>
                        <strong>Error:</strong> {error}
                    </div>
                )}

                {/* Empty state */}
                {!loading && !error && items.length === 0 && (
                    <div className="wall-empty">
                        <div className="wall-empty-icon">📭</div>
                        <p>No posts yet. Import your data from the Import screen!</p>
                    </div>
                )}

                {/* Item feed */}
                {!loading && items.map(item => (
                    <FeedEntryCard key={item.id} entry={adaptBytescapeItem(item)} />
                ))}

                {/* Load More */}
                {!loading && hasMore && (
                    <div style={{ textAlign: 'center', padding: '20px 0 40px' }}>
                        <button
                            className="wall-filter-btn"
                            onClick={loadMore}
                            disabled={loadingMore}
                            style={{ padding: '10px 28px', fontSize: '0.9rem' }}
                        >
                            {loadingMore ? '⏳ Loading...' : `Load More (${items.length} / ${total})`}
                        </button>
                    </div>
                )}
            </div>
        </>
    );
}
