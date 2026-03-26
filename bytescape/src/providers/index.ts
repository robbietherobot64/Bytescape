/**
 * Barrel export for the providers module.
 *
 * Usage:
 *   import { useDataProvider, DataProviderRoot, TauriDataProvider } from './providers';
 *   import type { DataProvider, RelayStatus, RelaySyncStatus } from './providers';
 */

export type { DataProvider, RelayStatus, RelaySyncStatus } from './DataProvider';
export { TauriDataProvider } from './TauriDataProvider';
export { useDataProvider, DataProviderRoot } from './DataProviderContext';

// Re-export commonly used param/result types
export type {
  UnifiedFeedResult,
  PagedVaultResult,
  SyncDocResult,
  ExportVaultDataResult,
} from './DataProvider';
