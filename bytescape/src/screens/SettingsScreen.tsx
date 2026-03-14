import { useState } from 'react';
import { invoke } from "@tauri-apps/api/core";
import { AuthStatus } from '../types';
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

    return (
        <div style={{ paddingBottom: 40 } as React.CSSProperties}>
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
                            {auth?.source === 'local' ? '🔒 Local identity' : '🦋 Bluesky'} · {auth?.did}
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
                    {auth?.source === 'bluesky' && (
                        <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '10px 14px', background: 'var(--bg-panel)', border: 'var(--border-default)', marginBottom: 8 }}>
                            <span style={{ fontSize: '1.2rem' }}>🦋</span>
                            <div style={{ flex: 1 }}>
                                <strong style={{ fontSize: '0.9rem' }}>{auth.handle}</strong>
                                <p className="text-xs text-muted" style={{ margin: 0 }}>Bluesky · Connected</p>
                            </div>
                            <span style={{ color: 'var(--accent-green)', fontSize: '0.82rem', fontWeight: 600 }}>✓ Linked</span>
                            <button
                                onClick={() => alert('Disconnect & remove Bluesky data — coming soon.')}
                                style={{
                                    padding: '4px 10px',
                                    fontSize: '0.75rem',
                                    background: 'transparent',
                                    border: '1px solid var(--accent-red, #ff5555)',
                                    color: 'var(--accent-red, #ff5555)',
                                    cursor: 'pointer',
                                    fontFamily: 'inherit',
                                    fontWeight: 500,
                                } as React.CSSProperties}
                            >
                                Disconnect
                            </button>
                        </div>
                    )}

                    {/* Green Plus — Connect Account */}
                    {!showLinkBluesky ? (
                        <button
                            onClick={() => setShowLinkBluesky(true)}
                            style={{
                                display: 'flex',
                                alignItems: 'center',
                                gap: 10,
                                padding: '12px 18px',
                                background: 'rgba(34, 197, 94, 0.1)',
                                border: '2px dashed #22c55e',
                                cursor: 'pointer',
                                width: '100%',
                                fontFamily: 'inherit',
                                fontSize: '0.9rem',
                                fontWeight: 600,
                                color: '#22c55e',
                                transition: 'background 0.15s',
                            } as React.CSSProperties}
                        >
                            <span style={{ fontSize: '1.4rem', lineHeight: 1 } as React.CSSProperties}>＋</span>
                            Connect Account
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
                            {linkSuccess && <p className="text-sm" style={{ color: 'var(--accent-green)', margin: 0 }}>{linkSuccess}</p>}
                            <p className="text-xs text-muted" style={{ margin: 0 }}>
                                Use an App Password from Bluesky Settings → Privacy → App Passwords.
                            </p>
                        </div>
                    )}
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
                    <button className="btn" onClick={() => alert('Export coming soon — this feature is being built.')} style={{ opacity: 0.7 } as React.CSSProperties}>
                        📤 Export Data
                    </button>
                    <button className="btn btn-danger" onClick={handleClearVault}>
                        🗑️ Delete Account ({archiveCount} items)
                    </button>
                </div>
                {status && <p className="text-sm" style={{ marginTop: 8, color: 'var(--accent-green)' }}>{status}</p>}
                {error && <p className="text-sm" style={{ marginTop: 8, color: "var(--accent-red)" }}>{error}</p>}
            </div>
        </div>
    );
}