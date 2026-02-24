import { create } from 'zustand';

interface AuthInfo {
    handle: string;
    did: string;
}

interface AuthState {
    auth: AuthInfo | null;
    setAuth: (auth: AuthInfo | null) => void;
}

export const useAuthStore = create<AuthState>((set) => ({
    auth: null,
    setAuth: (auth) => set({ auth }),
}));

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
