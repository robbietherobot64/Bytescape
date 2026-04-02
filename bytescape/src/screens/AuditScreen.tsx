import { useMemo, useRef, useEffect } from 'react';
import { ParseProgress, ParseResult, SidebarItem } from '../types';
import './AuditScreen.css';

/**
 * PURPOSE
 * Renders the post-parse audit/import experience as three terminal-like panels:
 * import logs, memory counts, and detected tracker summary.
 *
 * HOW IT WORKS
 * - Consumes parse progress/result and derives display totals for memories + trackers.
 * - Auto-scrolls the log terminal when new entries arrive.
 * - Exposes final actions to import parsed data and navigate after completion.
 *
 * RELATED FILES
 * - src/screens/ImportScreen.tsx (entry point where import flow is initiated)
 * - src/types/index.ts (ParseProgress, ParseResult, SidebarItem shapes)
 * - src/App.tsx (orchestrates screen routing and handler wiring)
 *
 * CROSS-SYSTEM NOTES
 * - This screen is presentation-only; state mutations are delegated to parent handlers
 *   (e.g., `onShred()`, `onNavigate()`). Backend `invoke(...)` calls and store/provider
 *   updates occur upstream and are projected into this component via props.
 */

interface AuditScreenProps {
    progress: ParseProgress | null;
    parseResult: ParseResult | null;
    logs: string[];
    isShredding: boolean;
    importComplete: boolean;
    error: string;
    onShred: () => void;
    onNavigate: (item: SidebarItem) => void;
}

/**
 * Displays import audit telemetry and completion actions.
 */
