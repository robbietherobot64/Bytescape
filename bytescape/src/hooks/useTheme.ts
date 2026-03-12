import { useEffect } from 'react';
import { useThemeStore } from '../store';
import type { ThemeId } from '../types';

const STORAGE_KEY = 'bytescape-theme';

/**
 * useTheme — reads the active theme from the store,
 * applies it as `data-theme` on <html>, and persists to localStorage.
 *
 * Call once in App.tsx to initialise; call anywhere else to read/set.
 */
export function useTheme() {
    const activeTheme = useThemeStore((s) => s.activeTheme);
    const setTheme = useThemeStore((s) => s.setTheme);

    /* On mount: restore from localStorage (runs once per app lifecycle) */
    useEffect(() => {
        const saved = localStorage.getItem(STORAGE_KEY) as ThemeId | null;
        if (saved && saved !== activeTheme) {
            setTheme(saved);
        }
        // Only run on mount
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    /* Whenever activeTheme changes → apply to DOM + persist */
    useEffect(() => {
        document.documentElement.setAttribute('data-theme', activeTheme);
        localStorage.setItem(STORAGE_KEY, activeTheme);
    }, [activeTheme]);

    return { activeTheme, setTheme } as const;
}
