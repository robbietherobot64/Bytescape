import { SidebarItem, AuthStatus } from '../types';

interface ImportScreenProps {
    auth: AuthStatus | null;
    isImporting: boolean;
    importStatus: string;
    archiveCount: number;
    onImportBluesky: () => void;
    onFilePick: () => void;
    onNavigate: (item: SidebarItem) => void;
    error: string;
}

export function ImportScreen({
    auth,
    isImporting,
    importStatus,
    archiveCount,
    onImportBluesky,
    onFilePick,
    onNavigate: _onNavigate,
    error
}: ImportScreenProps) {
    const isBlueskyLinked = auth?.source === 'bluesky';

    return (
        <>
            <div className="main-header">
                <h1 style={{ textAlign: 'center', width: '100%' } as React.CSSProperties}>Import Archive</h1>
            </div>

            <div className="import-tools">
                <div className="card card-glow">
                    <h2>🔌 Import Your Social History</h2>
                    <p className="text-muted text-sm" style={{ marginBottom: 16 }}>
                        Pull your posts into your private vault. Everything stays on your device.
                    </p>

                    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
                        {/* Bluesky — instant (requires linked Bluesky account) */}
                        <button
                            className="btn btn-primary btn-block"
                            onClick={onImportBluesky}
                            disabled={isImporting || !isBlueskyLinked}
                            style={{ padding: "14px 20px", fontSize: "0.95rem" }}
                        >
                            {isImporting ? "⏳ Importing..." : "🦋 Import from Bluesky"}
                        </button>
                        {!isBlueskyLinked && (
                            <p className="text-sm text-muted" style={{ textAlign: "center", marginTop: -4 }}>
                                Link a Bluesky account in <strong>Settings</strong> to enable this.
                            </p>
                        )}
                        {importStatus && (
                            <p className="text-sm" style={{
                                color: importStatus.startsWith("✅") ? "var(--accent-green)" : "var(--text-secondary)",
                                textAlign: "center",
                            }}>
                                {importStatus}
                            </p>
                        )}
                        {error && (
                            <p className="text-sm" style={{ color: "var(--accent-red)", textAlign: "center" }}>
                                {error}
                            </p>
                        )}

                        {/* Archive imports */}
                        <div style={{ borderTop: "1px solid var(--border)", paddingTop: 12 }}>
                            <p className="text-muted text-sm mb-8">Or import from a data export:</p>
                            <div className="drop-zone" onClick={onFilePick}>
                                <div className="drop-icon">📂</div>
                                <div className="drop-title">Drop a ZIP export</div>
                                <div className="drop-hint">
                                    Supports Facebook, Instagram, Google Takeout, Twitter/X archives
                                </div>
                            </div>
                        </div>
                    </div>
                </div>

                {archiveCount > 0 && (
                    <div className="card" style={{ marginTop: 16, textAlign: 'center' } as React.CSSProperties}>
                        <h2>📜 Your Vault</h2>
                        <p className="text-muted text-sm">
                            {archiveCount} files imported to your vault
                        </p>
                    </div>
                )}
            </div>
        </>
    );
}
