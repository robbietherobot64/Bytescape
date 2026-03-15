import { useState, useRef, useEffect } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';

interface WebViewScreenProps {
    url: string;
    title?: string;
    /** Whether this tab is currently the active/visible tab */
    isActive: boolean;
}

/** Domains known to block iframing via X-Frame-Options / CSP frame-ancestors */
const IFRAME_BLOCKED_DOMAINS = [
    'bsky.app',
    'twitter.com',
    'x.com',
    'facebook.com',
    'instagram.com',
    'linkedin.com',
    'github.com',
    'reddit.com',
    'youtube.com',
    'tiktok.com',
    'threads.net',
    'mastodon.social',
];

function isBlockedDomain(url: string): boolean {
    try {
        const hostname = new URL(url).hostname;
        return IFRAME_BLOCKED_DOMAINS.some(
            d => hostname === d || hostname.endsWith('.' + d)
        );
    } catch {
        return false;
    }
}

/**
 * In-app browser tab — renders an external URL inside an iframe.
 *
 * Sites known to block iframing are detected immediately and show
 * an "Open in browser" prompt. Other sites load in an iframe with
 * a post-load content check.
 *
 * Requires `frame-src https: http:` in the Tauri CSP config.
 */
export function WebViewScreen({ url }: WebViewScreenProps) {
    const blocked = isBlockedDomain(url);
    const [status, setStatus] = useState<'loading' | 'ready' | 'error'>(
        blocked ? 'error' : 'loading'
    );
    const iframeRef = useRef<HTMLIFrameElement>(null);

    // For non-blocked sites, set a timeout — if onLoad hasn't set 'ready'
    // after 10s, show error (likely silently blocked).
    useEffect(() => {
        if (blocked) return;
        const timer = setTimeout(() => {
            setStatus(prev => prev === 'loading' ? 'error' : prev);
        }, 10000);
        return () => clearTimeout(timer);
    }, [url, blocked]);

    const handleLoad = () => {
        if (blocked) return;
        // X-Frame-Options blocks still fire onLoad with a blank page.
        // Try to detect by reading contentWindow.location.
        try {
            const win = iframeRef.current?.contentWindow;
            if (win) {
                try {
                    const loc = win.location.href;
                    if (loc === 'about:blank') {
                        setStatus('error');
                        return;
                    }
                } catch {
                    // Cross-origin error = site actually loaded (good)
                }
            }
        } catch {
            // ignore
        }
        setStatus('ready');
    };

    const handleError = () => {
        setStatus('error');
    };

    const handleOpenExternal = async () => {
        try {
            await openUrl(url);
        } catch {
            window.open(url, '_blank');
        }
    };

    return (
        <div className="webview-container">
            {/* Loading indicator */}
            {status === 'loading' && (
                <div style={{
                    display: 'flex',
                    flexDirection: 'column',
                    alignItems: 'center',
                    justifyContent: 'center',
                    position: 'absolute',
                    inset: 0,
                    color: 'var(--text-secondary)',
                    gap: '12px',
                    zIndex: 1,
                    background: 'var(--bg-primary)',
                } as React.CSSProperties}>
                    <div style={{ fontSize: '16px' } as React.CSSProperties}>Loading…</div>
                </div>
            )}

            {/* Error state — site blocks iframing */}
            {status === 'error' && (
                <div style={{
                    display: 'flex',
                    flexDirection: 'column',
                    alignItems: 'center',
                    justifyContent: 'center',
                    position: 'absolute',
                    inset: 0,
                    color: 'var(--text-secondary)',
                    gap: '16px',
                    padding: '24px',
                    textAlign: 'center',
                    background: 'var(--bg-primary)',
                    zIndex: 1,
                } as React.CSSProperties}>
                    <div style={{ fontSize: '18px', color: 'var(--text-primary)' } as React.CSSProperties}>
                        This site can't be displayed in-app
                    </div>
                    <div style={{
                        fontSize: '12px',
                        color: 'var(--text-muted)',
                        maxWidth: '500px',
                        wordBreak: 'break-word',
                    } as React.CSSProperties}>
                        {url}
                    </div>
                    <div style={{
                        fontSize: '12px',
                        color: 'var(--text-muted)',
                    } as React.CSSProperties}>
                        The site blocks embedding. Click below to open it in your browser.
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
                        Open in browser
                    </button>
                </div>
            )}

            {/* The iframe — only rendered for non-blocked sites */}
            {!blocked && (
                <iframe
                    ref={iframeRef}
                    src={url}
                    onLoad={handleLoad}
                    onError={handleError}
                    sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-popups-to-escape-sandbox"
                    style={{
                        width: '100%',
                        flex: 1,
                        border: 'none',
                        background: '#fff',
                        display: status === 'error' ? 'none' : 'block',
                    } as React.CSSProperties}
                    title={url}
                />
            )}
        </div>
    );
}
