/**
 * Open URLs in the system browser.
 *
 * Native Tauri multi-webview (in-app browsing) is unreliable on Linux/WebKitGTK,
 * so all external links open in the user's default browser instead.
 */
import { openUrl } from '@tauri-apps/plugin-opener';

/**
 * Open a URL in the system's default browser.
 */
export function openInApp(url: string, _title?: string): void {
    openUrl(url).catch(() => {
        // Fallback: try window.open if plugin fails
        window.open(url, '_blank');
    });
}

/**
 * Open a Bluesky profile in the system browser.
 */
export function openBlueskyProfile(handle: string): void {
    if (!handle) return;
    const url = `https://bsky.app/profile/${handle}`;
    openInApp(url);
}

/**
 * Install a global click interceptor that catches all `<a target="_blank">`
 * clicks and opens them in the system browser instead of navigating
 * the Tauri webview away from the app.
 *
 * Call once at app startup (e.g. in a useEffect in App.tsx).
 * Returns a cleanup function that removes the listener.
 */
export function installLinkInterceptor(): () => void {
    function handler(e: MouseEvent) {
        // Walk up from the click target to find an <a> element
        let el = e.target as HTMLElement | null;
        while (el && el.tagName !== 'A') {
            el = el.parentElement;
        }
        if (!el) return;

        const anchor = el as HTMLAnchorElement;
        const href = anchor.href;

        // Only intercept external links (http/https) with target="_blank"
        if (!href || (!href.startsWith('http://') && !href.startsWith('https://'))) return;
        if (anchor.target !== '_blank') return;

        // Don't intercept if user is holding modifier keys (they want OS default)
        if (e.ctrlKey || e.metaKey || e.shiftKey) return;

        e.preventDefault();
        e.stopPropagation();

        openInApp(href);
    }

    document.addEventListener('click', handler, true); // capture phase
    return () => document.removeEventListener('click', handler, true);
}
