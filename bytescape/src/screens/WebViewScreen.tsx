import { useEffect, useRef, useCallback, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useNavStore, useBrowserStore } from '../store';
import { openInApp } from '../utils/openInApp';

interface WebViewScreenProps {
    url: string;
    title?: string;
    /** Whether this tab is currently the active/visible tab */
    isActive: boolean;
    /** Tab ID from the nav store — used for reliable title updates */
    tabId: string;
}

let idCounter = 0;

/**
 * Measure the area where the native webview should render.
 *
 * We measure `.main-content` (the stable layout ancestor) rather than
 * the container div itself, because the container is a flex child that
 * may not have resolved its height by the time we need coordinates.
 * `.main-content` is always visible and always has the correct size.
 */
function measureContentArea(containerEl: HTMLElement): DOMRect | null {
    const mainContent = containerEl.closest('.main-content') as HTMLElement | null;
    const target = mainContent || containerEl;
    const rect = target.getBoundingClientRect();
    if (rect.width < 1 || rect.height < 1) return null;
    return rect;
}

/**
 * In-app browser tab backed by a native OS webview.
 *
 * Creates a real WebKitGTK/WebView2/WKWebView within the main window,
 * positioned over the React content area. The native webview renders at
 * the OS level ABOVE all React content, so this component only needs to:
 *   1. Tell Rust to create the webview at the right coordinates
 *   2. Tell Rust to hide/show it when the tab activates/deactivates
 *   3. Show an error state if creation fails
 *
 * There is no "loading" overlay — the native webview handles its own
 * loading state (blank page → content) and renders above any React DOM.
 */
