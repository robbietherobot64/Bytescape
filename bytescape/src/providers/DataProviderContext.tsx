/**
 * DataProviderContext — React context that injects the active DataProvider.
 *
 * Wrapping the app in <DataProviderRoot> makes the provider available to
 * every component via `useDataProvider()`.
 *
 * Desktop (Tauri):  <DataProviderRoot provider={new TauriDataProvider()}>
 * Web (REST):       <DataProviderRoot provider={new RestDataProvider()}>
 * Portal (WASM):    <DataProviderRoot provider={new WasmProvider()}>
 */

import React, { createContext, useContext } from 'react';
import type { DataProvider } from './DataProvider';
import { TauriDataProvider } from './TauriDataProvider';

/** Singleton default — avoids re-creating the provider on every render. */
const defaultProvider = new TauriDataProvider();

const DataProviderContext = createContext<DataProvider>(defaultProvider);

/**
 * Hook to access the current DataProvider from any component.
 *
 * @example
 * ```tsx
 * const provider = useDataProvider();
 * const feed = await provider.getTimelineUnified({ ... });
 * ```
 */
export function useDataProvider(): DataProvider {
  return useContext(DataProviderContext);
}

/**
 * Root wrapper that injects a DataProvider into the React tree.
 * Place this at the top of the component hierarchy (in App.tsx or main.tsx).
 */
export function DataProviderRoot({
  provider,
  children,
}: {
  provider?: DataProvider;
  children: React.ReactNode;
}) {
  return (
    <DataProviderContext.Provider value={provider ?? defaultProvider}>
      {children}
    </DataProviderContext.Provider>
  );
}
