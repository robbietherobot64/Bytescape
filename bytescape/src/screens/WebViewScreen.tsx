import { useState, useRef, useEffect } from 'react';
import { BsArrowLeft, BsArrowRight, BsArrowClockwise, BsBoxArrowUpRight } from 'react-icons/bs';

interface WebViewScreenProps {
    url: string;
    title?: string;
}

/**
 * In-app browser tab — renders an external URL inside an iframe.
 * Provides back/forward/reload controls and a URL bar.
 *
 * Note: Many sites block iframe embedding via X-Frame-Options / CSP.
 * For those, we show a fallback with a link to open externally.
 */
export function WebViewScreen({ url, title }: WebViewScreenProps) {
    const iframeRef = useRef<HTMLIFrameElement>(null);
    const [currentUrl, setCurrentUrl] = useState(url);
    const [loadError, setLoadError] = useState(false);
    const [isLoading, setIsLoading] = useState(true);

    useEffect(() => {
        setCurrentUrl(url);
        setLoadError(false);
        setIsLoading(true);
    }, [url]);

    const handleReload = () => {
        setLoadError(false);
        setIsLoading(true);
        if (iframeRef.current) {
            iframeRef.current.src = currentUrl;
        }
    };

    const handleOpenExternal = () => {
        window.open(currentUrl, '_blank');
    };

    return (
        <div className="webview-screen">
            {/* Browser toolbar */}
            <div className="webview-toolbar">
                <button
                    className="webview-nav-btn"
                    onClick={() => iframeRef.current?.contentWindow?.history.back()}
                    title="Back"
                >
                    <BsArrowLeft size={14} />
                </button>
                <button
                    className="webview-nav-btn"
                    onClick={() => iframeRef.current?.contentWindow?.history.forward()}
                    title="Forward"
                >
                    <BsArrowRight size={14} />
                </button>
                <button
                    className="webview-nav-btn"
                    onClick={handleReload}
                    title="Reload"
                >
                    <BsArrowClockwise size={14} />
                </button>
                <div className="webview-url-bar">
                    <span className="webview-url-text">{currentUrl}</span>
                </div>
                <button
                    className="webview-nav-btn"
                    onClick={handleOpenExternal}
                    title="Open in external browser"
                >
                    <BsBoxArrowUpRight size={14} />
                </button>
            </div>

            {/* Content area */}
            {loadError ? (
                <div className="webview-error">
                    <div className="webview-error-box">
                        <div style={{ fontSize: '2rem', marginBottom: 12 }}>🔒</div>
                        <h3>{title || 'Page'}</h3>
                        <p>This site cannot be displayed inline.</p>
                        <a
                            href={currentUrl}
                            onClick={(e) => { e.preventDefault(); handleOpenExternal(); }}
                            className="btn btn-primary"
                            style={{ marginTop: 12, display: 'inline-block', textDecoration: 'none' }}
                        >
                            Open in External Browser →
                        </a>
                    </div>
                </div>
            ) : (
                <iframe
                    ref={iframeRef}
                    src={currentUrl}
                    className="webview-iframe"
                    title={title || currentUrl}
                    sandbox="allow-scripts allow-same-origin allow-popups allow-forms"
                    onLoad={() => setIsLoading(false)}
                    onError={() => { setLoadError(true); setIsLoading(false); }}
                />
            )}
            {isLoading && !loadError && (
                <div className="webview-loading">
                    <div className="dot online"></div> Loading...
                </div>
            )}
        </div>
    );
}
