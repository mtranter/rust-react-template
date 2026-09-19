import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// Dev: Vite serves the SPA on :5173 with HMR and proxies /api to the Rust
// server on :8080 (spec §9). In prod the SPA is embedded in the binary and
// served same-origin, so the client's "/api" baseUrl works unchanged.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      "/api": "http://localhost:8080",
    },
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./vitest.setup.ts"],
  },
});
