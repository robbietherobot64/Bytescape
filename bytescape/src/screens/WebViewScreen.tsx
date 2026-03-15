import { useEffect, useRef, useCallback, useState } from 'react';
import { Webview } from '@tauri-apps/api/webview';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { LogicalPosition, LogicalSize } from '@tauri-apps/api/dpi';
import { openUrl } from '@tauri-apps/plugin-opener';

interface WebViewScreenProps {
    url: string;
    title?: string;
    /** Whether this tab is currently the active/visible tab */
    isActive: boolean;
}

let webviewCounter = 0;

/**
 * In-app browser tab — renders an external URL using a native Tauri webview
 * overlay positioned inside the content area.
 *
 * Uses Tauri's native Webview API (not an iframe) so all sites load
 * properly, including those with X-Frame-Options restrictions like bsky.app.
 *
 * Requires the `unstable` feature flag on the tauri crate in Cargo.toml
 * to enable multi-webview support.
 *
 * On Linux (WebKitGTK), the invoke for webview creation may hang without
 * resolving, so we poll with Webview.getByLabel() as a fallback.
 */
export function WebViewScreen({ url, isActive }: WebViewScreenProps) {
    const containerRef = useRef<HTMLDivElement>(null);
    const webviewRef = useRef<Webview | null>(null);
    const labelRef = useRef<string>(`browser-${++webviewCounter}`);
    const isCreatingRef = useRef(false);
    const isActiveRef = useRef(isActive);
    isActiveRef.current = isActive;

    const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
    const [errorMsg, setErrorMsg] = useState<string>('');

    // Position the webview to match the container's bounds
    const syncPosition = useCallback(() => {
        const wv = webviewRef.current;
        const container = containerRef.current;
        if (!wv || !container) return;

        const rect = container.getBoundingClientRect();
        wv.setPosition(new LogicalPosition(rect.x, rect.y)).catch(() => {});
        wv.setSize(new LogicalSize(Math.max(rect.width, 100), Math.max(rect.height, 100))).catch(() => {});
    }, []);

    // Create the native webview on mount, destroy on unmount
    useEffect(() => {
        const label = labelRef.current;
        let destroyed = false;
        let pollTimer: ReturnType<typeof setInterval> | null = null;

        async function create() {
            if (isCreatingRef.current) return;
            isCreatingRef.current = true;
            setStatus('loading');
            setErrorMsg('');

            const container = containerRef.current;
            if (!container) {
                isCreatingRef.current = false;
                return;
            }

            // Wait for layout to settle — two rAF cycles to ensure paint
            await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));
            if (destroyed) return;

            let rect = container.getBoundingClientRect();

            // Validate container has reasonable dimensions
            if (rect.width < 10 || rect.height < 10) {
                await new Promise(r => setTimeout(r, 200));
                if (destroyed) return;
                rect = container.getBoundingClientRect();
                if (rect.width < 10 || rect.height < 10) {
                    setStatus('error');
                    setErrorMsg('Unable to determine content area dimensions.');
                    isCreatingRef.current = false;
                    return;
                }
            }

            try {
                const appWindow = getCurrentWindow();
                const wv = new Webview(appWindow, label, {
                    url,
                    x: rect.x,
                    y: rect.y,
                    width: Math.max(rect.width, 100),
                    height: Math.max(rect.height, 100),
                    focus: true,
                });

                // Listen for creation success/failure (works when invoke resolves)
                wv.once('tauri://created', () => {
                    if (destroyed) return;
                    if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
                    setStatus('ready');
                    if (!isActiveRef.current) {
                        wv.hide().catch(() => {});
                    }
                });

                wv.once('tauri://error', (e) => {
                    if (destroyed) return;
                    if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
                    const msg = typeof e.payload === 'string'
                        ? e.payload
                        : (e.payload as { message?: string })?.message || 'Unknown error creating webview';
                    console.error('Webview creation error:', msg);
                    setStatus('error');
                    setErrorMsg(msg);
                    webviewRef.current = null;
                });

                if (destroyed) {
                    wv.close().catch(() => {});
                    return;
                }

                webviewRef.current = wv;

                // Fallback: On Linux, the invoke('plugin:webview|create_webview')
                // may hang without resolving (neither created nor error fires).
                // Poll with Webview.getByLabel() to detect when it's actually ready.
                let pollCount = 0;
                pollTimer = setInterval(async () => {
                    pollCount++;
                    if (destroyed) {
                        if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
                        return;
                    }
                    try {
                        const found = await Webview.getByLabel(label);
                        if (found) {
                            // Webview exists — it was created successfully
                            if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
                            setStatus('ready');
                            if (!isActiveRef.current) {
                                wv.hide().catch(() => {});
                            } else {
                                syncPosition();
                            }
                        }
                    } catch {
                        // getByLabel failed — ignore, keep polling
                    }
                    // Give up after 15 seconds
                    if (pollCount >= 30) {
                        if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
                        setStatus('error');
                        setErrorMsg('Webview creation timed out. Try opening in your browser instead.');
                    }
                }, 500);

            } catch (err) {
                console.error('Failed to create native webview:', err);
                if (!destroyed) {
                    setStatus('error');
                    setErrorMsg(err instanceof Error ? err.message : String(err));
                }
            }
            isCreatingRef.current = false;
        }

        create();

        return () => {
            destroyed = true;
            if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
            const wv = webviewRef.current;
            if (wv) {
                wv.close().catch(() => {});
                webviewRef.current = null;
            }
        };
    }, [url]); // eslint-disable-line react-hooks/exhaustive-deps

    // Show/hide based on tab active state
    useEffect(() => {
        const wv = webviewRef.current;
        if (!wv) return;

        if (isActive) {
            wv.show().catch(() => {});
            wv.setFocus().catch(() => {});
            syncPosition();
        } else {
            wv.hide().catch(() => {});
        }
    }, [isActive, syncPosition]);

    // Track container size/position changes
    useEffect(() => {
        const container = containerRef.current;
        if (!container) return;

        const observer = new ResizeObserver(() => {
            if (isActiveRef.current) syncPosition();
        });
        observer.observe(container);

        const handleResize = () => {
            if (isActiveRef.current) syncPosition();
        };
        window.addEventListener('resize', handleResize);

        return () => {
            observer.disconnect();
            window.removeEventListener('resize', handleResize);
        };
    }, [syncPosition]);

    const handleOpenExternal = async () => {
        try {
            await openUrl(url);
        } catch {
            window.open(url, '_blank');
        }
    };

    return (
        <div
            ref={containerRef}
            className="webview-container"
        >
            {status === 'loading' && (
                <div style={{
                    display: 'flex',
                    flexDirection: 'column',
                    alignItems: 'center',
                    justifyContent: 'center',
                    height: '100%',
                    color: 'var(--text-secondary)',
                    gap: '12px',
                } as React.CSSProperties}>
                    <div style={{ fontSize: '16px' } as React.CSSProperties}>Loading…</div>
                    <div style={{ fontSize: '12px', color: 'var(--text-muted)' } as React.CSSProperties}>
                        {url}
                    </div>
                </div>
            )}
            {status === 'error' && (
                <div style={{
                    display: 'flex',
                    flexDirection: 'column',
                    alignItems: 'center',
                    justifyContent: 'center',
                    height: '100%',
                    color: 'var(--text-secondary)',
                    gap: '16px',
                    padding: '24px',
                    textAlign: 'center',
                } as React.CSSProperties}>
                    <div style={{ fontSize: '18px', color: 'var(--text-primary)' } as React.CSSProperties}>
                        Unable to load page
                    </div>
                    <div style={{
                        fontSize: '12px',
                        color: 'var(--text-muted)',
                        maxWidth: '500px',
                        wordBreak: 'break-word',
                    } as React.CSSProperties}>
                        {errorMsg || 'An error occurred while creating the browser view.'}
                    </div>
                    <button
                        onClick={handleOpenExternal}
                        style={{
                            padding: '8px 20px',
                            background: 'var(--accent-purple)',
                            color: 'var(--text-primary)',
                            border: 'none',
                            borderRadius: 'var(--border-radius)',
                            cursor: 'pointer',
                            fontSize: '13px',
                        } as React.CSSProperties}
                    >
                        Open in external browser
                    </button>
                </div>
            )}
        </div>
    );
}
