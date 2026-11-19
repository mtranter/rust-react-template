import type { CreateClientConfig } from "./api/client.gen";

let accessToken: () => string | undefined = () => undefined;

/** Where every request's bearer token comes from; none is sent while it gives none. */
export function sendAccessToken(source: () => string | undefined) {
  accessToken = source;
}

// Relative so the same build works behind the Vite proxy (dev) and served
// same-origin by the Rust binary (prod).
export const createClientConfig: CreateClientConfig = (config) => ({
  ...config,
  baseUrl: "/api",
  // A wrapper rather than `auth`, which the client adds only to operations declaring `security`.
  fetch: (request: Request) => {
    const token = accessToken();
    if (token) request.headers.set("Authorization", `Bearer ${token}`);
    return globalThis.fetch(request);
  },
});
