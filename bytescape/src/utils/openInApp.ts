/**
 * PURPOSE
 * - Centralize external navigation behavior for links and profile opens.
 * - Provide one routing surface for either in-app webview tabs or system browser.
 *
 * HOW IT WORKS
 * - `openInApp()` routes a URL via feature toggle.
 * - `openProfile()` decides whether to open a native profile tab or resolve to
 *   a protocol-specific external URL.
 * - `installLinkInterceptor()` captures eligible anchor clicks and forwards
 *   them through the same route for consistent behavior.
 *
 * RELATED FILES
 * - src/store/navStore* (tab lifecycle: open/switch metadata)
 * - src/store/authStore* ("is this me/local profile" decision path)
 * - src/screens/WebViewScreen* and profile screen routes
 * - src-tauri/src/commands/browser.rs (native/browser-side integration)
 *
 * CROSS-SYSTEM NOTE
 * - This utility is a seam between frontend navigation stores and native
 *   webview/browser capabilities. URL policy changes here affect both tab
 *   routing and platform opener behavior.
 *
 * CAVEAT (TECH-DEBT CONTEXT)
 * - Runtime toggle comments and effective behavior have drifted across sprints
 *   (known toggle-mismatch debt). Keep behavior unchanged in comment-only passes;
 *   resolve via dedicated functional fix when Track A/B coordination is scheduled.
 */
import { openUrl } from '@tauri-apps/plugin-opener';
import { useNavStore, useAuthStore } from '../store';

/**
 * Feature flag for URL routing strategy.
 *
 * `true`  => force system browser via opener plugin.
 * `false` => use in-app webview tab flow.
 */
const USE_SYSTEM_BROWSER = false;

// ═══════════════════════════════════════════════════════════════════════════════
// URL + Profile Routing
// ═══════════════════════════════════════════════════════════════════════════════

/**
 * Open a URL through the configured route (system browser or in-app tab).
 *
 * Cross-system behavior:
 * - System route delegates to native opener plugin.
 * - In-app route creates/switches `webview` tabs through nav store state.
 */
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
 * Open a profile target using protocol-aware routing.
 *
 * - Local/P2P/self profiles open native `profile-view` tabs.
 * - External profiles resolve to URL form and route via `openInApp()`.
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
        // Mastodon author_handle comes in multiple formats:
        //   "user@instance.tld"                    — no display name
        //   "Display Name (@user@instance.tld)"    — with display name
        //   "@user@instance.tld"                   — with leading @
        // Extract user@domain with a regex that handles all variants.
        const acctMatch = handle.match(/@?([\w.-]+)@([\w.-]+\.\w+)/);
        let url: string;
        if (acctMatch) {
            const username = acctMatch[1];
            const domain = acctMatch[2];
            url = `https://${domain}/@${username}`;
        } else {
            // Fallback: if handle looks like a plain username, we can't build a valid URL.
            // Try treating the whole handle as a domain path (unlikely to work, but safe).
            url = `https://${handle.replace(/[^a-zA-Z0-9.@/_-]/g, '')}`;
        }
        openInApp(url, `@${handle}`);
    } else {
        openInApp(`https://${handle}`, `@${handle}`);
    }
}

/**
 * Install a global click interceptor for external links.
 *
 * Captures `<a target="_blank">` clicks and forwards them through
 * `openInApp()` so tab/browser policy is enforced consistently app-wide.
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
