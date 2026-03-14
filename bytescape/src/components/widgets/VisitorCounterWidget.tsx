/**
 * Visitor Counter Widget (B2.4)
 *
 * A retro-styled visitor counter using the vault stats total as a base.
 * Displays the count in a classic LED-style font.
 */

import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface VisitorCounterWidgetProps {
    count?: number;
}

interface VaultStatsResult {
    total: number;
}

export function VisitorCounterWidget({ count: overrideCount }: VisitorCounterWidgetProps) {
    const [count, setCount] = useState(overrideCount || 0);

    useEffect(() => {
        if (overrideCount !== undefined) return;
        invoke<VaultStatsResult>('get_vault_stats')
            .then(stats => setCount(stats.total + 1337))
            .catch(() => setCount(1337));
    }, [overrideCount]);

    const display = String(count).padStart(7, '0');

    return (
        <div style={{
            background: '#111',
            border: '2px solid var(--border-color)',
            borderRadius: 'var(--border-radius)',
            padding: '8px 16px',
            textAlign: 'center',
            display: 'inline-block',
        } as React.CSSProperties}>
            <div style={{ color: 'var(--text-muted)', fontSize: 10, marginBottom: 4, textTransform: 'uppercase', letterSpacing: 1 } as React.CSSProperties}>
                Visitors
            </div>
            <div style={{
                fontFamily: "'Courier New', monospace",
                fontSize: 24,
                fontWeight: 700,
                color: '#0f0',
                textShadow: '0 0 6px #0f04',
                letterSpacing: 3,
            } as React.CSSProperties}>
                {display}
            </div>
        </div>
    );
}
