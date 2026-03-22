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
 * positioned over the React content area. The Rust side calls show() and
 * set_focus() on the webview directly (avoiding the Tauri JS API which
 * can hang on Linux). Once browser_create returns Ok, the overlay is
 * cleared immediately and the native webview renders on top.
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

        // Already created — just re-sync bounds
        if (createdRef.current) {
            setPhase('ready');
            setActiveLabel(labelRef.current);
            syncBounds();
            return;
        }

        let cancelled = false;

        const tryCreate = async () => {
            const el = containerRef.current;
            if (!el || cancelled) return;

            // Wait one frame for the container to have layout dimensions
            await new Promise<void>(r => requestAnimationFrame(() => r()));
            if (cancelled || destroyedRef.current) return;

            let rect = el.getBoundingClientRect();
            if (rect.width < 1 || rect.height < 1) {
                // Layout not ready yet — wait a beat and retry
                await new Promise<void>(r => setTimeout(r, 250));
                if (cancelled || destroyedRef.current) return;
                rect = el.getBoundingClientRect();
            }
            if (rect.width < 1 || rect.height < 1) {
                setPhase('error');
                setErrorMsg('Container has no dimensions — try closing and re-opening this tab.');
                return;
            }

            try {
                await invoke('browser_create', {
                    label: labelRef.current,
                    url,
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                });

                if (cancelled || destroyedRef.current) return;

                // browser_create succeeded — Rust already called show() + set_focus()
                // on the native webview. Clear the overlay immediately.
                createdRef.current = true;
                setActiveLabel(labelRef.current);
                setCurrentUrl(url);
                setPhase('ready');

                // If user switched away during creation, hide the webview
                if (!isActiveRef.current) {
                    invoke('browser_hide', { label: labelRef.current }).catch(() => {});
                }
            } catch (err) {
                if (cancelled || destroyedRef.current) return;
                console.error('[WebView] browser_create failed:', err);
                setErrorMsg(String(err));
                setPhase('error');
            }
        };

        tryCreate();

        return () => { cancelled = true; };
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
