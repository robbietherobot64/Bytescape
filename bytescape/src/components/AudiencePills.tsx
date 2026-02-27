import { useState } from 'react';
import { SyndicationEvent } from '../types';

interface AudiencePillsProps {
    /** The post's origin platform (e.g. 'instagram', 'facebook', 'bluesky', 'dtp') */
    source: string;
    /** Original posting timestamp (epoch seconds) on the source platform */
    originalTimestamp: number;
    /** The event log of where this post has been shared to and when */
    shareHistory: SyndicationEvent[];
    /** Compact mode — icons only, no labels */
    compact?: boolean;
}

/** Config for every platform that can appear in the chain */
const PLATFORM_CONFIG: Record<string, { emoji: string; label: string; bg: string; color: string; border: string }> = {
    // Origin platforms
    instagram: { emoji: '📷', label: 'Instagram', bg: 'rgba(228,64,95,0.12)', color: '#e44060', border: 'rgba(228,64,95,0.25)' },
    facebook: { emoji: '📘', label: 'Facebook', bg: 'rgba(66,103,178,0.12)', color: '#6889c4', border: 'rgba(66,103,178,0.25)' },
    mastodon: { emoji: '🐘', label: 'Mastodon', bg: 'rgba(99,100,255,0.12)', color: '#858afa', border: 'rgba(99,100,255,0.25)' },
    dtp: { emoji: '📦', label: 'Archive', bg: 'rgba(136,136,168,0.12)', color: 'var(--text-muted)', border: 'rgba(136,136,168,0.2)' },
    rss: { emoji: '📡', label: 'RSS', bg: 'rgba(251,146,60,0.12)', color: '#fb923c', border: 'rgba(251,146,60,0.25)' },
    bytescape: { emoji: '📝', label: 'Bytescape', bg: 'rgba(168,85,247,0.12)', color: '#a855f7', border: 'rgba(168,85,247,0.25)' },

    // Syndication targets
    bluesky: { emoji: '🦋', label: 'Bluesky', bg: 'rgba(59,130,246,0.12)', color: '#60a5fa', border: 'rgba(59,130,246,0.25)' },
    p2p: { emoji: '🔗', label: 'P2P', bg: 'rgba(16,185,129,0.12)', color: '#34d399', border: 'rgba(16,185,129,0.25)' },
    activitypub: { emoji: '🐘', label: 'ActivityPub', bg: 'rgba(99,100,255,0.12)', color: '#858afa', border: 'rgba(99,100,255,0.25)' },
};

const FALLBACK = { emoji: '📝', label: 'Unknown', bg: 'rgba(136,136,168,0.12)', color: 'var(--text-muted)', border: 'rgba(136,136,168,0.2)' };

function formatTimestamp(epoch: number): string {
    if (!epoch) return '';
    const d = new Date(epoch * 1000);
    const now = new Date();
    const isToday = d.toDateString() === now.toDateString();
    const time = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    if (isToday) return `Today at ${time}`;
    return d.toLocaleDateString([], { month: 'short', day: 'numeric', year: d.getFullYear() !== now.getFullYear() ? 'numeric' : undefined }) + ` at ${time}`;
}

export function AudiencePills({ source, originalTimestamp, shareHistory, compact = false }: AudiencePillsProps) {
    const [tooltip, setTooltip] = useState<{ platform: string; x: number; y: number } | null>(null);

    // Build ordered chain: origin first, then active syndicated platforms in chronological order
    const chain: { key: string; config: typeof FALLBACK; timestamp: number }[] = [];

    // 1. Origin platform
    const srcKey = source.toLowerCase();
    chain.push({
        key: srcKey,
        config: PLATFORM_CONFIG[srcKey] || { ...FALLBACK, label: source },
        timestamp: originalTimestamp,
    });

    // 2. Syndicated platforms, sorted by timestamp (oldest first)
    const sorted = [...shareHistory]
        .filter(event => event.active)
        .map(event => ({
            key: event.platform.toLowerCase(),
            config: PLATFORM_CONFIG[event.platform.toLowerCase()] || { ...FALLBACK, label: event.platform },
            timestamp: event.timestamp || 0,
        }))
        // Don't duplicate the source
        .filter(p => p.key !== srcKey)
        .sort((a, b) => a.timestamp - b.timestamp);

    chain.push(...sorted);

    const handlePillClick = (e: React.MouseEvent, platform: string) => {
        e.stopPropagation();
        const rect = (e.target as HTMLElement).getBoundingClientRect();
        if (tooltip?.platform === platform) {
            setTooltip(null);
        } else {
            setTooltip({ platform, x: rect.left + rect.width / 2, y: rect.bottom + 4 });
        }
    };

    const activeTooltip = tooltip ? chain.find(c => c.key === tooltip.platform) : null;

    return (
        <>
            <div style={{ display: 'flex', gap: 2, alignItems: 'center', flexWrap: 'nowrap' }}>
                {chain.map((item, i) => (
                    <span key={item.key} style={{ display: 'inline-flex', alignItems: 'center', gap: 0 }}>
                        {i > 0 && (
                            <span style={{ color: 'var(--text-muted)', fontSize: '0.55rem', margin: '0 1px', opacity: 0.5 }}>›</span>
                        )}
                        <span
                            onClick={(e) => handlePillClick(e, item.key)}
                            title={`${item.config.label}${item.timestamp ? ` — ${formatTimestamp(item.timestamp)}` : ''}`}
                            style={{
                                display: 'inline-flex',
                                alignItems: 'center',
                                gap: compact ? 0 : 3,
                                padding: compact ? '1px 4px' : '2px 7px',
                                borderRadius: 999,
                                fontSize: compact ? '0.6rem' : '0.65rem',
                                fontWeight: 600,
                                background: item.config.bg,
                                color: item.config.color,
                                border: `1px solid ${item.config.border}`,
                                lineHeight: 1,
                                whiteSpace: 'nowrap',
                                cursor: 'pointer',
                                transition: 'transform 0.15s',
                            }}
                        >
                            {item.config.emoji}
                        </span>
                    </span>
                ))}
            </div>

            {/* Timestamp tooltip */}
            {activeTooltip && tooltip && (
                <div
                    style={{
                        position: 'fixed',
                        left: tooltip.x,
                        top: tooltip.y,
                        transform: 'translateX(-50%)',
                        background: 'var(--bg-elevated)',
                        border: '1px solid var(--border)',
                        borderRadius: 8,
                        padding: '6px 10px',
                        fontSize: '0.72rem',
                        color: 'var(--text-primary)',
                        zIndex: 200,
                        boxShadow: '0 4px 16px rgba(0,0,0,0.3)',
                        whiteSpace: 'nowrap',
                        pointerEvents: 'none',
                    }}
                >
                    <div style={{ fontWeight: 600, color: activeTooltip.config.color }}>
                        {activeTooltip.config.emoji} {activeTooltip.config.label}
                    </div>
                    {activeTooltip.timestamp > 0 && (
                        <div style={{ color: 'var(--text-muted)', marginTop: 2 }}>
                            {formatTimestamp(activeTooltip.timestamp)}
                        </div>
                    )}
                </div>
            )}
        </>
    );
}

export { PLATFORM_CONFIG };
