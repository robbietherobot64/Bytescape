import "./ProfileScreen.css";
import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { appDataDir } from '@tauri-apps/api/path';
import { AuthStatus, BytescapeItem, LinkedAccount, ProfileSettings, ProfileModule, ProfileCustomPage, VaultFile } from '../types';
import { FeedEntryCard } from '../components/FeedEntryCard';
import { adaptBytescapeItem } from '../utils/adapters';
import { platformEmoji, platformAbbrev } from '../utils/platform';
import { openInApp } from '../utils/openInApp';
import { BlogModule } from '../components/profile/BlogModule';
import { MediaGallery } from '../components/profile/MediaGallery';
import { ConnectionsModule } from '../components/profile/ConnectionsModule';
import { ModuleCard } from '../components/profile/ModuleCard';
import { BsPersonFill, BsPencilFill, BsCheckLg, BsPaletteFill, BsPlusLg, BsTrash, BsUpload, BsImages, BsLink45Deg } from 'react-icons/bs';

const PAGE_SIZE = 20;

/** Convert file:// URLs to Tauri asset protocol; pass HTTP URLs through unchanged */
function resolveImageUrl(url: string | undefined): string | undefined {
    if (!url) return undefined;
    if (url.startsWith('file://')) {
        // Strip file:// prefix and pass the path to convertFileSrc
        const path = url.replace(/^file:\/\//, '');
        return convertFileSrc(path);
    }
    return url;
}

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
function detectEmbed(input: string): { url: string; label: string; height: number } | null {
    const trimmed = input.trim();

    // Handle <iframe> embed codes — extract src attribute and optional height
    const iframeMatch = trimmed.match(/<iframe[^>]+src=["']([^"']+)["']/i);
    if (iframeMatch) {
        const src = iframeMatch[1];
        const hMatch = trimmed.match(/height=["']?(\d+)/i);
        const parsedH = hMatch ? parseInt(hMatch[1], 10) : 0;
        if (/bandcamp\.com/i.test(src)) return { url: src, label: 'Bandcamp', height: parsedH || 120 };
        if (/soundcloud\.com|sndcdn\.com/i.test(src)) return { url: src, label: 'SoundCloud', height: parsedH || 166 };
        if (/spotify\.com/i.test(src)) return { url: src, label: 'Spotify', height: parsedH || 152 };
        if (/youtube\.com|youtube-nocookie\.com/i.test(src)) return { url: src, label: 'YouTube', height: parsedH || 315 };
        return { url: src, label: 'Embed', height: parsedH || 152 };
    }

    // Handle raw URLs
    const bcMatch = trimmed.match(/^https?:\/\/([^.]+)\.bandcamp\.com\/(track|album)\/([^/?#]+)/i);
    if (bcMatch) return { url: `https://bandcamp.com/EmbeddedPlayer/${bcMatch[2]}=${bcMatch[3]}/size=large/bgcol=333333/linkcol=e99708/tracklist=false/transparent=true/`, label: `Bandcamp · ${bcMatch[1]}`, height: 470 };
    if (/^https?:\/\/(www\.)?soundcloud\.com\/[^/]+\/[^/]+/i.test(trimmed)) return { url: `https://w.soundcloud.com/player/?url=${encodeURIComponent(trimmed)}&color=%23ff5500&auto_play=false&hide_related=true&show_comments=false`, label: 'SoundCloud', height: 166 };
    const spMatch = trimmed.match(/^https?:\/\/open\.spotify\.com\/(track|album|playlist)\/([a-zA-Z0-9]+)/i);
    if (spMatch) return { url: `https://open.spotify.com/embed/${spMatch[1]}/${spMatch[2]}`, label: `Spotify · ${spMatch[1]}`, height: spMatch[1] === 'track' ? 152 : 352 };
    const ytMatch = trimmed.match(/(?:youtube\.com\/watch\?v=|youtu\.be\/)([a-zA-Z0-9_-]{11})/i);
    if (ytMatch) return { url: `https://www.youtube-nocookie.com/embed/${ytMatch[1]}`, label: 'YouTube', height: 315 };
    return null;
}

interface ProfileLink { label: string; url: string; }

interface BlogEntry {
    id: string;
    title: string;
    body: string;
    timestamp: number;
    external_url?: string;
    author?: string;
    from_feed?: boolean;
}

interface FeedBlogEntry {
    id: string;
    title: string;
    body: string;
    timestamp: number;
    external_url: string | null;
    author: string | null;
}

interface BlogFeedResult {
    feed_url: string;
    site_title: string | null;
    entries: FeedBlogEntry[];
}

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
    const [followingCount, setFollowingCount] = useState(0);
    const [followerCount, setFollowerCount] = useState(0);
    const [allConnections, setAllConnections] = useState<Array<{ did: string; handle: string; source: string; avatar_url?: string; display_name?: string; profile_url?: string; relation: 'following' | 'follower' | 'mutual' }>>([]);
    /* ─── Connection filter state moved to ConnectionsModule ─── */
    /* ─── Connection hover card state moved to ConnectionsModule ─── */

    /* ─── Customize Dropdown ─── */
    const [customizeDropdownOpen, setCustomizeDropdownOpen] = useState(false);
    const [customizePanel, setCustomizePanel] = useState<string | null>(null);
    const customizeDropdownRef = useRef<HTMLDivElement>(null);

    /* ─── Blog Editor (state moved to BlogModule component) ─── */

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

    /* ─── Media Lightbox ─── */
    const [lightboxFile, setLightboxFile] = useState<VaultFile | null>(null);
    const [lightboxUrl, setLightboxUrl] = useState<string | null>(null);
    const [lightboxLoading, setLightboxLoading] = useState(false);
    const [lightboxPostUrl, setLightboxPostUrl] = useState<string | null>(null);
    const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);

    /* ─── Avatar ─── */
    const [showAvatarInput, setShowAvatarInput] = useState(false);
    const [avatarError, setAvatarError] = useState(false);

    /* ─── Vault Picker (for avatar/background) ─── */
    const [vaultPickerMode, setVaultPickerMode] = useState<'avatar' | 'background' | null>(null);
    const [vaultPickerFiles, setVaultPickerFiles] = useState<VaultFile[]>([]);
    const [vaultPickerThumbs, setVaultPickerThumbs] = useState<Record<string, string>>({});
    const [vaultPickerLoading, setVaultPickerLoading] = useState(false);



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

    // Auto-refresh blog feeds on profile load (max once per hour)
    // Uses a ref flag to run only once after initial load, avoiding stale closure issues
    const feedRefreshRan = useRef(false);
    useEffect(() => {
        if (modules.length === 0 || feedRefreshRan.current) return;
        feedRefreshRan.current = true;
        const allMods = [...modules];
        customPages.forEach(p => allMods.push(...p.modules));
        for (const mod of allMods) {
            if (mod.type !== 'blogs') continue;
            try {
                const parsed = JSON.parse(mod.content || '{}');
                const blogData = Array.isArray(parsed) ? null : parsed;
                if (!blogData?.feed?.feed_url) continue;
                const lastFetched = blogData.feed.last_fetched || 0;
                const hourAgo = Math.floor(Date.now() / 1000) - 3600;
                if (lastFetched > hourAgo) continue;
                const modId = mod.id;
                invoke<BlogFeedResult>('fetch_blog_feed', { url: blogData.feed.feed_url })
                    .then(result => {
                        const existingUrls = new Set((blogData.entries || []).filter((e: BlogEntry) => e.external_url).map((e: BlogEntry) => e.external_url));
                        const newEntries: BlogEntry[] = result.entries
                            .filter(fe => !fe.external_url || !existingUrls.has(fe.external_url))
                            .map(fe => ({
                                id: fe.id, title: fe.title, body: fe.body, timestamp: fe.timestamp,
                                external_url: fe.external_url || undefined, author: fe.author || undefined, from_feed: true,
                            }));
                        if (newEntries.length === 0) {
                            console.log('[blog] auto-refresh: no new entries');
                            return;
                        }
                        const merged = [...newEntries, ...(blogData.entries || [])].sort((a: BlogEntry, b: BlogEntry) => b.timestamp - a.timestamp);
                        const newBlogContent = JSON.stringify({ entries: merged, feed: { ...blogData.feed, last_fetched: Math.floor(Date.now() / 1000) } });
                        // Directly read fresh settings from backend to avoid stale closure
                        invoke<string | null>('get_profile_settings').then(raw => {
                            if (!raw) return;
                            try {
                                const fresh = JSON.parse(raw);
                                const updatedModules = (fresh.modules || []).map((m: ProfileModule) => m.id === modId ? { ...m, content: newBlogContent } : m);
                                const updatedPages = (fresh.custom_pages || []).map((p: ProfileCustomPage) => ({
                                    ...p,
                                    modules: p.modules.map((m: ProfileModule) => m.id === modId ? { ...m, content: newBlogContent } : m)
                                }));
                                const updated = { ...fresh, modules: updatedModules, custom_pages: updatedPages };
                                invoke('save_profile_settings', { settingsJson: JSON.stringify(updated) });
                                setModules(updatedModules);
                                setCustomPages(updatedPages);
                                setProfileSettings(updated);
                                console.log('[blog] auto-refresh: added', newEntries.length, 'new entries (fresh save)');
                            } catch { /* ignore */ }
                        });
                    })
                    .catch(err => console.warn('[blog] auto-refresh failed:', err));
            } catch { /* not a blog module with feed */ }
        }
    // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [modules.length]);

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
        const conns: Array<{ did: string; handle: string; source: string; avatar_url?: string; display_name?: string; profile_url?: string; relation: 'following' | 'follower' | 'mutual' }> = [];
        const promises: Promise<void>[] = [];

        const addProfileToConns = (f: { did: string; handle: string; displayName?: string; avatar?: string }, relation: 'following' | 'follower' | 'mutual') => {
            const displayName = f.displayName || f.handle || f.did.slice(0, 12);
            const isMastodon = f.did?.startsWith('did:web:') || f.handle?.includes('@');
            const source = isMastodon ? 'Mastodon' : 'Bluesky';
            const profileUrl = isMastodon
                ? `https://${f.handle?.split('@').pop() || ''}/@${f.handle?.split('@')[0] || ''}`
                : `https://bsky.app/profile/${f.handle || f.did}`;
            // Deduplicate: check if already added
            const existing = conns.find(c => c.did === f.did);
            if (existing) {
                if (relation === 'follower' && existing.relation === 'following') existing.relation = 'mutual';
                if (relation === 'following' && existing.relation === 'follower') existing.relation = 'mutual';
                // Merge avatar/display_name if the existing entry is missing them
                if (!existing.avatar_url && f.avatar) existing.avatar_url = f.avatar;
                if (!existing.display_name && f.displayName) existing.display_name = f.displayName;
                return;
            }
            conns.push({
                did: f.did,
                handle: displayName,
                source,
                avatar_url: f.avatar || undefined,
                display_name: f.displayName || undefined,
                profile_url: profileUrl,
                relation,
            });
        };

        if (auth?.did) {
            // Following
            promises.push(
                invoke<Array<{ did: string; handle: string; displayName?: string; avatar?: string; description?: string }>>('list_follows', { actor: auth.did })
                    .then((follows) => {
                        setFollowingCount(follows.length);
                        follows.forEach(f => addProfileToConns(f, 'following'));
                    })
                    .catch(() => {})
            );
            // Followers
            promises.push(
                invoke<Array<{ did: string; handle: string; displayName?: string; avatar?: string; description?: string }>>('list_followers', { actor: auth.did })
                    .then((followers) => {
                        setFollowerCount(followers.length);
                        followers.forEach(f => addProfileToConns(f, 'follower'));
                    })
                    .catch(() => { setFollowerCount(0); })
            );
        }
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
            // Clean up vault media not authored by the current user
            if (auth?.did) {
                await invoke('vault_cleanup_repost_media', { userDid: auth.did }).catch(() => {});
            }
            const files = await invoke<VaultFile[]>('vault_list_files');
            const visual = files.filter(f => f.kind === 'Image' || f.kind === 'Video');
            setMediaFiles(visual);
            // Fetch thumbnails: prefer dedicated thumbnail, fall back to full image
            const thumbs: Record<string, string> = {};
            await Promise.all(visual.map(async (f) => {
                if (f.kind !== 'Image') return;
                try {
                    if (f.thumbnail_hash) {
                        const bytes = await invoke<number[]>('vault_get_thumbnail', { fileId: f.id });
                        if (bytes) {
                            const blob = new Blob([new Uint8Array(bytes)], { type: 'image/webp' });
                            thumbs[f.id] = URL.createObjectURL(blob);
                            return;
                        }
                    }
                    // No thumbnail hash or thumbnail fetch failed — use full image
                    const bytes = await invoke<number[]>('vault_get_file_data', { fileId: f.id });
                    if (bytes) {
                        const blob = new Blob([new Uint8Array(bytes)], { type: f.mime_type || 'image/jpeg' });
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
        const maxOrder = modules.reduce((max, m) => Math.max(max, m.order), -1);
        const newMod: ProfileModule = {
            id: generateId(),
            type,
            title,
            order: maxOrder + 1,
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

    const updateModuleContent = useCallback((id: string, content: string, persist = false) => {
        const updated = modules.map(m => m.id === id ? { ...m, content } : m);
        if (persist) {
            saveModules(updated);
        } else {
            setModules(updated);
        }
    }, [modules, saveModules]);

    const updateModuleTitle = useCallback((id: string, title: string) => {
        const updated = modules.map(m => m.id === id ? { ...m, title } : m);
        setModules(updated);
    }, [modules]);

    const updateModuleStyle = useCallback((id: string, style: Partial<ProfileModule>) => {
        const updated = modules.map(m => m.id === id ? { ...m, ...style } : m);
        saveModules(updated);
    }, [modules, saveModules]);

    const toggleModuleWidth = useCallback((id: string) => {
        const updated = modules.map(m => {
            if (m.id !== id) return m;
            if (m.width === 'full') return { ...m, width: 'half' as const, column: (m.column ?? 0) as 0 | 1 };
            return { ...m, width: 'full' as const };
        });
        saveModules(updated);
    }, [modules, saveModules]);

    // Move module up/down within its column
    const moveModule = useCallback((id: string, direction: 'up' | 'down') => {
        const mod = modules.find(m => m.id === id);
        if (!mod) return;
        // Group-scoped: full-width among full-width, half among same column
        const siblings = modules
            .filter(m => m.visible)
            .filter(m => mod.width === 'full'
                ? m.width === 'full'
                : (m.width === 'half' && (m.column ?? 0) === (mod.column ?? 0)))
            .sort((a, b) => a.order - b.order);
        const sibIdx = siblings.findIndex(m => m.id === id);
        if (sibIdx === -1) return;
        const targetSibIdx = direction === 'up' ? sibIdx - 1 : sibIdx + 1;
        if (targetSibIdx < 0 || targetSibIdx >= siblings.length) return;
        // Normalize: reassign contiguous orders to this group, then swap
        const updated = [...modules];
        siblings.forEach((sib, i) => {
            const m = updated.find(u => u.id === sib.id)!;
            m.order = i;
        });
        const a = updated.find(m => m.id === siblings[sibIdx].id)!;
        const b = updated.find(m => m.id === siblings[targetSibIdx].id)!;
        const tmp = a.order;
        a.order = b.order;
        b.order = tmp;
        saveModules(updated);
    }, [modules, saveModules]);

    // Move module to other column
    const moveModuleColumn = useCallback((id: string) => {
        const updated = modules.map(m => {
            if (m.id !== id || m.width === 'full') return m;
            return { ...m, column: ((m.column ?? 0) === 0 ? 1 : 0) as 0 | 1 };
        });
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

    // Click-outside to close customize dropdown
    useEffect(() => {
        if (!customizeDropdownOpen) return;
        function handleClick(e: MouseEvent) {
            if (customizeDropdownRef.current && !customizeDropdownRef.current.contains(e.target as Node)) {
                setCustomizeDropdownOpen(false);
            }
        }
        document.addEventListener('mousedown', handleClick);
        return () => document.removeEventListener('mousedown', handleClick);
    }, [customizeDropdownOpen]);

    // Save modules on edit mode exit
    const exitEditMode = useCallback(() => {
        setEditMode(false);
        setCustomizeDropdownOpen(false);
        setCustomizePanel(null);
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
    const activePreset = PROFILE_THEME_PRESETS[profileSettings.theme || 'default'] || PROFILE_THEME_PRESETS.default;
    // Set tooltip CSS custom properties from active theme
    (containerStyle as Record<string, string>)['--profile-tooltip-bg'] = activePreset.banner;
    (containerStyle as Record<string, string>)['--profile-tooltip-text'] = activePreset.text;
    (containerStyle as Record<string, string>)['--profile-tooltip-border'] = activePreset.accent;
    if (profileSettings.profile_background) {
        containerStyle.background = profileSettings.profile_background;
    } else if (bgType === 'color' && bgValue) {
        containerStyle.background = bgValue;
    } else if (bgType === 'image' && bgValue) {
        containerStyle.backgroundImage = `url(${resolveImageUrl(bgValue) || bgValue})`;
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

    /* ════════════════════ Upload & Vault Picker ════════════════════ */

    const openVaultPicker = useCallback(async (mode: 'avatar' | 'background') => {
        setVaultPickerMode(mode);
        setVaultPickerLoading(true);
        try {
            const files = await invoke<VaultFile[]>('vault_list_files');
            const images = files.filter(f => f.kind === 'Image');
            setVaultPickerFiles(images);
            // Load thumbnails: prefer dedicated thumbnail, fall back to full image
            const thumbs: Record<string, string> = {};
            await Promise.all(images.slice(0, 60).map(async (f) => {
                try {
                    if (f.thumbnail_hash) {
                        const bytes = await invoke<number[]>('vault_get_thumbnail', { fileId: f.id });
                        if (bytes) {
                            const blob = new Blob([new Uint8Array(bytes)], { type: 'image/webp' });
                            thumbs[f.id] = URL.createObjectURL(blob);
                            return;
                        }
                    }
                    const bytes = await invoke<number[]>('vault_get_file_data', { fileId: f.id });
                    if (bytes) {
                        const blob = new Blob([new Uint8Array(bytes)], { type: f.mime_type || 'image/jpeg' });
                        thumbs[f.id] = URL.createObjectURL(blob);
                    }
                } catch { /* skip */ }
            }));
            setVaultPickerThumbs(thumbs);
        } catch (e) {
            console.error('Failed to load vault images:', e);
        } finally {
            setVaultPickerLoading(false);
        }
    }, []);

    const pickVaultFile = useCallback(async (file: VaultFile) => {
        // Get full file data and create a usable URL
        try {
            const bytes = await invoke<number[]>('vault_get_file_data', { fileId: file.id });
            if (!bytes) return;
            const blob = new Blob([new Uint8Array(bytes)], { type: file.mime_type });
            const blobUrl = URL.createObjectURL(blob);

            if (vaultPickerMode === 'avatar') {
                // Save via save_avatar_image for persistent storage
                const url = await invoke<string>('save_avatar_image', {
                    imageData: bytes,
                    mimeType: file.mime_type,
                });
                saveSettings({ ...profileSettings, avatar_url: url });
                setAvatarError(false);
                setShowAvatarInput(false);
            } else if (vaultPickerMode === 'background') {
                // For background, save the bytes as avatar-style persistent file and use that URL
                const url = await invoke<string>('save_avatar_image', {
                    imageData: bytes,
                    mimeType: file.mime_type,
                });
                saveSettings({ ...profileSettings, background_type: 'image', background_value: url });
            }
            URL.revokeObjectURL(blobUrl);
        } catch (e) {
            console.error('Failed to pick vault file:', e);
        }
        setVaultPickerMode(null);
        setVaultPickerFiles([]);
        // Clean up thumb URLs
        Object.values(vaultPickerThumbs).forEach(u => URL.revokeObjectURL(u));
        setVaultPickerThumbs({});
    }, [vaultPickerMode, profileSettings, saveSettings, vaultPickerThumbs]);

    const uploadImageFromDisk = useCallback(async (target: 'avatar' | 'background') => {
        try {
            const selected = await open({
                multiple: false,
                title: target === 'avatar' ? 'Choose a profile picture' : 'Choose a background image',
                filters: [{
                    name: 'Images',
                    extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif'],
                }],
            });
            if (!selected) return;
            const path = Array.isArray(selected) ? selected[0] : selected;
            if (!path) return;

            // Upload to vault first, then use as avatar/background
            const uploaded = await invoke<VaultFile[]>('vault_upload_files_from_paths', {
                paths: [path],
                tags: [target === 'avatar' ? 'profile-avatar' : 'profile-background'],
            });
            if (!uploaded || uploaded.length === 0) return;
            const file = uploaded[0];

            // Get the file data and save persistently
            const bytes = await invoke<number[]>('vault_get_file_data', { fileId: file.id });
            if (!bytes) return;

            const url = await invoke<string>('save_avatar_image', {
                imageData: bytes,
                mimeType: file.mime_type,
            });

            if (target === 'avatar') {
                saveSettings({ ...profileSettings, avatar_url: url });
                setAvatarError(false);
                setShowAvatarInput(false);
            } else {
                saveSettings({ ...profileSettings, background_type: 'image', background_value: url });
            }
        } catch (e) {
            console.error('Upload failed:', e);
        }
    }, [profileSettings, saveSettings]);

    /* ════════════════════ Module Renderers ════════════════════ */

    function renderModuleContent(mod: ProfileModule) {
        if (editMode && editingModuleContent === mod.id) {
            if (mod.type === 'blogs') return renderBlogs(mod);
            if (mod.type === 'links') return renderLinksEditor(mod);
            if (mod.type === 'gif-collection') return renderGifEditor(mod);
            if (mod.type === 'playlists') {
                return (
                    <textarea
                        className="profile-module-editor"
                        value={mod.content || ''}
                        onChange={(e) => updateModuleContent(mod.id, e.target.value)}
                        placeholder="Paste Bandcamp, SoundCloud, Spotify, or YouTube URLs or iframe embed codes (one per line)..."
                        autoFocus
                    />
                );
            }
            if (mod.type === 'listening-to') {
                return (
                    <textarea
                        className="profile-module-editor"
                        value={mod.content || ''}
                        onChange={(e) => updateModuleContent(mod.id, e.target.value)}
                        placeholder="Paste a Spotify, YouTube, Bandcamp, or SoundCloud link or embed code..."
                        autoFocus
                    />
                );
            }
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
            case 'blogs': return renderBlogs(mod);
            case 'playlists': return renderPlaylist(mod);
            case 'check-ins':
            case 'polls':
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
        const protocolUrl = (acct: LinkedAccount): string | null => {
            const p = acct.protocol.toLowerCase();
            const h = acct.display_handle.replace(/^@/, '');
            if (p === 'bluesky' || p === 'atproto') return `https://bsky.app/profile/${h}`;
            if (p === 'mastodon' || p === 'activitypub') {
                const parts = h.split('@');
                return parts.length >= 2 ? `https://${parts[1]}/@${parts[0]}` : null;
            }
            if (p === 'instagram') return `https://instagram.com/${h}`;
            return null;
        };
        if (linkedAccounts.length === 0) {
            return <div className="profile-empty"><div className="profile-empty-icon">🔗</div><p className="profile-empty-text">No connected accounts yet.</p></div>;
        }
        return (
            <div className="profile-identities-list">
                {linkedAccounts.map((acct, i) => {
                    const url = protocolUrl(acct);
                    const inner = (
                        <div key={i} className={`profile-identity-item ${url ? 'profile-identity-item--clickable' : ''}`}>
                            <span className="profile-identity-emoji">{protocolEmoji[acct.protocol] || '🔗'}</span>
                            <div className="profile-identity-info">
                                <div className="profile-identity-protocol">{acct.protocol}</div>
                                <div className="profile-identity-handle">{acct.display_handle}</div>
                            </div>
                            {acct.active && <span className="profile-identity-badge profile-identity-badge--active">● Active</span>}
                        </div>
                    );
                    if (url) {
                        return <a key={i} href={url} target="_blank" rel="noopener noreferrer" className="profile-identity-link">{inner}</a>;
                    }
                    return inner;
                })}
            </div>
        );
    }

    function renderConnections() {
        return (
            <ConnectionsModule
                allConnections={allConnections}
                setAllConnections={setAllConnections}
                followingCount={followingCount}
                setFollowingCount={setFollowingCount}
                followerCount={followerCount}
                authDid={auth?.did}
            />
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
                            style={{ height: embed.height } as React.CSSProperties}
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
            if (editMode) {
                return (
                    <div className="profile-empty">
                        <div className="profile-empty-icon">🖼️</div>
                        <p className="profile-empty-text">No GIFs yet.</p>
                        <button
                            className="profile-module-edit-btn"
                            style={{ width: 'auto', padding: '6px 14px', marginTop: 8, fontSize: 12 } as React.CSSProperties}
                            onClick={() => setEditingModuleContent(mod.id)}
                        >
                            <BsPlusLg size={9} /> Add GIFs
                        </button>
                    </div>
                );
            }
            return <div className="profile-empty"><div className="profile-empty-icon">🖼️</div><p className="profile-empty-text">No GIFs yet.</p></div>;
        }
        return (
            <div className="profile-gif-grid">
                {gifs.map((url, i) => <img key={i} src={url} alt="" className="profile-gif-item" loading="lazy" />)}
                {editMode && (
                    <button
                        className="profile-gif-add-tile"
                        onClick={() => setEditingModuleContent(mod.id)}
                        title="Add more GIFs"
                    >
                        <BsPlusLg size={16} />
                    </button>
                )}
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
                        <input
                            className="profile-link-input"
                            type="text"
                            placeholder="Paste a GIF URL (e.g. https://i.imgur.com/abc.gif)"
                            value={url}
                            onChange={(e) => updateGif(i, e.target.value)}
                            onPaste={(e) => {
                                const text = e.clipboardData.getData('text').trim();
                                if (text && !url) {
                                    e.preventDefault();
                                    updateGif(i, text);
                                }
                            }}
                        />
                        {url && <img src={url} alt="" style={{ width: 40, height: 40, objectFit: 'cover', borderRadius: 3, flexShrink: 0 } as React.CSSProperties} onError={(e) => { (e.target as HTMLImageElement).style.display = 'none'; }} />}
                        <button className="profile-module-edit-btn profile-module-edit-btn--danger" onClick={() => removeGif(i)}>✕</button>
                    </div>
                ))}
                <button className="profile-module-edit-btn" onClick={addGif} style={{ width: 'auto', padding: '4px 10px', marginTop: 4 } as React.CSSProperties}>
                    <BsPlusLg size={9} /> Add GIF
                </button>
                {gifs.length === 0 && (
                    <p style={{ fontSize: 11, color: 'rgba(255,255,255,0.4)', marginTop: 6 } as React.CSSProperties}>
                        Paste GIF URLs from Tenor, Giphy, Imgur, etc.
                    </p>
                )}
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

    function renderPlaylist(mod: ProfileModule) {
        if (!mod.content) {
            return <div className="profile-empty"><div className="profile-empty-icon">🎶</div><p className="profile-empty-text">{editMode ? 'Click ✏️ to add playlist URLs or embeds...' : 'No playlists yet.'}</p></div>;
        }
        const lines = mod.content.split('\n').filter(l => l.trim());
        // Each "line" might be a multi-line iframe tag — rejoin and split on <iframe
        const rawContent = mod.content.trim();
        const entries: string[] = [];
        // Split by <iframe to handle pasted iframe blocks
        if (rawContent.includes('<iframe')) {
            const parts = rawContent.split(/(?=<iframe)/i);
            parts.forEach(p => {
                const t = p.trim();
                if (t.startsWith('<iframe')) {
                    entries.push(t);
                } else {
                    // Regular URL lines
                    t.split('\n').filter(l => l.trim()).forEach(l => entries.push(l.trim()));
                }
            });
        } else {
            lines.forEach(l => entries.push(l.trim()));
        }
        const embeds = entries.map(l => detectEmbed(l)).filter(Boolean) as Array<{ url: string; label: string; height: number }>;
        if (embeds.length === 0) {
            return <div className="profile-listening-text"><span>🎶</span><span>{mod.content}</span></div>;
        }
        return (
            <div className="profile-playlist-stack">
                {embeds.map((embed, i) => (
                    <div key={i} className="profile-playlist-item">
                        <div className="profile-listening-embed">
                            <iframe src={embed.url} title={embed.label} style={{ height: embed.height } as React.CSSProperties} allow="autoplay; encrypted-media" sandbox="allow-scripts allow-same-origin allow-popups" />
                        </div>
                        <div className="profile-listening-source">{embed.label}</div>
                    </div>
                ))}
            </div>
        );
    }

    function blogSaveDirect(modId: string, newContent: string) {
        // Read fresh settings from backend to avoid stale closure issues,
        // then update the module content and save back
        invoke<string | null>('get_profile_settings').then(raw => {
            const base = raw ? JSON.parse(raw) : {};
            const updatedModules = (base.modules || modules).map((m: ProfileModule) =>
                m.id === modId ? { ...m, content: newContent } : m
            );
            const updatedPages = (base.custom_pages || customPages).map((p: ProfileCustomPage) => ({
                ...p,
                modules: p.modules.map((m: ProfileModule) =>
                    m.id === modId ? { ...m, content: newContent } : m
                )
            }));
            const updated = { ...base, modules: updatedModules, custom_pages: updatedPages };
            setModules(updatedModules);
            setCustomPages(updatedPages);
            setProfileSettings(updated);
            invoke('save_profile_settings', { settingsJson: JSON.stringify(updated) })
                .then(() => console.log('[blog] persisted to backend OK for', modId))
                .catch(err => console.error('[blog] backend save failed:', err));
        }).catch(err => {
            console.error('[blog] failed to read settings for save:', err);
            // Fallback: save with current React state
            const foundInMain = modules.some(m => m.id === modId);
            if (foundInMain) {
                const newModules = modules.map(m => m.id === modId ? { ...m, content: newContent } : m);
                setModules(newModules);
                const freshSettings = { ...profileSettings, modules: newModules, custom_pages: customPages };
                setProfileSettings(freshSettings);
                invoke('save_profile_settings', { settingsJson: JSON.stringify(freshSettings) });
            } else {
                const newPages = customPages.map(p => ({
                    ...p,
                    modules: p.modules.map(m => m.id === modId ? { ...m, content: newContent } : m)
                }));
                setCustomPages(newPages);
                const freshSettings = { ...profileSettings, modules, custom_pages: newPages };
                setProfileSettings(freshSettings);
                invoke('save_profile_settings', { settingsJson: JSON.stringify(freshSettings) });
            }
        });
    }

    function renderBlogs(mod: ProfileModule) {
        return <BlogModule mod={mod} editMode={editMode} onSaveDirect={blogSaveDirect} />;
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
        const fullWidthModules = visibleModules.filter(m => m.width === 'full');
        const leftModules = visibleModules.filter(m => m.width === 'half' && (m.column ?? 0) === 0);
        const rightModules = visibleModules.filter(m => m.width === 'half' && (m.column ?? 0) === 1);

        function renderModuleCard(mod: ProfileModule) {
            const group = mod.width === 'full'
                ? fullWidthModules
                : (mod.column ?? 0) === 0 ? leftModules : rightModules;
            return (
                <ModuleCard
                    key={mod.id}
                    mod={mod}
                    groupModules={group}
                    editMode={editMode}
                    previewMode={previewMode}
                    editingModuleTitle={editingModuleTitle}
                    editingModuleContent={editingModuleContent}
                    editingModuleStyle={editingModuleStyle}
                    onSetEditMode={setEditMode}
                    onSetEditingModuleTitle={setEditingModuleTitle}
                    onSetEditingModuleContent={setEditingModuleContent}
                    onSetEditingModuleStyle={setEditingModuleStyle}
                    onUpdateModuleTitle={updateModuleTitle}
                    onUpdateModuleStyle={updateModuleStyle}
                    onMoveModule={moveModule}
                    onMoveModuleColumn={moveModuleColumn}
                    onRemoveModule={removeModule}
                    onToggleModuleWidth={toggleModuleWidth}
                    renderContent={renderModuleContent}
                />
            );
        }

        return (
            <div className="profile-module-layout">
                {/* Full-width modules at top */}
                {fullWidthModules.map(mod => renderModuleCard(mod))}

                {/* Two independent columns */}
                <div className="profile-module-columns">
                    <div className="profile-module-column">
                        {leftModules.map(mod => renderModuleCard(mod))}
                    </div>
                    <div className="profile-module-column">
                        {rightModules.map(mod => renderModuleCard(mod))}
                    </div>
                </div>

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
                                previewDisplayName={previewMode ? (profileSettings.nickname || auth?.handle || 'User') : undefined}
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

    function openLightbox(file: VaultFile) {
        setLightboxFile(file);
        setLightboxUrl(null);
        setLightboxLoading(true);
        setLightboxPostUrl(null);
        invoke<number[]>('vault_get_file_data', { fileId: file.id })
            .then(bytes => {
                const blob = new Blob([new Uint8Array(bytes)], { type: file.mime_type });
                setLightboxUrl(URL.createObjectURL(blob));
            })
            .catch(e => console.error('Lightbox load failed:', e))
            .finally(() => setLightboxLoading(false));
        // Fetch linked post URL if item_id exists
        if (file.item_id) {
            invoke<BytescapeItem | null>('vault_get_item', { itemId: file.item_id })
                .then(item => {
                    if (item?.url) {
                        // Convert AT Protocol URIs to web URLs
                        const atMatch = item.url.match(/^at:\/\/([^/]+)\/app\.bsky\.feed\.post\/(.+)$/);
                        setLightboxPostUrl(atMatch ? `https://bsky.app/profile/${atMatch[1]}/post/${atMatch[2]}` : item.url);
                    }
                })
                .catch(() => { /* no linked post */ });
        }
    }

    function closeLightbox() {
        if (lightboxUrl && lightboxUrl.startsWith('blob:')) URL.revokeObjectURL(lightboxUrl);
        setLightboxFile(null);
        setLightboxUrl(null);
        setLightboxPostUrl(null);
    }

    async function deleteMediaFile(fileId: string) {
        try {
            await invoke('vault_delete_file', { fileId });
            setMediaFiles(prev => prev.filter(f => f.id !== fileId));
            // Clean up thumbnail blob URL
            const thumbUrl = mediaThumbs[fileId];
            if (thumbUrl && thumbUrl.startsWith('blob:')) URL.revokeObjectURL(thumbUrl);
            setMediaThumbs(prev => {
                const next = { ...prev };
                delete next[fileId];
                return next;
            });
            closeLightbox();
        } catch (e) {
            console.error('Failed to delete media file:', e);
        }
    }

    function lightboxNav(dir: -1 | 1) {
        if (!lightboxFile) return;
        const idx = mediaFiles.findIndex(f => f.id === lightboxFile.id);
        const next = mediaFiles[idx + dir];
        if (next) openLightbox(next);
    }

    function renderMedia() {
        return <MediaGallery mediaFiles={mediaFiles} mediaThumbs={mediaThumbs} mediaLoading={mediaLoading} onOpenLightbox={openLightbox} />;
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

    function renderCustomizeSection(section: string) {
        if (section === 'theme') {
            return (
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
            );
        }

        if (section === 'banner') {
            return (
                <div>
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
            );
        }

        if (section === 'background') {
            return (
                <div>
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
                        <div className="profile-bg-image-controls">
                            <div className="profile-bg-upload-row">
                                <button className="profile-upload-btn" onClick={() => uploadImageFromDisk('background')}>
                                    <BsUpload size={11} /> Upload
                                </button>
                                <button className="profile-upload-btn" onClick={() => openVaultPicker('background')}>
                                    <BsImages size={11} /> From Vault
                                </button>
                            </div>
                            <div className="profile-bg-url-row">
                                <BsLink45Deg size={12} style={{ flexShrink: 0, color: 'rgba(255,255,255,0.4)' } as React.CSSProperties} />
                                <input
                                    type="text"
                                    className="profile-customize-input"
                                    placeholder="Or paste image URL..."
                                    value={bgValue}
                                    maxLength={2000}
                                    onChange={(e) => saveSettings({ ...profileSettings, background_type: 'image', background_value: e.target.value })}
                                />
                            </div>
                        </div>
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
            );
        }

        if (section === 'marquee') {
            return (
                <div>
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
            );
        }

        return null;
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
        bannerStyle.backgroundImage = `url(${resolveImageUrl(bannerValue) || bannerValue})`;
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
                        <img src={resolveImageUrl(profileSettings.avatar_url)} alt="Avatar" onError={() => setAvatarError(true)} />
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
                            placeholder="Display name..."
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
                            <div className="profile-customize-dropdown-wrapper" ref={customizeDropdownRef}>
                                <button
                                    className={`profile-hero-btn ${customizeDropdownOpen ? 'profile-hero-btn--active' : ''}`}
                                    onClick={() => { setCustomizeDropdownOpen(!customizeDropdownOpen); if (customizeDropdownOpen) setCustomizePanel(null); }}
                                >
                                    <BsPaletteFill size={14} /> Customize ▾
                                </button>
                                {customizeDropdownOpen && (
                                    <div className="profile-customize-dropdown">
                                        <button onClick={() => setCustomizePanel(customizePanel === 'theme' ? null : 'theme')}>🎭 Profile Theme</button>
                                        <button onClick={() => setCustomizePanel(customizePanel === 'banner' ? null : 'banner')}>🎨 Banner Header</button>
                                        <button onClick={() => setCustomizePanel(customizePanel === 'background' ? null : 'background')}>🖼️ Background</button>
                                        <button onClick={() => setCustomizePanel(customizePanel === 'marquee' ? null : 'marquee')}>📰 Marquee Ticker</button>
                                    </div>
                                )}
                            </div>
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
                        <div className="profile-avatar-popup-actions">
                            <button className="profile-upload-btn" onClick={() => uploadImageFromDisk('avatar')}>
                                <BsUpload size={11} /> Upload
                            </button>
                            <button className="profile-upload-btn" onClick={() => openVaultPicker('avatar')}>
                                <BsImages size={11} /> Vault
                            </button>
                        </div>
                        <div className="profile-avatar-popup-url">
                            <BsLink45Deg size={12} />
                            <input
                                type="text"
                                placeholder="Or paste image URL..."
                                value={profileSettings.avatar_url || ''}
                                maxLength={2000}
                                onChange={(e) => {
                                    setAvatarError(false);
                                    saveSettings({ ...profileSettings, avatar_url: e.target.value || undefined });
                                }}
                            />
                        </div>
                        <button className="profile-avatar-popup-clear" onClick={() => {
                            saveSettings({ ...profileSettings, avatar_url: undefined });
                            setAvatarError(false);
                            setShowAvatarInput(false);
                        }}>Clear</button>
                    </div>
                )}
            </div>

            {/* ═══ Customize Popup Panel ═══ */}
            {customizePanel && editMode && (
                <div className="profile-customize-popup">
                    <div className="profile-customize-popup-header">
                        <span>{customizePanel === 'theme' ? '🎭 Profile Theme' : customizePanel === 'banner' ? '🎨 Banner Header' : customizePanel === 'background' ? '🖼️ Background' : '📰 Marquee Ticker'}</span>
                        <button className="profile-customize-popup-close" onClick={() => setCustomizePanel(null)}>✕</button>
                    </div>
                    <div className="profile-customize-popup-body">
                        {renderCustomizeSection(customizePanel)}
                    </div>
                </div>
            )}

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

            {/* ═══ Media Lightbox ═══ */}
            {lightboxFile && (
                <div className="profile-lightbox-backdrop" onClick={closeLightbox}>
                    <div className="profile-lightbox" onClick={e => e.stopPropagation()}>
                        <div className="profile-lightbox-topbar">
                            <span className="profile-lightbox-name" title={lightboxFile.name}>{lightboxFile.name}</span>
                            <button className="profile-lightbox-delete" onClick={() => setConfirmDeleteId(lightboxFile.id)} title="Delete">🗑️</button>
                            <button className="profile-lightbox-close" onClick={closeLightbox}>✕</button>
                        </div>
                        <div className="profile-lightbox-content">
                            {lightboxLoading && <div className="profile-lightbox-loading">⏳</div>}
                            {!lightboxLoading && lightboxUrl && lightboxFile.kind === 'Video' && (
                                <video className="profile-lightbox-video" src={lightboxUrl} controls autoPlay />
                            )}
                            {!lightboxLoading && lightboxUrl && lightboxFile.kind !== 'Video' && (
                                <img className="profile-lightbox-img" src={lightboxUrl} alt={lightboxFile.name} />
                            )}
                            {!lightboxLoading && !lightboxUrl && (
                                <div className="profile-lightbox-loading">Failed to load</div>
                            )}
                            {mediaFiles.findIndex(f => f.id === lightboxFile.id) > 0 && (
                                <button className="profile-lightbox-nav profile-lightbox-nav--prev" onClick={() => lightboxNav(-1)}>‹</button>
                            )}
                            {mediaFiles.findIndex(f => f.id === lightboxFile.id) < mediaFiles.length - 1 && (
                                <button className="profile-lightbox-nav profile-lightbox-nav--next" onClick={() => lightboxNav(1)}>›</button>
                            )}
                        </div>
                        <div className="profile-lightbox-info">
                            {lightboxPostUrl && (
                                <div className="profile-lightbox-info-row">
                                    <span className="profile-lightbox-info-label">Post</span>
                                    <span className="profile-lightbox-info-value">
                                        <a href="#" className="profile-lightbox-post-link" onClick={(e) => { e.preventDefault(); openInApp(lightboxPostUrl); }}>
                                            View original post ↗
                                        </a>
                                    </span>
                                </div>
                            )}
                            <div className="profile-lightbox-info-row">
                                <span className="profile-lightbox-info-label">Source</span>
                                <span className="profile-lightbox-info-value">{lightboxFile.source || 'Unknown'}</span>
                            </div>
                            <div className="profile-lightbox-info-row">
                                <span className="profile-lightbox-info-label">Date</span>
                                <span className="profile-lightbox-info-value">{lightboxFile.created_at ? new Date(lightboxFile.created_at * 1000).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' }) : lightboxFile.added_at ? new Date(lightboxFile.added_at * 1000).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' }) : '—'}</span>
                            </div>
                            <div className="profile-lightbox-info-row">
                                <span className="profile-lightbox-info-label">Type</span>
                                <span className="profile-lightbox-info-value">{lightboxFile.kind} — {lightboxFile.mime_type}</span>
                            </div>
                            <div className="profile-lightbox-info-row">
                                <span className="profile-lightbox-info-label">Size</span>
                                <span className="profile-lightbox-info-value">{lightboxFile.size < 1024 * 1024 ? `${(lightboxFile.size / 1024).toFixed(1)} KB` : `${(lightboxFile.size / (1024 * 1024)).toFixed(1)} MB`}</span>
                            </div>
                            {lightboxFile.tags.length > 0 && (
                                <div className="profile-lightbox-info-row">
                                    <span className="profile-lightbox-info-label">Tags</span>
                                    <span className="profile-lightbox-info-value">{lightboxFile.tags.join(', ')}</span>
                                </div>
                            )}
                        </div>
                    </div>
                </div>
            )}

            {/* ═══ Delete Confirmation Modal ═══ */}
            {confirmDeleteId && (
                <div className="profile-confirm-backdrop" onClick={() => setConfirmDeleteId(null)}>
                    <div className="profile-confirm-modal" onClick={e => e.stopPropagation()}>
                        <div className="profile-confirm-icon">🗑️</div>
                        <p className="profile-confirm-text">Are you sure you want to delete this file and the associated post?</p>
                        <div className="profile-confirm-actions">
                            <button className="profile-confirm-btn profile-confirm-btn--cancel" onClick={() => setConfirmDeleteId(null)}>Cancel</button>
                            <button className="profile-confirm-btn profile-confirm-btn--delete" onClick={() => { deleteMediaFile(confirmDeleteId); setConfirmDeleteId(null); }}>Yes, Delete</button>
                        </div>
                    </div>
                </div>
            )}

            {/* ═══ Vault Image Picker Modal ═══ */}
            {vaultPickerMode && (
                <div className="profile-vault-picker-backdrop" onClick={() => { setVaultPickerMode(null); Object.values(vaultPickerThumbs).forEach(u => URL.revokeObjectURL(u)); setVaultPickerThumbs({}); }}>
                    <div className="profile-vault-picker" onClick={e => e.stopPropagation()}>
                        <div className="profile-vault-picker-header">
                            <span>📁 {vaultPickerMode === 'avatar' ? 'Choose Profile Picture' : 'Choose Background Image'} — Vault</span>
                            <button className="profile-lightbox-close" onClick={() => { setVaultPickerMode(null); Object.values(vaultPickerThumbs).forEach(u => URL.revokeObjectURL(u)); setVaultPickerThumbs({}); }}>✕</button>
                        </div>
                        <div className="profile-vault-picker-body">
                            {vaultPickerLoading && <div className="profile-vault-picker-loading">Loading vault images...</div>}
                            {!vaultPickerLoading && vaultPickerFiles.length === 0 && (
                                <div className="profile-vault-picker-empty">
                                    <p>No images in your vault yet.</p>
                                    <button className="profile-upload-btn" onClick={() => { setVaultPickerMode(null); uploadImageFromDisk(vaultPickerMode!); }}>
                                        <BsUpload size={11} /> Upload from computer instead
                                    </button>
                                </div>
                            )}
                            <div className="profile-vault-picker-grid">
                                {vaultPickerFiles.map(file => (
                                    <button
                                        key={file.id}
                                        className="profile-vault-picker-item"
                                        onClick={() => pickVaultFile(file)}
                                        title={file.name}
                                    >
                                        {vaultPickerThumbs[file.id] ? (
                                            <img src={vaultPickerThumbs[file.id]} alt={file.name} />
                                        ) : (
                                            <div className="profile-vault-picker-placeholder">🖼️</div>
                                        )}
                                        <span className="profile-vault-picker-name">{file.name}</span>
                                    </button>
                                ))}
                            </div>
                        </div>
                    </div>
                </div>
            )}

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
