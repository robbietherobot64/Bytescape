import "./ProfileScreen.css";
import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { appDataDir } from '@tauri-apps/api/path';
import { AuthStatus, BytescapeItem, LinkedAccount, ProfileSettings, ProfileModule, ProfileCustomPage, VaultFile } from '../types';
import { FeedEntryCard } from '../components/FeedEntryCard';
import { adaptBytescapeItem } from '../utils/adapters';
import { platformEmoji, platformAbbrev } from '../utils/platform';
import { BsPersonFill, BsPencilFill, BsCheckLg, BsPaletteFill, BsPlusLg, BsTrash, BsChevronUp, BsChevronDown, BsArrowsFullscreen } from 'react-icons/bs';

const PAGE_SIZE = 20;

/* ═══ Profile Theme Presets ═══ */
const PROFILE_THEME_PRESETS: Record<string, { label: string; bg: string; banner: string; text: string; accent: string }> = {
    default:   { label: 'Default',     bg: '',                                                           banner: 'linear-gradient(135deg, rgba(0,0,128,0.85), rgba(75,0,130,0.65))', text: '#ffcc00', accent: '#7c3aed' },
    midnight:  { label: 'Midnight',    bg: 'linear-gradient(180deg, #0f0c29, #302b63, #24243e)',         banner: 'linear-gradient(135deg, #0f0c29, #302b63)',                        text: '#a78bfa', accent: '#a78bfa' },
    sunset:    { label: 'Sunset',      bg: 'linear-gradient(180deg, #2d1b00, #4a1a0a, #1a0a00)',         banner: 'linear-gradient(135deg, #f12711, #f5af19)',                        text: '#fef08a', accent: '#f5af19' },
    forest:    { label: 'Forest',      bg: 'linear-gradient(180deg, #0b1a0b, #0d2818, #0a1f0a)',         banner: 'linear-gradient(135deg, #0b3d0b, #2d6a4f, #40916c)',              text: '#95d5b2', accent: '#40916c' },
    ocean:     { label: 'Ocean',       bg: 'linear-gradient(180deg, #020a1a, #031a3a, #021230)',          banner: 'linear-gradient(135deg, #0077b6, #023e8a, #03045e)',              text: '#90e0ef', accent: '#0077b6' },
    retrowave: { label: 'Retrowave',   bg: 'linear-gradient(180deg, #0d0221, #1a0533, #0d0221)',         banner: 'linear-gradient(135deg, #0d0221, #4a0e4e, #cc5577)',              text: '#ff6ec7', accent: '#ff6ec7' },
    terminal:  { label: 'Terminal',    bg: '#0a0a0a',                                                    banner: '#111',                                                            text: '#00ff41', accent: '#00ff41' },
    cream:     { label: 'Cream',       bg: 'linear-gradient(180deg, #1a1814, #2a2520, #1a1814)',         banner: 'linear-gradient(135deg, #3c2f1e, #5a4a35)',                        text: '#f5deb3', accent: '#daa520' },
};

/* ═══ Background Presets ═══ */
const BG_PRESETS: Array<{ label: string; value: string }> = [
    { label: 'Stars',       value: 'radial-gradient(2px 2px at 20px 30px, #eee, transparent), radial-gradient(2px 2px at 40px 70px, #fff, transparent), radial-gradient(1px 1px at 90px 40px, #ddd, transparent)' },
    { label: 'Scanlines',   value: 'repeating-linear-gradient(0deg, transparent, transparent 2px, rgba(255,255,255,0.03) 2px, rgba(255,255,255,0.03) 4px)' },
    { label: 'Dots',        value: 'radial-gradient(circle, rgba(255,255,255,0.05) 1px, transparent 1px)' },
    { label: 'Diamonds',    value: 'repeating-linear-gradient(45deg, transparent, transparent 10px, rgba(255,255,255,0.03) 10px, rgba(255,255,255,0.03) 20px)' },
];

/* ═══ Navigation Sections ═══ */
const CORE_NAV = [
    { id: 'home', label: 'Home', emoji: '🏠' },
    { id: 'posts', label: 'Posts', emoji: '📰' },
    { id: 'media', label: 'Media', emoji: '🖼️' },
];

/* ═══ Default Home Modules ═══ */
const DEFAULT_MODULES: ProfileModule[] = [
    { id: 'identities', type: 'identities', title: 'Connected Platforms', order: 0, visible: true, width: 'full' },
    { id: 'connections', type: 'connections', title: 'Connections', order: 1, visible: true, width: 'full' },
];

/* ═══ Addable Module Types ═══ */
const ADDABLE_MODULES: Array<{ type: string; label: string; emoji: string; unique: boolean }> = [
    { type: 'identities', label: 'Connected Platforms', emoji: '🔗', unique: true },
    { type: 'connections', label: 'Connections', emoji: '👥', unique: true },
    { type: 'about', label: 'About Me', emoji: '📝', unique: true },
    { type: 'listening-to', label: 'Listening To', emoji: '🎵', unique: true },
    { type: 'away-message', label: 'Away Message', emoji: '💤', unique: true },
    { type: 'links', label: 'Links', emoji: '🔗', unique: true },
    { type: 'guestbook', label: 'Guestbook', emoji: '💬', unique: true },
    { type: 'gif-collection', label: 'GIF Collection', emoji: '🖼️', unique: true },
    { type: 'blogs', label: 'Blogs', emoji: '✍️', unique: true },
    { type: 'check-ins', label: 'Check Ins', emoji: '📍', unique: true },
    { type: 'polls', label: 'Polls', emoji: '📊', unique: true },
    { type: 'playlists', label: 'Playlists', emoji: '🎶', unique: true },
    { type: 'custom', label: 'Custom Section', emoji: '✨', unique: false },
];

