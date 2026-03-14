/**
 * Away Message Widget (B2.4)
 *
 * Displays an away/status message with configurable status (online/away/busy/offline).
 * In editable mode, provides controls to change status, message text, and enable/disable.
 */

import { useState, useEffect } from 'react';
import { AwayMessageConfig, WidgetData } from '../../types';
import { invoke } from '@tauri-apps/api/core';

interface AwayMessageWidgetProps {
    config?: AwayMessageConfig;
    editable?: boolean;
    onUpdate?: (config: AwayMessageConfig) => void;
}

export function AwayMessageWidget({ config, editable = false, onUpdate }: AwayMessageWidgetProps) {
    const [text, setText] = useState(config?.text || 'Currently offline. Leave me a message in the guestbook! BRB ✌️');
    const [status, setStatus] = useState<AwayMessageConfig['status']>(config?.status || 'away');
    const [enabled, setEnabled] = useState(config?.enabled ?? true);

    // Persist changes
    useEffect(() => {
        if (!onUpdate) return;
        const timeout = setTimeout(() => {
            const newConfig: AwayMessageConfig = { text, status, enabled };
            onUpdate(newConfig);
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
