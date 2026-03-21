import { useState, useEffect } from 'react';
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { AuthStatus, LinkedAccount } from '../types';
import { ThemePicker } from '../components/ThemePicker';

interface SettingsScreenProps {
    auth: AuthStatus | null;
    archiveCount: number;
    onLogout: () => void;
    onClearVault?: () => void;
    onAuthUpdated?: (auth: AuthStatus) => void;
}

export function SettingsScreen({ auth, archiveCount, onLogout, onAuthUpdated }: SettingsScreenProps) {
    const [error, setError] = useState("");
    const [status, setStatus] = useState("");

    // Export state
    const [isExporting, setIsExporting] = useState(false);
    const [exportStatus, setExportStatus] = useState<string | null>(null);

    // Link account state (shared)
    const [showLinkBluesky, setShowLinkBluesky] = useState(false);
    const [linkIdentifier, setLinkIdentifier] = useState("");
    const [linkLoading, setLinkLoading] = useState(false);
    const [linkError, setLinkError] = useState("");

    // Bluesky OAuth pending state
    const [blueskyOauthPending, setBlueskyOauthPending] = useState(false);

    // Mastodon OAuth pending state
    const [showLinkMastodon, setShowLinkMastodon] = useState(false);
    const [mastodonInstance, setMastodonInstance] = useState("");
    const [mastodonOauthPending, setMastodonOauthPending] = useState(false);

    // Sync state
    const [loadingSync, setLoadingSync] = useState(false);
    const [createdTicket, setCreatedTicket] = useState<string | null>(null);
    const [joinTicket, setJoinTicket] = useState("");
    const [syncStatus, setSyncStatus] = useState("");

    const [linkedAccounts, setLinkedAccounts] = useState<LinkedAccount[]>([]);

    useEffect(() => {
        loadLinkedAccounts();
    }, [auth]);

    // ── Bluesky OAuth callback listener ──
    useEffect(() => {
        if (!blueskyOauthPending) return;
        let unlistenComplete: UnlistenFn | undefined;
        let unlistenError: UnlistenFn | undefined;
        const timeoutId = setTimeout(() => {
            setBlueskyOauthPending(false);
            setLinkLoading(false);
            setLinkError('Bluesky OAuth timed out. Please try again.');
            invoke('cancel_bluesky_oauth').catch(() => {});
        }, 5 * 60 * 1000);

        listen<AuthStatus>('oauth-complete', (_event) => {
            setBlueskyOauthPending(false);
            setLinkLoading(false);
            setShowLinkBluesky(false);
            setLinkIdentifier('');
            clearTimeout(timeoutId);
            loadLinkedAccounts();
            if (onAuthUpdated) {
                invoke<AuthStatus | null>('check_auth_status').then(status => {
                    if (status) onAuthUpdated(status);
                }).catch(() => {});
            }
        }).then(fn => { unlistenComplete = fn; });

        listen<string>('oauth-error', (event) => {
            setBlueskyOauthPending(false);
            setLinkLoading(false);
            clearTimeout(timeoutId);
            setLinkError(typeof event.payload === 'string' ? event.payload : 'Bluesky OAuth failed');
        }).then(fn => { unlistenError = fn; });

        return () => {
            clearTimeout(timeoutId);
            unlistenComplete?.();
            unlistenError?.();
        };
    }, [blueskyOauthPending, onAuthUpdated]);

    // ── Mastodon OAuth callback listener ──
    useEffect(() => {
        if (!mastodonOauthPending) return;
        let unlistenComplete: UnlistenFn | undefined;
        let unlistenError: UnlistenFn | undefined;
        const timeoutId = setTimeout(() => {
            setMastodonOauthPending(false);
            setLinkLoading(false);
            setLinkError('Mastodon OAuth timed out. Please try again.');
        }, 5 * 60 * 1000);

        listen<AuthStatus>('mastodon-auth-complete', (_event) => {
            setMastodonOauthPending(false);
            setLinkLoading(false);
            setShowLinkMastodon(false);
            setMastodonInstance('');
            clearTimeout(timeoutId);
            loadLinkedAccounts();
            if (onAuthUpdated) {
                invoke<AuthStatus | null>('check_auth_status').then(status => {
                    if (status) onAuthUpdated(status);
                }).catch(() => {});
            }
        }).then(fn => { unlistenComplete = fn; });

        listen<string>('mastodon-auth-error', (event) => {
            setMastodonOauthPending(false);
            setLinkLoading(false);
            clearTimeout(timeoutId);
            setLinkError(typeof event.payload === 'string' ? event.payload : 'Mastodon OAuth failed');
        }).then(fn => { unlistenError = fn; });

        return () => {
            clearTimeout(timeoutId);
            unlistenComplete?.();
            unlistenError?.();
        };
    }, [mastodonOauthPending, onAuthUpdated]);

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
                        identifier: `acct:${mastodonSession.username}@${mastodonSession.instance}`,
                        display_handle: `@${mastodonSession.username}@${mastodonSession.instance}`,
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
            // Refresh auth status so the feed/friends views pick up the change
            if (onAuthUpdated) {
                try {
                    const status = await invoke<AuthStatus | null>('check_auth_status');
                    if (status) onAuthUpdated(status);
                } catch {}
            }
        } catch (e: any) {
            alert('Failed to disconnect: ' + e);
        }
    }

    async function startMastodonAuth() {
        if (!mastodonInstance) return;
        setLinkLoading(true);
        setLinkError("");
        try {
            let instance = mastodonInstance.trim();
            if (instance.startsWith('http://')) instance = instance.slice(7);
            if (instance.startsWith('https://')) instance = instance.slice(8);
            instance = instance.split('/')[0];
            setMastodonInstance(instance);

            const url = await invoke<string>('mastodon_register_app', { instance });
            setMastodonOauthPending(true);
            const { openInApp } = await import('../utils/openInApp');
            openInApp(url, 'Mastodon Auth');
        } catch (e: any) {
            setLinkLoading(false);
            setLinkError("Failed to start Mastodon auth: " + e);
        }
    }

    async function createSyncDoc() {
        setLoadingSync(true);
        setSyncStatus("");
        try {
            const result = await invoke<{ ticket: string, namespace_id: string }>("create_sync_doc");
            setCreatedTicket(result.ticket);
            setSyncStatus(`Created new group ID: ${result.namespace_id.slice(0, 8)}...`);
        } catch (e: any) {
            console.error("Failed to create sync doc:", e);
            setSyncStatus("Error creating sync group: " + (typeof e === "string" ? e : JSON.stringify(e)));
        } finally {
            setLoadingSync(false);
        }
    }

    async function joinSyncDoc() {
        if (!joinTicket) return;
        setLoadingSync(true);
        setSyncStatus("");
        try {
            const namespaceId = await invoke<string>("join_sync_doc", { ticketStr: joinTicket });
            setSyncStatus(`Successfully joined group ID: ${namespaceId.slice(0, 8)}...`);
            setJoinTicket(""); // Clear input
        } catch (e: any) {
            console.error("Failed to join sync doc:", e);
            setSyncStatus("Error joining sync group: " + (typeof e === "string" ? e : JSON.stringify(e)));
        } finally {
            setLoadingSync(false);
        }
    }

    function copyToClipboard(text: string | null) {
        if (text) {
            navigator.clipboard.writeText(text);
            setSyncStatus("Ticket copied to clipboard!");
            setTimeout(() => setSyncStatus(""), 3000);
        }
    }

    async function handleClearVault() {
        if (confirm("⚠️ DELETE ACCOUNT — This will permanently erase ALL your data, linked accounts, identity, and imported content. This cannot be undone. Continue?")) {
            try {
                await invoke("delete_account");
                setStatus("Account deleted. All data wiped.");
                onLogout();
            } catch (e: any) {
                setError(typeof e === "string" ? e : "Failed to delete account");
            }
        }
    }

    async function handleExport() {
        setExportStatus(null);
        try {
            const includeMedia = confirm('Include media files in export? (This will make the file larger but includes all images/videos.)');
            const filePath = await save({
                title: 'Export Vault Data',
                defaultPath: `bytescape-export-${new Date().toISOString().slice(0, 10)}.zip`,
                filters: [{ name: 'ZIP Archive', extensions: ['zip'] }],
            });
            if (!filePath) return; // user cancelled

            setIsExporting(true);
            setExportStatus('Exporting…');

            const result = await invoke<{
                path: string;
                item_count: number;
                contact_count: number;
                message_count: number;
                media_count: number;
                size_bytes: number;
            }>('export_vault_data', {
                outputPath: filePath,
                includeMedia: includeMedia,
            });

            const sizeMb = (result.size_bytes / (1024 * 1024)).toFixed(1);
            setExportStatus(
                `✅ Exported ${result.item_count} items, ${result.contact_count} contacts, ${result.message_count} messages` +
                (includeMedia ? `, ${result.media_count} media files` : '') +
                ` (${sizeMb} MB)`
            );
        } catch (e: any) {
            console.error('Export failed:', e);
            setExportStatus(`❌ Export failed: ${typeof e === 'string' ? e : 'Unknown error'}`);
        } finally {
            setIsExporting(false);
        }
    }

    async function handleLinkBluesky() {
        if (!linkIdentifier.trim()) {
            setLinkError("Enter your Bluesky handle (e.g. user.bsky.social)");
            return;
        }
        setLinkLoading(true);
        setLinkError("");
        try {
            const authUrl = await invoke<string>('start_bluesky_oauth', {
                handle: linkIdentifier.trim(),
            });
            setBlueskyOauthPending(true);
            const { openInApp } = await import('../utils/openInApp');
            openInApp(authUrl, 'Bluesky Auth');
        } catch (e: any) {
            setLinkLoading(false);
            setLinkError(typeof e === 'string' ? e : e.message || 'Failed to start Bluesky OAuth');
        }
    }

    return (
        <div className="screen-centered" style={{ paddingBottom: 40 } as React.CSSProperties}>
            <div className="main-header">
                <h1>⚙️ Settings</h1>
            </div>

            {/* Identity (merged Account + Connected Accounts) */}
            <div className="card">
                <h3>🪪 Identity</h3>
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16 }}>
                    <div>
                        <p className="text-muted text-sm" style={{ margin: 0 }}>
                            Signed in as <strong>{auth?.local_username || auth?.handle}</strong>
                        </p>
                        <p className="text-xs text-muted" style={{ opacity: 0.7 }}>
                            Master Identity: {(auth?.web_did || auth?.did || '').substring(0, 40)}{(auth?.web_did || auth?.did || '').length > 40 ? '...' : ''}
                            <button
                                onClick={() => {
                                    navigator.clipboard.writeText(auth?.web_did || auth?.did || '');
                                    alert('DID copied to clipboard');
                                }}
                                style={{ background: 'none', border: 'none', color: 'var(--accent-purple)', cursor: 'pointer', marginLeft: 8 }}
                            >
                                Copy
                            </button>
                        </p>
                    </div>
                    <button className="btn btn-danger" onClick={onLogout}>
                        Sign Out
                    </button>
                </div>

                {/* Linked Accounts */}
                <div style={{ marginTop: 12 }}>
                    <div style={{ fontSize: '0.85rem', fontWeight: 600, marginBottom: 8, color: 'var(--text-secondary)' }}>
                        Linked Accounts
                    </div>
                    
                    {linkedAccounts.map(account => (
                        <div key={account.identifier} style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '10px 14px', background: 'var(--bg-panel)', border: 'var(--border-default)', marginBottom: 8 }}>
                            <span style={{ fontSize: '1.2rem' }}>{account.protocol === 'bluesky' ? '🦋' : '🐘'}</span>
                            <div style={{ flex: 1 }}>
                                <strong style={{ fontSize: '0.9rem' }}>{account.display_handle}</strong>
                                <p className="text-xs text-muted" style={{ margin: 0 }}>
                                    {account.protocol.charAt(0).toUpperCase() + account.protocol.slice(1)} · {account.identifier.startsWith('did:plc:') ? account.identifier.substring(0, 20) + '...' : 'Connected'}
                                </p>
                            </div>
                            <span style={{ color: 'var(--accent-green)', fontSize: '0.82rem', fontWeight: 600 }}>✓ Linked</span>
                            <button
                                onClick={() => handleDisconnect(account)}
                                style={{
                                    padding: '4px 10px',
                                    fontSize: '0.75rem',
                                    background: 'transparent',
                                    border: '1px solid var(--accent-red, #ff5555)',
                                    color: 'var(--accent-red, #ff5555)',
                                    cursor: 'pointer',
                                    fontFamily: 'inherit',
                                    fontWeight: 500,
                                }}
                            >
                                Disconnect
                            </button>
                        </div>
                    ))}

                    {/* Green Plus — Connect Account */}
                    {!showLinkBluesky && !showLinkMastodon ? (
                        <div style={{ display: 'flex', gap: 10, flexDirection: 'column' }}>
                            <div style={{ display: 'flex', gap: 10 }}>
                                <button
                                    onClick={() => setShowLinkBluesky(true)}
                                    style={{
                                        flex: 1,
                                        display: 'flex',
                                        alignItems: 'center',
                                        justifyContent: 'center',
                                        gap: 8,
                                        padding: '12px',
                                        background: 'rgba(34, 197, 94, 0.1)',
                                        border: '2px dashed #22c55e',
                                        cursor: 'pointer',
                                        fontFamily: 'inherit',
                                        fontSize: '0.9rem',
                                        fontWeight: 600,
                                        color: '#22c55e',
                                        transition: 'background 0.15s',
                                    }}
                                >
                                    ＋ Connect Bluesky
                                </button>
                                <button
                                    onClick={() => setShowLinkMastodon(true)}
                                    style={{
                                        flex: 1,
                                        display: 'flex',
                                        alignItems: 'center',
                                        justifyContent: 'center',
                                        gap: 8,
                                        padding: '12px',
                                        background: 'rgba(34, 197, 94, 0.1)',
                                        border: '2px dashed #22c55e',
                                        cursor: 'pointer',
                                        fontFamily: 'inherit',
                                        fontSize: '0.9rem',
                                        fontWeight: 600,
                                        color: '#22c55e',
                                        transition: 'background 0.15s',
                                    }}
                                >
                                    ＋ Connect Mastodon
                                </button>
                            </div>
                        </div>
                    ) : showLinkBluesky ? (
                        <div style={{ display: 'flex', flexDirection: 'column', gap: 10, padding: 14, background: 'var(--bg-panel)', border: 'var(--border-default)' }}>
                            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                                <strong style={{ fontSize: '0.9rem' }}>🦋 Link Bluesky</strong>
                                <button className="btn btn-ghost" onClick={() => { setShowLinkBluesky(false); setBlueskyOauthPending(false); setLinkLoading(false); setLinkError(''); invoke('cancel_bluesky_oauth').catch(() => {}); }} style={{ padding: '2px 8px', fontSize: '0.8rem' }}>
                                    Cancel
                                </button>
                            </div>
                            {blueskyOauthPending ? (
                                <div style={{ textAlign: 'center', padding: '16px 0' }}>
                                    <p style={{ fontSize: '0.9rem', margin: '0 0 8px' }}>⏳ Waiting for Bluesky authorization...</p>
                                    <p className="text-xs text-muted" style={{ margin: 0 }}>Complete the login in your browser. This will update automatically.</p>
                                </div>
                            ) : (
                                <>
                                    <input
                                        className="input-field"
                                        type="text"
                                        placeholder="Handle (e.g. user.bsky.social)"
                                        value={linkIdentifier}
                                        onChange={(e) => setLinkIdentifier(e.target.value)}
                                        onKeyDown={(e) => e.key === 'Enter' && handleLinkBluesky()}
                                    />
                                    <button
                                        className="btn btn-primary"
                                        onClick={handleLinkBluesky}
                                        disabled={linkLoading}
                                    >
                                        {linkLoading ? 'Starting OAuth...' : 'Authorize in Browser'}
                                    </button>
                                </>
                            )}
                            {linkError && <p className="text-sm" style={{ color: 'var(--accent-red)', margin: 0 }}>{linkError}</p>}
                        </div>
                    ) : (
                        <div style={{ display: 'flex', flexDirection: 'column', gap: 10, padding: 14, background: 'var(--bg-panel)', border: 'var(--border-default)' }}>
                            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                                <strong style={{ fontSize: '0.9rem' }}>🐘 Link Mastodon</strong>
                                <button className="btn btn-ghost" onClick={() => { setShowLinkMastodon(false); setMastodonOauthPending(false); setLinkLoading(false); setLinkError(''); setMastodonInstance(''); }} style={{ padding: '2px 8px', fontSize: '0.8rem' }}>
                                    Cancel
                                </button>
                            </div>
                            {mastodonOauthPending ? (
                                <div style={{ textAlign: 'center', padding: '16px 0' }}>
                                    <p style={{ fontSize: '0.9rem', margin: '0 0 8px' }}>⏳ Waiting for Mastodon authorization...</p>
                                    <p className="text-xs text-muted" style={{ margin: 0 }}>Complete the login in your browser. This will update automatically.</p>
                                </div>
                            ) : (
                                <>
                                    <input
                                        className="input-field"
                                        type="text"
                                        placeholder="Instance (e.g. mastodon.social)"
                                        value={mastodonInstance}
                                        onChange={(e) => setMastodonInstance(e.target.value)}
                                        onKeyDown={(e) => e.key === 'Enter' && startMastodonAuth()}
                                    />
                                    <button
                                        className="btn btn-primary"
                                        onClick={startMastodonAuth}
                                        disabled={linkLoading}
                                    >
                                        {linkLoading ? 'Starting OAuth...' : 'Authorize in Browser'}
                                    </button>
                                </>
                            )}
                            {linkError && <p className="text-sm" style={{ color: 'var(--accent-red)', margin: 0 }}>{linkError}</p>}
                        </div>
                    )}</div>
            </div>

            {/* Appearance / Theme */}
            <div className="card" style={{ marginTop: 16 }}>
                <h3>🎨 Appearance</h3>
                <p className="text-muted text-sm" style={{ marginBottom: 12 }}>
                    Choose a visual theme. Changes apply instantly.
                </p>
                <ThemePicker />
            </div>

            {/* P2P Sync — simplified */}
            <div className="card" style={{ marginTop: 16 }}>
                <h3>🔗 Device Sync</h3>
                <p className="text-muted text-sm" style={{ marginBottom: 12 }}>
                    Sync your vault across devices using peer-to-peer tickets.
                </p>
                <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' } as React.CSSProperties}>
                    <button className="btn btn-primary" onClick={createSyncDoc} disabled={loadingSync} style={{ flex: 1, minWidth: 120 } as React.CSSProperties}>
                        {loadingSync ? 'Working...' : '🎟️ Create Ticket'}
                    </button>
                    <div style={{ display: 'flex', gap: 4, flex: 2, minWidth: 200 } as React.CSSProperties}>
                        <input
                            className="input-field"
                            type="text"
                            placeholder="Paste ticket to join..."
                            value={joinTicket}
                            onChange={e => setJoinTicket(e.target.value)}
                            style={{ flex: 1 } as React.CSSProperties}
                        />
                        <button className="btn btn-primary" onClick={joinSyncDoc} disabled={!joinTicket || loadingSync}>
                            Join
                        </button>
                    </div>
                </div>
                {createdTicket && (
                    <div style={{ marginTop: 10, background: 'var(--bg-toolbar)', padding: 8, fontFamily: 'monospace', fontSize: '0.75rem', wordBreak: 'break-all', cursor: 'pointer', border: 'var(--border-default)' } as React.CSSProperties} onClick={() => copyToClipboard(createdTicket)}>
                        {createdTicket}
                        <span style={{ color: 'var(--accent-green)', marginLeft: 8, fontSize: '0.7rem' } as React.CSSProperties}>📋 click to copy</span>
                    </div>
                )}
                {syncStatus && <p className="text-sm" style={{ marginTop: 8, color: syncStatus.includes('Error') ? 'var(--accent-red)' : 'var(--accent-green)' }}>{syncStatus}</p>}
            </div>

            {/* Data Management */}
            <div className="card" style={{ marginTop: 16 }}>
                <h3>📦 Data Management</h3>
                <p className="text-muted text-sm" style={{ marginBottom: 12 }}>
                    {archiveCount} items in your vault
                </p>
                <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap' } as React.CSSProperties}>
                    <button className="btn" onClick={handleExport} disabled={isExporting}>
                        {isExporting ? '⏳ Exporting…' : '📤 Export Data'}
                    </button>
                    <button className="btn btn-danger" onClick={handleClearVault}>
                        🗑️ Delete Account ({archiveCount} items)
                    </button>
                </div>
                {exportStatus && <p className="text-sm" style={{ marginTop: 8, color: exportStatus.startsWith('❌') ? 'var(--accent-red)' : 'var(--accent-green)' }}>{exportStatus}</p>}
                {status && <p className="text-sm" style={{ marginTop: 8, color: 'var(--accent-green)' }}>{status}</p>}
                {error && <p className="text-sm" style={{ marginTop: 8, color: "var(--accent-red)" }}>{error}</p>}
            </div>
        </div>
    );
}