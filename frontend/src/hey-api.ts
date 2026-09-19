import type { CreateClientConfig } from "./api/client.gen";

// Initial configuration for the generated fetch client (spec §8). "/api" is a
// same-origin relative base, so the same code works in dev (Vite proxies /api
// to the Rust server on :8080) and in prod (the SPA is embedded and served
// same-origin by the Rust binary).
export const createClientConfig: CreateClientConfig = (config) => ({
  ...config,
  baseUrl: "/api",
});