export function WebViewScreen({ url, title: _title, isActive, tabId }: WebViewScreenProps) {
    const containerRef = useRef<HTMLDivElement>(null);
    const labelRef = useRef(`browser-${++idCounter}`);
    const createdRef = useRef(false);
    const destroyedRef = useRef(false);
    const isActiveRef = useRef(isActive);
    isActiveRef.current = isActive;

    const [errorMsg, setErrorMsg] = useState<string | null>(null);

    const { setActiveLabel, setCurrentUrl } = useBrowserStore();

    /** Sync the native webview bounds to the content area. */
    const syncBounds = useCallback(async () => {
        if (!containerRef.current || destroyedRef.current || !createdRef.current) return;
        const rect = measureContentArea(containerRef.current);
        if (!rect) return;
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

    // ─── Register/unregister with browser store ───
    useEffect(() => {
        if (isActive) {
            if (createdRef.current) setActiveLabel(labelRef.current);
            setCurrentUrl(url);
        } else {
            const store = useBrowserStore.getState();
            if (store.activeLabel === labelRef.current) {
                setActiveLabel(null);
                setCurrentUrl(null);
            }
        }
    }, [isActive, url, setActiveLabel, setCurrentUrl]);

    useEffect(() => {
        const label = labelRef.current;
        return () => {
            const store = useBrowserStore.getState();
            if (store.activeLabel === label) {
                store.setActiveLabel(null);
                store.setCurrentUrl(null);
            }
        };
    }, []);

    // ─── Lifecycle: mount resets destroyed flag, unmount sets it ───
    // This MUST be a separate effect that runs before creation so that
    // React StrictMode's unmount→remount cycle works correctly.
    // Without this, StrictMode cleanup sets destroyedRef=true and the
    // creation effect on remount sees it and bails — webview never created.
    useEffect(() => {
        destroyedRef.current = false;
        createdRef.current = false;
        const label = labelRef.current;
        return () => {
            destroyedRef.current = true;
            if (createdRef.current) {
                invoke('browser_close', { label }).catch(() => {});
            }
        };
    }, []);

    // ─── Create the native webview ───
    useEffect(() => {
        if (destroyedRef.current || !isActive) return;
        if (createdRef.current) return;

        let timeoutId: ReturnType<typeof setTimeout> | null = null;
        let resyncId: ReturnType<typeof setTimeout> | null = null;

        const tryCreate = async () => {
            console.log('[WebView] === CREATION START ===');
            console.log('[WebView] label:', labelRef.current, 'url:', url);

            const el = containerRef.current;
            if (!el) {
                console.error('[WebView] ABORT: no container ref');
                return;
            }

            // Wait a frame for the flex layout to resolve, then measure
            await new Promise<void>(r => requestAnimationFrame(() => r()));
            if (destroyedRef.current) return;

            let rect = measureContentArea(el);

            // Fallback: if content area hasn't laid out yet, wait briefly
            if (!rect) {
                await new Promise<void>(r => setTimeout(r, 300));
                if (destroyedRef.current) return;
                rect = measureContentArea(el);
            }

            if (!rect) {
                console.error('[WebView] ABORT: content area has zero dimensions');
                setErrorMsg('Content area has no dimensions — try closing and re-opening this tab.');
                return;
            }

            const label = labelRef.current;
            console.log('[WebView] >>> browser_create label=' + label,
                'at', Math.round(rect.x), Math.round(rect.y),
                Math.round(rect.width) + 'x' + Math.round(rect.height));

            try {
                await invoke('browser_create', {
                    label,
                    url,
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                });
                console.log('[WebView] <<< browser_create OK');
            } catch (err) {
                console.error('[WebView] <<< browser_create FAILED:', err);
                if (destroyedRef.current) return;
                setErrorMsg(String(err));
                return;
            }

            if (destroyedRef.current) return;

            createdRef.current = true;
            setActiveLabel(label);
            setCurrentUrl(url);
            console.log('[WebView] created successfully');

            // Re-sync bounds after 300ms — catches layout shifts from React
            // re-render or late flex resolution.
            resyncId = setTimeout(() => {
                if (destroyedRef.current || !containerRef.current) return;
                const freshRect = measureContentArea(containerRef.current);
                if (!freshRect) return;
                invoke('browser_show_at', {
                    label,
                    x: freshRect.x,
                    y: freshRect.y,
                    width: freshRect.width,
                    height: freshRect.height,
                }).catch(() => {});
            }, 300);

            // If user switched tabs during creation, hide immediately
            if (!isActiveRef.current) {
                console.log('[WebView] tab inactive during creation, hiding');
                invoke('browser_hide', { label }).catch(() => {});
            }
        };

        tryCreate();

        // Safety timeout
        timeoutId = setTimeout(() => {
            if (!createdRef.current && !destroyedRef.current) {
                console.error('[WebView] TIMEOUT: browser_create did not complete in 15s');
                setErrorMsg(
                    'The browser engine is not responding. ' +
                    'Check the terminal for [browser_create] log messages.'
                );
            }
        }, 15000);

        return () => {
            if (timeoutId) clearTimeout(timeoutId);
            if (resyncId) clearTimeout(resyncId);
        };
    // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [isActive]);

    // ─── Show/hide native webview when tab activation changes ───
    useEffect(() => {
        const label = labelRef.current;
        if (isActive && createdRef.current) {
            console.log('[WebView] tab ACTIVE — will re-show, label:', label);
            const el = containerRef.current;
            if (el) {
                requestAnimationFrame(() => {
                    requestAnimationFrame(() => {
                        if (destroyedRef.current || !isActiveRef.current) return;
                        const rect = measureContentArea(el);
                        if (!rect) {
                            console.warn('[WebView] re-show: zero dimensions, skipping');
                            return;
                        }
                        invoke('browser_show_at', {
                            label,
                            x: rect.x,
                            y: rect.y,
                            width: rect.width,
                            height: rect.height,
                        }).catch((err) => console.error('[WebView] browser_show_at failed:', err));
                    });
                });
            }
        } else if (!isActive && createdRef.current) {
            console.log('[WebView] tab INACTIVE — hiding, label:', label);
            invoke('browser_hide', { label }).catch((err) => console.error('[WebView] hide failed:', err));
        }
    }, [isActive]);

    // ─── Track container size changes ───
    useEffect(() => {
        const el = containerRef.current;
        if (!el) return;
        const mainContent = el.closest('.main-content') as HTMLElement | null;
        const target = mainContent || el;
        const observer = new ResizeObserver(() => {
            if (isActiveRef.current && createdRef.current) syncBounds();
        });
        observer.observe(target);
        return () => observer.disconnect();
    }, [syncBounds]);

    // ─── Listen for URL changes from in-page navigation ───
    useEffect(() => {
        const label = labelRef.current;
        let unlisten: (() => void) | null = null;
        listen<string>(`browser-url-${label}`, (event) => {
            const newUrl = event.payload;
            if (!newUrl) return;
            const store = useBrowserStore.getState();
            if (store.activeLabel === label) {
                store.setCurrentUrl(newUrl);
                store.setIsLoading(false);
            }
        }).then(fn => { unlisten = fn; });
        return () => { unlisten?.(); };
    }, []);

    // ─── Listen for title changes ───
    useEffect(() => {
        const label = labelRef.current;
        let unlisten: (() => void) | null = null;
        listen<string>(`browser-title-${label}`, (event) => {
            const newTitle = event.payload;
            if (!newTitle) return;
            console.log('[WebView] title update:', newTitle, 'tabId:', tabId);
            const nav = useNavStore.getState();
            if (nav.updateTabLabel) nav.updateTabLabel(tabId, newTitle);
        }).then(fn => { unlisten = fn; });
        return () => { unlisten?.(); };
    }, [tabId]);

    // ─── Listen for "open in new tab" ───
    useEffect(() => {
        let unlisten: (() => void) | null = null;
        listen<string>('browser-new-tab', (event) => {
            openInApp(event.payload);
        }).then(fn => { unlisten = fn; });
        return () => { unlisten?.(); };
    }, []);

    // ─── Retry handler ───
    const handleRetry = useCallback(() => {
        if (createdRef.current) {
            invoke('browser_close', { label: labelRef.current }).catch(() => {});
        }
        createdRef.current = false;
        destroyedRef.current = false;
        labelRef.current = `browser-${++idCounter}`;
        setErrorMsg(null);
    }, []);

    // ─── Render ───
    // No loading overlay — the native webview renders at the OS level above
    // all React DOM, so a React spinner would be invisible anyway once the
    // webview appears. We only render an error state if creation fails.
    return (
        <div
            ref={containerRef}
            className="webview-container"
            style={{ position: 'relative' } as React.CSSProperties}
        >
            {errorMsg && (
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
                        {errorMsg}
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
