import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { AuthStatus, BytescapeItem, VaultStats } from '../types';
import { FeedEntryCard } from '../components/FeedEntryCard';
import { Win98Box } from '../components/Win98Box';
import { ProfileRenderer } from '../components/ProfileRenderer';
import { adaptBytescapeItem } from '../utils/adapters';
import { useNavStore } from '../store';
import { BsPersonFill, BsGlobe, BsStar, BsBookmark, BsPalette2 } from 'react-icons/bs';

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
    const [hasCustomLayout, setHasCustomLayout] = useState(false);
    const offsetRef = useRef(0);
    const { setScreen, setActiveSidebar } = useNavStore();

    // Check for custom profile layout
    useEffect(() => {
        invoke('get_profile_layout').then((layout) => {
            setHasCustomLayout(!!layout);
        }).catch(() => {});
    }, []);

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
        <div className="profile-geocities">
            {/* ═══ Hero Banner ═══ */}
            <div className="profile-hero">
                <div className="profile-hero-gradient" />
                <div className="profile-hero-content">
                    <div className="profile-avatar-large">
                        {auth?.handle ? (
                            <span>{auth.handle[0].toUpperCase()}</span>
                        ) : (
                            <BsPersonFill size={32} />
                        )}
                    </div>
                    <div className="profile-hero-info">
                        <h1 className="profile-display-name-large">
                            {auth?.handle || 'Anonymous'}
                        </h1>
                        <div className="profile-did">{auth?.did}</div>
                    </div>
                </div>
                <div className="profile-visitor-counter">
                    Visitors: {stats ? String(stats.total + 1337).padStart(6, '0') : '001337'}
                </div>
                <button
                    className="profile-edit-btn"
                    onClick={() => { setScreen('profile-builder'); setActiveSidebar('profile'); }}
                    title={hasCustomLayout ? 'Edit your custom layout' : 'Customize your profile page'}
                    style={{
                        position: 'absolute',
                        top: 12,
                        right: 12,
                        display: 'flex',
                        alignItems: 'center',
                        gap: 6,
                        padding: '6px 14px',
                        border: '1px solid var(--accent-purple, #7c3aed)',
                        borderRadius: 'var(--border-radius, 4px)',
                        background: 'rgba(124, 58, 237, 0.2)',
                        color: 'var(--accent-purple, #7c3aed)',
                        cursor: 'pointer',
                        fontSize: 13,
                        fontWeight: 600,
                        zIndex: 10,
                    } as React.CSSProperties}
                >
                    <BsPalette2 /> {hasCustomLayout ? 'Edit Layout' : 'Customize'}
                </button>
            </div>

            {/* ═══ Custom Layout or Default Wall ═══ */}
            {hasCustomLayout ? (
                <div className="profile-custom-layout" style={{ margin: '0 16px' } as React.CSSProperties}>
                    <ProfileRenderer editable={false} />
                </div>
            ) : (
            <>
            {/* ═══ GeoCities Content Grid ═══ */}
            <div className="profile-content-grid">
                {/* Left Column: About Boxes */}
                <div className="profile-sidebar-boxes">
                    <Win98Box variant="window" title="About Me">
                        <div className="profile-about-body">
                            <p className="profile-about-text">
                                Welcome to my corner of the internet! 🌐
                            </p>
                            <p className="profile-about-text">
                                This is where I keep my digital memories safe from
                                corporate surveillance. All data stored locally.
                            </p>
                        </div>
                    </Win98Box>

                    {stats && (
                        <Win98Box variant="window" title="My Stats">
                            <div className="profile-stats-list">
                                <div className="profile-stat-row">
                                    <BsBookmark size={12} />
                                    <span className="profile-stat-label">Total Posts</span>
                                    <span className="profile-stat-value">{stats.total}</span>
                                </div>
                                <div className="profile-stat-row">
                                    <BsStar size={12} />
                                    <span className="profile-stat-label">With Media</span>
                                    <span className="profile-stat-value">{stats.media_count}</span>
                                </div>
                                <div className="profile-stat-row">
                                    <BsGlobe size={12} />
                                    <span className="profile-stat-label">Vault Files</span>
                                    <span className="profile-stat-value">{archiveCount}</span>
                                </div>
                                <div className="profile-stat-row">
                                    <span>🛡️</span>
                                    <span className="profile-stat-label">Storage</span>
                                    <span className="profile-stat-value">Local Only</span>
                                </div>
                            </div>
                        </Win98Box>
                    )}

                    {sources.length > 0 && (
                        <Win98Box variant="window" title="My Networks">
                            <div className="profile-networks">
                                {sources.map(s => (
                                    <div key={s} className="profile-network-badge">
                                        {sourceEmoji(s)} {s}
                                    </div>
                                ))}
                            </div>
                        </Win98Box>
                    )}
                </div>

                {/* Right Column: Post Wall */}
                <div className="profile-wall-column">
                    <Win98Box variant="window" title={`My Wall (${total} posts)`}>
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

                        {loading && (
                            <div className="wall-empty">
                                <div className="wall-empty-icon">⏳</div>
                                <p>Loading your wall...</p>
                            </div>
                        )}

                        {error && !loading && (
                            <div className="profile-error">
                                <strong>Error:</strong> {error}
                            </div>
                        )}

                        {!loading && !error && items.length === 0 && (
                            <div className="wall-empty">
                                <div className="wall-empty-icon">📭</div>
                                <p>No posts yet. Import your data from the Import screen!</p>
                            </div>
                        )}
                    </Win98Box>

                    {!loading && items.map(item => (
                        <FeedEntryCard key={item.id} entry={adaptBytescapeItem(item)} />
                    ))}

                    {!loading && hasMore && (
                        <div className="profile-load-more">
                            <button
                                className="wall-filter-btn"
                                onClick={loadMore}
                                disabled={loadingMore}
                            >
                                {loadingMore ? '⏳ Loading...' : `Load More (${items.length} / ${total})`}
                            </button>
                        </div>
                    )}
                </div>
            </div>

            {/* ═══ Footer Guestbook ═══ */}
            <div className="profile-footer">
                <div className="profile-marquee">
                    ⭐ Thanks for visiting my page! ⭐ Built with Bytescape ⭐ Best viewed on any screen ⭐
                </div>
            </div>
            </>
            )}
        </div>
    );
}
