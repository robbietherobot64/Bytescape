import { useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useNavStore } from '../store';
import { openInApp } from '../utils/openInApp';

interface WebViewScreenProps {
    url: string;
    title?: string;
    /** Whether this tab is currently the active/visible tab */
    isActive: boolean;
}

let idCounter = 0;

/**
 * In-app browser tab backed by a native webview.
 *
 * Creates a real OS webview (WebKitGTK/WebView2/WKWebView) within the main
 * window, positioned over the React content area. This renders ANY URL with
 * no iframe restrictions — just like a real browser tab.
 *
 * Lifecycle:
 * - First activation → creates the native webview via Rust command
 * - Tab switch away → hides the webview (moves off-screen, preserves state)
 * - Tab switch back → restores the webview position/size
 * - Window resize → updates webview bounds via ResizeObserver
 * - Tab close (unmount) → destroys the native webview
 */
export function WebViewScreen({ url, title: _title, isActive }: WebViewScreenProps) {
    const containerRef = useRef<HTMLDivElement>(null);
    const labelRef = useRef(`browser-${++idCounter}`);
    const createdRef = useRef(false);
    const destroyedRef = useRef(false);
    const isActiveRef = useRef(isActive);
    isActiveRef.current = isActive;

    /** Measure the container and sync the native webview to match. */
    const syncBounds = useCallback(async () => {
        if (!containerRef.current || destroyedRef.current || !createdRef.current) return;
        const rect = containerRef.current.getBoundingClientRect();
        if (rect.width < 1 || rect.height < 1) return;
        try {
            await invoke('browser_set_bounds', {
                label: labelRef.current,
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            });
        } catch (err) {
            console.error('[WebView] set_bounds:', err);
        }
    }, []);

    // ─── Create / Show / Hide based on isActive ───
    useEffect(() => {
        if (destroyedRef.current) return;

        if (isActive) {
            if (!createdRef.current) {
                // First activation: create the native webview
                const create = async () => {
                    const el = containerRef.current;
                    if (!el) return;

                    // Wait one frame for layout to settle
                    await new Promise<void>(r => requestAnimationFrame(() => r()));
                    if (destroyedRef.current) return;

                    const rect = el.getBoundingClientRect();
                    if (rect.width < 1 || rect.height < 1) return;

                    try {
                        await invoke('browser_create', {
                            label: labelRef.current,
                            url,
                            x: rect.x,
                            y: rect.y,
                            width: rect.width,
                            height: rect.height,
                        });
                        createdRef.current = true;

                        // If the user switched away during creation, hide immediately
                        if (!isActiveRef.current) {
                            invoke('browser_hide', { label: labelRef.current }).catch(() => {});
                        }
                    } catch (err) {
                        console.error('[WebView] create error:', err);
                    }
                };
                create();
            } else {
                // Already created: restore position/size (show)
                requestAnimationFrame(() => syncBounds());
            }
        } else if (createdRef.current) {
            // Tab is inactive: hide the webview (move off-screen)
            invoke('browser_hide', { label: labelRef.current }).catch(() => {});
        }
    }, [isActive, url, syncBounds]);

    // ─── Track container size changes ───
    useEffect(() => {
        const el = containerRef.current;
        if (!el) return;

        const observer = new ResizeObserver(() => {
            if (isActiveRef.current && createdRef.current) {
                syncBounds();
            }
        });
        observer.observe(el);

        return () => observer.disconnect();
    }, [syncBounds]);

    // ─── Listen for title changes from the native webview ───
    useEffect(() => {
        const label = labelRef.current;
        let unlisten: (() => void) | null = null;

        listen<string>(`browser-title-${label}`, (event) => {
            const newTitle = event.payload;
            if (!newTitle) return;
            // Update the tab label in the nav store
            const nav = useNavStore.getState();
            const tab = nav.tabs.find(
                t => t.screen === 'webview' && t.meta?.url === url
            );
            if (tab && nav.updateTabLabel) {
                nav.updateTabLabel(tab.id, newTitle);
            }
        }).then(fn => { unlisten = fn; });

        return () => { unlisten?.(); };
    }, [url]);

    // ─── Listen for "open in new tab" from links clicked in the webview ───
    useEffect(() => {
        let unlisten: (() => void) | null = null;

        listen<string>('browser-new-tab', (event) => {
            openInApp(event.payload);
        }).then(fn => { unlisten = fn; });

        return () => { unlisten?.(); };
    }, []);

    // ─── Cleanup: destroy native webview on unmount ───
    useEffect(() => {
        const label = labelRef.current;
        return () => {
            destroyedRef.current = true;
            if (createdRef.current) {
                invoke('browser_close', { label }).catch(() => {});
            }
        };
    }, []);

    return (
        <div
            ref={containerRef}
            className="webview-container"
            style={{
                width: '100%',
                height: '100%',
                background: 'var(--bg-primary)',
            } as React.CSSProperties}
        />
    );
}
