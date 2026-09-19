import { defineConfig } from "@hey-api/openapi-ts";

// Reads the checked-in spec snapshot (no running server needed) and generates
// the typed fetch client + TanStack Query artifacts into src/api/ (spec §8).
// src/api/ is a build output — never hand-edited.
export default defineConfig({
  input: "./openapi.json",
  output: "./src/api",
  plugins: [
    {
      name: "@hey-api/client-fetch",
      // Client baseUrl "/api": identical code works in dev (Vite-proxied) and
      // prod (same-origin embedded SPA) — spec §8.
      runtimeConfigPath: "./src/hey-api.ts",
    },
    "@tanstack/react-query",
  ],
});
