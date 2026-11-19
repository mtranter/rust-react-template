# The SPA signs in with PKCE in the browser, not through a backend-for-frontend

Ticket 0027 guarded every operation but those in `PUBLIC`, and [ADR 0025](0025-auth-is-one-jwt-bearer-scheme.md) made a JWT bearer token the one way in. The embedded SPA had no way to get one: only the Vite proxy's dev token signed its requests, so a production build was refused everything (Spec 0002).

**The SPA is a public OIDC client using the authorization code flow with PKCE.** It reads `/auth/config` at start-up (ticket 0034): with no config (no `APP_JWT_ISSUER`, as in `make dev`) it signs no one in and the dev proxy's token stands in. With config, `oidc-client-ts`, pinned exact, does every protocol step: a visitor who isn't signed in goes straight to the provider's hosted page (the app has no sign-in screen), with PKCE, `state` and a `nonce`; `/auth/callback` redeems the code and returns to the page the visitor started on; a failure shows the provider's error and a retry, never a redirect. Tokens are kept in the tab's `sessionStorage`, so a reload stays signed in and closing the tab forgets them. `src/hey-api.ts` sends the access token as `Authorization: Bearer` on every request. Signing out forgets the tokens, signs out at the provider when it has an end-session endpoint, and ends on a page that never redirects by itself, since a live provider session would sign the user straight back in.

**The server stays as it is.** It verifies bearer tokens and keeps no sessions or cookies, so the SPA, REST clients and MCP clients all present the same kind of token.

## Considered options

- **A backend-for-frontend**: the server runs the code flow as a confidential client and holds the tokens, giving the browser an `HttpOnly` session cookie. Script in the page could no longer read a token, but the server would need a session store shared by every instance, CSRF protection, a second authentication scheme beside the bearer one (and in the Spec), and a token refresh of its own. That is the upgrade if the risk below stops being acceptable.
- **Tokens in `localStorage`.** Signed in across tabs and restarts, but a token outlives the tab, and the XSS exposure is the same or worse.
- **Tokens in memory only.** Safest from script reading storage later, but every reload goes back to the provider (silently, when its session is alive), and a deep link reloaded loses its state.
- **Our own OIDC code.** PKCE, `state`, `nonce` and ID-token checks are easy to get subtly wrong; a maintained library does them.

## Consequences

- Any script running in the page (XSS) can read the tokens in `sessionStorage`. Accepted for a single-tenant app that loads no third-party script; keep it that way, or move to a backend-for-frontend.
- One build serves any deployment: the provider is configured at runtime through `APP_JWT_ISSUER` and `APP_OIDC_*`, and the SPA is registered there as a public client (no secret) with redirect URI `<origin>/auth/callback`.
- `make dev` needs no provider; screen tests stay on MSW, with a stub of `oidc-client-ts`' `UserManager` passed to the app for sign-in tests.
- Each tab signs in on its own; a new tab goes to the provider, which signs it in silently while its own session is alive.
- Renewing tokens before they expire, and answering a `401`, are left to the SPA (ticket 0036), not the server.
