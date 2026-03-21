import { useEffect, useRef, useCallback, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useNavStore, useBrowserStore } from '../store';
import { openInApp } from '../utils/openInApp';

/**
 * Cached probe result for native child webview support.
 * On Linux/Wayland, `window.add_child()` returns UnsupportedWindowHandle.
 * On Linux/X11, child webviews have chronic z-ordering/visibility bugs.
 * We probe once and cache so subsequent tabs skip the native path.
 */
let nativeWebviewSupport: 'unknown' | 'yes' | 'no' = 'unknown';

interface WebViewScreenProps {
    url: string;
    title?: string;
    /** Whether this tab is currently the active/visible tab */
    isActive: boolean;
}

let idCounter = 0;

/**
 * In-app browser tab.
 *
 * Attempts to create a native OS webview (WebKitGTK/WebView2/WKWebView)
 * within the main window. If native creation fails or the webview doesn't
 * render within a short timeout, shows a fallback landing page with a
 * prominent "Open in System Browser" button.
 *
 * The native approach has chronic issues on Linux (Wayland unsupported,
 * X11 z-ordering bugs, GTK main loop deadlocks with Tauri JS API calls).
 * The fallback is cached so subsequent tabs skip the native probe.
 */
export function WebViewScreen({ url, title: _title, isActive }: WebViewScreenProps) {
    const containerRef = useRef<HTMLDivElement>(null);
    const labelRef = useRef(`browser-${++idCounter}`);
    const createdRef = useRef(false);
    const destroyedRef = useRef(false);
    const isActiveRef = useRef(isActive);
    isActiveRef.current = isActive;

    // Phase: 'loading' → attempting native | 'native' → it worked | 'fallback' → system browser
    const [phase, setPhase] = useState<'loading' | 'native' | 'fallback'>('loading');
    const [errorMsg, setErrorMsg] = useState<string | null>(null);
    const [openedExternal, setOpenedExternal] = useState(false);

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

    // ─── Core: attempt native webview, fall back to system browser ───
    useEffect(() => {
        if (destroyedRef.current || !isActive) return;

        // Skip native if we already know it doesn't work
        if (nativeWebviewSupport === 'no') {
            setPhase('fallback');
            return;
        }

        // Already created — just show it
        if (createdRef.current) {
            setPhase('native');
            setActiveLabel(labelRef.current);
            syncBounds();
            return;
        }

        let cancelled = false;

        // ── HARD safety timeout ──
        // If ANYTHING hangs (invoke, GTK deadlock, IPC stall), force fallback.
        // This runs as a plain setTimeout — independent of the async flow.
        const safetyTimer = setTimeout(() => {
            if (!cancelled) {
                console.warn('[WebView] Safety timeout: forcing fallback after 4s');
                nativeWebviewSupport = 'no';
                setPhase('fallback');
            }
        }, 4000);

        const tryCreate = async () => {
            const el = containerRef.current;
            if (!el || cancelled) return;

            // Wait one frame for layout
            await new Promise<void>(r => requestAnimationFrame(() => r()));
            if (cancelled || destroyedRef.current) { setPhase('fallback'); return; }

            const rect = el.getBoundingClientRect();
            if (rect.width < 1 || rect.height < 1) {
                await new Promise<void>(r => setTimeout(r, 200));
                if (cancelled || destroyedRef.current) return;
            }

            const r = el.getBoundingClientRect();
            if (r.width < 1 || r.height < 1) {
                if (!cancelled) { nativeWebviewSupport = 'no'; setPhase('fallback'); }
                return;
            }

            try {
                // Short timeout — if browser_create doesn't return in 3s, bail
                await Promise.race([
                    invoke('browser_create', {
                        label: labelRef.current,
                        url,
                        x: r.x,
                        y: r.y,
                        width: r.width,
                        height: r.height,
                    }),
                    new Promise<never>((_, rej) =>
                        setTimeout(() => rej(new Error('timeout')), 3000)
                    ),
                ]);

                if (cancelled || destroyedRef.current) return;
                createdRef.current = true;
                setActiveLabel(labelRef.current);
                setCurrentUrl(url);

                // ── Check if the webview actually rendered ──
                // Listen for a page-load event (emitted by on_page_load in Rust).
                // Give it 2.5s — if nothing, native webview is invisible.
                const loaded = await new Promise<boolean>((resolve) => {
                    if (cancelled) { resolve(false); return; }
                    const t = setTimeout(() => resolve(false), 2500);
                    listen<string>(`browser-url-${labelRef.current}`, () => {
                        clearTimeout(t);
                        resolve(true);
                    }).catch(() => { clearTimeout(t); resolve(false); });
                });

                if (cancelled || destroyedRef.current) return;

                if (loaded) {
                    nativeWebviewSupport = 'yes';
                    setPhase('native');
                } else {
                    // Created but invisible — clean up and fall back
                    console.warn('[WebView] Created but page never loaded — falling back');
                    invoke('browser_close', { label: labelRef.current }).catch(() => {});
                    createdRef.current = false;
                    nativeWebviewSupport = 'no';
                    setPhase('fallback');
                }
            } catch (err) {
                if (cancelled || destroyedRef.current) return;
                console.warn('[WebView] browser_create failed:', err);
                nativeWebviewSupport = 'no';
                setErrorMsg(String(err));
                setPhase('fallback');
            }
        };

        tryCreate();

        return () => {
            cancelled = true;
            clearTimeout(safetyTimer);
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

    // ─── Handlers ───
    const handleOpenExternal = useCallback(() => {
        openUrl(url).catch(() => {});
        setOpenedExternal(true);
    }, [url]);

    const handleRetryNative = useCallback(() => {
        if (createdRef.current) {
            invoke('browser_close', { label: labelRef.current }).catch(() => {});
        }
        nativeWebviewSupport = 'unknown';
        createdRef.current = false;
        destroyedRef.current = false;
        labelRef.current = `browser-${++idCounter}`;
        setPhase('loading');
        setErrorMsg(null);
        setOpenedExternal(false);
    }, []);

    const handleCopyUrl = useCallback(() => {
        navigator.clipboard.writeText(url).catch(() => {});
    }, [url]);

    const parsedHost = (() => {
        try { return new URL(url).hostname; } catch { return url; }
    })();

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
            {/* LOADING — max 4s then safety timeout kicks in */}
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

            {/* NATIVE — webview is rendering over this container */}
            {phase === 'native' && (
                <div style={{
                    position: 'absolute', bottom: 0, left: 0, right: 0,
                    height: '28px', display: 'flex', alignItems: 'center',
                    justifyContent: 'center',
                    background: 'rgba(0,0,0,0.4)',
                    color: 'var(--text-muted, #666)', fontSize: '11px',
                    zIndex: 0, pointerEvents: 'none',
                } as React.CSSProperties}>
                    {parsedHost}
                </div>
            )}

            {/* FALLBACK — system browser landing page */}
            {phase === 'fallback' && (
                <div style={{
                    position: 'absolute', inset: 0,
                    display: 'flex', flexDirection: 'column',
                    alignItems: 'center', justifyContent: 'center',
                    gap: '16px', color: 'var(--text-secondary, #aaa)',
                    fontSize: '14px', zIndex: 1, padding: '20px',
                } as React.CSSProperties}>
                    <span style={{ fontSize: '36px' }}>🌐</span>
                    <span style={{
                        fontSize: '16px', fontWeight: 600,
                        color: 'var(--text-primary, #fff)',
                    } as React.CSSProperties}>
                        External Page
                    </span>
                    <span style={{
                        textAlign: 'center', maxWidth: 420, lineHeight: 1.5,
                    } as React.CSSProperties}>
                        The in-app browser isn't available on this platform.
                        {errorMsg && (
                            <span style={{ display: 'block', fontSize: '11px', marginTop: 4, color: 'var(--text-muted, #666)' } as React.CSSProperties}>
                                ({errorMsg})
                            </span>
                        )}
                    </span>
                    <div style={{ display: 'flex', flexDirection: 'column', gap: '10px', alignItems: 'center', marginTop: 4 } as React.CSSProperties}>
                        {!openedExternal ? (
                            <button
                                onClick={handleOpenExternal}
                                style={{
                                    padding: '12px 28px', borderRadius: '8px',
                                    border: 'none',
                                    background: 'var(--accent-purple, #9b59b6)',
                                    color: '#fff', cursor: 'pointer',
                                    fontSize: '15px', fontWeight: 600,
                                } as React.CSSProperties}
                            >
                                Open in System Browser
                            </button>
                        ) : (
                            <span style={{
                                padding: '12px 28px', borderRadius: '8px',
                                border: '1px solid var(--border-color, #444)',
                                background: 'var(--bg-tertiary, #1a1a1a)',
                                color: 'var(--text-secondary, #aaa)', fontSize: '14px',
                            } as React.CSSProperties}>
                                ✓ Opened in your browser
                            </span>
                        )}
                        <div style={{ display: 'flex', gap: '10px' } as React.CSSProperties}>
                            <button
                                onClick={handleCopyUrl}
                                style={{
                                    padding: '8px 16px', borderRadius: '6px',
                                    border: '1px solid var(--border-color, #444)',
                                    background: 'var(--bg-secondary, #222)',
                                    color: 'var(--text-primary, #fff)',
                                    cursor: 'pointer', fontSize: '13px',
                                } as React.CSSProperties}
                            >
                                Copy URL
                            </button>
                            <button
                                onClick={handleRetryNative}
                                style={{
                                    padding: '8px 16px', borderRadius: '6px',
                                    border: '1px solid var(--border-color, #444)',
                                    background: 'var(--bg-secondary, #222)',
                                    color: 'var(--text-muted, #888)',
                                    cursor: 'pointer', fontSize: '13px',
                                } as React.CSSProperties}
                            >
                                Retry In-App
                            </button>
                        </div>
                    </div>
                    <div style={{
                        marginTop: 8, padding: '8px 16px',
                        background: 'var(--bg-tertiary, #1a1a1a)',
                        borderRadius: '6px', fontSize: '12px',
                        color: 'var(--text-muted, #666)',
                        wordBreak: 'break-all', maxWidth: 500, textAlign: 'center',
                    } as React.CSSProperties}>
                        {url}
                    </div>
                </div>
            )}

            <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>
        </div>
    );
}
