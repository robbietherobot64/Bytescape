/**
 * Tests for B10.1 Internal URL Scheme + B10.2 Navigation History Stack
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { resolveUrl, buildUrl, useLocationStore } from './useLocation';

describe('B10.1 — resolveUrl()', () => {
    it('resolves byte://feed to feed screen with all tab', () => {
        const route = resolveUrl('byte://feed');
        expect(route).not.toBeNull();
        expect(route!.screen).toBe('feed');
        expect(route!.sidebar).toBe('feed');
        expect(route!.feedTab).toBe('all');
    });

    it('resolves byte://feed/following to feed with following tab', () => {
        const route = resolveUrl('byte://feed/following');
        expect(route!.screen).toBe('feed');
        expect(route!.feedTab).toBe('following');
    });

    it('resolves byte://feed/subscribed to feed with all tab (removed tab, fallback)', () => {
        const route = resolveUrl('byte://feed/subscribed');
        expect(route!.feedTab).toBe('all');
    });

    it('resolves byte://feed/you to feed with you tab', () => {
        const route = resolveUrl('byte://feed/you');
        expect(route!.feedTab).toBe('you');
    });

    it('resolves byte://feed/invalid to feed with all tab (fallback)', () => {
        const route = resolveUrl('byte://feed/bogus');
        expect(route!.feedTab).toBe('all');
    });

    it('resolves byte://chat', () => {
        const route = resolveUrl('byte://chat');
        expect(route!.screen).toBe('chat');
        expect(route!.sidebar).toBe('chat');
        expect(route!.convoId).toBeNull();
    });

    it('resolves byte://chat/bsky/{convoId}', () => {
        const route = resolveUrl('byte://chat/bsky/abc123');
        expect(route!.screen).toBe('chat');
        expect(route!.convoId).toBe('abc123');
    });

    it('resolves byte://chat/p2p/{nodeId}', () => {
        const route = resolveUrl('byte://chat/p2p/node123');
        expect(route!.convoId).toBe('p2p:node123');
    });

    it('resolves byte://vault', () => {
        const route = resolveUrl('byte://vault');
        expect(route!.screen).toBe('vault');
        expect(route!.sidebar).toBe('vault');
    });

    it('resolves byte://profile', () => {
        const route = resolveUrl('byte://profile');
        expect(route!.screen).toBe('profile');
        expect(route!.sidebar).toBe('profile');
    });

    it('resolves byte://profile-builder', () => {
        const route = resolveUrl('byte://profile-builder');
        expect(route!.screen).toBe('profile-builder');
        expect(route!.sidebar).toBe('profile');
    });

    it('resolves byte://profile-browser', () => {
        const route = resolveUrl('byte://profile-browser');
        expect(route!.screen).toBe('profile-browser');
        expect(route!.sidebar).toBe('social');
    });

    it('resolves byte://peer/{nodeId}', () => {
        const route = resolveUrl('byte://peer/abc123def456');
        expect(route!.screen).toBe('profile-browser');
        expect(route!.peerNodeId).toBe('abc123def456');
    });

    it('resolves byte://peer/{nodeId}/profile', () => {
        const route = resolveUrl('byte://peer/abc123/profile');
        expect(route!.screen).toBe('profile-browser');
        expect(route!.peerNodeId).toBe('abc123');
    });

    it('resolves byte://import', () => {
        const route = resolveUrl('byte://import');
        expect(route!.screen).toBe('import');
        expect(route!.sidebar).toBe('import');
    });

    it('resolves byte://settings', () => {
        const route = resolveUrl('byte://settings');
        expect(route!.screen).toBe('settings');
    });

    it('resolves byte://friends', () => {
        const route = resolveUrl('byte://friends');
        expect(route!.screen).toBe('friends');
        expect(route!.sidebar).toBe('social');
    });

    it('resolves byte://social as alias for friends', () => {
        const route = resolveUrl('byte://social');
        expect(route!.screen).toBe('friends');
    });

    it('returns null for unknown URLs', () => {
        const route = resolveUrl('byte://nonexistent');
        expect(route).toBeNull();
    });

    it('handles path-only format (no scheme)', () => {
        const route = resolveUrl('/feed/following');
        expect(route!.screen).toBe('feed');
        expect(route!.feedTab).toBe('following');
    });

    it('strips trailing slashes', () => {
        const route = resolveUrl('byte://vault/');
        expect(route!.screen).toBe('vault');
    });
});

describe('B10.1 — buildUrl()', () => {
    it('builds feed URL with default tab', () => {
        expect(buildUrl('feed')).toBe('byte://feed');
    });

    it('builds feed URL with specific tab', () => {
        expect(buildUrl('feed', { feedTab: 'following' })).toBe('byte://feed/following');
    });

    it('builds chat URL without convo', () => {
        expect(buildUrl('chat')).toBe('byte://chat');
    });

    it('builds chat URL with Bluesky convo', () => {
        expect(buildUrl('chat', { convoId: 'abc123' })).toBe('byte://chat/bsky/abc123');
    });

    it('builds chat URL with P2P convo', () => {
        expect(buildUrl('chat', { convoId: 'p2p:node456' })).toBe('byte://chat/p2p/node456');
    });

    it('builds vault URL', () => {
        expect(buildUrl('vault')).toBe('byte://vault');
    });

    it('builds profile URL', () => {
        expect(buildUrl('profile')).toBe('byte://profile');
    });

    it('builds profile-builder URL', () => {
        expect(buildUrl('profile-builder')).toBe('byte://profile-builder');
    });

    it('builds profile-browser URL', () => {
        expect(buildUrl('profile-browser')).toBe('byte://profile-browser');
    });

    it('builds peer URL with node ID', () => {
        expect(buildUrl('profile-browser', { peerNodeId: 'abc' })).toBe('byte://peer/abc');
    });

    it('builds peer-profile URL with node ID', () => {
        expect(buildUrl('peer-profile', { peerNodeId: 'abc' })).toBe('byte://peer/abc');
    });

    it('builds import URL', () => {
        expect(buildUrl('import')).toBe('byte://import');
    });

    it('builds settings URL', () => {
        expect(buildUrl('settings')).toBe('byte://settings');
    });

    it('builds friends URL', () => {
        expect(buildUrl('friends')).toBe('byte://friends');
    });
});

describe('B10.2 — Navigation History Stack', () => {
    beforeEach(() => {
        // Reset the store to initial state
        useLocationStore.setState({
            currentUrl: 'byte://login',
            history: ['byte://login'],
            cursor: 0,
            canGoBack: false,
            canGoForward: false,
        });
    });

    it('navigate pushes to history', () => {
        const { navigate } = useLocationStore.getState();
        navigate('byte://feed');

        const state = useLocationStore.getState();
        expect(state.currentUrl).toBe('byte://feed');
        expect(state.history).toEqual(['byte://login', 'byte://feed']);
        expect(state.cursor).toBe(1);
        expect(state.canGoBack).toBe(true);
        expect(state.canGoForward).toBe(false);
    });

    it('multiple navigations build up history', () => {
        const { navigate } = useLocationStore.getState();
        navigate('byte://feed');
        navigate('byte://chat');
        navigate('byte://vault');

        const state = useLocationStore.getState();
        expect(state.history).toEqual(['byte://login', 'byte://feed', 'byte://chat', 'byte://vault']);
        expect(state.cursor).toBe(3);
        expect(state.canGoBack).toBe(true);
    });

    it('goBack navigates to previous URL', () => {
        const store = useLocationStore.getState();
        store.navigate('byte://feed');
        store.navigate('byte://chat');

        useLocationStore.getState().goBack();

        const state = useLocationStore.getState();
        expect(state.currentUrl).toBe('byte://feed');
        expect(state.cursor).toBe(1);
        expect(state.canGoBack).toBe(true);
        expect(state.canGoForward).toBe(true);
    });

    it('goForward navigates to next URL', () => {
        const store = useLocationStore.getState();
        store.navigate('byte://feed');
        store.navigate('byte://chat');
        useLocationStore.getState().goBack();

        useLocationStore.getState().goForward();

        const state = useLocationStore.getState();
        expect(state.currentUrl).toBe('byte://chat');
        expect(state.canGoForward).toBe(false);
    });

    it('navigating after goBack trims forward history', () => {
        const store = useLocationStore.getState();
        store.navigate('byte://feed');
        store.navigate('byte://chat');
        store.navigate('byte://vault');

        // Go back twice
        useLocationStore.getState().goBack();
        useLocationStore.getState().goBack();

        // Navigate to a new place — should trim chat and vault
        useLocationStore.getState().navigate('byte://settings');

        const state = useLocationStore.getState();
        expect(state.history).toEqual(['byte://login', 'byte://feed', 'byte://settings']);
        expect(state.cursor).toBe(2);
        expect(state.canGoForward).toBe(false);
    });

    it('cannot goBack when at start', () => {
        const state = useLocationStore.getState();
        expect(state.canGoBack).toBe(false);

        state.goBack(); // Should be a no-op
        expect(useLocationStore.getState().currentUrl).toBe('byte://login');
    });

    it('cannot goForward when at end', () => {
        const store = useLocationStore.getState();
        store.navigate('byte://feed');

        const state = useLocationStore.getState();
        expect(state.canGoForward).toBe(false);

        state.goForward(); // Should be a no-op
        expect(useLocationStore.getState().currentUrl).toBe('byte://feed');
    });

    it('replace option replaces current entry', () => {
        const store = useLocationStore.getState();
        store.navigate('byte://feed');
        store.navigate('byte://feed/following', { replace: true });

        const state = useLocationStore.getState();
        expect(state.history).toEqual(['byte://login', 'byte://feed/following']);
        expect(state.cursor).toBe(1);
    });

    it('enforces max history depth', () => {
        const store = useLocationStore.getState();
        // Navigate 55 times (beyond max of 50)
        for (let i = 0; i < 55; i++) {
            store.navigate(`byte://feed/${i % 3 === 0 ? 'all' : i % 3 === 1 ? 'following' : 'you'}`);
        }

        const state = useLocationStore.getState();
        expect(state.history.length).toBeLessThanOrEqual(50);
    });

    it('normalizes URLs without scheme', () => {
        const store = useLocationStore.getState();
        store.navigate('/feed/following');

        const state = useLocationStore.getState();
        // The navigate function prepends byte:// — path leading slash is handled by resolveUrl
        expect(state.currentUrl).toBe('byte://feed/following');
    });
});
