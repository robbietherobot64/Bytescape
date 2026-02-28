import { useRef, useEffect } from 'react';
import { ParseProgress, ParseResult, SidebarItem } from '../types';

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
    const terminalRef = useRef<HTMLDivElement>(null);

    // Auto-scroll terminal
    useEffect(() => {
        if (terminalRef.current) {
            terminalRef.current.scrollTop = terminalRef.current.scrollHeight;
        }
    }, [logs]);

    return (
        <>
            <div className="main-header">
                <h1>🔍 Data Audit</h1>
                <p className="text-muted text-sm">Scanning your archive...</p>
            </div>

            <div className="audit-screen">
                <div className="audit-columns">
                    {/* Left: Memories */}
                    <div className="audit-column">
                        <h3>💾 Memories</h3>
                        <div className="memories-counter">
                            <div className="counter-row">
                                <span className="label">Posts</span>
                                <span className="count">{progress?.posts_found ?? parseResult?.posts.length ?? 0}</span>
                            </div>
                            <div className="counter-row">
                                <span className="label">Photos</span>
                                <span className="count">{progress?.photos_found ?? parseResult?.total_photos ?? 0}</span>
                            </div>
                            <div className="counter-row">
                                <span className="label">Videos</span>
                                <span className="count">{progress?.videos_found ?? parseResult?.total_videos ?? 0}</span>
                            </div>
                            <div className="counter-row">
                                <span className="label">Messages</span>
                                <span className="count">{progress?.messages_found ?? parseResult?.messages?.length ?? 0}</span>
                            </div>
                        </div>
                    </div>

                    {/* Right: Surveillance */}
                    <div className="audit-column">
                        <h3>🕵️ Surveillance</h3>
                        <div className={`tracker-list ${isShredding ? "shredding" : ""}`}>
                            {parseResult ? (
                                <>
                                    {parseResult.trackers.slice(0, 100).map((t, i) => (
                                        <div
                                            key={t.name}
                                            className="tracker-item"
                                            style={{ animationDelay: `${Math.min(i, 50) * 0.05}s` }}
                                        >
                                            <span className="tracker-name">{t.name}</span>
                                            <span className="tracker-events">{t.event_count.toLocaleString()} events</span>
                                        </div>
                                    ))}
                                    {parseResult.trackers.length > 100 && (
                                        <div className="text-muted text-sm" style={{ padding: 12, textAlign: 'center' }}>
                                            ...and {(parseResult.trackers.length - 100).toLocaleString()} more trackers
                                        </div>
                                    )}
                                </>
                            ) : (
                                <div className="text-muted text-sm" style={{ padding: 12 }}>
                                    {progress ? "Scanning..." : "Waiting for import..."}
                                </div>
                            )}
                        </div>
                    </div>
                </div>

                {/* Terminal */}
                <div className="terminal" ref={terminalRef}>
                    {logs.map((log, i) => (
                        <div key={i} className="log-line">{log}</div>
                    ))}
                </div>

                {/* Shred Button */}
                {parseResult && !importComplete && (
                    <div className="shred-section">
                        <button
                            className="btn btn-danger shred-btn"
                            onClick={onShred}
                            disabled={isShredding}
                        >
                            {isShredding ? "🔥 Shredding..." : "🗑️ Shred Surveillance & Save Memories"}
                        </button>
                        <p className="text-muted text-sm mt-8">
                            This will permanently discard {parseResult.total_tracker_events.toLocaleString()} tracker events
                            from {parseResult.trackers.length.toLocaleString()} trackers
                            and save {(parseResult.posts.length + (parseResult.messages?.length || 0)).toLocaleString()} items to your local vault.
                        </p>
                    </div>
                )}

                {/* Success */}
                {importComplete && (
                    <div className="card card-glow" style={{ textAlign: "center" }}>
                        <h2>✅ Exorcism Complete</h2>
                        <p className="text-muted" style={{ marginBottom: 16 }}>
                            Your memories are safe. The surveillance is gone.
                        </p>
                        <div className="row" style={{ justifyContent: "center", gap: 12 }}>
                            <button className="btn btn-primary" onClick={() => onNavigate("feed")}>
                                View Feed
                            </button>
                            <button className="btn btn-ghost" onClick={() => onNavigate("import")}>
                                Import More
                            </button>
                        </div>
                    </div>
                )}

                {error && (
                    <div className="card" style={{ borderColor: "var(--accent-red)" }}>
                        <p style={{ color: "var(--accent-red)" }}>{error}</p>
                    </div>
                )}
            </div>
        </>
    );
}
