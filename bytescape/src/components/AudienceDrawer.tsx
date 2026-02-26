import { useState } from 'react';
import { createPortal } from 'react-dom';
import { invoke } from '@tauri-apps/api/core';
import { Audience } from '../types';
import { PLATFORM_CONFIG } from './AudiencePills';

/** Map Audience enum values to platform config keys */
function platformKey(a: Audience): string {
    return a.toLowerCase();
}

interface AudienceDrawerProps {
    itemId: string;
    currentAudiences: Audience[];
    syndicatedUris: Record<string, string>;
    onClose: () => void;
    onUpdated: (newAudiences: Audience[]) => void;
}

/** Available networks the user can expand to */
const EXPANDABLE_AUDIENCES: { id: Audience; available: boolean; disabledReason?: string }[] = [
    { id: 'Bluesky', available: true },
    { id: 'P2p', available: true },
    { id: 'Activitypub', available: false, disabledReason: 'Coming soon' },
];

const FALLBACK = { emoji: '📝', label: 'Unknown', bg: 'rgba(136,136,168,0.12)', color: 'var(--text-muted)', border: 'rgba(136,136,168,0.2)' };

export function AudienceDrawer({ itemId, currentAudiences, syndicatedUris, onClose, onUpdated }: AudienceDrawerProps) {
    const [loading, setLoading] = useState<Audience | null>(null);
    const [error, setError] = useState('');
    const [successMsg, setSuccessMsg] = useState('');

    const handleExpand = async (target: Audience) => {
        if (currentAudiences.includes(target)) return;
        setLoading(target);
        setError('');
        setSuccessMsg('');

        try {
            if (target === 'Bluesky') {
                await invoke<string>('syndicate_vault_item', { itemId });
                const updated = [...currentAudiences, 'Bluesky' as Audience];
                onUpdated(updated);
                setSuccessMsg(`Shared to Bluesky`);
            } else if (target === 'P2p') {
                setSuccessMsg('P2P sharing will be available soon');
            }
        } catch (e: any) {
            setError(typeof e === 'string' ? e : 'Failed to share');
        } finally {
            setLoading(null);
        }
    };

    return createPortal(
        <div
            className="modal-overlay"
            onClick={onClose}
            style={{ alignItems: 'flex-end' }}
        >
            <div
                onClick={e => e.stopPropagation()}
                style={{
                    width: '100%',
                    maxWidth: 480,
                    background: 'var(--bg-card)',
                    border: '1px solid var(--border)',
                    borderRadius: '16px 16px 0 0',
                    padding: 0,
                    boxShadow: '0 -8px 40px rgba(0,0,0,0.4)',
                    animation: 'slideUp 0.25s ease-out',
                }}
            >
                {/* Header */}
                <div style={{
                    padding: '16px 20px',
                    borderBottom: '1px solid var(--border)',
                    display: 'flex',
                    justifyContent: 'space-between',
                    alignItems: 'center',
                }}>
                    <h3 style={{ margin: 0, fontSize: '1rem', fontWeight: 700 }}>📤 Share this post</h3>
                    <button
                        className="btn btn-ghost btn-sm"
                        onClick={onClose}
                        style={{ padding: '0 8px', fontSize: '1rem' }}
                    >✕</button>
                </div>

                {/* Current audiences */}
                <div style={{ padding: '14px 20px', borderBottom: '1px solid var(--border)' }}>
                    <div style={{ fontSize: '0.72rem', fontWeight: 600, color: 'var(--text-muted)', marginBottom: 8, textTransform: 'uppercase', letterSpacing: '0.5px' }}>
                        Currently shared on
                    </div>
                    <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
                        {currentAudiences.filter(a => a !== 'Local').map(a => {
                            const key = platformKey(a);
                            const c = PLATFORM_CONFIG[key] || FALLBACK;
                            const uri = syndicatedUris[key];
                            return (
                                <div key={a} style={{
                                    display: 'flex',
                                    alignItems: 'center',
                                    gap: 6,
                                    padding: '6px 12px',
                                    borderRadius: 8,
                                    background: c.bg,
                                    border: `1px solid ${c.border}`,
                                    fontSize: '0.82rem',
                                    fontWeight: 600,
                                    color: c.color,
                                }}>
                                    <span>{c.emoji}</span>
                                    <span>{c.label}</span>
                                    <span style={{ color: 'var(--accent-green)', fontSize: '0.75rem' }}>✓</span>
                                    {uri && (
                                        <a href={uri} target="_blank" rel="noopener noreferrer"
                                            style={{ fontSize: '0.7rem', color: c.color, opacity: 0.7 }}
                                            title="View on platform"
                                        >🔗</a>
                                    )}
                                </div>
                            );
                        })}
                    </div>
                </div>

                {/* Expandable audiences */}
                <div style={{ padding: '14px 20px 20px' }}>
                    <div style={{ fontSize: '0.72rem', fontWeight: 600, color: 'var(--text-muted)', marginBottom: 8, textTransform: 'uppercase', letterSpacing: '0.5px' }}>
                        Expand to
                    </div>
                    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                        {EXPANDABLE_AUDIENCES.map(({ id, available, disabledReason }) => {
                            const key = platformKey(id);
                            const c = PLATFORM_CONFIG[key] || FALLBACK;
                            const alreadyShared = currentAudiences.includes(id);
                            const isLoading = loading === id;

                            return (
                                <div key={id} style={{
                                    display: 'flex',
                                    alignItems: 'center',
                                    justifyContent: 'space-between',
                                    padding: '10px 14px',
                                    borderRadius: 10,
                                    background: alreadyShared ? c.bg : 'rgba(255,255,255,0.03)',
                                    border: `1px solid ${alreadyShared ? c.border : 'var(--border)'}`,
                                    opacity: (!available && !alreadyShared) ? 0.5 : 1,
                                }}>
                                    <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                                        <span style={{ fontSize: '1.1rem' }}>{c.emoji}</span>
                                        <div>
                                            <div style={{ fontSize: '0.88rem', fontWeight: 600, color: 'var(--text-primary)' }}>{c.label}</div>
                                            {disabledReason && !alreadyShared && (
                                                <div style={{ fontSize: '0.72rem', color: 'var(--text-muted)' }}>{disabledReason}</div>
                                            )}
                                        </div>
                                    </div>
                                    {alreadyShared ? (
                                        <span style={{ color: 'var(--accent-green)', fontSize: '0.82rem', fontWeight: 600 }}>✓ Shared</span>
                                    ) : available ? (
                                        <button
                                            className="btn btn-sm"
                                            onClick={() => handleExpand(id)}
                                            disabled={isLoading}
                                            style={{
                                                background: c.bg,
                                                color: c.color,
                                                border: `1px solid ${c.border}`,
                                                borderRadius: 8,
                                                padding: '5px 14px',
                                                fontSize: '0.78rem',
                                                fontWeight: 600,
                                                cursor: 'pointer',
                                            }}
                                        >
                                            {isLoading ? '...' : 'Share'}
                                        </button>
                                    ) : null}
                                </div>
                            );
                        })}
                    </div>

                    {/* Feedback */}
                    {error && (
                        <div style={{ marginTop: 10, padding: '8px 12px', borderRadius: 6, background: 'rgba(239,68,68,0.1)', color: '#f87171', fontSize: '0.82rem' }}>
                            {error}
                        </div>
                    )}
                    {successMsg && (
                        <div style={{ marginTop: 10, padding: '8px 12px', borderRadius: 6, background: 'rgba(34,197,94,0.1)', color: '#34d399', fontSize: '0.82rem' }}>
                            ✅ {successMsg}
                        </div>
                    )}
                </div>
            </div>
        </div>,
        document.body
    );
}
