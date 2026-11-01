import type { CreateClientConfig } from "./api/client.gen";

// Relative so the same build works behind the Vite proxy (dev) and served
// same-origin by the Rust binary (prod).
export const createClientConfig: CreateClientConfig = (config) => ({
  ...config,
  baseUrl: "/api",
});
