import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  build: {
    modulePreload: false,
    chunkSizeWarningLimit: 900,
    rollupOptions: {
      output: {
        manualChunks: {
          vendor: ['react', 'react-dom'],
          'matrix-wasm': ['matrix-js-sdk'],
        },
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    proxy: {
      "/uploads": "http://localhost:8080",
      "/api": "http://localhost:8080",
    },
  },
});
