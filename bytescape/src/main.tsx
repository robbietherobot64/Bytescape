/**
 * main.tsx — Application boot entry point.
 *
 * HOW IT WORKS:
 * 1. Grabs the #root DOM element created by index.html.
 * 2. Creates a React 18 concurrent root via `createRoot`.
 * 3. Wraps the entire component tree in `<React.StrictMode>` for dev-time
 *    double-render checks and deprecation warnings.
 * 4. Wraps `<App />` in `<DataProviderRoot>` so every descendant can call
 *    `useDataProvider()` to access Tauri IPC (or a future REST/WASM backend)
 *    without prop-drilling.
 *
 * RELATED FILES:
 * - src/App.tsx              — top-level layout & routing
 * - src/providers/index.ts   — barrel export for DataProviderRoot & friends
 * - src/providers/DataProviderContext.tsx — context definition & hook
 * - index.html               — contains the #root mount point
 */
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { DataProviderRoot } from "./providers";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <DataProviderRoot>
      <App />
    </DataProviderRoot>
  </React.StrictMode>,
);
