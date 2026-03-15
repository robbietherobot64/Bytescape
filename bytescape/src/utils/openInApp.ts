/**
 * Open a URL in an in-app browser tab.
 *
 * Creates a new tab with screen='webview' that renders the URL in an
 * embedded iframe within the app shell.
 */
import { useNavStore } from '../store';

/**
 * Open a URL in a new in-app browser tab.
 * If a tab with the same URL already exists, switch to it instead.
 */
export function openInApp(url: string, title?: string): void {
    const store = useNavStore.getState();
    // Check if a webview tab with this URL already exists
    const existing = store.tabs.find(
        (t) => t.screen === 'webview' && t.meta?.url === url
    );
    if (existing) {
        store.switchTab(existing.id);
        return;
    }
    // Derive a short title from the URL if none provided
    const displayTitle = title || (() => {
        try {
            const u = new URL(url);
            return u.hostname + (u.pathname !== '/' ? u.pathname.slice(0, 40) : '');
        } catch {
            return url.slice(0, 40);
        }
    })();
    store.openTab('webview', displayTitle, { url });
}

/**
 * Open a Bluesky profile in an in-app browser tab.
 */
export function openBlueskyProfile(handle: string): void {
    if (!handle) return;
    const url = `https://bsky.app/profile/${handle}`;
    openInApp(url, `@${handle}`);
}

/**
 * Install a global click interceptor that catches all `<a target="_blank">`
 * clicks and routes them to in-app browser tabs instead of opening in the
 * system browser.
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

        const title = anchor.title || anchor.textContent?.trim() || undefined;
        openInApp(href, title);
    }

    document.addEventListener('click', handler, true); // capture phase
    return () => document.removeEventListener('click', handler, true);
}