/* ═══ Embed Detection ═══ */
function detectEmbed(url: string): { url: string; label: string } | null {
    const trimmed = url.trim();
    const bcMatch = trimmed.match(/^https?:\/\/([^.]+)\.bandcamp\.com\/(track|album)\/([^/?#]+)/i);
    if (bcMatch) return { url: `https://bandcamp.com/EmbeddedPlayer/${bcMatch[2]}=${bcMatch[3]}/size=large/bgcol=333333/linkcol=e99708/tracklist=false/transparent=true/`, label: `Bandcamp · ${bcMatch[1]}` };
    if (/^https?:\/\/(www\.)?soundcloud\.com\/[^/]+\/[^/]+/i.test(trimmed)) return { url: `https://w.soundcloud.com/player/?url=${encodeURIComponent(trimmed)}&color=%23ff5500&auto_play=false&hide_related=true&show_comments=false`, label: 'SoundCloud' };
    const spMatch = trimmed.match(/^https?:\/\/open\.spotify\.com\/(track|album|playlist)\/([a-zA-Z0-9]+)/i);
    if (spMatch) return { url: `https://open.spotify.com/embed/${spMatch[1]}/${spMatch[2]}`, label: `Spotify · ${spMatch[1]}` };
    const ytMatch = trimmed.match(/(?:youtube\.com\/watch\?v=|youtu\.be\/)([a-zA-Z0-9_-]{11})/i);
    if (ytMatch) return { url: `https://www.youtube-nocookie.com/embed/${ytMatch[1]}`, label: 'YouTube' };
    return null;
}

interface ProfileLink { label: string; url: string; }

interface GuestbookEntry {
    id: string;
    author: string;
    message: string;
    timestamp: number;
    node_id: string | null;
}

interface PagedResult {
    items: BytescapeItem[];
    total: number;
}

interface ProfileScreenProps {
    auth: AuthStatus | null;
    archiveCount: number;
}

type SourceFilter = 'all' | string;

function generateId(): string {
    return `mod-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

export function ProfileScreen({ auth }: ProfileScreenProps) {
    /* ─── Core State ─── */
    const [activeNav, setActiveNav] = useState('home');
    const [editMode, setEditMode] = useState(false);
    const [previewMode, setPreviewMode] = useState(false);
    const [showCustomize, setShowCustomize] = useState(false);
    const [mediaDir, setMediaDir] = useState('');
    const [profileSettings, setProfileSettings] = useState<ProfileSettings>({});

    /* ─── Home Modules ─── */
    const [modules, setModules] = useState<ProfileModule[]>(DEFAULT_MODULES.map(m => ({ ...m })));
    const [editingModuleContent, setEditingModuleContent] = useState<string | null>(null);
    const [editingModuleTitle, setEditingModuleTitle] = useState<string | null>(null);
    const [showAddModuleMenu, setShowAddModuleMenu] = useState(false);
    const [editingModuleStyle, setEditingModuleStyle] = useState<string | null>(null);
    const addMenuRef = useRef<HTMLDivElement>(null);

    /* ─── Custom Nav Pages ─── */
    const [customPages, setCustomPages] = useState<ProfileCustomPage[]>([]);
    const [showAddPageInput, setShowAddPageInput] = useState(false);
    const [newPageLabel, setNewPageLabel] = useState('');
    const [newPageEmoji, setNewPageEmoji] = useState('📄');

    /* ─── Identities ─── */
    const [linkedAccounts, setLinkedAccounts] = useState<LinkedAccount[]>([]);

    /* ─── Connections ─── */
    const [followCount, setFollowCount] = useState(0);
    const [contactCount, setContactCount] = useState(0);
    const [allConnections, setAllConnections] = useState<Array<{ handle: string; source: string }>>([]);
    const [showAllConnections, setShowAllConnections] = useState(false);
    const [connectionSearch, setConnectionSearch] = useState('');
    const [connectionSourceFilter, setConnectionSourceFilter] = useState('all');

    /* ─── Wall (Guestbook) ─── */
    const [wallEntries, setWallEntries] = useState<GuestbookEntry[]>([]);
    const [wallMessage, setWallMessage] = useState('');
    const [wallSubmitting, setWallSubmitting] = useState(false);

    /* ─── Posts ─── */
    const [items, setItems] = useState<BytescapeItem[]>([]);
    const [total, setTotal] = useState(0);
    const [sourceFilter, setSourceFilter] = useState<SourceFilter>('all');
    const [sources, setSources] = useState<string[]>([]);
    const [loading, setLoading] = useState(true);
    const [loadingMore, setLoadingMore] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null);
    const offsetRef = useRef(0);

    /* ─── Media ─── */
    const [mediaFiles, setMediaFiles] = useState<VaultFile[]>([]);
    const [mediaLoading, setMediaLoading] = useState(false);
    const [mediaThumbs, setMediaThumbs] = useState<Record<string, string>>({});

    /* ─── Avatar ─── */
    const [showAvatarInput, setShowAvatarInput] = useState(false);
    const [avatarError, setAvatarError] = useState(false);

    /* ════════ Profile Settings Persistence ════════ */
    useEffect(() => {
        invoke<string | null>('get_profile_settings')
            .then((raw) => {
                if (raw) {
                    try {
                        const parsed = JSON.parse(raw) as ProfileSettings;
                        setProfileSettings(parsed);
                        if (parsed.modules && parsed.modules.length > 0) {
                            setModules(parsed.modules);
                        }
                        if (parsed.custom_pages) {
                            setCustomPages(parsed.custom_pages);
                        }
                    } catch { /* ignore */ }
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

    const saveModules = useCallback(async (updated: ProfileModule[]) => {
        setModules(updated);
        const newSettings = { ...profileSettings, modules: updated };
        setProfileSettings(newSettings);
        try {
            await invoke('save_profile_settings', { settingsJson: JSON.stringify(newSettings) });
        } catch (err) {
            console.error('Failed to save modules:', err);
        }
    }, [profileSettings]);

    /* ════════ Media Path Resolution ════════ */
    useEffect(() => {
        appDataDir().then(dir => setMediaDir(dir)).catch(console.error);
    }, []);

    const resolveMediaPath = (src: string) => {
        if (!src) return '';
        if (src.startsWith('http://') || src.startsWith('https://') || src.startsWith('asset://') || src.startsWith('tauri://') || src.startsWith('blob:')) return src;
        if (mediaDir) {
            if (src.startsWith('media/')) return convertFileSrc(`${mediaDir}/${src.replace(/^media\//, '')}`);
            if (src.startsWith('/') || src.match(/^[A-Za-z]:\\/)) return convertFileSrc(src);
            return convertFileSrc(`${mediaDir}/${src}`);
        }
        return src;
    };

    /* ════════ Data Loading ════════ */

    // Linked accounts
    useEffect(() => {
        invoke<LinkedAccount[]>('list_linked_accounts')
            .then(setLinkedAccounts)
            .catch(() => {});
    }, []);

    // Connections
    useEffect(() => {
        const conns: Array<{ handle: string; source: string }> = [];
        const promises: Promise<void>[] = [];
        if (auth?.did) {
            promises.push(
                invoke<Array<{ did: string; handle: string; displayName?: string }>>('list_follows', { actor: auth.did })
                    .then((follows) => {
                        setFollowCount(follows.length);
                        follows.forEach(f => conns.push({ handle: f.handle || f.displayName || f.did.slice(0, 12), source: 'Bluesky' }));
                    })
                    .catch(() => {})
            );
        }
        promises.push(
            invoke<Array<{ did: string; handle: string }>>('list_contacts')
                .then((contacts) => {
                    setContactCount(contacts.length);
                    contacts.forEach(c => conns.push({ handle: c.handle || c.did.slice(0, 12), source: 'P2P' }));
                })
                .catch(() => {})
        );
        Promise.all(promises).then(() => setAllConnections(conns));
    }, [auth?.did]);

    // Wall entries
    useEffect(() => {
        invoke<GuestbookEntry[]>('list_guestbook_entries')
            .then(setWallEntries)
            .catch(() => {});
    }, []);

    // Posts
    const loadPosts = useCallback(async () => {
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
        if (activeNav === 'posts') loadPosts();
    }, [activeNav, loadPosts]);

    // Media
    const loadMedia = useCallback(async () => {
        setMediaLoading(true);
        try {
            const files = await invoke<VaultFile[]>('vault_list_files');
            const visual = files.filter(f => f.kind === 'Image' || f.kind === 'Video');
            setMediaFiles(visual);
            // Fetch thumbnails for images
            const thumbs: Record<string, string> = {};
            await Promise.all(visual.map(async (f) => {
                if (f.kind !== 'Image' || !f.thumbnail_hash) return;
                try {
                    const bytes = await invoke<number[]>('vault_get_thumbnail', { fileId: f.id });
                    if (bytes) {
                        const blob = new Blob([new Uint8Array(bytes)], { type: 'image/webp' });
                        thumbs[f.id] = URL.createObjectURL(blob);
                    }
                } catch { /* skip */ }
            }));
            setMediaThumbs(thumbs);
        } catch {
            setMediaFiles([]);
        } finally {
            setMediaLoading(false);
        }
    }, []);

    useEffect(() => {
        if (activeNav === 'media') loadMedia();
    }, [activeNav, loadMedia]);

    /* ════════ Handlers ════════ */

    const handleSourceFilter = (s: SourceFilter) => {
        if (s === sourceFilter) return;
        setSourceFilter(s);
        setItems([]);
        setTotal(0);
    };

    const handleDeletePost = useCallback((id: string) => setDeleteConfirmId(id), []);

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

    // Wall
    const addWallEntry = useCallback(async () => {
        if (!wallMessage.trim()) return;
        setWallSubmitting(true);
        try {
            const entry = await invoke<GuestbookEntry>('add_guestbook_entry', {
                author: auth?.local_username || auth?.handle || 'Anonymous',
                message: wallMessage.trim(),
                nodeId: null,
            });
            setWallEntries(prev => [entry, ...prev]);
            setWallMessage('');
        } catch (err) {
            console.error('Wall post failed:', err);
        } finally {
            setWallSubmitting(false);
        }
    }, [wallMessage, auth]);

    const deleteWallEntry = useCallback(async (entryId: string) => {
        try {
            await invoke('delete_guestbook_entry', { entryId });
            setWallEntries(prev => prev.filter(e => e.id !== entryId));
        } catch (err) {
            console.error('Wall delete failed:', err);
        }
    }, []);

    // Module CRUD
    const addModule = useCallback((type: string, title: string) => {
        const newMod: ProfileModule = {
            id: generateId(),
            type,
            title,
            order: modules.length,
            visible: true,
            width: 'full',
            content: type === 'links' || type === 'gif-collection' ? '[]' : '',
        };
        const updated = [...modules, newMod];
        saveModules(updated);
        setShowAddModuleMenu(false);
    }, [modules, saveModules]);

    const removeModule = useCallback((id: string) => {
        saveModules(modules.filter(m => m.id !== id));
    }, [modules, saveModules]);

    const updateModuleContent = useCallback((id: string, content: string) => {
        const updated = modules.map(m => m.id === id ? { ...m, content } : m);
        setModules(updated);
    }, [modules]);

    const updateModuleTitle = useCallback((id: string, title: string) => {
        const updated = modules.map(m => m.id === id ? { ...m, title } : m);
        setModules(updated);
    }, [modules]);

    const updateModuleStyle = useCallback((id: string, style: Partial<ProfileModule>) => {
        const updated = modules.map(m => m.id === id ? { ...m, ...style } : m);
        saveModules(updated);
    }, [modules, saveModules]);

    const toggleModuleWidth = useCallback((id: string) => {
        const updated = modules.map(m => m.id === id ? { ...m, width: m.width === 'full' ? 'half' as const : 'full' as const } : m);
        saveModules(updated);
    }, [modules, saveModules]);

    // Move module up/down
    const moveModule = useCallback((id: string, direction: 'up' | 'down') => {
        const idx = modules.findIndex(m => m.id === id);
        if (idx === -1) return;
        const targetIdx = direction === 'up' ? idx - 1 : idx + 1;
        if (targetIdx < 0 || targetIdx >= modules.length) return;
        const updated = [...modules];
        [updated[idx], updated[targetIdx]] = [updated[targetIdx], updated[idx]];
        updated.forEach((m, i) => m.order = i);
        saveModules(updated);
    }, [modules, saveModules]);

    // Click-outside to close add menu
    useEffect(() => {
        if (!showAddModuleMenu) return;
        function handleClick(e: MouseEvent) {
            if (addMenuRef.current && !addMenuRef.current.contains(e.target as Node)) {
                setShowAddModuleMenu(false);
            }
        }
        document.addEventListener('mousedown', handleClick);
        return () => document.removeEventListener('mousedown', handleClick);
    }, [showAddModuleMenu]);

    // Save modules on edit mode exit
    const exitEditMode = useCallback(() => {
        setEditMode(false);
        setShowCustomize(false);
        setEditingModuleContent(null);
        setEditingModuleTitle(null);
        saveModules(modules);
    }, [modules, saveModules]);

    // Custom page CRUD
    const saveCustomPages = useCallback(async (pages: ProfileCustomPage[]) => {
        setCustomPages(pages);
        const newSettings = { ...profileSettings, custom_pages: pages };
        setProfileSettings(newSettings);
        try {
            await invoke('save_profile_settings', { settingsJson: JSON.stringify(newSettings) });
        } catch (err) {
            console.error('Failed to save custom pages:', err);
        }
    }, [profileSettings]);

    const addCustomPage = useCallback(() => {
        if (!newPageLabel.trim()) return;
        const page: ProfileCustomPage = {
            id: `page-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
            label: newPageLabel.trim(),
            emoji: newPageEmoji || '📄',
            modules: [],
        };
        saveCustomPages([...customPages, page]);
        setNewPageLabel('');
        setNewPageEmoji('📄');
        setShowAddPageInput(false);
        setActiveNav(`custom-${page.id}`);
    }, [newPageLabel, newPageEmoji, customPages, saveCustomPages]);

    const removeCustomPage = useCallback((pageId: string) => {
        saveCustomPages(customPages.filter(p => p.id !== pageId));
        if (activeNav === `custom-${pageId}`) setActiveNav('home');
    }, [customPages, saveCustomPages, activeNav]);

    const addModuleToPage = useCallback((pageId: string, type: string, title: string) => {
        const newMod: ProfileModule = {
            id: generateId(),
            type,
            title,
            order: 0,
            visible: true,
            width: 'full',
            content: type === 'links' || type === 'gif-collection' ? '[]' : '',
        };
        const updated = customPages.map(p => {
            if (p.id !== pageId) return p;
            const mods = [...p.modules, { ...newMod, order: p.modules.length }];
            return { ...p, modules: mods };
        });
        saveCustomPages(updated);
    }, [customPages, saveCustomPages]);

    const removeModuleFromPage = useCallback((pageId: string, moduleId: string) => {
        const updated = customPages.map(p => {
            if (p.id !== pageId) return p;
            return { ...p, modules: p.modules.filter(m => m.id !== moduleId) };
        });
        saveCustomPages(updated);
    }, [customPages, saveCustomPages]);

    const hasMore = offsetRef.current < total;
    const displayName = auth?.local_username || auth?.handle || 'Anonymous';

    /* Background */
    const bgType = profileSettings.background_type || 'default';
    const bgValue = profileSettings.background_value || '';
    const containerStyle: React.CSSProperties = {};
    if (profileSettings.profile_background) {
        containerStyle.background = profileSettings.profile_background;
    } else if (bgType === 'color' && bgValue) {
        containerStyle.background = bgValue;
    } else if (bgType === 'image' && bgValue) {
        containerStyle.backgroundImage = `url(${bgValue})`;
        containerStyle.backgroundSize = 'cover';
        containerStyle.backgroundPosition = 'center';
    }

    /* Marquee */
    const marqueeEnabled = profileSettings.marquee_enabled || false;
    const marqueeText = profileSettings.marquee_text || '⭐ Thanks for visiting my page! ⭐ Built with Bytescape ⭐';
    const marqueeColor = profileSettings.marquee_color || '#ffcc00';
    const marqueeSpeed = profileSettings.marquee_speed || 'medium';
    const marqueeSpeedMap: Record<string, string> = { slow: '20s', medium: '12s', fast: '6s' };

    const presentTypes = new Set(modules.map(m => m.type));
    const availableModules = ADDABLE_MODULES.filter(
        t => !t.unique || !presentTypes.has(t.type)
    );

    /* ════════════════════ Module Renderers ════════════════════ */

    function renderModuleContent(mod: ProfileModule) {
        if (editMode && editingModuleContent === mod.id) {
            if (mod.type === 'links') return renderLinksEditor(mod);
            if (mod.type === 'gif-collection') return renderGifEditor(mod);
            return (
                <textarea
                    className="profile-module-editor"
                    value={mod.content || ''}
                    onChange={(e) => updateModuleContent(mod.id, e.target.value)}
                    placeholder={`Enter your ${mod.title.toLowerCase()} here...`}
                    autoFocus
                />
            );
        }

        switch (mod.type) {
            case 'identities': return renderIdentities();
            case 'connections': return renderConnections();
            case 'about': return renderTextContent(mod, '📝', 'Tell the world about yourself.');
            case 'listening-to': return renderListeningTo(mod);
            case 'away-message': return renderTextContent(mod, '💤', 'Set your away message...');
            case 'links': return renderLinksView(mod);
            case 'guestbook': return renderGuestbook();
            case 'gif-collection': return renderGifView(mod);
            case 'blogs':
            case 'check-ins':
            case 'polls':
            case 'playlists':
                return renderPlaceholder(mod);
            case 'custom': return renderTextContent(mod, '✨', 'Add your custom content...');
            default: return renderPlaceholder(mod);
        }
    }

    function renderIdentities() {
        const protocolEmoji: Record<string, string> = {
            atproto: '🦋', bluesky: '🦋', activitypub: '🐘', mastodon: '🐘',
            meta: '📘', x: '𝕏', instagram: '📷',
        };
        if (linkedAccounts.length === 0 && !auth?.did) {
            return <div className="profile-empty"><div className="profile-empty-icon">🔗</div><p className="profile-empty-text">No connected accounts yet.</p></div>;
        }
        return (
            <div className="profile-identities-list">
                {auth?.did && (
                    <div className="profile-identity-item">
                        <span className="profile-identity-emoji">🌐</span>
                        <div className="profile-identity-info">
                            <div className="profile-identity-protocol">Bytescape</div>
                            <div className="profile-identity-handle">{auth.web_did || auth.did}</div>
                        </div>
                        <span className="profile-identity-badge profile-identity-badge--active">● Primary</span>
                    </div>
                )}
                {linkedAccounts.map((acct, i) => (
                    <div key={i} className="profile-identity-item">
                        <span className="profile-identity-emoji">{protocolEmoji[acct.protocol] || '🔗'}</span>
                        <div className="profile-identity-info">
                            <div className="profile-identity-protocol">{acct.protocol}</div>
                            <div className="profile-identity-handle">{acct.display_handle}</div>
                        </div>
                        {acct.active && <span className="profile-identity-badge profile-identity-badge--active">● Active</span>}
                    </div>
                ))}
            </div>
        );
    }

    function renderConnections() {
        const sourceEmoji: Record<string, string> = { Bluesky: '🦋', P2P: '🔗', Mastodon: '🐘', ActivityPub: '📡' };
        const uniqueSources = [...new Set(allConnections.map(c => c.source))];
        const filtered = allConnections
            .filter(c => connectionSourceFilter === 'all' || c.source === connectionSourceFilter)
            .filter(c => !connectionSearch || c.handle.toLowerCase().includes(connectionSearch.toLowerCase()));
        const displayList = showAllConnections ? filtered : filtered.slice(0, 12);
        const totalConns = allConnections.length;

        return (
            <div>
                <div className="profile-connections-summary">
                    {followCount > 0 && (
                        <div className="profile-connections-stat">👥 <strong>{followCount}</strong> following</div>
                    )}
                    {contactCount > 0 && (
                        <div className="profile-connections-stat">🤝 <strong>{contactCount}</strong> contacts</div>
                    )}
                    {totalConns > 0 && (
                        <div className="profile-connections-stat">🌐 <strong>{totalConns}</strong> total connections</div>
                    )}
                </div>

                {uniqueSources.length > 1 && (
                    <div className="profile-connections-filters">
                        <button
                            className={`profile-connections-filter-btn ${connectionSourceFilter === 'all' ? 'active' : ''}`}
                            onClick={() => setConnectionSourceFilter('all')}
                        >All</button>
                        {uniqueSources.map(s => (
                            <button
                                key={s}
                                className={`profile-connections-filter-btn ${connectionSourceFilter === s ? 'active' : ''}`}
                                onClick={() => setConnectionSourceFilter(s)}
                            >{sourceEmoji[s] || '🌐'} {s}</button>
                        ))}
                    </div>
                )}

                {showAllConnections && (
                    <input
                        type="text"
                        className="profile-connections-search"
                        placeholder="Search connections..."
                        value={connectionSearch}
                        onChange={(e) => setConnectionSearch(e.target.value)}
                    />
                )}

                <div className="profile-connections-grid">
                    {displayList.map((c, i) => (
                        <div key={`${c.source}-${c.handle}-${i}`} className="profile-connection-card">
                            <div className="profile-connection-avatar">
                                {c.handle[0]?.toUpperCase() || '?'}
                            </div>
                            <div className="profile-connection-name" title={c.handle}>{c.handle}</div>
                            <div className="profile-connection-platform">
                                <span className="profile-connection-platform-icon">{sourceEmoji[c.source] || '🌐'}</span>
                            </div>
                        </div>
                    ))}
                </div>

                {totalConns > 12 && (
                    <button
                        className="profile-connections-toggle"
                        onClick={() => { setShowAllConnections(!showAllConnections); if (showAllConnections) setConnectionSearch(''); }}
                    >
                        {showAllConnections ? '▲ Show less' : `▼ Show all (${totalConns})`}
                    </button>
                )}

                {totalConns === 0 && followCount === 0 && contactCount === 0 && (
                    <div className="profile-empty"><div className="profile-empty-icon">👥</div><p className="profile-empty-text">No connections yet.</p></div>
                )}
            </div>
        );
    }

    function renderTextContent(mod: ProfileModule, icon: string, placeholder: string) {
        if (!mod.content) {
            return (
                <div className="profile-empty">
                    <div className="profile-empty-icon">{icon}</div>
                    <p className="profile-empty-text">{editMode ? `Click ✏️ to edit...` : placeholder}</p>
                </div>
            );
        }
        return (
            <div className="profile-about-content">
                {mod.content.split('\n').map((line, i) => (
                    <p key={i}>{line || '\u00A0'}</p>
                ))}
            </div>
        );
    }

    function renderListeningTo(mod: ProfileModule) {
        if (!mod.content) {
            return (
                <div className="profile-listening-text">
                    <span>💿</span>
                    <span style={{ color: 'rgba(255,255,255,0.4)', fontStyle: 'italic' } as React.CSSProperties}>
                        {editMode ? 'Click ✏️ to set what you\'re listening to...' : 'Nothing playing right now.'}
                    </span>
                </div>
            );
        }
        const embed = detectEmbed(mod.content);
        if (embed) {
            return (
                <div>
                    <div className="profile-listening-embed">
                        <iframe
                            src={embed.url}
                            title={embed.label}
                            allow="autoplay; encrypted-media"
                            sandbox="allow-scripts allow-same-origin allow-popups"
                        />
                    </div>
                    <div className="profile-listening-source">{embed.label}</div>
                </div>
            );
        }
        return <div className="profile-listening-text"><span>💿</span><span>{mod.content}</span></div>;
    }

    function renderLinksView(mod: ProfileModule) {
        let links: ProfileLink[] = [];
        try { links = JSON.parse(mod.content || '[]'); } catch { links = []; }
        if (links.length === 0) {
            return (
                <div className="profile-empty">
                    <div className="profile-empty-icon">🔗</div>
                    <p className="profile-empty-text">{editMode ? 'Click ✏️ to add links...' : 'No links yet.'}</p>
                </div>
            );
        }
        return (
            <div className="profile-links-list">
                {links.map((link, i) => (
                    <div key={i} className="profile-link-item">
                        🔗 <a href={link.url} target="_blank" rel="noopener noreferrer">{link.label || link.url}</a>
                    </div>
                ))}
            </div>
        );
    }

    function renderLinksEditor(mod: ProfileModule) {
        let links: ProfileLink[] = [];
        try { links = JSON.parse(mod.content || '[]'); } catch { links = []; }
        const updateLink = (idx: number, field: 'label' | 'url', value: string) => {
            const updated = [...links];
            updated[idx] = { ...updated[idx], [field]: value };
            updateModuleContent(mod.id, JSON.stringify(updated));
        };
        const addLink = () => updateModuleContent(mod.id, JSON.stringify([...links, { label: '', url: '' }]));
        const removeLink = (idx: number) => updateModuleContent(mod.id, JSON.stringify(links.filter((_, i) => i !== idx)));

        return (
            <div>
                {links.map((link, i) => (
                    <div key={i} className="profile-link-edit-row">
                        <input className="profile-link-input" type="text" placeholder="Label" value={link.label} onChange={(e) => updateLink(i, 'label', e.target.value)} />
                        <input className="profile-link-input" type="text" placeholder="https://..." value={link.url} onChange={(e) => updateLink(i, 'url', e.target.value)} />
                        <button className="profile-module-edit-btn profile-module-edit-btn--danger" onClick={() => removeLink(i)}>✕</button>
                    </div>
                ))}
                <button className="profile-module-edit-btn" onClick={addLink} style={{ width: 'auto', padding: '4px 10px', marginTop: 4 } as React.CSSProperties}>
                    <BsPlusLg size={9} /> Add
                </button>
            </div>
        );
    }

    function renderGifView(mod: ProfileModule) {
        let gifs: string[] = [];
        try { gifs = JSON.parse(mod.content || '[]'); } catch { gifs = []; }
        if (gifs.length === 0) {
            return <div className="profile-empty"><div className="profile-empty-icon">🖼️</div><p className="profile-empty-text">{editMode ? 'Click ✏️ to add GIFs...' : 'No GIFs yet.'}</p></div>;
        }
        return (
            <div className="profile-gif-grid">
                {gifs.map((url, i) => <img key={i} src={url} alt="" className="profile-gif-item" loading="lazy" />)}
            </div>
        );
    }

    function renderGifEditor(mod: ProfileModule) {
        let gifs: string[] = [];
        try { gifs = JSON.parse(mod.content || '[]'); } catch { gifs = []; }
        const updateGif = (idx: number, value: string) => {
            const updated = [...gifs]; updated[idx] = value;
            updateModuleContent(mod.id, JSON.stringify(updated));
        };
        const addGif = () => updateModuleContent(mod.id, JSON.stringify([...gifs, '']));
        const removeGif = (idx: number) => updateModuleContent(mod.id, JSON.stringify(gifs.filter((_, i) => i !== idx)));

        return (
            <div>
                {gifs.map((url, i) => (
                    <div key={i} className="profile-link-edit-row">
                        <input className="profile-link-input" type="text" placeholder="https://example.com/funny.gif" value={url} onChange={(e) => updateGif(i, e.target.value)} />
                        <button className="profile-module-edit-btn profile-module-edit-btn--danger" onClick={() => removeGif(i)}>✕</button>
                    </div>
                ))}
                <button className="profile-module-edit-btn" onClick={addGif} style={{ width: 'auto', padding: '4px 10px', marginTop: 4 } as React.CSSProperties}>
                    <BsPlusLg size={9} /> Add GIF
                </button>
            </div>
        );
    }

    function renderGuestbook() {
        return (
            <div>
                <div className="profile-wall-form">
                    <input
                        className="profile-wall-input"
                        type="text"
                        placeholder="Sign the guestbook..."
                        value={wallMessage}
                        onChange={(e) => setWallMessage(e.target.value)}
                        onKeyDown={(e) => { if (e.key === 'Enter') addWallEntry(); }}
                        maxLength={500}
                    />
                    <button className="profile-wall-submit" onClick={addWallEntry} disabled={wallSubmitting || !wallMessage.trim()}>Sign</button>
                </div>
                {wallEntries.length === 0 && <div className="profile-empty"><div className="profile-empty-icon">✍️</div><p className="profile-empty-text">Be the first to sign!</p></div>}
                <div className="profile-wall-entries">
                    {wallEntries.slice(0, 10).map(entry => (
                        <div key={entry.id} className="profile-wall-entry">
                            <div className="profile-wall-entry-header">
                                <span className="profile-wall-author">{entry.author}</span>
                                <span className="profile-wall-time">{new Date(entry.timestamp * 1000).toLocaleDateString()}</span>
                            </div>
                            <div className="profile-wall-message">{entry.message}</div>
                            <button className="profile-wall-delete" onClick={() => deleteWallEntry(entry.id)}>🗑 Remove</button>
                        </div>
                    ))}
                </div>
            </div>
        );
    }

    function renderPlaceholder(mod: ProfileModule) {
        const emojiMap: Record<string, string> = { blogs: '✍️', 'check-ins': '📍', polls: '📊', playlists: '🎶' };
        return (
            <div className="profile-empty">
                <div className="profile-empty-icon">{emojiMap[mod.type] || '✨'}</div>
                <p className="profile-empty-text">{mod.title} — coming soon</p>
            </div>
        );
    }

    /* ════════════════════ Section Renderers ════════════════════ */

    function renderHome() {
        const visibleModules = modules.filter(m => m.visible).sort((a, b) => a.order - b.order);
        return (
            <div className="profile-module-grid">
                {visibleModules.map((mod, idx) => {
                    const modStyle: React.CSSProperties = {};
                    if (mod.background) { modStyle.background = mod.background; modStyle.backgroundSize = 'cover'; modStyle.backgroundPosition = 'center'; }
                    if (mod.font_color) modStyle.color = mod.font_color;
                    if (mod.font_size) modStyle.fontSize = mod.font_size;
                    if (mod.font_family) modStyle.fontFamily = mod.font_family;

                    return (
                    <div
                        key={mod.id}
                        className={`profile-module profile-module--${mod.width} ${editMode ? 'profile-module--editing' : ''}`}
                        style={modStyle}
                    >
                        <div className="profile-module-header">
                            {editMode && (
                                <div className="profile-module-reorder">
                                    <button className="profile-module-arrow-btn" onClick={() => moveModule(mod.id, 'up')} disabled={idx === 0} title="Move up"><BsChevronUp size={10} /></button>
                                    <button className="profile-module-arrow-btn" onClick={() => moveModule(mod.id, 'down')} disabled={idx === visibleModules.length - 1} title="Move down"><BsChevronDown size={10} /></button>
                                </div>
                            )}
                            {editMode && editingModuleTitle === mod.id ? (
                                <input
                                    className="profile-module-title-input"
                                    value={mod.title}
                                    onChange={(e) => updateModuleTitle(mod.id, e.target.value)}
                                    onBlur={() => setEditingModuleTitle(null)}
                                    onKeyDown={(e) => { if (e.key === 'Enter') setEditingModuleTitle(null); }}
                                    autoFocus
                                />
                            ) : (
                                <span className="profile-module-header-title" onDoubleClick={() => editMode && setEditingModuleTitle(mod.id)}>{mod.title}</span>
                            )}
                            {editMode && (
                                <div className="profile-module-edit-controls">
                                    <button className="profile-module-edit-btn" onClick={() => setEditingModuleStyle(editingModuleStyle === mod.id ? null : mod.id)} title="Style"><BsPaletteFill size={9} /></button>
                                    <button className="profile-module-edit-btn" onClick={() => setEditingModuleContent(editingModuleContent === mod.id ? null : mod.id)} title="Edit content"><BsPencilFill size={9} /></button>
                                    <button className="profile-module-edit-btn profile-module-edit-btn--danger" onClick={() => removeModule(mod.id)} title="Remove"><BsTrash size={9} /></button>
                                </div>
                            )}
                        </div>

                        {/* Per-module style editor */}
                        {editMode && editingModuleStyle === mod.id && (
                            <div className="profile-module-style-editor">
                                <div className="profile-module-style-row">
                                    <label>Font:</label>
                                    <select value={mod.font_family || ''} onChange={(e) => updateModuleStyle(mod.id, { font_family: e.target.value || undefined })}>
                                        <option value="">Default</option>
                                        <option value="serif">Serif</option>
                                        <option value="'Courier New', monospace">Monospace</option>
                                        <option value="'Comic Sans MS', cursive">Comic Sans</option>
                                        <option value="'Georgia', serif">Georgia</option>
                                        <option value="'Impact', sans-serif">Impact</option>
                                    </select>
                                </div>
                                <div className="profile-module-style-row">
                                    <label>Size:</label>
                                    <select value={mod.font_size || ''} onChange={(e) => updateModuleStyle(mod.id, { font_size: e.target.value || undefined })}>
                                        <option value="">Default</option>
                                        <option value="11px">Small</option>
                                        <option value="13px">Medium</option>
                                        <option value="15px">Large</option>
                                        <option value="18px">XL</option>
                                    </select>
                                </div>
                                <div className="profile-module-style-row">
                                    <label>Text:</label>
                                    <input type="color" value={mod.font_color || '#eeeeee'} onChange={(e) => updateModuleStyle(mod.id, { font_color: e.target.value })} />
                                    <button className="profile-customize-reset-btn" onClick={() => updateModuleStyle(mod.id, { font_color: undefined })}>×</button>
                                </div>
                                <div className="profile-module-style-row">
                                    <label>BG:</label>
                                    <input type="color" value={mod.background || '#1a1a2e'} onChange={(e) => updateModuleStyle(mod.id, { background: e.target.value })} />
                                    <button className="profile-customize-reset-btn" onClick={() => updateModuleStyle(mod.id, { background: undefined })}>×</button>
                                </div>
                            </div>
                        )}

                        <div className="profile-module-body">
                            {renderModuleContent(mod)}
                        </div>
                        {editMode && (
                            <button
                                className="profile-module-resize-toggle"
                                onClick={() => toggleModuleWidth(mod.id)}
                                title={mod.width === 'full' ? 'Shrink to half' : 'Expand to full'}
                            >
                                <BsArrowsFullscreen />
                            </button>
                        )}
                    </div>
                    );
                })}

                {editMode && (
                    <div className="profile-add-module" ref={addMenuRef}>
                        <button className="profile-add-module-btn" onClick={() => setShowAddModuleMenu(!showAddModuleMenu)}>
                            <BsPlusLg size={11} /> Add Module
                        </button>
                        {showAddModuleMenu && (
                            <div className="profile-add-dropdown">
                                {availableModules.map((t) => (
                                    <button key={t.type + t.label} onClick={() => addModule(t.type, t.type === 'custom' ? 'My Section' : t.label)}>
                                        {t.emoji} {t.label}
                                    </button>
                                ))}
                            </div>
                        )}
                    </div>
                )}
            </div>
        );
    }

    function renderPosts() {
        return (
            <div>
                <div className="profile-section-header">
                    <span className="profile-section-header-emoji">📰</span> Posts
                </div>

                {sources.length > 1 && (
                    <div className="profile-posts-filters">
                        <button
                            className={`profile-posts-filter-btn ${sourceFilter === 'all' ? 'profile-posts-filter-btn--active' : ''}`}
                            onClick={() => handleSourceFilter('all')}
                        >All</button>
                        {sources.map(s => (
                            <button
                                key={s}
                                className={`profile-posts-filter-btn ${sourceFilter === s ? 'profile-posts-filter-btn--active' : ''}`}
                                onClick={() => handleSourceFilter(s)}
                            >
                                {platformEmoji(s)} {platformAbbrev(s)}
                            </button>
                        ))}
                    </div>
                )}

                {deleteConfirmId && (
                    <div className="profile-delete-confirm">
                        <span>Delete this post? This cannot be undone.</span>
                        <button className="profile-delete-yes" onClick={confirmDelete}>🗑 Delete</button>
                        <button className="profile-delete-no" onClick={() => setDeleteConfirmId(null)}>Cancel</button>
                    </div>
                )}

                {loading && <div className="profile-empty"><div className="profile-empty-icon">⏳</div><p className="profile-empty-text">Loading posts...</p></div>}
                {error && !loading && <div className="profile-error"><strong>Error:</strong> {error}</div>}
                {!loading && !error && items.length === 0 && <div className="profile-empty"><div className="profile-empty-icon">📭</div><p className="profile-empty-text">No posts yet. Import data from the Import screen!</p></div>}

                <div className="profile-posts-list">
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
                </div>

                {!loading && hasMore && (
                    <div className="profile-load-more">
                        <button className="profile-load-more-btn" onClick={loadMore} disabled={loadingMore}>
                            {loadingMore ? '⏳ Loading...' : `Load More (${items.length} / ${total})`}
                        </button>
                    </div>
                )}
            </div>
        );
    }

    function renderMedia() {
        return (
            <div>
                <div className="profile-section-header">
                    <span className="profile-section-header-emoji">🖼️</span> Media
                </div>

                {mediaLoading && <div className="profile-empty"><div className="profile-empty-icon">⏳</div><p className="profile-empty-text">Loading media...</p></div>}

                {!mediaLoading && mediaFiles.length === 0 && (
                    <div className="profile-empty"><div className="profile-empty-icon">🖼️</div><p className="profile-empty-text">No media files yet.</p></div>
                )}

                <div className="profile-media-grid">
                    {mediaFiles.map(file => (
                        <div key={file.id} className="profile-media-item">
                            {mediaThumbs[file.id] ? (
                                <img
                                    src={mediaThumbs[file.id]}
                                    alt={file.name}
                                    loading="lazy"
                                />
                            ) : (
                                <div className="profile-media-placeholder">
                                    {file.kind === 'Video' ? '🎬' : '🖼️'}
                                </div>
                            )}
                        </div>
                    ))}
                </div>
            </div>
        );
    }

    function renderCustomPage(pageId: string) {
        const page = customPages.find(p => p.id === pageId);
        if (!page) return <div className="profile-empty"><div className="profile-empty-icon">❓</div><p className="profile-empty-text">Page not found.</p></div>;

        const pageMods = page.modules.sort((a, b) => a.order - b.order);
        const pageModTypes = new Set(pageMods.map(m => m.type));
        const pageAvailableModules = ADDABLE_MODULES.filter(t => !t.unique || !pageModTypes.has(t.type));

        return (
            <div>
                <div className="profile-section-header">
                    <span className="profile-section-header-emoji">{page.emoji}</span> {page.label}
                </div>

                <div className="profile-module-grid">
                    {pageMods.map((mod) => (
                        <div key={mod.id} className={`profile-module profile-module--${mod.width} ${editMode ? 'profile-module--editing' : ''}`}>
                            <div className="profile-module-header">
                                <span className="profile-module-header-title">{mod.title}</span>
                                {editMode && (
                                    <div className="profile-module-edit-controls">
                                        <button className="profile-module-edit-btn" onClick={() => setEditingModuleContent(editingModuleContent === mod.id ? null : mod.id)} title="Edit"><BsPencilFill size={9} /></button>
                                        <button className="profile-module-edit-btn profile-module-edit-btn--danger" onClick={() => removeModuleFromPage(pageId, mod.id)} title="Remove"><BsTrash size={9} /></button>
                                    </div>
                                )}
                            </div>
                            <div className="profile-module-body">
                                {renderModuleContent(mod)}
                            </div>
                        </div>
                    ))}

                    {editMode && (
                        <div className="profile-add-module">
                            <button className="profile-add-module-btn" onClick={() => setShowAddModuleMenu(!showAddModuleMenu)}>
                                <BsPlusLg size={11} /> Add Module
                            </button>
                            {showAddModuleMenu && (
                                <div className="profile-add-dropdown">
                                    {pageAvailableModules.map((t) => (
                                        <button key={t.type + t.label} onClick={() => { addModuleToPage(pageId, t.type, t.type === 'custom' ? 'My Section' : t.label); setShowAddModuleMenu(false); }}>
                                            {t.emoji} {t.label}
                                        </button>
                                    ))}
                                </div>
                            )}
                        </div>
                    )}
                </div>

                {pageMods.length === 0 && !editMode && (
                    <div className="profile-empty">
                        <div className="profile-empty-icon">{page.emoji}</div>
                        <p className="profile-empty-text">This page is empty. Click Edit to add modules.</p>
                    </div>
                )}
            </div>
        );
    }

    function renderCustomize() {
        return (
            <div className="profile-customize-panel">
                {/* Theme Presets */}
                <div className="profile-customize-section">
                    <h3>🎭 Profile Theme</h3>
                    <div className="profile-theme-grid">
                        {Object.entries(PROFILE_THEME_PRESETS).map(([key, preset]) => (
                            <button
                                key={key}
                                className={`profile-theme-swatch ${profileSettings.theme === key ? 'profile-theme-swatch--active' : ''}`}
                                style={{ background: preset.banner, color: preset.text, borderColor: profileSettings.theme === key ? preset.accent : 'transparent' } as React.CSSProperties}
                                onClick={() => saveSettings({
                                    ...profileSettings,
                                    theme: key,
                                    banner_type: 'gradient',
                                    banner_value: preset.banner,
                                    banner_text_color: preset.text,
                                    profile_background: preset.bg || undefined,
                                    background_type: preset.bg ? 'color' : 'default',
                                    background_value: preset.bg || '',
                                })}
                            >
                                {preset.label}
                            </button>
                        ))}
                    </div>
                </div>

                <div className="profile-customize-section">
                    <h3>🎨 Banner Header</h3>
                    <div className="profile-customize-row">
                        <label>
                            <input type="radio" name="banner" checked={bannerType === 'theme'} onChange={() => saveSettings({ ...profileSettings, banner_type: 'theme', banner_value: '' })} /> Match Theme
                        </label>
                        <label>
                            <input type="radio" name="banner" checked={bannerType === 'color'} onChange={() => saveSettings({ ...profileSettings, banner_type: 'color', banner_value: '#1a1a2e' })} /> Solid Color
                        </label>
                        <label>
                            <input type="radio" name="banner" checked={bannerType === 'image'} onChange={() => saveSettings({ ...profileSettings, banner_type: 'image', banner_value: '' })} /> Image
                        </label>
                        <label>
                            <input type="radio" name="banner" checked={bannerType === 'gradient'} onChange={() => saveSettings({ ...profileSettings, banner_type: 'gradient', banner_value: 'linear-gradient(135deg, #667eea, #764ba2)' })} /> Gradient
                        </label>
                    </div>
                    {bannerType === 'color' && (
                        <input type="color" className="profile-customize-color" value={bannerValue || '#1a1a2e'}
                            onChange={(e) => saveSettings({ ...profileSettings, banner_type: 'color', banner_value: e.target.value })} />
                    )}
                    {bannerType === 'image' && (
                        <input type="text" className="profile-customize-input" placeholder="https://example.com/banner.jpg" value={bannerValue} maxLength={2000}
                            onChange={(e) => saveSettings({ ...profileSettings, banner_type: 'image', banner_value: e.target.value })} />
                    )}
                    {bannerType === 'gradient' && (
                        <input type="text" className="profile-customize-input" placeholder="linear-gradient(135deg, #667eea, #764ba2)" value={bannerValue} maxLength={500}
                            onChange={(e) => saveSettings({ ...profileSettings, banner_type: 'gradient', banner_value: e.target.value })} />
                    )}
                    <div className="profile-customize-sub">
                        <label>Text Color:
                            <input type="color" value={bannerTextColor || '#ffcc00'}
                                onChange={(e) => saveSettings({ ...profileSettings, banner_text_color: e.target.value })} />
                        </label>
                        <button className="profile-customize-reset-btn" onClick={() => saveSettings({ ...profileSettings, banner_text_color: undefined })}>Reset</button>
                    </div>
                </div>

                <div className="profile-customize-section">
                    <h3>🖼️ Profile Background</h3>
                    <div className="profile-customize-row">
                        <label>
                            <input type="radio" name="bg" checked={bgType === 'default'} onChange={() => saveSettings({ ...profileSettings, background_type: 'default', background_value: '' })} /> Default
                        </label>
                        <label>
                            <input type="radio" name="bg" checked={bgType === 'color'} onChange={() => saveSettings({ ...profileSettings, background_type: 'color', background_value: '#1a1a2e' })} /> Color
                        </label>
                        <label>
                            <input type="radio" name="bg" checked={bgType === 'image'} onChange={() => saveSettings({ ...profileSettings, background_type: 'image', background_value: '' })} /> Image
                        </label>
                    </div>
                    {bgType === 'color' && (
                        <input
                            type="color"
                            className="profile-customize-color"
                            value={bgValue || '#1a1a2e'}
                            onChange={(e) => saveSettings({ ...profileSettings, background_type: 'color', background_value: e.target.value })}
                        />
                    )}
                    {bgType === 'image' && (
                        <input
                            type="text"
                            className="profile-customize-input"
                            placeholder="https://example.com/background.jpg"
                            value={bgValue}
                            maxLength={2000}
                            onChange={(e) => saveSettings({ ...profileSettings, background_type: 'image', background_value: e.target.value })}
                        />
                    )}
                    <div className="profile-bg-presets">
                        {BG_PRESETS.map((p) => (
                            <button key={p.label} className="profile-bg-preset-btn"
                                style={{ background: p.value } as React.CSSProperties}
                                onClick={() => saveSettings({ ...profileSettings, profile_background: p.value })}
                                title={p.label}
                            >{p.label}</button>
                        ))}
                        <button className="profile-bg-preset-btn" onClick={() => saveSettings({ ...profileSettings, profile_background: undefined })} title="Clear overlay">None</button>
                    </div>
                </div>

                <div className="profile-customize-section">
                    <h3>📰 Marquee Ticker</h3>
                    <div className="profile-customize-row">
                        <label>
                            <input
                                type="checkbox"
                                checked={marqueeEnabled}
                                onChange={(e) => saveSettings({ ...profileSettings, marquee_enabled: e.target.checked })}
                            /> Enable scrolling ticker
                        </label>
                    </div>
                    {marqueeEnabled && (
                        <>
                            <input
                                type="text"
                                className="profile-customize-input"
                                placeholder="Your scrolling message..."
                                value={profileSettings.marquee_text || ''}
                                maxLength={500}
                                onChange={(e) => saveSettings({ ...profileSettings, marquee_text: e.target.value })}
                            />
                            <div className="profile-customize-marquee-row">
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
                        </>
                    )}
                </div>
            </div>
        );
    }

    /* ════════════════════ Main Render ════════════════════ */

    /* Banner styling */
    const bannerType = profileSettings.banner_type || 'theme';
    const bannerValue = profileSettings.banner_value || '';
    const bannerTextColor = profileSettings.banner_text_color || '';
    const bannerStyle: React.CSSProperties = {};
    if (bannerType === 'color' && bannerValue) {
        bannerStyle.background = bannerValue;
    } else if (bannerType === 'image' && bannerValue) {
        bannerStyle.backgroundImage = `url(${bannerValue})`;
        bannerStyle.backgroundSize = 'cover';
        bannerStyle.backgroundPosition = 'center';
    } else if (bannerType === 'gradient' && bannerValue) {
        bannerStyle.background = bannerValue;
    }
    // 'theme' → no inline style, CSS inherits from theme variables
    if (bannerTextColor) {
        bannerStyle.color = bannerTextColor;
    }

    return (
        <div className="profile-container" style={containerStyle}>
            {/* ═══ Hero Banner ═══ */}
            <div className={`profile-hero ${bannerType === 'theme' ? 'profile-hero--themed' : ''}`} style={bannerStyle}>
                {bannerType === 'theme' && <div className="profile-hero-gradient" />}
                <div
                    className={`profile-avatar ${editMode ? 'profile-avatar--editable' : ''}`}
                    onClick={() => { if (editMode) setShowAvatarInput(!showAvatarInput); }}
                    title={editMode ? 'Click to change avatar' : undefined}
                >
                    {profileSettings.avatar_url && !avatarError ? (
                        <img src={profileSettings.avatar_url} alt="Avatar" onError={() => setAvatarError(true)} />
                    ) : displayName ? (
                        <span>{displayName[0].toUpperCase()}</span>
                    ) : (
                        <BsPersonFill size={32} />
                    )}
                    {editMode && <div className="profile-avatar-overlay">📷</div>}
                </div>

                <div className="profile-hero-info">
                    {profileSettings.nickname && (
                        <div className="profile-hero-nickname">{profileSettings.nickname}</div>
                    )}
                    {editMode && (
                        <input
                            className="profile-hero-nickname-input"
                            type="text"
                            placeholder="Add a nickname..."
                            value={profileSettings.nickname || ''}
                            maxLength={50}
                            onChange={(e) => saveSettings({ ...profileSettings, nickname: e.target.value || undefined })}
                        />
                    )}
                    <h1 className="profile-hero-name" style={bannerTextColor ? { color: bannerTextColor } as React.CSSProperties : undefined}>{displayName}</h1>
                    <div className="profile-hero-did">{auth?.web_did || auth?.did}</div>
                </div>

                {auth && !previewMode && (
                    <div className="profile-hero-actions">
                        {editMode && (
                            <button
                                className={`profile-hero-btn ${showCustomize ? 'profile-hero-btn--active' : ''}`}
                                onClick={() => setShowCustomize(!showCustomize)}
                            >
                                <BsPaletteFill size={14} /> Customize
                            </button>
                        )}
                        <button
                            className="profile-hero-btn"
                            onClick={() => setPreviewMode(true)}
                            title="Preview as external visitor"
                        >
                            👁 Preview
                        </button>
                        <button
                            className={`profile-hero-btn ${editMode ? 'profile-hero-btn--active' : ''}`}
                            onClick={() => { if (editMode) exitEditMode(); else setEditMode(true); }}
                        >
                            {editMode ? <><BsCheckLg size={14} /> Done</> : <><BsPencilFill size={14} /> Edit</>}
                        </button>
                    </div>
                )}
                {previewMode && (
                    <div className="profile-hero-actions">
                        <button className="profile-hero-btn profile-hero-btn--active" onClick={() => setPreviewMode(false)}>
                            ✕ Exit Preview
                        </button>
                    </div>
                )}

                {showAvatarInput && editMode && (
                    <div className="profile-avatar-popup">
                        <input
                            type="text"
                            placeholder="Avatar image URL..."
                            value={profileSettings.avatar_url || ''}
                            maxLength={2000}
                            onChange={(e) => {
                                setAvatarError(false);
                                saveSettings({ ...profileSettings, avatar_url: e.target.value || undefined });
                            }}
                            autoFocus
                        />
                        <button onClick={() => {
                            saveSettings({ ...profileSettings, avatar_url: undefined });
                            setAvatarError(false);
                            setShowAvatarInput(false);
                        }}>Clear</button>
                    </div>
                )}
            </div>

            {/* ═══ Customize Panel ═══ */}
            {showCustomize && editMode && renderCustomize()}

            {/* ═══ Main Layout ═══ */}
            <div className="profile-main">
                {/* Left Navigation */}
                <nav className="profile-nav">
                    <div className="profile-nav-heading">Navigation</div>
                    {CORE_NAV.map(section => (
                        <button
                            key={section.id}
                            className={`profile-nav-item ${activeNav === section.id ? 'profile-nav-item--active' : ''}`}
                            onClick={() => setActiveNav(section.id)}
                        >
                            <span className="profile-nav-emoji">{section.emoji}</span>
                            {section.label}
                        </button>
                    ))}

                    {/* Custom Pages */}
                    {customPages.length > 0 && <div className="profile-nav-divider" />}
                    {customPages.map(page => (
                        <div key={page.id} className="profile-nav-custom-row">
                            <button
                                className={`profile-nav-item ${activeNav === `custom-${page.id}` ? 'profile-nav-item--active' : ''}`}
                                onClick={() => setActiveNav(`custom-${page.id}`)}
                            >
                                <span className="profile-nav-emoji">{page.emoji}</span>
                                {page.label}
                            </button>
                            {editMode && (
                                <button className="profile-nav-remove-btn" onClick={() => removeCustomPage(page.id)} title="Remove page">✕</button>
                            )}
                        </div>
                    ))}

                    {editMode && (
                        <div className="profile-nav-add">
                            {showAddPageInput ? (
                                <div className="profile-nav-add-form">
                                    <input
                                        type="text"
                                        className="profile-nav-add-emoji-input"
                                        value={newPageEmoji}
                                        onChange={(e) => setNewPageEmoji(e.target.value.slice(0, 2))}
                                        maxLength={2}
                                        title="Page emoji"
                                    />
                                    <input
                                        type="text"
                                        className="profile-nav-add-label-input"
                                        placeholder="Page name..."
                                        value={newPageLabel}
                                        onChange={(e) => setNewPageLabel(e.target.value)}
                                        maxLength={30}
                                        onKeyDown={(e) => { if (e.key === 'Enter') addCustomPage(); }}
                                        autoFocus
                                    />
                                    <button className="profile-nav-add-confirm" onClick={addCustomPage} disabled={!newPageLabel.trim()}>✓</button>
                                    <button className="profile-nav-add-cancel" onClick={() => { setShowAddPageInput(false); setNewPageLabel(''); }}>✕</button>
                                </div>
                            ) : (
                                <button className="profile-nav-add-btn" onClick={() => setShowAddPageInput(true)}>
                                    <BsPlusLg size={10} /> Add Page
                                </button>
                            )}
                        </div>
                    )}
                </nav>

                {/* Content Area */}
                <div className="profile-content">
                    <div className="profile-content-inset">
                        {activeNav === 'home' && renderHome()}
                        {activeNav === 'posts' && renderPosts()}
                        {activeNav === 'media' && renderMedia()}
                        {activeNav.startsWith('custom-') && renderCustomPage(activeNav.replace('custom-', ''))}
                    </div>
                </div>
            </div>

            {/* ═══ Marquee Footer ═══ */}
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
