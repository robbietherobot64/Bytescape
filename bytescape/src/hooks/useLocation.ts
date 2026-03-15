/**
 * useLocation — B10.1 Internal URL Scheme + B10.2 Navigation History Stack
 *
 * Provides URL-based navigation for all Bytescape screens.
 * Every screen is addressable by a `byte://` URL.
 *
 * URL Scheme:
 *   byte://feed                        → Feed (all)
 *   byte://feed/following              → Feed (following tab)
 *   byte://feed/you                    → Feed (your posts)
 *   byte://chat                        → Chat (no convo selected)
 *   byte://chat/bsky/{convoId}         → Chat with Bluesky conversation
 *   byte://chat/p2p/{nodeId}           → Chat with P2P peer
 *   byte://vault                       → Vault file manager
 *   byte://profile                     → Own profile
 *   byte://profile-builder             → Profile builder (GrapesJS / Raw HTML)
 *   byte://profile-browser             → Peer profile browser
 *   byte://peer/{nodeId}               → View specific peer's profile
 *   byte://import                      → Archive import
 *   byte://settings                    → Settings
 *   byte://friends                     → Social / Friends screen
 *   byte://login                       → Login screen
 */

import { create } from 'zustand';
import { useNavStore, useChatStore, Screen } from '../store';
import type { FeedTab } from './useUnifiedFeed';

// ─── URL ↔ Screen Resolution ───

/** Parsed result from a byte:// URL */
export interface ResolvedRoute {
    screen: Screen;
    sidebar: 'feed' | 'search' | 'profile' | 'chat' | 'social' | 'import' | 'vault' | 'settings';
    /** Feed tab when navigating to feed */
    feedTab?: FeedTab;
    /** Chat conversation ID */
    convoId?: string | null;
    /** Peer node ID for peer-profile or profile-browser */
    peerNodeId?: string | null;
    /** Peer display name (if known) */
    peerName?: string | null;
}

/**
 * Parse a byte:// URL into a ResolvedRoute.
 * Returns null if the URL is invalid.
 */
export function resolveUrl(url: string): ResolvedRoute | null {
    // Strip scheme
    let path = url;
    if (path.startsWith('byte://')) {
        path = path.slice(7);
    } else if (path.startsWith('/')) {
        path = path.slice(1);
    }

    // Remove trailing slash
    path = path.replace(/\/+$/, '');

    const segments = path.split('/');
    const base = segments[0];

    switch (base) {
        case 'feed': {
            const tab = (segments[1] as FeedTab) || 'all';
            const validTabs: FeedTab[] = ['all', 'following', 'subscribed', 'you'];
            return {
                screen: 'feed',
                sidebar: 'feed',
                feedTab: validTabs.includes(tab) ? tab : 'all',
            };
        }

        case 'chat': {
            const chatType = segments[1]; // 'bsky' or 'p2p'
            const chatId = segments.slice(2).join('/'); // convo ID or node ID
            let convoId: string | null = null;
            if (chatType === 'bsky' && chatId) {
                convoId = chatId;
            } else if (chatType === 'p2p' && chatId) {
                convoId = `p2p:${chatId}`;
            }
            return {
                screen: 'chat',
                sidebar: 'chat',
                convoId,
            };
        }

        case 'vault':
            return {
                screen: 'vault',
                sidebar: 'vault',
            };

        case 'profile':
            return {
                screen: 'profile',
                sidebar: 'profile',
            };

        case 'profile-builder':
            return {
                screen: 'profile-builder',
                sidebar: 'profile',
            };

        case 'profile-browser':
            return {
                screen: 'profile-browser',
                sidebar: 'social',
            };

        case 'peer': {
            const nodeId = segments[1] || null;
            if (nodeId) {
                // byte://peer/{nodeId} or byte://peer/{nodeId}/profile
                return {
                    screen: 'profile-browser',
                    sidebar: 'social',
                    peerNodeId: nodeId,
                };
            }
            return {
                screen: 'profile-browser',
                sidebar: 'social',
            };
        }

        case 'import':
            return {
                screen: 'import',
                sidebar: 'import',
            };

        case 'audit':
            return {
                screen: 'audit',
                sidebar: 'import',
            };

        case 'settings':
            return {
                screen: 'settings',
                sidebar: 'settings',
            };

        case 'friends':
        case 'social':
            return {
                screen: 'friends',
                sidebar: 'social',
            };

        case 'login':
            return {
                screen: 'login',
                sidebar: 'feed',
            };

        default:
            return null;
    }
}

