import "./ProfileScreen.css";
import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { appDataDir } from '@tauri-apps/api/path';
import { AuthStatus, BytescapeItem } from '../types';
import { FeedEntryCard } from '../components/FeedEntryCard';
import { ProfileBioSections } from '../components/ProfileBioSections';
import { adaptBytescapeItem } from '../utils/adapters';
import { platformEmoji, platformAbbrev } from '../utils/platform';
import { BsPersonFill, BsPencilFill, BsCheckLg } from 'react-icons/bs';

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

export function ProfileScreen({ auth }: ProfileScreenProps) {
    const [items, setItems] = useState<BytescapeItem[]>([]);
    const [total, setTotal] = useState(0);
    const [sourceFilter, setSourceFilter] = useState<SourceFilter>('all');
    const [sources, setSources] = useState<string[]>([]);
    const [loading, setLoading] = useState(true);
    const [loadingMore, setLoadingMore] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [editMode, setEditMode] = useState(false);
    const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null);
    const [mediaDir, setMediaDir] = useState<string>('');
    const offsetRef = useRef(0);

    /* ════════════════════ Media path resolution ════════════════════ */
    useEffect(() => {
        appDataDir().then(dir => setMediaDir(dir)).catch(console.error);
    }, []);

    const resolveMediaPath = (src: string) => {
        if (!src) return '';
        if (src.startsWith('http://') || src.startsWith('https://') || src.startsWith('asset://') || src.startsWith('tauri://') || src.startsWith('blob:')) return src;
        if (mediaDir) {
            if (src.startsWith('media/')) {
                return convertFileSrc(`${mediaDir}/${src.replace(/^media\//, '')}`);
            }
            if (src.startsWith('/') || src.match(/^[A-Za-z]:\\/)) {
                return convertFileSrc(src);
            }
            return convertFileSrc(`${mediaDir}/${src}`);
        }
        return src;
    };

    /* ════════════════════ Data loading ════════════════════ */
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

    /* ════════════════════ Handlers ════════════════════ */
    const handleSourceFilter = (s: SourceFilter) => {
        if (s === sourceFilter) return;
        setSourceFilter(s);
        setItems([]);
        setTotal(0);
    };

    const handleDeletePost = useCallback((id: string) => {
        setDeleteConfirmId(id);
    }, []);

    const confirmDelete = useCallback(async () => {
        if (!deleteConfirmId) return;
        try {
            await invoke('delete_vault_item', { itemId: deleteConfirmId });
            setItems(prev => prev.filter(item => item.id !== deleteConfirmId));
            setTotal(prev => prev - 1);
        } catch (err) {
            console.error('Delete failed:', err);
        } finally {
            setDeleteConfirmId(null);
        }
    }, [deleteConfirmId]);

    const hasMore = offsetRef.current < total;
    const displayName = auth?.local_username || auth?.handle || 'Anonymous';

    /* ════════════════════ Render ════════════════════ */
    return (
        <div className="profile-geocities">
            {/* ═══ Hero Banner ═══ */}
            <div className="profile-hero">
                <div className="profile-hero-gradient" />
                <div className="profile-hero-content">
                    <div className="profile-avatar-large">
                        {displayName ? (
                            <span>{displayName[0].toUpperCase()}</span>
                        ) : (
                            <BsPersonFill size={32} />
                        )}
                    </div>
                    <div className="profile-hero-info">
                        <h1 className="profile-display-name-large">{displayName}</h1>
                        <div className="profile-did">{auth?.web_did || auth?.did}</div>
                    </div>
                </div>
                {auth && (
                    <button
                        className="profile-edit-btn"
                        onClick={() => setEditMode(!editMode)}
                    >
                        {editMode ? <><BsCheckLg size={14} /> Done</> : <><BsPencilFill size={14} /> Edit</>}
                    </button>
                )}
            </div>

            {/* ═══ 2-Column Content Grid ═══ */}
            <div className="profile-content-grid">
                {/* Main column — posts */}
                <div className="profile-main-column">
                    {/* Source/network filter */}
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
                                    {platformEmoji(s)} {platformAbbrev(s)}
                                </button>
                            ))}
                        </div>
                    )}

                    {/* Delete confirmation */}
                    {deleteConfirmId && (
                        <div className="profile-delete-confirm">
                            <span>Delete this post? This cannot be undone.</span>
                            <button className="profile-delete-confirm-yes" onClick={confirmDelete}>🗑 Delete</button>
                            <button className="profile-delete-confirm-no" onClick={() => setDeleteConfirmId(null)}>Cancel</button>
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

                    {!loading && items.map(item => (
                        <div key={item.id} className="profile-post-wrapper">
                            <FeedEntryCard
                                entry={adaptBytescapeItem(item)}
                                resolveMediaPath={resolveMediaPath}
                                mediaDir={mediaDir}
                                onDelete={editMode ? handleDeletePost : undefined}
                            />
                        </div>
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

                {/* Sidebar — bio widgets */}
                <div className="profile-sidebar">
                    <ProfileBioSections
                        handle={auth?.handle}
                        did={auth?.did}
                        source={auth?.source}
                    />
                </div>
            </div>
        </div>
    );
}
