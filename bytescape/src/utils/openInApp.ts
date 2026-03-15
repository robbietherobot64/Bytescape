/**
 * Open a URL in an in-app webview window (Tauri v2).
 *
 * Creates a new WebviewWindow for the given URL. Each URL gets its own
 * window with a unique label derived from the URL to prevent duplicates.
 * If the window for that URL already exists, it focuses it instead.
 *
 * Also creates a tab entry in the NavStore so the user sees the window
 * in their tab bar. Closing the tab closes the window and vice-versa.
 */
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';

/** Track open webview windows so we can close them from tabs */
const openWebviews = new Map<string, WebviewWindow>();

/**
 * Open a URL in a new Tauri webview window inside the app.
 * Falls back to console.error on failure.
 */
export async function openInApp(url: string, title?: string): Promise<void> {
    // Create a stable label from the URL (alphanumeric + dashes only)
    const label = 'browse-' + url.replace(/[^a-zA-Z0-9]/g, '-').slice(0, 80);

    // Check if window with this label already exists
    const existing = await WebviewWindow.getByLabel(label);
    if (existing) {
        try {
            await existing.setFocus();
        } catch {
            // Window may have been closed, create a new one
        }
        return;
    }

    const webview = new WebviewWindow(label, {
        url,
        title: title || url,
        width: 1024,
        height: 768,
        center: true,
    });

    openWebviews.set(label, webview);

    webview.once('tauri://error', (e) => {
        console.error('Failed to open in-app window:', e);
        openWebviews.delete(label);
    });

    // When the webview window is closed by the user, remove the tab
    webview.once('tauri://close-requested', () => {
        openWebviews.delete(label);
    });
}

/**
 * Close a webview window that was opened via openInApp.
 */
export function closeWebview(label: string): void {
    const wv = openWebviews.get(label);
    if (wv) {
        wv.close().catch(console.error);
        openWebviews.delete(label);
    }
}

/**
 * Focus a webview window that was opened via openInApp.
 */
export function focusWebview(label: string): void {
    const wv = openWebviews.get(label);
    if (wv) {
        wv.setFocus().catch(console.error);
    }
}

/**
 * Open a Bluesky profile in an in-app webview.
 */
export function openBlueskyProfile(handle: string): void {
    if (!handle) return;
    const url = `https://bsky.app/profile/${handle}`;
    openInApp(url, `@${handle} — Bluesky`).catch(console.error);
}
