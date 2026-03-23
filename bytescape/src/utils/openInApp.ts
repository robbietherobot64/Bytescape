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
import { useNavStore, useAuthStore } from '../store';

/**
 * Open a URL. Routes to the system browser because the in-app WebView
 * (Tauri add_child / WebKitGTK) has unresolved rendering/visibility bugs
 * on Linux. This has been broken across 3+ sprints — system browser is
 * the reliable path until a native profile screen replaces raw webviews.
 */
const USE_SYSTEM_BROWSER = false;

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
    // Generate a stable label for the webview so WebViewScreen can
    // store it in meta and the title listener can find this tab.
    store.openTab('webview', displayTitle, { url });
}

/**
 * Open a profile in the native ProfileViewScreen tab.
 */
export function openProfile(handle: string, protocol?: 'bluesky' | 'activitypub' | 'mastodon' | 'local' | 'p2p'): void {
    if (!handle) return;

    let derivedProtocol = protocol;
    if (!derivedProtocol) {
        if (handle.includes('@')) {
            derivedProtocol = 'activitypub';
        } else {
            derivedProtocol = 'bluesky';
        }
    }
    
    const store = useNavStore.getState();
    const { auth } = useAuthStore.getState();
    
    // Check if it's "me" or explicitly local/p2p
    const isMe = auth && (auth.handle === handle || auth.did === handle || auth.local_username === handle);
    const hasBytescapeProfile = isMe || derivedProtocol === 'p2p' || derivedProtocol === 'local';

    if (hasBytescapeProfile) {
        const existing = store.tabs.find(
            (t) => t.screen === 'profile-view' && t.meta?.handle === handle
        );
        if (existing) {
            store.switchTab(existing.id);
            return;
        }
        store.openTab('profile-view', `@${handle}`, { handle, protocol: derivedProtocol });
        return;
    }

    // Default to in-app browser tab for regular social media profiles
    if (derivedProtocol === 'bluesky') {
        const url = `https://bsky.app/profile/${handle}`;
        openInApp(url, `@${handle}`);
    } else if (derivedProtocol === 'activitypub' || derivedProtocol === 'mastodon') {
        // basic parser: @username@instance.social -> https://instance.social/@username
        let url = `https://${handle}`;
        if (handle.includes('@')) {
            const parts = handle.split('@').filter(Boolean); // remove leading @
            if (parts.length >= 2) {
                const domain = parts[1];
                const username = parts[0];
                url = `https://${domain}/@${username}`;
            }
        }
        openInApp(url, `@${handle}`);
    } else {
        openInApp(`https://${handle}`, `@${handle}`);
    }
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
