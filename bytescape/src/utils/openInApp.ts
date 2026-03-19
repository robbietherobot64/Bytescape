/**
 * Open a URL — either in an in-app tab or in the system browser.
 *
 * The native WebView overlay (commands/browser.rs) is currently broken due
 * to a window-label mismatch in the Rust backend (Track A). Until that's
 * fixed, we route:
 *   • Bluesky profile URLs  → system browser (instant, reliable)
 *   • All other http(s) URLs → system browser via @tauri-apps/plugin-opener
 *
 * The in-app webview path is preserved for future re-enablement.
 */
import { openUrl } from '@tauri-apps/plugin-opener';
import { useNavStore } from '../store';

/**
 * Open a URL. Currently routes everything to the system browser because
 * the in-app WebView is broken. When Track A fixes browser.rs, flip the
 * `USE_SYSTEM_BROWSER` flag to false to re-enable in-app tabs.
 */
const USE_SYSTEM_BROWSER = true;

export function openInApp(url: string, title?: string): void {
    if (USE_SYSTEM_BROWSER) {
        openUrl(url).catch((err) =>
            console.error('[openInApp] Failed to open URL in system browser:', err)
        );
        return;
    }

    // --- In-app webview path (currently disabled) ---
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
 * Open a Bluesky profile. Opens in system browser for now.
 * Future: render in a native BlueskyProfileScreen tab.
 */
export function openBlueskyProfile(handle: string): void {
    if (!handle) return;
    const url = `https://bsky.app/profile/${handle}`;
    openInApp(url, `@${handle}`);
}

/**
 * Install a global click interceptor that catches all `<a target="_blank">`
 * clicks and routes them through openInApp (currently → system browser).
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
