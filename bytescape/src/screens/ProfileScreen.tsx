import "./ProfileScreen.css";
import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { appDataDir } from '@tauri-apps/api/path';
import { AuthStatus, BytescapeItem, ProfileSettings } from '../types';
import { FeedEntryCard } from '../components/FeedEntryCard';
import { ProfileBioSections } from '../components/ProfileBioSections';
import { ProfilePageNav } from '../components/ProfilePageNav';
import { ProfilePageEditor } from '../components/ProfilePageEditor';
import { useProfilePages } from '../hooks/useProfilePages';
import { adaptBytescapeItem } from '../utils/adapters';
import { platformEmoji, platformAbbrev } from '../utils/platform';
import { BsPersonFill, BsPencilFill, BsCheckLg, BsPaletteFill } from 'react-icons/bs';

const PAGE_SIZE = 20;

/* ═══ Profile Theme Presets ═══ */
const PROFILE_THEMES: Record<string, { label: string; heroGradient: string; accent: string; cardBg: string }> = {
    default:   { label: 'Default',    heroGradient: 'linear-gradient(135deg, #000080 0%, #4b0082 50%, #2e003e 100%)', accent: '#ffcc00', cardBg: '' },
    midnight:  { label: 'Midnight',   heroGradient: 'linear-gradient(135deg, #0f0c29, #302b63, #24243e)', accent: '#a78bfa', cardBg: 'rgba(30,27,75,0.7)' },
    sunset:    { label: 'Sunset',     heroGradient: 'linear-gradient(135deg, #f12711, #f5af19)', accent: '#fef08a', cardBg: 'rgba(120,30,10,0.5)' },
    forest:    { label: 'Forest',     heroGradient: 'linear-gradient(135deg, #0b3d0b, #2d6a4f, #40916c)', accent: '#95d5b2', cardBg: 'rgba(11,61,11,0.5)' },
    ocean:     { label: 'Ocean',      heroGradient: 'linear-gradient(135deg, #0077b6, #023e8a, #03045e)', accent: '#90e0ef', cardBg: 'rgba(2,62,138,0.5)' },
    retrowave: { label: 'Retrowave',  heroGradient: 'linear-gradient(135deg, #0d0221, #4a0e4e, #cc5577)', accent: '#ff6ec7', cardBg: 'rgba(74,14,78,0.5)' },
};

