import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  build: {
    modulePreload: false,
  },
  server: {
    port: 1421,
    strictPort: true,
    proxy: {
      "/uploads": "http://localhost:8080",
      "/api": "http://localhost:8080",
    },
  },
});
