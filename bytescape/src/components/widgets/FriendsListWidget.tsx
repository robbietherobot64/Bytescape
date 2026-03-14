/**
 * Friends List Widget (B2.4)
 *
 * Shows the user's P2P peers with online status indicators.
 * Reads from the peers list via Tauri and the peer store for live status.
 */

import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { usePeerStore } from '../../store';
import { BsPersonFill, BsCircleFill } from 'react-icons/bs';

interface PeerInfo {
    node_id: string;
    name?: string;
    last_seen?: number;
}

interface FriendsListWidgetProps {
    editable?: boolean;
}

export function FriendsListWidget({ editable: _editable = false }: FriendsListWidgetProps) {
    const [peers, setPeers] = useState<PeerInfo[]>([]);
    const peerStatuses = usePeerStore(s => s.peerStatuses);

    useEffect(() => {
        invoke<PeerInfo[]>('list_peers')
            .then(setPeers)
            .catch(() => {});
    }, []);

    const isPeerOnline = (nodeId: string) => peerStatuses[nodeId] === 'online';

    return (
        <div style={{
            background: 'var(--bg-secondary)',
            border: '2px solid var(--border-color)',
            borderRadius: 'var(--border-radius)',
            padding: 12,
        } as React.CSSProperties}>
            <div style={{
                fontSize: 13,
                fontWeight: 600,
                color: 'var(--text-primary)',
                marginBottom: 8,
                borderBottom: '1px solid var(--border-color)',
                paddingBottom: 6,
            } as React.CSSProperties}>
                👥 Friends ({peers.length})
            </div>

            {peers.length === 0 ? (
                <div style={{ color: 'var(--text-muted)', fontSize: 12, textAlign: 'center', padding: 16 } as React.CSSProperties}>
                    No peers added yet. Add friends from the Friends screen!
                </div>
            ) : (
                <div style={{ maxHeight: 200, overflowY: 'auto' } as React.CSSProperties}>
                    {peers.map(peer => {
                        const online = isPeerOnline(peer.node_id);
                        return (
                            <div key={peer.node_id} style={{
                                display: 'flex',
                                alignItems: 'center',
                                gap: 8,
                                padding: '6px 4px',
                                borderRadius: 4,
                            } as React.CSSProperties}>
                                <div style={{
                                    width: 28,
                                    height: 28,
                                    borderRadius: '50%',
                                    background: 'var(--bg-tertiary)',
                                    display: 'flex',
                                    alignItems: 'center',
                                    justifyContent: 'center',
                                    flexShrink: 0,
                                } as React.CSSProperties}>
                                    <BsPersonFill size={14} color="var(--text-secondary)" />
                                </div>
                                <div style={{ flex: 1, minWidth: 0 }}>
                                    <div style={{
                                        fontSize: 13,
                                        color: 'var(--text-primary)',
                                        overflow: 'hidden',
                                        textOverflow: 'ellipsis',
                                        whiteSpace: 'nowrap',
                                    } as React.CSSProperties}>
                                        {peer.name || peer.node_id.slice(0, 12) + '…'}
                                    </div>
                                </div>
                                <BsCircleFill
                                    size={8}
                                    color={online ? '#4caf50' : '#888'}
                                    title={online ? 'Online' : 'Offline'}
                                />
                            </div>
                        );
                    })}
                </div>
            )}
        </div>
    );
}
