import { useEffect, useRef, useCallback, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Webview } from '@tauri-apps/api/webview';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useNavStore, useBrowserStore } from '../store';
import { openInApp } from '../utils/openInApp';

/** Wrap an invoke in a timeout so a hanging Rust command doesn't freeze the UI. */
function invokeWithTimeout<T>(cmd: string, args: Record<string, unknown>, ms: number): Promise<T> {
    return Promise.race([
        invoke<T>(cmd, args),
        new Promise<never>((_, reject) =>
            setTimeout(() => reject(new Error(`Command '${cmd}' timed out after ${ms / 1000}s`)), ms)
        ),
    ]);
}

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
 * Navigation controls live in the global Omnibar (TopBar). This component
 * communicates the current URL and webview label via useBrowserStore so
 * the Omnibar can control back/forward/reload/URL-navigate.
 */
export function WebViewScreen({ url, title: _title, isActive }: WebViewScreenProps) {
    const containerRef = useRef<HTMLDivElement>(null);
    const labelRef = useRef(`browser-${++idCounter}`);
    const createdRef = useRef(false);
    const destroyedRef = useRef(false);
    const isActiveRef = useRef(isActive);
    isActiveRef.current = isActive;

    const [createError, setCreateError] = useState<string | null>(null);
    const [isCreating, setIsCreating] = useState(false);

    const { setActiveLabel, setCurrentUrl } = useBrowserStore();

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

    // ─── Register/unregister this webview with the browser store ───
    useEffect(() => {
        if (isActive && createdRef.current) {
            setActiveLabel(labelRef.current);
            setCurrentUrl(url);
        } else if (isActive && !createdRef.current) {
            setCurrentUrl(url);
        }
        if (!isActive) {
            const store = useBrowserStore.getState();
            if (store.activeLabel === labelRef.current) {
                setActiveLabel(null);
                setCurrentUrl(null);
            }
        }
    }, [isActive, url, setActiveLabel, setCurrentUrl]);

    // Cleanup store on unmount
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

    // ─── Create / Show / Hide based on isActive ───
    useEffect(() => {
        if (destroyedRef.current) return;

        if (isActive) {
            if (!createdRef.current) {
                const create = async () => {
                    const el = containerRef.current;
                    if (!el) return;

                    setIsCreating(true);
                    setCreateError(null);

                    const delays = [0, 50, 150, 300, 500];
                    for (let attempt = 0; attempt < delays.length; attempt++) {
                        if (destroyedRef.current) return;

                        await new Promise<void>(r => {
                            if (attempt === 0) {
                                requestAnimationFrame(() => r());
                            } else {
                                setTimeout(r, delays[attempt]);
                            }
                        });

                        if (destroyedRef.current) return;

                        const rect = el.getBoundingClientRect();
                        if (rect.width < 1 || rect.height < 1) {
                            if (attempt < delays.length - 1) continue;
                            setIsCreating(false);
                            setCreateError('Page container failed to initialize. Try closing and re-opening this tab.');
                            return;
                        }

                        try {
                            await invokeWithTimeout('browser_create', {
                                label: labelRef.current,
                                url,
                                x: rect.x,
                                y: rect.y,
                                width: rect.width,
                                height: rect.height,
                            }, 8000);
                            createdRef.current = true;
                            setIsCreating(false);
                            setActiveLabel(labelRef.current);
                            setCurrentUrl(url);

                            try {
                                const wv = await Webview.getByLabel(labelRef.current);
                                if (wv) {
                                    await wv.show();
                                    await wv.setFocus();
                                }
                            } catch (focusErr) {
                                console.warn('[WebView] show/focus failed (non-fatal):', focusErr);
                            }

                            if (!isActiveRef.current) {
                                invoke('browser_hide', { label: labelRef.current }).catch(() => {});
                            }
                            return;
                        } catch (err) {
                            console.error('[WebView] create error (attempt', attempt + 1, '):', err);
                            if (attempt < delays.length - 1) continue;
                            setIsCreating(false);
                            setCreateError(`Failed to open page: ${err}`);
                        }
                    }
                };
                create();
            } else {
                setActiveLabel(labelRef.current);
                requestAnimationFrame(async () => {
                    await syncBounds();
                    try {
                        const wv = await Webview.getByLabel(labelRef.current);
                        if (wv) {
                            await wv.show();
                            await wv.setFocus();
                        }
                    } catch {
                        // non-fatal
                    }
                });
            }
        } else if (createdRef.current) {
            invoke('browser_hide', { label: labelRef.current }).catch(() => {});
        }
    }, [isActive, url, syncBounds, setActiveLabel, setCurrentUrl]);

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

    // ─── Listen for title changes from the native webview ───
    useEffect(() => {
        const label = labelRef.current;
        let unlisten: (() => void) | null = null;
        listen<string>(`browser-title-${label}`, (event) => {
            const newTitle = event.payload;
            if (!newTitle) return;
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

    const handleRetry = () => {
        setCreateError(null);
        createdRef.current = false;
        destroyedRef.current = false;
        const el = containerRef.current;
        if (!el) return;
        setIsCreating(true);

        const create = async () => {
            const delays = [0, 100, 300, 600, 1000];
            for (let attempt = 0; attempt < delays.length; attempt++) {
                if (destroyedRef.current) return;
                await new Promise<void>(r => setTimeout(r, delays[attempt]));
                if (destroyedRef.current) return;
                const rect = el.getBoundingClientRect();
                if (rect.width < 1 || rect.height < 1) {
                    if (attempt < delays.length - 1) continue;
                    setIsCreating(false);
                    setCreateError('Page container failed to initialize. Try closing and re-opening this tab.');
                    return;
                }
                try {
                    await invokeWithTimeout('browser_create', {
                        label: labelRef.current,
                        url,
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: rect.height,
                    }, 8000);
                    createdRef.current = true;
                    setIsCreating(false);
                    setActiveLabel(labelRef.current);
                    setCurrentUrl(url);
                    return;
                } catch (err) {
                    if (attempt < delays.length - 1) continue;
                    setIsCreating(false);
                    setCreateError(`Failed to open page: ${err}`);
                }
            }
        };
        create();
    };

    return (
        <div
            ref={containerRef}
            className="webview-container"
            style={{
                width: '100%',
                height: '100%',
                background: 'var(--bg-primary)',
                position: 'relative',
            } as React.CSSProperties}
        >
            {(isCreating || createError) && (
                <div style={{
                    position: 'absolute',
                    inset: 0,
                    display: 'flex',
                    flexDirection: 'column',
                    alignItems: 'center',
                    justifyContent: 'center',
                    gap: '12px',
                    color: 'var(--text-secondary, #aaa)',
                    fontSize: '14px',
                    zIndex: 1,
                } as React.CSSProperties}>
                    {isCreating && !createError && (
                        <span>Loading page…</span>
                    )}
                    {createError && (
                        <>
                            <span style={{ color: 'var(--error-color, #e74c3c)' } as React.CSSProperties}>
                                {createError}
                            </span>
                            <div style={{ display: 'flex', gap: 10 } as React.CSSProperties}>
                                <button
                                    onClick={handleRetry}
                                    style={{
                                        padding: '8px 16px',
                                        borderRadius: '6px',
                                        border: '1px solid var(--border-color, #444)',
                                        background: 'var(--bg-secondary, #222)',
                                        color: 'var(--text-primary, #fff)',
                                        cursor: 'pointer',
                                        fontSize: '13px',
                                    } as React.CSSProperties}
                                >
                                    Retry
                                </button>
                                <button
                                    onClick={() => openUrl(url).catch(() => {})}
                                    style={{
                                        padding: '8px 16px',
                                        borderRadius: '6px',
                                        border: '1px solid var(--accent-purple, #9b59b6)',
                                        background: 'var(--accent-purple, #9b59b6)',
                                        color: '#fff',
                                        cursor: 'pointer',
                                        fontSize: '13px',
                                    } as React.CSSProperties}
                                >
                                    Open in System Browser
                                </button>
                            </div>
                        </>
                    )}
                </div>
            )}
        </div>
    );
}
