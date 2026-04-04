/**
 * Barrel export for the providers module.
 *
 * Usage:
 *   import { useDataProvider, DataProviderRoot, TauriDataProvider } from './providers';
 *   import type { DataProvider, RelayStatus, RelaySyncStatus } from './providers';
 *
 * RELATED FILES:
 * - src/providers/DataProvider.ts        — DataProvider interface & type definitions
 * - src/providers/TauriDataProvider.ts   — Tauri IPC implementation of DataProvider
 * - src/providers/DataProviderContext.tsx — React context, useDataProvider hook, DataProviderRoot
 * - src/main.tsx                         — mounts <DataProviderRoot> at the tree root
 */

export type { DataProvider, RelayStatus, RelaySyncStatus } from './DataProvider';
export { TauriDataProvider } from './TauriDataProvider';
export { useDataProvider, DataProviderRoot } from './DataProviderContext';

// Re-export commonly used param/result types
export type {
  PagedVaultResult,
  SyncDocResult,
  ExportVaultDataResult,
} from './DataProvider';
