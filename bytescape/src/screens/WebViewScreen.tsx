import { useEffect, useRef, useCallback, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useNavStore, useBrowserStore } from '../store';
import { openInApp } from '../utils/openInApp';

/**
 * PURPOSE
 * Hosts the in-app browser tab shell that coordinates a native OS webview from React.
 *
 * HOW IT WORKS
 * 1) Computes container bounds from layout and asks Rust to create/show/hide/close.
 * 2) Syncs active URL/loading state through `useBrowserStore`.
 * 3) Subscribes to browser events (URL/title/new-tab) and forwards updates to stores/navigation.
 * 4) Uses lifecycle guards to avoid duplicate side effects in React StrictMode.
 *
 * RELATED FILES
 * - src/store/* (browser + nav stores)
 * - src/utils/openInApp.ts (new-tab routing into app browser tabs)
 * - src-tauri/src/lib.rs (browser_* Tauri command handlers + event emitters)
 */

interface WebViewScreenProps {
    url: string;
    title?: string;
    isActive: boolean;
    tabId: string;
}

/**
 * Get the bounds where the native webview should render.
 * Measures `.main-content` — the stable ancestor that always has correct size.
 * Returns null if the element is detached or has zero dimensions.
 */
function getWebviewBounds(el: HTMLElement | null): { x: number; y: number; width: number; height: number } | null {
    if (!el || !el.isConnected) return null;
    const mc = el.closest('.main-content') as HTMLElement | null;
    const rect = (mc || el).getBoundingClientRect();
    if (rect.width < 1 || rect.height < 1) return null;
    return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
}

/**
 * In-app browser tab backed by a native OS webview.
 *
 * The native webview is an OS-level surface that renders ABOVE all React DOM.
 * This component's only jobs:
 *   1. Tell Rust to create the webview at the right coordinates
 *   2. Tell Rust to show/hide when this tab activates/deactivates
 *   3. Tell Rust to destroy it when the tab closes (component unmounts)
 *
 * ## StrictMode safety
 *
 * Every async effect uses a local `cancelled` flag (not a ref) so that
 * stale callbacks from StrictMode's mount→unmount→remount cycle are no-ops.
 * The webview label is derived from `tabId` so it's stable across remounts.
 */
