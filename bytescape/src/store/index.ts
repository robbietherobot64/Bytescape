import { create } from 'zustand';
import { VaultMessage, SidebarItem, ThemeId } from '../types';
import { FeedTab } from '../hooks/useUnifiedFeed';

// ─── Browser (WebView) Store ───
// Shared state between WebViewScreen and Omnibar so the global Omnibar
// can display the current URL and control back/forward/reload in the
// active native webview.

interface BrowserState {
    /** The webview label currently active (null when no webview tab is active) */
    activeLabel: string | null;
    /** Current URL being displayed by the active webview */
    currentUrl: string | null;
    /** Whether the active webview is loading a page */
    isLoading: boolean;
    setActiveLabel: (label: string | null) => void;
    setCurrentUrl: (url: string | null) => void;
    setIsLoading: (loading: boolean) => void;
}

export const useBrowserStore = create<BrowserState>((set) => ({
    activeLabel: null,
    currentUrl: null,
    isLoading: false,
    setActiveLabel: (label) => set({ activeLabel: label }),
    setCurrentUrl: (url) => set({ currentUrl: url }),
    setIsLoading: (loading) => set({ isLoading: loading }),
}));

// ─── Auth Store ───
interface AuthInfo {
    handle: string;
    did: string;
    /** "bluesky" | "local" */
    source: string;
    /** Bytescape-local username */
    local_username?: string | null;
    /** Whether the identity setup is complete */
    identity_ready: boolean;
}

interface AuthState {
    auth: AuthInfo | null;
    setAuth: (auth: AuthInfo | null) => void;
}

export const useAuthStore = create<AuthState>((set) => ({
    auth: null,
    setAuth: (auth) => set({ auth }),
}));

// ─── App Store (import/error/global) ───
interface AppState {
    importProgress: string | null;
    setImportProgress: (progress: string | null) => void;

    globalError: string | null;
    setGlobalError: (error: string | null) => void;

    archiveCount: number;
    setArchiveCount: (count: number) => void;
}

export const useAppStore = create<AppState>((set) => ({
    importProgress: null,
    setImportProgress: (progress) => set({ importProgress: progress }),

    globalError: null,
    setGlobalError: (error) => set({ globalError: error }),

    archiveCount: 0,
    setArchiveCount: (count) => set({ archiveCount: count }),
}));

// ─── Navigation Store ───
export type Screen = "login" | "feed" | "import" | "audit" | "archive" | "profile" | "profile-builder" | "profile-browser" | "settings" | "chat" | "friends" | "vault" | "peer-profile" | "webview" | "profile-view" | "forum" | "group";

/** Browser-style tab descriptor */
export interface TabItem {
    id: string;
    screen: Screen;
    label: string;
    /** Extra context for restoring screen state (e.g. peer node ID) */
    meta?: Record<string, string>;
}

let tabCounter = 0;
function nextTabId(): string {
    return `tab-${++tabCounter}`;
}

const TAB_LABELS: Partial<Record<Screen, string>> = {
    feed: 'Feed',
    chat: 'Chat',
    friends: 'Social',
    profile: 'Profile',
    settings: 'Settings',
    vault: 'Vault',
    import: 'Import',
    audit: 'Audit',
    'profile-builder': 'Profile Builder',
    'profile-browser': 'Profile Browser',
    'peer-profile': 'Peer Profile',
    webview: 'Browser',
    forum: 'Forums',
    group: 'Group',
};

interface NavState {
    screen: Screen;
    setScreen: (screen: Screen) => void;

    activeSidebar: SidebarItem;
    setActiveSidebar: (item: SidebarItem) => void;

    feedActiveTab: FeedTab;
    setFeedActiveTab: (tab: FeedTab) => void;

    /** Node ID of the peer whose profile is being viewed */
    peerProfileNodeId: string | null;
    setPeerProfileNodeId: (id: string | null) => void;

    /** Cached display name for the peer profile being viewed */
    peerProfileName: string | null;
    setPeerProfileName: (name: string | null) => void;