/**
 * Generate a byte:// URL from the current navigation state.
 */
export function buildUrl(screen: Screen, params?: {
    feedTab?: FeedTab;
    convoId?: string | null;
    peerNodeId?: string | null;
}): string {
    switch (screen) {
        case 'feed': {
            const tab = params?.feedTab || 'all';
            return tab === 'all' ? 'byte://feed' : `byte://feed/${tab}`;
        }
        case 'chat': {
            if (params?.convoId) {
                if (params.convoId.startsWith('p2p:')) {
                    return `byte://chat/p2p/${params.convoId.slice(4)}`;
                }
                return `byte://chat/bsky/${params.convoId}`;
            }
            return 'byte://chat';
        }
        case 'vault':
            return 'byte://vault';
        case 'profile':
            return 'byte://profile';
        case 'profile-builder':
            return 'byte://profile-builder';
        case 'profile-browser':
            if (params?.peerNodeId) {
                return `byte://peer/${params.peerNodeId}`;
            }
            return 'byte://profile-browser';
        case 'peer-profile':
            if (params?.peerNodeId) {
                return `byte://peer/${params.peerNodeId}`;
            }
            return 'byte://profile-browser';
        case 'import':
            return 'byte://import';
        case 'audit':
            return 'byte://audit';
        case 'settings':
            return 'byte://settings';
        case 'friends':
            return 'byte://friends';
        case 'login':
            return 'byte://login';
        case 'archive':
            return 'byte://feed';
        case 'webview':
            // Webview tabs display their actual URL, not a byte:// URL.
            // The real URL is passed separately via tab meta.
            return 'byte://browser';
        default:
            return 'byte://feed';
    }
}

// ─── B10.2 Navigation History Stack ───

const MAX_HISTORY = 50;

interface LocationState {
    /** Current byte:// URL */
    currentUrl: string;

    /** Navigation history stack */
    history: string[];

    /** Cursor position in the history stack (-1 = at most recent) */
    cursor: number;

    /** Whether back navigation is possible */
    canGoBack: boolean;

    /** Whether forward navigation is possible */
    canGoForward: boolean;

    /**
     * Navigate to a byte:// URL.
     * Resolves the URL to screen state and updates NavStore + ChatStore.
     * Pushes onto the history stack.
     *
     * @param url — A byte:// URL or a path (e.g., '/feed/following')
     * @param options.replace — If true, replaces the current URL instead of pushing
     * @param options.skipHistory — If true, doesn't add to history (used by goBack/goForward)
     */
    navigate: (url: string, options?: { replace?: boolean; skipHistory?: boolean }) => void;

    /** Go back in history */
    goBack: () => void;

    /** Go forward in history */
    goForward: () => void;

    /**
     * Sync the current URL from the existing NavStore state.
     * Call this when the screen changes from outside the location system
     * (e.g., existing setScreen() calls) to keep the URL in sync.
     */
    syncFromNavStore: () => void;
}

