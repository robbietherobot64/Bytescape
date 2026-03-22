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
}

let idCounter = 0;

/**
 * In-app browser tab backed by a native OS webview.
 *
 * Creates a real WebKitGTK/WebView2/WKWebView within the main window,
 * positioned over the React content area.
 *
 * ## Threading model
 *
 * The Rust `browser_create` command is **async**. Tauri spawns it on Tokio,
 * so the GTK main thread is free to process `window.add_child()` messages.
 * `await invoke('browser_create', ...)` resolves when the native webview is
 * fully created, shown, and focused.
 *
 * A 15-second safety timeout fires if the invoke never returns.
 */
export function WebViewScreen({ url, title: _title, isActive }: WebViewScreenProps) {
    const containerRef = useRef<HTMLDivElement>(null);
    const labelRef = useRef(`browser-${++idCounter}`);
    const createdRef = useRef(false);
    const destroyedRef = useRef(false);
    const isActiveRef = useRef(isActive);
    isActiveRef.current = isActive;

    const [phase, setPhase] = useState<'loading' | 'ready' | 'error'>('loading');
    const [errorMsg, setErrorMsg] = useState<string | null>(null);

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

    // ─── Create / show / hide the native webview ───
    useEffect(() => {
        if (destroyedRef.current || !isActive) return;

        // Already created — just re-sync bounds and show
        if (createdRef.current) {
            setPhase('ready');
            setActiveLabel(labelRef.current);
            syncBounds();
            // Re-show in case it was hidden by a tab switch
            invoke('browser_set_bounds', {
                label: labelRef.current,
                ...(containerRef.current ? (() => {
                    const r = containerRef.current!.getBoundingClientRect();
                    return { x: r.x, y: r.y, width: r.width, height: r.height };
                })() : { x: 0, y: 0, width: 800, height: 600 }),
            }).catch(() => {});
            return;
        }

        let timeoutId: ReturnType<typeof setTimeout> | null = null;

        const tryCreate = async () => {
            console.log('[WebView] === CREATION START ===');
            console.log('[WebView] label:', labelRef.current, 'url:', url);

            // If another concurrent call already created it, just adopt
            if (createdRef.current) {
                console.log('[WebView] already created by concurrent attempt, adopting');
                setPhase('ready');
                setActiveLabel(labelRef.current);
                syncBounds();
                return;
            }

            const el = containerRef.current;
            if (!el) {
                console.error('[WebView] ABORT: no container ref');
                return;
            }

            let rect = el.getBoundingClientRect();
            console.log('[WebView] rect:', JSON.stringify({
                x: rect.x, y: rect.y, w: rect.width, h: rect.height
            }));

            // Fallback: if container hasn't laid out yet, wait briefly
            if (rect.width < 1 || rect.height < 1) {
                console.log('[WebView] rect too small, waiting 150ms...');
                await new Promise<void>(r => setTimeout(r, 150));
                if (destroyedRef.current) return;
                rect = el.getBoundingClientRect();
            }

            if (rect.width < 1 || rect.height < 1) {
                console.error('[WebView] ABORT: container has zero dimensions');
                setPhase('error');
                setErrorMsg('Container has no dimensions — try closing and re-opening this tab.');
                return;
            }

            // Double-check — another attempt may have completed during await
            if (createdRef.current) {
                console.log('[WebView] created during wait, adopting');
                setPhase('ready');
                syncBounds();
                return;
            }

            const label = labelRef.current;

            console.log('[WebView] >>> invoke("browser_create")');
            console.log('[WebView] params:', JSON.stringify({
                label, url, x: rect.x, y: rect.y, width: rect.width, height: rect.height
            }));

            try {
                await invoke('browser_create', {
                    label,
                    url,
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                });
                console.log('[WebView] <<< invoke OK');
            } catch (err) {
                const errStr = String(err);
                console.warn('[WebView] <<< invoke error:', errStr);

                // If the webview already exists (created by a concurrent StrictMode
                // attempt), that's fine — just adopt it.
                if (errStr.includes('already exists') || errStr.includes('already been created')) {
                    console.log('[WebView] webview already exists, adopting');
                } else if (destroyedRef.current) {
                    return;
                } else {
                    setErrorMsg(errStr);
                    setPhase('error');
                    return;
                }
            }

            if (destroyedRef.current) return;

            // Mark as created — all concurrent attempts will see this
            createdRef.current = true;
            setActiveLabel(label);
            setCurrentUrl(url);
            setPhase('ready');
            console.log('[WebView] phase → ready');

            // Sync bounds immediately after creation to ensure full coverage
            syncBounds();

            // If user switched tabs during creation, hide the webview
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
                setPhase('error');
            }
        }, 15000);

        return () => {
            if (timeoutId) clearTimeout(timeoutId);
        };
    // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [isActive]);

    // ─── Show/hide native webview when tab activation changes ───
    useEffect(() => {
        if (!createdRef.current) return;
        const label = labelRef.current;
        if (isActive) {
            // Re-show and sync bounds when tab becomes active
            syncBounds();
        } else {
            // Hide when tab becomes inactive
            invoke('browser_hide', { label }).catch(() => {});
        }
    }, [isActive, syncBounds]);

    // ─── Track container size changes ───
    useEffect(() => {
        const el = containerRef.current;
        if (!el) return;
        const observer = new ResizeObserver(() => {
            if (isActiveRef.current && createdRef.current) syncBounds();
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

    // ─── Listen for title changes ───
    useEffect(() => {
        const label = labelRef.current;
        let unlisten: (() => void) | null = null;
        listen<string>(`browser-title-${label}`, (event) => {
            const newTitle = event.payload;
            if (!newTitle) return;
            const nav = useNavStore.getState();
            const tab = nav.tabs.find(t => t.screen === 'webview' && t.meta?.url === url);
            if (tab && nav.updateTabLabel) nav.updateTabLabel(tab.id, newTitle);
        }).then(fn => { unlisten = fn; });
        return () => { unlisten?.(); };
    }, [url]);

    // ─── Listen for "open in new tab" ───
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

    // ─── Retry handler ───
    const handleRetry = useCallback(() => {
        if (createdRef.current) {
            invoke('browser_close', { label: labelRef.current }).catch(() => {});
        }
        createdRef.current = false;
        destroyedRef.current = false;
        labelRef.current = `browser-${++idCounter}`;
        setPhase('loading');
        setErrorMsg(null);
    }, []);

    // ─── Render ───
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
            {/* LOADING — spinner while browser_create is in flight */}
            {phase === 'loading' && (
                <div style={{
                    position: 'absolute', inset: 0,
                    display: 'flex', flexDirection: 'column',
                    alignItems: 'center', justifyContent: 'center',
                    gap: '12px', color: 'var(--text-secondary, #aaa)',
                    fontSize: '14px', zIndex: 1,
                } as React.CSSProperties}>
                    <div style={{
                        width: 24, height: 24,
                        border: '3px solid var(--border-color, #444)',
                        borderTopColor: 'var(--accent-purple, #9b59b6)',
                        borderRadius: '50%',
                        animation: 'spin 0.8s linear infinite',
                    } as React.CSSProperties} />
                    <span>Loading page…</span>
                </div>
            )}

            {/* READY — overlay is gone, native webview renders on top */}
            {/* (nothing to render here — the native webview covers us) */}

            {/* ERROR — creation failed, show retry */}
            {phase === 'error' && (
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
                    {errorMsg && (
                        <span style={{
                            fontSize: '12px', color: 'var(--text-muted, #666)',
                            textAlign: 'center', maxWidth: 450, wordBreak: 'break-word',
                        } as React.CSSProperties}>
                            {errorMsg}
                        </span>
                    )}
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

            <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>
        </div>
    );
}
