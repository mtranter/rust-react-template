---
id: 0036
name: The SPA renews the access token before it expires and recovers from a 401 by renewing once and retrying
status: in-review
spec: docs/platform/specs/0002-the-embedded-spa-signs-users-in-with-the-identity-provider-and-sends-their-token.md
blocked_by: [docs/platform/tickets/0035-a-visitor-signs-in-to-the-embedded-spa-with-the-id.md]
blocks: []
---

# The SPA renews the access token before it expires and recovers from a 401 by renewing once and retrying

Keep a signed-in session working without user action: renew the access token through `oidc-client-ts` before it expires, and handle a `401` from the API by renewing once and retrying the request, then sending the user to sign in again, returning them to the same page. Silent renewal must work with the only redirect URI the SPA is registered with (`<origin>/auth/callback`, per `.env.example`). If it needs another, document it there.

## Acceptance criteria
- [ ] Before the access token expires, the SPA renews it with the refresh token when the provider issued one, and otherwise silently through the provider's session.
- [ ] A request that gets `401` triggers one renewal and one retry of that request; if renewal or the retry fails, the user is sent to sign in and afterwards returns to the same page (path and search). It never loops.
- [ ] `403` handling is unchanged (e.g. the access screen still hides on `403`).
- [ ] Unit and screen tests against a stubbed `oidc-client-ts` cover: refresh-token renewal, silent renewal when there's no refresh token, `401` → renew → retry succeeds, `401` → renew fails → sign-in with return to the page, and `403` passes through unchanged.
- [ ] With no `/auth/config` (dev), nothing renews and a `401` behaves as today.
- [ ] `frontend/AGENTS.md` "Signing in" covers renewal and `401` handling.
- [ ] `make check-changed` passes.
