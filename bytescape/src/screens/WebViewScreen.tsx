import { useState, useRef } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';

interface WebViewScreenProps {
    url: string;
    title?: string;
    /** Whether this tab is currently the active/visible tab */
    isActive: boolean;
}

/**
 * In-app browser tab — renders an external URL inside an iframe.
 *
 * Sites that block iframing via X-Frame-Options/CSP will show an error state
 * with a button to open in the system browser as a fallback.
 *
 * Requires `frame-src https: http:` in the Tauri CSP config.
 */
export function WebViewScreen({ url }: WebViewScreenProps) {
    const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
    const iframeRef = useRef<HTMLIFrameElement>(null);

    const handleLoad = () => {
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
            {/* Address bar */}
            <div style={{
                display: 'flex',
                alignItems: 'center',
                gap: '8px',
                padding: '6px 12px',
                background: 'var(--bg-secondary)',
                borderBottom: '1px solid var(--border-color)',
                fontSize: '12px',
                flexShrink: 0,
            } as React.CSSProperties}>
                <span style={{ color: 'var(--text-muted)', whiteSpace: 'nowrap' } as React.CSSProperties}>
                    🌐
                </span>
                <span style={{
                    color: 'var(--text-secondary)',
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                    flex: 1,
                    fontFamily: 'monospace',
                } as React.CSSProperties}>
                    {url}
                </span>
                <button
                    onClick={handleOpenExternal}
                    style={{
                        background: 'var(--bg-tertiary)',
                        border: '1px solid var(--border-color)',
                        borderRadius: 'var(--border-radius)',
                        color: 'var(--text-secondary)',
                        cursor: 'pointer',
                        fontSize: '11px',
                        padding: '2px 8px',
                        whiteSpace: 'nowrap',
                    } as React.CSSProperties}
                    title="Open in system browser"
                >
                    ↗ External
                </button>
            </div>

            {/* Loading indicator */}
            {status === 'loading' && (
                <div style={{
                    display: 'flex',
                    flexDirection: 'column',
                    alignItems: 'center',
                    justifyContent: 'center',
                    position: 'absolute',
                    inset: 0,
                    top: '36px',
                    color: 'var(--text-secondary)',
                    gap: '12px',
                    zIndex: 1,
                    background: 'var(--bg-primary)',
                } as React.CSSProperties}>
                    <div style={{ fontSize: '16px' } as React.CSSProperties}>Loading…</div>
                    <div style={{ fontSize: '12px', color: 'var(--text-muted)' } as React.CSSProperties}>
                        {url}
                    </div>
                </div>
            )}

            {/* Error state */}
            {status === 'error' && (
                <div style={{
                    display: 'flex',
                    flexDirection: 'column',
                    alignItems: 'center',
                    justifyContent: 'center',
                    position: 'absolute',
                    inset: 0,
                    top: '36px',
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
                        The site blocks embedding. You can open it in your browser instead.
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

            {/* The iframe */}
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
        </div>
    );
}
