---
status: done
---

# The embedded SPA signs users in with the identity provider and sends their token

## Problem

Ticket 0027 guarded every operation not listed in `PUBLIC`, but the SPA never got a way to sign in. In `make dev`, the Vite proxy adds a token for the seeded user Alice to every request (`devToken`, `frontend/vite.config.ts`). A production build sends no `Authorization` header and has no sign-in screen, so the binary's own SPA gets `401` from every guarded operation. As shipped, the app doesn't work.

No check could catch it:

- screen tests answer from MSW;
- journeys mint their own tokens in Rust;
- the dev server injects a token;
- nothing runs the shipped SPA against the real API.

## Solution overview

The SPA signs in with the identity provider configured as `APP_JWT_ISSUER`, using the OIDC authorization code flow with PKCE, as a public client. It attaches the access token to every API request.

- The server tells the SPA how to reach the provider at runtime, so one build works for any deployment.
- `make dev` is unchanged: no provider is needed, and the proxy still signs requests in as Alice.
- A new end-to-end check signs in to a production-mode build through a fake OIDC provider and loads a guarded screen. It runs in `make check` and CI.

The SPA keeps the one bearer scheme (ADR 0025). The server keeps no sessions or cookies of its own, so REST, MCP and the SPA all present the same kind of token.

## Behaviour

### Configuration

- `APP_OIDC_CLIENT_ID` is the SPA's public client id at the provider.
- `APP_OIDC_SCOPES` is extra scopes to request, beyond `openid profile email`.
- `APP_OIDC_EXTRA_PARAMS` is extra authorization request parameters, such as Auth0's `audience`.
- The binary serves the SPA's settings at `GET /auth/config`: issuer, client id, scopes and extra parameters.
  - It's outside `/api`, like `/health`, so it isn't an operation, a Spec entry or an MCP tool.
  - It returns `404` when there's no issuer.
- In production with the SPA embedded, the server refuses to start when `APP_JWT_ISSUER` is set but `APP_OIDC_CLIENT_ID` isn't, naming both. This matches the dev-secret refusal in `crates/server/src/config.rs`.
- `.env.example` documents the new settings, and how to register the SPA as a public client with redirect URI `<origin>/auth/callback`.

### Signing in

- A visitor who isn't signed in is redirected straight to the provider, with PKCE, `state` and `nonce`. The app has no sign-in screen of its own: the provider's hosted page asks for credentials.
- `/auth/callback` completes the sign-in and returns to the page the user started from (the deep link is kept). A failed or cancelled sign-in shows an error page with the provider's error and a retry button. It doesn't loop.
- The token handling uses `oidc-client-ts`, pinned exact. The SPA doesn't implement OIDC itself.
- Tokens live in `sessionStorage`, so a reload in the same tab stays signed in and closing the tab forgets them. They never go in `localStorage` or cookies.
- `frontend/src/hey-api.ts` attaches `Authorization: Bearer <access token>` to every request.

### During a session

- Before the access token expires, the SPA renews it with a refresh token when the provider issues one. Otherwise it renews silently through the provider's session.
- If a request gets `401`, the SPA renews once and retries. If that also fails, it sends the user to sign in again and returns them to the same page afterwards.
- `403` is unchanged: screens already handle it, for example the access screen hides on `403`.
- Signing out clears the local tokens and, when the provider has an end-session endpoint, signs out there too. It ends on a signed-out page with a "sign in again" link. That page never redirects by itself: if the provider's own session were still alive, a redirect would silently sign the user back in.
- The header shows who is signed in, from the ID token's name or email, and has a sign-out action.

### Dev and tests

- `make dev` keeps the proxy token as Alice, so a developer needs no provider. With no `/auth/config` (no issuer), the SPA skips sign-in, exactly as today.
- Screen tests keep MSW. The sign-in, callback, renewal and `401` flows get unit and screen tests against a stubbed `oidc-client-ts`.

### End-to-end check

- A Playwright test starts the binary with the SPA embedded, against a database seeded for the test. Its `APP_JWT_ISSUER` points at a **fake OIDC provider** that the test harness starts.
  - The fake provider serves discovery, JWKS, an authorize endpoint that approves straight away as a named subject, a token endpoint and end-session.
  - It's written in Rust, in the test harness, so no extra container or third-party image is needed.
- The test signs in, loads a guarded screen, and sees data that only that subject's token could fetch. It then signs out, and the signed-out page shows and stays, without signing back in.
- It runs in `make check` (via `make check-changed` when `frontend/`, `crates/server/` or `crates/common/web/` change) and in CI.
- It builds the binary in debug mode with `embed-frontend`, not a release build, to keep the check fast.

### `make init`

- Sign-in is platform, not reference code, so the app `make init` produces keeps it. The end-to-end check uses the `access` screen, the one guarded screen every app has.

### Docs

- `frontend/AGENTS.md` "Signing in" replaces "there is no sign-in screen yet".
- A new ADR records PKCE in the SPA, rather than a backend-for-frontend with session cookies, and why.
- `GLOSSARY.md` gets **Sign-in** if the wording needs pinning down.

## Out of scope

- Sign-up and account recovery screens: the provider's hosted pages do that.
- A backend-for-frontend with server-held sessions or cookies.
- More than one identity provider, and choosing one at sign-in.
- The app issuing tokens itself.
- A profile or account screen beyond the header's name and sign-out.
- Changing how `/mcp` clients sign in: ticket 0010 already covers that.

## Open risks

- **Providers differ.** Auth0 needs an `audience` parameter to issue a JWT access token. Entra needs `api://…/.default` scopes. Some providers issue no refresh tokens to browser clients. The extra-params and scopes settings are meant to cover these, but each provider needs trying.
- **Name and email.** Ticket 0033 records them from the access token, but many providers put them only in the ID token. The Users tab may show only `sub` until the provider is configured to add them to the access token.
- **A token in `sessionStorage` can be read by any script running in the page (XSS).** Accepted for a single-tenant app with no third-party scripts. A backend-for-frontend is the upgrade if that changes.
