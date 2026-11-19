---
id: 0035
name: A visitor signs in to the embedded SPA with the identity provider and every API request carries their token
status: in-review
spec: docs/platform/specs/0002-the-embedded-spa-signs-users-in-with-the-identity-provider-and-sends-their-token.md
blocked_by: [docs/platform/tickets/0034-the-binary-serves-the-spa-s-sign-in-settings-at-au.md]
blocks: [docs/platform/tickets/0036-the-spa-renews-the-access-token-before-it-expires.md, docs/platform/tickets/0037-an-end-to-end-check-signs-in-to-the-embedded-spa-t.md]
---

# A visitor signs in to the embedded SPA with the identity provider and every API request carries their token

The SPA reads `/auth/config` at start-up. With no config (`404`, as in `make dev`), it skips sign-in exactly as today and the Vite proxy's `devToken` keeps signing requests as Alice. With config, it signs in through `oidc-client-ts` (authorization code + PKCE, public client) and attaches the access token to every request in `frontend/src/hey-api.ts`. This ticket covers sign-in, the callback, sign-out and the header. Renewal and `401` handling come in a later ticket. Follow `frontend/AGENTS.md`. Sign-in is platform, not reference code, so it must survive `make init` (`scripts/init.sh` reduces the UI to a shell).

## Acceptance criteria
- [ ] `oidc-client-ts` is added to `frontend/package.json` pinned to an exact version; the SPA doesn't implement any OIDC protocol steps itself.
- [ ] With no `/auth/config` (`404`), the app renders exactly as today with no sign-in screen, and `make dev` is unchanged.
- [ ] With config, a visitor who isn't signed in is redirected straight to the provider, with no sign-in screen of the app's own (the provider's hosted page asks for credentials), using the configured client id, `openid profile email` plus the configured scopes, the extra params, PKCE, `state` and `nonce`.
- [ ] `/auth/callback` completes the sign-in and returns the user to the page (path and search) they started on; a failed or cancelled sign-in shows an error page with the provider's error and a retry button, and never redirects in a loop.
- [ ] Tokens are stored in `sessionStorage` only (never `localStorage` or cookies), so a reload in the same tab stays signed in.
- [ ] `frontend/src/hey-api.ts` sends `Authorization: Bearer <access token>` on every request when signed in.
- [ ] The header shows the signed-in user's name, or email, from the ID token and has a sign-out action. Sign-out clears local tokens and, when the provider has an end-session endpoint, signs out there too, ending on a signed-out page with a "sign in again" link. That page never redirects by itself: if the provider's own session were still alive, a redirect would silently sign the user back in.
- [ ] Unit and screen tests against a stubbed `oidc-client-ts` (screens still answered by MSW) cover: no config → no sign-in; no session → redirected to the provider; signed-out page → no redirect until "sign in again"; callback success with deep link kept; callback error → message + retry; bearer header attached; header name and sign-out.
- [ ] `frontend/AGENTS.md` "Signing in" describes the flow and replaces "there is no sign-in screen yet".
- [ ] A new platform ADR (`docs/platform/adr/0042-…`) records PKCE in the SPA rather than a backend-for-frontend with session cookies, with the options weighed and their consequences (incl. the accepted `sessionStorage` XSS risk); `docs/platform/glossary.md` gets **Sign-in** if the wording needs pinning down.
- [ ] `make check-changed` passes, including `check-init`'s result if `scripts/init.sh` or what it edits changed.
