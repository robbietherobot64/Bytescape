import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";
import "react-grid-layout/css/styles.css";

// Global error handler for React rendering crashes
window.addEventListener('error', (e) => {
  console.error('[BYTESCAPE ERROR]', e.error || e.message);
});

ReactDOM.createRoot(document.getElementById("root")!).render(<App />);
