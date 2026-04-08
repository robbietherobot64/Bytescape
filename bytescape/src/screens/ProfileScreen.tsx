import "./ProfileScreen.css";
import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { appDataDir } from '@tauri-apps/api/path';
import { AuthStatus, BytescapeItem, ProfileSettings } from '../types';
import { FeedEntryCard } from '../components/FeedEntryCard';
import { ProfileRenderer } from '../components/ProfileRenderer';
import { ProfileBioSections } from '../components/ProfileBioSections';
import { ProfilePageNav } from '../components/ProfilePageNav';
import { ProfilePageEditor } from '../components/ProfilePageEditor';
import { useProfilePages } from '../hooks/useProfilePages';
import { adaptBytescapeItem } from '../utils/adapters';
import { platformEmoji, platformAbbrev } from '../utils/platform';
import { useNavStore } from '../store';
import { BsPersonFill, BsPencilFill, BsCheckLg, BsPaletteFill } from 'react-icons/bs';

/**
 * PURPOSE
 * Renders the local user's profile page, including a customizable layout mode,
 * bio wall sections, and paginated cross-source post history.
 *
 * HOW IT WORKS
 * 1) Checks whether a saved profile layout exists (`get_profile_layout`).
 * 2) Fetches source metadata and paged posts (`list_vault_sources`, `list_vault_posts_paged`).
 * 3) Supports source filtering and incremental pagination via local state + refs.
 * 4) Routes to profile builder through navigation store actions.
 *
 * RELATED FILES
 * - src/components/ProfileRenderer.tsx (custom profile layout renderer)
 * - src/components/ProfileBioSections.tsx (wall tab sections)
 * - src/components/FeedEntryCard.tsx (post card rendering)
 * - src/utils/adapters.ts (BytescapeItem -> FeedEntry conversion)
 * - src-tauri/src/lib.rs (profile/vault Tauri command handlers)
 * - src/store/* (navigation state transitions)
 */

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

/* ═══════════════════════════════════════════════════════════════════════════
 * Pre-built theme palettes for profile customization.
 * Each theme is a set of CSS custom properties applied on the root element.
 * ═══════════════════════════════════════════════════════════════════════════ */
const PROFILE_THEMES: Record<string, Record<string, string>> = {
    default: {},
    midnight: {
        '--profile-hero-bg': 'linear-gradient(135deg, #0f0c29 0%, #302b63 50%, #24243e 100%)',
        '--profile-hero-accent': '#a78bfa',
        '--profile-card-bg': 'rgba(30, 27, 75, 0.7)',
    },
    sunset: {
        '--profile-hero-bg': 'linear-gradient(135deg, #f12711 0%, #f5af19 100%)',
        '--profile-hero-accent': '#fef08a',
        '--profile-card-bg': 'rgba(120, 30, 10, 0.5)',
    },
    forest: {
        '--profile-hero-bg': 'linear-gradient(135deg, #0b3d0b 0%, #2d6a4f 50%, #40916c 100%)',
        '--profile-hero-accent': '#95d5b2',
        '--profile-card-bg': 'rgba(11, 61, 11, 0.5)',
    },
    ocean: {
        '--profile-hero-bg': 'linear-gradient(135deg, #0077b6 0%, #023e8a 50%, #03045e 100%)',
        '--profile-hero-accent': '#90e0ef',
        '--profile-card-bg': 'rgba(2, 62, 138, 0.5)',
    },
    retrowave: {
        '--profile-hero-bg': 'linear-gradient(135deg, #0d0221 0%, #4a0e4e 50%, #cc5577 100%)',
        '--profile-hero-accent': '#ff6ec7',
        '--profile-card-bg': 'rgba(74, 14, 78, 0.5)',
    },
};

/**
 * Local profile screen: identity hero, bio sidebar, post feed, customization.
 *
 * Cross-system notes:
 * - Command boundary: reads profile/layout and vault posts via Tauri `invoke`.
 * - Store boundary: uses `useNavStore` to navigate into the profile-builder flow.
 * - Rendering boundary: delegates custom layout + bio sections to dedicated components.
 */
