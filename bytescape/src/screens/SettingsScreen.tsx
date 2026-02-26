import { useState, useEffect, useCallback } from 'react';
import { invoke } from "@tauri-apps/api/core";
import { AuthStatus, GeneratorView } from '../types';

interface NodeStatus {
    node_id: string;
    is_online: boolean;
    relay_url: string | null;
    local_addrs: string[];
}

interface TestDocResult {
    ticket: string;
    namespace_id: string;
    entry_count: number;
}

interface TestDocReadResult {
    namespace_id: string;
    entries: { key: string; content: string; author: string }[];
    sync_elapsed_ms: number;
}

interface SettingsScreenProps {
    auth: AuthStatus | null;
    archiveCount: number;
    onLogout: () => void;
    onClearVault: () => void;
}

export function SettingsScreen({ auth, archiveCount, onLogout, onClearVault }: SettingsScreenProps) {
    const [error, setError] = useState("");
    const [status, setStatus] = useState("");
    const [savedFeeds, setSavedFeeds] = useState<GeneratorView[]>([]);
    const [loadingFeeds, setLoadingFeeds] = useState(false);

    // Sync state
    const [loadingSync, setLoadingSync] = useState(false);
    const [createdTicket, setCreatedTicket] = useState<string | null>(null);
    const [joinTicket, setJoinTicket] = useState("");
    const [syncStatus, setSyncStatus] = useState("");

    // P2P Network state
    const [nodeStatus, setNodeStatus] = useState<NodeStatus | null>(null);
    const [nodeStatusError, setNodeStatusError] = useState("");
    const [loadingNodeStatus, setLoadingNodeStatus] = useState(false);
    const [p2pTestTicket, setP2pTestTicket] = useState<string | null>(null);
    const [p2pTestResult, setP2pTestResult] = useState<TestDocReadResult | null>(null);
    const [p2pJoinTicketInput, setP2pJoinTicketInput] = useState("");
    const [p2pStatus, setP2pStatus] = useState("");
    const [loadingP2p, setLoadingP2p] = useState(false);
    const [p2pLog, setP2pLog] = useState<string[]>([]);

    const addP2pLog = useCallback((msg: string) => {
        const ts = new Date().toLocaleTimeString();
        setP2pLog(prev => [`[${ts}] ${msg}`, ...prev].slice(0, 50));
    }, []);

    useEffect(() => {
        fetchSavedFeeds();
    }, []);

    async function fetchNodeStatus() {
        setLoadingNodeStatus(true);
        setNodeStatusError("");
        try {
            const status = await invoke<NodeStatus>("get_node_status");
            setNodeStatus(status);
            addP2pLog(`Node online: ${status.node_id.slice(0, 12)}... | ${status.local_addrs.length} local addr(s)`);
        } catch (e: any) {
            const msg = typeof e === "string" ? e : JSON.stringify(e);
            setNodeStatusError(msg);
            addP2pLog(`❌ Status check failed: ${msg}`);
        } finally {
            setLoadingNodeStatus(false);
        }
    }

    async function handleCreateTestDoc() {
        setLoadingP2p(true);
        setP2pStatus("");
        try {
            addP2pLog("Creating test document...");
            const result = await invoke<TestDocResult>("p2p_create_test_doc");
            setP2pTestTicket(result.ticket);
            setP2pStatus(`✅ Test doc created (NS: ${result.namespace_id.slice(0, 8)}...) with ${result.entry_count} entry`);
            addP2pLog(`✅ Created doc ${result.namespace_id.slice(0, 12)}... — ticket ready to share`);
        } catch (e: any) {
            const msg = typeof e === "string" ? e : JSON.stringify(e);
            setP2pStatus("❌ " + msg);
            addP2pLog(`❌ Create failed: ${msg}`);
        } finally {
            setLoadingP2p(false);
        }
    }

    async function handleJoinTestDoc() {
        if (!p2pJoinTicketInput.trim()) return;
        setLoadingP2p(true);
        setP2pStatus("");
        setP2pTestResult(null);
        try {
            addP2pLog("Joining test document... (waiting for sync)");
            const result = await invoke<TestDocReadResult>("p2p_join_and_read_test_doc", { ticketStr: p2pJoinTicketInput.trim() });
            setP2pTestResult(result);
            setP2pStatus(`✅ Synced ${result.entries.length} entries in ${result.sync_elapsed_ms}ms`);
            addP2pLog(`✅ Joined doc ${result.namespace_id.slice(0, 12)}... — ${result.entries.length} entries synced in ${result.sync_elapsed_ms}ms`);
            setP2pJoinTicketInput("");
        } catch (e: any) {
            const msg = typeof e === "string" ? e : JSON.stringify(e);
            setP2pStatus("❌ " + msg);
            addP2pLog(`❌ Join failed: ${msg}`);
        } finally {
            setLoadingP2p(false);
        }
    }

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

    const inputStyle: React.CSSProperties = {
        flex: 1, padding: 8, borderRadius: 4,
        border: '1px solid var(--border-color)',
        background: 'var(--bg-primary)', color: 'var(--text-primary)',
        fontFamily: 'monospace', fontSize: '0.8rem'
    };

    return (
        <div style={{ paddingBottom: 40 }}>
            <div className="main-header">
                <h1>⚙️ Settings</h1>
            </div>

            <div className="card">
                <h3>Account</h3>
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16 }}>
                    <div>
                        <p className="text-muted text-sm" style={{ margin: 0 }}>
                            Signed in as <strong>{auth?.handle}</strong>
                        </p>
                        <p className="text-xs text-muted" style={{ opacity: 0.7 }}>{auth?.did}</p>
                    </div>
                    <button className="btn btn-danger" onClick={onLogout}>
                        Sign Out
                    </button>
                </div>
            </div>

            {/* P2P Network Diagnostics */}
            <div className="card" style={{ marginTop: 16 }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 16 }}>
                    <h3 style={{ margin: 0 }}>🌐 P2P Network</h3>
                    <span style={{
                        display: 'inline-block', width: 8, height: 8, borderRadius: '50%',
                        background: nodeStatus?.is_online ? '#4caf50' : 'var(--text-muted)'
                    }} />
                </div>

                {/* Node Status */}
                <div style={{ padding: 12, background: 'var(--bg-secondary)', borderRadius: 8, marginBottom: 12 }}>
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
                        <h4 style={{ margin: 0, fontSize: '0.9rem' }}>Node Status</h4>
                        <button className="btn btn-ghost btn-sm" onClick={fetchNodeStatus} disabled={loadingNodeStatus}
                            style={{ padding: '2px 8px', fontSize: '0.75rem' }}>
                            {loadingNodeStatus ? '⏳' : '🔄'} Refresh
                        </button>
                    </div>
                    {nodeStatus ? (
                        <div style={{ fontSize: '0.8rem' }}>
                            <p style={{ margin: '2px 0', fontFamily: 'monospace', wordBreak: 'break-all' }}>
                                <strong>Node ID:</strong> {nodeStatus.node_id}
                                <button className="btn btn-ghost btn-sm" onClick={() => { navigator.clipboard.writeText(nodeStatus.node_id); addP2pLog('Node ID copied!'); }}
                                    style={{ marginLeft: 6, padding: '0 4px', fontSize: '0.7rem' }}>📋</button>
                            </p>
                            <p style={{ margin: '2px 0' }}>
                                <strong>Relay:</strong> {nodeStatus.relay_url || 'None (LAN only)'}
                            </p>
                            <p style={{ margin: '2px 0' }}>
                                <strong>Local Addrs:</strong> {nodeStatus.local_addrs.length > 0 ? nodeStatus.local_addrs.join(', ') : 'None detected'}
                            </p>
                        </div>
                    ) : (
                        <p className="text-muted text-sm">Click Refresh to check node status</p>
                    )}
                    {nodeStatusError && <p style={{ color: 'var(--accent-red)', fontSize: '0.8rem', margin: '4px 0 0' }}>{nodeStatusError}</p>}
                </div>

                {/* P2P Sync Test */}
                <div style={{ padding: 12, background: 'var(--bg-secondary)', borderRadius: 8, marginBottom: 12 }}>
                    <h4 style={{ margin: '0 0 8px 0', fontSize: '0.9rem' }}>🧪 P2P Sync Test</h4>
                    <p className="text-muted text-xs" style={{ marginBottom: 12 }}>
                        Create a test document on one device, join it on another to verify P2P sync.
                    </p>

                    <div style={{ display: 'grid', gap: 12 }}>
                        {/* Create Test Doc */}
                        <div>
                            <button className="btn btn-primary btn-sm" onClick={handleCreateTestDoc} disabled={loadingP2p}
                                style={{ marginBottom: 8 }}>
                                {loadingP2p ? '⏳ Creating...' : '📝 Create Test Doc'}
                            </button>
                            {p2pTestTicket && (
                                <div style={{
                                    background: 'var(--bg-tertiary)', padding: 8, borderRadius: 4,
                                    wordBreak: 'break-all', fontSize: '0.75rem', fontFamily: 'monospace',
                                    userSelect: 'all', cursor: 'pointer', maxHeight: 80, overflow: 'auto'
                                }} onClick={() => { navigator.clipboard.writeText(p2pTestTicket); addP2pLog('Test ticket copied!'); }}>
                                    {p2pTestTicket}
                                </div>
                            )}
                            {p2pTestTicket && <p className="text-xs text-muted" style={{ marginTop: 2 }}>(Click ticket to copy)</p>}
                        </div>

                        {/* Join Test Doc */}
                        <div>
                            <div style={{ display: 'flex', gap: 8 }}>
                                <input type="text" placeholder="Paste test ticket here..."
                                    value={p2pJoinTicketInput}
                                    onChange={e => setP2pJoinTicketInput(e.target.value)}
                                    style={inputStyle} />
                                <button className="btn btn-secondary btn-sm" onClick={handleJoinTestDoc}
                                    disabled={!p2pJoinTicketInput.trim() || loadingP2p}>
                                    {loadingP2p ? '⏳' : '🔗 Join'}
                                </button>
                            </div>
                        </div>
                    </div>

                    {p2pStatus && (
                        <p style={{
                            marginTop: 8, fontSize: '0.8rem',
                            color: p2pStatus.includes('❌') ? 'var(--accent-red)' : 'var(--accent-green)'
                        }}>
                            {p2pStatus}
                        </p>
                    )}

                    {/* Synced entries */}
                    {p2pTestResult && p2pTestResult.entries.length > 0 && (
                        <div style={{ marginTop: 12, padding: 8, background: 'var(--bg-tertiary)', borderRadius: 4 }}>
                            <h5 style={{ margin: '0 0 4px 0', fontSize: '0.8rem' }}>Synced Entries ({p2pTestResult.entries.length})</h5>
                            {p2pTestResult.entries.map((entry, i) => (
                                <div key={i} style={{ fontSize: '0.75rem', padding: '4px 0', borderTop: i > 0 ? '1px solid var(--border-color)' : 'none' }}>
                                    <span style={{ color: 'var(--text-muted)' }}>{entry.key}:</span>{' '}
                                    <span>{entry.content}</span>
                                </div>
                            ))}
                        </div>
                    )}
                </div>

                {/* P2P Event Log */}
                {p2pLog.length > 0 && (
                    <div style={{ padding: 12, background: 'var(--bg-secondary)', borderRadius: 8 }}>
                        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 4 }}>
                            <h4 style={{ margin: 0, fontSize: '0.9rem' }}>📋 Event Log</h4>
                            <button className="btn btn-ghost btn-sm" onClick={() => setP2pLog([])}
                                style={{ padding: '0 6px', fontSize: '0.7rem' }}>Clear</button>
                        </div>
                        <div style={{ maxHeight: 150, overflow: 'auto', fontSize: '0.75rem', fontFamily: 'monospace' }}>
                            {p2pLog.map((msg, i) => (
                                <div key={i} style={{ padding: '2px 0', color: msg.includes('❌') ? 'var(--accent-red)' : 'var(--text-secondary)' }}>
                                    {msg}
                                </div>
                            ))}
                        </div>
                    </div>
                )}
            </div>

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
