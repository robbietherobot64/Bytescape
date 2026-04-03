import React, { useState, useEffect, useRef, useCallback } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { SidebarItem, AuthStatus } from '../types';

/**
 * PURPOSE
 * Provides import entry actions for Bluesky sync and archive ZIP ingestion,
 * including native drag-drop handling in Tauri desktop context.
 *
 * HOW IT WORKS
 * - Shows protocol import CTA (Bluesky) gated by auth/link status.
 * - Opens archive picker via callback and accepts ZIP path drops from native window events.
 * - During import, shows retro ASCII terminal animation with progress.
 * - On completion, auto-redirects to profile posts.
 *
 * RELATED FILES
 * - src/App.tsx (imports handlers and routing to this screen)
 * - src/screens/AuditScreen.tsx (downstream import audit/progress presentation)
 * - src-tauri/src/lib.rs (archive/bluesky commands eventually triggered by parent handlers)
 *
 * CROSS-SYSTEM NOTES
 * - This screen binds directly to Tauri window drag-drop events, but delegates all
 *   import/store/provider side effects to parent callbacks (`onImportBluesky`, `onFilePick`, `onFileDropPath`).
 */

interface ImportScreenProps {
    auth: AuthStatus | null;
    isImporting: boolean;
    importStatus: string;
    archiveCount: number;
    onImportBluesky: () => void;
    onFilePick: () => void;
    /** Called when a file is dropped via Tauri native drag-drop (provides filesystem path) */
    onFileDropPath?: (path: string) => void;
    onNavigate: (item: SidebarItem) => void;
    error: string;
}

/**
 * ASCII art terminal log messages cycled during import animation.
 */
const TERMINAL_LINES = [
    '> Initializing data pipeline...',
    '> Scanning archive structure...',
    '> Extracting posts metadata...',
    '> Parsing timestamps & authors...',
    '> Normalizing media references...',
    '> Classifying content types...',
    '> Building search index...',
    '> Resolving media attachments...',
    '> Hashing blob references...',
    '> Writing to local vault...',
    '> Verifying data integrity...',
    '> Finalizing import batch...',
];

/**
 * Import launcher UI for account-based and file-based data ingestion.
 */
