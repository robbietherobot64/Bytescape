import { SidebarItem } from '../types';

interface ImportScreenProps {
    isImporting: boolean;
    importStatus: string;
    archiveCount: number;
    onImportBluesky: () => void;
    onFilePick: () => void;
    onNavigate: (item: SidebarItem) => void;
}

export function ImportScreen({
    isImporting,
    importStatus,
    archiveCount,
    onImportBluesky,
    onFilePick,
    onNavigate
}: ImportScreenProps) {
    return (
        <>
            <div className="main-header">
                <h1>Import</h1>
            </div>

            <div className="import-tools">
                <div className="card card-glow">
                    <h2>🔌 Import Your Social History</h2>
                    <p className="text-muted text-sm" style={{ marginBottom: 16 }}>
                        Pull your posts into your private vault. Everything stays on your device.
                    </p>

                    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
                        {/* Bluesky — instant */}
                        <button
                            className="btn btn-primary btn-block"
                            onClick={onImportBluesky}
                            disabled={isImporting}
                            style={{ padding: "14px 20px", fontSize: "0.95rem" }}
                        >
                            {isImporting ? "⏳ Importing..." : "🦋 Import from Bluesky"}
                        </button>
                        {importStatus && (
                            <p className="text-sm" style={{
                                color: importStatus.startsWith("✅") ? "var(--accent-green)" : "var(--text-secondary)",
                                textAlign: "center",
                            }}>
                                {importStatus}
                            </p>
                        )}

                        {/* Archive imports */}
                        <div style={{ borderTop: "1px solid var(--border)", paddingTop: 12 }}>
                            <p className="text-muted text-sm mb-8">Or import from a data export:</p>
                            <div className="drop-zone" onClick={onFilePick}>
                                <div className="drop-icon">📂</div>
                                <div className="drop-title">Drop a ZIP export</div>
                                <div className="drop-hint">
                                    Supports Facebook, Google Takeout, Twitter/X archives
                                </div>
                            </div>
                        </div>
                    </div>
                </div>

                {archiveCount > 0 && (
                    <div className="card" style={{ marginTop: 16 }}>
                        <h2>📜 Your Vault</h2>
                        <p className="text-muted text-sm" style={{ marginBottom: 12 }}>
                            {archiveCount} files in your vault
                        </p>
                        <button
                            className="btn btn-primary"
                            onClick={() => onNavigate("feed")}
                        >
                            View Feed
                        </button>
                    </div>
                )}
            </div>
        </>
    );
}