export const useLocationStore = create<LocationState>((set, get) => ({
    currentUrl: 'byte://login',
    history: ['byte://login'],
    cursor: 0,
    canGoBack: false,
    canGoForward: false,

    navigate: (url, options) => {
        // Normalize URL to byte:// scheme
        if (!url.startsWith('byte://')) {
            // Strip leading slashes and prepend scheme
            const cleanPath = url.replace(/^\/+/, '');
            url = `byte://${cleanPath}`;
        }

        const route = resolveUrl(url);
        if (!route) {
            console.warn('[useLocation] Invalid URL:', url);
            return;
        }

        // Apply navigation to NavStore
        const navStore = useNavStore.getState();
        navStore.setScreen(route.screen);
        navStore.setActiveSidebar(route.sidebar);

        if (route.feedTab) {
            navStore.setFeedActiveTab(route.feedTab);
        }

        if (route.convoId !== undefined) {
            useChatStore.getState().setSelectedConvoId(route.convoId);
        }

        if (route.peerNodeId) {
            navStore.setPeerProfileNodeId(route.peerNodeId);
        }

        // Update history
        if (options?.skipHistory) {
            set({ currentUrl: url });
            return;
        }

        set((state) => {
            if (options?.replace) {
                // Replace current entry
                const newHistory = [...state.history];
                newHistory[state.cursor] = url;
                return {
                    currentUrl: url,
                    history: newHistory,
                };
            }

            // Push new entry (trim forward history)
            const trimmed = state.history.slice(0, state.cursor + 1);
            const newHistory = [...trimmed, url];

            // Enforce max depth
            if (newHistory.length > MAX_HISTORY) {
                newHistory.shift();
            }

            const newCursor = newHistory.length - 1;
            return {
                currentUrl: url,
                history: newHistory,
                cursor: newCursor,
                canGoBack: newCursor > 0,
                canGoForward: false,
            };
        });
    },

    goBack: () => {
        const state = get();
        if (!state.canGoBack || state.cursor <= 0) return;

        const newCursor = state.cursor - 1;
        const url = state.history[newCursor];

        set({
            cursor: newCursor,
            canGoBack: newCursor > 0,
            canGoForward: true,
        });

        // Navigate without pushing to history
        state.navigate(url, { skipHistory: true });
    },

    goForward: () => {
        const state = get();
        if (!state.canGoForward || state.cursor >= state.history.length - 1) return;

        const newCursor = state.cursor + 1;
        const url = state.history[newCursor];

        set({
            cursor: newCursor,
            canGoBack: true,
            canGoForward: newCursor < state.history.length - 1,
        });

        // Navigate without pushing to history
        state.navigate(url, { skipHistory: true });
    },

    syncFromNavStore: () => {
        const navState = useNavStore.getState();
        const chatState = useChatStore.getState();

        // For webview tabs, show the actual external URL instead of byte://
        let url: string;
        if (navState.screen === 'webview') {
            const activeTab = navState.tabs.find(t => t.id === navState.activeTabId);
            url = activeTab?.meta?.url || 'byte://browser';
        } else {
            url = buildUrl(navState.screen, {
                feedTab: navState.feedActiveTab,
                convoId: chatState.selectedConvoId,
                peerNodeId: navState.peerProfileNodeId,
            });
        }

        const state = get();
        if (url !== state.currentUrl) {
            // Push to history
            const trimmed = state.history.slice(0, state.cursor + 1);
            const newHistory = [...trimmed, url];
            if (newHistory.length > MAX_HISTORY) {
                newHistory.shift();
            }
            const newCursor = newHistory.length - 1;

            set({
                currentUrl: url,
                history: newHistory,
                cursor: newCursor,
                canGoBack: newCursor > 0,
                canGoForward: false,
            });
        }
    },
}));

/**
 * Hook providing the current URL and navigation functions.
 *
 * Usage:
 * ```tsx
 * const { currentUrl, navigate, goBack, goForward, canGoBack, canGoForward } = useLocation();
 * navigate('byte://feed/following');
 * ```
 */
export function useLocation() {
    const currentUrl = useLocationStore(s => s.currentUrl);
    const canGoBack = useLocationStore(s => s.canGoBack);
    const canGoForward = useLocationStore(s => s.canGoForward);
    const navigate = useLocationStore(s => s.navigate);
    const goBack = useLocationStore(s => s.goBack);
    const goForward = useLocationStore(s => s.goForward);
    const syncFromNavStore = useLocationStore(s => s.syncFromNavStore);

    return {
        currentUrl,
        canGoBack,
        canGoForward,
        navigate,
        goBack,
        goForward,
        syncFromNavStore,
    };
}