export function WebViewScreen({ url, title: _title, isActive, tabId }: WebViewScreenProps) {
    /* ════════════════════ Instance identity + shared state ════════════════════ */
    const containerRef = useRef<HTMLDivElement>(null);
    // Stable label derived from tabId — survives StrictMode remount
    const label = `wv-${tabId}`;
    const createdRef = useRef(false);
    const isActiveRef = useRef(isActive);
    isActiveRef.current = isActive;

    const [error, setError] = useState<string | null>(null);
    const { setActiveLabel, setCurrentUrl } = useBrowserStore();

    /* ════════════════════ Create on mount / destroy on unmount ════════════════════ */
    // Uses a local `cancelled` flag so stale StrictMode callbacks are no-ops.
    useEffect(() => {
        let cancelled = false;
        createdRef.current = false;
        setError(null);

        async function create() {
            // Wait one frame for flex layout to resolve
            await new Promise<void>(r => requestAnimationFrame(() => r()));
            if (cancelled) return;

            // Always re-read containerRef at each async step (never use a stale reference)
            let bounds = getWebviewBounds(containerRef.current);
            if (!bounds) {
                // Layout might not be ready yet — wait and retry once
                await new Promise<void>(r => setTimeout(r, 250));
                if (cancelled) return;
                bounds = getWebviewBounds(containerRef.current);
            }
            if (!bounds) {
                // Still nothing — bail silently. When the tab becomes active
                // and the show/hide effect fires, it will try again.
                console.warn('[WebView] no valid bounds, deferring creation');
                return;
            }

            console.log('[WebView] creating', label, 'at',
                Math.round(bounds.x), Math.round(bounds.y),
                Math.round(bounds.width) + 'x' + Math.round(bounds.height));

            try {
                await invoke('browser_create', { label, url, ...bounds });
            } catch (err) {
                if (cancelled) return;
                console.error('[WebView] create failed:', err);
                setError(String(err));
                return;
            }

            if (cancelled) return;
            createdRef.current = true;
            setActiveLabel(label);
            setCurrentUrl(url);

            // Delayed re-sync in case layout shifted during creation
            setTimeout(() => {
                if (cancelled || !createdRef.current) return;
                const fresh = getWebviewBounds(containerRef.current);
                if (fresh) {
                    invoke('browser_show_at', { label, ...fresh }).catch(() => {});
                }
            }, 300);

            // If user switched away during creation, hide
            if (!isActiveRef.current) {
                invoke('browser_hide', { label }).catch(() => {});
            }
        }

        if (isActiveRef.current) {
            create();
        }

        return () => {
            cancelled = true;
            if (createdRef.current) {
                invoke('browser_close', { label }).catch(() => {});
                createdRef.current = false;
            }
        };
    // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    /* ════════════════════ Activation show/hide lifecycle ════════════════════ */
    useEffect(() => {
        if (!createdRef.current) {
            // Webview not created yet. If we just became active, try creating now.
            // This handles the case where creation was deferred (tab opened in background).
            if (isActive) {
                let cancelled = false;
                (async () => {
                    await new Promise<void>(r => requestAnimationFrame(() => r()));
                    if (cancelled || createdRef.current) return;
                    const bounds = getWebviewBounds(containerRef.current);
                    if (!bounds) return;

                    try {
                        await invoke('browser_create', { label, url, ...bounds });
                    } catch { return; }

                    if (cancelled) return;
                    createdRef.current = true;
                    setActiveLabel(label);
                    setCurrentUrl(url);
                })();
                return () => { cancelled = true; };
            }
            return;
        }

        if (isActive) {
            // Show: use double-rAF so the tab pane has reflowed from display:none
            let cancelled = false;
            requestAnimationFrame(() => {
                requestAnimationFrame(() => {
                    if (cancelled) return;
                    const bounds = getWebviewBounds(containerRef.current);
                    if (!bounds) return;
                    invoke('browser_show_at', { label, ...bounds }).catch(() => {});
                });
            });
            return () => { cancelled = true; };
        } else {
            // Hide immediately
            invoke('browser_hide', { label }).catch(() => {});
        }
    // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [isActive]);

    /* ════════════════════ Resize tracking -> native bounds sync ════════════════════ */
    useEffect(() => {
        const el = containerRef.current;
        if (!el) return;
        const target = (el.closest('.main-content') as HTMLElement) || el;
        const observer = new ResizeObserver(() => {
            if (!isActiveRef.current || !createdRef.current) return;
            const bounds = getWebviewBounds(containerRef.current);
            if (bounds) {
                invoke('browser_set_bounds', { label, ...bounds }).catch(() => {});
            }
        });
        observer.observe(target);
        return () => observer.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    /* ════════════════════ Browser store synchronization ════════════════════ */
    useEffect(() => {
        if (isActive && createdRef.current) {
            setActiveLabel(label);
            setCurrentUrl(url);
        } else if (!isActive) {
            const s = useBrowserStore.getState();
            if (s.activeLabel === label) { setActiveLabel(null); setCurrentUrl(null); }
        }
    }, [isActive, url, label, setActiveLabel, setCurrentUrl]);

    /* ════════════════════ Event bridge subscriptions ════════════════════ */
    useEffect(() => {
        let dead = false;
        const unsubs: (() => void)[] = [];

        listen<string>(`browser-url-${label}`, (e) => {
            if (dead || !e.payload) return;
            const s = useBrowserStore.getState();
            if (s.activeLabel === label) { s.setCurrentUrl(e.payload); s.setIsLoading(false); }
        }).then(fn => { if (dead) fn(); else unsubs.push(fn); });

        listen<string>(`browser-title-${label}`, (e) => {
            if (dead || !e.payload) return;
            const nav = useNavStore.getState();
            nav.updateTabLabel?.(tabId, e.payload);
        }).then(fn => { if (dead) fn(); else unsubs.push(fn); });

        listen<string>('browser-new-tab', (e) => {
            if (!dead && e.payload) openInApp(e.payload);
        }).then(fn => { if (dead) fn(); else unsubs.push(fn); });

        return () => { dead = true; unsubs.forEach(fn => fn()); };
    }, [label, tabId]);

    /* ════════════════════ Retry/error recovery ════════════════════ */
    const handleRetry = useCallback(() => {
        setError(null);
        if (createdRef.current) {
            invoke('browser_close', { label }).catch(() => {});
            createdRef.current = false;
        }
        const bounds = getWebviewBounds(containerRef.current);
        if (!bounds) return;
        invoke('browser_create', { label, url, ...bounds })
            .then(() => { createdRef.current = true; })
            .catch(err => setError(String(err)));
    }, [label, url]);

    /* ════════════════════ Render shell + error overlay ════════════════════ */
    return (
        <div ref={containerRef} className="webview-container" style={{ position: 'relative' } as React.CSSProperties}>
            {error && (
                <div style={{
                    position: 'absolute', inset: 0,
                    display: 'flex', flexDirection: 'column',
                    alignItems: 'center', justifyContent: 'center',
                    gap: '14px', color: 'var(--text-secondary, #aaa)',
                    fontSize: '14px', zIndex: 1, padding: '20px',
                } as React.CSSProperties}>
                    <span style={{ color: 'var(--error-color, #e74c3c)', fontSize: '15px' } as React.CSSProperties}>
                        Failed to load page
                    </span>
                    <span style={{
                        fontSize: '12px', color: 'var(--text-muted, #666)',
                        textAlign: 'center', maxWidth: 450, wordBreak: 'break-word',
                    } as React.CSSProperties}>
                        {error}
                    </span>
                    <button
                        onClick={handleRetry}
                        style={{
                            padding: '10px 24px', borderRadius: '6px',
                            border: '1px solid var(--accent-purple, #9b59b6)',
                            background: 'var(--accent-purple, #9b59b6)',
                            color: '#fff', cursor: 'pointer',
                            fontSize: '14px', fontWeight: 600, marginTop: 4,
                        } as React.CSSProperties}
                    >
                        Retry
                    </button>
                    <span style={{
                        fontSize: '11px', color: 'var(--text-muted, #666)',
                        wordBreak: 'break-all', maxWidth: 450, textAlign: 'center',
                    } as React.CSSProperties}>
                        {url}
                    </span>
                </div>
            )}
        </div>
    );
}
