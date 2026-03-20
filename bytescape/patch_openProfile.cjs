const fs = require('fs');
let content = fs.readFileSync('/home/korendian/Bytescape/bytescape/src/utils/openInApp.ts', 'utf-8');

const target = `export function openProfile(handle: string, protocol: 'bluesky' | 'activitypub' | 'mastodon' | 'local' | 'p2p' = 'bluesky'): void {
    if (!handle) return;
    
    const store = useNavStore.getState();
    const { auth } = useAuthStore.getState();
    
    // Check if it's "me" or explicitly local/p2p
    const isMe = auth && (auth.handle === handle || auth.did === handle || auth.local_username === handle);
    const hasBytescapeProfile = isMe || protocol === 'p2p' || protocol === 'local';`;

const replacement = `export function openProfile(handle: string, protocol?: 'bluesky' | 'activitypub' | 'mastodon' | 'local' | 'p2p'): void {
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
    const hasBytescapeProfile = isMe || derivedProtocol === 'p2p' || derivedProtocol === 'local';`;

content = content.replace(target, replacement);

const target2 = `    if (hasBytescapeProfile) {
        const existing = store.tabs.find(
            (t) => t.screen === 'profile-view' && t.meta?.handle === handle
        );
        if (existing) {
            store.switchTab(existing.id);
            return;
        }
        store.openTab('profile-view', \`@\${handle}\`, { handle, protocol });
        return;
    }

    // Default to in-app browser tab for regular social media profiles
    if (protocol === 'bluesky') {`;

const replacement2 = `    if (hasBytescapeProfile) {
        const existing = store.tabs.find(
            (t) => t.screen === 'profile-view' && t.meta?.handle === handle
        );
        if (existing) {
            store.switchTab(existing.id);
            return;
        }
        store.openTab('profile-view', \`@\${handle}\`, { handle, protocol: derivedProtocol });
        return;
    }

    // Default to in-app browser tab for regular social media profiles
    if (derivedProtocol === 'bluesky') {`;

content = content.replace(target2, replacement2);

const target3 = `    } else if (protocol === 'activitypub' || protocol === 'mastodon') {`;
const replacement3 = `    } else if (derivedProtocol === 'activitypub' || derivedProtocol === 'mastodon') {`;
content = content.replace(target3, replacement3);

fs.writeFileSync('/home/korendian/Bytescape/bytescape/src/utils/openInApp.ts', content);
