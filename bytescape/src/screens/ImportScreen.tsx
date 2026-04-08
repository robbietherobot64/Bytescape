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
    // Manual navigation handler (no auto-redirect)
    // ═══════════════════════════════════════════════════════════════════════
    const importComplete = importStatus?.startsWith('✅');
    const handleViewPosts = useCallback(() => {
        onNavigate('profile');
    }, [onNavigate]);

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

    return (
        <div className="screen-centered">
            <div className="main-header">
                <h1 style={{ textAlign: 'center', width: '100%' } as React.CSSProperties}>Import Archive</h1>
            </div>

            <div className="import-tools">
                {/* ─── Active import: show terminal animation ─── */}
                {isImporting && (
                    <div className="import-terminal" style={{ marginBottom: 16 } as React.CSSProperties}>
                        <div className="import-terminal-body" ref={terminalRef}>
                            {terminalLog.map((line, i) => (
                                <div key={i} className="import-terminal-line">
                                    {line}
                                </div>
                            ))}
                            <div className="import-terminal-line import-terminal-cursor">
                                {'>'} <span className="import-terminal-blink">█</span>
                            </div>
                        </div>
                        <div className="import-terminal-progress">
                            <div className="import-terminal-progress-bar" />
                        </div>
                    </div>
                )}

                {/* ─── Import complete banner ─── */}
                {importComplete && !isImporting && (
                    <div className="card" style={{ marginBottom: 16, textAlign: 'center', background: 'rgba(16,185,129,0.1)', border: '1px solid rgba(16,185,129,0.3)' } as React.CSSProperties}>
                        <p style={{ color: '#34d399', fontWeight: 600, margin: '4px 0' } as React.CSSProperties}>
                            {importStatus}
                        </p>
                        <button
                            className="btn btn-sm btn-primary"
                            onClick={handleViewPosts}
                            style={{ marginTop: 8 } as React.CSSProperties}
                        >
                            📋 View Imported Posts
                        </button>
                    </div>
                )}

                {/* ─── Import actions (always visible) ─── */}
                <div className="card card-glow">
                    <h2>🔌 Import Your Social History</h2>
                    <p className="text-muted text-sm" style={{ marginBottom: 16 }}>
                        Pull your posts into your private vault. Everything stays on your device.
                    </p>

                    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
                        {/* Bluesky — instant (requires linked Bluesky account) */}
                        <button
                            className="btn btn-block import-btn-primary"
                            onClick={onImportBluesky}
                            disabled={isImporting || !isBlueskyLinked}
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
                                onClick={isImporting ? undefined : onFilePick}
                                style={isImporting ? { opacity: 0.5, pointerEvents: 'none' } as React.CSSProperties : undefined}
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
