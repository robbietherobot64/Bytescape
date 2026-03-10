import { create } from 'zustand';
import { VaultMessage, SidebarItem } from '../types';
import { FeedTab } from '../hooks/useUnifiedFeed';

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
export type Screen = "login" | "feed" | "import" | "audit" | "archive" | "profile" | "settings" | "chat" | "friends" | "vault" | "peer-profile";

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
}

export const useNavStore = create<NavState>((set) => ({
    screen: 'login',
    setScreen: (screen) => set({ screen }),

    activeSidebar: 'feed',
    setActiveSidebar: (item) => set({ activeSidebar: item }),

    feedActiveTab: 'all',
    setFeedActiveTab: (tab) => set({ feedActiveTab: tab }),

    peerProfileNodeId: null,
    setPeerProfileNodeId: (id) => set({ peerProfileNodeId: id }),
}));

// ─── Chat Store ───
interface ChatState {
    messages: VaultMessage[];
    setMessages: (msgs: VaultMessage[] | ((prev: VaultMessage[]) => VaultMessage[])) => void;

    selectedConvoId: string | null;
    setSelectedConvoId: (id: string | null) => void;

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

    messageInput: '',
    setMessageInput: (val) => set({ messageInput: val }),

    isSending: false,
    setIsSending: (sending) => set({ isSending: sending }),
}));