    /** Browser-style tabs */
    tabs: TabItem[];
    activeTabId: string | null;
    openTab: (screen: Screen, label?: string, meta?: Record<string, string>) => void;
    /** Navigate the current active tab to a different screen (in-place, no new tab) */
    navigateInTab: (screen: Screen, label?: string) => void;
    closeTab: (tabId: string) => void;
    switchTab: (tabId: string) => void;
    /** Update the label of an existing tab (e.g. when a browser page title changes) */
    updateTabLabel: (tabId: string, label: string) => void;
}

export const useNavStore = create<NavState>((set, get) => ({
    screen: 'login',
    setScreen: (screen) => set({ screen }),

    activeSidebar: 'feed',
    setActiveSidebar: (item) => set({ activeSidebar: item }),

    feedActiveTab: 'all',
    setFeedActiveTab: (tab) => set({ feedActiveTab: tab }),

    peerProfileNodeId: null,
    setPeerProfileNodeId: (id) => set({ peerProfileNodeId: id }),

    peerProfileName: null,
    setPeerProfileName: (name) => set({ peerProfileName: name }),

    tabs: [],
    activeTabId: null,

    openTab: (screen, label, meta) => {
        const state = get();

        // Content tabs with meta (e.g. peer profiles) — always create a NEW tab next to active
        if (meta) {
            const id = nextTabId();
            const newTab: TabItem = {
                id,
                screen,
                label: label || TAB_LABELS[screen] || screen,
                meta,
            };
            // Insert next to the active tab
            const activeIdx = state.tabs.findIndex(t => t.id === state.activeTabId);
            const insertIdx = activeIdx >= 0 ? activeIdx + 1 : state.tabs.length;
            const newTabs = [...state.tabs];
            newTabs.splice(insertIdx, 0, newTab);
            set({ tabs: newTabs, activeTabId: id, screen });
            return;
        }

        // Module tabs — reuse existing tab for same screen, or create NEW tab
        const existing = state.tabs.find(t => t.screen === screen && !t.meta);
        if (existing) {
            set({ activeTabId: existing.id, screen });
            return;
        }

        // No existing tab for this module — create one next to active
        const id = nextTabId();
        const newTab: TabItem = {
            id,
            screen,
            label: label || TAB_LABELS[screen] || screen,
        };
        const activeIdx = state.tabs.findIndex(t => t.id === state.activeTabId);
        const insertIdx = activeIdx >= 0 ? activeIdx + 1 : state.tabs.length;
        const newTabs = [...state.tabs];
        newTabs.splice(insertIdx, 0, newTab);
        set({ tabs: newTabs, activeTabId: id, screen });
    },

    navigateInTab: (screen, label) => {
        const state = get();
        const activeTab = state.tabs.find(t => t.id === state.activeTabId);
        if (!activeTab) {
            // No active tab — fall back to creating one
            get().openTab(screen, label);
            return;
        }
        const newTabs = state.tabs.map(t =>
            t.id === state.activeTabId
                ? { ...t, screen, label: label || TAB_LABELS[screen] || screen, meta: undefined }
                : t
        );
        set({ tabs: newTabs, screen });
    },

    closeTab: (tabId) => {
        const state = get();
        const idx = state.tabs.findIndex(t => t.id === tabId);
        if (idx === -1) return;
        const remaining = state.tabs.filter(t => t.id !== tabId);
        if (remaining.length === 0) {
            // Last tab closed — show empty state
            set({ tabs: [], activeTabId: null });
            return;
        }
        if (state.activeTabId === tabId) {
            // Switch to neighbor tab
            const newIdx = Math.min(idx, remaining.length - 1);
            const neighbor = remaining[newIdx];
            set({ tabs: remaining, activeTabId: neighbor.id, screen: neighbor.screen });
        } else {
            set({ tabs: remaining });
        }
    },

    switchTab: (tabId) => {
        const state = get();
        const tab = state.tabs.find(t => t.id === tabId);
        if (!tab) return;
        set({ activeTabId: tabId, screen: tab.screen });
    },

    updateTabLabel: (tabId, label) => {
        set((state) => ({
            tabs: state.tabs.map(t =>
                t.id === tabId ? { ...t, label } : t
            ),
        }));
    },
}));