export function ImportScreen({
    auth,
    isImporting,
    importStatus,
    archiveCount,
    onImportBluesky,
    onFilePick,
    onFileDropPath,
    onNavigate,
    error
}: ImportScreenProps) {
    // ═══════════════════════════════════════════════════════════════════════
    // View state
    // ═══════════════════════════════════════════════════════════════════════
    const isBlueskyLinked = auth?.source === 'bluesky';
    const [dragOver, setDragOver] = useState(false);

    // Terminal animation state
    const [terminalLog, setTerminalLog] = useState<string[]>([]);
    const terminalRef = useRef<HTMLDivElement>(null);
    const lineIndexRef = useRef(0);

    // Redirect state
    const [redirectCountdown, setRedirectCountdown] = useState<number | null>(null);

    // ═══════════════════════════════════════════════════════════════════════
    // Terminal animation — adds log lines while importing
    // ═══════════════════════════════════════════════════════════════════════
    useEffect(() => {
        if (!isImporting) {
            lineIndexRef.current = 0;
            return;
        }
        setTerminalLog(['> Import started...']);
        lineIndexRef.current = 0;
        const interval = setInterval(() => {
            const line = TERMINAL_LINES[lineIndexRef.current % TERMINAL_LINES.length];
            setTerminalLog(prev => [...prev.slice(-20), line]);
            lineIndexRef.current++;
        }, 800 + Math.random() * 600);
        return () => clearInterval(interval);
    }, [isImporting]);

    // Auto-scroll terminal to bottom
    useEffect(() => {
        if (terminalRef.current) {
            terminalRef.current.scrollTop = terminalRef.current.scrollHeight;
        }
    }, [terminalLog]);

    // ═══════════════════════════════════════════════════════════════════════
    // Redirect to profile on completion
    // ═══════════════════════════════════════════════════════════════════════
    const importComplete = importStatus?.startsWith('✅');
    const handleRedirect = useCallback(() => {
        onNavigate('profile');
    }, [onNavigate]);

    useEffect(() => {
        if (!importComplete) {
            setRedirectCountdown(null);
            return;
        }
        setRedirectCountdown(5);
        const interval = setInterval(() => {
            setRedirectCountdown(prev => {
                if (prev === null || prev <= 1) {
                    clearInterval(interval);
                    handleRedirect();
                    return 0;
                }
                return prev - 1;
            });
        }, 1000);
        return () => clearInterval(interval);
    }, [importComplete, handleRedirect]);

    // ═══════════════════════════════════════════════════════════════════════
    // Native drag-drop subscription
    // ═══════════════════════════════════════════════════════════════════════
    useEffect(() => {
        let cancelled = false;
        let unlisten: (() => void) | undefined;

        getCurrentWindow().onDragDropEvent((event) => {
            if (cancelled) return;
            if (event.payload.type === 'over') {
                setDragOver(true);
            } else if (event.payload.type === 'leave') {
                setDragOver(false);
            } else if (event.payload.type === 'drop') {
                setDragOver(false);
                const paths: string[] = event.payload.paths;
                const zipPath = paths.find(p => p.toLowerCase().endsWith('.zip'));
                if (zipPath && onFileDropPath) {
                    onFileDropPath(zipPath);
                }
            }
        }).then(fn => {
            if (!cancelled) unlisten = fn;
        });

        return () => {
            cancelled = true;
            unlisten?.();
        };
    }, [onFileDropPath]);

    // ═══════════════════════════════════════════════════════════════════════
    // Render
    // ═══════════════════════════════════════════════════════════════════════

    // ─── Importing state: show terminal animation ───
    if (isImporting || importComplete) {
        return (
            <div className="screen-centered">
                <div className="main-header">
                    <h1 style={{ textAlign: 'center', width: '100%' } as React.CSSProperties}>
                        {importComplete ? '✅ Import Complete' : '📡 Importing...'}
                    </h1>
                </div>
                <div className="import-tools">
                    <div className="import-terminal">
                        <div className="import-terminal-titlebar">
                            <span className="import-terminal-dot import-terminal-dot--red" />
                            <span className="import-terminal-dot import-terminal-dot--yellow" />
                            <span className="import-terminal-dot import-terminal-dot--green" />
                            <span className="import-terminal-title">bytescape — import</span>
                        </div>
                        <div className="import-terminal-body" ref={terminalRef}>
                            {terminalLog.map((line, i) => (
                                <div key={i} className="import-terminal-line">
                                    {line}
                                </div>
                            ))}
                            {!importComplete && (
                                <div className="import-terminal-line import-terminal-cursor">
                                    {'>'} <span className="import-terminal-blink">█</span>
                                </div>
                            )}
                            {importComplete && (
                                <>
                                    <div className="import-terminal-line import-terminal-line--success">
                                        {importStatus}
                                    </div>
                                    <div className="import-terminal-line">
                                        {'>'} Redirecting to profile in {redirectCountdown ?? 0}s...
                                    </div>
                                </>
                            )}
                        </div>
                        {!importComplete && (
                            <div className="import-terminal-progress">
                                <div className="import-terminal-progress-bar" />
                            </div>
                        )}
                    </div>
                    {importComplete && (
                        <button
                            className="btn btn-primary"
                            onClick={handleRedirect}
                            style={{ marginTop: 12, width: '100%', padding: '12px 20px' } as React.CSSProperties}
                        >
                            📋 View Imported Posts Now
                        </button>
                    )}
                </div>
            </div>
        );
    }

    // ─── Default state: import actions ───
    return (
        <div className="screen-centered">
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
                            🦋 Import from Bluesky
                        </button>
                        {!isBlueskyLinked && (
                            <p className="text-sm text-muted" style={{ textAlign: "center", marginTop: -4 }}>
                                Link a Bluesky account in <strong>Settings</strong> to enable this.
                            </p>
                        )}
                        {error && (
                            <p className="text-sm" style={{ color: "var(--accent-red)", textAlign: "center" }}>
                                {error}
                            </p>
                        )}

                        {/* Archive imports — click opens file picker, drag-drop handled by Tauri native event */}
                        <div style={{ borderTop: "1px solid var(--border)", paddingTop: 12 }}>
                            <p className="text-muted text-sm mb-8">Or import from a data export:</p>
                            <div
                                className={`drop-zone${dragOver ? ' dragging' : ''}`}
                                onClick={onFilePick}
                            >
                                {dragOver && (
                                    <div className="drop-icon" style={{ color: 'var(--accent-blue)' }}>📥</div>
                                )}
                                {!dragOver && <div className="drop-icon">📂</div>}
                                <div className="drop-title">{dragOver ? 'Release to import' : 'Drop a ZIP export or click to browse'}</div>
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
        </div>
    );
}
