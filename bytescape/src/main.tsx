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
