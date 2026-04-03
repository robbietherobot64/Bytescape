/**
 * RelayStatusBadge
 *
 * Compact connectivity indicator for relay synchronization state.
 *
 * Cross-system note:
 * - Pulls status through the data provider abstraction, which bridges to engine-facing
 *   command implementations (Tauri / backend adapter).
 */
import { useState, useEffect } from 'react';
import { useDataProvider } from '../providers';
import type { RelayStatus } from '../providers';

// ============================================================
// Status Display Configuration
// ============================================================

const STATUS_CONFIG: Record<RelayStatus, { icon: string; label: string; color: string }> = {
  connected: { icon: '🟢', label: 'Connected', color: 'var(--accent-green, #22c55e)' },
  syncing:   { icon: '🟡', label: 'Syncing',   color: 'var(--accent-yellow, #eab308)' },
  offline:   { icon: '⚫', label: 'Offline',    color: 'var(--text-muted, #888)' },
};

/**
 * Exported badge used in persistent chrome regions (e.g., sidebar footer).
 *
 * Small status badge showing relay connectivity.
 * Placed in the sidebar footer — visible but unobtrusive.
 *
 * Initially always shows "Offline" until the relay backend is deployed.
 */
export function RelayStatusBadge() {
  // Provider resolves platform-specific transport/state details.
  const provider = useDataProvider();

  // UI state reflects the latest async relay status snapshot.
  const [status, setStatus] = useState<RelayStatus>('offline');
  const [detail, setDetail] = useState<string>('Checking…');

  // One-shot fetch on provider change; guarded for unmount race safety.
  useEffect(() => {
    let cancelled = false;
    provider.relaySyncStatus().then(result => {
      if (cancelled) return;
      setStatus(result.status);
      setDetail(
        result.error
          ? `Relay: ${result.status} — ${result.error}`
          : `Relay: ${result.status}${result.lastSync ? ` (last sync ${new Date(result.lastSync * 1000).toLocaleTimeString()})` : ''}`
      );
    }).catch(() => {
      if (cancelled) return;
      setStatus('offline');
      setDetail('Relay: offline — server not configured');
    });
    return () => { cancelled = true; };
  }, [provider]);

  // View model lookup for icon/label/color.
  const cfg = STATUS_CONFIG[status];

  return (
    <div
      className="relay-status-badge"
      title={detail}
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 6,
        padding: '4px 8px',
        fontSize: '0.72rem',
        color: cfg.color,
        opacity: 0.8,
        cursor: 'default',
        userSelect: 'none',
      } as React.CSSProperties}
    >
      <span>{cfg.icon}</span>
      <span>Relay: {cfg.label}</span>
    </div>
  );
}
