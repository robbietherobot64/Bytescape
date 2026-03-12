import { useState, useEffect } from 'react';
import { invoke } from "@tauri-apps/api/core";
import { AuthStatus, GeneratorView } from '../types';
import { ThemePicker } from '../components/ThemePicker';

interface SettingsScreenProps {
    auth: AuthStatus | null;
    archiveCount: number;
    onLogout: () => void;
    onClearVault: () => void;
    onAuthUpdated?: (auth: AuthStatus) => void;
}

export function SettingsScreen({ auth, archiveCount, onLogout, onClearVault, onAuthUpdated }: SettingsScreenProps) {
    const [error, setError] = useState("");
    const [status, setStatus] = useState("");
    const [savedFeeds, setSavedFeeds] = useState<GeneratorView[]>([]);
    const [loadingFeeds, setLoadingFeeds] = useState(false);

    // Link Bluesky account state
    const [showLinkBluesky, setShowLinkBluesky] = useState(false);
    const [linkIdentifier, setLinkIdentifier] = useState("");
    const [linkPassword, setLinkPassword] = useState("");
    const [linkLoading, setLinkLoading] = useState(false);
    const [linkError, setLinkError] = useState("");
    const [linkSuccess, setLinkSuccess] = useState("");

    // Sync state
    const [loadingSync, setLoadingSync] = useState(false);
    const [createdTicket, setCreatedTicket] = useState<string | null>(null);
    const [joinTicket, setJoinTicket] = useState("");
    const [syncStatus, setSyncStatus] = useState("");

    useEffect(() => {
        fetchSavedFeeds();
    }, []);

    async function fetchSavedFeeds() {
        setLoadingFeeds(true);
        try {
            const feeds = await invoke<GeneratorView[]>("list_saved_feeds");
            setSavedFeeds(feeds);
        } catch (e) {
            console.error("Failed to fetch feeds in Settings:", e);
        } finally {
            setLoadingFeeds(false);
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
        if (confirm("Clear all files and imported items from your vault? You can re-import them.")) {
            try {
                const cleared = await invoke<number>("clear_vault");
                setStatus(`Cleared ${cleared} vault records/files. Re-import to get a clean copy.`);
                onClearVault();
            } catch (e: any) {
                setError(typeof e === "string" ? e : "Failed to clear vault");
            }
        }
    }

    async function handleLinkBluesky() {
        if (!linkIdentifier.trim() || !linkPassword.trim()) {
            setLinkError("Handle and app password are required.");
            return;
        }
        setLinkLoading(true);
        setLinkError("");
        setLinkSuccess("");
        try {
            const status = await invoke<AuthStatus>("login", {
                identifier: linkIdentifier.trim(),
                password: linkPassword.trim(),
            });
            setLinkSuccess(`✅ Linked as ${status.handle}! You can now import from Bluesky.`);
            setShowLinkBluesky(false);
            setLinkIdentifier("");
            setLinkPassword("");
            if (onAuthUpdated) {
                onAuthUpdated(status);
            }
        } catch (e: any) {
            setLinkError(typeof e === "string" ? e : e.message || "Failed to link Bluesky account");
        } finally {
            setLinkLoading(false);
        }
    }

    const isLocalOnly = auth?.source === 'local';

    return (
        <div style={{ paddingBottom: 40 } as React.CSSProperties}>
            <div className="main-header">
                <h1>⚙️ Settings</h1>
            </div>

            <div className="card">
                <h3>Account</h3>
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16 }}>
                    <div>
                        <p className="text-muted text-sm" style={{ margin: 0 }}>
                            Signed in as <strong>{auth?.local_username || auth?.handle}</strong>
                        </p>
                        <p className="text-xs text-muted" style={{ opacity: 0.7 }}>
                            {auth?.source === 'local' ? '🔒 Local identity' : '🦋 Bluesky'} · {auth?.did}
                        </p>
                    </div>
                    <button className="btn btn-danger" onClick={onLogout}>
                        Sign Out
                    </button>
                </div>
            </div>

            {/* Appearance / Theme */}
            <div className="card" style={{ marginTop: 16 }}>
                <h3>🎨 Appearance</h3>
                <p className="text-muted text-sm" style={{ marginBottom: 12 }}>
                    Choose a visual theme. Changes apply instantly.
                </p>
                <ThemePicker />
            </div>

            {/* Connected Accounts */}
            <div className="card" style={{ marginTop: 16 }}>
                <h3>🔗 Connected Accounts</h3>
                <p className="text-muted text-sm" style={{ marginBottom: 16 }}>
                    Link external accounts to import live feeds and sync data.
                </p>

                {auth?.source === 'bluesky' ? (
                    <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '10px 14px', background: 'var(--bg-panel)', border: 'var(--border-default)' }}>
                        <span style={{ fontSize: '1.2rem' }}>🦋</span>
                        <div style={{ flex: 1 }}>
                            <strong style={{ fontSize: '0.9rem' }}>{auth.handle}</strong>
                            <p className="text-xs text-muted" style={{ margin: 0 }}>Bluesky · Connected</p>
                        </div>
                        <span style={{ color: 'var(--accent-green)', fontSize: '0.82rem', fontWeight: 600 }}>✓ Linked</span>
                    </div>
                ) : (
                    <>
                        {linkSuccess && (
                            <p className="text-sm" style={{ color: 'var(--accent-green)', marginBottom: 12 }}>{linkSuccess}</p>
                        )}
                        {!showLinkBluesky ? (
                            <button
                                className="btn btn-primary"
                                onClick={() => setShowLinkBluesky(true)}
                                style={{ padding: '10px 18px' }}
                            >
                                🦋 Link Bluesky Account
                            </button>
                        ) : (
                            <div style={{ display: 'flex', flexDirection: 'column', gap: 10, padding: 14, background: 'var(--bg-panel)', border: 'var(--border-default)' }}>
                                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                                    <strong style={{ fontSize: '0.9rem' }}>🦋 Link Bluesky</strong>
                                    <button className="btn btn-ghost" onClick={() => { setShowLinkBluesky(false); setLinkError(''); }} style={{ padding: '2px 8px', fontSize: '0.8rem' }}>
                                        Cancel
                                    </button>
                                </div>
                                <input
                                    className="input-field"
                                    type="text"
                                    placeholder="Handle (e.g. user.bsky.social)"
                                    value={linkIdentifier}
                                    onChange={(e) => setLinkIdentifier(e.target.value)}
                                    onKeyDown={(e) => e.key === 'Enter' && handleLinkBluesky()}
                                />
                                <input
                                    className="input-field"
                                    type="password"
                                    placeholder="App Password"
                                    value={linkPassword}
                                    onChange={(e) => setLinkPassword(e.target.value)}
                                    onKeyDown={(e) => e.key === 'Enter' && handleLinkBluesky()}
                                />
                                <button
                                    className="btn btn-primary"
                                    onClick={handleLinkBluesky}
                                    disabled={linkLoading}
                                >
                                    {linkLoading ? "Linking..." : "Connect & Link"}
                                </button>
                                {linkError && <p className="text-sm" style={{ color: 'var(--accent-red)', margin: 0 }}>{linkError}</p>}
                                <p className="text-xs text-muted" style={{ margin: 0 }}>
                                    Use an App Password from Bluesky Settings → Privacy → App Passwords.
                                </p>
                            </div>
                        )}
                    </>
                )}
            </div>

            {!isLocalOnly && (
            <div className="card" style={{ marginTop: 16 }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 16 }}>
                    <h3 style={{ margin: 0 }}>🦋 Bluesky Feeds</h3>
                    <span className="badge" style={{ background: 'var(--bg-tertiary)', padding: '2px 8px', borderRadius: 12, fontSize: '0.75rem' }}>
                        {savedFeeds.length}
                    </span>
                </div>

                <p className="text-muted text-sm" style={{ marginBottom: 16 }}>
                    These feeds are imported from your Bluesky account and are used to populate your <strong>Following</strong> tab.
                </p>

                {loadingFeeds ? (
                    <div style={{ textAlign: 'center', padding: 20 }}>Loading feeds...</div>
                ) : (
                    <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))', gap: 12 }}>
                        {savedFeeds.map(feed => (
                            <div key={feed.uri} style={{
                                display: 'flex',
                                alignItems: 'center',
                                gap: 10,
                                padding: '8px 12px',
                                background: 'var(--bg-secondary)',
                                borderRadius: 8,
                                border: '1px solid var(--border-color)'
                            }}>
                                {feed.avatar ? (
                                    <img src={feed.avatar} style={{ width: 24, height: 24, borderRadius: 4 }} alt="" />
                                ) : (
                                    <div style={{ width: 24, height: 24, background: 'var(--bg-tertiary)', borderRadius: 4, display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: '0.8rem' }}>🔖</div>
                                )}
                                <span style={{ fontSize: '0.85rem', fontWeight: 500, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                                    {feed.displayName}
                                </span>
                            </div>
                        ))}
                        {savedFeeds.length === 0 && (
                            <div style={{ gridColumn: '1 / -1', textAlign: 'center', padding: 20, color: 'var(--text-muted)', fontStyle: 'italic' }}>
                                No saved feeds found on Bluesky.
                            </div>
                        )}
                    </div>
                )}
            </div>
            )}

            <div className="card" style={{ marginTop: 16 }}>
                <h3>🔄 Device Sync (Beta)</h3>
                <p className="text-muted text-sm" style={{ marginBottom: 16 }}>
                    Connect multiple devices to sync your vault peer-to-peer.
                </p>
                
                <div style={{ display: 'grid', gap: 16 }}>
                    <div style={{ padding: 12, background: 'var(--bg-secondary)', borderRadius: 8 }}>
                        <h4 style={{ margin: '0 0 8px 0', fontSize: '0.9rem' }}>Create New Group</h4>
                        <button className="btn btn-primary" onClick={createSyncDoc} disabled={loadingSync}>
                            {loadingSync ? 'Creating...' : 'Generate New Sync Ticket'}
                        </button>
                        {createdTicket && (
                            <div style={{ marginTop: 12 }}>
                                <p className="text-xs text-muted">Share this ticket with another device:</p>
                                <div style={{ 
                                    background: 'var(--bg-tertiary)', 
                                    padding: 8, 
                                    borderRadius: 4, 
                                    wordBreak: 'break-all',
                                    fontSize: '0.8rem',
                                    fontFamily: 'monospace',
                                    userSelect: 'all',
                                    cursor: 'pointer'
                                }} onClick={() => copyToClipboard(createdTicket)}>
                                    {createdTicket}
                                </div>
                                <p className="text-xs success" style={{ marginTop: 4 }}>
                                    (Click to copy)
                                </p>
                            </div>
                        )}
                    </div>

                    <div style={{ padding: 12, background: 'var(--bg-secondary)', borderRadius: 8 }}>
                        <h4 style={{ margin: '0 0 8px 0', fontSize: '0.9rem' }}>Join Existing Group</h4>
                        <div style={{ display: 'flex', gap: 8 }}>
                           <input 
                                type="text" 
                                placeholder="Paste ticket here..." 
                                value={joinTicket}
                                onChange={e => setJoinTicket(e.target.value)}
                                style={{ flex: 1, padding: 8, borderRadius: 4, border: '1px solid var(--border-color)', background: 'var(--bg-primary)', color: 'var(--text-primary)' }}
                           />
                           <button className="btn btn-secondary" onClick={joinSyncDoc} disabled={!joinTicket || loadingSync}>
                                {loadingSync ? 'Joining...' : 'Join'}
                           </button>
                        </div>
                    </div>
                </div>
                {syncStatus && <p className="text-sm" style={{ marginTop: 12, color: syncStatus.includes('Error') ? 'var(--accent-red)' : 'var(--accent-green)' }}>{syncStatus}</p>}
            </div>

            <div className="card" style={{ marginTop: 16 }}>
                <h3>Data</h3>
                <p className="text-muted text-sm" style={{ marginBottom: 12 }}>
                    {archiveCount} files in your vault
                </p>
                <button className="btn btn-danger" onClick={handleClearVault}>
                    🗑️ Clear Vault ({archiveCount} files)
                </button>
                {status && <p className="text-sm success" style={{ marginTop: 8 }}>{status}</p>}
                {error && <p className="text-sm error" style={{ marginTop: 8, color: "var(--accent-red)" }}>{error}</p>}
            </div>
        </div>
    );
}
