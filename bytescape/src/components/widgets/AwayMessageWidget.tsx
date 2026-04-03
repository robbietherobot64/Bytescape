/**
 * Away Message Widget (B2.4)
 *
 * PURPOSE
 * - Show a profile status + away text for visitors.
 * - In edit mode, provide controls for status, text, and visibility toggle.
 *
 * HOW IT WORKS
 * - Initializes local state from optional `config` prop.
 * - Debounces config writes and notifies parent through `onUpdate`.
 * - Persists config to profile widget payload via `get_widget_data` + `save_widget_data`.
 *
 * RELATED FILES
 * - `src/types/index.ts` (`AwayMessageConfig`, `WidgetData`)
 * - `src-tauri/src/lib.rs` (`get_widget_data`, `save_widget_data` commands)
 * - `src/screens/ProfileScreen.tsx` (widget composition/host surface)
 *
 * Cross-system note (profile/widget data boundary):
 * - This widget owns only `WidgetData.away_message` and should preserve all
 *   other widget entries during save.
 */

import { useState, useEffect } from 'react';
import { AwayMessageConfig, WidgetData } from '../../types';
import { invoke } from '@tauri-apps/api/core';

interface AwayMessageWidgetProps {
    config?: AwayMessageConfig;
    editable?: boolean;
    onUpdate?: (config: AwayMessageConfig) => void;
}

/**
 * `AwayMessageWidget`
 *
 * Exported profile widget component for viewing/editing status + away message.
 */
export function AwayMessageWidget({ config, editable = false, onUpdate }: AwayMessageWidgetProps) {
    // ===== Section: Local component state =====
    const [text, setText] = useState(config?.text || 'Currently offline. Leave me a message in the guestbook! BRB ✌️');
    const [status, setStatus] = useState<AwayMessageConfig['status']>(config?.status || 'away');
    const [enabled, setEnabled] = useState(config?.enabled ?? true);

    // ===== Section: Persistence effect =====
    // Persist changes
    useEffect(() => {
        if (!onUpdate) return;
        const timeout = setTimeout(() => {
            const newConfig: AwayMessageConfig = { text, status, enabled };
            onUpdate(newConfig);
            // Cross-system boundary: only mutate `away_message` and preserve
            // all sibling keys in shared `WidgetData`.
            // Auto-save to backend
            invoke('get_widget_data').then((raw) => {
                const data: WidgetData = raw ? JSON.parse(raw as string) : {};
                data.away_message = newConfig;
                invoke('save_widget_data', { widgetData: JSON.stringify(data) }).catch(console.error);
            }).catch(console.error);
        }, 500);
        return () => clearTimeout(timeout);
    }, [text, status, enabled, onUpdate]);

    if (!enabled && !editable) return null;

    const statusColors: Record<string, string> = {
        online: '#4caf50',
        away: '#ffc107',
        busy: '#f44336',
        offline: '#888',
    };

    const statusEmoji: Record<string, string> = {
        online: '🟢',
        away: '🌙',
        busy: '🔴',
        offline: '⚫',
    };

    // ===== Section: Render =====
    return (
        <div style={{
            background: 'var(--bg-secondary)',
            border: '2px solid var(--border-color)',
            borderRadius: 'var(--border-radius)',
            padding: 16,
            textAlign: 'center',
        } as React.CSSProperties}>
            {editable ? (
                <div>
                    <div style={{ marginBottom: 12 }}>
                        <label style={{ color: 'var(--text-secondary)', fontSize: 12, display: 'block', marginBottom: 4 } as React.CSSProperties}>
                            Status
                        </label>
                        <div style={{ display: 'flex', gap: 6, justifyContent: 'center', flexWrap: 'wrap' } as React.CSSProperties}>
                            {(['online', 'away', 'busy', 'offline'] as const).map(s => (
                                <button
                                    key={s}
                                    onClick={() => setStatus(s)}
                                    style={{
                                        background: status === s ? statusColors[s] : 'var(--bg-tertiary)',
                                        border: status === s ? `2px solid ${statusColors[s]}` : '2px solid var(--border-color)',
                                        borderRadius: 6,
                                        padding: '4px 12px',
                                        cursor: 'pointer',
                                        color: 'var(--text-primary)',
                                        fontSize: 13,
                                        opacity: status === s ? 1 : 0.6,
                                    } as React.CSSProperties}
                                >
                                    {statusEmoji[s]} {s}
                                </button>
                            ))}
                        </div>
                    </div>
                    <div style={{ marginBottom: 12 }}>
                        <label style={{ color: 'var(--text-secondary)', fontSize: 12, display: 'block', marginBottom: 4 } as React.CSSProperties}>
                            Away Message
                        </label>
                        <textarea
                            value={text}
                            onChange={e => setText(e.target.value.slice(0, 500))}
                            rows={3}
                            style={{
                                width: '100%',
                                background: 'var(--bg-tertiary)',
                                border: '1px solid var(--border-color)',
                                borderRadius: 4,
                                color: 'var(--text-primary)',
                                padding: 8,
                                resize: 'vertical',
                                fontSize: 13,
                            } as React.CSSProperties}
                        />
                        <div style={{ textAlign: 'right', fontSize: 11, color: 'var(--text-muted)' } as React.CSSProperties}>
                            {text.length}/500
                        </div>
                    </div>
                    <label style={{ display: 'flex', alignItems: 'center', gap: 6, color: 'var(--text-secondary)', fontSize: 12, marginBottom: 12 } as React.CSSProperties}>
                        <input
                            type="checkbox"
                            checked={enabled}
                            onChange={e => setEnabled(e.target.checked)}
                        />
                        Show away message on profile
                    </label>
                </div>
            ) : (
                <div>
                    <div style={{ marginBottom: 8 }}>
                        <span style={{
                            display: 'inline-block',
                            width: 10,
                            height: 10,
                            borderRadius: '50%',
                            background: statusColors[status],
                            marginRight: 6,
                        } as React.CSSProperties} />
                        <span style={{
                            fontWeight: 600,
                            color: 'var(--text-primary)',
                            textTransform: 'capitalize',
                        } as React.CSSProperties}>
                            {status}
                        </span>
                    </div>
                    <div style={{
                        fontStyle: 'italic',
                        color: 'var(--text-secondary)',
                        fontSize: 14,
                        lineHeight: 1.5,
                    } as React.CSSProperties}>
                        {text}
                    </div>
                </div>
            )}
        </div>
    );
}
