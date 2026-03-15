import { useEffect, useRef, useState } from 'react';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { useNavStore } from '../store';

interface WebViewScreenProps {
    url: string;
    title?: string;
    /** Whether this tab is currently the active/visible tab */
    isActive: boolean;
}

let windowCounter = 0;

/**
 * In-app browser tab — opens a native OS browser window for the URL.
 *
 * Uses Tauri's WebviewWindow API which creates a real browser window
 * backed by the OS webview engine (WebKitGTK on Linux, WebView2 on Windows,
 * WKWebView on macOS). This loads ANY URL — no iframe restrictions.
 *
 * The tab in the main app tracks the window lifecycle:
 * - Mount → creates the window
 * - Unmount (tab closed) → destroys the window
 * - User closes the window → tab is removed
 * - Tab becomes active → window is focused
 */
export function WebViewScreen({ url, title, isActive }: WebViewScreenProps) {
    const windowRef = useRef<WebviewWindow | null>(null);
    const labelRef = useRef<string>(`browser-${++windowCounter}`);
    const isCreatingRef = useRef(false);
    const destroyedRef = useRef(false);
    const [status, setStatus] = useState<'creating' | 'open' | 'error'>('creating');
    const [errorMsg, setErrorMsg] = useState('');

    // Create the native window on mount
    useEffect(() => {
        const label = labelRef.current;
        destroyedRef.current = false;
        let unlistenClose: (() => void) | null = null;

        async function createWindow() {
            if (isCreatingRef.current) return;
            isCreatingRef.current = true;

            try {
                const displayTitle = title || (() => {
                    try {
                        return new URL(url).hostname;
                    } catch {
                        return 'Browser';
                    }
                })();

                const wvw = new WebviewWindow(label, {
                    url,
                    title: `${displayTitle} — Bytescape`,
                    width: 1024,
                    height: 768,
                    center: true,
                    focus: true,
                });

                // Wait for creation
                await new Promise<void>((resolve, reject) => {
                    wvw.once('tauri://created', () => resolve());
                    wvw.once('tauri://error', (e) => {
                        const msg = typeof e.payload === 'string'
                            ? e.payload
                            : (e.payload as { message?: string })?.message || 'Failed to create browser window';
                        reject(new Error(msg));
                    });
                });

                if (destroyedRef.current) {
                    wvw.destroy().catch(() => {});
                    return;
                }

                windowRef.current = wvw;
                setStatus('open');

                // Listen for the user closing the native window → close the tab
                unlistenClose = await wvw.onCloseRequested(async () => {
                    if (destroyedRef.current) return;
                    destroyedRef.current = true;
                    windowRef.current = null;
                    // Find and close the tab that owns this webview
                    const navState = useNavStore.getState();
                    const tab = navState.tabs.find(
                        t => t.screen === 'webview' && t.meta?.url === url
                    );
                    if (tab) {
                        navState.closeTab(tab.id);
                    }
                });
            } catch (err) {
                console.error('Failed to create browser window:', err);
                if (!destroyedRef.current) {
                    setStatus('error');
                    setErrorMsg(err instanceof Error ? err.message : String(err));
                }
            } finally {
                isCreatingRef.current = false;
            }
        }

        createWindow();

        return () => {
            destroyedRef.current = true;
            if (unlistenClose) unlistenClose();
            const wvw = windowRef.current;
            if (wvw) {
                wvw.destroy().catch(() => {});
                windowRef.current = null;
            }
        };
    }, [url, title]);

    // Focus the window when the tab becomes active
    useEffect(() => {
        if (isActive && windowRef.current) {
            windowRef.current.setFocus().catch(() => {});
        }
    }, [isActive]);

    return (
        <div className="webview-container">
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
                {status === 'creating' && (
                    <>
                        <div style={{ fontSize: '24px' } as React.CSSProperties}>🌐</div>
                        <div style={{ fontSize: '14px' } as React.CSSProperties}>Opening browser window…</div>
                        <div style={{ fontSize: '12px', color: 'var(--text-muted)', maxWidth: '500px', wordBreak: 'break-all' } as React.CSSProperties}>
                            {url}
                        </div>
                    </>
                )}
                {status === 'open' && (
                    <>
                        <div style={{ fontSize: '24px' } as React.CSSProperties}>🌐</div>
                        <div style={{ fontSize: '14px', color: 'var(--text-primary)' } as React.CSSProperties}>
                            Browsing in a separate window
                        </div>
                        <div style={{ fontSize: '12px', color: 'var(--text-muted)', maxWidth: '500px', wordBreak: 'break-all' } as React.CSSProperties}>
                            {url}
                        </div>
                        <button
                            onClick={() => windowRef.current?.setFocus().catch(() => {})}
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
                            Focus window
                        </button>
                    </>
                )}
                {status === 'error' && (
                    <>
                        <div style={{ fontSize: '24px' } as React.CSSProperties}>⚠️</div>
                        <div style={{ fontSize: '14px', color: 'var(--accent-red, #ff6b6b)' } as React.CSSProperties}>
                            Failed to open browser window
                        </div>
                        <div style={{ fontSize: '12px', color: 'var(--text-muted)', maxWidth: '500px', wordBreak: 'break-all' } as React.CSSProperties}>
                            {errorMsg}
                        </div>
                    </>
                )}
            </div>
        </div>
    );
}