const BACKGROUND_PATTERNS: Record<string, string> = {
    scanlines: 'repeating-linear-gradient(0deg, transparent, transparent 2px, rgba(255,255,255,0.03) 2px, rgba(255,255,255,0.03) 4px)',
    dots: 'radial-gradient(circle, rgba(255,255,255,0.05) 1px, transparent 1px)',
    checkerboard: 'repeating-conic-gradient(rgba(255,255,255,0.03) 0% 25%, transparent 0% 50%)',
    stars: 'radial-gradient(2px 2px at 20px 30px, #eee, transparent), radial-gradient(2px 2px at 40px 70px, #fff, transparent), radial-gradient(1px 1px at 90px 40px, #ddd, transparent)',
    diamonds: 'repeating-linear-gradient(45deg, transparent, transparent 10px, rgba(255,255,255,0.03) 10px, rgba(255,255,255,0.03) 20px)',
};

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
    const [profileSettings, setProfileSettings] = useState<ProfileSettings>({});
    const [showThemePanel, setShowThemePanel] = useState(false);
    const [activePage, setActivePage] = useState<string | null>(null);
    const offsetRef = useRef(0);
    const { pages, savePage, deletePage, createPage } = useProfilePages();

    /* ════════════════════ Profile settings persistence ════════════════════ */
    useEffect(() => {
        invoke<string | null>('get_profile_settings')
            .then((raw) => {
                if (raw) {
                    try { setProfileSettings(JSON.parse(raw)); } catch { /* ignore */ }
                }
            })
            .catch(() => {});
    }, []);

    const saveSettings = useCallback(async (updated: ProfileSettings) => {
        setProfileSettings(updated);
        try {
            await invoke('save_profile_settings', { settingsJson: JSON.stringify(updated) });
        } catch (err) {
            console.error('Failed to save profile settings:', err);
        }
    }, []);

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

    /* Resolve active theme values */
    const activeThemeKey = profileSettings.theme || 'default';
    const activeTheme = PROFILE_THEMES[activeThemeKey] || PROFILE_THEMES.default;
    const heroStyle: React.CSSProperties = {
        background: activeTheme.heroGradient,
    };
    const accentColor = activeTheme.accent;

    /* Resolve background */
    const bgType = profileSettings.background_type || 'default';
    const bgValue = profileSettings.background_value || '';
    const pageStyle: React.CSSProperties = {};
    if (bgType === 'color' && bgValue) {
        pageStyle.background = bgValue;
    } else if (bgType === 'image' && bgValue) {
        pageStyle.backgroundImage = `url(${bgValue})`;
        pageStyle.backgroundSize = 'cover';
        pageStyle.backgroundPosition = 'center';
    } else if (bgType === 'pattern' && BACKGROUND_PATTERNS[bgValue]) {
        pageStyle.background = BACKGROUND_PATTERNS[bgValue];
    }

    /* Marquee */
    const marqueeEnabled = profileSettings.marquee_enabled || false;
    const marqueeText = profileSettings.marquee_text || '⭐ Thanks for visiting my page! ⭐ Built with Bytescape ⭐';
    const marqueeColor = profileSettings.marquee_color || '#ffcc00';
    const marqueeSpeed = profileSettings.marquee_speed || 'medium';
    const marqueeSpeedMap: Record<string, string> = { slow: '20s', medium: '12s', fast: '6s' };

    /* ════════════════════ Render ════════════════════ */
    return (
        <div className="profile-geocities" style={pageStyle}>
            {/* ═══ Hero Banner ═══ */}
            <div className="profile-hero" style={heroStyle}>
                <div className="profile-hero-gradient" />
                <div className="profile-hero-content">
                    <div className="profile-avatar-large" style={{ borderColor: accentColor } as React.CSSProperties}>
                        {displayName ? (
                            <span>{displayName[0].toUpperCase()}</span>
                        ) : (
                            <BsPersonFill size={32} />
                        )}
                    </div>
                    <div className="profile-hero-info">
                        <h1 className="profile-display-name-large" style={{ color: accentColor } as React.CSSProperties}>{displayName}</h1>
                        <div className="profile-did">{auth?.web_did || auth?.did}</div>
                    </div>
                </div>
                {auth && (
                    <div className="profile-hero-actions">
                        {editMode && (
                            <button
                                className="profile-theme-btn"
                                onClick={() => setShowThemePanel(!showThemePanel)}
                            >
                                <BsPaletteFill size={14} /> Theme
                            </button>
                        )}
                        <button
                            className="profile-edit-btn"
                            onClick={() => { setEditMode(!editMode); if (editMode) setShowThemePanel(false); }}
                        >
                            {editMode ? <><BsCheckLg size={14} /> Done</> : <><BsPencilFill size={14} /> Edit</>}
                        </button>
                    </div>
                )}
            </div>

            {/* ═══ Theme/Background Panel ═══ */}
            {showThemePanel && editMode && (
                <div className="profile-theme-panel">
                    <div className="profile-theme-section">
                        <h3>🎨 Theme</h3>
                        <div className="profile-theme-grid">
                            {Object.entries(PROFILE_THEMES).map(([key, theme]) => (
                                <button
                                    key={key}
                                    className={`profile-theme-swatch ${activeThemeKey === key ? 'active' : ''}`}
                                    onClick={() => saveSettings({ ...profileSettings, theme: key })}
                                    style={{ background: theme.heroGradient } as React.CSSProperties}
                                    title={theme.label}
                                >
                                    <span className="profile-theme-swatch-label">{theme.label}</span>
                                </button>
                            ))}
                        </div>
                    </div>

                    <div className="profile-theme-section">
                        <h3>🖼️ Background</h3>
                        <div className="profile-bg-options">
                            <label>
                                <input type="radio" name="bg" checked={bgType === 'default'} onChange={() => saveSettings({ ...profileSettings, background_type: 'default', background_value: '' })} /> Default
                            </label>
                            <label>
                                <input type="radio" name="bg" checked={bgType === 'color'} onChange={() => saveSettings({ ...profileSettings, background_type: 'color', background_value: '#1a1a2e' })} /> Solid Color
                            </label>
                            <label>
                                <input type="radio" name="bg" checked={bgType === 'image'} onChange={() => saveSettings({ ...profileSettings, background_type: 'image', background_value: '' })} /> Image URL
                            </label>
                            <label>
                                <input type="radio" name="bg" checked={bgType === 'pattern'} onChange={() => saveSettings({ ...profileSettings, background_type: 'pattern', background_value: 'scanlines' })} /> Pattern
                            </label>
                        </div>
                        {bgType === 'color' && (
                            <input
                                type="color"
                                className="profile-bg-color-input"
                                value={bgValue || '#1a1a2e'}
                                onChange={(e) => saveSettings({ ...profileSettings, background_type: 'color', background_value: e.target.value })}
                            />
                        )}
                        {bgType === 'image' && (
                            <input
                                type="text"
                                className="profile-bg-url-input"
                                placeholder="https://example.com/background.jpg"
                                value={bgValue}
                                maxLength={2000}
                                onChange={(e) => saveSettings({ ...profileSettings, background_type: 'image', background_value: e.target.value })}
                            />
                        )}
                        {bgType === 'pattern' && (
                            <div className="profile-pattern-grid">
                                {Object.keys(BACKGROUND_PATTERNS).map(p => (
                                    <button
                                        key={p}
                                        className={`profile-pattern-btn ${bgValue === p ? 'active' : ''}`}
                                        onClick={() => saveSettings({ ...profileSettings, background_type: 'pattern', background_value: p })}
                                    >
                                        {p}
                                    </button>
                                ))}
                            </div>
                        )}
                    </div>

                    <div className="profile-theme-section">
                        <h3>📰 Marquee Ticker</h3>
                        <label className="profile-marquee-toggle">
                            <input
                                type="checkbox"
                                checked={marqueeEnabled}
                                onChange={(e) => saveSettings({ ...profileSettings, marquee_enabled: e.target.checked })}
                            /> Enable scrolling ticker
                        </label>
                        {marqueeEnabled && (
                            <div className="profile-marquee-options">
                                <input
                                    type="text"
                                    className="profile-marquee-text-input"
                                    placeholder="Your scrolling message..."
                                    value={profileSettings.marquee_text || ''}
                                    maxLength={500}
                                    onChange={(e) => saveSettings({ ...profileSettings, marquee_text: e.target.value })}
                                />
                                <div className="profile-marquee-row">
                                    <label>Color: <input
                                        type="color"
                                        value={marqueeColor}
                                        onChange={(e) => saveSettings({ ...profileSettings, marquee_color: e.target.value })}
                                    /></label>
                                    <label>Speed:
                                        <select
                                            value={marqueeSpeed}
                                            onChange={(e) => saveSettings({ ...profileSettings, marquee_speed: e.target.value })}
                                        >
                                            <option value="slow">Slow</option>
                                            <option value="medium">Medium</option>
                                            <option value="fast">Fast</option>
                                        </select>
                                    </label>
                                </div>
                            </div>
                        )}
                    </div>
                </div>
            )}

            {/* ═══ Page Navigation ═══ */}
            {(pages.length > 0 || editMode) && (
                <ProfilePageNav
                    pages={pages}
                    activePage={activePage}
                    editMode={editMode}
                    onSelectPage={setActivePage}
                    onCreatePage={async (title, type) => {
                        const page = await createPage(title, type);
                        setActivePage(page.id);
                    }}
                    onDeletePage={async (pageId) => {
                        await deletePage(pageId);
                        if (activePage === pageId) setActivePage(null);
                    }}
                />
            )}

            {/* ═══ Page Content (when a page is active) ═══ */}
            {activePage && pages.find(p => p.id === activePage) && (
                <div className="profile-page-container">
                    <ProfilePageEditor
                        page={pages.find(p => p.id === activePage)!}
                        editMode={editMode}
                        onSave={savePage}
                    />
                </div>
            )}

            {/* ═══ 2-Column Content Grid (Home) ═══ */}
            {!activePage && <div className="profile-content-grid">
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
            </div>}

            {/* ═══ Marquee Footer (opt-in) ═══ */}
            {marqueeEnabled && (
                <div className="profile-footer">
                    <div
                        className="profile-marquee"
                        style={{
                            color: marqueeColor,
                            animationDuration: marqueeSpeedMap[marqueeSpeed] || '12s',
                        } as React.CSSProperties}
                    >
                        {marqueeText}
                    </div>
                </div>
            )}
        </div>
    );
}
