const fs = require('fs');
let content = fs.readFileSync('/home/korendian/Bytescape/bytescape/src/screens/SettingsScreen.tsx', 'utf-8');

const target = `    const [loadingSync, setLoadingSync] = useState(false);
    const [createdTicket, setCreatedTicket] = useState<string | null>(null);
    const [joinTicket, setJoinTicket] = useState("");
    const [syncStatus, setSyncStatus] = useState("");

    async function createSyncDoc() {`;

const replacement = `    const [loadingSync, setLoadingSync] = useState(false);
    const [createdTicket, setCreatedTicket] = useState<string | null>(null);
    const [joinTicket, setJoinTicket] = useState("");
    const [syncStatus, setSyncStatus] = useState("");

    const [linkedAccounts, setLinkedAccounts] = useState<LinkedAccount[]>([]);
    const [showLinkMastodon, setShowLinkMastodon] = useState(false);
    const [mastodonInstance, setMastodonInstance] = useState("");
    const [mastodonAuthCode, setMastodonAuthCode] = useState("");
    const [mastodonStep, setMastodonStep] = useState<"instance" | "code">("instance");

    useEffect(() => {
        loadLinkedAccounts();
    }, [auth]);

    async function loadLinkedAccounts() {
        try {
            const accounts = await invoke<LinkedAccount[]>('list_linked_accounts');
            setLinkedAccounts(accounts);
        } catch (e) {
            console.warn('Failed to load linked accounts, using fallback', e);
            // Fallback if backend command not ready
            const fallback: LinkedAccount[] = [];
            if (auth?.source === 'bluesky') {
                fallback.push({
                    protocol: 'bluesky',
                    identifier: auth.did,
                    display_handle: '@' + auth.handle,
                    connected_at: Date.now() / 1000,
                    active: true,
                });
            }
            try {
                const mastodonSession = await invoke<any>('mastodon_get_session');
                if (mastodonSession) {
                    fallback.push({
                        protocol: 'mastodon',
                        identifier: \`acct:\${mastodonSession.username}@\${mastodonSession.instance}\`,
                        display_handle: \`@\${mastodonSession.username}@\${mastodonSession.instance}\`,
                        connected_at: Date.now() / 1000,
                        active: true,
                    });
                }
            } catch (e2) {}
            setLinkedAccounts(fallback);
        }
    }

    async function handleDisconnect(account: LinkedAccount) {
        try {
            try {
                await invoke('unlink_account', { protocol: account.protocol, identifier: account.identifier });
            } catch (e) {
                // Fallbacks if unified command missing
                if (account.protocol === 'mastodon') {
                    await invoke('mastodon_disconnect');
                } else if (account.protocol === 'bluesky') {
                    await invoke('logout_bluesky');
                }
            }
            await loadLinkedAccounts();
        } catch (e: any) {
            alert('Failed to disconnect: ' + e);
        }
    }

    async function startMastodonAuth() {
        if (!mastodonInstance) return;
        setLinkLoading(true);
        setLinkError("");
        try {
            const url = await invoke<string>('mastodon_register_app', { instance: mastodonInstance.trim() });
            const { openInApp } = await import('../utils/openInApp');
            openInApp(url, 'Mastodon Auth');
            setMastodonStep("code");
        } catch (e: any) {
            setLinkError("Failed to register app: " + e);
        } finally {
            setLinkLoading(false);
        }
    }

    async function finalizeMastodonAuth() {
        if (!mastodonAuthCode) return;
        setLinkLoading(true);
        setLinkError("");
        try {
            await invoke('mastodon_complete_auth', { instance: mastodonInstance.trim(), authCode: mastodonAuthCode.trim() });
            setLinkSuccess("Mastodon account connected!");
            setShowLinkMastodon(false);
            setMastodonStep("instance");
            setMastodonInstance("");
            setMastodonAuthCode("");
            await loadLinkedAccounts();
        } catch (e: any) {
            setLinkError("Failed to complete auth: " + e);
        } finally {
            setLinkLoading(false);
        }
    }

    async function createSyncDoc() {`;

content = content.replace(target, replacement);
fs.writeFileSync('/home/korendian/Bytescape/bytescape/src/screens/SettingsScreen.tsx', content);
