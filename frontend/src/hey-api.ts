import type { CreateClientConfig } from "./api/client.gen";

/** How requests are authorized while a user is signed in (`Session`, `src/lib/sign-in/`). */
export type RequestAuthorization = {
  accessToken: () => string | undefined;
  /** Resolves to a fresh access token, or `undefined` when there is none to be had. */
  renew: () => Promise<string | undefined>;
  /** Leaves to sign in again, to come back to the page the user is on, unless it just did. */
  signIn: () => void;
  /** The API accepted the token. */
  accepted: () => void;
};

let authorization: RequestAuthorization | undefined;

/** With none, as in `make dev`, requests carry no token of the SPA's own. */
export function authorizeRequests(source: RequestAuthorization | undefined) {
  authorization = source;
}

function send(request: Request, token: string | undefined): Promise<Response> {
  if (token) request.headers.set("Authorization", `Bearer ${token}`);
  return globalThis.fetch(request);
}

/**
 * Sends `request` with the access token; on `401` it renews the token once and retries once, and
 * when that fails too it sends the user to sign in, answering with the `401` (which reaches the
 * screen when the user has just come back from signing in for the same reason).
 */
async function authorizedFetch(request: Request): Promise<Response> {
  const auth = authorization;
  if (!auth) return globalThis.fetch(request);
  // Before sending: a sent request's body can't be read again.
  const retry = request.clone();
  const response = await send(request, auth.accessToken());
  if (response.status !== 401) {
    auth.accepted();
    return response;
  }
  const token = await auth.renew();
  if (token) {
    const retried = await send(retry, token);
    if (retried.status !== 401) {
      auth.accepted();
      return retried;
    }
  }
  auth.signIn();
  return response;
}

// Relative so the same build works behind the Vite proxy (dev) and served
// same-origin by the Rust binary (prod).
export const createClientConfig: CreateClientConfig = (config) => ({
  ...config,
  baseUrl: "/api",
  // A wrapper rather than `auth`, which the client adds only to operations declaring `security`.
  fetch: authorizedFetch,
});