// ─── Chat Store ───

// ─── Peer Store (global P2P status tracking) ───
interface PeerState {
    /** Map of node_id → online status */
    peerStatuses: Record<string, 'online' | 'offline'>;
    setPeerStatus: (nodeId: string, status: 'online' | 'offline') => void;
    setPeerStatuses: (statuses: Record<string, 'online' | 'offline'>) => void;
}

export const usePeerStore = create<PeerState>((set) => ({
    peerStatuses: {},
    setPeerStatus: (nodeId, status) => set((state) => ({
        peerStatuses: { ...state.peerStatuses, [nodeId]: status },
    })),
    setPeerStatuses: (statuses) => set({ peerStatuses: statuses }),
}));

// ─── Chat Store ───
interface PendingConvoMeta {
    did: string;
    handle: string;
    protocol?: string;
}

interface ChatState {
    messages: VaultMessage[];
    setMessages: (msgs: VaultMessage[] | ((prev: VaultMessage[]) => VaultMessage[])) => void;

    selectedConvoId: string | null;
    setSelectedConvoId: (id: string | null) => void;

    /** Metadata for a newly created convo that has no messages yet */
    pendingConvoMeta: PendingConvoMeta | null;
    setPendingConvoMeta: (meta: PendingConvoMeta | null) => void;

    messageInput: string;
    setMessageInput: (val: string) => void;

    isSending: boolean;
    setIsSending: (sending: boolean) => void;
}

export const useChatStore = create<ChatState>((set) => ({
    messages: [],
    setMessages: (msgs) => set((state) =>
        ({ messages: typeof msgs === 'function' ? msgs(state.messages) : msgs })
    ),

    selectedConvoId: null,
    setSelectedConvoId: (id) => set({ selectedConvoId: id }),

    pendingConvoMeta: null,
    setPendingConvoMeta: (meta) => set({ pendingConvoMeta: meta }),

    messageInput: '',
    setMessageInput: (val) => set({ messageInput: val }),

    isSending: false,
    setIsSending: (sending) => set({ isSending: sending }),
}));

// ─── Feed Cache Store ───
// Persists feed data across tab switches and screen navigation.
// Enables cache-first rendering (stale-while-revalidate pattern).

import { FeedEntry, GeneratorView, ProfileView } from '../types';

interface FeedCacheEntry {
    entries: FeedEntry[];
    cursor: string | null;
    fetchedAt: number; // epoch ms
}

interface FeedCacheState {
    /** Cached feed data keyed by tab name */
    cache: Record<string, FeedCacheEntry>;
    setCache: (tab: string, entries: FeedEntry[], cursor: string | null) => void;
    getCache: (tab: string, maxAgeMs?: number) => FeedCacheEntry | null;
    appendCache: (tab: string, entries: FeedEntry[], cursor: string | null) => void;
    clearCache: (tab: string) => void;
    /** Remove all cache entries whose key starts with the given prefix */
    clearCachePrefix: (prefix: string) => void;
    /** Nuke all cached data — call on account disconnect */
    clearAllCaches: () => void;

    /** Cached context data */
    cachedSavedFeeds: GeneratorView[];
    setCachedSavedFeeds: (feeds: GeneratorView[]) => void;
    cachedFollowingList: ProfileView[];
    setCachedFollowingList: (list: ProfileView[]) => void;
}

