import { Audience } from '../types';

interface AudiencePillsProps {
    audiences: Audience[];
    compact?: boolean;
}

const AUDIENCE_CONFIG: Record<Audience, { emoji: string; label: string; bg: string; color: string; border: string }> = {
    Local: { emoji: '🔒', label: 'Local', bg: 'rgba(136,136,168,0.12)', color: 'var(--text-muted)', border: 'rgba(136,136,168,0.2)' },
    Bluesky: { emoji: '🦋', label: 'Bluesky', bg: 'rgba(59,130,246,0.12)', color: '#60a5fa', border: 'rgba(59,130,246,0.25)' },
    P2p: { emoji: '🌐', label: 'P2P', bg: 'rgba(16,185,129,0.12)', color: '#34d399', border: 'rgba(16,185,129,0.25)' },
    Activitypub: { emoji: '🐘', label: 'Mastodon', bg: 'rgba(99,100,255,0.12)', color: '#858afa', border: 'rgba(99,100,255,0.25)' },
};

export function AudiencePills({ audiences, compact = false }: AudiencePillsProps) {
    if (!audiences || audiences.length === 0) return null;

    return (
        <div style={{ display: 'flex', gap: 4, alignItems: 'center', flexWrap: 'wrap' }}>
            {audiences.map(a => {
                const c = AUDIENCE_CONFIG[a];
                if (!c) return null;
                return (
                    <span
                        key={a}
                        title={c.label}
                        style={{
                            display: 'inline-flex',
                            alignItems: 'center',
                            gap: compact ? 0 : 3,
                            padding: compact ? '1px 4px' : '2px 7px',
                            borderRadius: 999,
                            fontSize: compact ? '0.6rem' : '0.65rem',
                            fontWeight: 600,
                            background: c.bg,
                            color: c.color,
                            border: `1px solid ${c.border}`,
                            lineHeight: 1,
                            whiteSpace: 'nowrap',
                        }}
                    >
                        {c.emoji}{!compact && ` ${c.label}`}
                    </span>
                );
            })}
        </div>
    );
}

export { AUDIENCE_CONFIG };
