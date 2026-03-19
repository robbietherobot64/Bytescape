import { useEffect } from 'react';
import { useThemeStore } from '../store';
import type { ThemeId } from '../types';
import type { ColorOverrides, UserBackground, BackgroundMode } from '../store';

const STORAGE_KEY = 'bytescape-theme';
const OVERRIDES_KEY = 'bytescape-theme-overrides';
const BG_KEY = 'bytescape-theme-bg';
const BG_MODE_KEY = 'bytescape-theme-bg-mode';
const USER_BGS_KEY = 'bytescape-theme-user-bgs';

/** Module-level guard: localStorage restore runs exactly once across all useTheme() consumers. */
let _restored = false;

/**
 * useTheme — reads the active theme from the store,
 * applies it as `data-theme` on <html>, applies custom colour overrides
 * as inline CSS variables, and persists everything to localStorage.
 *
 * Call once in App.tsx to initialise; call anywhere else to read/set.
 */
export function useTheme() {
    const activeTheme = useThemeStore((s) => s.activeTheme);
    const setTheme = useThemeStore((s) => s.setTheme);
    const customOverrides = useThemeStore((s) => s.customOverrides);
    const customBackground = useThemeStore((s) => s.customBackground);
    const setColorOverride = useThemeStore((s) => s.setColorOverride);
    const removeColorOverride = useThemeStore((s) => s.removeColorOverride);
    const resetAllOverrides = useThemeStore((s) => s.resetAllOverrides);
    const setCustomBackground = useThemeStore((s) => s.setCustomBackground);
    const backgroundMode = useThemeStore((s) => s.backgroundMode);
    const userBackgrounds = useThemeStore((s) => s.userBackgrounds);
    const setBackgroundMode = useThemeStore((s) => s.setBackgroundMode);
    const addUserBackground = useThemeStore((s) => s.addUserBackground);
    const removeUserBackground = useThemeStore((s) => s.removeUserBackground);

    /* On mount: restore from localStorage (runs exactly once across all consumers) */
    useEffect(() => {
        if (_restored) return;
        _restored = true;

        const saved = localStorage.getItem(STORAGE_KEY) as ThemeId | null;
        if (saved && saved !== activeTheme) {
            setTheme(saved);
        }

        // Restore custom overrides
        try {
            const savedOverrides = localStorage.getItem(OVERRIDES_KEY);
            if (savedOverrides) {
                const parsed: ColorOverrides = JSON.parse(savedOverrides);
                for (const [variable, value] of Object.entries(parsed)) {
                    setColorOverride(variable, value);
                }
            }
        } catch { /* ignore corrupt data */ }

        // Restore custom background
        const savedBg = localStorage.getItem(BG_KEY);
        if (savedBg) {
            setCustomBackground(savedBg);
        }

        // Restore background mode
        const savedMode = localStorage.getItem(BG_MODE_KEY) as BackgroundMode | null;
        if (savedMode === 'cover' || savedMode === 'tile') {
            setBackgroundMode(savedMode);
        }

        // Restore user-uploaded backgrounds
        try {
            const savedUserBgs = localStorage.getItem(USER_BGS_KEY);
            if (savedUserBgs) {
                const parsed: UserBackground[] = JSON.parse(savedUserBgs);
                for (const bg of parsed) {
                    addUserBackground(bg);
                }
            }
        } catch { /* ignore corrupt data */ }

        // Only run on mount
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    /* Whenever activeTheme changes → apply to DOM + persist */
    useEffect(() => {
        document.documentElement.setAttribute('data-theme', activeTheme);
        try { localStorage.setItem(STORAGE_KEY, activeTheme); } catch { /* quota */ }
    }, [activeTheme]);

    /* Apply custom colour overrides as inline CSS variables on :root */
    useEffect(() => {
        const root = document.documentElement;
        // Apply each override
        for (const [variable, value] of Object.entries(customOverrides)) {
            root.style.setProperty(variable, value);
        }
        // Persist
        try { localStorage.setItem(OVERRIDES_KEY, JSON.stringify(customOverrides)); } catch { /* quota */ }

        // Cleanup: remove any overrides that were removed from the store
        return () => {
            for (const variable of Object.keys(customOverrides)) {
                root.style.removeProperty(variable);
            }
        };
    }, [customOverrides]);

    /* Apply custom background */
    useEffect(() => {
        const root = document.documentElement;
        if (customBackground) {
            // Determine if it's a URL or a CSS gradient
            if (customBackground.startsWith('http') || customBackground.startsWith('data:') || customBackground.startsWith('blob:')) {
                root.style.setProperty('--custom-bg-image', `url(${customBackground})`);
            } else {
                // CSS gradient or pattern
                root.style.setProperty('--custom-bg-image', customBackground);
            }
            root.classList.add('has-custom-bg');
            localStorage.setItem(BG_KEY, customBackground);
        } else {
            root.style.removeProperty('--custom-bg-image');
            root.classList.remove('has-custom-bg');
            localStorage.removeItem(BG_KEY);
        }
    }, [customBackground]);

    /* Apply background display mode (cover vs tile) */
    useEffect(() => {
        if (backgroundMode === 'tile') {
            document.documentElement.classList.add('bg-tiled');
        } else {
            document.documentElement.classList.remove('bg-tiled');
        }
        try { localStorage.setItem(BG_MODE_KEY, backgroundMode); } catch { /* quota */ }
    }, [backgroundMode]);

    /* Persist user-uploaded backgrounds */
    useEffect(() => {
        try { localStorage.setItem(USER_BGS_KEY, JSON.stringify(userBackgrounds)); } catch { /* quota */ }
    }, [userBackgrounds]);

    return {
        activeTheme,
        setTheme,
        customOverrides,
        customBackground,
        backgroundMode,
        userBackgrounds,
        setColorOverride,
        removeColorOverride,
        resetAllOverrides,
        setCustomBackground,
        setBackgroundMode,
        addUserBackground,
        removeUserBackground,
    } as const;
}