export function AuditScreen({
    progress,
    parseResult,
    logs,
    isShredding,
    importComplete,
    error,
    onShred,
    onNavigate
}: AuditScreenProps) {
    // ═══════════════════════════════════════════════════════════════════════
    // Derived state and refs
    // ═══════════════════════════════════════════════════════════════════════
    const terminalRef = useRef<HTMLDivElement>(null);
    const memoriesRef = useRef<HTMLDivElement>(null);
    const trackersRef = useRef<HTMLDivElement>(null);

    // Auto-scroll terminals
    useEffect(() => {
        if (terminalRef.current) terminalRef.current.scrollTop = terminalRef.current.scrollHeight;
    }, [logs]);

    // Group trackers
    const trackerList = useMemo(() => {
        if (!parseResult) return [];
        return [...parseResult.trackers]
            .sort((a, b) => b.event_count - a.event_count);
    }, [parseResult]);

    const totalTrackerEvents = parseResult?.total_tracker_events ?? 0;
    const totalTrackers = parseResult?.trackers.length ?? 0;
    const postsFound = progress?.posts_found ?? parseResult?.posts.length ?? 0;
    const photosFound = progress?.photos_found ?? parseResult?.total_photos ?? 0;
    const videosFound = progress?.videos_found ?? parseResult?.total_videos ?? 0;
    const messagesFound = progress?.messages_found ?? parseResult?.messages?.length ?? 0;
    const totalMemories = postsFound + photosFound + videosFound + messagesFound;

    const termStyle: React.CSSProperties = {
        background: 'var(--bg-tertiary, #0a0a0a)',
        border: '1px solid var(--border-color, #333)',
        fontFamily: '"Courier New", Courier, monospace',
        fontSize: '0.78rem',
        color: 'var(--accent-green, #33ff33)',
        padding: 0,
        display: 'flex',
        flexDirection: 'column',
        height: 220,
    };

    const termHeaderStyle: React.CSSProperties = {
        padding: '6px 10px',
        background: 'var(--bg-secondary, #1a1a1a)',
        borderBottom: '1px solid var(--border-color, #333)',
        fontWeight: 700,
        fontSize: '0.82rem',
        display: 'flex',
        justifyContent: 'space-between',
        alignItems: 'center',
    };

    const termBodyStyle: React.CSSProperties = {
        flex: 1,
        overflow: 'auto',
        padding: '8px 10px',
    };

    // ═══════════════════════════════════════════════════════════════════════
    // Render
    // ═══════════════════════════════════════════════════════════════════════
    return (
        <>
            <div className="audit-screen" style={{ padding: '0 16px 20px' } as React.CSSProperties}>
                {/* 3-Terminal Layout — Stacked rows */}
                <div style={{ display: 'flex', flexDirection: 'column', gap: 12, marginBottom: 16, maxWidth: 700, margin: '0 auto 16px' } as React.CSSProperties}>

                    {/* Header — inside max-width container to match terminal width */}
                    <div className="main-header" style={{ borderRadius: 'var(--border-radius-sm, 0)', marginBottom: 4 } as React.CSSProperties}>
                        <h1>📦 Data Import</h1>
                        <p className="text-muted text-sm">Liberating your data...</p>
                    </div>

                    {/* Terminal 1: Import Progress */}
                    <div style={termStyle}>
                        <div style={termHeaderStyle}>
                            <span>📥 IMPORT</span>
                            <span style={{ color: isShredding ? 'var(--accent-orange, #ffaa00)' : importComplete ? 'var(--accent-green, #33ff33)' : 'var(--text-muted, #888)', fontSize: '0.72rem' } as React.CSSProperties}>
                                {isShredding ? '● RUNNING' : importComplete ? '● DONE' : '● READY'}
                            </span>
                        </div>
                        <div style={termBodyStyle} ref={terminalRef}>
                            {logs.map((log, i) => (
                                <div key={i} style={{ marginBottom: 2, wordBreak: 'break-word' } as React.CSSProperties}>
                                    <span style={{ color: 'var(--text-muted, #666)', marginRight: 6 } as React.CSSProperties}>&gt;</span>
                                    {log}
                                </div>
                            ))}
                            {logs.length === 0 && (
                                <div style={{ color: 'var(--text-muted, #666)' } as React.CSSProperties}>
                                    Awaiting import...
                                    <br />
                                    <span style={{ color: 'var(--accent-green, #33ff33)' } as React.CSSProperties}>█</span>
                                </div>
                            )}
                        </div>
                    </div>

                    {/* Terminal 2: Memories Found */}
                    <div style={termStyle}>
                        <div style={termHeaderStyle}>
                            <span>💾 MEMORIES</span>
                            <span style={{ color: 'var(--accent-green, #33ff33)', fontSize: '0.72rem' } as React.CSSProperties}>
                                {totalMemories.toLocaleString()} found
                            </span>
                        </div>
                        <div style={termBodyStyle} ref={memoriesRef}>
                            <div style={{ marginBottom: 12 } as React.CSSProperties}>
                                <div style={{ display: 'flex', justifyContent: 'space-between', padding: '4px 0', borderBottom: '1px solid var(--border-color, #222)' } as React.CSSProperties}>
                                    <span>╔═ Posts</span>
                                    <span style={{ color: 'var(--text-primary, #fff)' } as React.CSSProperties}>{postsFound.toLocaleString()}</span>
                                </div>
                                <div style={{ display: 'flex', justifyContent: 'space-between', padding: '4px 0', borderBottom: '1px solid var(--border-color, #222)' } as React.CSSProperties}>
                                    <span>╠═ Photos</span>
                                    <span style={{ color: 'var(--text-primary, #fff)' } as React.CSSProperties}>{photosFound.toLocaleString()}</span>
                                </div>
                                <div style={{ display: 'flex', justifyContent: 'space-between', padding: '4px 0', borderBottom: '1px solid var(--border-color, #222)' } as React.CSSProperties}>
                                    <span>╠═ Videos</span>
                                    <span style={{ color: 'var(--text-primary, #fff)' } as React.CSSProperties}>{videosFound.toLocaleString()}</span>
                                </div>
                                <div style={{ display: 'flex', justifyContent: 'space-between', padding: '4px 0', borderBottom: '1px solid var(--border-color, #222)' } as React.CSSProperties}>
                                    <span>╚═ Messages</span>
                                    <span style={{ color: 'var(--text-primary, #fff)' } as React.CSSProperties}>{messagesFound.toLocaleString()}</span>
                                </div>
                            </div>
                            {parseResult && (
                                <div style={{ color: 'var(--text-muted, #888)', fontSize: '0.72rem' } as React.CSSProperties}>
                                    Source: {parseResult.source}
                                    <br />
                                    All items will be saved to your local vault.
                                </div>
                            )}
                        </div>
                    </div>

                    {/* Terminal 3: Trackers Detected */}
                    <div style={termStyle}>
                        <div style={termHeaderStyle}>
                            <span>🛡️ TRACKERS</span>
                            <span style={{ color: totalTrackers > 0 ? 'var(--accent-red, #ff6b6b)' : 'var(--text-muted, #888)', fontSize: '0.72rem' } as React.CSSProperties}>
                                {totalTrackers.toLocaleString()} detected
                            </span>
                        </div>
                        <div style={termBodyStyle} ref={trackersRef}>
                            {trackerList.length > 0 ? (
                                <>
                                    <div style={{ marginBottom: 8, color: 'var(--accent-red, #ff6b6b)', fontSize: '0.72rem' } as React.CSSProperties}>
                                        {totalTrackerEvents.toLocaleString()} tracking events from {totalTrackers.toLocaleString()} trackers
                                    </div>
                                    {trackerList.slice(0, 40).map((t, i) => (
                                        <div
                                            key={t.name}
                                            style={{
                                                display: 'flex',
                                                justifyContent: 'space-between',
                                                padding: '2px 0',
                                                borderBottom: '1px solid var(--bg-tertiary, #1a1a1a)',
                                                color: 'var(--accent-orange, #cc8833)',
                                                animationDelay: `${i * 0.03}s`,
                                            } as React.CSSProperties}
                                        >
                                            <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', maxWidth: '70%' } as React.CSSProperties}>
                                                {t.name}
                                            </span>
                                            <span style={{ color: 'var(--accent-red, #ff6b6b)', flexShrink: 0 } as React.CSSProperties}>
                                                {t.event_count.toLocaleString()}
                                            </span>
                                        </div>
                                    ))}
                                    {trackerList.length > 40 && (
                                        <div style={{ color: 'var(--text-muted, #666)', marginTop: 4 } as React.CSSProperties}>
                                            ... +{(trackerList.length - 40).toLocaleString()} more
                                        </div>
                                    )}
                                </>
                            ) : (
                                <div style={{ color: 'var(--text-muted, #666)' } as React.CSSProperties}>
                                    {progress ? 'Scanning for trackers...' : 'No trackers detected yet.'}
                                </div>
                            )}
                        </div>
                    </div>

                {/* Import Button */}
                {parseResult && !importComplete && (
                    <div style={{ textAlign: 'center', padding: '16px 0' } as React.CSSProperties}>
                        <button
                            className="btn btn-primary"
                            onClick={onShred}
                            disabled={isShredding}
                            style={{ padding: '12px 32px', fontSize: '1rem' } as React.CSSProperties}
                        >
                            {isShredding ? '📦 Importing & Protecting...' : '📦 Import & Protect'}
                        </button>
                        <p className="text-muted text-sm" style={{ marginTop: 8 } as React.CSSProperties}>
                            {(parseResult.posts.length + (parseResult.messages?.length || 0)).toLocaleString()} memories will be saved.
                            {totalTrackers > 0 && ` ${totalTrackerEvents.toLocaleString()} tracking events will be excluded.`}
                        </p>
                    </div>
                )}

                {/* Success */}
                {importComplete && (
                    <div className="card card-glow" style={{ textAlign: 'center' } as React.CSSProperties}>
                        <h2>✅ Import Complete</h2>
                        <p className="text-muted" style={{ marginBottom: 16 } as React.CSSProperties}>
                            Your memories are protected. Your data is yours.
                        </p>
                        <div style={{ display: 'flex', justifyContent: 'center', gap: 12 } as React.CSSProperties}>
                            <button className="btn btn-primary" onClick={() => onNavigate('feed')}>
                                View Feed
                            </button>
                            <button className="btn btn-ghost" onClick={() => onNavigate('import')}>
                                Import More
                            </button>
                        </div>
                    </div>
                )}

                {error && (
                    <div className="card" style={{ borderColor: 'var(--accent-red)' } as React.CSSProperties}>
                        <p style={{ color: 'var(--accent-red)' } as React.CSSProperties}>{error}</p>
                    </div>
                )}
                </div>
            </div>
        </>
    );
}
