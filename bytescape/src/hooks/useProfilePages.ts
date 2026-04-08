/**
 * PURPOSE
 * Custom hook for profile page CRUD operations.
 * Manages multi-page profiles (blog, about-me pages, etc.)
 *
 * RELATED FILES
 * - src/types/index.ts (ProfilePage interface)
 * - src-tauri/src/commands/profile.rs (save_profile_page, get_profile_pages, delete_profile_page)
 * - src/screens/ProfileScreen.tsx (consumer)
 */

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ProfilePage } from '../types';

export function useProfilePages() {
    const [pages, setPages] = useState<ProfilePage[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    // Load all pages
    const loadPages = useCallback(async () => {
        setLoading(true);
        try {
            const raw = await invoke<string[]>('get_profile_pages');
            const parsed: ProfilePage[] = raw
                .map(r => { try { return JSON.parse(r) as ProfilePage; } catch { return null; } })
                .filter((p): p is ProfilePage => p !== null)
                .sort((a, b) => a.order - b.order);
            setPages(parsed);
            setError(null);
        } catch (e) {
            console.error('[useProfilePages] Load failed:', e);
            setError(String(e));
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => { loadPages(); }, [loadPages]);

    // Save / update a page
    const savePage = useCallback(async (page: ProfilePage) => {
        try {
            await invoke('save_profile_page', {
                pageId: page.id,
                pageData: JSON.stringify(page),
            });
            setPages(prev => {
                const idx = prev.findIndex(p => p.id === page.id);
                if (idx >= 0) {
                    const updated = [...prev];
                    updated[idx] = page;
                    return updated.sort((a, b) => a.order - b.order);
                }
                return [...prev, page].sort((a, b) => a.order - b.order);
            });
        } catch (e) {
            console.error('[useProfilePages] Save failed:', e);
            throw e;
        }
    }, []);

    // Delete a page
    const deletePage = useCallback(async (pageId: string) => {
        try {
            await invoke('delete_profile_page', { pageId });
            setPages(prev => prev.filter(p => p.id !== pageId));
        } catch (e) {
            console.error('[useProfilePages] Delete failed:', e);
            throw e;
        }
    }, []);

    // Create a new page with defaults
    const createPage = useCallback(async (title: string, contentType: ProfilePage['content_type'] = 'rich-text') => {
        const slug = title.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
        const page: ProfilePage = {
            id: `page-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
            slug,
            title,
            content_type: contentType,
            content: '',
            order: pages.length,
            visible: true,
        };
        await savePage(page);
        return page;
    }, [pages.length, savePage]);

    return { pages, loading, error, savePage, deletePage, createPage, reload: loadPages };
}
