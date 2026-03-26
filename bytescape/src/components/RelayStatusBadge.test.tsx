import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { RelayStatusBadge } from './RelayStatusBadge';

// Mock the DataProvider context
const mockRelaySyncStatus = vi.fn();
vi.mock('../providers', () => ({
  useDataProvider: () => ({
    relaySyncStatus: mockRelaySyncStatus,
  }),
}));

describe('RelayStatusBadge', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows "Offline" when relay status returns offline', async () => {
    mockRelaySyncStatus.mockResolvedValue({
      status: 'offline',
      lastSync: null,
      error: 'Relay not configured',
    });
    render(<RelayStatusBadge />);
    // Initially shows checking state, then resolves
    expect(await screen.findByText('Relay: Offline')).toBeTruthy();
  });

  it('shows "Connected" when relay status returns connected', async () => {
    mockRelaySyncStatus.mockResolvedValue({
      status: 'connected',
      lastSync: Date.now() / 1000,
      error: null,
    });
    render(<RelayStatusBadge />);
    expect(await screen.findByText('Relay: Connected')).toBeTruthy();
  });

  it('shows "Syncing" when relay status returns syncing', async () => {
    mockRelaySyncStatus.mockResolvedValue({
      status: 'syncing',
      lastSync: null,
      error: null,
    });
    render(<RelayStatusBadge />);
    expect(await screen.findByText('Relay: Syncing')).toBeTruthy();
  });

  it('gracefully degrades to Offline when provider throws', async () => {
    mockRelaySyncStatus.mockRejectedValue(new Error('Command not found'));
    render(<RelayStatusBadge />);
    expect(await screen.findByText('Relay: Offline')).toBeTruthy();
  });

  it('displays tooltip with detail text', async () => {
    mockRelaySyncStatus.mockResolvedValue({
      status: 'offline',
      lastSync: null,
      error: 'Relay not configured',
    });
    render(<RelayStatusBadge />);
    const badge = await screen.findByText('Relay: Offline');
    const container = badge.closest('.relay-status-badge');
    expect(container?.getAttribute('title')).toContain('Relay: offline');
    expect(container?.getAttribute('title')).toContain('Relay not configured');
  });
});
