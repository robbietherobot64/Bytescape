import { useEffect, useRef, useCallback } from 'react';
import { Webview } from '@tauri-apps/api/webview';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { LogicalPosition, LogicalSize } from '@tauri-apps/api/dpi';

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
 * No toolbar or URL bar — the existing Omnibar shows the URL.
 * Works just like opening a new tab in a real browser: the tab appears
 * at the top, and the content fills the screen.
 *
 * Uses Tauri's native Webview API (not an iframe) so all sites load
 * properly, including those with X-Frame-Options restrictions like bsky.app.
 */
export function WebViewScreen({ url, isActive }: WebViewScreenProps) {
    const containerRef = useRef<HTMLDivElement>(null);
    const webviewRef = useRef<Webview | null>(null);
    const labelRef = useRef<string>(`browser-${++webviewCounter}`);
    const isCreatingRef = useRef(false);
    const isActiveRef = useRef(isActive);
    isActiveRef.current = isActive;

    // Position the webview to match the container's bounds
    const syncPosition = useCallback(() => {
        const wv = webviewRef.current;
        const container = containerRef.current;
        if (!wv || !container) return;

        const rect = container.getBoundingClientRect();
        wv.setPosition(new LogicalPosition(rect.x, rect.y)).catch(() => {});
        wv.setSize(new LogicalSize(rect.width, rect.height)).catch(() => {});
    }, []);

    // Create the native webview on mount, destroy on unmount
    useEffect(() => {
        const label = labelRef.current;
        let destroyed = false;

        async function create() {
            if (isCreatingRef.current) return;
            isCreatingRef.current = true;

            const container = containerRef.current;
            if (!container) {
                isCreatingRef.current = false;
                return;
            }

            // Wait a frame for layout to settle
            await new Promise(r => requestAnimationFrame(r));
            if (destroyed) return;

            const rect = container.getBoundingClientRect();
            try {
                const appWindow = getCurrentWindow();
                const wv = new Webview(appWindow, label, {
                    url,
                    x: rect.x,
                    y: rect.y,
                    width: Math.max(rect.width, 100),
                    height: Math.max(rect.height, 100),
                });

                if (destroyed) {
                    wv.close().catch(() => {});
                    return;
                }

                webviewRef.current = wv;

                // If not active right now, hide after a brief delay for init
                if (!isActiveRef.current) {
                    setTimeout(() => {
                        if (!destroyed && webviewRef.current) {
                            webviewRef.current.hide().catch(() => {});
                        }
                    }, 150);
                }
            } catch (err) {
                console.error('Failed to create native webview:', err);
            }
            isCreatingRef.current = false;
        }

        create();

        return () => {
            destroyed = true;
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

    return (
        <div
            ref={containerRef}
            className="webview-container"
        />
    );
}