export function ProfileScreen({ auth }: ProfileScreenProps) {
    /* ════════════════════ Screen state + store wiring ════════════════════ */
    const [items, setItems] = useState<BytescapeItem[]>([]);
    const [total, setTotal] = useState(0);
    const [sourceFilter, setSourceFilter] = useState<SourceFilter>('all');
    const [sources, setSources] = useState<string[]>([]);
    const [loading, setLoading] = useState(true);
    const [loadingMore, setLoadingMore] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [hasCustomLayout, setHasCustomLayout] = useState(false);
    const [mediaDir, setMediaDir] = useState<string>('');
    const [editMode, setEditMode] = useState(false);
    const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);
    const [profileSettings, setProfileSettings] = useState<ProfileSettings>({});
    const [showThemePicker, setShowThemePicker] = useState(false);
    const [activePage, setActivePage] = useState<string | null>(null);
    const offsetRef = useRef(0);
    const { setScreen, setActiveSidebar } = useNavStore();

    /* ════════════════════ Profile pages ════════════════════ */
    const { pages, savePage, deletePage, createPage } = useProfilePages();

    /* ════════════════════ Media path resolution (mirrors FeedScreen) ════════════════════ */
    useEffect(() => {
        appDataDir().then(dir => setMediaDir(dir)).catch(console.error);
    }, []);

    const resolveMediaPath = (src: string) => {
        if (!src) return '';
        if (src.startsWith('http://') || src.startsWith('https://') || src.startsWith('asset://') || src.startsWith('tauri://') || src.startsWith('blob:')) return src;
        if (mediaDir) {
            if (src.startsWith('media/')) {
                const filename = src.replace(/^media\//, '');
                return convertFileSrc(`${mediaDir}/${filename}`);
            }
            if (src.startsWith('/') || src.match(/^[A-Za-z]:\\/)) {
                return convertFileSrc(src);
            }
            return convertFileSrc(`${mediaDir}/${src}`);
        }
        return src;
    };

    /* ════════════════════ Initialization/data loading ════════════════════ */
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

    /* ════════════════════ Profile settings loading ════════════════════ */
    useEffect(() => {
        invoke<string | null>('get_profile_settings')
            .then((raw) => {
                if (raw) {
                    try { setProfileSettings(JSON.parse(raw)); } catch { /* noop */ }
                }
            })
            .catch(() => {});
    }, []);

    /* ════════════════════ UI handlers/helpers ════════════════════ */
    // Reset when source filter changes
    const handleSourceFilter = (s: SourceFilter) => {
        if (s === sourceFilter) return;
        setSourceFilter(s);
        setItems([]);
        setTotal(0);
    };

    // Delete a post from the vault
    const handleDeletePost = async (itemId: string) => {
        if (deleteConfirm !== itemId) {
            setDeleteConfirm(itemId);
            return;
        }
        try {
            await invoke<boolean>('delete_vault_item', { itemId });
            setItems(prev => prev.filter(i => i.id !== itemId));
            setTotal(prev => Math.max(0, prev - 1));
        } catch (e) {
            console.error('Delete failed:', e);
        } finally {
            setDeleteConfirm(null);
        }
    };

    // Save profile settings (theme, etc.)
    const saveSettings = async (updated: ProfileSettings) => {
        setProfileSettings(updated);
        try {
            await invoke('save_profile_settings', { settings: JSON.stringify(updated) });
        } catch (e) {
            console.error('Save profile settings failed:', e);
        }
    };

    // Compute theme CSS variables
    const themeVars = profileSettings.theme && PROFILE_THEMES[profileSettings.theme]
        ? PROFILE_THEMES[profileSettings.theme]
        : {};

    const hasMore = offsetRef.current < total;
    const displayName = auth?.local_username || auth?.handle || 'Anonymous';

    /* ════════════════════ Render ════════════════════ */
    return (
        <div className="profile-geocities" style={themeVars as React.CSSProperties}>
            {/* ═══ Hero Banner ═══ */}
            <div className="profile-hero" style={themeVars['--profile-hero-bg'] ? { background: themeVars['--profile-hero-bg'] } as React.CSSProperties : undefined}>
                <div className="profile-hero-gradient" />
                <div className="profile-hero-content">
                    <div className="profile-avatar-large" style={themeVars['--profile-hero-accent'] ? { borderColor: themeVars['--profile-hero-accent'] } as React.CSSProperties : undefined}>
                        {displayName ? (
                            <span>{displayName[0].toUpperCase()}</span>
                        ) : (
                            <BsPersonFill size={32} />
                        )}
                    </div>
                    <div className="profile-hero-info">
                        <h1 className="profile-display-name-large" style={themeVars['--profile-hero-accent'] ? { color: themeVars['--profile-hero-accent'] } as React.CSSProperties : undefined}>
                            {displayName}
                        </h1>
                        <div className="profile-did">{auth?.web_did || auth?.did}</div>
                    </div>
                </div>

                {/* Edit Profile / Theme controls — only visible when logged in */}
                {auth && (
                    <div className="profile-hero-actions">
                        <button
                            className={`profile-edit-btn ${editMode ? 'profile-edit-btn--active' : ''}`}
                            onClick={() => setEditMode(!editMode)}
                            title={editMode ? 'Exit edit mode' : 'Edit your profile'}
                        >
                            {editMode ? <><BsCheckLg size={14} /> <span>Done</span></> : <><BsPencilFill size={14} /> <span>Edit Profile</span></>}
                        </button>
                        {editMode && (
                            <button
                                className="profile-edit-btn"
                                onClick={() => setShowThemePicker(!showThemePicker)}
                                title="Change profile theme"
                            >
                                <BsPaletteFill size={14} /> <span>Theme</span>
                            </button>
                        )}
                        {editMode && (
                            <button
                                className="profile-edit-btn"
                                onClick={() => { setScreen('profile-builder'); setActiveSidebar('profile'); }}
                                title="Open advanced profile builder"
                            >
                                🛠 <span>Builder</span>
                            </button>
                        )}
                    </div>
                )}

                {/* Theme Picker Dropdown */}
                {showThemePicker && (
                    <div className="profile-theme-picker">
                        {Object.keys(PROFILE_THEMES).map(key => (
                            <button
                                key={key}
                                className={`profile-theme-swatch ${profileSettings.theme === key || (!profileSettings.theme && key === 'default') ? 'active' : ''}`}
                                onClick={() => {
                                    saveSettings({ ...profileSettings, theme: key === 'default' ? undefined : key });
                                    setShowThemePicker(false);
                                }}
                                title={key.charAt(0).toUpperCase() + key.slice(1)}
                            >
                                <div
                                    className="theme-swatch-preview"
                                    style={{ background: PROFILE_THEMES[key]['--profile-hero-bg'] || 'linear-gradient(135deg, #000080 0%, #4b0082 50%, #2e003e 100%)' } as React.CSSProperties}
                                />
                                <span>{key.charAt(0).toUpperCase() + key.slice(1)}</span>
                            </button>
                        ))}
                    </div>
                )}
            </div>

            {/* ═══ Page Navigation ═══ */}
            {(pages.length > 0 || editMode) && (
                <ProfilePageNav
                    pages={pages}
                    activePage={activePage}
                    editMode={editMode}
                    onSelectPage={setActivePage}
                    onCreatePage={(title, contentType) => createPage(title, contentType)}
                    onDeletePage={(id) => { deletePage(id); if (activePage === id) setActivePage(null); }}
                />
            )}

            {/* ═══ Active Sub-Page Content ═══ */}
            {activePage && (() => {
                const page = pages.find(p => p.id === activePage);
                if (!page) return null;
                return (
                    <div className="profile-page-content-wrap">
                        <ProfilePageEditor
                            page={page}
                            editMode={editMode}
                            onSave={savePage}
                        />
                    </div>
                );
            })()}

            {/* ═══ Custom Layout (if saved from builder) ═══ */}
            {!activePage && hasCustomLayout && !editMode ? (
                <div className="profile-custom-layout" style={{ margin: '0 16px' } as React.CSSProperties}>
                    <ProfileRenderer editable={false} did={auth?.did} handle={auth?.handle} />
                </div>
            ) : !activePage ? (
            <>
            {/* ═══ 2-Column Layout: Posts + Bio Sidebar ═══ */}
            <div className="profile-content-grid">
                {/* ─── Left Column: Post Feed ─── */}
                <div className="profile-posts-column">
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
                        <div key={item.id} className="profile-post-card-wrap">
                            {deleteConfirm === item.id && (
                                <div className="delete-confirm-bar">
                                    <span>Delete this post permanently?</span>
                                    <button className="delete-confirm-yes" onClick={() => handleDeletePost(item.id)}>Yes, delete</button>
                                    <button className="delete-confirm-no" onClick={() => setDeleteConfirm(null)}>Cancel</button>
                                </div>
                            )}
                            <FeedEntryCard
                                entry={adaptBytescapeItem(item)}
                                resolveMediaPath={resolveMediaPath}
                                mediaDir={mediaDir}
                                onDelete={auth ? handleDeletePost : undefined}
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

                {/* ─── Right Column: Bio Sidebar ─── */}
                <div className="profile-sidebar">
                    <ProfileBioSections
                        handle={auth?.handle}
                        did={auth?.did}
                        source={auth?.source}
                        editMode={editMode}
                    />
                </div>
            </div>
            </>
            ) : null}
        </div>
    );
}
