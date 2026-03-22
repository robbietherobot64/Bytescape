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
    // Attempt counter survives React StrictMode's mount/cleanup/remount cycle.
    // Each effect run increments this; stale attempts bail out instead of using
    // a closure-based `cancelled` flag that StrictMode's cleanup resets.
    const attemptRef = useRef(0);

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

        // Already created — just re-sync bounds
        if (createdRef.current) {
            setPhase('ready');
            setActiveLabel(labelRef.current);
            syncBounds();
            return;
        }

        // Increment attempt counter. If a newer attempt starts (e.g. from
        // React StrictMode remount), stale attempts will bail out.
        const myAttempt = ++attemptRef.current;
        const isStale = () => attemptRef.current !== myAttempt || destroyedRef.current;

        let invokeReturned = false;
        let timeoutId: ReturnType<typeof setTimeout> | null = null;

        const tryCreate = async () => {
            console.log('[WebView] === CREATION START === attempt:', myAttempt);
            console.log('[WebView] label:', labelRef.current, 'url:', url);

            const el = containerRef.current;
            if (!el) {
                console.error('[WebView] ABORT: no container ref');
                return;
            }

            // Effects run after paint — layout is already computed.
            // No requestAnimationFrame needed (and rAF creates a timing
            // gap that React StrictMode exploits in dev mode).
            let rect = el.getBoundingClientRect();
            console.log('[WebView] rect:', JSON.stringify({
                x: rect.x, y: rect.y, w: rect.width, h: rect.height
            }));

            // Fallback: if container hasn't laid out yet, wait briefly
            if (rect.width < 1 || rect.height < 1) {
                console.log('[WebView] rect too small, waiting 150ms...');
                await new Promise<void>(r => setTimeout(r, 150));
                if (isStale()) { console.log('[WebView] ABORT: stale after dim wait'); return; }
                rect = el.getBoundingClientRect();
                console.log('[WebView] rect after wait:', JSON.stringify({
                    x: rect.x, y: rect.y, w: rect.width, h: rect.height
                }));
            }

            if (rect.width < 1 || rect.height < 1) {
                console.error('[WebView] ABORT: container has zero dimensions');
                if (!isStale()) {
                    setPhase('error');
                    setErrorMsg('Container has no dimensions — try closing and re-opening this tab.');
                }
                return;
            }

            // Skip if a more recent attempt already created the webview
            if (createdRef.current) {
                console.log('[WebView] another attempt already created webview, bailing out');
                return;
            }

            const label = labelRef.current;

            console.log('[WebView] >>> calling invoke("browser_create")...');
            console.log('[WebView] params:', JSON.stringify({
                label, url, x: rect.x, y: rect.y, width: rect.width, height: rect.height
            }));

            try {
                const result = await invoke('browser_create', {
                    label,
                    url,
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                });
                invokeReturned = true;
                console.log('[WebView] <<< invoke returned OK, result:', result);

                if (destroyedRef.current) {
                    console.log('[WebView] component unmounted after invoke returned');
                    return;
                }

                createdRef.current = true;
                setActiveLabel(label);
                setCurrentUrl(url);
                setPhase('ready');
                console.log('[WebView] phase → ready');

                // If user switched tabs during creation, hide the webview
                if (!isActiveRef.current) {
                    console.log('[WebView] tab inactive during creation, hiding');
                    invoke('browser_hide', { label }).catch(() => {});
                }
            } catch (err) {
                invokeReturned = true;
                console.error('[WebView] <<< invoke REJECTED:', err);
                if (isStale()) return;
                setErrorMsg(String(err));
                setPhase('error');
            }
        };

        tryCreate();

        // Safety timeout: if invoke never returns, show an actionable error
        timeoutId = setTimeout(() => {
            if (!createdRef.current && !destroyedRef.current && attemptRef.current === myAttempt) {
                if (!invokeReturned) {
                    console.error('[WebView] TIMEOUT: invoke("browser_create") never returned after 15s');
                    setErrorMsg(
                        'The browser engine is not responding — invoke("browser_create") ' +
                        'never returned. Check the terminal (where `pnpm tauri dev` runs) ' +
                        'for [browser_create] log messages.'
                    );
                } else {
                    console.error('[WebView] TIMEOUT: invoke returned but phase never reached ready');
                    setErrorMsg(
                        'Webview was created but did not display. ' +
                        'Check the terminal for [browser_create] log messages.'
                    );
                }
                setPhase('error');
            }
        }, 15000);

        return () => {
            if (timeoutId) clearTimeout(timeoutId);
        };
    // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [isActive]);

    // ─── Hide native webview when tab is inactive ───
    useEffect(() => {
        if (!isActive && createdRef.current) {
            invoke('browser_hide', { label: labelRef.current }).catch(() => {});
        }
    }, [isActive]);

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
