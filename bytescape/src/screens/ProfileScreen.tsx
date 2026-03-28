import "./ProfileScreen.css";
import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { AuthStatus, BytescapeItem, FeedEntry } from '../types';
import { VirtualizedFeedList } from '../components/feed/VirtualizedFeedList';
import { ProfileRenderer } from '../components/ProfileRenderer';
import { ProfileBioSections } from '../components/ProfileBioSections';
import { adaptBytescapeItem } from '../utils/adapters';
import { VideoPlaybackManager } from '../utils/VideoPlaybackManager';
import { useNavStore } from '../store';
import { BsPersonFill, BsPencilFill } from 'react-icons/bs';

const PAGE_SIZE = 20;
/** Hard cap on accumulated profile entries to prevent unbounded memory growth. */
const MAX_PROFILE_ENTRIES = 200;

interface PagedResult {
    items: BytescapeItem[];
    total: number;
}

interface ProfileScreenProps {
    auth: AuthStatus | null;
    archiveCount: number;
}

type SourceFilter = 'all' | string;
type ProfileTab = 'wall' | 'posts';

export function ProfileScreen({ auth }: ProfileScreenProps) {
    const [items, setItems] = useState<BytescapeItem[]>([]);
    const [total, setTotal] = useState(0);
    const [sourceFilter, setSourceFilter] = useState<SourceFilter>('all');
    const [sources, setSources] = useState<string[]>([]);
    const [loading, setLoading] = useState(true);
    const [loadingMore, setLoadingMore] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [hasCustomLayout, setHasCustomLayout] = useState(false);
    const [profileTab, setProfileTab] = useState<ProfileTab>('wall');
    const offsetRef = useRef(0);
    const scrollContainerRef = useRef<HTMLDivElement>(null);
    const { setScreen, setActiveSidebar } = useNavStore();

    // Release all video pipelines when source filter changes — prevents
    // GStreamer pipeline pile-up during filter toggle (each pipeline ~100-500 MB).
    const prevSourceFilterRef = useRef(sourceFilter);
    useEffect(() => {
        if (prevSourceFilterRef.current !== sourceFilter) {
            VideoPlaybackManager.releaseAll();
            prevSourceFilterRef.current = sourceFilter;
        }
    }, [sourceFilter]);

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
            const vaultSources = await invoke<string[]>('list_vault_sources');
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
            setItems(prev => {
                const combined = [...prev, ...result.items];
                return combined.length > MAX_PROFILE_ENTRIES
                    ? combined.slice(0, MAX_PROFILE_ENTRIES)
                    : combined;
            });
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

    const hasMore = offsetRef.current < total && items.length < MAX_PROFILE_ENTRIES;

    // Convert BytescapeItem[] to FeedEntry[] for VirtualizedFeedList
    const feedEntries: FeedEntry[] = useMemo(
        () => items.map(adaptBytescapeItem),
        [items]
    );

    return (
        <div ref={scrollContainerRef} className="profile-geocities" style={{ overflowY: 'auto', height: '100%' }}>
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
                        <div className="profile-did">{auth?.web_did || auth?.did}</div>
                    </div>
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
                        flexDirection: 'column',
                        alignItems: 'center',
                        gap: 2,
                        padding: '8px 14px',
                        border: '1px solid var(--accent-purple, #7c3aed)',
                        borderRadius: 'var(--border-radius, 4px)',
                        background: 'rgba(124, 58, 237, 0.2)',
                        color: 'var(--accent-purple, #7c3aed)',
                        cursor: 'pointer',
                        fontSize: 11,
                        fontWeight: 600,
                        zIndex: 10,
                    } as React.CSSProperties}
                >
                    <BsPencilFill size={16} />
                    <span>Customize</span>
                </button>
            </div>

            {/* ═══ Custom Layout or Default Wall ═══ */}
            {hasCustomLayout ? (
                <div className="profile-custom-layout" style={{ margin: '0 16px' } as React.CSSProperties}>
                    <ProfileRenderer editable={false} did={auth?.did} handle={auth?.handle} />
                </div>
            ) : (
            <>
            {/* ═══ Profile Tabs & Content ═══ */}
            <div className="profile-tabs" style={{ display: 'flex', gap: 0, borderBottom: '2px solid var(--border-outset-dark)', margin: '0 auto', maxWidth: 560, padding: '0 16px' } as React.CSSProperties}>
                <button
                    className={`wall-filter-btn ${profileTab === 'wall' ? 'active' : ''}`}
                    onClick={() => setProfileTab('wall')}
                    style={{ padding: '8px 20px', fontSize: '0.9rem' } as React.CSSProperties}
                >
                    📋 Wall
                </button>
                <button
                    className={`wall-filter-btn ${profileTab === 'posts' ? 'active' : ''}`}
                    onClick={() => setProfileTab('posts')}
                    style={{ padding: '8px 20px', fontSize: '0.9rem' } as React.CSSProperties}
                >
                    📝 Posts
                </button>
            </div>

            <div className="profile-content-narrow" style={{ maxWidth: 560, margin: '16px auto', padding: '0 16px' } as React.CSSProperties}>
                {profileTab === 'wall' ? (
                    /* Wall tab — editable bio sections */
                    <ProfileBioSections
                        handle={auth?.handle}
                        did={auth?.did}
                        source={auth?.source}
                    />
                ) : (
                    /* Posts tab — user's posts across platforms */
                    <div>
                        {/* Source/network filter */}
                        {sources.length > 1 && (
                            <div className="wall-filters" style={{ marginBottom: 12 } as React.CSSProperties}>
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
                                <p>Loading your posts...</p>
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

                        {!loading && feedEntries.length > 0 && (
                            <VirtualizedFeedList
                                entries={feedEntries}
                                scrollContainerRef={scrollContainerRef}
                            />
                        )}

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
                )}
            </div>

            {/* ═══ Footer ═══ */}
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