export const useFeedCacheStore = create<FeedCacheState>((set, get) => ({
    cache: {},
    setCache: (tab, entries, cursor) => set((state) => {
        // Prune expired entries (>2 minutes) to prevent unbounded growth
        const now = Date.now();
        const pruned: Record<string, FeedCacheEntry> = {};
        for (const [key, val] of Object.entries(state.cache)) {
            if (now - val.fetchedAt < 120_000) pruned[key] = val;
        }
        pruned[tab] = { entries, cursor, fetchedAt: now };
        return { cache: pruned };
    }),
    getCache: (tab, maxAgeMs = 120_000) => {
        const entry = get().cache[tab];
        if (!entry) return null;
        if (Date.now() - entry.fetchedAt > maxAgeMs) {
            // Remove expired entry to free memory instead of just returning null
            set((state) => {
                const { [tab]: _, ...rest } = state.cache;
                return { cache: rest };
            });
            return null;
        }
        return entry;
    },
    appendCache: (tab, newEntries, cursor) => set((state) => {
        const existing = state.cache[tab];
        const merged = existing ? [...existing.entries, ...newEntries] : newEntries;
        return {
            cache: {
                ...state.cache,
                [tab]: { entries: merged, cursor, fetchedAt: existing?.fetchedAt ?? Date.now() },
            },
        };
    }),

    clearCache: (tab) => set((state) => {
        const { [tab]: _, ...rest } = state.cache;
        return { cache: rest };
    }),
    clearCachePrefix: (prefix) => set((state) => {
        const filtered: Record<string, FeedCacheEntry> = {};
        for (const [key, val] of Object.entries(state.cache)) {
            if (!key.startsWith(prefix)) filtered[key] = val;
        }
        return { cache: filtered };
    }),
    clearAllCaches: () => set({
        cache: {},
        cachedSavedFeeds: [],
        cachedFollowingList: [],
    }),
    cachedSavedFeeds: [],
    setCachedSavedFeeds: (feeds) => set({ cachedSavedFeeds: feeds }),
    cachedFollowingList: [],
    setCachedFollowingList: (list) => set({ cachedFollowingList: list }),
}));

// ─── Theme Store ───

/** Per-variable color overrides keyed by CSS variable name (e.g. "--bg-sidebar") */
export type ColorOverrides = Record<string, string>;

/** User-uploaded background image persisted as base64 data URL */
export interface UserBackground {
    id: string;
    name: string;
    dataUrl: string;
}

/** Background display mode — cover (full bleed) or tile (repeat pattern) */
export type BackgroundMode = 'cover' | 'tile';

interface ThemeState {
    activeTheme: ThemeId;
    /** Per-CSS-variable colour overrides applied on top of the active theme */
    customOverrides: ColorOverrides;
    /** Background image/gif URL or CSS gradient for the app root */
    customBackground: string | null;
    /** How the background image is displayed */
    backgroundMode: BackgroundMode;
    /** User-uploaded background images (persisted independently of active background) */
    userBackgrounds: UserBackground[];
    setTheme: (theme: ThemeId) => void;
    setColorOverride: (variable: string, value: string) => void;
    removeColorOverride: (variable: string) => void;
    resetAllOverrides: () => void;
    setCustomBackground: (bg: string | null) => void;
    setBackgroundMode: (mode: BackgroundMode) => void;
    addUserBackground: (bg: UserBackground) => void;
    removeUserBackground: (id: string) => void;
}

export const useThemeStore = create<ThemeState>((set) => ({
    activeTheme: 'geocities',
    customOverrides: {},
    customBackground: null,
    backgroundMode: 'cover',
    userBackgrounds: [],
    setTheme: (theme) => set({ activeTheme: theme }),
    setColorOverride: (variable, value) =>
        set((s) => ({ customOverrides: { ...s.customOverrides, [variable]: value } })),
    removeColorOverride: (variable) =>
        set((s) => {
            const next = { ...s.customOverrides };
            delete next[variable];
            return { customOverrides: next };
        }),
    resetAllOverrides: () => set({ customOverrides: {}, customBackground: null, backgroundMode: 'cover' }),
    setCustomBackground: (bg) => set({ customBackground: bg }),
    setBackgroundMode: (mode) => set({ backgroundMode: mode }),
    addUserBackground: (bg) =>
        set((s) => {
            if (s.userBackgrounds.some((b) => b.id === bg.id)) return s;
            return { userBackgrounds: [...s.userBackgrounds, bg] };
        }),
    removeUserBackground: (id) =>
        set((s) => ({
            userBackgrounds: s.userBackgrounds.filter((b) => b.id !== id),
            customBackground: s.userBackgrounds.find((b) => b.id === id)?.dataUrl === s.customBackground
                ? null
                : s.customBackground,
        })),
}));
